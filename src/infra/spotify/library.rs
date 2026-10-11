//! The user's library from collection v2: liked songs, saved albums and shows,
//! followed artists.

use super::catalog::full_artists;
use super::json;
use super::metadata::get_many;
use super::playlists;
use super::router::{api_error, query_value, retry_read, upstream_error, Paging};
use super::session::SpotifySession;
use chrono::{DateTime, Utc};
use librespot_metadata::{Album, Show, Track};
use librespot_protocol::collection2v2::CollectionItem;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const CACHE_TTL: Duration = Duration::from_secs(30);
const PAGE_SIZE: i32 = 300;
/// A runaway pagination token must not loop forever.
const MAX_PAGES: usize = 200;

/// The collection set that holds a uri, or `None` for a playlist or unknown kind.
fn set_for(uri: &str) -> Option<&'static str> {
  match uri.split(':').nth(1)? {
    "track" | "album" => Some("collection"),
    "artist" => Some("artist"),
    "show" => Some("show"),
    "episode" => Some("listenlater"),
    _ => None,
  }
}

#[derive(Clone)]
struct Entry {
  uri: String,
  added_at: i64,
}

struct CachedSet {
  entries: Vec<Entry>,
  fetched: Instant,
}

fn cache() -> &'static Mutex<HashMap<&'static str, CachedSet>> {
  static CACHE: OnceLock<Mutex<HashMap<&'static str, CachedSet>>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The set's items, newest first.
async fn entries(session: &SpotifySession, set: &'static str) -> anyhow::Result<Vec<Entry>> {
  let mut cache = cache().lock().await;
  if let Some(cached) = cache.get(set) {
    if cached.fetched.elapsed() < CACHE_TTL {
      return Ok(cached.entries.clone());
    }
  }
  let mut entries = Vec::new();
  let mut token = String::new();
  for _ in 0..MAX_PAGES {
    let spclient = session.session().spclient();
    let page = retry_read(|| spclient.get_collection_page(set, &token, PAGE_SIZE))
      .await
      .map_err(upstream_error)?;
    entries.extend(
      page
        .items
        .iter()
        .filter(|item| !item.is_removed)
        .map(|item| Entry {
          uri: item.uri.clone(),
          added_at: i64::from(item.added_at),
        }),
    );
    if page.next_page_token.is_empty() {
      break;
    }
    token = page.next_page_token;
  }
  entries.sort_by_key(|entry| std::cmp::Reverse(entry.added_at));
  cache.insert(
    set,
    CachedSet {
      entries: entries.clone(),
      fetched: Instant::now(),
    },
  );
  Ok(entries)
}

async fn apply_to_cache(set: &'static str, uris: &[String], removed: bool, now: i64) {
  let mut cache = cache().lock().await;
  let Some(cached) = cache.get_mut(set) else {
    return;
  };
  cached.entries.retain(|entry| !uris.contains(&entry.uri));
  if !removed {
    for uri in uris.iter().rev() {
      cached.entries.insert(
        0,
        Entry {
          uri: uri.clone(),
          added_at: now,
        },
      );
    }
  }
}

fn with_prefix(entries: Vec<Entry>, prefix: &str) -> Vec<Entry> {
  entries
    .into_iter()
    .filter(|entry| entry.uri.starts_with(prefix))
    .collect()
}

fn added_at(seconds: i64) -> String {
  DateTime::<Utc>::from_timestamp(seconds, 0)
    .unwrap_or_default()
    .to_rfc3339()
}

pub async fn saved_tracks(session: &SpotifySession, paging: Paging) -> anyhow::Result<Value> {
  let tracks = with_prefix(entries(session, "collection").await?, "spotify:track:");
  let window = paging.window(&tracks);
  let uris: Vec<String> = window.iter().map(|entry| entry.uri.clone()).collect();
  let items = window
    .iter()
    .zip(get_many::<Track>(session, &uris).await?)
    .filter_map(|(entry, track)| {
      Some(json!({ "added_at": added_at(entry.added_at), "track": json::track_full(track.as_deref()?) }))
    })
    .collect();
  Ok(json::page(items, paging.offset, paging.limit, tracks.len()))
}

pub async fn saved_albums(session: &SpotifySession, paging: Paging) -> anyhow::Result<Value> {
  let albums = with_prefix(entries(session, "collection").await?, "spotify:album:");
  let window = paging.window(&albums);
  let uris: Vec<String> = window.iter().map(|entry| entry.uri.clone()).collect();
  let metadata = get_many::<Album>(session, &uris).await?;
  let track_uris: Vec<String> = metadata
    .iter()
    .flatten()
    .flat_map(|album| album.tracks().map(|track| track.to_uri()))
    .collect();
  let tracks: HashMap<String, Value> = track_uris
    .iter()
    .cloned()
    .zip(get_many::<Track>(session, &track_uris).await?)
    .filter_map(|(uri, track)| Some((uri, json::track_simplified(track.as_deref()?))))
    .collect();
  let items = window
    .iter()
    .zip(metadata)
    .filter_map(|(entry, album)| {
      let album = album?;
      let album_tracks: Vec<Value> = album
        .tracks()
        .filter_map(|track| tracks.get(&track.to_uri()).cloned())
        .collect();
      let total = album_tracks.len();
      let page = json::page(album_tracks, 0, total.max(1), total);
      Some(json!({ "added_at": added_at(entry.added_at), "album": json::album_full(&album, page) }))
    })
    .collect();
  Ok(json::page(items, paging.offset, paging.limit, albums.len()))
}

pub async fn saved_shows(session: &SpotifySession, paging: Paging) -> anyhow::Result<Value> {
  let shows = entries(session, "show").await?;
  let window = paging.window(&shows);
  let uris: Vec<String> = window.iter().map(|entry| entry.uri.clone()).collect();
  let items = window
    .iter()
    .zip(get_many::<Show>(session, &uris).await?)
    .filter_map(|(entry, show)| {
      Some(json!({ "added_at": added_at(entry.added_at), "show": json::show_simplified(show.as_deref()?) }))
    })
    .collect();
  Ok(json::page(items, paging.offset, paging.limit, shows.len()))
}

/// Cursor paging: `after` names the last artist id of the previous page.
pub async fn followed_artists(
  session: &SpotifySession,
  query: &[(&str, String)],
) -> anyhow::Result<Value> {
  let artists = entries(session, "artist").await?;
  let limit = Paging::from_query(query, 20).limit;
  let start = query_value(query, "after")
    .and_then(|after| {
      artists
        .iter()
        .position(|entry| entry.uri == format!("spotify:artist:{after}"))
    })
    .map_or(0, |index| index + 1);
  let window: Vec<String> = artists
    .iter()
    .skip(start)
    .take(limit)
    .map(|entry| entry.uri.clone())
    .collect();
  let items = full_artists(session, &window).await?;
  let more = start + window.len() < artists.len();
  let after = window.last().and_then(|uri| uri.rsplit(':').next());
  Ok(json!({
    "artists": {
      "href": "",
      "items": items,
      "limit": limit,
      "total": artists.len(),
      "next": more.then_some("internal://following"),
      "cursors": { "after": after },
    }
  }))
}

fn uris_from(query: &[(&str, String)], body: Option<&Value>) -> Vec<String> {
  let from_body = body.and_then(|body| body["uris"].as_array()).map(|uris| {
    uris
      .iter()
      .filter_map(|uri| uri.as_str().map(str::to_string))
      .collect::<Vec<_>>()
  });
  from_body.unwrap_or_else(|| {
    query_value(query, "uris")
      .unwrap_or_default()
      .split(',')
      .filter(|uri| !uri.is_empty())
      .map(str::to_string)
      .collect()
  })
}

pub async fn contains(session: &SpotifySession, query: &[(&str, String)]) -> anyhow::Result<Value> {
  let uris = uris_from(query, None);
  let mut found = vec![false; uris.len()];
  let mut by_set: HashMap<&'static str, Vec<usize>> = HashMap::new();
  let mut playlist_indexes = Vec::new();
  for (index, uri) in uris.iter().enumerate() {
    match set_for(uri) {
      Some(set) => by_set.entry(set).or_default().push(index),
      None if uri.starts_with("spotify:playlist:") => playlist_indexes.push(index),
      None => {}
    }
  }
  for (set, indexes) in by_set {
    let set_uris: Vec<&str> = indexes.iter().map(|&index| uris[index].as_str()).collect();
    let answers = session
      .session()
      .spclient()
      .collection_contains(set, &set_uris)
      .await
      .map_err(upstream_error)?;
    for (index, answer) in indexes.into_iter().zip(answers) {
      found[index] = answer;
    }
  }
  if !playlist_indexes.is_empty() {
    let listed = playlists::listed_uris(session).await?;
    for index in playlist_indexes {
      found[index] = listed.contains(&uris[index]);
    }
  }
  Ok(json!(found))
}

/// `PUT me/library` (save) and `DELETE me/library` (remove).
pub async fn write(
  session: &SpotifySession,
  query: &[(&str, String)],
  body: Option<&Value>,
  removed: bool,
) -> anyhow::Result<Value> {
  let uris = uris_from(query, body);
  if uris.is_empty() {
    return Err(api_error(StatusCode::BAD_REQUEST, "No uris"));
  }
  let now = Utc::now().timestamp();
  let mut by_set: HashMap<&'static str, Vec<String>> = HashMap::new();
  let mut playlist_uris = Vec::new();
  for uri in uris {
    match set_for(&uri) {
      Some(set) => by_set.entry(set).or_default().push(uri),
      None if uri.starts_with("spotify:playlist:") => playlist_uris.push(uri),
      None => {
        return Err(api_error(
          StatusCode::BAD_REQUEST,
          format!("Unsupported uri {uri}"),
        ))
      }
    }
  }
  for (set, set_uris) in by_set {
    let items = set_uris
      .iter()
      .map(|uri| CollectionItem {
        uri: uri.clone(),
        added_at: if removed { 0 } else { now as i32 },
        is_removed: removed,
        ..Default::default()
      })
      .collect();
    session
      .session()
      .spclient()
      .write_collection(set, items)
      .await
      .map_err(upstream_error)?;
    apply_to_cache(set, &set_uris, removed, now).await;
  }
  for uri in playlist_uris {
    if removed {
      playlists::unlist(session, &uri).await?;
    } else {
      playlists::list(session, &uri).await?;
    }
  }
  Ok(Value::Null)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn each_uri_kind_maps_to_its_collection_set() {
    assert_eq!(set_for("spotify:track:x"), Some("collection"));
    assert_eq!(set_for("spotify:album:x"), Some("collection"));
    assert_eq!(set_for("spotify:artist:x"), Some("artist"));
    assert_eq!(set_for("spotify:show:x"), Some("show"));
    assert_eq!(set_for("spotify:episode:x"), Some("listenlater"));
    assert_eq!(set_for("spotify:playlist:x"), None);
  }

  #[test]
  fn uris_come_from_the_body_before_the_query() {
    let query = [("uris", "spotify:track:a,spotify:track:b".to_string())];
    assert_eq!(uris_from(&query, None).len(), 2);
    let body = json!({ "uris": ["spotify:album:c"] });
    assert_eq!(uris_from(&query, Some(&body)), vec!["spotify:album:c"]);
  }
}
