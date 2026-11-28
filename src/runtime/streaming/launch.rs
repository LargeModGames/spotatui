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
fn native_streaming_refusal(
  reason: Unavailable,
  strings: &crate::core::i18n::Strings,
) -> Option<&'static str> {
  match reason {
    Unavailable::NoLogin => None,
    Unavailable::NotPremium => Some(strings.status.runtime.premium_required),
    Unavailable::Offline => Some(strings.status.runtime.native_offline),
    Unavailable::TimedOut => Some(strings.status.runtime.native_timed_out),
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
/// account probe, librespot session handshake, player event handler, and the
/// saved-device startup decision. Bundled so
/// `deferred_streaming_startup` can run it all on a background task after the
/// UI is already up.
pub(crate) struct DeferredStreamingContext {
  pub(crate) app: Arc<Mutex<App>>,
  /// The recovery generation whose pending window startup owns.
  pub(crate) startup_generation: u64,
  pub(crate) streaming_config: player::StreamingConfig,
  pub(crate) device_startup_behavior: StartupBehavior,
  /// Spotify startup Play/Pause, run after the device decision so it lands on
  /// the selected device instead of 404ing with NO_ACTIVE_DEVICE while init is
  /// still in flight. `None` when a non-Spotify session restore owns startup.
  pub(crate) spotify_startup_behavior: Option<StartupBehavior>,
  pub(crate) initial_shuffle_enabled: bool,
  pub(crate) recovery_tx: tokio::sync::mpsc::UnboundedSender<player::StreamingRecoveryRequest>,
  pub(crate) shared_position: Arc<AtomicU64>,
  pub(crate) shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub(crate) mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub(crate) macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  pub(crate) windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
}

/// End startup's pending window only while it still owns it. Returns whether
/// startup may still run its behavior and replay; a newer recovery or Soloist
/// takes ownership without bumping the generation in Soloist's case.
fn finish_startup_pending(app: &mut App, generation: u64) -> bool {
  if !app.native_engine_still_selected() || app.native_recovery_generation() != generation {
    return false;
  }
  app.native_backend_pending = false;
  true
}

/// Initialize native streaming in the background. The UI renders its
/// first frame immediately; this task probes the account (reusing the auth
/// `/me` when available), performs the librespot handshake with the same
/// double-timeout as before, stores the player in
/// `App`, spawns the player event handler, and finally makes the saved-device
/// startup decision — dispatching its outcome through the normal IoEvent pump.
// CLI mode never starts native streaming; only a frontend launch does.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub(crate) fn deferred_streaming_startup(ctx: DeferredStreamingContext) {
  tokio::spawn(async move {
    let app = Arc::clone(&ctx.app);
    let startup_generation = ctx.startup_generation;
    let spotify_startup_behavior = ctx.spotify_startup_behavior;
    let initial_shuffle_enabled = ctx.initial_shuffle_enabled;
    deferred_streaming_startup_inner(ctx).await;
    // Whatever happened above (backend up, unsupported account, failed or
    // timed-out init), startup's pending window is over — unless a newer
    // recovery claimed it while init was running.
    let mut app = app.lock().await;
    if finish_startup_pending(&mut app, startup_generation) {
      // The Spotify startup Play/Pause runs here, after the device decision.
      app.run_spotify_startup_behavior(spotify_startup_behavior, initial_shuffle_enabled);
      // A StartPlayback parked during init replays now — against the native
      // backend when it exists, else through the normal Connect path.
      app.replay_pending_start_playback();
    }
  });
}

async fn deferred_streaming_startup_inner(ctx: DeferredStreamingContext) {
  // The session refuses a non-Premium account, so it answers the plan check.
  if let Err(reason) = session::current().await {
    info!("native streaming not started: {}", reason.message());
    let mut app = ctx.app.lock().await;
    if let Some(message) = native_streaming_refusal(reason, app.strings()) {
      app.set_status_message(message, 12);
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
      let mut app = ctx.app.lock().await;
      let message = app.strings().status.runtime.native_start_failed;
      app.set_status_message(message, 12);
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
  let published = {
    let mut app_mut = ctx.app.lock().await;
    if app_mut.io_tx_clone().is_none() {
      drop(app_mut);
      streaming_player.shutdown();
      return;
    }
    let published = app_mut.streaming_player.clone();
    if published.is_none() {
      app_mut.streaming_player = Some(Arc::clone(&streaming_player));
      // Startup playlist loading may have fallen back to a flat list while the
      // deferred player was unavailable. Refresh once so rootlist folders are
      // reconciled now that librespot is ready.
      app_mut.dispatch(IoEvent::GetPlaylists);
    }
    published
  };

  let streaming_player = match published {
    // Recovery already published a player and its event handler: overwriting
    // it would leave a ghost Connect device. The device decision still runs.
    Some(published) => {
      streaming_player.shutdown();
      published
    }
    None => {
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
      streaming_player
    }
  };

  // Auto-select the saved playback device when available (fallback to native
  // streaming). This used to run inline in the network task before the pump
  // started; the decision's outcome now dispatches through the pump.
  let device_name = streaming_player.device_name().to_string();
  let device_id = streaming_player.device_id();
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

  let strings = ctx.app.lock().await.strings();
  let startup_decision = startup_device_decision(
    ctx.device_startup_behavior,
    saved_device_id,
    devices_snapshot.as_deref(),
    &device_name,
    &device_id,
    strings,
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
    assert_eq!(
      native_streaming_refusal(Unavailable::NoLogin, &crate::core::i18n::EN),
      None
    );
    assert!(
      native_streaming_refusal(Unavailable::NotPremium, &crate::core::i18n::EN)
        .is_some_and(|message| message.contains("Premium"))
    );
    assert!(native_streaming_refusal(Unavailable::Offline, &crate::core::i18n::EN).is_some());
    assert!(
      native_streaming_refusal(Unavailable::TimedOut, &crate::core::i18n::EN)
        .is_some_and(|message| message.contains("timed out"))
    );
  }

  fn startup_pending_app() -> (App, u64) {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_backend_pending = true;
    let generation = app.begin_native_recovery();
    (app, generation)
  }

  #[test]
  fn startup_pending_window_clears_while_it_still_owns_the_flag() {
    let (mut app, generation) = startup_pending_app();
    assert!(super::finish_startup_pending(&mut app, generation));
    assert!(!app.native_backend_pending);
  }

  #[test]
  fn startup_pending_window_yields_to_a_newer_recovery() {
    let (mut app, generation) = startup_pending_app();
    // A transport drop during init takes over the pending window.
    let newer = app.begin_native_recovery();
    app.native_backend_pending = true;
    assert_ne!(newer, generation);

    assert!(!super::finish_startup_pending(&mut app, generation));
    assert!(
      app.native_backend_pending,
      "the newer recovery still owns the flag"
    );

    // Its own cleanup still ends the window.
    assert!(super::finish_startup_pending(&mut app, newer));
    assert!(!app.native_backend_pending);
  }

  #[cfg(all(
    feature = "soloist",
    not(all(target_os = "macos", target_arch = "x86_64"))
  ))]
  #[test]
  fn a_soloist_switch_keeps_its_pending_flag_and_skips_startup_replay() {
    use crate::core::user_config::{PlaybackEngine, UserConfig};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_backend_pending = true;
    let generation = app.begin_native_recovery();

    // Soloist takes over the pending flag without bumping the recovery generation.
    app.user_config.behavior.enable_streaming = true;
    app.user_config.behavior.playback_engine = PlaybackEngine::Soloist;
    assert!(app.apply_playback_engine().is_some());
    assert!(app.native_backend_pending);
    assert_eq!(app.native_recovery_generation(), generation);

    assert!(!super::finish_startup_pending(&mut app, generation));
    assert!(
      app.native_backend_pending,
      "Soloist owns the pending flag now"
    );
  }
}
