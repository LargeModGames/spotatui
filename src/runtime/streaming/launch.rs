//! The credential gate and the deferred librespot bring-up.

use super::{startup_device_decision, StartupDeviceEvent};
use crate::core::app::App;
use crate::core::config::ClientConfig;
use crate::core::onboarding::Onboarding;
use crate::core::user_config::StartupBehavior;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::requests::spotify_get_typed_compat_for_with_refresh;
use crate::infra::network::IoEvent;
use crate::infra::player;
use log::{info, warn};
use rspotify::{model::user::PrivateUser, AuthCodePkceSpotify};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicU64, Arc};
use std::time::Duration;
use tokio::sync::Mutex;

fn subscription_level_label(level: rspotify::model::SubscriptionLevel) -> &'static str {
  match level {
    rspotify::model::SubscriptionLevel::Premium => "premium",
    rspotify::model::SubscriptionLevel::Free => "free",
  }
}

/// Can run with the UI already up, so outcomes are reported through `info!` and
/// the returned status message only, never `println!`, which would corrupt the
/// TUI. Reuses the `/me` captured during token validation when available
/// instead of paying a second round trip.
async fn account_supports_native_streaming(
  spotify: &AuthCodePkceSpotify,
  cached_me: Option<PrivateUser>,
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
) -> (bool, Option<&'static str>) {
  let user_result = match cached_me {
    Some(user) => Ok(user),
    None => {
      spotify_get_typed_compat_for_with_refresh::<PrivateUser>(
        spotify,
        "me",
        &[],
        token_cache_path,
        app,
      )
      .await
    }
  };
  match user_result {
    #[allow(deprecated)]
    Ok(user) => match user.product {
      Some(rspotify::model::SubscriptionLevel::Premium) => (true, None),
      Some(level) => {
        let plan = subscription_level_label(level);
        info!(
          "spotify {} account detected: playback is unavailable (native streaming and Web API playback controls require premium)",
          plan
        );
        (
          false,
          Some("Spotify Free account: playback controls unavailable (Premium required)"),
        )
      }
      None => {
        info!("spotify account level unknown: native streaming disabled to avoid librespot exit");
        (
          false,
          Some("Could not verify Spotify plan: native streaming disabled"),
        )
      }
    },
    Err(e) => {
      info!(
        "spotify account level check failed ({}); native streaming disabled to avoid librespot exit",
        e
      );
      (
        false,
        Some("Could not verify Spotify plan: native streaming disabled"),
      )
    }
  }
}

/// Runs before the frontend starts: the OAuth flow blocks on a browser round trip.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub(crate) async fn cache_streaming_credentials(
  client_config: &ClientConfig,
  spotify: Option<&AuthCodePkceSpotify>,
  cached_me: Option<&PrivateUser>,
  token_cache_path: &Path,
  onboarding: &Arc<dyn Onboarding>,
  app: &Arc<Mutex<App>>,
) {
  if !client_config.enable_streaming || player::streaming_credentials_are_cached().unwrap_or(false)
  {
    return;
  }
  let Some(spotify) = spotify else {
    return;
  };
  let (supported, status_message) =
    account_supports_native_streaming(spotify, cached_me.cloned(), token_cache_path, app).await;
  if let Some(message) = status_message {
    app.lock().await.set_status_message(message, 12);
  }
  if !supported {
    return;
  }
  // The OAuth flow spins up a blocking local callback server and waits on
  // the browser; keep it off the async reactor so it never ties up a
  // worker thread while the user completes sign-in.
  let onboarding_for_streaming = onboarding.clone();
  let cached = tokio::task::spawn_blocking(move || {
    player::ensure_streaming_credentials_cached(onboarding_for_streaming.as_ref())
  })
  .await
  .unwrap_or_else(|e| Err(anyhow::anyhow!("credential caching task panicked: {e}")));
  if let Err(error) = cached {
    warn!("native streaming authentication unavailable: {error}");
    // Name the actual failure. The generic message left issue #414's
    // reporter with a browser "unable to connect" page, a login that
    // repeated every launch, and nothing on screen or in reach that
    // said which half of startup had failed or why.
    app.lock().await.set_status_message(
      format!("Native streaming authentication failed ({error}); using Spotify Connect."),
      10,
    );
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
  pub(crate) spotify: AuthCodePkceSpotify,
  pub(crate) cached_me: Option<PrivateUser>,
  pub(crate) token_cache_path: PathBuf,
  pub(crate) client_config: ClientConfig,
  pub(crate) redirect_uri: String,
  pub(crate) volume_percent: u8,
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
    // The Spotify startup Play/Pause runs here, after the device decision:
    // before init was deferred, the device transfer always completed first,
    // and firing these earlier 404s with NO_ACTIVE_DEVICE straight onto the
    // Error screen. A request the user parked during init takes precedence
    // over the configured startup behavior — their intent is newer.
    if app.pending_start_playback.is_none() {
      match spotify_startup_behavior {
        Some(StartupBehavior::Play) => {
          app.dispatch(IoEvent::Shuffle(initial_shuffle_enabled));
          app.dispatch(IoEvent::StartPlayback(None, None, None));
        }
        Some(StartupBehavior::Pause) => {
          app.dispatch(IoEvent::PausePlayback);
        }
        Some(StartupBehavior::Continue) | None => {}
      }
    }
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
    client_config: ctx.client_config.clone(),
    redirect_uri: ctx.redirect_uri.clone(),
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    mpris_manager: ctx.mpris_manager.clone(),
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    macos_media_manager: ctx.macos_media_manager.clone(),
    #[cfg(all(feature = "windows-media", target_os = "windows"))]
    windows_media_manager: ctx.windows_media_manager.clone(),
  });

  let (supported, status_message) =
    account_supports_native_streaming(&ctx.spotify, ctx.cached_me, &ctx.token_cache_path, &ctx.app)
      .await;
  if let Some(message) = status_message {
    ctx.app.lock().await.set_status_message(message, 12);
  }
  if !supported {
    return;
  }

  info!("initializing native streaming player");
  let streaming_config = player::StreamingConfig {
    device_name: ctx.client_config.streaming_device_name.clone(),
    bitrate: ctx.client_config.streaming_bitrate,
    audio_cache: ctx.client_config.streaming_audio_cache,
    cache_path: player::get_default_cache_path(),
    initial_volume: ctx.volume_percent,
  };
  let client_id = ctx.client_config.client_id.clone();
  let redirect_uri = ctx.redirect_uri;

  // Internal Spirc timeout defaults to 30s (configurable via
  // SPOTATUI_STREAMING_INIT_TIMEOUT_SECS). The outer timeout here is a safety net
  // that catches hangs *outside* Spirc init (e.g. OAuth callback never arriving,
  // blocking I/O in credential retrieval). Set it above the internal timeout.
  let internal_timeout_secs: u64 = std::env::var("SPOTATUI_STREAMING_INIT_TIMEOUT_SECS")
    .ok()
    .and_then(|v| v.parse().ok())
    .filter(|&v: &u64| v > 0)
    .unwrap_or(30);
  let outer_timeout = Duration::from_secs(internal_timeout_secs.saturating_add(15));

  let init_task = tokio::spawn(async move {
    player::StreamingPlayer::new_cache_only(&client_id, &redirect_uri, streaming_config).await
  });
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
  let saved_device_id = ctx.client_config.device_id.clone();
  let mut devices_snapshot = None;
  if let Ok(devices) =
    spotify_get_typed_compat_for_with_refresh::<rspotify::model::device::DevicePayload>(
      &ctx.spotify,
      "me/player/devices",
      &[],
      &ctx.token_cache_path,
      &ctx.app,
    )
    .await
  {
    let devices_vec = devices.devices;
    let mut app_mut = ctx.app.lock().await;
    app_mut.devices = Some(rspotify::model::device::DevicePayload {
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
