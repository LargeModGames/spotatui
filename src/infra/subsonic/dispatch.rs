//! Subsonic browse/search/playback routing.
//!
//! The seam that keeps the Spotify [`Network`](crate::infra::network)
//! Spotify-only: [`route_subsonic_event`] is called from the runtime IoEvent
//! pump *after* the local-files dispatch and *before* `handle_network_event`.
//! When an event targets the Subsonic source (a browse/search request, or a
//! `subsonic:` playback URI) it is handled here and consumed; otherwise it falls
//! through to the normal Spotify dispatch.
//!
//! Unlike the local-files source, Subsonic's [`MediaSource`]/[`Searcher`] methods
//! are genuinely async (reqwest REST calls), so they are awaited directly rather
//! than run on the blocking pool.
//!
//! ## Decoupling & device ownership
//!
//! Subsonic playback owns a single piece of state, [`App::subsonic_playback`],
//! and never writes Spotify/librespot fields — the playbar reads progress/pause
//! live from the player. Only one backend holds the audio device at a time:
//! starting Subsonic pauses or parks librespot **and** tears down any local session; the
//! reciprocal teardown lives in the local and network start paths.
//!
//! ## Streaming
//!
//! Each track is downloaded from `stream.view` to a tempfile off the pump,
//! then staged in pump order through the shared [`LocalPlayer`]. The download
//! window is much longer than a local decode, so the `advancing` guard is
//! load-bearing, and a download failure tears the session down rather than
//! skipping past (avoids cascading the whole queue when the network is down).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::{track_id_from_uri, PendingDownload, SubsonicPlaybackState, SubsonicSource};
use crate::core::app::{App, TrackTableContext};
use crate::core::source::{MediaSource, Searcher, Source};
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;
use crate::infra::queue::{
  advance_index, apply_resolved, placeholder_uri, replay_file, restage, snapshot_tracks,
  DownloadAbortHandle, ResumePoint,
};

/// Environment variable that overrides the configured Subsonic password. Prefer
/// it over the plaintext config field so the secret is never written to disk.
const PASSWORD_ENV: &str = "SPOTATUI_SUBSONIC_PASSWORD";

/// Whether a URI is owned by the Subsonic source.
fn is_subsonic_uri(uri: &str) -> bool {
  uri.starts_with("subsonic:")
}

/// Skip direction within the subsonic queue.
#[derive(Clone, Copy)]
enum Direction {
  Next,
  Prev,
}

/// Intercept events that target the Subsonic source.
///
/// Returns `true` if the event was handled (and must **not** be forwarded to the
/// Spotify network), `false` to let the normal dispatch run.
pub async fn route_subsonic_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    IoEvent::GetSubsonicPlaylists => {
      load_subsonic_playlists(app).await;
      true
    }
    IoEvent::GetSubsonicTracks(uri) => {
      load_subsonic_tracks(app, uri).await;
      true
    }
    IoEvent::GetSubsonicSearchResults(query) => {
      run_subsonic_search(app, query).await;
      true
    }
    // Start a playlist of subsonic tracks: queue all and start at the offset.
    IoEvent::StartPlayback(None, Some(uris), offset)
      if uris.first().is_some_and(|u| is_subsonic_uri(u)) =>
    {
      start_subsonic_queue(app, uris, offset.unwrap_or(0), None).await;
      true
    }
    // A single subsonic track with no surrounding playlist: a one-track queue.
    IoEvent::StartPlayback(Some(uri), _, _) if is_subsonic_uri(uri) => {
      start_subsonic_queue(app, std::slice::from_ref(uri), 0, None).await;
      true
    }
    // Bare "resume current" — ours only while subsonic owns the session.
    IoEvent::StartPlayback(None, None, None) => set_paused(app, false).await,
    // Any other start is a local/Spotify play: relinquish the device, then let
    // the normal dispatch run.
    IoEvent::StartPlayback(..) => {
      teardown_subsonic(app).await;
      false
    }
    IoEvent::PausePlayback => set_paused(app, true).await,
    IoEvent::Seek(position_ms) => seek(app, u64::from(*position_ms)).await,
    IoEvent::ChangeVolume(volume) => match player(app).await {
      Some(p) => {
        p.set_volume(*volume);
        let mut app = app.lock().await;
        app.runtime_state.volume_percent = *volume;
        app.schedule_state_save(crate::core::state::PersistedRuntimeState::volume_percent(
          *volume,
        ));
        true
      }
      None => false,
    },
    IoEvent::NextTrack => skip(app, Direction::Next).await,
    IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => skip(app, Direction::Prev).await,
    // Repeat-one auto-advance: replay the current track (dispatched by the runner
    // tick only while Subsonic owns the session).
    IoEvent::ReplayCurrentTrack => replay_current(app).await,
    // Explicit repeat-mode set (e.g. a plugin's `set_repeat`): store it as the
    // decoded repeat mode while a queueable decoded source owns playback (the
    // method's ownership gate excludes a context suspended under the native
    // queue), so it never reaches the Spotify context.
    IoEvent::Repeat(state) => app.lock().await.set_decoded_repeat_from_state(*state),
    _ => false,
  }
}

// ---------------------------------------------------------------------------
// Browse + search
// ---------------------------------------------------------------------------

/// Build a [`SubsonicSource`] from the saved server config, with the password
/// taken from the `SPOTATUI_SUBSONIC_PASSWORD` env var when set. Returns `None`
/// (after surfacing a status message) when no server URL is configured.
pub(crate) async fn build_source(app: &Arc<Mutex<App>>) -> Option<SubsonicSource> {
  match build_sync_source(app).await {
    Ok(source) => Some(source),
    Err(_) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .subsonic_no_server
          .to_string(),
      )
      .await;
      None
    }
  }
}

/// A source for the playlist sync, with the unconfigured case left to the caller.
pub(crate) async fn build_sync_source(app: &Arc<Mutex<App>>) -> Result<SubsonicSource> {
  let (url, username, config_password) = {
    let guard = app.lock().await;
    let behavior = &guard.user_config.behavior;
    (
      behavior.subsonic_url.clone(),
      behavior.subsonic_username.clone(),
      behavior.subsonic_password.clone(),
    )
  };

  let Some(url) = url else {
    return Err(crate::core::playlist_sync::SyncReason::SubsonicNotConfigured.into());
  };

  // Env override takes precedence over the plaintext config field.
  let password = std::env::var(PASSWORD_ENV)
    .ok()
    .or(config_password)
    .unwrap_or_default();

  Ok(SubsonicSource::new(
    url,
    username.unwrap_or_default(),
    password,
  ))
}

/// Fetch the user's server playlists into `app.subsonic_playlists()`.
async fn load_subsonic_playlists(app: &Arc<Mutex<App>>) {
  let Some(source) = build_source(app).await else {
    return;
  };
  match source.playlists().await {
    Ok(playlists) => {
      let mut app = app.lock().await;
      *app.subsonic_playlists_mut() = playlists;
    }
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .subsonic_playlists_failed(&e.to_string()),
      )
      .await
    }
  }
}

/// Fetch a playlist's tracks into the shared track table, tagged
/// [`TrackTableContext::SubsonicPlaylist`] so selecting a row plays it.
async fn load_subsonic_tracks(app: &Arc<Mutex<App>>, playlist_uri: &str) {
  let Some(source) = build_source(app).await else {
    return;
  };
  match source.tracks(playlist_uri).await {
    Ok(tracks) => {
      let mut app = app.lock().await;
      app.set_source_track_table(playlist_uri, tracks, TrackTableContext::SubsonicPlaylist);
    }
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .subsonic_playlist_failed(&e.to_string()),
      )
      .await
    }
  }
}

/// Run a catalog search and populate `app.search_results`.
///
/// M2 populates **only** the songs block: album/artist search-result Enter
/// dispatches rspotify-bound `GetAlbum`/`GetArtist` events that fail for Subsonic
/// ids, so those blocks are left `None` to avoid dead rows. Album/artist
/// drill-down is a tracked follow-up.
async fn run_subsonic_search(app: &Arc<Mutex<App>>, query: &str) {
  let Some(source) = build_source(app).await else {
    return;
  };
  match source.search(query).await {
    Ok(results) => app
      .lock()
      .await
      .show_source_search_tracks(query, results.tracks),
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .subsonic_search_failed(&e.to_string()),
      )
      .await
    }
  }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// The live subsonic player, if a subsonic session is active.
async fn player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app
    .lock()
    .await
    .subsonic_playback
    .as_ref()
    .map(|s| Arc::clone(&s.player))
}

async fn set_paused(app: &Arc<Mutex<App>>, paused: bool) -> bool {
  let mut guard = app.lock().await;
  let Some(session) = guard.subsonic_playback.as_mut() else {
    return false;
  };
  session.set_paused(paused);
  true
}

async fn seek(app: &Arc<Mutex<App>>, position_ms: u64) -> bool {
  let mut guard = app.lock().await;
  let Some(session) = guard.subsonic_playback.as_mut() else {
    return false;
  };
  if session.fetch.is_some() {
    session.resume_at.get_or_insert_default().position_ms = position_ms;
  } else {
    let player = Arc::clone(&session.player);
    drop(guard);
    let _ = player.seek(Duration::from_millis(position_ms));
  }
  true
}

/// Release the other backends so only subsonic holds the output device.
async fn release_other_backends(app: &Arc<Mutex<App>>) {
  // Take the sink from native Spotify so no rebuild resumes it under this
  // source.
  #[cfg(feature = "streaming")]
  app.lock().await.release_native_for_decoded();
  // The other decoded sources never see this subsonic: start (the pump's
  // `!handled_subsonic` short-circuit), so their sessions are torn down here.
  let players = app
    .lock()
    .await
    .take_decoded_sessions_except(Source::Subsonic);
  for player in players {
    player.stop_detached();
  }
}

/// Reuse the live subsonic player, or open a fresh output device for one. A
/// freshly opened player is published by the caller with the pending download.
async fn acquire_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(p) = player(app).await {
    return Some(p);
  }
  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .subsonic_audio_unavailable(&e.to_string()),
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

/// Download a track's audio into a fresh tempfile. Must be awaited **without**
/// holding the `App` lock (it can take seconds).
async fn download_track(source: &SubsonicSource, track_id: &str) -> Result<NamedTempFile> {
  let tmp = NamedTempFile::new().context("creating temp file for Subsonic stream")?;
  source.download_track(track_id, tmp.path()).await?;
  Ok(tmp)
}

/// Download the track at `uri` into a tempfile, for the native queue engine's
/// off-pump fetch (playing is the queue's job — it re-checks that the queue
/// slot is still current before touching the shared player).
pub(crate) async fn download_for_queue(
  source: &SubsonicSource,
  uri: &str,
) -> Result<NamedTempFile> {
  let track_id = track_id_from_uri(uri)?.to_string();
  download_track(source, &track_id).await
}

/// Begin playing a queue of subsonic tracks, taking over the session and
/// starting at `start_idx` (clamped into range).
pub(crate) async fn start_subsonic_queue(
  app: &Arc<Mutex<App>>,
  uris: &[String],
  start_idx: usize,
  resume: Option<ResumePoint>,
) {
  // Snapshot the track metadata under one short lock, from whichever browse view
  // the request came from (track table for browse, search results for search).
  let tracks = {
    let guard = app.lock().await;
    let search = guard
      .search_results()
      .tracks
      .as_ref()
      .map(|p| p.items.as_slice());
    snapshot_tracks(&guard.track_table.tracks, search, uris)
  };
  if tracks.is_empty() {
    set_error(
      app,
      crate::infra::app_strings(app)
        .await
        .status
        .infra
        .subsonic_no_tracks
        .to_string(),
    )
    .await;
    return;
  }
  let index = start_idx.min(tracks.len() - 1);

  let track_id = match tracks[index].uri.as_deref().map(track_id_from_uri) {
    Some(Ok(id)) => id.to_string(),
    _ => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .subsonic_uri_invalid
          .to_string(),
      )
      .await;
      return;
    }
  };

  let Some(source) = build_source(app).await else {
    return;
  };
  let source = Arc::new(source);

  // Only one backend owns the device at a time.
  app.lock().await.claim_decoded_sink(Source::Subsonic);
  release_other_backends(app).await;

  let Some(player) = acquire_player(app).await else {
    return;
  };

  // The old context's file must survive until the reused player stops.
  let previous = app.lock().await.subsonic_playback.take();
  if let Some(mut previous) = previous {
    previous.cancel_download();
    if previous.tempfile.is_some() {
      previous.player.pause();
      let _ = tokio::task::spawn_blocking(move || {
        previous.player.stop();
        drop(previous);
      })
      .await;
    }
  }

  // Publish the pending context so transport can address it during the download.
  let mut guard = app.lock().await;
  let mut state = SubsonicPlaybackState {
    player,
    source,
    tracks,
    index,
    advancing: true,
    tempfile: None,
    shuffle_backup: None,
    resume_at: resume,
    fetch: None,
  };
  if guard.decoded_shuffle {
    state.set_shuffle(true);
  }
  let index = state.index;
  spawn_download(app, &mut state, index, track_id);
  guard.subsonic_playback = Some(state);
}

/// Move the subsonic queue index in `direction` and play the new track. Returns
/// `true` if subsonic owns the session (so the event is consumed).
async fn skip(app: &Arc<Mutex<App>>, direction: Direction) -> bool {
  let mut guard = app.lock().await;
  if guard.subsonic_playback.is_some() && guard.queue_suspended.is_some() {
    return true; // the frontend has already handed this context to the queue
  }
  // Repeat-all wraps at the ends; Off/Repeat-one clamp.
  let mode = guard.decoded_repeat;
  let Some(s) = guard.subsonic_playback.as_mut() else {
    return false; // not ours
  };
  let forward = matches!(direction, Direction::Next);
  if let Some(index) = advance_index(s.requested_index(), s.tracks.len(), mode, forward) {
    play_index(app, &mut guard, index, None);
  } else {
    // A boundary skip must not clear the latch for a download still in flight.
    s.advancing = s.fetch.is_some();
  }
  true
}

/// Replay the current Subsonic track (repeat-one auto-advance) via
/// [`replay_file`]. Re-decodes the **already-downloaded tempfile** — no second
/// network fetch — and on failure tears the session down (rather than leaving
/// an empty sink that would re-fire replay every tick). Returns `true` if
/// Subsonic owns the session.
async fn replay_current(app: &Arc<Mutex<App>>) -> bool {
  let (player, path, resume) = {
    let mut guard = app.lock().await;
    let Some(s) = guard.subsonic_playback.as_mut() else {
      return false; // not ours
    };
    if s.fetch.is_some() {
      // The in-flight download will apply the driver's recovery point.
      return true;
    }
    let Some(tmp) = s.tempfile.as_ref() else {
      return true;
    };
    s.advancing = true;
    (
      Arc::clone(&s.player),
      tmp.path().to_path_buf(),
      s.resume_at
        .take()
        .or(Some(crate::infra::queue::ResumePoint {
          position_ms: 0,
          paused: s.player.is_paused(),
        })),
    )
  };
  if replay_file(player, path, resume).await {
    if let Some(s) = app.lock().await.subsonic_playback.as_mut() {
      s.advancing = false;
    }
  } else {
    teardown_subsonic(app).await;
    set_error(
      app,
      crate::infra::app_strings(app)
        .await
        .status
        .infra
        .subsonic_replay_failed
        .to_string(),
    )
    .await;
  }
  true
}

/// Request the track at `target` in the published context. Called under the
/// App lock so the target and its download stay in step with queue suspension.
pub(crate) fn play_index(
  app: &Arc<Mutex<App>>,
  guard: &mut App,
  target: usize,
  resume: Option<ResumePoint>,
) {
  let Some(session) = guard.subsonic_playback.as_mut() else {
    return;
  };
  let Some(track) = session.tracks.get(target) else {
    session.advancing = session.fetch.is_some();
    return;
  };
  let Some(Ok(track_id)) = track.uri.as_deref().map(track_id_from_uri) else {
    let message = guard.strings().status.infra.subsonic_uri_invalid;
    fail_playback(guard, message.to_string());
    return;
  };
  let track_id = track_id.to_string();
  session.resume_at = resume;
  spawn_download(app, session, target, track_id);
}

fn spawn_download(
  app: &Arc<Mutex<App>>,
  session: &mut SubsonicPlaybackState,
  index: usize,
  track_id: String,
) {
  static FETCH_SEQ: AtomicU64 = AtomicU64::new(0);
  let id = FETCH_SEQ.fetch_add(1, Ordering::Relaxed);
  let app = Arc::downgrade(app);
  let source = Arc::clone(&session.source);
  let task = tokio::spawn(async move {
    let result = download_track(&source, &track_id).await;
    if let Some(app) = app.upgrade() {
      app
        .lock()
        .await
        .dispatch_without_spinner(IoEvent::SubsonicDownloadFinished(id, result));
    }
  });
  session.advancing = true;
  session.resume_at.get_or_insert_default();
  session.fetch = Some(PendingDownload {
    id,
    index,
    _abort: DownloadAbortHandle(task.abort_handle()),
  });
}

fn current_download(app: &mut App, id: u64) -> Option<&mut SubsonicPlaybackState> {
  if app.apple_music_owns_playback() || app.queue_owns_playback() || app.queue_suspended.is_some() {
    return None;
  }
  app
    .subsonic_playback
    .as_mut()
    .filter(|s| s.fetch.as_ref().is_some_and(|fetch| fetch.id == id))
}

/// Only the current download can stage audio. Keep staging on the pump in
/// transport order, and re-check after the blocking stage before unpausing:
/// the frontend can suspend the context for the native queue in the meantime.
pub(crate) async fn finish_download(app: &Arc<Mutex<App>>, id: u64, result: Result<NamedTempFile>) {
  let mut guard = app.lock().await;
  let strings = guard.strings();
  let Some(session) = current_download(&mut guard, id) else {
    return;
  };
  let tmp = match result {
    Ok(t) => t,
    Err(e) => {
      fail_playback(
        &mut guard,
        strings
          .status
          .infra
          .subsonic_download_failed(&e.to_string()),
      );
      return;
    }
  };
  let resume = ResumePoint {
    paused: true,
    ..session.resume_at.unwrap_or_default()
  };
  let path = tmp.path().to_path_buf();
  let player = Arc::clone(&session.player);
  drop(guard);
  let result = tokio::task::spawn_blocking(move || restage(&player, &path, Some(resume))).await;

  let mut guard = app.lock().await;
  let volume = guard.runtime_state.volume_percent;
  let Some(session) = current_download(&mut guard, id) else {
    return;
  };
  match result {
    Ok(Ok(())) => {
      session.index = session.requested_index();
      session.cancel_download();
      session.tempfile = Some(tmp);
      session.advancing = false;
      session.player.set_volume(volume);
      if !session.resume_at.take().unwrap_or_default().paused {
        session.player.resume();
      }
      spawn_resolve(app, session);
      let display = session.tracks[session.index].name.clone();
      guard.set_status_message(format!("\u{266a} {display}"), 4);
    }
    Ok(Err(e)) => fail_playback(
      &mut guard,
      strings.status.infra.subsonic_play_failed(&e.to_string()),
    ),
    Err(e) => fail_playback(
      &mut guard,
      strings.status.infra.subsonic_task_failed(&e.to_string()),
    ),
  }
}

/// End on a failed download rather than storming the server with auto-skips.
fn fail_playback(app: &mut App, message: String) {
  stop_session(app.subsonic_playback.take());
  app.set_status_message(message, 6);
}

/// Look the current song up with `getSong` when it is a placeholder (a URI
/// started from outside the track table, as an MCP or AI DJ `play_now` sends,
/// #716), so the playbar and the OS media controls show its title instead of
/// the URI. Called once the song is current, and runs on its own task so the
/// pump never waits for it. The answer lands only while that row is still the
/// current one; a failed lookup keeps the placeholder and never stops playback.
fn spawn_resolve(app: &Arc<Mutex<App>>, session: &SubsonicPlaybackState) {
  let Some(uri) = placeholder_uri(&session.tracks, session.index) else {
    return;
  };
  let Ok(track_id) = track_id_from_uri(&uri).map(str::to_string) else {
    return;
  };
  let app = Arc::clone(app);
  let source = Arc::clone(&session.source);
  tokio::spawn(async move {
    let track = match source.song_info(&track_id).await {
      Ok(track) => track,
      Err(e) => {
        log::info!("subsonic: no getSong metadata for {uri}: {e:#}");
        return;
      }
    };
    let mut guard = app.lock().await;
    // A new session (another server, or a fresh start) builds a new source:
    // the same URI there is a different track, so drop this result.
    if let Some(s) = guard
      .subsonic_playback
      .as_mut()
      .filter(|s| Arc::ptr_eq(&s.source, &source))
    {
      let index = s.index;
      apply_resolved(&mut s.tracks, index, &uri, track);
    }
  });
}

/// End the subsonic session, releasing the output device and cleaning up the
/// current tempfile.
async fn teardown_subsonic(app: &Arc<Mutex<App>>) {
  // Take under the lock, stop off it: a sink clear waits for the audio thread.
  let session = app.lock().await.subsonic_playback.take();
  stop_session(session);
}

fn stop_session(session: Option<SubsonicPlaybackState>) {
  if let Some(mut s) = session {
    s.cancel_download();
    // Dropping `s` drops the tempfile (cleanup) and, if it held the last
    // reference, the keepalive thread exits and the device is released.
    Arc::clone(&s.player).stop_detached_holding(s);
  }
}

async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}

#[cfg(test)]
pub(crate) async fn finish_test_download(
  app: &Arc<Mutex<App>>,
  rx: &std::sync::mpsc::Receiver<IoEvent>,
) {
  let event = tokio::time::timeout(super::DOWNLOAD_TIMEOUT, async {
    loop {
      match rx.try_recv() {
        Ok(event) => break event,
        Err(std::sync::mpsc::TryRecvError::Empty) => {
          tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(e) => panic!("{e}"),
      }
    }
  })
  .await
  .expect("the download should send a completion");
  let IoEvent::SubsonicDownloadFinished(id, result) = event else {
    panic!("expected a Subsonic download completion");
  };
  finish_download(app, id, result).await;
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
  use tokio::net::{TcpListener, TcpStream};

  const LOOPBACK_TIMEOUT: Duration = Duration::from_secs(5);

  async fn loopback_context() -> (Arc<Mutex<App>>, TcpListener, Vec<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let mut app = App::default();
    app.user_config.behavior.subsonic_url = Some(base.clone());
    let uris: Vec<_> = (0..3).map(|i| format!("subsonic:track:{i}")).collect();
    app.subsonic_playback = Some(SubsonicPlaybackState {
      player: Arc::new(LocalPlayer::disconnected_for_test()),
      source: Arc::new(SubsonicSource::new(base, "alice", String::new())),
      tracks: snapshot_tracks(&[], None, &uris),
      index: 0,
      advancing: false,
      tempfile: None,
      shuffle_backup: None,
      resume_at: None,
      fetch: None,
    });
    (Arc::new(Mutex::new(app)), listener, uris)
  }

  /// Keep a response body open after sending its first bytes.
  async fn stalled_audio(listener: &TcpListener, id: usize) -> TcpStream {
    tokio::time::timeout(LOOPBACK_TIMEOUT, async {
      let (mut stream, _) = listener.accept().await.unwrap();
      let mut reader = BufReader::new(&mut stream);
      let mut line = String::new();
      reader.read_line(&mut line).await.unwrap();
      assert!(line.starts_with("GET /rest/stream.view?"));
      assert!(
        line.contains(&format!("&id={id} ")),
        "expected a download for track {id}"
      );
      while line != "\r\n" {
        line.clear();
        assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
      }
      stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nID3")
        .await
        .unwrap();
      stream
    })
    .await
    .expect("expected an audio request")
  }

  async fn route_without_waiting(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
    tokio::time::timeout(LOOPBACK_TIMEOUT, route_subsonic_event(app, event))
      .await
      .expect("the source router must return while the response is stalled")
  }

  async fn assert_download_cancelled(mut stream: TcpStream) {
    let mut byte = [0];
    let received = tokio::time::timeout(LOOPBACK_TIMEOUT, stream.read(&mut byte))
      .await
      .expect("the old HTTP request should be cancelled");
    match received {
      Ok(0) => (),
      Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => (),
      other => panic!("expected the old connection to close, got {other:?}"),
    }
  }

  #[tokio::test]
  async fn subsonic_context_downloads_leave_transport_responsive() {
    let (app, listener, uris) = loopback_context().await;
    let start = IoEvent::StartPlayback(None, Some(uris), Some(0));
    assert!(route_without_waiting(&app, &start).await);
    let first = stalled_audio(&listener, 0).await;

    assert!(route_without_waiting(&app, &IoEvent::PausePlayback).await);
    assert!(route_without_waiting(&app, &IoEvent::Seek(5000)).await);
    assert_eq!(
      app
        .lock()
        .await
        .subsonic_playback
        .as_ref()
        .unwrap()
        .resume_at,
      Some(ResumePoint {
        position_ms: 5000,
        paused: true
      })
    );

    assert!(route_without_waiting(&app, &IoEvent::NextTrack).await);
    let second = stalled_audio(&listener, 1).await;
    assert_download_cancelled(first).await;
    assert!(route_without_waiting(&app, &IoEvent::NextTrack).await);
    let _third = stalled_audio(&listener, 2).await;
    assert_download_cancelled(second).await;
    assert!(route_without_waiting(&app, &IoEvent::NextTrack).await);
    let guard = app.lock().await;
    let session = guard.subsonic_playback.as_ref().unwrap();
    assert_eq!(session.requested_index(), 2);
    assert!(session.advancing);
  }

  #[tokio::test]
  async fn subsonic_completions_after_a_skip_or_handoff_are_ignored() {
    #[derive(Debug)]
    enum Handoff {
      Skip,
      Queue,
      OtherSource,
    }

    for handoff in [Handoff::Skip, Handoff::Queue, Handoff::OtherSource] {
      let (app, listener, _) = loopback_context().await;
      assert!(route_without_waiting(&app, &IoEvent::NextTrack).await);
      let old = stalled_audio(&listener, 1).await;
      let id = app
        .lock()
        .await
        .subsonic_playback
        .as_ref()
        .unwrap()
        .fetch
        .as_ref()
        .unwrap()
        .id;
      match handoff {
        Handoff::Skip => assert!(route_without_waiting(&app, &IoEvent::PreviousTrack).await),
        Handoff::Queue => {
          app.lock().await.suspend_active_decoded_context_mid_track();
          // A transport event already in the pump must not start another
          // context download while the queue is taking over.
          assert!(route_without_waiting(&app, &IoEvent::NextTrack).await);
        }
        Handoff::OtherSource => {
          let event = IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None);
          assert!(!route_without_waiting(&app, &event).await);
        }
      }
      assert_download_cancelled(old).await;

      // A result already queued on the pump can still arrive after cancellation.
      for result in [
        Ok(NamedTempFile::new().unwrap()),
        Err(anyhow::anyhow!("old request failed")),
      ] {
        finish_download(&app, id, result).await;
        let guard = app.lock().await;
        assert!(guard.status_message().is_none(), "{handoff:?}");
        match handoff {
          Handoff::Skip => {
            let session = guard.subsonic_playback.as_ref().unwrap();
            assert_eq!(session.requested_index(), 0);
            assert!(session.fetch.is_some());
          }
          Handoff::Queue => {
            assert!(guard.subsonic_playback.as_ref().unwrap().fetch.is_none());
            assert!(matches!(
              guard.queue_suspended,
              Some(crate::core::queue::SuspendedContext::Subsonic {
                resume_index: Some(1),
                ..
              })
            ));
          }
          Handoff::OtherSource => assert!(guard.subsonic_playback.is_none()),
        }
      }
    }
  }

  /// End-to-end dispatch test: drive `route_subsonic_event` exactly as the
  /// runtime pump does — browse, play, advance the queue, and tear down on a
  /// foreign start. Exercises the dispatch glue/state machine the direct-client
  /// smoke tests bypass. Ignored (needs network **and** an audio device); run:
  /// `cargo test --features subsonic -- --ignored live_dispatch`
  #[tokio::test]
  #[ignore = "hits the live demo server AND requires an audio output device"]
  async fn live_dispatch_browse_play_and_advance() {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user_config.behavior.subsonic_url = Some("https://demo.navidrome.org".to_string());
    app.user_config.behavior.subsonic_username = Some("demo".to_string());
    app.user_config.behavior.subsonic_password = Some("demo".to_string());
    let app = Arc::new(Mutex::new(app));

    // Browse playlists, then a playlist's tracks into the shared table.
    assert!(route_subsonic_event(&app, &IoEvent::GetSubsonicPlaylists).await);
    let playlist_uri = app
      .lock()
      .await
      .subsonic_playlists()
      .first()
      .expect("demo has playlists")
      .uri
      .clone();
    assert!(route_subsonic_event(&app, &IoEvent::GetSubsonicTracks(playlist_uri)).await);
    let uris: Vec<String> = app
      .lock()
      .await
      .track_table
      .tracks
      .iter()
      .filter_map(|t| t.uri.clone())
      .collect();
    assert!(
      uris.len() >= 2,
      "need a multi-track playlist to test advance"
    );

    // Start the queue at index 0 — downloads + plays the first track.
    assert!(
      route_subsonic_event(&app, &IoEvent::StartPlayback(None, Some(uris), Some(0))).await,
      "subsonic StartPlayback must be consumed"
    );
    finish_test_download(&app, &rx).await;
    {
      let guard = app.lock().await;
      let s = guard.subsonic_playback.as_ref().expect("session published");
      assert_eq!(s.index, 0);
      assert!(!s.player.is_paused(), "should be playing");
    }

    // Advance — downloads + plays the next track, moving the index.
    assert!(route_subsonic_event(&app, &IoEvent::NextTrack).await);
    finish_test_download(&app, &rx).await;
    {
      let guard = app.lock().await;
      let s = guard
        .subsonic_playback
        .as_ref()
        .expect("session still active");
      assert_eq!(s.index, 1, "Next should advance the queue index");
    }

    // A foreign (Spotify) start tears the session down and falls through.
    assert!(
      !route_subsonic_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None)
      )
      .await,
      "a non-subsonic start must fall through to the network"
    );
    assert!(
      app.lock().await.subsonic_playback.is_none(),
      "a foreign start must tear down the subsonic session (device handoff)"
    );
  }

  /// Regression for the search->play path: search, then play a result **without**
  /// browsing it into the track table first, so the metadata can only come from
  /// the search results. Before the `snapshot_tracks` fix this published no
  /// session ("No Subsonic tracks to play").
  #[tokio::test]
  #[ignore = "hits the live demo server AND requires an audio output device"]
  async fn live_dispatch_search_to_play() {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user_config.behavior.subsonic_url = Some("https://demo.navidrome.org".to_string());
    app.user_config.behavior.subsonic_username = Some("demo".to_string());
    app.user_config.behavior.subsonic_password = Some("demo".to_string());
    let app = Arc::new(Mutex::new(app));

    assert!(
      route_subsonic_event(&app, &IoEvent::GetSubsonicSearchResults("love".to_string())).await
    );
    let uris: Vec<String> = app
      .lock()
      .await
      .search_results()
      .tracks
      .as_ref()
      .expect("search populated the songs block")
      .items
      .iter()
      .filter_map(|t| t.uri.clone())
      .collect();
    assert!(!uris.is_empty(), "search should return tracks");
    // Track table is empty (never browsed) — metadata must resolve from search.
    assert!(app.lock().await.track_table.tracks.is_empty());

    assert!(route_subsonic_event(&app, &IoEvent::StartPlayback(None, Some(uris), Some(0))).await);
    finish_test_download(&app, &rx).await;
    let guard = app.lock().await;
    let s = guard
      .subsonic_playback
      .as_ref()
      .expect("search->play must publish a session");
    assert!(
      s.current().is_some_and(|t| !t.name.is_empty()),
      "metadata must resolve from the search results"
    );
  }
}

#[cfg(test)]
mod sync_error_localization_tests {
  use super::*;
  #[tokio::test]
  async fn an_unconfigured_sync_source_is_structured_and_keeps_the_english_diagnostic() {
    use crate::core::playlist_sync::SyncReason;
    let mut state = App::default();
    state.user_config.behavior.subsonic_url = None;
    let app = Arc::new(Mutex::new(state));
    let error = match build_sync_source(&app).await {
      Err(error) => error,
      Ok(_) => panic!("missing URL accepted"),
    };
    assert_eq!(
      error.to_string(),
      "No Subsonic server configured (set behavior.subsonic_url)"
    );
    let reason = SyncReason::from_error(&error);
    assert_eq!(reason, SyncReason::SubsonicNotConfigured);
    for strings in [
      &crate::core::i18n::EN,
      &crate::core::i18n::ZH_HANS,
      &crate::core::i18n::ZH_HANT,
    ] {
      assert_eq!(
        reason.render(strings),
        strings.status.infra.sync_subsonic_not_configured
      );
    }
  }
}
