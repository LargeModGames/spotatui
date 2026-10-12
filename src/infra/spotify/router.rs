//! Answers Web API requests from the internal API, in the Web API's JSON shape,
//! so the handlers above the request funnel do not change.

use super::session::{self, SpotifySession};
use super::{catalog, library, listening, lyrics, player, playlists, search};
use crate::infra::network::requests::SpotifyApiError;
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

#[derive(Debug, PartialEq)]
enum Route {
  Me,
  Track(String),
  Tracks,
  TrackLyrics(String),
  Album(String),
  AlbumTracks(String),
  Artist(String),
  Artists,
  ArtistTopTracks(String),
  RelatedArtists(String),
  ArtistAlbums(String),
  Show(String),
  ShowEpisodes(String),
  Recommendations,
  TopTracks,
  TopArtists,
  RecentlyPlayed,
  SavedTracks,
  SavedAlbums,
  SavedShows,
  FollowedArtists,
  LibraryContains,
  LibrarySave,
  LibraryRemove,
  UserPlaylists,
  CreatePlaylist,
  Playlist(String),
  PlaylistItems(String),
  AddPlaylistItems(String),
  RemovePlaylistItems(String),
  Search,
  PlaybackState,
  Devices,
  Queue,
  Play,
  Pause,
  Next,
  Previous,
  Seek,
  Shuffle,
  Repeat,
  Volume,
  Transfer,
  AddToQueue,
}

impl Route {
  fn parse(method: &Method, path: &str) -> Option<Self> {
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let id = |id: &str| id.to_string();
    let route = match (method.as_str(), segments.as_slice()) {
      ("GET", ["me"]) => Self::Me,
      ("GET", ["search"]) => Self::Search,
      ("GET", ["me", "player"]) => Self::PlaybackState,
      ("GET", ["me", "player", "devices"]) => Self::Devices,
      ("GET", ["me", "player", "queue"]) => Self::Queue,
      ("PUT", ["me", "player", "play"]) => Self::Play,
      ("PUT", ["me", "player", "pause"]) => Self::Pause,
      ("POST", ["me", "player", "next"]) => Self::Next,
      ("POST", ["me", "player", "previous"]) => Self::Previous,
      ("PUT", ["me", "player", "seek"]) => Self::Seek,
      ("PUT", ["me", "player", "shuffle"]) => Self::Shuffle,
      ("PUT", ["me", "player", "repeat"]) => Self::Repeat,
      ("PUT", ["me", "player", "volume"]) => Self::Volume,
      ("PUT", ["me", "player"]) => Self::Transfer,
      ("POST", ["me", "player", "queue"]) => Self::AddToQueue,
      ("GET", ["tracks", track]) => Self::Track(id(track)),
      ("GET", ["tracks"]) => Self::Tracks,
      ("GET", ["tracks", track, "lyrics"]) => Self::TrackLyrics(id(track)),
      ("GET", ["albums", album]) => Self::Album(id(album)),
      ("GET", ["albums", album, "tracks"]) => Self::AlbumTracks(id(album)),
      ("GET", ["artists", artist]) => Self::Artist(id(artist)),
      ("GET", ["artists"]) => Self::Artists,
      ("GET", ["artists", artist, "top-tracks"]) => Self::ArtistTopTracks(id(artist)),
      ("GET", ["artists", artist, "related-artists"]) => Self::RelatedArtists(id(artist)),
      ("GET", ["artists", artist, "albums"]) => Self::ArtistAlbums(id(artist)),
      ("GET", ["shows", show]) => Self::Show(id(show)),
      ("GET", ["shows", show, "episodes"]) => Self::ShowEpisodes(id(show)),
      ("GET", ["recommendations"]) => Self::Recommendations,
      ("GET", ["me", "top", "tracks"]) => Self::TopTracks,
      ("GET", ["me", "top", "artists"]) => Self::TopArtists,
      ("GET", ["me", "player", "recently-played"]) => Self::RecentlyPlayed,
      ("GET", ["me", "tracks"]) => Self::SavedTracks,
      ("GET", ["me", "albums"]) => Self::SavedAlbums,
      ("GET", ["me", "shows"]) => Self::SavedShows,
      ("GET", ["me", "following"]) => Self::FollowedArtists,
      ("GET", ["me", "library", "contains"]) => Self::LibraryContains,
      ("PUT", ["me", "library"]) => Self::LibrarySave,
      ("DELETE", ["me", "library"]) => Self::LibraryRemove,
      ("GET", ["me", "playlists"]) => Self::UserPlaylists,
      ("POST", ["me", "playlists"]) => Self::CreatePlaylist,
      ("GET", ["playlists", playlist]) => Self::Playlist(id(playlist)),
      ("GET", ["playlists", playlist, "items" | "tracks"]) => Self::PlaylistItems(id(playlist)),
      ("POST", ["playlists", playlist, "items" | "tracks"]) => Self::AddPlaylistItems(id(playlist)),
      ("DELETE", ["playlists", playlist, "items" | "tracks"]) => {
        Self::RemovePlaylistItems(id(playlist))
      }
      _ => return None,
    };
    Some(route)
  }
}

/// `offset` and `limit` from the query, with the Web API's defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paging {
  pub offset: usize,
  pub limit: usize,
}

impl Paging {
  pub fn from_query(query: &[(&str, String)], default_limit: usize) -> Self {
    let number = |key| query_value(query, key).and_then(|value| value.parse().ok());
    Self {
      offset: number("offset").unwrap_or(0),
      limit: number("limit").unwrap_or(default_limit).max(1),
    }
  }

  pub fn window<'a, T>(&self, items: &'a [T]) -> &'a [T] {
    let start = self.offset.min(items.len());
    let end = start.saturating_add(self.limit).min(items.len());
    &items[start..end]
  }
}

pub fn query_value<'a>(query: &'a [(&str, String)], key: &str) -> Option<&'a str> {
  query
    .iter()
    .find(|(name, _)| *name == key)
    .map(|(_, value)| value.as_str())
}

/// Answers a Web API path from the internal API. An unknown path is a 501.
pub async fn route(
  method: &Method,
  path: &str,
  query: &[(&str, String)],
  body: Option<&Value>,
) -> anyhow::Result<Value> {
  let Some(route) = Route::parse(method, path) else {
    return Err(api_error(
      StatusCode::NOT_IMPLEMENTED,
      format!("No Spotify route for {method} {path}"),
    ));
  };
  let session = session::current().await.map_err(unavailable_error)?;
  log::debug!("Spotify internal route: {method} {path}");
  Box::pin(run(&session, route, query, body)).await
}

fn unavailable_error(reason: session::Unavailable) -> anyhow::Error {
  let status = match reason {
    session::Unavailable::NoLogin => StatusCode::UNAUTHORIZED,
    session::Unavailable::NotPremium => StatusCode::FORBIDDEN,
    session::Unavailable::Offline => StatusCode::SERVICE_UNAVAILABLE,
  };
  api_error(status, reason.message())
}

async fn run(
  session: &SpotifySession,
  route: Route,
  query: &[(&str, String)],
  body: Option<&Value>,
) -> anyhow::Result<Value> {
  let paging = |default_limit| Paging::from_query(query, default_limit);
  let value = |key| query_value(query, key);
  match route {
    Route::Me => me(session).await,
    Route::Track(id) => catalog::track(session, &id).await,
    Route::Tracks => catalog::tracks_by_ids(session, value("ids").unwrap_or_default()).await,
    Route::TrackLyrics(id) => lyrics::track_lyrics(session, &id).await,
    Route::Album(id) => catalog::album(session, &id).await,
    Route::AlbumTracks(id) => catalog::album_tracks(session, &id, paging(20)).await,
    Route::Artist(id) => catalog::artist(session, &id).await,
    Route::Artists => catalog::artists_by_ids(session, value("ids").unwrap_or_default()).await,
    Route::ArtistTopTracks(id) => catalog::artist_top_tracks(session, &id, value("market")).await,
    Route::RelatedArtists(id) => catalog::related_artists(session, &id).await,
    Route::ArtistAlbums(id) => catalog::artist_albums(session, &id, paging(20)).await,
    Route::Show(id) => catalog::show(session, &id).await,
    Route::ShowEpisodes(id) => catalog::show_episodes(session, &id, paging(20)).await,
    Route::Recommendations => catalog::recommendations(session, query).await,
    Route::TopTracks => listening::top_tracks(session, value("time_range"), paging(20).limit).await,
    Route::TopArtists => {
      listening::top_artists(session, value("time_range"), paging(20).limit).await
    }
    Route::RecentlyPlayed => listening::recently_played(session, paging(20).limit).await,
    Route::SavedTracks => library::saved_tracks(session, paging(20)).await,
    Route::SavedAlbums => library::saved_albums(session, paging(20)).await,
    Route::SavedShows => library::saved_shows(session, paging(20)).await,
    Route::FollowedArtists => library::followed_artists(session, query).await,
    Route::LibraryContains => library::contains(session, query).await,
    Route::LibrarySave => library::write(session, query, body, false).await,
    Route::LibraryRemove => library::write(session, query, body, true).await,
    Route::UserPlaylists => playlists::user_playlists(session, query).await,
    Route::CreatePlaylist => playlists::create(session, body).await,
    Route::Playlist(id) => playlists::playlist(session, &id).await,
    Route::PlaylistItems(id) => playlists::playlist_items(session, &id, query).await,
    Route::AddPlaylistItems(id) => playlists::add_items(session, &id, body).await,
    Route::RemovePlaylistItems(id) => playlists::remove_items(session, &id, body).await,
    Route::Search => search::search(session, query).await,
    Route::PlaybackState => player::playback_state(session).await,
    Route::Devices => player::devices(session).await,
    Route::Queue => player::queue(session).await,
    Route::Play => player::play(session, query, body).await,
    Route::Pause => player::simple(session, query, "pause").await,
    Route::Next => player::simple(session, query, "skip_next").await,
    Route::Previous => player::simple(session, query, "skip_prev").await,
    Route::Seek => player::seek(session, query).await,
    Route::Shuffle => player::shuffle(session, query).await,
    Route::Repeat => player::repeat(session, query).await,
    Route::Volume => player::volume(session, query).await,
    Route::Transfer => player::transfer(session, body).await,
    Route::AddToQueue => player::add_to_queue(session, query).await,
  }
}
pub(crate) fn api_error(status: StatusCode, body: impl Into<String>) -> anyhow::Error {
  SpotifyApiError {
    status,
    body: body.into(),
    detail: None,
    endpoint: None,
  }
  .into()
}

/// Runs an idempotent read again after a 5xx answer, twice at most.
pub(crate) async fn retry_read<T, F, Fut>(mut read: F) -> Result<T, librespot_core::Error>
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = Result<T, librespot_core::Error>>,
{
  let mut attempt = 0;
  loop {
    match read().await {
      Err(error) if attempt < 2 && is_server_error(&error) => {
        attempt += 1;
        log::debug!("Spotify read failed ({error}); retry {attempt}");
        tokio::time::sleep(std::time::Duration::from_millis(400 * attempt)).await;
      }
      result => return result,
    }
  }
}

fn is_server_error(error: &librespot_core::Error) -> bool {
  let text = error.to_string();
  ["500 ", "502 ", "503 ", "504 "]
    .iter()
    .any(|code| text.contains(code))
}

pub(crate) fn upstream_error(error: librespot_core::Error) -> anyhow::Error {
  use librespot_core::error::ErrorKind;
  let status = match error.kind {
    ErrorKind::NotFound => StatusCode::NOT_FOUND,
    ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
    ErrorKind::Unauthenticated => StatusCode::UNAUTHORIZED,
    ErrorKind::ResourceExhausted => StatusCode::TOO_MANY_REQUESTS,
    ErrorKind::InvalidArgument | ErrorKind::FailedPrecondition => StatusCode::BAD_REQUEST,
    _ => StatusCode::SERVICE_UNAVAILABLE,
  };
  api_error(status, error.to_string())
}

async fn me(session: &SpotifySession) -> anyhow::Result<Value> {
  let username = session.username();
  let profile = session
    .session()
    .spclient()
    .get_user_profile(&username, Some(0), Some(0))
    .await
    .ok()
    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    .unwrap_or(Value::Null);
  Ok(user_json(&username, &session.country(), &profile))
}

fn user_json(username: &str, country: &str, profile: &Value) -> Value {
  let display_name = profile["name"].as_str().unwrap_or(username);
  let images = profile["image_url"]
    .as_str()
    .map(|url| json!([{ "url": url, "height": null, "width": null }]))
    .unwrap_or_else(|| json!([]));
  json!({
    "id": username,
    "display_name": display_name,
    "country": country,
    "product": "premium",
    "type": "user",
    "uri": format!("spotify:user:{username}"),
    "href": format!("https://api.spotify.com/v1/users/{username}"),
    "external_urls": { "spotify": format!("https://open.spotify.com/user/{username}") },
    "followers": { "href": null, "total": profile["followers_count"].as_u64().unwrap_or(0) },
    "images": images,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use rspotify::prelude::Id;

  #[test]
  fn me_is_routed_only_for_get() {
    assert_eq!(Route::parse(&Method::GET, "me"), Some(Route::Me));
    assert_eq!(Route::parse(&Method::PUT, "me"), None);
    assert_eq!(Route::parse(&Method::GET, "me/unknown"), None);
  }

  #[test]
  fn each_missing_session_reason_has_its_own_status() {
    let status = |reason| {
      unavailable_error(reason)
        .downcast::<SpotifyApiError>()
        .unwrap()
        .status
    };
    assert_eq!(
      status(session::Unavailable::NoLogin),
      StatusCode::UNAUTHORIZED
    );
    assert_eq!(
      status(session::Unavailable::NotPremium),
      StatusCode::FORBIDDEN
    );
    assert_eq!(
      status(session::Unavailable::Offline),
      StatusCode::SERVICE_UNAVAILABLE
    );
  }

  #[tokio::test]
  async fn an_unknown_path_is_not_implemented() {
    let error = route(&Method::GET, "browse/new-releases", &[], None)
      .await
      .unwrap_err()
      .downcast::<SpotifyApiError>()
      .unwrap();
    assert_eq!(error.status, StatusCode::NOT_IMPLEMENTED);
  }

  #[tokio::test]
  async fn a_known_path_without_a_login_is_unauthorized() {
    let error = route(&Method::GET, "me", &[], None)
      .await
      .unwrap_err()
      .downcast::<SpotifyApiError>()
      .unwrap();
    assert_eq!(error.status, StatusCode::UNAUTHORIZED);
  }

  #[test]
  fn catalogue_paths_carry_their_id() {
    assert_eq!(
      Route::parse(&Method::GET, "artists/abc/top-tracks"),
      Some(Route::ArtistTopTracks("abc".into()))
    );
    assert_eq!(
      Route::parse(&Method::GET, "albums/xyz/tracks"),
      Some(Route::AlbumTracks("xyz".into()))
    );
    assert_eq!(Route::parse(&Method::GET, "tracks"), Some(Route::Tracks));
    assert_eq!(
      Route::parse(&Method::GET, "tracks/abc/lyrics"),
      Some(Route::TrackLyrics("abc".into()))
    );
    assert_eq!(
      Route::parse(&Method::GET, "artists/abc"),
      Some(Route::Artist("abc".into()))
    );
    assert_eq!(Route::parse(&Method::GET, "artists"), Some(Route::Artists));
  }

  #[test]
  fn paging_reads_the_query_and_clamps_the_window() {
    let query = [("offset", "2".to_string()), ("limit", "5".to_string())];
    let paging = Paging::from_query(&query, 20);
    assert_eq!(
      paging,
      Paging {
        offset: 2,
        limit: 5
      }
    );
    assert_eq!(paging.window(&[1, 2, 3, 4]), &[3, 4]);
    assert!(Paging {
      offset: 9,
      limit: 5
    }
    .window(&[1, 2])
    .is_empty());
    assert_eq!(Paging::from_query(&[], 50).limit, 50);
  }

  #[test]
  #[allow(deprecated)]
  fn the_user_falls_back_to_the_username_without_a_profile() {
    let user: rspotify::model::PrivateUser =
      serde_json::from_value(user_json("jay", "NL", &Value::Null)).unwrap();
    assert_eq!(user.id.id(), "jay");
    assert_eq!(user.display_name.as_deref(), Some("jay"));
    assert_eq!(user.country, Some(rspotify::model::Country::Netherlands));
  }
}
