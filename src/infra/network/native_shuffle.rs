//! Client-side ("Spotify-style") shuffle for native streaming playback.
//!
//! When shuffle is on and a Spotify context (playlist / album / Liked Songs)
//! starts on the native streaming device, the app fetches the context's track
//! list, shuffles it once (selected track first) and loads the flat URI list
//! into Spirc via `LoadRequest::from_tracks` — instead of delegating shuffle to
//! Spirc/Spotify, which regenerate the shuffle order on every context reload
//! (so a queue suspend/resume could repeat already-played tracks).
//!
//! The app-owned play order lives in
//! [`App::native_spotify_shuffle`](crate::core::app::App::native_spotify_shuffle).
//! Playback starts instantly from the tracks already on hand; a background
//! fetch completes the full context (capped) and swaps it in with a single
//! reload that preserves the playback position.
//!
//! The pure order/merge helpers at the top are unconditional so the slim
//! (non-`streaming`) CI build still compiles and runs their tests.

#[cfg(feature = "streaming")]
use super::requests::spotify_get_typed_compat_for_with_refresh;
#[cfg(feature = "streaming")]
use super::Network;
#[cfg(feature = "streaming")]
use crate::core::app::{App, NativeSpotifyShuffleSession, TrackTableContext};
#[cfg(feature = "streaming")]
use crate::infra::player::StreamingPlayer;
#[cfg(feature = "streaming")]
use anyhow::anyhow;
#[cfg(feature = "streaming")]
use librespot_connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
#[cfg(feature = "streaming")]
use log::{info, warn};
#[cfg(feature = "streaming")]
use rspotify::model::{
  idtypes::{PlayContextId, PlayableId, PlaylistId},
  page::Page,
  playlist::PlaylistItem,
  track::SavedTrack,
  PlayableItem,
};
#[cfg(feature = "streaming")]
use rspotify::{prelude::*, AuthCodePkceSpotify};
#[cfg(feature = "streaming")]
use std::path::Path;
#[cfg(feature = "streaming")]
use std::sync::Arc;
#[cfg(feature = "streaming")]
use std::time::Instant;
#[cfg(feature = "streaming")]
use tokio::sync::Mutex;

/// Full-context fetch cap. Beyond this the shuffle covers the first
/// `MAX_NATIVE_SHUFFLE_TRACKS` tracks of the context (with a status message)
/// rather than stalling playback behind an unbounded pagination walk.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) const MAX_NATIVE_SHUFFLE_TRACKS: usize = 3000;

/// Fisher-Yates shuffle with the track at `first` moved to the front — the
/// same permutation shape as the decoded sources' `shuffle_in_place`, so the
/// selected/current track keeps playing and everything else follows once.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn shuffled_order(mut uris: Vec<String>, first: usize) -> Vec<String> {
  use rand::seq::SliceRandom;
  if first < uris.len() {
    uris.swap(0, first);
  }
  if uris.len() > 1 {
    uris[1..].shuffle(&mut rand::rng());
  }
  uris
}

/// Merge a completed full-context fetch into a partially-loaded play order:
/// the already-played prefix (`order[..=index]`) is preserved verbatim, and
/// every fetched track not accounted for by that prefix follows in a fresh
/// shuffled order. Duplicate-safe: the prefix consumes fetched occurrences one
/// for one.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn merge_full_fetch(order: &[String], index: usize, fetched: &[String]) -> Vec<String> {
  fold_full_context(order, index, fetched, true)
}

/// Shared body of [`merge_full_fetch`]: preserve the already-played prefix
/// (`order[..=index]`) verbatim and append every fetched track not accounted
/// for by that prefix. `shuffle_rest` randomizes the appended tail (shuffle on)
/// or keeps it in fetched (natural) order (shuffle off). Duplicate-safe: the
/// prefix consumes fetched occurrences one for one. Keeping the prefix intact
/// leaves the currently-playing track fixed at `index`, so callers can anchor
/// on that index instead of a duplicate-ambiguous URI lookup.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn fold_full_context(
  order: &[String],
  index: usize,
  fetched: &[String],
  shuffle_rest: bool,
) -> Vec<String> {
  use rand::seq::SliceRandom;
  use std::collections::HashMap;

  let prefix_end = (index + 1).min(order.len());
  let prefix = &order[..prefix_end];
  let mut consumed: HashMap<&str, usize> = HashMap::new();
  for uri in prefix {
    *consumed.entry(uri.as_str()).or_default() += 1;
  }
  let mut rest: Vec<String> = Vec::with_capacity(fetched.len().saturating_sub(prefix.len()));
  for uri in fetched {
    if let Some(count) = consumed.get_mut(uri.as_str()) {
      if *count > 0 {
        *count -= 1;
        continue;
      }
    }
    rest.push(uri.clone());
  }
  if shuffle_rest {
    rest.shuffle(&mut rand::rng());
  }
  let mut merged = prefix.to_vec();
  merged.extend(rest);
  merged
}

/// Index of the `nth` (1-based) occurrence of `uri` in `list`, or the last
/// occurrence when there are fewer than `nth` (and `None` when absent). Used to
/// re-anchor a duplicated track onto the matching copy — by its rank among the
/// tracks played so far — instead of always the first occurrence.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn nth_occurrence(list: &[String], uri: &str, nth: usize) -> Option<usize> {
  let mut seen = 0;
  let mut last = None;
  for (i, u) in list.iter().enumerate() {
    if u == uri {
      seen += 1;
      last = Some(i);
      if seen >= nth.max(1) {
        return Some(i);
      }
    }
  }
  last
}

/// Whether two URI lists hold the same tracks (order-insensitive,
/// duplicate-aware) — used to skip a pointless reload when the initially
/// loaded tracks already were the whole context.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn same_track_multiset(a: &[String], b: &[String]) -> bool {
  if a.len() != b.len() {
    return false;
  }
  let mut a: Vec<&str> = a.iter().map(String::as_str).collect();
  let mut b: Vec<&str> = b.iter().map(String::as_str).collect();
  a.sort_unstable();
  b.sort_unstable();
  a == b
}

/// Load the session's play order into Spirc as a flat track list. Spirc-side
/// shuffle is forced off first: the list itself carries the order, and a
/// `shuffling_context` Spirc would re-shuffle it on load. `start_playing`
/// preserves the current play/pause state across the reload so, e.g., toggling
/// shuffle while paused does not resume playback.
#[cfg(feature = "streaming")]
fn load_session_tracks(
  player: &StreamingPlayer,
  order: Vec<String>,
  index: usize,
  seek_ms: u32,
  start_playing: bool,
) -> anyhow::Result<()> {
  let _ = player.set_shuffle(false);
  let options = LoadRequestOptions {
    start_playing,
    seek_to: seek_ms,
    context_options: None,
    playing_track: u32::try_from(index).ok().map(PlayingTrack::Index),
  };
  player.load(LoadRequest::from_tracks(order, options))
}

/// Drop a stale reload confirmation after a failed `load_session_tracks`: no
/// `TrackChanged` follows a load that never landed, so a later genuine one must
/// not be mistaken for the reload's confirmation (which would pin or move
/// `session.index` onto the wrong duplicate occurrence). A no-op when the error
/// path already cleared the whole session.
#[cfg(feature = "streaming")]
async fn clear_pending_reload(app: &Arc<Mutex<App>>) {
  if let Some(session) = app.lock().await.native_spotify_shuffle.as_mut() {
    session.pending_reload_index = None;
  }
}

/// What the background task should fetch to complete the session's context.
#[cfg(feature = "streaming")]
enum FullFetch {
  Playlist(PlaylistId<'static>),
  SavedTracks,
}

/// The shuffleable playback target classified out of a `StartPlayback`
/// request. Anything else (artist/show contexts, raw search/recommendation
/// lists) keeps the pre-existing shuffle behavior.
#[cfg(feature = "streaming")]
enum ShuffleTarget {
  Playlist {
    id: PlaylistId<'static>,
    selected_uri: String,
  },
  Album {
    id: rspotify::model::idtypes::AlbumId<'static>,
    selected: usize,
  },
  SavedTracks {
    list: Vec<String>,
    selected: usize,
  },
}

#[cfg(feature = "streaming")]
impl Network {
  /// Intercept a native `StartPlayback` when shuffle is on and the target is a
  /// shuffleable Spotify context. Returns `true` when this path handled the
  /// request (playback started, or failed with an error surfaced); `false`
  /// falls back to the pre-existing ContextApi / direct-load routes.
  pub(super) async fn try_start_native_shuffled_playback(
    &mut self,
    player: &Arc<StreamingPlayer>,
    context_id: &Option<PlayContextId<'static>>,
    uris: &Option<Vec<PlayableId<'static>>>,
    offset: Option<usize>,
  ) -> bool {
    let target = match (context_id, uris) {
      (Some(PlayContextId::Playlist(id)), maybe_uris) => {
        // The playlist Enter path passes the selected track as a single-item
        // URI list; without it there is nothing to pin first, so fall back.
        let Some(selected_uri) = maybe_uris.as_ref().and_then(|v| v.first()).map(|u| u.uri())
        else {
          return false;
        };
        ShuffleTarget::Playlist {
          id: id.clone(),
          selected_uri,
        }
      }
      (Some(PlayContextId::Album(id)), _) => ShuffleTarget::Album {
        id: id.clone(),
        selected: offset.unwrap_or(0),
      },
      (None, Some(list)) => {
        // Raw URI lists are only shuffleable when they stand in for Liked
        // Songs (the full library is fetchable); search/recommendation lists
        // have no larger backing context.
        let is_saved_tracks = {
          let app = self.app.lock().await;
          matches!(
            app.track_table.context,
            Some(TrackTableContext::SavedTracks)
          )
        };
        if !is_saved_tracks || list.is_empty() {
          return false;
        }
        ShuffleTarget::SavedTracks {
          list: list.iter().map(|u| u.uri()).collect(),
          selected: offset.unwrap_or(0),
        }
      }
      _ => return false,
    };

    let (original, order, fetch) = match target {
      ShuffleTarget::Playlist { id, selected_uri } => (
        vec![selected_uri.clone()],
        vec![selected_uri],
        Some(FullFetch::Playlist(id)),
      ),
      ShuffleTarget::Album { id, selected } => {
        // Albums are small: fetch the whole track list up front (one page for
        // almost every album) so the first load is already complete.
        let tracks = match super::metadata::fetch_album_tracks_from(self, id.id(), 0).await {
          Ok(tracks) => tracks,
          Err(e) => {
            info!("native shuffle: album track fetch failed, falling back: {e}");
            return false;
          }
        };
        let uris: Vec<String> = tracks
          .iter()
          .filter_map(|t| t.id.as_ref().map(|id| id.uri()))
          .collect();
        if uris.is_empty() {
          return false;
        }
        let selected = selected.min(uris.len() - 1);
        (uris.clone(), shuffled_order(uris, selected), None)
      }
      ShuffleTarget::SavedTracks { list, selected } => {
        let selected = selected.min(list.len() - 1);
        (
          list.clone(),
          shuffled_order(list, selected),
          Some(FullFetch::SavedTracks),
        )
      }
    };

    let generation = {
      let mut app = self.app.lock().await;
      let generation = app.next_native_shuffle_generation();
      app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
        order: order.clone(),
        original,
        index: 0,
        shuffled: true,
        fetch_complete: fetch.is_none(),
        fetch_failed: false,
        generation,
        // The load below plays index 0; confirm that on the first TrackChanged.
        pending_reload_index: Some(0),
        pending_manual_skip: None,
      });
      // Park the ORIGINAL request so zombie-session recovery replays it
      // through this same interception, and arm the load watchdog like the
      // direct-load route does.
      app.park_start_playback(
        context_id.as_ref().map(|c| c.uri()),
        uris
          .as_ref()
          .map(|list| list.iter().map(|u| u.uri()).collect()),
        offset,
      );
      app.native_load_watchdog = Some(Instant::now());
      if let Some(ctx) = &mut app.current_playback_context {
        ctx.is_playing = true;
        ctx.shuffle_state = true;
      }
      app.user_config.behavior.shuffle_enabled = true;
      generation
    };

    info!(
      "starting native playback via client-side shuffle ({} tracks loaded)",
      order.len()
    );
    if let Err(e) = load_session_tracks(player, order, 0, 0, true) {
      let mut app = self.app.lock().await;
      app.clear_native_shuffle_session();
      app.handle_error(anyhow!("Failed to start native playback: {}", e));
      return true;
    }
    if let Some(kind) = fetch {
      self.spawn_full_context_fetch(kind, generation);
    }
    true
  }

  /// Handler for `IoEvent::ToggleNativeShuffleSession`: reorder the app-owned
  /// list and reload it. With no session and shuffle turning on mid-playback,
  /// build one from the current context when possible; otherwise fall back to
  /// Spirc shuffle (previous behavior).
  pub(super) async fn toggle_native_shuffle_session(&mut self, on: bool) {
    enum Action {
      Reload(Arc<StreamingPlayer>, Vec<String>, usize, u32, bool),
      Build {
        context_uri: String,
        current_uri: String,
      },
      Spirc(Arc<StreamingPlayer>),
      Nothing,
    }

    let action = {
      let mut guard = self.app.lock().await;
      let player = guard.streaming_player.clone();
      let seek_ms = u32::try_from(guard.song_progress_ms).unwrap_or(u32::MAX);
      let is_playing = guard.native_shuffle_is_playing();
      if let Some(session) = guard.native_spotify_shuffle.as_mut() {
        if session.shuffled == on {
          Action::Nothing
        } else if !session.fetch_complete {
          // The full context is still being fetched, so the app-owned
          // `original` is incomplete: reordering now would truncate playback to
          // the handful of loaded tracks. Flip the flag and let the
          // fetch-completion path apply it against the whole context. Leave
          // `pending_reload_index` intact — the initial load's confirmation may
          // still be in flight, and this path issues no new reload.
          session.shuffled = on;
          Action::Nothing
        } else {
          let current_uri = session.order.get(session.index).cloned();
          if on {
            let first = current_uri
              .as_ref()
              .and_then(|uri| session.original.iter().position(|x| x == uri))
              .unwrap_or(0);
            session.order = shuffled_order(session.original.clone(), first);
            session.index = 0;
          } else {
            session.order = session.original.clone();
            session.index = current_uri
              .as_ref()
              .and_then(|uri| session.order.iter().position(|x| x == uri))
              .unwrap_or(0);
          }
          session.shuffled = on;
          match player {
            Some(player) => {
              session.pending_reload_index = Some(session.index);
              Action::Reload(
                player,
                session.order.clone(),
                session.index,
                seek_ms,
                is_playing,
              )
            }
            None => Action::Nothing,
          }
        }
      } else if on {
        let context_uri = guard
          .current_playback_context
          .as_ref()
          .and_then(|ctx| ctx.context.as_ref())
          .map(|c| c.uri.clone());
        let current_uri = guard
          .current_playback_context
          .as_ref()
          .and_then(|ctx| ctx.item.as_ref())
          .and_then(|item| match item {
            PlayableItem::Track(t) => t.id.as_ref().map(|id| id.uri()),
            _ => None,
          });
        match (context_uri, current_uri, player) {
          (Some(context_uri), Some(current_uri), Some(_)) => Action::Build {
            context_uri,
            current_uri,
          },
          (_, _, Some(player)) => Action::Spirc(player),
          _ => Action::Nothing,
        }
      } else {
        match player {
          Some(player) => Action::Spirc(player),
          None => Action::Nothing,
        }
      }
    };

    match action {
      Action::Reload(player, order, index, seek_ms, start_playing) => {
        if let Err(e) = load_session_tracks(&player, order, index, seek_ms, start_playing) {
          clear_pending_reload(&self.app).await;
          self
            .app
            .lock()
            .await
            .handle_error(anyhow!("Failed to apply shuffle: {}", e));
        }
      }
      Action::Build {
        context_uri,
        current_uri,
      } => {
        self
          .build_shuffle_session_from_current(context_uri, current_uri)
          .await;
      }
      Action::Spirc(player) => {
        let _ = player.set_shuffle(on);
      }
      Action::Nothing => {}
    }
  }

  /// Shuffle was turned on mid-playback with no session: build one around the
  /// currently playing track. Playlists stay lazy (the fetch-completion reload
  /// swaps the full shuffled list in); albums load fully right away. Contexts
  /// the session can't cover fall back to Spirc shuffle.
  #[cfg(feature = "streaming")]
  async fn build_shuffle_session_from_current(&mut self, context_uri: String, current_uri: String) {
    match super::ids::play_context_id(&context_uri) {
      Some(PlayContextId::Playlist(id)) => {
        let generation = {
          let mut app = self.app.lock().await;
          let generation = app.next_native_shuffle_generation();
          app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
            order: vec![current_uri.clone()],
            original: vec![current_uri],
            index: 0,
            shuffled: true,
            fetch_complete: false,
            fetch_failed: false,
            generation,
            // No reload here: Spirc keeps playing its own context.
            pending_reload_index: None,
            pending_manual_skip: None,
          });
          generation
        };
        // No reload yet: Spirc keeps playing its current context, and the
        // fetch completion swaps in the full shuffled list (single rebuffer).
        self.spawn_full_context_fetch(FullFetch::Playlist(id), generation);
      }
      Some(PlayContextId::Album(id)) => {
        let tracks = match super::metadata::fetch_album_tracks_from(self, id.id(), 0).await {
          Ok(tracks) => tracks,
          Err(e) => {
            info!("native shuffle: album track fetch failed, falling back: {e}");
            self.fallback_spirc_shuffle(true).await;
            return;
          }
        };
        let uris: Vec<String> = tracks
          .iter()
          .filter_map(|t| t.id.as_ref().map(|id| id.uri()))
          .collect();
        let Some(first) = uris.iter().position(|uri| *uri == current_uri) else {
          self.fallback_spirc_shuffle(true).await;
          return;
        };
        let reload = {
          let mut app = self.app.lock().await;
          let generation = app.next_native_shuffle_generation();
          let order = shuffled_order(uris.clone(), first);
          app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
            order: order.clone(),
            original: uris,
            index: 0,
            shuffled: true,
            fetch_complete: true,
            fetch_failed: false,
            generation,
            // The reload below plays index 0.
            pending_reload_index: Some(0),
            pending_manual_skip: None,
          });
          let seek_ms = u32::try_from(app.song_progress_ms).unwrap_or(u32::MAX);
          let is_playing = app.native_shuffle_is_playing();
          app
            .streaming_player
            .clone()
            .map(|p| (p, order, seek_ms, is_playing))
        };
        if let Some((player, order, seek_ms, is_playing)) = reload {
          if let Err(e) = load_session_tracks(&player, order, 0, seek_ms, is_playing) {
            let mut app = self.app.lock().await;
            app.clear_native_shuffle_session();
            app.handle_error(anyhow!("Failed to apply shuffle: {}", e));
          }
        }
      }
      _ => self.fallback_spirc_shuffle(true).await,
    }
  }

  #[cfg(feature = "streaming")]
  async fn fallback_spirc_shuffle(&self, on: bool) {
    if let Some(player) = { self.app.lock().await.streaming_player.clone() } {
      let _ = player.set_shuffle(on);
    }
  }

  /// Handler for `IoEvent::ReshuffleNativeShuffleLap`: a repeat-all lap
  /// completed — re-randomize the order (current track first) so the next lap
  /// plays in a fresh order, like Spotify.
  pub(super) async fn reshuffle_native_shuffle_lap(&mut self) {
    let reload = {
      let mut guard = self.app.lock().await;
      let player = guard.streaming_player.clone();
      let seek_ms = u32::try_from(guard.song_progress_ms).unwrap_or(0);
      let is_playing = guard.native_shuffle_is_playing();
      let Some(session) = guard.native_spotify_shuffle.as_mut() else {
        return;
      };
      if !session.shuffled || session.order.len() < 2 {
        return;
      }
      let current_uri = session.order.get(session.index).cloned();
      let first = current_uri
        .as_ref()
        .and_then(|uri| session.original.iter().position(|x| x == uri))
        .unwrap_or(0);
      session.order = shuffled_order(session.original.clone(), first);
      session.index = 0;
      session.pending_reload_index = Some(0);
      player.map(|p| (p, session.order.clone(), seek_ms, is_playing))
    };
    if let Some((player, order, seek_ms, is_playing)) = reload {
      if let Err(e) = load_session_tracks(&player, order, 0, seek_ms, is_playing) {
        warn!("native shuffle: lap reshuffle reload failed: {e}");
        clear_pending_reload(&self.app).await;
      }
    }
  }

  /// Handler for `IoEvent::ResumeNativeShuffleSession`: the native queue
  /// drained — reload the session's unchanged play order at the resume index.
  /// This is the path that used to reload (and reshuffle) the whole context.
  pub(super) async fn resume_native_shuffle_session(
    &mut self,
    resume_index: Option<usize>,
    generation: u64,
  ) {
    let action = {
      let mut guard = self.app.lock().await;
      // The suspend snapshotted a specific session; a session replaced while the
      // queue drained bumps the generation, so a stale resume must not touch it.
      let session_matches = guard
        .native_spotify_shuffle
        .as_ref()
        .is_some_and(|s| s.generation == generation);
      match resume_index {
        Some(index) if session_matches => {
          let player = guard.streaming_player.clone();
          match guard.native_spotify_shuffle.as_mut() {
            Some(session) if index < session.order.len() => {
              session.index = index;
              session.pending_reload_index = Some(index);
              player.map(|p| (p, session.order.clone(), index))
            }
            _ => {
              guard.set_status_message("Queue finished", 3);
              None
            }
          }
        }
        Some(_) => {
          // The stored index belongs to a session that is no longer active. If a
          // newer session replaced it, leave that one playing untouched;
          // otherwise the session is simply gone, so note the queue finished.
          if guard.native_spotify_shuffle.is_none() {
            guard.set_status_message("Queue finished", 3);
          }
          None
        }
        None => {
          // Session exhausted under repeat-off: nothing left to resume. Only tear
          // down the session we suspended — a newer one must be left running.
          if session_matches {
            guard.clear_native_shuffle_session();
          }
          guard.set_status_message("Queue finished", 3);
          None
        }
      }
    };
    if let Some((player, order, index)) = action {
      player.activate();
      // The queue drained, so resume playback regardless of prior pause state.
      if let Err(e) = load_session_tracks(&player, order, index, 0, true) {
        clear_pending_reload(&self.app).await;
        self
          .app
          .lock()
          .await
          .handle_error(anyhow!("Failed to resume playback: {}", e));
      }
    }
  }

  /// Complete the session's context off the IoEvent pump (pagination can take
  /// many round trips). Completion merges into whatever the session looks like
  /// by then; the generation stamp discards results for a replaced session.
  #[cfg(feature = "streaming")]
  fn spawn_full_context_fetch(&self, kind: FullFetch, generation: u64) {
    let spotify = self.spotify().clone();
    let app = Arc::clone(&self.app);
    let token_cache_path = self.token_cache_path.clone();
    tokio::spawn(async move {
      // A failed fetch strands a lazily-seeded session on its single seed
      // track (nothing to advance into), so retry transient API failures
      // before declaring it failed for good.
      const FETCH_ATTEMPTS: u32 = 3;
      let mut attempt = 0u32;
      let result = loop {
        let result = match &kind {
          FullFetch::Playlist(id) => {
            fetch_playlist_uris(&spotify, &token_cache_path, &app, id).await
          }
          FullFetch::SavedTracks => fetch_saved_track_uris(&spotify, &token_cache_path, &app).await,
        };
        attempt += 1;
        match result {
          Err(e) if attempt < FETCH_ATTEMPTS => {
            info!("native shuffle: context fetch attempt {attempt} failed, retrying: {e}");
            tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
          }
          result => break result,
        }
      };
      finish_full_context_fetch(&app, generation, result).await;
    });
  }
}

/// Walk a paginated Spotify collection at `path`, mapping each item to a track
/// URI via `extract` (items yielding `None` are skipped), in native pagination
/// order and capped at [`MAX_NATIVE_SHUFFLE_TRACKS`]. Returns `(uris,
/// truncated)`, where `truncated` is true when the cap cut the list short.
#[cfg(feature = "streaming")]
async fn paginate_uris<Item>(
  spotify: &AuthCodePkceSpotify,
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
  path: &str,
  extract: impl Fn(&Item) -> Option<String>,
) -> anyhow::Result<(Vec<String>, bool)>
where
  Item: serde::de::DeserializeOwned,
{
  let limit = 50u32;
  let mut offset = 0u32;
  let mut uris = Vec::new();
  loop {
    let page = spotify_get_typed_compat_for_with_refresh::<Page<Item>>(
      spotify,
      path,
      &[("limit", limit.to_string()), ("offset", offset.to_string())],
      token_cache_path,
      app,
    )
    .await?;
    let has_next = page.next.is_some();
    for item in &page.items {
      if let Some(uri) = extract(item) {
        uris.push(uri);
      }
    }
    if uris.len() >= MAX_NATIVE_SHUFFLE_TRACKS {
      let truncated = uris.len() > MAX_NATIVE_SHUFFLE_TRACKS || has_next;
      uris.truncate(MAX_NATIVE_SHUFFLE_TRACKS);
      return Ok((uris, truncated));
    }
    if !has_next {
      break;
    }
    offset = page.offset.saturating_add(page.limit);
  }
  Ok((uris, false))
}

/// All track URIs of a playlist, in playlist order, capped at
/// [`MAX_NATIVE_SHUFFLE_TRACKS`]. Returns `(uris, truncated)`. Episodes and
/// unplayable items are skipped.
#[cfg(feature = "streaming")]
async fn fetch_playlist_uris(
  spotify: &AuthCodePkceSpotify,
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
  playlist_id: &PlaylistId<'static>,
) -> anyhow::Result<(Vec<String>, bool)> {
  let path = format!("playlists/{}/items", playlist_id.id());
  paginate_uris(
    spotify,
    token_cache_path,
    app,
    &path,
    |item: &PlaylistItem| match item.item.as_ref() {
      Some(PlayableItem::Track(track)) => track.id.as_ref().map(|id| id.uri()),
      _ => None,
    },
  )
  .await
}

/// All Liked Songs track URIs, newest first (the library's native order),
/// capped at [`MAX_NATIVE_SHUFFLE_TRACKS`]. Returns `(uris, truncated)`.
#[cfg(feature = "streaming")]
async fn fetch_saved_track_uris(
  spotify: &AuthCodePkceSpotify,
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
) -> anyhow::Result<(Vec<String>, bool)> {
  paginate_uris(
    spotify,
    token_cache_path,
    app,
    "me/tracks",
    |item: &SavedTrack| item.track.id.as_ref().map(|id| id.uri()),
  )
  .await
}

/// Re-anchor the currently-playing track when the play order is being replaced
/// by `target`: find the occurrence in `target` matching the current track's
/// rank among the tracks played so far (`order[..=index]`), so a duplicated
/// track lands on the matching copy rather than always the first. Falls back to
/// index 0 when the current track is absent from `target`.
#[cfg(feature = "streaming")]
fn reanchor_by_rank(order: &[String], index: usize, target: &[String]) -> usize {
  let index = index.min(order.len().saturating_sub(1));
  order
    .get(index)
    .and_then(|uri| {
      let rank = order[..=index].iter().filter(|u| *u == uri).count();
      nth_occurrence(target, uri, rank)
    })
    .unwrap_or(0)
}

/// Fold a finished full-context fetch into the live session and reload Spirc
/// once, anchored on the actually-playing track with the position preserved.
#[cfg(feature = "streaming")]
async fn finish_full_context_fetch(
  app: &Arc<Mutex<App>>,
  generation: u64,
  result: anyhow::Result<(Vec<String>, bool)>,
) {
  let reload = {
    let mut guard = app.lock().await;
    let player = guard.streaming_player.clone();
    let seek_ms = u32::try_from(guard.song_progress_ms).unwrap_or(u32::MAX);
    let start_playing = guard.native_shuffle_is_playing();
    // The session may be suspended behind a queued track; folding the context
    // in is fine, but reloading Spirc would hijack the sink from the queue.
    let queue_active = guard.queue_owns_playback() || guard.queue_suspended.is_some();
    // The resume point the suspend already computed (same-track handoff vs a
    // plain advance), captured so the fold can remap it rather than clobber it.
    let suspended_resume: Option<Option<usize>> = match &guard.queue_suspended {
      Some(crate::core::queue::SuspendedContext::SpotifyShuffled { resume_index, .. }) => {
        Some(*resume_index)
      }
      _ => None,
    };
    let mut status: Option<String> = None;
    // When the session is suspended behind the queue, the new play order is
    // still folded in but Spirc is not reloaded; this carries the re-pointed
    // resume index out to `queue_suspended` once the session borrow ends.
    let mut resume_update: Option<Option<usize>> = None;
    // Carries the seed-only fetch failure out of the session borrow: a
    // suspension taken while the fetch was still in flight stored a shuffled
    // resume into the seed-only order and must be converted to the context
    // route (mirror of the suspend-time fallback).
    let mut convert_suspend_to_context = false;
    let reload = {
      let Some(session) = guard.native_spotify_shuffle.as_mut() else {
        return;
      };
      if session.generation != generation {
        return;
      }
      session.fetch_complete = true;
      match result {
        Err(e) => {
          session.fetch_failed = true;
          info!(
            "native shuffle: full-context fetch failed; shuffle covers the {} loaded tracks: {e}",
            session.order.len()
          );
          // A seed-only session can't shuffle anything; tell the user instead
          // of silently degrading (the queue-suspend path now falls back to
          // the context route, see `suspend_native_spotify_context_for_queue`).
          if session.order.len() <= 1 {
            status = Some("Shuffle: failed to load the context's track list".to_string());
            convert_suspend_to_context = true;
          }
          // A shuffle-off deferred to fetch completion still needs applying, or
          // state stays inconsistent (`shuffled == false` yet the shuffled order
          // is loaded, and a repeat shuffle-off is a no-op, and a queue drain
          // resumes shuffled). Fall back to the available (partial) natural order
          // so the toggle is honored — whether or not the queue owns the sink.
          if session.shuffled || session.order == session.original {
            None
          } else {
            let old_index = session.index.min(session.order.len().saturating_sub(1));
            let restored = session.original.clone();
            // Anchor on the current track's rank so a duplicate doesn't jump.
            let new_index = reanchor_by_rank(&session.order, old_index, &restored);
            session.order = restored;
            session.index = new_index;
            if queue_active {
              // Suspended behind the queue: remap the resume index, don't reload.
              let same_track = matches!(suspended_resume, Some(Some(j)) if j <= old_index);
              let resume = if same_track {
                Some(new_index)
              } else {
                (new_index + 1 < session.order.len()).then_some(new_index + 1)
              };
              resume_update = Some(resume);
              None
            } else {
              session.pending_reload_index = Some(new_index);
              Some((session.order.clone(), new_index))
            }
          }
        }
        Ok((fetched, truncated)) => {
          if fetched.is_empty() {
            None
          } else if queue_active {
            // The queue owns the sink (session suspended). Fold the full
            // context into the app-owned order so the resume plays the whole
            // thing, but never reload over the queued track. The already-played
            // prefix is preserved, so `index` still points at the suspended
            // track and the resume math is unambiguous even with duplicates.
            let index = session.index.min(session.order.len().saturating_sub(1));
            let new_order = fold_full_context(&session.order, index, &fetched, session.shuffled);
            // Remap, don't clobber. The played prefix is preserved verbatim, so
            // any stored resume index within it (`j <= index`) maps to itself —
            // covering a same-track handoff (`j == index`) and a repeat-context
            // last-to-first wrap (`j == 0`) alike. A resume past the prefix (a
            // plain advance, or `None` when the partial order looked exhausted)
            // continues at the first not-yet-played track.
            let resume = match suspended_resume {
              Some(Some(j)) if j <= index => Some(j),
              _ => (index + 1 < new_order.len()).then_some(index + 1),
            };
            session.original = fetched;
            session.order = new_order;
            session.index = index;
            resume_update = Some(resume);
            None
          } else if !session.shuffled {
            // Shuffle is off (toggled off while the fetch ran): restore the
            // context's natural order and continue from the current track, so
            // Next follows the real tracklist. Anchor on the occurrence matching
            // the current track's rank among the tracks played so far, so a
            // duplicated track isn't reloaded at an earlier copy.
            let index = reanchor_by_rank(&session.order, session.index, &fetched);
            let changed = session.order != fetched;
            session.original = fetched.clone();
            session.order = fetched;
            session.index = index;
            if changed {
              session.pending_reload_index = Some(index);
              Some((session.order.clone(), index))
            } else {
              None
            }
          } else if same_track_multiset(&session.order, &fetched) {
            // The initial load already held the whole context; keep its
            // shuffle rather than re-rolling and reloading for nothing.
            session.original = fetched;
            None
          } else {
            // The played prefix is preserved, so the currently-playing track
            // stays fixed at `session.index`; anchor on it directly rather than
            // a duplicate-ambiguous URI lookup.
            let index = session.index.min(session.order.len().saturating_sub(1));
            let merged = merge_full_fetch(&session.order, index, &fetched);
            session.original = fetched;
            session.order = merged;
            session.index = index;
            if truncated {
              status = Some(format!(
                "Large context: shuffling the first {MAX_NATIVE_SHUFFLE_TRACKS} tracks"
              ));
            }
            session.pending_reload_index = Some(index);
            Some((session.order.clone(), index))
          }
        }
      }
    };
    if let Some(message) = status {
      guard.set_status_message(message, 4);
    }
    // Re-point the suspended resume index into the folded-in order so the
    // queue drains back into the full context, not the partial one.
    if let Some(resume) = resume_update {
      if let Some(crate::core::queue::SuspendedContext::SpotifyShuffled { resume_index, .. }) =
        guard.queue_suspended.as_mut()
      {
        *resume_index = resume;
      }
    }
    // The seed-only fetch failed while this session's suspension already sat
    // behind the queue: its shuffled resume dead-ends, so hand the drain to
    // the context route instead.
    if convert_suspend_to_context {
      guard.convert_shuffled_suspension_to_context(Some(generation));
    }
    reload.and_then(|(order, index)| player.map(|p| (p, order, index, seek_ms, start_playing)))
  };
  if let Some((player, order, index, seek_ms, start_playing)) = reload {
    if let Err(e) = load_session_tracks(&player, order, index, seek_ms, start_playing) {
      warn!("native shuffle: reload after full fetch failed: {e}");
      clear_pending_reload(app).await;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn uris(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| format!("spotify:track:{id}")).collect()
  }

  /// A queue suspension taken while the seed-only context fetch was still in
  /// flight stores a shuffled resume; when the fetch then fails for good, the
  /// failure handler must convert that suspension to the context route so the
  /// drain does not dead-end in "Queue finished".
  #[cfg(feature = "streaming")]
  #[test]
  fn failed_fetch_converts_a_suspended_seed_session_to_the_context() {
    use crate::core::app::NativeSpotifyShuffleSession;
    use crate::core::queue::SuspendedContext;

    let rt = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .unwrap();
    rt.block_on(async {
      let (tx, _rx) = std::sync::mpsc::channel();
      let mut app = App::new(
        tx,
        crate::core::user_config::UserConfig::new(),
        Some(std::time::SystemTime::now()),
      );
      app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
        order: uris(&["seed"]),
        original: uris(&["seed"]),
        index: 0,
        shuffled: true,
        fetch_complete: false,
        fetch_failed: false,
        generation: 9,
        pending_reload_index: None,
        pending_manual_skip: None,
      });
      // The suspend computed from the seed-only order: an exhausted resume.
      app.queue_suspended = Some(SuspendedContext::SpotifyShuffled {
        resume_index: None,
        generation: 9,
        context_uri: Some("spotify:playlist:suspended".to_string()),
        resume_track_uri: None,
      });
      let app = Arc::new(Mutex::new(app));

      finish_full_context_fetch(&app, 9, Err(anyhow!("fetch failed"))).await;

      let guard = app.lock().await;
      match &guard.queue_suspended {
        Some(SuspendedContext::Spotify { context_uri, .. }) => {
          assert_eq!(
            context_uri.as_deref(),
            Some("spotify:playlist:suspended"),
            "the context captured at suspension time must survive the conversion"
          );
        }
        other => panic!("expected a context suspension, got {other:?}"),
      }
      assert!(
        guard
          .native_spotify_shuffle
          .as_ref()
          .is_some_and(|s| s.fetch_failed),
        "the session must be marked failed for the suspend fallback"
      );
    });
  }

  #[test]
  fn shuffled_order_moves_selected_to_front_and_keeps_every_track() {
    let original = uris(&["a", "b", "c", "d", "e"]);
    let shuffled = shuffled_order(original.clone(), 3);
    assert_eq!(shuffled[0], "spotify:track:d");
    let mut a = original.clone();
    let mut b = shuffled.clone();
    a.sort();
    b.sort();
    assert_eq!(a, b, "shuffle must be a permutation");
  }

  #[test]
  fn shuffled_order_handles_out_of_range_and_tiny_lists() {
    assert!(shuffled_order(Vec::new(), 0).is_empty());
    assert_eq!(shuffled_order(uris(&["a"]), 5), uris(&["a"]));
  }

  #[test]
  fn merge_preserves_played_prefix_and_covers_the_rest_once() {
    // Initial lazy session: only the selected track was loaded and played.
    let order = uris(&["d"]);
    let fetched = uris(&["a", "b", "c", "d", "e"]);
    let merged = merge_full_fetch(&order, 0, &fetched);
    assert_eq!(merged[0], "spotify:track:d");
    assert_eq!(merged.len(), 5);
    let mut rest = merged[1..].to_vec();
    rest.sort();
    assert_eq!(rest, uris(&["a", "b", "c", "e"]));
  }

  #[test]
  fn nth_occurrence_resolves_by_duplicate_rank() {
    let list = uris(&["a", "b", "a", "c", "a"]);
    assert_eq!(nth_occurrence(&list, "spotify:track:a", 1), Some(0));
    assert_eq!(nth_occurrence(&list, "spotify:track:a", 2), Some(2));
    assert_eq!(nth_occurrence(&list, "spotify:track:a", 3), Some(4));
    // Fewer than `nth` copies falls back to the last occurrence.
    assert_eq!(nth_occurrence(&list, "spotify:track:a", 9), Some(4));
    assert_eq!(nth_occurrence(&list, "spotify:track:z", 1), None);
  }

  #[test]
  fn fold_with_natural_tail_keeps_context_order_for_shuffle_off() {
    // Shuffle toggled off mid-fetch: the played prefix stays put and the rest
    // follows in the fetched (natural) order, not a re-shuffle.
    let order = uris(&["c"]);
    let fetched = uris(&["a", "b", "c", "d", "e"]);
    let folded = fold_full_context(&order, 0, &fetched, false);
    assert_eq!(folded, uris(&["c", "a", "b", "d", "e"]));
  }

  #[test]
  fn merge_is_duplicate_safe() {
    // "d" appears twice in the playlist; the played copy consumes exactly one.
    let order = uris(&["d", "b"]);
    let fetched = uris(&["a", "b", "d", "d"]);
    let merged = merge_full_fetch(&order, 1, &fetched);
    assert_eq!(&merged[..2], &uris(&["d", "b"])[..]);
    assert_eq!(merged.len(), 4);
    let mut rest = merged[2..].to_vec();
    rest.sort();
    assert_eq!(rest, uris(&["a", "d"]));
  }

  #[test]
  fn merge_drops_played_tracks_no_longer_in_the_context() {
    // A played track was removed from the playlist between load and fetch:
    // the prefix keeps it (it already played), the rest covers the fetch.
    let order = uris(&["x", "b"]);
    let merged = merge_full_fetch(&order, 1, &uris(&["a", "b", "c"]));
    assert_eq!(&merged[..2], &uris(&["x", "b"])[..]);
    let mut rest = merged[2..].to_vec();
    rest.sort();
    assert_eq!(rest, uris(&["a", "c"]));
  }

  #[test]
  fn same_track_multiset_is_order_insensitive_but_count_sensitive() {
    assert!(same_track_multiset(
      &uris(&["a", "b", "b"]),
      &uris(&["b", "a", "b"])
    ));
    assert!(!same_track_multiset(&uris(&["a", "b"]), &uris(&["a", "a"])));
    assert!(!same_track_multiset(&uris(&["a"]), &uris(&["a", "a"])));
  }
}
