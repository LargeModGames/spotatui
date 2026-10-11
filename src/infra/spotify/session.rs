//! The Spotify session: a librespot session with no player, connected on first
//! use from the cached login.

use super::credentials::{
  clear_cached_streaming_credentials, get_default_cache_path, streaming_proxy_from_env,
};
use super::player::Observer;
use anyhow::{anyhow, Result};
use librespot_core::{
  authentication::Credentials, cache::Cache, config::SessionConfig, session::Session,
};
use log::{info, warn};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(30);
/// The account type arrives in a packet right after the login.
const PRODUCT_INFO_WAIT: Duration = Duration::from_secs(3);

pub struct SpotifySession {
  session: Session,
  observer: tokio::sync::OnceCell<Arc<Observer>>,
}

impl SpotifySession {
  pub fn session(&self) -> &Session {
    &self.session
  }

  /// The connect-state observer, registered on first use.
  pub async fn observer(&self) -> Result<Arc<Observer>> {
    self
      .observer
      .get_or_try_init(|| Observer::start(&self.session))
      .await
      .cloned()
  }

  pub fn username(&self) -> String {
    self.session.username()
  }

  pub fn country(&self) -> String {
    self.session.country()
  }
}

/// Why there is no session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
  NoLogin,
  NotPremium,
  /// The connect failed; it is tried again after 30 s.
  Offline,
}

impl Unavailable {
  pub fn message(self) -> &'static str {
    match self {
      Self::NoLogin => "Not logged in to Spotify",
      Self::NotPremium => "Spotify Premium is required",
      Self::Offline => "Could not connect to Spotify",
    }
  }
}

#[derive(Default)]
struct Slot {
  current: Option<Arc<SpotifySession>>,
  last_failure: Option<Instant>,
  not_premium: bool,
}

enum Connected {
  Session(SpotifySession),
  NoLogin,
  NotPremium,
}

fn slot() -> &'static Mutex<Slot> {
  static SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();
  SLOT.get_or_init(|| Mutex::new(Slot::default()))
}

/// The live session, connected from the cached login when there is none yet.
pub async fn current() -> Result<Arc<SpotifySession>, Unavailable> {
  if !allowed() {
    return Err(Unavailable::NoLogin);
  }
  let mut slot = slot().lock().await;
  if let Some(session) = &slot.current {
    if !session.session.is_invalid() {
      return Ok(session.clone());
    }
    info!("Spotify session ended; reconnecting");
    slot.current = None;
  }
  if slot.not_premium {
    return Err(Unavailable::NotPremium);
  }
  if !may_connect(slot.last_failure, Instant::now()) {
    return Err(Unavailable::Offline);
  }
  drop_refused_login(install(&mut slot, connect(None).await))
}

/// Connects with the credentials of a fresh login; librespot caches reusable
/// ones on success.
pub async fn log_in(credentials: Credentials) -> Result<(), Unavailable> {
  let mut slot = slot().lock().await;
  *slot = Slot::default();
  drop_refused_login(install(&mut slot, connect(Some(credentials)).await)).map(|_| ())
}

/// A non-Premium login is not kept, so the next launch offers the login again.
fn drop_refused_login(
  result: Result<Arc<SpotifySession>, Unavailable>,
) -> Result<Arc<SpotifySession>, Unavailable> {
  if matches!(result, Err(Unavailable::NotPremium)) {
    clear_cached_streaming_credentials(&get_default_cache_path());
  }
  result
}

/// Logs out: drops the session and the cached login.
pub async fn forget() {
  *slot().lock().await = Slot::default();
  clear_cached_streaming_credentials(&get_default_cache_path());
}

fn install(
  slot: &mut Slot,
  connected: Result<Connected>,
) -> Result<Arc<SpotifySession>, Unavailable> {
  match connected {
    Ok(Connected::Session(session)) => {
      let session = Arc::new(session);
      slot.current = Some(session.clone());
      slot.last_failure = None;
      Ok(session)
    }
    Ok(Connected::NoLogin) => Err(Unavailable::NoLogin),
    Ok(Connected::NotPremium) => {
      slot.not_premium = true;
      Err(Unavailable::NotPremium)
    }
    Err(e) => {
      warn!("Spotify session failed to connect: {e:#}");
      slot.last_failure = Some(Instant::now());
      Err(Unavailable::Offline)
    }
  }
}

/// Unit tests never reach Spotify; the ignored live tests opt in.
#[cfg(test)]
pub(super) static ALLOW_IN_TESTS: std::sync::atomic::AtomicBool =
  std::sync::atomic::AtomicBool::new(false);

fn allowed() -> bool {
  #[cfg(test)]
  return ALLOW_IN_TESTS.load(std::sync::atomic::Ordering::Relaxed);
  #[cfg(not(test))]
  true
}

fn may_connect(last_failure: Option<Instant>, now: Instant) -> bool {
  last_failure.is_none_or(|at| now.duration_since(at) >= RETRY_AFTER_FAILURE)
}

/// Connects with `credentials`, or with the cached login when `None`.
async fn connect(credentials: Option<Credentials>) -> Result<Connected> {
  let cache_path = get_default_cache_path();
  if let Some(path) = cache_path.as_ref() {
    crate::core::paths::ensure_private_dir(path)?;
  }
  let cache = Cache::new(cache_path, None, None, None)?;
  let Some(credentials) = credentials.or_else(|| cache.credentials()) else {
    return Ok(Connected::NoLogin);
  };
  let config = SessionConfig {
    proxy: streaming_proxy_from_env(),
    ..Default::default()
  };
  let session = Session::new(config, Some(cache));
  tokio::time::timeout(CONNECT_TIMEOUT, session.connect(credentials, true))
    .await
    .map_err(|_| anyhow!("timed out after {}s", CONNECT_TIMEOUT.as_secs()))??;

  let started = Instant::now();
  while session.get_user_attribute("type").is_none() && started.elapsed() < PRODUCT_INFO_WAIT {
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  let account_type = session.get_user_attribute("type");
  if !account_is_premium(account_type.as_deref()) {
    warn!(
      "Spotify session refused: the account is not Premium ({})",
      account_type.as_deref().unwrap_or("unknown")
    );
    session.shutdown();
    return Ok(Connected::NotPremium);
  }
  info!("Spotify session connected ({})", session.country());
  Ok(Connected::Session(SpotifySession {
    session,
    observer: tokio::sync::OnceCell::new(),
  }))
}

fn account_is_premium(account_type: Option<&str>) -> bool {
  account_type == Some("premium")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_recent_failure_blocks_a_new_connect() {
    let now = Instant::now();
    assert!(may_connect(None, now));
    assert!(!may_connect(Some(now), now + Duration::from_secs(5)));
    assert!(may_connect(Some(now), now + RETRY_AFTER_FAILURE));
  }

  #[test]
  fn only_a_premium_account_gets_a_session() {
    assert!(account_is_premium(Some("premium")));
    assert!(!account_is_premium(Some("free")));
    assert!(!account_is_premium(Some("open")));
    assert!(!account_is_premium(None));
  }

  #[test]
  fn a_failed_connect_starts_the_retry_gap() {
    let mut slot = Slot::default();
    assert_eq!(
      install(&mut slot, Err(anyhow!("network down"))).err(),
      Some(Unavailable::Offline)
    );
    assert!(slot.last_failure.is_some());
    assert!(slot.current.is_none());
  }

  #[test]
  fn a_non_premium_account_is_remembered() {
    let mut slot = Slot::default();
    assert_eq!(
      install(&mut slot, Ok(Connected::NotPremium)).err(),
      Some(Unavailable::NotPremium)
    );
    assert!(slot.not_premium);
    assert_eq!(
      install(&mut slot, Ok(Connected::NoLogin)).err(),
      Some(Unavailable::NoLogin)
    );
  }

  #[test]
  fn every_reason_has_a_message() {
    for reason in [
      Unavailable::NoLogin,
      Unavailable::NotPremium,
      Unavailable::Offline,
    ] {
      assert!(!reason.message().is_empty());
    }
  }
}
