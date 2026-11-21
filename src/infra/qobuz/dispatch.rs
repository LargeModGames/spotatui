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
//! ## The web-player constants
//!
//! The first Qobuz action of a process needs the bundle constants, a scrape
//! that can take minutes. The pump never waits for it: one scrape runs on its
//! own task ([`Constants`]), a track's fetch task waits for it like for the
//! download, and a browse, search or login request runs on a task of its own
//! until the constants are ready. Work that waited checks that it is still
//! wanted before it writes: the fetch by `fetch_id`, a listing by
//! [`App::source_table_awaits`], a search by [`App::search_is_current`]. Browse
//! and search results also check their login epoch under the token lock, so
//! replacing the login cannot interleave with publishing the result.
//!
//! Failures are status messages (never `handle_error`): the CLI never reaches
//! this router, so no exit signal is lost. A 401 clears its own login's token so
//! the next browse re-runs the browser login. A failure before the first bytes
//! tears the session down instead of skipping (a skip would walk the queue at
//! tick speed); a failure after that ends the track early.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::future::{BoxFuture, FutureExt, Shared};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::auth::{self, QobuzBundleCache};
use super::stream::progressive;
use super::{
  track_id_from_uri, NoFullStream, QobuzPlaybackState, QobuzSource, ResumePoint, StreamQuality,
  Unauthorized,
};
use crate::core::app::{App, TrackTableContext};
use crate::core::plugin_api::{PlaylistInfo, TrackInfo};
use crate::core::source::{MediaSource, Searcher, Source};
use crate::core::state::PersistedRuntimeState;
use crate::infra::audio::{LocalPlayer, PreparedStream};
use crate::infra::network::IoEvent;
use crate::infra::queue::{advance_index, apply_resolved, placeholder_uri, snapshot_tracks};

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
      off_pump_until_ready(web_constants(), load_qobuz_playlists(Arc::clone(app))).await;
      true
    }
    IoEvent::GetQobuzTracks(uri) => {
      let work = load_qobuz_tracks(Arc::clone(app), uri.clone());
      off_pump_until_ready(web_constants(), work).await;
      true
    }
    IoEvent::GetQobuzSearchResults(query) => {
      let work = run_qobuz_search(Arc::clone(app), query.clone());
      off_pump_until_ready(web_constants(), work).await;
      true
    }
    IoEvent::QobuzLogin => {
      off_pump_until_ready(web_constants(), begin_login(Arc::clone(app))).await;
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
    IoEvent::StartPlayback(None, None, None) => app.lock().await.set_qobuz_playing(true),
    // Any other start is a foreign play: relinquish the device, then let the
    // normal dispatch run.
    IoEvent::StartPlayback(..) => {
      teardown_qobuz(app).await;
      false
    }
    IoEvent::PausePlayback => app.lock().await.set_qobuz_playing(false),
    IoEvent::Seek(position_ms) => {
      let player = {
        let mut guard = app.lock().await;
        if !guard.qobuz_owns_playback() {
          return false;
        }
        let Some(s) = guard.qobuz_playback.as_mut() else {
          return false;
        };
        if s.advancing {
          // The sink is silent while the incoming track is being prepared.
          // Only the fetch commit may apply this seek to the incoming track.
          s.resume_at
            .get_or_insert_with(ResumePoint::default)
            .position_ms = u64::from(*position_ms);
          return true;
        }
        Arc::clone(&s.player)
      };
      // A seek past the downloaded part waits for that segment: off the pump.
      let position = Duration::from_millis(u64::from(*position_ms));
      tokio::task::spawn_blocking(move || {
        let _ = player.seek(position);
      });
      true
    }
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

type Resolve =
  dyn Fn(Option<QobuzBundleCache>) -> BoxFuture<'static, Result<QobuzBundleCache>> + Send + Sync;
type Scrape = Shared<BoxFuture<'static, Result<QobuzBundleCache, Arc<anyhow::Error>>>>;

/// The login changed while the constants were scraped: the work that waited
/// for them belongs to the old login and is dropped without a message.
#[derive(Debug)]
struct LoginChanged;
impl std::fmt::Display for LoginChanged {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("the Qobuz login changed")
  }
}
impl std::error::Error for LoginChanged {}

/// The bundle constants of this process. The scrape (a login-page GET and, on
/// a new bundle version, the 9 MB bundle GET) runs on its own task, one at a
/// time: every caller that needs the constants meanwhile waits for that one
/// answer, and a caller that goes away does not stop it. A failed scrape is
/// forgotten, so the next caller starts another.
struct Constants {
  resolve: Box<Resolve>,
  login_epoch: &'static AtomicU64,
  state: std::sync::Mutex<ConstantsState>,
}

#[derive(Default)]
struct ConstantsState {
  ready: Option<QobuzBundleCache>,
  /// The scrape in flight and the login epoch it began under.
  scrape: Option<(u64, Scrape)>,
}

impl Constants {
  fn new(login_epoch: &'static AtomicU64, resolve: Box<Resolve>) -> Arc<Self> {
    Arc::new(Self {
      resolve,
      login_epoch,
      state: Default::default(),
    })
  }

  fn state(&self) -> std::sync::MutexGuard<'_, ConstantsState> {
    self
      .state
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  /// The constants, when a scrape already succeeded in this process.
  fn ready(&self) -> Option<QobuzBundleCache> {
    self.state().ready.clone()
  }

  /// The constants, waiting for the scrape when they are not ready. Never
  /// call this on the pump before [`Constants::ready`] answers.
  async fn get(self: &Arc<Self>, app: &Arc<Mutex<App>>) -> Result<QobuzBundleCache> {
    let scrape = {
      let mut state = self.state();
      if let Some(ready) = &state.ready {
        return Ok(ready.clone());
      }
      let epoch = self.login_epoch.load(Ordering::SeqCst);
      match &state.scrape {
        Some((began, scrape)) if *began == epoch => scrape.clone(),
        // None in flight, or one that began under another login.
        _ => {
          let scrape = self.spawn_scrape(app, epoch);
          state.scrape = Some((epoch, scrape.clone()));
          scrape
        }
      }
    };
    {
      let mut guard = app.lock().await;
      let notice = guard.strings().status.infra.qobuz_preparing;
      guard.set_status_message(notice, PREPARING_NOTICE_SECS);
    }
    let result = scrape.await;
    result.map_err(|e| match e.downcast_ref::<LoginChanged>() {
      Some(_) => LoginChanged.into(),
      None => anyhow!("{e:#}"),
    })
  }

  /// Start the scrape on its own task. The task itself records the answer,
  /// also when it panicked, and takes the "getting ready" notice down, so a
  /// scrape nobody waits for any more leaves nothing behind.
  fn spawn_scrape(self: &Arc<Self>, app: &Arc<Mutex<App>>, epoch: u64) -> Scrape {
    let this = Arc::clone(self);
    let app = Arc::clone(app);
    let task = tokio::spawn(async move {
      let result = AssertUnwindSafe(this.scrape(&app, epoch))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(anyhow!("the resolver panicked")));
      let idle = {
        let mut state = this.state();
        if state
          .scrape
          .as_ref()
          .is_some_and(|(began, _)| *began == epoch)
        {
          state.scrape = None;
        }
        if let Ok(constants) = &result {
          state.ready = Some(constants.clone());
        }
        state.scrape.is_none()
      };
      let mut guard = app.lock().await;
      let notice = guard.strings().status.infra.qobuz_preparing;
      if idle && guard.status_message() == Some(notice) && !guard.status_message_is_error() {
        guard.clear_status_message();
      }
      drop(guard);
      result.map_err(Arc::new)
    });
    async move {
      task
        .await
        .unwrap_or_else(|e| Err(Arc::new(anyhow!("resolver task: {e}"))))
    }
    .boxed()
    .shared()
  }

  /// One scrape; a fresh answer is saved into `state.yml`. An answer that
  /// arrives after the login changed is neither saved nor given out.
  async fn scrape(&self, app: &Arc<Mutex<App>>, epoch: u64) -> Result<QobuzBundleCache> {
    let cached = cached_bundle(app).await;
    let resolved = (self.resolve)(cached.clone()).await?;
    if self.login_epoch.load(Ordering::SeqCst) != epoch {
      return Err(LoginChanged.into());
    }
    if cached.as_ref() != Some(&resolved) && resolved.bundle_version != "env" {
      app
        .lock()
        .await
        .schedule_state_save(PersistedRuntimeState::qobuz_bundle_cache(resolved.clone()));
    }
    Ok(resolved)
  }
}

/// How long the "getting ready" notice may stay: longer than the scrape's own
/// timeouts, which end it first.
const PREPARING_NOTICE_SECS: u64 = 180;

/// The process-wide constants, scraped from the web player.
fn web_constants() -> &'static Arc<Constants> {
  static WEB: OnceLock<Arc<Constants>> = OnceLock::new();
  WEB.get_or_init(|| {
    Constants::new(
      &auth::TOKEN_EPOCH,
      Box::new(|cached| {
        async move { auth::resolve_constants(&super::shared_qobuz_client(), cached.as_ref()).await }
          .boxed()
      }),
    )
  })
}

/// Run a browse, search or login request: on the pump as before once the
/// constants are ready, and on a task of its own until then, so the pump never
/// waits for the scrape.
async fn off_pump_until_ready(
  constants: &Arc<Constants>,
  work: impl Future<Output = ()> + Send + 'static,
) {
  if constants.ready().is_some() {
    work.await;
  } else {
    tokio::spawn(work);
  }
}

/// Build a [`QobuzSource`] for a browse path from the in-memory token; with
/// no token it starts the browser login and answers `None`.
async fn build_source(app: &Arc<Mutex<App>>) -> Option<(QobuzSource, u64)> {
  let (token, epoch) = auth::token_snapshot();
  let Some(token) = token else {
    app.lock().await.dispatch(IoEvent::QobuzLogin);
    return None;
  };
  match web_constants().get(app).await {
    Ok(c) => Some((QobuzSource::new(c.app_id, c.app_secret, token), epoch)),
    Err(e) => {
      report(app, QobuzStep::Constants, e, epoch).await;
      None
    }
  }
}

/// Whether a playback path may start: shows the login message and answers
/// `false` when logged out. The source itself is built by the fetch.
pub(crate) async fn logged_in_for_playback(app: &Arc<Mutex<App>>) -> bool {
  if auth::current_token().is_some() {
    return true;
  }
  let message = crate::infra::app_strings(app)
    .await
    .status
    .infra
    .qobuz_login_expired;
  set_error(app, message.to_string()).await;
  false
}

/// Name the constants as the failed step of a fetch, unless a login change
/// dropped it.
fn constants_error(err: anyhow::Error) -> anyhow::Error {
  if err.is::<LoginChanged>() {
    err
  } else {
    anyhow!("web player constants: {err:#}")
  }
}

/// A source for the playlist sync; the caller reports the logged-out case itself.
pub(crate) async fn build_sync_source(app: &Arc<Mutex<App>>) -> Result<QobuzSource> {
  let token = sync_token(auth::current_token())?;
  let constants = web_constants()
    .get(app)
    .await
    .context("Qobuz web player constants")?;
  Ok(QobuzSource::new(
    constants.app_id,
    constants.app_secret,
    token,
  ))
}

fn sync_token(token: Option<String>) -> Result<String> {
  token.ok_or_else(|| crate::core::playlist_sync::SyncReason::QobuzNotLoggedIn.into())
}

/// Report a failed call as one status message; a 401 clears only its own login.
#[derive(Clone, Copy)]
enum QobuzStep {
  Constants,
  Login,
  Library,
  Tracks,
  Search,
  Stream,
}
impl QobuzStep {
  fn label(self, strings: &crate::core::i18n::status_infra::InfraStatusStrings) -> &'static str {
    match self {
      Self::Constants => strings.qobuz_step_constants,
      Self::Login => strings.qobuz_step_login,
      Self::Library => strings.qobuz_step_library,
      Self::Tracks => strings.qobuz_step_tracks,
      Self::Search => strings.qobuz_step_search,
      Self::Stream => strings.qobuz_step_stream,
    }
  }
}
impl std::fmt::Display for QobuzStep {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.label(&crate::core::i18n::status_infra::EN))
  }
}

async fn report(app: &Arc<Mutex<App>>, step: QobuzStep, err: anyhow::Error, epoch: u64) {
  let mut guard = app.lock().await;
  report_locked(&mut guard, step, err, epoch);
}

fn report_locked(app: &mut App, step: QobuzStep, err: anyhow::Error, epoch: u64) {
  if err.downcast_ref::<LoginChanged>().is_some() {
    log::info!("[qobuz] {step}: dropped, the login changed");
  } else if err.downcast_ref::<Unauthorized>().is_some() {
    if auth::clear_token_if_current(epoch) {
      let strings = app.strings();
      app.set_error_status_message(strings.status.infra.qobuz_login_expired, 8);
    }
  } else if err.downcast_ref::<NoFullStream>().is_some() {
    let strings = app.strings();
    app.set_error_status_message(strings.status.infra.qobuz_no_full_stream, 8);
  } else {
    log::warn!("[qobuz] {step}: {err:#}");
    let strings = app.strings();
    app.set_status_message(
      strings
        .status
        .infra
        .qobuz_operation_failed(step.label(&strings.status.infra), &format!("{err:#}")),
      6,
    );
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
async fn begin_login(app: Arc<Mutex<App>>) {
  let app = &app;
  let epoch = auth::TOKEN_EPOCH.load(Ordering::SeqCst);
  if LOGIN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
    set_error(
      app,
      crate::infra::app_strings(app)
        .await
        .status
        .infra
        .qobuz_login_busy
        .to_string(),
    )
    .await;
    return;
  }
  let constants = match web_constants().get(app).await {
    Ok(c) => c,
    Err(e) => {
      LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
      report(app, QobuzStep::Constants, e, epoch).await;
      return;
    }
  };
  let attempt = match auth::LoginAttempt::bind(constants).await {
    Ok(a) => a,
    Err(e) => {
      LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
      report(app, QobuzStep::Login, e, epoch).await;
      return;
    }
  };
  let url = attempt.url();
  {
    let mut guard = app.lock().await;
    if let Err(e) = open::that_detached(&url) {
      log::warn!("[qobuz] failed to open browser automatically: {e}");
      {
        let strings = guard.strings();
        guard.set_status_message(strings.status.infra.qobuz_login_url(&url), 30)
      };
    } else {
      {
        let strings = guard.strings();
        guard.set_status_message(strings.status.infra.qobuz_browser_login, 12)
      };
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
        {
          let mut guard = app.lock().await;
          let message = guard.strings().status.infra.qobuz_logged_in;
          guard.set_status_message(message, 4);
        };
        if let Some(tx) = io_tx {
          let _ = tx.send(IoEvent::GetQobuzPlaylists);
        }
      }
      Err(e) => {
        log::warn!("[qobuz] login failed: {e:#}");
        let mut guard = app.lock().await;
        let message = guard
          .strings()
          .status
          .infra
          .qobuz_login_failed(&format!("{e:#}"));
        guard.set_status_message(message, 8);
      }
    }
    LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
  });
}

// ---------------------------------------------------------------------------
// Browse + search
// ---------------------------------------------------------------------------

/// Fetch the sidebar rows (favorites, playlists, albums) into `app.qobuz_playlists()`.
async fn load_qobuz_playlists(app: Arc<Mutex<App>>) {
  let Some((source, epoch)) = build_source(&app).await else {
    return;
  };
  let result = source.playlists().await;
  show_playlists(&mut *app.lock().await, result, epoch);
}

fn show_playlists(app: &mut App, result: Result<Vec<PlaylistInfo>>, epoch: u64) {
  match result {
    Ok(playlists) => {
      auth::with_current_login(epoch, || *app.qobuz_playlists_mut() = playlists);
    }
    Err(e) => report_locked(app, QobuzStep::Library, e, epoch),
  }
}

/// Fetch a listing's tracks into the shared track table.
async fn load_qobuz_tracks(app: Arc<Mutex<App>>, playlist_uri: String) {
  let Some((source, epoch)) = build_source(&app).await else {
    return;
  };
  let result = source.tracks(&playlist_uri).await;
  show_tracks(&mut *app.lock().await, &playlist_uri, result, epoch);
}

/// Show a listing's tracks, tagged [`TrackTableContext::QobuzPlaylist`] so
/// selecting a row plays it. Rows for a list the user has left (another list
/// opened in the table meanwhile), or from an earlier login, are dropped.
fn show_tracks(app: &mut App, playlist_uri: &str, result: Result<Vec<TrackInfo>>, epoch: u64) {
  match result {
    Ok(tracks) => {
      auth::with_current_login(epoch, || {
        if app.source_table_awaits(playlist_uri) {
          app.set_source_track_table(playlist_uri, tracks, TrackTableContext::QobuzPlaylist);
        }
      });
    }
    Err(e) => report_locked(app, QobuzStep::Tracks, e, epoch),
  }
}

/// Run a catalog search and populate the songs block of `app.search_results`.
async fn run_qobuz_search(app: Arc<Mutex<App>>, query: String) {
  let Some((source, epoch)) = build_source(&app).await else {
    return;
  };
  let result = source.search(&query).await.map(|results| results.tracks);
  show_search(&mut *app.lock().await, &query, result, epoch);
}

/// Show a search's hits. Hits for a search the user has replaced with another
/// one, or from an earlier login, are dropped.
fn show_search(app: &mut App, query: &str, result: Result<Vec<TrackInfo>>, epoch: u64) {
  match result {
    Ok(tracks) => {
      auth::with_current_login(epoch, || {
        if app.search_is_current(Source::Qobuz, query) {
          app.show_source_search_tracks(query, tracks);
        }
      });
    }
    Err(e) => report_locked(app, QobuzStep::Search, e, epoch),
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
  // Take the sink from native Spotify so no rebuild resumes it under this
  // source.
  #[cfg(feature = "streaming")]
  app.lock().await.release_native_for_decoded();
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
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .qobuz_audio_unavailable(&e.to_string()),
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

static FETCH_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_fetch_id() -> u64 {
  FETCH_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Download the track at `uri` into a tempfile, for the native queue engine's
/// off-pump fetch (playing is the queue's job), and return its delivered
/// format label. Reads the progressive stream to its end, so its per-segment
/// retries apply here too.
pub(crate) async fn download_for_queue(
  app: &Arc<Mutex<App>>,
  uri: &str,
  quality: u8,
) -> Result<(NamedTempFile, String)> {
  let track_id = track_id_from_uri(uri)?;
  // The first fetch of a process waits here for the constants.
  let token = auth::current_token().ok_or(Unauthorized)?;
  let constants = web_constants().get(app).await.map_err(constants_error)?;
  let source = QobuzSource::new(constants.app_id, constants.app_secret, token);
  let source = &source;
  let tmp = NamedTempFile::new().context("creating temp file for Qobuz stream")?;
  let (init, stream) = source.begin_stream(track_id, quality).await?;
  let failure = stream.failure();
  let mut reader = progressive::open(stream, &tmp).await?;
  let _cancel_on_abort = reader.cancellation_token().drop_guard();
  tokio::task::spawn_blocking(move || std::io::copy(&mut reader, &mut std::io::sink()))
    .await
    .context("download task")?
    .context("downloading the Qobuz stream")
    .map_err(|e| stream_cause(&failure, e))?;
  Ok((tmp, init.quality().label()))
}

/// The segment error behind a failed read, when there is one: the reader and
/// the decoder only see a stream that ended early.
fn stream_cause(failure: &progressive::Failure, err: anyhow::Error) -> anyhow::Error {
  match failure.get() {
    Some(cause) => anyhow!("{cause}"),
    None => err,
  }
}

/// Open the track's stream into a fresh tempfile and build its decoder, whose
/// first read waits for the prefetch.
async fn prepare_track(
  source: &QobuzSource,
  track_id: &str,
  quality: u8,
) -> Result<(
  NamedTempFile,
  progressive::Completion,
  StreamQuality,
  PreparedStream,
)> {
  let tmp = NamedTempFile::new().context("creating temp file for Qobuz stream")?;
  let (init, stream) = source.begin_stream(track_id, quality).await?;
  let delivered = init.quality();
  let total = init.total_bytes();
  let failure = stream.failure();
  let complete = stream.completion();
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
  .context("decoder task")?
  .map_err(|e| stream_cause(&failure, e))?;
  Ok((tmp, complete, delivered, prepared))
}

/// Open the track's stream on a detached task and build its decoder, then
/// play it when the session still waits for this fetch. The task builds the
/// source first, so the first fetch of a process waits there for the
/// constants. Called under the `App` lock that stamped `session.fetch_id`, so
/// the abort handle is in place before any skip looks for it: a skip, a new
/// queue, a queue takeover, or a teardown cancels the task, and a decoder
/// build already in progress ends on its own and drops its stream, which
/// cancels that download.
fn spawn_fetch(
  app: &Arc<Mutex<App>>,
  session: &mut QobuzPlaybackState,
  track_id: String,
  quality: u8,
) {
  let stopped = session.player.stop_for_change();
  let placeholder = placeholder_uri(&session.tracks, session.index);
  let app = Arc::clone(app);
  let fetch_id = session.fetch_id;
  let task = tokio::spawn(async move {
    let _ = stopped.await;
    let (token, epoch) = auth::token_snapshot();
    let prepared = async {
      let token = token.ok_or(Unauthorized)?;
      let constants = constants_for_fetch(&app, web_constants()).await?;
      {
        let mut guard = app.lock().await;
        if guard.queue_owns_playback()
          || guard
            .qobuz_playback
            .as_ref()
            .is_none_or(|s| s.fetch_id != fetch_id)
        {
          return Err(anyhow!("superseded Qobuz fetch"));
        }
        let notice = guard.strings().status.infra.qobuz_preparing;
        guard.set_status_message(notice, 30);
      }
      let source = QobuzSource::new(constants.app_id, constants.app_secret, token);
      let source = Arc::new(source);
      if let Some(uri) = placeholder {
        spawn_resolve(&app, &source, fetch_id, uri);
      }
      prepare_track(&source, &track_id, quality).await
    };
    match prepared.await {
      Ok((tmp, complete, delivered, prepared)) => {
        commit_fetch(&app, fetch_id, (tmp, complete), delivered, prepared).await
      }
      Err(e) => fail_fetch(&app, fetch_id, QobuzStep::Stream, e, epoch).await,
    }
  });
  session.fetch = Some(task.abort_handle());
}

/// Wait for the constants off the pump, keeping transport intent in the session.
async fn constants_for_fetch(
  app: &Arc<Mutex<App>>,
  constants: &Arc<Constants>,
) -> Result<QobuzBundleCache> {
  constants.get(app).await.map_err(constants_error)
}

/// Look the current track up in the catalogue when it is a placeholder (a URI
/// started from outside the track table, as an MCP or AI DJ `play_now` sends,
/// #716), so the playbar and the OS media controls show its title instead of
/// the URI. Runs on its own task: neither the pump nor the stream waits for
/// it. The answer lands only while the session still plays that fetch and
/// that row; a failed lookup keeps the placeholder and never stops playback.
fn spawn_resolve(app: &Arc<Mutex<App>>, source: &Arc<QobuzSource>, fetch_id: u64, uri: String) {
  let Ok(track_id) = track_id_from_uri(&uri).map(str::to_string) else {
    return;
  };
  let app = Arc::clone(app);
  let source = Arc::clone(source);
  tokio::spawn(async move {
    let track = match source.track_info(&track_id).await {
      Ok(track) => track,
      Err(e) => {
        log::info!("[qobuz] no catalogue entry for {uri}: {e:#}");
        return;
      }
    };
    let mut guard = app.lock().await;
    if let Some(s) = guard
      .qobuz_playback
      .as_mut()
      .filter(|s| s.fetch_id == fetch_id)
    {
      let index = s.index;
      apply_resolved(&mut s.tracks, index, &uri, track);
    }
  });
}

/// A failed fetch: tear the session down only if it still waits for this
/// fetch, and report the error. Under the native queue the session stays for
/// the queue's resume, which fetches again.
async fn fail_fetch(
  app: &Arc<Mutex<App>>,
  fetch_id: u64,
  step: QobuzStep,
  err: anyhow::Error,
  epoch: u64,
) {
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
  guard.release_decoded_sink_claim();
  report_locked(&mut guard, step, err, epoch);
  drop(guard);
  if let Some(s) = session {
    Arc::clone(&s.player).stop_detached();
  }
}

/// Stage and finalize only the current fetch. The outgoing track was already
/// silenced when this change began; a stale stream is dropped and cancelled.
async fn commit_fetch(
  app: &Arc<Mutex<App>>,
  fetch_id: u64,
  download: (NamedTempFile, progressive::Completion),
  quality: StreamQuality,
  prepared: PreparedStream,
) {
  commit_fetch_with_stage(
    app,
    fetch_id,
    download,
    quality,
    prepared,
    LocalPlayer::stage_prepared_for,
  )
  .await;
}

async fn commit_fetch_with_stage(
  app: &Arc<Mutex<App>>,
  fetch_id: u64,
  (tmp, complete): (NamedTempFile, progressive::Completion),
  quality: StreamQuality,
  prepared: PreparedStream,
  stage: impl FnOnce(&LocalPlayer, u64, PreparedStream) -> Result<()> + Send + 'static,
) {
  // Claim the session under the lock, stage off it: the clear inside
  // `stage_prepared` waits on the audio thread, and the runner takes this
  // lock on every frame.
  let (player, resume, volume, generation) = {
    let mut guard = app.lock().await;
    if guard.queue_owns_playback() {
      return;
    }
    let volume = guard.runtime_state.volume_percent;
    let Some(s) = guard
      .qobuz_playback
      .as_mut()
      .filter(|s| s.fetch_id == fetch_id)
    else {
      return;
    };
    s.tempfile = Some(tmp);
    s.complete = complete;
    s.quality = Some(quality);
    s.fetch = None;
    // Keep seek and pause intent through staging, after the tempfile is published.
    let resume = *s.resume_at.get_or_insert_with(ResumePoint::default);
    (
      Arc::clone(&s.player),
      resume,
      volume,
      s.player.begin_stage(),
    )
  };
  let stage_player = Arc::clone(&player);
  let staged = tokio::task::spawn_blocking(move || {
    stage(&stage_player, generation, prepared)?;
    stage_player.set_volume(volume);
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
  let mut position_ms = {
    let mut guard = app.lock().await;
    if guard.queue_owns_playback() {
      return;
    }
    let Some(s) = guard
      .qobuz_playback
      .as_ref()
      .filter(|s| s.fetch_id == fetch_id)
    else {
      return;
    };
    if !staged {
      // The device went under the stage, and the dropped stream left the file
      // partial: replay it with the latest seek and pause. Keep the latch on.
      guard.dispatch(IoEvent::ReplayCurrentTrack);
      return;
    }
    s.resume_at.unwrap_or(resume).position_ms
  };
  let mut needs_seek = position_ms > 0;
  loop {
    // Restore before playing, off the App lock. A newer track invalidates this
    // stage's seek even if its blocking worker has not started yet.
    if needs_seek {
      let seek_player = Arc::clone(&player);
      let _ = tokio::task::spawn_blocking(move || {
        seek_player.seek_for_stage(generation, Duration::from_millis(position_ms))
      })
      .await;
    }
    let mut guard = app.lock().await;
    let Some(s) = current_playback_stage(&mut guard, fetch_id, generation) else {
      return;
    };
    let latest = s.resume_at.unwrap_or(resume);
    if latest.position_ms != position_ms {
      // A seek received during the wait supersedes that target, including zero.
      position_ms = latest.position_ms;
      needs_seek = true;
      continue;
    }
    s.resume_at = None;
    s.advancing = false;
    s.set_paused(latest.paused);
    if let Some(display) = s.current().map(|t| t.name.clone()) {
      guard.set_status_message(format!("\u{266a} {display}"), 4);
    }
    return;
  }
}

/// Called under the final App lock: recovery owns the empty-output latch if it
/// replaced this stage's output, or a seek abandoned the output before recovery.
fn current_playback_stage(
  app: &mut App,
  fetch_id: u64,
  generation: u64,
) -> Option<&mut QobuzPlaybackState> {
  if app.queue_owns_playback() {
    return None;
  }
  app.qobuz_playback.as_mut().filter(|s| {
    s.fetch_id == fetch_id && s.player.stage_is_current(generation) && !s.player.device_lost()
  })
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
        .qobuz_no_tracks
        .to_string(),
    )
    .await;
    discard_failed_start(app).await;
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
          .qobuz_uri_invalid
          .to_string(),
      )
      .await;
      discard_failed_start(app).await;
      return;
    }
  };
  if !logged_in_for_playback(app).await {
    discard_failed_start(app).await;
    return;
  }

  // Only one backend owns the device at a time.
  app.lock().await.claim_decoded_sink(Source::Qobuz);
  release_other_backends(app).await;
  let Some(player) = acquire_player(app).await else {
    app.lock().await.release_decoded_sink_claim();
    return;
  };

  // Publish the session now, marked advancing, so the playbar and the skip
  // keys see it during the download; `commit_fetch` finalizes it.
  let mut guard = app.lock().await;
  let quality = guard.user_config.behavior.qobuz_quality;
  let mut state = QobuzPlaybackState {
    player,
    tracks,
    index,
    advancing: true,
    tempfile: None,
    complete: Default::default(),
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
  guard.song_progress_ms = state.position_ms();
  guard.qobuz_playback = Some(state);
  let notice = guard.strings().status.infra.qobuz_preparing;
  guard.set_status_message(notice, 30);
}

/// The queue already stopped this context and cancelled its fetch. A rejected
/// start must not leave that orphan holding the advance latch and sink claim.
async fn discard_failed_start(app: &Arc<Mutex<App>>) {
  let mut guard = app.lock().await;
  if !guard.queue_owns_playback()
    && guard.queue_suspended.is_none()
    && guard
      .qobuz_playback
      .as_ref()
      .is_some_and(|s| s.advancing && s.fetch.is_none())
  {
    guard.qobuz_playback = None;
    guard.release_decoded_sink_claim();
    guard.song_progress_ms = 0;
  }
}

/// Move the queue index in `direction` and play the new track. Returns `true`
/// if Qobuz owns the session (so the event is consumed).
async fn skip(app: &Arc<Mutex<App>>, direction: Direction) -> bool {
  let target = {
    let guard = app.lock().await;
    let mode = guard.decoded_repeat;
    let Some(s) = guard.qobuz_playback.as_ref() else {
      return false;
    };
    let forward = matches!(direction, Direction::Next);
    advance_index(s.index, s.tracks.len(), mode, forward)
  };
  // A boundary skip is a no-op, including while a commit owns the empty sink.
  if let Some(idx) = target {
    play_index(app, idx, None).await;
  }
  true
}

/// Replay the current track (repeat-one) from its retained tempfile, with no
/// second download when the file is whole. Returns `true` if Qobuz owns the session.
pub(crate) async fn replay_current(app: &Arc<Mutex<App>>) -> bool {
  replay_current_with_stage(app, LocalPlayer::stage_file_for).await
}

async fn replay_current_with_stage(
  app: &Arc<Mutex<App>>,
  stage: impl FnOnce(&LocalPlayer, u64, &std::path::Path) -> Result<()> + Send + 'static,
) -> bool {
  let (player, path, fetch_id, generation) = {
    let mut guard = app.lock().await;
    if !guard.qobuz_owns_playback() {
      return false;
    }
    let s = guard.qobuz_playback.as_mut().unwrap();
    // An in-flight fetch commits the parked seek and intent itself.
    if s.tempfile.is_none() {
      return true;
    }
    let resume = *s.resume_at.get_or_insert(ResumePoint {
      position_ms: 0,
      paused: s.player.is_paused(),
    });
    s.advancing = true;
    s.player.pause();
    match s.finished_file().map(std::path::Path::to_path_buf) {
      Some(path) => {
        s.fetch_id = next_fetch_id();
        (
          Arc::clone(&s.player),
          path,
          s.fetch_id,
          s.player.begin_stage(),
        )
      }
      None => {
        let index = s.index;
        drop(guard);
        play_index(app, index, Some(resume)).await;
        return true;
      }
    }
  };
  let stage_player = Arc::clone(&player);
  let staged = matches!(
    tokio::task::spawn_blocking(move || stage(&stage_player, generation, &path)).await,
    Ok(Ok(()))
  );
  let position_ms = {
    let mut guard = app.lock().await;
    if guard.queue_owns_playback() {
      return true;
    }
    let Some(s) = guard
      .qobuz_playback
      .as_ref()
      .filter(|s| s.fetch_id == fetch_id)
    else {
      return true;
    };
    if !staged {
      guard.qobuz_playback = None;
      guard.release_decoded_sink_claim();
      let message = guard.strings().status.infra.qobuz_replay_failed;
      guard.set_error_status_message(message, 8);
      return true;
    }
    s.resume_at.unwrap_or_default().position_ms
  };
  // Keep the change pending until this seek completes. The serial pump must
  // not start the next track while an old replay seek is still outstanding.
  if position_ms > 0 {
    let seek_player = Arc::clone(&player);
    let _ = tokio::task::spawn_blocking(move || {
      seek_player.seek_for_stage(generation, Duration::from_millis(position_ms))
    })
    .await;
  }
  let mut guard = app.lock().await;
  if let Some(s) = current_playback_stage(&mut guard, fetch_id, generation) {
    let resume = s.resume_at.take().unwrap_or_default();
    s.advancing = false;
    s.set_paused(resume.paused);
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
    return;
  };
  let track_id = match track.uri.as_deref().map(track_id_from_uri) {
    Some(Ok(id)) => id.to_string(),
    _ => {
      drop(guard);
      teardown_qobuz(app).await;
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .qobuz_uri_invalid
          .to_string(),
      )
      .await;
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
  guard.song_progress_ms = guard.qobuz_playback.as_ref().unwrap().position_ms();
  let notice = guard.strings().status.infra.qobuz_preparing;
  guard.set_status_message(notice, 30);
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
  use crate::core::test_helpers::queued_track;
  use std::sync::atomic::AtomicUsize;
  use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

  fn bundle() -> QobuzBundleCache {
    QobuzBundleCache {
      bundle_version: "7.1.0".to_string(),
      app_id: "app-id".to_string(),
      app_secret: "secret".to_string(),
      oauth_key: "key".to_string(),
    }
  }

  /// Constants whose every scrape waits for one answer sent by the test.
  struct Scrapes {
    constants: Arc<Constants>,
    answer: UnboundedSender<Result<QobuzBundleCache>>,
    started: Arc<AtomicUsize>,
    login_epoch: &'static AtomicU64,
  }

  fn scrapes() -> Scrapes {
    let (answer, answers) = unbounded_channel::<Result<QobuzBundleCache>>();
    let answers = Arc::new(Mutex::new(answers));
    let started = Arc::new(AtomicUsize::new(0));
    let login_epoch: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
    let count = Arc::clone(&started);
    let constants = Constants::new(
      login_epoch,
      Box::new(move |_cached| {
        count.fetch_add(1, Ordering::SeqCst);
        let answers = Arc::clone(&answers);
        async move { answers.lock().await.recv().await.expect("an answer") }.boxed()
      }),
    );
    Scrapes {
      constants,
      answer,
      started,
      login_epoch,
    }
  }

  fn test_app() -> Arc<Mutex<App>> {
    Arc::new(Mutex::new(App::default()))
  }

  /// An app with a `state.yml` of its own, and what a save wrote there.
  fn app_with_state() -> (Arc<Mutex<App>>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::default();
    app.state_path = Some(dir.path().join("state.yml"));
    (Arc::new(Mutex::new(app)), dir)
  }

  fn saved_bundle(app: &mut App) -> Option<QobuzBundleCache> {
    app.flush_state_save(true);
    let path = app.state_path.clone()?;
    crate::core::state::load(&path).ok()?.qobuz_bundle_cache
  }

  /// Let the spawned tasks run until they wait again.
  async fn settle() {
    for _ in 0..20 {
      tokio::task::yield_now().await;
    }
  }

  async fn status(app: &Arc<Mutex<App>>) -> Option<String> {
    app.lock().await.status_message().map(str::to_string)
  }

  const NOTICE: &str = crate::core::i18n::EN.status.infra.qobuz_preparing;

  #[test]
  fn successful_sidebar_results_from_an_old_login_keep_the_current_library() {
    let epoch = auth::TOKEN_EPOCH.load(Ordering::SeqCst);
    let mut app = App::default();
    let playlists = |name: &str| {
      Ok(vec![PlaylistInfo {
        generated_name: None,
        name: name.to_string(),
        uri: "qobuz:favorites:tracks".to_string(),
        owner: String::new(),
        track_count: 1,
        id: None,
        owner_id: None,
        collaborative: false,
        public: None,
        image_url: None,
      }])
    };
    show_playlists(&mut app, playlists("Current favorites"), epoch);
    assert_eq!(app.qobuz_playlists()[0].name, "Current favorites");

    show_playlists(&mut app, playlists("Old favorites"), epoch.wrapping_sub(1));
    assert_eq!(app.qobuz_playlists()[0].name, "Current favorites");
  }

  #[test]
  fn successful_track_lists_from_an_old_login_do_not_fill_the_current_table() {
    let epoch = auth::TOKEN_EPOCH.load(Ordering::SeqCst);
    for uri in [
      "qobuz:favorites:tracks",
      "qobuz:album:1",
      "qobuz:playlist:1",
    ] {
      let mut app = App::default();
      app.open_source_playlist_tracks(uri.to_string());
      show_tracks(
        &mut app,
        uri,
        Ok(vec![queued_track("qobuz:track:1", "Old track")]),
        epoch.wrapping_sub(1),
      );
      assert!(app.track_table.tracks.is_empty(), "stale rows for {uri}");
      assert!(app.source_table_awaits(uri));

      show_tracks(
        &mut app,
        uri,
        Ok(vec![queued_track("qobuz:track:2", "Current track")]),
        epoch,
      );
      assert_eq!(app.track_table.tracks[0].name, "Current track");
    }
  }

  #[test]
  fn successful_search_results_from_an_old_login_keep_the_current_hits() {
    use crate::core::action::Action;
    let epoch = auth::TOKEN_EPOCH.load(Ordering::SeqCst);
    let mut app = App::default();
    app.active_source = Source::Qobuz;
    app.apply(Action::SearchActiveSource("same query".to_string()));
    show_search(
      &mut app,
      "same query",
      Ok(vec![queued_track("qobuz:track:2", "Current hit")]),
      epoch,
    );
    show_search(
      &mut app,
      "same query",
      Ok(vec![queued_track("qobuz:track:1", "Old hit")]),
      epoch.wrapping_sub(1),
    );
    assert_eq!(
      app.search_results().tracks.as_ref().unwrap().items[0].name,
      "Current hit"
    );
  }

  /// A published session that waits for its first fetch, on a fresh (paused)
  /// player with no audio device.
  fn waiting_session(fetch_id: u64) -> QobuzPlaybackState {
    let player = LocalPlayer::without_device();
    player.pause();
    QobuzPlaybackState {
      player: Arc::new(player),
      tracks: vec![queued_track("qobuz:track:1", "One")],
      index: 0,
      advancing: true,
      tempfile: None,
      complete: Default::default(),
      quality: None,
      shuffle_backup: None,
      fetch_id,
      resume_at: None,
      fetch: None,
    }
  }

  fn prepared_test_track() -> (NamedTempFile, PreparedStream) {
    use std::io::Write;
    let mut file = NamedTempFile::new().unwrap();
    let data_len = 8_000u32 * 2 * 30;
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&1u16.to_le_bytes()).unwrap();
    file.write_all(&8_000u32.to_le_bytes()).unwrap();
    file.write_all(&16_000u32.to_le_bytes()).unwrap();
    file.write_all(&2u16.to_le_bytes()).unwrap();
    file.write_all(&16u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();
    file.write_all(&vec![0; data_len as usize]).unwrap();
    file.flush().unwrap();
    let prepared = LocalPlayer::prepare_stream(
      file.reopen().unwrap(),
      Some("audio/wav"),
      Some(44 + u64::from(data_len)),
    )
    .unwrap();
    (file, prepared)
  }

  fn test_quality() -> StreamQuality {
    StreamQuality {
      sample_rate: 8_000,
      bits_per_sample: 16,
      flac: false,
    }
  }

  /// Stage a local WAV through the real commit path, without an audio device.
  async fn commit_test_track(app: &Arc<Mutex<App>>) {
    let (file, prepared) = prepared_test_track();
    commit_fetch(app, 7, (file, Default::default()), test_quality(), prepared).await;
    let guard = app.lock().await;
    let session = guard.qobuz_playback.as_ref().unwrap();
    assert!(session.tempfile.is_some());
    assert!(!session.advancing);
  }

  async fn seek_during_fetch(resume: Option<ResumePoint>) {
    let app = test_app();
    let mut session = waiting_session(7);
    let player = Arc::new(LocalPlayer::without_device_with_consumer());
    let (_outgoing, prepared) = prepared_test_track();
    player.stage_prepared(prepared).unwrap();
    player.seek(Duration::from_millis(100)).unwrap();
    session.player = Arc::clone(&player);
    session.resume_at = resume;
    app.lock().await.qobuz_playback = Some(session);

    assert!(route_qobuz_event(&app, &IoEvent::Seek(400)).await);
    {
      let guard = app.lock().await;
      let session = guard.qobuz_playback.as_ref().unwrap();
      assert_eq!(
        session.resume_at,
        Some(ResumePoint {
          position_ms: 400,
          paused: resume.is_some_and(|r| r.paused)
        })
      );
      assert_eq!(
        player.position(),
        Duration::from_millis(100),
        "the outgoing track stays put"
      );
      assert!(session.advancing);
      assert_eq!(session.fetch_id, 7);
    }
    // A fresh sink starts paused without a user pause intent. A seek must
    // not turn that initial sink state into a pause of the incoming track.
    // Pause explicitly before committing so its final seek can be read exactly.
    assert!(route_qobuz_event(&app, &IoEvent::PausePlayback).await);
    commit_test_track(&app).await;
    tokio::time::timeout(Duration::from_secs(2), async {
      while player.position() != Duration::from_millis(400) {
        tokio::time::sleep(Duration::from_millis(1)).await;
      }
    })
    .await
    .expect("the committed track seeks to the latest target");
    assert!(player.is_paused());
    assert_eq!(
      app.lock().await.qobuz_playback.as_ref().unwrap().resume_at,
      None
    );
  }

  #[tokio::test]
  async fn a_seek_during_fetch_creates_a_resume_point_without_moving_the_outgoing_track() {
    seek_during_fetch(None).await;
  }

  #[tokio::test]
  async fn a_seek_during_fetch_replaces_the_saved_position_without_moving_the_outgoing_track() {
    seek_during_fetch(Some(ResumePoint {
      position_ms: 800,
      paused: true,
    }))
    .await;
  }

  #[tokio::test]
  async fn a_seek_on_a_fresh_player_keeps_the_default_play_intent() {
    let app = test_app();
    app.lock().await.qobuz_playback = Some(waiting_session(7));
    assert!(route_qobuz_event(&app, &IoEvent::Seek(0)).await);
    assert_eq!(
      app.lock().await.qobuz_playback.as_ref().unwrap().resume_at,
      Some(ResumePoint::default())
    );
    commit_test_track(&app).await;
    assert!(!app
      .lock()
      .await
      .qobuz_playback
      .as_ref()
      .unwrap()
      .player
      .is_paused());
  }

  #[tokio::test]
  async fn a_seek_during_staging_overrides_the_position_snapshot() {
    for target in [0, 600] {
      let app = test_app();
      let mut session = waiting_session(7);
      let player = Arc::new(LocalPlayer::without_device_with_consumer());
      session.player = Arc::clone(&player);
      session.resume_at = Some(ResumePoint {
        position_ms: 800,
        paused: true,
      });
      app.lock().await.qobuz_playback = Some(session);
      let (file, prepared) = prepared_test_track();
      let (staging, started) = tokio::sync::oneshot::channel();
      let (release, released) = std::sync::mpsc::channel();
      let commit_app = Arc::clone(&app);
      let commit = tokio::spawn(async move {
        commit_fetch_with_stage(
          &commit_app,
          7,
          (file, Default::default()),
          test_quality(),
          prepared,
          move |player, generation, prepared| {
            staging.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            player.stage_prepared_for(generation, prepared)
          },
        )
        .await;
      });
      tokio::time::timeout(Duration::from_secs(2), started)
        .await
        .unwrap()
        .unwrap();
      assert!(route_qobuz_event(&app, &IoEvent::Seek(target)).await);
      {
        let guard = app.lock().await;
        let session = guard.qobuz_playback.as_ref().unwrap();
        assert!(session.advancing && session.tempfile.is_some());
        assert_eq!(
          session.resume_at,
          Some(ResumePoint {
            position_ms: u64::from(target),
            paused: true
          })
        );
        assert_eq!(player.position(), Duration::ZERO);
      }
      release.send(()).unwrap();
      commit.await.unwrap();
      tokio::time::timeout(Duration::from_secs(2), async {
        while player.position() != Duration::from_millis(u64::from(target)) {
          tokio::time::sleep(Duration::from_millis(1)).await;
        }
      })
      .await
      .expect("staging applies the latest seek, including zero");
      assert!(player.is_paused());
      let guard = app.lock().await;
      let session = guard.qobuz_playback.as_ref().unwrap();
      assert!(!session.advancing);
      assert_eq!(session.resume_at, None);
    }
  }

  /// A real outgoing stream at six seconds, with commands wired to the pump.
  fn playing_app() -> (
    Arc<Mutex<App>>,
    std::sync::mpsc::Receiver<IoEvent>,
    NamedTempFile,
  ) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut app = App::new(tx, crate::core::user_config::UserConfig::new(), None);
    let mut session = waiting_session(7);
    let player = Arc::new(LocalPlayer::without_device_with_consumer());
    let (file, prepared) = prepared_test_track();
    player.stage_prepared(prepared).unwrap();
    player.seek(Duration::from_secs(6)).unwrap();
    session.player = player;
    session.tracks.push(queued_track("qobuz:track:2", "Two"));
    session.advancing = false;
    app.qobuz_playback = Some(session);
    (Arc::new(Mutex::new(app)), rx, file)
  }

  fn tick(app: &mut App) {
    use crate::core::driver::{Driver, TickEnv};
    Driver::new(None, None).tick(
      app,
      Duration::ZERO,
      TickEnv {
        now: std::time::Instant::now(),
        #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
        viz: None,
        #[cfg(feature = "art-decode")]
        cover_art_full_image_support: false,
      },
    );
  }

  #[tokio::test]
  async fn next_during_playback_silences_the_outgoing_track_and_reports_zero_until_commit() {
    let (app, _rx, outgoing) = playing_app();
    let player = Arc::clone(&app.lock().await.qobuz_playback.as_ref().unwrap().player);
    player.resume();
    assert!(route_qobuz_event(&app, &IoEvent::NextTrack).await);
    let fetch_id = {
      let mut guard = app.lock().await;
      let session = guard.qobuz_playback.as_mut().unwrap();
      // Hold the download window open without network or a real account.
      session.fetch.take().unwrap().abort();
      assert!(
        player.is_paused(),
        "Next silences A before returning to the pump"
      );
      let fetch_id = session.fetch_id;
      assert_eq!(session.index, 1);
      assert_eq!(session.position_ms(), 0);
      assert_eq!(guard.song_progress_ms, 0);
      assert_eq!(
        guard.status_message(),
        Some(guard.strings().status.infra.qobuz_preparing)
      );
      for _ in 0..2 {
        tick(&mut guard);
        assert_eq!(guard.song_progress_ms, 0);
        let snapshot = crate::infra::media_metadata::current_playback_snapshot(&guard).unwrap();
        assert_eq!(snapshot.metadata.title, "Two");
        assert_eq!(snapshot.progress_ms, 0);
        assert!(!snapshot.is_playing);
      }
      fetch_id
    };
    tokio::time::timeout(Duration::from_secs(2), async {
      while !player.is_finished() {
        tokio::task::yield_now().await;
      }
    })
    .await
    .expect("the old source is discarded off the pump");
    drop(outgoing);
    let (file, prepared) = prepared_test_track();
    commit_fetch(
      &app,
      fetch_id,
      (file, Default::default()),
      test_quality(),
      prepared,
    )
    .await;
    assert!(!app.lock().await.qobuz_playback.as_ref().unwrap().advancing);
    assert!(!player.is_paused(), "B starts with its default play intent");
  }

  #[tokio::test]
  async fn relative_seeks_during_a_pending_change_use_the_parked_position_or_zero() {
    for parked in [None, Some(20_000)] {
      let (app, rx, _outgoing) = playing_app();
      {
        let mut guard = app.lock().await;
        let s = guard.qobuz_playback.as_mut().unwrap();
        s.advancing = true;
        s.tempfile = None;
        s.resume_at = parked.map(|position_ms| ResumePoint {
          position_ms,
          paused: false,
        });
        guard.song_progress_ms = 6_000;
        assert_eq!(
          guard.decoded_transport_state(),
          Some((false, u128::from(parked.unwrap_or(0))))
        );
        assert!(matches!(guard.current_persisted_playback(),
          Some(crate::core::persisted_playback::PersistedPlayback::Qobuz {
            position_ms, paused: false, ..
          }) if position_ms == parked.unwrap_or(0)));
        guard.seek_forwards();
      }
      let expected = parked.unwrap_or(0) as u32 + 5_000;
      let event = rx.try_recv().unwrap();
      assert!(matches!(event, IoEvent::Seek(ms) if ms == expected));
      assert!(route_qobuz_event(&app, &event).await);
      {
        let mut guard = app.lock().await;
        tick(&mut guard);
        assert_eq!(guard.song_progress_ms, u128::from(expected));
        guard.seek_backwards();
      }
      let event = rx
        .try_iter()
        .find(|e| matches!(e, IoEvent::Seek(_)))
        .unwrap();
      assert!(matches!(event, IoEvent::Seek(ms) if ms == expected - 5_000));
      assert!(route_qobuz_event(&app, &event).await);
      assert_eq!(
        app
          .lock()
          .await
          .qobuz_playback
          .as_ref()
          .unwrap()
          .resume_at
          .unwrap()
          .position_ms,
        parked.unwrap_or(0)
      );
    }
  }

  #[tokio::test]
  async fn previous_during_a_pending_change_steps_back_instead_of_restarting_the_outgoing_track() {
    let (app, rx, _outgoing) = playing_app();
    {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_mut().unwrap();
      s.index = 1;
      s.advancing = true;
      s.tempfile = None;
      guard.song_progress_ms = 6_000; // Before even the first tick can refresh it.
      guard.previous_track();
    }
    let event = rx.try_recv().unwrap();
    assert!(matches!(event, IoEvent::PreviousTrack));
    assert!(route_qobuz_event(&app, &event).await);
    let mut guard = app.lock().await;
    let s = guard.qobuz_playback.as_mut().unwrap();
    s.fetch.take().unwrap().abort();
    assert_eq!(s.index, 0);
    assert!(s.advancing);
    assert_eq!(guard.song_progress_ms, 0);
  }

  #[test]
  fn device_recovery_during_a_pending_change_keeps_the_parked_seek_and_play_intent() {
    for tempfile in [false, true] {
      for saved in [
        None,
        Some(ResumePoint {
          position_ms: 42_000,
          paused: false,
        }),
        Some(ResumePoint {
          position_ms: 17_000,
          paused: true,
        }),
      ] {
        let mut s = waiting_session(7);
        if tempfile {
          s.tempfile = Some(NamedTempFile::new().unwrap());
        }
        s.resume_at = saved;
        assert!(s.player.is_paused(), "a fresh/reopened sink is paused");
        assert_eq!(s.recovery_point(), saved.unwrap_or_default());
        assert_eq!(s.resume_at, saved);
      }
    }
  }

  #[tokio::test]
  async fn device_recovery_during_a_replay_seek_keeps_the_queued_replay_pending() {
    let (app, rx, file) = playing_app();
    let player = {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_mut().unwrap();
      s.tracks.truncate(1);
      s.tempfile = Some(file);
      s.complete.store(true, Ordering::Release);
      s.resume_at = Some(ResumePoint {
        position_ms: 400,
        paused: false,
      });
      Arc::clone(&s.player)
    };
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    player.on_next_seek(move || {
      started.send(()).unwrap();
      released.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let replay_app = Arc::clone(&app);
    let replay = tokio::spawn(async move { replay_current(&replay_app).await });
    tokio::time::timeout(Duration::from_secs(2), ready)
      .await
      .unwrap()
      .unwrap();
    let (fetch_id, resume) = {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_mut().unwrap();
      let resume = s.recovery_point();
      player.reopen_without_device();
      s.advancing = true;
      s.resume_at = Some(resume);
      let fetch_id = s.fetch_id;
      // Recovery queues behind the replay still awaiting its seek on the pump.
      guard.dispatch(IoEvent::ReplayCurrentTrack);
      (fetch_id, resume)
    };
    release.send(()).unwrap();
    assert!(replay.await.unwrap());
    {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_ref().unwrap();
      assert_eq!(s.fetch_id, fetch_id, "recovery has not run its replay yet");
      assert!(player.is_finished(), "recovery opened an empty output");
      assert!(s.advancing, "the old replay must keep recovery's latch set");
      assert_eq!(s.resume_at, Some(resume));
      tick(&mut guard);
      assert!(guard.qobuz_playback.is_some(), "the tick keeps the session");
    }
    let event = rx.try_recv().unwrap();
    assert!(matches!(event, IoEvent::ReplayCurrentTrack));
    assert!(route_qobuz_event(&app, &event).await);
    let guard = app.lock().await;
    let s = guard.qobuz_playback.as_ref().unwrap();
    assert_ne!(s.fetch_id, fetch_id, "the queued replay ran");
    assert!(!s.advancing);
    assert_eq!(s.resume_at, None);
    assert!(
      !player.is_finished(),
      "the track is staged on the new output"
    );
    assert!(!player.is_paused(), "the queued replay plays the track");
    assert_eq!(s.current().unwrap().uri.as_deref(), Some("qobuz:track:1"));
  }

  #[tokio::test]
  async fn pause_during_a_replay_restage_holds_when_the_stage_finishes() {
    use crate::core::action::Action;
    let (app, _rx, file) = playing_app();
    {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_mut().unwrap();
      s.tempfile = Some(file);
      s.resume_at = Some(ResumePoint {
        position_ms: 400,
        paused: false,
      });
      s.complete.store(true, Ordering::Release);
      s.player.resume();
    }
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let replay_app = Arc::clone(&app);
    let replay = tokio::spawn(async move {
      replay_current_with_stage(&replay_app, move |player, generation, path| {
        started.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        player.stage_file_for(generation, path)
      })
      .await
    });
    tokio::time::timeout(Duration::from_secs(2), ready)
      .await
      .unwrap()
      .unwrap();
    app.lock().await.apply(Action::Pause);
    release.send(()).unwrap();
    assert!(replay.await.unwrap());
    let guard = app.lock().await;
    let s = guard.qobuz_playback.as_ref().unwrap();
    assert!(s.player.is_paused());
    assert_eq!(
      s.player.position(),
      Duration::from_millis(400),
      "the restore seek finishes before the pump can start another track"
    );
    assert!(!s.advancing);
    assert_eq!(s.resume_at, None, "the replay consumes the latest intent");
  }

  #[tokio::test]
  async fn a_rejected_qobuz_start_releases_a_context_orphaned_by_the_queue() {
    use crate::infra::queue::dispatch::route_queue_event;
    let app = test_app();
    {
      let mut guard = app.lock().await;
      guard.claim_decoded_sink(Source::Qobuz);
      guard.qobuz_playback = Some(waiting_session(7));
      guard.suspend_active_decoded_context_mid_track();
      assert!(guard.queue_suspended.is_some());
    }
    let event = IoEvent::StartPlayback(Some("qobuz:playlist:invalid".into()), None, None);
    assert!(!route_queue_event(&app, &event).await);
    assert!(route_qobuz_event(&app, &event).await);
    let guard = app.lock().await;
    assert!(guard.queue_suspended.is_none());
    assert!(guard.qobuz_playback.is_none());
    assert!(!guard.decoded_sink_claimed());
  }

  #[tokio::test]
  async fn a_rejected_start_releases_an_orphaned_commit_stage() {
    use crate::infra::queue::dispatch::route_queue_event;
    let app = test_app();
    {
      let mut guard = app.lock().await;
      guard.claim_decoded_sink(Source::Qobuz);
      guard.qobuz_playback = Some(waiting_session(7));
      guard.song_progress_ms = 800;
    }
    let (file, prepared) = prepared_test_track();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let commit_app = Arc::clone(&app);
    let commit = tokio::spawn(async move {
      commit_fetch_with_stage(
        &commit_app,
        7,
        (file, Default::default()),
        test_quality(),
        prepared,
        move |player, generation, prepared| {
          started.send(()).unwrap();
          released.recv_timeout(Duration::from_secs(5)).unwrap();
          player.stage_prepared_for(generation, prepared)
        },
      )
      .await;
    });
    tokio::time::timeout(Duration::from_secs(2), ready)
      .await
      .unwrap()
      .unwrap();
    {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_ref().unwrap();
      assert!(s.advancing && s.tempfile.is_some() && s.fetch.is_none());
      guard.suspend_active_decoded_context_mid_track();
    }
    let event = IoEvent::StartPlayback(Some("qobuz:playlist:invalid".into()), None, None);
    assert!(!route_queue_event(&app, &event).await);
    assert!(route_qobuz_event(&app, &event).await);
    release.send(()).unwrap();
    commit.await.unwrap();
    let guard = app.lock().await;
    assert!(guard.queue_suspended.is_none());
    assert!(guard.qobuz_playback.is_none());
    assert!(!guard.decoded_sink_claimed());
    assert_eq!(guard.song_progress_ms, 0);
  }

  #[tokio::test]
  async fn a_fetch_commit_seeks_before_resuming_and_keeps_a_late_pause() {
    for target in [0, 400, 800] {
      let app = test_app();
      let mut session = waiting_session(7);
      let player = Arc::new(LocalPlayer::without_device_with_consumer());
      session.player = Arc::clone(&player);
      session.resume_at = Some(ResumePoint {
        position_ms: 800,
        paused: false,
      });
      app.lock().await.qobuz_playback = Some(session);
      let (started, ready) = tokio::sync::oneshot::channel();
      let (release, released) = std::sync::mpsc::channel();
      let seek_player = Arc::clone(&player);
      player.on_next_seek(move || {
        started.send(seek_player.is_paused()).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
      });
      let commit_app = Arc::clone(&app);
      let commit = tokio::spawn(async move { commit_test_track(&commit_app).await });
      let paused_before_seek = tokio::time::timeout(Duration::from_secs(2), ready)
        .await
        .unwrap()
        .unwrap();
      let pending_during_seek = app.lock().await.qobuz_playback.as_ref().unwrap().advancing;
      assert!(route_qobuz_event(&app, &IoEvent::PausePlayback).await);
      assert!(route_qobuz_event(&app, &IoEvent::Seek(target)).await);
      release.send(()).unwrap();
      commit.await.unwrap();
      assert!(
        paused_before_seek,
        "the restored track must not play at zero"
      );
      assert!(pending_during_seek, "a restore seek is part of the change");
      assert!(
        player.is_paused(),
        "Pause during the seek stays authoritative"
      );
      assert_eq!(player.position(), Duration::from_millis(u64::from(target)));
      assert_eq!(
        app.lock().await.qobuz_playback.as_ref().unwrap().resume_at,
        None
      );
    }
  }

  #[tokio::test]
  async fn a_superseded_fetch_seek_does_not_move_the_newer_track() {
    let app = test_app();
    let mut session = waiting_session(7);
    let player = Arc::new(LocalPlayer::without_device_with_consumer());
    session.player = Arc::clone(&player);
    session.resume_at = Some(ResumePoint {
      position_ms: 800,
      paused: true,
    });
    app.lock().await.qobuz_playback = Some(session);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let seek_finished = player.on_next_seek(move || {
      started.send(()).unwrap();
      released.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let (file, prepared) = prepared_test_track();
    let commit_app = Arc::clone(&app);
    let commit = tokio::spawn(async move {
      commit_fetch(
        &commit_app,
        7,
        (file, Default::default()),
        test_quality(),
        prepared,
      )
      .await;
    });
    tokio::time::timeout(Duration::from_secs(2), ready)
      .await
      .unwrap()
      .unwrap();
    {
      let mut guard = app.lock().await;
      let s = guard.qobuz_playback.as_mut().unwrap();
      s.fetch_id = 8;
      s.advancing = true;
      s.resume_at = Some(ResumePoint {
        position_ms: 0,
        paused: true,
      });
    }
    let (file, prepared) = prepared_test_track();
    commit_fetch(
      &app,
      8,
      (file, Default::default()),
      test_quality(),
      prepared,
    )
    .await;
    release.send(()).unwrap();
    commit.await.unwrap();
    tokio::task::spawn_blocking(move || seek_finished.recv_timeout(Duration::from_secs(2)))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(player.position(), Duration::ZERO);
    let guard = app.lock().await;
    let s = guard.qobuz_playback.as_ref().unwrap();
    assert_eq!(s.fetch_id, 8);
    assert!(!s.advancing && s.player.is_paused());
    assert_eq!(s.resume_at, None);
  }

  #[tokio::test]
  async fn boundary_skips_and_invalid_indices_during_staging_keep_the_session() {
    for boundary in 0..3 {
      let (tx, _rx) = std::sync::mpsc::channel();
      let mut state = App::new(tx, crate::core::user_config::UserConfig::new(), None);
      state.qobuz_playback = Some(waiting_session(7));
      let app = Arc::new(Mutex::new(state));
      let (file, prepared) = prepared_test_track();
      let (started, ready) = tokio::sync::oneshot::channel();
      let (release, released) = std::sync::mpsc::channel();
      let commit_app = Arc::clone(&app);
      let commit = tokio::spawn(async move {
        commit_fetch_with_stage(
          &commit_app,
          7,
          (file, Default::default()),
          test_quality(),
          prepared,
          move |player, generation, prepared| {
            started.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            player.stage_prepared_for(generation, prepared)
          },
        )
        .await;
      });
      tokio::time::timeout(Duration::from_secs(2), ready)
        .await
        .unwrap()
        .unwrap();
      match boundary {
        0 => assert!(route_qobuz_event(&app, &IoEvent::NextTrack).await),
        1 => assert!(skip(&app, Direction::Prev).await),
        _ => play_index(&app, usize::MAX, None).await,
      }
      let kept = {
        let mut guard = app.lock().await;
        tick(&mut guard);
        guard.qobuz_playback.as_ref().is_some_and(|s| s.advancing)
      };
      release.send(()).unwrap();
      commit.await.unwrap();
      assert!(
        kept,
        "a boundary request must keep the pending stage latched"
      );
      let guard = app.lock().await;
      let s = guard.qobuz_playback.as_ref().unwrap();
      assert!(!s.advancing);
      assert!(!s.player.is_finished() && !s.player.is_paused());
    }
  }

  #[test]
  fn stale_unauthorized_list_and_search_results_keep_the_current_login_status() {
    let epoch = auth::TOKEN_EPOCH.load(Ordering::SeqCst).wrapping_sub(1);
    let mut app = App::default();
    app.set_status_message("Signed in again", 30);
    show_tracks(
      &mut app,
      "qobuz:playlist:1",
      Err(Unauthorized.into()),
      epoch,
    );
    assert_eq!(app.status_message(), Some("Signed in again"));
    show_search(&mut app, "query", Err(Unauthorized.into()), epoch);
    assert_eq!(app.status_message(), Some("Signed in again"));
    assert!(!app.status_message_is_error());
  }

  #[tokio::test]
  async fn the_pump_returns_before_the_scrape_and_the_work_runs_after_it() {
    let s = scrapes();
    let app = test_app();
    let done = Arc::new(AtomicBool::new(false));
    let work = |constants: &Arc<Constants>| {
      let (constants, app, done) = (Arc::clone(constants), Arc::clone(&app), Arc::clone(&done));
      async move {
        if constants.get(&app).await.is_ok() {
          done.store(true, Ordering::SeqCst);
        }
      }
    };

    // Cold: the call returns at once, the scrape still unanswered.
    off_pump_until_ready(&s.constants, work(&s.constants)).await;
    settle().await;
    assert_eq!(s.started.load(Ordering::SeqCst), 1);
    assert!(!done.load(Ordering::SeqCst));
    assert_eq!(status(&app).await.as_deref(), Some(NOTICE));
    assert!(!app.lock().await.status_message_is_error());

    s.answer.send(Ok(bundle())).unwrap();
    settle().await;
    assert!(done.load(Ordering::SeqCst));
    assert_eq!(status(&app).await, None, "the notice goes with the scrape");
    assert_eq!(s.constants.ready(), Some(bundle()));

    // Ready: the work runs on the pump, in order, as on main.
    done.store(false, Ordering::SeqCst);
    off_pump_until_ready(&s.constants, work(&s.constants)).await;
    assert!(done.load(Ordering::SeqCst));
    assert_eq!(s.started.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn every_waiter_shares_one_scrape() {
    let s = scrapes();
    let (app, _dir) = app_with_state();
    let waiters: Vec<_> = (0..3)
      .map(|_| {
        let (constants, app) = (Arc::clone(&s.constants), Arc::clone(&app));
        tokio::spawn(async move { constants.get(&app).await })
      })
      .collect();
    settle().await;
    assert_eq!(s.started.load(Ordering::SeqCst), 1);

    s.answer.send(Ok(bundle())).unwrap();
    for waiter in waiters {
      assert_eq!(waiter.await.unwrap().unwrap(), bundle());
    }
    // A fresh answer is saved for the next launch.
    assert_eq!(saved_bundle(&mut *app.lock().await), Some(bundle()));
  }

  #[tokio::test]
  async fn a_failed_scrape_answers_every_waiter_and_the_next_action_retries() {
    let s = scrapes();
    let app = test_app();
    let waiters: Vec<_> = (0..2)
      .map(|_| {
        let (constants, app) = (Arc::clone(&s.constants), Arc::clone(&app));
        tokio::spawn(async move { constants.get(&app).await })
      })
      .collect();
    settle().await;
    s.answer.send(Err(anyhow!("login page request"))).unwrap();
    for waiter in waiters {
      let err = waiter.await.unwrap().unwrap_err();
      assert_eq!(err.to_string(), "login page request");
    }
    assert_eq!(s.started.load(Ordering::SeqCst), 1);
    assert_eq!(s.constants.ready(), None);
    assert_eq!(status(&app).await, None);

    s.answer.send(Ok(bundle())).unwrap();
    assert_eq!(s.constants.get(&app).await.unwrap(), bundle());
    assert_eq!(s.started.load(Ordering::SeqCst), 2);
  }

  #[tokio::test]
  async fn a_waiter_that_goes_away_does_not_stop_the_scrape() {
    let s = scrapes();
    let app = test_app();
    let (constants, waiter_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let waiter = tokio::spawn(async move { constants.get(&waiter_app).await });
    settle().await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());

    s.answer.send(Ok(bundle())).unwrap();
    settle().await;
    assert_eq!(s.constants.ready(), Some(bundle()));
    assert_eq!(status(&app).await, None);
  }

  #[tokio::test]
  async fn a_scrape_that_outlives_a_login_change_saves_and_answers_nothing() {
    let s = scrapes();
    let (app, _dir) = app_with_state();
    let (constants, waiter_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let stale = tokio::spawn(async move { constants.get(&waiter_app).await });
    settle().await;

    s.login_epoch.fetch_add(1, Ordering::SeqCst);
    // An action under the new login does not join the old scrape.
    let (constants, waiter_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fresh = tokio::spawn(async move { constants.get(&waiter_app).await });
    settle().await;
    assert_eq!(s.started.load(Ordering::SeqCst), 2);

    s.answer.send(Ok(bundle())).unwrap();
    let err = stale.await.unwrap().unwrap_err();
    assert!(err.is::<LoginChanged>());
    assert_eq!(s.constants.ready(), None);
    {
      let mut guard = app.lock().await;
      assert_eq!(saved_bundle(&mut guard), None);
      // The stale work is dropped without a message.
      report_locked(&mut guard, QobuzStep::Constants, err, 0);
      assert_eq!(guard.status_message(), Some(NOTICE));
    }

    s.answer.send(Ok(bundle())).unwrap();
    assert_eq!(fresh.await.unwrap().unwrap(), bundle());
    assert_eq!(s.constants.ready(), Some(bundle()));
  }

  #[tokio::test]
  async fn a_panicking_resolver_fails_that_scrape_only() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let constants = Constants::new(
      Box::leak(Box::new(AtomicU64::new(0))),
      Box::new(move |_cached| {
        let first = count.fetch_add(1, Ordering::SeqCst) == 0;
        async move {
          assert!(!first, "the bundle parser panicked");
          Ok(bundle())
        }
        .boxed()
      }),
    );
    let app = test_app();

    let err = constants.get(&app).await.unwrap_err();
    assert_eq!(err.to_string(), "the resolver panicked");
    assert_eq!(status(&app).await, None);

    assert_eq!(constants.get(&app).await.unwrap(), bundle());
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
  }

  #[tokio::test]
  async fn a_cold_listing_lands_after_the_scrape() {
    let s = scrapes();
    let app = test_app();
    let uri = "qobuz:playlist:1";
    app
      .lock()
      .await
      .open_source_playlist_tracks(uri.to_string());
    let (constants, work_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let work = async move {
      let result = constants.get(&work_app).await;
      let rows = result.map(|_| vec![queued_track("qobuz:track:1", "One")]);
      show_tracks(&mut *work_app.lock().await, uri, rows, 0);
    };
    off_pump_until_ready(&s.constants, work).await;
    settle().await;
    assert!(app.lock().await.track_table.tracks.is_empty());

    s.answer.send(Ok(bundle())).unwrap();
    settle().await;
    let guard = app.lock().await;
    assert_eq!(guard.track_table.tracks.len(), 1);
    assert_eq!(
      guard.track_table.context,
      Some(TrackTableContext::QobuzPlaylist)
    );
  }

  #[test]
  fn rows_for_a_list_the_user_left_do_not_replace_the_newer_view() {
    let rows = || Ok(vec![queued_track("qobuz:track:1", "One")]);

    // Another Qobuz list opened meanwhile: only its own rows land.
    let mut app = App::default();
    app.open_source_playlist_tracks("qobuz:playlist:1".to_string());
    app.open_source_playlist_tracks("qobuz:playlist:2".to_string());
    show_tracks(&mut app, "qobuz:playlist:1", rows(), 0);
    assert!(app.track_table.tracks.is_empty());
    show_tracks(&mut app, "qobuz:playlist:2", rows(), 0);
    assert_eq!(app.track_table.tracks.len(), 1);

    // Another source's list opened meanwhile, and its rows already landed.
    let mut app = App::default();
    app.open_source_playlist_tracks("qobuz:playlist:1".to_string());
    app.set_track_table(
      vec![queued_track("spotify:track:1", "Liked")],
      TrackTableContext::SavedTracks,
    );
    show_tracks(&mut app, "qobuz:playlist:1", rows(), 0);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
    assert_eq!(app.track_table.tracks[0].name, "Liked");

    // A failure is still reported.
    show_tracks(&mut app, "qobuz:playlist:1", Err(anyhow!("HTTP 500")), 0);
    assert!(app.status_message().is_some_and(|m| m.contains("HTTP 500")));
  }

  #[tokio::test]
  async fn a_cold_search_lands_after_the_scrape_unless_a_newer_search_replaced_it() {
    use crate::core::action::Action;
    let s = scrapes();
    let app = test_app();
    let search = |query: &'static str| {
      let (constants, app) = (Arc::clone(&s.constants), Arc::clone(&app));
      async move {
        let result = constants.get(&app).await;
        let hits = result.map(|_| vec![queued_track("qobuz:track:1", query)]);
        show_search(&mut *app.lock().await, query, hits, 0);
      }
    };
    let shown = |app: &App| {
      let tracks = app.search_results().tracks.as_ref();
      tracks.map(|page| page.items[0].name.clone())
    };
    {
      let mut guard = app.lock().await;
      guard.active_source = Source::Qobuz;
      guard.apply(Action::SearchActiveSource("first".to_string()));
    }
    off_pump_until_ready(&s.constants, search("first")).await;
    app
      .lock()
      .await
      .apply(Action::SearchActiveSource("second".to_string()));
    off_pump_until_ready(&s.constants, search("second")).await;
    settle().await;
    assert_eq!(s.started.load(Ordering::SeqCst), 1);

    s.answer.send(Ok(bundle())).unwrap();
    settle().await;
    assert_eq!(shown(&*app.lock().await).as_deref(), Some("second"));

    // A search on another source replaces a Qobuz one that answers late.
    let mut guard = app.lock().await;
    guard.apply(Action::SearchActiveSource("third".to_string()));
    guard.apply(Action::Search("third".to_string()));
    show_search(
      &mut guard,
      "third",
      Ok(vec![queued_track("qobuz:track:3", "third")]),
      0,
    );
    assert_eq!(shown(&guard).as_deref(), Some("second"));
  }

  #[tokio::test]
  async fn a_cold_start_waits_in_its_fetch_and_then_plays() {
    let s = scrapes();
    let app = test_app();
    app.lock().await.qobuz_playback = Some(waiting_session(7));
    let (constants, fetch_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fetch = tokio::spawn(async move { constants_for_fetch(&fetch_app, &constants).await });
    settle().await;
    assert_eq!(status(&app).await.as_deref(), Some(NOTICE));
    {
      let guard = app.lock().await;
      let session = guard.qobuz_playback.as_ref().unwrap();
      assert!(session.advancing, "the tick does not read the empty sink");
      assert!(session.player.is_paused(), "the sink stays silent");
      assert!(!session.is_paused(), "default intent is playing");
    }

    s.answer.send(Ok(bundle())).unwrap();
    assert_eq!(fetch.await.unwrap().unwrap(), bundle());
    let guard = app.lock().await;
    let session = guard.qobuz_playback.as_ref().unwrap();
    assert_eq!(session.fetch_id, 7);
    assert_eq!(session.resume_at, None, "it plays when it is staged");
  }

  #[tokio::test]
  async fn a_pause_during_the_scrape_holds_the_started_track_paused() {
    let s = scrapes();
    let app = test_app();
    app.lock().await.qobuz_playback = Some(waiting_session(7));
    let (constants, fetch_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fetch = tokio::spawn(async move { constants_for_fetch(&fetch_app, &constants).await });
    settle().await;

    app.lock().await.toggle_playback();
    s.answer.send(Ok(bundle())).unwrap();
    fetch.await.unwrap().unwrap();
    let guard = app.lock().await;
    let resume = guard.qobuz_playback.as_ref().unwrap().resume_at;
    assert!(resume.is_some_and(|r| r.paused && r.position_ms == 0));
    drop(guard);
    commit_test_track(&app).await;
    assert!(app
      .lock()
      .await
      .qobuz_playback
      .as_ref()
      .unwrap()
      .player
      .is_paused());
  }

  #[tokio::test]
  async fn a_resume_during_preparation_overrides_a_pause_during_the_scrape() {
    let s = scrapes();
    let app = test_app();
    app.lock().await.qobuz_playback = Some(waiting_session(7));
    let (constants, fetch_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fetch = tokio::spawn(async move { constants_for_fetch(&fetch_app, &constants).await });
    settle().await;

    app.lock().await.toggle_playback();
    s.answer.send(Ok(bundle())).unwrap();
    fetch.await.unwrap().unwrap();
    app.lock().await.toggle_playback();
    commit_test_track(&app).await;
    assert!(!app
      .lock()
      .await
      .qobuz_playback
      .as_ref()
      .unwrap()
      .player
      .is_paused());
  }

  #[tokio::test]
  async fn a_routed_resume_during_preparation_overrides_the_saved_pause() {
    let s = scrapes();
    let app = test_app();
    app.lock().await.qobuz_playback = Some(waiting_session(7));
    let (constants, fetch_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fetch = tokio::spawn(async move { constants_for_fetch(&fetch_app, &constants).await });
    settle().await;

    assert!(route_qobuz_event(&app, &IoEvent::PausePlayback).await);
    s.answer.send(Ok(bundle())).unwrap();
    fetch.await.unwrap().unwrap();
    assert!(route_qobuz_event(&app, &IoEvent::StartPlayback(None, None, None)).await);
    commit_test_track(&app).await;
    assert!(!app
      .lock()
      .await
      .qobuz_playback
      .as_ref()
      .unwrap()
      .player
      .is_paused());
  }

  #[tokio::test]
  async fn a_newer_start_during_the_scrape_supersedes_the_waiting_one() {
    let s = scrapes();
    let app = test_app();
    let (constants, fetch_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fetch = tokio::spawn(async move {
      match constants_for_fetch(&fetch_app, &constants).await {
        Ok(_) => panic!("a superseded fetch went on to the stream"),
        Err(e) => fail_fetch(&fetch_app, 7, QobuzStep::Stream, e, 0).await,
      }
    });
    let mut first = waiting_session(7);
    first.fetch = Some(fetch.abort_handle());
    app.lock().await.qobuz_playback = Some(first);
    settle().await;

    // The user starts another Qobuz track: the session is replaced, which
    // cancels the first fetch, as a skip or another source's start does.
    app.lock().await.qobuz_playback = Some(waiting_session(8));
    assert!(fetch.await.unwrap_err().is_cancelled());

    // A late failure of the old fetch leaves the new session alone.
    fail_fetch(&app, 7, QobuzStep::Stream, anyhow!("late"), 0).await;
    s.answer.send(Ok(bundle())).unwrap();
    settle().await;
    let guard = app.lock().await;
    let session = guard.qobuz_playback.as_ref().unwrap();
    assert_eq!(session.fetch_id, 8);
    assert_eq!(session.resume_at, None);
    assert_eq!(guard.status_message(), None);
  }

  #[tokio::test]
  async fn a_failed_scrape_ends_the_started_session_like_a_failed_stream() {
    let s = scrapes();
    let app = test_app();
    app.lock().await.qobuz_playback = Some(waiting_session(7));
    let (constants, fetch_app) = (Arc::clone(&s.constants), Arc::clone(&app));
    let fetch = tokio::spawn(async move {
      let err = constants_for_fetch(&fetch_app, &constants)
        .await
        .unwrap_err();
      fail_fetch(&fetch_app, 7, QobuzStep::Stream, err, 0).await;
    });
    settle().await;
    s.answer.send(Err(anyhow!("login page request"))).unwrap();
    fetch.await.unwrap();

    let guard = app.lock().await;
    assert!(guard.qobuz_playback.is_none());
    let message = guard.status_message().unwrap_or_default();
    assert!(
      message.contains("web player constants: login page request"),
      "{message}"
    );
  }

  #[test]
  fn a_segment_failure_replaces_the_decoder_error() {
    let failure = progressive::Failure::default();
    let decoder = || anyhow!("decoding audio stream: format not recognized");
    assert!(stream_cause(&failure, decoder())
      .to_string()
      .starts_with("decoding"));
    failure
      .set("segment 1 returned HTTP 403 Forbidden".to_string())
      .unwrap();
    assert_eq!(
      stream_cause(&failure, decoder()).to_string(),
      "segment 1 returned HTTP 403 Forbidden"
    );
  }

  #[test]
  fn a_track_without_a_full_stream_names_the_subscription() {
    let mut app = App::default();
    let err = anyhow::Error::new(NoFullStream).context("file url");
    report_locked(&mut app, QobuzStep::Stream, err, 0);
    assert_eq!(
      app.status_message(),
      Some(crate::core::i18n::EN.status.infra.qobuz_no_full_stream)
    );
    assert!(app.status_message_is_error());
  }

  #[test]
  fn qobuz_uris_are_recognised_by_scheme() {
    assert!(is_qobuz_uri("qobuz:track:1"));
    assert!(!is_qobuz_uri("subsonic:track:1"));
  }
}

#[cfg(test)]
mod sync_error_localization_tests {
  use super::*;
  #[test]
  fn a_missing_sync_login_is_structured_and_keeps_the_english_diagnostic() {
    use crate::core::playlist_sync::SyncReason;
    let error = sync_token(None).unwrap_err();
    assert_eq!(error.to_string(), "Qobuz is not logged in");
    let reason = SyncReason::from_error(&error);
    assert_eq!(reason, SyncReason::QobuzNotLoggedIn);
    for strings in [
      &crate::core::i18n::EN,
      &crate::core::i18n::ZH_HANS,
      &crate::core::i18n::ZH_HANT,
    ] {
      assert_eq!(
        reason.render(strings),
        strings.status.infra.sync_qobuz_not_logged_in
      );
    }
    assert_eq!(sync_token(Some("{token}".into())).unwrap(), "{token}");
  }
}
