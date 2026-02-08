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

mod app;
mod audio;
mod banner;
mod cli;
mod config;
#[cfg(feature = "discord-rpc")]
mod discord_rpc;
mod event;
mod handlers;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
mod macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
mod mpris;
mod network;
#[cfg(feature = "streaming")]
mod player;
mod redirect_uri;
mod sort;
mod ui;
mod user_config;

use crate::app::RouteId;
use crate::event::Key;
use anyhow::{anyhow, Result};
use app::{ActiveBlock, App};
use backtrace::Backtrace;
use banner::BANNER;
use clap::{Arg, Command as ClapApp};
use clap_complete::{generate, Shell};
use config::ClientConfig;
use crossterm::{
  cursor::MoveTo,
  event::{DisableMouseCapture, EnableMouseCapture},
  execute,
  terminal::SetTitle,
  ExecutableCommand,
};
use network::{IoEvent, Network};
use ratatui::backend::Backend;
use redirect_uri::redirect_uri_web_server;
use rspotify::{
  prelude::*,
  {AuthCodeSpotify, Config, Credentials, OAuth, Token},
};
use std::{
  cmp::{max, min},
  fs,
  io::{self, stdout, Write},
  panic,
  path::PathBuf,
  sync::{atomic::AtomicU64, Arc},
  time::SystemTime,
};
#[cfg(feature = "streaming")]
use std::{
  sync::atomic::Ordering,
  time::{Duration, Instant},
};
use tokio::sync::Mutex;
use user_config::{UserConfig, UserConfigPaths};

#[cfg(feature = "discord-rpc")]
type DiscordRpcHandle = Option<discord_rpc::DiscordRpcManager>;
#[cfg(not(feature = "discord-rpc"))]
type DiscordRpcHandle = Option<()>;

const SCOPES: [&str; 16] = [
  "playlist-read-collaborative",
  "playlist-read-private",
  "playlist-modify-private",
  "playlist-modify-public",
  "user-follow-read",
  "user-follow-modify",
  "user-library-modify",
  "user-library-read",
  "user-modify-playback-state",
  "user-read-currently-playing",
  "user-read-playback-state",
  "user-read-playback-position",
  "user-read-private",
  "user-read-recently-played",
  "user-top-read", // Required for Top Tracks/Artists in Discover
  "streaming",     // Required for native playback
];

#[cfg(feature = "discord-rpc")]
const DEFAULT_DISCORD_CLIENT_ID: &str = "1464235043462447166";

#[cfg(feature = "discord-rpc")]
#[derive(Clone, Debug, PartialEq)]
struct DiscordTrackInfo {
  title: String,
  artist: String,
  album: String,
  image_url: Option<String>,
  duration_ms: u32,
}

#[cfg(feature = "discord-rpc")]
#[derive(Default)]
struct DiscordPresenceState {
  last_track: Option<DiscordTrackInfo>,
  last_is_playing: Option<bool>,
  last_progress_ms: u128,
}

#[cfg(feature = "mpris")]
#[derive(Default, PartialEq)]
struct MprisMetadata {
  title: String,
  artists: Vec<String>,
  album: String,
  duration_ms: u32,
  art_url: Option<String>,
}
#[cfg(feature = "mpris")]
type MprisMetadataTuple = (String, Vec<String>, String, u32, Option<String>);

#[cfg(feature = "discord-rpc")]
fn resolve_discord_app_id(user_config: &UserConfig) -> Option<String> {
  std::env::var("SPOTATUI_DISCORD_APP_ID")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .or_else(|| user_config.behavior.discord_rpc_client_id.clone())
    .or_else(|| Some(DEFAULT_DISCORD_CLIENT_ID.to_string()))
}

#[cfg(feature = "discord-rpc")]
fn build_discord_playback(app: &App) -> Option<discord_rpc::DiscordPlayback> {
  use crate::ui::util::create_artist_string;
  use rspotify::model::PlayableItem;

  let (track_info, is_playing) = if let Some(native_info) = &app.native_track_info {
    let is_playing = app.native_is_playing.unwrap_or(true);
    (
      DiscordTrackInfo {
        title: native_info.name.clone(),
        artist: native_info.artists_display.clone(),
        album: native_info.album.clone(),
        image_url: None,
        duration_ms: native_info.duration_ms,
      },
      is_playing,
    )
  } else if let Some(context) = &app.current_playback_context {
    let is_playing = if app.is_streaming_active {
      app.native_is_playing.unwrap_or(context.is_playing)
    } else {
      context.is_playing
    };

    let item = context.item.as_ref()?;
    match item {
      PlayableItem::Track(track) => (
        DiscordTrackInfo {
          title: track.name.clone(),
          artist: create_artist_string(&track.artists),
          album: track.album.name.clone(),
          image_url: track.album.images.first().map(|image| image.url.clone()),
          duration_ms: track.duration.num_milliseconds() as u32,
        },
        is_playing,
      ),
      PlayableItem::Episode(episode) => (
        DiscordTrackInfo {
          title: episode.name.clone(),
          artist: episode.show.name.clone(),
          album: String::new(),
          image_url: episode.images.first().map(|image| image.url.clone()),
          duration_ms: episode.duration.num_milliseconds() as u32,
        },
        is_playing,
      ),
    }
  } else {
    return None;
  };

  let base_state = if track_info.album.is_empty() {
    track_info.artist.clone()
  } else {
    format!("{} - {}", track_info.artist, track_info.album)
  };
  let state = if is_playing {
    base_state
  } else if base_state.is_empty() {
    "Paused".to_string()
  } else {
    format!("Paused: {}", base_state)
  };

  Some(discord_rpc::DiscordPlayback {
    title: track_info.title,
    artist: track_info.artist,
    album: track_info.album,
    state,
    image_url: track_info.image_url,
    duration_ms: track_info.duration_ms,
    progress_ms: app.song_progress_ms,
    is_playing,
  })
}

#[cfg(feature = "mpris")]
fn get_mpris_metadata(app: &App) -> Option<MprisMetadataTuple> {
  use crate::ui::util::create_artist_string;
  use rspotify::model::PlayableItem;

  if let Some(context) = &app.current_playback_context {
    let item = context.item.as_ref()?;
    match item {
      PlayableItem::Track(track) => Some((
        track.name.clone(),
        vec![create_artist_string(&track.artists)],
        track.album.name.clone(),
        track.duration.num_milliseconds() as u32,
        track.album.images.first().map(|image| image.url.clone()),
      )),
      PlayableItem::Episode(episode) => Some((
        episode.name.clone(),
        vec![episode.show.name.clone()],
        String::new(),
        episode.duration.num_milliseconds() as u32,
        episode.images.first().map(|image| image.url.clone()),
      )),
    }
  } else {
    None
  }
}

#[cfg(feature = "discord-rpc")]
fn update_discord_presence(
  manager: &discord_rpc::DiscordRpcManager,
  state: &mut DiscordPresenceState,
  app: &App,
) {
  let playback = build_discord_playback(app);

  match playback {
    Some(playback) => {
      let track_info = DiscordTrackInfo {
        title: playback.title.clone(),
        artist: playback.artist.clone(),
        album: playback.album.clone(),
        image_url: playback.image_url.clone(),
        duration_ms: playback.duration_ms,
      };

      let track_changed = state.last_track.as_ref() != Some(&track_info);
      let playing_changed = state.last_is_playing != Some(playback.is_playing);
      let progress_delta = playback.progress_ms.abs_diff(state.last_progress_ms);
      let progress_changed = progress_delta > 5000;

      if track_changed || playing_changed || progress_changed {
        manager.set_activity(&playback);
        state.last_track = Some(track_info);
        state.last_is_playing = Some(playback.is_playing);
        state.last_progress_ms = playback.progress_ms;
      }
    }
    None => {
      if state.last_track.is_some() {
        manager.clear();
        state.last_track = None;
        state.last_is_playing = None;
        state.last_progress_ms = 0;
      }
    }
  }
}

#[cfg(feature = "mpris")]
fn update_mpris_metadata(
  manager: &mpris::MprisManager,
  last_metadata: &mut Option<MprisMetadata>,
  app: &App,
) {
  if let Some((title, artists, album, duration_ms, art_url)) = get_mpris_metadata(app) {
    let new_metadata = MprisMetadata {
      title: title.clone(),
      artists: artists.clone(),
      album: album.clone(),
      duration_ms,
      art_url: art_url.clone(),
    };

    // Only update if metadata changed
    if last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(&title, &artists, &album, duration_ms, art_url);
      *last_metadata = Some(new_metadata);
    }
  } else {
    // Clear if no playback
    if last_metadata.is_some() {
      *last_metadata = None;
    }
  }
}

// Manual token cache helpers since rspotify's built-in caching isn't working
async fn save_token_to_file(spotify: &AuthCodeSpotify, path: &PathBuf) -> Result<()> {
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  if let Some(ref token) = *token_lock {
    let token_json = serde_json::to_string_pretty(token)?;
    fs::write(path, token_json)?;
    println!("Token saved to {}", path.display());
  }
  Ok(())
}

async fn load_token_from_file(spotify: &AuthCodeSpotify, path: &PathBuf) -> Result<bool> {
  if !path.exists() {
    return Ok(false);
  }

  let token_json = fs::read_to_string(path)?;
  let token: Token = serde_json::from_str(&token_json)?;

  let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
  *token_lock = Some(token);
  drop(token_lock);

  println!("Found cached authentication token");
  Ok(true)
}

#[cfg(all(target_os = "linux", feature = "streaming"))]
fn init_audio_backend() {
  alsa_silence::suppress_alsa_errors();
}

#[cfg(not(all(target_os = "linux", feature = "streaming")))]
fn init_audio_backend() {}

fn install_panic_hook() {
  let default_hook = panic::take_hook();
  panic::set_hook(Box::new(move |info| {
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

#[tokio::main]
async fn main() -> Result<()> {
  init_audio_backend();

  install_panic_hook();

  let mut clap_app = ClapApp::new(env!("CARGO_PKG_NAME"))
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about(env!("CARGO_PKG_DESCRIPTION"))
    .override_usage("Press `?` while running the app to see keybindings")
    .before_help(BANNER)
    .after_help(
      "Your spotify Client ID and Client Secret are stored in $HOME/.config/spotatui/client.yml",
    )
    .arg(
      Arg::new("tick-rate")
        .short('t')
        .long("tick-rate")
        .help("Set the tick rate (milliseconds): the lower the number the higher the FPS.")
        .long_help(
          "Specify the tick rate in milliseconds: the lower the number the \
higher the FPS. It can be nicer to have a lower value when you want to use the audio analysis view \
of the app. Beware that this comes at a CPU cost!",
        ),
    )
    .arg(
      Arg::new("config")
        .short('c')
        .long("config")
        .help("Specify configuration file path."),
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
    .subcommand(cli::search_subcommand())
    // Self-update command
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
    );

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
  if let Some(update_matches) = matches.subcommand_matches("update") {
    let do_install = update_matches.get_flag("install");
    return cli::check_for_update(do_install);
  }

  let mut user_config = UserConfig::new();
  if let Some(config_file_path) = matches.get_one::<String>("config") {
    let config_file_path = PathBuf::from(config_file_path);
    let path = UserConfigPaths { config_file_path };
    user_config.path_to_config.replace(path);
  }
  user_config.load_config()?;
  let initial_shuffle_enabled = user_config.behavior.shuffle_enabled;

  // Prompt for global song count opt-in if missing (only for interactive TUI, not CLI)
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
      // Prompt if file is empty OR doesn't mention the setting
      config_string.trim().is_empty() || !config_string.contains("enable_global_song_count")
    } else {
      // For existing users (have client.yml but no config.yml), prompt them
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

      // Save the choice to config
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

  if let Some(tick_rate) = matches
    .get_one::<String>("tick-rate")
    .and_then(|tick_rate| tick_rate.parse().ok())
  {
    if tick_rate >= 1000 {
      panic!("Tick rate must be below 1000");
    } else {
      user_config.behavior.tick_rate_milliseconds = tick_rate;
    }
  }

  let mut client_config = ClientConfig::new();
  client_config.load_config()?;

  let config_paths = client_config.get_or_build_paths()?;

  // Start authorization with spotify
  let creds = Credentials::new(&client_config.client_id, &client_config.client_secret);

  let oauth = OAuth {
    redirect_uri: client_config.get_redirect_uri(),
    scopes: SCOPES.iter().map(|s| s.to_string()).collect(),
    ..Default::default()
  };

  let config = Config {
    cache_path: config_paths.token_cache_path.clone(),
    ..Default::default()
  };

  let mut spotify = AuthCodeSpotify::with_config(creds, oauth, config);

  let config_port = client_config.get_port();

  // Try to load token from our manual cache
  let needs_auth = match load_token_from_file(&spotify, &config_paths.token_cache_path).await {
    Ok(true) => false,
    Ok(false) => {
      println!("No cached token found, need to authenticate");
      true
    }
    Err(e) => {
      println!("Failed to read token cache: {}", e);
      true
    }
  };

  if needs_auth {
    // If token is not in cache, get it from web flow
    // Get the authorization URL first
    let auth_url = spotify.get_authorize_url(false)?;

    // Try to open the URL in the browser
    println!("\nAttempting to open this URL in your browser:");
    println!("{}\n", auth_url);

    if let Err(e) = open::that(&auth_url) {
      println!("Failed to open browser automatically: {}", e);
      println!("Please manually open the URL above in your browser.");
    }

    println!(
      "Waiting for authorization callback on http://127.0.0.1:{}...\n",
      config_port
    );

    match redirect_uri_web_server(&mut spotify, config_port) {
      Ok(url) => {
        if let Some(code) = spotify.parse_response_code(&url) {
          spotify.request_token(&code).await?;
          // Write the token to our manual cache
          save_token_to_file(&spotify, &config_paths.token_cache_path).await?;
          println!("✓ Successfully authenticated with Spotify!");
        } else {
          return Err(anyhow!(
            "Failed to parse authorization code from callback URL"
          ));
        }
      }
      Err(()) => {
        println!("Starting webserver failed. Continuing with manual authentication");
        println!("Please open this URL in your browser: {}", auth_url);
        println!("Enter the URL you were redirected to: ");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if let Some(code) = spotify.parse_response_code(&input) {
          spotify.request_token(&code).await?;
          // Write the token to our manual cache
          save_token_to_file(&spotify, &config_paths.token_cache_path).await?;
        } else {
          return Err(anyhow!("Failed to parse authorization code from input URL"));
        }
      }
    }
  }

  // Verify that we have a valid token before proceeding
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  let token_expiry = if let Some(ref token) = *token_lock {
    // Convert TimeDelta to SystemTime
    let expires_in_secs = token.expires_in.num_seconds() as u64;
    SystemTime::now()
      .checked_add(std::time::Duration::from_secs(expires_in_secs))
      .unwrap_or_else(SystemTime::now)
  } else {
    drop(token_lock);
    return Err(anyhow!("Authentication failed: no valid token available"));
  };
  drop(token_lock); // Release the lock

  let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel::<IoEvent>();

  // Initialise app state
  let app = Arc::new(Mutex::new(App::new(
    sync_io_tx,
    user_config.clone(),
    token_expiry,
  )));

  // Work with the cli (not really async)
  if let Some(cmd) = matches.subcommand_name() {
    // Save, because we checked if the subcommand is present at runtime
    let m = matches.subcommand_matches(cmd).unwrap();
    #[cfg(feature = "streaming")]
    let network = Network::new(spotify, client_config, &app, None); // CLI doesn't use streaming
    #[cfg(not(feature = "streaming"))]
    let network = Network::new(spotify, client_config, &app);
    println!(
      "{}",
      cli::handle_matches(m, cmd.to_string(), network, user_config).await?
    );
  // Launch the UI (async)
  } else {
    // Initialize streaming player if enabled
    #[cfg(feature = "streaming")]
    let streaming_player = if client_config.enable_streaming {
      let streaming_config = player::StreamingConfig {
        device_name: client_config.streaming_device_name.clone(),
        bitrate: client_config.streaming_bitrate,
        audio_cache: client_config.streaming_audio_cache,
        cache_path: player::get_default_cache_path(),
        initial_volume: user_config.behavior.volume_percent,
      };

      let client_id = client_config.client_id.clone();
      let redirect_uri = client_config.get_redirect_uri();

      let mut init_handle = tokio::spawn(async move {
        player::StreamingPlayer::new(&client_id, &redirect_uri, streaming_config).await
      });

      let init_timeout_secs = std::env::var("SPOTATUI_STREAMING_INIT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(30);

      let init_result = tokio::select! {
        res = &mut init_handle => Some(res),
        _ = tokio::time::sleep(std::time::Duration::from_secs(init_timeout_secs)) => {
          init_handle.abort();
          None
        }
      };

      match init_result {
        Some(Ok(Ok(p))) => {
          println!("Streaming player initialized as '{}'", p.device_name());
          // Note: We don't activate() here - that's handled by AutoSelectStreamingDevice
          // which respects the user's saved device preference (e.g., spotifyd)
          Some(Arc::new(p))
        }
        Some(Ok(Err(e))) => {
          println!("Failed to initialize streaming: {}", e);
          println!("Falling back to API-based playback control");
          None
        }
        Some(Err(e)) => {
          println!("Streaming initialization panicked: {}", e);
          println!("Falling back to API-based playback control");
          None
        }
        None => {
          println!(
            "Streaming initialization timed out after {}s; falling back to API-based playback control (set SPOTATUI_STREAMING_INIT_TIMEOUT_SECS to adjust)",
            init_timeout_secs
          );
          None
        }
      }
    } else {
      None
    };

    #[cfg(feature = "streaming")]
    if streaming_player.is_some() {
      println!("Native playback enabled - 'spotatui' is available as a Spotify Connect device");
    }

    // Store streaming player reference in App for direct control (bypasses event channel)
    #[cfg(feature = "streaming")]
    {
      let mut app_mut = app.lock().await;
      app_mut.streaming_player = streaming_player.clone();
    }

    // Clone streaming player and device name for use in network spawn
    #[cfg(feature = "streaming")]
    let streaming_player_clone = streaming_player.clone();
    #[cfg(feature = "streaming")]
    let streaming_device_name = streaming_player
      .as_ref()
      .map(|p| p.device_name().to_string());

    // Create shared atomic for real-time position updates from native player
    // This avoids lock contention - the player event handler can update position
    // without needing to acquire the app mutex
    #[cfg(feature = "streaming")]
    let shared_position = Arc::new(AtomicU64::new(0));
    #[cfg(feature = "streaming")]
    let shared_position_for_events = Arc::clone(&shared_position);
    #[cfg(feature = "streaming")]
    let shared_position_for_ui = Arc::clone(&shared_position);

    // Create shared atomic for playing state (lock-free for MPRIS toggle)
    #[cfg(feature = "streaming")]
    let shared_is_playing = Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(feature = "streaming")]
    let shared_is_playing_for_events = Arc::clone(&shared_is_playing);
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let shared_is_playing_for_mpris = Arc::clone(&shared_is_playing);
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let shared_position_for_mpris = Arc::clone(&shared_position);
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    let shared_is_playing_for_macos = Arc::clone(&shared_is_playing);

    // Initialize MPRIS D-Bus integration for desktop media control
    // This registers spotatui as a controllable media player on the session bus
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let mpris_manager: Option<Arc<mpris::MprisManager>> = if streaming_player.is_some() {
      match mpris::MprisManager::new() {
        Ok(mgr) => {
          println!("MPRIS D-Bus interface registered - media keys and playerctl enabled");
          Some(Arc::new(mgr))
        }
        Err(e) => {
          println!(
            "Failed to initialize MPRIS: {} - media key control disabled",
            e
          );
          None
        }
      }
    } else {
      None
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
            println!("macOS Now Playing interface registered - media keys enabled");
            Some(Arc::new(mgr))
          }
          Err(e) => {
            println!(
              "Failed to initialize macOS media control: {} - media keys disabled",
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
      resolve_discord_app_id(&user_config)
        .and_then(|app_id| discord_rpc::DiscordRpcManager::new(app_id).ok())
    } else {
      None
    };
    #[cfg(not(feature = "discord-rpc"))]
    let discord_rpc_manager: DiscordRpcHandle = None;

    // Spawn MPRIS event handler to process external control requests (media keys, playerctl)
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    if let Some(ref mpris) = mpris_manager {
      if let Some(event_rx) = mpris.take_event_rx() {
        let streaming_player_for_mpris = streaming_player.clone();
        let mpris_for_seek = Arc::clone(mpris);
        let app_for_mpris = Arc::clone(&app);
        tokio::spawn(async move {
          handle_mpris_events(
            event_rx,
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
        let streaming_player_for_macos = streaming_player.clone();
        tokio::spawn(async move {
          handle_macos_media_events(
            event_rx,
            streaming_player_for_macos,
            shared_is_playing_for_macos,
          )
          .await;
        });
      }
    }

    // Clone MPRIS manager for player event handler
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let mpris_for_events = mpris_manager.clone();

    // Clone MPRIS manager for UI loop (to update status on device changes)
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let mpris_for_ui = mpris_manager.clone();

    // Spawn player event listener (updates app state from native player events)
    #[cfg(feature = "streaming")]
    if let Some(ref player) = streaming_player {
      let event_rx = player.get_event_channel();
      let app_for_events = Arc::clone(&app);
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      tokio::spawn(async move {
        handle_player_events(
          event_rx,
          app_for_events,
          shared_position_for_events,
          shared_is_playing_for_events,
          mpris_for_events,
        )
        .await;
      });
      #[cfg(not(all(feature = "mpris", target_os = "linux")))]
      tokio::spawn(async move {
        handle_player_events(
          event_rx,
          app_for_events,
          shared_position_for_events,
          shared_is_playing_for_events,
        )
        .await;
      });
    }

    let cloned_app = Arc::clone(&app);
    tokio::spawn(async move {
      #[cfg(feature = "streaming")]
      let mut network = Network::new(spotify, client_config, &app, streaming_player_clone);
      #[cfg(not(feature = "streaming"))]
      let mut network = Network::new(spotify, client_config, &app);

      // Auto-select the saved playback device when available (fallback to native streaming).
      #[cfg(feature = "streaming")]
      if let Some(device_name) = streaming_device_name {
        let saved_device_id = network.client_config.device_id.clone();
        let mut devices_snapshot = None;

        if let Ok(devices_vec) = network.spotify.device().await {
          let mut app = network.app.lock().await;
          app.devices = Some(rspotify::model::device::DevicePayload {
            devices: devices_vec.clone(),
          });
          devices_snapshot = Some(devices_vec);
        }

        let mut status_message = None;
        let startup_event = match saved_device_id {
          Some(saved_device_id) => {
            if let Some(devices_vec) = devices_snapshot.as_ref() {
              if devices_vec
                .iter()
                .any(|device| device.id.as_ref() == Some(&saved_device_id))
              {
                Some(IoEvent::TransferPlaybackToDevice(saved_device_id, true))
              } else {
                status_message = Some(format!("Saved device unavailable; using {}", device_name));
                let native_device_id = devices_vec
                  .iter()
                  .find(|device| device.name.eq_ignore_ascii_case(&device_name))
                  .and_then(|device| device.id.clone());
                if let Some(native_device_id) = native_device_id {
                  Some(IoEvent::TransferPlaybackToDevice(native_device_id, false))
                } else {
                  Some(IoEvent::AutoSelectStreamingDevice(
                    device_name.clone(),
                    false,
                  ))
                }
              }
            } else {
              Some(IoEvent::TransferPlaybackToDevice(saved_device_id, true))
            }
          }
          None => Some(IoEvent::AutoSelectStreamingDevice(
            device_name.clone(),
            true,
          )),
        };

        if let Some(message) = status_message {
          let mut app = network.app.lock().await;
          app.status_message = Some(message);
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(5));
        }

        if let Some(event) = startup_event {
          network.handle_network_event(event).await;
        }
      }

      // Apply saved shuffle preference on startup
      network
        .handle_network_event(IoEvent::Shuffle(initial_shuffle_enabled))
        .await;

      start_tokio(sync_io_rx, &mut network).await;
    });
    // The UI must run in the "main" thread
    #[cfg(all(feature = "streaming", feature = "mpris", target_os = "linux"))]
    start_ui(
      user_config,
      &cloned_app,
      Some(shared_position_for_ui),
      mpris_for_ui,
      discord_rpc_manager,
    )
    .await?;
    #[cfg(all(
      feature = "streaming",
      not(all(feature = "mpris", target_os = "linux"))
    ))]
    start_ui(
      user_config,
      &cloned_app,
      Some(shared_position_for_ui),
      None,
      discord_rpc_manager,
    )
    .await?;
    #[cfg(not(feature = "streaming"))]
    start_ui(user_config, &cloned_app, None, None, discord_rpc_manager).await?;
  }

  Ok(())
}

async fn start_tokio(io_rx: std::sync::mpsc::Receiver<IoEvent>, network: &mut Network) {
  while let Ok(io_event) = io_rx.recv() {
    network.handle_network_event(io_event).await;
  }
}

/// Handle player events from librespot and update app state directly
/// This bypasses the Spotify Web API for instant UI updates
#[cfg(all(feature = "streaming", feature = "mpris", target_os = "linux"))]
async fn handle_player_events(
  mut event_rx: librespot_playback::player::PlayerEventChannel,
  app: Arc<Mutex<App>>,
  shared_position: Arc<AtomicU64>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
  mpris_manager: Option<Arc<mpris::MprisManager>>,
) {
  use chrono::TimeDelta;
  use player::PlayerEvent;
  use std::sync::atomic::Ordering;

  while let Some(event) = event_rx.recv().await {
    // Use try_lock() to avoid blocking when the UI thread is busy
    // If we can't get the lock, skip this update - the UI will catch up on the next tick
    match event {
      PlayerEvent::Playing {
        play_request_id: _,
        track_id,
        position_ms,
      } => {
        // Always update atomic - this never fails (lock-free for MPRIS)
        shared_is_playing.store(true, Ordering::Relaxed);

        // Update MPRIS playback status
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(true);
        }

        // Always update native_is_playing - this is critical for UI state
        // Use blocking lock since this is a brief operation
        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(true);
        }

        // Try to get lock for other updates - skip if busy
        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;

          // Update is_playing state
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = true;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }

          // Reset the poll timer so we don't immediately overwrite with stale API data
          app.instant_since_last_current_playback_poll = std::time::Instant::now();

          // Check if track changed and dispatch fetch
          let track_id_str = track_id.to_string();
          if app.last_track_id.as_ref() != Some(&track_id_str) {
            app.last_track_id = Some(track_id_str);
            app.dispatch(IoEvent::GetCurrentPlayback);
          }
        }
      }
      PlayerEvent::Paused {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        // Always update atomic - this never fails (lock-free for MPRIS)
        shared_is_playing.store(false, Ordering::Relaxed);

        // Update MPRIS playback status
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(false);
        }

        // Always update native_is_playing - this is critical for UI state
        // Use blocking lock since this is a brief operation
        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(false);
        }

        // Try to get lock for other updates - skip if busy
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
        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;
          app.seek_ms = None;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::TrackChanged { audio_item } => {
        // Track metadata changed - extract immediate info for instant UI updates
        use librespot_metadata::audio::UniqueFields;

        // Extract artist names and album from UniqueFields
        let (artists, album) = match &audio_item.unique_fields {
          UniqueFields::Track { artists, album, .. } => {
            // Extract artist names from ArtistsWithRole
            let artist_names: Vec<String> = artists.0.iter().map(|a| a.name.clone()).collect();
            (artist_names, album.clone())
          }
          UniqueFields::Episode { show_name, .. } => (vec![show_name.clone()], String::new()),
          UniqueFields::Local { artists, album, .. } => {
            let artist_vec = artists
              .as_ref()
              .map(|a| vec![a.clone()])
              .unwrap_or_default();
            let album_str = album.clone().unwrap_or_default();
            (artist_vec, album_str)
          }
        };

        // Update MPRIS metadata
        if let Some(ref mpris) = mpris_manager {
          mpris.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms,
            None,
          );
        }

        if let Ok(mut app) = app.try_lock() {
          // Store immediate track info for instant UI display
          app.native_track_info = Some(app::NativeTrackInfo {
            name: audio_item.name.clone(),
            artists_display: artists.join(", "),
            album: album.clone(),
            duration_ms: audio_item.duration_ms,
          });

          app.song_progress_ms = 0;
          app.last_track_id = Some(audio_item.track_id.to_string());
          // Reset the poll timer so we don't immediately overwrite with stale API data
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
          app.dispatch(IoEvent::GetCurrentPlayback);
        }
      }
      PlayerEvent::Stopped { .. } => {
        // Update MPRIS status
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        // When a track stops, refresh state.
        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          // Clear the last track ID so the next Playing event will trigger a full refresh
          app.last_track_id = None;
        }

        // Small delay to let Spotify's backend transition
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to dispatch - skip if busy
        if let Ok(mut app) = app.try_lock() {
          app.dispatch(IoEvent::GetCurrentPlayback);
        }
      }
      PlayerEvent::EndOfTrack { track_id, .. } => {
        // Update MPRIS status
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
        }

        // Ensure we don't land on the next item paused after the track transition.
        // (librespot Spirc will advance; we may need to resume playback.)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(mut app) = app.try_lock() {
          app.dispatch(IoEvent::EnsurePlaybackContinues(track_id.to_string()));
        }
      }
      PlayerEvent::VolumeChanged { volume } => {
        // Update MPRIS volume
        let volume_percent = ((volume as f64 / 65535.0) * 100.0).round() as u8;
        if let Some(ref mpris) = mpris_manager {
          mpris.set_volume(volume_percent);
        }

        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.device.volume_percent = Some(volume_percent as u32);
          }
          // Persist the latest volume so it is restored on next launch
          app.user_config.behavior.volume_percent = volume_percent.min(100);
          let _ = app.user_config.save_config();
        }
      }
      PlayerEvent::PositionChanged {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        // Use atomic store for lock-free position updates
        // This never blocks or fails, ensuring every position update is captured
        shared_position.store(position_ms as u64, Ordering::Relaxed);

        // Update MPRIS position so external clients (playerctl, desktop widgets) stay in sync
        if let Some(ref mpris) = mpris_manager {
          mpris.set_position(position_ms as u64);
        }
      }
      _ => {
        // Ignore other events
      }
    }
  }
}

/// Handle player events from librespot and update app state directly
/// This bypasses the Spotify Web API for instant UI updates
#[cfg(all(
  feature = "streaming",
  not(all(feature = "mpris", target_os = "linux"))
))]
async fn handle_player_events(
  mut event_rx: librespot_playback::player::PlayerEventChannel,
  app: Arc<Mutex<App>>,
  shared_position: Arc<AtomicU64>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
) {
  use chrono::TimeDelta;
  use player::PlayerEvent;
  use std::sync::atomic::Ordering;

  while let Some(event) = event_rx.recv().await {
    match event {
      PlayerEvent::Playing {
        play_request_id: _,
        track_id,
        position_ms,
      } => {
        shared_is_playing.store(true, Ordering::Relaxed);
        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(true);
        }
        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = true;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
          let track_id_str = track_id.to_string();
          if app.last_track_id.as_ref() != Some(&track_id_str) {
            app.last_track_id = Some(track_id_str);
            app.dispatch(IoEvent::GetCurrentPlayback);
          }
        }
      }
      PlayerEvent::Paused {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        shared_is_playing.store(false, Ordering::Relaxed);
        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(false);
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
        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;
          app.seek_ms = None;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::TrackChanged { audio_item } => {
        if let Ok(mut app) = app.try_lock() {
          use librespot_metadata::audio::UniqueFields;
          let (artists, album) = match &audio_item.unique_fields {
            UniqueFields::Track { artists, album, .. } => {
              let artist_names: Vec<String> = artists.0.iter().map(|a| a.name.clone()).collect();
              (artist_names, album.clone())
            }
            UniqueFields::Episode { show_name, .. } => (vec![show_name.clone()], String::new()),
            UniqueFields::Local { artists, album, .. } => {
              let artist_vec = artists
                .as_ref()
                .map(|a| vec![a.clone()])
                .unwrap_or_default();
              let album_str = album.clone().unwrap_or_default();
              (artist_vec, album_str)
            }
          };
          app.native_track_info = Some(app::NativeTrackInfo {
            name: audio_item.name.clone(),
            artists_display: artists.join(", "),
            album,
            duration_ms: audio_item.duration_ms,
          });
          app.song_progress_ms = 0;
          app.last_track_id = Some(audio_item.track_id.to_string());
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
          app.dispatch(IoEvent::GetCurrentPlayback);
        }
      }
      PlayerEvent::Stopped { .. } => {
        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(mut app) = app.try_lock() {
          app.dispatch(IoEvent::GetCurrentPlayback);
        }
      }
      PlayerEvent::EndOfTrack { track_id, .. } => {
        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(mut app) = app.try_lock() {
          app.dispatch(IoEvent::EnsurePlaybackContinues(track_id.to_string()));
        }
      }
      PlayerEvent::VolumeChanged { volume } => {
        if let Ok(mut app) = app.try_lock() {
          let volume_percent = ((volume as f64 / 65535.0) * 100.0).round() as u32;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.device.volume_percent = Some(volume_percent);
          }
          app.user_config.behavior.volume_percent = volume_percent.min(100) as u8;
          let _ = app.user_config.save_config();
        }
      }
      PlayerEvent::PositionChanged {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        shared_position.store(position_ms as u64, Ordering::Relaxed);
      }
      _ => {}
    }
  }
}

/// Handle MPRIS events from external clients (media keys, playerctl, etc.)
/// Routes control requests to the native streaming player
#[cfg(all(feature = "mpris", target_os = "linux"))]
async fn handle_mpris_events(
  mut event_rx: tokio::sync::mpsc::UnboundedReceiver<mpris::MprisEvent>,
  streaming_player: Option<Arc<player::StreamingPlayer>>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
  shared_position: Arc<AtomicU64>,
  mpris_manager: Arc<mpris::MprisManager>,
  app: Arc<Mutex<App>>,
) {
  use mpris::MprisEvent;
  use std::sync::atomic::Ordering;

  let Some(player) = streaming_player else {
    // No streaming player, nothing to control
    return;
  };

  while let Some(event) = event_rx.recv().await {
    match event {
      MprisEvent::PlayPause => {
        // Toggle based on atomic state (lock-free, always up-to-date)
        if shared_is_playing.load(Ordering::Relaxed) {
          player.pause();
        } else {
          player.play();
        }
      }
      MprisEvent::Play => {
        player.play();
      }
      MprisEvent::Pause => {
        player.pause();
      }
      MprisEvent::Next => {
        player.activate();
        player.next();
        // Keep Connect + audio state in sync.
        player.play();
      }
      MprisEvent::Previous => {
        player.activate();
        player.prev();
        // Keep Connect + audio state in sync.
        player.play();
      }
      MprisEvent::Stop => {
        player.stop();
      }
      MprisEvent::Seek(offset_micros) => {
        // MPRIS sends relative offset in microseconds (can be negative for rewind)
        // We need to calculate: new_absolute_position = current_position + offset

        // Get current position (stored in milliseconds)
        let current_ms = shared_position.load(Ordering::Relaxed) as i64;

        // Convert offset from microseconds to milliseconds
        let offset_ms = offset_micros / 1000;

        // Calculate new position, clamping to prevent going negative
        let new_position_ms = (current_ms + offset_ms).max(0) as u32;

        // Seek the player
        player.seek(new_position_ms);

        // Update shared position immediately so UI reflects the change
        shared_position.store(new_position_ms as u64, Ordering::Relaxed);

        // Update app's song_progress_ms so UI updates even when paused
        if let Ok(mut app_lock) = app.try_lock() {
          app_lock.song_progress_ms = new_position_ms as u128;
        }

        // Emit Seeked signal so external clients know position jumped
        mpris_manager.emit_seeked(new_position_ms as u64);
      }
      MprisEvent::SetPosition(position_micros) => {
        // MPRIS SetPosition sends absolute position in microseconds
        // Convert to milliseconds and seek directly
        let new_position_ms = (position_micros / 1000).max(0) as u32;

        // Seek the player
        player.seek(new_position_ms);

        // Update shared position immediately so UI reflects the change
        shared_position.store(new_position_ms as u64, Ordering::Relaxed);

        // Update app's song_progress_ms so UI updates even when paused
        if let Ok(mut app_lock) = app.try_lock() {
          app_lock.song_progress_ms = new_position_ms as u128;
        }

        // Emit Seeked signal so external clients know position jumped
        mpris_manager.emit_seeked(new_position_ms as u64);
      }
      MprisEvent::SetShuffle(shuffle) => {
        if let Err(e) = player.set_shuffle(shuffle) {
          eprintln!("MPRIS: Failed to set shuffle: {}", e);
        } else {
          // Update MPRIS state so clients see the new value
          mpris_manager.set_shuffle(shuffle);
          // Update app UI state (use await to ensure update happens)
          let mut app_lock = app.lock().await;
          if let Some(ref mut ctx) = app_lock.current_playback_context {
            ctx.shuffle_state = shuffle;
          }
          app_lock.user_config.behavior.shuffle_enabled = shuffle;
        }
      }
      MprisEvent::SetLoopStatus(loop_status) => {
        use mpris::LoopStatusEvent;
        use rspotify::model::enums::RepeatState;

        // Map MPRIS LoopStatus to Spotify RepeatState
        let repeat_state = match loop_status {
          LoopStatusEvent::None => RepeatState::Off,
          LoopStatusEvent::Track => RepeatState::Track,
          LoopStatusEvent::Playlist => RepeatState::Context,
        };

        if let Err(e) = player.set_repeat_mode(repeat_state) {
          eprintln!("MPRIS: Failed to set repeat mode: {}", e);
        } else {
          // Update MPRIS state so clients see the new value
          mpris_manager.set_loop_status(loop_status);
          // Update app UI state (use await to ensure update happens)
          let mut app_lock = app.lock().await;
          if let Some(ref mut ctx) = app_lock.current_playback_context {
            ctx.repeat_state = repeat_state;
          }
        }
      }
    }
  }
}

/// Handle macOS media events from external sources (media keys, Control Center, AirPods, etc.)
/// Routes control requests to the native streaming player
#[cfg(all(feature = "macos-media", target_os = "macos"))]
async fn handle_macos_media_events(
  mut event_rx: tokio::sync::mpsc::UnboundedReceiver<macos_media::MacMediaEvent>,
  streaming_player: Option<Arc<player::StreamingPlayer>>,
  shared_is_playing: Arc<std::sync::atomic::AtomicBool>,
) {
  use macos_media::MacMediaEvent;
  use std::sync::atomic::Ordering;

  let Some(player) = streaming_player else {
    // No streaming player, nothing to control
    return;
  };

  while let Some(event) = event_rx.recv().await {
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

#[cfg(all(feature = "mpris", target_os = "linux"))]
async fn start_ui(
  user_config: UserConfig,
  app: &Arc<Mutex<App>>,
  shared_position: Option<Arc<AtomicU64>>,
  mpris_manager: Option<Arc<mpris::MprisManager>>,
  discord_rpc_manager: DiscordRpcHandle,
) -> Result<()> {
  #[cfg(not(feature = "discord-rpc"))]
  let _ = discord_rpc_manager;
  // Terminal initialization
  let mut terminal = ratatui::init();
  execute!(stdout(), EnableMouseCapture)?;

  if user_config.behavior.set_window_title {
    execute!(stdout(), SetTitle("spt - spotatui"))?;
  }

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

  // Track previous streaming state to detect device changes for MPRIS
  // When switching from native streaming to external device (like spotifyd),
  // we set MPRIS to stopped so the external player's MPRIS takes precedence
  let mut prev_is_streaming_active = false;

  // Lazy audio capture: only capture when in Analysis view
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  let mut audio_capture: Option<audio::AudioCaptureManager> = None;

  #[cfg(feature = "discord-rpc")]
  let mut discord_presence_state = DiscordPresenceState::default();

  #[cfg(feature = "mpris")]
  let mut mpris_metadata_state: Option<MprisMetadata> = None;

  // Update check will run async after first render to avoid blocking startup
  let mut update_check_spawned = false;
  let mut is_first_render = true;

  loop {
    let terminal_size = terminal.backend().size().ok();
    {
      let mut app = app.lock().await;

      // MPRIS device change detection: When switching from native streaming to
      // an external device (like spotifyd), set MPRIS to stopped so the external
      // player's MPRIS interface takes precedence in desktop widgets
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      {
        let current_is_streaming_active = app.is_streaming_active;
        if prev_is_streaming_active && !current_is_streaming_active {
          // Switched away from native streaming to external device
          if let Some(ref mpris) = mpris_manager {
            mpris.set_stopped();
          }
        }
        prev_is_streaming_active = current_is_streaming_active;
      }

      // Get the size of the screen on each loop to account for resize event
      if let Some(size) = terminal_size {
        // Reset the help menu is the terminal was resized
        if is_first_render || app.size != size {
          app.help_menu_max_lines = 0;
          app.help_menu_offset = 0;
          app.help_menu_page = 0;

          app.size = size;

          // Based on the size of the terminal, adjust the search limit.
          let potential_limit = max((app.size.height as i32) - 13, 0) as u32;
          let max_limit = min(potential_limit, 50);
          let large_search_limit = min((f32::from(size.height) / 1.4) as u32, max_limit);
          let small_search_limit = min((f32::from(size.height) / 2.85) as u32, max_limit / 2);

          app.dispatch(IoEvent::UpdateSearchLimits(
            large_search_limit,
            small_search_limit,
          ));

          // Based on the size of the terminal, adjust how many lines are
          // displayed in the help menu
          if app.size.height > 8 {
            app.help_menu_max_lines = (app.size.height as u32) - 8;
          } else {
            app.help_menu_max_lines = 0;
          }
        }
      };

      let current_route = app.get_current_route();
      terminal.draw(|f| match current_route.active_block {
        ActiveBlock::HelpMenu => {
          ui::draw_help_menu(f, &app);
        }
        ActiveBlock::Error => {
          ui::draw_error_screen(f, &app);
        }
        ActiveBlock::SelectDevice => {
          ui::draw_device_list(f, &app);
        }
        ActiveBlock::Analysis => {
          ui::audio_analysis::draw(f, &app);
        }
        ActiveBlock::BasicView => {
          ui::draw_basic_view(f, &app);
        }
        ActiveBlock::UpdatePrompt => {
          ui::draw_update_prompt(f, &app);
        }
        ActiveBlock::Settings => {
          ui::settings::draw_settings(f, &app);
        }
        _ => {
          ui::draw_main_layout(f, &app);
        }
      })?;

      if current_route.active_block == ActiveBlock::Input {
        terminal.show_cursor()?;
      } else {
        terminal.hide_cursor()?;
      }

      let cursor_offset = if app.size.height > ui::util::SMALL_TERMINAL_HEIGHT {
        2
      } else {
        1
      };

      // Put the cursor back inside the input box
      terminal.backend_mut().execute(MoveTo(
        cursor_offset + app.input_cursor_position,
        cursor_offset,
      ))?;

      // Handle authentication refresh
      if SystemTime::now() > app.spotify_token_expiry {
        app.dispatch(IoEvent::RefreshAuthentication);
      }
    }

    match events.next()? {
      event::Event::Input(key) => {
        let mut app = app.lock().await;
        if key == Key::Ctrl('c') {
          app.close_io_channel();
          break;
        }

        let current_active_block = app.get_current_route().active_block;

        // To avoid swallowing the global key presses `q` and `-` make a special
        // case for the input handler
        if current_active_block == ActiveBlock::Input {
          handlers::input_handler(key, &mut app);
        } else if key == app.user_config.keys.back {
          if app.get_current_route().active_block != ActiveBlock::Input {
            // Go back through navigation stack when not in search input mode and exit the app if there are no more places to back to

            let pop_result = match app.pop_navigation_stack() {
              Some(ref x) if x.id == RouteId::Search => app.pop_navigation_stack(),
              Some(x) => Some(x),
              None => None,
            };
            if pop_result.is_none() {
              app.close_io_channel();
              break; // Exit application
            }
          }
        } else {
          handlers::handle_app(key, &mut app);
        }
      }
      event::Event::Tick => {
        let mut app = app.lock().await;
        app.update_on_tick();

        // Flush any pending seeks (throttled to avoid overwhelming player/API)
        #[cfg(feature = "streaming")]
        app.flush_pending_native_seek();
        app.flush_pending_api_seek();

        #[cfg(feature = "discord-rpc")]
        if let Some(ref manager) = discord_rpc_manager {
          update_discord_presence(manager, &mut discord_presence_state, &app);
        }

        #[cfg(feature = "mpris")]
        if let Some(ref mpris) = mpris_manager {
          update_mpris_metadata(mpris, &mut mpris_metadata_state, &app);
        }

        // Read position from shared atomic if native streaming is active
        // This provides lock-free real-time updates from player events
        // Skip if we recently seeked - let the UI show our target position until the player catches up
        #[cfg(feature = "streaming")]
        if let Some(ref pos) = shared_position {
          if app.is_streaming_active {
            let recently_seeked = app
              .last_native_seek
              .is_some_and(|t| t.elapsed().as_millis() < app::SEEK_POSITION_IGNORE_MS);

            if !recently_seeked {
              let position_ms = pos.load(Ordering::Relaxed);
              if position_ms > 0 {
                app.song_progress_ms = position_ms as u128;
              }
            }
          }
        }
        #[cfg(not(feature = "streaming"))]
        if let Some(ref pos) = shared_position {
          if app.is_streaming_active {
            let position_ms = pos.load(Ordering::Relaxed);
            if position_ms > 0 {
              app.song_progress_ms = position_ms as u128;
            }
          }
        }

        // Lazy audio capture: only capture when in Analysis view
        #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
        {
          let in_analysis_view = app.get_current_route().active_block == ActiveBlock::Analysis;

          if in_analysis_view {
            if audio_capture.is_none() {
              audio_capture = audio::AudioCaptureManager::new();
              app.audio_capture_active = audio_capture.is_some();
            }

            if let Some(ref capture) = audio_capture {
              if let Some(spectrum) = capture.get_spectrum() {
                app.spectrum_data = Some(app::SpectrumData {
                  bands: spectrum.bands,
                  peak: spectrum.peak,
                });
                app.audio_capture_active = capture.is_active();
              }
            }
          } else if audio_capture.is_some() {
            audio_capture = None;
            app.audio_capture_active = false;
            app.spectrum_data = None;
          }
        }
      }
    }

    // Delay spotify request until first render, will have the effect of improving
    // startup speed
    if is_first_render {
      let mut app = app.lock().await;
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetUser);
      app.dispatch(IoEvent::GetCurrentPlayback);
      if app.user_config.behavior.enable_global_song_count {
        app.dispatch(IoEvent::FetchGlobalSongCount);
      }
      app.help_docs_size = ui::help::get_help_docs(&app.user_config.keys).len() as u32;

      is_first_render = false;
    }

    // Check for updates async after first render to avoid blocking startup
    if !update_check_spawned {
      update_check_spawned = true;
      let app_for_update = Arc::clone(app);
      tokio::spawn(async move {
        if let Some(update_info) = tokio::task::spawn_blocking(cli::check_for_update_silent)
          .await
          .ok()
          .flatten()
        {
          let mut app = app_for_update.lock().await;
          app.update_available = Some(update_info);
          // Push the update prompt modal onto navigation stack
          app.push_navigation_stack(RouteId::UpdatePrompt, ActiveBlock::UpdatePrompt);
        }
      });
    }
  }

  execute!(stdout(), DisableMouseCapture)?;
  ratatui::restore();

  #[cfg(feature = "discord-rpc")]
  if let Some(ref manager) = discord_rpc_manager {
    manager.clear();
  }

  Ok(())
}

/// Non-MPRIS version of start_ui - used when mpris feature is disabled
#[cfg(not(all(feature = "mpris", target_os = "linux")))]
async fn start_ui(
  user_config: UserConfig,
  app: &Arc<Mutex<App>>,
  shared_position: Option<Arc<AtomicU64>>,
  _mpris_manager: Option<()>,
  discord_rpc_manager: DiscordRpcHandle,
) -> Result<()> {
  #[cfg(not(feature = "discord-rpc"))]
  let _ = discord_rpc_manager;
  #[cfg(not(feature = "streaming"))]
  let _ = shared_position;
  use ratatui::{prelude::Style, widgets::Block};

  // Terminal initialization
  let mut terminal = ratatui::init();
  execute!(stdout(), EnableMouseCapture)?;

  if user_config.behavior.set_window_title {
    execute!(stdout(), SetTitle("spt - spotatui"))?;
  }

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

  // Check for updates SYNCHRONOUSLY before starting the event loop
  {
    let update_info = tokio::task::spawn_blocking(cli::check_for_update_silent)
      .await
      .ok()
      .flatten();
    if let Some(info) = update_info {
      let mut app = app.lock().await;
      app.update_available = Some(info);
      app.push_navigation_stack(RouteId::UpdatePrompt, ActiveBlock::UpdatePrompt);
    }
  }

  // Lazy audio capture: only capture when in Analysis view
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  let mut audio_capture: Option<audio::AudioCaptureManager> = None;

  #[cfg(feature = "discord-rpc")]
  let mut discord_presence_state = DiscordPresenceState::default();

  let mut is_first_render = true;

  loop {
    let terminal_size = terminal.backend().size().ok();
    {
      let mut app = app.lock().await;

      if let Some(size) = terminal_size {
        if is_first_render || app.size != size {
          app.help_menu_max_lines = 0;
          app.help_menu_offset = 0;
          app.help_menu_page = 0;
          app.size = size;

          let potential_limit = max((app.size.height as i32) - 13, 0) as u32;
          let max_limit = min(potential_limit, 50);
          let large_search_limit = min((f32::from(size.height) / 1.4) as u32, max_limit);
          let small_search_limit = min((f32::from(size.height) / 2.85) as u32, max_limit / 2);

          app.dispatch(IoEvent::UpdateSearchLimits(
            large_search_limit,
            small_search_limit,
          ));

          if app.size.height > 8 {
            app.help_menu_max_lines = (app.size.height as u32) - 8;
          } else {
            app.help_menu_max_lines = 0;
          }
        }
      };

      let current_route = app.get_current_route();
      terminal.draw(|f| {
        f.render_widget(
          Block::default().style(Style::default().bg(app.user_config.theme.background)),
          f.area(),
        );
        match current_route.active_block {
          ActiveBlock::HelpMenu => ui::draw_help_menu(f, &app),
          ActiveBlock::Error => ui::draw_error_screen(f, &app),
          ActiveBlock::SelectDevice => ui::draw_device_list(f, &app),
          ActiveBlock::Analysis => ui::audio_analysis::draw(f, &app),
          ActiveBlock::BasicView => ui::draw_basic_view(f, &app),
          ActiveBlock::UpdatePrompt => ui::draw_update_prompt(f, &app),
          ActiveBlock::Settings => ui::settings::draw_settings(f, &app),
          _ => ui::draw_main_layout(f, &app),
        }
      })?;

      if current_route.active_block == ActiveBlock::Input {
        terminal.show_cursor()?;
      } else {
        terminal.hide_cursor()?;
      }

      let cursor_offset = if app.size.height > ui::util::SMALL_TERMINAL_HEIGHT {
        2
      } else {
        1
      };
      terminal.backend_mut().execute(MoveTo(
        cursor_offset + app.input_cursor_position,
        cursor_offset,
      ))?;

      if SystemTime::now() > app.spotify_token_expiry {
        app.dispatch(IoEvent::RefreshAuthentication);
      }
    }

    match events.next()? {
      event::Event::Input(key) => {
        let mut app = app.lock().await;
        if key == Key::Ctrl('c') {
          app.close_io_channel();
          break;
        }

        let current_active_block = app.get_current_route().active_block;

        if current_active_block == ActiveBlock::Input {
          handlers::input_handler(key, &mut app);
        } else if key == app.user_config.keys.back {
          if app.get_current_route().active_block != ActiveBlock::Input {
            let pop_result = match app.pop_navigation_stack() {
              Some(ref x) if x.id == RouteId::Search => app.pop_navigation_stack(),
              Some(x) => Some(x),
              None => None,
            };
            if pop_result.is_none() {
              app.close_io_channel();
              break;
            }
          }
        } else {
          handlers::handle_app(key, &mut app);
        }
      }
      event::Event::Tick => {
        let mut app = app.lock().await;
        app.update_on_tick();

        // Flush any pending seeks (throttled to avoid overwhelming player/API)
        #[cfg(feature = "streaming")]
        app.flush_pending_native_seek();
        app.flush_pending_api_seek();

        #[cfg(feature = "discord-rpc")]
        if let Some(ref manager) = discord_rpc_manager {
          update_discord_presence(manager, &mut discord_presence_state, &app);
        }

        // Read position from shared atomic if native streaming is active
        // Skip if we recently seeked - let the UI show our target position until the player catches up
        #[cfg(feature = "streaming")]
        if let Some(ref pos) = shared_position {
          let recently_seeked = app
            .last_native_seek
            .is_some_and(|t| t.elapsed().as_millis() < app::SEEK_POSITION_IGNORE_MS);

          if !recently_seeked {
            let pos_ms = pos.load(Ordering::Relaxed) as u128;
            if pos_ms > 0 && app.is_streaming_active {
              app.song_progress_ms = pos_ms;
            }
          }
        }

        // Lazy audio capture: only capture when in Analysis view
        #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
        {
          let in_analysis_view = app.get_current_route().active_block == ActiveBlock::Analysis;

          if in_analysis_view {
            if audio_capture.is_none() {
              audio_capture = audio::AudioCaptureManager::new();
              app.audio_capture_active = audio_capture.is_some();
            }

            if let Some(ref capture) = audio_capture {
              if let Some(spectrum) = capture.get_spectrum() {
                app.spectrum_data = Some(app::SpectrumData {
                  bands: spectrum.bands,
                  peak: spectrum.peak,
                });
                app.audio_capture_active = capture.is_active();
              }
            }
          } else if audio_capture.is_some() {
            audio_capture = None;
            app.audio_capture_active = false;
            app.spectrum_data = None;
          }
        }
      }
    }

    if is_first_render {
      let mut app = app.lock().await;
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetUser);
      app.dispatch(IoEvent::GetCurrentPlayback);
      if app.user_config.behavior.enable_global_song_count {
        app.dispatch(IoEvent::FetchGlobalSongCount);
      }
      app.help_docs_size = ui::help::get_help_docs(&app.user_config.keys).len() as u32;
      is_first_render = false;
    }
  }

  execute!(stdout(), DisableMouseCapture)?;
  ratatui::restore();

  #[cfg(feature = "discord-rpc")]
  if let Some(ref manager) = discord_rpc_manager {
    manager.clear();
  }

  Ok(())
}
