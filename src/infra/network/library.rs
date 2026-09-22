use super::mapping::{map_page, playlist_items_page};
use super::requests::{
  is_forbidden_error, spotify_api_request_json_for_with_refresh,
  spotify_get_typed_compat_for_with_refresh,
};
use super::{IoEvent, Network};
use crate::core::app::{
  ActiveBlock, App, PlaylistFolder, PlaylistFolderItem, PlaylistFolderNode, PlaylistFolderNodeType,
  RouteId,
};
use crate::core::pagination::Paged;
use crate::core::plugin_api::{PlayableInfo, PlaylistInfo, ShowInfo, TrackInfo};
use crate::core::sort::Sorter;
use crate::core::source::Source;
use anyhow::anyhow;
use reqwest::Method;
use rspotify::model::{
  idtypes::{AlbumId, LibraryId, PlaylistId, ShowId, TrackId, UserId},
  page::Page,
  playlist::{PlaylistItem, SimplifiedPlaylist},
  track::SavedTrack,
};
use rspotify::{prelude::*, AuthCodePkceSpotify};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[cfg(feature = "streaming")]
use crate::infra::player::StreamingPlayer;
#[cfg(feature = "streaming")]
use librespot_core::SpotifyUri;
#[cfg(feature = "streaming")]
use librespot_metadata::{
  Episode as LibrespotEpisode, Metadata, Playlist as LibrespotPlaylist, Track as LibrespotTrack,
};

// Spotify's `me/library` endpoints (contains, save, remove) accept at most 40
// uris per request; anything larger fails with a 400 "Too many uris".
const LIBRARY_CONTAINS_MAX_URIS: usize = 40;

const EXTERNAL_PLAYLIST_UNAVAILABLE_STATUS: &str = concat!(
  "Spotify Development Mode blocks playlist contents owned by another user. ",
  "Only playlists you own or collaborate on are available."
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaylistAccess {
  Owned,
  Collaborative,
  External,
  Unknown,
}

#[derive(Debug, serde::Deserialize)]
struct PlaylistAccessMetadata {
  collaborative: bool,
  owner: Option<PlaylistOwnerMetadata>,
}

#[derive(Debug, serde::Deserialize)]
struct PlaylistOwnerMetadata {
  id: String,
}

/// The Web API page is still the primary representation. The librespot page is
/// source-agnostic domain data because its internal metadata API does not
/// expose rspotify's `PlaylistItem` model.
enum PlaylistTracksPage {
  Api(Page<PlaylistItem>),
  #[cfg(feature = "streaming")]
  Librespot(Paged<(u32, PlayableInfo)>),
}

impl PlaylistTracksPage {
  fn into_domain(self) -> Paged<(u32, PlayableInfo)> {
    match self {
      Self::Api(page) => playlist_items_page(&page),
      #[cfg(feature = "streaming")]
      Self::Librespot(page) => page,
    }
  }
}

#[derive(Debug)]
enum PlaylistPageError {
  /// Spotify's Development Mode restriction applies to this external playlist
  /// and neither the Web API nor the optional native fallback can serve it.
  UnsupportedExternal,
  Request(anyhow::Error),
}

fn playlist_access_from_owner(
  user_id: Option<&str>,
  owner_id: Option<&str>,
  collaborative: bool,
) -> PlaylistAccess {
  if collaborative {
    return PlaylistAccess::Collaborative;
  }

  match (user_id, owner_id) {
    (Some(user_id), Some(owner_id)) if owner_id == user_id => PlaylistAccess::Owned,
    (Some(_), Some(_)) => PlaylistAccess::External,
    _ => PlaylistAccess::Unknown,
  }
}

fn known_playlist_info<'a>(app: &'a App, playlist_id: &str) -> Option<&'a PlaylistInfo> {
  app
    .all_playlists
    .iter()
    .find(|playlist| playlist.id.as_deref() == Some(playlist_id))
    .or_else(|| {
      app
        .search_results
        .playlists
        .as_ref()?
        .items
        .iter()
        .find(|playlist| playlist.id.as_deref() == Some(playlist_id))
    })
}

fn playlist_access(app: &App, playlist_id: &str) -> PlaylistAccess {
  let Some(playlist) = known_playlist_info(app, playlist_id) else {
    return PlaylistAccess::Unknown;
  };

  playlist_access_from_owner(
    app.user.as_ref().map(|user| user.id.as_str()),
    playlist.owner_id.as_deref(),
    playlist.collaborative,
  )
}

fn log_playlist_access(app: &App, playlist_id: &str, access: PlaylistAccess) {
  if let Some(playlist) = known_playlist_info(app, playlist_id) {
    log::debug!(
      "playlist content access: id={} access={access:?} owner_id={:?} user_id={:?} collaborative={} public={:?}",
      playlist_id,
      playlist.owner_id,
      app.user.as_ref().map(|user| user.id.as_str()),
      playlist.collaborative,
      playlist.public,
    );
  } else {
    log::debug!(
      "playlist content access: id={} access={access:?} metadata=unknown",
      playlist_id,
    );
  }
}

async fn classify_playlist_after_forbidden(
  spotify: &AuthCodePkceSpotify,
  app: &Arc<Mutex<App>>,
  token_cache_path: &Path,
  playlist_id: &PlaylistId<'_>,
) -> PlaylistAccess {
  let known_access = {
    let app_guard = app.lock().await;
    let access = playlist_access(&app_guard, playlist_id.id());
    log_playlist_access(&app_guard, playlist_id.id(), access);
    access
  };
  if known_access != PlaylistAccess::Unknown {
    return known_access;
  }

  // Search results and pasted playlist URIs are not necessarily in the user's
  // library. The metadata endpoint remains useful for relationship checks even
  // when the items endpoint is restricted, and the minimal shape avoids asking
  // rspotify to deserialize a response with no item page.
  let metadata = match spotify_get_typed_compat_for_with_refresh::<PlaylistAccessMetadata>(
    spotify,
    &format!("playlists/{}", playlist_id.id()),
    &[],
    token_cache_path,
    app,
  )
  .await
  {
    Ok(metadata) => metadata,
    Err(error) => {
      log::debug!(
        "playlist access metadata unavailable for {}: {}",
        playlist_id.id(),
        error
      );
      return PlaylistAccess::Unknown;
    }
  };

  let access = {
    let app_guard = app.lock().await;
    playlist_access_from_owner(
      app_guard.user.as_ref().map(|user| user.id.as_str()),
      metadata.owner.as_ref().map(|owner| owner.id.as_str()),
      metadata.collaborative,
    )
  };
  log::debug!(
    "playlist content access from metadata: id={} access={access:?} owner_id={:?}",
    playlist_id.id(),
    metadata.owner.as_ref().map(|owner| owner.id.as_str()),
  );
  access
}

/// Fetch playlist items through the Development Mode Web API first. A 403 is
/// special only when the playlist metadata proves that it is external to the
/// current user and non-collaborative; all other errors remain real errors.
async fn fetch_playlist_tracks_page(
  spotify: &AuthCodePkceSpotify,
  app: &Arc<Mutex<App>>,
  token_cache_path: &Path,
  playlist_id: &PlaylistId<'_>,
  offset: u32,
  limit: u32,
) -> Result<PlaylistTracksPage, PlaylistPageError> {
  let path = format!("playlists/{}/items", playlist_id.id());
  let query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
  match spotify_get_typed_compat_for_with_refresh::<Page<PlaylistItem>>(
    spotify,
    &path,
    &query,
    token_cache_path,
    app,
  )
  .await
  {
    Ok(page) => Ok(PlaylistTracksPage::Api(page)),
    Err(error) => {
      let forbidden = is_forbidden_error(&error);
      let access = if forbidden {
        classify_playlist_after_forbidden(spotify, app, token_cache_path, playlist_id).await
      } else {
        PlaylistAccess::Unknown
      };

      if access == PlaylistAccess::External && forbidden {
        #[cfg(feature = "streaming")]
        match fetch_librespot_playlist_tracks_page(app, playlist_id, offset, limit).await {
          Ok(page) => return Ok(PlaylistTracksPage::Librespot(page)),
          Err(fallback_error) => {
            log::warn!(
              "librespot playlist fallback failed for {}: {}",
              playlist_id.id(),
              fallback_error
            );
          }
        }
        return Err(PlaylistPageError::UnsupportedExternal);
      }

      Err(PlaylistPageError::Request(error))
    }
  }
}

#[cfg(feature = "streaming")]
async fn fetch_librespot_playlist_tracks_page(
  app: &Arc<Mutex<App>>,
  playlist_id: &PlaylistId<'_>,
  offset: u32,
  limit: u32,
) -> anyhow::Result<Paged<(u32, PlayableInfo)>> {
  let player = {
    let app_guard = app.lock().await;
    app_guard.streaming_player.clone()
  }
  .ok_or_else(|| anyhow!("native streaming session is unavailable"))?;
  let session = player.session();
  let id = librespot_core::SpotifyId::from_base62(playlist_id.id())
    .map_err(|error| anyhow!("invalid playlist id for librespot: {error}"))?;
  let uri = SpotifyUri::Playlist { user: None, id };
  let playlist = LibrespotPlaylist::get(&session, &uri)
    .await
    .map_err(|error| anyhow!("librespot playlist request failed: {error}"))?;

  if playlist.contents.is_truncated {
    return Err(anyhow!("librespot returned truncated playlist contents"));
  }

  let limit = limit.max(1);
  let total = playlist.length.max(0) as u32;
  let end = offset.saturating_add(limit);
  let content_offset = playlist.contents.position.max(0) as u32;
  let selected = playlist
    .contents
    .items
    .iter()
    .enumerate()
    .map(|(index, item)| (content_offset + index as u32, item.id.clone()))
    .filter(|(position, _)| *position >= offset && *position < end)
    .collect::<Vec<_>>();

  let resolved = futures::future::join_all(selected.into_iter().map(|(position, uri)| {
    let session = session.clone();
    async move {
      let item = match &uri {
        SpotifyUri::Track { .. } => match LibrespotTrack::get(&session, &uri).await {
          Ok(track) => librespot_track_info(&track).map(PlayableInfo::Track),
          Err(error) => {
            log::debug!("librespot track metadata failed for {}: {}", uri, error);
            None
          }
        },
        SpotifyUri::Episode { .. } => match LibrespotEpisode::get(&session, &uri).await {
          Ok(episode) => librespot_episode_info(&episode).map(PlayableInfo::Episode),
          Err(error) => {
            log::debug!("librespot episode metadata failed for {}: {}", uri, error);
            None
          }
        },
        _ => {
          log::debug!("librespot playlist item is not playable: {}", uri);
          None
        }
      };
      (position, item)
    }
  }))
  .await;

  Ok(Paged {
    items: resolved
      .into_iter()
      .filter_map(|(position, item)| item.map(|item| (position, item)))
      .collect(),
    offset,
    limit,
    total,
    next: (end < total).then(|| "librespot:playlist:next".to_string()),
    previous: None,
  })
}

#[cfg(feature = "streaming")]
fn librespot_track_info(track: &LibrespotTrack) -> Option<TrackInfo> {
  let id = track.id.to_id().ok()?;
  let uri = track.id.to_uri().ok();
  let album_id = track.album.id.to_id().ok();
  let artist_refs = track
    .artists
    .iter()
    .map(|artist| crate::core::plugin_api::ArtistRef {
      id: artist.id.to_id().ok(),
      name: artist.name.clone(),
    })
    .collect::<Vec<_>>();

  Some(TrackInfo {
    uri,
    name: track.name.clone(),
    artists: artist_refs
      .iter()
      .map(|artist| artist.name.clone())
      .collect(),
    album: track.album.name.clone(),
    duration_ms: track.duration.max(0) as u64,
    id: Some(id),
    album_id,
    artist_refs,
    is_playable: track.restrictions.is_empty(),
    is_local: false,
    track_number: track.number.max(0) as u32,
    explicit: track.is_explicit,
    image_url: None,
  })
}

#[cfg(feature = "streaming")]
fn librespot_episode_info(
  episode: &LibrespotEpisode,
) -> Option<crate::core::plugin_api::EpisodeInfo> {
  Some(crate::core::plugin_api::EpisodeInfo {
    id: episode.id.to_id().ok(),
    uri: episode.id.to_uri().ok(),
    name: episode.name.clone(),
    duration_ms: episode.duration.max(0) as u64,
    show_name: episode.show_name.clone(),
    description: episode.description.clone(),
    release_date: String::new(),
    is_playable: episode.restrictions.is_empty(),
    resume_point: None,
    image_url: None,
  })
}

#[cfg(test)]
fn next_saved_tracks_offset(page: &Page<SavedTrack>) -> Option<u32> {
  page.next.as_ref().map(|_| page.offset + page.limit)
}

fn uri_batches(uris: &[String]) -> impl Iterator<Item = &[String]> {
  uris.chunks(LIBRARY_CONTAINS_MAX_URIS)
}

fn populate_liked_song_ids_from_saved_tracks(
  liked_song_ids_set: &mut std::collections::HashSet<String>,
  page: &Page<SavedTrack>,
) {
  for item in &page.items {
    if let Some(track_id) = &item.track.id {
      liked_song_ids_set.insert(track_id.id().to_string());
    }
  }
}

fn playlist_track_search_terms(query: &str) -> Vec<String> {
  query
    .split_whitespace()
    .map(|term| term.to_lowercase())
    .filter(|term| !term.is_empty())
    .collect()
}

#[cfg(test)]
fn playlist_track_search_haystack(track: &rspotify::model::track::FullTrack) -> String {
  let mut haystack = format!("{} {}", track.name, track.album.name);
  for artist in &track.artists {
    haystack.push(' ');
    haystack.push_str(&artist.name);
  }
  haystack.to_lowercase()
}

#[cfg(test)]
fn playlist_track_matches_terms(
  track: &rspotify::model::track::FullTrack,
  terms: &[String],
) -> bool {
  let haystack = playlist_track_search_haystack(track);
  terms.iter().all(|term| haystack.contains(term))
}

fn playlist_track_info_matches_terms(track: &TrackInfo, terms: &[String]) -> bool {
  let haystack =
    format!("{} {} {}", track.name, track.album, track.artists.join(" "),).to_lowercase();
  terms.iter().all(|term| haystack.contains(term))
}

pub async fn prefetch_saved_tracks_page_task(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  token_cache_path: std::path::PathBuf,
  limit: u32,
  mut offset: u32,
  generation: u64,
) {
  loop {
    let should_fetch = {
      let mut app = app.lock().await;
      app.saved_tracks_prefetch_generation == generation
        && app
          .library
          .saved_tracks
          .page_index_for_offset(offset)
          .is_none()
        && app.saved_tracks_prefetch_in_flight.insert(offset)
    };

    if !should_fetch {
      return;
    }

    let query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
    let Ok(page) = spotify_get_typed_compat_for_with_refresh::<Page<rspotify::model::SavedTrack>>(
      &spotify,
      "me/tracks",
      &query,
      &token_cache_path,
      &app,
    )
    .await
    else {
      let mut app_guard = app.lock().await;
      app_guard.saved_tracks_prefetch_in_flight.remove(&offset);
      return;
    };

    if page.items.is_empty() {
      let mut app_guard = app.lock().await;
      app_guard.saved_tracks_prefetch_in_flight.remove(&offset);
      return;
    }

    let next_offset = page.next.as_ref().map(|_| page.offset + page.limit);
    let mut app_guard = app.lock().await;
    app_guard.saved_tracks_prefetch_in_flight.remove(&offset);
    if app_guard.saved_tracks_prefetch_generation != generation {
      return;
    }

    populate_liked_song_ids_from_saved_tracks(&mut app_guard.liked_song_ids_set, &page);
    let domain_page =
      crate::infra::network::mapping::map_page(&page, |st| TrackInfo::from(&st.track));
    app_guard
      .library
      .saved_tracks
      .upsert_page_by_offset(domain_page);
    app_guard.set_saved_tracks_to_table_continuous();
    let Some(candidate_next_offset) = next_offset else {
      return;
    };
    let should_prefetch_next = app_guard
      .library
      .saved_tracks
      .page_index_for_offset(candidate_next_offset)
      .is_none()
      && !app_guard
        .saved_tracks_prefetch_in_flight
        .contains(&candidate_next_offset);
    drop(app_guard);

    if should_prefetch_next {
      offset = candidate_next_offset;
    } else {
      return;
    }
  }
}

pub async fn prefetch_playlist_tracks_page_task(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  token_cache_path: std::path::PathBuf,
  limit: u32,
  playlist_id: PlaylistId<'static>,
  mut offset: u32,
  generation: u64,
) {
  loop {
    let should_fetch = {
      let mut app = app.lock().await;
      app.playlist_tracks_prefetch_generation == generation
        && app.is_playlist_track_table_active_for(&playlist_id)
        && app
          .playlist_track_pages
          .page_index_for_offset(offset)
          .is_none()
        && app.playlist_tracks_prefetch_in_flight.insert(offset)
    };

    if !should_fetch {
      return;
    }

    let page = match fetch_playlist_tracks_page(
      &spotify,
      &app,
      &token_cache_path,
      &playlist_id,
      offset,
      limit,
    )
    .await
    {
      Ok(page) => page.into_domain(),
      Err(PlaylistPageError::UnsupportedExternal) => {
        let mut app_guard = app.lock().await;
        app_guard.playlist_tracks_prefetch_in_flight.remove(&offset);
        app_guard.set_status_message(EXTERNAL_PLAYLIST_UNAVAILABLE_STATUS, 8);
        return;
      }
      Err(PlaylistPageError::Request(error)) => {
        let mut app_guard = app.lock().await;
        app_guard.playlist_tracks_prefetch_in_flight.remove(&offset);
        app_guard.handle_error(error);
        return;
      }
    };

    if page.items.is_empty() {
      let mut app_guard = app.lock().await;
      app_guard.playlist_tracks_prefetch_in_flight.remove(&offset);
      return;
    }

    let next_offset = page.next.as_ref().map(|_| page.offset + page.limit);
    let mut app_guard = app.lock().await;
    app_guard.playlist_tracks_prefetch_in_flight.remove(&offset);
    if app_guard.playlist_tracks_prefetch_generation != generation
      || !app_guard.is_playlist_track_table_active_for(&playlist_id)
    {
      return;
    }

    app_guard.playlist_track_pages.upsert_page_by_offset(page);
    app_guard.set_playlist_tracks_to_table_continuous();
    let Some(candidate_next_offset) = next_offset else {
      return;
    };
    let should_prefetch_next = app_guard
      .playlist_track_pages
      .page_index_for_offset(candidate_next_offset)
      .is_none()
      && !app_guard
        .playlist_tracks_prefetch_in_flight
        .contains(&candidate_next_offset);
    drop(app_guard);

    if should_prefetch_next {
      offset = candidate_next_offset;
    } else {
      return;
    }
  }
}

pub trait LibraryNetwork {
  async fn get_current_user_playlists(&mut self);
  async fn get_playlist_tracks(&mut self, playlist_id: PlaylistId<'static>, playlist_offset: u32);
  async fn search_playlist_tracks(&mut self, playlist_id: PlaylistId<'static>, query: String);
  async fn get_current_user_saved_tracks(&mut self, offset: Option<u32>);
  async fn get_current_user_saved_albums(&mut self, offset: Option<u32>);
  async fn current_user_saved_albums_contains(&mut self, album_ids: Vec<AlbumId<'static>>);
  async fn current_user_saved_album_delete(&mut self, album_id: AlbumId<'static>);
  async fn current_user_saved_album_add(&mut self, album_id: AlbumId<'static>);
  async fn current_user_saved_shows_contains(&mut self, show_ids: Vec<ShowId<'static>>);
  async fn current_user_saved_shows_delete(&mut self, show_id: ShowId<'static>);
  async fn current_user_saved_shows_add(&mut self, show_id: ShowId<'static>);
  async fn get_current_user_saved_shows(&mut self, offset: Option<u32>);
  async fn user_follow_playlist(
    &mut self,
    playlist_owner_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
    is_public: Option<bool>,
  );
  async fn user_unfollow_playlist(
    &mut self,
    user_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
  );
  async fn add_track_to_playlist(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
  );
  async fn remove_track_from_playlist_at_position(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
    position: usize,
  );
  async fn toggle_save_track(&mut self, track_id: rspotify::model::idtypes::PlayableId<'static>);
  async fn current_user_saved_tracks_contains(&mut self, ids: Vec<TrackId<'static>>);
  async fn fetch_all_playlist_tracks_and_sort(&mut self, playlist_id: PlaylistId<'static>);
  async fn create_new_playlist(&mut self, name: String, track_ids: Vec<TrackId<'static>>);
}

// Private helper methods
impl Network {
  pub(crate) async fn library_contains_uris(&self, uris: &[String]) -> anyhow::Result<Vec<bool>> {
    if uris.is_empty() {
      return Ok(Vec::new());
    }

    let mut all_results = Vec::with_capacity(uris.len());
    for batch in uri_batches(uris) {
      let batch_results = spotify_get_typed_compat_for_with_refresh::<Vec<bool>>(
        self.spotify(),
        "me/library/contains",
        &[("uris", batch.join(","))],
        &self.token_cache_path,
        &self.app,
      )
      .await?;
      all_results.extend(batch_results);
    }

    Ok(all_results)
  }

  /// Resolve liked state for bare track ids inline and merge it: the CLI's
  /// synchronous path. The `IoEvent` handler defers to the detached worker
  /// instead (it must not block the TUI's serial pump), which the CLI cannot
  /// await for a result. Failures propagate so the CLI reports them instead
  /// of acting on stale membership.
  pub(crate) async fn resolve_liked_state_now(&mut self, ids: &[String]) -> anyhow::Result<()> {
    let uris: Vec<String> = ids.iter().map(|id| format!("spotify:track:{id}")).collect();
    let is_saved_vec = self.library_contains_uris(&uris).await?;
    let mut app = self.app.lock().await;
    for (i, id) in ids.iter().enumerate() {
      match is_saved_vec.get(i) {
        Some(true) => {
          app.liked_song_ids_set.insert(id.clone());
        }
        Some(false) => {
          app.liked_song_ids_set.remove(id);
        }
        None => {}
      }
    }
    Ok(())
  }

  pub(super) async fn library_save_uris(&self, uris: &[String]) -> anyhow::Result<()> {
    for batch in uri_batches(uris) {
      let query = vec![("uris", batch.join(","))];
      spotify_api_request_json_for_with_refresh(
        self.spotify(),
        Method::PUT,
        "me/library",
        &query,
        Some(json!({ "uris": batch })),
        &self.token_cache_path,
        &self.app,
      )
      .await?;
    }
    Ok(())
  }

  pub(super) async fn library_remove_uris(&self, uris: &[String]) -> anyhow::Result<()> {
    for batch in uri_batches(uris) {
      let query = vec![("uris", batch.join(","))];
      spotify_api_request_json_for_with_refresh(
        self.spotify(),
        Method::DELETE,
        "me/library",
        &query,
        Some(json!({ "uris": batch })),
        &self.token_cache_path,
        &self.app,
      )
      .await?;
    }
    Ok(())
  }

  pub fn spawn_saved_tracks_prefetch(&self, offset: u32, generation: u64) {
    let spotify = self.spotify().clone();
    let app = self.app.clone();
    let token_cache_path = self.token_cache_path.clone();
    let large_search_limit = self.large_search_limit;
    tokio::spawn(async move {
      prefetch_saved_tracks_page_task(
        spotify,
        app,
        token_cache_path,
        large_search_limit,
        offset,
        generation,
      )
      .await;
    });
  }

  pub fn spawn_playlist_tracks_prefetch(
    &self,
    playlist_id: PlaylistId<'static>,
    offset: u32,
    generation: u64,
  ) {
    let spotify = self.spotify().clone();
    let app = self.app.clone();
    let token_cache_path = self.token_cache_path.clone();
    let large_search_limit = self.large_search_limit;
    tokio::spawn(async move {
      prefetch_playlist_tracks_page_task(
        spotify,
        app,
        token_cache_path,
        large_search_limit,
        playlist_id,
        offset,
        generation,
      )
      .await;
    });
  }
}

/// Detached body of `fetch_all_playlist_tracks_and_sort`. On a page failure
/// the error routes through `App::handle_error` exactly as the inline version
/// did.
/// Single detached worker draining [`App::liked_lookup_pending`] in 40-uri
/// batches, merging each batch into `liked_song_ids_set` as it lands. Runs off
/// the IoEvent pump: a big playlist's liked-state sweep is 8-15 batches at
/// 1-2s each, which used to head-of-line-block playback controls (a
/// skip-to-queued-track waited ~20s behind it, #386). One worker at a time
/// (`liked_lookup_worker_running`), so rapid navigation coalesces into the
/// deduped pending set instead of parallel request floods.
async fn liked_lookup_worker_task(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  token_cache_path: std::path::PathBuf,
) {
  const BATCH: usize = 40;
  const MAX_CONSECUTIVE_FAILURES: u32 = 3;
  let mut consecutive_failures = 0u32;
  loop {
    let (batch, epoch) = {
      let mut guard = app.lock().await;
      if guard.liked_lookup_pending.is_empty() {
        guard.liked_lookup_worker_running = false;
        return;
      }
      let batch: Vec<String> = guard
        .liked_lookup_pending
        .iter()
        .take(BATCH)
        .cloned()
        .collect();
      for id in &batch {
        guard.liked_lookup_pending.remove(id);
      }
      (batch, guard.liked_state_epoch)
    };
    let uris: Vec<String> = batch
      .iter()
      .map(|id| format!("spotify:track:{id}"))
      .collect();
    let result = spotify_get_typed_compat_for_with_refresh::<Vec<bool>>(
      &spotify,
      "me/library/contains",
      &[("uris", uris.join(","))],
      &token_cache_path,
      &app,
    )
    .await;
    let mut guard = app.lock().await;
    match result {
      Ok(is_saved_vec) => {
        consecutive_failures = 0;
        if guard.liked_state_epoch != epoch {
          // A local like/unlike landed while this read was in flight; the
          // response may predate it, so re-read instead of applying it.
          guard.liked_lookup_pending.extend(batch);
          continue;
        }
        for (i, id) in batch.into_iter().enumerate() {
          match is_saved_vec.get(i) {
            Some(true) => {
              guard.liked_song_ids_set.insert(id);
            }
            Some(false) => {
              guard.liked_song_ids_set.remove(&id);
            }
            None => {}
          }
        }
      }
      Err(_) if consecutive_failures + 1 < MAX_CONSECUTIVE_FAILURES => {
        // Transient failure: back off and retry from THIS worker, so ids
        // enqueued while the request was in flight (which saw the running
        // flag and did not spawn) are not stranded without a worker.
        guard.liked_lookup_pending.extend(batch);
        consecutive_failures += 1;
        drop(guard);
        tokio::time::sleep(std::time::Duration::from_secs(u64::from(
          2 * consecutive_failures,
        )))
        .await;
      }
      Err(e) => {
        // Persistently failing: stop rather than hammer the endpoint. The ids
        // stay pending and the next dispatch restarts the worker.
        guard.liked_lookup_pending.extend(batch);
        guard.liked_lookup_worker_running = false;
        guard.set_status_message(format!("Could not check liked track state: {e}"), 5);
        return;
      }
    }
  }
}

async fn fetch_all_playlist_tracks_and_sort_task(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  token_cache_path: std::path::PathBuf,
  playlist_id: PlaylistId<'static>,
) {
  let playlist_id_string = playlist_id.id().to_string();
  let mut all_tracks: Vec<TrackInfo> = Vec::new();
  let mut offset = 0u32;
  let limit = 50u32;

  loop {
    {
      let mut app = app.lock().await;
      if !app.is_playlist_track_table_active_for(&playlist_id) {
        app
          .playlist_sort_fetch_in_flight
          .remove(&playlist_id_string);
        return;
      }
    }

    let page = match fetch_playlist_tracks_page(
      &spotify,
      &app,
      &token_cache_path,
      &playlist_id,
      offset,
      limit,
    )
    .await
    {
      Ok(page) => page.into_domain(),
      Err(PlaylistPageError::UnsupportedExternal) => {
        let mut app = app.lock().await;
        app
          .playlist_sort_fetch_in_flight
          .remove(&playlist_id_string);
        app.set_status_message(EXTERNAL_PLAYLIST_UNAVAILABLE_STATUS, 8);
        return;
      }
      Err(PlaylistPageError::Request(error)) => {
        let mut app = app.lock().await;
        app
          .playlist_sort_fetch_in_flight
          .remove(&playlist_id_string);
        app.handle_error(error);
        return;
      }
    };

    if page.items.is_empty() {
      break;
    }

    for (_, item) in page.items {
      if let PlayableInfo::Track(track) = item {
        all_tracks.push(track);
      }
    }

    if page.next.is_none() {
      break;
    }
    offset = page.offset.saturating_add(page.limit);
  }

  // Apply sort if any
  let mut app = app.lock().await;
  app
    .playlist_sort_fetch_in_flight
    .remove(&playlist_id_string);
  Sorter::new(app.playlist_sort).sort_track_infos(&mut all_tracks);
  let _ = app.apply_sorted_playlist_track_infos_if_current(&playlist_id, all_tracks);
}

/// Background half of `get_current_user_playlists`: fetch the remaining pages
/// and the librespot rootlist folder structure, then publish the complete list
/// (unless a newer refresh superseded this one). A page failure here keeps the
/// already-published pages instead of error-routing the UI.
#[allow(clippy::too_many_arguments)]
async fn finish_playlists_fetch(
  spotify: AuthCodePkceSpotify,
  app: Arc<Mutex<App>>,
  token_cache_path: std::path::PathBuf,
  mut all_playlists: Vec<PlaylistInfo>,
  has_more: bool,
  limit: u32,
  generation: u64,
  original_selection: (Option<String>, usize, Option<usize>),
  page_one_selection: (Option<String>, usize, Option<usize>),
  previous_complete: (
    Vec<PlaylistInfo>,
    Option<Vec<PlaylistFolderNode>>,
    Vec<PlaylistFolderItem>,
  ),
) {
  let mut pagination_error = None;
  if has_more {
    let mut offset = limit;
    loop {
      {
        let app = app.lock().await;
        if app.playlist_refresh_generation != generation {
          return;
        }
      }
      let mut attempts = 0u8;
      let page = loop {
        match spotify_get_typed_compat_for_with_refresh::<Page<SimplifiedPlaylist>>(
          &spotify,
          "me/playlists",
          &[("limit", limit.to_string()), ("offset", offset.to_string())],
          &token_cache_path,
          &app,
        )
        .await
        {
          Ok(page) => break Some(page),
          Err(e) if attempts < 2 => {
            attempts += 1;
            log::warn!("playlist page fetch failed at offset {offset}; retry {attempts}/2: {e}");
            tokio::time::sleep(Duration::from_millis(250 * u64::from(attempts))).await;
          }
          Err(e) => {
            pagination_error = Some(e.to_string());
            break None;
          }
        }
      };
      let Some(page) = page else { break };
      if page.items.is_empty() {
        break;
      }
      all_playlists.extend(page.items.iter().map(PlaylistInfo::from_simplified));
      if page.next.is_none() {
        break;
      }
      offset += limit;
    }
  }

  if let Some(error) = pagination_error {
    log::warn!("playlist refresh incomplete: {error}");
    let mut app = app.lock().await;
    if app.playlist_refresh_generation != generation {
      return;
    }
    let (previous_playlists, previous_nodes, previous_items) = previous_complete;
    if !previous_playlists.is_empty() {
      app.all_playlists = previous_playlists;
      app._playlist_folder_nodes = previous_nodes;
      app.playlist_folder_items = previous_items;
      reconcile_playlist_selection(
        &mut app,
        original_selection.0.as_deref(),
        original_selection.1,
        original_selection.2,
      );
    }
    app.set_status_message("Playlist refresh incomplete; kept the previous list.", 8);
    return;
  }

  #[cfg(feature = "streaming")]
  let folder_nodes = {
    let streaming_player = {
      let app = app.lock().await;
      app.streaming_player.clone()
    };
    fetch_rootlist_folders(streaming_player).await
  };
  #[cfg(not(feature = "streaming"))]
  let folder_nodes: Option<Vec<PlaylistFolderNode>> = None;

  let folder_items = if let Some(ref nodes) = folder_nodes {
    structurize_playlist_folders(nodes, &all_playlists)
  } else {
    build_flat_playlist_items(&all_playlists)
  };

  let mut app = app.lock().await;
  if app.playlist_refresh_generation != generation {
    return;
  }
  // Restore the original selection only if the user has not navigated since
  // page 1 was published. Otherwise their newer choice wins.
  let current_selection = (
    app.get_selected_playlist_id(),
    app.current_playlist_folder_id,
    app.view.selected_playlist_index,
  );
  let preferred = if current_selection == page_one_selection {
    original_selection
  } else {
    current_selection
  };

  app.all_playlists = all_playlists;
  app
    .plugin_data_generations
    .bump(crate::core::app::PluginDataKind::Playlists);
  app._playlist_folder_nodes = folder_nodes;
  app.playlist_folder_items = folder_items;

  reconcile_playlist_selection(&mut app, preferred.0.as_deref(), preferred.1, preferred.2);

  maybe_prompt_community_pin(&mut app);
}

/// Offer the one-time community-pin hide prompt once the full playlist list is
/// known. A user who already follows the community playlist never sees it (no
/// pin to explain); if they later unfollow, the pin appears for the first time
/// and this can fire then — intended, not a bug.
fn maybe_prompt_community_pin(app: &mut App) {
  if app.active_source == Source::Spotify
    && !app.runtime_state.community_pin_prompt_shown
    && app.user_config.behavior.pin_community_playlist
    && !app.follows_community_playlist()
    && app.get_current_route().id != RouteId::CommunityPinPrompt
  {
    // Mark shown at push time, not only on dismiss, so paths that close the
    // prompt without going through its handler (the back key, mouse-interactive
    // layout dismissal) can never make it reappear.
    app.mark_community_pin_prompt_shown();
    app.push_navigation_stack(RouteId::CommunityPinPrompt, ActiveBlock::CommunityPinPrompt);
  }
}

impl LibraryNetwork for Network {
  async fn get_current_user_playlists(&mut self) {
    let original_selection = {
      let app = self.app.lock().await;
      (
        app.get_selected_playlist_id(),
        app.current_playlist_folder_id,
        app.view.selected_playlist_index,
      )
    };

    let limit = 50u32;

    // Page 1 first: publish it immediately so the sidebar is usable, then
    // fetch the remaining pages and the rootlist folder structure in a
    // background task instead of blocking the pump for the whole pagination
    // (each page pays the pacing floor; 500 playlists used to block startup
    // dispatches for seconds).
    //
    // Always use the compat path: parses raw JSON to serde_json::Value first,
    // which silently deduplicates keys (last-wins). This handles the known Spotify
    // API bug where "items" appears twice in the same JSON object.
    let first_page = match spotify_get_typed_compat_for_with_refresh::<Page<SimplifiedPlaylist>>(
      self.spotify(),
      "me/playlists",
      &[("limit", limit.to_string()), ("offset", "0".to_string())],
      &self.token_cache_path,
      &self.app,
    )
    .await
    {
      Ok(page) => page,
      Err(e) => {
        let mut app = self.app.lock().await;
        app.pending_playlist_track_search = None;
        drop(app);
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    // Convert to source-agnostic domain types at the network boundary.
    let first_items: Vec<PlaylistInfo> = first_page
      .items
      .iter()
      .map(PlaylistInfo::from_simplified)
      .collect();
    let has_more = first_page.next.is_some() && !first_page.items.is_empty();
    let mapped_first = map_page(&first_page, PlaylistInfo::from_simplified);

    let (generation, page_one_selection, previous_complete) = {
      let mut app = self.app.lock().await;
      let had_previous_complete = !app.all_playlists.is_empty();
      // Snapshot the existing complete list only when background pagination can
      // fail partway (`has_more`) and there is something worth restoring. A
      // single-page refresh or a first-ever load never uses this, so skip the
      // clone in the common case.
      let previous_complete = if has_more && had_previous_complete {
        (
          app.all_playlists.clone(),
          app._playlist_folder_nodes.clone(),
          app.playlist_folder_items.clone(),
        )
      } else {
        (Vec::new(), None, Vec::new())
      };
      app.playlist_refresh_generation = app.playlist_refresh_generation.wrapping_add(1);
      app.playlists = Some(mapped_first);
      let selected_is_in_first_page = original_selection.0.as_ref().is_some_and(|selected| {
        first_items
          .iter()
          .any(|playlist| playlist.id.as_deref() == Some(selected.as_str()))
      });
      if !had_previous_complete || original_selection.0.is_none() || selected_is_in_first_page {
        app.all_playlists = first_items.clone();
        app
          .plugin_data_generations
          .bump(crate::core::app::PluginDataKind::Playlists);
        app.playlist_folder_items = build_flat_playlist_items(&first_items);
        reconcile_playlist_selection(
          &mut app,
          original_selection.0.as_deref(),
          original_selection.1,
          original_selection.2,
        );
      }
      let page_one_selection = (
        app.get_selected_playlist_id(),
        app.current_playlist_folder_id,
        app.view.selected_playlist_index,
      );
      (
        app.playlist_refresh_generation,
        page_one_selection,
        previous_complete,
      )
    };

    let spotify = self.spotify().clone();
    let app = Arc::clone(&self.app);
    let token_cache_path = self.token_cache_path.clone();
    tokio::spawn(async move {
      finish_playlists_fetch(
        spotify,
        app,
        token_cache_path,
        first_items,
        has_more,
        limit,
        generation,
        original_selection,
        page_one_selection,
        previous_complete,
      )
      .await;
    });
  }

  async fn get_playlist_tracks(&mut self, playlist_id: PlaylistId<'static>, playlist_offset: u32) {
    let generation = {
      let app = self.app.lock().await;
      app.playlist_tracks_prefetch_generation
    };

    match fetch_playlist_tracks_page(
      self.spotify(),
      &self.app,
      &self.token_cache_path,
      &playlist_id,
      playlist_offset,
      self.large_search_limit,
    )
    .await
    {
      Ok(playlist_tracks) => {
        let mut app = self.app.lock().await;
        app
          .playlist_tracks_prefetch_in_flight
          .remove(&playlist_offset);
        if app.pending_playlist_open.as_deref() == Some(playlist_id.id()) {
          app.pending_playlist_open = None;
        }
        if app.playlist_tracks_prefetch_generation != generation
          || !app.is_playlist_track_table_active_for(&playlist_id)
        {
          return;
        }

        let playlist_tracks_index = app
          .playlist_track_pages
          .upsert_page_by_offset(playlist_tracks.into_domain());
        app.set_playlist_tracks_to_table_continuous();

        let next_offset = app.next_missing_playlist_tracks_offset(playlist_tracks_index);
        let generation = app.playlist_tracks_prefetch_generation;
        // Navigation happens in the handler when the user opens the playlist
        // (`App::open_playlist_tracks`), not here on response arrival.
        drop(app);

        if let Some(next_offset) = next_offset {
          self.spawn_playlist_tracks_prefetch(playlist_id, next_offset, generation);
        }
      }
      Err(PlaylistPageError::UnsupportedExternal) => {
        let mut app = self.app.lock().await;
        app
          .playlist_tracks_prefetch_in_flight
          .remove(&playlist_offset);
        if app.pending_playlist_open.as_deref() == Some(playlist_id.id()) {
          app.pending_playlist_open = None;
        }
        app.set_status_message(EXTERNAL_PLAYLIST_UNAVAILABLE_STATUS, 8);
      }
      Err(PlaylistPageError::Request(error)) => {
        let mut app = self.app.lock().await;
        app
          .playlist_tracks_prefetch_in_flight
          .remove(&playlist_offset);
        if app.pending_playlist_open.as_deref() == Some(playlist_id.id()) {
          app.pending_playlist_open = None;
        }
        drop(app);
        self.handle_error(error).await;
      }
    }
  }

  async fn search_playlist_tracks(&mut self, playlist_id: PlaylistId<'static>, query: String) {
    let terms = playlist_track_search_terms(&query);
    if terms.is_empty() {
      let mut app = self.app.lock().await;
      app.clear_playlist_track_filter();
      return;
    }

    let limit = self.large_search_limit;
    let mut offset = 0u32;
    let mut matches: Vec<(TrackInfo, usize)> = Vec::new();

    loop {
      let page = match fetch_playlist_tracks_page(
        self.spotify(),
        &self.app,
        &self.token_cache_path,
        &playlist_id,
        offset,
        limit,
      )
      .await
      {
        Ok(page) => page.into_domain(),
        Err(PlaylistPageError::UnsupportedExternal) => {
          let mut app = self.app.lock().await;
          app.pending_playlist_track_search = None;
          app.set_status_message(EXTERNAL_PLAYLIST_UNAVAILABLE_STATUS, 8);
          return;
        }
        Err(PlaylistPageError::Request(error)) => {
          self.handle_error(error).await;
          return;
        }
      };

      if page.items.is_empty() {
        break;
      }

      for (position, item) in page.items {
        if let PlayableInfo::Track(track) = item {
          if playlist_track_info_matches_terms(&track, &terms) {
            matches.push((track, position as usize));
          }
        }
      }

      if page.next.is_none() {
        break;
      }
      offset = page.offset.saturating_add(page.limit);
    }

    let match_count = matches.len();
    let mut app = self.app.lock().await;
    if app.apply_playlist_track_search_info_results(&playlist_id, query.clone(), matches) {
      app.set_status_message(
        format!("{match_count} playlist tracks match \"{query}\""),
        3,
      );
    } else {
      app.pending_playlist_track_search = None;
    }
  }

  async fn get_current_user_saved_tracks(&mut self, offset: Option<u32>) {
    let requested_offset = offset.unwrap_or(0);
    let generation = {
      let app = self.app.lock().await;
      app.saved_tracks_prefetch_generation
    };

    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match spotify_get_typed_compat_for_with_refresh::<Page<rspotify::model::SavedTrack>>(
      self.spotify(),
      "me/tracks",
      &query,
      &self.token_cache_path,
      &self.app,
    )
    .await
    {
      Ok(saved_tracks) => {
        let mut app = self.app.lock().await;
        app
          .saved_tracks_prefetch_in_flight
          .remove(&requested_offset);
        if app.saved_tracks_prefetch_generation != generation {
          return;
        }

        populate_liked_song_ids_from_saved_tracks(&mut app.liked_song_ids_set, &saved_tracks);
        let domain_page =
          crate::infra::network::mapping::map_page(&saved_tracks, |st| TrackInfo::from(&st.track));
        let saved_tracks_index = app.library.saved_tracks.upsert_page_by_offset(domain_page);
        app.set_saved_tracks_to_table_continuous();
        app
          .plugin_data_generations
          .bump(crate::core::app::PluginDataKind::SavedTracks);

        let next_offset = app.next_missing_saved_tracks_offset(saved_tracks_index);
        let generation = app.saved_tracks_prefetch_generation;
        drop(app);

        if let Some(next_offset) = next_offset {
          self.spawn_saved_tracks_prefetch(next_offset, generation);
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app
          .saved_tracks_prefetch_in_flight
          .remove(&requested_offset);
        drop(app);
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_user_saved_albums(&mut self, offset: Option<u32>) {
    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match spotify_get_typed_compat_for_with_refresh::<Page<rspotify::model::SavedAlbum>>(
      self.spotify(),
      "me/albums",
      &query,
      &self.token_cache_path,
      &self.app,
    )
    .await
    {
      Ok(saved_albums) => {
        let mut app = self.app.lock().await;
        if !saved_albums.items.is_empty() {
          let domain_page = crate::infra::network::mapping::map_page(
            &saved_albums,
            crate::infra::network::mapping::saved_album_info,
          );
          app.library.saved_albums.add_pages(domain_page);
        }
        // Bump even on an empty page: completion is the signal plugin data
        // requests wait on, and an empty library never writes a page.
        app
          .plugin_data_generations
          .bump(crate::core::app::PluginDataKind::SavedAlbums);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_albums_contains(&mut self, album_ids: Vec<AlbumId<'static>>) {
    let uris: Vec<String> = album_ids
      .iter()
      .map(|id| format!("spotify:album:{}", id.id()))
      .collect();

    match self.library_contains_uris(&uris).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in album_ids.iter().enumerate() {
          if let Some(is_saved) = is_saved_vec.get(i) {
            if *is_saved {
              app.saved_album_ids_set.insert(id.id().to_string());
            } else if app.saved_album_ids_set.contains(id.id()) {
              app.saved_album_ids_set.remove(id.id());
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_album_delete(&mut self, album_id: AlbumId<'static>) {
    let uris = vec![format!("spotify:album:{}", album_id.id())];
    match self.library_remove_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.remove(album_id.id());
        // Reload saved albums to refresh UI
        // dispatching event would require loop access, but we can't from here easily unless we return IoEvent
        // For now, assume optimistic update is handled or manually remove
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_album_add(&mut self, album_id: AlbumId<'static>) {
    let uris = vec![format!("spotify:album:{}", album_id.id())];
    match self.library_save_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.insert(album_id.id().to_string());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_shows_contains(&mut self, show_ids: Vec<ShowId<'static>>) {
    let uris: Vec<String> = show_ids
      .iter()
      .map(|id| format!("spotify:show:{}", id.id()))
      .collect();
    match self.library_contains_uris(&uris).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in show_ids.iter().enumerate() {
          if let Some(is_saved) = is_saved_vec.get(i) {
            if *is_saved {
              app.saved_show_ids_set.insert(id.id().to_string());
            } else if app.saved_show_ids_set.contains(id.id()) {
              app.saved_show_ids_set.remove(id.id());
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_shows_delete(&mut self, show_id: ShowId<'static>) {
    let uris = vec![format!("spotify:show:{}", show_id.id())];
    match self.library_remove_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.remove(show_id.id());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_shows_add(&mut self, show_id: ShowId<'static>) {
    let uris = vec![format!("spotify:show:{}", show_id.id())];
    match self.library_save_uris(&uris).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.insert(show_id.id().to_string());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn get_current_user_saved_shows(&mut self, offset: Option<u32>) {
    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match spotify_get_typed_compat_for_with_refresh::<Page<rspotify::model::show::Show>>(
      self.spotify(),
      "me/shows",
      &query,
      &self.token_cache_path,
      &self.app,
    )
    .await
    {
      Ok(saved_shows) => {
        let mut app = self.app.lock().await;
        if !saved_shows.items.is_empty() {
          let domain_page =
            crate::infra::network::mapping::map_page(&saved_shows, |s| ShowInfo::from(&s.show));
          app.library.saved_shows.add_pages(domain_page);
        }
        // Bump even on an empty page (see saved-albums note above).
        app
          .plugin_data_generations
          .bump(crate::core::app::PluginDataKind::SavedShows);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_follow_playlist(
    &mut self,
    _playlist_owner_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
    _is_public: Option<bool>,
  ) {
    match self
      .spotify()
      .library_add([LibraryId::Playlist(playlist_id)])
      .await
    {
      Ok(_) => {
        // Optimistic update handled in handler or next refresh
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn user_unfollow_playlist(
    &mut self,
    _user_id: UserId<'static>,
    playlist_id: PlaylistId<'static>,
  ) {
    match self
      .spotify()
      .library_remove([LibraryId::Playlist(playlist_id)])
      .await
    {
      Ok(_) => {
        // Handled
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn add_track_to_playlist(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
  ) {
    match self
      .spotify()
      .playlist_add_items(playlist_id.clone(), vec![PlayableId::Track(track_id)], None)
      .await
    {
      Ok(_) => {
        let status_message = {
          let mut app = self.app.lock().await;
          let playlist_name = app
            .all_playlists
            .iter()
            .find(|playlist| playlist.id.as_deref() == Some(playlist_id.id()))
            .map(|playlist| playlist.name.clone());

          if app.is_current_route_playlist_track_table_for(&playlist_id) {
            let playlist_offset = app.playlist_offset;
            app.dispatch(IoEvent::GetPlaylistItems(
              playlist_id.id().to_string(),
              playlist_offset,
            ));
          }

          playlist_name
            .map(|name| format!("Added to {}", name))
            .unwrap_or_else(|| "Added to playlist".to_string())
        };
        self.show_status_message(status_message, 3).await;
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn remove_track_from_playlist_at_position(
    &mut self,
    playlist_id: PlaylistId<'static>,
    track_id: TrackId<'static>,
    position: usize,
  ) {
    let body = json!({
        "items": [{
            "uri": format!("spotify:track:{}", track_id.id()),
            "positions": [position]
        }]
    });

    match spotify_api_request_json_for_with_refresh(
      self.spotify(),
      Method::DELETE,
      &format!("playlists/{}/items", playlist_id.id()),
      &[],
      Some(body),
      &self.token_cache_path,
      &self.app,
    )
    .await
    {
      Ok(_) => {
        self
          .show_status_message("Removed from playlist".to_string(), 3)
          .await;
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn toggle_save_track(&mut self, track_id: rspotify::model::idtypes::PlayableId<'static>) {
    let id_str = match &track_id {
      PlayableId::Track(id) => id.id(),
      PlayableId::Episode(id) => id.id(),
    };
    let uri = match &track_id {
      PlayableId::Track(id) => format!("spotify:track:{}", id.id()),
      PlayableId::Episode(id) => format!("spotify:episode:{}", id.id()),
    };

    let is_liked = {
      let app = self.app.lock().await;
      app.liked_song_ids_set.contains(id_str)
    };

    if is_liked {
      if let Err(e) = self.library_remove_uris(&[uri]).await {
        self.handle_error(anyhow!(e)).await;
      } else {
        let mut app = self.app.lock().await;
        app.liked_song_ids_set.remove(id_str);
        app.liked_state_epoch = app.liked_state_epoch.wrapping_add(1);
      }
    } else if let Err(e) = self.library_save_uris(&[uri]).await {
      self.handle_error(anyhow!(e)).await;
    } else {
      let mut app = self.app.lock().await;
      app.liked_song_ids_set.insert(id_str.to_string());
      app.liked_state_epoch = app.liked_state_epoch.wrapping_add(1);
    }
  }

  async fn current_user_saved_tracks_contains(&mut self, ids: Vec<TrackId<'static>>) {
    // Enqueue and let the single detached worker resolve: these lookups are
    // UI garnish and must never head-of-line-block playback events on the
    // serial pump (see `liked_lookup_worker_task`).
    let spawn = {
      let mut app = self.app.lock().await;
      for id in &ids {
        app.liked_lookup_pending.insert(id.id().to_string());
      }
      if app.liked_lookup_pending.is_empty() || app.liked_lookup_worker_running {
        false
      } else {
        app.liked_lookup_worker_running = true;
        true
      }
    };
    if !spawn {
      return;
    }
    let spotify = self.spotify().clone();
    let app = Arc::clone(&self.app);
    let token_cache_path = self.token_cache_path.clone();
    tokio::spawn(async move {
      liked_lookup_worker_task(spotify, app, token_cache_path).await;
    });
  }

  async fn fetch_all_playlist_tracks_and_sort(&mut self, playlist_id: PlaylistId<'static>) {
    // The full pagination pays the pacing floor per page (a 3,000-track
    // playlist is 60 pages, 15s+), so it runs detached instead of
    // head-of-line-blocking playback controls and polling on the serial pump.
    // `apply_sorted_playlist_tracks_if_current` already guards a stale result.
    {
      let mut app = self.app.lock().await;
      if !app
        .playlist_sort_fetch_in_flight
        .insert(playlist_id.id().to_string())
      {
        return;
      }
    }
    let spotify = self.spotify().clone();
    let app = Arc::clone(&self.app);
    let token_cache_path = self.token_cache_path.clone();
    tokio::spawn(async move {
      fetch_all_playlist_tracks_and_sort_task(spotify, app, token_cache_path, playlist_id).await;
    });
  }

  async fn create_new_playlist(&mut self, name: String, track_ids: Vec<TrackId<'static>>) {
    // Use raw API call to avoid rspotify deserializing FullPlaylist, which crashes when
    // Spotify returns a duplicate "items" key in the response (known API migration bug).
    let create_body = json!({
      "name": name,
      "public": false,
      "collaborative": false,
      "description": "Created with spotatui"
    });
    let playlist_value = match spotify_api_request_json_for_with_refresh(
      self.spotify(),
      Method::POST,
      "me/playlists",
      &[],
      Some(create_body),
      &self.token_cache_path,
      &self.app,
    )
    .await
    {
      Ok(v) => v,
      Err(e) => {
        self.handle_error(e).await;
        return;
      }
    };

    let playlist_id_str = match playlist_value.get("id").and_then(|v| v.as_str()) {
      Some(id) => id.to_string(),
      None => {
        self
          .show_status_message("Playlist created but could not get its ID".to_string(), 4)
          .await;
        return;
      }
    };

    let playlist_id = match PlaylistId::from_id(playlist_id_str) {
      Ok(id) => id.into_static(),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    if !track_ids.is_empty() {
      let items: Vec<rspotify::model::idtypes::PlayableId> = track_ids
        .iter()
        .map(|id| rspotify::model::idtypes::PlayableId::Track(id.clone()))
        .collect();
      if let Err(e) = self
        .spotify()
        .playlist_add_items(playlist_id, items, None)
        .await
      {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    }

    // Refresh playlists
    {
      let mut app = self.app.lock().await;
      app.dispatch(IoEvent::GetPlaylists);
    }

    let status = format!("Playlist \"{}\" created!", name);
    self.show_status_message(status, 4).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{Duration as ChronoDuration, Utc};
  use rspotify::model::{artist::SimplifiedArtist, track::FullTrack};
  use std::collections::{HashMap, HashSet};

  #[test]
  fn playlist_access_keeps_external_403_distinct_from_owned_and_collaborative() {
    use crate::core::test_helpers::{playlist_info, user_info};

    let mut app = App::default();
    app.user = Some(user_info("me"));
    let mut external = playlist_info("external", "External", "other", false);
    external.public = Some(true);
    app.all_playlists = vec![
      playlist_info("owned", "Owned", "me", false),
      playlist_info("collab", "Collaborative", "other", true),
    ];
    app.search_results.playlists = Some(Paged {
      items: vec![external],
      ..Default::default()
    });

    assert_eq!(playlist_access(&app, "owned"), PlaylistAccess::Owned);
    assert_eq!(
      playlist_access(&app, "collab"),
      PlaylistAccess::Collaborative
    );
    assert_eq!(playlist_access(&app, "external"), PlaylistAccess::External);
    assert_eq!(playlist_access(&app, "missing"), PlaylistAccess::Unknown);
  }

  #[allow(deprecated)]
  fn full_track(id: &str) -> FullTrack {
    FullTrack {
      album: rspotify::model::album::SimplifiedAlbum {
        name: "Album".to_string(),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: ChronoDuration::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: Some(TrackId::from_id(id).unwrap().into_static()),
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: format!("Track {id}"),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    }
  }

  fn saved_track(id: &str) -> SavedTrack {
    SavedTrack {
      added_at: Utc::now(),
      track: full_track(id),
    }
  }

  fn saved_tracks_page(offset: u32, limit: u32, has_next: bool) -> Page<SavedTrack> {
    let ids = match offset {
      0 => vec!["0000000000000000000001", "0000000000000000000002"],
      20 => vec!["0000000000000000000003", "0000000000000000000004"],
      40 => vec!["0000000000000000000005", "0000000000000000000006"],
      _ => vec!["0000000000000000000007", "0000000000000000000008"],
    };

    Page {
      href: "https://example.com/me/tracks".to_string(),
      items: ids.into_iter().map(saved_track).collect(),
      limit,
      next: has_next.then(|| "https://example.com/me/tracks?next".to_string()),
      offset,
      previous: None,
      total: 60,
    }
  }

  #[test]
  fn reconcile_restores_selection_against_folders_first_order() {
    use crate::core::app::{App, PlaylistFolder, PlaylistFolderItem};
    use crate::core::test_helpers::playlist_info;

    // Root order: P0, folderA, P1, folderB. Restore selection to P1.
    let mut app = App::default();
    // Keep this test about folder-first ordering; the community pin is covered
    // by its own tests.
    app.user_config.behavior.pin_community_playlist = false;
    app.all_playlists = vec![
      playlist_info("00000000000000000000p0", "P0", "me", false),
      playlist_info("00000000000000000000p1", "P1", "me", false),
    ];
    app.playlist_folder_items = vec![
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 0,
      },
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: "A".to_string(),
        current_id: 0,
        target_id: 1,
      }),
      PlaylistFolderItem::Playlist {
        index: 1,
        current_id: 0,
      },
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: "B".to_string(),
        current_id: 0,
        target_id: 2,
      }),
    ];
    app.user_config.behavior.group_folders_first = true;

    reconcile_playlist_selection(&mut app, Some("00000000000000000000p1"), 0, None);

    // Sorted display view is [A, B, P0, P1]; P1 is display index 3, which maps
    // to sidebar row 4 (row 0 is the leading "+ Add Playlist" entry).
    let idx = app
      .view
      .selected_playlist_index
      .expect("selection restored");
    assert_eq!(idx, 4);
    assert!(matches!(
      app.get_playlist_display_item_at(idx - 1),
      Some(PlaylistFolderItem::Playlist { index: 1, .. })
    ));
  }

  #[test]
  fn community_pin_prompt_marks_shown_at_push_time() {
    let dir = tempfile::tempdir().unwrap();
    // Trigger conditions: Spotify source, flag false, toggle on, not following.
    let mut app = App::default();
    app.state_path = Some(dir.path().join("state.yml"));
    assert!(app.active_source == Source::Spotify);
    assert!(app.user_config.behavior.pin_community_playlist);
    assert!(!app.follows_community_playlist());
    assert!(!app.runtime_state.community_pin_prompt_shown);

    maybe_prompt_community_pin(&mut app);

    // Shown is marked at push time so a non-handler dismissal can't reappear it.
    assert!(app.runtime_state.community_pin_prompt_shown);
    assert_eq!(app.get_current_route().id, RouteId::CommunityPinPrompt);
  }

  #[test]
  fn next_saved_tracks_offset_uses_page_limit() {
    let page = saved_tracks_page(20, 20, true);
    assert_eq!(next_saved_tracks_offset(&page), Some(40));
  }

  #[test]
  fn next_saved_tracks_offset_returns_none_without_next_link() {
    let page = saved_tracks_page(20, 20, false);
    assert_eq!(next_saved_tracks_offset(&page), None);
  }

  #[test]
  fn uri_batches_split_large_contains_requests() {
    let uris = (0..120)
      .map(|index| format!("spotify:track:{index:022}"))
      .collect::<Vec<_>>();
    let batches = uri_batches(&uris)
      .map(|batch| batch.len())
      .collect::<Vec<_>>();

    assert_eq!(batches, vec![40, 40, 40]);
  }

  #[test]
  fn populate_liked_song_ids_from_saved_tracks_uses_raw_track_ids() {
    let page = saved_tracks_page(0, 20, false);
    let mut liked_song_ids_set = HashSet::new();

    populate_liked_song_ids_from_saved_tracks(&mut liked_song_ids_set, &page);

    assert!(liked_song_ids_set.contains("0000000000000000000001"));
    assert!(liked_song_ids_set.contains("0000000000000000000002"));
    assert!(!liked_song_ids_set.contains("spotify:track:0000000000000000000001"));
  }

  #[test]
  fn playlist_track_filter_matches_title_artist_album_case_insensitively() {
    let mut track = full_track("0000000000000000000001");
    track.name = "Midnight City".to_string();
    track.artists[0].name = "M83".to_string();
    track.album.name = "Hurry Up".to_string();

    assert!(playlist_track_matches_terms(
      &track,
      &playlist_track_search_terms("midnight")
    ));
    assert!(playlist_track_matches_terms(
      &track,
      &playlist_track_search_terms("m83")
    ));
    assert!(playlist_track_matches_terms(
      &track,
      &playlist_track_search_terms("hurry")
    ));
    assert!(playlist_track_matches_terms(
      &track,
      &playlist_track_search_terms("MIDNIGHT m83")
    ));
  }

  #[test]
  fn playlist_track_filter_requires_every_query_term() {
    let mut track = full_track("0000000000000000000001");
    track.name = "Midnight City".to_string();
    track.artists[0].name = "M83".to_string();
    track.album.name = "Hurry Up".to_string();

    assert!(playlist_track_matches_terms(
      &track,
      &playlist_track_search_terms("city hurry")
    ));
    assert!(!playlist_track_matches_terms(
      &track,
      &playlist_track_search_terms("city missing")
    ));
  }
}

#[cfg(feature = "streaming")]
async fn fetch_rootlist_folders(
  streaming_player: Option<Arc<StreamingPlayer>>,
) -> Option<Vec<PlaylistFolderNode>> {
  let player = streaming_player?;
  let session = player.session();

  let bytes = match session.spclient().get_rootlist(0, Some(100_000)).await {
    Ok(bytes) => bytes,
    Err(_) => return None,
  };

  use protobuf::Message;
  let selected: librespot_protocol::playlist4_external::SelectedListContent =
    Message::parse_from_bytes(&bytes).ok()?;

  let contents = selected.contents.as_ref()?;
  Some(parse_rootlist_items(&contents.items))
}

fn build_flat_playlist_items(playlists: &[PlaylistInfo]) -> Vec<PlaylistFolderItem> {
  playlists
    .iter()
    .enumerate()
    .map(|(index, _)| PlaylistFolderItem::Playlist {
      index,
      current_id: 0,
    })
    .collect()
}

fn reconcile_playlist_selection(
  app: &mut App,
  preferred_playlist_id: Option<&str>,
  preferred_folder_id: usize,
  preferred_selected_index: Option<usize>,
) {
  if app.playlist_folder_items.is_empty() {
    app.current_playlist_folder_id = 0;
    app.view.selected_playlist_index = None;
    return;
  }

  let folder_has_visible = |folder_id: usize, app: &App| {
    app.playlist_folder_items.iter().any(|item| match item {
      PlaylistFolderItem::Folder(folder) => folder.current_id == folder_id,
      PlaylistFolderItem::Playlist { current_id, .. } => *current_id == folder_id,
      PlaylistFolderItem::CommunityPin => false,
    })
  };

  app.current_playlist_folder_id = if folder_has_visible(preferred_folder_id, app) {
    preferred_folder_id
  } else {
    0
  };

  if let Some(playlist_id) = preferred_playlist_id {
    let visible_playlist_index = app
      .get_playlist_display_items()
      .into_iter()
      .enumerate()
      .find_map(|(display_idx, item)| match item {
        PlaylistFolderItem::Playlist { index, .. } => app
          .all_playlists
          .get(*index)
          .filter(|playlist| playlist.id.as_deref() == Some(playlist_id))
          .map(|_| display_idx),
        PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => None,
      });

    if let Some(display_idx) = visible_playlist_index {
      // Sidebar rows are offset by the leading "+ Add Playlist" row.
      app.view.selected_playlist_index = Some(display_idx + 1);
      return;
    }

    let mut target_folder: Option<usize> = None;
    for item in &app.playlist_folder_items {
      if let PlaylistFolderItem::Playlist { index, current_id } = item {
        if let Some(playlist) = app.all_playlists.get(*index) {
          if playlist.id.as_deref() == Some(playlist_id) {
            target_folder = Some(*current_id);
            break;
          }
        }
      }
    }

    if let Some(folder_id) = target_folder {
      app.current_playlist_folder_id = folder_id;
      let display_idx = app
        .get_playlist_display_items()
        .into_iter()
        .enumerate()
        .find_map(|(idx, item)| match item {
          PlaylistFolderItem::Playlist { index, .. } => app
            .all_playlists
            .get(*index)
            .filter(|playlist| playlist.id.as_deref() == Some(playlist_id))
            .map(|_| idx),
          PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => None,
        });
      if let Some(idx) = display_idx {
        // Sidebar rows are offset by the leading "+ Add Playlist" row.
        app.view.selected_playlist_index = Some(idx + 1);
        return;
      }
    }
  }

  // Sidebar rows run 0..=count: row 0 is "+ Add Playlist", items follow, so a
  // preferred row clamps to `count` (the last item), not `count - 1`.
  let visible_count = app.get_playlist_display_count();
  if visible_count == 0 {
    app.current_playlist_folder_id = 0;
    let root_count = app.get_playlist_display_count();
    app.view.selected_playlist_index = if root_count == 0 {
      None
    } else {
      Some(preferred_selected_index.unwrap_or(0).min(root_count))
    };
    return;
  }

  app.view.selected_playlist_index = Some(preferred_selected_index.unwrap_or(0).min(visible_count));
}

#[cfg(feature = "streaming")]
fn parse_rootlist_items(
  items: &[librespot_protocol::playlist4_external::Item],
) -> Vec<PlaylistFolderNode> {
  let mut root: Vec<PlaylistFolderNode> = Vec::new();
  let mut stack: Vec<Vec<PlaylistFolderNode>> = Vec::new();
  let mut name_stack: Vec<(String, String)> = Vec::new();

  for item in items {
    let uri = item.uri();

    if let Some(rest) = uri.strip_prefix("spotify:start-group:") {
      let (group_id, name) = match rest.find(':') {
        Some(pos) => (rest[..pos].to_string(), rest[pos + 1..].to_string()),
        None => (rest.to_string(), String::new()),
      };
      name_stack.push((group_id, name));
      stack.push(std::mem::take(&mut root));
      root = Vec::new();
    } else if uri.starts_with("spotify:end-group:") {
      if let Some((group_id, name)) = name_stack.pop() {
        let children = std::mem::take(&mut root);
        root = stack.pop().unwrap_or_default();
        root.push(PlaylistFolderNode {
          name: Some(name),
          node_type: PlaylistFolderNodeType::Folder,
          uri: format!("spotify:folder:{}", group_id),
          children,
        });
      }
    } else {
      root.push(PlaylistFolderNode {
        name: None,
        node_type: PlaylistFolderNodeType::Playlist,
        uri: uri.to_string(),
        children: Vec::new(),
      });
    }
  }

  while let Some((group_id, name)) = name_stack.pop() {
    let children = std::mem::take(&mut root);
    root = stack.pop().unwrap_or_default();
    root.push(PlaylistFolderNode {
      name: Some(name),
      node_type: PlaylistFolderNodeType::Folder,
      uri: format!("spotify:folder:{}", group_id),
      children,
    });
  }

  root
}

fn structurize_playlist_folders(
  nodes: &[PlaylistFolderNode],
  playlists: &[PlaylistInfo],
) -> Vec<PlaylistFolderItem> {
  use std::collections::{HashMap, HashSet};

  let playlist_map: HashMap<String, usize> = playlists
    .iter()
    .enumerate()
    .filter_map(|(idx, playlist)| playlist.id.clone().map(|id| (id, idx)))
    .collect();

  let mut items: Vec<PlaylistFolderItem> = Vec::new();
  let mut next_folder_id: usize = 1;
  let mut used_playlist_indices: HashSet<usize> = HashSet::new();

  fn walk(
    nodes: &[PlaylistFolderNode],
    current_folder_id: usize,
    items: &mut Vec<PlaylistFolderItem>,
    next_folder_id: &mut usize,
    playlist_map: &std::collections::HashMap<String, usize>,
    used_playlist_indices: &mut std::collections::HashSet<usize>,
  ) {
    for node in nodes {
      match node.node_type {
        PlaylistFolderNodeType::Folder => {
          let folder_id = *next_folder_id;
          *next_folder_id += 1;

          let name = node.name.as_deref().unwrap_or("Unnamed Folder").to_string();

          items.push(PlaylistFolderItem::Folder(PlaylistFolder {
            name: name.clone(),
            current_id: current_folder_id,
            target_id: folder_id,
          }));

          items.push(PlaylistFolderItem::Folder(PlaylistFolder {
            name: format!("\u{2190} {}", name),
            current_id: folder_id,
            target_id: current_folder_id,
          }));

          walk(
            &node.children,
            folder_id,
            items,
            next_folder_id,
            playlist_map,
            used_playlist_indices,
          );
        }
        PlaylistFolderNodeType::Playlist => {
          let playlist_id = node
            .uri
            .strip_prefix("spotify:playlist:")
            .unwrap_or(&node.uri);

          if let Some(&index) = playlist_map.get(playlist_id) {
            items.push(PlaylistFolderItem::Playlist {
              index,
              current_id: current_folder_id,
            });
            used_playlist_indices.insert(index);
          }
        }
      }
    }
  }

  walk(
    nodes,
    0,
    &mut items,
    &mut next_folder_id,
    &playlist_map,
    &mut used_playlist_indices,
  );

  for (index, _) in playlists.iter().enumerate() {
    if !used_playlist_indices.contains(&index) {
      items.push(PlaylistFolderItem::Playlist {
        index,
        current_id: 0,
      });
    }
  }

  items
}
