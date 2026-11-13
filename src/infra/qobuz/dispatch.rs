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

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::future::{BoxFuture, Shared};
use futures::FutureExt;
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::auth::{self, QobuzBundleCache};
use super::stream::progressive;
use super::{
  track_id_from_uri, NoFullStream, QobuzPlaybackState, QobuzSource, ResumePoint, StreamQuality,
  Unauthorized,
};
use crate::core::app::{App, TrackTableContext};
use crate::core::source::{MediaSource, Searcher, Source};
use crate::core::state::PersistedRuntimeState;
use crate::infra::audio::{LocalPlayer, PreparedStream};
use crate::infra::network::IoEvent;
use crate::infra::queue::{
  advance_index, apply_resolved, placeholder_uri, replay_file, snapshot_tracks,
};

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

type ConstantsResolution = Shared<BoxFuture<'static, Result<QobuzBundleCache, String>>>;

/// Share failures as well as successes, and keep resolving if a waiter is cancelled.
#[derive(Default)]
struct ConstantsResolver {
  state: std::sync::Mutex<ConstantsState>,
  ready: tokio::sync::Notify,
}

#[derive(Default)]
struct ConstantsState {
  resolved: Option<QobuzBundleCache>,
  in_flight: Option<ConstantsResolution>,
  completed: Option<Result<QobuzBundleCache, String>>,
  pending: VecDeque<PendingConstantsEvent>,
}

struct PendingConstantsEvent {
  app: Arc<Mutex<App>>,
  event: IoEvent,
  epoch: u64,
  playback_intent: Option<u64>,
}

impl ConstantsResolver {
  fn resolve_with<F, Fut>(self: &Arc<Self>, resolve: F) -> ConstantsResolution
  where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<QobuzBundleCache>> + Send,
  {
    if let Ok(mut state) = self.state.lock() {
      self.start(&mut state, resolve)
    } else {
      futures::future::ready(Err("Qobuz constants resolver lock poisoned".into()))
        .boxed()
        .shared()
    }
  }

  fn defer_with<F, Fut>(
    self: &Arc<Self>,
    app: &Arc<Mutex<App>>,
    locked_app: &mut App,
    event: IoEvent,
    epoch: u64,
    resolve: F,
  ) -> bool
  where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<QobuzBundleCache>> + Send,
  {
    let Ok(mut state) = self.state.lock() else {
      report_locked(
        locked_app,
        QobuzStep::Constants,
        anyhow!("Qobuz constants resolver lock poisoned"),
      );
      return true;
    };
    if state.resolved.is_some() && state.pending.is_empty() {
      return false;
    }
    if state.pending.is_empty()
      && state
        .in_flight
        .as_ref()
        .is_none_or(|resolution| resolution.peek().is_none())
      && auth::token_epoch() == epoch
    {
      let message = locked_app.strings().status.infra.qobuz_preparing;
      locked_app.set_status_message(message, 30);
    }
    let playback_intent = matches!(
      event,
      IoEvent::StartPlayback(..) | IoEvent::AdvanceNativeQueue
    )
    .then(|| locked_app.playback_intent_generation());
    state.pending.push_back(PendingConstantsEvent {
      app: Arc::clone(app),
      event,
      epoch,
      playback_intent,
    });
    let _resolution = self.start(&mut state, resolve);
    true
  }

  /// Only the serial pump drains this queue. New pump events keep deferring
  /// while it is nonempty; replay never goes to the back of the IO channel.
  async fn next_pending(&self) -> Option<PendingConstantsEvent> {
    loop {
      let (pending, result) = {
        let Ok(mut state) = self.state.lock() else {
          return None;
        };
        let result = state.completed.clone()?;
        let pending = state.pending.pop_front()?;
        if state.pending.is_empty() {
          state.in_flight = None;
          state.completed = None;
        }
        (pending, result)
      };
      let mut app = pending.app.lock().await;
      if auth::token_epoch() != pending.epoch
        || pending
          .playback_intent
          .is_some_and(|generation| generation != app.playback_intent_generation())
      {
        continue;
      }
      if app.status_message() == Some(app.strings().status.infra.qobuz_preparing) {
        app.clear_status_message();
      }
      match result {
        Ok(_) => {
          drop(app);
          return Some(pending);
        }
        Err(err) => report_locked(&mut app, QobuzStep::Constants, anyhow!(err)),
      }
    }
  }

  fn start<F, Fut>(self: &Arc<Self>, state: &mut ConstantsState, resolve: F) -> ConstantsResolution
  where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<QobuzBundleCache>> + Send,
  {
    if let Some(constants) = state.resolved.clone() {
      return futures::future::ready(Ok(constants)).boxed().shared();
    }
    if let Some(resolution) = state.in_flight.as_ref() {
      return resolution.clone();
    }
    let resolution = async move { resolve().await.map_err(|err| format!("{err:#}")) }
      .boxed()
      .shared();
    state.in_flight = Some(resolution.clone());
    let resolver = Arc::clone(self);
    let task = resolution.clone();
    tokio::spawn(async move {
      let result = task.await;
      if let Ok(mut state) = resolver.state.lock() {
        state.resolved = result.as_ref().ok().cloned();
        if state.pending.is_empty() {
          state.in_flight = None;
        } else {
          state.completed = Some(result);
        }
      }
      resolver.ready.notify_one();
    });
    resolution
  }
}

fn constants_resolver() -> &'static Arc<ConstantsResolver> {
  static RESOLVER: OnceLock<Arc<ConstantsResolver>> = OnceLock::new();
  RESOLVER.get_or_init(|| Arc::new(ConstantsResolver::default()))
}

/// Pump callers defer their event; off-pump callers (playlist sync, startup)
/// can await the same resolve directly.
async fn constants(app: &Arc<Mutex<App>>) -> Result<QobuzBundleCache> {
  let app = Arc::clone(app);
  constants_resolver()
    .resolve_with(move || async move { resolve_constants(&app).await })
    .await
    .map_err(|err| anyhow!(err))
}

/// The scrape and cache read run only on the detached resolver task.
async fn resolve_constants(app: &Arc<Mutex<App>>) -> Result<QobuzBundleCache> {
  let epoch = auth::token_epoch();
  let http = super::shared_qobuz_client();
  if auth::constants_resolved() {
    return auth::resolve_constants(&http, None).await;
  }
  let cached = cached_bundle(app).await;
  let resolved = auth::resolve_constants(&http, cached.as_ref()).await?;
  if cached.as_ref() != Some(&resolved) && resolved.bundle_version != "env" {
    let mut app = app.lock().await;
    if auth::token_epoch() == epoch {
      app.schedule_state_save(PersistedRuntimeState::qobuz_bundle_cache(resolved.clone()));
    }
  }
  Ok(resolved)
}

/// Hold the exact user action without awaiting the scrape on the serial pump.
pub(crate) fn defer_constants(
  app: &Arc<Mutex<App>>,
  locked_app: &mut App,
  event: &IoEvent,
) -> bool {
  let retry = match event {
    IoEvent::GetQobuzPlaylists => IoEvent::GetQobuzPlaylists,
    IoEvent::GetQobuzTracks(uri) => IoEvent::GetQobuzTracks(uri.clone()),
    IoEvent::GetQobuzSearchResults(query) => IoEvent::GetQobuzSearchResults(query.clone()),
    IoEvent::QobuzLogin => IoEvent::QobuzLogin,
    IoEvent::StartPlayback(context, uris, offset)
      if context.as_deref().is_some_and(is_qobuz_uri)
        || (context.is_none()
          && uris
            .as_ref()
            .and_then(|uris| uris.first())
            .is_some_and(|uri| is_qobuz_uri(uri))) =>
    {
      IoEvent::StartPlayback(context.clone(), uris.clone(), *offset)
    }
    IoEvent::AdvanceNativeQueue
      if !locked_app.apple_music_owns_playback()
        && locked_app
          .native_queue
          .first()
          .and_then(|track| track.uri.as_deref())
          .is_some_and(is_qobuz_uri) =>
    {
      IoEvent::AdvanceNativeQueue
    }
    _ => return false,
  };
  let epoch = auth::token_epoch();
  let app_for_resolve = Arc::clone(app);
  constants_resolver().defer_with(app, locked_app, retry, epoch, move || async move {
    resolve_constants(&app_for_resolve).await
  })
}

pub(crate) fn constants_ready() -> &'static tokio::sync::Notify {
  &constants_resolver().ready
}

pub(crate) async fn next_pending_constants_event() -> Option<IoEvent> {
  constants_resolver()
    .next_pending()
    .await
    .map(|pending| pending.event)
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
      WhenLoggedOut::Message => {
        set_error(
          app,
          crate::infra::app_strings(app)
            .await
            .status
            .infra
            .qobuz_login_expired
            .to_string(),
        )
        .await
      }
    }
    return None;
  };
  match constants(app).await {
    Ok(c) => Some(QobuzSource::new(c.app_id, c.app_secret, token)),
    Err(e) => {
      report(app, QobuzStep::Constants, e).await;
      None
    }
  }
}

/// A source for a playback path (the native queue lane); shows the login
/// message and returns `None` when logged out.
pub(crate) async fn build_playback_source(app: &Arc<Mutex<App>>) -> Option<QobuzSource> {
  build_source(app, WhenLoggedOut::Message).await
}

/// A source for the playlist sync; the caller reports the logged-out case itself.
pub(crate) async fn build_sync_source(app: &Arc<Mutex<App>>) -> Result<QobuzSource> {
  let token = sync_token(auth::current_token())?;
  let constants = constants(app).await.context("Qobuz web player constants")?;
  Ok(QobuzSource::new(
    constants.app_id,
    constants.app_secret,
    token,
  ))
}

fn sync_token(token: Option<String>) -> Result<String> {
  token.ok_or_else(|| crate::core::playlist_sync::SyncReason::QobuzNotLoggedIn.into())
}

/// Report a failed call as one status message; a 401 also clears the token.
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

async fn report(app: &Arc<Mutex<App>>, step: QobuzStep, err: anyhow::Error) {
  let mut guard = app.lock().await;
  report_locked(&mut guard, step, err);
}

fn report_locked(app: &mut App, step: QobuzStep, err: anyhow::Error) {
  if err.downcast_ref::<Unauthorized>().is_some() {
    auth::set_token(None);
    let strings = app.strings();
    app.set_error_status_message(strings.status.infra.qobuz_login_expired, 8);
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
async fn begin_login(app: &Arc<Mutex<App>>) {
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
  let constants = match constants(app).await {
    Ok(c) => c,
    Err(e) => {
      LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
      report(app, QobuzStep::Constants, e).await;
      return;
    }
  };
  let attempt = match auth::LoginAttempt::bind(constants).await {
    Ok(a) => a,
    Err(e) => {
      LOGIN_IN_PROGRESS.store(false, Ordering::SeqCst);
      report(app, QobuzStep::Login, e).await;
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
async fn load_qobuz_playlists(app: &Arc<Mutex<App>>) {
  let Some(source) = build_source(app, WhenLoggedOut::Login).await else {
    return;
  };
  match source.playlists().await {
    Ok(playlists) => *app.lock().await.qobuz_playlists_mut() = playlists,
    Err(e) => report(app, QobuzStep::Library, e).await,
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
      app.set_source_track_table(playlist_uri, tracks, TrackTableContext::QobuzPlaylist);
    }
    Err(e) => report(app, QobuzStep::Tracks, e).await,
  }
}

/// Run a catalog search and populate the songs block of `app.search_results`.
async fn run_qobuz_search(app: &Arc<Mutex<App>>, query: &str) {
  let Some(source) = build_source(app, WhenLoggedOut::Login).await else {
    return;
  };
  match source.search(query).await {
    Ok(results) => app
      .lock()
      .await
      .show_source_search_tracks(query, results.tracks),
    Err(e) => report(app, QobuzStep::Search, e).await,
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
  source: &QobuzSource,
  uri: &str,
  quality: u8,
) -> Result<(NamedTempFile, String)> {
  let track_id = track_id_from_uri(uri)?;
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
  spawn_resolve(app, session);
  let app = Arc::clone(app);
  let source = Arc::clone(&session.source);
  let fetch_id = session.fetch_id;
  let task = tokio::spawn(async move {
    match prepare_track(&source, &track_id, quality).await {
      Ok((tmp, complete, delivered, prepared)) => {
        commit_fetch(&app, fetch_id, (tmp, complete), delivered, prepared).await
      }
      Err(e) => fail_fetch(&app, fetch_id, QobuzStep::Stream, e).await,
    }
  });
  session.fetch = Some(task.abort_handle());
}

/// Look the current track up in the catalogue when it is a placeholder (a URI
/// started from outside the track table, as an MCP or AI DJ `play_now` sends,
/// #716), so the playbar and the OS media controls show its title instead of
/// the URI. Runs on its own task: neither the pump nor the stream waits for
/// it. The answer lands only while the session still plays that fetch and
/// that row; a failed lookup keeps the placeholder and never stops playback.
fn spawn_resolve(app: &Arc<Mutex<App>>, session: &QobuzPlaybackState) {
  let Some(uri) = placeholder_uri(&session.tracks, session.index) else {
    return;
  };
  let Ok(track_id) = track_id_from_uri(&uri).map(str::to_string) else {
    return;
  };
  let app = Arc::clone(app);
  let source = Arc::clone(&session.source);
  let fetch_id = session.fetch_id;
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
async fn fail_fetch(app: &Arc<Mutex<App>>, fetch_id: u64, step: QobuzStep, err: anyhow::Error) {
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
  (tmp, complete): (NamedTempFile, progressive::Completion),
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
    s.complete = complete;
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
    // The device went under the stage, and the dropped stream left the file
    // partial: the replay fetches it again, with the same pause. The advance
    // latch stays on until it plays.
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
      return;
    }
  };
  let Some(source) = build_source(app, WhenLoggedOut::Message).await else {
    return;
  };
  let source = Arc::new(source);

  // Only one backend owns the device at a time.
  app.lock().await.claim_decoded_sink(Source::Qobuz);
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
/// second download when the file is whole. Returns `true` if Qobuz owns the session.
async fn replay_current(app: &Arc<Mutex<App>>) -> bool {
  let (player, path, resume) = {
    let mut guard = app.lock().await;
    let Some(s) = guard.qobuz_playback.as_mut() else {
      return false;
    };
    s.advancing = true;
    // Still downloading: the commit applies `resume_at`, clears `advancing`
    // and starts playback.
    if s.tempfile.is_none() {
      return true;
    }
    let finished = s.finished_file().map(std::path::Path::to_path_buf);
    let resume = s.resume_at.take().unwrap_or(ResumePoint {
      position_ms: 0,
      paused: s.player.is_paused(),
    });
    match finished {
      Some(path) => (Arc::clone(&s.player), path, resume),
      // A partial file (a dropped reader cancelled its download): fetch again.
      None => {
        let index = s.index;
        drop(guard);
        play_index(app, index, Some(resume)).await;
        return true;
      }
    }
  };
  if replay_file(player, path, Some(resume)).await {
    if let Some(s) = app.lock().await.qobuz_playback.as_mut() {
      s.advancing = false;
    }
  } else {
    teardown_qobuz(app).await;
    set_error(
      app,
      crate::infra::app_strings(app)
        .await
        .status
        .infra
        .qobuz_replay_failed
        .to_string(),
    )
    .await;
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

  fn app_with_events() -> (Arc<Mutex<App>>, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = App::new(tx, crate::core::user_config::UserConfig::new(), None);
    (Arc::new(Mutex::new(app)), rx)
  }

  fn bundle_constants() -> QobuzBundleCache {
    QobuzBundleCache {
      bundle_version: "fixture".into(),
      app_id: "fixture-id".into(),
      app_secret: "fixture-secret".into(),
      oauth_key: "fixture-key".into(),
    }
  }

  async fn resolution_finishes(resolver: &ConstantsResolver) {
    tokio::time::timeout(Duration::from_secs(5), async {
      loop {
        if let Some(pending) = resolver.next_pending().await {
          pending
            .app
            .lock()
            .await
            .dispatch_without_spinner(pending.event);
        }
        if resolver
          .state
          .lock()
          .expect("resolver state")
          .in_flight
          .is_none()
        {
          return;
        }
        tokio::task::yield_now().await;
      }
    })
    .await
    .expect("detached constants resolution did not finish");
  }

  #[tokio::test]
  async fn later_playback_requests_drop_parked_qobuz_starts_but_keep_browsing() {
    for advance in [false, true] {
      for event in [
        IoEvent::StartPlayback(Some("file:///later.mp3".into()), None, None),
        IoEvent::StartPlayback(None, Some(vec!["spotify:track:later".into()]), Some(0)),
        IoEvent::StartPlayback(Some("radio:https://example.com/live".into()), None, None),
        IoEvent::StartPlayback(Some("qobuz:track:later".into()), None, None),
        IoEvent::StartPlayback(Some("subsonic:track:later".into()), None, None),
        IoEvent::StartPlayback(Some("youtube:track:later".into()), None, None),
        IoEvent::StartPlayback(Some("applemusic:track:later".into()), None, None),
        IoEvent::PausePlayback,
        IoEvent::NextTrack,
        IoEvent::PreviousTrack,
        IoEvent::ForcePreviousTrack,
        IoEvent::AdvanceNativeQueue,
        IoEvent::FinishNativeQueue,
        IoEvent::ReplayCurrentTrack,
        IoEvent::ResumeSpotifyContext(None, None),
        IoEvent::ResumeNativeShuffleSession(Some(0), 0),
        IoEvent::ReshuffleNativeShuffleLap,
      ] {
        let resolver = Arc::new(ConstantsResolver::default());
        let (app, rx) = app_with_events();
        let (finish, wait) = tokio::sync::oneshot::channel();
        let epoch = auth::token_epoch();
        {
          let mut guard = app.lock().await;
          let start = if advance {
            IoEvent::AdvanceNativeQueue
          } else {
            IoEvent::StartPlayback(Some("qobuz:track:older".into()), None, None)
          };
          guard.note_playback_intent(&start);
          assert!(
            resolver.defer_with(&app, &mut guard, start, epoch, || async {
              wait.await.unwrap()
            })
          );
          assert!(resolver.defer_with(
            &app,
            &mut guard,
            IoEvent::GetQobuzPlaylists,
            epoch,
            || async { panic!("browsing shares the parked start's resolve") }
          ));
          for event in [
            IoEvent::GetQobuzTracks("qobuz:album:1".into()),
            IoEvent::GetQobuzSearchResults("query".into()),
            IoEvent::QobuzLogin,
          ] {
            assert!(
              resolver.defer_with(&app, &mut guard, event, epoch, || async {
                panic!("shared scrape")
              })
            );
          }
          guard.dispatch(event);
        }
        assert!(rx.try_recv().is_ok(), "the later request reached the pump");
        finish.send(Ok(bundle_constants())).unwrap();
        resolution_finishes(&resolver).await;
        assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzPlaylists)));
        assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzTracks(_))));
        assert!(matches!(
          rx.try_recv(),
          Ok(IoEvent::GetQobuzSearchResults(_))
        ));
        assert!(matches!(rx.try_recv(), Ok(IoEvent::QobuzLogin)));
        assert!(
          rx.try_recv().is_err(),
          "a superseded start must never replay"
        );
      }
    }
  }

  #[tokio::test]
  async fn playback_adjustments_keep_parked_qobuz_starts() {
    for advance in [false, true] {
      for event in [
        IoEvent::ChangeVolume(50),
        IoEvent::Seek(100),
        IoEvent::Shuffle(true),
        IoEvent::Repeat(rspotify::model::RepeatState::Context),
        IoEvent::TransferPlaybackToDevice("device".into(), false),
        IoEvent::AutoSelectStreamingDevice("device".into(), false, false),
        IoEvent::EnsurePlaybackContinues("spotify:track:current".into()),
        IoEvent::ToggleNativeShuffleSession(true),
      ] {
        let resolver = Arc::new(ConstantsResolver::default());
        let (app, rx) = app_with_events();
        let (finish, wait) = tokio::sync::oneshot::channel();
        let mut guard = app.lock().await;
        let start = if advance {
          IoEvent::AdvanceNativeQueue
        } else {
          IoEvent::StartPlayback(Some("qobuz:track:waiting".into()), None, None)
        };
        guard.note_playback_intent(&start);
        assert!(
          resolver.defer_with(&app, &mut guard, start, auth::token_epoch(), || async {
            wait.await.unwrap()
          })
        );
        let generation = guard.playback_intent_generation();
        guard.dispatch(event);
        // The pump records direct channel requests as well as App dispatches.
        let adjustment = rx.try_recv().expect("adjustment reached the pump");
        guard.note_playback_intent(&adjustment);
        assert_eq!(guard.playback_intent_generation(), generation);
        drop(guard);
        finish.send(Ok(bundle_constants())).unwrap();
        resolution_finishes(&resolver).await;
        if advance {
          assert!(matches!(rx.try_recv(), Ok(IoEvent::AdvanceNativeQueue)));
        } else {
          assert!(
            matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(Some(uri), None, None)) if uri == "qobuz:track:waiting")
          );
        }
        assert!(rx.try_recv().is_err());
        assert_eq!(app.lock().await.playback_intent_generation(), generation);
      }
    }
  }

  #[tokio::test]
  async fn a_new_qobuz_start_supersedes_the_old_start_in_the_pending_batch() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (app, rx) = app_with_events();
    let (finish, wait) = tokio::sync::oneshot::channel();
    let epoch = auth::token_epoch();
    let mut guard = app.lock().await;
    let first = IoEvent::StartPlayback(Some("qobuz:track:older".into()), None, None);
    guard.note_playback_intent(&first);
    assert!(
      resolver.defer_with(&app, &mut guard, first, epoch, || async {
        wait.await.unwrap()
      })
    );
    let newer = IoEvent::StartPlayback(None, Some(vec!["qobuz:track:newer".into()]), Some(0));
    guard.note_playback_intent(&newer);
    assert!(
      resolver.defer_with(&app, &mut guard, newer, epoch, || async {
        panic!("shared scrape")
      })
    );
    let generation = guard.playback_intent_generation();
    guard.dispatch(IoEvent::ChangeVolume(50));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::ChangeVolume(50))));
    assert!(resolver.defer_with(
      &app,
      &mut guard,
      IoEvent::GetQobuzPlaylists,
      epoch,
      || async { panic!("shared scrape") }
    ));
    drop(guard);
    finish.send(Ok(bundle_constants())).unwrap();
    resolution_finishes(&resolver).await;
    assert!(
      matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(None, Some(uris), Some(0))) if uris == ["qobuz:track:newer"])
    );
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzPlaylists)));
    assert!(rx.try_recv().is_err());
    assert_eq!(app.lock().await.playback_intent_generation(), generation);
  }

  #[tokio::test]
  async fn direct_playback_decisions_supersede_a_parked_start() {
    let controls: &[fn(&mut App)] = &[
      App::toggle_playback,
      App::next_track,
      App::previous_track,
      App::force_previous_track,
      |app| app.start_playback_uris(vec!["file:///later.mp3".into()], None),
      |app| app.start_playback_context("radio:https://example.com/live".into(), None),
      |app| app.play_apple_music_track("applemusic:track:later".into()),
      |app| app.set_apple_music_playing(false),
    ];
    for control in controls {
      let resolver = Arc::new(ConstantsResolver::default());
      let (app, rx) = app_with_events();
      let (finish, wait) = tokio::sync::oneshot::channel();
      let mut guard = app.lock().await;
      assert!(resolver.defer_with(
        &app,
        &mut guard,
        IoEvent::AdvanceNativeQueue,
        auth::token_epoch(),
        || async { wait.await.unwrap() }
      ));
      control(&mut guard);
      drop(guard);
      while rx.try_recv().is_ok() {}
      finish.send(Ok(bundle_constants())).unwrap();
      resolution_finishes(&resolver).await;
      assert!(rx.try_recv().is_err());
    }
  }

  #[tokio::test]
  async fn new_arrivals_during_the_ready_batch_stay_behind_older_events() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (app, rx) = app_with_events();
    let (finish, wait) = tokio::sync::oneshot::channel();
    let epoch = auth::token_epoch();
    let resolution = resolver.resolve_with(|| async { wait.await.unwrap() });
    for uri in ["qobuz:album:older", "qobuz:album:second"] {
      assert!(resolver.defer_with(
        &app,
        &mut *app.lock().await,
        IoEvent::GetQobuzTracks(uri.into()),
        epoch,
        || async { panic!("shared scrape") }
      ));
    }
    finish.send(Ok(bundle_constants())).unwrap();
    assert_eq!(resolution.await.unwrap(), bundle_constants());
    resolver.ready.notified().await;
    let first = resolver.next_pending().await.unwrap();
    assert!(matches!(first.event, IoEvent::GetQobuzTracks(uri) if uri == "qobuz:album:older"));
    assert!(
      rx.try_recv().is_err(),
      "replay must not move to the back of the channel"
    );
    assert!(resolver.defer_with(
      &app,
      &mut *app.lock().await,
      IoEvent::GetQobuzTracks("qobuz:album:newer".into()),
      epoch,
      || async { panic!("ready constants") }
    ));
    // Off-pump work can use the result before the pump finishes the batch.
    assert_eq!(
      resolver
        .resolve_with(|| async { panic!("cached constants") })
        .await
        .unwrap(),
      bundle_constants()
    );
    assert!(
      matches!(resolver.next_pending().await.unwrap().event, IoEvent::GetQobuzTracks(uri) if uri == "qobuz:album:second")
    );
    assert!(
      matches!(resolver.next_pending().await.unwrap().event, IoEvent::GetQobuzTracks(uri) if uri == "qobuz:album:newer")
    );
    assert!(resolver.next_pending().await.is_none());
    assert!(!resolver.defer_with(
      &app,
      &mut *app.lock().await,
      IoEvent::QobuzLogin,
      epoch,
      || async { panic!("cached constants") }
    ));
  }

  #[tokio::test]
  async fn a_poisoned_resolver_reports_an_error_without_panicking() {
    let resolver = Arc::new(ConstantsResolver::default());
    let to_poison = Arc::clone(&resolver);
    assert!(std::thread::spawn(move || {
      let _guard = to_poison.state.lock().expect("unpoisoned state");
      panic!("injected resolver panic");
    })
    .join()
    .is_err());
    assert!(resolver
      .resolve_with(|| async { panic!("must not resolve") })
      .await
      .is_err());
    let (app, rx) = app_with_events();
    let mut guard = app.lock().await;
    assert!(resolver.defer_with(
      &app,
      &mut guard,
      IoEvent::QobuzLogin,
      auth::token_epoch(),
      || async { panic!("must not resolve") }
    ));
    assert!(guard
      .status_message()
      .is_some_and(|message| message.contains("Qobuz constants resolver lock poisoned")));
    assert!(guard.api_error().is_empty());
    assert!(rx.try_recv().is_err());
    assert!(resolver.next_pending().await.is_none());
  }

  #[cfg(feature = "streaming")]
  #[tokio::test]
  async fn music_owned_queue_requests_do_not_start_a_constants_lookup() {
    let (app, rx) = app_with_events();
    let mut guard = app.lock().await;
    guard.claim_apple_music();
    guard
      .native_queue
      .push(crate::core::test_helpers::queued_track(
        "qobuz:track:1",
        "Queued",
      ));
    assert!(!defer_constants(
      &app,
      &mut guard,
      &IoEvent::AdvanceNativeQueue
    ));
    assert_eq!(guard.status_message(), None);
    assert!(rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn the_first_parked_event_shows_preparation_without_replacing_later_status() {
    for started_off_pump in [false, true] {
      let resolver = Arc::new(ConstantsResolver::default());
      let (app, _rx) = app_with_events();
      let (finish, wait) = tokio::sync::oneshot::channel();
      let resolve = || async { wait.await.unwrap() };
      let epoch = auth::token_epoch();
      {
        let mut guard = app.lock().await;
        if started_off_pump {
          let _resolution = resolver.resolve_with(resolve);
          assert!(
            resolver.defer_with(&app, &mut guard, IoEvent::QobuzLogin, epoch, || async {
              panic!("the off-pump caller already started the scrape")
            })
          );
        } else {
          assert!(resolver.defer_with(&app, &mut guard, IoEvent::QobuzLogin, epoch, resolve));
        }
        assert_eq!(
          guard.status_message(),
          Some(guard.strings().status.infra.qobuz_preparing)
        );
        assert!(!guard.status_message_is_error());
        guard.set_status_message("another source is playing", 30);
        assert!(resolver.defer_with(
          &app,
          &mut guard,
          IoEvent::GetQobuzPlaylists,
          epoch,
          || async { panic!("later events must share the scrape") },
        ));
        assert_eq!(guard.status_message(), Some("another source is playing"));
      }
      finish.send(Ok(bundle_constants())).unwrap();
      resolution_finishes(&resolver).await;
      let mut guard = app.lock().await;
      assert!(
        !resolver.defer_with(&app, &mut guard, IoEvent::QobuzLogin, epoch, || async {
          panic!("ready constants must not be resolved again")
        })
      );
      assert_eq!(guard.status_message(), Some("another source is playing"));
    }
  }

  #[tokio::test]
  async fn waiting_for_constants_returns_before_the_network_and_allows_other_events() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (app, rx) = app_with_events();
    let (finish, wait) = tokio::sync::oneshot::channel();
    assert!(resolver.defer_with(
      &app,
      &mut *app.lock().await,
      IoEvent::QobuzLogin,
      auth::token_epoch(),
      || async { wait.await.unwrap() }
    ));
    // The resolver cannot finish until this pump-side call has returned.
    app.lock().await.dispatch(IoEvent::PausePlayback);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
    assert!(rx.try_recv().is_err());
    finish.send(Ok(bundle_constants())).unwrap();
    resolution_finishes(&resolver).await;
    assert!(matches!(rx.try_recv(), Ok(IoEvent::QobuzLogin)));
    assert!(rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn the_preparing_notice_clears_when_the_parked_event_replays() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (app, rx) = app_with_events();
    let (finish, wait) = tokio::sync::oneshot::channel();
    {
      let mut guard = app.lock().await;
      let epoch = auth::token_epoch();
      assert!(
        resolver.defer_with(&app, &mut guard, IoEvent::QobuzLogin, epoch, || async {
          wait.await.unwrap()
        })
      );
      assert_eq!(
        guard.status_message(),
        Some(guard.strings().status.infra.qobuz_preparing)
      );
    }
    finish.send(Ok(bundle_constants())).unwrap();
    resolution_finishes(&resolver).await;
    assert!(matches!(rx.try_recv(), Ok(IoEvent::QobuzLogin)));
    assert_eq!(app.lock().await.status_message(), None);
  }

  #[tokio::test]
  async fn one_constants_scrape_replays_all_waiting_actions_in_arrival_order() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (app, rx) = app_with_events();
    let (finish, wait) = tokio::sync::oneshot::channel();
    let calls = Arc::new(AtomicU64::new(0));
    let calls_for_resolve = Arc::clone(&calls);
    // An off-pump caller and pump events share the same in-flight result.
    let resolution = resolver.resolve_with(move || async move {
      calls_for_resolve.fetch_add(1, Ordering::Relaxed);
      wait.await.unwrap()
    });
    let epoch = auth::token_epoch();
    for event in [
      IoEvent::GetQobuzPlaylists,
      IoEvent::GetQobuzTracks("qobuz:album:1".into()),
      IoEvent::GetQobuzSearchResults("query".into()),
      IoEvent::StartPlayback(
        None,
        Some(vec!["qobuz:track:1".into(), "qobuz:track:2".into()]),
        Some(1),
      ),
      IoEvent::AdvanceNativeQueue,
    ] {
      assert!(
        resolver.defer_with(&app, &mut *app.lock().await, event, epoch, || async {
          panic!("a second waiter must not start another scrape")
        })
      );
    }
    finish.send(Ok(bundle_constants())).unwrap();
    assert_eq!(resolution.await.unwrap(), bundle_constants());
    resolution_finishes(&resolver).await;
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzPlaylists)));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzTracks(uri)) if uri == "qobuz:album:1"));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzSearchResults(query)) if query == "query"));
    assert!(
      matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(None, Some(uris), Some(1)))
      if uris == ["qobuz:track:1", "qobuz:track:2"])
    );
    assert!(matches!(rx.try_recv(), Ok(IoEvent::AdvanceNativeQueue)));
    assert!(rx.try_recv().is_err());
    assert!(!resolver.defer_with(
      &app,
      &mut *app.lock().await,
      IoEvent::QobuzLogin,
      epoch,
      || async { panic!("ready constants must not be resolved again") }
    ));
  }

  #[tokio::test]
  async fn a_failed_constants_scrape_reports_to_all_waiters_and_allows_a_retry() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (first, first_rx) = app_with_events();
    let (second, second_rx) = app_with_events();
    let (finish, wait) = tokio::sync::oneshot::channel();
    let epoch = auth::token_epoch();
    assert!(resolver.defer_with(
      &first,
      &mut *first.lock().await,
      IoEvent::QobuzLogin,
      epoch,
      || async { wait.await.unwrap() }
    ));
    assert!(resolver.defer_with(
      &second,
      &mut *second.lock().await,
      IoEvent::GetQobuzPlaylists,
      epoch,
      || async { panic!("a failed scrape is shared too") }
    ));
    finish
      .send(Err(anyhow!("injected scrape failure")))
      .unwrap();
    resolution_finishes(&resolver).await;
    for app in [&first, &second] {
      let app = app.lock().await;
      let strings = &app.strings().status.infra;
      let expected =
        strings.qobuz_operation_failed(strings.qobuz_step_constants, "injected scrape failure");
      assert_eq!(app.status_message(), Some(expected.as_str()));
      assert!(app.api_error().is_empty());
    }
    assert!(first_rx.try_recv().is_err());
    assert!(second_rx.try_recv().is_err());
    assert!(resolver.defer_with(
      &first,
      &mut *first.lock().await,
      IoEvent::QobuzLogin,
      epoch,
      || async { Ok(bundle_constants()) }
    ));
    resolution_finishes(&resolver).await;
    assert!(matches!(first_rx.try_recv(), Ok(IoEvent::QobuzLogin)));
    assert!(first_rx.try_recv().is_err());
  }

  #[tokio::test]
  async fn a_stale_constants_wait_neither_replays_an_action_nor_changes_status() {
    for result in [Ok(bundle_constants()), Err(anyhow!("stale failure"))] {
      let resolver = Arc::new(ConstantsResolver::default());
      let (app, rx) = app_with_events();
      app.lock().await.set_status_message("new login status", 30);
      let old_epoch = auth::token_epoch().wrapping_sub(1);
      assert!(resolver.defer_with(
        &app,
        &mut *app.lock().await,
        IoEvent::QobuzLogin,
        old_epoch,
        || async { result }
      ));
      resolution_finishes(&resolver).await;
      assert!(rx.try_recv().is_err());
      assert_eq!(app.lock().await.status_message(), Some("new login status"));
    }
  }

  #[tokio::test]
  async fn cancelling_a_constants_waiter_keeps_the_shared_scrape_running() {
    let resolver = Arc::new(ConstantsResolver::default());
    let (finish, wait) = tokio::sync::oneshot::channel();
    let resolution = resolver.resolve_with(|| async { wait.await.unwrap() });
    drop(resolution);
    finish.send(Ok(bundle_constants())).unwrap();
    resolution_finishes(&resolver).await;
    assert_eq!(
      resolver
        .resolve_with(|| async { panic!("the successful result must be cached") })
        .await
        .unwrap(),
      bundle_constants()
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
    report_locked(&mut app, QobuzStep::Stream, err);
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
