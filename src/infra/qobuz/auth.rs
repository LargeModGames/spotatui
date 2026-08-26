//! Bundle constants and the credentials file.
//!
//! The three constants the API needs (app id, signature secret, OAuth key) are
//! served to every visitor of the Qobuz web player; they are scraped from its
//! bundle at runtime, cached by bundle version, and never embedded here. Env
//! overrides skip the scrape when all three are set.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::GeneralPurpose;
use base64::engine::{DecodePaddingMode, GeneralPurposeConfig};
use base64::{alphabet, Engine};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub use crate::core::state::QobuzBundleCache;

const LOGIN_URL: &str = "https://play.qobuz.com/login";
const PLAY_ORIGIN: &str = "https://play.qobuz.com";
const BUNDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

pub const APP_ID_ENV: &str = "SPOTATUI_QOBUZ_APP_ID";
pub const APP_SECRET_ENV: &str = "SPOTATUI_QOBUZ_APP_SECRET";
pub const OAUTH_KEY_ENV: &str = "SPOTATUI_QOBUZ_OAUTH_KEY";
/// Overrides the saved user token (the value of `X-User-Auth-Token`).
pub const TOKEN_ENV: &str = "SPOTATUI_QOBUZ_TOKEN";

/// Base64 that accepts both alphabets, with or without padding.
const LENIENT_BASE64: GeneralPurpose = GeneralPurpose::new(
  &alphabet::STANDARD,
  GeneralPurposeConfig::new()
    .with_decode_allow_trailing_bits(true)
    .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Decode base64 or base64url, padded or not.
pub fn decode_base64(text: &str) -> Result<Vec<u8>> {
  let standard: String = text
    .trim_end_matches('=')
    .chars()
    .map(|c| match c {
      '-' => '+',
      '_' => '/',
      other => other,
    })
    .collect();
  LENIENT_BASE64
    .decode(standard)
    .map_err(|e| anyhow!("base64 decode failed: {e}"))
}

// ---------------------------------------------------------------------------
// Bundle constants
// ---------------------------------------------------------------------------

/// The constants from the env overrides, when all three are set and non-empty.
pub fn constants_from_env_with(
  app_id: Option<String>,
  app_secret: Option<String>,
  oauth_key: Option<String>,
) -> Option<QobuzBundleCache> {
  let non_empty = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
  Some(QobuzBundleCache {
    bundle_version: "env".to_string(),
    app_id: non_empty(app_id)?,
    app_secret: non_empty(app_secret)?,
    oauth_key: non_empty(oauth_key)?,
  })
}

fn constants_from_env() -> Option<QobuzBundleCache> {
  constants_from_env_with(
    std::env::var(APP_ID_ENV).ok(),
    std::env::var(APP_SECRET_ENV).ok(),
    std::env::var(OAUTH_KEY_ENV).ok(),
  )
}

/// The bundle path (`/resources/<version>/bundle.js`) from the login page.
pub fn bundle_path(login_html: &str) -> Option<String> {
  Regex::new(r#"src="(/resources/[^"/]+/bundle\.js)""#)
    .ok()?
    .captures(login_html)
    .map(|c| c[1].to_string())
}

/// The version segment of a bundle path.
pub fn bundle_version(path: &str) -> Option<String> {
  path
    .strip_prefix("/resources/")?
    .strip_suffix("/bundle.js")
    .map(str::to_string)
}

fn capture(js: &str, pattern: &str, what: &str) -> Result<String> {
  Regex::new(pattern)?
    .captures(js)
    .map(|c| c[1].to_string())
    .ok_or_else(|| anyhow!("{what} not found in the Qobuz bundle"))
}

/// Extract the three constants from the bundle source.
pub fn parse_bundle(js: &str, version: &str) -> Result<QobuzBundleCache> {
  let app_id = capture(js, r#"production:\{api:\{appId:"(\d+)""#, "app id")?;
  // ASCII classes only: the captures are sliced by byte index further down.
  let seed_re = Regex::new(r#"initialSeed\("([A-Za-z0-9+/=]+)",window\.utimezone\.([A-Za-z]+)\)"#)?;
  let (seed, timezone) = seed_re
    .captures_iter(js)
    .last()
    .map(|c| (c[1].to_string(), c[2].to_string()))
    .ok_or_else(|| anyhow!("signature seed not found in the Qobuz bundle"))?;
  let mut zone = timezone.chars();
  let zone_name: String = zone
    .next()
    .map(|c| c.to_uppercase().collect::<String>())
    .unwrap_or_default()
    + zone.as_str();
  let zone_re = Regex::new(&format!(
    r#"name:"[A-Za-z_]+/{zone_name}",info:"([A-Za-z0-9+/=]+)",extras:"([A-Za-z0-9+/=]+)""#
  ))?;
  let caps = zone_re
    .captures(js)
    .ok_or_else(|| anyhow!("timezone block {zone_name} not found in the Qobuz bundle"))?;
  let app_secret = secret_from_seed(&seed, &caps[1], &caps[2])?;
  let oauth_key = capture(js, r#"authenticate\(\{privateKey:"([^"]+)""#, "OAuth key")?;
  Ok(QobuzBundleCache {
    bundle_version: version.to_string(),
    app_id,
    app_secret,
    oauth_key,
  })
}

/// The signature secret: base64 of `seed + info + extras` minus its last 44 chars.
fn secret_from_seed(seed: &str, info: &str, extras: &str) -> Result<String> {
  let joined = format!("{seed}{info}{extras}");
  let keep = joined
    .len()
    .checked_sub(44)
    .ok_or_else(|| anyhow!("signature seed is too short"))?;
  let secret = String::from_utf8(decode_base64(&joined[..keep])?)
    .map_err(|_| anyhow!("signature secret is not text"))?;
  let is_hex = secret.len() == 32
    && secret
      .bytes()
      .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
  if !is_hex {
    return Err(anyhow!("signature secret has an unexpected shape"));
  }
  let seed_prefix = decode_base64(&seed[..seed.len() / 4 * 4])?;
  if !secret.as_bytes().starts_with(&seed_prefix) {
    return Err(anyhow!("signature secret does not start with its seed"));
  }
  Ok(secret)
}

static RESOLVED: tokio::sync::OnceCell<QobuzBundleCache> = tokio::sync::OnceCell::const_new();

/// Whether [`resolve_constants`] already succeeded in this process.
pub fn constants_resolved() -> bool {
  RESOLVED.get().is_some()
}

/// Resolve the constants once per process: env, then the cached version, then a scrape.
pub async fn resolve_constants(
  http: &Client,
  cached: Option<&QobuzBundleCache>,
) -> Result<QobuzBundleCache> {
  RESOLVED
    .get_or_try_init(|| async {
      if let Some(constants) = constants_from_env() {
        return Ok(constants);
      }
      let html = http
        .get(LOGIN_URL)
        .send()
        .await
        .context("login page request")?
        .error_for_status()
        .context("login page")?
        .text()
        .await
        .context("login page body")?;
      let path =
        bundle_path(&html).ok_or_else(|| anyhow!("bundle path not found in the login page"))?;
      let version = bundle_version(&path).ok_or_else(|| anyhow!("bundle path has no version"))?;
      if let Some(constants) = cached.filter(|c| c.bundle_version == version) {
        return Ok(constants.clone());
      }
      log::info!("[qobuz] scraping bundle {version}");
      let js = http
        .get(format!("{PLAY_ORIGIN}{path}"))
        .timeout(BUNDLE_TIMEOUT)
        .send()
        .await
        .context("bundle request")?
        .error_for_status()
        .context("bundle")?
        .text()
        .await
        .context("bundle body")?;
      parse_bundle(&js, &version)
    })
    .await
    .cloned()
}

// ---------------------------------------------------------------------------
// Credentials file
// ---------------------------------------------------------------------------

/// The saved login: the token every Qobuz call carries, plus the user id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QobuzCredentials {
  pub user_auth_token: String,
  #[serde(default)]
  pub user_id: Option<String>,
}

/// Read the credentials file; `None` when missing, unreadable, or without a token.
pub fn read_credentials(path: &Path) -> Option<QobuzCredentials> {
  std::fs::read_to_string(path)
    .ok()
    .and_then(|text| serde_yaml::from_str::<QobuzCredentials>(&text).ok())
    .filter(|c| !c.user_auth_token.is_empty())
}

/// Write the credentials file with private permissions.
pub fn write_credentials(path: &Path, credentials: &QobuzCredentials) -> Result<()> {
  if let Some(dir) = path.parent() {
    crate::core::paths::ensure_private_dir(dir)?;
  }
  let yaml = serde_yaml::to_string(credentials).context("serializing Qobuz credentials")?;
  crate::core::auth::write_private_file_atomic(path, yaml.as_bytes())
    .with_context(|| format!("writing {}", path.display()))
}

/// The token to use: the env override first, then the credentials file.
pub fn token_with(env_token: Option<String>, saved: Option<QobuzCredentials>) -> Option<String> {
  env_token
    .filter(|t| !t.trim().is_empty())
    .or_else(|| saved.map(|c| c.user_auth_token))
}

/// The saved or overridden token, if any.
pub fn load_token() -> Option<String> {
  token_with(
    std::env::var(TOKEN_ENV).ok(),
    crate::core::paths::qobuz_credentials_path().and_then(|p| read_credentials(&p)),
  )
}

fn token_slot() -> &'static std::sync::Mutex<Option<String>> {
  static TOKEN: std::sync::OnceLock<std::sync::Mutex<Option<String>>> = std::sync::OnceLock::new();
  TOKEN.get_or_init(|| std::sync::Mutex::new(load_token()))
}

/// The token in use: loaded once from env or file, replaced by [`set_token`].
pub fn current_token() -> Option<String> {
  token_slot().lock().map(|t| t.clone()).unwrap_or(None)
}

/// Replace the in-memory token: `Some` after a login, `None` after a 401.
pub fn set_token(token: Option<String>) {
  if let Ok(mut slot) = token_slot().lock() {
    *slot = token;
  }
}

// ---------------------------------------------------------------------------
// Browser login
// ---------------------------------------------------------------------------

const OAUTH_URL: &str = "https://www.qobuz.com/signin/oauth";
/// How long the browser round trip may take before the login is abandoned.
pub const LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// The authorization URL for `app_id` with a `localhost:<port>` redirect.
pub fn login_url(app_id: &str, port: u16) -> String {
  format!("{OAUTH_URL}?ext_app_id={app_id}&redirect_url=http%3A%2F%2Flocalhost%3A{port}")
}

/// The authorization code from the callback URL (`code_autorisation`, else `code`).
pub fn callback_code(url: &str) -> Option<String> {
  let parsed = url::Url::parse(url).ok()?;
  let mut fallback = None;
  for (key, value) in parsed.query_pairs() {
    if value.is_empty() {
      continue;
    }
    match key.as_ref() {
      "code_autorisation" => return Some(value.into_owned()),
      "code" => fallback = Some(value.into_owned()),
      _ => {}
    }
  }
  fallback
}

fn accepts_qobuz_callback(path: &str) -> bool {
  path.contains("code_autorisation=") || path.contains("code=")
}

/// A bound login: open [`url`](Self::url) in the browser, then [`wait`](Self::wait).
pub struct LoginAttempt {
  constants: QobuzBundleCache,
  v4: tokio::net::TcpListener,
  /// A second listener for browsers that resolve `localhost` to `::1` first.
  v6: Option<tokio::net::TcpListener>,
  port: u16,
}

impl LoginAttempt {
  /// Bind the loopback callback listener(s) on a free port.
  pub async fn bind(constants: QobuzBundleCache) -> Result<Self> {
    let v4 = tokio::net::TcpListener::bind("127.0.0.1:0")
      .await
      .context("binding the Qobuz login callback listener")?;
    let port = v4.local_addr()?.port();
    let v6 = tokio::net::TcpListener::bind(("::1", port)).await.ok();
    Ok(LoginAttempt {
      constants,
      v4,
      v6,
      port,
    })
  }

  pub fn url(&self) -> String {
    login_url(&self.constants.app_id, self.port)
  }

  /// Wait for the browser callback and exchange its code for credentials.
  pub async fn wait(self) -> Result<QobuzCredentials> {
    use crate::infra::redirect_uri::redirect_uri_web_server_on;
    let LoginAttempt {
      constants, v4, v6, ..
    } = self;
    // Both accept loops live inside this future, so the timeout drops both
    // listeners with it.
    let callback = async move {
      match v6 {
        None => redirect_uri_web_server_on(v4, accepts_qobuz_callback).await,
        Some(v6) => tokio::select! {
          r = redirect_uri_web_server_on(v4, accepts_qobuz_callback) => r,
          r = redirect_uri_web_server_on(v6, accepts_qobuz_callback) => r,
        },
      }
    };
    let url = tokio::time::timeout(LOGIN_TIMEOUT, callback)
      .await
      .map_err(|_| anyhow!("login timed out after {}s", LOGIN_TIMEOUT.as_secs()))?
      .map_err(|()| anyhow!("login callback server failed"))?;
    let code =
      callback_code(&url).ok_or_else(|| anyhow!("no authorization code in the callback"))?;
    exchange_code(&constants, &code).await
  }
}

/// `GET oauth/callback`: trade the browser's code for the user token.
pub async fn exchange_code(constants: &QobuzBundleCache, code: &str) -> Result<QobuzCredentials> {
  let source = super::QobuzSource::new(&constants.app_id, &constants.app_secret, "");
  let reply: super::types::OauthCallback = source
    .get(
      "oauth/callback",
      &[
        ("code", code.to_string()),
        ("private_key", constants.oauth_key.clone()),
      ],
    )
    .await
    .context("oauth callback")?;
  let user_auth_token = reply
    .token
    .or(reply.user_auth_token)
    .filter(|t| !t.is_empty())
    .ok_or_else(|| anyhow!("oauth callback returned no token"))?;
  let user_id = reply.user_id.or(reply.user.and_then(|u| u.id));
  Ok(QobuzCredentials {
    user_auth_token,
    user_id,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  const SECRET: &str = "0123456789abcdef0123456789abcdef";

  /// A bundle snippet in the live shapes, with synthetic values.
  fn bundle() -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(SECRET);
    let (seed, info) = encoded.split_at(30);
    let extras = "A".repeat(44);
    format!(
      concat!(
        r#"x={{production:{{api:{{appId:"123456789",appSecret:"{legacy}"}}}}}};"#,
        r#"c.initialSeed("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",window.utimezone.london);"#,
        r#"c.initialSeed("{seed}",window.utimezone.berlin);"#,
        r#"[{{name:"Europe/London",info:"BBBB",extras:"CCCC"}},"#,
        r#"{{name:"Europe/Berlin",info:"{info}",extras:"{extras}"}}];"#,
        r#"r.authenticate({{privateKey:"oauth-private-key"}})"#
      ),
      legacy = "f".repeat(32),
      seed = seed,
      info = info,
      extras = extras,
    )
  }

  #[test]
  fn parse_bundle_extracts_all_three_constants() {
    let constants = parse_bundle(&bundle(), "8.2.0-b034").unwrap();
    assert_eq!(constants.bundle_version, "8.2.0-b034");
    assert_eq!(constants.app_id, "123456789");
    assert_eq!(constants.app_secret, SECRET);
    assert_eq!(constants.oauth_key, "oauth-private-key");
  }

  #[test]
  fn parse_bundle_reports_a_missing_seed() {
    let js = r#"production:{api:{appId:"123456789",appSecret:"x"}}"#;
    let err = parse_bundle(js, "v").unwrap_err().to_string();
    assert!(err.contains("seed"), "{err}");
  }

  #[test]
  fn secret_from_seed_rejects_a_non_hex_result() {
    let encoded =
      base64::engine::general_purpose::STANDARD.encode("not a hex secret, 32 chars!!!!!!");
    let (seed, info) = encoded.split_at(30);
    assert!(secret_from_seed(seed, info, &"A".repeat(44)).is_err());
  }

  #[test]
  fn bundle_path_and_version_parse_the_login_page() {
    let html = r#"<script src="/resources/8.2.0-b034/bundle.js"></script>"#;
    let path = bundle_path(html).unwrap();
    assert_eq!(path, "/resources/8.2.0-b034/bundle.js");
    assert_eq!(bundle_version(&path).as_deref(), Some("8.2.0-b034"));
    assert!(bundle_path("<html></html>").is_none());
  }

  #[test]
  fn env_overrides_need_all_three_values() {
    let all =
      constants_from_env_with(Some("1".into()), Some("s".into()), Some("k".into())).unwrap();
    assert_eq!(all.bundle_version, "env");
    assert_eq!(all.app_id, "1");
    assert!(constants_from_env_with(Some("1".into()), Some("s".into()), None).is_none());
    assert!(
      constants_from_env_with(Some("1".into()), Some(" ".into()), Some("k".into())).is_none()
    );
  }

  #[test]
  fn decode_base64_accepts_both_alphabets_and_padding() {
    assert_eq!(decode_base64("-_8=").unwrap(), vec![0xfb, 0xff]);
    assert_eq!(decode_base64("+/8").unwrap(), vec![0xfb, 0xff]);
  }

  #[test]
  fn token_prefers_env_over_the_file() {
    let saved = QobuzCredentials {
      user_auth_token: "file".into(),
      user_id: None,
    };
    assert_eq!(
      token_with(Some("env".into()), Some(saved.clone())).as_deref(),
      Some("env")
    );
    assert_eq!(
      token_with(Some(" ".into()), Some(saved)).as_deref(),
      Some("file")
    );
    assert!(token_with(None, None).is_none());
  }

  #[test]
  fn callback_code_prefers_code_autorisation_and_falls_back_to_code() {
    assert_eq!(
      callback_code("http://localhost:4321/?code_autorisation=ab%2Fcd&x=1").as_deref(),
      Some("ab/cd")
    );
    assert_eq!(
      callback_code("http://localhost:4321/?code=legacy").as_deref(),
      Some("legacy")
    );
    assert!(callback_code("http://localhost:4321/?state=only").is_none());
    assert!(callback_code("not a url").is_none());
  }

  #[test]
  fn login_url_targets_localhost_on_the_bound_port() {
    assert_eq!(
      login_url("123456789", 4321),
      "https://www.qobuz.com/signin/oauth?ext_app_id=123456789&redirect_url=http%3A%2F%2Flocalhost%3A4321"
    );
    assert!(accepts_qobuz_callback("/?code_autorisation=x"));
    assert!(accepts_qobuz_callback("/?code=x"));
    assert!(!accepts_qobuz_callback("/favicon.ico"));
  }

  #[test]
  fn credentials_round_trip_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qobuz_credentials.yml");
    assert!(read_credentials(&path).is_none());
    let creds = QobuzCredentials {
      user_auth_token: "tok".into(),
      user_id: Some("42".into()),
    };
    write_credentials(&path, &creds).unwrap();
    assert_eq!(read_credentials(&path), Some(creds));
  }
}
