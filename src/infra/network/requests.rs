use super::Network;
use crate::core::{app::App, auth, spotify_access::restricted_endpoint};
use anyhow::anyhow;
use log::{debug, trace, warn};
use reqwest::header::CONTENT_LENGTH;
use reqwest::{Method, StatusCode};
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

/// Longest `Retry-After` honored: a bound on the window, so a bogus header
/// value can neither park the app for a day nor overflow the deadline.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(600);

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
fn redacted_target(url: &reqwest::Url) -> String {
  let path = url.path().to_string();
  let mut pairs = url.query_pairs().peekable();
  if pairs.peek().is_none() {
    return path;
  }
  // Re-encoded rather than concatenated: `query_pairs` hands back decoded
  // values, so a `&` or a newline inside one would split the logged endpoint
  // into something that reads like two parameters or two log lines.
  let mut query = url::form_urlencoded::Serializer::new(String::new());
  for (key, value) in pairs {
    if is_redacted_query_key(&key) {
      query.append_pair(&key, REDACTED_PLACEHOLDER);
    } else {
      query.append_pair(&key, &value);
    }
  }
  format!("{path}?{}", query.finish())
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

/// Everything the `warn!` line for a non-2xx response needs. Grouped into a
/// struct because the line carries eight values and the point of extracting it
/// is testability, not a shorter signature.
struct FailureLine<'a> {
  /// Whether the request body may go in. The body carries user-entered text
  /// (a playlist name, a search term), and this line is logged at `warn`,
  /// which is on without `--debug` — so the body is opt-in like everything
  /// else the debug mode adds.
  include_body: bool,
  endpoint: &'a str,
  status: reqwest::StatusCode,
  elapsed_ms: u128,
  attempt: u8,
  max_attempts: u8,
  token_age: Option<std::time::Duration>,
  body: Option<&'a Value>,
  response_body: &'a str,
}

impl FailureLine<'_> {
  fn render(&self) -> String {
    let token_age = self.token_age.map_or_else(
      || "unknown".to_string(),
      |age| format!("{}s", age.as_secs()),
    );
    format!(
      "Spotify API {} -> {} in {}ms (attempt {}/{}, token age {}){}: {}",
      self.endpoint,
      self.status,
      self.elapsed_ms,
      self.attempt,
      self.max_attempts,
      token_age,
      self.body_for_log(),
      response_body_for_log(self.response_body)
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

/// Cooldown state for forced token refreshes triggered by a 401 response.
///
/// Deliberately long-lived and shared: every request is a fresh call into
/// [`spotify_api_request_json_for_base_with_refresh`], so per-call state would
/// reset on each poll and never suppress anything.
#[derive(Clone)]
pub struct ForcedRefreshGate {
  last_forced_refresh: Arc<Mutex<Option<Instant>>>,
  cooldown: Duration,
  /// End of the last `Retry-After` window: no request goes out before it.
  rate_limited_until: Arc<Mutex<Option<Instant>>>,
}

impl ForcedRefreshGate {
  pub fn new(cooldown: Duration) -> Self {
    Self {
      last_forced_refresh: Arc::new(Mutex::new(None)),
      cooldown,
      rate_limited_until: Arc::new(Mutex::new(None)),
    }
  }

  /// Time left in the `Retry-After` window, if one is open.
  pub(crate) async fn rate_limit_remaining(&self) -> Option<Duration> {
    let until = (*self.rate_limited_until.lock().await)?;
    until.checked_duration_since(Instant::now())
  }

  /// Open (or extend) the window: a shorter `Retry-After` that lands during a
  /// longer one must not cut the longer one short.
  pub(crate) async fn rate_limit_for(&self, window: Duration) {
    let until = Instant::now() + window.min(MAX_RETRY_AFTER);
    let mut slot = self.rate_limited_until.lock().await;
    *slot = Some(slot.map_or(until, |current| current.max(until)));
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

/// The process-wide gate used by every real Spotify request (one Spotify
/// session per process). Tests drive the base helper with their own instance.
pub(crate) fn shared_forced_refresh_gate() -> &'static ForcedRefreshGate {
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
  // Key-tier gate: every caller funnels through here, so a surface that forgot
  // to pre-check still cannot spend a refused request. Answered before the
  // token, pacing and socket, so callers see the same 403 as Spotify's own.
  if let Some(endpoint) = restricted_endpoint(method.as_str(), path, query) {
    let blocked = app.lock().await.spotify_endpoint_blocked(endpoint);
    if blocked {
      return Err(
        SpotifyApiError {
          status: StatusCode::FORBIDDEN,
          body: "Forbidden".to_string(),
          detail: Some(endpoint.unavailable_note().to_string()),
          endpoint: None,
        }
        .into(),
      );
    }
  }

  let base_url = &spotify.config.api_base_url;

  spotify_api_request_json_for_base_with_refresh(
    spotify,
    SpotifyApiRequest {
      base_url,
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

  // One redacted `METHOD /path?query` for every log line and error below.
  let endpoint = format!("{} {}", method, redacted_target(&url));

  // Inside a `Retry-After` window every call fails here, at once and with no
  // request: a sleep would park the serial pump and every event behind it.
  if let Some(left) = forced_refresh_gate.rate_limit_remaining().await {
    // Worth one line: without it the debug log just goes quiet for the length
    // of the window, with no request to explain the gap.
    debug!(
      "Spotify API {endpoint} not sent: rate limited for another {}s",
      left.as_secs().max(1)
    );
    return Err(
      SpotifyApiError {
        status: reqwest::StatusCode::TOO_MANY_REQUESTS,
        body: format!(
          "rate limited by Spotify, retry in {}s",
          left.as_secs().max(1)
        ),
        detail: None,
        endpoint: Some(endpoint.clone()),
      }
      .into(),
    );
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

    let attempt_started_at = Instant::now();
    let response = match request.send().await {
      Ok(response) => response,
      Err(e) => {
        if attempt + 1 < max_attempts && (e.is_connect() || e.is_timeout() || e.is_request()) {
          let backoff_secs = 1 + u64::from(attempt);
          tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
          attempt += 1;
          continue;
        }
        // Which call broke goes in the log, not in the message: reqwest's
        // `Display` carries the full request URL, and the query string holds
        // the search box, while this message is surfaced on the error screen
        // and recorded by `App::handle_error` at `info` — both without
        // `--debug`. `without_url` drops it; `is_transient_network_error`
        // matches what is left.
        let e = e.without_url();
        warn!(
          "{} transport failure after {:?} on attempt {}/{}: {}",
          endpoint,
          attempt_started_at.elapsed(),
          attempt + 1,
          max_attempts,
          e
        );
        return Err(anyhow!("Spotify API request failed: {}", e));
      }
    };
    let elapsed = attempt_started_at.elapsed();
    let status = response.status();
    if status.is_success() {
      // The one line every successful call leaves behind (issue #566): what ran
      // before the failure that is being reported. `log!` only formats its
      // arguments once the level is enabled, so the redaction below costs
      // nothing with debug off.
      debug!(
        "{}",
        request_success_line(&endpoint, status, elapsed.as_millis(), body.as_ref())
      );
      let should_parse_json = response_is_json(&response);
      let response_body = response.text().await?;
      // Response bodies are the largest PII surface in the whole log and are
      // rarely what a successful call is diagnosed by, so they stay at trace.
      trace!("{}", response_trace_line(&endpoint, status, &response_body));
      if response_body.trim().is_empty() {
        return Ok(Value::Null);
      }
      if should_parse_json {
        return Ok(serde_json::from_str(&response_body)?);
      }
      return Ok(Value::Null);
    }

    let retry_after_secs = response
      .headers()
      .get("retry-after")
      .and_then(|h| h.to_str().ok())
      .and_then(|v| v.parse::<u64>().ok())
      .unwrap_or(1);
    // `text()` consumes the response, so everything the branches below need
    // (status, retry-after, body) is captured here, once.
    let response_body = response.text().await.unwrap_or_default();

    // Diagnostics for every non-2xx: which endpoint (path *and* query), which
    // status, what was asked for, what Spotify actually said, and how old the
    // token we attached was. Failure is the case that gets reported, so it
    // carries the full picture at any level; the token *value* is never logged.
    // This is what separates "the token really expired" from "Spotify rejected
    // a valid token" in a user-supplied log (issue #395).
    // Built outside the macro on purpose: a struct of references costs nothing
    // on a path that just did HTTP, and it puts these lines on the executed
    // path of the failure tests below. `render()` stays inside, so the
    // redaction still only runs when the level is enabled.
    let failure = FailureLine {
      include_body: log::max_level() >= log::LevelFilter::Debug,
      endpoint: &endpoint,
      status,
      elapsed_ms: elapsed.as_millis(),
      attempt: attempt + 1,
      max_attempts,
      token_age: access_token_age,
      body: body.as_ref(),
      response_body: &response_body,
    };
    warn!("{}", failure.render());

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
            return Err(
              SpotifyApiError {
                status,
                body: response_body,
                detail: Some("token refresh unavailable for this request".to_string()),
                endpoint: Some(endpoint),
              }
              .into(),
            );
          }
          Err(refresh_err) => {
            return Err(
              SpotifyApiError {
                status,
                body: response_body,
                detail: Some(format!("token refresh failed: {refresh_err}")),
                endpoint: Some(endpoint),
              }
              .into(),
            );
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

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
      let window = retry_after_secs.max(1).min(MAX_RETRY_AFTER.as_secs());
      forced_refresh_gate
        .rate_limit_for(Duration::from_secs(window))
        .await;
      // The raw body went to the log above; the user sees the wait instead.
      return Err(
        SpotifyApiError {
          status,
          body: format!("rate limited by Spotify, retry in {window}s"),
          detail: None,
          endpoint: Some(endpoint),
        }
        .into(),
      );
    }

    return Err(
      SpotifyApiError {
        status,
        body: response_body,
        detail: None,
        endpoint: Some(endpoint),
      }
      .into(),
    );
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

/// Whether an error is Spotify refusing the endpoint because of the key's tier.
pub fn is_restricted_client_id_error(e: &anyhow::Error) -> bool {
  is_forbidden_error(e) || is_not_found_error(e)
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

/// The boot-time token check: the same pacing, `Retry-After` retries, and 401
/// recovery as every other call, for the one request made before an `App`
/// exists to report refresh state to.
pub async fn spotify_get_typed_before_app<T: DeserializeOwned>(
  spotify: &AuthCodePkceSpotify,
  path: &str,
  token_cache_path: &Path,
) -> anyhow::Result<T> {
  let mut value = spotify_api_request_json_for_base_with_refresh(
    spotify,
    SpotifyApiRequest {
      base_url: SPOTIFY_API_BASE_URL,
      method: Method::GET,
      path,
      query: &[],
      body: None,
    },
    |force| async move {
      auth::refresh_token_and_cache(spotify, token_cache_path, force)
        .await
        .map(Some)
    },
    // Its own gate: nothing else uses this client yet, and a fallback
    // candidate must not inherit the previous candidate's 401 cooldown.
    &ForcedRefreshGate::default(),
  )
  .await?;
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

  fn failure_line(body: Option<&Value>, response_body: &str) -> String {
    FailureLine {
      include_body: true,
      endpoint: "PUT /v1/me/player/play",
      status: reqwest::StatusCode::FORBIDDEN,
      elapsed_ms: 13,
      attempt: 2,
      max_attempts: 3,
      token_age: Some(std::time::Duration::from_secs(90)),
      body,
      response_body,
    }
    .render()
  }

  #[test]
  fn failure_line_redacts_both_bodies() {
    let body = serde_json::json!({ "refresh_token": "AQsecret", "position_ms": 0 });
    let line = failure_line(
      Some(&body),
      r#"{"error":{"status":403},"password":"hunter2"}"#,
    );
    assert!(!line.contains("AQsecret"), "{line}");
    assert!(!line.contains("hunter2"), "{line}");
    assert!(line.contains("position_ms"), "{line}");
    assert!(line.contains("403"), "{line}");
  }

  /// The `warn!` line is written without `--debug`, so a playlist name or a
  /// search term must not ride along in it by default.
  #[test]
  fn failure_line_omits_the_request_body_unless_debug_is_on() {
    let body = serde_json::json!({ "name": "My Private Playlist" });
    let line = FailureLine {
      include_body: false,
      endpoint: "POST /v1/users/me/playlists",
      status: reqwest::StatusCode::FORBIDDEN,
      elapsed_ms: 5,
      attempt: 1,
      max_attempts: 1,
      token_age: None,
      body: Some(&body),
      response_body: "",
    }
    .render();
    assert!(!line.contains("My Private Playlist"), "{line}");
    assert!(line.contains("--debug"), "{line}");
  }

  #[test]
  fn failure_line_reports_attempt_and_token_age() {
    let line = failure_line(None, "");
    assert!(line.contains("attempt 2/3"), "{line}");
    assert!(line.contains("token age 90s"), "{line}");
  }

  #[test]
  fn failure_line_says_unknown_when_the_token_age_is_unavailable() {
    let line = FailureLine {
      include_body: true,
      endpoint: "GET /v1/me",
      status: reqwest::StatusCode::UNAUTHORIZED,
      elapsed_ms: 1,
      attempt: 1,
      max_attempts: 1,
      token_age: None,
      body: None,
      response_body: "",
    }
    .render();
    assert!(line.contains("token age unknown"), "{line}");
  }
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
    let url = reqwest::Url::parse("https://api.spotify.com/v1/search?q=abba&type=track").unwrap();
    assert_eq!(redacted_target(&url), "/v1/search?q=***&type=track");

    let url =
      reqwest::Url::parse("https://accounts.spotify.com/api/token?code=xyz&grant_type=pkce")
        .unwrap();
    assert_eq!(redacted_target(&url), "/api/token?code=***&grant_type=pkce");
  }

  /// A decoded separator or newline put back verbatim would read as an extra
  /// parameter, or as an extra log line.
  #[test]
  fn redacted_target_re_encodes_separators_and_newlines() {
    let url =
      reqwest::Url::parse("https://api.spotify.com/v1/search?market=a%26b%0Afake=1").unwrap();
    let target = redacted_target(&url);
    assert!(!target.contains('\n'), "{target}");
    assert_eq!(target, "/v1/search?market=a%26b%0Afake%3D1");
  }

  #[test]
  fn redacted_target_is_only_the_path_when_there_is_no_query() {
    let url = reqwest::Url::parse("https://api.spotify.com/v1/me/player").unwrap();
    assert_eq!(redacted_target(&url), "/v1/me/player");
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

  #[test]
  fn restricted_key_classification_accepts_both_refusal_statuses() {
    for status in [
      reqwest::StatusCode::FORBIDDEN,
      reqwest::StatusCode::NOT_FOUND,
    ] {
      let error = anyhow::Error::new(SpotifyApiError {
        status,
        body: "Forbidden".to_string(),
        detail: None,
        endpoint: None,
      });
      assert!(is_restricted_client_id_error(&error));
    }

    assert!(!is_restricted_client_id_error(&anyhow::anyhow!(
      "transport failure"
    )));
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

  /// Issue #566: `Spotify API 403 Forbidden failed: …` never said which call
  /// broke. The failing call's method, path *and* query now travel with it.
  #[tokio::test]
  async fn a_failed_request_names_the_endpoint_it_used() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());

    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let _ = read_http_request(&mut stream).await;
      let body = r#"{"error":{"status":403,"message":"Player command failed"}}"#;
      let response = format!(
        "HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });

    let spotify = spotify_with_access_token("access").await;
    let error = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      SpotifyApiRequest {
        base_url: &base_url,
        method: Method::PUT,
        path: "me/player/play",
        query: &[("device_id", "device-1".to_string())],
        body: Some(json!({ "uris": ["spotify:track:1"] })),
      },
      |_force| async move { Ok(Some(SystemTime::now() + Duration::from_secs(3600))) },
      &ForcedRefreshGate::default(),
    )
    .await
    .unwrap_err();
    server.await.unwrap();

    assert!(
      error
        .to_string()
        .ends_with("[PUT /v1/me/player/play?device_id=device-1]"),
      "unexpected error text: {error}"
    );
    assert!(is_forbidden_error(&error));
  }

  /// A 429 opens a `Retry-After` window on the gate: the call fails at once
  /// (no sleep on the pump) and the next call fails without a request.
  #[tokio::test]
  async fn a_429_fails_at_once_and_blocks_the_next_call_locally() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let _ = read_http_request(&mut stream).await;
      let body = r#"{"error":{"status":429}}"#;
      let response = format!(
        "HTTP/1.1 429 Too Many Requests\r\nretry-after: 30\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });

    let spotify = spotify_with_access_token("access").await;
    let gate = ForcedRefreshGate::default();
    let request = || SpotifyApiRequest {
      base_url: &base_url,
      method: Method::GET,
      path: "me/player",
      query: &[],
      body: None,
    };
    let refresh = |_force| async move { Ok(Some(SystemTime::now() + Duration::from_secs(3600))) };

    let started_at = Instant::now();
    let first = spotify_api_request_json_for_base_with_refresh(&spotify, request(), refresh, &gate)
      .await
      .unwrap_err();
    server.await.unwrap();
    let second =
      spotify_api_request_json_for_base_with_refresh(&spotify, request(), refresh, &gate)
        .await
        .unwrap_err();

    assert!(
      started_at.elapsed() < Duration::from_secs(5),
      "a 429 must not sleep"
    );
    assert!(is_rate_limited_error(&first));
    assert!(is_rate_limited_error(&second));
    assert!(second.to_string().contains("retry in"), "{second}");
  }

  #[tokio::test]
  async fn a_shorter_window_never_cuts_a_longer_one_short() {
    let gate = ForcedRefreshGate::default();
    gate.rate_limit_for(Duration::from_secs(30)).await;
    gate.rate_limit_for(Duration::from_secs(1)).await;

    let left = gate.rate_limit_remaining().await.unwrap();
    assert!(left > Duration::from_secs(20), "{left:?}");
  }

  #[tokio::test]
  async fn an_extreme_retry_after_is_bounded_and_does_not_panic() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let _ = read_http_request(&mut stream).await;
      let response = format!(
        "HTTP/1.1 429 Too Many Requests\r\nretry-after: {}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
        u64::MAX
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });

    let spotify = spotify_with_access_token("access").await;
    let gate = ForcedRefreshGate::default();
    let refresh = |_force| async move { Ok(Some(SystemTime::now() + Duration::from_secs(3600))) };
    let error = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      SpotifyApiRequest {
        base_url: &base_url,
        method: Method::GET,
        path: "me/player",
        query: &[],
        body: None,
      },
      refresh,
      &gate,
    )
    .await
    .unwrap_err();
    server.await.unwrap();

    assert!(is_rate_limited_error(&error));
    let left = gate.rate_limit_remaining().await.unwrap();
    assert!(left <= MAX_RETRY_AFTER, "{left:?}");
  }
}
