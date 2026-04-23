//! Authentication helpers for startup orchestration.
//!
//! This module owns deterministic auth configuration, client-candidate selection,
//! stale-token classification, and thin token-cache file helpers. Interactive
//! browser/manual callback orchestration stays in `main.rs`.

use crate::core::config::{ClientConfig, NCSPOT_CLIENT_ID};
use anyhow::Result;
use log::info;
use rspotify::{AuthCodePkceSpotify, Config, Credentials, OAuth, Token};
use std::{
  fmt::Display,
  fs,
  path::{Path, PathBuf},
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientCandidateKind {
  Primary,
  Fallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientCandidate {
  pub kind: ClientCandidateKind,
  pub client_id: String,
  pub redirect_uri: String,
  pub auth_port: u16,
  pub token_cache_path: PathBuf,
}

pub async fn save_token_to_file(spotify: &AuthCodePkceSpotify, path: &Path) -> Result<()> {
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  if let Some(ref token) = *token_lock {
    let token_json = serde_json::to_string_pretty(token)?;
    fs::write(path, token_json)?;
    info!("token cached to {}", path.display());
  }
  Ok(())
}

pub async fn load_token_from_file(spotify: &AuthCodePkceSpotify, path: &Path) -> Result<bool> {
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

pub fn redirect_uri_for_client(client_config: &ClientConfig, client_id: &str) -> String {
  if client_id == NCSPOT_CLIENT_ID {
    "http://127.0.0.1:8989/login".to_string()
  } else {
    client_config.get_redirect_uri()
  }
}

pub fn auth_port_from_redirect_uri(redirect_uri: &str) -> u16 {
  redirect_uri
    .split(':')
    .nth(2)
    .and_then(|value| value.split('/').next())
    .and_then(|value| value.parse::<u16>().ok())
    .unwrap_or(8888)
}

pub fn build_pkce_spotify_client(candidate: &ClientCandidate) -> AuthCodePkceSpotify {
  let creds = Credentials::new_pkce(&candidate.client_id);
  let oauth = OAuth {
    redirect_uri: candidate.redirect_uri.clone(),
    scopes: SCOPES.iter().map(|scope| scope.to_string()).collect(),
    ..Default::default()
  };
  let config = Config {
    cache_path: candidate.token_cache_path.clone(),
    ..Default::default()
  };

  AuthCodePkceSpotify::with_config(creds, oauth, config)
}

pub fn is_stale_token_validation_error(error: impl Display) -> bool {
  let error_text = error.to_string();
  let error_text_lower = error_text.to_lowercase();

  error_text_lower.contains("401")
    || error_text_lower.contains("unauthorized")
    || error_text_lower.contains("status code 400")
    || error_text_lower.contains("invalid_grant")
    || error_text_lower.contains("access token expired")
    || error_text_lower.contains("token expired")
}

pub fn select_client_candidates(
  client_config: &ClientConfig,
  base_token_cache_path: &Path,
) -> Vec<ClientCandidate> {
  let mut candidates = vec![ClientCandidate {
    kind: ClientCandidateKind::Primary,
    client_id: client_config.client_id.clone(),
    redirect_uri: redirect_uri_for_client(client_config, &client_config.client_id),
    auth_port: auth_port_from_redirect_uri(&redirect_uri_for_client(
      client_config,
      &client_config.client_id,
    )),
    token_cache_path: token_cache_path_for_client(base_token_cache_path, &client_config.client_id),
  }];

  if let Some(fallback_client_id) = client_config.fallback_client_id.as_ref() {
    if fallback_client_id != &client_config.client_id {
      let redirect_uri = redirect_uri_for_client(client_config, fallback_client_id);
      candidates.push(ClientCandidate {
        kind: ClientCandidateKind::Fallback,
        client_id: fallback_client_id.clone(),
        auth_port: auth_port_from_redirect_uri(&redirect_uri),
        redirect_uri,
        token_cache_path: token_cache_path_for_client(base_token_cache_path, fallback_client_id),
      });
    }
  }

  candidates
}

#[cfg(test)]
mod tests {
  use super::{
    auth_port_from_redirect_uri, is_stale_token_validation_error, redirect_uri_for_client,
    select_client_candidates, token_cache_path_for_client, ClientCandidateKind,
  };
  use crate::core::config::{ClientConfig, NCSPOT_CLIENT_ID};
  use std::path::Path;

  #[test]
  fn token_cache_path_uses_stem_and_client_suffix() {
    let path = token_cache_path_for_client(
      Path::new("/tmp/.spotify_token_cache.json"),
      "1234567890abcd",
    );

    assert_eq!(path, Path::new("/tmp/.spotify_token_cache_12345678.json"));
  }

  #[test]
  fn redirect_uri_uses_ncspot_login_for_shared_client() {
    let mut client_config = ClientConfig::new();
    client_config.port = Some(7777);

    assert_eq!(
      redirect_uri_for_client(&client_config, NCSPOT_CLIENT_ID),
      "http://127.0.0.1:8989/login"
    );
  }

  #[test]
  fn redirect_uri_uses_configured_callback_for_custom_client() {
    let mut client_config = ClientConfig::new();
    client_config.port = Some(7777);

    assert_eq!(
      redirect_uri_for_client(&client_config, "custom-client"),
      "http://127.0.0.1:7777/callback"
    );
  }

  #[test]
  fn auth_port_comes_from_redirect_uri() {
    assert_eq!(
      auth_port_from_redirect_uri("http://127.0.0.1:8989/login"),
      8989
    );
    assert_eq!(
      auth_port_from_redirect_uri("http://127.0.0.1:7777/callback"),
      7777
    );
  }

  #[test]
  fn stale_token_detection_matches_current_validation_errors() {
    assert!(is_stale_token_validation_error("401 unauthorized"));
    assert!(is_stale_token_validation_error(
      "status code 400 invalid_grant"
    ));
    assert!(is_stale_token_validation_error("access token expired"));
    assert!(!is_stale_token_validation_error("connection reset by peer"));
  }

  #[test]
  fn candidate_selection_prefers_primary_then_distinct_fallback() {
    let mut client_config = ClientConfig::new();
    client_config.client_id = NCSPOT_CLIENT_ID.to_string();
    client_config.fallback_client_id = Some("fallback-client".to_string());
    client_config.port = Some(8888);

    let candidates =
      select_client_candidates(&client_config, Path::new("/tmp/.spotify_token_cache.json"));

    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].kind, ClientCandidateKind::Primary);
    assert_eq!(candidates[0].client_id, NCSPOT_CLIENT_ID);
    assert_eq!(candidates[0].auth_port, 8989);
    assert_eq!(candidates[1].kind, ClientCandidateKind::Fallback);
    assert_eq!(candidates[1].client_id, "fallback-client");
    assert_eq!(candidates[1].auth_port, 8888);
  }

  #[test]
  fn candidate_selection_dedupes_matching_fallback() {
    let mut client_config = ClientConfig::new();
    client_config.client_id = "same-client".to_string();
    client_config.fallback_client_id = Some("same-client".to_string());

    let candidates =
      select_client_candidates(&client_config, Path::new("/tmp/.spotify_token_cache.json"));

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].kind, ClientCandidateKind::Primary);
  }
}
