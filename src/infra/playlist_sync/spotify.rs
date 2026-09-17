use super::{PlaylistRead, SyncClient};
use crate::core::app::App;
use crate::core::playlist_sync::{normalize_isrc, pick_candidate, SyncTrack};
use crate::infra::network::requests::spotify_api_request_json_for_with_refresh;
use crate::infra::network::search::SPOTIFY_SEARCH_LIMIT;
use anyhow::{anyhow, Result};
use reqwest::Method;
use rspotify::AuthCodePkceSpotify;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Everything the matcher needs from one playlist item and nothing else, asked
/// for under both spellings so either side of Spotify's `track` rename parses.
const ITEM_FIELDS: &str = "items(item(id,uri,type,is_local,name,duration_ms,artists(name),external_ids(isrc)),track(id,uri,type,is_local,name,duration_ms,artists(name),external_ids(isrc))),next";
/// The library endpoints still take 50 under a Development Mode app.
const PAGE_LIMIT: u32 = 50;
/// URIs per playlist write call.
const WRITE_CHUNK: usize = 100;
/// Hard cap on one playlist read.
const MAX_ITEMS: usize = 10_000;

/// Spotify playlist reads, searches and writes through the shared paced helper.
pub(crate) struct SpotifyClient {
  spotify: AuthCodePkceSpotify,
  token_cache_path: PathBuf,
  app: Arc<Mutex<App>>,
}

impl SpotifyClient {
  pub(crate) fn new(
    spotify: AuthCodePkceSpotify,
    token_cache_path: PathBuf,
    app: Arc<Mutex<App>>,
  ) -> Self {
    SpotifyClient {
      spotify,
      token_cache_path,
      app,
    }
  }

  /// One `search` call as candidates, capped by the endpoint's own ceiling.
  async fn search(&self, q: &str, limit: u32) -> Result<Vec<SyncTrack>> {
    let params = [
      ("q", q.to_string()),
      ("type", "track".to_string()),
      ("limit", limit.min(SPOTIFY_SEARCH_LIMIT).to_string()),
      ("market", "from_token".to_string()),
    ];
    let value = spotify_api_request_json_for_with_refresh(
      &self.spotify,
      Method::GET,
      "search",
      &params,
      None,
      &self.token_cache_path,
      &self.app,
    )
    .await?;
    Ok(search_candidates(value))
  }

  /// The ISRC query's hit, with a failed or empty query reading as no hit.
  async fn resolve_by_isrc(&self, target: &SyncTrack) -> Option<SyncTrack> {
    let isrc = normalize_isrc(target.isrc.as_deref()?);
    if isrc.is_empty() {
      return None;
    }
    let found = self.search(&isrc_query(&isrc), 5).await.ok()?;
    let index = pick_candidate(target, &found)?;
    Some(found[index].clone())
  }
}

impl SyncClient for SpotifyClient {
  async fn find_playlist(&self, name: &str) -> Result<Option<String>> {
    let me = spotify_api_request_json_for_with_refresh(
      &self.spotify,
      Method::GET,
      "me",
      &[],
      None,
      &self.token_cache_path,
      &self.app,
    )
    .await?;
    let me = me
      .get("id")
      .and_then(Value::as_str)
      .unwrap_or_default()
      .to_string();
    let mut offset: u32 = 0;
    loop {
      let params = [
        ("fields", "items(id,name,owner(id)),next".to_string()),
        ("limit", PAGE_LIMIT.to_string()),
        ("offset", offset.to_string()),
      ];
      let value = spotify_api_request_json_for_with_refresh(
        &self.spotify,
        Method::GET,
        "me/playlists",
        &params,
        None,
        &self.token_cache_path,
        &self.app,
      )
      .await?;
      let (hit, count, has_next) = owned_playlist_on_page(&value, name, &me);
      if hit.is_some() {
        return Ok(hit);
      }
      offset = offset.saturating_add(PAGE_LIMIT);
      if is_last_page(count, has_next) || offset as usize >= MAX_ITEMS {
        return Ok(None);
      }
    }
  }

  async fn create_playlist(&self, name: &str) -> Result<String> {
    let value = spotify_api_request_json_for_with_refresh(
      &self.spotify,
      Method::POST,
      "me/playlists",
      &[],
      Some(create_body(name)),
      &self.token_cache_path,
      &self.app,
    )
    .await?;
    created_playlist_uri(value)
  }

  async fn read_playlist(&self, playlist_uri: &str) -> Result<PlaylistRead> {
    let path = format!("playlists/{}/items", playlist_id_of(playlist_uri));
    let mut out = PlaylistRead::default();
    let mut offset: u32 = 0;
    loop {
      let params = [
        ("fields", ITEM_FIELDS.to_string()),
        ("limit", PAGE_LIMIT.to_string()),
        ("offset", offset.to_string()),
      ];
      log::info!("playlist sync: reading Spotify playlist page at offset {offset}");
      let value = spotify_api_request_json_for_with_refresh(
        &self.spotify,
        Method::GET,
        &path,
        &params,
        None,
        &self.token_cache_path,
        &self.app,
      )
      .await?;
      let page = read_items_page(value);
      if page.unrecognized > 0 {
        return Err(anyhow!(
          "Spotify playlist items came back in an unknown shape; nothing was changed"
        ));
      }
      out.tracks.extend(page.read.tracks);
      out.not_syncable.extend(page.read.not_syncable);
      offset = offset.saturating_add(PAGE_LIMIT);
      if is_last_page(page.count, page.has_next) || offset as usize >= MAX_ITEMS {
        return Ok(out);
      }
    }
  }

  async fn resolve(&self, target: &SyncTrack) -> Result<Option<SyncTrack>> {
    if let Some(found) = self.resolve_by_isrc(target).await {
      return Ok(Some(found));
    }
    let found = self
      .search(&title_query(target), SPOTIFY_SEARCH_LIMIT)
      .await?;
    Ok(pick_candidate(target, &found).map(|index| found[index].clone()))
  }

  async fn add(&self, playlist_uri: &str, tracks: &[SyncTrack]) -> Result<()> {
    let path = format!("playlists/{}/items", playlist_id_of(playlist_uri));
    for chunk in tracks.chunks(WRITE_CHUNK) {
      spotify_api_request_json_for_with_refresh(
        &self.spotify,
        Method::POST,
        &path,
        &[],
        Some(add_body(chunk)),
        &self.token_cache_path,
        &self.app,
      )
      .await?;
    }
    Ok(())
  }

  async fn remove(&self, playlist_uri: &str, keys: &[String]) -> Result<()> {
    let path = format!("playlists/{}/items", playlist_id_of(playlist_uri));
    for chunk in keys.chunks(WRITE_CHUNK) {
      spotify_api_request_json_for_with_refresh(
        &self.spotify,
        Method::DELETE,
        &path,
        &[],
        Some(remove_body(chunk)),
        &self.token_cache_path,
        &self.app,
      )
      .await?;
    }
    Ok(())
  }
}

/// One `items` page, shaped by the fields mask; entries stay raw so a null one is skipped.
#[derive(Debug, Default, Deserialize)]
struct ItemsPage {
  #[serde(default)]
  items: Vec<Value>,
  #[serde(default)]
  next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemEnvelope {
  #[serde(default, alias = "track")]
  item: Option<RawTrack>,
}

/// One parsed `items` page.
#[derive(Debug, Default)]
struct ItemsRead {
  read: PlaylistRead,
  /// Raw entries on the page, dead ones included.
  count: usize,
  /// Entries that carry neither `item` nor `track`: a shape the sync must not trust.
  unrecognized: usize,
  has_next: bool,
}

/// Stop paging when a page is empty, or when it is short and Spotify names no next page.
fn is_last_page(count: usize, has_next: bool) -> bool {
  count == 0 || (!has_next && count < PAGE_LIMIT as usize)
}

/// A playlist item's track or a search result, every field optional.
#[derive(Debug, Deserialize)]
struct RawTrack {
  #[serde(default)]
  id: Option<String>,
  #[serde(default)]
  uri: Option<String>,
  #[serde(rename = "type", default)]
  kind: Option<String>,
  #[serde(default)]
  is_local: bool,
  #[serde(default)]
  name: String,
  #[serde(default)]
  duration_ms: Option<u64>,
  #[serde(default)]
  artists: Vec<RawArtist>,
  #[serde(default)]
  external_ids: RawExternalIds,
}

#[derive(Debug, Deserialize)]
struct RawArtist {
  #[serde(default)]
  name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawExternalIds {
  #[serde(default)]
  isrc: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchPage {
  #[serde(default)]
  tracks: Option<RawTracks>,
}

#[derive(Debug, Default, Deserialize)]
struct RawTracks {
  #[serde(default)]
  items: Vec<Value>,
}

/// The bare base62 id of a `spotify:playlist:<id>` URI, or the value as given.
fn playlist_id_of(uri: &str) -> &str {
  uri.strip_prefix("spotify:playlist:").unwrap_or(uri)
}

/// One raw item as the sync currency, under the key the caller can address it with.
fn sync_track_of(raw: &RawTrack, key: String) -> SyncTrack {
  SyncTrack {
    key,
    isrc: raw
      .external_ids
      .isrc
      .as_ref()
      .filter(|isrc| !isrc.trim().is_empty())
      .cloned(),
    title: raw.name.clone(),
    artist: raw
      .artists
      .first()
      .map(|artist| artist.name.clone())
      .unwrap_or_default(),
    duration_ms: raw.duration_ms.filter(|ms| *ms > 0),
  }
}

/// One `items` page: its tracks, its unsyncable items, and how the page ended.
fn read_items_page(page: Value) -> ItemsRead {
  let page: ItemsPage = serde_json::from_value(page).unwrap_or_default();
  let mut out = ItemsRead {
    count: page.items.len(),
    has_next: page.next.is_some(),
    ..Default::default()
  };
  for entry in page.items {
    if let Some(object) = entry.as_object() {
      if !object.contains_key("item") && !object.contains_key("track") {
        out.unrecognized += 1;
        continue;
      }
    }
    let Ok(envelope) = serde_json::from_value::<ItemEnvelope>(entry) else {
      continue;
    };
    let Some(raw) = envelope.item else {
      continue;
    };
    let syncable_key = raw
      .id
      .clone()
      .filter(|_| !raw.is_local && raw.kind.as_deref() == Some("track"));
    if let Some(key) = syncable_key {
      out.read.tracks.push(sync_track_of(&raw, key));
    } else if let Some(key) = raw.uri.clone().or_else(|| raw.id.clone()) {
      out.read.not_syncable.push(sync_track_of(&raw, key));
    }
  }
  out
}

/// The `tracks.items` of a search response as candidates.
fn search_candidates(page: Value) -> Vec<SyncTrack> {
  let page: SearchPage = serde_json::from_value(page).unwrap_or_default();
  page
    .tracks
    .unwrap_or_default()
    .items
    .into_iter()
    .filter_map(|entry| serde_json::from_value::<RawTrack>(entry).ok())
    .filter_map(|raw| {
      let key = raw.id.clone().filter(|_| !raw.is_local)?;
      Some(sync_track_of(&raw, key))
    })
    .collect()
}

/// `isrc:<CODE>`, normalized.
fn isrc_query(isrc: &str) -> String {
  format!("isrc:{}", normalize_isrc(isrc))
}

/// `track:<title> artist:<artist>`, with the artist clause dropped when there is none.
fn title_query(target: &SyncTrack) -> String {
  let title = target.title.trim();
  let artist = target.artist.trim();
  if artist.is_empty() {
    return format!("track:{title}");
  }
  format!("track:{title} artist:{artist}")
}

/// The body of one new private playlist.
fn create_body(name: &str) -> Value {
  json!({
    "name": name,
    "public": false,
    "collaborative": false,
    "description": "Created with spotatui"
  })
}

/// The user's own playlist named `name` on one `me/playlists` page, as a URI, with
/// the page's size and whether a next page exists.
fn owned_playlist_on_page(page: &Value, name: &str, me: &str) -> (Option<String>, usize, bool) {
  let empty = Vec::new();
  let items = page
    .get("items")
    .and_then(Value::as_array)
    .unwrap_or(&empty);
  let hit = items
    .iter()
    .find(|item| {
      let owner = item
        .get("owner")
        .and_then(|owner| owner.get("id"))
        .and_then(Value::as_str);
      owner == Some(me)
        && item
          .get("name")
          .and_then(Value::as_str)
          .is_some_and(|listed| super::same_name(listed, name))
    })
    .and_then(|item| item.get("id").and_then(Value::as_str))
    .map(|id| format!("spotify:playlist:{id}"));
  let has_next = page.get("next").is_some_and(|next| !next.is_null());
  (hit, items.len(), has_next)
}

/// The `spotify:playlist:` URI of a create response.
fn created_playlist_uri(value: Value) -> Result<String> {
  value
    .get("id")
    .and_then(Value::as_str)
    .filter(|id| !id.is_empty())
    .map(|id| format!("spotify:playlist:{id}"))
    .ok_or_else(|| anyhow!("Spotify created the playlist but returned no id"))
}

/// `{"uris": [...]}` for one batch.
fn add_body(tracks: &[SyncTrack]) -> Value {
  let uris: Vec<String> = tracks
    .iter()
    .map(|track| format!("spotify:track:{}", track.key))
    .collect();
  json!({ "uris": uris })
}

/// `{"items": [{"uri": ...}]}` for one batch, no positions, so every occurrence goes.
fn remove_body(keys: &[String]) -> Value {
  let items: Vec<Value> = keys
    .iter()
    .map(|key| json!({ "uri": format!("spotify:track:{key}") }))
    .collect();
  json!({ "items": items })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn track(key: &str, title: &str, artist: &str) -> SyncTrack {
    SyncTrack {
      key: key.to_string(),
      isrc: None,
      title: title.to_string(),
      artist: artist.to_string(),
      duration_ms: None,
    }
  }

  #[test]
  fn the_items_mask_names_item_and_the_isrc() {
    assert!(ITEM_FIELDS.starts_with("items(item("));
    assert!(ITEM_FIELDS.contains("is_local"));
    assert!(ITEM_FIELDS.contains("duration_ms"));
    assert!(ITEM_FIELDS.contains("external_ids(isrc)"));
    assert!(ITEM_FIELDS.contains("),track("));
    assert!(ITEM_FIELDS.ends_with(",next"));
    assert!(!ITEM_FIELDS.contains("added_at"));
  }

  #[test]
  fn a_track_spelled_page_parses_and_an_unknown_shape_is_flagged() {
    let renamed = json!({
      "next": "https://api.spotify.com/v1/playlists/x/items?offset=50",
      "items": [
        { "track": { "id": "1", "type": "track", "name": "Alpha", "artists": [{ "name": "A" }] } },
        { "track": null }
      ]
    });
    let page = read_items_page(renamed);
    assert_eq!(page.read.keys(), ["1"]);
    assert_eq!((page.count, page.unrecognized, page.has_next), (2, 0, true));

    let unknown = json!({ "items": [{ "entry": { "id": "1" } }] });
    let page = read_items_page(unknown);
    assert_eq!((page.count, page.unrecognized), (1, 1));
    assert!(page.read.tracks.is_empty());

    assert!(is_last_page(0, true));
    assert!(is_last_page(3, false));
    assert!(!is_last_page(3, true));
    assert!(!is_last_page(PAGE_LIMIT as usize, false));
  }

  #[test]
  fn a_page_splits_tracks_from_local_files_and_episodes() {
    let page = json!({
      "total": 4,
      "items": [
        { "item": {
          "id": "4cOdK2wGLETKBW3PvgPWqT",
          "uri": "spotify:track:4cOdK2wGLETKBW3PvgPWqT",
          "type": "track",
          "is_local": false,
          "name": "Never Gonna Give You Up",
          "duration_ms": 213573,
          "artists": [{ "name": "Rick Astley" }, { "name": "Someone Else" }],
          "external_ids": { "isrc": "GBARL9300135" }
        }},
        { "item": {
          "uri": "spotify:local:Artist:Album:Song:180",
          "type": "track",
          "is_local": true,
          "name": "Song",
          "artists": [{ "name": "Artist" }]
        }},
        { "item": {
          "id": "5Ggwid8mfGpx6PNfUVjWAE",
          "uri": "spotify:episode:5Ggwid8mfGpx6PNfUVjWAE",
          "type": "episode",
          "is_local": false,
          "name": "An episode"
        }},
        { "item": null },
        null
      ]
    });

    let page = read_items_page(page);
    let read = page.read;

    assert_eq!(
      (page.count, page.unrecognized, page.has_next),
      (5, 0, false)
    );
    assert_eq!(read.tracks.len(), 1);
    assert_eq!(read.tracks[0].key, "4cOdK2wGLETKBW3PvgPWqT");
    assert_eq!(read.tracks[0].isrc.as_deref(), Some("GBARL9300135"));
    assert_eq!(read.tracks[0].title, "Never Gonna Give You Up");
    assert_eq!(read.tracks[0].artist, "Rick Astley");
    assert_eq!(read.tracks[0].duration_ms, Some(213573));
    let keys: Vec<&str> = read
      .not_syncable
      .iter()
      .map(|entry| entry.key.as_str())
      .collect();
    assert_eq!(
      keys,
      [
        "spotify:local:Artist:Album:Song:180",
        "spotify:episode:5Ggwid8mfGpx6PNfUVjWAE"
      ]
    );
    assert_eq!(read.keys(), ["4cOdK2wGLETKBW3PvgPWqT"]);
  }

  #[test]
  fn a_search_page_becomes_candidates_with_their_isrc() {
    let page = json!({
      "tracks": {
        "items": [
          {
            "id": "1",
            "type": "track",
            "name": "Alpha",
            "duration_ms": 1000,
            "artists": [{ "name": "A" }],
            "external_ids": { "isrc": "GB-AAA-00-00001" }
          },
          { "type": "track", "name": "No id", "artists": [] },
          { "id": "3", "type": "track", "is_local": true, "name": "Local", "artists": [] }
        ]
      }
    });

    let found = search_candidates(page);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, "1");
    assert_eq!(found[0].isrc.as_deref(), Some("GB-AAA-00-00001"));
    assert_eq!(found[0].title, "Alpha");
    assert_eq!(found[0].artist, "A");
    assert!(search_candidates(json!({})).is_empty());
  }

  #[test]
  fn the_isrc_query_is_normalized_and_the_fallback_names_title_and_artist() {
    assert_eq!(isrc_query("gb-aaa-00-00001"), "isrc:GBAAA0000001");
    assert_eq!(
      title_query(&track("1", "  Alpha  ", " A ")),
      "track:Alpha artist:A"
    );
    assert_eq!(title_query(&track("1", "Alpha", "")), "track:Alpha");
  }

  #[test]
  fn an_add_body_carries_track_uris_a_hundred_per_call() {
    let tracks: Vec<SyncTrack> = (0..250).map(|n| track(&n.to_string(), "T", "A")).collect();
    let batches: Vec<usize> = tracks
      .chunks(WRITE_CHUNK)
      .map(|chunk| chunk.len())
      .collect();

    assert_eq!(batches, [100, 100, 50]);
    assert_eq!(
      add_body(&tracks[..2]),
      json!({ "uris": ["spotify:track:0", "spotify:track:1"] })
    );
    assert_eq!(add_body(&[]), json!({ "uris": [] }));
  }

  #[test]
  fn a_remove_body_names_items_by_uri_without_positions() {
    let body = remove_body(&["abc".to_string(), "def".to_string()]);

    assert_eq!(
      body,
      json!({ "items": [{ "uri": "spotify:track:abc" }, { "uri": "spotify:track:def" }] })
    );
    assert!(!body.to_string().contains("positions"));
  }

  #[test]
  fn only_the_users_own_playlist_with_the_name_is_found_on_a_page() {
    let page = json!({
      "next": null,
      "items": [
        { "id": "theirs", "name": "Road Trip", "owner": { "id": "someone" } },
        { "id": "mine", "name": " road trip ", "owner": { "id": "me" } }
      ]
    });
    assert_eq!(
      owned_playlist_on_page(&page, "Road Trip", "me"),
      (Some("spotify:playlist:mine".to_string()), 2, false)
    );
    assert_eq!(owned_playlist_on_page(&page, "Gym", "me"), (None, 2, false));

    let more = json!({ "next": "https://api.spotify.com/v1/me/playlists?offset=50", "items": [] });
    assert_eq!(owned_playlist_on_page(&more, "Gym", "me"), (None, 0, true));
  }

  #[test]
  fn a_created_playlist_answers_its_uri_and_a_missing_id_is_an_error() {
    assert_eq!(
      created_playlist_uri(json!({ "id": "3cEYpjA9oz9GiPac4AsH4n" })).unwrap(),
      "spotify:playlist:3cEYpjA9oz9GiPac4AsH4n"
    );
    assert!(created_playlist_uri(json!({ "id": "" })).is_err());
    assert!(created_playlist_uri(json!({ "name": "Road Trip" })).is_err());
    assert_eq!(
      create_body("Road Trip"),
      json!({
        "name": "Road Trip",
        "public": false,
        "collaborative": false,
        "description": "Created with spotatui"
      })
    );
  }

  #[test]
  fn a_playlist_uri_or_a_bare_id_both_yield_the_bare_id() {
    assert_eq!(
      playlist_id_of("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
      "37i9dQZF1DXcBWIGoYBM5M"
    );
    assert_eq!(
      playlist_id_of("37i9dQZF1DXcBWIGoYBM5M"),
      "37i9dQZF1DXcBWIGoYBM5M"
    );
  }
}
