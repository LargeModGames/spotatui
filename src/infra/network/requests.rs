use super::Network;
use crate::core::{app::App, auth};
use anyhow::anyhow;
use log::warn;
use reqwest::header::CONTENT_LENGTH;
use reqwest::Method;
use rspotify::AuthCodePkceSpotify;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
  future::Future,
  path::Path,
  sync::Arc,
  sync::OnceLock,
  time::{Duration, Instant},
};
use tokio::sync::Mutex;

// Leaky-bucket pacing state: the theoretical arrival time (GCRA "TAT") of the
// next request. Sustained throughput is one request per SPOTIFY_API_MIN_INTERVAL,
// with up to SPOTIFY_API_BURST requests allowed to start at once so the
// concurrent fan-outs (search joins five queries, the artist page two) aren't
// artificially staggered.
static SPOTIFY_API_PACING: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
const SPOTIFY_API_MIN_INTERVAL: Duration = Duration::from_millis(250);
const SPOTIFY_API_BURST: u32 = 5;
const SPOTIFY_API_BASE_URL: &str = "https://api.spotify.com/v1/";

static SHARED_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// How long one forced token refresh suppresses the next one.
///
/// A 401 forces a full `POST /api/token`, which under PKCE also *rotates the
/// refresh token*. Spotify's player service intermittently answers a perfectly
/// valid token with 401 (issue #395), and the playback poll runs once a second
/// against an external device, so force-refreshing on every 401 mints a whole
/// new token family every few seconds — pure churn, and a lost rotated refresh
/// token logs the user out. Inside the cooldown the request is retried with the
/// token already in hand instead.
const FORCED_REFRESH_COOLDOWN: Duration = Duration::from_secs(30);

/// Delay before the single post-401 retry. Without it the retry lands inside
/// the same ~1s server-side window that produced the 401 (a track transition,
/// or a freshly minted token that has not propagated yet) and fails
/// identically, turning a recoverable blip into a surfaced error.
const UNAUTHORIZED_RETRY_BACKOFF: Duration = Duration::from_secs(1);

/// Longest response body echoed into the diagnostics log.
const MAX_LOGGED_BODY_CHARS: usize = 512;

/// Cooldown state for forced token refreshes triggered by a 401 response.
///
/// Deliberately long-lived and shared: every request is a fresh call into
/// [`spotify_api_request_json_for_base_with_refresh`], so per-call state would
/// reset on each poll and never suppress anything.
#[derive(Clone)]
pub struct ForcedRefreshGate {
  last_forced_refresh: Arc<Mutex<Option<Instant>>>,
  cooldown: Duration,
}

impl ForcedRefreshGate {
  pub fn new(cooldown: Duration) -> Self {
    Self {
      last_forced_refresh: Arc::new(Mutex::new(None)),
      cooldown,
    }
  }

  /// Claims the right to force a refresh now. Returns `false` when one already
  /// happened within the cooldown, meaning the caller should retry with the
  /// token it already holds rather than rotating the token family again.
  async fn try_begin(&self) -> bool {
    let mut last_forced_refresh = self.last_forced_refresh.lock().await;
    let now = Instant::now();
    match *last_forced_refresh {
      Some(previous) if now.duration_since(previous) < self.cooldown => false,
      _ => {
        *last_forced_refresh = Some(now);
        true
      }
    }
  }

  async fn reset(&self) {
    *self.last_forced_refresh.lock().await = None;
  }
}

impl Default for ForcedRefreshGate {
  fn default() -> Self {
    Self::new(FORCED_REFRESH_COOLDOWN)
  }
}

/// The process-wide gate used by every real Spotify request (one Spotify
/// session per process). Tests drive the base helper with their own instance.
fn shared_forced_refresh_gate() -> &'static ForcedRefreshGate {
  static GATE: OnceLock<ForcedRefreshGate> = OnceLock::new();
  GATE.get_or_init(ForcedRefreshGate::default)
}

/// Forget the last forced refresh, so the next 401 may force one immediately.
/// Called when a brand-new token family enters play (in-TUI login), where the
/// previous session's cooldown carries no information.
pub async fn reset_forced_refresh_cooldown() {
  shared_forced_refresh_gate().reset().await;
}

/// Age of the access token now attached to a request, derived from
/// `expires_at - expires_in`. Diagnostics only: a 401 on a token minted seconds
/// ago points at propagation lag or a server-side reject, not at expiry.
fn token_age(token: &rspotify::Token) -> Option<Duration> {
  let issued_at = token.expires_at? - token.expires_in;
  (chrono::Utc::now() - issued_at).to_std().ok()
}

fn truncate_for_log(body: &str) -> String {
  let trimmed = body.trim();
  if trimmed.chars().count() <= MAX_LOGGED_BODY_CHARS {
    return trimmed.to_string();
  }
  let head: String = trimmed.chars().take(MAX_LOGGED_BODY_CHARS).collect();
  format!("{head}… (truncated)")
}

/// Returns the process-wide shared [`reqwest::Client`].
///
/// A `reqwest::Client` owns a connection pool and is meant to be built once and
/// reused; building one per request (the previous behavior) discarded keep-alive
/// connections and forced a fresh TLS handshake on every call. The client is
/// internally reference-counted, so the returned `&'static` reference is shared
/// cheaply across all request paths (Spotify API, friends relay, telemetry,
/// lyrics).
pub fn shared_http_client() -> &'static reqwest::Client {
  SHARED_HTTP_CLIENT.get_or_init(|| {
    // Spotify API (and the friends relay / lyrics / telemetry paths that share
    // this client) all return bounded, non-streaming responses, so a blanket
    // request timeout is safe here. Without one, a post-connect read stall
    // (captive portal, half-open TCP, an edge that accepts then sends nothing)
    // makes `send()`/`text()` await forever on the serial IoEvent pump and
    // freezes the whole app until it is killed. An explicit connect timeout
    // additionally bounds the TCP/TLS handshake.
    reqwest::Client::builder()
      .connect_timeout(Duration::from_secs(10))
      .timeout(Duration::from_secs(30))
      .build()
      .unwrap_or_else(|_| reqwest::Client::new())
  })
}

fn response_is_json(response: &reqwest::Response) -> bool {
  response
    .headers()
    .get(reqwest::header::CONTENT_TYPE)
    .and_then(|value| value.to_str().ok())
    .is_some_and(|value| {
      let value = value.to_ascii_lowercase();
      value.contains("/json") || value.contains("+json")
    })
}

pub async fn pace_spotify_api_call() {
  // Reserve a start slot under the lock, then sleep OUTSIDE it. The previous
  // implementation held the lock across the sleep, which serialized every
  // "concurrent" tokio::join! call site into 250ms-apart starts (adding ~1s of
  // pure pacing to every search).
  let burst_allowance = SPOTIFY_API_MIN_INTERVAL * (SPOTIFY_API_BURST - 1);
  let start_at = {
    let pacing_lock = SPOTIFY_API_PACING.get_or_init(|| Mutex::new(None));
    let mut theoretical_arrival = pacing_lock.lock().await;
    let now = Instant::now();
    // Clamp to `now` so idle time never banks more than one burst of credit.
    let tat = theoretical_arrival.map_or(now, |t| t.max(now));
    *theoretical_arrival = Some(tat + SPOTIFY_API_MIN_INTERVAL);
    // A call may start up to `burst_allowance` ahead of its theoretical slot.
    tat.checked_sub(burst_allowance).map_or(now, |t| t.max(now))
  };

  let now = Instant::now();
  if start_at > now {
    tokio::time::sleep(start_at - now).await;
  }
}

pub async fn spotify_api_request_json_for_with_refresh(
  spotify: &AuthCodePkceSpotify,
  method: Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Value>,
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
) -> anyhow::Result<Value> {
  spotify_api_request_json_for_base_with_refresh(
    spotify,
    SpotifyApiRequest {
      base_url: SPOTIFY_API_BASE_URL,
      method,
      path,
      query,
      body,
    },
    |force| async move {
      match auth::refresh_token_and_cache(spotify, token_cache_path, force).await {
        Ok(expiry) => {
          let mut app = app.lock().await;
          app.spotify_token_expiry = Some(expiry);
          app.auth_refresh_in_progress = false;
          Ok(Some(expiry))
        }
        Err(e) => {
          let mut app = app.lock().await;
          app.auth_refresh_in_progress = false;
          app.is_loading = false;
          Err(e)
        }
      }
    },
    shared_forced_refresh_gate(),
  )
  .await
}

/// One Spotify Web API call: everything about *what* to send, kept together so
/// the shared helper's signature stays readable next to its refresh closure and
/// retry policy.
struct SpotifyApiRequest<'a> {
  base_url: &'a str,
  method: Method,
  path: &'a str,
  query: &'a [(&'a str, String)],
  body: Option<Value>,
}

async fn spotify_api_request_json_for_base_with_refresh<F, Fut>(
  spotify: &AuthCodePkceSpotify,
  request: SpotifyApiRequest<'_>,
  mut refresh_token: F,
  forced_refresh_gate: &ForcedRefreshGate,
) -> anyhow::Result<Value>
where
  F: FnMut(bool) -> Fut,
  Fut: Future<Output = anyhow::Result<Option<std::time::SystemTime>>>,
{
  let SpotifyApiRequest {
    base_url,
    method,
    path,
    query,
    body,
  } = request;

  refresh_token(false).await?;

  let mut url = reqwest::Url::parse(base_url)?.join(path)?;
  if !query.is_empty() {
    let mut qp = url.query_pairs_mut();
    for (k, v) in query {
      qp.append_pair(k, v);
    }
  }

  let client = shared_http_client();
  let mut attempt: u8 = 0;
  let max_attempts: u8 = 4;
  let mut attempted_unauthorized_recovery = false;

  loop {
    let (access_token, access_token_age) = {
      let token_lock = spotify.token.lock().await.expect("Failed to lock token");
      let token = token_lock
        .as_ref()
        .ok_or_else(|| anyhow!("No access token available"))?;
      (token.access_token.clone(), token_age(token))
    };

    pace_spotify_api_call().await;

    let mut request = client
      .request(method.clone(), url.clone())
      .header("Authorization", format!("Bearer {}", access_token))
      .header("Content-Type", "application/json");

    if let Some(payload) = body.clone() {
      request = request.json(&payload);
    } else if matches!(
      method,
      Method::POST | Method::PUT | Method::DELETE | Method::PATCH
    ) {
      // Some Spotify mutation endpoints reject bodyless requests unless the
      // transport explicitly declares an empty body with Content-Length: 0.
      request = request.header(CONTENT_LENGTH, "0").body(Vec::new());
    }

    let response = match request.send().await {
      Ok(response) => response,
      Err(e) => {
        if attempt + 1 < max_attempts && (e.is_connect() || e.is_timeout() || e.is_request()) {
          let backoff_secs = 1 + u64::from(attempt);
          tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
          attempt += 1;
          continue;
        }
        return Err(anyhow!("Spotify API request failed: {}", e));
      }
    };
    if response.status().is_success() {
      let should_parse_json = response_is_json(&response);
      let response_body = response.text().await?;
      if response_body.trim().is_empty() {
        return Ok(Value::Null);
      }
      if should_parse_json {
        return Ok(serde_json::from_str(&response_body)?);
      }
      return Ok(Value::Null);
    }

    let status = response.status();
    let retry_after_secs = response
      .headers()
      .get("retry-after")
      .and_then(|h| h.to_str().ok())
      .and_then(|v| v.parse::<u64>().ok())
      .unwrap_or(1);
    // `text()` consumes the response, so everything the branches below need
    // (status, retry-after, body) is captured here, once.
    let body = response.text().await.unwrap_or_default();

    // Diagnostics for every non-2xx: which endpoint, which status, what Spotify
    // actually said, and how old the token we attached was. The token *value* is
    // never logged. This is what separates "the token really expired" from
    // "Spotify rejected a valid token" in a user-supplied log (issue #395).
    warn!(
      "Spotify API {} {} -> {} (attempt {}/{}, token age {}): {}",
      method,
      url.path(),
      status,
      attempt + 1,
      max_attempts,
      access_token_age.map_or_else(
        || "unknown".to_string(),
        |age| format!("{}s", age.as_secs())
      ),
      truncate_for_log(&body)
    );

    if status == reqwest::StatusCode::UNAUTHORIZED && !attempted_unauthorized_recovery {
      // One-shot: whichever recovery path runs below, a second 401 falls through
      // to the error return instead of looping.
      attempted_unauthorized_recovery = true;

      if forced_refresh_gate.try_begin().await {
        match refresh_token(true).await {
          Ok(Some(_)) => {
            tokio::time::sleep(UNAUTHORIZED_RETRY_BACKOFF).await;
            continue;
          }
          Ok(None) => {
            return Err(anyhow!(
              "Spotify API {} failed: {} (token refresh unavailable for this request)",
              status,
              body
            ));
          }
          Err(refresh_err) => {
            return Err(anyhow!(
              "Spotify API {} failed: {} (token refresh failed: {})",
              status,
              body,
              refresh_err
            ));
          }
        }
      }

      // A forced refresh already ran within the cooldown, so the token in hand
      // is freshly minted and this 401 is coming from Spotify's side. Retry with
      // it after a backoff instead of rotating the token family yet again.
      warn!("401 within the forced-refresh cooldown; retrying with the current token");
      tokio::time::sleep(UNAUTHORIZED_RETRY_BACKOFF).await;
      continue;
    }

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt + 1 < max_attempts {
      let backoff_secs = retry_after_secs.max(1) + u64::from(attempt);
      tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
      attempt += 1;
      continue;
    }

    return Err(anyhow!("Spotify API {} failed: {}", status, body));
  }
}

impl Network {
  pub async fn spotify_api_request_json(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Value>,
  ) -> anyhow::Result<Value> {
    spotify_api_request_json_for_with_refresh(
      self.spotify(),
      method,
      path,
      query,
      body,
      &self.token_cache_path,
      &self.app,
    )
    .await
  }

  pub async fn spotify_get_typed<T: DeserializeOwned>(
    &self,
    path: &str,
    query: &[(&str, String)],
  ) -> anyhow::Result<T> {
    let mut value = self
      .spotify_api_request_json(Method::GET, path, query, None)
      .await?;
    normalize_spotify_payload(&mut value);
    Ok(serde_json::from_value(value)?)
  }
}

pub fn normalize_spotify_payload(value: &mut Value) {
  match value {
    Value::Object(map) => {
      if let Some(Value::Array(items)) = map.get_mut("items") {
        items.retain(|item| !item.is_null());
      }

      if map.contains_key("snapshot_id")
        && map.contains_key("owner")
        && map.contains_key("id")
        && !map.contains_key("tracks")
      {
        if let Some(items_obj) = map.get("items").cloned() {
          map.insert("tracks".to_string(), items_obj);
        } else {
          map.insert("tracks".to_string(), json!({ "href": "", "total": 0 }));
        }
      }

      if map.contains_key("added_at") && !map.contains_key("track") {
        if let Some(item_obj) = map.get("item").cloned() {
          map.insert("track".to_string(), item_obj);
        }
      }

      if map.contains_key("album")
        && map.contains_key("artists")
        && map.contains_key("track_number")
        && map.contains_key("duration_ms")
      {
        map
          .entry("available_markets".to_string())
          .or_insert_with(|| json!([]));
        map
          .entry("external_ids".to_string())
          .or_insert_with(|| json!({}));
        map.entry("linked_from".to_string()).or_insert(Value::Null);
        map
          .entry("popularity".to_string())
          .or_insert_with(|| json!(0));
      }

      if map.contains_key("media_type")
        && map.contains_key("languages")
        && map.contains_key("description")
        && map.contains_key("name")
      {
        map
          .entry("available_markets".to_string())
          .or_insert_with(|| json!([]));
        map
          .entry("publisher".to_string())
          .or_insert_with(|| json!(""));
      }

      if map.contains_key("album_type")
        && map.contains_key("artists")
        && map.contains_key("images")
        && map.contains_key("name")
      {
        if map.contains_key("tracks") {
          map
            .entry("available_markets".to_string())
            .or_insert(Value::Null);
          map
            .entry("external_ids".to_string())
            .or_insert_with(|| json!({}));
          map
            .entry("popularity".to_string())
            .or_insert_with(|| json!(0));
          map.entry("label".to_string()).or_insert(Value::Null);
        } else {
          map
            .entry("available_markets".to_string())
            .or_insert_with(|| json!([]));
        }
      }

      let looks_like_artist = map
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t == "artist")
        || (map.contains_key("external_urls")
          && map.contains_key("name")
          && map.contains_key("id")
          && (map.contains_key("genres") || map.contains_key("images")));

      if looks_like_artist {
        map.entry("href".to_string()).or_insert_with(|| json!(""));
        map.entry("genres".to_string()).or_insert_with(|| json!([]));
        map.entry("images".to_string()).or_insert_with(|| json!([]));
        map
          .entry("followers".to_string())
          .or_insert_with(|| json!({ "href": null, "total": 0 }));
        map
          .entry("popularity".to_string())
          .or_insert_with(|| json!(0));
      }

      for child in map.values_mut() {
        normalize_spotify_payload(child);
      }
    }
    Value::Array(values) => {
      values.retain(|item| !item.is_null());
      for child in values.iter_mut() {
        normalize_spotify_payload(child);
      }
    }
    _ => {}
  }
}

pub fn is_rate_limited_error(e: &anyhow::Error) -> bool {
  let text = e.to_string();
  text.contains("429") || text.contains("Too Many Requests") || text.contains("Too many requests")
}

#[allow(dead_code)]
pub fn is_transient_network_error(e: &anyhow::Error) -> bool {
  let text = e.to_string().to_lowercase();
  text.contains("error sending request for url")
    || text.contains("connection reset")
    || text.contains("connection refused")
    || text.contains("timed out")
    || text.contains("temporary failure")
    || text.contains("dns")
}

pub async fn spotify_get_typed_compat_for_with_refresh<T: DeserializeOwned>(
  spotify: &AuthCodePkceSpotify,
  path: &str,
  query: &[(&str, String)],
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
) -> anyhow::Result<T> {
  let mut value = spotify_api_request_json_for_with_refresh(
    spotify,
    Method::GET,
    path,
    query,
    None,
    token_cache_path,
    app,
  )
  .await?;
  normalize_spotify_payload(&mut value);
  Ok(serde_json::from_value(value)?)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{TimeDelta, Utc};
  use rspotify::{Config, Credentials, OAuth, Token};
  use std::{
    sync::{
      atomic::{AtomicUsize, Ordering},
      Arc,
    },
    time::SystemTime,
  };
  use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
  };

  async fn spotify_with_access_token(access_token: &str) -> AuthCodePkceSpotify {
    let spotify = AuthCodePkceSpotify::with_config(
      Credentials::new_pkce("test_client_id"),
      OAuth {
        redirect_uri: "http://localhost:8888/callback".to_string(),
        ..Default::default()
      },
      Config::default(),
    );

    let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
    *token_lock = Some(Token {
      access_token: access_token.to_string(),
      refresh_token: Some("refresh_token".to_string()),
      expires_in: TimeDelta::seconds(3600),
      expires_at: Some(Utc::now() + TimeDelta::seconds(3600)),
      scopes: Default::default(),
    });
    drop(token_lock);

    spotify
  }

  async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut buf = vec![0; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf[..n]).to_string()
  }

  #[tokio::test]
  async fn retries_once_with_refreshed_token_after_401() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let seen_authorization = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_authorization_for_server = Arc::clone(&seen_authorization);

    let server = tokio::spawn(async move {
      for status in ["401 Unauthorized", "200 OK"] {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        if let Some(header) = request
          .lines()
          .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
        {
          seen_authorization_for_server
            .lock()
            .await
            .push(header.to_ascii_lowercase());
        }

        let body = if status.starts_with("200") {
          r#"{"ok":true}"#
        } else {
          r#"{"error":"expired"}"#
        };
        let response = format!(
          "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
          body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
      }
    });

    let spotify = spotify_with_access_token("old_access").await;
    let refresh_calls = Arc::new(AtomicUsize::new(0));
    let refresh_calls_for_closure = Arc::clone(&refresh_calls);
    let spotify_for_closure = spotify.clone();

    let started_at = Instant::now();
    let result = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      SpotifyApiRequest {
        base_url: &base_url,
        method: Method::GET,
        path: "me",
        query: &[],
        body: None,
      },
      move |force| {
        let spotify = spotify_for_closure.clone();
        let refresh_calls = Arc::clone(&refresh_calls_for_closure);
        async move {
          refresh_calls.fetch_add(1, Ordering::SeqCst);
          if force {
            let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
            let token = token_lock.as_mut().unwrap();
            token.access_token = "new_access".to_string();
          }
          Ok(Some(SystemTime::now() + Duration::from_secs(3600)))
        }
      },
      &ForcedRefreshGate::default(),
    )
    .await
    .unwrap();
    let elapsed = started_at.elapsed();

    server.await.unwrap();

    assert_eq!(result, json!({ "ok": true }));
    assert_eq!(refresh_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
      *seen_authorization.lock().await,
      vec![
        "authorization: bearer old_access".to_string(),
        "authorization: bearer new_access".to_string()
      ]
    );
    // The retry must not land inside the window that produced the 401.
    assert!(
      elapsed >= UNAUTHORIZED_RETRY_BACKOFF,
      "post-refresh retry was not backed off (took {elapsed:?})"
    );
  }

  /// Issue #395: the playback poll runs once a second, and Spotify's player
  /// service can answer a valid token with 401. Only the first 401 in a cooldown
  /// window may force a (refresh-token-rotating) `POST /api/token`; the next one
  /// retries with the token already in hand.
  #[tokio::test]
  async fn second_unauthorized_within_cooldown_retries_without_forcing_refresh() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let seen_authorization = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_authorization_for_server = Arc::clone(&seen_authorization);

    let server = tokio::spawn(async move {
      // Two calls, each answered 401 then 200 — the pattern a track transition
      // produces across consecutive polls.
      for status in ["401 Unauthorized", "200 OK", "401 Unauthorized", "200 OK"] {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        if let Some(header) = request
          .lines()
          .find(|line| line.to_ascii_lowercase().starts_with("authorization:"))
        {
          seen_authorization_for_server
            .lock()
            .await
            .push(header.to_ascii_lowercase());
        }

        let body = if status.starts_with("200") {
          r#"{"ok":true}"#
        } else {
          r#"{ "error" : { "status" : 401, "message" : "Access token missing" }}"#
        };
        let response = format!(
          "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
          body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
      }
    });

    let spotify = spotify_with_access_token("old_access").await;
    let forced_refresh_calls = Arc::new(AtomicUsize::new(0));
    let gate = ForcedRefreshGate::default();

    for _ in 0..2 {
      let forced_refresh_calls = Arc::clone(&forced_refresh_calls);
      let spotify_for_closure = spotify.clone();
      let result = spotify_api_request_json_for_base_with_refresh(
        &spotify,
        SpotifyApiRequest {
          base_url: &base_url,
          method: Method::GET,
          path: "me/player",
          query: &[],
          body: None,
        },
        move |force| {
          let spotify = spotify_for_closure.clone();
          let forced_refresh_calls = Arc::clone(&forced_refresh_calls);
          async move {
            if force {
              forced_refresh_calls.fetch_add(1, Ordering::SeqCst);
              let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
              let token = token_lock.as_mut().unwrap();
              token.access_token = "new_access".to_string();
            }
            Ok(Some(SystemTime::now() + Duration::from_secs(3600)))
          }
        },
        &gate,
      )
      .await
      .unwrap();

      assert_eq!(result, json!({ "ok": true }));
    }

    server.await.unwrap();

    assert_eq!(
      forced_refresh_calls.load(Ordering::SeqCst),
      1,
      "the second 401 must not mint another token family"
    );
    assert_eq!(
      *seen_authorization.lock().await,
      vec![
        "authorization: bearer old_access".to_string(),
        "authorization: bearer new_access".to_string(),
        "authorization: bearer new_access".to_string(),
        "authorization: bearer new_access".to_string(),
      ]
    );
  }

  /// An expired cooldown must let the next 401 force a refresh again, so a token
  /// that really did go bad still recovers.
  #[tokio::test]
  async fn unauthorized_forces_refresh_again_once_cooldown_expires() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());

    let server = tokio::spawn(async move {
      for status in ["401 Unauthorized", "200 OK", "401 Unauthorized", "200 OK"] {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        let body = if status.starts_with("200") {
          r#"{"ok":true}"#
        } else {
          r#"{"error":"expired"}"#
        };
        let response = format!(
          "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
          body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
      }
    });

    let spotify = spotify_with_access_token("access_token").await;
    let forced_refresh_calls = Arc::new(AtomicUsize::new(0));
    // A zero cooldown is the "cooldown has elapsed" case.
    let gate = ForcedRefreshGate::new(Duration::ZERO);

    for _ in 0..2 {
      let forced_refresh_calls = Arc::clone(&forced_refresh_calls);
      spotify_api_request_json_for_base_with_refresh(
        &spotify,
        SpotifyApiRequest {
          base_url: &base_url,
          method: Method::GET,
          path: "me/player",
          query: &[],
          body: None,
        },
        move |force| {
          let forced_refresh_calls = Arc::clone(&forced_refresh_calls);
          async move {
            if force {
              forced_refresh_calls.fetch_add(1, Ordering::SeqCst);
            }
            Ok(Some(SystemTime::now() + Duration::from_secs(3600)))
          }
        },
        &gate,
      )
      .await
      .unwrap();
    }

    server.await.unwrap();

    assert_eq!(forced_refresh_calls.load(Ordering::SeqCst), 2);
  }

  /// A second 401 inside the same call must surface the error instead of
  /// looping: the recovery is one-shot regardless of which path it took.
  #[tokio::test]
  async fn repeated_unauthorized_in_one_call_gives_up_after_one_recovery() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_for_server = Arc::clone(&request_count);

    let server = tokio::spawn(async move {
      for _ in 0..2 {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _request = read_http_request(&mut stream).await;
        request_count_for_server.fetch_add(1, Ordering::SeqCst);
        let body = r#"{ "error" : { "status" : 401, "message" : "Access token missing" }}"#;
        let response = format!(
          "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
          body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
      }
    });

    let spotify = spotify_with_access_token("access_token").await;

    let error = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      SpotifyApiRequest {
        base_url: &base_url,
        method: Method::GET,
        path: "me/player",
        query: &[],
        body: None,
      },
      |_force| async move { Ok(Some(SystemTime::now() + Duration::from_secs(3600))) },
      &ForcedRefreshGate::default(),
    )
    .await
    .unwrap_err();

    server.await.unwrap();

    assert_eq!(request_count.load(Ordering::SeqCst), 2);
    // playback.rs classifies by substring, so the status must stay in the text.
    assert!(
      error.to_string().contains("401"),
      "unexpected error text: {error}"
    );
  }

  #[tokio::test]
  async fn sends_content_length_zero_for_empty_mutation_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let seen_request = Arc::new(Mutex::new(String::new()));
    let seen_request_for_server = Arc::clone(&seen_request);

    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let request = read_http_request(&mut stream).await;
      *seen_request_for_server.lock().await = request;

      let body = r#"{"ok":true}"#;
      let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });

    let spotify = spotify_with_access_token("access_token").await;

    let result = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      SpotifyApiRequest {
        base_url: &base_url,
        method: Method::PUT,
        path: "me/player/shuffle",
        query: &[("state", "true".to_string())],
        body: None,
      },
      |_force| async move { Ok(Some(SystemTime::now() + Duration::from_secs(3600))) },
      &ForcedRefreshGate::default(),
    )
    .await
    .unwrap();

    server.await.unwrap();

    let request = seen_request.lock().await.clone();
    assert_eq!(result, json!({ "ok": true }));
    assert!(request.starts_with("PUT /v1/me/player/shuffle?state=true HTTP/1.1\r\n"));
    assert!(request
      .to_ascii_lowercase()
      .contains("content-length: 0\r\n"));
  }

  #[tokio::test]
  async fn ignores_non_json_success_body_for_mutation_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());

    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let _request = read_http_request(&mut stream).await;

      let body = "OK";
      let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });

    let spotify = spotify_with_access_token("access_token").await;

    let result = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      SpotifyApiRequest {
        base_url: &base_url,
        method: Method::PUT,
        path: "me/player/play",
        query: &[],
        body: None,
      },
      |_force| async move { Ok(Some(SystemTime::now() + Duration::from_secs(3600))) },
      &ForcedRefreshGate::default(),
    )
    .await
    .unwrap();

    server.await.unwrap();

    assert_eq!(result, Value::Null);
  }
}
