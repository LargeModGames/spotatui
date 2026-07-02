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
#[cfg(feature = "streaming")]
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

#[cfg(feature = "streaming")]
async fn account_supports_native_streaming(
  spotify: &AuthCodePkceSpotify,
  token_cache_path: &Path,
  app: &Arc<Mutex<App>>,
) -> (bool, Option<&'static str>) {
  match spotify_get_typed_compat_for_with_refresh::<PrivateUser>(
    spotify,
    "me",
    &[],
    token_cache_path,
    app,
  )
  .await
  {
    #[allow(deprecated)]
    Ok(user) => match user.product {
      Some(rspotify::model::SubscriptionLevel::Premium) => (true, None),
      Some(level) => {
        let plan = subscription_level_label(level);
        info!(
          "spotify {} account detected: playback is unavailable (native streaming and Web API playback controls require premium)",
          plan
        );
        println!(
          "Spotify {} account detected. Playback is unavailable in spotatui: native streaming (librespot) and Web API playback controls both require Premium. Browsing/search/library views still work.",
          plan
        );
        (
          false,
          Some("Spotify Free account: playback controls unavailable (Premium required)"),
        )
      }
      None => {
        info!("spotify account level unknown: native streaming disabled to avoid librespot exit");
        println!(
          "Could not determine Spotify subscription level. Native streaming is disabled to avoid startup exit. If this account is not Premium, playback controls will not work; browsing/search/library views still work."
        );
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
      println!(
        "Could not verify Spotify subscription level. Native streaming is disabled to avoid startup exit. If this account is not Premium, playback controls will not work; browsing/search/library views still work."
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

  run_auto_update(&matches, &user_config).await;

  let initial_shuffle_enabled = user_config.behavior.shuffle_enabled;
  let initial_startup_behavior = user_config.behavior.startup_behavior;

  if let Some(tick_rate) = matches
    .get_one::<String>("tick-rate")
    .and_then(|tick_rate| tick_rate.parse().ok())
  {
    user_config.behavior.tick_rate_milliseconds =
      validate_tick_rate_milliseconds(tick_rate, "Tick rate")?;
  }

  let mut client_config = ClientConfig::new();
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
  let authenticated = auth::authenticate_with_fallback(&mut client_config, &config_paths).await?;
  let spotify = authenticated.spotify;
  let final_token_cache_path = authenticated.token_cache_path;
  #[cfg(feature = "streaming")]
  let selected_redirect_uri = authenticated.redirect_uri;

  // Persist whatever token is now in memory. All later Spotify requests go through
  // spotatui's refresh-and-cache path so the on-disk token stays current.
  if let Err(e) = auth::save_token_to_file(&spotify, &final_token_cache_path).await {
    log::warn!("Failed to cache token on startup: {}", e);
  }
  // Verify that we have a valid token before proceeding
  let token_expiry = auth::token_expiry(&spotify).await?;

  let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel::<IoEvent>();
  info!("app state initialized");

  // Initialise app state
  let app = Arc::new(Mutex::new(App::new(
    sync_io_tx,
    user_config.clone(),
    token_expiry,
  )));

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
    crate::infra::history::spawn_history_collector(Arc::clone(&app));
    #[cfg(feature = "streaming")]
    let (streaming_supported_for_account, streaming_startup_status_message) =
      if client_config.enable_streaming {
        account_supports_native_streaming(&spotify, &final_token_cache_path, &app).await
      } else {
        (false, None)
      };

    #[cfg(feature = "streaming")]
    if let Some(message) = streaming_startup_status_message {
      let mut app_mut = app.lock().await;
      app_mut.set_status_message(message, 12);
    }

    // Initialize streaming player if enabled
    #[cfg(feature = "streaming")]
    let streaming_player = if client_config.enable_streaming && streaming_supported_for_account {
      info!("initializing native streaming player");
      let streaming_config = player::StreamingConfig {
        device_name: client_config.streaming_device_name.clone(),
        bitrate: client_config.streaming_bitrate,
        audio_cache: client_config.streaming_audio_cache,
        cache_path: player::get_default_cache_path(),
        initial_volume: user_config.behavior.volume_percent,
      };

      let client_id = client_config.client_id.clone();
      let redirect_uri = selected_redirect_uri.clone();

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
        player::StreamingPlayer::new(&client_id, &redirect_uri, streaming_config).await
      });
      let abort_handle = init_task.abort_handle();

      match tokio::time::timeout(outer_timeout, init_task).await {
        Ok(Ok(Ok(p))) => {
          info!(
            "native streaming player initialized as '{}'",
            p.device_name()
          );
          // Note: We don't activate() here - that's handled by AutoSelectStreamingDevice
          // which respects the user's saved device preference (e.g., spotifyd)
          Some(Arc::new(p))
        }
        Ok(Ok(Err(e))) => {
          info!(
            "failed to initialize streaming: {} - falling back to web api",
            e
          );
          None
        }
        Ok(Err(e)) => {
          info!(
            "streaming initialization panicked: {} - falling back to web api",
            e
          );
          None
        }
        Err(_) => {
          abort_handle.abort();
          warn!(
            "streaming initialization hung unexpectedly (outer timeout {}s) - falling back to web api",
            outer_timeout.as_secs()
          );
          None
        }
      }
    } else {
      None
    };

    #[cfg(feature = "streaming")]
    if streaming_player.is_some() {
      info!("native playback enabled - spotatui is available as a spotify connect device");
    }

    // Store streaming player reference in App for direct control (bypasses event channel)
    #[cfg(feature = "streaming")]
    {
      let mut app_mut = app.lock().await;
      app_mut.streaming_player = streaming_player.clone();
    }

    // Clone the device name for startup device selection in the network task.
    #[cfg(feature = "streaming")]
    let streaming_device_name = streaming_player
      .as_ref()
      .map(|p| p.device_name().to_string());

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
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    let macos_media_manager: Option<Arc<macos_media::MacMediaManager>> =
      if streaming_player.is_some() {
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
    let windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>> =
      if streaming_player.is_some() {
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
        #[cfg(feature = "streaming")]
        let streaming_player_for_mpris = streaming_player.clone();
        let mpris_for_seek = Arc::clone(mpris);
        let app_for_mpris = Arc::clone(&app);
        tokio::spawn(async move {
          handle_mpris_events(
            event_rx,
            #[cfg(feature = "streaming")]
            streaming_player_for_mpris,
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

    // Spawn player event listener (updates app state from native player events)
    #[cfg(feature = "streaming")]
    if let Some(ref player) = streaming_player {
      player::spawn_player_event_handler(player::PlayerEventContext {
        player: Arc::clone(player),
        app: Arc::clone(&app),
        shared_position: shared_position_for_events,
        shared_is_playing: shared_is_playing_for_events,
        recovery_tx: streaming_recovery_tx.clone(),
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

      // Auto-select the saved playback device when available (fallback to native streaming).
      #[cfg(feature = "streaming")]
      if let Some(device_name) = streaming_device_name {
        let saved_device_id = network.client_config.device_id.clone();
        let mut devices_snapshot = None;

        if let Ok(devices) = network
          .spotify_get_typed::<rspotify::model::device::DevicePayload>("me/player/devices", &[])
          .await
        {
          let devices_vec = devices.devices;
          let mut app = network.app.lock().await;
          app.devices = Some(rspotify::model::device::DevicePayload {
            devices: devices_vec.clone(),
          });
          devices_snapshot = Some(devices_vec);
        }

        let startup_decision = startup_device_decision(
          initial_startup_behavior,
          saved_device_id,
          devices_snapshot.as_deref(),
          &device_name,
        );

        if let Some(message) = startup_decision.status_message {
          let mut app = network.app.lock().await;
          app.set_status_message(message, 5);
        }

        if let Some(event) = startup_decision.event {
          network.handle_network_event(event.into_io_event()).await;
        }
      }

      // Apply configured startup play behavior. Continue is passive and must not
      // transfer devices, change shuffle, or otherwise activate Spotatui.
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

      start_tokio(sync_io_rx, &mut network).await;
    });
    // The UI must run in the "main" thread
    info!("starting terminal ui event loop");
    #[cfg(feature = "streaming")]
    let shared_pos_for_start_ui: Option<Arc<AtomicU64>> = Some(shared_position_for_ui);
    #[cfg(not(feature = "streaming"))]
    let shared_pos_for_start_ui: Option<Arc<AtomicU64>> = None;
    crate::tui::runner::start_ui(
      user_config,
      &cloned_app,
      shared_pos_for_start_ui,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_for_ui,
      #[cfg(not(all(feature = "mpris", target_os = "linux")))]
      None,
      discord_rpc_manager,
    )
    .await?;
  }

  Ok(())
}

async fn start_tokio(io_rx: std::sync::mpsc::Receiver<IoEvent>, network: &mut Network) {
  loop {
    match io_rx.try_recv() {
      Ok(io_event) => {
        network.handle_network_event(io_event).await;
      }
      Err(std::sync::mpsc::TryRecvError::Empty) => {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
      }
      Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
    }
    network.process_party_messages().await;
  }
}

/// Handle MPRIS events from external clients (media keys, playerctl, etc.)
/// Routes to native streaming player when available, or dispatches IoEvents as fallback
#[cfg(all(feature = "mpris", target_os = "linux"))]
async fn handle_mpris_events(
  mut event_rx: tokio::sync::mpsc::UnboundedReceiver<mpris::MprisEvent>,
  #[cfg(feature = "streaming")] streaming_player: Option<Arc<player::StreamingPlayer>>,
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
    match event {
      MprisEvent::PlayPause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
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
        if let Some(ref player) = streaming_player {
          player.play();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      }
      MprisEvent::Pause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.pause();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      MprisEvent::Next => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.activate();
          player.next();
          player.play();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::NextTrack);
      }
      MprisEvent::Previous => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.activate();
          player.prev();
          player.play();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PreviousTrack);
      }
      MprisEvent::Stop => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.stop();
          continue;
        }
        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      MprisEvent::Seek(offset_micros) => {
        // MPRIS sends relative offset in microseconds (can be negative for rewind)
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
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
        if let Some(ref player) = streaming_player {
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
        if let Some(ref player) = streaming_player {
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
        if let Some(ref player) = streaming_player {
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
        player.activate();
        player.next();
        // Keep Connect + audio state in sync.
        player.play();
      }
      MacMediaEvent::Previous => {
        player.activate();
        player.prev();
        // Keep Connect + audio state in sync.
        player.play();
      }
      MacMediaEvent::Stop => {
        player.stop();
      }
    }
  }
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
          player.activate();
          player.next();
          player.play();
        } else {
          app.lock().await.dispatch(IoEvent::NextTrack);
        }
      }
      WindowsMediaEvent::Previous => {
        if let Some(player) = &player_opt {
          player.activate();
          player.prev();
          player.play();
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
