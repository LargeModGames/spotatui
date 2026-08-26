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
//! librespot fields. A track is a 30 to 200 MB download, so the session is
//! published at once (marked `advancing`, like the native queue slot) and the
//! download runs on a detached task: the pump keeps serving every other event
//! and a skip during the download supersedes it through `fetch_id`.
//!
//! Failures are status messages (never `handle_error`): the CLI never reaches
//! this router, so no exit signal is lost. A 401 clears the in-memory token so
//! the next browse re-runs the browser login. A failed download tears the
//! session down instead of skipping (a skip would walk the queue at tick speed).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::auth::{self, QobuzBundleCache};
use super::stream::download::TrackDownload;
use super::{track_id_from_uri, QobuzPlaybackState, QobuzSource, StreamQuality, Unauthorized};
use crate::core::app::{App, SearchResultBlock, TrackTableContext};
use crate::core::pagination::Paged;
use crate::core::plugin_api::TrackInfo;
use crate::core::source::{MediaSource, Searcher};
use crate::core::state::PersistedRuntimeState;
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;
use crate::infra::queue::{advance_index, replay_file};

const LOGIN_EXPIRED: &str = "Qobuz: login expired, press `d` and pick Qobuz to log in again";
/// Downloads above this size get a status message that names the size.
const LARGE_DOWNLOAD_BYTES: u64 = 20_000_000;

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
      start_qobuz_queue(app, uris, offset.unwrap_or(0)).await;
      true
    }
    // A single Qobuz track with no surrounding list: a one-track queue.
    IoEvent::StartPlayback(Some(uri), _, _) if is_qobuz_uri(uri) => {
      start_qobuz_queue(app, std::slice::from_ref(uri), 0).await;
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
        let _ = p.seek(Duration::from_millis(*position_ms as u64));
        true
      }
      None => false,
    },
    IoEvent::ChangeVolume(volume) => match player(app).await {
      Some(p) => {
        p.set_volume(*volume as f32 / 100.0);
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
    if let Err(e) = open::that(&url) {
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
        match crate::core::paths::qobuz_credentials_path() {
          Some(path) => {
            if let Err(e) = auth::write_credentials(&path, &credentials) {
              log::warn!("[qobuz] cannot save credentials: {e:#}");
            }
          }
          None => log::warn!("[qobuz] no config directory to save credentials in"),
        }
        auth::set_token(Some(credentials.user_auth_token));
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
    Ok(results) => {
      let total = results.tracks.len() as u32;
      let mut app = app.lock().await;
      app.search_results.tracks = Some(Paged {
        items: results.tracks,
        total,
        ..Default::default()
      });
      app.search_results.albums = None;
      app.search_results.artists = None;
      app.search_results.playlists = None;
      app.search_results.shows = None;
      app.search_results.selected_tracks_index = Some(0);
      app.search_results.hovered_block = SearchResultBlock::SongSearch;
      app.search_results.selected_block = SearchResultBlock::Empty;
    }
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

/// Snapshot the `TrackInfo`s for `uris` in order from the track table, then
/// the search results (a play can come from either view). Unknown URIs are dropped.
fn snapshot_tracks(
  table: &[TrackInfo],
  search: Option<&[TrackInfo]>,
  uris: &[String],
) -> Vec<TrackInfo> {
  uris
    .iter()
    .filter_map(|uri| find_track(table, search, uri).cloned())
    .collect()
}

fn find_track<'a>(
  table: &'a [TrackInfo],
  search: Option<&'a [TrackInfo]>,
  uri: &str,
) -> Option<&'a TrackInfo> {
  let matches = |t: &&TrackInfo| t.uri.as_deref() == Some(uri);
  table
    .iter()
    .find(matches)
    .or_else(|| search.and_then(|s| s.iter().find(matches)))
}

/// Release every other backend so only Qobuz holds the output device.
#[cfg_attr(
  not(any(
    feature = "streaming",
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  )),
  allow(unused_variables)
)]
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
  #[cfg(feature = "local-files")]
  {
    let local = app.lock().await.local_playback.take();
    if let Some(local) = local {
      local.player.stop();
    }
  }
  #[cfg(feature = "subsonic")]
  {
    let subsonic = app.lock().await.subsonic_playback.take();
    if let Some(subsonic) = subsonic {
      subsonic.player.stop();
    }
  }
  #[cfg(feature = "internet-radio")]
  {
    let radio = app.lock().await.radio_playback.take();
    if let Some(radio) = radio {
      radio.player.stop();
    }
  }
  #[cfg(feature = "youtube")]
  {
    let youtube = app.lock().await.youtube_playback.take();
    if let Some(youtube) = youtube {
      youtube.player.stop();
    }
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

/// Start a track download into a fresh tempfile: segment 0 is parsed and the
/// header written, so the total size is known before the long part.
async fn begin_track_download(
  source: &QobuzSource,
  track_id: &str,
  quality: u8,
) -> Result<(NamedTempFile, TrackDownload)> {
  let tmp = NamedTempFile::new().context("creating temp file for Qobuz stream")?;
  let download = source.begin_download(track_id, quality, tmp.path()).await?;
  Ok((tmp, download))
}

/// Download the track at `uri` into a tempfile, for the native queue engine's
/// off-pump fetch (playing is the queue's job).
pub(crate) async fn download_for_queue(
  source: &QobuzSource,
  uri: &str,
  quality: u8,
) -> Result<NamedTempFile> {
  let track_id = track_id_from_uri(uri)?.to_string();
  let (tmp, download) = begin_track_download(source, &track_id, quality).await?;
  download.finish().await?;
  Ok(tmp)
}

/// Whether the published session still waits for the download stamped `fetch_id`.
async fn is_current(app: &Arc<Mutex<App>>, fetch_id: u64) -> bool {
  app
    .lock()
    .await
    .qobuz_playback
    .as_ref()
    .is_some_and(|s| s.fetch_id == fetch_id)
}

/// Run the download for the published session on a detached task, then play
/// it when the session still waits for this fetch. The task's abort handle is
/// stored on the session, so a skip, a new queue, or a teardown cancels it.
async fn spawn_fetch(
  app: &Arc<Mutex<App>>,
  source: Arc<QobuzSource>,
  track_id: String,
  fetch_id: u64,
) {
  let task_app = Arc::clone(app);
  let task = tokio::spawn(async move {
    let app = task_app;
    let quality = app.lock().await.user_config.behavior.qobuz_quality;
    let (tmp, download) = match begin_track_download(&source, &track_id, quality).await {
      Ok(started) => started,
      Err(e) => return fail_fetch(&app, fetch_id, e).await,
    };
    // The size is known after segment 0, before the long part.
    let total = download.total_bytes();
    if total > LARGE_DOWNLOAD_BYTES && is_current(&app, fetch_id).await {
      app
        .lock()
        .await
        .set_status_message(format!("Qobuz: downloading {} MB", total / 1_000_000), 6);
    }
    let delivered = download.quality();
    if let Err(e) = download.finish().await {
      return fail_fetch(&app, fetch_id, e).await;
    }
    commit_fetch(&app, fetch_id, tmp, delivered).await;
  });
  if let Some(s) = app
    .lock()
    .await
    .qobuz_playback
    .as_mut()
    .filter(|s| s.fetch_id == fetch_id)
  {
    s.fetch = Some(task.abort_handle());
  }
}

/// A failed download: tear the session down only if it still waits for this
/// fetch, under one lock, and report the error from the same guard.
async fn fail_fetch(app: &Arc<Mutex<App>>, fetch_id: u64, err: anyhow::Error) {
  let mut guard = app.lock().await;
  let Some(s) = guard.qobuz_playback.take_if(|s| s.fetch_id == fetch_id) else {
    return;
  };
  s.player.stop();
  report_locked(&mut guard, "download", err);
}

/// Play the downloaded file and finalize the session, under one lock so a
/// concurrent skip (which restamps `fetch_id` under the same lock) cannot
/// interleave. A decode failure tears the session down.
async fn commit_fetch(
  app: &Arc<Mutex<App>>,
  fetch_id: u64,
  tmp: NamedTempFile,
  quality: StreamQuality,
) {
  let mut guard = app.lock().await;
  let Some((player, was_paused)) = guard
    .qobuz_playback
    .as_ref()
    .filter(|s| s.fetch_id == fetch_id)
    // A pause pressed during the download window applies to the previous
    // track's sink; a fresh player starts paused, so only a session that
    // already played something counts.
    .map(|s| {
      (
        Arc::clone(&s.player),
        s.tempfile.is_some() && s.player.is_paused(),
      )
    })
  else {
    return;
  };
  let path = tmp.path().to_path_buf();
  let decode_player = Arc::clone(&player);
  let played = tokio::task::spawn_blocking(move || decode_player.play_file(&path))
    .await
    .map(|r| r.map_err(|e| e.to_string()))
    .unwrap_or_else(|e| Err(e.to_string()));
  if let Err(e) = played {
    if let Some(s) = guard.qobuz_playback.take_if(|s| s.fetch_id == fetch_id) {
      s.player.stop();
    }
    guard.set_status_message(format!("Qobuz: play: {e}"), 6);
    return;
  }
  player.set_volume(guard.runtime_state.volume_percent as f32 / 100.0);
  let display = match guard.qobuz_playback.as_mut() {
    Some(s) => {
      s.tempfile = Some(tmp);
      s.quality = Some(quality);
      s.advancing = false;
      s.fetch = None;
      let resume = s.resume_at.take();
      if let Some(position_ms) = resume.map(|r| r.position_ms).filter(|&ms| ms > 0) {
        let _ = player.seek(Duration::from_millis(position_ms));
      }
      if was_paused || resume.is_some_and(|r| r.paused) {
        player.pause();
      }
      s.current().map(|t| t.name.clone())
    }
    None => None,
  };
  if let Some(display) = display {
    guard.set_status_message(format!("\u{266a} {display}"), 4);
  }
}

/// Begin playing a list of Qobuz tracks, taking over the session and starting
/// at `start_idx` (clamped into range).
async fn start_qobuz_queue(app: &Arc<Mutex<App>>, uris: &[String], start_idx: usize) {
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
  let fetch_id = next_fetch_id();
  {
    let mut guard = app.lock().await;
    let mut state = QobuzPlaybackState {
      player,
      source: Arc::clone(&source),
      tracks,
      index,
      advancing: true,
      tempfile: None,
      quality: None,
      shuffle_backup: None,
      fetch_id,
      resume_at: None,
      fetch: None,
    };
    // Honor the player-global decoded shuffle for the freshly built queue.
    if guard.decoded_shuffle {
      state.set_shuffle(true);
    }
    // Dropping a previous session aborts its download.
    guard.qobuz_playback = Some(state);
  }
  spawn_fetch(app, source, track_id, fetch_id).await;
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
    Some(idx) => play_index(app, idx).await,
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
    s.tempfile
      .as_ref()
      .map(|t| (Arc::clone(&s.player), t.path().to_path_buf()))
  };
  let Some((player, path)) = replay else {
    // Still downloading: the commit clears `advancing` and starts playback.
    return true;
  };
  if replay_file(player, path).await {
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
/// auto-advance, and the native queue's resume.
pub(crate) async fn play_index(app: &Arc<Mutex<App>>, target: usize) {
  let plan = {
    let mut guard = app.lock().await;
    let Some(s) = guard.qobuz_playback.as_mut() else {
      return; // session torn down between dispatch and here
    };
    match s.tracks.get(target) {
      None => {
        s.advancing = false;
        return;
      }
      Some(track) => match track.uri.as_deref().map(track_id_from_uri) {
        Some(Ok(id)) => {
          let fetch_id = next_fetch_id();
          // Cancel a superseded download and forget its restore point.
          if let Some(fetch) = s.fetch.take() {
            fetch.abort();
          }
          s.resume_at = None;
          s.quality = None;
          s.index = target;
          s.advancing = true;
          s.fetch_id = fetch_id;
          Ok((Arc::clone(&s.source), id.to_string(), fetch_id))
        }
        _ => Err(()),
      },
    }
  };
  match plan {
    Ok((source, track_id, fetch_id)) => spawn_fetch(app, source, track_id, fetch_id).await,
    Err(()) => {
      teardown_qobuz(app).await;
      set_error(app, "Invalid Qobuz track URI".to_string()).await;
    }
  }
}

/// End the Qobuz session, releasing the output device and the tempfile.
async fn teardown_qobuz(app: &Arc<Mutex<App>>) {
  if let Some(s) = app.lock().await.qobuz_playback.take() {
    s.player.stop();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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

  #[test]
  fn snapshot_finds_tracks_in_table_preserving_order() {
    let table = vec![track("qobuz:track:a", "A"), track("qobuz:track:b", "B")];
    let snap = snapshot_tracks(
      &table,
      None,
      &["qobuz:track:b".to_string(), "qobuz:track:a".to_string()],
    );
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].name, "B");
    assert_eq!(snap[1].name, "A");
  }

  #[test]
  fn snapshot_falls_back_to_search_results_for_search_to_play() {
    let table = vec![track("qobuz:track:browsed", "Browsed")];
    let search = vec![track("qobuz:track:searched", "Searched")];
    let snap = snapshot_tracks(&table, Some(&search), &["qobuz:track:searched".to_string()]);
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].name, "Searched");
  }

  #[test]
  fn snapshot_drops_unknown_uris() {
    let table = vec![track("qobuz:track:a", "A")];
    let snap = snapshot_tracks(&table, None, &["qobuz:track:missing".to_string()]);
    assert!(snap.is_empty());
  }

  #[test]
  fn qobuz_uris_are_recognised_by_scheme() {
    assert!(is_qobuz_uri("qobuz:track:1"));
    assert!(!is_qobuz_uri("subsonic:track:1"));
  }
}
