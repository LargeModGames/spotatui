//! Every route against the real account, parsed into the types the app uses.
//! Ignored: they need the cached login. Run with
//! `cargo test --lib infra::spotify::live_tests -- --ignored --test-threads=1`.

use super::router::route;
use super::session::ALLOW_IN_TESTS;
use crate::infra::network::requests::normalize_spotify_payload;
use reqwest::Method;
use rspotify::model::{
  CurrentPlaybackContext, CurrentUserQueue, CursorBasedPage, DevicePayload, FullAlbum, FullArtist,
  FullShow, FullTrack, Page, PlayHistory, PlaylistItem, PrivateUser, SavedAlbum, SavedTrack,
  SimplifiedAlbum, SimplifiedEpisode, SimplifiedPlaylist, SimplifiedShow, SimplifiedTrack,
};
use rspotify::prelude::Id;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

const DISCOVERY: &str = "2noRn2Aes5aoNVsU6iWThc";
const DAFT_PUNK: &str = "4tZwfgrHOc3mvqYlEYSvVi";
const ONE_MORE_TIME: &str = "0DiWol3AO6WpXZgp0goxAV";

/// The name of every playlist a live test makes; the cleanup test unlists them.
const LIVE_PLAYLIST: &str = "spotatui live test";
const AERODYNAMIC: &str = "3H3cOQ6LBLSvmcaV7QkZEu";

async fn call(method: Method, path: &str, query: &[(&str, String)]) -> Value {
  send(method, path, query, None).await
}

async fn send(method: Method, path: &str, query: &[(&str, String)], body: Option<Value>) -> Value {
  ALLOW_IN_TESTS.store(true, std::sync::atomic::Ordering::Relaxed);
  route(&method, path, query, body.as_ref())
    .await
    .unwrap_or_else(|e| panic!("{method} {path}: {e:#}"))
}

async fn item_uris(playlist: &str) -> Vec<String> {
  let items: Page<PlaylistItem> = typed(&format!("playlists/{playlist}/items"), &[]).await;
  items
    .items
    .iter()
    .filter_map(|item| item.item.as_ref())
    .filter_map(|item| match item {
      rspotify::model::PlayableItem::Track(track) => track.id.as_ref().map(|id| id.uri()),
      rspotify::model::PlayableItem::Episode(episode) => Some(episode.id.uri()),
      _ => None,
    })
    .collect()
}

/// Unlists every playlist named [`LIVE_PLAYLIST`] from the user's list.
async fn unlist_live_playlists() -> usize {
  ALLOW_IN_TESTS.store(true, std::sync::atomic::Ordering::Relaxed);
  let session = super::session::current().await.expect("a session");
  let playlists: Page<SimplifiedPlaylist> = typed("me/playlists", &q(&[("limit", "1000")])).await;
  let mut unlisted = 0;
  for playlist in playlists.items.iter().filter(|p| p.name == LIVE_PLAYLIST) {
    println!(
      "{}: {:?}",
      playlist.id.uri(),
      item_uris(playlist.id.id()).await
    );
    super::playlists::unlist(&session, &playlist.id.uri())
      .await
      .expect("unlist");
    unlisted += 1;
  }
  unlisted
}

async fn typed<T: DeserializeOwned>(path: &str, query: &[(&str, String)]) -> T {
  let mut value = call(Method::GET, path, query).await;
  normalize_spotify_payload(&mut value);
  serde_json::from_value(value).unwrap_or_else(|e| panic!("GET {path}: {e}"))
}

fn q(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
  pairs
    .iter()
    .map(|(key, value)| (*key, value.to_string()))
    .collect()
}

#[derive(Deserialize)]
struct Tracks {
  tracks: Vec<FullTrack>,
}

#[derive(Deserialize)]
struct SimpleTracks {
  tracks: Vec<SimplifiedTrack>,
}

#[derive(Deserialize)]
struct Artists {
  artists: Vec<FullArtist>,
}

#[derive(Deserialize)]
struct Following {
  artists: CursorBasedPage<FullArtist>,
}

#[derive(Deserialize)]
struct SearchAll {
  tracks: Page<FullTrack>,
  albums: Page<SimplifiedAlbum>,
  artists: Page<FullArtist>,
  playlists: Page<SimplifiedPlaylist>,
  shows: Page<SimplifiedShow>,
}

#[derive(Deserialize)]
struct SavedShow {
  show: SimplifiedShow,
}

#[tokio::test]
#[ignore = "contacts Spotify with the cached login"]
async fn catalogue_routes_parse_into_the_app_types() {
  let user: PrivateUser = typed("me", &[]).await;
  assert!(!user.id.id().is_empty());

  let track: FullTrack = typed(&format!("tracks/{ONE_MORE_TIME}"), &[]).await;
  assert_eq!(track.name, "One More Time");
  let pair: Tracks = typed(
    "tracks",
    &q(&[("ids", &format!("{ONE_MORE_TIME},{ONE_MORE_TIME}"))]),
  )
  .await;
  assert_eq!(pair.tracks.len(), 2);

  let album: FullAlbum = typed(&format!("albums/{DISCOVERY}"), &[]).await;
  assert!(album.tracks.total >= 14, "Discovery has 14 tracks");
  let album_tracks: Page<SimplifiedTrack> = typed(
    &format!("albums/{DISCOVERY}/tracks"),
    &q(&[("limit", "5"), ("offset", "10")]),
  )
  .await;
  assert_eq!(album_tracks.offset, 10);
  assert!(!album_tracks.items.is_empty());

  let top: Tracks = typed(
    &format!("artists/{DAFT_PUNK}/top-tracks"),
    &q(&[("market", "NL")]),
  )
  .await;
  assert!(!top.tracks.is_empty());
  let related: Artists = typed(&format!("artists/{DAFT_PUNK}/related-artists"), &[]).await;
  println!("related artists: {}", related.artists.len());
  let mut offset = 0;
  loop {
    let page: Page<SimplifiedAlbum> = typed(
      &format!("artists/{DAFT_PUNK}/albums"),
      &q(&[("limit", "10"), ("offset", &offset.to_string())]),
    )
    .await;
    if page.next.is_none() {
      assert!(offset + page.items.len() > 0);
      break;
    }
    offset += 10;
    assert!(offset < 500, "artist albums never end");
  }

  let recommendations: SimpleTracks = typed(
    "recommendations",
    &q(&[("seed_tracks", ONE_MORE_TIME), ("limit", "50")]),
  )
  .await;
  assert!(!recommendations.tracks.is_empty());

  let search: SearchAll = typed(
    "search",
    &q(&[
      ("q", "daft punk"),
      ("type", "track,album,artist,playlist,show"),
      ("limit", "5"),
    ]),
  )
  .await;
  assert!(!search.tracks.items.is_empty());
  assert!(!search.albums.items.is_empty());
  assert!(!search.artists.items.is_empty());
  assert!(!search.playlists.items.is_empty());
  let show = search.shows.items.first().expect("a show in the results");
  let full_show: FullShow = typed(&format!("shows/{}", show.id.id()), &[]).await;
  let episodes: Page<SimplifiedEpisode> = typed(
    &format!("shows/{}/episodes", full_show.id.id()),
    &q(&[("limit", "5")]),
  )
  .await;
  assert!(!episodes.items.is_empty());
}

#[tokio::test]
#[ignore = "contacts Spotify with the cached login"]
async fn audio_analysis_parses_into_pitch_segments() {
  let value = call(Method::GET, &format!("audio-analysis/{ONE_MORE_TIME}"), &[]).await;
  let analysis = crate::infra::network::mapping::track_analysis(value).unwrap();
  let peaks = analysis.segments.iter().map(|segment| segment.loudness_max);
  let quietest = peaks.clone().fold(f32::MAX, f32::min);
  let loudest = peaks.fold(f32::MIN, f32::max);
  println!(
    "sections: {}, segments: {}, peak loudness {quietest:.1} to {loudest:.1} dB",
    analysis.sections.len(),
    analysis.segments.len()
  );
  assert!(!analysis.sections.is_empty());
  assert!(quietest < loudest && loudest < 0.0);
  assert!(analysis
    .segments
    .iter()
    .any(|segment| segment.pitches.iter().any(|pitch| *pitch > 0.0)));
}

#[tokio::test]
#[ignore = "contacts Spotify with the cached login"]
async fn library_routes_parse_into_the_app_types() {
  let _: Page<SavedTrack> = typed("me/tracks", &q(&[("limit", "20")])).await;
  let _: Page<SavedAlbum> = typed("me/albums", &q(&[("limit", "5")])).await;
  let shows: Page<SavedShow> = typed("me/shows", &q(&[("limit", "5")])).await;
  assert!(shows.items.iter().all(|saved| !saved.show.name.is_empty()));
  let following: Following =
    typed("me/following", &q(&[("type", "artist"), ("limit", "20")])).await;
  assert!(following
    .artists
    .items
    .iter()
    .all(|artist| !artist.name.is_empty()));
  let contains: Vec<bool> = typed(
    "me/library/contains",
    &q(&[(
      "uris",
      &format!("spotify:track:{ONE_MORE_TIME},spotify:artist:{DAFT_PUNK}"),
    )]),
  )
  .await;
  assert_eq!(contains.len(), 2);

  let playlists: Page<SimplifiedPlaylist> = typed("me/playlists", &q(&[("limit", "50")])).await;
  let playlist = playlists
    .items
    .iter()
    .find(|p| p.items.total > 0)
    .expect("a playlist with items");
  let items: Page<PlaylistItem> = typed(
    &format!("playlists/{}/items", playlist.id.id()),
    &q(&[("limit", "20"), ("offset", "0")]),
  )
  .await;
  assert!(!items.items.is_empty());
  let owner = call(Method::GET, &format!("playlists/{}", playlist.id.id()), &[]).await;
  assert!(owner["owner"]["id"].is_string());

  let _: Page<FullTrack> = typed(
    "me/top/tracks",
    &q(&[("time_range", "medium_term"), ("limit", "50")]),
  )
  .await;
  let _: Page<FullArtist> = typed("me/top/artists", &q(&[("limit", "5")])).await;
  let _: CursorBasedPage<PlayHistory> =
    typed("me/player/recently-played", &q(&[("limit", "50")])).await;
}

#[tokio::test]
#[ignore = "changes the account: makes a playlist, edits it, and unlists it"]
async fn a_playlist_is_created_edited_and_unlisted() {
  let created = send(
    Method::POST,
    "me/playlists",
    &[],
    Some(serde_json::json!({ "name": LIVE_PLAYLIST, "public": false })),
  )
  .await;
  let id = created["id"].as_str().expect("an id").to_string();
  let first = format!("spotify:track:{ONE_MORE_TIME}");
  let second = format!("spotify:track:{AERODYNAMIC}");
  send(
    Method::POST,
    &format!("playlists/{id}/items"),
    &[],
    Some(serde_json::json!({ "uris": [first, second] })),
  )
  .await;
  assert_eq!(item_uris(&id).await, vec![first.clone(), second.clone()]);

  // Position 0 holds the first track, so this names the wrong one.
  send(
    Method::DELETE,
    &format!("playlists/{id}/items"),
    &[],
    Some(serde_json::json!({ "items": [{ "uri": second, "positions": [0] }] })),
  )
  .await;
  assert_eq!(item_uris(&id).await.len(), 2, "a mismatch removes nothing");

  send(
    Method::DELETE,
    &format!("playlists/{id}/items"),
    &[],
    Some(serde_json::json!({ "items": [{ "uri": first, "positions": [0] }] })),
  )
  .await;
  assert_eq!(item_uris(&id).await, vec![second]);

  assert!(unlist_live_playlists().await >= 1);
  let playlists: Page<SimplifiedPlaylist> = typed("me/playlists", &q(&[("limit", "1000")])).await;
  assert!(playlists.items.iter().all(|p| p.name != LIVE_PLAYLIST));
}

/// Cleanup after a manual MCP check that made [`LIVE_PLAYLIST`] playlists.
#[tokio::test]
#[ignore = "changes the account: unlists the live-test playlists"]
async fn live_test_playlists_are_unlisted() {
  println!("unlisted: {}", unlist_live_playlists().await);
}

#[tokio::test]
#[ignore = "contacts Spotify with the cached login"]
async fn player_reads_parse_into_the_app_types() {
  let devices: DevicePayload = typed("me/player/devices", &[]).await;
  println!("devices: {}", devices.devices.len());
  let mut state = call(Method::GET, "me/player", &[]).await;
  normalize_spotify_payload(&mut state);
  let _: Option<CurrentPlaybackContext> = serde_json::from_value(state).unwrap();
  let _: CurrentUserQueue = typed("me/player/queue", &[]).await;
}

#[tokio::test]
#[ignore = "contacts Spotify with the cached login"]
async fn lyrics_load_for_a_spotify_track() {
  let value = call(Method::GET, &format!("tracks/{ONE_MORE_TIME}/lyrics"), &[]).await;
  let lyrics = super::lyrics::parse_lyrics(value).expect("Spotify has lyrics for One More Time");
  println!("synced: {}, lines: {}", lyrics.synced, lyrics.lines.len());
  assert!(!lyrics.lines.is_empty());
}
