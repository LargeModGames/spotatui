//! Qobuz browse/search/login/playback routing.
//!
//! The seam that keeps the Spotify [`Network`](crate::infra::network)
//! Spotify-only: [`route_qobuz_event`] is called from the runtime IoEvent pump
//! after the Subsonic dispatch and before Radio. An event that targets the
//! Qobuz source (a browse, search, or login request, or a `qobuz:` playback
//! URI) is handled here and consumed; anything else falls through.
//!
//! ## Playback
//!
//! Qobuz playback owns [`App::qobuz_playback`] and never writes Spotify or
//! librespot fields. A track plays while it downloads (`stream::progressive`):
//! the session is published at once (marked `advancing`, like the native queue
//! slot) and a detached task fetches segment 0, opens the stream, and builds
//! the decoder, which waits for the first bytes. The pump keeps serving every
//! other event, and a skip during that window supersedes it through
//! `fetch_id`; the superseded stream is dropped, which cancels its download.
//!
//! Failures are status messages (never `handle_error`): the CLI never reaches
//! this router, so no exit signal is lost. A 401 clears the in-memory token so
//! the next browse re-runs the browser login. A failure before the first bytes
//! tears the session down instead of skipping (a skip would walk the queue at
//! tick speed); a failure after that ends the track early.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::auth::{self, QobuzBundleCache};
use super::stream::progressive;
use super::{
  track_id_from_uri, QobuzPlaybackState, QobuzSource, ResumePoint, StreamQuality, Unauthorized,
};
use crate::core::app::{App, TrackTableContext};
use crate::core::source::{MediaSource, Searcher, Source};
use crate::core::state::PersistedRuntimeState;
use crate::infra::audio::{LocalPlayer, PreparedStream};
use crate::infra::network::IoEvent;
use crate::infra::queue::{advance_index, replay_file, snapshot_tracks};

const LOGIN_EXPIRED: &str = "Qobuz: login expired, press `d` and pick Qobuz to log in again";

/// Whether a URI is owned by the Qobuz source.
pub fn is_qobuz_uri(uri: &str) -> bool {
  uri.starts_with("qobuz:")
}

/// Skip direction within the queue.
#[derive(Clone, Copy)]
enum Direction {
  Next,
  Prev,
}

/// Intercept events that target the Qobuz source.
///
/// Returns `true` if the event was handled (and must **not** be forwarded to
/// the Spotify network), `false` to let the normal dispatch run.
pub async fn route_qobuz_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    IoEvent::GetQobuzPlaylists => {
      load_qobuz_playlists(app).await;
      true
    }
    IoEvent::GetQobuzTracks(uri) => {
      load_qobuz_tracks(app, uri).await;
      true
    }
    IoEvent::GetQobuzSearchResults(query) => {
      run_qobuz_search(app, query).await;
      true
    }
    IoEvent::QobuzLogin => {
      begin_login(app).await;
      true
    }
    // Start a list of Qobuz tracks: queue all and start at the offset.
    IoEvent::StartPlayback(None, Some(uris), offset)
      if uris.first().is_some_and(|u| is_qobuz_uri(u)) =>
    {
      start_qobuz_queue(app, uris, offset.unwrap_or(0), None).await;
      true
    }
    // A single Qobuz track with no surrounding list: a one-track queue.
    IoEvent::StartPlayback(Some(uri), _, _) if is_qobuz_uri(uri) => {
      start_qobuz_queue(app, std::slice::from_ref(uri), 0, None).await;
      true
    }
    // Bare "resume current": ours only while Qobuz owns the session.
    IoEvent::StartPlayback(None, None, None) => match player(app).await {
      Some(p) => {
        p.resume();
        true
      }
      None => false,
    },
    // Any other start is a foreign play: relinquish the device, then let the
    // normal dispatch run.
    IoEvent::StartPlayback(..) => {
      teardown_qobuz(app).await;
      false
    }
    IoEvent::PausePlayback => match player(app).await {
      Some(p) => {
        p.pause();
        true
      }
      None => false,
    },
    IoEvent::Seek(position_ms) => match player(app).await {
      Some(p) => {
        // A seek past the downloaded part waits for that segment: off the pump.
        let position = Duration::from_millis(*position_ms as u64);
        tokio::task::spawn_blocking(move || {
          let _ = p.seek(position);
        });
        true
      }
      None => false,
    },
    IoEvent::ChangeVolume(volume) => match player(app).await {
      Some(p) => {
        p.set_volume(*volume);
        let mut app = app.lock().await;
        app.runtime_state.volume_percent = *volume;
        app.schedule_state_save(PersistedRuntimeState::volume_percent(*volume));
        true
      }
      None => false,
    },
    IoEvent::NextTrack => skip(app, Direction::Next).await,
    IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => skip(app, Direction::Prev).await,
    IoEvent::ReplayCurrentTrack => replay_current(app).await,
    IoEvent::Repeat(state) => app.lock().await.set_decoded_repeat_from_state(*state),
    _ => false,
  }
}

// ---------------------------------------------------------------------------
// Constants, token, source
// ---------------------------------------------------------------------------

/// The bundle cache saved in `state.yml`, when the app has a state path.
async fn cached_bundle(app: &Arc<Mutex<App>>) -> Option<QobuzBundleCache> {
  let path = app.lock().await.state_path.clone()?;
  crate::core::state::load(&path).ok()?.qobuz_bundle_cache
}

/// The bundle constants; a fresh scrape is saved into `state.yml`. The first
/// call of a process runs the login-page GET (and, on a new bundle version, the
/// 9 MB bundle GET) inline on the serial pump; accepted for v1.
async fn constants(app: &Arc<Mutex<App>>) -> Result<QobuzBundleCache> {
  let http = super::shared_qobuz_client();
  if auth::constants_resolved() {
    return auth::resolve_constants(&http, None).await;
  }
  let cached = cached_bundle(app).await;
  let resolved = auth::resolve_constants(&http, cached.as_ref()).await?;
  if cached.as_ref() != Some(&resolved) && resolved.bundle_version != "env" {
    app
      .lock()
      .await
      .schedule_state_save(PersistedRuntimeState::qobuz_bundle_cache(resolved.clone()));
  }
  Ok(resolved)
}

/// What to do when no token is available.
#[derive(Clone, Copy)]
enum WhenLoggedOut {
  /// Browse paths start the browser login.
  Login,
  /// Playback paths only show the login message.
  Message,
}

/// Build a [`QobuzSource`] from the in-memory token, or `None` after handling
/// the logged-out case as `when_logged_out` says.
async fn build_source(
  app: &Arc<Mutex<App>>,
  when_logged_out: WhenLoggedOut,
) -> Option<QobuzSource> {
  let Some(token) = auth::current_token() else {
    match when_logged_out {
      WhenLoggedOut::Login => app.lock().await.dispatch(IoEvent::QobuzLogin),
      WhenLoggedOut::Message => set_error(app, LOGIN_EXPIRED.to_string()).await,
    }
    return None;
  };
  match constants(app).await {
    Ok(c) => Some(QobuzSource::new(c.app_id, c.app_secret, token)),
    Err(e) => {
      report(app, "web player constants", e).await;
      None
    }
  }
}

/// A source for a playback path (the native queue lane); shows the login
/// message and returns `None` when logged out.
pub(crate) async fn build_playback_source(app: &Arc<Mutex<App>>) -> Option<QobuzSource> {
  build_source(app, WhenLoggedOut::Message).await
}

/// Report a failed call as one status message; a 401 also clears the token.
async fn report(app: &Arc<Mutex<App>>, step: &str, err: anyhow::Error) {
  let mut guard = app.lock().await;
  report_locked(&mut guard, step, err);
}

fn report_locked(app: &mut App, step: &str, err: anyhow::Error) {
  if err.downcast_ref::<Unauthorized>().is_some() {
    auth::set_token(None);
    app.set_error_status_message(LOGIN_EXPIRED, 8);
  } else {
    log::warn!("[qobuz] {step}: {err:#}");
    app.set_status_message(format!("Qobuz: {step}: {err:#}"), 6);
  }
}

/// Deliberate divergence from `handle_error`: a status message, no error route.
async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

static LOGIN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Start the in-TUI browser login on a detached task so the pump keeps running.
async fn begin_login(app: &Arc<Mutex<App>>) {
  if LOGIN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
    set_error(app, "Qobuz login already in progress...".to_string()).await;
    return;
  }
  let constants = match constants(app).await {
    Ok(c) => c,
    Err(e) => {
      LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
      report(app, "web player constants", e).await;
      return;
    }
  };
  let attempt = match auth::LoginAttempt::bind(constants).await {
    Ok(a) => a,
    Err(e) => {
      LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
      report(app, "login", e).await;
      return;
    }
  };
  let url = attempt.url();
  {
    let mut guard = app.lock().await;
    if let Err(e) = open::that_detached(&url) {
      log::warn!("[qobuz] failed to open browser automatically: {e}");
      guard.set_status_message(
        format!("Open this URL in your browser to log in to Qobuz: {url}"),
        30,
      );
    } else {
      guard.set_status_message("Qobuz: open the browser window to log in", 12);
    }
  }
  let io_tx = app.lock().await.io_tx_clone();
  let app = Arc::clone(app);
  tokio::spawn(async move {
    match attempt.wait().await {
      Ok(credentials) => {
        if let Err(e) = auth::save_login(&credentials) {
          log::warn!("[qobuz] cannot save credentials: {e:#}");
        }
        app.lock().await.set_status_message("Qobuz: logged in", 4);
        if let Some(tx) = io_tx {
          let _ = tx.send(IoEvent::GetQobuzPlaylists);
        }
      }
      Err(e) => {
        log::warn!("[qobuz] login failed: {e:#}");
        app
          .lock()
          .await
          .set_status_message(format!("Qobuz login failed: {e:#}"), 8);
      }
    }
    LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
  });
}

// ---------------------------------------------------------------------------
// Browse + search
// ---------------------------------------------------------------------------

/// Fetch the sidebar rows (favorites, playlists, albums) into `app.qobuz_playlists`.
async fn load_qobuz_playlists(app: &Arc<Mutex<App>>) {
  let Some(source) = build_source(app, WhenLoggedOut::Login).await else {
    return;
  };
  match source.playlists().await {
    Ok(playlists) => app.lock().await.qobuz_playlists = playlists,
    Err(e) => report(app, "library", e).await,
  }
}

/// Fetch a listing's tracks into the shared track table, tagged
/// [`TrackTableContext::QobuzPlaylist`] so selecting a row plays it.
async fn load_qobuz_tracks(app: &Arc<Mutex<App>>, playlist_uri: &str) {
  let Some(source) = build_source(app, WhenLoggedOut::Login).await else {
    return;
  };
  match source.tracks(playlist_uri).await {
    Ok(tracks) => {
      let mut app = app.lock().await;
      app.track_table.tracks = tracks;
      app.track_table.selected_index = 0;
      app.track_table.context = Some(TrackTableContext::QobuzPlaylist);
    }
    Err(e) => report(app, "tracks", e).await,
  }
}

/// Run a catalog search and populate the songs block of `app.search_results`.
async fn run_qobuz_search(app: &Arc<Mutex<App>>, query: &str) {
  let Some(source) = build_source(app, WhenLoggedOut::Login).await else {
    return;
  };
  match source.search(query).await {
    Ok(results) => app.lock().await.show_source_search_tracks(results.tracks),
    Err(e) => report(app, "search", e).await,
  }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// The live Qobuz player, if a Qobuz session is active.
async fn player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app
    .lock()
    .await
    .qobuz_playback
    .as_ref()
    .map(|s| Arc::clone(&s.player))
}

/// Release every other backend so only Qobuz holds the output device.
async fn release_other_backends(app: &Arc<Mutex<App>>) {
  // Pause native Spotify so librespot releases the device.
  #[cfg(feature = "streaming")]
  {
    let streaming = app.lock().await.streaming_player.clone();
    if let Some(player) = streaming {
      player.pause();
    }
  }
  // The other decoded sources never see this `qobuz:` start (the pump
  // short-circuits), so their sessions are torn down here.
  let players = app.lock().await.take_decoded_sessions_except(Source::Qobuz);
  for player in players {
    player.stop_detached();
  }
}

/// Reuse the live Qobuz player, or open a fresh output device for one. A
/// freshly opened player is **not** published to `App` here.
async fn acquire_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(p) = player(app).await {
    return Some(p);
  }
  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_error(app, format!("No audio output for Qobuz playback: {e}")).await;
      None
    }
    Err(e) => {
      set_error(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

static FETCH_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_fetch_id() -> u64 {
  FETCH_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Download the track at `uri` into a tempfile, for the native queue engine's
/// off-pump fetch (playing is the queue's job), and return its delivered
/// format label. Reads the progressive stream to its end, so its per-segment
/// retries apply here too.
pub(crate) async fn download_for_queue(
  source: &QobuzSource,
  uri: &str,
  quality: u8,
) -> Result<(NamedTempFile, String)> {
  let track_id = track_id_from_uri(uri)?;
  let tmp = NamedTempFile::new().context("creating temp file for Qobuz stream")?;
  let (init, stream) = source.begin_stream(track_id, quality).await?;
  let mut reader = progressive::open(stream, &tmp).await?;
  tokio::task::spawn_blocking(move || std::io::copy(&mut reader, &mut std::io::sink()))
    .await
    .context("download task")?
    .context("downloading the Qobuz stream")?;
  Ok((tmp, init.quality().label()))
}

/// Open the track's stream into a fresh tempfile and build its decoder, whose
/// first read waits for the prefetch.
async fn prepare_track(
  source: &QobuzSource,
  track_id: &str,
  quality: u8,
) -> Result<(NamedTempFile, StreamQuality, PreparedStream)> {
  let tmp = NamedTempFile::new().context("creating temp file for Qobuz stream")?;
  let (init, stream) = source.begin_stream(track_id, quality).await?;
  let delivered = init.quality();
  let total = init.total_bytes();
  let reader = progressive::open(stream, &tmp).await?;
  let mime = if delivered.flac {
    "audio/flac"
  } else {
    "audio/mpeg"
  };
  let prepared = tokio::task::spawn_blocking(move || {
    LocalPlayer::prepare_stream(reader, Some(mime), Some(total))
  })
  .await
  .context("decoder task")??;
  Ok((tmp, delivered, prepared))
}

/// Open the track's stream on a detached task and build its decoder, then
/// play it when the session still waits for this fetch. Called under the
/// `App` lock that stamped `session.fetch_id`, so the abort handle is in
/// place before any skip looks for it: a skip, a new queue, a queue takeover,
/// or a teardown cancels the task, and a decoder build already in progress
/// ends on its own and drops its stream, which cancels that download.
fn spawn_fetch(
  app: &Arc<Mutex<App>>,
  session: &mut QobuzPlaybackState,
  track_id: String,
  quality: u8,
) {
  let app = Arc::clone(app);
  let source = Arc::clone(&session.source);
  let fetch_id = session.fetch_id;
  let task = tokio::spawn(async move {
    match prepare_track(&source, &track_id, quality).await {
      Ok((tmp, delivered, prepared)) => {
        commit_fetch(&app, fetch_id, tmp, delivered, prepared).await
      }
      Err(e) => fail_fetch(&app, fetch_id, "stream", e).await,
    }
  });
  session.fetch = Some(task.abort_handle());
}

/// A failed fetch: tear the session down only if it still waits for this
/// fetch, and report the error. Under the native queue the session stays for
/// the queue's resume, which fetches again.
async fn fail_fetch(app: &Arc<Mutex<App>>, fetch_id: u64, step: &str, err: anyhow::Error) {
  let mut guard = app.lock().await;
  if guard
    .qobuz_playback
    .as_ref()
    .is_none_or(|s| s.fetch_id != fetch_id)
  {
    return;
  }
  if guard.queue_owns_playback() {
    log::warn!("[qobuz] {step}: {err:#}");
    return;
  }
  let session = guard.qobuz_playback.take();
  report_locked(&mut guard, step, err);
  drop(guard);
  if let Some(s) = session {
    Arc::clone(&s.player).stop_detached();
  }
}

/// Play the prepared stream and finalize the session. The previous track is
/// cleared off the `App` lock first (the sink clear waits for the audio
/// thread, which a stalled stream holds), then the session is checked again
/// under the lock so a concurrent skip (which restamps `fetch_id`) cannot
/// interleave. A stale stream is dropped, which cancels its download.
async fn commit_fetch(
  app: &Arc<Mutex<App>>,
  fetch_id: u64,
  tmp: NamedTempFile,
  quality: StreamQuality,
  prepared: PreparedStream,
) {
  let claimed = {
    let guard = app.lock().await;
    // The native queue owns the sink: the session stays for its resume.
    if guard.queue_owns_playback() {
      return;
    }
    guard
      .qobuz_playback
      .as_ref()
      .filter(|s| s.fetch_id == fetch_id)
      // A pause pressed during the fetch window applies to the previous
      // track's sink; a fresh player starts paused, so only a session that
      // already played something counts.
      .map(|s| {
        (
          Arc::clone(&s.player),
          s.tempfile.is_some() && s.player.is_paused(),
        )
      })
  };
  let Some((player, was_paused)) = claimed else {
    return;
  };
  let stop_player = Arc::clone(&player);
  if tokio::task::spawn_blocking(move || stop_player.stop())
    .await
    .is_err()
  {
    return;
  }
  // Claim the session under the lock, stage off it: the clear inside
  // `stage_prepared` waits on the audio thread, and the runner takes this
  // lock on every frame.
  let (resume, volume) = {
    let mut guard = app.lock().await;
    let volume = guard.runtime_state.volume_percent;
    let Some(s) = guard
      .qobuz_playback
      .as_mut()
      .filter(|s| s.fetch_id == fetch_id)
    else {
      return;
    };
    s.tempfile = Some(tmp);
    s.quality = Some(quality);
    s.fetch = None;
    (s.resume_at.take(), volume)
  };
  let paused = was_paused || resume.is_some_and(|r| r.paused);
  let stage_player = Arc::clone(&player);
  let staged = tokio::task::spawn_blocking(move || {
    stage_player.stage_prepared(prepared)?;
    stage_player.set_volume(volume);
    if !paused {
      stage_player.resume();
    }
    Ok::<(), anyhow::Error>(())
  })
  .await;
  let staged = match staged {
    Ok(Ok(())) => true,
    Ok(Err(e)) => {
      log::warn!("[qobuz] stage: {e:#}");
      false
    }
    Err(e) => {
      log::warn!("[qobuz] stage task: {e}");
      false
    }
  };
  let mut guard = app.lock().await;
  let Some(s) = guard
    .qobuz_playback
    .as_mut()
    .filter(|s| s.fetch_id == fetch_id)
  else {
    return;
  };
  if !staged {
    // The device went under the stage. Replay the retained file instead,
    // with the same pause; the advance latch stays on until it does.
    s.resume_at = Some(ResumePoint {
      position_ms: resume.map_or(0, |r| r.position_ms),
      paused,
    });
    guard.dispatch(IoEvent::ReplayCurrentTrack);
    return;
  }
  s.advancing = false;
  let display = s.current().map(|t| t.name.clone());
  if let Some(display) = display {
    guard.set_status_message(format!("\u{266a} {display}"), 4);
  }
  drop(guard);
  // The restore seek waits for that part of the download: off the App lock.
  if let Some(position_ms) = resume.map(|r| r.position_ms).filter(|&ms| ms > 0) {
    tokio::task::spawn_blocking(move || {
      let _ = player.seek(Duration::from_millis(position_ms));
    });
  }
}

/// Begin playing a list of Qobuz tracks, taking over the session and starting
/// at `start_idx` (clamped into range). `resume` is applied when the first
/// track plays (session restore), so it is in place before the fetch starts.
pub(crate) async fn start_qobuz_queue(
  app: &Arc<Mutex<App>>,
  uris: &[String],
  start_idx: usize,
  resume: Option<ResumePoint>,
) {
  let tracks = {
    let guard = app.lock().await;
    let search = guard
      .search_results
      .tracks
      .as_ref()
      .map(|p| p.items.as_slice());
    snapshot_tracks(&guard.track_table.tracks, search, uris)
  };
  if tracks.is_empty() {
    set_error(app, "No Qobuz tracks to play".to_string()).await;
    return;
  }
  let index = start_idx.min(tracks.len() - 1);
  let track_id = match tracks[index].uri.as_deref().map(track_id_from_uri) {
    Some(Ok(id)) => id.to_string(),
    _ => {
      set_error(app, "Invalid Qobuz track URI".to_string()).await;
      return;
    }
  };
  let Some(source) = build_source(app, WhenLoggedOut::Message).await else {
    return;
  };
  let source = Arc::new(source);

  // Only one backend owns the device at a time.
  release_other_backends(app).await;
  let Some(player) = acquire_player(app).await else {
    return;
  };

  // Publish the session now, marked advancing, so the playbar and the skip
  // keys see it during the download; `commit_fetch` finalizes it.
  let mut guard = app.lock().await;
  let quality = guard.user_config.behavior.qobuz_quality;
  let mut state = QobuzPlaybackState {
    player,
    source,
    tracks,
    index,
    advancing: true,
    tempfile: None,
    quality: None,
    shuffle_backup: None,
    fetch_id: next_fetch_id(),
    resume_at: resume,
    fetch: None,
  };
  // Honor the player-global decoded shuffle for the freshly built queue.
  if guard.decoded_shuffle {
    state.set_shuffle(true);
  }
  spawn_fetch(app, &mut state, track_id, quality);
  // Dropping a previous session aborts its download.
  guard.qobuz_playback = Some(state);
}

/// Move the queue index in `direction` and play the new track. Returns `true`
/// if Qobuz owns the session (so the event is consumed).
async fn skip(app: &Arc<Mutex<App>>, direction: Direction) -> bool {
  let target = {
    let mut guard = app.lock().await;
    let mode = guard.decoded_repeat;
    let Some(s) = guard.qobuz_playback.as_mut() else {
      return false;
    };
    s.advancing = true;
    let forward = matches!(direction, Direction::Next);
    advance_index(s.index, s.tracks.len(), mode, forward)
  };
  match target {
    Some(idx) => play_index(app, idx, None).await,
    None => {
      // Queue boundary: clear the guard so auto-advance is not wedged off. A
      // pending first download keeps it (the sink is still empty).
      if let Some(s) = app.lock().await.qobuz_playback.as_mut() {
        if s.tempfile.is_some() {
          s.advancing = false;
        }
      }
    }
  }
  true
}

/// Replay the current track (repeat-one) from its retained tempfile, with no
/// second download. Returns `true` if Qobuz owns the session.
async fn replay_current(app: &Arc<Mutex<App>>) -> bool {
  let replay = {
    let mut guard = app.lock().await;
    let Some(s) = guard.qobuz_playback.as_mut() else {
      return false;
    };
    s.advancing = true;
    match s.tempfile.as_ref() {
      Some(t) => Some((
        Arc::clone(&s.player),
        t.path().to_path_buf(),
        s.resume_at.take().or(Some(ResumePoint {
          position_ms: 0,
          paused: s.player.is_paused(),
        })),
      )),
      // Still downloading: the commit applies `resume_at`, clears `advancing`
      // and starts playback.
      None => None,
    }
  };
  let Some((player, path, resume)) = replay else {
    return true;
  };
  if replay_file(player, path, resume).await {
    if let Some(s) = app.lock().await.qobuz_playback.as_mut() {
      s.advancing = false;
    }
  } else {
    teardown_qobuz(app).await;
    set_error(app, "Cannot replay Qobuz track".to_string()).await;
  }
  true
}

/// Play the queued track at `target` in the published session: the index moves
/// at once and the download runs off the pump. Used by Next/Previous, the tick's
/// auto-advance, and the native queue's resume, which passes how the track
/// starts as `resume`.
pub(crate) async fn play_index(app: &Arc<Mutex<App>>, target: usize, resume: Option<ResumePoint>) {
  let mut guard = app.lock().await;
  let quality = guard.user_config.behavior.qobuz_quality;
  let Some(s) = guard.qobuz_playback.as_mut() else {
    return; // session torn down between dispatch and here
  };
  let Some(track) = s.tracks.get(target) else {
    s.advancing = false;
    return;
  };
  let track_id = match track.uri.as_deref().map(track_id_from_uri) {
    Some(Ok(id)) => id.to_string(),
    _ => {
      drop(guard);
      teardown_qobuz(app).await;
      set_error(app, "Invalid Qobuz track URI".to_string()).await;
      return;
    }
  };
  // Cancel a superseded download; the file, format, and restore point of the
  // previous track go with it, so the session never describes two tracks.
  if let Some(fetch) = s.fetch.take() {
    fetch.abort();
  }
  s.tempfile = None;
  s.quality = None;
  s.resume_at = resume;
  s.index = target;
  s.advancing = true;
  s.fetch_id = next_fetch_id();
  spawn_fetch(app, s, track_id, quality);
}

/// End the Qobuz session, releasing the output device and the tempfile. The
/// player stops off the `App` lock.
async fn teardown_qobuz(app: &Arc<Mutex<App>>) {
  let session = app.lock().await.qobuz_playback.take();
  if let Some(s) = session {
    Arc::clone(&s.player).stop_detached_holding(s);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn qobuz_uris_are_recognised_by_scheme() {
    assert!(is_qobuz_uri("qobuz:track:1"));
    assert!(!is_qobuz_uri("subsonic:track:1"));
  }
}
