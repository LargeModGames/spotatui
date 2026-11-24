use crate::core::app::{self, App, NativeTrackKind};
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::IoEvent;
use crate::infra::player::{
  player_build_timeout, PlayerEvent, SessionDisconnectReason, StreamingBuildError, StreamingConfig,
  StreamingConnectionState, StreamingPlayer,
};
use log::{info, warn};
use std::sync::{
  atomic::{AtomicBool, AtomicU64, Ordering},
  Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const STALLED_PLAYBACK_RECOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const END_OF_TRACK_CONTINUATION_DELAY: Duration = Duration::from_millis(500);
const END_OF_TRACK_RECONNECT_TIMEOUT: Duration = Duration::from_secs(12);
/// Rebuild-retry backoff bounds (5 s doubling to the cap): the usual cause is
/// transient DNS loss after a laptop wake.
const RECOVERY_RETRY_BASE_DELAY: Duration = Duration::from_secs(5);
const RECOVERY_RETRY_MAX_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionDisconnectRecovery {
  Stop,
  RebuildIdle,
  RestorePlayback,
}

fn session_disconnect_recovery(reason: SessionDisconnectReason) -> SessionDisconnectRecovery {
  match reason {
    SessionDisconnectReason::ExternalDeviceHandoff => SessionDisconnectRecovery::RebuildIdle,
    SessionDisconnectReason::UnexpectedShutdown => SessionDisconnectRecovery::RestorePlayback,
    SessionDisconnectReason::LocalCommand => SessionDisconnectRecovery::Stop,
  }
}

#[derive(Clone, Default)]
pub struct StreamingRecoveryRequest {
  /// Identity of the rebuild request that owns this chain's writes.
  pub generation: u64,
  pub reselect_device: bool,
  pub restore_playback: bool,
  pub continue_after_track: Option<String>,
  /// A Spotify start rebuilds a parked backend: restore the snapshot after the
  /// rebuild, paused or playing as it asks.
  pub reacquire: bool,
}

/// Restore intent alone decides the replay: an idle-app queue never matches
/// the native context (`reselect_device` stays false), and a handoff teardown
/// must not replay at all (#437). A reacquire replays even behind a merged
/// handoff: a slot published while parked came after it.
fn should_replay_published_queue_slot(
  request: &StreamingRecoveryRequest,
  queue_now_is_spotify: bool,
) -> bool {
  (request.restore_playback || request.reacquire) && queue_now_is_spotify
}

fn recovery_needs_native_selection(
  request: &StreamingRecoveryRequest,
  replay_queue_slot: bool,
) -> bool {
  request.reselect_device || replay_queue_slot
}

/// Bundled context for player event handling tasks.
/// Groups all shared state and managers needed by event handlers.
pub struct PlayerEventContext {
  pub player: Arc<StreamingPlayer>,
  pub app: Arc<Mutex<App>>,
  pub shared_position: Arc<AtomicU64>,
  pub shared_is_playing: Arc<AtomicBool>,
  pub recovery_tx: tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  pub windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
}

// Only a frontend launch starts the supervisor (`runtime/startup.rs`).
#[cfg_attr(not(any(feature = "tui", feature = "gui")), allow(dead_code))]
pub struct StreamingRecoveryContext {
  pub app: Arc<Mutex<App>>,
  pub shared_position: Arc<AtomicU64>,
  pub shared_is_playing: Arc<AtomicBool>,
  pub recovery_rx: tokio::sync::mpsc::UnboundedReceiver<StreamingRecoveryRequest>,
  pub recovery_tx: tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  /// The player settings; the volume is read again at each rebuild.
  pub streaming_config: StreamingConfig,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  pub windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
}

#[cfg_attr(not(any(feature = "tui", feature = "gui")), allow(dead_code))]
pub fn spawn_streaming_recovery_handler(ctx: StreamingRecoveryContext) {
  tokio::spawn(async move {
    handle_streaming_recovery(ctx, LibrespotBackend).await;
  });
}

/// Compact recovery-snapshot summary for the log: enough to tell which restore
/// branch should have run and with what track/position, without dumping a whole
/// context's URI list.
fn recovery_snapshot_summary(app: &App) -> String {
  match app.native_playback_recovery.as_ref() {
    None => "snapshot=none".to_string(),
    Some(s) => format!(
      "snapshot gen={} current={:?} loading={:?} pos={} dur={:?} playing={} ctx={:?} uris={} offset={:?} attempts={}",
      s.generation,
      s.current_track_uri,
      s.loading_track_uri,
      s.position_ms,
      s.track_duration_ms,
      s.desired_playing,
      s.context_uri,
      s.uris.as_ref().map_or(0, |uris| uris.len()),
      s.offset,
      s.recovery_attempts,
    ),
  }
}

/// Intent ORs across coalesced transport-drop requests, but a handoff
/// (`restore_playback == false`) stickily vetoes restore/reselect: restoring
/// over the user's handoff would steal playback back (#437).
fn merge_recovery_requests(request: &mut StreamingRecoveryRequest, next: StreamingRecoveryRequest) {
  // The newest generation owns the identity; intent still merges.
  request.generation = request.generation.max(next.generation);
  request.reacquire |= next.reacquire;
  if !request.restore_playback || !next.restore_playback {
    request.reselect_device = false;
    request.restore_playback = false;
    request.continue_after_track = None;
    return;
  }
  request.reselect_device |= next.reselect_device;
  if next.continue_after_track.is_some() {
    request.continue_after_track = next.continue_after_track;
  }
}

fn next_recovery_retry_delay(current: Duration) -> Duration {
  current.saturating_mul(2).min(RECOVERY_RETRY_MAX_DELAY)
}

/// One rebuild attempt.
type BuildAttempt<P> = std::pin::Pin<
  Box<dyn std::future::Future<Output = std::result::Result<P, StreamingBuildError>> + Send>,
>;

/// The result of one rebuild chain
enum RebuildOutcome<P> {
  /// A live player
  Built(P),
  /// An available player was published while the chain waited.
  Reused,
  /// A newer rebuild request superseded this chain.
  Superseded,
  /// An error we give up on
  Failed(StreamingBuildError),
  /// A decoded owner interrupted the rebuild.
  DecodedOwner,
  /// The frontend exited or another engine took over.
  Cancelled,
}

/// Who else wants the audio sink, and so must stop a rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RebuildCancellation {
  DecodedOwner,
  /// The frontend exited, another engine was selected, or Music took the sink.
  Cancelled,
}

impl RebuildCancellation {
  fn outcome<P>(self) -> RebuildOutcome<P> {
    match self {
      Self::DecodedOwner => RebuildOutcome::DecodedOwner,
      Self::Cancelled => RebuildOutcome::Cancelled,
    }
  }
}

/// Why a rebuild is no longer useful: frontend gone, another engine, or a decoded owner.
fn rebuild_cancellation(app: &App) -> Option<RebuildCancellation> {
  if app.io_tx_clone().is_none()
    || !app.native_engine_still_selected()
    || app.apple_music_owns_playback()
  {
    Some(RebuildCancellation::Cancelled)
  } else if app.active_decoded_source() {
    Some(RebuildCancellation::DecodedOwner)
  } else {
    None
  }
}

/// Finish a cancelled librespot recovery without clearing Soloist's startup flag.
fn finish_cancelled_recovery(app: &mut App, interrupted_by_decoded: bool) {
  if !app.native_engine_still_selected() {
    return;
  }
  app.native_backend_pending = false;
  if app.io_tx_clone().is_none() {
    return;
  }
  if app.apple_music_owns_playback() {
    app.park_cancelled_native_recovery();
  } else if interrupted_by_decoded {
    app.park_cancelled_native_recovery();
    if !app.active_decoded_source() {
      app.reacquire_parked_backend();
    }
  }
}

/// Build a replacement Connect device, retrying transient failures with backoff.
/// A new recovery request restarts the chain with a fresh delay and merged intent.
/// `chain_started` is when the chain began; its elapsed time is credited back to
/// a parked start before every replay, so retrying does not expire the request.
async fn rebuild_with_retries<P, B, A>(
  ctx: &mut StreamingRecoveryContext,
  request: &mut StreamingRecoveryRequest,
  base_delay: Duration,
  chain_started: Instant,
  build: B,
  active_player: A,
) -> RebuildOutcome<P>
where
  P: Send + 'static,
  B: Fn(StreamingConfig) -> BuildAttempt<P>,
  A: Fn(&App) -> Option<Arc<P>>,
{
  let mut attempt = 1_u32;
  let mut delay = base_delay;

  loop {
    // Cancel if not native
    let mut app = ctx.app.lock().await;
    if app.native_recovery_generation() != request.generation {
      return RebuildOutcome::Superseded;
    }
    if let Some(cancellation) = rebuild_cancellation(&app) {
      return cancellation.outcome();
    }
    // Startup can publish a player while the chain sleeps; replay over it,
    // crediting the retry time first so a start parked before it is not expired.
    if active_player(&app).is_some() {
      app.exclude_recovery_time_from_parked_start(chain_started.elapsed());
      replay_recovery_over_live_player(&mut app, request, &ctx.streaming_config.device_name);
      return RebuildOutcome::Reused;
    }

    let initial_volume = app.runtime_state.volume_percent;
    drop(app);
    let streaming_config = StreamingConfig {
      initial_volume,
      ..ctx.streaming_config.clone()
    };
    info!("attempting native streaming recovery (attempt {attempt})");

    let build_task = tokio::spawn(build(streaming_config));
    let abort_handle = build_task.abort_handle();
    let build_timeout = player_build_timeout();
    let built = match tokio::time::timeout(build_timeout, build_task).await {
      Ok(Ok(result)) => result,
      Ok(Err(e)) => Err(StreamingBuildError::Setup(anyhow::anyhow!(
        "rebuild task failed: {e}"
      ))),
      Err(_) => {
        abort_handle.abort();
        Err(StreamingBuildError::Timeout {
          secs: build_timeout.as_secs(),
        })
      }
    };

    match built {
      Ok(player) => return RebuildOutcome::Built(player),
      Err(e) => {
        // Handle retry
        {
          let error = e.to_string();
          let mut app = ctx.app.lock().await;

          if app.native_recovery_generation() != request.generation {
            return RebuildOutcome::Superseded;
          }
          if let Some(cancellation) = rebuild_cancellation(&app) {
            return cancellation.outcome();
          }

          if !e.is_retryable() {
            return RebuildOutcome::Failed(e);
          }

          // A bare resume would otherwise reach Spotify with no
          // active device and surface a full-screen error.
          app.native_backend_pending = true;
          if attempt == 1 {
            let strings = app.strings();
            app.set_status_message(strings.status.app.native_reconnecting, 8);
          }
          info!(
            "native streaming recovery attempt {attempt} failed ({error}); retrying in {}s",
            delay.as_secs()
          );
        }

        // If nothing, proceed. If something, handle accordingly
        let next_request = tokio::select! {
          _ = tokio::time::sleep(delay) => None,
          next = ctx.recovery_rx.recv() => Some(next),
        };

        match next_request {
          None => {
            attempt = attempt.saturating_add(1);
            delay = next_recovery_retry_delay(delay);
          }
          Some(Some(next)) => {
            // Merge if an incoming one exists
            merge_recovery_requests(request, next);
            while let Ok(more) = ctx.recovery_rx.try_recv() {
              merge_recovery_requests(request, more);
            }
            info!("native streaming recovery superseded by a new request; restarting retries");
            attempt = 1;
            delay = base_delay;
          }
          Some(None) => {
            // The recovery channel closed
            return RebuildOutcome::Cancelled;
          }
        }
      }
    }
  }
}

/// Replay a recovery request's intent: parked start, queue slot, then snapshot.
/// `rebuild_device` is reselected first, yielding to playback that moved away.
fn replay_recovery_intent(
  app: &mut App,
  request: &StreamingRecoveryRequest,
  rebuild_device: Option<&str>,
  replay_queue_slot: bool,
) {
  let after_rebuild = rebuild_device.is_some();
  if let Some(device_name) = rebuild_device {
    if app.native_should_drive() && recovery_needs_native_selection(request, replay_queue_slot) {
      app.dispatch(IoEvent::AutoSelectStreamingDevice(
        device_name.to_string(),
        false,
        true,
      ));
    }
  }
  // A new explicit request wins; both paths queue after device selection.
  if app.pending_start_playback.is_some() {
    info!("recovery route: replay parked StartPlayback");
    app.replay_pending_start_playback();
  } else if replay_queue_slot {
    // The queue slot outranks the snapshot: its direct load was discarded with
    // the replaced player, and a queue over an idle app has no TrackChanged snapshot.
    info!("recovery route: ReplayPublishedSpotifyQueueSlot");
    app.dispatch(IoEvent::ReplayPublishedSpotifyQueueSlot);
  } else if !app.native_context_should_drive() {
    info!("recovery route: none - another player owns the sink");
  } else if let Some(previous_track_id) = request.continue_after_track.as_deref() {
    if app.native_transition_has_advanced(previous_track_id) {
      if let Some(generation) = app.native_playback_restore_generation() {
        info!("recovery route: RestoreNativePlayback({generation}) (transition advanced)");
        app.dispatch(IoEvent::RestoreNativePlayback(generation));
      } else {
        warn!("recovery route: none - transition advanced but the snapshot is gone");
      }
    } else {
      info!("recovery route: EnsurePlaybackContinues({previous_track_id})");
      app.dispatch(IoEvent::EnsurePlaybackContinues(
        previous_track_id.to_string(),
      ));
    }
  } else if request.reselect_device || (after_rebuild && request.reacquire) {
    if let Some(generation) = app.native_playback_restore_generation() {
      info!("recovery route: RestoreNativePlayback({generation}) (reselect or reacquire)");
      app.dispatch(IoEvent::RestoreNativePlayback(generation));
    } else {
      warn!("recovery route: none - restore requested but the snapshot is gone");
      if after_rebuild {
        let strings = app.strings();
        app.set_status_message(strings.status.infra.native_recovered, 6);
      }
    }
  } else if after_rebuild && request.restore_playback {
    // A handoff rebuild instead keeps the teardown's "moved to another
    // device" message.
    let strings = app.strings();
    app.set_status_message(strings.status.infra.native_recovered, 6);
  } else {
    info!("recovery route: none - no restore intent on the request");
  }
}

/// Replay a request over a live player; the caller holds the lock and has checked ownership.
fn replay_recovery_over_live_player(
  app: &mut App,
  request: &StreamingRecoveryRequest,
  device_name: &str,
) {
  app.native_backend_pending = false;
  info!(
    "recovery over a live player: reselect={} continue_after={:?} {}",
    request.reselect_device,
    request.continue_after_track,
    recovery_snapshot_summary(app)
  );
  // A live player gets the reselect, but not a queue-slot replay: the
  // retained-slot replay over a live player is a separate path.
  replay_recovery_intent(app, request, Some(device_name), false);
}

/// The player-specific half of a recovery chain: what a rebuild attempts and
/// what the app publishes.
trait RecoveryBackend: Send + Sync {
  type Player: Send + 'static;

  /// One rebuild attempt.
  fn build(&self, config: StreamingConfig) -> BuildAttempt<Self::Player>;
  /// The player the app publishes, under a held app lock.
  fn published_player(&self, app: &App) -> Option<Arc<Self::Player>>;
  /// Whether a player can take commands now.
  fn is_available(player: &Self::Player) -> bool;
  /// Publish a rebuilt player under the app lock, stopping the one it replaces.
  fn publish(
    &self,
    app: &mut App,
    ctx: &StreamingRecoveryContext,
    player: Arc<Self::Player>,
    reacquired: bool,
  );
  /// Start delivering a published player's events into the app.
  fn watch(&self, ctx: &StreamingRecoveryContext, player: Arc<Self::Player>);
  /// Stop a built player that will not be published.
  fn discard(&self, player: Arc<Self::Player>);

  /// The published player, when it can take commands.
  fn available_published_player(&self, app: &App) -> Option<Arc<Self::Player>> {
    self
      .published_player(app)
      .filter(|player| Self::is_available(player))
  }
}

/// The native backend a frontend's recovery handler rebuilds.
struct LibrespotBackend;

impl RecoveryBackend for LibrespotBackend {
  type Player = StreamingPlayer;

  fn build(&self, config: StreamingConfig) -> BuildAttempt<Self::Player> {
    Box::pin(StreamingPlayer::new_cache_only(config))
  }

  fn published_player(&self, app: &App) -> Option<Arc<Self::Player>> {
    app.streaming_player.clone()
  }

  fn is_available(player: &Self::Player) -> bool {
    player.is_available()
  }

  fn publish(
    &self,
    app: &mut App,
    ctx: &StreamingRecoveryContext,
    player: Arc<Self::Player>,
    reacquired: bool,
  ) {
    // A disconnected old player may still be referenced here; shut its
    // spirc down before replacing it so it can't leave a ghost device (#297).
    if let Some(old) = app.streaming_player.take() {
      if !Arc::ptr_eq(&old, &player) {
        old.shutdown();
      }
    }
    // The parked player's last events never reached the shared flag.
    if reacquired {
      ctx.shared_is_playing.store(false, Ordering::Relaxed);
    }
    app.streaming_player = Some(player);
  }

  fn watch(&self, ctx: &StreamingRecoveryContext, player: Arc<Self::Player>) {
    spawn_player_event_handler(PlayerEventContext {
      player,
      app: Arc::clone(&ctx.app),
      shared_position: Arc::clone(&ctx.shared_position),
      shared_is_playing: Arc::clone(&ctx.shared_is_playing),
      recovery_tx: ctx.recovery_tx.clone(),
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: ctx.mpris_manager.clone(),
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      macos_media_manager: ctx.macos_media_manager.clone(),
      #[cfg(all(feature = "windows-media", target_os = "windows"))]
      windows_media_manager: ctx.windows_media_manager.clone(),
    });
  }

  fn discard(&self, player: Arc<Self::Player>) {
    player.shutdown();
    tokio::task::spawn_blocking(move || drop(player));
  }
}

async fn handle_streaming_recovery<B: RecoveryBackend>(
  mut ctx: StreamingRecoveryContext,
  backend: B,
) {
  while let Some(mut request) = ctx.recovery_rx.recv().await {
    while let Ok(next_request) = ctx.recovery_rx.try_recv() {
      merge_recovery_requests(&mut request, next_request);
    }

    let live_player = {
      let app = ctx.app.lock().await;
      backend.available_published_player(&app)
    };
    if let Some(live_player) = live_player {
      // A live player already exists (e.g. a queued duplicate request): the
      // pending window is over, so replay anything parked against it.
      let mut app = ctx.app.lock().await;
      // Between the two locks the player can be replaced and this request
      // superseded: only the owner of both may replay.
      if !request_may_replay_over(
        app.native_recovery_generation(),
        request.generation,
        backend.published_player(&app).as_ref(),
        &live_player,
      ) {
        info!("recovery over a superseded player or a stale request: skipping the replay");
        continue;
      }
      replay_recovery_over_live_player(&mut app, &request, &ctx.streaming_config.device_name);
      continue;
    }

    let recovery_started = Instant::now();
    let outcome = rebuild_with_retries(
      &mut ctx,
      &mut request,
      RECOVERY_RETRY_BASE_DELAY,
      recovery_started,
      |config| backend.build(config),
      |app| backend.available_published_player(app),
    )
    .await;
    // The retry chain can outlast a single player build. Not user's wait, so credit it
    // back to any start parked for this backend before replaying it on the built route.
    {
      let mut app = ctx.app.lock().await;
      if app.native_recovery_generation() == request.generation {
        app.exclude_recovery_time_from_parked_start(recovery_started.elapsed());
      }
    }
    match outcome {
      RebuildOutcome::Built(built) => {
        let recovered_player = Arc::new(built);
        let reacquired = {
          let mut app = ctx.app.lock().await;
          if app.native_recovery_generation() != request.generation {
            drop(app);
            backend.discard(Arc::clone(&recovered_player));
            continue;
          }
          // A player published during the build wins; discard this build.
          if backend.available_published_player(&app).is_some() {
            replay_recovery_over_live_player(&mut app, &request, &ctx.streaming_config.device_name);
            drop(app);
            info!("native rebuild not installed: a live player was published during the build");
            backend.discard(Arc::clone(&recovered_player));
            continue;
          }
          let reacquired = app.native_backend_parked();
          // The frontend is exiting, or a park gave the sink to another
          // source during the build: nothing would drive this player.
          if app.io_tx_clone().is_none() || !app.accept_rebuilt_native_backend() {
            if app.native_engine_still_selected() {
              app.native_backend_pending = false;
            }
            drop(app);
            info!("native rebuild not installed: exiting, or another source owns the sink");
            backend.discard(Arc::clone(&recovered_player));
            continue;
          }
          backend.publish(&mut app, &ctx, Arc::clone(&recovered_player), reacquired);
          app.native_backend_pending = false;
          reacquired
        };

        backend.watch(&ctx, Arc::clone(&recovered_player));

        let mut app = ctx.app.lock().await;
        // The install lock was released: replay only while this request still
        // owns the identity and its player is still published.
        if !request_may_replay_over(
          app.native_recovery_generation(),
          request.generation,
          backend.published_player(&app).as_ref(),
          &recovered_player,
        ) {
          drop(app);
          info!("recovery superseded after the rebuild: skipping the replay");
          backend.discard(recovered_player);
          continue;
        }
        // Queue replay over an otherwise idle app also needs the serialized
        // ownership check. Auto-selection can then clear the slot before the
        // replay event if another device became active during the rebuild.
        let replay_queue_slot = app.pending_start_playback.is_none()
          && should_replay_published_queue_slot(&request, app.queue_now_is_spotify());
        info!(
          "recovery rebuilt the backend: restore={} reselect={} continue_after={:?} replay_queue_slot={} {}",
          request.restore_playback,
          request.reselect_device,
          request.continue_after_track,
          replay_queue_slot,
          recovery_snapshot_summary(&app)
        );
        replay_recovery_intent(
          &mut app,
          &request,
          Some(&ctx.streaming_config.device_name),
          replay_queue_slot,
        );
        // A playlist refresh while parked had no session for the folders.
        if reacquired && app.playlist_folder_nodes().is_none() {
          app.dispatch(IoEvent::GetPlaylists);
        }
      }
      RebuildOutcome::Reused => {
        info!("native streaming recovery reused a player published during retries");
      }
      RebuildOutcome::Superseded => {
        info!("native streaming recovery superseded by a newer rebuild request");
      }
      RebuildOutcome::DecodedOwner | RebuildOutcome::Cancelled => {
        info!(
          "native streaming recovery cancelled: another owner took the sink or the frontend exited"
        );
        let mut app = ctx.app.lock().await;
        if app.native_recovery_generation() == request.generation {
          finish_cancelled_recovery(&mut app, matches!(outcome, RebuildOutcome::DecodedOwner));
        }
      }
      RebuildOutcome::Failed(e) => {
        info!("native streaming recovery failed: {}", e);
        let mut app = ctx.app.lock().await;
        if app.native_recovery_generation() != request.generation
          || !app.native_engine_still_selected()
        {
          continue;
        }
        app.native_backend_pending = false;
        app.native_restore_pending = None;
        app.native_load_watchdog = None;
        // A published Spotify slot waits on this rebuild: show it paused. Space
        // retries a parked backend.
        if app.queue_now_is_spotify() {
          app.native_is_playing = Some(false);
        }
        if app.pending_start_playback.take().is_some() {
          let strings = app.strings();
          app.set_status_message(
            strings.status.infra.native_recovery_dropped(&e.to_string()),
            8,
          );
        } else if app.native_should_drive() {
          let strings = app.strings();
          app.set_status_message(
            strings.status.infra.native_recovery_failed(&e.to_string()),
            8,
          );
        }
      }
    }
  }
}

/// Get the currently active streaming player (if any).
#[cfg_attr(
  not(any(
    all(feature = "macos-media", target_os = "macos"),
    all(feature = "windows-media", target_os = "windows")
  )),
  allow(dead_code)
)]
pub async fn active_streaming_player(app: &Arc<Mutex<App>>) -> Option<Arc<StreamingPlayer>> {
  let app = app.lock().await;
  LibrespotBackend.available_published_player(&app)
}

pub fn spawn_player_event_handler(ctx: PlayerEventContext) {
  let event_rx = ctx.player.get_event_channel();
  info!("spawning native player event handler");

  let player = ctx.player.clone();
  let app = Arc::clone(&ctx.app);
  let shared_position = Arc::clone(&ctx.shared_position);
  let shared_is_playing = Arc::clone(&ctx.shared_is_playing);
  let recovery_tx = ctx.recovery_tx.clone();
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mpris_manager = ctx.mpris_manager.clone();
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  let macos_media_manager = ctx.macos_media_manager.clone();
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  let windows_media_manager = ctx.windows_media_manager.clone();

  tokio::spawn(async move {
    handle_player_events(
      event_rx,
      player,
      app,
      shared_position,
      shared_is_playing,
      recovery_tx,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager,
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      macos_media_manager,
      #[cfg(all(feature = "windows-media", target_os = "windows"))]
      windows_media_manager,
    )
    .await;
  });
}

/// Handle player events from librespot and update app state directly.
/// This bypasses the Spotify Web API for instant UI updates.
async fn handle_player_events(
  mut event_rx: librespot_playback::player::PlayerEventChannel,
  player: Arc<StreamingPlayer>,
  app: Arc<Mutex<App>>,
  shared_position: Arc<AtomicU64>,
  shared_is_playing: Arc<AtomicBool>,
  recovery_tx: tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  #[cfg(all(feature = "mpris", target_os = "linux"))] mpris_manager: Option<
    Arc<mpris::MprisManager>,
  >,
  #[cfg(all(feature = "macos-media", target_os = "macos"))] macos_media_manager: Option<
    Arc<macos_media::MacMediaManager>,
  >,
  #[cfg(all(feature = "windows-media", target_os = "windows"))] windows_media_manager: Option<
    Arc<smtc_tokio::WindowsMediaManager>,
  >,
) {
  use chrono::TimeDelta;

  // Count consecutive failed (Unavailable) loads so we can escalate the message
  // when an account is hit by the upstream Spotify audio-key block (#282). A
  // single genuinely-unavailable track only trips the mild message and resets on
  // the next successful Playing.
  let mut consecutive_unavailable: u32 = 0;
  const UNAVAILABLE_ESCALATION_THRESHOLD: u32 = 3;
  let mut loading_track: Option<String> = None;

  // The StreamingPlayer first reconnects its Session/Spirc in place, retaining
  // the Player and buffered audio. Only replace the whole backend when that
  // attempt fails or desired playback stops making progress.
  let mut connection_state_rx = player.connection_state_receiver();
  let mut session_lost = false;
  let mut fast_reconnect_failed = false;
  let mut progress_watchdog_armed = false;
  let mut transport_recovery_pending = false;
  let mut pending_end_of_track = None;
  let mut observed_connection_generation = 0_u64;
  let mut audibly_playing = shared_is_playing.load(Ordering::Relaxed);
  let mut last_position = shared_position.load(Ordering::Relaxed);
  let mut last_progress_at = Instant::now();
  let mut watchdog = tokio::time::interval(Duration::from_secs(1));
  watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let playback_transition_generation = Arc::new(AtomicU64::new(0));

  loop {
    let event = tokio::select! {
      maybe_event = event_rx.recv() => match maybe_event {
        Some(event) => event,
        None => break,
      },
      changed = connection_state_rx.changed() => {
        if changed.is_err() {
          return;
        }
        let connection_state = *connection_state_rx.borrow_and_update();
        match connection_state {
          StreamingConnectionState::Reconnecting { generation } => {
            observed_connection_generation = generation;
            session_lost = true;
            fast_reconnect_failed = false;
            progress_watchdog_armed = true;
            transport_recovery_pending = true;
            last_progress_at = Instant::now();
            info!("native streaming fast reconnect generation {} started", generation);
            let mut app = app.lock().await;
            // A parked or replaced player must not latch the pending flag.
            if app
              .streaming_player
              .as_ref()
              .is_some_and(|current| Arc::ptr_eq(current, &player))
            {
              app.native_backend_pending = true;
              let strings = app.strings();
              app.set_status_message(strings.status.infra.native_lost_reconnecting, 6);
            }
          }
          StreamingConnectionState::Connected { generation }
            if session_lost || generation != observed_connection_generation =>
          {
            observed_connection_generation = generation;
            session_lost = false;
            fast_reconnect_failed = false;
            progress_watchdog_armed = true;
            transport_recovery_pending = true;
            last_progress_at = Instant::now();
            info!("native streaming fast reconnect generation {} completed", generation);
            let mut app = app.lock().await;
            if app
              .streaming_player
              .as_ref()
              .is_some_and(|current| Arc::ptr_eq(current, &player))
            {
              app.native_backend_pending = false;
              if app.native_load_watchdog.is_some() {
                app.native_load_watchdog = Some(Instant::now());
              }
              let strings = app.strings();
              app.set_status_message(strings.status.infra.native_reconnected, 5);
            }
          }
          StreamingConnectionState::Connected { .. } => {}
          StreamingConnectionState::Failed { generation } => {
            session_lost = true;
            fast_reconnect_failed = true;
            progress_watchdog_armed = true;
            transport_recovery_pending = true;
            info!("native streaming fast reconnect generation {} failed", generation);
            if audibly_playing {
              continue;
            }
            request_full_streaming_recovery(
              &app,
              &player,
              &shared_position,
              &shared_is_playing,
              &recovery_tx,
              crate::infra::app_strings(&app).await.status.infra.native_lost_recovering,
              pending_end_of_track.clone(),
            )
            .await;
            return;
          }
          StreamingConnectionState::Shutdown => return,
        }
        continue;
      }
      _ = watchdog.tick(), if progress_watchdog_armed => {
        let position = shared_position.load(Ordering::Relaxed);
        if position != last_position {
          last_position = position;
          last_progress_at = Instant::now();
        }
        let (desired_playing, should_drive) = {
          let app = app.lock().await;
          (
            app
              .native_playback_recovery
              .as_ref()
              .map_or_else(
                || shared_is_playing.load(Ordering::Relaxed),
                |snapshot| snapshot.desired_playing,
              ),
            app.native_should_drive(),
          )
        };
        if !should_drive || (!desired_playing && !session_lost) {
          progress_watchdog_armed = false;
          transport_recovery_pending = false;
          continue;
        }
        if recovery_watchdog_should_escalate(desired_playing, last_progress_at.elapsed()) {
          info!("native playback made no progress for {}s during session recovery; replacing backend", STALLED_PLAYBACK_RECOVERY_TIMEOUT.as_secs());
          request_full_streaming_recovery(
            &app,
            &player,
            &shared_position,
            &shared_is_playing,
            &recovery_tx,
            crate::infra::app_strings(&app).await.status.infra.native_stalled,
            pending_end_of_track.clone(),
          )
          .await;
          return;
        }
        continue;
      }
    };

    if !is_current_streaming_player(&app, &player).await {
      continue;
    }

    match &event {
      PlayerEvent::Playing { .. } => {
        audibly_playing = true;
        player.cancel_play_after_transfer();
        pending_end_of_track = None;
        last_progress_at = Instant::now();
        if !session_lost {
          progress_watchdog_armed = false;
          transport_recovery_pending = false;
        }
      }
      PlayerEvent::Stopped { .. } => {
        audibly_playing = false;
        if stop_disarms_watchdog(session_lost) {
          progress_watchdog_armed = false;
          transport_recovery_pending = false;
        }
      }
      PlayerEvent::Paused { .. } | PlayerEvent::EndOfTrack { .. } => {
        audibly_playing = false;
      }
      PlayerEvent::Seeked { .. }
      | PlayerEvent::TrackChanged { .. }
      | PlayerEvent::Loading { .. } => {
        last_progress_at = Instant::now();
      }
      PlayerEvent::PositionChanged { position_ms, .. } => {
        let position = u64::from(*position_ms);
        if position != last_position {
          last_position = position;
          pending_end_of_track = None;
          last_progress_at = Instant::now();
          if !session_lost {
            progress_watchdog_armed = false;
            transport_recovery_pending = false;
          }
        }
      }
      _ => {}
    }

    // Music owns the output: a librespot that starts playing anyway (a late
    // load, a remote Connect command) is paused before anyone hears both,
    // and its playback-state events must not repaint Music's playbar. Session
    // lifecycle events still reach their handlers below: a disconnect has to
    // clean up whoever owns playback.
    if app.lock().await.apple_music_owns_playback()
      && matches!(
        event,
        PlayerEvent::Playing { .. }
          | PlayerEvent::Paused { .. }
          | PlayerEvent::Loading { .. }
          | PlayerEvent::Preloading { .. }
          | PlayerEvent::PositionChanged { .. }
          | PlayerEvent::Seeked { .. }
          | PlayerEvent::TrackChanged { .. }
          | PlayerEvent::VolumeChanged { .. }
          | PlayerEvent::EndOfTrack { .. }
          | PlayerEvent::Stopped { .. }
          | PlayerEvent::Unavailable { .. }
      )
    {
      if matches!(event, PlayerEvent::Playing { .. }) {
        player.pause();
        app.lock().await.set_native_playback_intent(false);
      }
      continue;
    }

    let notes_playback = !matches!(
      event,
      PlayerEvent::PositionChanged { .. } | PlayerEvent::Preloading { .. }
    );
    match event {
      PlayerEvent::Playing {
        play_request_id: _,
        track_id,
        position_ms,
      } => {
        // While a decoded source or a *decoded* queue slot owns the sink, or the
        // native queue is mid-handoff, librespot must stay paused. The handoff
        // pauses Spirc, but a
        // self-advance load (or a stale-slot reissue) already in flight at that
        // moment can complete afterwards and start audio over the queue slot —
        // re-pause instead of accepting the state update. Librespot playing is
        // legitimate here only when the slot itself is a Spotify track; with a
        // decoded slot, or with a context suspended under the queue and no
        // Spotify slot (the between-items window: the old slot is cleared, the
        // next one not yet published), it never is. One-shot: a paused Spirc
        // emits no further Playing events, so this can't ping-pong.
        {
          let stray_over_owner = app.lock().await.librespot_playing_is_stray();
          if stray_over_owner {
            player.pause();
            app.lock().await.set_native_playback_intent(false);
            continue;
          }
        }

        // Playback is actually working: reset the failure streak.
        consecutive_unavailable = 0;
        shared_is_playing.store(true, Ordering::Relaxed);
        let track_uri = track_id.to_string();

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(true);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_playback_status(true);
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_playback_status(true);
        }

        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(true);
          app_lock.clear_native_unavailable_halt();
          app_lock.observe_native_position(position_ms, true);
          // A real Playing event proves the session is alive: disarm the load
          // watchdog and drop any request parked for its potential recovery.
          let restore_confirmed =
            app_lock.observe_native_playback_state(track_uri.clone(), position_ms, true);
          if app_lock.native_restore_pending.is_none() {
            app_lock.native_load_watchdog = None;
          }
          app_lock.pending_start_playback = None;
          app_lock.native_backend_pending = false;
          if restore_confirmed {
            {
              let strings = app_lock.strings();
              app_lock.set_status_message(strings.status.infra.native_restored, 5)
            };
          }
        }

        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = true;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }

          app.instant_since_last_current_playback_poll = std::time::Instant::now();

          let track_id_str = app::base62_id_of(&track_uri).to_string();
          if app.last_track_id.as_ref() != Some(&track_id_str) {
            app.last_track_id = Some(track_id_str);
            app.dispatch(IoEvent::GetCurrentPlayback);
          }
          if app.pending_stop_after_track {
            app.pending_stop_after_track = false;
            if let Some(ref mut ctx) = app.current_playback_context {
              ctx.is_playing = false;
            }
            app.dispatch(IoEvent::PausePlayback);
          }
        }
      }
      PlayerEvent::Paused {
        play_request_id: _,
        track_id,
        position_ms,
      } => {
        if player.take_play_after_transfer() {
          player.play();
          continue;
        }
        shared_is_playing.store(false, Ordering::Relaxed);
        let track_uri = track_id.to_string();

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(false);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_playback_status(false);
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_playback_status(false);
        }

        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(false);
          app_lock.observe_native_position(position_ms, false);
          let restore_confirmed =
            app_lock.observe_native_playback_state(track_uri, position_ms, false);
          if app_lock.native_restore_pending.is_none() {
            app_lock.native_load_watchdog = None;
          }
          if restore_confirmed {
            {
              let strings = app_lock.strings();
              app_lock.set_status_message(strings.status.infra.native_restored_paused, 5)
            };
          }
        }

        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::Seeked {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_position(position_ms as u64);
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_position(position_ms as u64);
        }

        {
          let mut app = app.lock().await;
          app.seek_ms = None;
          // A newer local seek already moved the clock; a Seeked for an older target must not pull it back.
          if !app.native_seek_settling() {
            app.song_progress_ms = position_ms as u128;
            app.set_native_recovery_position(position_ms);
            app.jump_native_position(position_ms);
          }

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::TrackChanged { audio_item } => {
        playback_transition_generation.fetch_add(1, Ordering::Relaxed);
        use librespot_metadata::audio::UniqueFields;

        let (artists, album, kind) = match &audio_item.unique_fields {
          UniqueFields::Track { artists, album, .. } => {
            let artist_names: Vec<String> = artists.0.iter().map(|a| a.name.clone()).collect();
            (artist_names, album.clone(), NativeTrackKind::Track)
          }
          UniqueFields::Episode { show_name, .. } => (
            vec![show_name.clone()],
            String::new(),
            NativeTrackKind::Episode,
          ),
          UniqueFields::Local { artists, album, .. } => {
            let artist_vec = artists
              .as_ref()
              .map(|a| vec![a.clone()])
              .unwrap_or_default();
            let album_str = album.clone().unwrap_or_default();
            (artist_vec, album_str, NativeTrackKind::Track)
          }
        };

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms,
            None,
          );
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms,
            None,
          );
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms as u64,
            None,
          );
        }

        let mut app = app.lock().await;
        // A TrackChanged proves the session is processing loads: disarm the
        // zombie-session watchdog.
        if app.native_restore_pending.is_none() {
          app.native_load_watchdog = None;
        }
        app.pending_start_playback = None;
        app.native_backend_pending = false;
        app.native_track_info = Some(app::NativeTrackInfo {
          name: audio_item.name.clone(),
          artists: artists.clone(),
          album: album.clone(),
          duration_ms: audio_item.duration_ms,
          kind,
          // `covers` arrives widest-first: the pinned librespot fork sorts it
          // descending by width in `get_covers` (metadata/src/audio/item.rs)
          // before building the list, and the filter that follows preserves that
          // order, so `.first()` is the largest. Each url is already a fully
          // resolved `https://i.scdn.co/image/<file_id>` — the same CDN form the
          // Web API returns, so the cover-art key stays stable when the polled
          // context later catches up and takes over. The empty-string filter
          // matters: librespot substitutes into a session-supplied url template,
          // so a blank template yields `Some("")`, which would defeat the
          // fallback below instead of deferring to it. Local files legitimately
          // carry no covers at all (their art is embedded in the file).
          image_url: audio_item
            .covers
            .first()
            .map(|cover| cover.url.clone())
            .filter(|url| !url.is_empty()),
        });

        app.song_progress_ms = 0;
        app.anchor_native_position(0, false);
        // librespot 0.8's `SpotifyUri` Display is the full `spotify:track:<id>`
        // URI; app-side ids (shuffle index sync, the queue guard,
        // `last_track_id`) are bare base62, so normalize at the event boundary.
        // The restore observer keeps the full URI (it canonicalizes internally).
        let playing_uri = audio_item.track_id.to_string();
        app.observe_native_track_changed(playing_uri.clone(), kind, audio_item.duration_ms);
        let playing_id = app::base62_id_of(&playing_uri).to_string();
        app.last_track_id = Some(playing_id.clone());
        // Keep the client-side shuffle session pointed at what Spirc actually
        // plays (also detects a completed repeat-all lap for the reshuffle).
        app.sync_native_shuffle_index(&playing_id);
        app.instant_since_last_current_playback_poll = std::time::Instant::now();
        app.dispatch(IoEvent::GetCurrentPlayback);

        // Spirc self-advance guard: a queued Spotify track plays via a direct
        // `player.load` (no Spirc context), so Spirc can switch to the next
        // context track on its own. If that happened, reissue the queued track
        // (bounded). NOTE: pending the live experiment in the plan (Risk #1),
        // this mitigation is unverified without a real Spotify session.
        let reload_uri = app.spotify_queue_guard_reload_uri(&playing_id);
        let slot_desired_playing = app.queue_slot_desired_playing;
        let end_of_track_transition_advanced = pending_end_of_track
          .as_deref()
          .is_some_and(|previous| app.native_transition_has_advanced(previous));
        drop(app);
        if end_of_track_transition_advanced {
          pending_end_of_track = None;
        }
        if let Some(uri) = reload_uri {
          info!("spirc advanced off the queued track; reissuing {}", uri);
          reissue_published_queue_slot(&player, &uri, slot_desired_playing).await;
        }
      }
      PlayerEvent::Stopped { .. } => {
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_stopped();
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_stopped();
        }

        {
          let mut app = app.lock().await;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          // The next poll copies this flag over the API's play state, so a
          // stale `Some(true)` would report the stopped player as playing.
          app.native_is_playing = Some(false);
          app.song_progress_ms = 0;
          app.last_track_id = None;
          app.native_track_info = None;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if let Ok(mut app) = app.try_lock() {
          app.dispatch(IoEvent::GetCurrentPlayback);
        }
      }
      PlayerEvent::EndOfTrack { track_id, .. } => {
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_stopped();
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_stopped();
        }

        // Recovery and queue continuation compare against the bare Web API
        // id, while librespot formats this event as a full Spotify URI.
        let ended_uri = track_id.to_string();
        let previous_track_id = app::base62_id_of(&ended_uri).to_string();

        // Full `lock().await`, not `try_lock`: this arm decides whether the
        // native queue takes over, and a dropped decision here strands the
        // queue (nothing advances, nothing continues).
        let mut reissue_queue_slot: Option<(String, bool)> = None;
        let mut end_decision = "ensure-playback-continues";
        let should_ensure_playback = {
          let mut app = app.lock().await;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
          app.native_track_info = None;
          if app.user_config.behavior.stop_after_current_track {
            end_decision = "stop-after-current";
            app.pending_stop_after_track = true;
            app.set_native_playback_intent(false);
            false
          } else if let Some(uri) = app.spotify_queue_guard_reload_uri(&previous_track_id) {
            // The published Spotify queue slot is not the track that just
            // ended, so the slot never started (its direct load was deferred
            // during an in-place reconnect and possibly discarded). Reissue it
            // instead of letting the advance below consume the never-played
            // item; the guard's reload budget bounds the retries.
            end_decision = "reissue-queue-slot";
            reissue_queue_slot = Some((uri, app.queue_slot_desired_playing));
            false
          } else if app.handle_native_spotify_track_end() {
            end_decision = "native-queue-takeover";
            // The native queue takes priority: a queued Spotify track that just
            // ended advances the queue; a context track that ended while items
            // wait suspends the context (preempting Spirc's self-advance) and
            // hands off to the queue.
            false
          } else if app.native_raw_list_playback_exhausted(&previous_track_id) {
            end_decision = "raw-list-exhausted";
            // The raw URI list ran out with repeat off: stopping here is
            // legitimate, so record stopped intent instead of arming the
            // watchdog. This must happen under this lock, before the watchdog
            // could arm: the serial network pump that runs
            // `ensure_playback_continues` can lag past the stall window.
            app.set_native_playback_intent(false);
            false
          } else {
            // Neither the queue nor list exhaustion applies: fall back to the
            // normal continue-playback path.
            true
          }
        };
        info!("end of track {}: {}", previous_track_id, end_decision);

        if let Some((uri, desired_playing)) = reissue_queue_slot {
          info!(
            "queue slot did not start before the previous track ended; reissuing {}",
            uri
          );
          reissue_published_queue_slot(&player, &uri, desired_playing).await;
        }

        if should_ensure_playback {
          pending_end_of_track = Some(previous_track_id.clone());
          progress_watchdog_armed = true;
          last_progress_at = Instant::now();
          spawn_end_of_track_continuation(
            Arc::clone(&app),
            Arc::clone(&player),
            previous_track_id,
            Arc::clone(&playback_transition_generation),
          );
        }
      }
      PlayerEvent::VolumeChanged { volume } => {
        let volume_percent = ((volume as f64 / 65535.0) * 100.0).round() as u8;
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_volume(volume_percent);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_volume(volume_percent);
        }

        if let Ok(mut app) = app.try_lock() {
          if let Some(pending) = app.pending_volume {
            if volume_percent == pending {
              app.pending_volume = None;
              app.last_dispatched_volume = None;
            }
          } else {
            if let Some(ref mut ctx) = app.current_playback_context {
              ctx.device.volume_percent = Some(volume_percent as u32);
            }
            app.runtime_state.volume_percent = volume_percent.min(100);
            let _ = app.save_runtime_state(
              &crate::core::state::PersistedRuntimeState::volume_percent(volume_percent.min(100)),
            );
          }
        }
      }
      PlayerEvent::PositionChanged {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        shared_position.store(position_ms as u64, Ordering::Relaxed);
        {
          let mut app = app.lock().await;
          // A report decoded before a local seek must not pull the clock back.
          if !app.native_seek_settling() {
            app.set_native_recovery_position(position_ms);
            app.observe_native_position(position_ms, true);
          }
        }

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_position(position_ms as u64);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_position(position_ms as u64);
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_position(position_ms as u64);
        }
      }
      PlayerEvent::SessionDisconnected { reason, .. } => {
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_stopped();
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_stopped();
        }

        let recovery = session_disconnect_recovery(reason);
        let (status_message, restore_playback) = match recovery {
          SessionDisconnectRecovery::Stop => (
            crate::infra::app_strings(&app)
              .await
              .status
              .infra
              .native_stopped,
            false,
          ),
          SessionDisconnectRecovery::RebuildIdle => (
            crate::infra::app_strings(&app)
              .await
              .status
              .infra
              .native_moved,
            false,
          ),
          SessionDisconnectRecovery::RestorePlayback => (
            crate::infra::app_strings(&app)
              .await
              .status
              .infra
              .native_disconnected,
            true,
          ),
        };
        if let Some(request) = disconnect_streaming_player(
          &app,
          &player,
          &shared_position,
          &shared_is_playing,
          status_message,
          restore_playback,
        )
        .await
        {
          if recovery == SessionDisconnectRecovery::Stop {
            // No rebuild follows a Stop; release the pending latch so later
            // requests can't park forever. (Presently unreachable: local
            // shutdowns remove the player before this event, failing the
            // ptr_eq guard above.)
            app.lock().await.native_backend_pending = false;
          } else {
            if recovery == SessionDisconnectRecovery::RebuildIdle {
              app.lock().await.mark_native_handed_off();
            }
            let _ = recovery_tx.send(request);
          }
        }
        return;
      }
      PlayerEvent::Unavailable { track_id, .. } => {
        if unavailable_is_transport_related(transport_recovery_pending, player.connection_state()) {
          info!(
            "native playback unavailable for track {} while the session transport is down; deferring to connection recovery",
            track_id
          );
          continue;
        }
        if !unavailable_is_for_the_loading_track(loading_track.as_deref(), &track_id.to_string()) {
          info!(
            "native preload unavailable for track {}; the current track keeps playing",
            track_id
          );
          continue;
        }

        // librespot emits Unavailable when a track can't be loaded — including
        // when Spotify rejects the audio key (`error audio key 0 1`), which makes
        // decryption fail. This was previously dropped by the `_` arm, so the
        // failure was completely silent (#282). Surface it to the user.
        consecutive_unavailable += 1;
        last_progress_at = Instant::now();

        // Clear the ghost native track and the request/watchdog together so an
        // unavailable track cannot be replayed as a supposed session failure.
        {
          let mut app = app.lock().await;
          app.song_progress_ms = 0;
          app.native_track_info = None;
          app.native_load_watchdog = None;
          app.native_restore_pending = None;
          app.pending_start_playback = None;
          app.native_backend_pending = false;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
        }

        info!(
          "native playback unavailable (track {}, consecutive {})",
          track_id, consecutive_unavailable
        );

        // Once several consecutive tracks fail to load (genuinely unavailable,
        // e.g. region-locked or the audio-key block), halt playback instead of
        // letting librespot auto-skip through the entire queue at machine speed;
        // that stampede hammers Spotify and can get the account rate-limited.
        // A pause only makes Spirc's next load a paused one; a stop cancels that
        // load, and Spirc takes the Stopped event. A stop that reaches the
        // player before Spirc's next load retries on the next Unavailable.
        if consecutive_unavailable >= UNAVAILABLE_ESCALATION_THRESHOLD {
          player.stop();
          app.lock().await.halt_native_after_unavailable();
          progress_watchdog_armed = false;
          pending_end_of_track = None;
        }

        // Emit on the threshold transitions only (== not >=) so we don't spam the
        // same message on every auto-skip during an account-wide failure.
        if consecutive_unavailable == 1 {
          let mut app = app.lock().await;
          let strings = app.strings();
          app.set_status_message(strings.status.infra.native_skip_unavailable, 6);
        } else if consecutive_unavailable == UNAVAILABLE_ESCALATION_THRESHOLD {
          let mut app = app.lock().await;
          let strings = app.strings();
          app.set_status_message(strings.status.infra.native_many_unavailable, 20);
          app.offer_soloist_setup();
        }
      }
      PlayerEvent::Loading {
        track_id,
        position_ms,
        ..
      } => {
        loading_track = Some(track_id.to_string());
        let mut app = app.lock().await;
        app.observe_native_loading(track_id.to_string(), position_ms);
        if app.native_load_watchdog.is_some() {
          app.native_load_watchdog = Some(std::time::Instant::now());
        }
      }
      PlayerEvent::Preloading { .. } => {}
      _ => {}
    }

    if notes_playback {
      app.lock().await.note_display_changes();
    }

    // A failed in-place reconnect owns no viable Spirc. Runs after event
    // bookkeeping so EndOfTrack still records queue/stop-after-current state.
    if session_lost && fast_reconnect_failed && !audibly_playing {
      info!("buffered playback ended after fast reconnect failure; replacing backend");
      request_full_streaming_recovery(
        &app,
        &player,
        &shared_position,
        &shared_is_playing,
        &recovery_tx,
        crate::infra::app_strings(&app)
          .await
          .status
          .infra
          .native_lost_recovering,
        pending_end_of_track.clone(),
      )
      .await;
      return;
    }
  }

  if matches!(
    player.connection_state(),
    StreamingConnectionState::Shutdown
  ) {
    return;
  }

  request_full_streaming_recovery(
    &app,
    &player,
    &shared_position,
    &shared_is_playing,
    &recovery_tx,
    crate::infra::app_strings(&app)
      .await
      .status
      .infra
      .native_stopped_recovering,
    pending_end_of_track,
  )
  .await;
}

fn recovery_watchdog_should_escalate(desired_playing: bool, stalled_for: Duration) -> bool {
  desired_playing && stalled_for >= STALLED_PLAYBACK_RECOVERY_TIMEOUT
}

/// Spirc stops on purpose at the end of its list, and a new backend cannot add tracks;
/// only a lost session keeps the watchdog armed.
fn stop_disarms_watchdog(session_lost: bool) -> bool {
  !session_lost
}

/// Reissue the published Spotify queue slot, honoring its desired-playing
/// state: a slot the user paused is reloaded silently so a stale Spirc event
/// cannot restart it.
async fn reissue_published_queue_slot(
  player: &Arc<StreamingPlayer>,
  uri: &str,
  desired_playing: bool,
) {
  let result = if desired_playing {
    player.play_uri(uri).await
  } else {
    player.load_uri_paused(uri).await
  };
  if let Err(e) = result {
    info!("failed to reissue queued Spotify track: {}", e);
  }
}

/// A failed preload also sends Unavailable; only a failed load names the track of the last Loading event.
fn unavailable_is_for_the_loading_track(loading: Option<&str>, unavailable: &str) -> bool {
  loading.is_none_or(|loading| loading == unavailable)
}

fn unavailable_is_transport_related(
  transport_recovery_pending: bool,
  connection_state: StreamingConnectionState,
) -> bool {
  transport_recovery_pending
    || !matches!(connection_state, StreamingConnectionState::Connected { .. })
}

async fn request_full_streaming_recovery(
  app: &Arc<Mutex<App>>,
  player: &Arc<StreamingPlayer>,
  shared_position: &Arc<AtomicU64>,
  shared_is_playing: &Arc<AtomicBool>,
  recovery_tx: &tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  status_message: &str,
  continue_after_track: Option<String>,
) {
  if let Some(mut request) = disconnect_streaming_player(
    app,
    player,
    shared_position,
    shared_is_playing,
    status_message,
    true,
  )
  .await
  {
    request.continue_after_track = continue_after_track.or(request.continue_after_track);
    let _ = recovery_tx.send(request);
  }
}

fn spawn_end_of_track_continuation(
  app: Arc<Mutex<App>>,
  player: Arc<StreamingPlayer>,
  previous_track_id: String,
  playback_transition_generation: Arc<AtomicU64>,
) {
  let observed_transition_generation = playback_transition_generation.load(Ordering::Relaxed);
  let mut connection_state_rx = player.connection_state_receiver();

  tokio::spawn(async move {
    let wait_for_stable_connection = async {
      loop {
        let connection_state = *connection_state_rx.borrow_and_update();
        match connection_state {
          StreamingConnectionState::Connected { generation } => {
            tokio::time::sleep(END_OF_TRACK_CONTINUATION_DELAY).await;
            if matches!(
              *connection_state_rx.borrow(),
              StreamingConnectionState::Connected {
                generation: current_generation
              } if current_generation == generation
            ) {
              return true;
            }
          }
          StreamingConnectionState::Reconnecting { .. } => {
            if connection_state_rx.changed().await.is_err() {
              return false;
            }
          }
          StreamingConnectionState::Failed { .. } | StreamingConnectionState::Shutdown => {
            return false;
          }
        }
      }
    };

    if !tokio::time::timeout(END_OF_TRACK_RECONNECT_TIMEOUT, wait_for_stable_connection)
      .await
      .unwrap_or(false)
    {
      warn!(
        "end-of-track continuation for {} abandoned: no stable connection within {}s",
        previous_track_id,
        END_OF_TRACK_RECONNECT_TIMEOUT.as_secs()
      );
      return;
    }

    // TrackChanged is proof that Spirc already advanced. The fallback must only
    // run when no transition was observed, otherwise it skips twice. Loading
    // alone is not sufficient: a dead connection can stall after that event.
    if playback_transition_generation.load(Ordering::Relaxed) != observed_transition_generation
      || !is_current_streaming_player(&app, &player).await
      || !app.lock().await.native_context_should_drive()
    {
      return;
    }

    app
      .lock()
      .await
      .dispatch(IoEvent::EnsurePlaybackContinues(previous_track_id));
  });
}

async fn is_current_streaming_player(app: &Arc<Mutex<App>>, player: &Arc<StreamingPlayer>) -> bool {
  let app_lock = app.lock().await;
  owns_published_player(app_lock.streaming_player.as_ref(), player)
}

/// Whether `player` is still the one the app publishes.
fn owns_published_player<T>(current: Option<&Arc<T>>, player: &Arc<T>) -> bool {
  current.is_some_and(|current| Arc::ptr_eq(current, player))
}

/// Whether a request may still replay: it owns the identity and its player is
/// still published.
fn request_may_replay_over<T>(
  current_generation: u64,
  request_generation: u64,
  published_player: Option<&Arc<T>>,
  player: &Arc<T>,
) -> bool {
  current_generation == request_generation && owns_published_player(published_player, player)
}

fn current_playback_matches_native(app: &App, player: &StreamingPlayer) -> bool {
  let Some(ctx) = app.current_playback_context.as_ref() else {
    return app.is_streaming_active;
  };

  if let Some(native_id) = app.native_device_id.as_ref() {
    if ctx.device.id.as_ref() == Some(native_id) {
      return true;
    }
  }

  ctx.device.name.eq_ignore_ascii_case(player.device_name()) && app.has_fresh_native_activity()
}

async fn disconnect_streaming_player(
  app: &Arc<Mutex<App>>,
  player: &Arc<StreamingPlayer>,
  shared_position: &Arc<AtomicU64>,
  shared_is_playing: &Arc<AtomicBool>,
  status_message: &str,
  restore_playback: bool,
) -> Option<StreamingRecoveryRequest> {
  let mut app_lock = app.lock().await;
  let current_player = app_lock.streaming_player.as_ref()?;
  if !Arc::ptr_eq(current_player, player) {
    return None;
  }

  // Only a restoring teardown (unexpected shutdown / forced recovery) may
  // reselect, and only while the API context still points at the native
  // device. The flag also gates the queue-slot replay in the returned request.
  let reselect_device = restore_playback && current_playback_matches_native(&app_lock, player);
  let continue_after_track = if reselect_device {
    let position_ms = u32::try_from(shared_position.load(Ordering::Relaxed)).unwrap_or(u32::MAX);
    let is_playing = shared_is_playing.load(Ordering::Relaxed);
    app_lock.prepare_native_playback_recovery(position_ms, is_playing);
    let completed_track = app_lock
      .native_playback_recovery
      .as_ref()
      .and_then(|snapshot| snapshot.completed_track_uri())
      .map(app::base62_id_of)
      .map(str::to_string);
    if let Some(track_id) = completed_track.as_deref() {
      info!(
        "native track {} completed while the transport was down; continuing after recovery",
        track_id
      );
    }
    completed_track
  } else {
    // An intentional Spotify Connect transfer must not be undone by a later
    // native recovery attempt.
    app_lock.clear_native_playback_recovery();
    None
  };

  if !restore_playback {
    // A handoff or local stop invalidates parked playback intent and the
    // published queue slot; replaying either would steal playback back, and a
    // leftover slot would ghost-own the playbar and transport (#437).
    app_lock.pending_start_playback = None;
    app_lock.forget_queue_slot_after_handoff();
  }

  app_lock.streaming_player = None;
  // Stop the old Connect session so it doesn't linger as a ghost device (#297).
  player.shutdown();
  app_lock.is_streaming_active = false;
  app_lock.native_activation_pending = false;
  app_lock.native_device_id = None;
  app_lock.native_is_playing = Some(false);
  app_lock.native_track_info = None;
  app_lock.native_playback_origin = None;
  // Clearing the session below bumps its generation, which would turn a
  // shuffled queue suspension into a silent no-op at resume time; convert it
  // to a context snapshot first (while the cached context is still around).
  app_lock.convert_shuffled_suspension_to_context(None);
  app_lock.clear_native_shuffle_session();
  app_lock.song_progress_ms = 0;
  app_lock.clear_native_position();
  app_lock.last_track_id = None;
  app_lock.last_device_activation = None;
  app_lock.seek_ms = None;
  // The cached API context may still point at the stale native session; the
  // dispatch below repopulates it if Spotify has already moved to another device.
  app_lock.current_playback_context = None;
  app_lock.set_status_message(status_message, 8);
  app_lock.dispatch(IoEvent::GetCurrentPlayback);
  app_lock.note_display_changes();

  shared_position.store(0, Ordering::Relaxed);
  shared_is_playing.store(false, Ordering::Relaxed);

  let generation = app_lock.begin_native_recovery();
  Some(StreamingRecoveryRequest {
    generation,
    reselect_device,
    restore_playback,
    continue_after_track,
    reacquire: false,
  })
}

#[cfg(test)]
mod tests {
  use super::{
    handle_streaming_recovery, merge_recovery_requests, next_recovery_retry_delay,
    owns_published_player, recovery_needs_native_selection, recovery_watchdog_should_escalate,
    request_may_replay_over, session_disconnect_recovery, should_replay_published_queue_slot,
    stop_disarms_watchdog, unavailable_is_for_the_loading_track, unavailable_is_transport_related,
    BuildAttempt, RebuildOutcome, RecoveryBackend, SessionDisconnectReason,
    SessionDisconnectRecovery, StreamingBuildError, StreamingConfig, StreamingConnectionState,
    StreamingRecoveryContext, StreamingRecoveryRequest, RECOVERY_RETRY_BASE_DELAY,
    RECOVERY_RETRY_MAX_DELAY, STALLED_PLAYBACK_RECOVERY_TIMEOUT,
  };
  use crate::core::app::App;
  use librespot_core::error::Error as LibrespotError;
  use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
  use std::sync::Arc;
  use std::time::{Duration, Instant};

  /// The startup path's recovery context, minus the OS media integrations.
  fn recovery_context(
    app: Arc<tokio::sync::Mutex<App>>,
  ) -> (
    StreamingRecoveryContext,
    tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  ) {
    let (recovery_tx, recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    (
      StreamingRecoveryContext {
        app,
        shared_position: Arc::new(AtomicU64::new(0)),
        shared_is_playing: Arc::new(AtomicBool::new(false)),
        recovery_rx,
        recovery_tx: recovery_tx.clone(),
        streaming_config: StreamingConfig::default(),
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        mpris_manager: None,
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        macos_media_manager: None,
        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        windows_media_manager: None,
      },
      recovery_tx,
    )
  }

  /// What a wake usually loses: DNS or the socket, not the login.
  fn transient_build_failure() -> StreamingBuildError {
    StreamingBuildError::Spirc(LibrespotError::unavailable(anyhow::anyhow!("dns error")))
  }

  /// What a transport drop asks for: restore what played on our device.
  fn restore_request(generation: u64) -> StreamingRecoveryRequest {
    StreamingRecoveryRequest {
      generation,
      reselect_device: true,
      restore_playback: true,
      continue_after_track: None,
      reacquire: false,
    }
  }

  /// What the recovery handler did with a built player, by build attempt.
  #[derive(Clone, Debug, PartialEq)]
  enum BuildEvent {
    Stopped(u32),
    Published(u32),
    Watched(u32),
  }

  /// The player one scripted build returns.
  struct ScriptedPlayer {
    attempt: u32,
    available: AtomicBool,
  }

  impl ScriptedPlayer {
    fn new(attempt: u32) -> Self {
      Self {
        attempt,
        available: AtomicBool::new(true),
      }
    }

    /// The player deferred startup publishes: no attempt number.
    fn from_startup() -> Arc<Self> {
      Arc::new(Self::new(0))
    }
  }

  /// Where a scripted backend's player is published; every build gets a clone.
  #[derive(Clone, Default)]
  struct ScriptedSlot(Arc<std::sync::Mutex<Option<Arc<ScriptedPlayer>>>>);

  impl ScriptedSlot {
    /// As deferred startup does: whatever was published is replaced.
    fn publish(&self, player: Arc<ScriptedPlayer>) {
      *self.0.lock().unwrap() = Some(player);
    }

    /// Returns the player it replaced.
    fn replace(&self, player: Arc<ScriptedPlayer>) -> Option<Arc<ScriptedPlayer>> {
      self.0.lock().unwrap().replace(player)
    }

    fn player(&self) -> Option<Arc<ScriptedPlayer>> {
      self.0.lock().unwrap().clone()
    }
  }

  /// A backend without a session, so the recovery handler itself can run in a
  /// test.
  struct ScriptedBackend<S>(Arc<ScriptedSession<S>>);

  struct ScriptedSession<S> {
    script: S,
    attempts: AtomicU32,
    /// Like the app's slot: it keeps a stopped player until a rebuild
    /// replaces it.
    published: ScriptedSlot,
    events: std::sync::Mutex<Vec<BuildEvent>>,
  }

  impl<S> ScriptedBackend<S> {
    fn new(script: S) -> Self {
      Self(Arc::new(ScriptedSession {
        script,
        attempts: AtomicU32::new(0),
        published: ScriptedSlot::default(),
        events: std::sync::Mutex::new(Vec::new()),
      }))
    }

    fn attempts(&self) -> u32 {
      self.0.attempts.load(Ordering::SeqCst)
    }

    /// A player deferred startup published, before the handler sees any request.
    fn publish_startup_player(&self) {
      self.0.published.publish(ScriptedPlayer::from_startup());
    }

    fn events(&self) -> Vec<BuildEvent> {
      self.0.events.lock().unwrap().to_vec()
    }

    fn record(&self, event: BuildEvent) {
      self.0.events.lock().unwrap().push(event);
    }

    /// Recorded the first time only: a stop, like a real shutdown, is idempotent.
    fn stop(&self, player: &ScriptedPlayer) {
      if player.available.swap(false, Ordering::SeqCst) {
        self.record(BuildEvent::Stopped(player.attempt));
      }
    }
  }

  impl<S> Clone for ScriptedBackend<S> {
    fn clone(&self) -> Self {
      Self(Arc::clone(&self.0))
    }
  }

  impl<S> RecoveryBackend for ScriptedBackend<S>
  where
    S: Fn(u32, ScriptedSlot) -> BuildAttempt<ScriptedPlayer> + Send + Sync,
  {
    type Player = ScriptedPlayer;

    fn build(&self, _config: StreamingConfig) -> BuildAttempt<Self::Player> {
      let attempt = self.0.attempts.fetch_add(1, Ordering::SeqCst) + 1;
      (self.0.script)(attempt, self.0.published.clone())
    }

    fn published_player(&self, _app: &App) -> Option<Arc<Self::Player>> {
      self.0.published.player()
    }

    fn is_available(player: &Self::Player) -> bool {
      player.available.load(Ordering::SeqCst)
    }

    fn publish(
      &self,
      _app: &mut App,
      _ctx: &StreamingRecoveryContext,
      player: Arc<Self::Player>,
      _reacquired: bool,
    ) {
      // As the librespot backend does: the player it replaces is stopped.
      if let Some(old) = self.0.published.replace(Arc::clone(&player)) {
        if !Arc::ptr_eq(&old, &player) {
          self.stop(&old);
        }
      }
      self.record(BuildEvent::Published(player.attempt));
    }

    fn watch(&self, _ctx: &StreamingRecoveryContext, player: Arc<Self::Player>) {
      self.record(BuildEvent::Watched(player.attempt));
    }

    fn discard(&self, player: Arc<Self::Player>) {
      self.stop(&player);
    }
  }

  /// Runs a recovery handler until it has handled every queued request and is
  /// back on its channel.
  async fn run_recovery_handler_until_parked<F: std::future::Future>(handler: F) {
    assert!(
      tokio::time::timeout(RECOVERY_RETRY_MAX_DELAY, handler)
        .await
        .is_err(),
      "the recovery handler returned instead of waiting for the next request"
    );
  }

  #[test]
  fn a_replaced_or_cleared_player_is_no_longer_owned() {
    let installed = Arc::new(());
    let other = Arc::new(());

    assert!(owns_published_player(Some(&installed), &installed));
    assert!(!owns_published_player(Some(&other), &installed));
    assert!(!owns_published_player(None, &installed));
  }

  /// A delayed request from an older identity must not replay over a newer recovery's player.
  #[test]
  fn a_stale_request_does_not_replay_over_a_newer_published_player() {
    let published = Arc::new(());
    // The newer recovery owns generation 2 and its player is published.
    assert!(request_may_replay_over(2, 2, Some(&published), &published));
    // The delayed generation-1 request is stale: the identity check alone would pass.
    assert!(!request_may_replay_over(2, 1, Some(&published), &published));
    // A current generation whose player was replaced or removed is still refused.
    assert!(!request_may_replay_over(2, 2, None, &published));
    let other = Arc::new(());
    assert!(!request_may_replay_over(2, 2, Some(&other), &published));
  }

  #[test]
  fn decoded_owner_cancels_recovery_and_can_reacquire_after_release() {
    use crate::core::{app::App, source::Source, user_config::UserConfig};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let (recovery_tx, mut recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    app.streaming_recovery_tx = Some(recovery_tx);
    app.claim_decoded_sink(Source::YouTube);
    assert!(matches!(
      super::rebuild_cancellation(&app),
      Some(super::RebuildCancellation::DecodedOwner)
    ));
    app.native_backend_pending = true;
    super::finish_cancelled_recovery(&mut app, true);
    assert!(!app.native_backend_pending);
    assert!(app.native_backend_parked());
    app.release_decoded_sink_claim();
    assert!(app.reacquire_parked_backend());
    assert!(recovery_rx.try_recv().unwrap().reacquire);
  }

  #[test]
  fn apple_music_cancellation_preserves_the_absent_backend_for_reacquisition() {
    use crate::core::{app::App, user_config::UserConfig};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.claim_apple_music();
    assert!(app.apple_music_owns_playback());
    assert!(matches!(
      super::rebuild_cancellation(&app),
      Some(super::RebuildCancellation::Cancelled)
    ));

    app.native_backend_pending = true;
    super::finish_cancelled_recovery(&mut app, false);
    assert!(!app.native_backend_pending);
    assert!(app.native_backend_parked());
  }

  #[test]
  fn frontend_shutdown_is_not_a_decoded_owner_cancellation() {
    use crate::core::{app::App, source::Source, user_config::UserConfig};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.claim_decoded_sink(Source::YouTube);
    app.close_io_channel();
    assert!(matches!(
      super::rebuild_cancellation(&app),
      Some(super::RebuildCancellation::Cancelled)
    ));
    app.native_backend_pending = true;
    super::finish_cancelled_recovery(&mut app, false);
    assert!(!app.native_backend_pending);
    assert!(!app.native_backend_parked());
  }

  #[cfg(all(
    feature = "soloist",
    not(all(target_os = "macos", target_arch = "x86_64"))
  ))]
  #[test]
  fn cancelling_librespot_recovery_does_not_clear_soloist_startup_pending() {
    use crate::core::{
      app::App,
      user_config::{PlaybackEngine, UserConfig},
    };
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user_config.behavior.enable_streaming = true;
    app.user_config.behavior.playback_engine = PlaybackEngine::Soloist;
    assert!(app.apply_playback_engine().is_some());
    assert!(app.native_backend_pending);
    assert!(matches!(
      super::rebuild_cancellation(&app),
      Some(super::RebuildCancellation::Cancelled)
    ));

    for interrupted_by_decoded in [false, true] {
      super::finish_cancelled_recovery(&mut app, interrupted_by_decoded);
      assert!(app.native_backend_pending);
      assert!(!app.native_backend_parked());
    }
  }

  #[test]
  fn a_dns_failure_keeps_retrying_the_connect_device_rebuild() {
    let dns_error =
      StreamingBuildError::Spirc(LibrespotError::unavailable(anyhow::anyhow!("dns error")));
    assert!(dns_error.is_retryable());
    assert!(StreamingBuildError::Timeout { secs: 45 }.is_retryable());
  }

  #[test]
  fn a_missing_login_or_rejected_credentials_stop_the_rebuild_retries() {
    assert!(!StreamingBuildError::Setup(anyhow::anyhow!("No cached Spotify login")).is_retryable());
    assert!(
      !StreamingBuildError::Spirc(LibrespotError::unauthenticated(anyhow::anyhow!(
        "bad token"
      )))
      .is_retryable()
    );
    assert!(
      !StreamingBuildError::Spirc(LibrespotError::permission_denied(anyhow::anyhow!("denied")))
        .is_retryable()
    );
    // login5 answers a bad stored credential with FailedPrecondition.
    for detail in [
      "Login request was denied: INVALID_CREDENTIALS",
      "Login request was denied: UNKNOWN_IDENTIFIER",
    ] {
      assert!(
        !StreamingBuildError::Spirc(LibrespotError::failed_precondition(anyhow::anyhow!(detail)))
          .is_retryable(),
        "{detail} is a rejected login, not a transient failure"
      );
    }
    // FailedPrecondition is also librespot's generic local-state error, which a
    // retry can still fix.
    assert!(
      StreamingBuildError::Spirc(LibrespotError::failed_precondition(anyhow::anyhow!(
        "No client token response type"
      )))
      .is_retryable()
    );
  }

  /// Six access points answering `TryAnotherAP` surface as `PermissionDenied`:
  /// a retryable cause, not a rejected login.
  #[test]
  fn an_exhausted_access_point_list_is_not_a_rejected_login() {
    assert!(
      StreamingBuildError::Spirc(LibrespotError::permission_denied(anyhow::anyhow!(
        "Login failed with reason: Try another access point"
      )))
      .is_retryable()
    );
  }

  /// A refused login ends the chain on its first attempt.
  #[tokio::test]
  async fn a_login5_rejected_login_ends_the_recovery_chain_without_retrying() {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (mut ctx, _recovery_tx) = recovery_context(Arc::clone(&app));

    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_for_build = Arc::clone(&attempts);
    let build = move |_config: StreamingConfig| -> BuildAttempt<()> {
      let attempt = attempts_for_build.fetch_add(1, Ordering::SeqCst) + 1;
      Box::pin(async move {
        // A second attempt is the regression; answer it so the test fails.
        if attempt > 1 {
          return Ok(());
        }
        Err(StreamingBuildError::Spirc(
          LibrespotError::failed_precondition(anyhow::anyhow!(
            "Login request was denied: INVALID_CREDENTIALS"
          )),
        ))
      })
    };

    let mut request = StreamingRecoveryRequest {
      generation: 1,
      reselect_device: true,
      restore_playback: true,
      continue_after_track: None,
      reacquire: false,
    };
    let outcome = super::rebuild_with_retries(
      &mut ctx,
      &mut request,
      Duration::from_millis(1),
      Instant::now(),
      build,
      |_| None,
    )
    .await;

    assert!(
      matches!(outcome, RebuildOutcome::Failed(ref error) if !error.is_retryable()),
      "a rejected login is a failure the chain gives up on"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
  }

  /// A transient failure is retried; a failed attempt that finds a newer
  /// recovery identity ends the chain and leaves the newer request queued.
  #[tokio::test]
  async fn recovery_retries_a_transient_failure_then_yields_to_a_newer_request() {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (mut ctx, recovery_tx) = recovery_context(Arc::clone(&app));

    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_for_build = Arc::clone(&attempts);
    let app_for_build = Arc::clone(&app);
    let build = move |_config: StreamingConfig| -> BuildAttempt<()> {
      let attempt = attempts_for_build.fetch_add(1, Ordering::SeqCst) + 1;
      let app = Arc::clone(&app_for_build);
      let recovery_tx = recovery_tx.clone();
      Box::pin(async move {
        match attempt {
          // A DNS failure right after a wake is retried.
          1 => Err(transient_build_failure()),
          // A transport drop owns a newer generation from here on.
          2 => {
            let generation = {
              let mut app = app.lock().await;
              app.begin_native_recovery()
            };
            recovery_tx
              .send(restore_request(generation))
              .expect("the recovery channel is open");
            Err(transient_build_failure())
          }
          _ => Ok(()),
        }
      })
    };

    let mut request = restore_request(1);
    let outcome = super::rebuild_with_retries(
      &mut ctx,
      &mut request,
      Duration::from_millis(1),
      Instant::now(),
      &build,
      |_| None,
    )
    .await;

    assert!(matches!(outcome, RebuildOutcome::Superseded));
    // One retry, then the second failure finds the chain taken over.
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    // The newer request is the handler's next chain, not this one's.
    let newer = ctx
      .recovery_rx
      .try_recv()
      .expect("the newer request is queued");
    assert_eq!(newer.generation, 2);
    assert!(ctx.recovery_rx.try_recv().is_err());
  }

  /// Through the handler: a transient failure is retried, a late build is
  /// stopped, and the newer request's parked start replays over its build.
  #[tokio::test(start_paused = true)]
  async fn recovery_retries_a_transient_failure_then_replays_the_parked_start_of_the_newer_request()
  {
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;
    use std::sync::mpsc::{channel, TryRecvError};
    use std::time::SystemTime;

    let (tx, io_rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    app.park_start_playback(Some("spotify:playlist:parked".to_string()), None, None);
    // A start asked for while the backend is gone waits on this flag.
    app.native_backend_pending = true;
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (ctx, recovery_tx) = recovery_context(Arc::clone(&app));

    let app_for_script = Arc::clone(&app);
    let recovery_tx_for_script = recovery_tx.clone();
    let script = move |attempt: u32, _published: ScriptedSlot| -> BuildAttempt<ScriptedPlayer> {
      let app = Arc::clone(&app_for_script);
      let recovery_tx = recovery_tx_for_script.clone();
      Box::pin(async move {
        match attempt {
          // A DNS failure right after a wake is retried.
          1 => return Err(transient_build_failure()),
          // A transport drop owns a newer generation from here on.
          2 => {
            let generation = app.lock().await.begin_native_recovery();
            recovery_tx
              .send(restore_request(generation))
              .expect("the recovery channel is open");
          }
          // The newer recovery's own build.
          _ => {}
        }
        Ok(ScriptedPlayer::new(attempt))
      })
    };
    let backend = ScriptedBackend::new(script);
    let record = backend.clone();
    recovery_tx
      .send(restore_request(1))
      .expect("the recovery channel is open");

    run_recovery_handler_until_parked(handle_streaming_recovery(ctx, backend)).await;

    // One failure, the superseded build, then the newer recovery's build.
    assert_eq!(record.attempts(), 3);
    assert_eq!(
      record.events(),
      [
        BuildEvent::Stopped(2),
        BuildEvent::Published(3),
        BuildEvent::Watched(3),
      ],
      "the build that lost its recovery is stopped, and only the newer one is published"
    );
    {
      let app = app.lock().await;
      assert!(
        !app.native_backend_pending,
        "the published player ends the pending window"
      );
    }
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::AutoSelectStreamingDevice(_, false, true))
    ));
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), None, None))
        if context == "spotify:playlist:parked"
    ));
    // Once only: nothing was replayed over the superseded build.
    assert!(matches!(io_rx.try_recv(), Err(TryRecvError::Empty)));
  }

  /// A player published during backoff ends the chain and receives the parked intent.
  #[tokio::test(start_paused = true)]
  async fn a_player_published_during_backoff_ends_retries_and_replays_the_parked_start() {
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;
    use std::sync::mpsc::{channel, TryRecvError};
    use std::time::SystemTime;

    let (tx, io_rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    app.park_start_playback(Some("spotify:playlist:parked".to_string()), None, None);
    app.native_backend_pending = true;
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (ctx, recovery_tx) = recovery_context(Arc::clone(&app));

    let script = move |attempt: u32, published: ScriptedSlot| -> BuildAttempt<ScriptedPlayer> {
      Box::pin(async move {
        if attempt == 1 {
          // Deferred startup finishes while the failed attempt waits to retry.
          published.publish(ScriptedPlayer::from_startup());
          return Err(transient_build_failure());
        }
        Ok(ScriptedPlayer::new(attempt))
      })
    };
    let backend = ScriptedBackend::new(script);
    let record = backend.clone();
    recovery_tx
      .send(restore_request(1))
      .expect("the recovery channel is open");

    run_recovery_handler_until_parked(handle_streaming_recovery(ctx, backend)).await;

    assert_eq!(
      record.attempts(),
      1,
      "the published player must end the retry chain"
    );
    assert_eq!(
      record.events(),
      [],
      "nothing is published over the player startup published"
    );
    {
      let app = app.lock().await;
      assert!(!app.native_backend_pending);
    }
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::AutoSelectStreamingDevice(_, false, true))
    ));
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), None, None))
        if context == "spotify:playlist:parked"
    ));
    assert!(matches!(io_rx.try_recv(), Err(TryRecvError::Empty)));
  }

  /// A build that outlives startup's player gives way: stopped, never published.
  #[tokio::test(start_paused = true)]
  async fn a_build_that_outlives_startup_publishing_a_player_is_stopped_not_published() {
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;
    use std::sync::mpsc::{channel, TryRecvError};
    use std::time::SystemTime;

    let (tx, io_rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    app.park_start_playback(Some("spotify:playlist:parked".to_string()), None, None);
    app.native_backend_pending = true;
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (ctx, recovery_tx) = recovery_context(Arc::clone(&app));

    let script = move |attempt: u32, published: ScriptedSlot| -> BuildAttempt<ScriptedPlayer> {
      Box::pin(async move {
        // Deferred startup finishes while this attempt is still building.
        published.publish(ScriptedPlayer::from_startup());
        Ok(ScriptedPlayer::new(attempt))
      })
    };
    let backend = ScriptedBackend::new(script);
    let record = backend.clone();
    recovery_tx
      .send(restore_request(1))
      .expect("the recovery channel is open");

    run_recovery_handler_until_parked(handle_streaming_recovery(ctx, backend)).await;

    assert_eq!(record.attempts(), 1);
    assert_eq!(
      record.events(),
      [BuildEvent::Stopped(1)],
      "the build gives way to the player startup published"
    );
    {
      let app = app.lock().await;
      assert!(!app.native_backend_pending);
    }
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::AutoSelectStreamingDevice(_, false, true))
    ));
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), None, None))
        if context == "spotify:playlist:parked"
    ));
    assert!(matches!(io_rx.try_recv(), Err(TryRecvError::Empty)));
  }

  /// A delayed duplicate from an older identity must not replay over the live player.
  #[tokio::test(start_paused = true)]
  async fn a_stale_duplicate_does_not_replay_over_a_live_player() {
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;
    use std::sync::mpsc::{channel, TryRecvError};
    use std::time::SystemTime;

    let (tx, io_rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    app.park_start_playback(Some("spotify:playlist:parked".to_string()), None, None);
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (ctx, recovery_tx) = recovery_context(Arc::clone(&app));

    let backend = ScriptedBackend::new(|_attempt: u32, _slot: ScriptedSlot| {
      Box::pin(async { Ok(ScriptedPlayer::new(1)) }) as BuildAttempt<ScriptedPlayer>
    });
    backend.publish_startup_player();
    let record = backend.clone();

    // The stale duplicate arrives after the current request has been handled.
    let stale_tx = recovery_tx.clone();
    tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(10)).await;
      let _ = stale_tx.send(StreamingRecoveryRequest {
        generation: 0,
        restore_playback: true,
        ..StreamingRecoveryRequest::default()
      });
    });
    recovery_tx
      .send(StreamingRecoveryRequest {
        generation: 1,
        restore_playback: true,
        ..StreamingRecoveryRequest::default()
      })
      .expect("the recovery channel is open");

    run_recovery_handler_until_parked(handle_streaming_recovery(ctx, backend)).await;

    assert_eq!(record.attempts(), 0, "no build over the live player");
    // The current request replayed the parked start over the live player once.
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), None, None))
        if context == "spotify:playlist:parked"
    ));
    // The stale duplicate replayed nothing.
    assert!(matches!(io_rx.try_recv(), Err(TryRecvError::Empty)));
  }

  /// The in-loop replay credits the chain's retry time first: a start parked
  /// before the chain began replays instead of being reported expired.
  #[tokio::test(start_paused = true)]
  async fn the_reused_player_replay_credits_chain_time_to_a_parked_start() {
    use crate::core::app::max_parked_age;
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;
    use std::sync::mpsc::{channel, TryRecvError};
    use std::time::SystemTime;

    let (tx, io_rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert_eq!(app.begin_native_recovery(), 1);
    app.park_start_playback(Some("spotify:playlist:parked".to_string()), None, None);
    // Expired already, were it not for the chain time the replay credits.
    let chain = Duration::from_secs(180);
    if let Some(pending) = app.pending_start_playback.as_mut() {
      pending.parked_at = Instant::now() - max_parked_age() - chain + Duration::from_secs(1);
    }
    let chain_started = Instant::now() - chain;
    let app = Arc::new(tokio::sync::Mutex::new(app));
    let (mut ctx, _recovery_tx) = recovery_context(Arc::clone(&app));

    let script = move |_attempt: u32, published: ScriptedSlot| -> BuildAttempt<ScriptedPlayer> {
      Box::pin(async move {
        // Deferred startup finishes while the failed attempt waits to retry.
        published.publish(ScriptedPlayer::from_startup());
        Err(transient_build_failure())
      })
    };
    let backend = ScriptedBackend::new(script);
    let record = backend.clone();

    let mut request = restore_request(1);
    let outcome = super::rebuild_with_retries(
      &mut ctx,
      &mut request,
      Duration::from_millis(1),
      chain_started,
      |config| backend.build(config),
      |app| backend.available_published_player(app),
    )
    .await;

    assert!(matches!(outcome, RebuildOutcome::Reused));
    assert_eq!(
      record.attempts(),
      1,
      "the published player ends the retry chain"
    );
    assert_eq!(
      record.events(),
      [],
      "nothing is published over the player startup published"
    );
    {
      let app = app.lock().await;
      assert!(!app.native_backend_pending);
      assert_ne!(
        app.status_message(),
        Some(crate::core::i18n::EN.status.app.native_request_expired)
      );
    }
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::AutoSelectStreamingDevice(_, false, true))
    ));
    assert!(matches!(
      io_rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), None, None))
        if context == "spotify:playlist:parked"
    ));
    assert!(matches!(io_rx.try_recv(), Err(TryRecvError::Empty)));
  }

  #[test]
  fn retry_delays_back_off_up_to_a_minute() {
    assert_eq!(
      next_recovery_retry_delay(RECOVERY_RETRY_BASE_DELAY),
      Duration::from_secs(10)
    );
    assert_eq!(
      next_recovery_retry_delay(Duration::from_secs(30)),
      Duration::from_secs(60)
    );
    assert_eq!(
      next_recovery_retry_delay(RECOVERY_RETRY_MAX_DELAY),
      RECOVERY_RETRY_MAX_DELAY
    );
  }

  #[test]
  fn recovery_watchdog_waits_for_the_full_stall_window() {
    assert!(!recovery_watchdog_should_escalate(
      true,
      STALLED_PLAYBACK_RECOVERY_TIMEOUT - Duration::from_millis(1)
    ));
    assert!(recovery_watchdog_should_escalate(
      true,
      STALLED_PLAYBACK_RECOVERY_TIMEOUT
    ));
  }

  #[test]
  fn recovery_watchdog_never_rebuilds_explicitly_paused_playback() {
    assert!(!recovery_watchdog_should_escalate(
      false,
      STALLED_PLAYBACK_RECOVERY_TIMEOUT * 2
    ));
  }

  #[test]
  fn a_stop_at_the_end_of_the_list_disarms_the_watchdog_unless_the_session_is_lost() {
    assert!(stop_disarms_watchdog(false));
    assert!(!stop_disarms_watchdog(true));
  }

  #[test]
  fn a_failed_preload_does_not_count_as_an_unavailable_load() {
    let loading = Some("spotify:track:playing");
    assert!(!unavailable_is_for_the_loading_track(
      loading,
      "spotify:track:queued"
    ));
    assert!(unavailable_is_for_the_loading_track(
      loading,
      "spotify:track:playing"
    ));
    assert!(unavailable_is_for_the_loading_track(
      None,
      "spotify:track:queued"
    ));
  }

  #[test]
  fn unavailable_is_transport_related_until_recovery_progress_is_verified() {
    assert!(unavailable_is_transport_related(
      true,
      StreamingConnectionState::Connected { generation: 1 }
    ));
    assert!(unavailable_is_transport_related(
      false,
      StreamingConnectionState::Reconnecting { generation: 2 }
    ));
    assert!(!unavailable_is_transport_related(
      false,
      StreamingConnectionState::Connected { generation: 2 }
    ));
  }

  #[test]
  fn session_disconnect_recovery_respects_device_handoffs_and_local_shutdowns() {
    assert_eq!(
      session_disconnect_recovery(SessionDisconnectReason::ExternalDeviceHandoff),
      SessionDisconnectRecovery::RebuildIdle
    );
    assert_eq!(
      session_disconnect_recovery(SessionDisconnectReason::UnexpectedShutdown),
      SessionDisconnectRecovery::RestorePlayback
    );
    assert_eq!(
      session_disconnect_recovery(SessionDisconnectReason::LocalCommand),
      SessionDisconnectRecovery::Stop
    );
  }

  #[test]
  fn queue_slot_replay_follows_restore_intent_not_reselect() {
    // A queue playing over an idle app recovers with restore intent but no
    // native context match (reselect stays false); the slot must still replay.
    let idle_app_restore = StreamingRecoveryRequest {
      restore_playback: true,
      ..StreamingRecoveryRequest::default()
    };
    assert!(should_replay_published_queue_slot(&idle_app_restore, true));
    assert!(!should_replay_published_queue_slot(
      &idle_app_restore,
      false
    ));

    // A handoff rebuild never replays, regardless of reselect.
    let handoff = StreamingRecoveryRequest::default();
    assert!(!should_replay_published_queue_slot(&handoff, true));
    let handoff_with_reselect = StreamingRecoveryRequest {
      reselect_device: true,
      ..StreamingRecoveryRequest::default()
    };
    assert!(!should_replay_published_queue_slot(
      &handoff_with_reselect,
      true
    ));
  }

  #[test]
  fn idle_app_queue_recovery_still_checks_device_ownership_before_replay() {
    let idle_app_restore = StreamingRecoveryRequest {
      restore_playback: true,
      ..StreamingRecoveryRequest::default()
    };

    assert!(recovery_needs_native_selection(&idle_app_restore, true));
    assert!(!recovery_needs_native_selection(&idle_app_restore, false));
  }

  #[test]
  fn recovery_request_merge_ors_intent_across_transport_drops() {
    let mut request = StreamingRecoveryRequest {
      restore_playback: true,
      continue_after_track: Some("older".to_string()),
      ..StreamingRecoveryRequest::default()
    };
    merge_recovery_requests(
      &mut request,
      StreamingRecoveryRequest {
        generation: 2,
        reselect_device: true,
        restore_playback: true,
        continue_after_track: None,
        reacquire: false,
      },
    );
    assert!(request.reselect_device);
    assert!(request.restore_playback);
    assert_eq!(request.generation, 2);
    // The newest non-None continuation wins; None does not erase it.
    assert_eq!(request.continue_after_track.as_deref(), Some("older"));

    merge_recovery_requests(
      &mut request,
      StreamingRecoveryRequest {
        generation: 3,
        reselect_device: false,
        restore_playback: true,
        continue_after_track: Some("newer".to_string()),
        reacquire: false,
      },
    );
    assert!(request.reselect_device);
    assert_eq!(request.generation, 3);
    assert_eq!(request.continue_after_track.as_deref(), Some("newer"));
  }

  #[test]
  fn a_reacquire_merged_with_a_transport_drop_still_restores_after_the_rebuild() {
    let mut request = StreamingRecoveryRequest {
      restore_playback: true,
      ..StreamingRecoveryRequest::default()
    };
    merge_recovery_requests(
      &mut request,
      StreamingRecoveryRequest {
        restore_playback: true,
        reacquire: true,
        ..StreamingRecoveryRequest::default()
      },
    );
    assert!(request.reacquire);
    assert!(request.restore_playback);
    assert!(!request.reselect_device);
  }

  #[test]
  fn recovery_request_merge_handoff_vetoes_restore_intent() {
    let restore = StreamingRecoveryRequest {
      generation: 1,
      reselect_device: true,
      restore_playback: true,
      continue_after_track: Some("track".to_string()),
      reacquire: false,
    };
    let handoff = StreamingRecoveryRequest::default();

    // Handoff queued behind a restore request.
    let mut request = restore.clone();
    merge_recovery_requests(&mut request, handoff.clone());
    assert!(!request.reselect_device);
    assert!(!request.restore_playback);
    assert!(request.continue_after_track.is_none());

    // Restore queued behind a handoff request.
    let mut request = handoff;
    merge_recovery_requests(&mut request, restore.clone());
    assert!(!request.reselect_device);
    assert!(!request.restore_playback);
    assert!(request.continue_after_track.is_none());

    // The veto is sticky: a later restore request cannot resurrect intent.
    merge_recovery_requests(&mut request, restore);
    assert!(!request.reselect_device);
    assert!(!request.restore_playback);
    assert!(request.continue_after_track.is_none());
  }
}
