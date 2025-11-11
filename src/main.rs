mod app;
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
use clap::{App as ClapApp, Arg, Shell};
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
  {AuthCodeSpotify, Config, Credentials, OAuth},
};
use std::{
  cmp::{max, min},
  io::{self, stdout, Stdout},
  panic::{self, PanicInfo},
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

fn close_application() -> Result<()> {
  disable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
  Ok(())
}

fn panic_hook(info: &PanicInfo<'_>) {
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
    .usage("Press `?` while running the app to see keybindings")
    .before_help(BANNER)
    .after_help(
      "Your spotify Client ID and Client Secret are stored in $HOME/.config/spotify-tui/client.yml",
    )
    .arg(
      Arg::with_name("tick-rate")
        .short("t")
        .long("tick-rate")
        .help("Set the tick rate (milliseconds): the lower the number the higher the FPS.")
        .long_help(
          "Specify the tick rate in milliseconds: the lower the number the \
higher the FPS. It can be nicer to have a lower value when you want to use the audio analysis view \
of the app. Beware that this comes at a CPU cost!",
        )
        .takes_value(true),
    )
    .arg(
      Arg::with_name("config")
        .short("c")
        .long("config")
        .help("Specify configuration file path.")
        .takes_value(true),
    )
    .arg(
      Arg::with_name("completions")
        .long("completions")
        .help("Generates completions for your preferred shell")
        .takes_value(true)
        .possible_values(&["bash", "zsh", "fish", "power-shell", "elvish"])
        .value_name("SHELL"),
    )
    // Control spotify from the command line
    .subcommand(cli::playback_subcommand())
    .subcommand(cli::play_subcommand())
    .subcommand(cli::list_subcommand())
    .subcommand(cli::search_subcommand());

  let matches = clap_app.clone().get_matches();

  // Shell completions don't need any spotify work
  if let Some(s) = matches.value_of("completions") {
    let shell = match s {
      "fish" => Shell::Fish,
      "bash" => Shell::Bash,
      "zsh" => Shell::Zsh,
      "power-shell" => Shell::PowerShell,
      "elvish" => Shell::Elvish,
      _ => return Err(anyhow!("no completions avaible for '{}'", s)),
    };
    clap_app.gen_completions_to("spt", shell, &mut io::stdout());
    return Ok(());
  }

  let mut user_config = UserConfig::new();
  if let Some(config_file_path) = matches.value_of("config") {
    let config_file_path = PathBuf::from(config_file_path);
    let path = UserConfigPaths { config_file_path };
    user_config.path_to_config.replace(path);
  }
  user_config.load_config()?;

  if let Some(tick_rate) = matches
    .value_of("tick-rate")
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

  let mut config = Config::default();
  config.cache_path = config_paths.token_cache_path;

  let mut spotify = AuthCodeSpotify::with_config(creds, oauth, config);

  let config_port = client_config.get_port();

  // Attempt to read token from cache.
  // Gracefully handle missing cache file - read_token_cache returns Ok(None) if file doesn't exist
  let token_result = spotify.read_token_cache(true).await;
  let has_cached_token = match token_result {
    Ok(Some(_)) => true,
    Ok(None) => false,
    Err(_) => false, // Treat cache read errors as "no token"
  };

  if !has_cached_token {
    // If token is not in cache, get it from web flow
    match redirect_uri_web_server(&mut spotify, config_port) {
      Ok(url) => {
        if let Some(code) = spotify.parse_response_code(&url) {
          spotify.request_token(&code).await?;
        }
      }
      Err(()) => {
        println!("Starting webserver failed. Continuing with manual authentication");
        let url = spotify.get_authorize_url(false)?;
        println!("Please open this URL in your browser: {}", url);
        println!("Enter the URL you were redirected to: ");
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if let Some(code) = spotify.parse_response_code(&input) {
          spotify.request_token(&code).await?;
        }
      }
    }
  }

  // The client is now authenticated and ready to be used.
  // The token will be automatically refreshed by rspotify.
  // Access the token to get expiry time
  let token_lock = spotify.token.lock().await.expect("Failed to lock token");
  let token_expiry = if let Some(ref token) = *token_lock {
    // Convert TimeDelta to SystemTime
    let expires_in_secs = token.expires_in.num_seconds() as u64;
    SystemTime::now()
      .checked_add(std::time::Duration::from_secs(expires_in_secs))
      .unwrap_or_else(SystemTime::now)
  } else {
    // If no token, set expiry to now so we trigger refresh
    SystemTime::now()
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
    backend.execute(SetTitle("spt - Spotify TUI"))?;
  }

  let mut terminal = Terminal::new(backend)?;
  terminal.hide_cursor()?;

  let events = event::Events::new(user_config.behavior.tick_rate_milliseconds);

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
        ui::draw_help_menu::<CrosstermBackend<Stdout>>(f, &app);
      }
      ActiveBlock::Error => {
        ui::draw_error_screen::<CrosstermBackend<Stdout>>(f, &app);
      }
      ActiveBlock::SelectDevice => {
        ui::draw_device_list::<CrosstermBackend<Stdout>>(f, &app);
      }
      ActiveBlock::Analysis => {
        ui::audio_analysis::draw::<CrosstermBackend<Stdout>>(f, &app);
      }
      ActiveBlock::BasicView => {
        ui::draw_basic_view::<CrosstermBackend<Stdout>>(f, &app);
      }
      _ => {
        ui::draw_main_layout::<CrosstermBackend<Stdout>>(f, &app);
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
