//! Native queue playback routing.
//!
//! [`route_queue_event`] is wired **first** in the runtime IoEvent pump (before
//! the per-source dispatchers). It owns [`IoEvent::AdvanceNativeQueue`] and the
//! transport controls for the queue slot's player, and it relinquishes the queue
//! slot when the user starts an unrelated playback.
//!
//! Compiled unconditionally; the decoded-playback bodies are gated per source
//! feature. In a slim build the engine reduces to "skip every item with a
//! `not available in this build` status" — correct, and enough for the pump to
//! stay one shape across builds.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::core::app::App;
use crate::core::plugin_api::TrackInfo;
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "youtube",
  feature = "streaming"
))]
use crate::core::queue::QueueItemSource;
use crate::core::queue::{queue_item_source, source_available, source_label};
use crate::infra::network::IoEvent;

// The decoded queue slot exists only for the sources that own a finite track
// list; internet radio enables `audio-decode` but is never queueable.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
use crate::infra::audio::LocalPlayer;
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
use std::time::Duration;

/// Intercept queue-owned events before the per-source dispatchers.
///
/// Returns `true` when the event was consumed and must **not** be forwarded,
/// `false` to let the normal dispatch run. [`IoEvent::StartPlayback`] variants
/// return `false` (the per-source teardowns/starts still run) but first clear
/// the queue slot so a new play cleanly takes over.
pub async fn route_queue_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  if let IoEvent::AdvanceNativeQueue = event {
    advance_native_queue(app).await;
    return true;
  }

  // Transport for the queue slot's own player (Pause / Seek / Volume / Next /
  // bare-resume). Only meaningful when a decoded queued track owns the sink;
  // compiles out entirely without a queueable decoded source.
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  if let Some(handled) = route_queue_transport(app, event).await {
    return handled;
  }

  #[cfg(feature = "streaming")]
  if let Some(handled) = route_spotify_queue_transport(app, event).await {
    return handled;
  }

  // An explicit new-playback start relinquishes the queue slot (keeping the
  // queued items) so the per-source teardowns/starts run against a clean state.
  if matches!(
    event,
    IoEvent::StartPlayback(Some(_), _, _) | IoEvent::StartPlayback(_, Some(_), _)
  ) {
    clear_queue_playback(app).await;
  }
  false
}

/// Transport controls for the queue slot's player, when a decoded queued track
/// owns the sink. Returns `Some(true)` when consumed, `None` when this event is
/// not a queue-slot transport control (so the caller falls through).
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
async fn route_queue_transport(app: &Arc<Mutex<App>>, event: &IoEvent) -> Option<bool> {
  let player = {
    let guard = app.lock().await;
    guard.queue_now_decoded_player().map(Arc::clone)
  }?;
  match event {
    IoEvent::PausePlayback => {
      player.pause();
      Some(true)
    }
    // Bare "resume current" while the queue owns playback resumes the queue slot.
    IoEvent::StartPlayback(None, None, None) => {
      player.resume();
      Some(true)
    }
    IoEvent::Seek(position_ms) => {
      let _ = player.seek(Duration::from_millis(*position_ms as u64));
      Some(true)
    }
    IoEvent::ChangeVolume(volume) => {
      player.set_volume(*volume as f32 / 100.0);
      app.lock().await.user_config.behavior.volume_percent = *volume;
      Some(true)
    }
    // Skip the queued track: advance to the next queued item (or resume).
    IoEvent::NextTrack => {
      drop(player);
      advance_native_queue(app).await;
      Some(true)
    }
    // A forward-only queue has no "previous"; restart the current queued track.
    IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => {
      let _ = player.seek(Duration::from_millis(0));
      Some(true)
    }
    _ => None,
  }
}

/// Transport controls for a queued Spotify track playing through librespot.
#[cfg(feature = "streaming")]
async fn route_spotify_queue_transport(app: &Arc<Mutex<App>>, event: &IoEvent) -> Option<bool> {
  let is_spotify_slot = { app.lock().await.queue_now_is_spotify() };
  if !is_spotify_slot {
    return None;
  }
  match event {
    IoEvent::NextTrack => {
      advance_native_queue(app).await;
      Some(true)
    }
    IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => {
      if let Some(player) = { app.lock().await.streaming_player.clone() } {
        player.seek(0);
      }
      Some(true)
    }
    _ => None,
  }
}

/// Drop the queue slot (stopping its player) and forget any suspended context,
/// but keep the queued items. Called when the user starts an unrelated playback.
async fn clear_queue_playback(app: &Arc<Mutex<App>>) {
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  {
    let player = {
      let mut guard = app.lock().await;
      guard.queue_suspended = None;
      guard.take_queue_now_decoded_player()
    };
    if let Some(player) = player {
      player.stop();
    }
  }
  #[cfg(all(
    feature = "streaming",
    not(any(feature = "local-files", feature = "subsonic", feature = "youtube"))
  ))]
  {
    let mut guard = app.lock().await;
    guard.queue_suspended = None;
    guard.queue_now = None;
  }
  // No queueable source at all (includes a radio-only build): there is no queue
  // slot to clear, and `queue_suspended` is only ever set by a source that can
  // be suspended *under* the queue.
  #[cfg(not(any(
    feature = "streaming",
    feature = "local-files",
    feature = "subsonic",
    feature = "youtube"
  )))]
  {
    let _ = app;
  }
}

// ---------------------------------------------------------------------------
// Advance
// ---------------------------------------------------------------------------

/// Pop the head of the native queue and play it, skipping unplayable items with
/// a status message (bounded by the queue length — never unbounded recursion).
/// When the queue drains, resume the suspended context (or finish).
async fn advance_native_queue(app: &Arc<Mutex<App>>) {
  loop {
    let track = {
      let mut guard = app.lock().await;
      if guard.native_queue.is_empty() {
        None
      } else {
        Some(guard.native_queue.remove(0))
      }
    };
    let Some(track) = track else {
      resume_or_finish(app).await;
      return;
    };
    if try_play_queued(app, &track).await {
      return; // now playing this track
    }
    // Unplayable / skipped — loop to the next item.
  }
}

/// Try to play one queued track. Returns `true` if it is now playing, `false`
/// if it was skipped (feature off, no URI, download/decode error) — the caller
/// then advances to the next item.
async fn try_play_queued(app: &Arc<Mutex<App>>, track: &TrackInfo) -> bool {
  let Some(uri) = track.uri.clone() else {
    set_status(app, "Skipped a queued track with no URI".to_string()).await;
    return false;
  };
  let source = queue_item_source(&uri);
  if !source_available(source) {
    set_status(
      app,
      format!(
        "{} playback isn't available in this build",
        source_label(source)
      ),
    )
    .await;
    return false;
  }
  match source {
    #[cfg(feature = "local-files")]
    QueueItemSource::LocalFile => play_queued_local(app, track, &uri).await,
    #[cfg(feature = "subsonic")]
    QueueItemSource::Subsonic => play_queued_subsonic(app, track, &uri).await,
    #[cfg(feature = "youtube")]
    QueueItemSource::YouTube => play_queued_youtube(app, track, &uri).await,
    #[cfg(feature = "streaming")]
    QueueItemSource::Spotify => play_queued_spotify(app, track, &uri).await,
    // Reached only when a source is `source_available` but its play arm is
    // cfg'd out — impossible (the check above *is* the cfg gate), but the match
    // must be exhaustive across builds.
    #[allow(unreachable_patterns)]
    _ => {
      set_status(
        app,
        format!(
          "{} playback isn't available in this build",
          source_label(source)
        ),
      )
      .await;
      false
    }
  }
}

// ---------------------------------------------------------------------------
// Per-source queue playback
// ---------------------------------------------------------------------------

#[cfg(feature = "local-files")]
async fn play_queued_local(app: &Arc<Mutex<App>>, track: &TrackInfo, uri: &str) -> bool {
  release_librespot(app).await;
  let Some(player) = acquire_queue_player(app).await else {
    return false;
  };
  let _ = publish_pending_decoded(app, &player, track).await;
  match crate::infra::local::dispatch::play_single_file(&player, uri).await {
    Ok(_info) => {
      apply_volume(app, &player).await;
      publish_decoded(app, player, track.clone(), None).await;
      true
    }
    Err(e) => {
      set_status(app, format!("Cannot play {}: {e}", track.name)).await;
      false
    }
  }
}

#[cfg(feature = "subsonic")]
async fn play_queued_subsonic(app: &Arc<Mutex<App>>, track: &TrackInfo, uri: &str) -> bool {
  release_librespot(app).await;
  let Some(source) = crate::infra::subsonic::dispatch::build_source(app).await else {
    return false; // build_source surfaced its own status
  };
  let Some(player) = acquire_queue_player(app).await else {
    return false;
  };
  let fetch_id = publish_pending_decoded(app, &player, track).await;
  // Fetch off the IoEvent pump: awaiting the download here would freeze every
  // other event (skips included, for every source) for its whole duration.
  let app = Arc::clone(app);
  let uri = uri.to_string();
  let name = track.name.clone();
  tokio::spawn(async move {
    let result = crate::infra::subsonic::dispatch::download_for_queue(&source, &uri).await;
    finish_decoded_fetch(&app, fetch_id, result, &name).await;
  });
  true
}

#[cfg(feature = "youtube")]
async fn play_queued_youtube(app: &Arc<Mutex<App>>, track: &TrackInfo, uri: &str) -> bool {
  release_librespot(app).await;
  let Some(player) = acquire_queue_player(app).await else {
    return false;
  };
  let fetch_id = publish_pending_decoded(app, &player, track).await;
  {
    let mut guard = app.lock().await;
    guard.set_status_message(format!("Fetching {}\u{2026}", track.name), 30);
  }
  let source = crate::infra::youtube::dispatch::build_source(app).await;
  // Fetch off the IoEvent pump: awaiting yt-dlp here would freeze every other
  // event (skips included, for every source) for its whole duration.
  let app = Arc::clone(app);
  let uri = uri.to_string();
  let name = track.name.clone();
  tokio::spawn(async move {
    let result = crate::infra::youtube::dispatch::download_for_queue(&source, &uri).await;
    finish_decoded_fetch(&app, fetch_id, result, &name).await;
  });
  true
}

/// Play a queued Spotify track through the native streaming player via a direct
/// `player.load` (no Spirc context), publishing a Spotify queue slot. Requires a
/// connected streaming player; otherwise the item is skipped like any other
/// unplayable one. Any decoded audio is silenced first so librespot doesn't play
/// over it.
#[cfg(feature = "streaming")]
async fn play_queued_spotify(app: &Arc<Mutex<App>>, track: &TrackInfo, uri: &str) -> bool {
  let player = { app.lock().await.streaming_player.clone() };
  let Some(player) = player.filter(|p| p.is_connected()) else {
    set_status(
      app,
      format!(
        "Native streaming isn't connected; skipped \"{}\"",
        track.name
      ),
    )
    .await;
    return false;
  };
  // Silence any decoded audio so two players never share the sink. A decoded
  // queue slot is stopped and dropped; a suspended decoded context (which keeps
  // its player for resume) is paused — resume reloads its sink either way.
  // Both lookups only ever see the queueable sources (radio is torn down at
  // suspension rather than kept for reuse), so this compiles out without them.
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  {
    if let Some(p) = { app.lock().await.take_queue_now_decoded_player() } {
      p.stop();
    }
    if let Some(p) = suspended_context_player(app).await {
      p.pause();
    }
  }
  // Publish the slot *before* the load, so librespot events arriving during it
  // are classified correctly: the stray-playback guard sees a Spotify slot and
  // lets this track start, and a Spirc self-advance racing the load is caught
  // by the reload guard instead of slipping through an empty slot.
  {
    use crate::infra::queue::QueueNowPlaying;
    let mut guard = app.lock().await;
    guard.queue_now = Some(QueueNowPlaying::Spotify {
      track: track.clone(),
    });
    // Fresh slot: reset the Spirc self-advance retry budget.
    guard.spotify_queue_guard_reloads = 0;
  }
  player.activate();
  if let Err(e) = player.play_uri(uri).await {
    // Unpublish so the failed slot can't shadow the next item (or the resume).
    app.lock().await.queue_now = None;
    set_status(app, format!("Cannot play {}: {e}", track.name)).await;
    return false;
  }
  {
    let mut guard = app.lock().await;
    guard.set_status_message(format!("\u{266a} {} (queue)", track.name), 4);
    preload_next_queued_spotify(&guard);
  }
  true
}

/// Warm the *next* queued Spotify track's audio while the current queue slot
/// plays. A queued Spotify track is a cold direct `player.load` (metadata +
/// audio key + CDN handshake), which reads as a small skip delay that Spirc's
/// own in-context skipping doesn't have — Spirc preloads. This levels that:
/// called whenever a queue slot starts playing, under whatever `App` borrow the
/// caller already holds.
#[cfg(feature = "streaming")]
fn preload_next_queued_spotify(app: &App) {
  let Some(uri) = app.native_queue.first().and_then(|t| t.uri.clone()) else {
    return;
  };
  if queue_item_source(&uri) != QueueItemSource::Spotify {
    return;
  }
  if let Some(player) = app.streaming_player.as_ref().filter(|p| p.is_connected()) {
    player.preload_uri(&uri);
  }
}

/// Monotonic source for [`DecodedQueuePlayback::fetch_id`] stamps.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
static QUEUE_FETCH_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
fn next_fetch_id() -> u64 {
  QUEUE_FETCH_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Publish the queue slot for `track` *before* its (possibly multi-second)
/// download or decode, marked `advancing` so the runner tick doesn't read the
/// still-empty sink as end-of-track. From this instant `queue_owns_playback()`
/// is true, so the transport paths (skip, pause, playbar) see the queued track
/// as current during the fetch. Without this, a skip in the silent download
/// window fell through to the "context playing with items waiting" branch,
/// which re-suspended the context and dispatched a second advance — dropping
/// one queued item on the floor. Returns the slot's fetch stamp, which a
/// background fetch passes back to [`finish_decoded_fetch`].
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
async fn publish_pending_decoded(
  app: &Arc<Mutex<App>>,
  player: &Arc<LocalPlayer>,
  track: &TrackInfo,
) -> u64 {
  use crate::infra::queue::{DecodedQueuePlayback, QueueNowPlaying};
  let fetch_id = next_fetch_id();
  let mut guard = app.lock().await;
  guard.queue_now = Some(QueueNowPlaying::Decoded(DecodedQueuePlayback {
    player: Arc::clone(player),
    track: track.clone(),
    advancing: true,
    fetch_id,
    #[cfg(any(feature = "subsonic", feature = "youtube"))]
    tempfile: None,
  }));
  fetch_id
}

/// Complete a background queue fetch: if the slot still carries `fetch_id`,
/// play the downloaded file and finalize the slot; otherwise (skipped, torn
/// down, or replaced meanwhile) drop the result silently. On a download or
/// decode failure the queue advances past the item, exactly like the old
/// inline path. Play + finalize happen under one `App` lock so a concurrent
/// advance (which pops under the same lock) can never interleave.
#[cfg(any(feature = "subsonic", feature = "youtube"))]
async fn finish_decoded_fetch(
  app: &Arc<Mutex<App>>,
  fetch_id: u64,
  result: anyhow::Result<tempfile::NamedTempFile>,
  track_name: &str,
) {
  use crate::infra::queue::QueueNowPlaying;
  let mut guard = app.lock().await;
  let player = match guard.queue_now.as_ref() {
    Some(QueueNowPlaying::Decoded(d)) if d.fetch_id == fetch_id => Arc::clone(&d.player),
    _ => return, // superseded — the tempfile drops here
  };
  let tmp = match result {
    Ok(tmp) => tmp,
    Err(e) => {
      guard.set_status_message(format!("Cannot play {track_name}: {e}"), 4);
      guard.dispatch(IoEvent::AdvanceNativeQueue);
      return;
    }
  };
  let path = tmp.path().to_path_buf();
  let decode_player = Arc::clone(&player);
  let played = tokio::task::spawn_blocking(move || decode_player.play_file(&path))
    .await
    .map(|r| r.map_err(|e| e.to_string()))
    .unwrap_or_else(|e| Err(e.to_string()));
  if let Err(e) = played {
    guard.set_status_message(format!("Cannot play {track_name}: {e}"), 4);
    guard.dispatch(IoEvent::AdvanceNativeQueue);
    return;
  }
  player.set_volume(guard.user_config.behavior.volume_percent as f32 / 100.0);
  if let Some(QueueNowPlaying::Decoded(d)) = guard.queue_now.as_mut() {
    d.tempfile = Some(tmp);
    d.advancing = false;
  }
  guard.set_status_message(format!("\u{266a} {track_name} (queue)"), 4);
  #[cfg(feature = "streaming")]
  preload_next_queued_spotify(&guard);
}

/// Publish the decoded queue slot and announce the track. Only the local-file
/// path finalizes synchronously through here (it plays straight from disk);
/// downloaded sources finalize via [`finish_decoded_fetch`].
#[cfg(feature = "local-files")]
async fn publish_decoded(
  app: &Arc<Mutex<App>>,
  player: Arc<LocalPlayer>,
  track: TrackInfo,
  #[cfg(any(feature = "subsonic", feature = "youtube"))] tempfile: Option<tempfile::NamedTempFile>,
  #[cfg(not(any(feature = "subsonic", feature = "youtube")))] _tempfile: Option<()>,
) {
  use crate::infra::queue::{DecodedQueuePlayback, QueueNowPlaying};
  let name = track.name.clone();
  let mut guard = app.lock().await;
  guard.queue_now = Some(QueueNowPlaying::Decoded(DecodedQueuePlayback {
    player,
    track,
    advancing: false,
    fetch_id: next_fetch_id(),
    #[cfg(any(feature = "subsonic", feature = "youtube"))]
    tempfile,
  }));
  guard.set_status_message(format!("\u{266a} {name} (queue)"), 4);
  #[cfg(feature = "streaming")]
  preload_next_queued_spotify(&guard);
}

/// Acquire an output-device player for the queue slot, in priority order:
/// 1. reuse the queue slot's own player (advancing within the queue);
/// 2. reuse the suspended decoded context's player (device-handoff-free);
/// 3. open a fresh device.
///
/// Callers must [`release_librespot`] *before* acquiring, not just on the
/// fresh-device path: the outgoing queue slot can be a still-playing Spotify
/// track (mid-track skip / Enter-jump), and on the reuse paths nothing else
/// silences it — it would keep playing under the whole download window.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
async fn acquire_queue_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(p) = {
    let guard = app.lock().await;
    guard.queue_now_decoded_player().map(Arc::clone)
  } {
    return Some(p);
  }
  if let Some(p) = suspended_context_player(app).await {
    return Some(p);
  }
  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_status(app, format!("No audio output for queue playback: {e}")).await;
      None
    }
    Err(e) => {
      set_status(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

/// The player of whichever decoded context (local / Subsonic / YouTube) is
/// currently suspended under the queue, so the queue slot can reuse its output
/// device. Radio is excluded: it is torn down at suspension (a live stream can't
/// share the sink), so the queue opens a fresh player and reconnects on resume.
/// That exclusion is exactly why this is gated on the three queueable sources
/// rather than `audio-decode` — under radio alone every arm below is cfg'd out
/// and the function is unreachable.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
async fn suspended_context_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  let guard = app.lock().await;
  #[cfg(feature = "local-files")]
  if let Some(s) = guard.local_playback.as_ref() {
    return Some(Arc::clone(&s.player));
  }
  #[cfg(feature = "subsonic")]
  if let Some(s) = guard.subsonic_playback.as_ref() {
    return Some(Arc::clone(&s.player));
  }
  #[cfg(feature = "youtube")]
  if let Some(s) = guard.youtube_playback.as_ref() {
    return Some(Arc::clone(&s.player));
  }
  None
}

/// Pause native Spotify before a decoded queue item takes over: it both
/// releases the output device (when a fresh one is opened) and silences a
/// still-playing queued Spotify track that is being skipped mid-play. Called
/// unconditionally at the top of every decoded queue-play path — a Spirc pause
/// on an already-paused or idle librespot is a no-op.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
async fn release_librespot(app: &Arc<Mutex<App>>) {
  #[cfg(feature = "streaming")]
  {
    let streaming = app.lock().await.streaming_player.clone();
    if let Some(player) = streaming {
      player.pause();
    }
  }
  #[cfg(not(feature = "streaming"))]
  {
    let _ = app;
  }
}

#[cfg(feature = "local-files")]
async fn apply_volume(app: &Arc<Mutex<App>>, player: &Arc<LocalPlayer>) {
  let volume = app.lock().await.user_config.behavior.volume_percent;
  player.set_volume(volume as f32 / 100.0);
}

// ---------------------------------------------------------------------------
// Resume
// ---------------------------------------------------------------------------

/// Queue drained: resume the suspended context, or finish if nothing was
/// suspended. The queue slot's player is stopped only when it is **not** shared
/// with the context being resumed (`Arc::ptr_eq`).
async fn resume_or_finish(app: &Arc<Mutex<App>>) {
  #[cfg(any(
    feature = "streaming",
    feature = "local-files",
    feature = "subsonic",
    feature = "youtube",
    feature = "internet-radio"
  ))]
  use crate::core::queue::SuspendedContext;

  let suspended = { app.lock().await.queue_suspended.take() };

  // The slot can still be a *playing* Spotify track when the drain came from a
  // mid-play skip (only unplayable items were left); silence it before anything
  // resumes over it. A naturally-ended slot was already cleared at EndOfTrack.
  #[cfg(feature = "streaming")]
  {
    let player = {
      let guard = app.lock().await;
      if guard.queue_now_is_spotify() {
        guard.streaming_player.clone()
      } else {
        None
      }
    };
    if let Some(player) = player {
      player.pause();
    }
  }

  // Take the queue slot's player so we can decide whether to stop it.
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  let queue_player = { app.lock().await.take_queue_now_decoded_player() };
  #[cfg(all(
    feature = "streaming",
    not(any(feature = "local-files", feature = "subsonic", feature = "youtube"))
  ))]
  {
    app.lock().await.queue_now = None;
  }

  match suspended {
    None => {
      // Nothing was suspended: the queue was playing over an idle app (or a
      // context finished before the queue started). Stop the slot and note it.
      #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
      if let Some(player) = queue_player {
        player.stop();
        app
          .lock()
          .await
          .set_status_message("Queue finished".to_string(), 3);
      }
    }
    #[cfg(feature = "local-files")]
    Some(SuspendedContext::Local {
      resume_index,
      resume_position_ms,
    }) => resume_local(app, resume_index, resume_position_ms, queue_player).await,
    #[cfg(feature = "subsonic")]
    Some(SuspendedContext::Subsonic {
      resume_index,
      resume_position_ms,
    }) => resume_subsonic(app, resume_index, resume_position_ms, queue_player).await,
    #[cfg(feature = "youtube")]
    Some(SuspendedContext::YouTube {
      resume_index,
      resume_position_ms,
    }) => resume_youtube(app, resume_index, resume_position_ms, queue_player).await,
    #[cfg(feature = "internet-radio")]
    Some(SuspendedContext::Radio { station }) => {
      // Radio uses its own fresh player, so always stop the queue slot. A
      // radio-only build has no queueable source, hence no slot to stop.
      #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
      if let Some(player) = queue_player {
        player.stop();
      }
      if let Some(uri) = station.uri.clone() {
        let mut guard = app.lock().await;
        // Seed the browse table so the radio start path resolves the station.
        guard.track_table.tracks = vec![station];
        guard.dispatch(IoEvent::StartPlayback(Some(uri), None, None));
      }
    }
    #[cfg(feature = "streaming")]
    Some(SuspendedContext::Spotify {
      context_uri,
      resume_track_uri,
    }) => {
      // The network handler re-loads the Spotify context (offset by the resume
      // track) on the native device. Stop the decoded queue slot if one exists.
      #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
      if let Some(player) = queue_player {
        player.stop();
      }
      app
        .lock()
        .await
        .dispatch(IoEvent::ResumeSpotifyContext(context_uri, resume_track_uri));
    }
    // In slim builds `SuspendedContext` is uninhabited, so every arm above is
    // cfg'd out and only `None` is reachable.
    #[allow(unreachable_patterns)]
    _ =>
    {
      #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
      if let Some(player) = queue_player {
        player.stop();
      }
    }
  }
}

#[cfg(feature = "local-files")]
async fn resume_local(
  app: &Arc<Mutex<App>>,
  resume_index: Option<usize>,
  resume_position_ms: u64,
  queue_player: Option<Arc<LocalPlayer>>,
) {
  let Some(index) = resume_index else {
    // Context exhausted: tear it down and stop the queue slot.
    if let Some(local) = app.lock().await.local_playback.take() {
      local.player.stop();
    }
    if let Some(player) = queue_player {
      player.stop();
    }
    return;
  };
  // Point the context at the resume track and keep it latched until play_index
  // commits. Stop the queue slot only if it is a different player.
  let shared = {
    let mut guard = app.lock().await;
    match guard.local_playback.as_mut() {
      Some(local) => {
        let shared = queue_player
          .as_ref()
          .is_some_and(|qp| Arc::ptr_eq(qp, &local.player));
        local.index = index;
        local.advancing = true;
        shared
      }
      None => false,
    }
  };
  if !shared {
    if let Some(player) = queue_player {
      player.stop();
    }
  }
  // Local has no retained tempfile; play_index re-reads from disk (restarting
  // the track), then we seek to the saved position for a mid-track resume.
  crate::infra::local::dispatch::play_index(app, index).await;
  if resume_position_ms > 0 {
    let guard = app.lock().await;
    if let Some(local) = guard.local_playback.as_ref() {
      let _ = local.player.seek(Duration::from_millis(resume_position_ms));
    }
  }
}

#[cfg(feature = "subsonic")]
async fn resume_subsonic(
  app: &Arc<Mutex<App>>,
  resume_index: Option<usize>,
  resume_position_ms: u64,
  queue_player: Option<Arc<LocalPlayer>>,
) {
  let Some(index) = resume_index else {
    if let Some(s) = app.lock().await.subsonic_playback.take() {
      s.player.stop();
    }
    if let Some(player) = queue_player {
      player.stop();
    }
    return;
  };
  // Same track and its tempfile is still loaded (mid-track Enter-jump): replay
  // the retained tempfile and seek, avoiding a re-download. Otherwise re-download
  // the target index through the existing play_index machinery.
  let replay = {
    let mut guard = app.lock().await;
    match guard.subsonic_playback.as_mut() {
      Some(s) if index == s.index => {
        s.advancing = true;
        Some((Arc::clone(&s.player), s.tempfile.path().to_path_buf()))
      }
      Some(s) => {
        s.index = index;
        s.advancing = true;
        None
      }
      None => None,
    }
  };
  // The queue slot shares the context player (reused at acquire time), so it is
  // never stopped here — the same sink is reloaded on resume.
  let _ = queue_player;
  match replay {
    Some((player, path)) => {
      let decode_player = Arc::clone(&player);
      let ok = tokio::task::spawn_blocking(move || decode_player.play_file(&path))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
      // Clear the latch either way: on failure the sink is empty, so leaving
      // `advancing = true` would wedge the runner tick's advance off forever.
      if let Some(s) = app.lock().await.subsonic_playback.as_mut() {
        s.advancing = false;
      }
      if ok && resume_position_ms > 0 {
        let _ = player.seek(Duration::from_millis(resume_position_ms));
      }
    }
    None => crate::infra::subsonic::dispatch::play_index(app, index).await,
  }
}

#[cfg(feature = "youtube")]
async fn resume_youtube(
  app: &Arc<Mutex<App>>,
  resume_index: Option<usize>,
  resume_position_ms: u64,
  queue_player: Option<Arc<LocalPlayer>>,
) {
  let Some(index) = resume_index else {
    if let Some(s) = app.lock().await.youtube_playback.take() {
      s.player.stop();
    }
    if let Some(player) = queue_player {
      player.stop();
    }
    return;
  };
  let replay = {
    let mut guard = app.lock().await;
    match guard.youtube_playback.as_mut() {
      Some(s) if index == s.index => {
        s.advancing = true;
        Some((Arc::clone(&s.player), s.tempfile.path().to_path_buf()))
      }
      Some(s) => {
        s.index = index;
        s.advancing = true;
        None
      }
      None => None,
    }
  };
  let _ = queue_player;
  match replay {
    Some((player, path)) => {
      let decode_player = Arc::clone(&player);
      let ok = tokio::task::spawn_blocking(move || decode_player.play_file(&path))
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
      // Clear the latch either way: on failure the sink is empty, so leaving
      // `advancing = true` would wedge the runner tick's advance off forever.
      if let Some(s) = app.lock().await.youtube_playback.as_mut() {
        s.advancing = false;
      }
      if ok && resume_position_ms > 0 {
        let _ = player.seek(Duration::from_millis(resume_position_ms));
      }
    }
    None => crate::infra::youtube::dispatch::play_index(app, index).await,
  }
}

async fn set_status(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 4);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn track(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(uri.to_string()),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1000,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  fn test_app() -> Arc<Mutex<App>> {
    let (tx, _rx) = channel();
    Arc::new(Mutex::new(App::new(
      tx,
      UserConfig::new(),
      Some(SystemTime::now()),
    )))
  }

  /// A queued item whose source feature is off in this build must be skipped
  /// with an actionable status message — never panic, never stall the queue.
  /// In the slim CI build every alternative source is unavailable, so a
  /// `subsonic:` item exercises exactly that path.
  #[cfg(not(feature = "subsonic"))]
  #[tokio::test]
  async fn advance_skips_unavailable_source_without_panicking() {
    let app = test_app();
    app
      .lock()
      .await
      .native_queue
      .push(track("subsonic:track:1", "Unplayable"));

    assert!(route_queue_event(&app, &IoEvent::AdvanceNativeQueue).await);

    let guard = app.lock().await;
    assert!(guard.native_queue.is_empty(), "the item is consumed");
    assert!(
      guard
        .status_message
        .as_deref()
        .is_some_and(|m| m.contains("isn't available in this build")),
      "expected an unavailable-source message, got {:?}",
      guard.status_message
    );
  }

  /// An empty queue with nothing suspended is a no-op advance: it must not
  /// panic and must leave the queue empty.
  #[tokio::test]
  async fn advance_on_empty_queue_is_a_noop() {
    let app = test_app();
    assert!(route_queue_event(&app, &IoEvent::AdvanceNativeQueue).await);
    assert!(app.lock().await.native_queue.is_empty());
  }

  #[cfg(feature = "streaming")]
  #[tokio::test]
  async fn next_track_is_consumed_by_spotify_queue_slot() {
    use crate::infra::queue::QueueNowPlaying;
    let app = test_app();
    app.lock().await.queue_now = Some(QueueNowPlaying::Spotify {
      track: track("spotify:track:queued", "Queued"),
    });

    assert!(route_queue_event(&app, &IoEvent::NextTrack).await);
  }

  #[cfg(feature = "streaming")]
  #[tokio::test]
  async fn bare_resume_does_not_clear_spotify_queue_slot() {
    use crate::core::queue::SuspendedContext;
    use crate::infra::queue::QueueNowPlaying;
    let app = test_app();
    {
      let mut guard = app.lock().await;
      guard.queue_now = Some(QueueNowPlaying::Spotify {
        track: track("spotify:track:queued", "Queued"),
      });
      guard.queue_suspended = Some(SuspendedContext::Spotify {
        context_uri: Some("spotify:playlist:ctx".to_string()),
        resume_track_uri: Some("spotify:track:resume".to_string()),
      });
    }

    assert!(!route_queue_event(&app, &IoEvent::StartPlayback(None, None, None)).await);

    let guard = app.lock().await;
    assert!(guard.queue_now_is_spotify());
    assert!(guard.queue_suspended.is_some());
  }

  #[cfg(feature = "streaming")]
  #[tokio::test]
  async fn new_playback_clears_spotify_queue_slot() {
    use crate::core::queue::SuspendedContext;
    use crate::infra::queue::QueueNowPlaying;
    let app = test_app();
    {
      let mut guard = app.lock().await;
      guard.queue_now = Some(QueueNowPlaying::Spotify {
        track: track("spotify:track:queued", "Queued"),
      });
      guard.queue_suspended = Some(SuspendedContext::Spotify {
        context_uri: Some("spotify:playlist:ctx".to_string()),
        resume_track_uri: Some("spotify:track:resume".to_string()),
      });
    }

    assert!(
      !route_queue_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:playlist:new".to_string()), None, None)
      )
      .await
    );

    let guard = app.lock().await;
    assert!(!guard.queue_owns_playback());
    assert!(guard.queue_suspended.is_none());
  }

  /// A live end-to-end queue test: browse a Subsonic playlist, start it, queue a
  /// track from mid-playlist, then advance the native queue. Asserts the
  /// suspended context (index + tempfile) survives the queue playback so it can
  /// resume. Ignored (needs the demo server AND an audio device); run:
  /// `cargo test --features subsonic -- --ignored live_queue`
  #[cfg(feature = "subsonic")]
  #[tokio::test]
  #[ignore = "hits the live demo server AND requires an audio output device"]
  async fn live_queue_suspends_and_preserves_subsonic_context() {
    use crate::infra::subsonic::dispatch::route_subsonic_event;

    let app = {
      let (tx, _rx) = channel();
      let mut a = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
      a.user_config.behavior.subsonic_url = Some("https://demo.navidrome.org".to_string());
      a.user_config.behavior.subsonic_username = Some("demo".to_string());
      a.user_config.behavior.subsonic_password = Some("demo".to_string());
      Arc::new(Mutex::new(a))
    };

    assert!(route_subsonic_event(&app, &IoEvent::GetSubsonicPlaylists).await);
    let playlist_uri = app
      .lock()
      .await
      .subsonic_playlists
      .first()
      .unwrap()
      .uri
      .clone();
    assert!(route_subsonic_event(&app, &IoEvent::GetSubsonicTracks(playlist_uri)).await);
    let tracks: Vec<TrackInfo> = app.lock().await.track_table.tracks.clone();
    assert!(tracks.len() >= 3, "need a multi-track playlist");
    let uris: Vec<String> = tracks.iter().filter_map(|t| t.uri.clone()).collect();

    // Start the playlist at index 0.
    assert!(route_subsonic_event(&app, &IoEvent::StartPlayback(None, Some(uris), Some(0))).await);

    // Queue a track from later in the playlist, then advance the native queue
    // (as an end-of-track suspension would).
    {
      let mut guard = app.lock().await;
      guard.native_queue.push(tracks[2].clone());
      guard.queue_suspended = Some(crate::core::queue::SuspendedContext::Subsonic {
        resume_index: crate::infra::queue::next_index(0, tracks.len()),
        resume_position_ms: 0,
      });
      if let Some(s) = guard.subsonic_playback.as_mut() {
        s.advancing = true;
      }
    }
    assert!(route_queue_event(&app, &IoEvent::AdvanceNativeQueue).await);

    let guard = app.lock().await;
    let s = guard.subsonic_playback.as_ref().expect("context preserved");
    assert_eq!(s.index, 0, "the suspended context index is untouched");
    assert!(
      guard.queue_owns_playback(),
      "the queue slot now owns playback"
    );
  }
}
