use crate::core::app::{FriendEntry, FriendNowPlaying, FriendSearchResult};
use crate::infra::network::Network;
use anyhow::Result;
use log::{info, warn};
use serde::Deserialize;

const FRIENDS_URL: &str = "https://spotatui.com/api/friends";
const PROFILE_URL: &str = "https://spotatui.com/api/profile";
const USERS_SEARCH_URL: &str = "https://spotatui.com/api/users/search";

// ── Response shapes from the spotatui.com API ─────────────────────────────────

#[derive(Debug, Deserialize)]
struct NowPlayingData {
  title: String,
  artists: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicUserData {
  id: String,
  name: String,
  is_online: bool,
  now_playing: Option<NowPlayingData>,
  listening_ms: Option<u64>,
  total_listens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FriendsResponse {
  friends: Vec<PublicUserData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileResponse {
  friend_code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchUserData {
  id: String,
  name: String,
  #[serde(default)]
  is_following: bool,
}

#[derive(Debug, Deserialize)]
struct UsersSearchResponse {
  users: Vec<SearchUserData>,
}

// ── Read sync token from App state ────────────────────────────────────────────

/// Read the sync token by briefly locking the app.
/// Call this before any `.await` branches that need the token.
async fn read_sync_token(network: &Network) -> Option<String> {
  let app = network.app.lock().await;
  app.user_config.behavior.sync_token.clone()
}

// ── Actual HTTP functions ─────────────────────────────────────────────────────

async fn fetch_profile(token: &str) -> Result<String> {
  let client = crate::infra::network::requests::shared_http_client();
  let resp = client
    .get(PROFILE_URL)
    .header("Authorization", format!("Bearer {}", token))
    .send()
    .await?;

  if !resp.status().is_success() {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    return Err(anyhow::anyhow!(
      "profile fetch failed ({}): {}",
      status,
      body
    ));
  }

  let data: ProfileResponse = resp.json().await?;
  Ok(data.friend_code)
}

async fn fetch_friends(token: &str) -> Result<Vec<FriendEntry>> {
  let client = crate::infra::network::requests::shared_http_client();
  let resp = client
    .get(FRIENDS_URL)
    .header("Authorization", format!("Bearer {}", token))
    .send()
    .await?;

  if !resp.status().is_success() {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    return Err(anyhow::anyhow!(
      "friends fetch failed ({}): {}",
      status,
      body
    ));
  }

  let data: FriendsResponse = resp.json().await?;
  let entries = data
    .friends
    .into_iter()
    .map(|u| FriendEntry {
      id: u.id,
      name_lower: u.name.to_lowercase(),
      name: u.name,
      is_online: u.is_online,
      now_playing: u.now_playing.map(|np| FriendNowPlaying {
        title: np.title,
        artists: np.artists,
      }),
      listening_ms: u.listening_ms.unwrap_or(0),
      total_listens: u.total_listens.unwrap_or(0),
    })
    .collect();
  Ok(entries)
}

async fn post_add_friend_by_code(token: &str, friend_code: &str) -> Result<()> {
  let body = serde_json::json!({ "friendCode": friend_code });
  post_friend_request(token, body).await
}

async fn post_add_friend_by_user_id(token: &str, user_id: &str) -> Result<()> {
  let body = serde_json::json!({ "userId": user_id });
  post_friend_request(token, body).await
}

async fn post_friend_request(token: &str, body: serde_json::Value) -> Result<()> {
  let client = crate::infra::network::requests::shared_http_client();
  let resp = client
    .post(FRIENDS_URL)
    .header("Authorization", format!("Bearer {}", token))
    .json(&body)
    .send()
    .await?;

  if !resp.status().is_success() {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    // Try to parse a JSON error message
    let msg = serde_json::from_str::<serde_json::Value>(&text)
      .ok()
      .and_then(|v| v["error"].as_str().map(String::from))
      .unwrap_or(text);
    return Err(anyhow::anyhow!("{} (HTTP {})", msg, status));
  }

  Ok(())
}

async fn delete_friend(token: &str, user_id: &str) -> Result<()> {
  let client = crate::infra::network::requests::shared_http_client();
  let body = serde_json::json!({ "userId": user_id });
  let resp = client
    .delete(FRIENDS_URL)
    .header("Authorization", format!("Bearer {}", token))
    .json(&body)
    .send()
    .await?;

  if !resp.status().is_success() {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let msg = serde_json::from_str::<serde_json::Value>(&text)
      .ok()
      .and_then(|v| v["error"].as_str().map(String::from))
      .unwrap_or(text);
    return Err(anyhow::anyhow!("{} (HTTP {})", msg, status));
  }

  Ok(())
}

async fn fetch_user_search(token: &str, query: &str) -> Result<Vec<FriendSearchResult>> {
  if query.trim().is_empty() {
    return Ok(vec![]);
  }

  let client = crate::infra::network::requests::shared_http_client();
  let resp = client
    .get(USERS_SEARCH_URL)
    .header("Authorization", format!("Bearer {}", token))
    .query(&[("q", query)])
    .send()
    .await?;

  if !resp.status().is_success() {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    return Err(anyhow::anyhow!("user search failed ({}): {}", status, body));
  }

  let data: UsersSearchResponse = resp.json().await?;
  let results = data
    .users
    .into_iter()
    .map(|u| FriendSearchResult {
      id: u.id,
      name: u.name,
      is_following: u.is_following,
    })
    .collect();
  Ok(results)
}

// ── Convenience wrappers that extract the sync token before the async work ────
//
// These free functions are called from the IoEvent match in `Network::handle_network_event`.
// Each one grabs the token from App state, then delegates to the proper HTTP helper.

pub async fn handle_get_friend_code(network: &mut Network) {
  let token = match read_sync_token(network).await {
    Some(t) => t,
    None => return,
  };
  match fetch_profile(&token).await {
    Ok(code) => {
      let mut app = network.app.lock().await;
      app.friend_code = Some(code);
    }
    Err(e) => warn!("friends: failed to fetch friend code: {}", e),
  }
}

pub async fn handle_get_friends(network: &mut Network) {
  let token = match read_sync_token(network).await {
    Some(t) => t,
    None => {
      let mut app = network.app.lock().await;
      app.friends_loading = false;
      return;
    }
  };
  {
    let mut app = network.app.lock().await;
    app.friends_loading = true;
  }
  match fetch_friends(&token).await {
    Ok(friends) => {
      let mut app = network.app.lock().await;
      let len = friends.len();
      app.friends = friends;
      app.friends_loading = false;
      if app.friend_selected_index >= len && len > 0 {
        app.friend_selected_index = len - 1;
      }
      info!("friends: loaded {} friends", len);
    }
    Err(e) => {
      let mut app = network.app.lock().await;
      app.friends_loading = false;
      warn!("friends: failed to load friends: {}", e);
    }
  }
}

pub async fn handle_add_friend_by_code(network: &mut Network, friend_code: String) {
  let token = match read_sync_token(network).await {
    Some(t) => t,
    None => return,
  };
  match post_add_friend_by_code(&token, &friend_code).await {
    Ok(_) => {
      handle_get_friends(network).await;
      network
        .show_status_message(format!("Added friend (code: {})", friend_code), 4)
        .await;
    }
    Err(e) => {
      network
        .show_status_message(format!("Could not add friend: {}", e), 5)
        .await;
    }
  }
}

pub async fn handle_add_friend_by_user_id(network: &mut Network, user_id: String) {
  let token = match read_sync_token(network).await {
    Some(t) => t,
    None => return,
  };
  match post_add_friend_by_user_id(&token, &user_id).await {
    Ok(_) => {
      handle_get_friends(network).await;
      network
        .show_status_message("Added friend".to_string(), 4)
        .await;
    }
    Err(e) => {
      network
        .show_status_message(format!("Could not add friend: {}", e), 5)
        .await;
    }
  }
}

pub async fn handle_unfollow_friend(network: &mut Network, user_id: String) {
  let token = match read_sync_token(network).await {
    Some(t) => t,
    None => return,
  };
  match delete_friend(&token, &user_id).await {
    Ok(_) => {
      handle_get_friends(network).await;
      network
        .show_status_message("Unfollowed".to_string(), 3)
        .await;
    }
    Err(e) => {
      network
        .show_status_message(format!("Failed to unfollow: {}", e), 5)
        .await;
    }
  }
}

pub async fn handle_search_friend_users(network: &mut Network, query: String) {
  let token = match read_sync_token(network).await {
    Some(t) => t,
    None => return,
  };
  match fetch_user_search(&token, &query).await {
    Ok(results) => {
      let mut app = network.app.lock().await;
      let current_query: String = app.friend_user_search_input.iter().collect();
      if current_query == query {
        app.friend_user_search_results = results;
        app.friend_user_search_selected = 0;
      }
    }
    Err(e) => {
      let mut app = network.app.lock().await;
      let current_query: String = app.friend_user_search_input.iter().collect();
      if current_query == query {
        app.friend_user_search_results.clear();
        app.friend_user_search_selected = 0;
      }
      warn!("friends: user search failed: {}", e);
    }
  }
}
