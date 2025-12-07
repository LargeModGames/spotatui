mod app;
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
mod audio;
mod banner;
mod cli;
mod config;
mod event;
mod handlers;
mod network;
mod redirect_uri;
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
  style::Print,
  terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, SetTitle,
  },
  ExecutableCommand,
};
use network::{IoEvent, Network};
use ratatui::{
  backend::{Backend, CrosstermBackend},
  Terminal,
};
use redirect_uri::redirect_uri_web_server;
use rspotify::{
  prelude::*,
  {AuthCodeSpotify, Config, Credentials, OAuth, Token},
};
use std::{
  cmp::{max, min},
  fs,
  io::{self, stdout},
  panic::{self, PanicHookInfo},
  path::PathBuf,
  sync::Arc,
  time::SystemTime,
};
use tokio::sync::Mutex;
use user_config::{UserConfig, UserConfigPaths};

const SCOPES: [&str; 14] = [
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
];

// Manual token cache helpers since rspotify's built-in caching isn't working
async fn save_token_to_file(spotify: &AuthCodeSpotify, path: &PathBuf) -> Result<()> {
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  if let Some(ref token) = *token_lock {
    let token_json = serde_json::to_string_pretty(token)?;
    fs::write(path, token_json)?;
    println!("✓ Token saved to {}", path.display());
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

  println!("✓ Found cached authentication token");
  Ok(true)
}

fn close_application() -> Result<()> {
  disable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
  Ok(())
}

fn panic_hook(info: &PanicHookInfo<'_>) {
  if cfg!(debug_assertions) {
    let location = info.location().unwrap();

    let msg = match info.payload().downcast_ref::<&'static str>() {
      Some(s) => *s,
      None => match info.payload().downcast_ref::<String>() {
        Some(s) => &s[..],
        None => "Box<Any>",
      },
    };

    let stacktrace: String = format!("{:?}", Backtrace::new()).replace('\n', "\n\r");

    disable_raw_mode().unwrap();
    execute!(
      io::stdout(),
      LeaveAlternateScreen,
      Print(format!(
        "thread '<unnamed>' panicked at '{}', {}\n\r{}",
        msg, location, stacktrace
      )),
      DisableMouseCapture
    )
    .unwrap();
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  panic::set_hook(Box::new(|info| {
    panic_hook(info);
  }));

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
      println!("📊 Global Song Counter");
      println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
      println!("\nSpotatui can contribute to a global counter showing total");
      println!("songs played by all users worldwide.");
      println!("\n🔒 Privacy: This feature is completely anonymous.");
      println!("   • No personal information is collected");
      println!("   • No song names, artists, or listening history");
      println!("   • Only a simple increment when a new song starts");
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
        println!("✓ Thank you for participating!\n");
      } else {
        println!("✓ Opted out. You can change this anytime in ~/.config/spotatui/config.yml\n");
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
    let network = Network::new(spotify, client_config, &app);
    println!(
      "{}",
      cli::handle_matches(m, cmd.to_string(), network, user_config).await?
    );
  // Launch the UI (async)
  } else {
    let cloned_app = Arc::clone(&app);
    tokio::spawn(async move {
      let mut network = Network::new(spotify, client_config, &app);
      start_tokio(sync_io_rx, &mut network).await;
    });
    // The UI must run in the "main" thread
    start_ui(user_config, &cloned_app).await?;
  }

  Ok(())
}

async fn start_tokio(io_rx: std::sync::mpsc::Receiver<IoEvent>, network: &mut Network) {
  while let Ok(io_event) = io_rx.recv() {
    network.handle_network_event(io_event).await;
  }
}

async fn start_ui(user_config: UserConfig, app: &Arc<Mutex<App>>) -> Result<()> {
  // Terminal initialization
  let mut stdout = stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
  enable_raw_mode()?;

  let mut backend = CrosstermBackend::new(stdout);

  if user_config.behavior.set_window_title {
    backend.execute(SetTitle("spt - Spotatui"))?;
  }

  let mut terminal = Terminal::new(backend)?;
  terminal.hide_cursor()?;

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

  // Audio capture is initialized lazily - only when entering visualization view
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  let mut audio_capture: Option<audio::AudioCaptureManager> = None;

  // Check for updates SYNCHRONOUSLY before starting the event loop
  // This ensures the update prompt appears before any user interaction
  {
    let update_info = tokio::task::spawn_blocking(cli::check_for_update_silent)
      .await
      .ok()
      .flatten();
    if let Some(info) = update_info {
      let mut app = app.lock().await;
      app.update_available = Some(info);
      // Push the mandatory update prompt modal onto navigation stack
      app.push_navigation_stack(RouteId::UpdatePrompt, ActiveBlock::UpdatePrompt);
    }
  }

  // play music on, if not send them to the device selection view

  let mut is_first_render = true;

  loop {
    let mut app = app.lock().await;
    // Get the size of the screen on each loop to account for resize event
    if let Ok(size) = terminal.backend().size() {
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

    match events.next()? {
      event::Event::Input(key) => {
        if key == Key::Ctrl('c') {
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
              break; // Exit application
            }
          }
        } else {
          handlers::handle_app(key, &mut app);
        }
      }
      event::Event::Tick => {
        app.update_on_tick();

        // Lazy audio capture: only capture when in Analysis view
        #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
        {
          let in_analysis_view = app.get_current_route().active_block == ActiveBlock::Analysis;

          if in_analysis_view {
            // Start capture if not already running
            if audio_capture.is_none() {
              audio_capture = audio::AudioCaptureManager::new();
              app.audio_capture_active = audio_capture.is_some();
            }

            // Update spectrum data
            if let Some(ref capture) = audio_capture {
              if let Some(spectrum) = capture.get_spectrum() {
                app.spectrum_data = Some(app::SpectrumData {
                  bands: spectrum.bands,
                  peak: spectrum.peak,
                });
                app.audio_capture_active = capture.is_active();
              }
            }
          } else {
            // Stop capture when leaving Analysis view
            if audio_capture.is_some() {
              audio_capture = None;
              app.audio_capture_active = false;
              app.spectrum_data = None;
            }
          }
        }
      }
    }

    // Delay spotify request until first render, will have the effect of improving
    // startup speed
    if is_first_render {
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetUser);
      app.dispatch(IoEvent::GetCurrentPlayback);
      app.help_docs_size = ui::help::get_help_docs(&app.user_config.keys).len() as u32;

      is_first_render = false;
    }
  }

  terminal.show_cursor()?;
  close_application()?;

  Ok(())
}
