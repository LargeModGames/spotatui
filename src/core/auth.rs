use crate::core::config::{ClientConfig, ConfigPaths, NCSPOT_CLIENT_ID};
use crate::infra::redirect_uri::redirect_uri_web_server;
use anyhow::{anyhow, Result};
use log::info;
use rspotify::{
  prelude::*,
  {AuthCodePkceSpotify, Config, Credentials, OAuth, Token},
};
use std::{
  fs, io,
  path::{Path, PathBuf},
  time::SystemTime,
};

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
}

// Manual token cache helpers since rspotify's built-in caching isn't working.
pub async fn save_token_to_file(spotify: &AuthCodePkceSpotify, path: &PathBuf) -> Result<()> {
  let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
  if let Some(ref mut token) = *token_lock {
    if token.refresh_token.is_none() && path.exists() {
      if let Ok(old_json) = fs::read_to_string(path) {
        if let Ok(old_token) = serde_json::from_str::<Token>(&old_json) {
          token.refresh_token = old_token.refresh_token;
        }
      }
    }
    let token_json = serde_json::to_string_pretty(token)?;
    fs::write(path, token_json)?;
    info!("token cached to {}", path.display());
  }
  Ok(())
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
  let config = Config {
    cache_path,
    ..Default::default()
  };
  AuthCodePkceSpotify::with_config(creds, oauth, config)
}

async fn ensure_auth_token(
  spotify: &mut AuthCodePkceSpotify,
  token_cache_path: &PathBuf,
  auth_port: u16,
) -> Result<()> {
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

  if !needs_auth {
    if let Err(e) = spotify.me().await {
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

  if needs_auth {
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

    match redirect_uri_web_server(auth_port) {
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

  Ok(())
}

pub async fn authenticate_with_fallback(
  client_config: &mut ClientConfig,
  config_paths: &ConfigPaths,
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

  for (index, client_id) in client_candidates.iter().enumerate() {
    let token_cache_path = token_cache_path_for_client(&config_paths.token_cache_path, client_id);
    let redirect_uri = redirect_uri_for_client(client_config, client_id);
    let auth_port = auth_port_from_redirect_uri(&redirect_uri);
    let mut candidate =
      build_pkce_spotify_client(client_id, redirect_uri.clone(), token_cache_path.clone());

    let auth_result = ensure_auth_token(&mut candidate, &token_cache_path, auth_port).await;

    match auth_result {
      Ok(()) => {
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
  })
}

pub async fn token_expiry(spotify: &AuthCodePkceSpotify) -> Result<SystemTime> {
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  let token_expiry = if let Some(ref token) = *token_lock {
    if let Some(expires_at) = token.expires_at {
      let unix_secs = expires_at.timestamp().max(0) as u64;
      SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(unix_secs)
    } else {
      let expires_in_secs = token.expires_in.num_seconds().max(0) as u64;
      SystemTime::now()
        .checked_add(std::time::Duration::from_secs(expires_in_secs))
        .unwrap_or_else(SystemTime::now)
    }
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
