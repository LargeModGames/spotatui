//! Launching the interactive frontend: OS media integrations, the deferred
//! native-streaming startup, persisted-session restore, and the handoff to
//! the terminal UI's event loop.

use super::bootstrap::Boot;
use super::pump::start_tokio;
use crate::core::app::App;
#[cfg(feature = "streaming")]
use crate::core::config::ClientConfig;
use crate::core::user_config::StartupBehavior;
#[cfg(feature = "discord-rpc")]
use crate::core::user_config::UserConfig;
#[cfg(feature = "discord-rpc")]
use crate::infra::discord_rpc;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
#[cfg(feature = "streaming")]
use crate::infra::network::requests::spotify_get_typed_compat_for_with_refresh;
use crate::infra::network::{IoEvent, Network};
#[cfg(feature = "streaming")]
use crate::infra::player;
use anyhow::Result;
use log::info;
#[cfg(feature = "streaming")]
use log::warn;
#[cfg(feature = "streaming")]
use rspotify::{model::user::PrivateUser, AuthCodePkceSpotify};
#[cfg(feature = "streaming")]
use std::path::Path;
#[cfg(feature = "streaming")]
use std::path::PathBuf;
use std::sync::{atomic::AtomicU64, Arc};
// Used by the streaming OAuth timeout and by `restore_playback_session`'s
// per-source position seeks.
#[cfg(any(
  feature = "streaming",
  feature = "local-files",
  feature = "subsonic",
  feature = "youtube"
))]
use std::time::Duration;
use tokio::sync::Mutex;

#[cfg(feature = "discord-rpc")]
type DiscordRpcHandle = Option<discord_rpc::DiscordRpcManager>;
#[cfg(not(feature = "discord-rpc"))]
type DiscordRpcHandle = Option<()>;

#[cfg(feature = "discord-rpc")]
const DEFAULT_DISCORD_CLIENT_ID: &str = "1464235043462447166";

#[cfg(all(feature = "macos-media", target_os = "macos"))]
#[derive(Default, PartialEq)]
struct MacosMetadata {
  title: String,
  artists: Vec<String>,
  album: String,
  duration_ms: u32,
  art_url: Option<String>,
}

#[cfg(all(feature = "windows-media", target_os = "windows"))]
#[derive(Default, PartialEq)]
struct WindowsMetadata {
  title: String,
  artists: Vec<String>,
  album: String,
  duration: u64,
  art_url: Option<String>,
}

#[cfg(feature = "discord-rpc")]
fn resolve_discord_app_id(user_config: &UserConfig) -> Option<String> {
  std::env::var("SPOTATUI_DISCORD_APP_ID")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .or_else(|| user_config.behavior.discord_rpc_client_id.clone())
    .or_else(|| Some(DEFAULT_DISCORD_CLIENT_ID.to_string()))
}

#[cfg(all(feature = "macos-media", target_os = "macos"))]
fn update_macos_metadata(
  manager: &macos_media::MacMediaManager,
  last_metadata: &mut Option<MacosMetadata>,
  app: &App,
) {
  // Local-file playback owns its own state and never populates the Spotify
  // playback context, so Now Playing must read metadata, play state, and
  // position straight from the live local player when local is active. Skipped
  // while the native queue owns the sink: `local_playback` is then a *suspended*
  // context, so fall through to the snapshot path, which describes the queued
  // track actually playing. Mirrors the same filter on the MPRIS twin in
  // `update_mpris_state` in the TUI runner.
  #[cfg(feature = "local-files")]
  if let Some(local) = app
    .local_playback
    .as_ref()
    .filter(|_| !app.queue_owns_playback())
  {
    use crate::infra::media_metadata::{select_media_metadata, LocalMediaMetadata};

    let is_playing = !local.player.is_paused();
    let position_ms = local.player.position().as_millis() as u64;

    // `select_media_metadata` is the single, unit-tested decision for which
    // source the OS integration follows; local always wins while it is active.
    let metadata = select_media_metadata(
      Some(LocalMediaMetadata {
        title: local.name.clone(),
        artists: vec![local.artists.clone()],
        album: local.album.clone(),
        duration_ms: local.duration_ms as u32,
      }),
      None,
    )
    .expect("local metadata is present");

    let new_metadata = MacosMetadata {
      title: metadata.title.clone(),
      artists: metadata.artists.clone(),
      album: metadata.album.clone(),
      duration_ms: metadata.duration_ms,
      art_url: metadata.image_url.clone(),
    };

    if last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(
        &metadata.title,
        &metadata.artists,
        &metadata.album,
        metadata.duration_ms,
        metadata.image_url,
      );
      *last_metadata = Some(new_metadata);
    }

    manager.set_playback_status(is_playing);
    manager.set_position(position_ms);
    return;
  }

  if let Some(snapshot) = crate::infra::media_metadata::current_playback_snapshot(app) {
    let new_metadata = MacosMetadata {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      duration_ms: snapshot.metadata.duration_ms,
      art_url: snapshot.metadata.image_url.clone(),
    };

    // Only update if metadata changed to avoid repeated artwork fetches.
    if last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(
        &snapshot.metadata.title,
        &snapshot.metadata.artists,
        &snapshot.metadata.album,
        snapshot.metadata.duration_ms,
        snapshot.metadata.image_url,
      );
      *last_metadata = Some(new_metadata);
    }
  } else if last_metadata.is_some() {
    *last_metadata = None;
  }
}

#[cfg(all(feature = "windows-media", target_os = "windows"))]
fn update_windows_metadata(
  manager: &smtc_tokio::WindowsMediaManager,
  last_metadata: &mut Option<WindowsMetadata>,
  app: &App,
) {
  if let Some(snapshot) = crate::infra::media_metadata::current_playback_snapshot(app) {
    let new_metadata = WindowsMetadata {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      duration: snapshot.metadata.duration_ms as u64,
      art_url: snapshot.metadata.image_url.clone(),
    };

    if last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(
        &snapshot.metadata.title,
        &snapshot.metadata.artists,
        &snapshot.metadata.album,
        snapshot.metadata.duration_ms as u64,
        snapshot.metadata.image_url,
      );
      *last_metadata = Some(new_metadata);
    }
  } else if last_metadata.is_some() {
    *last_metadata = None;
  }
}

#[cfg(feature = "streaming")]
fn subscription_level_label(level: rspotify::model::SubscriptionLevel) -> &'static str {
  match level {
    rspotify::model::SubscriptionLevel::Premium => "premium",
    rspotify::model::SubscriptionLevel::Free => "free",
  }
}

/// Runs after the UI is up (see `deferred_streaming_startup`), so outcomes are
/// reported via `info!` + the returned status message only — never `println!`,
/// which would corrupt the TUI. Reuses the `/me` captured during token
/// validation when available instead of paying a second round trip.
#[cfg(feature = "streaming")]
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

#[cfg(any(feature = "streaming", test))]
#[derive(Debug, PartialEq, Eq)]
enum StartupDeviceEvent {
  Transfer {
    device_id: String,
    persist_device_id: bool,
  },
  AutoSelectStreaming {
    device_name: String,
    persist_device_id: bool,
  },
}

#[cfg(any(feature = "streaming", test))]
#[derive(Debug, PartialEq, Eq)]
struct StartupDeviceDecision {
  event: Option<StartupDeviceEvent>,
  status_message: Option<String>,
}

#[cfg(feature = "streaming")]
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

#[cfg(any(feature = "streaming", test))]
fn startup_device_decision(
  startup_behavior: StartupBehavior,
  saved_device_id: Option<String>,
  devices_snapshot: Option<&[rspotify::model::device::Device]>,
  native_device_name: &str,
) -> StartupDeviceDecision {
  if startup_behavior != StartupBehavior::Play {
    return StartupDeviceDecision {
      event: None,
      status_message: None,
    };
  }

  let event = match saved_device_id {
    Some(saved_device_id) => {
      if let Some(devices) = devices_snapshot {
        let mut saved_device_available = false;
        let mut native_device_id = None;

        for device in devices {
          if device.id.as_ref() == Some(&saved_device_id) {
            saved_device_available = true;
            break;
          }

          if native_device_id.is_none() && device.name.eq_ignore_ascii_case(native_device_name) {
            native_device_id = device.id.clone();
          }
        }

        if saved_device_available {
          Some(StartupDeviceEvent::Transfer {
            device_id: saved_device_id,
            persist_device_id: true,
          })
        } else {
          native_device_id.map_or_else(
            || {
              Some(StartupDeviceEvent::AutoSelectStreaming {
                device_name: native_device_name.to_string(),
                persist_device_id: false,
              })
            },
            |device_id| {
              Some(StartupDeviceEvent::Transfer {
                device_id,
                persist_device_id: false,
              })
            },
          )
        }
      } else {
        Some(StartupDeviceEvent::Transfer {
          device_id: saved_device_id,
          persist_device_id: true,
        })
      }
    }
    None => Some(StartupDeviceEvent::AutoSelectStreaming {
      device_name: native_device_name.to_string(),
      persist_device_id: true,
    }),
  };

  let status_message = matches!(
    event,
    Some(
      StartupDeviceEvent::Transfer {
        persist_device_id: false,
        ..
      } | StartupDeviceEvent::AutoSelectStreaming {
        persist_device_id: false,
        ..
      }
    )
  )
  .then(|| format!("Saved device unavailable; using {}", native_device_name));

  StartupDeviceDecision {
    event,
    status_message,
  }
}

/// Everything native streaming needs that used to gate the first frame:
/// account probe, librespot session handshake, player event handler, and the
/// saved-device startup decision. Bundled so `deferred_streaming_startup` can
/// run it all on a background task after the UI is already up.
#[cfg(feature = "streaming")]
struct DeferredStreamingContext {
  app: Arc<Mutex<App>>,
  spotify: AuthCodePkceSpotify,
  cached_me: Option<PrivateUser>,
  token_cache_path: PathBuf,
  client_config: ClientConfig,
  redirect_uri: String,
  volume_percent: u8,
  device_startup_behavior: StartupBehavior,
  /// Spotify startup Play/Pause, run after the device decision so it lands on
  /// the selected device instead of 404ing with NO_ACTIVE_DEVICE while init is
  /// still in flight. `None` when a non-Spotify session restore owns startup.
  spotify_startup_behavior: Option<StartupBehavior>,
  initial_shuffle_enabled: bool,
  recovery_tx: tokio::sync::mpsc::UnboundedSender<player::StreamingRecoveryRequest>,
  shared_position: Arc<AtomicU64>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
}

/// Initialize native streaming in the background (D1). The UI renders its
/// first frame immediately; this task probes the account (reusing the auth
/// `/me` when available), performs the librespot handshake with the same
/// double-timeout as before, stores the player in `App`, spawns the player
/// event handler, and finally makes the saved-device startup decision —
/// dispatching its outcome through the normal IoEvent pump.
#[cfg(feature = "streaming")]
fn deferred_streaming_startup(ctx: DeferredStreamingContext) {
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

#[cfg(feature = "streaming")]
async fn deferred_streaming_startup_inner(ctx: DeferredStreamingContext) {
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
  let redirect_uri = ctx.redirect_uri.clone();

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

/// Launch the terminal UI: OS media integrations, the deferred native
/// streaming startup, persisted-session restore, the network task driving
/// the IoEvent pump, and finally the blocking UI event loop.
pub(super) async fn launch_ui(boot: Boot) -> Result<()> {
  let app = boot.app;
  let sync_io_rx = boot.sync_io_rx;
  let user_config = boot.user_config;
  let client_config = boot.client_config;
  let spotify = boot.spotify;
  let final_token_cache_path = boot.token_cache_path;
  let restore_playback = boot.restore_playback;
  let restore_queue = boot.restore_queue;
  let initial_shuffle_enabled = boot.initial_shuffle_enabled;
  let initial_startup_behavior = boot.initial_startup_behavior;
  let client_id_notice_message = boot.client_id_notice_message;
  #[cfg(feature = "streaming")]
  let runtime_state = boot.runtime_state;
  #[cfg(feature = "streaming")]
  let onboarding = boot.onboarding;
  #[cfg(feature = "streaming")]
  let cached_me = boot.cached_me;
  #[cfg(feature = "streaming")]
  let selected_redirect_uri = boot.selected_redirect_uri;

  info!("launching interactive terminal ui");
  #[cfg(feature = "streaming")]
  if client_config.enable_streaming && !player::streaming_credentials_are_cached().unwrap_or(false)
  {
    if let Some(spotify) = spotify.as_ref() {
      let (supported, status_message) = account_supports_native_streaming(
        spotify,
        cached_me.clone(),
        &final_token_cache_path,
        &app,
      )
      .await;
      if let Some(message) = status_message {
        app.lock().await.set_status_message(message, 12);
      }
      if supported {
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
    }
  }
  // Shown here rather than at authentication time: the streaming credential
  // flow above can block on a browser round trip, which would burn the
  // message's TTL before the first frame ever renders. Never displaces a
  // message something else just set (they are more urgent than this notice).
  if let Some(message) = client_id_notice_message {
    let mut app_mut = app.lock().await;
    if app_mut.status_message.is_none() {
      app_mut.set_status_message(message, 15);
    }
  }

  let history_collector = crate::infra::history::spawn_history_collector(Arc::clone(&app));

  // Opt-in MCP control socket. Nothing listens unless the user asked for it;
  // a bind failure is reported and shrugged off rather than blocking startup,
  // since the player is perfectly usable without it.
  #[cfg(feature = "mcp-server")]
  {
    let (enabled, io_tx) = {
      let app_ref = app.lock().await;
      (
        app_ref.user_config.behavior.mcp_enabled,
        app_ref.io_tx_clone(),
      )
    };
    if enabled {
      match io_tx {
        Some(io_tx) => match crate::infra::mcp::spawn_listener(Arc::clone(&app), io_tx).await {
          Ok(port) => {
            let mut app_mut = app.lock().await;
            if app_mut.status_message.is_none() {
              app_mut.set_status_message(format!("MCP server listening on 127.0.0.1:{port}"), 6);
            }
          }
          Err(e) => {
            log::warn!("MCP: could not start the control socket: {e}");
            let mut app_mut = app.lock().await;
            app_mut.set_error_status_message(format!("MCP server failed to start: {e}"), 8);
          }
        },
        None => log::warn!("MCP: no IoEvent sender available; control socket not started"),
      }
    }
  }
  // Native streaming needs a Spotify session; when it will be attempted, the
  // account probe and librespot handshake run in a background task after the
  // UI is up (see `deferred_streaming_startup`) instead of gating the first
  // frame for seconds (worst case tens of seconds).
  #[cfg(feature = "streaming")]
  let streaming_attempted = client_config.enable_streaming && spotify.is_some();

  // Create shared atomic for real-time position updates from native player
  // This avoids lock contention - the player event handler can update position
  // without needing to acquire the app mutex
  #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
  let shared_position = Arc::new(AtomicU64::new(0));
  #[cfg(feature = "streaming")]
  let shared_position_for_events = Arc::clone(&shared_position);
  #[cfg(feature = "streaming")]
  let shared_position_for_ui = Arc::clone(&shared_position);

  // Create shared atomic for playing state (lock-free for MPRIS toggle)
  #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
  let shared_is_playing = Arc::new(std::sync::atomic::AtomicBool::new(false));
  #[cfg(feature = "streaming")]
  let shared_is_playing_for_events = Arc::clone(&shared_is_playing);
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let shared_is_playing_for_mpris = Arc::clone(&shared_is_playing);
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let shared_position_for_mpris = Arc::clone(&shared_position);
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  let shared_is_playing_for_macos = Arc::clone(&shared_is_playing);
  #[cfg(feature = "streaming")]
  let (streaming_recovery_tx, streaming_recovery_rx) =
    tokio::sync::mpsc::unbounded_channel::<player::StreamingRecoveryRequest>();
  #[cfg(feature = "streaming")]
  {
    let mut app_mut = app.lock().await;
    app_mut.streaming_recovery_tx = Some(streaming_recovery_tx.clone());
  }

  // Initialize MPRIS D-Bus integration for desktop media control
  // This registers spotatui as a controllable media player on the session bus
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mpris_manager: Option<Arc<mpris::MprisManager>> = match mpris::MprisManager::new() {
    Ok(mgr) => {
      info!("mpris d-bus interface registered - media keys and playerctl enabled");
      Some(Arc::new(mgr))
    }
    Err(e) => {
      info!(
        "failed to initialize mpris: {} - media key control disabled",
        e
      );
      None
    }
  };

  // Store MPRIS manager reference in App for emitting Seeked signals from native seeks
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  {
    let mut app_mut = app.lock().await;
    app_mut.mpris_manager = mpris_manager.clone();
  }

  // Initialize macOS Now Playing integration for media key control
  // This registers with MPRemoteCommandCenter for media key events
  // Gated on whether streaming will be attempted (the player itself now
  // initializes in the background): registering media keys for a session
  // whose native init later fails is harmless — the handlers just no-op.
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  let macos_media_manager: Option<Arc<macos_media::MacMediaManager>> = if streaming_attempted {
    match macos_media::MacMediaManager::new() {
      Ok(mgr) => {
        info!("macos now playing interface registered - media keys enabled");
        Some(Arc::new(mgr))
      }
      Err(e) => {
        info!(
          "failed to initialize macos media control: {} - media keys disabled",
          e
        );
        None
      }
    }
  } else {
    None
  };

  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  let windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>> = if streaming_attempted {
    match smtc_tokio::WindowsMediaManager::new() {
      Ok(mgr) => {
        info!("windows smtc com registered - media keys enabled");
        Some(Arc::new(mgr))
      }
      Err(e) => {
        info!(
          "failed to initialize windows smtc com: {} - media keys disabled",
          e
        );
        None
      }
    }
  } else {
    None
  };

  #[cfg(feature = "discord-rpc")]
  let discord_rpc_manager: DiscordRpcHandle = if user_config.behavior.enable_discord_rpc {
    match resolve_discord_app_id(&user_config)
      .and_then(|app_id| discord_rpc::DiscordRpcManager::new(app_id).ok())
    {
      Some(mgr) => {
        info!("discord rich presence enabled");
        Some(mgr)
      }
      None => {
        info!("discord rich presence failed to initialize");
        None
      }
    }
  } else {
    info!("discord rich presence disabled");
    None
  };
  #[cfg(not(feature = "discord-rpc"))]
  let discord_rpc_manager: DiscordRpcHandle = None;

  // Spawn MPRIS event handler to process external control requests (media keys, playerctl)
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  if let Some(ref mpris) = mpris_manager {
    if let Some(event_rx) = mpris.take_event_rx() {
      let mpris_for_seek = Arc::clone(mpris);
      let app_for_mpris = Arc::clone(&app);
      tokio::spawn(async move {
        handle_mpris_events(
          event_rx,
          shared_is_playing_for_mpris,
          shared_position_for_mpris,
          mpris_for_seek,
          app_for_mpris,
        )
        .await;
      });
    }
  }

  // Spawn macOS media event handler to process external control requests (media keys, Control Center)
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  if let Some(ref macos_media) = macos_media_manager {
    if let Some(event_rx) = macos_media.take_event_rx() {
      let app_for_macos = Arc::clone(&app);
      tokio::spawn(async move {
        handle_macos_media_events(event_rx, app_for_macos, shared_is_playing_for_macos).await;
      });
    }
  }

  // Keep Now Playing metadata (including artwork URL from Web API playback state)
  // synchronized with Control Center.
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  if let Some(ref macos_media) = macos_media_manager {
    let macos_media_for_metadata = Arc::clone(macos_media);
    let app_for_macos_metadata = Arc::clone(&app);
    tokio::spawn(async move {
      let mut last_metadata: Option<MacosMetadata> = None;
      let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

      loop {
        interval.tick().await;
        if let Ok(app) = app_for_macos_metadata.try_lock() {
          update_macos_metadata(&macos_media_for_metadata, &mut last_metadata, &app);
        }
      }
    });
  }

  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  if let Some(ref windows_media) = windows_media_manager {
    if let Some(event_rx) = windows_media.take_event_rx() {
      let app_for_windows = Arc::clone(&app);
      tokio::spawn(async move {
        handle_windows_media_events(event_rx, app_for_windows).await;
      });
    }
  }

  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  if let Some(ref windows_media) = windows_media_manager {
    let windows_media_for_metadata = Arc::clone(windows_media);
    let app_for_windows_metadata = Arc::clone(&app);
    tokio::spawn(async move {
      let mut last_metadata: Option<WindowsMetadata> = None;
      let mut last_playing: Option<bool> = None; // Track play state
      let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

      loop {
        interval.tick().await;
        if let Ok(app) = app_for_windows_metadata.try_lock() {
          update_windows_metadata(&windows_media_for_metadata, &mut last_metadata, &app);
          let is_playing = if app.native_track_info.is_some() {
            app.native_is_playing.unwrap_or(false)
          } else {
            app
              .current_playback_context
              .as_ref()
              .map(|c| c.is_playing)
              .unwrap_or(false)
          };

          if app.native_track_info.is_none() {
            if last_playing != Some(is_playing) {
              windows_media_for_metadata.set_playback_status(is_playing);
              last_playing = Some(is_playing);
            }
            windows_media_for_metadata.set_position(app.song_progress_ms as u64);
          } else {
            last_playing = Some(is_playing);
          }
        }
      }
    });
  }

  // Clone MPRIS manager for player event handler
  #[cfg(all(feature = "streaming", feature = "mpris", target_os = "linux"))]
  let mpris_for_events = mpris_manager.clone();

  // Clone macOS media manager for player event handler
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  let macos_media_for_events = macos_media_manager.clone();

  // Clone MPRIS manager for UI loop (to update status on device changes)
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mpris_for_ui = mpris_manager.clone();

  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  let windows_media_for_events = windows_media_manager.clone();

  // Kick off the deferred native-streaming startup: account probe, librespot
  // handshake, player event handler, and saved-device startup decision all
  // run on a background task while the UI renders (see
  // `deferred_streaming_startup`).
  #[cfg(feature = "streaming")]
  if streaming_attempted {
    // When resuming a non-Spotify session, never transfer Spotify playback
    // to a device on startup — that would fight the restored source for the
    // audio output. Treat the device decision as passive (Continue); the
    // device list is still fetched for the UI.
    let device_startup_behavior = if restore_playback.is_some() {
      StartupBehavior::Continue
    } else {
      initial_startup_behavior
    };
    // While init is running, a playback request that finds no active device
    // parks itself for replay instead of erroring (the task clears this and
    // replays whatever parked when it finishes, whatever the outcome).
    app.lock().await.native_backend_pending = true;
    deferred_streaming_startup(DeferredStreamingContext {
      app: Arc::clone(&app),
      spotify: spotify
        .clone()
        .expect("streaming_attempted implies a Spotify session"),
      cached_me,
      token_cache_path: final_token_cache_path.clone(),
      client_config: client_config.clone(),
      redirect_uri: selected_redirect_uri.clone(),
      volume_percent: runtime_state.volume_percent,
      device_startup_behavior,
      // A restored non-Spotify session owns the startup play/pause decision;
      // otherwise the deferred task fires it once the device is selected.
      spotify_startup_behavior: if restore_playback.is_some() {
        None
      } else {
        Some(initial_startup_behavior)
      },
      initial_shuffle_enabled,
      recovery_tx: streaming_recovery_tx.clone(),
      shared_position: shared_position_for_events,
      shared_is_playing: shared_is_playing_for_events,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: mpris_for_events,
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      macos_media_manager: macos_media_for_events,
      #[cfg(all(feature = "windows-media", target_os = "windows"))]
      windows_media_manager: windows_media_for_events,
    });
  }

  #[cfg(feature = "streaming")]
  {
    player::spawn_streaming_recovery_handler(player::StreamingRecoveryContext {
      app: Arc::clone(&app),
      shared_position: Arc::clone(&shared_position),
      shared_is_playing: Arc::clone(&shared_is_playing),
      recovery_rx: streaming_recovery_rx,
      recovery_tx: streaming_recovery_tx.clone(),
      client_config: client_config.clone(),
      redirect_uri: selected_redirect_uri.clone(),
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: mpris_manager.clone(),
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      macos_media_manager: macos_media_manager.clone(),
      #[cfg(all(feature = "windows-media", target_os = "windows"))]
      windows_media_manager: windows_media_manager.clone(),
    });
  }

  let cloned_app = Arc::clone(&app);
  info!("spawning spotify network event handler");
  tokio::spawn(async move {
    let mut network = Network::new(spotify, client_config, &app, final_token_cache_path);

    // The saved-device startup decision moved into
    // `deferred_streaming_startup` (it needs the native player's device
    // name, which now materializes in the background).

    // Resume a persisted non-Spotify session if there is one; it honors the
    // startup behavior for its own play/pause decision. Otherwise fall back to
    // the Spotify startup play behavior. Continue is passive and must not
    // transfer devices, change shuffle, or otherwise activate Spotatui.
    // Restore the persisted native queue into app state before the runner
    // starts, independent of whether a source playback is resumed.
    if !restore_queue.is_empty() {
      network.app.lock().await.native_queue = restore_queue;
    }
    if let Some(session) = restore_playback {
      // Resume off the event pump: a slow source (yt-dlp download, remote
      // fetch) must not stall the Spotify startup events the UI's first render
      // queues (user, playlists, current playback). The restore drives the
      // source's own start path, which serializes on the `App` lock like every
      // other event, so running it concurrently is safe.
      let restore_app = Arc::clone(&network.app);
      tokio::spawn(async move {
        restore_playback_session(&restore_app, session, initial_startup_behavior).await;
      });
    } else if network.spotify.is_some() {
      // Spotify startup play/pause only applies with a Spotify session; a
      // free-source launch has nothing to activate here. When native
      // streaming init is deferred, `deferred_streaming_startup` fires this
      // after the device decision instead — running it here would race the
      // init and 404 with NO_ACTIVE_DEVICE onto the Error screen.
      #[cfg(feature = "streaming")]
      let startup_behavior_runs_here = !streaming_attempted;
      #[cfg(not(feature = "streaming"))]
      let startup_behavior_runs_here = true;
      if startup_behavior_runs_here {
        match initial_startup_behavior {
          StartupBehavior::Continue => {}
          StartupBehavior::Play => {
            network
              .handle_network_event(IoEvent::Shuffle(initial_shuffle_enabled))
              .await;
            network
              .handle_network_event(IoEvent::StartPlayback(None, None, None))
              .await;
          }
          StartupBehavior::Pause => {
            network.handle_network_event(IoEvent::PausePlayback).await;
          }
        }
      }
    }

    start_tokio(sync_io_rx, &mut network).await;
  });
  // The UI must run in the "main" thread
  info!("starting terminal ui event loop");
  #[cfg(feature = "streaming")]
  let shared_pos_for_start_ui: Option<Arc<AtomicU64>> = Some(shared_position_for_ui);
  #[cfg(not(feature = "streaming"))]
  let shared_pos_for_start_ui: Option<Arc<AtomicU64>> = None;
  let ui_result = crate::tui::runner::start_ui(
    user_config,
    &cloned_app,
    shared_pos_for_start_ui,
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    mpris_for_ui,
    #[cfg(not(all(feature = "mpris", target_os = "linux")))]
    None,
    discord_rpc_manager,
    history_collector,
  )
  .await;
  // Unpublish the control file on the way out, whether the UI exited cleanly
  // or not: leaving it behind sends the next `spotatui mcp` at a port nothing
  // is listening on, which reads as a broken MCP server rather than an absent
  // one.
  #[cfg(feature = "mcp-server")]
  crate::infra::mcp::clear_handshake();
  if ui_result.is_err() {
    let mut app = cloned_app.lock().await;
    app.flush_state_save(true);
  }
  ui_result?;

  Ok(())
}

/// Resume a persisted non-Spotify playback session at launch.
///
/// Drives the source's existing, tested start path (seeding the browse table
/// with the persisted metadata so the snapshot resolves, then dispatching the
/// same `StartPlayback` the keyboard would), and afterwards applies the saved
/// position and the play/pause decision. That decision follows `startup_behavior`:
/// `Play` forces playing, `Pause` forces paused, and `Continue` restores the
/// exact state the session had when it was saved.
///
/// A failed start (removed video, dead network, macOS local playback, missing
/// yt-dlp) publishes no session, so the resume step simply finds nothing and
/// no-ops — startup is never blocked or crashed by a stale session. Any variant
/// whose source feature is disabled in this build is a no-op.
#[allow(unused_variables)]
async fn restore_playback_session(
  app: &Arc<Mutex<App>>,
  session: crate::core::persisted_playback::PersistedPlayback,
  startup_behavior: StartupBehavior,
) {
  #[cfg(any(
    feature = "youtube",
    feature = "subsonic",
    feature = "local-files",
    feature = "internet-radio"
  ))]
  use crate::core::persisted_playback::PersistedPlayback;

  // Resolve whether the restored track should end up paused.
  let resolve_paused = |saved_paused: bool| match startup_behavior {
    StartupBehavior::Play => false,
    StartupBehavior::Pause => true,
    StartupBehavior::Continue => saved_paused,
  };

  match session {
    #[cfg(feature = "local-files")]
    PersistedPlayback::Local {
      queue,
      index,
      position_ms,
      paused,
      repeat,
      shuffle_on,
      shuffle,
    } => {
      if queue.is_empty() {
        return;
      }
      // Local reads tags from disk, so the URI queue alone drives the start.
      crate::infra::local::dispatch::route_local_event(
        app,
        &IoEvent::StartPlayback(None, Some(queue), Some(index)),
      )
      .await;
      let mut guard = app.lock().await;
      // Only adopt the persisted modes if the source actually started; a decode
      // failure would otherwise leave the player-global flags set with no source.
      // Trust the persisted intent (`shuffle_on`) for the flag and restore the
      // backup as-is (the start path above did not re-shuffle because
      // `decoded_shuffle` is still off). When the two disagree — a toggle deferred
      // at exit — the runner's `reconcile_decoded_shuffle` re-syncs the queue
      // order to the flag on the next tick.
      if guard.local_playback.is_some() {
        guard.decoded_repeat = repeat;
        guard.decoded_shuffle = shuffle_on;
      }
      if let Some(s) = guard.local_playback.as_mut() {
        if position_ms > 0 {
          let _ = s.player.seek(Duration::from_millis(position_ms));
        }
        if resolve_paused(paused) {
          s.player.pause();
        }
        s.shuffle_backup = shuffle;
      }
    }
    #[cfg(feature = "subsonic")]
    PersistedPlayback::Subsonic {
      tracks,
      index,
      position_ms,
      paused,
      repeat,
      shuffle_on,
      shuffle,
    } => {
      let uris: Vec<String> = tracks.iter().filter_map(|t| t.uri.clone()).collect();
      if uris.is_empty() {
        return;
      }
      // Seed the browse table so the start path's snapshot resolves metadata.
      app.lock().await.track_table.tracks = tracks;
      crate::infra::subsonic::dispatch::route_subsonic_event(
        app,
        &IoEvent::StartPlayback(None, Some(uris), Some(index)),
      )
      .await;
      let mut guard = app.lock().await;
      if guard.subsonic_playback.is_some() {
        guard.decoded_repeat = repeat;
        guard.decoded_shuffle = shuffle_on;
      }
      if let Some(s) = guard.subsonic_playback.as_mut() {
        if position_ms > 0 {
          let _ = s.player.seek(Duration::from_millis(position_ms));
        }
        if resolve_paused(paused) {
          s.player.pause();
        }
        s.shuffle_backup = shuffle;
      }
    }
    #[cfg(feature = "youtube")]
    PersistedPlayback::YouTube {
      tracks,
      index,
      position_ms,
      paused,
      repeat,
      shuffle_on,
      shuffle,
    } => {
      let uris: Vec<String> = tracks.iter().filter_map(|t| t.uri.clone()).collect();
      if uris.is_empty() {
        return;
      }
      app.lock().await.track_table.tracks = tracks;
      crate::infra::youtube::dispatch::route_youtube_event(
        app,
        &IoEvent::StartPlayback(None, Some(uris), Some(index)),
      )
      .await;
      let mut guard = app.lock().await;
      if guard.youtube_playback.is_some() {
        guard.decoded_repeat = repeat;
        guard.decoded_shuffle = shuffle_on;
      }
      if let Some(s) = guard.youtube_playback.as_mut() {
        if position_ms > 0 {
          let _ = s.player.seek(Duration::from_millis(position_ms));
        }
        if resolve_paused(paused) {
          s.player.pause();
        }
        s.shuffle_backup = shuffle;
      }
    }
    #[cfg(feature = "internet-radio")]
    PersistedPlayback::Radio { station, paused } => {
      let Some(uri) = station.uri.clone() else {
        return;
      };
      // Seed the browse table so the start path's station snapshot resolves.
      app.lock().await.track_table.tracks = vec![station];
      crate::infra::radio::dispatch::route_radio_event(
        app,
        &IoEvent::StartPlayback(Some(uri), None, None),
      )
      .await;
      // A live stream has no seekable position; only apply the pause decision.
      let guard = app.lock().await;
      if let Some(s) = guard.radio_playback.as_ref() {
        if resolve_paused(paused) {
          s.player.pause();
        }
      }
    }
    // Any variant whose source feature is disabled in this build.
    #[allow(unreachable_patterns)]
    _ => {}
  }
}

/// Handle MPRIS events from external clients (media keys, playerctl, etc.)
/// Routes to native streaming player when available, or dispatches IoEvents as fallback
#[cfg(all(feature = "mpris", target_os = "linux"))]
async fn handle_mpris_events(
  mut event_rx: tokio::sync::mpsc::UnboundedReceiver<mpris::MprisEvent>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
  shared_position: Arc<AtomicU64>,
  mpris_manager: Arc<mpris::MprisManager>,
  app: Arc<Mutex<App>>,
) {
  use mpris::MprisEvent;
  #[cfg(feature = "streaming")]
  use std::sync::atomic::Ordering;

  while let Some(event) = event_rx.recv().await {
    if !app.lock().await.user_config.behavior.enable_media_keys {
      continue;
    }

    // A decoded source (local file, Subsonic, radio, or YouTube) owns the
    // session: route transport through the same IoEvents the keyboard uses
    // (intercepted by the per-source route_*_event dispatchers before the
    // Spotify network) so media keys follow the audible source instead of
    // librespot. This must run *before* the streaming-player branches below,
    // since librespot is initialized even while a decoded source is playing.
    #[cfg(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "internet-radio",
      feature = "youtube"
    ))]
    if route_decoded_mpris_event(&event, &app, &mpris_manager).await {
      continue;
    }

    // Dynamically fetch the current active player so MPRIS can target the correct player
    // and not the stale player. The old player can be stale on, e.g., native streaming recovery.
    #[cfg(feature = "streaming")]
    let current_player = {
      let app_lock = app.lock().await;
      app_lock.streaming_player.clone()
    };

    match event {
      MprisEvent::PlayPause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          if shared_is_playing.load(Ordering::Relaxed) {
            player.pause();
          } else {
            player.play();
          }
          continue;
        }
        // Fallback: dispatch IoEvent
        let mut app_lock = app.lock().await;
        let is_playing = app_lock.native_is_playing.unwrap_or_else(|| {
          app_lock
            .current_playback_context
            .as_ref()
            .map(|c| c.is_playing)
            .unwrap_or(false)
        });
        if is_playing {
          app_lock.dispatch(IoEvent::PausePlayback);
        } else {
          app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
        }
      }
      MprisEvent::Play => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          player.play();
          app.lock().await.set_native_playback_intent(true);
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      }
      MprisEvent::Pause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          player.pause();
          app.lock().await.set_native_playback_intent(false);
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      MprisEvent::Next => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          let _ = player;
          app.lock().await.next_track();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::NextTrack);
      }
      MprisEvent::Previous => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          let _ = player;
          app.lock().await.previous_track();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PreviousTrack);
      }
      MprisEvent::Stop => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          player.stop();
          app.lock().await.set_native_playback_intent(false);
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      MprisEvent::Seek(offset_micros) => {
        // MPRIS sends relative offset in microseconds (can be negative for rewind)
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          let current_ms = shared_position.load(Ordering::Relaxed) as i64;
          let offset_ms = offset_micros / 1000;
          let new_position_ms = (current_ms + offset_ms).max(0) as u32;
          player.seek(new_position_ms);
          shared_position.store(new_position_ms as u64, Ordering::Relaxed);
          if let Ok(mut app_lock) = app.try_lock() {
            app_lock.song_progress_ms = new_position_ms as u128;
            app_lock.set_native_recovery_position(new_position_ms);
          }
          mpris_manager.emit_seeked(new_position_ms as u64);
          continue;
        }
        // Fallback: read current position from app, dispatch Seek IoEvent
        let mut app_lock = app.lock().await;
        let current_ms = app_lock.song_progress_ms as i64;
        let offset_ms = offset_micros / 1000;
        let new_position_ms = (current_ms + offset_ms).max(0) as u32;
        app_lock.song_progress_ms = new_position_ms as u128;
        app_lock.dispatch(IoEvent::Seek(new_position_ms));
        drop(app_lock);
        mpris_manager.emit_seeked(new_position_ms as u64);
      }
      MprisEvent::SetPosition(position_micros) => {
        // MPRIS SetPosition sends absolute position in microseconds
        let new_position_ms = (position_micros / 1000).max(0) as u32;
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          player.seek(new_position_ms);
          shared_position.store(new_position_ms as u64, Ordering::Relaxed);
          if let Ok(mut app_lock) = app.try_lock() {
            app_lock.song_progress_ms = new_position_ms as u128;
            app_lock.set_native_recovery_position(new_position_ms);
          }
          mpris_manager.emit_seeked(new_position_ms as u64);
          continue;
        }
        // Fallback: dispatch Seek IoEvent
        let mut app_lock = app.lock().await;
        app_lock.song_progress_ms = new_position_ms as u128;
        app_lock.dispatch(IoEvent::Seek(new_position_ms));
        drop(app_lock);
        mpris_manager.emit_seeked(new_position_ms as u64);
      }
      MprisEvent::SetShuffle(shuffle) => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          if let Err(e) = player.set_shuffle(shuffle) {
            eprintln!("MPRIS: Failed to set shuffle: {}", e);
          } else {
            mpris_manager.set_shuffle(shuffle);
            let mut app_lock = app.lock().await;
            app_lock.set_native_recovery_shuffle(shuffle);
            if let Some(ref mut ctx) = app_lock.current_playback_context {
              ctx.shuffle_state = shuffle;
            }
            app_lock.runtime_state.shuffle_enabled = shuffle;
            app_lock.schedule_state_save(
              crate::core::state::PersistedRuntimeState::shuffle_enabled(shuffle),
            );
          }
          continue;
        }
        // Fallback: dispatch Shuffle IoEvent
        mpris_manager.set_shuffle(shuffle);
        let mut app_lock = app.lock().await;
        if let Some(ref mut ctx) = app_lock.current_playback_context {
          ctx.shuffle_state = shuffle;
        }
        app_lock.runtime_state.shuffle_enabled = shuffle;
        app_lock.schedule_state_save(crate::core::state::PersistedRuntimeState::shuffle_enabled(
          shuffle,
        ));
        app_lock.dispatch(IoEvent::Shuffle(shuffle));
      }
      MprisEvent::SetLoopStatus(loop_status) => {
        use mpris::LoopStatusEvent;
        use rspotify::model::enums::RepeatState;

        let repeat_state = match loop_status {
          LoopStatusEvent::None => RepeatState::Off,
          LoopStatusEvent::Track => RepeatState::Track,
          LoopStatusEvent::Playlist => RepeatState::Context,
        };
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          if let Err(e) = player.set_repeat_mode(repeat_state) {
            eprintln!("MPRIS: Failed to set repeat mode: {}", e);
          } else {
            mpris_manager.set_loop_status(loop_status);
            let mut app_lock = app.lock().await;
            app_lock.set_native_recovery_repeat(repeat_state);
            if let Some(ref mut ctx) = app_lock.current_playback_context {
              ctx.repeat_state = repeat_state;
            }
          }
          continue;
        }
        // Fallback: dispatch Repeat IoEvent
        mpris_manager.set_loop_status(loop_status);
        let mut app_lock = app.lock().await;
        if let Some(ref mut ctx) = app_lock.current_playback_context {
          ctx.repeat_state = repeat_state;
        }
        app_lock.dispatch(IoEvent::Repeat(repeat_state));
      }
      MprisEvent::SetVolume(volume_percent) => {
        let mut app_lock = app.lock().await;
        app_lock.set_volume_percent(volume_percent);
      }
    }
  }
}

/// Route an MPRIS transport event through the standard dispatch path when any
/// decoded source (local file, Subsonic, internet radio, or YouTube) owns the
/// session.
///
/// Returns `true` if the event was consumed (and the caller must skip the
/// Spotify/librespot branches). Play/pause/next/previous/stop/seek map onto the
/// same `IoEvent`s the keyboard uses; the per-source `route_*_event` dispatchers
/// intercept them before the Spotify network, so the control lands on whichever
/// source is actually audible instead of the paused librespot session.
/// Non-transport events (shuffle/loop) return `false` so existing behaviour is
/// preserved.
#[cfg(all(
  feature = "mpris",
  target_os = "linux",
  any(
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  )
))]
async fn route_decoded_mpris_event(
  event: &mpris::MprisEvent,
  app: &Arc<Mutex<App>>,
  mpris_manager: &Arc<mpris::MprisManager>,
) -> bool {
  use mpris::MprisEvent;

  let mut app_lock = app.lock().await;

  // The native queue slot owns playback: shuffle/repeat mean nothing over an
  // explicit queue, whatever it suspended, so reject them here rather than in the
  // match below. A queued *Spotify* track has no decoded player and would
  // otherwise take the early return underneath and let the Spotify handler mutate
  // the suspended context — the one state where MPRIS and the keyboard
  // (`App::shuffle` / `App::repeat`, which no-op) would disagree. The corrective
  // push matches what the snapshot reports for a queue slot, so the runner's
  // dedup cache converges instead of stranding the property.
  if app_lock.queue_owns_playback() {
    match event {
      MprisEvent::SetShuffle(_) => {
        drop(app_lock);
        mpris_manager.set_shuffle(false);
        return true;
      }
      MprisEvent::SetLoopStatus(_) => {
        drop(app_lock);
        mpris_manager.set_loop_status(mpris::LoopStatusEvent::None);
        return true;
      }
      _ => {}
    }
  }

  // Read the live source-player state up front, then drop the borrow so the
  // immutable read does not conflict with the `&mut self` dispatch calls below.
  let Some(player) = app_lock.active_decoded_player() else {
    return false;
  };
  let is_paused = player.is_paused();
  let position_ms = player.position().as_millis() as i64;

  match event {
    MprisEvent::PlayPause => {
      if is_paused {
        app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      } else {
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      true
    }
    MprisEvent::Play => {
      app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      true
    }
    MprisEvent::Pause | MprisEvent::Stop => {
      app_lock.dispatch(IoEvent::PausePlayback);
      true
    }
    MprisEvent::Next => {
      app_lock.dispatch(IoEvent::NextTrack);
      true
    }
    MprisEvent::Previous => {
      app_lock.dispatch(IoEvent::PreviousTrack);
      true
    }
    MprisEvent::Seek(offset_micros) => {
      let offset_ms = offset_micros / 1000;
      let new_position_ms = (position_ms + offset_ms).max(0) as u32;
      app_lock.dispatch(IoEvent::Seek(new_position_ms));
      drop(app_lock);
      mpris_manager.emit_seeked(new_position_ms as u64);
      true
    }
    MprisEvent::SetPosition(position_micros) => {
      let new_position_ms = (position_micros / 1000).max(0) as u32;
      app_lock.dispatch(IoEvent::Seek(new_position_ms));
      drop(app_lock);
      mpris_manager.emit_seeked(new_position_ms as u64);
      true
    }
    // Shuffle/repeat drive the player-global decoded state so an external
    // controller changes the audible decoded source, not the stale Spotify
    // context. A queueable source (Local/Subsonic/YouTube) consumes them; the
    // sources that cannot honour them reject them (below) rather than falling
    // through. Only Spotify (no decoded player at all, so this function returned
    // early) reaches the Spotify handling.
    // Volume is handled by the top-level `set_volume_percent`.
    MprisEvent::SetShuffle(on) => {
      if app_lock.set_decoded_shuffle(*on) {
        drop(app_lock);
        mpris_manager.set_shuffle(*on);
      } else {
        // A decoded source owns playback but has no shuffle of its own: radio is
        // an endless stream, and the queue slot plays an explicit list over a
        // suspended context. Consume the event instead of falling through to the
        // Spotify handler, which would flip shuffle on the user's real device for
        // a source they are not listening to (and persist it to their config).
        // Push the true value back so the client's optimistic flip is corrected:
        // the runner re-pushes only when the snapshot *changes* against its own
        // cache, and it never sees this out-of-band write, so leaving the client
        // at the rejected value would strand the property there for the session.
        drop(app_lock);
        mpris_manager.set_shuffle(false);
      }
      true
    }
    MprisEvent::SetLoopStatus(status) => {
      use crate::infra::queue::RepeatMode;
      let mode = match status {
        mpris::LoopStatusEvent::None => RepeatMode::Off,
        mpris::LoopStatusEvent::Track => RepeatMode::Track,
        mpris::LoopStatusEvent::Playlist => RepeatMode::Context,
      };
      if app_lock.set_decoded_repeat(mode) {
        drop(app_lock);
        mpris_manager.set_loop_status(*status);
      } else {
        // Rejected for the same reason as `SetShuffle` above, and corrected back
        // to the `None` the snapshot reports for these sources.
        drop(app_lock);
        mpris_manager.set_loop_status(mpris::LoopStatusEvent::None);
      }
      true
    }
    MprisEvent::SetVolume(_) => false,
  }
}

/// Handle macOS media events from external sources (media keys, Control Center, AirPods, etc.)
/// Routes control requests to the native streaming player
#[cfg(all(feature = "macos-media", target_os = "macos"))]
async fn handle_macos_media_events(
  mut event_rx: tokio::sync::mpsc::UnboundedReceiver<macos_media::MacMediaEvent>,
  app: Arc<Mutex<App>>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
) {
  use macos_media::MacMediaEvent;
  use std::sync::atomic::Ordering;

  while let Some(event) = event_rx.recv().await {
    if !app.lock().await.user_config.behavior.enable_media_keys {
      continue;
    }

    // A decoded source (local file, Subsonic, radio, or YouTube) owns the
    // session: route transport through the same IoEvents the keyboard uses
    // (intercepted by the per-source route_*_event dispatchers before the
    // Spotify network) so media keys follow the audible source instead of
    // librespot. This must run *before* `active_streaming_player` below, since
    // librespot stays active even while a decoded source is playing.
    #[cfg(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "internet-radio",
      feature = "youtube"
    ))]
    if route_decoded_macos_event(&event, &app).await {
      continue;
    }

    let Some(player) = player::active_streaming_player(&app).await else {
      continue;
    };

    match event {
      MacMediaEvent::PlayPause => {
        // Toggle based on atomic state (lock-free, always up-to-date)
        if shared_is_playing.load(Ordering::Relaxed) {
          player.pause();
        } else {
          player.play();
        }
      }
      MacMediaEvent::Play => {
        player.play();
      }
      MacMediaEvent::Pause => {
        player.pause();
      }
      MacMediaEvent::Next => {
        let _ = player;
        app.lock().await.next_track();
      }
      MacMediaEvent::Previous => {
        let _ = player;
        app.lock().await.previous_track();
      }
      MacMediaEvent::Stop => {
        player.stop();
      }
    }
  }
}

/// Route a macOS media transport event through the standard dispatch path when
/// any decoded source (local file, Subsonic, internet radio, or YouTube) owns
/// the session.
///
/// Returns `true` if the event was consumed (and the caller must skip the
/// streaming-player branches). Play/pause/next/previous/stop map onto the same
/// `IoEvent`s the keyboard uses; the per-source `route_*_event` dispatchers
/// intercept them before the Spotify network, so the control lands on whichever
/// source is actually audible instead of the paused librespot session.
#[cfg(all(
  feature = "macos-media",
  target_os = "macos",
  any(
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  )
))]
async fn route_decoded_macos_event(
  event: &macos_media::MacMediaEvent,
  app: &Arc<Mutex<App>>,
) -> bool {
  use macos_media::MacMediaEvent;

  let mut app_lock = app.lock().await;
  // Read the live source-player state up front, then drop the borrow so the
  // immutable read does not conflict with the `&mut self` dispatch calls below.
  let Some(player) = app_lock.active_decoded_player() else {
    return false;
  };
  let is_paused = player.is_paused();

  match event {
    MacMediaEvent::PlayPause => {
      if is_paused {
        app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      } else {
        app_lock.dispatch(IoEvent::PausePlayback);
      }
    }
    MacMediaEvent::Play => {
      app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
    }
    MacMediaEvent::Pause | MacMediaEvent::Stop => {
      app_lock.dispatch(IoEvent::PausePlayback);
    }
    MacMediaEvent::Next => {
      app_lock.dispatch(IoEvent::NextTrack);
    }
    MacMediaEvent::Previous => {
      app_lock.dispatch(IoEvent::PreviousTrack);
    }
  }
  true
}

#[cfg(all(feature = "windows-media", target_os = "windows"))]
async fn handle_windows_media_events(
  mut event_rx: tokio::sync::mpsc::UnboundedReceiver<smtc_tokio::WindowsMediaEvent>,
  app: Arc<Mutex<App>>,
) {
  use smtc_tokio::WindowsMediaEvent;

  while let Some(event) = event_rx.recv().await {
    if !app.lock().await.user_config.behavior.enable_media_keys {
      continue;
    }

    // A decoded source (local file, Subsonic, radio, or YouTube) owns the
    // session: route transport through the same IoEvents the keyboard uses
    // (intercepted by the per-source route_*_event dispatchers before the
    // Spotify network) so SMTC controls follow the audible source instead of
    // librespot. This must run *before* the streaming-player branches below,
    // since librespot stays active even while a decoded source is playing.
    #[cfg(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "internet-radio",
      feature = "youtube"
    ))]
    if route_decoded_windows_event(&event, &app).await {
      continue;
    }

    let player_opt = player::active_streaming_player(&app).await;

    let is_native_loaded = app.lock().await.native_track_info.is_some();

    match event {
      WindowsMediaEvent::Play => {
        if let Some(player) = &player_opt {
          if is_native_loaded {
            player.play();
            continue;
          }
        }
        app
          .lock()
          .await
          .dispatch(IoEvent::StartPlayback(None, None, None));
      }
      WindowsMediaEvent::Pause => {
        if let Some(player) = &player_opt {
          if is_native_loaded {
            player.pause();
            continue;
          }
        }
        app.lock().await.dispatch(IoEvent::PausePlayback);
      }
      WindowsMediaEvent::Next => {
        if let Some(player) = &player_opt {
          let _ = player;
          app.lock().await.next_track();
        } else {
          app.lock().await.dispatch(IoEvent::NextTrack);
        }
      }
      WindowsMediaEvent::Previous => {
        if let Some(player) = &player_opt {
          let _ = player;
          app.lock().await.previous_track();
        } else {
          app.lock().await.dispatch(IoEvent::PreviousTrack);
        }
      }
      WindowsMediaEvent::Stop => {
        if let Some(player) = &player_opt {
          player.stop();
        } else {
          app.lock().await.dispatch(IoEvent::PausePlayback);
        }
      }
      WindowsMediaEvent::SetPosition(pos) => {
        if let Some(player) = &player_opt {
          if is_native_loaded {
            player.seek(pos as u32);
            continue;
          }
        }
        let mut app_lock = app.lock().await;
        app_lock.song_progress_ms = pos as u128;
        app_lock.dispatch(IoEvent::Seek(pos as u32));
      }
    }
  }
}

/// Route a Windows SMTC media transport event through the standard dispatch
/// path when any decoded source (local file, Subsonic, internet radio, or
/// YouTube) owns the session.
///
/// Returns `true` if the event was consumed (and the caller must skip the
/// streaming-player branches). Play/pause/next/previous/stop/seek map onto the
/// same `IoEvent`s the keyboard uses; the per-source `route_*_event` dispatchers
/// intercept them before the Spotify network, so the control lands on whichever
/// source is actually audible instead of the paused librespot session.
#[cfg(all(
  feature = "windows-media",
  target_os = "windows",
  any(
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  )
))]
async fn route_decoded_windows_event(
  event: &smtc_tokio::WindowsMediaEvent,
  app: &Arc<Mutex<App>>,
) -> bool {
  use smtc_tokio::WindowsMediaEvent;

  let mut app_lock = app.lock().await;
  // Only consume the event while a decoded source owns the session; otherwise
  // fall through to the streaming-player branches for Spotify/librespot.
  if app_lock.active_decoded_player().is_none() {
    return false;
  }

  match event {
    WindowsMediaEvent::Play => {
      app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
    }
    WindowsMediaEvent::Pause | WindowsMediaEvent::Stop => {
      app_lock.dispatch(IoEvent::PausePlayback);
    }
    WindowsMediaEvent::Next => {
      app_lock.dispatch(IoEvent::NextTrack);
    }
    WindowsMediaEvent::Previous => {
      app_lock.dispatch(IoEvent::PreviousTrack);
    }
    WindowsMediaEvent::SetPosition(pos) => {
      app_lock.song_progress_ms = *pos as u128;
      app_lock.dispatch(IoEvent::Seek(*pos as u32));
    }
  }
  true
}

#[cfg(test)]
mod tests {
  use super::{startup_device_decision, StartupDeviceEvent};
  use crate::core::user_config::StartupBehavior;
  use rspotify::model::{device::Device, DeviceType};

  const NATIVE_NAME: &str = "spotatui";
  const NATIVE_ID: &str = "native-device";
  const EXTERNAL_ID: &str = "phone-device";

  #[allow(deprecated)]
  fn device(id: &str, name: &str) -> Device {
    Device {
      id: Some(id.to_string()),
      is_active: false,
      is_private_session: false,
      is_restricted: false,
      name: name.to_string(),
      _type: DeviceType::Computer,
      volume_percent: Some(50),
    }
  }

  fn startup_device_event(
    startup_behavior: StartupBehavior,
    saved_device_id: Option<String>,
    devices_snapshot: Option<&[Device]>,
  ) -> Option<StartupDeviceEvent> {
    startup_device_decision(
      startup_behavior,
      saved_device_id,
      devices_snapshot,
      NATIVE_NAME,
    )
    .event
  }

  #[test]
  fn continue_without_saved_device_does_not_transfer() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(StartupBehavior::Continue, None, Some(&devices)),
      None
    );
  }

  #[test]
  fn continue_with_saved_native_device_does_not_transfer() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Continue,
        Some(NATIVE_ID.to_string()),
        Some(&devices),
      ),
      None
    );
  }

  #[test]
  fn continue_with_saved_external_device_does_not_transfer() {
    let devices = vec![
      device(EXTERNAL_ID, "Jay's phone"),
      device(NATIVE_ID, NATIVE_NAME),
    ];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Continue,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices),
      ),
      None
    );
  }

  #[test]
  fn play_with_saved_available_device_transfers_to_saved_device() {
    let devices = vec![
      device(EXTERNAL_ID, "Jay's phone"),
      device(NATIVE_ID, NATIVE_NAME),
    ];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Play,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices),
      ),
      Some(StartupDeviceEvent::Transfer {
        device_id: EXTERNAL_ID.to_string(),
        persist_device_id: true,
      })
    );
  }

  #[test]
  fn play_without_saved_device_auto_selects_native_fallback() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(StartupBehavior::Play, None, Some(&devices)),
      Some(StartupDeviceEvent::AutoSelectStreaming {
        device_name: NATIVE_NAME.to_string(),
        persist_device_id: true,
      })
    );
  }

  #[test]
  fn continue_with_unavailable_saved_device_does_not_fall_back_to_native() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Continue,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices),
      ),
      None
    );
  }

  #[test]
  fn play_with_unavailable_saved_device_transfers_to_native_without_persisting() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    let decision = startup_device_decision(
      StartupBehavior::Play,
      Some(EXTERNAL_ID.to_string()),
      Some(&devices),
      NATIVE_NAME,
    );

    assert_eq!(
      decision.event,
      Some(StartupDeviceEvent::Transfer {
        device_id: NATIVE_ID.to_string(),
        persist_device_id: false,
      })
    );
    assert_eq!(
      decision.status_message,
      Some(format!("Saved device unavailable; using {}", NATIVE_NAME))
    );
  }

  #[test]
  fn play_with_unavailable_saved_device_auto_selects_native_without_persisting() {
    let devices = vec![device("other-device", "Other speaker")];

    let decision = startup_device_decision(
      StartupBehavior::Play,
      Some(EXTERNAL_ID.to_string()),
      Some(&devices),
      NATIVE_NAME,
    );

    assert_eq!(
      decision.event,
      Some(StartupDeviceEvent::AutoSelectStreaming {
        device_name: NATIVE_NAME.to_string(),
        persist_device_id: false,
      })
    );
    assert_eq!(
      decision.status_message,
      Some(format!("Saved device unavailable; using {}", NATIVE_NAME))
    );
  }

  #[test]
  fn play_with_saved_device_and_no_snapshot_transfers_to_saved_device() {
    let decision = startup_device_decision(
      StartupBehavior::Play,
      Some(EXTERNAL_ID.to_string()),
      None,
      NATIVE_NAME,
    );

    assert_eq!(
      decision.event,
      Some(StartupDeviceEvent::Transfer {
        device_id: EXTERNAL_ID.to_string(),
        persist_device_id: true,
      })
    );
    assert_eq!(decision.status_message, None);
  }
}
