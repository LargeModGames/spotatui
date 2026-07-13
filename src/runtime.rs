#[cfg(all(target_os = "linux", feature = "streaming"))]
mod alsa_silence {
  use std::os::raw::{c_char, c_int};

  type SndLibErrorHandlerT =
    Option<unsafe extern "C" fn(*const c_char, c_int, *const c_char, c_int, *const c_char)>;

  extern "C" {
    fn snd_lib_error_set_handler(handler: SndLibErrorHandlerT) -> c_int;
  }

  unsafe extern "C" fn silent_error_handler(
    _file: *const c_char,
    _line: c_int,
    _function: *const c_char,
    _err: c_int,
    _fmt: *const c_char,
  ) {
  }

  pub fn suppress_alsa_errors() {
    unsafe {
      snd_lib_error_set_handler(Some(silent_error_handler));
    }
  }
}

use crate::cli;
use crate::core::app::App;
use crate::core::auth;
use crate::core::config::ClientConfig;
use crate::core::user_config::{
  validate_tick_rate_milliseconds, StartupBehavior, UserConfig, UserConfigPaths,
};
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
use crate::tui::banner::BANNER;

use anyhow::{anyhow, Result};
use backtrace::Backtrace;
use clap::{Arg, ArgMatches, Command as ClapApp};
use clap_complete::{generate, Shell};
use log::info;
#[cfg(feature = "streaming")]
use log::warn;
#[cfg(feature = "streaming")]
use rspotify::{model::user::PrivateUser, AuthCodePkceSpotify};
#[cfg(feature = "streaming")]
use std::path::Path;
// Used by the streaming OAuth timeout and by `restore_playback_session`'s
// per-source position seeks.
#[cfg(any(
  feature = "streaming",
  feature = "local-files",
  feature = "subsonic",
  feature = "youtube"
))]
use std::time::Duration;
use std::{
  fs,
  io::{self, Write},
  panic,
  path::PathBuf,
  sync::{atomic::AtomicU64, Arc},
};
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
  // position straight from the live local player when local is active.
  #[cfg(feature = "local-files")]
  if let Some(local) = app.local_playback.as_ref() {
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
      } => IoEvent::AutoSelectStreamingDevice(device_name, persist_device_id),
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

#[cfg(all(target_os = "linux", feature = "streaming"))]
fn init_audio_backend() {
  alsa_silence::suppress_alsa_errors();
}

#[cfg(not(all(target_os = "linux", feature = "streaming")))]
fn init_audio_backend() {}

fn setup_logging() -> anyhow::Result<()> {
  // Get the current Process ID
  let pid = std::process::id();

  // Construct the log file path using the PID
  let log_dir = "/tmp/spotatui_logs/";
  let log_path = format!("{}spotatuilog{}", log_dir, pid);

  // Ensure the directory exists. If not, create.
  if !std::path::Path::new(log_dir).exists() {
    std::fs::create_dir_all(log_dir)
      .map_err(|e| anyhow::anyhow!("Failed to create log directory {}: {}", log_dir, e))?;
  }
  // define format of log messages.
  fern::Dispatch::new()
    .format(|out, message, record| {
      out.finish(format_args!(
        "{}[{}][{}] {}",
        chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
        record.target(),
        record.level(),
        message
      ))
    })
    .level(log::LevelFilter::Info)
    .chain(fern::log_file(&log_path)?) // Use the dynamic path
    .apply()
    .map_err(|e| anyhow::anyhow!("Failed to initialize logger: {}", e))?;

  // Print the location of log for user reference.
  println!("Logging to: {}", log_path);

  Ok(())
}

fn install_panic_hook() {
  let default_hook = panic::take_hook();
  panic::set_hook(Box::new(move |info| {
    let is_audio_backend_panic = info
      .location()
      .map(|location| {
        let file = location.file();
        file.contains("audio_backend/portaudio.rs") || file.contains("audio_backend/rodio.rs")
      })
      .unwrap_or(false);

    if is_audio_backend_panic {
      eprintln!(
        "Recoverable audio backend panic detected. Playback may pause while the output device changes."
      );
      return;
    }

    ratatui::restore();
    let panic_log_path = dirs::home_dir().map(|home| {
      home
        .join(".config")
        .join("spotatui")
        .join("spotatui_panic.log")
    });

    if let Some(path) = panic_log_path.as_ref() {
      if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
      }
      if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
      {
        let _ = writeln!(f, "\n==== spotatui panic ====");
        let _ = writeln!(f, "{}", info);
        let _ = writeln!(f, "{:?}", Backtrace::new());
      }
      eprintln!("A crash log was written to: {}", path.to_string_lossy());
    }
    default_hook(info);

    if cfg!(debug_assertions) && std::env::var_os("RUST_BACKTRACE").is_none() {
      eprintln!("{:?}", Backtrace::new());
    }

    if cfg!(target_os = "windows") && std::env::var_os("SPOTATUI_PAUSE_ON_PANIC").is_some() {
      eprintln!("Press Enter to close...");
      let mut s = String::new();
      let _ = std::io::stdin().read_line(&mut s);
    }
  }));
}

#[cfg(feature = "self-update")]
fn add_self_update_cli(clap_app: ClapApp) -> ClapApp {
  clap_app
    .arg(
      Arg::new("no-update")
        .short('U')
        .long("no-update")
        .action(clap::ArgAction::SetTrue)
        .help("Skip the automatic update check on startup"),
    )
    .subcommand(
      ClapApp::new("update")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Check for and install updates")
        .arg(
          Arg::new("install")
            .short('i')
            .long("install")
            .action(clap::ArgAction::SetTrue)
            .help("Install the update if available"),
        ),
    )
}

#[cfg(not(feature = "self-update"))]
fn add_self_update_cli(clap_app: ClapApp) -> ClapApp {
  clap_app
}

#[cfg(feature = "self-update")]
async fn handle_self_update_command(matches: &ArgMatches) -> Result<bool> {
  if let Some(update_matches) = matches.subcommand_matches("update") {
    let do_install = update_matches.get_flag("install");
    // Must use spawn_blocking because self_update uses reqwest::blocking internally,
    // which creates its own tokio runtime and panics if called from an async context.
    tokio::task::spawn_blocking(move || cli::check_for_update(do_install)).await??;
    return Ok(true);
  }

  Ok(false)
}

#[cfg(not(feature = "self-update"))]
async fn handle_self_update_command(_matches: &ArgMatches) -> Result<bool> {
  Ok(false)
}

#[cfg(feature = "self-update")]
async fn run_auto_update(matches: &ArgMatches, user_config: &UserConfig) {
  if matches.subcommand_name().is_some()
    || std::env::var_os("SPOTATUI_SKIP_UPDATE").is_some()
    || matches.get_flag("no-update")
    || user_config.behavior.disable_auto_update
  {
    return;
  }

  println!("Checking for updates...");
  // Must use spawn_blocking because self_update uses reqwest::blocking internally,
  // which creates its own tokio runtime and panics if called from an async context.
  let delay_secs =
    crate::core::user_config::parse_update_delay_secs(&user_config.behavior.auto_update_delay)
      .unwrap_or(0);
  let update_result =
    match tokio::task::spawn_blocking(move || cli::install_update_silent(delay_secs)).await {
      Ok(Ok(outcome)) => Some(outcome),
      Ok(Err(e)) => {
        log::warn!("auto-update failed: {:#}", e);
        None
      }
      Err(e) => {
        log::warn!("auto-update task panicked: {}", e);
        None
      }
    };

  match update_result {
    Some(cli::UpdateOutcome::Installed(new_version)) => {
      println!("Updated to v{}! Restarting...", new_version);
      // Re-exec the current binary with the same args, skipping the update check.
      let exe = std::env::current_exe().expect("failed to get current executable path");
      let args: Vec<String> = std::env::args().skip(1).collect();
      let status = std::process::Command::new(&exe)
        .args(&args)
        .env("SPOTATUI_SKIP_UPDATE", "1")
        .status();
      match status {
        Ok(exit_status) => std::process::exit(exit_status.code().unwrap_or(0)),
        Err(e) => {
          eprintln!("Failed to restart after update: {}", e);
          eprintln!("Please restart spotatui manually.");
          std::process::exit(1);
        }
      }
    }
    Some(cli::UpdateOutcome::Pending {
      version,
      secs_remaining,
    }) => {
      println!(
        "Update v{} detected — will install in {}. Run `spotatui update --install` to update now.",
        version,
        crate::core::user_config::format_update_delay_secs(secs_remaining)
      );
    }
    // Up-to-date, check failed, or no update — continue normally.
    _ => {}
  }
}

#[cfg(not(feature = "self-update"))]
async fn run_auto_update(_matches: &ArgMatches, _user_config: &UserConfig) {}

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

pub async fn run() -> Result<()> {
  setup_logging()?;
  info!("spotatui {} starting up", env!("CARGO_PKG_VERSION"));
  init_audio_backend();
  info!("audio backend initialized");

  install_panic_hook();
  info!("panic hook configured");

  let mut clap_app = add_self_update_cli(
    ClapApp::new(env!("CARGO_PKG_NAME"))
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about(env!("CARGO_PKG_DESCRIPTION"))
    .override_usage("Press `?` while running the app to see keybindings")
    .before_help(BANNER)
    .after_help(
      "Client authentication settings are stored in $HOME/.config/spotatui/client.yml (use --reconfigure-auth to update them)",
    )
    .arg(
      Arg::new("tick-rate")
        .short('t')
        .long("tick-rate")
        .help("Set the normal UI tick rate in milliseconds.")
        .long_help(
          "Specify the normal UI tick rate in milliseconds. Lower values refresh non-animated \
screens more often and cost more CPU. Animation-heavy views keep their separate animation tick rate.",
        ),
    )
    .arg(
      Arg::new("config")
        .short('c')
        .long("config")
        .help("Specify configuration file path."),
    )
    .arg(
      Arg::new("reconfigure-auth")
        .long("reconfigure-auth")
        .action(clap::ArgAction::SetTrue)
        .help("Rerun client authentication setup wizard"),
    )
    .arg(
      Arg::new("play-file")
        .long("play-file")
        .value_name("PATH")
        .help("Play a local audio file on startup (requires the local-files build feature)."),
    )
    .arg(
      Arg::new("completions")
        .long("completions")
        .help("Generates completions for your preferred shell")
        .value_parser(["bash", "zsh", "fish", "power-shell", "elvish"])
        .value_name("SHELL"),
    )
    // Control spotify from the command line
    .subcommand(cli::playback_subcommand())
    .subcommand(cli::play_subcommand())
    .subcommand(cli::list_subcommand())
    .subcommand(cli::history_subcommand())
    .subcommand(cli::search_subcommand()),
  );

  #[cfg(feature = "scripting")]
  {
    clap_app = clap_app.subcommand(cli::plugin_subcommand());
  }

  let matches = clap_app.clone().get_matches();

  // Shell completions don't need any spotify work
  if let Some(s) = matches.get_one::<String>("completions") {
    let shell = match s.as_str() {
      "fish" => Shell::Fish,
      "bash" => Shell::Bash,
      "zsh" => Shell::Zsh,
      "power-shell" => Shell::PowerShell,
      "elvish" => Shell::Elvish,
      _ => return Err(anyhow!("no completions avaible for '{}'", s)),
    };
    generate(shell, &mut clap_app, "spotatui", &mut io::stdout());
    return Ok(());
  }

  // Handle self-update command (doesn't need Spotify auth)
  if handle_self_update_command(&matches).await? {
    return Ok(());
  }

  if let Some(history_matches) = matches.subcommand_matches("history") {
    println!("{}", cli::handle_history_matches(history_matches)?);
    return Ok(());
  }

  // Plugin management is pure git + filesystem work; it must not require Spotify auth.
  #[cfg(feature = "scripting")]
  if let Some(plugin_matches) = matches.subcommand_matches("plugin") {
    cli::handle_plugin_command(plugin_matches)?;
    return Ok(());
  }

  // Auto-update on launch: silently check, download, install, and restart.
  // Skip if a CLI subcommand is active or SPOTATUI_SKIP_UPDATE is set (prevents restart loops).
  let mut user_config = UserConfig::new();
  if let Some(config_file_path) = matches.get_one::<String>("config") {
    let config_file_path = PathBuf::from(config_file_path);
    let path = UserConfigPaths { config_file_path };
    user_config.path_to_config.replace(path);
  }
  user_config.load_config()?;
  info!("user config loaded successfully");

  let initial_shuffle_enabled = user_config.behavior.shuffle_enabled;
  let initial_startup_behavior = user_config.behavior.startup_behavior;

  // Load the persisted non-Spotify playback session so the last song can resume
  // on launch. The file's mere existence means a non-Spotify source was playing
  // at the last save: the runner clears it whenever playback stops or switches
  // to Spotify, so a present file is always something worth resuming — even when
  // the browse source was later switched to Spotify while the song kept playing
  // (browse-source and playback-source are deliberately decoupled). A session
  // whose source feature isn't compiled into this build is a no-op on restore.
  let restore_session: Option<crate::core::persisted_playback::PersistedSession> =
    match crate::core::persisted_playback::default_session_path()
      .and_then(|path| crate::core::persisted_playback::load(&path))
    {
      Ok(session) => session,
      Err(e) => {
        log::warn!("[session] ignoring unreadable playback session: {e}");
        None
      }
    };
  // Split the session: the playback (if any) drives the source resume, while the
  // native queue is restored into app state regardless of whether a source is
  // resumed (a queue-only session must not suppress Spotify's device transfer).
  let (restore_playback, restore_queue): (
    Option<crate::core::persisted_playback::PersistedPlayback>,
    Vec<crate::core::plugin_api::TrackInfo>,
  ) = match restore_session {
    Some(s) => (s.playback, s.queue),
    None => (None, Vec::new()),
  };

  if let Some(tick_rate) = matches
    .get_one::<String>("tick-rate")
    .and_then(|tick_rate| tick_rate.parse().ok())
  {
    user_config.behavior.tick_rate_milliseconds =
      validate_tick_rate_milliseconds(tick_rate, "Tick rate")?;
  }

  let mut client_config = ClientConfig::new();
  // First-run source picker (interactive TUI only): lets the user pick a free
  // source and skip Spotify entirely. Must run before `load_config`, which would
  // otherwise launch the Spotify-only auth wizard on a fresh install. Skipped for
  // CLI subcommands (Spotify-only) and when `--reconfigure-auth` is requested.
  if matches.subcommand_name().is_none() && !matches.get_flag("reconfigure-auth") {
    crate::core::first_run::run_first_run_picker(&mut user_config, &mut client_config).await?;
  }
  client_config.load_config()?;
  info!("client authentication config loaded");

  let reconfigure_auth = matches.get_flag("reconfigure-auth");

  if reconfigure_auth {
    println!("\nReconfiguring client authentication...");
    client_config.reconfigure_auth()?;
    println!("Client authentication setup updated.\n");
  } else if matches.subcommand_name().is_none() && client_config.needs_auth_setup_migration() {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Authentication Setup Update");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
      "\nConfiguration handling has changed and your authentication setup may need an update."
    );
    println!("Would you like to run the new auth setup wizard now? (Y/n): ");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    let run_migration = input.is_empty() || input == "y" || input == "yes";

    if run_migration {
      client_config.reconfigure_auth()?;
      println!("Client authentication setup updated.\n");
    } else {
      client_config.mark_auth_setup_migrated()?;
      println!("Skipped. You can run this anytime with `spotatui --reconfigure-auth`.\n");
    }
  }

  // Prompt for global song count opt-in if missing (only for interactive TUI, not CLI)
  // Keep this after client setup so first-run UX asks for auth mode first.
  if matches.subcommand_name().is_none() {
    let config_paths_check = match &user_config.path_to_config {
      Some(path) => path,
      None => {
        user_config.get_or_build_paths()?;
        user_config.path_to_config.as_ref().unwrap()
      }
    };

    let should_prompt = if config_paths_check.config_file_path.exists() {
      let config_string = fs::read_to_string(&config_paths_check.config_file_path)?;
      config_string.trim().is_empty() || !config_string.contains("enable_global_song_count")
    } else {
      let client_yml_path = config_paths_check
        .config_file_path
        .parent()
        .map(|p| p.join("client.yml"));
      client_yml_path.is_some_and(|p| p.exists())
    };

    if should_prompt {
      println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
      println!("Global Song Counter");
      println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
      println!("\nspotatui can contribute to a global counter showing total");
      println!("songs played by all users worldwide.");
      println!("\nPrivacy: This feature is completely anonymous.");
      println!("• No personal information is collected");
      println!("• No song names, artists, or listening history");
      println!("• Only a simple increment when a new song starts");
      println!("\nWould you like to participate? (Y/n): ");

      let mut input = String::new();
      io::stdin().read_line(&mut input)?;
      let input = input.trim().to_lowercase();

      let enable = input.is_empty() || input == "y" || input == "yes";
      user_config.behavior.enable_global_song_count = enable;

      let config_yml = if config_paths_check.config_file_path.exists() {
        fs::read_to_string(&config_paths_check.config_file_path).unwrap_or_default()
      } else {
        String::new()
      };

      let mut config: serde_yaml::Value = if config_yml.trim().is_empty() {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
      } else {
        serde_yaml::from_str(&config_yml)?
      };

      if let serde_yaml::Value::Mapping(ref mut map) = config {
        let behavior = map
          .entry(serde_yaml::Value::String("behavior".to_string()))
          .or_insert(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));

        if let serde_yaml::Value::Mapping(ref mut behavior_map) = behavior {
          behavior_map.insert(
            serde_yaml::Value::String("enable_global_song_count".to_string()),
            serde_yaml::Value::Bool(enable),
          );
        }
      }

      let updated_config = serde_yaml::to_string(&config)?;
      fs::write(&config_paths_check.config_file_path, updated_config)?;

      if enable {
        println!("Thank you for participating!\n");
      } else {
        println!("Opted out. You can change this anytime in ~/.config/spotatui/config.yml\n");
      }
    }
  }

  let config_paths = client_config.get_or_build_paths()?;

  // Spotify is only mandatory when the active source IS Spotify, or when running
  // a CLI subcommand (every subcommand is Spotify-only and should fail cleanly
  // when unauthenticated). A free-source TUI launch tries a silent token load and
  // tolerates its absence; the user can add Spotify later via in-TUI login.
  let spotify_required = matches.subcommand_name().is_some()
    || user_config.behavior.active_source == crate::core::source::Source::Spotify;

  // The GitHub update check runs concurrently with authentication: both are
  // network round trips and neither depends on the other, so the check no
  // longer adds its own latency to startup. (An update that actually installs
  // still restarts the process, exactly as before.)
  let (authenticated, _) = tokio::join!(
    async {
      if spotify_required {
        auth::authenticate_with_fallback(&mut client_config, &config_paths)
          .await
          .map(Some)
      } else {
        Ok(auth::try_load_spotify_silently(&mut client_config, &config_paths).await)
      }
    },
    run_auto_update(&matches, &user_config)
  );
  let authenticated: Option<auth::AuthenticatedClient> = authenticated?;

  // Redirect URI for native streaming: from the authenticated client when a
  // Spotify session exists, else the configured default (streaming stays off
  // without Spotify anyway, see the `spotify.is_some()` gate below).
  #[cfg(feature = "streaming")]
  let selected_redirect_uri = authenticated
    .as_ref()
    .map(|a| a.redirect_uri.clone())
    .unwrap_or_else(|| client_config.get_redirect_uri());

  let final_token_cache_path = authenticated
    .as_ref()
    .map(|a| a.token_cache_path.clone())
    .unwrap_or_else(|| {
      auth::token_cache_path_for_client(&config_paths.token_cache_path, &client_config.client_id)
    });

  // The /me captured while validating the cached token; the streaming account
  // probe reuses it instead of a second round trip.
  #[cfg(feature = "streaming")]
  let cached_me = authenticated.as_ref().and_then(|a| a.me.clone());

  // Persist whatever token is now in memory and verify it. All later Spotify
  // requests go through spotatui's refresh-and-cache path so the on-disk token
  // stays current. With no Spotify session both stay `None`.
  let (spotify, token_expiry) = match authenticated.map(|a| a.spotify) {
    Some(spotify) => {
      if let Err(e) = auth::save_token_to_file(&spotify, &final_token_cache_path).await {
        log::warn!("Failed to cache token on startup: {}", e);
      }
      let token_expiry = auth::token_expiry(&spotify).await?;
      (Some(spotify), Some(token_expiry))
    }
    None => (None, None),
  };

  let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel::<IoEvent>();
  info!("app state initialized");

  // Initialise app state
  let app = Arc::new(Mutex::new(App::new(
    sync_io_tx,
    user_config.clone(),
    token_expiry,
  )));

  // `--play-file <PATH>`: queue a local file to start once the UI is up. The
  // path is canonicalised to an absolute `file://` URI so the local-files
  // dispatch can route it; an unreadable path is reported as a status message.
  if let Some(path) = matches.get_one::<String>("play-file") {
    match std::fs::canonicalize(path).ok().and_then(|abs| {
      url::Url::from_file_path(abs)
        .ok()
        .map(|url| url.to_string())
    }) {
      Some(uri) => app.lock().await.pending_play_file = Some(uri),
      None => {
        app
          .lock()
          .await
          .set_status_message(format!("Cannot find local file: {path}"), 8);
      }
    }
  }

  // Work with the cli (not really async)
  if let Some(cmd) = matches.subcommand_name() {
    info!("running in cli mode with command: {}", cmd);
    // Save, because we checked if the subcommand is present at runtime
    let m = matches.subcommand_matches(cmd).unwrap();
    #[cfg(feature = "streaming")]
    let network = Network::new(spotify, client_config, &app, final_token_cache_path); // CLI doesn't use streaming
    #[cfg(not(feature = "streaming"))]
    let network = Network::new(spotify, client_config, &app, final_token_cache_path);
    println!(
      "{}",
      cli::handle_matches(m, cmd.to_string(), network, user_config).await?
    );
  // Launch the UI (async)
  } else {
    info!("launching interactive terminal ui");
    #[cfg(feature = "streaming")]
    if client_config.enable_streaming
      && !player::streaming_credentials_are_cached().unwrap_or(false)
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
          let cached = tokio::task::spawn_blocking(player::ensure_streaming_credentials_cached)
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("credential caching task panicked: {e}")));
          if let Err(error) = cached {
            warn!("native streaming authentication unavailable: {error}");
            app.lock().await.set_status_message(
              "Native streaming authentication failed; using Spotify Connect.",
              10,
            );
          }
        }
      }
    }
    crate::infra::history::spawn_history_collector(Arc::clone(&app));
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
    let windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>> = if streaming_attempted
    {
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
        volume_percent: user_config.behavior.volume_percent,
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
      #[cfg(feature = "streaming")]
      let mut network = Network::new(spotify, client_config, &app, final_token_cache_path);
      #[cfg(not(feature = "streaming"))]
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
    )
    .await;
    if ui_result.is_err() {
      cloned_app.lock().await.flush_config_save(true);
    }
    ui_result?;
  }

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
      let guard = app.lock().await;
      if let Some(s) = guard.local_playback.as_ref() {
        if position_ms > 0 {
          let _ = s.player.seek(Duration::from_millis(position_ms));
        }
        if resolve_paused(paused) {
          s.player.pause();
        }
      }
    }
    #[cfg(feature = "subsonic")]
    PersistedPlayback::Subsonic {
      tracks,
      index,
      position_ms,
      paused,
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
      let guard = app.lock().await;
      if let Some(s) = guard.subsonic_playback.as_ref() {
        if position_ms > 0 {
          let _ = s.player.seek(Duration::from_millis(position_ms));
        }
        if resolve_paused(paused) {
          s.player.pause();
        }
      }
    }
    #[cfg(feature = "youtube")]
    PersistedPlayback::YouTube {
      tracks,
      index,
      position_ms,
      paused,
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
      let guard = app.lock().await;
      if let Some(s) = guard.youtube_playback.as_ref() {
        if position_ms > 0 {
          let _ = s.player.seek(Duration::from_millis(position_ms));
        }
        if resolve_paused(paused) {
          s.player.pause();
        }
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

async fn start_tokio(io_rx: std::sync::mpsc::Receiver<IoEvent>, network: &mut Network) {
  // Bridge the sync dispatch channel onto the async runtime: a dedicated
  // thread does blocking recv() and forwards into a tokio channel the pump can
  // await, replacing the old try_recv() + 5ms sleep poll (which added 0-5ms
  // latency to every event and busy-woke the task while idle).
  let (bridge_tx, mut bridge_rx) = tokio::sync::mpsc::unbounded_channel::<IoEvent>();
  std::thread::spawn(move || {
    while let Ok(io_event) = io_rx.recv() {
      if bridge_tx.send(io_event).is_err() {
        break;
      }
    }
  });

  // Party relay messages used to piggyback on the 5ms poll loop; with the pump
  // parked on recv(), drain them on an interval instead.
  let mut party_poll = tokio::time::interval(std::time::Duration::from_millis(25));
  party_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

  loop {
    let io_event = tokio::select! {
      maybe_event = bridge_rx.recv() => match maybe_event {
        Some(io_event) => io_event,
        None => break,
      },
      _ = party_poll.tick(), if network.party_incoming_rx.is_some() => {
        network.process_party_messages().await;
        continue;
      }
    };

    // Source-agnostic service events (lyrics, cover art, telemetry,
    // announcements, friends, stats/recap) only touch `App` and their own HTTP
    // clients — never the Spotify client, pacing, or `Network` state — so they
    // run concurrently on a detached task instead of queueing behind
    // rate-limited Spotify calls on this serial pump (and vice versa).
    if Network::runs_on_service_lane(&io_event) {
      let mut service_network = Network::new(
        None,
        network.client_config.clone(),
        &network.app,
        network.token_cache_path.clone(),
      );
      tokio::spawn(async move {
        service_network.handle_network_event(io_event).await;
      });
      continue;
    }

    {
      // The native queue router runs first: it owns `AdvanceNativeQueue` and
      // the queue slot's transport controls, and relinquishes the slot on an
      // unrelated `StartPlayback` (returning false so the per-source
      // teardowns/starts still run). Compiled unconditionally.
      let handled_queue =
        crate::infra::queue::dispatch::route_queue_event(&network.app, &io_event).await;
      // Local-file playback is intercepted before the Spotify network so the
      // network stays Spotify-only (see infra::local::dispatch).
      let handled_locally = !handled_queue && {
        #[cfg(feature = "local-files")]
        {
          crate::infra::local::dispatch::route_local_event(&network.app, &io_event).await
        }
        #[cfg(not(feature = "local-files"))]
        {
          false
        }
      };
      // Subsonic is intercepted after local and before the Spotify network. A
      // `subsonic:` URI falls through the local dispatch (its `is_file_uri` is
      // false) and is caught here (see infra::subsonic::dispatch). Skipped when
      // local already consumed the event.
      #[cfg(feature = "subsonic")]
      let handled_subsonic = !handled_queue
        && !handled_locally
        && crate::infra::subsonic::dispatch::route_subsonic_event(&network.app, &io_event).await;
      #[cfg(not(feature = "subsonic"))]
      let handled_subsonic = false;
      // Internet radio is intercepted last before the Spotify network. A
      // `radio:` URI falls through both earlier dispatches and is caught here
      // (see infra::radio::dispatch). Skipped when already consumed.
      #[cfg(feature = "internet-radio")]
      let handled_radio = !handled_queue
        && !handled_locally
        && !handled_subsonic
        && crate::infra::radio::dispatch::route_radio_event(&network.app, &io_event).await;
      #[cfg(not(feature = "internet-radio"))]
      let handled_radio = false;
      // YouTube is intercepted last before the Spotify network. A `youtube:`
      // URI falls through the three earlier dispatches and is caught here
      // (see infra::youtube::dispatch). Skipped when already consumed.
      #[cfg(feature = "youtube")]
      let handled_youtube = !handled_queue
        && !handled_locally
        && !handled_subsonic
        && !handled_radio
        && crate::infra::youtube::dispatch::route_youtube_event(&network.app, &io_event).await;
      #[cfg(not(feature = "youtube"))]
      let handled_youtube = false;
      if !handled_queue
        && !handled_locally
        && !handled_subsonic
        && !handled_radio
        && !handled_youtube
      {
        network.handle_network_event(io_event).await;
      } else {
        // A source router consumed the event and returned without touching
        // `is_loading`, which `App::dispatch` set to true. Only
        // `handle_network_event` resets it, and we skipped that path, so clear
        // it here — otherwise selecting/loading Local, Subsonic, Radio, or
        // YouTube content leaves the UI stuck on the loading indicator.
        network.app.lock().await.is_loading = false;
      }
    }
    network.process_party_messages().await;
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
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      }
      MprisEvent::Pause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = current_player {
          player.pause();
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
            if let Some(ref mut ctx) = app_lock.current_playback_context {
              ctx.shuffle_state = shuffle;
            }
            app_lock.user_config.behavior.shuffle_enabled = shuffle;
          }
          continue;
        }
        // Fallback: dispatch Shuffle IoEvent
        mpris_manager.set_shuffle(shuffle);
        let mut app_lock = app.lock().await;
        if let Some(ref mut ctx) = app_lock.current_playback_context {
          ctx.shuffle_state = shuffle;
        }
        app_lock.user_config.behavior.shuffle_enabled = shuffle;
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
    // Shuffle/loop don't apply to single-file local playback. Volume is handled
    // by the top-level `set_volume_percent`, which already routes to whichever
    // decoded source owns the sink. Leave all three to the existing handling.
    MprisEvent::SetShuffle(_) | MprisEvent::SetLoopStatus(_) | MprisEvent::SetVolume(_) => false,
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
