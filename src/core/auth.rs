use crate::core::config::{ClientConfig, ConfigPaths, NCSPOT_CLIENT_ID};
use crate::infra::redirect_uri::redirect_uri_web_server_async;
use anyhow::{anyhow, Result};
use log::{info, warn};
use rspotify::{
  prelude::*,
  {AuthCodePkceSpotify, Config, Credentials, OAuth, Token, TokenCallback},
};
use std::{
  fs, io,
  path::{Path, PathBuf},
  sync::{Arc, OnceLock},
  time::{Duration, SystemTime},
};
use tokio::sync::Mutex;

pub const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(60);
static TOKEN_REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const SCOPES: [&str; 16] = [
  "playlist-read-collaborative",
  "playlist-read-private",
  "playlist-modify-private",
  "playlist-modify-public",
  "user-follow-read",
  "user-follow-modify",
  "user-library-modify",
  "user-library-read",
  "user-modify-playback-state",
  "user-read-currently-playing",
  "user-read-playback-state",
  "user-read-playback-position",
  "user-read-private",
  "user-read-recently-played",
  "user-top-read",
  "streaming",
];

pub struct AuthenticatedClient {
  pub spotify: AuthCodePkceSpotify,
  pub token_cache_path: PathBuf,
  #[cfg(feature = "streaming")]
  pub redirect_uri: String,
  /// The `/me` response captured while validating the cached token, so later
  /// startup steps (the native-streaming account probe) can reuse it instead
  /// of paying a second round trip. `None` when validation didn't run (fresh
  /// interactive login).
  #[cfg_attr(not(feature = "streaming"), allow(dead_code))]
  pub me: Option<rspotify::model::PrivateUser>,
}

// Manual token cache helpers since rspotify's built-in caching isn't working.
fn preserve_refresh_token_from_file(token: &mut Token, path: &Path) {
  if token.refresh_token.is_none() && path.exists() {
    if let Ok(old_json) = fs::read_to_string(path) {
      if let Ok(old_token) = serde_json::from_str::<Token>(&old_json) {
        token.refresh_token = old_token.refresh_token;
      }
    }
  }
}

/// Write `contents` to `path`, restricting permissions to owner-only where
/// the platform supports it (0o600 on Unix). Shared by any caller that
/// persists sensitive data to disk (the Spotify token cache, and — via
/// `user_config::save_config` — the app config file, which holds the
/// Subsonic password and party sync token in cleartext).
#[cfg(unix)]
pub(crate) fn write_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
  use std::io::Write;
  use std::os::unix::fs::OpenOptionsExt;
  std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .mode(0o600)
    .open(path)?
    .write_all(contents)
}

#[cfg(not(unix))]
pub(crate) fn write_private_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
  std::fs::write(path, contents)
}

fn persist_token_to_file(mut token: Token, path: &Path) -> Result<()> {
  preserve_refresh_token_from_file(&mut token, path);
  let token_json = serde_json::to_string_pretty(&token)?;
  write_private_file(path, token_json.as_bytes()).map_err(|e| anyhow!(e))?;
  info!("token cached to {}", path.display());
  Ok(())
}

pub async fn save_token_to_file(spotify: &AuthCodePkceSpotify, path: &Path) -> Result<()> {
  let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
  if let Some(ref mut token) = *token_lock {
    preserve_refresh_token_from_file(token, path);
    persist_token_to_file(token.clone(), path)?;
  }
  Ok(())
}

async fn restore_refresh_token_from_file(spotify: &AuthCodePkceSpotify, path: &Path) {
  let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
  if let Some(ref mut token) = *token_lock {
    preserve_refresh_token_from_file(token, path);
  }
}

pub async fn load_token_from_file(spotify: &AuthCodePkceSpotify, path: &PathBuf) -> Result<bool> {
  if !path.exists() {
    return Ok(false);
  }

  let token_json = fs::read_to_string(path)?;
  let token: Token = serde_json::from_str(&token_json)?;

  let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
  *token_lock = Some(token);
  drop(token_lock);

  info!("authentication token loaded from cache");
  Ok(true)
}

pub fn token_cache_path_for_client(base_path: &Path, client_id: &str) -> PathBuf {
  let suffix = &client_id[..8.min(client_id.len())];
  let stem = base_path
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("spotify_token_cache");
  let file_name = format!("{}_{}.json", stem, suffix);
  base_path.with_file_name(file_name)
}

fn redirect_uri_for_client(client_config: &ClientConfig, client_id: &str) -> String {
  if client_id == NCSPOT_CLIENT_ID {
    "http://127.0.0.1:8989/login".to_string()
  } else {
    client_config.get_redirect_uri()
  }
}

fn auth_port_from_redirect_uri(redirect_uri: &str) -> u16 {
  redirect_uri
    .split(':')
    .nth(2)
    .and_then(|v| v.split('/').next())
    .and_then(|v| v.parse::<u16>().ok())
    .unwrap_or(8888)
}

fn build_pkce_spotify_client(
  client_id: &str,
  redirect_uri: String,
  cache_path: PathBuf,
) -> AuthCodePkceSpotify {
  let creds = Credentials::new_pkce(client_id);
  let oauth = OAuth {
    redirect_uri,
    scopes: SCOPES.iter().map(|s| s.to_string()).collect(),
    ..Default::default()
  };
  let token_callback_path = cache_path.clone();
  let token_callback = TokenCallback(Box::new(move |token| {
    if let Err(e) = persist_token_to_file(token, &token_callback_path) {
      warn!(
        "failed to persist refreshed token to {}: {}",
        token_callback_path.display(),
        e
      );
    }
    Ok(())
  }));
  let config = Config {
    cache_path,
    token_refreshing: false,
    token_callback_fn: Arc::new(Some(token_callback)),
    ..Default::default()
  };
  AuthCodePkceSpotify::with_config(creds, oauth, config)
}

pub fn should_refresh_token_at(expiry: SystemTime, now: SystemTime) -> bool {
  now
    .checked_add(TOKEN_REFRESH_MARGIN)
    .map(|refresh_deadline| refresh_deadline >= expiry)
    .unwrap_or(true)
}

fn expiry_from_token(token: &Token) -> SystemTime {
  if let Some(expires_at) = token.expires_at {
    let unix_secs = expires_at.timestamp().max(0) as u64;
    SystemTime::UNIX_EPOCH + Duration::from_secs(unix_secs)
  } else {
    let expires_in_secs = token.expires_in.num_seconds().max(0) as u64;
    SystemTime::now()
      .checked_add(Duration::from_secs(expires_in_secs))
      .unwrap_or_else(SystemTime::now)
  }
}

async fn token_state(spotify: &AuthCodePkceSpotify) -> Result<(SystemTime, bool)> {
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  let token = token_lock
    .as_ref()
    .ok_or_else(|| anyhow!("Authentication failed: no valid token available"))?;

  Ok((expiry_from_token(token), token.refresh_token.is_some()))
}

pub async fn token_needs_refresh(spotify: &AuthCodePkceSpotify) -> Result<bool> {
  let (expiry, has_refresh_token) = token_state(spotify).await?;
  Ok(has_refresh_token && should_refresh_token_at(expiry, SystemTime::now()))
}

pub async fn refresh_token_and_cache(
  spotify: &AuthCodePkceSpotify,
  token_cache_path: &Path,
  force: bool,
) -> Result<SystemTime> {
  let refresh_lock = TOKEN_REFRESH_LOCK.get_or_init(|| Mutex::new(()));
  let _guard = refresh_lock.lock().await;

  restore_refresh_token_from_file(spotify, token_cache_path).await;
  let (current_expiry, has_refresh_token) = token_state(spotify).await?;
  if !force && !should_refresh_token_at(current_expiry, SystemTime::now()) {
    return Ok(current_expiry);
  }

  if !has_refresh_token {
    return Err(anyhow!(
      "Authentication refresh failed: no refresh token available"
    ));
  }

  spotify.refresh_token().await?;
  restore_refresh_token_from_file(spotify, token_cache_path).await;
  if spotify.config.token_callback_fn.as_ref().is_none() {
    save_token_to_file(spotify, token_cache_path).await?;
  }
  let expiry = token_expiry(spotify).await?;
  info!("refreshed token cached to {}", token_cache_path.display());
  Ok(expiry)
}

/// Returns the `/me` response when the cached token was validated with one,
/// so callers can reuse it instead of re-fetching (`None` after a fresh
/// interactive login, which skips the validation call).
async fn ensure_auth_token(
  spotify: &mut AuthCodePkceSpotify,
  token_cache_path: &PathBuf,
  auth_port: u16,
  interactive: bool,
) -> Result<Option<rspotify::model::PrivateUser>> {
  let mut needs_auth = match load_token_from_file(spotify, token_cache_path).await {
    Ok(true) => false,
    Ok(false) => {
      info!("no cached token found, authentication required");
      true
    }
    Err(e) => {
      info!("failed to read token cache: {}", e);
      true
    }
  };

  if !needs_auth && token_needs_refresh(spotify).await.unwrap_or(false) {
    match refresh_token_and_cache(spotify, token_cache_path, false).await {
      Ok(_) => {}
      Err(e) => {
        info!("cached authentication token refresh failed: {}", e);
        if token_cache_path.exists() {
          if let Err(remove_err) = fs::remove_file(token_cache_path) {
            info!(
              "failed to remove stale token cache {}: {}",
              token_cache_path.display(),
              remove_err
            );
          }
        }
        needs_auth = true;
      }
    }
  }

  let mut validated_me = None;
  if !needs_auth {
    match spotify.me().await {
      Ok(user) => validated_me = Some(user),
      Err(e) => {
        let err_text = e.to_string();
        let err_text_lower = err_text.to_lowercase();
        let should_reauth = err_text_lower.contains("401")
          || err_text_lower.contains("unauthorized")
          || err_text_lower.contains("status code 400")
          || err_text_lower.contains("invalid_grant")
          || err_text_lower.contains("access token expired")
          || err_text_lower.contains("token expired");

        if should_reauth {
          info!("cached authentication token is invalid, re-authentication required");
          if token_cache_path.exists() {
            if let Err(remove_err) = fs::remove_file(token_cache_path) {
              info!(
                "failed to remove stale token cache {}: {}",
                token_cache_path.display(),
                remove_err
              );
            }
          }
          needs_auth = true;
        } else {
          return Err(anyhow!(e));
        }
      }
    }
  }

  if needs_auth {
    // Silent load (free-source launch, or a returning user whose cached token is
    // present but stale): never open a browser or block on the callback server.
    // Interactive Spotify login is either the pre-TUI wizard path or the in-TUI
    // `BeginSpotifyLogin` flow.
    if !interactive {
      return Err(anyhow!(
        "Spotify authentication required but no valid cached token is available"
      ));
    }

    info!("starting spotify authentication flow on port {}", auth_port);
    let auth_url = spotify.get_authorize_url(None)?;

    println!("\nAttempting to open this URL in your browser:");
    println!("{}\n", auth_url);

    if let Err(e) = open::that(&auth_url) {
      println!("Failed to open browser automatically: {}", e);
      println!("Please manually open the URL above in your browser.");
    }

    println!(
      "Waiting for authorization callback on http://127.0.0.1:{}...\n",
      auth_port
    );

    // Async server, same as the in-TUI login path: the blocking variant used
    // to park a tokio worker thread in a std accept() loop with no timeout,
    // which could hang the whole login on startup (#364).
    match redirect_uri_web_server_async(auth_port).await {
      Ok(url) => {
        if let Some(code) = spotify.parse_response_code(&url) {
          info!("authorization code received, requesting access token");
          spotify.request_token(&code).await?;
          save_token_to_file(spotify, token_cache_path).await?;
          info!("successfully authenticated with spotify");
        } else {
          return Err(anyhow!(
            "Failed to parse authorization code from callback URL"
          ));
        }
      }
      Err(()) => {
        info!("redirect uri web server failed, using manual authentication");
        println!("Starting webserver failed. Continuing with manual authentication");
        println!("Please open this URL in your browser: {}", auth_url);
        println!("Enter the URL you were redirected to: ");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if let Some(code) = spotify.parse_response_code(&input) {
          info!("authorization code received from manual input, requesting access token");
          spotify.request_token(&code).await?;
          save_token_to_file(spotify, token_cache_path).await?;
          info!("successfully authenticated with spotify");
        } else {
          return Err(anyhow!("Failed to parse authorization code from input URL"));
        }
      }
    }
  }

  Ok(validated_me)
}

/// Authenticate the Spotify client, trying the primary client id then the
/// optional fallback. When `interactive` is false, candidates that lack a valid
/// cached token fail fast (no browser, no blocking callback server) so a
/// free-source launch or a returning user with a stale token degrades to "no
/// Spotify session" instead of forcing an OAuth flow.
pub async fn authenticate_with_fallback(
  client_config: &mut ClientConfig,
  config_paths: &ConfigPaths,
) -> Result<AuthenticatedClient> {
  authenticate_candidates(client_config, config_paths, true).await
}

/// Best-effort silent Spotify load for a free-source launch: returns `Some` only
/// when a cached token is present and usable, `None` otherwise (never prompts).
pub async fn try_load_spotify_silently(
  client_config: &mut ClientConfig,
  config_paths: &ConfigPaths,
) -> Option<AuthenticatedClient> {
  authenticate_candidates(client_config, config_paths, false)
    .await
    .ok()
}

async fn authenticate_candidates(
  client_config: &mut ClientConfig,
  config_paths: &ConfigPaths,
  interactive: bool,
) -> Result<AuthenticatedClient> {
  let mut client_candidates = vec![client_config.client_id.clone()];
  if let Some(fallback_id) = client_config.fallback_client_id.clone() {
    if fallback_id != client_config.client_id {
      client_candidates.push(fallback_id);
    }
  }

  let mut spotify = None;
  #[cfg(feature = "streaming")]
  let mut selected_redirect_uri = client_config.get_redirect_uri();
  let mut last_auth_error = None;
  let mut validated_me = None;

  for (index, client_id) in client_candidates.iter().enumerate() {
    let token_cache_path = token_cache_path_for_client(&config_paths.token_cache_path, client_id);
    let redirect_uri = redirect_uri_for_client(client_config, client_id);
    let auth_port = auth_port_from_redirect_uri(&redirect_uri);
    let mut candidate =
      build_pkce_spotify_client(client_id, redirect_uri.clone(), token_cache_path.clone());

    let auth_result =
      ensure_auth_token(&mut candidate, &token_cache_path, auth_port, interactive).await;

    match auth_result {
      Ok(me) => {
        validated_me = me;
        if *client_id == NCSPOT_CLIENT_ID {
          info!(
            "Using ncspot shared client ID. If it breaks in the future, configure fallback_client_id in client.yml."
          );
        } else {
          info!("Using fallback client ID {}", client_id);
        }
        client_config.client_id = client_id.clone();
        #[cfg(feature = "streaming")]
        {
          selected_redirect_uri = redirect_uri;
        }
        spotify = Some(candidate);
        break;
      }
      Err(e) => {
        last_auth_error = Some(e);
        if index + 1 < client_candidates.len() {
          info!(
            "Authentication with client {} failed, trying fallback client...",
            client_id
          );
          continue;
        }
      }
    }
  }

  let spotify =
    spotify.ok_or_else(|| last_auth_error.unwrap_or_else(|| anyhow!("Authentication failed")))?;
  let token_cache_path =
    token_cache_path_for_client(&config_paths.token_cache_path, &client_config.client_id);

  Ok(AuthenticatedClient {
    spotify,
    token_cache_path,
    #[cfg(feature = "streaming")]
    redirect_uri: selected_redirect_uri,
    me: validated_me,
  })
}

/// Prepare an in-TUI Spotify login: build a fresh PKCE client and return it with
/// the authorize URL to open, the callback port to listen on, and the token cache
/// path to persist to. PKCE stores the `code_verifier` inside this client
/// instance, so the caller MUST keep this exact client and later run
/// `request_token` on it — rebuilding the client would break the exchange with
/// `invalid_grant`.
pub fn prepare_interactive_login(
  client_config: &ClientConfig,
  config_paths: &ConfigPaths,
) -> Result<(AuthCodePkceSpotify, String, u16, PathBuf)> {
  let client_id = client_config.client_id.clone();
  let token_cache_path = token_cache_path_for_client(&config_paths.token_cache_path, &client_id);
  let redirect_uri = redirect_uri_for_client(client_config, &client_id);
  let auth_port = auth_port_from_redirect_uri(&redirect_uri);
  let mut spotify = build_pkce_spotify_client(&client_id, redirect_uri, token_cache_path.clone());
  let authorize_url = spotify.get_authorize_url(None)?;
  Ok((spotify, authorize_url, auth_port, token_cache_path))
}

pub async fn token_expiry(spotify: &AuthCodePkceSpotify) -> Result<SystemTime> {
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  let token_expiry = if let Some(ref token) = *token_lock {
    expiry_from_token(token)
  } else {
    return Err(anyhow!("Authentication failed: no valid token available"));
  };

  Ok(token_expiry)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{TimeDelta, Utc};

  fn create_test_token(refresh_token: Option<String>) -> Token {
    Token {
      access_token: "test_access_token".to_string(),
      refresh_token,
      expires_in: TimeDelta::seconds(3600),
      expires_at: Some(Utc::now() + TimeDelta::seconds(3600)),
      scopes: Default::default(),
    }
  }

  async fn create_test_spotify(token: Token) -> AuthCodePkceSpotify {
    let creds = Credentials::new("test_client_id", "test_client_secret");
    let oauth = OAuth {
      redirect_uri: "http://localhost:8888/callback".to_string(),
      scopes: Default::default(),
      ..Default::default()
    };
    let config = Config::default();
    let spotify = AuthCodePkceSpotify::with_config(creds, oauth, config);

    let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
    *token_lock = Some(token);
    drop(token_lock);

    spotify
  }

  fn create_temp_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
      "spotatui_test_token_{}.json",
      rand::random::<u32>()
    ));
    path
  }

  #[tokio::test]
  async fn test_save_token_preserves_refresh_token_when_missing() {
    let path = create_temp_path();

    let initial_token = create_test_token(Some("initial_refresh_token".to_string()));
    let spotify1 = create_test_spotify(initial_token).await;
    save_token_to_file(&spotify1, &path).await.unwrap();

    let refreshed_token = create_test_token(None);
    let spotify2 = create_test_spotify(refreshed_token).await;
    save_token_to_file(&spotify2, &path).await.unwrap();

    let saved_json = fs::read_to_string(&path).unwrap();
    let saved_token: Token = serde_json::from_str(&saved_json).unwrap();
    assert_eq!(
      saved_token.refresh_token,
      Some("initial_refresh_token".to_string())
    );
    assert_eq!(saved_token.access_token, "test_access_token");

    let _ = fs::remove_file(&path);
  }

  #[tokio::test]
  async fn test_save_token_uses_new_refresh_token_when_present() {
    let path = create_temp_path();

    let initial_token = create_test_token(Some("initial_refresh_token".to_string()));
    let spotify1 = create_test_spotify(initial_token).await;
    save_token_to_file(&spotify1, &path).await.unwrap();

    let new_token = create_test_token(Some("new_refresh_token".to_string()));
    let spotify2 = create_test_spotify(new_token).await;
    save_token_to_file(&spotify2, &path).await.unwrap();

    let saved_json = fs::read_to_string(&path).unwrap();
    let saved_token: Token = serde_json::from_str(&saved_json).unwrap();
    assert_eq!(
      saved_token.refresh_token,
      Some("new_refresh_token".to_string())
    );

    let _ = fs::remove_file(&path);
  }

  #[tokio::test]
  async fn test_save_token_works_without_existing_file() {
    let path = create_temp_path();

    let token = create_test_token(None);
    let spotify = create_test_spotify(token).await;
    save_token_to_file(&spotify, &path).await.unwrap();

    let saved_json = fs::read_to_string(&path).unwrap();
    let saved_token: Token = serde_json::from_str(&saved_json).unwrap();
    assert_eq!(saved_token.refresh_token, None);
    assert_eq!(saved_token.access_token, "test_access_token");

    let _ = fs::remove_file(&path);
  }

  #[tokio::test]
  async fn test_expired_token_detection_with_refresh_token() {
    let expired_token = Token {
      access_token: "expired_access_token".to_string(),
      refresh_token: Some("valid_refresh_token".to_string()),
      expires_in: TimeDelta::seconds(3600),
      expires_at: Some(Utc::now() - TimeDelta::seconds(3600)),
      scopes: Default::default(),
    };

    let spotify = create_test_spotify(expired_token).await;

    let should_refresh = {
      let token_lock = spotify.token.lock().await.expect("Failed to lock token");
      if let Some(ref token) = *token_lock {
        token
          .expires_at
          .map(|exp| exp < Utc::now())
          .unwrap_or(false)
          && token.refresh_token.is_some()
      } else {
        false
      }
    };

    assert!(
      should_refresh,
      "Expired token with refresh_token should be detected as needing refresh"
    );
  }

  #[tokio::test]
  async fn test_expired_token_without_refresh_token_not_refreshable() {
    let expired_token = Token {
      access_token: "expired_access_token".to_string(),
      refresh_token: None,
      expires_in: TimeDelta::seconds(3600),
      expires_at: Some(Utc::now() - TimeDelta::seconds(3600)),
      scopes: Default::default(),
    };

    let spotify = create_test_spotify(expired_token).await;

    let should_refresh = {
      let token_lock = spotify.token.lock().await.expect("Failed to lock token");
      if let Some(ref token) = *token_lock {
        token
          .expires_at
          .map(|exp| exp < Utc::now())
          .unwrap_or(false)
          && token.refresh_token.is_some()
      } else {
        false
      }
    };

    assert!(
      !should_refresh,
      "Expired token without refresh_token should NOT be refreshable"
    );
  }

  #[tokio::test]
  async fn test_valid_token_does_not_need_refresh() {
    let valid_token = Token {
      access_token: "valid_access_token".to_string(),
      refresh_token: Some("refresh_token".to_string()),
      expires_in: TimeDelta::seconds(3600),
      expires_at: Some(Utc::now() + TimeDelta::seconds(3600)),
      scopes: Default::default(),
    };

    let spotify = create_test_spotify(valid_token).await;

    let should_refresh = {
      let token_lock = spotify.token.lock().await.expect("Failed to lock token");
      if let Some(ref token) = *token_lock {
        token
          .expires_at
          .map(|exp| exp < Utc::now())
          .unwrap_or(false)
          && token.refresh_token.is_some()
      } else {
        false
      }
    };

    assert!(
      !should_refresh,
      "Valid non-expired token should not need refresh"
    );
  }

  #[test]
  fn test_token_refresh_deadline_uses_safety_margin() {
    let now = SystemTime::now();
    let outside_margin = now + TOKEN_REFRESH_MARGIN + Duration::from_secs(30);
    let inside_margin = now + TOKEN_REFRESH_MARGIN - Duration::from_secs(1);

    assert!(!should_refresh_token_at(outside_margin, now));
    assert!(should_refresh_token_at(inside_margin, now));
  }

  #[tokio::test]
  async fn test_token_needs_refresh_inside_margin() {
    let expiring_token = Token {
      access_token: "expiring_access_token".to_string(),
      refresh_token: Some("refresh_token".to_string()),
      expires_in: TimeDelta::seconds(30),
      expires_at: Some(Utc::now() + TimeDelta::seconds(30)),
      scopes: Default::default(),
    };

    let spotify = create_test_spotify(expiring_token).await;

    assert!(
      token_needs_refresh(&spotify).await.unwrap(),
      "Token inside the refresh margin should refresh before it expires"
    );
  }

  #[test]
  fn test_pkce_client_disables_rspotify_auto_refresh_with_cache_callback() {
    let spotify = build_pkce_spotify_client(
      "test_client_id",
      "http://localhost:8888/callback".to_string(),
      create_temp_path(),
    );

    assert!(
      !spotify.config.token_refreshing,
      "authenticated requests should use spotatui's shared refresh-and-retry path"
    );
    assert!(
      spotify.config.token_callback_fn.as_ref().is_some(),
      "rspotify-driven refreshes must still persist through spotatui's cache path"
    );
  }

  #[test]
  fn test_rspotify_token_callback_preserves_cached_refresh_token() {
    let path = create_temp_path();
    let old_token = create_test_token(Some("cached_refresh_token".to_string()));
    fs::write(&path, serde_json::to_string_pretty(&old_token).unwrap()).unwrap();

    let spotify = build_pkce_spotify_client(
      "test_client_id",
      "http://localhost:8888/callback".to_string(),
      path.clone(),
    );
    let callback = spotify.config.token_callback_fn.as_ref().as_ref().unwrap();
    let mut refreshed_token = create_test_token(None);
    refreshed_token.access_token = "callback_access_token".to_string();

    callback.0(refreshed_token).unwrap();

    let saved_json = fs::read_to_string(&path).unwrap();
    let saved_token: Token = serde_json::from_str(&saved_json).unwrap();
    assert_eq!(saved_token.access_token, "callback_access_token");
    assert_eq!(
      saved_token.refresh_token,
      Some("cached_refresh_token".to_string())
    );

    let _ = fs::remove_file(&path);
  }

  #[tokio::test]
  async fn test_token_without_expires_at_does_not_need_refresh() {
    let token = Token {
      access_token: "access_token".to_string(),
      refresh_token: Some("refresh_token".to_string()),
      expires_in: TimeDelta::seconds(3600),
      expires_at: None,
      scopes: Default::default(),
    };

    let spotify = create_test_spotify(token).await;

    let should_refresh = {
      let token_lock = spotify.token.lock().await.expect("Failed to lock token");
      if let Some(ref token) = *token_lock {
        token
          .expires_at
          .map(|exp| exp < Utc::now())
          .unwrap_or(false)
          && token.refresh_token.is_some()
      } else {
        false
      }
    };

    assert!(
      !should_refresh,
      "Token without expires_at should not trigger refresh"
    );
  }
}
