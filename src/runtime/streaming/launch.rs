//! The deferred librespot bring-up.

use super::{startup_device_decision, StartupDeviceEvent};
use crate::core::app::App;
use crate::core::user_config::StartupBehavior;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::requests::spotify_get_typed_for;
use crate::infra::network::IoEvent;
use crate::infra::player;
use crate::infra::spotify::session::{self, Unavailable};
use log::{info, warn};
use std::sync::{atomic::AtomicU64, Arc};
use tokio::sync::Mutex;

/// The status line for a session that cannot carry native streaming, if any.
fn native_streaming_refusal(reason: Unavailable) -> Option<&'static str> {
  match reason {
    Unavailable::NoLogin => None,
    Unavailable::NotPremium => Some("Spotify Premium is required for playback"),
    Unavailable::Offline => Some("Could not connect to Spotify: native streaming disabled"),
  }
}
impl StartupDeviceEvent {
  fn into_io_event(self) -> IoEvent {
    match self {
      StartupDeviceEvent::Transfer {
        device_id,
        persist_device_id,
      } => IoEvent::TransferPlaybackToDevice(device_id, persist_device_id),
      StartupDeviceEvent::AutoSelectStreaming {
        device_name,
        persist_device_id,
      } => IoEvent::AutoSelectStreamingDevice(device_name, persist_device_id, false),
    }
  }
}

/// Everything native streaming needs that used to gate the first frame:
/// account probe, librespot session handshake, player event handler, recovery
/// supervisor, and the saved-device startup decision. Bundled so
/// `deferred_streaming_startup` can run it all on a background task after the
/// UI is already up.
pub(crate) struct DeferredStreamingContext {
  pub(crate) app: Arc<Mutex<App>>,
  pub(crate) streaming_config: player::StreamingConfig,
  pub(crate) device_startup_behavior: StartupBehavior,
  /// Spotify startup Play/Pause, run after the device decision so it lands on
  /// the selected device instead of 404ing with NO_ACTIVE_DEVICE while init is
  /// still in flight. `None` when a non-Spotify session restore owns startup.
  pub(crate) spotify_startup_behavior: Option<StartupBehavior>,
  pub(crate) initial_shuffle_enabled: bool,
  pub(crate) recovery_tx: tokio::sync::mpsc::UnboundedSender<player::StreamingRecoveryRequest>,
  pub(crate) recovery_rx: tokio::sync::mpsc::UnboundedReceiver<player::StreamingRecoveryRequest>,
  pub(crate) shared_position: Arc<AtomicU64>,
  pub(crate) shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub(crate) mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub(crate) macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  pub(crate) windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
}

/// Initialize native streaming in the background. The UI renders its
/// first frame immediately; this task spawns the recovery supervisor, probes
/// the account (reusing the auth `/me` when available), performs the librespot
/// handshake with the same double-timeout as before, stores the player in
/// `App`, spawns the player event handler, and finally makes the saved-device
/// startup decision — dispatching its outcome through the normal IoEvent pump.
// CLI mode never starts native streaming; only a frontend launch does.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub(crate) fn deferred_streaming_startup(ctx: DeferredStreamingContext) {
  tokio::spawn(async move {
    let app = Arc::clone(&ctx.app);
    let spotify_startup_behavior = ctx.spotify_startup_behavior;
    let initial_shuffle_enabled = ctx.initial_shuffle_enabled;
    deferred_streaming_startup_inner(ctx).await;
    // Whatever happened above (backend up, unsupported account, failed or
    // timed-out init), the pending window is over.
    let mut app = app.lock().await;
    app.native_backend_pending = false;
    // The Spotify startup Play/Pause runs here, after the device decision.
    app.run_spotify_startup_behavior(spotify_startup_behavior, initial_shuffle_enabled);
    // A StartPlayback parked during init replays now — against the native
    // backend when it exists, else through the normal Connect path.
    app.replay_pending_start_playback();
  });
}

async fn deferred_streaming_startup_inner(ctx: DeferredStreamingContext) {
  player::spawn_streaming_recovery_handler(player::StreamingRecoveryContext {
    app: Arc::clone(&ctx.app),
    shared_position: Arc::clone(&ctx.shared_position),
    shared_is_playing: Arc::clone(&ctx.shared_is_playing),
    recovery_rx: ctx.recovery_rx,
    recovery_tx: ctx.recovery_tx.clone(),
    streaming_config: ctx.streaming_config.clone(),
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    mpris_manager: ctx.mpris_manager.clone(),
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    macos_media_manager: ctx.macos_media_manager.clone(),
    #[cfg(all(feature = "windows-media", target_os = "windows"))]
    windows_media_manager: ctx.windows_media_manager.clone(),
  });

  // The session refuses a non-Premium account, so it answers the plan check.
  if let Err(reason) = session::current().await {
    info!("native streaming not started: {}", reason.message());
    if let Some(message) = native_streaming_refusal(reason) {
      ctx.app.lock().await.set_status_message(message, 12);
    }
    return;
  }

  info!("initializing native streaming player");
  let streaming_config = ctx.streaming_config;

  // Internal Spirc timeout defaults to 30s (configurable via
  // SPOTATUI_STREAMING_INIT_TIMEOUT_SECS). The outer timeout here is a safety net
  // that catches hangs *outside* Spirc init (e.g. blocking I/O in credential
  // retrieval). Set it above the internal timeout.
  let outer_timeout = player::player_build_timeout();

  let init_task =
    tokio::spawn(async move { player::StreamingPlayer::new_cache_only(streaming_config).await });
  let abort_handle = init_task.abort_handle();

  let streaming_player = match tokio::time::timeout(outer_timeout, init_task).await {
    Ok(Ok(Ok(p))) => {
      info!(
        "native streaming player initialized as '{}'",
        p.device_name()
      );
      // Note: We don't activate() here - that's handled by AutoSelectStreamingDevice
      // which respects the user's saved device preference (e.g., spotifyd)
      Arc::new(p)
    }
    Ok(Ok(Err(e))) => {
      info!(
        "failed to initialize streaming: {} - falling back to web api",
        e
      );
      ctx.app.lock().await.set_status_message(
        "Native streaming didn't start; using Spotify Connect for now. Restart spotatui to reconnect native playback.",
        12,
      );
      return;
    }
    Ok(Err(e)) => {
      info!(
        "streaming initialization panicked: {} - falling back to web api",
        e
      );
      return;
    }
    Err(_) => {
      abort_handle.abort();
      warn!(
        "streaming initialization hung unexpectedly (outer timeout {}s) - falling back to web api",
        outer_timeout.as_secs()
      );
      return;
    }
  };

  info!("native playback enabled - spotatui is available as a spotify connect device");

  // Store streaming player reference in App for direct control (bypasses event channel)
  {
    let mut app_mut = ctx.app.lock().await;
    // The frontend is exiting: nothing would shut this player down.
    if app_mut.io_tx_clone().is_none() {
      drop(app_mut);
      streaming_player.shutdown();
      return;
    }
    app_mut.streaming_player = Some(Arc::clone(&streaming_player));
    // Startup playlist loading may have fallen back to a flat list while the
    // deferred player was unavailable. Refresh once so rootlist folders are
    // reconciled now that librespot is ready.
    app_mut.dispatch(IoEvent::GetPlaylists);
  }

  // Spawn player event listener (updates app state from native player events)
  player::spawn_player_event_handler(player::PlayerEventContext {
    player: Arc::clone(&streaming_player),
    app: Arc::clone(&ctx.app),
    shared_position: ctx.shared_position,
    shared_is_playing: ctx.shared_is_playing,
    recovery_tx: ctx.recovery_tx,
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    mpris_manager: ctx.mpris_manager,
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    macos_media_manager: ctx.macos_media_manager,
    #[cfg(all(feature = "windows-media", target_os = "windows"))]
    windows_media_manager: ctx.windows_media_manager,
  });

  // Auto-select the saved playback device when available (fallback to native
  // streaming). This used to run inline in the network task before the pump
  // started; the decision's outcome now dispatches through the pump.
  let device_name = streaming_player.device_name().to_string();
  let saved_device_id = ctx.app.lock().await.saved_spotify_device_id();
  let mut devices_snapshot = None;
  if let Ok(devices) =
    spotify_get_typed_for::<rspotify::model::device::DevicePayload>("me/player/devices", &[]).await
  {
    let devices_vec = devices.devices;
    let mut app_mut = ctx.app.lock().await;
    app_mut.set_devices(rspotify::model::device::DevicePayload {
      devices: devices_vec.clone(),
    });
    devices_snapshot = Some(devices_vec);
  }

  let startup_decision = startup_device_decision(
    ctx.device_startup_behavior,
    saved_device_id,
    devices_snapshot.as_deref(),
    &device_name,
  );

  let mut app_mut = ctx.app.lock().await;
  if let Some(message) = startup_decision.status_message {
    app_mut.set_status_message(message, 5);
  }
  if let Some(event) = startup_decision.event {
    app_mut.dispatch(event.into_io_event());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_a_missing_login_starts_without_a_status_line() {
    assert_eq!(native_streaming_refusal(Unavailable::NoLogin), None);
    assert!(native_streaming_refusal(Unavailable::NotPremium)
      .is_some_and(|message| message.contains("Premium")));
    assert!(native_streaming_refusal(Unavailable::Offline).is_some());
  }
}
