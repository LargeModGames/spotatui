use super::Network;
use crate::core::{app::App, auth};
use anyhow::anyhow;
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

static SPOTIFY_API_PACING: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
const SPOTIFY_API_MIN_INTERVAL: Duration = Duration::from_millis(250);
const SPOTIFY_API_BASE_URL: &str = "https://api.spotify.com/v1/";

pub async fn pace_spotify_api_call() {
  let pacing_lock = SPOTIFY_API_PACING.get_or_init(|| Mutex::new(None));
  let mut last_request_started_at = pacing_lock.lock().await;

  if let Some(last) = *last_request_started_at {
    let elapsed = last.elapsed();
    if elapsed < SPOTIFY_API_MIN_INTERVAL {
      tokio::time::sleep(SPOTIFY_API_MIN_INTERVAL - elapsed).await;
    }
  }

  *last_request_started_at = Some(Instant::now());
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
    SPOTIFY_API_BASE_URL,
    method,
    path,
    query,
    body,
    |force| async move {
      match auth::refresh_token_and_cache(spotify, token_cache_path, force).await {
        Ok(expiry) => {
          let mut app = app.lock().await;
          app.spotify_token_expiry = expiry;
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
  )
  .await
}

async fn spotify_api_request_json_for_base_with_refresh<F, Fut>(
  spotify: &AuthCodePkceSpotify,
  base_url: &str,
  method: Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<Value>,
  mut refresh_token: F,
) -> anyhow::Result<Value>
where
  F: FnMut(bool) -> Fut,
  Fut: Future<Output = anyhow::Result<Option<std::time::SystemTime>>>,
{
  refresh_token(false).await?;

  let mut url = reqwest::Url::parse(base_url)?.join(path)?;
  if !query.is_empty() {
    let mut qp = url.query_pairs_mut();
    for (k, v) in query {
      qp.append_pair(k, v);
    }
  }

  let client = reqwest::Client::new();
  let mut attempt: u8 = 0;
  let max_attempts: u8 = 4;
  let mut refreshed_after_unauthorized = false;

  loop {
    let access_token = {
      let token_lock = spotify.token.lock().await.expect("Failed to lock token");
      token_lock
        .as_ref()
        .map(|t| t.access_token.clone())
        .ok_or_else(|| anyhow!("No access token available"))?
    };

    pace_spotify_api_call().await;

    let mut request = client
      .request(method.clone(), url.clone())
      .header("Authorization", format!("Bearer {}", access_token))
      .header("Content-Type", "application/json");

    if let Some(payload) = body.clone() {
      request = request.json(&payload);
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
      let response_body = response.text().await?;
      if response_body.trim().is_empty() {
        return Ok(Value::Null);
      }
      return Ok(serde_json::from_str(&response_body)?);
    }

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED && !refreshed_after_unauthorized {
      match refresh_token(true).await {
        Ok(Some(_)) => {
          refreshed_after_unauthorized = true;
          continue;
        }
        Ok(None) => {
          let body = response.text().await.unwrap_or_default();
          return Err(anyhow!(
            "Spotify API {} failed: {} (token refresh unavailable for this request)",
            status,
            body
          ));
        }
        Err(refresh_err) => {
          let body = response.text().await.unwrap_or_default();
          return Err(anyhow!(
            "Spotify API {} failed: {} (token refresh failed: {})",
            status,
            body,
            refresh_err
          ));
        }
      }
    }

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt + 1 < max_attempts {
      let retry_after_secs = response
        .headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1);

      let backoff_secs = retry_after_secs.max(1) + u64::from(attempt);
      tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
      attempt += 1;
      continue;
    }

    let body = response.text().await.unwrap_or_default();
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
      &self.spotify,
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

    let result = spotify_api_request_json_for_base_with_refresh(
      &spotify,
      &base_url,
      Method::GET,
      "me",
      &[],
      None,
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
    )
    .await
    .unwrap();

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
  }
}
