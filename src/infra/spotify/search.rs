//! Search through Pathfinder's persisted search queries: one per type, with
//! `searchDesktop` (every type in one answer) as the fallback. Pathfinder
//! ranks; the catalogue objects come from extended-metadata.

use super::catalog::{full_artists, full_tracks};
use super::json;
use super::metadata::get_many;
use super::router::{api_error, query_value, Paging};
use super::session::SpotifySession;
use librespot_core::error::ErrorKind;
use librespot_metadata::{Album, Show};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Persisted query hashes, newest first. Spotify rotates them every few
/// months and answers a retired one with HTTP 412; the next one is tried then.
const SEARCH_DESKTOP_HASHES: &[&str] = &[
  "eef7cc54888d91bdd6802623477873caa3948ae173a0c34fd86827b267e94c03",
  "fcad5a3e0d5af727fb76966f06971c19cfa2275e6ff7671196753e008611873c",
  "3c9d3f60dac5dea3876b6db3f534192b1c1d90032c4233c1bbaba526db41eb31",
  "4801118d4a100f756e833d33984436a3899cff359c532f8fd3aaf174b60b3b49",
];
/// A repeated search within this time is answered from memory.
const RESULT_TTL: Duration = Duration::from_secs(15);
const MAX_LIMIT: usize = 50;

static WORKING_HASH: AtomicUsize = AtomicUsize::new(0);

/// A persisted Pathfinder search query.
struct SearchQuery {
  operation: &'static str,
  /// Newest first; only `searchDesktop` keeps older ones to fall back on.
  hashes: &'static [&'static str],
}

/// Every type in one large answer: the fallback when a type's own query fails.
const DESKTOP: SearchQuery = SearchQuery {
  operation: "searchDesktop",
  hashes: SEARCH_DESKTOP_HASHES,
};

/// The query of one search type. Its answer is a tenth of `searchDesktop`'s
/// and comes in about half the time, with the same items in the same section.
fn type_query(kind: &str) -> Option<SearchQuery> {
  let (operation, hashes): (_, &'static [&'static str]) = match kind {
    "track" => (
      "searchTracks",
      &["b02683192a98dde7966b5e6655a79eeb62713eab703eda9902c932818dd52751"],
    ),
    "artist" => (
      "searchArtists",
      &["7bf95d754fdbe32c8b161fbbe54d1ae50974900df4dce4c8f1afcbcad153224d"],
    ),
    "album" => (
      "searchAlbums",
      &["202cb3305e31e5a0767ba7925f28bd728cf8f8b0217e6da43909056071cd70e9"],
    ),
    "playlist" => (
      "searchPlaylists",
      &["d520014e748f9ea44f7707d8df1819867ac1205e8b7f3e28f22fe5fc858921b1"],
    ),
    "show" => (
      "searchPodcasts",
      &["0195d9f61b43606d490bca64c3456e3593528cea6cc05c7e822c7c42beed0f4e"],
    ),
    _ => return None,
  };
  Some(SearchQuery { operation, hashes })
}

type Cache = HashMap<(&'static str, String, usize, usize), (Value, Instant)>;

fn cache() -> &'static Mutex<Cache> {
  static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Web API field filters (`track:"x" artist:"y"`, `isrc:CODE`) as plain search
/// terms: Pathfinder takes free text.
fn search_term(query: &str) -> String {
  let mut term = String::new();
  for word in query.replace('"', " ").split_whitespace() {
    let word = ["track:", "artist:", "album:", "isrc:"]
      .iter()
      .find_map(|filter| word.strip_prefix(filter))
      .unwrap_or(word);
    if !word.is_empty() {
      if !term.is_empty() {
        term.push(' ');
      }
      term.push_str(word);
    }
  }
  term
}

async fn search_v2(
  session: &SpotifySession,
  query: &SearchQuery,
  term: &str,
  offset: usize,
  limit: usize,
) -> anyhow::Result<Value> {
  // The parallel calls of a search that falls back share one searchDesktop answer.
  static DESKTOP_FETCH: OnceLock<Mutex<()>> = OnceLock::new();
  let _shared = match query.operation == DESKTOP.operation {
    true => Some(DESKTOP_FETCH.get_or_init(|| Mutex::new(())).lock().await),
    false => None,
  };
  let key = (query.operation, term.to_string(), offset, limit);
  {
    let mut cache = cache().lock().await;
    cache.retain(|_, (_, at)| at.elapsed() < RESULT_TTL);
    if let Some((value, _)) = cache.get(&key) {
      return Ok(value.clone());
    }
  }
  let variables = json!({
    "searchTerm": term,
    "offset": offset,
    "limit": limit,
    "numberOfTopResults": 5,
    "includeAudiobooks": false,
    "includeArtistHasConcertsField": false,
    "includePreReleases": false,
    "includeLocalConcertsField": false,
    "includeAuthors": false,
  });
  // Only searchDesktop has older hashes, so only its working one is remembered.
  let rotates = query.hashes.len() > 1;
  let start = if rotates {
    WORKING_HASH.load(Ordering::Relaxed)
  } else {
    0
  };
  for step in 0..query.hashes.len() {
    let index = (start + step) % query.hashes.len();
    match session
      .session()
      .spclient()
      .pathfinder_query(query.operation, query.hashes[index], variables.clone())
      .await
    {
      Ok(answer) if answer["data"]["searchV2"].is_object() => {
        if rotates {
          WORKING_HASH.store(index, Ordering::Relaxed);
        }
        let result = answer["data"]["searchV2"].clone();
        cache()
          .lock()
          .await
          .insert(key, (result.clone(), Instant::now()));
        return Ok(result);
      }
      Ok(answer) => log::warn!("{} gave no result: {}", query.operation, answer["errors"]),
      Err(e) if e.kind == ErrorKind::FailedPrecondition => {
        log::warn!(
          "{} hash {} is retired",
          query.operation,
          &query.hashes[index][..8]
        );
      }
      Err(e) => return Err(super::router::upstream_error(e)),
    }
  }
  Err(api_error(
    StatusCode::SERVICE_UNAVAILABLE,
    format!(
      "Search is unavailable: every known {} hash is retired",
      query.operation
    ),
  ))
}

fn uris(section: &Value, pointer: &str) -> Vec<String> {
  section["items"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|item| item.pointer(pointer)?.as_str().map(str::to_string))
    .collect()
}

fn total(section: &Value) -> usize {
  section["totalCount"].as_u64().unwrap_or(0) as usize
}

fn playlist(data: &Value) -> Option<Value> {
  let uri = data["uri"].as_str()?;
  let id = uri.strip_prefix("spotify:playlist:")?;
  let owner = &data["ownerV2"]["data"];
  let username = owner["username"].as_str().unwrap_or_default();
  let images: Vec<Value> = data["images"]["items"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|image| image["sources"].get(0))
    .map(|source| json!({ "url": source["url"], "width": source["width"], "height": source["height"] }))
    .collect();
  Some(json!({
    "id": id,
    "uri": uri,
    "type": "playlist",
    "href": format!("https://api.spotify.com/v1/playlists/{id}"),
    "external_urls": { "spotify": format!("https://open.spotify.com/playlist/{id}") },
    "name": data["name"].as_str().unwrap_or_default(),
    "description": data["description"].as_str().unwrap_or_default(),
    "collaborative": false,
    "images": images,
    "owner": {
      "id": username,
      "display_name": owner["name"].as_str().unwrap_or(username),
      "type": "user",
      "uri": format!("spotify:user:{username}"),
      "href": format!("https://api.spotify.com/v1/users/{username}"),
      "external_urls": { "spotify": format!("https://open.spotify.com/user/{username}") },
    },
    "snapshot_id": "",
    "items": { "href": "", "total": 0 },
  }))
}

/// The answer of the type's own query, else of searchDesktop.
async fn type_result(
  session: &SpotifySession,
  kind: &str,
  term: &str,
  offset: usize,
  limit: usize,
) -> anyhow::Result<Value> {
  if let Some(query) = type_query(kind) {
    match search_v2(session, &query, term, offset, limit).await {
      Ok(result) => return Ok(result),
      Err(e) => log::warn!(
        "{} failed, searching with searchDesktop: {e:#}",
        query.operation
      ),
    }
  }
  search_v2(session, &DESKTOP, term, offset, limit).await
}

/// `GET search`: one Web API page per requested type.
pub async fn search(session: &SpotifySession, query: &[(&str, String)]) -> anyhow::Result<Value> {
  let term = search_term(query_value(query, "q").unwrap_or_default());
  if term.is_empty() {
    return Err(api_error(StatusCode::BAD_REQUEST, "No search query"));
  }
  let paging = Paging::from_query(query, 20);
  let limit = paging.limit.min(MAX_LIMIT);
  let page =
    |items: Vec<Value>, section: &Value| json::page(items, paging.offset, limit, total(section));

  let mut answer = serde_json::Map::new();
  for kind in query_value(query, "type").unwrap_or("track").split(',') {
    let result = type_result(session, kind, &term, paging.offset, limit).await?;
    match kind {
      "track" => {
        let section = &result["tracksV2"];
        let items = full_tracks(session, &uris(section, "/item/data/uri")).await?;
        answer.insert("tracks".into(), page(items, section));
      }
      "album" => {
        let section = &result["albumsV2"];
        let items = get_many::<Album>(session, &uris(section, "/data/uri"))
          .await?
          .into_iter()
          .flatten()
          .map(|album| json::album_simplified(&album))
          .collect();
        answer.insert("albums".into(), page(items, section));
      }
      "artist" => {
        let section = &result["artists"];
        let items = full_artists(session, &uris(section, "/data/uri")).await?;
        answer.insert("artists".into(), page(items, section));
      }
      "show" => {
        let section = &result["podcasts"];
        let items = get_many::<Show>(session, &uris(section, "/data/uri"))
          .await?
          .into_iter()
          .flatten()
          .map(|show| json::show_simplified(&show))
          .collect();
        answer.insert("shows".into(), page(items, section));
      }
      "playlist" => {
        let section = &result["playlists"];
        let items = section["items"]
          .as_array()
          .into_iter()
          .flatten()
          .filter_map(|item| playlist(&item["data"]))
          .collect();
        answer.insert("playlists".into(), page(items, section));
      }
      _ => {}
    }
  }
  Ok(Value::Object(answer))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn web_api_field_filters_become_free_text() {
    assert_eq!(
      search_term(r#"track:"Get Lucky" artist:"Daft Punk""#),
      "Get Lucky Daft Punk"
    );
    assert_eq!(search_term("isrc:USRC17607839"), "USRC17607839");
    assert_eq!(search_term("  bonobo  "), "bonobo");
  }

  #[test]
  fn a_pathfinder_playlist_deserializes_as_a_simplified_playlist() {
    let data = json!({
      "uri": "spotify:playlist:56njU2ALIY4Fan7rLZeSG5",
      "name": "Best Of Bonobo",
      "images": { "items": [{ "sources": [{ "url": "https://i", "width": null, "height": null }] }] },
      "ownerV2": { "data": { "name": "Bonobo", "username": "bonobo_music" } },
    });
    let playlist: rspotify::model::SimplifiedPlaylist =
      serde_json::from_value(playlist(&data).unwrap()).unwrap();
    assert_eq!(playlist.name, "Best Of Bonobo");
    assert_eq!(playlist.owner.display_name.as_deref(), Some("Bonobo"));
  }
}
