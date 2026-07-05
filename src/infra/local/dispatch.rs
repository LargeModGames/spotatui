//! Local-file playback routing.
//!
//! This is the seam that keeps the Spotify [`Network`](crate::infra::network)
//! Spotify-only: [`route_local_event`] is called from the runtime IoEvent pump
//! *before* `handle_network_event`. When the event targets local files (a
//! `file://` playback URI, a transport control while a local file owns the
//! session, or a browse request) it is handled here and consumed; otherwise it
//! falls through to the normal Spotify dispatch.
//!
//! ## Decoupling
//!
//! Local playback owns a single piece of state, [`App::local_playback`]. This
//! module never writes Spotify/librespot fields (`native_track_info`,
//! `song_progress_ms`, `is_streaming_active`, …): the playbar reads progress and
//! pause state live from the player, so the two playback worlds cannot desync.
//!
//! ## Device ownership
//!
//! Only one backend holds the audio output device at a time (required on
//! exclusive-ALSA setups, harmless elsewhere). Starting local playback pauses
//! native Spotify (librespot releases the device when its sink stops); starting
//! Spotify tears the local session down (dropping it releases the device).
//!
//! ## Publish-once
//!
//! `local_playback` is set exactly once, in the success arm of [`start_local_queue`],
//! *after* the source is decoding. While it is `None` neither the playbar nor
//! the runtime tick touch local state, so the brief "opening" window is simply
//! invisible — there is no half-initialised state for a tick to misread.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::{
  file_uri_to_path, next_index, prev_index, track_info_from_path, LocalPlaybackState, LocalSource,
};
use crate::core::app::{App, TrackTableContext};
use crate::core::source::MediaSource;
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;

/// Whether a URI is owned by the local-files source.
fn is_file_uri(uri: &str) -> bool {
  uri.starts_with("file:")
}

/// Intercept playback events that target local files.
///
/// Returns `true` if the event was handled locally (and must **not** be
/// forwarded to the Spotify network), `false` to let the normal dispatch run.
pub async fn route_local_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    // Browse: scan the configured music directory and a folder's tracks.
    IoEvent::GetLocalPlaylists => {
      load_local_playlists(app).await;
      true
    }
    IoEvent::GetLocalTracks(uri) => {
      load_local_tracks(app, uri).await;
      true
    }
    // Start playing a folder of local files: queue every track and start at the
    // selected offset. `on_enter` for a LocalPlaylist sends the whole folder
    // this way so Next/Previous/auto-advance have a queue to move through.
    IoEvent::StartPlayback(None, Some(uris), offset)
      if uris.first().is_some_and(|u| is_file_uri(u)) =>
    {
      start_local_queue(app, uris.clone(), offset.unwrap_or(0)).await;
      true
    }
    // Start playing a single local file (no surrounding folder context): a
    // one-track queue.
    IoEvent::StartPlayback(Some(uri), _, _) if is_file_uri(uri) => {
      start_local_queue(app, vec![uri.clone()], 0).await;
      true
    }
    // Bare "resume current" — ours only while a local file owns the session.
    IoEvent::StartPlayback(None, None, None) => match player(app).await {
      Some(player) => {
        player.resume();
        true
      }
      None => false,
    },
    // Any other start is a real Spotify play: relinquish the device first, then
    // let the network handle it.
    IoEvent::StartPlayback(..) => {
      teardown_local(app).await;
      false
    }
    IoEvent::PausePlayback => match player(app).await {
      Some(player) => {
        player.pause();
        true
      }
      None => false,
    },
    IoEvent::Seek(position_ms) => match player(app).await {
      Some(player) => {
        // The playbar reads position live from the player, so the seek shows up
        // on the next render with nothing else to update.
        let _ = player.seek(Duration::from_millis(*position_ms as u64));
        true
      }
      None => false,
    },
    IoEvent::ChangeVolume(volume) => match player(app).await {
      Some(player) => {
        player.set_volume(*volume as f32 / 100.0);
        // Keep the playbar's volume readout in sync.
        app.lock().await.user_config.behavior.volume_percent = *volume;
        true
      }
      None => false,
    },
    // Skip forward in the local queue. Also the target of the runner tick's
    // auto-advance dispatch and (via U3) OS media-key Next. Consumed whenever a
    // local file owns the session so it never reaches Spotify.
    IoEvent::NextTrack => skip(app, Direction::Next).await,
    // Skip backward. `ForcePreviousTrack` (restart-or-previous) behaves the same
    // here: there is no "restart current vs go back" distinction for local files.
    IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => skip(app, Direction::Prev).await,
    _ => false,
  }
}

/// Skip direction within the local queue.
#[derive(Clone, Copy)]
enum Direction {
  Next,
  Prev,
}

/// Move the local queue index in `direction` and play the new track.
///
/// Returns `true` if a local file owns the session (so the event is consumed
/// and never reaches Spotify), `false` otherwise. At a queue boundary the index
/// is clamped — the skip is a no-op but the event is still consumed.
async fn skip(app: &Arc<Mutex<App>>, direction: Direction) -> bool {
  // Read the target index under a short lock, then release it before the
  // blocking decode in `play_index`.
  let target = {
    let mut guard = app.lock().await;
    let Some(local) = guard.local_playback.as_mut() else {
      return false; // not ours — let Spotify handle it
    };
    // Mark a track change in progress so the runner tick does not mistake the
    // empty sink during the upcoming decode for end-of-track and fire a spurious
    // auto-advance. Cleared in `play_index`'s commit once the new source is in
    // the sink. (We can't unqueue an already-dispatched auto-advance, so a Next
    // pressed mid-advance may skip one extra track — benign and accepted.)
    local.advancing = true;
    match direction {
      Direction::Next => next_index(local.index, local.queue.len()),
      Direction::Prev => prev_index(local.index, local.queue.len()),
    }
  };

  match target {
    Some(idx) => play_index(app, idx).await,
    None => {
      // Boundary hit: the skip clamps to a no-op. Clear the guard we optimistically
      // set so it does not wedge auto-advance off for the rest of the track.
      if let Some(local) = app.lock().await.local_playback.as_mut() {
        local.advancing = false;
      }
    }
  }
  // Either way the event is ours and must not fall through to Spotify.
  true
}

/// The live local player, if a local file currently owns the session.
async fn player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app
    .lock()
    .await
    .local_playback
    .as_ref()
    .map(|local| Arc::clone(&local.player))
}

/// Begin playing a queue of local files, taking over the playback session and
/// starting at `start_idx` (clamped into range). Subsequent skips and
/// auto-advance reuse [`play_index`] against the queue published here.
async fn start_local_queue(app: &Arc<Mutex<App>>, queue: Vec<String>, start_idx: usize) {
  if queue.is_empty() {
    set_error(app, "No local tracks to play".to_string()).await;
    return;
  }
  let index = start_idx.min(queue.len() - 1);

  let path = match file_uri_to_path(&queue[index]) {
    Ok(path) => path,
    Err(e) => {
      set_error(app, format!("Invalid local file URI: {e}")).await;
      return;
    }
  };

  // Pause native Spotify so librespot releases the output device.
  #[cfg(feature = "streaming")]
  {
    let streaming = app.lock().await.streaming_player.clone();
    if let Some(player) = streaming {
      player.pause();
    }
  }

  // Tear down any Subsonic session (the `!handled_locally` short-circuit in the
  // runtime means the subsonic dispatch never sees this file:// start, so the
  // teardown must happen here — see infra::subsonic::dispatch device ownership).
  #[cfg(feature = "subsonic")]
  {
    let subsonic = app.lock().await.subsonic_playback.take();
    if let Some(subsonic) = subsonic {
      subsonic.player.stop();
    }
  }

  // Tear down any radio session, for the same short-circuit reason.
  #[cfg(feature = "internet-radio")]
  {
    let radio = app.lock().await.radio_playback.take();
    if let Some(radio) = radio {
      radio.player.stop();
    }
  }

  // Tear down any YouTube session, for the same short-circuit reason.
  #[cfg(feature = "youtube")]
  {
    let youtube = app.lock().await.youtube_playback.take();
    if let Some(youtube) = youtube {
      youtube.player.stop();
    }
  }

  let player = match acquire_player(app).await {
    Some(player) => player,
    None => return, // error already surfaced
  };

  // Tag reading and decoder construction are blocking file I/O — keep them off
  // the async executor.
  let decode_path = path.clone();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || {
    let info = track_info_from_path(&decode_path);
    decode_player.play_file(&decode_path).map(|()| info)
  })
  .await;

  match result {
    Ok(Ok(info)) => {
      let volume = app.lock().await.user_config.behavior.volume_percent;
      player.set_volume(volume as f32 / 100.0);

      // Publish the session exactly once, now that the source is decoding.
      // Publish-once covers the empty-sink race here: `local_playback` is `None`
      // throughout the decode above, so `advancing` starts `false`.
      let display_name = info.name.clone();
      let mut app = app.lock().await;
      app.local_playback = Some(LocalPlaybackState {
        player,
        queue,
        index,
        name: info.name,
        artists: info.artists.join(", "),
        album: info.album,
        duration_ms: info.duration_ms,
        advancing: false,
      });
      app.set_status_message(format!("\u{266a} {display_name}"), 4);
    }
    Ok(Err(e)) => set_error(app, format!("Cannot play local file: {e}")).await,
    Err(e) => set_error(app, format!("Local playback task failed: {e}")).await,
  }
}

/// Decode and play a single local file on `player`, returning its metadata.
/// Extracted so the native queue engine can play a one-off `file://` track on a
/// borrowed player without touching `local_playback`. Mirrors the core of
/// [`play_index`] (tag read + blocking decode) with no session bookkeeping.
#[cfg_attr(not(feature = "local-files"), allow(dead_code))]
pub(crate) async fn play_single_file(
  player: &Arc<LocalPlayer>,
  uri: &str,
) -> anyhow::Result<crate::core::plugin_api::TrackInfo> {
  let path = file_uri_to_path(uri)?;
  let decode_player = Arc::clone(player);
  let info = tokio::task::spawn_blocking(move || {
    let info = track_info_from_path(&path);
    decode_player.play_file(&path).map(|()| info)
  })
  .await??;
  Ok(info)
}

/// Play the queued track at `target`, reusing the already-published session's
/// player and queue. Used by Next/Previous and the runner tick's auto-advance.
///
/// The index is committed to `target` in **both** the success and failure arms:
/// on a decode failure the sink drains and the runner tick auto-advances from
/// `target` to the *following* track, so a single corrupt file is skipped past
/// rather than retried forever. `advancing` is cleared in both arms once the new
/// source is in the sink (or the play failed), reopening auto-advance.
pub(crate) async fn play_index(app: &Arc<Mutex<App>>, target: usize) {
  // Snapshot the player + URI under a short lock.
  let (player, uri) = {
    let mut guard = app.lock().await;
    let Some(local) = guard.local_playback.as_mut() else {
      return; // session torn down between dispatch and here
    };
    match local.queue.get(target) {
      Some(uri) => (Arc::clone(&local.player), uri.clone()),
      None => {
        // Out of range — nothing to play. The caller (skip/auto-advance)
        // optimistically set `advancing`; clear it here so this dead-end does
        // not wedge auto-advance off for the rest of the session.
        local.advancing = false;
        return;
      }
    }
  };

  let path = match file_uri_to_path(&uri) {
    Ok(path) => path,
    Err(e) => {
      // Commit the index so the tick advances past this entry, then surface.
      commit_index(app, target, None).await;
      set_error(app, format!("Invalid local file URI: {e}")).await;
      return;
    }
  };

  // Blocking tag read + decode off the executor.
  let decode_path = path.clone();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || {
    let info = track_info_from_path(&decode_path);
    decode_player.play_file(&decode_path).map(|()| info)
  })
  .await;

  match result {
    Ok(Ok(info)) => {
      let display_name = info.name.clone();
      commit_index(app, target, Some(info)).await;
      app
        .lock()
        .await
        .set_status_message(format!("\u{266a} {display_name}"), 4);
    }
    Ok(Err(e)) => {
      // `play_file` clears the sink before any fallible step, so on a normal
      // decode/open error the sink is already empty and the runner tick
      // auto-advances past this bad track. Stop the player anyway as
      // defense-in-depth: it is idempotent and guarantees no stale audio even if
      // the blocking closure failed *before* reaching `play_file`.
      player.stop();
      commit_index(app, target, None).await;
      set_error(app, format!("Cannot play local file: {e}")).await;
    }
    Err(e) => {
      // The blocking task panicked (e.g. inside the tag read) *before*
      // `play_file` could clear the sink, so the old track may still be playing.
      // Stop it explicitly so the empty sink lets the tick auto-advance instead
      // of dead-ending on a stale track with a silently moved index.
      player.stop();
      commit_index(app, target, None).await;
      set_error(app, format!("Local playback task failed: {e}")).await;
    }
  }
}

/// Commit `target` as the live index and clear the auto-advance guard. On a
/// successful play, also refresh the displayed track metadata; on failure leave
/// the previous metadata in place (the empty sink + moved index lets the tick
/// carry on past the bad track).
async fn commit_index(
  app: &Arc<Mutex<App>>,
  target: usize,
  info: Option<crate::core::plugin_api::TrackInfo>,
) {
  let mut guard = app.lock().await;
  if let Some(local) = guard.local_playback.as_mut() {
    local.index = target;
    local.advancing = false;
    if let Some(info) = info {
      local.name = info.name;
      local.artists = info.artists.join(", ");
      local.album = info.album;
      local.duration_ms = info.duration_ms;
    }
  }
}

/// Reuse the live player if a local file is already playing, otherwise open the
/// output device for a fresh one. A freshly opened player is **not** published
/// to `App` here — [`start_local_queue`] publishes it only on success, so there
/// is no window where `local_playback` is `Some` with an empty sink.
async fn acquire_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(player) = player(app).await {
    return Some(player);
  }

  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(player)) => Some(Arc::new(player)),
    Ok(Err(e)) => {
      set_error(app, format!("No audio output for local playback: {e}")).await;
      None
    }
    Err(e) => {
      set_error(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

/// End the local session, releasing the output device.
async fn teardown_local(app: &Arc<Mutex<App>>) {
  if let Some(local) = app.lock().await.local_playback.take() {
    local.player.stop();
    // `local` is dropped here; if it held the last reference the keepalive
    // thread exits and the output device is released.
  }
}

async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}

/// The configured music-library root, or `None` (with a status message) if it
/// is unset.
async fn music_root(app: &Arc<Mutex<App>>) -> Option<String> {
  let root = app
    .lock()
    .await
    .user_config
    .behavior
    .local_music_path
    .clone();
  if root.is_none() {
    set_error(
      app,
      "No local music folder configured (set behavior.local_music_path)".to_string(),
    )
    .await;
  }
  root
}

/// Scan the music root's immediate subdirectories into `app.local_playlists`.
///
/// `LocalSource`'s methods are async but do blocking filesystem I/O, so they run
/// on the blocking pool (via `block_on`) rather than stalling the executor.
async fn load_local_playlists(app: &Arc<Mutex<App>>) {
  let Some(root) = music_root(app).await else {
    return;
  };
  let result = tokio::task::spawn_blocking(move || {
    futures::executor::block_on(LocalSource::new(root).playlists())
  })
  .await;
  match result {
    Ok(Ok(playlists)) => {
      let mut app = app.lock().await;
      app.local_playlists = playlists;
      app.local_playlists_index = 0;
    }
    Ok(Err(e)) => set_error(app, format!("Cannot scan music folder: {e}")).await,
    Err(e) => set_error(app, format!("Local folder scan failed: {e}")).await,
  }
}

/// Scan a folder's audio files into the shared track table (tagged as
/// [`TrackTableContext::LocalPlaylist`] so selecting a row plays the file).
async fn load_local_tracks(app: &Arc<Mutex<App>>, playlist_uri: &str) {
  let Some(root) = music_root(app).await else {
    return;
  };
  let uri = playlist_uri.to_string();
  let result = tokio::task::spawn_blocking(move || {
    futures::executor::block_on(LocalSource::new(root).tracks(&uri))
  })
  .await;
  match result {
    Ok(Ok(tracks)) => {
      let mut app = app.lock().await;
      app.track_table.tracks = tracks;
      app.track_table.selected_index = 0;
      app.track_table.context = Some(TrackTableContext::LocalPlaylist);
    }
    Ok(Err(e)) => set_error(app, format!("Cannot read folder: {e}")).await,
    Err(e) => set_error(app, format!("Local track scan failed: {e}")).await,
  }
}
