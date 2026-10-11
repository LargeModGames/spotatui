//! The Spotify login and its credential cache, shared by the browse session
//! and native playback.

use anyhow::{anyhow, Result};
use librespot_core::{authentication::Credentials, cache::Cache};
use librespot_oauth::{DeviceAuthClient, DeviceAuthClientBuilder, DeviceAuthorization};
use log::{info, warn};
use std::path::{Path, PathBuf};

/// OAuth scopes required for streaming (based on spotify-player)
pub(crate) const STREAMING_SCOPES: [&str; 6] = [
  "streaming",
  "user-read-playback-state",
  "user-modify-playback-state",
  "user-read-currently-playing",
  "user-library-read",
  "user-read-private",
];

/// Spotify's desktop client id, librespot's `KEYMASTER_CLIENT_ID`: Spotify
/// allows its device login, and its tokens reach the internal API.
pub(crate) const SPOTIFY_PLAYER_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

/// A login that waits for the user to approve it at [`Self::url`].
pub struct PendingLogin {
  client: DeviceAuthClient,
  authorization: DeviceAuthorization,
}

impl PendingLogin {
  /// The pairing page, with the code filled in when Spotify allows it.
  pub fn url(&self) -> &str {
    self.authorization.url()
  }

  pub fn code(&self) -> &str {
    self.authorization.user_code()
  }

  /// Waits until the user approves or declines, or the code expires.
  pub async fn finish(self) -> Result<Credentials> {
    let token = self
      .client
      .poll_for_token_async(&self.authorization)
      .await
      .map_err(|e| anyhow!("Spotify login failed: {e}"))?;
    Ok(Credentials::with_access_token(token.access_token))
  }
}

/// Asks Spotify for a pairing code. Nothing listens on a port.
pub async fn begin_login() -> Result<PendingLogin> {
  let client = DeviceAuthClientBuilder::new(SPOTIFY_PLAYER_CLIENT_ID, STREAMING_SCOPES.to_vec())
    .build()
    .map_err(|e| anyhow!("Spotify login is unavailable: {e}"))?;
  let authorization = client
    .request_device_code_async()
    .await
    .map_err(|e| anyhow!("Spotify login is unavailable: {e}"))?;
  Ok(PendingLogin {
    client,
    authorization,
  })
}
pub fn streaming_credentials_are_cached() -> Result<bool> {
  let cache_path = get_default_cache_path();
  if let Some(path) = cache_path.as_ref() {
    crate::core::paths::ensure_private_dir(path)?;
  }
  Ok(
    Cache::new(cache_path, None, None, None)?
      .credentials()
      .is_some(),
  )
}

pub(crate) fn clear_cached_streaming_credentials(cache_path: &Option<PathBuf>) {
  let Some(credentials_path) = cache_path
    .as_ref()
    .map(|path| path.join("credentials.json"))
  else {
    return;
  };

  match std::fs::remove_file(&credentials_path) {
    Ok(()) => {
      info!(
        "Cleared cached streaming credentials at {}",
        credentials_path.display()
      );
    }
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => {
      warn!(
        "Failed to clear cached streaming credentials at {}: {}",
        credentials_path.display(),
        e
      );
    }
  }
}

/// Drops any `user:password@` from a proxy URL before it is logged or handed
/// on. Two reasons, and the second is why this strips rather than masks:
///
/// - The log file is the app's own diagnostic artifact and gets pasted into bug
///   reports; a proxy password has no business in it. librespot logs the URL it
///   is given at `info` (`socket.rs`), so masking only our own message would
///   still leak it one layer down.
/// - librespot's `proxy_connect` sends a bare `CONNECT` with no
///   `Proxy-Authorization` header, so embedded credentials are never used for
///   authentication anyway. Carrying them further buys nothing and only widens
///   the exposure.
fn redact_proxy_url(url: &url::Url) -> url::Url {
  let mut redacted = url.clone();
  if redacted.set_username("").is_err() || redacted.set_password(None).is_err() {
    // Only fails for cannot-be-a-base URLs, which the scheme check rejects.
    return redacted;
  }
  redacted
}

/// A raw candidate is echoed in warnings only after the same treatment. It may
/// have failed to parse, so this works textually: everything between the scheme
/// separator and the last `@` of the authority is replaced.
pub(crate) fn redact_raw_proxy(raw: &str) -> String {
  let (scheme, rest) = match raw.split_once("://") {
    Some((scheme, rest)) => (scheme, rest),
    None => ("", raw),
  };
  let authority_end = rest.find('/').unwrap_or(rest.len());
  let Some(at) = rest[..authority_end].rfind('@') else {
    return raw.to_string();
  };
  let separator = if scheme.is_empty() { "" } else { "://" };
  format!("{scheme}{separator}<redacted>@{}", &rest[at + 1..])
}

/// Only `http` is accepted. librespot tunnels through the proxy with a plain
/// `CONNECT` and never wraps that conversation in TLS, so an `https://` proxy
/// URL cannot mean what it says: without an explicit port the `url` crate
/// resolves it to 443, and librespot then speaks plaintext HTTP at a port that
/// expects a TLS client hello. `https_proxy` conventionally names *which*
/// proxy tunnels HTTPS traffic, not that the proxy itself is reached over TLS,
/// so its value is an `http://` URL - accepting `https` here would only
/// convert a config typo into a silent handshake timeout.
fn proxy_scheme_is_supported(scheme: &str) -> bool {
  scheme == "http"
}

/// The HTTP proxy librespot should tunnel its access-point connection through,
/// picked from the first non-empty candidate.
///
/// librespot opens a raw TCP socket to the access point, so unlike every
/// reqwest-based call in the app it does not pick up `https_proxy` by itself.
/// On a network that only allows outbound traffic through a corporate proxy
/// that leaves the Spotify session hanging until the init timeout, the Connect
/// device never registers, and every Web API playback call answers
/// `NO_ACTIVE_DEVICE` - with nothing in the log naming the cause.
///
/// A malformed or unsupported value is reported and ignored rather than
/// failing startup, because a direct connection may still work.
pub(crate) fn proxy_url_from_candidates<'a>(
  candidates: impl IntoIterator<Item = Option<&'a str>>,
) -> Option<url::Url> {
  let raw = candidates
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty())?;
  match url::Url::parse(raw) {
    Ok(url) if proxy_scheme_is_supported(url.scheme()) => {
      if !url.username().is_empty() || url.password().is_some() {
        // Reported rather than rejected: the credentials are useless either
        // way, but a proxy that authenticates by source address - the common
        // corporate setup - still works once they are stripped. Rejecting the
        // whole URL would break those installs for no gain.
        warn!(
          "proxy credentials are ignored: librespot sends no Proxy-Authorization header. \
           A proxy that requires authentication will answer 407 and native streaming \
           will not start."
        );
      }
      Some(redact_proxy_url(&url))
    }
    Ok(url) if url.scheme() == "https" => {
      warn!(
        "ignoring proxy '{}': librespot tunnels with a plaintext CONNECT and cannot reach a \
         proxy over TLS. If the proxy speaks plain HTTP, write it as 'http://...'.",
        redact_raw_proxy(raw)
      );
      None
    }
    Ok(url) => {
      warn!(
        "ignoring proxy '{}': librespot only supports http proxies, not '{}'",
        redact_raw_proxy(raw),
        url.scheme()
      );
      None
    }
    Err(e) => {
      warn!(
        "ignoring unparseable proxy '{}': {e}",
        redact_raw_proxy(raw)
      );
      None
    }
  }
}

/// `SPOTATUI_STREAMING_PROXY` is checked first so the streaming session can be
/// pointed at a different proxy than the rest of the app; the remaining
/// candidates follow the usual lowercase-before-uppercase convention.
pub(crate) fn streaming_proxy_from_env() -> Option<url::Url> {
  let vars = [
    "SPOTATUI_STREAMING_PROXY",
    "https_proxy",
    "HTTPS_PROXY",
    "http_proxy",
    "HTTP_PROXY",
  ]
  .map(|name| std::env::var(name).ok());
  proxy_url_from_candidates(vars.iter().map(Option::as_deref))
}

/// Helper to get the default cache path for streaming
pub fn get_default_cache_path() -> Option<PathBuf> {
  let cache_path = crate::core::paths::app_cache_dir()?.join("streaming_cache");
  if let Some(config_dir) = crate::core::paths::app_config_dir() {
    if let Err(error) = migrate_legacy_streaming_cache_if_unclaimed(&config_dir, &cache_path) {
      log::warn!(
        "failed to migrate legacy streaming cache from {} to {}: {error}",
        config_dir.join("streaming_cache").display(),
        cache_path.display()
      );
    }
  }
  Some(cache_path)
}

pub(crate) fn migrate_legacy_streaming_cache_if_unclaimed(
  app_config_dir: &Path,
  cache_path: &Path,
) -> Result<bool> {
  crate::core::migrations::migrate_legacy_path_if_unclaimed(
    &app_config_dir.join("streaming_cache"),
    cache_path,
  )
}
