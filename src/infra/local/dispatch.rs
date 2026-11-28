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
//! exclusive-ALSA setups, harmless elsewhere). Starting local playback pauses or
//! parks native Spotify (librespot releases the device when its sink stops); starting
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

use super::{file_uri_to_path, track_info_from_path, LocalPlaybackState, LocalSource};
use crate::core::app::{App, TrackTableContext};
use crate::core::source::{MediaSource, Source};
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;
use crate::infra::queue::{advance_index, replay_file, restage, ResumePoint};

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
    IoEvent::GetLocalTracks(uri, epoch) => {
      load_local_tracks(app, uri, *epoch).await;
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
    #[cfg(feature = "streaming")]
    IoEvent::ReplayStartPlayback(request) => {
      // A dead replay (expired, or its request cancelled or superseded in
      // flight) must not stop the local session; the network handler applies
      // the same rule.
      if app.lock().await.retire_expired_replay(request) {
        return true;
      }
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
        player.set_volume(*volume);
        // Keep the playbar's volume readout in sync.
        let mut app = app.lock().await;
        app.runtime_state.volume_percent = *volume;
        app.schedule_state_save(crate::core::state::PersistedRuntimeState::volume_percent(
          *volume,
        ));
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
    // Repeat-one auto-advance: replay the current track (dispatched by the runner
    // tick only while a local file owns the session).
    IoEvent::ReplayCurrentTrack => replay_current(app).await,
    // Explicit repeat-mode set (e.g. a plugin's `set_repeat`): store it as the
    // decoded repeat mode while a queueable decoded source owns playback (the
    // method's ownership gate excludes a context suspended under the native
    // queue), so it never reaches the Spotify context.
    IoEvent::Repeat(state) => app.lock().await.set_decoded_repeat_from_state(*state),
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
    // Repeat-all wraps at the ends; Off/Repeat-one clamp (manual skip advances
    // normally under repeat-one — only *auto* advance replays).
    let mode = guard.decoded_repeat;
    let Some(local) = guard.local_playback.as_mut() else {
      return false; // not ours — let Spotify handle it
    };
    // Mark a track change in progress so the runner tick does not mistake the
    // empty sink during the upcoming decode for end-of-track and fire a spurious
    // auto-advance. Cleared in `play_index`'s commit once the new source is in
    // the sink. (We can't unqueue an already-dispatched auto-advance, so a Next
    // pressed mid-advance may skip one extra track — benign and accepted.)
    local.advancing = true;
    let forward = matches!(direction, Direction::Next);
    advance_index(local.index, local.queue.len(), mode, forward)
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

/// Replay the current local track (repeat-one auto-advance) via [`replay_file`];
/// a drained rodio sink cannot seek, so the file is re-decoded from disk. On any
/// failure (bad index/URI or decode error) the session is **torn down** rather
/// than left with an empty sink — otherwise the runner tick would see
/// end-of-track again next tick and re-fire replay forever.
///
/// Returns `true` when a local file owns the session (event consumed), `false`
/// otherwise.
async fn replay_current(app: &Arc<Mutex<App>>) -> bool {
  let target = {
    let mut guard = app.lock().await;
    let Some(local) = guard.local_playback.as_mut() else {
      return false; // not ours — let Spotify handle it
    };
    local.advancing = true;
    let resume = local.resume_at.take().or(Some(ResumePoint {
      position_ms: 0,
      paused: local.player.is_paused(),
    }));
    local
      .queue
      .get(local.index)
      .and_then(|uri| file_uri_to_path(uri).ok())
      .map(|path| (Arc::clone(&local.player), path, resume))
  };
  let replayed = match target {
    Some((player, path, resume)) => replay_file(player, path, resume).await,
    None => false,
  };
  if replayed {
    // Same track, same metadata: just clear the in-flight guard. The track did
    // play, so forget earlier failures like any other successful play — a stale
    // count would otherwise tear a later run down early.
    if let Some(local) = app.lock().await.local_playback.as_mut() {
      local.advancing = false;
      local.failed_since_played.clear();
    }
  } else {
    // Take under the lock, stop off it: a sink clear waits for the audio thread.
    let ended = app.lock().await.local_playback.take();
    if let Some(local) = ended {
      Arc::clone(&local.player).stop_detached_holding(local);
    }
    set_error(
      app,
      crate::infra::app_strings(app)
        .await
        .status
        .infra
        .local_replay_failed
        .to_string(),
    )
    .await;
  }
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
    set_error(
      app,
      crate::infra::app_strings(app)
        .await
        .status
        .infra
        .local_no_tracks
        .to_string(),
    )
    .await;
    return;
  }
  let index = start_idx.min(queue.len() - 1);

  let path = match file_uri_to_path(&queue[index]) {
    Ok(path) => path,
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_uri_invalid(&e.to_string()),
      )
      .await;
      return;
    }
  };

  app.lock().await.claim_decoded_sink(Source::Local);

  // Take the sink from native Spotify so no rebuild resumes it under this
  // source.
  #[cfg(feature = "streaming")]
  app.lock().await.release_native_for_decoded();

  // The other decoded sources never see this file:// start (the pump's
  // `!handled_locally` short-circuit), so their sessions are torn down here.
  let players = app.lock().await.take_decoded_sessions_except(Source::Local);
  for player in players {
    player.stop_detached();
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
      let volume = app.lock().await.runtime_state.volume_percent;
      player.set_volume(volume);

      // Publish the session exactly once, now that the source is decoding.
      // Publish-once covers the empty-sink race here: `local_playback` is `None`
      // throughout the decode above, so `advancing` starts `false`.
      let mut app = app.lock().await;
      let display_name = info.display_name(app.strings()).to_owned();
      let mut state = LocalPlaybackState {
        player,
        queue,
        index,
        name: info.name,
        generated_name: info.generated_name,
        artists: info.artists.join(", "),
        album: info.album,
        duration_ms: info.duration_ms,
        advancing: false,
        shuffle_backup: None,
        failed_since_played: std::collections::HashSet::new(),
        resume_at: None,
      };
      // Honor the player-global decoded shuffle for the freshly-built queue so it
      // persists across contexts like Spotify (the just-decoded start track stays
      // playing, moved to the front).
      if app.decoded_shuffle {
        state.set_shuffle(true);
      }
      app.local_playback = Some(state);
      app.set_status_message(format!("\u{266a} {display_name}"), 4);
    }
    Ok(Err(e)) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_play_failed(&e.to_string()),
      )
      .await
    }
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_task_failed(&e.to_string()),
      )
      .await
    }
  }
}

/// Decode and stage a single local file on `player`, returning its metadata;
/// the caller starts it. Extracted so the native queue engine can play a
/// one-off `file://` track on a borrowed player without touching
/// `local_playback`. Mirrors the core of [`play_index`] (tag read + blocking
/// decode) with no session bookkeeping.
#[cfg_attr(not(feature = "local-files"), allow(dead_code))]
pub(crate) async fn stage_single_file(
  player: &Arc<LocalPlayer>,
  uri: &str,
) -> anyhow::Result<crate::core::plugin_api::TrackInfo> {
  let path = file_uri_to_path(uri)?;
  let decode_player = Arc::clone(player);
  let info = tokio::task::spawn_blocking(move || {
    let info = track_info_from_path(&path);
    decode_player.stage_file(&path).map(|()| info)
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
  let (player, uri, resume) = {
    let mut guard = app.lock().await;
    let Some(local) = guard.local_playback.as_mut() else {
      return; // session torn down between dispatch and here
    };
    match local.queue.get(target) {
      Some(uri) => (
        Arc::clone(&local.player),
        uri.clone(),
        local.resume_at.take(),
      ),
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
      fail_index(
        app,
        target,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_uri_invalid(&e.to_string()),
      )
      .await;
      return;
    }
  };

  // Blocking tag read + decode off the executor.
  let decode_path = path.clone();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || {
    let info = track_info_from_path(&decode_path);
    restage(&decode_player, &decode_path, resume).map(|()| info)
  })
  .await;

  match result {
    Ok(Ok(info)) => {
      let mut guard = app.lock().await;
      let display_name = info.display_name(guard.strings()).to_owned();
      commit_index(&mut guard, target, info);
      guard.set_status_message(format!("\u{266a} {display_name}"), 4);
    }
    Ok(Err(e)) => {
      // `stage_file` clears the sink before any fallible step, so on a normal
      // decode/open error the sink is already empty and the runner tick
      // auto-advances past this bad track. Stop the player anyway as
      // defense-in-depth: it is idempotent and guarantees no stale audio even if
      // the blocking closure failed *before* reaching `stage_file`.
      Arc::clone(&player).stop_detached();
      fail_index(
        app,
        target,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_play_failed(&e.to_string()),
      )
      .await;
    }
    Err(e) => {
      // The blocking task panicked (e.g. inside the tag read) *before*
      // `stage_file` could clear the sink, so the old track may still be playing.
      // Stop it explicitly so the empty sink lets the tick auto-advance instead
      // of dead-ending on a stale track with a silently moved index.
      Arc::clone(&player).stop_detached();
      fail_index(
        app,
        target,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_task_failed(&e.to_string()),
      )
      .await;
    }
  }
}

/// Commit `target` as the live index and clear the auto-advance guard. On a
/// successful play, also refresh the displayed track metadata; on failure leave
/// the previous metadata in place (the empty sink + moved index lets the tick
/// carry on past the bad track).
///
fn commit_index(guard: &mut App, target: usize, info: crate::core::plugin_api::TrackInfo) {
  if let Some(local) = guard.local_playback.as_mut() {
    local.index = target;
    local.advancing = false;
    local.failed_since_played.clear();
    local.name = info.name;
    local.generated_name = info.generated_name;
    local.artists = info.artists.join(", ");
    local.album = info.album;
    local.duration_ms = info.duration_ms;
  }
}

/// Record a failed play of `target`: move the index on and clear the
/// auto-advance guard so the tick carries past the bad track (the empty sink +
/// moved index), leave the previous metadata in place, and surface `msg`.
///
/// Once *every* track in the queue has failed since the last successful play the
/// session ends instead — see
/// [`failed_since_played`](LocalPlaybackState::failed_since_played).
///
/// That teardown takes the session out under the **same lock** that observed the
/// exhaustion, and leaves `advancing` latched rather than clearing it. Clearing
/// the guard and releasing the lock first would leave a window where a runner
/// tick sees a live session with an empty sink, decides `AdvanceContext`, and
/// dispatches `NextTrack`; by the time that event is routed the session is gone,
/// so it falls through to Spotify and skips a track on the user's real device.
async fn fail_index(app: &Arc<Mutex<App>>, target: usize, msg: String) {
  let exhausted = {
    let mut guard = app.lock().await;
    let Some(local) = guard.local_playback.as_mut() else {
      return;
    };
    local.index = target;
    local.failed_since_played.insert(target);
    if local.failed_since_played.len() < local.queue.len() {
      local.advancing = false;
      None
    } else {
      guard.local_playback.take()
    }
  };

  match exhausted {
    Some(local) => {
      Arc::clone(&local.player).stop_detached_holding(local);
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_exhausted(&msg),
      )
      .await;
    }
    None => set_error(app, msg).await,
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
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_audio_unavailable(&e.to_string()),
      )
      .await;
      None
    }
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .audio_init_failed(&e.to_string()),
      )
      .await;
      None
    }
  }
}

/// End the local session, releasing the output device.
async fn teardown_local(app: &Arc<Mutex<App>>) {
  // Take under the lock, stop off it: a sink clear waits for the audio thread.
  let session = app.lock().await.local_playback.take();
  if let Some(local) = session {
    // If `local` held the last reference the keepalive thread exits and the
    // output device is released.
    Arc::clone(&local.player).stop_detached_holding(local);
  }
}

async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}

/// The configured music-library root, or `None` (with a status message) if it
/// is unset.
fn music_root(app: &mut App) -> Option<String> {
  let root = app.user_config.behavior.local_music_path.clone();
  if root.is_none() {
    let message = app.strings().status.infra.local_no_folder;
    app.set_status_message(message, 6);
  }
  root
}

/// Scan the music root's folders into `app.local_playlists()`.
///
/// `LocalSource`'s methods are async but do blocking filesystem I/O, so they run
/// on the blocking pool (via `block_on`) rather than stalling the executor.
async fn load_local_playlists(app: &Arc<Mutex<App>>) {
  let Some(root) = music_root(&mut *app.lock().await) else {
    return;
  };
  let result = tokio::task::spawn_blocking(move || {
    futures::executor::block_on(LocalSource::new(root).playlists())
  })
  .await;
  match result {
    Ok(Ok(playlists)) => {
      let mut app = app.lock().await;
      *app.local_playlists_mut() = playlists;
      app.view.local_playlists_index = 0;
    }
    Ok(Err(e)) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_scan_failed(&e.to_string()),
      )
      .await
    }
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .local_scan_task_failed(&e.to_string()),
      )
      .await
    }
  }
}

/// Scan a folder's audio files into the shared track table (tagged as
/// [`TrackTableContext::LocalPlaylist`] so selecting a row plays the file).
async fn load_local_tracks(app: &Arc<Mutex<App>>, playlist_uri: &str, epoch: u64) {
  let root = {
    let mut app = app.lock().await;
    if !app.is_current_source_track_table_load(epoch) {
      return;
    }
    music_root(&mut app)
  };
  let Some(root) = root else {
    return;
  };
  let uri = playlist_uri.to_string();
  let result = tokio::task::spawn_blocking(move || {
    futures::executor::block_on(LocalSource::new(root).tracks(&uri))
  })
  .await;
  let mut app = app.lock().await;
  if !app.is_current_source_track_table_load(epoch) {
    return;
  }
  let strings = app.strings();
  let message = match result {
    Ok(Ok(tracks)) => {
      app.set_source_track_table(playlist_uri, tracks, TrackTableContext::LocalPlaylist);
      return;
    }
    Ok(Err(e)) => strings.status.infra.local_read_failed(&e.to_string()),
    Err(e) => strings
      .status
      .infra
      .local_tracks_scan_failed(&e.to_string()),
  };
  app.set_status_message(message, 6);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  /// An app whose configured music folder is `root`.
  fn app_with_music_root(root: &std::path::Path) -> Arc<Mutex<App>> {
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.local_music_path = Some(root.to_string_lossy().into_owned());
    Arc::new(Mutex::new(App::new(tx, config, Some(SystemTime::now()))))
  }

  #[tokio::test]
  async fn get_local_playlists_lists_the_music_folders() {
    let dir = tempfile::tempdir().unwrap();
    let album = dir.path().join("Album");
    std::fs::create_dir(&album).unwrap();
    super::super::tests::write_wav(&album.join("song.wav"), 44100, 200);
    let app = app_with_music_root(dir.path());

    assert!(route_local_event(&app, &IoEvent::GetLocalPlaylists).await);

    let app = app.lock().await;
    let names: Vec<&str> = app
      .local_playlists()
      .iter()
      .map(|p| p.name.as_str())
      .collect();
    assert_eq!(names, ["Album"]);
  }

  #[tokio::test]
  async fn get_local_tracks_fills_the_track_table_with_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let album = dir.path().join("Album");
    std::fs::create_dir(&album).unwrap();
    super::super::tests::write_wav(&album.join("song.wav"), 44100, 200);
    let (tx, rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.local_music_path = Some(dir.path().to_string_lossy().into_owned());
    let app = Arc::new(Mutex::new(App::new(tx, config, Some(SystemTime::now()))));
    let uri = super::super::path_to_file_uri(&album);
    app.lock().await.open_source_playlist_tracks(uri);
    let event = rx
      .try_recv()
      .expect("opening the folder dispatches its load");

    assert!(route_local_event(&app, &event).await);

    let app = app.lock().await;
    assert_eq!(app.track_table.tracks.len(), 1);
    assert!(app.track_table.tracks[0].is_local);
    assert!(matches!(
      app.track_table.context,
      Some(TrackTableContext::LocalPlaylist)
    ));
  }
}

#[cfg(all(test, feature = "streaming"))]
mod replay_routing_tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn test_app() -> Arc<Mutex<App>> {
    let (tx, _rx) = channel();
    Arc::new(Mutex::new(App::new(
      tx,
      UserConfig::new(),
      Some(SystemTime::now()),
    )))
  }

  /// A dead replay (expired in a rate-limit window, or superseded in flight)
  /// is swallowed here: with a local session live, this consume is what keeps
  /// the file playing — the event must never reach the Spotify handler.
  #[tokio::test]
  async fn a_dead_replay_is_consumed_before_it_reaches_spotify() {
    let app = test_app();
    let request = {
      let mut guard = app.lock().await;
      guard.park_start_playback(Some("spotify:playlist:p".to_string()), None, None, None);
      let mut request = guard.pending_start_playback.clone().unwrap();
      request.pressed_at = std::time::Instant::now()
        - crate::core::app::max_parked_start_age()
        - std::time::Duration::from_secs(5);
      guard.pending_start_playback = Some(request.clone());
      request
    };

    assert!(route_local_event(&app, &IoEvent::ReplayStartPlayback(request)).await);
    assert!(app.lock().await.pending_start_playback.is_none());
  }

  /// A live replay takes the session over: the local teardown runs and the
  /// event falls through to the network handler.
  #[tokio::test]
  async fn a_live_replay_relinquishes_the_local_session() {
    let app = test_app();
    let request = {
      let mut guard = app.lock().await;
      guard.park_start_playback(Some("spotify:playlist:p".to_string()), None, None, None);
      guard.pending_start_playback.clone().unwrap()
    };

    assert!(!route_local_event(&app, &IoEvent::ReplayStartPlayback(request)).await);
  }

  /// The full late-replay hand: a local session published before a held
  /// replay keeps playing when the replay finally arrives dead.
  #[tokio::test]
  #[ignore = "needs an audio output device"]
  async fn a_late_replay_leaves_the_local_session_playing() {
    let app = test_app();
    {
      let mut guard = app.lock().await;
      guard.local_playback = Some(LocalPlaybackState {
        player: Arc::new(LocalPlayer::new().expect("audio output device")),
        queue: vec!["file:///tmp/track.mp3".to_string()],
        index: 0,
        name: "Song".to_string(),
        generated_name: None,
        artists: String::new(),
        album: String::new(),
        duration_ms: 1_000,
        advancing: false,
        shuffle_backup: None,
        failed_since_played: std::collections::HashSet::new(),
        resume_at: None,
      });
      guard.park_start_playback(Some("spotify:playlist:p".to_string()), None, None, None);
      let mut request = guard.pending_start_playback.clone().unwrap();
      request.pressed_at = std::time::Instant::now()
        - crate::core::app::max_parked_start_age()
        - std::time::Duration::from_secs(5);
      guard.pending_start_playback = Some(request);
    }

    // `route_local_event` re-locks `app`, so the replay runs off the guard.
    let request = app
      .lock()
      .await
      .pending_start_playback
      .clone()
      .expect("the aged replay is still parked");
    assert!(route_local_event(&app, &IoEvent::ReplayStartPlayback(request)).await);
    assert!(
      app.lock().await.local_playback.is_some(),
      "the local session keeps playing"
    );
  }
}
