use super::Network;
use anyhow::anyhow;
use log::{debug, trace, warn};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
  sync::Arc,
  sync::OnceLock,
  time::{Duration, Instant},
};
use tokio::sync::Mutex;

static SHARED_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Longest rate-limit window honored, so a bad value cannot park the app.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(600);

/// How long a 429 from the internal API holds every call back: librespot does
/// not pass Spotify's `Retry-After` on.
const ROUTED_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(10);

/// Total time one Spotify call may take. Above the session connect timeout,
/// so the first call of a run can still connect.
const SPOTIFY_REQUEST_DEADLINE: Duration = Duration::from_secs(40);

fn deadline_error(deadline: Duration) -> anyhow::Error {
  anyhow!(
    "Spotify API request failed: timed out after {}s",
    deadline.as_secs()
  )
}
/// Longest response body echoed into the diagnostics log.
const MAX_LOGGED_BODY_CHARS: usize = 512;

/// What a redacted value reads as in the log.
const REDACTED_PLACEHOLDER: &str = "***";

/// JSON object keys and query parameters whose *value* never reaches the log,
/// matched case-insensitively and by the whole key.
///
/// Deliberately a denylist rather than an allowlist: an allowlist would blank
/// every field spotatui does not already know about, which is exactly the
/// unknown endpoint a bug report is about, and would make the debug log
/// useless. The price is that a secret-ish field Spotify adds tomorrow passes
/// through — the mitigation is the `--debug` help text asking users to read
/// the log before posting it publicly. The access token is not covered by this
/// at all: it lives in the `Authorization` header, and the rule there is that
/// no code path ever formats a header into a log line.
const REDACTED_KEYS: [&str; 11] = [
  "access_token",
  "refresh_token",
  "id_token",
  "token",
  "client_secret",
  "code",
  "email",
  "phone_number",
  "password",
  "secret",
  "authorization",
];

/// Whole-key match, so `token_type` (a plain, useful field) survives while
/// `token` does not. A substring match would blank half of every payload.
fn is_redacted_key(key: &str) -> bool {
  REDACTED_KEYS
    .iter()
    .any(|denied| key.eq_ignore_ascii_case(denied))
}

/// Redacted in a query string on top of [`REDACTED_KEYS`]. `q` is the search
/// box. The endpoint appears in the `warn!` line for every failed request, and
/// `warn` is on without `--debug`, so a failed search would otherwise write
/// what the user typed into a log they never opted into.
const REDACTED_QUERY_KEYS: [&str; 1] = ["q"];

fn is_redacted_query_key(key: &str) -> bool {
  is_redacted_key(key)
    || REDACTED_QUERY_KEYS
      .iter()
      .any(|denied| key.eq_ignore_ascii_case(denied))
}

/// Replaces the value of every denylisted key with `***`, at any depth and
/// whatever its type: a denylisted key holding an object is replaced whole,
/// never walked into.
fn redact_json(value: &Value) -> Value {
  match value {
    Value::Object(map) => Value::Object(
      map
        .iter()
        .map(|(key, child)| {
          let redacted = if is_redacted_key(key) {
            Value::String(REDACTED_PLACEHOLDER.to_string())
          } else {
            redact_json(child)
          };
          (key.clone(), redacted)
        })
        .collect(),
    ),
    Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
    other => other.clone(),
  }
}

/// The endpoint as it may appear in a log line or an error message: the path
/// plus the query, with denylisted parameter values replaced.
fn redacted_target(path: &str, query: &[(&str, String)]) -> String {
  let path = format!("/v1/{}", path.trim_start_matches('/'));
  if query.is_empty() {
    return path;
  }
  // Encoded rather than concatenated: a `&` or a newline inside a value would
  // otherwise split the logged endpoint into two parameters or two log lines.
  let mut encoded = url::form_urlencoded::Serializer::new(String::new());
  for (key, value) in query {
    if is_redacted_query_key(key) {
      encoded.append_pair(key, REDACTED_PLACEHOLDER);
    } else {
      encoded.append_pair(key, value);
    }
  }
  format!("{path}?{}", encoded.finish())
}
/// Redacted, truncated request body for a log line; empty when there is none.
fn request_body_for_log(body: Option<&Value>) -> String {
  body.map_or_else(String::new, |payload| {
    format!(
      " body {}",
      truncate_for_log(&redact_json(payload).to_string())
    )
  })
}

/// Redacted, truncated response body. JSON is redacted key-wise; anything else
/// (an HTML error page, a bare `OK`) is only truncated.
fn response_body_for_log(body: &str) -> String {
  serde_json::from_str::<Value>(body).map_or_else(
    |_| truncate_for_log(body),
    |value| truncate_for_log(&redact_json(&value).to_string()),
  )
}

/// The `debug!` line a successful request leaves behind. A free function
/// rather than an inline `format!` so a test can assert that the body really
/// goes through [`request_body_for_log`]: an argument swapped at the call site
/// would otherwise put an unredacted body in the log with every test green.
fn request_success_line(
  endpoint: &str,
  status: reqwest::StatusCode,
  elapsed_ms: u128,
  body: Option<&Value>,
) -> String {
  format!(
    "Spotify API {endpoint} -> {status} in {elapsed_ms}ms{}",
    request_body_for_log(body)
  )
}

/// The `trace!` line carrying a response body. Same reasoning as
/// [`request_success_line`].
fn response_trace_line(endpoint: &str, status: reqwest::StatusCode, response_body: &str) -> String {
  format!(
    "Spotify API {endpoint} -> {status} response {}",
    response_body_for_log(response_body)
  )
}

/// Everything the `warn!` line for a failed request needs, grouped so a test
/// can check what the line carries.
struct FailureLine<'a> {
  /// Whether the request body may go in. The body carries user-entered text
  /// (a playlist name, a search term), and this line is logged at `warn`,
  /// which is on without `--debug` — so the body is opt-in like everything
  /// else the debug mode adds.
  include_body: bool,
  endpoint: &'a str,
  elapsed_ms: u128,
  body: Option<&'a Value>,
  error: &'a str,
}

impl FailureLine<'_> {
  fn render(&self) -> String {
    format!(
      "Spotify API {} failed in {}ms{}: {}",
      self.endpoint,
      self.elapsed_ms,
      self.body_for_log(),
      truncate_for_log(self.error)
    )
  }

  fn body_for_log(&self) -> String {
    match (self.include_body, self.body.is_some()) {
      (true, _) => request_body_for_log(self.body),
      (false, true) => " body omitted (--debug logs it)".to_string(),
      (false, false) => String::new(),
    }
  }
}

/// The rate-limit window every Spotify call shares: while it is open, calls
/// fail at once instead of adding load.
#[derive(Clone, Default)]
pub struct RateLimitGate {
  rate_limited_until: Arc<Mutex<Option<Instant>>>,
}

impl RateLimitGate {
  /// Time left in the window, if one is open.
  pub(crate) async fn rate_limit_remaining(&self) -> Option<Duration> {
    let until = (*self.rate_limited_until.lock().await)?;
    until.checked_duration_since(Instant::now())
  }

  /// Open (or extend) the window: a shorter one that lands during a longer
  /// one must not cut the longer one short.
  pub(crate) async fn rate_limit_for(&self, window: Duration) {
    let until = Instant::now() + window.min(MAX_RETRY_AFTER);
    let mut slot = self.rate_limited_until.lock().await;
    *slot = Some(slot.map_or(until, |current| current.max(until)));
  }
}
/// A non-2xx answer from Spotify, with the status kept so a caller can tell a
/// rejected token from a request that merely failed. `Display` is the plain
/// text every other caller already matches on.
#[derive(Debug)]
pub struct SpotifyApiError {
  pub status: reqwest::StatusCode,
  pub(crate) body: String,
  pub(crate) detail: Option<String>,
  /// `METHOD /path?query`, redacted. Issue #566: a bare
  /// `Spotify API 403 Forbidden failed: …` never said which call broke.
  pub(crate) endpoint: Option<String>,
}

/// # This text is parsed
///
/// Several call sites classify a failure by matching substrings of this
/// output — `is_rate_limited_error`/`is_transient_network_error` here,
/// `is_no_active_device_error`/`is_restriction_violated_error` and the
/// playback poll's inline `contains("404")`/`contains("401")` checks in
/// `playback.rs`. Their own tests hand-build the string, so a change here
/// breaks them *silently*. The endpoint is therefore appended at the end,
/// behind `detail`, leaving every existing substring in place; and the two
/// classifiers in this file strip it again before matching, because an
/// endpoint carries user-supplied ids and search terms (a track id containing
/// `dns`, a playlist id containing `429`) that would otherwise read as a
/// transport failure or a rate limit.
impl std::fmt::Display for SpotifyApiError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Spotify API {} failed: {}", self.status, self.body)?;
    if let Some(detail) = &self.detail {
      write!(f, " ({detail})")?;
    }
    match &self.endpoint {
      Some(endpoint) => write!(f, " [{endpoint}]"),
      None => Ok(()),
    }
  }
}

impl std::error::Error for SpotifyApiError {}

/// The process-wide gate every Spotify request goes through.
pub(crate) fn shared_rate_gate() -> &'static RateLimitGate {
  static GATE: OnceLock<RateLimitGate> = OnceLock::new();
  GATE.get_or_init(RateLimitGate::default)
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

/// The one Spotify call every handler makes: a Web API path answered from the
/// internal API. A 429 opens the shared rate-limit window.
pub async fn spotify_api_request_json_for(
  method: Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Value>,
) -> anyhow::Result<Value> {
  request_with(
    shared_rate_gate(),
    SPOTIFY_REQUEST_DEADLINE,
    method,
    path,
    query,
    body,
  )
  .await
}

async fn request_with(
  gate: &RateLimitGate,
  deadline: Duration,
  method: Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Value>,
) -> anyhow::Result<Value> {
  let endpoint = format!("{method} {}", redacted_target(path, query));
  // Inside the window every call fails here at once: a sleep would park the
  // serial pump and every event behind it.
  if let Some(left) = gate.rate_limit_remaining().await {
    debug!(
      "Spotify API {endpoint} not sent: rate limited for another {}s",
      left.as_secs().max(1)
    );
    return Err(rate_limited_error(left, endpoint));
  }

  let started_at = Instant::now();
  let answered = tokio::time::timeout(
    deadline,
    Box::pin(answer(&method, path, query, body.as_ref())),
  )
  .await
  .unwrap_or_else(|_| Err(deadline_error(deadline)));
  let elapsed_ms = started_at.elapsed().as_millis();

  match answered {
    Ok(value) => {
      debug!(
        "{}",
        request_success_line(&endpoint, StatusCode::OK, elapsed_ms, body.as_ref())
      );
      if log::log_enabled!(log::Level::Trace) {
        trace!(
          "{}",
          response_trace_line(&endpoint, StatusCode::OK, &value.to_string())
        );
      }
      Ok(value)
    }
    Err(error) => {
      let error = match error.downcast::<SpotifyApiError>() {
        Ok(api_error) if api_error.status == StatusCode::TOO_MANY_REQUESTS => {
          gate.rate_limit_for(ROUTED_RATE_LIMIT_WINDOW).await;
          rate_limited_error(ROUTED_RATE_LIMIT_WINDOW, endpoint.clone())
        }
        Ok(mut api_error) => {
          api_error.endpoint = Some(endpoint.clone());
          api_error.into()
        }
        Err(other) => other,
      };
      warn!(
        "{}",
        FailureLine {
          include_body: log::log_enabled!(log::Level::Debug),
          endpoint: &endpoint,
          elapsed_ms,
          body: body.as_ref(),
          error: &error.to_string(),
        }
        .render()
      );
      Err(error)
    }
  }
}

fn rate_limited_error(left: Duration, endpoint: String) -> anyhow::Error {
  SpotifyApiError {
    status: StatusCode::TOO_MANY_REQUESTS,
    body: format!(
      "rate limited by Spotify, retry in {}s",
      left.as_secs().max(1)
    ),
    detail: None,
    endpoint: Some(endpoint),
  }
  .into()
}

async fn answer(
  method: &Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<&Value>,
) -> anyhow::Result<Value> {
  #[cfg(test)]
  if let Some(base_url) = fake::base_url() {
    return fake::request(&base_url, method, path, query, body).await;
  }
  #[cfg(feature = "spotify-session")]
  return crate::infra::spotify::route(method, path, query, body).await;
  #[cfg(not(feature = "spotify-session"))]
  {
    let _ = (method, path, query, body);
    Err(anyhow!("This build of spotatui has no Spotify support"))
  }
}

/// A local HTTP server in place of Spotify, for handler tests.
#[cfg(test)]
pub(crate) mod fake {
  use super::*;
  use std::cell::RefCell;

  thread_local! {
    static BASE_URL: RefCell<Option<String>> = const { RefCell::new(None) };
  }

  /// Sends this thread's Spotify calls to `base_url` until the guard drops.
  pub(crate) fn use_fake_spotify(base_url: impl Into<String>) -> FakeSpotify {
    BASE_URL.with(|url| *url.borrow_mut() = Some(base_url.into()));
    FakeSpotify
  }

  pub(crate) struct FakeSpotify;

  impl Drop for FakeSpotify {
    fn drop(&mut self) {
      BASE_URL.with(|url| *url.borrow_mut() = None);
    }
  }

  pub(super) fn base_url() -> Option<String> {
    BASE_URL.with(|url| url.borrow().clone())
  }

  pub(super) async fn request(
    base_url: &str,
    method: &Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> anyhow::Result<Value> {
    let mut url = reqwest::Url::parse(base_url)?.join(path.trim_start_matches('/'))?;
    if !query.is_empty() {
      let mut pairs = url.query_pairs_mut();
      for (key, value) in query {
        pairs.append_pair(key, value);
      }
    }
    let mut request = shared_http_client().request(method.clone(), url);
    if let Some(body) = body {
      request = request.json(body);
    }
    let response = request
      .send()
      .await
      .map_err(|e| anyhow!("Spotify API request failed: {e}"))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
      return Err(
        SpotifyApiError {
          status,
          body: text,
          detail: None,
          endpoint: None,
        }
        .into(),
      );
    }
    if text.trim().is_empty() {
      return Ok(Value::Null);
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
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
    spotify_api_request_json_for(method, path, query, body).await
  }

  pub async fn spotify_get_typed<T: DeserializeOwned>(
    &self,
    path: &str,
    query: &[(&str, String)],
  ) -> anyhow::Result<T> {
    spotify_get_typed_for(path, query).await
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

/// The error text a substring classifier may look at: everything `Display`
/// writes except the endpoint.
///
/// The endpoint carries user-supplied material — a track id containing `dns`,
/// a playlist id containing `429`, a search term — and the classifiers below
/// match on three-character substrings, so leaving it in would turn an
/// ordinary 404 into a "transport failure" or a "rate limit" at random.
///
/// The classifiers in `playback.rs` deliberately do not go through this: they
/// match on whole phrases (`no_active_device`, `restriction violated`) that a
/// URL cannot contain, and the one substring block there is fed only by the
/// fixed `/v1/me/player` endpoint. Any new classifier that matches a short
/// substring belongs here instead.
fn classifiable_text(e: &anyhow::Error) -> String {
  let text = e.to_string();
  let suffix = e
    .downcast_ref::<SpotifyApiError>()
    .and_then(|error| error.endpoint.as_deref())
    .map(|endpoint| format!(" [{endpoint}]"));
  match suffix {
    Some(suffix) => text.strip_suffix(&suffix).unwrap_or(&text).to_string(),
    None => text,
  }
}

pub fn is_rate_limited_error(e: &anyhow::Error) -> bool {
  let text = classifiable_text(e);
  text.contains("429") || text.contains("Too Many Requests") || text.contains("Too many requests")
}

/// Whether an error is the exact 403 response returned by Spotify.
///
/// Callers use this to distinguish Development Mode's playlist-content
/// restriction from authentication, transport, and other API failures. The
/// typed error is retained through `anyhow`, so this never relies on matching
/// a translated or truncated response-body string.
pub fn is_forbidden_error(e: &anyhow::Error) -> bool {
  e.downcast_ref::<SpotifyApiError>()
    .is_some_and(|error| error.status == reqwest::StatusCode::FORBIDDEN)
}

pub fn is_transient_network_error(e: &anyhow::Error) -> bool {
  let text = classifiable_text(e).to_lowercase();
  // Deliberately shorter than reqwest's full "error sending request for url
  // (…)": the request path strips the URL off that message before surfacing
  // it, since the query string carries the user's search terms. The prefix
  // still matches every text the longer form matched, so no hand-built test
  // string and no caller silently stops being classified as transient.
  text.contains("error sending request")
    || text.contains("connection reset")
    || text.contains("connection refused")
    || text.contains("timed out")
    || text.contains("temporary failure")
    || text.contains("dns")
}

#[cfg(test)]
mod transient_classification_tests {
  use super::*;

  /// reqwest writes "error sending request for url (…)"; dropping the URL to
  /// keep the search box out of the log must not stop the retry paths from
  /// recognising a transport failure.
  #[test]
  fn transient_network_error_is_recognised_without_the_url() {
    assert!(is_transient_network_error(&anyhow!(
      "Spotify API request failed: error sending request"
    )));
    assert!(is_transient_network_error(&anyhow!(
      "Spotify API request failed: error sending request for url (https://api.spotify.com/v1/me)"
    )));
  }

  /// The same message must not read as a rate limit just because an id or a
  /// search term happens to contain the digits.
  #[test]
  fn a_transport_failure_is_not_a_rate_limit() {
    assert!(!is_rate_limited_error(&anyhow!(
      "Spotify API request failed: error sending request"
    )));
  }
}

pub fn is_not_found_error(e: &anyhow::Error) -> bool {
  e.downcast_ref::<SpotifyApiError>()
    .is_some_and(|se| se.status == reqwest::StatusCode::NOT_FOUND)
}

/// A typed `GET` through [`spotify_api_request_json_for`].
pub async fn spotify_get_typed_for<T: DeserializeOwned>(
  path: &str,
  query: &[(&str, String)],
) -> anyhow::Result<T> {
  let mut value = spotify_api_request_json_for(Method::GET, path, query, None).await?;
  normalize_spotify_payload(&mut value);
  Ok(serde_json::from_value(value)?)
}
#[cfg(test)]
mod tests {
  use super::*;

  /// The call sites are the part a unit test cannot otherwise reach: the
  /// redaction helpers are covered in isolation, but nothing would notice an
  /// argument swapped for the raw body at the `debug!`/`warn!` line itself.
  #[test]
  fn request_success_line_redacts_the_request_body() {
    let body = serde_json::json!({ "access_token": "BQsecret", "uris": ["spotify:track:1"] });
    let line = request_success_line(
      "GET /v1/me/player",
      reqwest::StatusCode::OK,
      42,
      Some(&body),
    );
    assert!(line.contains("GET /v1/me/player"), "{line}");
    assert!(line.contains("42ms"), "{line}");
    assert!(line.contains("spotify:track:1"), "{line}");
    assert!(!line.contains("BQsecret"), "{line}");
  }

  #[test]
  fn request_success_line_omits_the_body_section_when_there_is_none() {
    let line = request_success_line("GET /v1/me", reqwest::StatusCode::OK, 7, None);
    assert!(!line.contains("body"), "{line}");
  }

  #[test]
  fn response_trace_line_redacts_the_response_body() {
    let line = response_trace_line(
      "POST /api/token",
      reqwest::StatusCode::OK,
      r#"{"access_token":"BQsecret","token_type":"Bearer"}"#,
    );
    assert!(!line.contains("BQsecret"), "{line}");
    assert!(line.contains("Bearer"), "{line}");
  }

  fn failure_line(include_body: bool, body: Option<&Value>, error: &str) -> String {
    FailureLine {
      include_body,
      endpoint: "PUT /v1/me/player/play",
      elapsed_ms: 13,
      body,
      error,
    }
    .render()
  }

  #[test]
  fn failure_line_redacts_the_request_body() {
    let body = serde_json::json!({ "refresh_token": "AQsecret", "position_ms": 0 });
    let line = failure_line(
      true,
      Some(&body),
      "Spotify API 403 Forbidden failed: Restricted",
    );
    assert!(!line.contains("AQsecret"), "{line}");
    assert!(line.contains("position_ms"), "{line}");
    assert!(line.contains("403"), "{line}");
    assert!(line.contains("13ms"), "{line}");
  }

  /// The `warn!` line is written without `--debug`, so a playlist name or a
  /// search term must not ride along in it by default.
  #[test]
  fn failure_line_omits_the_request_body_unless_debug_is_on() {
    let body = serde_json::json!({ "name": "My Private Playlist" });
    let line = failure_line(false, Some(&body), "failed");
    assert!(!line.contains("My Private Playlist"), "{line}");
    assert!(line.contains("--debug"), "{line}");
  }

  #[test]
  fn failure_line_bounds_a_long_error() {
    let line = failure_line(false, None, &"x".repeat(5_000));
    assert!(line.contains("(truncated)"), "{line}");
  }

  use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
  };

  async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut buf = vec![0; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf[..n]).to_string()
  }

  /// One answer from a local server; the request it got comes back on join.
  async fn serve_once(
    status: &'static str,
    body: &'static str,
  ) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let request = read_http_request(&mut stream).await;
      let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
      request
    });
    (base_url, server)
  }
  #[test]
  fn redactor_replaces_a_nested_access_token_without_walking_into_it() {
    let payload = json!({
      "device": { "id": "abc", "credentials": { "access_token": "secret" } },
      "access_token": { "value": "secret", "scopes": ["a", "b"] },
      "position_ms": 42
    });

    assert_eq!(
      redact_json(&payload),
      json!({
        "device": { "id": "abc", "credentials": { "access_token": "***" } },
        "access_token": "***",
        "position_ms": 42
      })
    );
  }

  #[test]
  fn redactor_blanks_every_denylisted_key_whatever_its_type() {
    let payload = json!({
      "refresh_token": "r",
      "id_token": "i",
      "token": 1,
      "client_secret": "c",
      "code": ["a"],
      "email": "a@b.c",
      "phone_number": "+49",
      "password": null,
      "authorization": "Bearer x"
    });

    let redacted = redact_json(&payload);
    for (_, value) in redacted.as_object().unwrap() {
      assert_eq!(value, "***", "{redacted}");
    }
  }

  /// The README and `docs/configuration.md` both tell the user which fields of
  /// their profile a `trace` log can still expose. This pins that promise to
  /// the redactor: `email` goes, `country` stays. Dropping `email` from the
  /// denylist, or adding `country` to it, makes the documentation wrong
  /// without any other test noticing.
  #[test]
  fn a_profile_response_keeps_the_country_and_blanks_the_email() {
    let profile = json!({
      "display_name": "Ada",
      "email": "a@b.c",
      "country": "DE",
      "id": "ada"
    });

    assert_eq!(
      redact_json(&profile),
      json!({
        "display_name": "Ada",
        "email": "***",
        "country": "DE",
        "id": "ada"
      })
    );
  }

  #[test]
  fn redactor_matches_keys_case_insensitively() {
    let payload = json!({ "Access_Token": "s", "AUTHORIZATION": "s" });

    assert_eq!(
      redact_json(&payload),
      json!({ "Access_Token": "***", "AUTHORIZATION": "***" })
    );
  }

  /// A deliberate decision: the denylist matches whole keys, so the useful
  /// `token_type` / `email_verified` fields survive. A substring match would
  /// blank half of every payload and make the log unreadable.
  #[test]
  fn redactor_keeps_a_key_that_merely_contains_a_denylisted_word() {
    let payload = json!({ "token_type": "Bearer", "email_verified": true, "encoded": "x" });

    assert_eq!(redact_json(&payload), payload);
  }

  #[test]
  fn redactor_walks_arrays_of_objects() {
    let payload = json!({
      "items": [
        { "uri": "spotify:track:1", "token": "s" },
        { "uri": "spotify:track:2", "owner": { "email": "a@b.c" } }
      ]
    });

    assert_eq!(
      redact_json(&payload),
      json!({
        "items": [
          { "uri": "spotify:track:1", "token": "***" },
          { "uri": "spotify:track:2", "owner": { "email": "***" } }
        ]
      })
    );
  }

  #[test]
  fn redactor_returns_non_object_input_unchanged() {
    for value in [
      json!("token"),
      json!(7),
      json!(null),
      json!(true),
      json!(["token", "email"]),
    ] {
      assert_eq!(redact_json(&value), value);
    }
  }

  #[test]
  fn redacted_target_keeps_the_query_and_blanks_denylisted_parameters() {
    let query = [("q", "abba".to_string()), ("type", "track".to_string())];
    assert_eq!(
      redacted_target("search", &query),
      "/v1/search?q=***&type=track"
    );

    let query = [
      ("code", "xyz".to_string()),
      ("grant_type", "pkce".to_string()),
    ];
    assert_eq!(
      redacted_target("token", &query),
      "/v1/token?code=***&grant_type=pkce"
    );
  }

  /// A separator or newline put in verbatim would read as an extra parameter,
  /// or as an extra log line.
  #[test]
  fn redacted_target_encodes_separators_and_newlines() {
    let target = redacted_target("search", &[("market", "a&b\nfake=1".to_string())]);
    assert!(!target.contains('\n'), "{target}");
    assert_eq!(target, "/v1/search?market=a%26b%0Afake%3D1");
  }

  #[test]
  fn redacted_target_is_only_the_path_when_there_is_no_query() {
    assert_eq!(redacted_target("me/player", &[]), "/v1/me/player");
  }
  #[test]
  fn a_logged_request_body_is_redacted_and_bounded() {
    assert_eq!(request_body_for_log(None), "");

    let line = request_body_for_log(Some(
      &json!({ "uris": ["spotify:track:1"], "token": "supersecret" }),
    ));
    assert!(line.contains(r#""token":"***""#), "{line}");
    assert!(!line.contains("supersecret"), "{line}");
    assert!(line.contains("spotify:track:1"), "{line}");

    let long = json!({ "note": "x".repeat(MAX_LOGGED_BODY_CHARS * 2) });
    assert!(
      request_body_for_log(Some(&long)).contains("(truncated)"),
      "an unbounded body reached the log"
    );
  }

  #[test]
  fn a_logged_response_body_falls_back_to_plain_truncation_when_it_is_not_json() {
    assert_eq!(
      response_body_for_log(r#"{"error":{"message":"nope"},"access_token":"s"}"#),
      r#"{"access_token":"***","error":{"message":"nope"}}"#
    );
    assert_eq!(
      response_body_for_log("  <html>502</html> "),
      "<html>502</html>"
    );
  }

  #[test]
  fn the_error_text_names_the_endpoint_that_failed() {
    let error = anyhow::Error::from(SpotifyApiError {
      status: reqwest::StatusCode::FORBIDDEN,
      body: "Player command failed".to_string(),
      detail: None,
      endpoint: Some("PUT /v1/me/player/play?device_id=abc".to_string()),
    });

    assert_eq!(
      error.to_string(),
      "Spotify API 403 Forbidden failed: Player command failed [PUT /v1/me/player/play?device_id=abc]"
    );
  }

  #[test]
  fn the_endpoint_is_appended_behind_the_detail() {
    let error = anyhow::Error::from(SpotifyApiError {
      status: reqwest::StatusCode::UNAUTHORIZED,
      body: "expired".to_string(),
      detail: Some("token refresh failed: no network".to_string()),
      endpoint: Some("GET /v1/me".to_string()),
    });

    assert_eq!(
      error.to_string(),
      "Spotify API 401 Unauthorized failed: expired (token refresh failed: no network) [GET /v1/me]"
    );
  }

  /// The endpoint carries ids and search terms the user chose, and the
  /// classifiers match three-character substrings. A track id containing
  /// `dns`, or a playlist id containing `429`, must not turn a plain 404 into
  /// a transport failure or a rate limit.
  #[test]
  fn user_supplied_ids_in_the_endpoint_are_not_read_as_a_failure_class() {
    let error = anyhow::Error::from(SpotifyApiError {
      status: reqwest::StatusCode::NOT_FOUND,
      body: "not found".to_string(),
      detail: None,
      endpoint: Some("GET /v1/playlists/3dns429xTimedOut/tracks?q=connection+reset".to_string()),
    });

    assert!(!is_transient_network_error(&error), "{error}");
    assert!(!is_rate_limited_error(&error), "{error}");
    assert!(is_not_found_error(&error));
  }

  #[test]
  fn a_real_rate_limit_is_still_classified_with_an_endpoint_attached() {
    let error = anyhow::Error::from(SpotifyApiError {
      status: reqwest::StatusCode::TOO_MANY_REQUESTS,
      body: "rate limited by Spotify, retry in 17s".to_string(),
      detail: None,
      endpoint: Some("GET /v1/me/player".to_string()),
    });

    assert!(is_rate_limited_error(&error), "{error}");
  }

  #[test]
  fn forbidden_error_classification_uses_status_not_body_text() {
    let error = anyhow::Error::new(SpotifyApiError {
      status: reqwest::StatusCode::FORBIDDEN,
      body: "playlist contents unavailable".to_string(),
      detail: None,
      endpoint: None,
    });
    assert!(is_forbidden_error(&error));
    assert!(!is_rate_limited_error(&error));
    assert!(!is_forbidden_error(&anyhow::anyhow!(
      "Spotify API 403 Forbidden"
    )));
  }

  #[test]
  fn not_found_is_recognized_but_rate_limit_is_not() {
    let not_found = anyhow::Error::from(SpotifyApiError {
      status: reqwest::StatusCode::NOT_FOUND,
      body: String::new(),
      detail: None,
      endpoint: None,
    });
    let rate_limited = anyhow::Error::from(SpotifyApiError {
      status: reqwest::StatusCode::TOO_MANY_REQUESTS,
      body: String::new(),
      detail: None,
      endpoint: None,
    });
    assert!(is_not_found_error(&not_found));
    assert!(!is_not_found_error(&rate_limited));
  }

  /// Issue #566: `Spotify API 403 Forbidden failed: …` never said which call
  /// broke. The failing call's method, path *and* query now travel with it.
  #[tokio::test]
  async fn a_failed_request_names_the_endpoint_it_used() {
    let (base_url, server) = serve_once(
      "403 Forbidden",
      r#"{"error":{"status":403,"message":"Player command failed"}}"#,
    )
    .await;
    let _fake = fake::use_fake_spotify(base_url);

    let error = request_with(
      &RateLimitGate::default(),
      SPOTIFY_REQUEST_DEADLINE,
      Method::PUT,
      "me/player/play",
      &[("device_id", "device-1".to_string())],
      Some(serde_json::json!({ "uris": ["spotify:track:1"] })),
    )
    .await
    .unwrap_err();
    let request = server.await.unwrap();

    assert!(
      request.starts_with("PUT /v1/me/player/play?device_id=device-1"),
      "{request}"
    );
    assert!(
      error
        .to_string()
        .ends_with("[PUT /v1/me/player/play?device_id=device-1]"),
      "unexpected error text: {error}"
    );
    assert!(is_forbidden_error(&error));
  }

  /// A 429 opens the window: the call fails at once and the next one fails
  /// without a request.
  #[tokio::test]
  async fn a_429_fails_at_once_and_blocks_the_next_call_locally() {
    let (base_url, server) =
      serve_once("429 Too Many Requests", r#"{"error":{"status":429}}"#).await;
    let _fake = fake::use_fake_spotify(base_url);
    let gate = RateLimitGate::default();
    let call = || {
      request_with(
        &gate,
        SPOTIFY_REQUEST_DEADLINE,
        Method::GET,
        "me/player",
        &[],
        None,
      )
    };

    let started_at = Instant::now();
    let first = call().await.unwrap_err();
    server.await.unwrap();
    let second = call().await.unwrap_err();

    assert!(
      started_at.elapsed() < Duration::from_secs(5),
      "a 429 must not sleep"
    );
    assert!(is_rate_limited_error(&first));
    assert!(is_rate_limited_error(&second));
    assert!(second.to_string().contains("retry in"), "{second}");
    let left = gate.rate_limit_remaining().await.unwrap();
    assert!(left <= ROUTED_RATE_LIMIT_WINDOW, "{left:?}");
  }

  #[tokio::test]
  async fn a_stalled_response_gives_up_at_the_deadline_as_a_transient_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let _ = read_http_request(&mut stream).await;
      tokio::time::sleep(Duration::from_secs(10)).await;
    });
    let _fake = fake::use_fake_spotify(base_url);

    let started_at = Instant::now();
    let error = request_with(
      &RateLimitGate::default(),
      Duration::from_millis(300),
      Method::GET,
      "me/player",
      &[],
      None,
    )
    .await
    .unwrap_err();
    server.abort();

    assert!(started_at.elapsed() < Duration::from_secs(3));
    assert!(is_transient_network_error(&error), "{error}");
  }

  #[tokio::test]
  async fn an_empty_success_body_reads_as_null() {
    let (base_url, server) = serve_once("200 OK", "").await;
    let _fake = fake::use_fake_spotify(base_url);

    let value = request_with(
      &RateLimitGate::default(),
      SPOTIFY_REQUEST_DEADLINE,
      Method::PUT,
      "me/player/pause",
      &[],
      None,
    )
    .await
    .unwrap();
    server.await.unwrap();

    assert_eq!(value, Value::Null);
  }

  #[cfg(feature = "spotify-session")]
  #[tokio::test]
  async fn without_a_login_a_call_is_unauthorized() {
    let error = request_with(
      &RateLimitGate::default(),
      SPOTIFY_REQUEST_DEADLINE,
      Method::GET,
      "me",
      &[],
      None,
    )
    .await
    .unwrap_err();
    assert_eq!(
      error.downcast_ref::<SpotifyApiError>().map(|e| e.status),
      Some(StatusCode::UNAUTHORIZED)
    );
  }

  #[tokio::test]
  async fn a_shorter_window_never_cuts_a_longer_one_short() {
    let gate = RateLimitGate::default();
    gate.rate_limit_for(Duration::from_secs(30)).await;
    gate.rate_limit_for(Duration::from_secs(1)).await;

    let left = gate.rate_limit_remaining().await.unwrap();
    assert!(left > Duration::from_secs(20), "{left:?}");
  }

  #[tokio::test]
  async fn an_extreme_window_is_bounded() {
    let gate = RateLimitGate::default();
    gate.rate_limit_for(Duration::MAX).await;

    let left = gate.rate_limit_remaining().await.unwrap();
    assert!(left <= MAX_RETRY_AFTER, "{left:?}");
  }
}
