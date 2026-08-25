//! The frontend-neutral bootstrap: logging, the panic hook, the ALSA error
//! silencer, config/state loading with migrations, Spotify authentication,
//! and `App` construction. `boot()` is the one entry point; `run_cli()` is
//! its only caller today, and a future windowed entry point boots through
//! the same sequence.

use crate::core::app::App;
use crate::core::auth;
use crate::core::config::ClientConfig;
use crate::core::limits::MAX_PLAYBAR_ROWS;
use crate::core::migrations::{
  apply_legacy_config_radio_station_migration, apply_legacy_config_runtime_state_migration,
};
use crate::core::onboarding::{Onboarding, OnboardingAnswer, OnboardingPrompt};
use crate::core::state::{PersistedRuntimeState, RuntimeState};
use crate::core::user_config::{
  validate_tick_rate_milliseconds, BehaviorConfig, StartupBehavior, UserConfig, UserConfigPaths,
};
use crate::infra::network::IoEvent;
use anyhow::Result;
use backtrace::Backtrace;
use clap::ArgMatches;
use log::info;
#[cfg(feature = "streaming")]
use rspotify::model::user::PrivateUser;
use rspotify::AuthCodePkceSpotify;
use std::{
  fs,
  io::Write,
  panic,
  path::{Path, PathBuf},
  sync::Arc,
};
use tokio::sync::Mutex;

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

#[cfg(all(target_os = "linux", feature = "streaming"))]
pub(super) fn init_audio_backend() {
  alsa_silence::suppress_alsa_errors();
}

#[cfg(not(all(target_os = "linux", feature = "streaming")))]
pub(super) fn init_audio_backend() {}

pub(super) fn setup_logging() -> anyhow::Result<()> {
  let log_dir = crate::core::paths::app_log_dir();
  let log_path = crate::core::paths::app_log_path();

  // Owner-only, not plain create_dir_all: this sits in the shared OS temp
  // directory under a predictable name, so the default mode would leave the
  // logs readable by every other local user.
  crate::core::paths::ensure_private_dir(&log_dir).map_err(|e| {
    anyhow::anyhow!(
      "Failed to create log directory {}: {}",
      log_dir.display(),
      e
    )
  })?;
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

  // Print the location of log for user reference — on stderr, not stdout.
  //
  // stdout belongs to program output: `spotatui history recap` writes HTML there
  // and `spotatui mcp` writes JSON-RPC there, where the MCP spec is explicit
  // that a server "MUST NOT write anything to its stdout that is not a valid MCP
  // message" (a stray line makes every client fail to start). A diagnostic notice
  // is exactly what stderr is for, and it stays visible on a terminal either way.
  eprintln!("Logging to: {}", log_path.display());

  Ok(())
}

pub(super) fn install_panic_hook() {
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

    #[cfg(feature = "tui")]
    ratatui::restore();
    let panic_log_path =
      crate::core::paths::app_state_dir().map(|dir| dir.join("spotatui_panic.log"));

    if let Some(path) = panic_log_path.as_ref() {
      if let Some(parent) = path.parent() {
        // Owner-only like every other state-dir creation: a panic can be the
        // first thing that ever creates the directory.
        let _ = crate::core::paths::ensure_private_dir(parent);
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

/// Abbreviate a Spotify client ID for user-facing messages: enough to tell two
/// apart, short enough to fit a status line. The 8-char prefix is also what
/// names the token cache file, so it is recognisable.
fn short_client_id(client_id: &str) -> String {
  let head: String = client_id.chars().take(8).collect();
  if client_id.chars().count() > 8 {
    format!("{head}…")
  } else {
    head
  }
}

fn name_client_id(client_id: &str) -> String {
  if client_id == crate::core::config::NCSPOT_CLIENT_ID {
    "the shared ncspot client ID".to_string()
  } else {
    format!("your own app ({})", short_client_id(client_id))
  }
}

/// Phrase an [`auth::ClientIdNotice`] for the user. Both variants are
/// informational: neither means anything is broken, and neither should read as
/// if the user misconfigured something.
fn describe_client_id_notice(notice: auth::ClientIdNotice) -> String {
  match notice {
    auth::ClientIdNotice::SharedWhilePersonalConfigured { personal_client_id } => format!(
      "Spotify signed in with the shared ncspot client ID. Your own app ({}) is set as the fallback, so it is only used if the shared one stops working.",
      short_client_id(&personal_client_id)
    ),
    auth::ClientIdNotice::FellBack {
      from_client_id,
      to_client_id,
    } => format!(
      "No usable Spotify session for {}; signed in with {} instead.",
      name_client_id(&from_client_id),
      name_client_id(&to_client_id)
    ),
  }
}

/// Prompt once for the anonymous global song counter opt-in and persist the
/// choice into `config.yml`. Asked before the first-run source picker so the
/// answer applies to whichever source(s) the user sets up. Non-interactive runs
/// default to opt-out so telemetry is never enabled for a user we couldn't ask.
fn prompt_global_song_count_opt_in(
  user_config: &mut UserConfig,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  let config_paths = match &user_config.path_to_config {
    Some(path) => path,
    None => {
      user_config.get_or_build_paths()?;
      user_config.path_to_config.as_ref().unwrap()
    }
  };

  // A genuine first run has no `client.yml` yet — the source picker keys off the
  // same signal, so asking here keeps the two in step even when a stale config
  // (e.g. one silently opted out by an older build) already carries the setting.
  // Outside a first run, only ask when the config predates the setting.
  let client_yml_exists = config_paths
    .config_file_path
    .parent()
    .map(|dir| dir.join("client.yml").exists())
    .unwrap_or(false);

  let config_has_answer = config_paths.config_file_path.exists() && {
    let config_string = fs::read_to_string(&config_paths.config_file_path)?;
    !config_string.trim().is_empty() && config_string.contains("enable_global_song_count")
  };

  if !should_prompt_global_song_count(client_yml_exists, config_has_answer) {
    return Ok(());
  }

  let interactive = onboarding.is_interactive();
  let enable = if interactive {
    matches!(
      onboarding.ask(&global_song_counter_prompt())?,
      OnboardingAnswer::Yes
    )
  } else {
    // Never enable anonymous telemetry for a user we couldn't prompt.
    false
  };

  user_config.behavior.enable_global_song_count = enable;

  persist_global_song_count(&config_paths.config_file_path, enable)?;

  if interactive {
    if enable {
      onboarding.info("Thank you for participating!\n");
    } else {
      onboarding.info("Opted out. You can change this anytime in Settings -> Behavior.\n");
    }
  }

  Ok(())
}

/// A genuine first run has no answer yet; a stale config predating the setting
/// is asked again even when `client.yml` exists.
fn should_prompt_global_song_count(client_yml_exists: bool, config_has_answer: bool) -> bool {
  !client_yml_exists || !config_has_answer
}

fn global_song_counter_prompt() -> OnboardingPrompt {
  OnboardingPrompt::Confirm {
    title: "Global Song Counter".to_string(),
    body: "\nspotatui can contribute to a global counter showing total\nsongs played by all users worldwide.\n\nPrivacy: This feature is completely anonymous.\n• No personal information is collected\n• No song names, artists, or listening history\n• Only a simple increment when a new song starts".to_string(),
    question: "\nWould you like to participate? (Y/n): ".to_string(),
  }
}

fn auth_setup_migration_prompt() -> OnboardingPrompt {
  OnboardingPrompt::Confirm {
    title: "Authentication Setup Update".to_string(),
    body: "\nConfiguration handling has changed and your authentication setup may need an update."
      .to_string(),
    question: "Would you like to run the new auth setup wizard now? (Y/n): ".to_string(),
  }
}

/// Ask about the auth-setup migration. Deliberately NOT gated on
/// interactivity: piped or closed stdin reads as empty input, which answers
/// yes and runs the wizard, exactly as before this went through `Onboarding`.
fn ask_auth_setup_migration(onboarding: &dyn Onboarding) -> Result<bool> {
  Ok(matches!(
    onboarding.ask(&auth_setup_migration_prompt())?,
    OnboardingAnswer::Yes
  ))
}

/// Merge `enable` into the `behavior` section of `config.yml`, creating the file
/// when absent. Sibling keys are preserved; this hand-patch bypasses
/// `UserConfig::save_config` on purpose (it runs before that config is loaded).
fn persist_global_song_count(config_file_path: &Path, enable: bool) -> Result<()> {
  let config_yml = if config_file_path.exists() {
    fs::read_to_string(config_file_path).unwrap_or_default()
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
  fs::write(config_file_path, updated_config)?;

  Ok(())
}

fn apply_configured_runtime_defaults(
  runtime_state: &mut RuntimeState,
  persisted_state: &PersistedRuntimeState,
  behavior: &BehaviorConfig,
) -> PersistedRuntimeState {
  let mut applied_defaults = PersistedRuntimeState::default();

  if persisted_state.volume_percent.is_none() {
    if let Some(volume_percent) = behavior.volume_percent {
      runtime_state.volume_percent = volume_percent.min(100);
      applied_defaults.volume_percent = Some(runtime_state.volume_percent);
    }
  }
  if persisted_state.sidebar_width_percent.is_none() {
    if let Some(sidebar_width_percent) = behavior.sidebar_width_percent {
      runtime_state.sidebar_width_percent = sidebar_width_percent.min(100);
      applied_defaults.sidebar_width_percent = Some(runtime_state.sidebar_width_percent);
    }
  }
  if persisted_state.playbar_height_rows.is_none() {
    if let Some(playbar_height_rows) = behavior.playbar_height_rows {
      runtime_state.playbar_height_rows = playbar_height_rows.min(MAX_PLAYBAR_ROWS);
      applied_defaults.playbar_height_rows = Some(runtime_state.playbar_height_rows);
    }
  }
  if persisted_state.library_height_percent.is_none() {
    if let Some(library_height_percent) = behavior.library_height_percent {
      runtime_state.library_height_percent = library_height_percent.min(100);
      applied_defaults.library_height_percent = Some(runtime_state.library_height_percent);
    }
  }

  applied_defaults
}

/// Everything the shared bootstrap hands to whichever consumer runs next
/// (CLI dispatch or a frontend launch). Several fields are read by only one
/// consumer; each `cfg_attr` allowance names the build that leaves the field
/// unread.
pub(super) struct Boot {
  pub(super) app: Arc<Mutex<App>>,
  /// Consumed by the IoEvent pump. CLI mode holds it (unread) for the length
  /// of the command so an `App::dispatch` during CLI handling is silently
  /// queued rather than logged as a send error.
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) sync_io_rx: std::sync::mpsc::Receiver<IoEvent>,
  pub(super) user_config: UserConfig,
  pub(super) client_config: ClientConfig,
  pub(super) spotify: Option<AuthCodePkceSpotify>,
  pub(super) token_cache_path: PathBuf,
  /// Read after boot only by the frontend launch's streaming startup.
  #[cfg_attr(not(all(feature = "tui", feature = "streaming")), allow(dead_code))]
  pub(super) runtime_state: RuntimeState,
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) restore_playback: Option<crate::core::persisted_playback::PersistedPlayback>,
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) restore_queue: Vec<crate::core::plugin_api::TrackInfo>,
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) initial_shuffle_enabled: bool,
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) initial_startup_behavior: StartupBehavior,
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) client_id_notice_message: Option<String>,
  /// Read after boot only by the frontend launch's streaming startup.
  #[cfg_attr(not(all(feature = "tui", feature = "streaming")), allow(dead_code))]
  pub(super) onboarding: Arc<dyn Onboarding>,
  #[cfg(feature = "streaming")]
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) cached_me: Option<PrivateUser>,
  #[cfg(feature = "streaming")]
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) selected_redirect_uri: String,
}

/// The shared bootstrap sequence: user config, runtime state, the persisted
/// playback session, client credentials, Spotify authentication (joined with
/// the auto-update check), and `App` construction. Frontend-neutral: every
/// interactive step goes through `onboarding`.
pub(super) async fn boot(matches: &ArgMatches, onboarding: Arc<dyn Onboarding>) -> Result<Boot> {
  // Auto-update on launch: silently check, download, install, and restart.
  // Skip if a CLI subcommand is active or SPOTATUI_SKIP_UPDATE is set (prevents restart loops).
  let mut user_config = UserConfig::new();
  if let Some(config_file_path) = matches.get_one::<String>("config") {
    let config_file_path = PathBuf::from(config_file_path);
    let path = UserConfigPaths { config_file_path };
    user_config.path_to_config.replace(path);
  }
  user_config.load_config()?;
  let mut runtime_state = RuntimeState::default();
  let mut persisted_state = PersistedRuntimeState::default();
  let mut state_path = None;
  let mut should_save_initial_state = false;
  let mut can_save_initial_state = false;
  match crate::core::state::default_state_path() {
    Ok(path) => {
      let state_file_exists = path.exists();
      match crate::core::state::load(&path) {
        Ok(state) => {
          persisted_state = state;
          runtime_state.apply_persisted(&persisted_state);
          if !state_file_exists {
            should_save_initial_state = true;
          }
          can_save_initial_state = true;
          state_path = Some(path);
        }
        Err(e) => {
          log::warn!("[state] ignoring unreadable runtime state: {e}");
        }
      }
    }
    Err(e) => {
      log::warn!("[state] runtime state path is unavailable: {e}");
    }
  }
  let default_fields =
    apply_configured_runtime_defaults(&mut runtime_state, &persisted_state, &user_config.behavior);
  let legacy_radio_fields = apply_legacy_config_radio_station_migration(
    &mut runtime_state,
    &persisted_state,
    &user_config.behavior,
  );
  let legacy_runtime_fields = match user_config
    .path_to_config
    .as_ref()
    .map(|paths| paths.config_file_path.clone())
  {
    Some(config_path) => match apply_legacy_config_runtime_state_migration(
      &config_path,
      &mut runtime_state,
      &persisted_state,
    ) {
      Ok(fields) => fields,
      Err(e) => {
        log::warn!("[state] failed to read legacy runtime fields from config.yml: {e}");
        PersistedRuntimeState::default()
      }
    },
    None => PersistedRuntimeState::default(),
  };
  let state = if should_save_initial_state {
    runtime_state.to_persisted()
  } else {
    let mut state = default_fields;
    state.merge_patch(&legacy_radio_fields);
    state.merge_patch(&legacy_runtime_fields);
    state
  };
  if can_save_initial_state && !state.is_empty() {
    if let Some(path) = &state_path {
      if let Err(e) = crate::core::state::save(path, &state) {
        log::warn!("[state] failed to save initial runtime state: {e}");
      }
    }
  }
  info!("user config loaded successfully");

  let initial_shuffle_enabled = runtime_state.shuffle_enabled;
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

  // Global song counter opt-in (interactive TUI only). Asked before the source
  // picker so the choice applies no matter which source(s) the user sets up.
  if matches.subcommand_name().is_none() {
    prompt_global_song_count_opt_in(&mut user_config, onboarding.as_ref())?;
  }

  let mut client_config = ClientConfig::new();
  // First-run source picker (interactive TUI only): lets the user pick a free
  // source and skip Spotify entirely. Must run before `load_config`, which would
  // otherwise launch the Spotify-only auth wizard on a fresh install. Skipped for
  // CLI subcommands (Spotify-only) and when `--reconfigure-auth` is requested.
  if matches.subcommand_name().is_none() && !matches.get_flag("reconfigure-auth") {
    crate::core::first_run::run_first_run_picker(
      &mut user_config,
      &mut runtime_state,
      &mut client_config,
      onboarding.as_ref(),
    )
    .await?;
  }
  client_config.load_config(onboarding.as_ref())?;
  info!("client authentication config loaded");

  let reconfigure_auth = matches.get_flag("reconfigure-auth");

  if reconfigure_auth {
    onboarding.info("\nReconfiguring client authentication...");
    client_config.reconfigure_auth(onboarding.as_ref())?;
    onboarding.info("Client authentication setup updated.\n");
  } else if matches.subcommand_name().is_none() && client_config.needs_auth_setup_migration() {
    if ask_auth_setup_migration(onboarding.as_ref())? {
      client_config.reconfigure_auth(onboarding.as_ref())?;
      onboarding.info("Client authentication setup updated.\n");
    } else {
      client_config.mark_auth_setup_migrated()?;
      onboarding.info("Skipped. You can run this anytime with `spotatui --reconfigure-auth`.\n");
    }
  }

  let config_paths = client_config.get_or_build_paths()?;

  // Spotify is only mandatory when the active source IS Spotify, or when running
  // a CLI subcommand (every subcommand is Spotify-only and should fail cleanly
  // when unauthenticated). A free-source TUI launch tries a silent token load and
  // tolerates its absence; the user can add Spotify later via in-TUI login.
  let spotify_required = matches.subcommand_name().is_some()
    || runtime_state.active_source == crate::core::source::Source::Spotify;

  // The GitHub update check runs concurrently with authentication: both are
  // network round trips and neither depends on the other, so the check no
  // longer adds its own latency to startup. (An update that actually installs
  // still restarts the process, exactly as before.)
  let (authenticated, installed_update) = tokio::join!(
    async {
      if spotify_required {
        auth::authenticate_with_fallback(&mut client_config, &config_paths, onboarding.as_ref())
          .await
          .map(Some)
      } else {
        Ok(
          auth::try_load_spotify_silently(&mut client_config, &config_paths, onboarding.as_ref())
            .await,
        )
      }
    },
    super::cli::run_auto_update(matches, &user_config)
  );

  // Only now that authentication has released the OAuth callback port and the
  // terminal. Runs before the `?` below so a broken auth state is still allowed
  // to restart into the newer build, which may be what fixes it.
  super::cli::restart_after_update(installed_update)?;

  let authenticated: Option<auth::AuthenticatedClient> = authenticated?;

  // Which Spotify app the session actually authenticated as, when the user would
  // likely assume otherwise. Logged now (so it is in every log file) and shown as
  // a status message once the UI is about to come up (issue #395).
  let client_id_notice_message = authenticated
    .as_ref()
    .and_then(|a| a.client_id_notice.clone())
    .map(describe_client_id_notice);
  if let Some(message) = client_id_notice_message.as_deref() {
    log::warn!("{}", message);
  }

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
  let app = Arc::new(Mutex::new(App::new_with_state(
    sync_io_tx,
    user_config.clone(),
    runtime_state.clone(),
    state_path.clone(),
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

  Ok(Boot {
    app,
    sync_io_rx,
    user_config,
    client_config,
    spotify,
    token_cache_path: final_token_cache_path,
    runtime_state,
    restore_playback,
    restore_queue,
    initial_shuffle_enabled,
    initial_startup_behavior,
    client_id_notice_message,
    onboarding,
    #[cfg(feature = "streaming")]
    cached_me,
    #[cfg(feature = "streaming")]
    selected_redirect_uri,
  })
}

#[cfg(test)]
mod tests {
  use super::{
    apply_configured_runtime_defaults, ask_auth_setup_migration, auth_setup_migration_prompt,
    global_song_counter_prompt, persist_global_song_count, prompt_global_song_count_opt_in,
    should_prompt_global_song_count,
  };
  use crate::core::limits::MAX_PLAYBAR_ROWS;
  use crate::core::onboarding::OnboardingPrompt;
  use crate::core::state::{PersistedRuntimeState, RuntimeState};
  use crate::core::test_helpers::ScriptedOnboarding;
  use crate::core::user_config::UserConfig;

  fn user_config_with_runtime_defaults() -> UserConfig {
    let mut config = UserConfig::new();
    config.behavior.volume_percent = Some(80);
    config.behavior.sidebar_width_percent = Some(40);
    config.behavior.playbar_height_rows = Some(MAX_PLAYBAR_ROWS + 1);
    config.behavior.library_height_percent = Some(60);
    config
  }

  #[test]
  fn configured_runtime_defaults_do_not_override_persisted_state() {
    let config = user_config_with_runtime_defaults();
    let persisted = PersistedRuntimeState {
      volume_percent: Some(42),
      sidebar_width_percent: Some(25),
      playbar_height_rows: Some(5),
      library_height_percent: Some(35),
      ..Default::default()
    };
    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&persisted);

    assert_eq!(
      apply_configured_runtime_defaults(&mut runtime, &persisted, &config.behavior),
      PersistedRuntimeState::default()
    );
    assert_eq!(runtime.volume_percent, 42);
    assert_eq!(runtime.sidebar_width_percent, 25);
    assert_eq!(runtime.playbar_height_rows, 5);
    assert_eq!(runtime.library_height_percent, 35);
  }

  #[test]
  fn configured_runtime_defaults_seed_only_missing_persisted_fields() {
    let config = user_config_with_runtime_defaults();
    let persisted = PersistedRuntimeState {
      volume_percent: Some(42),
      ..Default::default()
    };
    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&persisted);

    assert_eq!(
      apply_configured_runtime_defaults(&mut runtime, &persisted, &config.behavior),
      PersistedRuntimeState {
        sidebar_width_percent: Some(40),
        playbar_height_rows: Some(MAX_PLAYBAR_ROWS),
        library_height_percent: Some(60),
        ..Default::default()
      }
    );
    assert_eq!(runtime.volume_percent, 42);
    assert_eq!(runtime.sidebar_width_percent, 40);
    assert_eq!(runtime.playbar_height_rows, MAX_PLAYBAR_ROWS);
    assert_eq!(runtime.library_height_percent, 60);
  }

  fn user_config_at(config_file_path: std::path::PathBuf) -> UserConfig {
    let mut config = UserConfig::new();
    let path = crate::core::user_config::UserConfigPaths { config_file_path };
    config.path_to_config.replace(path);
    config
  }

  #[test]
  fn global_song_counter_prompt_predicate_matches_first_run_and_stale_configs() {
    // Genuine first run: no client.yml, nothing answered.
    assert!(should_prompt_global_song_count(false, false));
    // Stale config predating the setting.
    assert!(should_prompt_global_song_count(true, false));
    // No client.yml yet, even with an answer present.
    assert!(should_prompt_global_song_count(false, true));
    // Asked and answered already.
    assert!(!should_prompt_global_song_count(true, true));
  }

  #[test]
  fn first_run_prompts_carry_the_historical_banner_text() {
    let OnboardingPrompt::Confirm {
      title,
      body,
      question,
    } = global_song_counter_prompt();
    assert_eq!(title, "Global Song Counter");
    assert_eq!(
      body,
      "\nspotatui can contribute to a global counter showing total\nsongs played by all users worldwide.\n\nPrivacy: This feature is completely anonymous.\n• No personal information is collected\n• No song names, artists, or listening history\n• Only a simple increment when a new song starts"
    );
    assert_eq!(question, "\nWould you like to participate? (Y/n): ");

    let OnboardingPrompt::Confirm {
      title,
      body,
      question,
    } = auth_setup_migration_prompt();
    assert_eq!(title, "Authentication Setup Update");
    assert_eq!(
      body,
      "\nConfiguration handling has changed and your authentication setup may need an update."
    );
    assert_eq!(
      question,
      "Would you like to run the new auth setup wizard now? (Y/n): "
    );
  }

  #[test]
  fn scripted_yes_answers_the_global_song_counter_without_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    std::fs::write(
      &config_path,
      "theme:\n  background: '#000000'\nbehavior:\n  volume_percent: 42\n",
    )
    .unwrap();
    let mut user_config = user_config_at(config_path.clone());
    let onboarding = ScriptedOnboarding::with_answers(&["y"]);

    prompt_global_song_count_opt_in(&mut user_config, &onboarding).unwrap();

    assert!(user_config.behavior.enable_global_song_count);
    let persisted = std::fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("enable_global_song_count: true"));
    assert!(persisted.contains("volume_percent: 42"));
    assert!(persisted.contains("'#000000'"));
    assert!(onboarding.saw("Global Song Counter"));
    assert!(onboarding.saw("\nWould you like to participate? (Y/n): "));
    assert!(onboarding.saw("Thank you for participating!\n"));
  }

  #[test]
  fn scripted_no_answers_the_global_song_counter_and_persists_opt_out() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    let mut user_config = user_config_at(config_path.clone());
    let onboarding = ScriptedOnboarding::with_answers(&["nope"]);

    prompt_global_song_count_opt_in(&mut user_config, &onboarding).unwrap();

    assert!(!user_config.behavior.enable_global_song_count);
    let persisted = std::fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("enable_global_song_count: false"));
    assert!(onboarding.saw("Opted out. You can change this anytime in Settings -> Behavior.\n"));
  }

  #[test]
  fn non_interactive_onboarding_silently_opts_out_and_persists_false() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    let mut user_config = user_config_at(config_path.clone());
    let onboarding = ScriptedOnboarding::non_interactive(&[]);

    prompt_global_song_count_opt_in(&mut user_config, &onboarding).unwrap();

    // Never enable anonymous telemetry for a user we couldn't prompt.
    assert!(!user_config.behavior.enable_global_song_count);
    let persisted = std::fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("enable_global_song_count: false"));
    assert!(!onboarding.saw("\nWould you like to participate? (Y/n): "));
    assert!(!onboarding.saw("Opted out. You can change this anytime in Settings -> Behavior.\n"));
    assert!(!onboarding.saw("Thank you for participating!\n"));
  }

  #[test]
  fn global_song_counter_write_creates_the_config_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");

    persist_global_song_count(&config_path, true).unwrap();

    let persisted = std::fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("enable_global_song_count: true"));
  }

  #[test]
  fn auth_setup_migration_ask_runs_the_wizard_on_an_empty_answer() {
    let onboarding = ScriptedOnboarding::with_answers(&[""]);

    assert!(ask_auth_setup_migration(&onboarding).unwrap());
    assert!(onboarding.saw("Authentication Setup Update"));
    assert!(onboarding.saw("Would you like to run the new auth setup wizard now? (Y/n): "));
  }

  #[test]
  fn auth_setup_migration_ask_skips_the_wizard_on_a_decline() {
    let onboarding = ScriptedOnboarding::with_answers(&["n"]);

    assert!(!ask_auth_setup_migration(&onboarding).unwrap());
    assert!(onboarding.saw("Would you like to run the new auth setup wizard now? (Y/n): "));
  }
}
