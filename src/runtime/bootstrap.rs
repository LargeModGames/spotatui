//! The frontend-neutral bootstrap: logging, the panic hook, the ALSA error
//! silencer, config/state loading with migrations, Spotify authentication,
//! and `App` construction. `boot()` is the one entry point; `run_cli()` and
//! `run_gui()` call it.

use crate::core::app::App;
use crate::core::auth::{self, SpotifyLogin};
use crate::core::limits::MAX_PLAYBAR_ROWS;
use crate::core::migrations::{
  apply_legacy_client_config_migration, apply_legacy_config_radio_station_migration,
  apply_legacy_config_runtime_state_migration,
};
use crate::core::onboarding::{Onboarding, OnboardingAnswer, OnboardingPrompt};
use crate::core::state::{PersistedRuntimeState, RuntimeState};
use crate::core::user_config::{
  validate_tick_rate_milliseconds, BehaviorConfig, StartupBehavior, UserConfig, UserConfigPaths,
};
use crate::infra::network::IoEvent;
use anyhow::Result;
use backtrace::Backtrace;
use log::info;
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

pub(super) fn setup_logging(
  resolved_level: log::LevelFilter,
  target_levels: &[(&'static str, log::LevelFilter)],
) -> anyhow::Result<()> {
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
  // The global level never goes above `Info`: dependencies (reqwest, hyper,
  // rustls, h2, ...) are deliberately never pinned individually with
  // `level_for`, so the global default is what keeps them quiet at
  // `--debug`/`SPOTATUI_LOG=trace` without a pin list that has to be kept in
  // sync as dependencies change. Below `Info` (e.g. `SPOTATUI_LOG=warn`) the
  // requested level applies globally instead, since nothing needs raising.
  let global_level = resolved_level.min(log::LevelFilter::Info);
  let mut dispatch = fern::Dispatch::new()
    .format(|out, message, record| {
      out.finish(format_args!(
        "{}[{}][{}] {}",
        chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
        record.target(),
        record.level(),
        message
      ))
    })
    .level(global_level);
  for &(target, level) in target_levels {
    dispatch = dispatch.level_for(target, level);
  }
  dispatch
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

    // The keepawake crate unwraps its D-Bus release; that panic ends only its own worker thread.
    if info
      .location()
      .is_some_and(|location| location.file().contains("keepawake"))
    {
      log::warn!("keepawake: the sleep inhibitor panicked: {info}");
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
        // Only the tail (at most the last 64 KiB / 200 lines) is read, never
        // the whole file: the running log can be megabytes by the time a
        // panic happens. `read_log_tail` never panics (every fallible step
        // returns `None`), which matters here — a panic inside a panic hook
        // aborts the process with no message at all.
        let log_tail =
          super::logging::read_log_tail(&crate::core::paths::app_log_path(), 64 * 1024, 200);
        if let Some(tail) = log_tail {
          if !tail.is_empty() {
            let _ = writeln!(f, "\n==== log tail (up to the last 200 lines) ====");
            let _ = writeln!(f, "{tail}");
          }
        }
      }
      eprintln!(
        "A crash log was written to: {} (rerun with --debug for a much more detailed log)",
        path.to_string_lossy()
      );
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

/// Prompt once for the anonymous global song counter opt-in and persist the
/// choice into `config.yml`. Asked before the first-run source picker so the
/// answer applies to whichever source(s) the user sets up. Non-interactive runs
/// default to opt-out so telemetry is never enabled for a user we couldn't ask.
fn prompt_global_song_count_opt_in(
  user_config: &mut UserConfig,
  first_run: bool,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  let config_paths = match &user_config.path_to_config {
    Some(path) => path,
    None => {
      user_config.get_or_build_paths()?;
      user_config.path_to_config.as_ref().unwrap()
    }
  };

  // The source picker keys off the same first-run signal, so asking here keeps
  // the two in step even when a stale config (e.g. one silently opted out by an
  // older build) already carries the setting. Outside a first run, only ask
  // when the config predates the setting.
  let config_has_answer = config_paths.config_file_path.exists() && {
    let config_string = fs::read_to_string(&config_paths.config_file_path)?;
    !config_string.trim().is_empty() && config_string.contains("enable_global_song_count")
  };

  if !should_prompt_global_song_count(first_run, config_has_answer) {
    return Ok(());
  }

  let interactive = onboarding.is_interactive();
  let enable = if interactive {
    matches!(
      onboarding.ask(&global_song_counter_prompt(onboarding.strings()))?,
      OnboardingAnswer::Yes
    )
  } else {
    // Never enable anonymous telemetry for a user we couldn't prompt.
    false
  };

  user_config.behavior.enable_global_song_count = enable;

  if let Err(e) = persist_global_song_count(&config_paths.config_file_path, enable) {
    log::warn!(
      "could not save the global song counter answer to {}: {e}",
      config_paths.config_file_path.display()
    );
    if interactive {
      onboarding.info(
        &onboarding
          .strings()
          .status
          .runtime
          .song_counter_save_failed(&config_paths.config_file_path.display().to_string()),
      );
    }
    return Ok(());
  }

  if interactive {
    if enable {
      onboarding.info(onboarding.strings().status.runtime.song_counter_thanks);
    } else {
      onboarding.info(onboarding.strings().status.runtime.song_counter_opted_out);
    }
  }

  Ok(())
}

/// A genuine first run has no answer yet; a stale config predating the setting
/// is asked again on a later launch.
fn should_prompt_global_song_count(first_run: bool, config_has_answer: bool) -> bool {
  first_run || !config_has_answer
}

/// How boot obtains a Spotify session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SpotifyAuthMode {
  /// Log in through Spotify's pairing page now.
  Interactive,
  /// A cached login with a live session, or a failed boot with a clear message.
  CachedOrFail,
  /// A cached login or no session; the frontend offers the login.
  CachedOrNone,
}

/// Interactive only when the user just asked for Spotify (`--reconfigure-auth`,
/// or Spotify picked on the first run). Most subcommands need a session; `sync`
/// and a UI launch do not.
fn spotify_auth_mode(
  subcommand: Option<&str>,
  reconfigure_auth: bool,
  picked_spotify: bool,
) -> SpotifyAuthMode {
  if reconfigure_auth || picked_spotify {
    SpotifyAuthMode::Interactive
  } else {
    match subcommand {
      Some("sync") | None => SpotifyAuthMode::CachedOrNone,
      Some(_) => SpotifyAuthMode::CachedOrFail,
    }
  }
}

fn global_song_counter_prompt(strings: &crate::core::i18n::Strings) -> OnboardingPrompt {
  OnboardingPrompt::Confirm {
    title: strings.status.runtime.song_counter_title.to_string(),
    body: strings.status.runtime.song_counter_body.to_string(),
    question: strings.status.runtime.song_counter_question.to_string(),
  }
}

/// Merge `enable` into the `behavior` section of `config.yml`, creating the file
/// when absent. Sibling keys are preserved; this hand-patch bypasses
/// `UserConfig::save_config` on purpose: a full save writes every default back
/// into a config that predates the setting, and a written key reads as chosen.
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
  auth::write_private_file_atomic(config_file_path, updated_config.as_bytes())?;

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
  pub(super) spotify: Option<SpotifyLogin>,
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
  /// A status line for the first frame, such as a failed boot login.
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub(super) startup_notice: Option<String>,
}
/// The launch inputs `boot` reads, built from the command line by `cli::boot_options`.
pub(super) struct BootOptions {
  pub(super) system_language: Option<crate::core::i18n::Language>,
  pub(super) config_path: Option<PathBuf>,
  pub(super) tick_rate: Option<u64>,
  pub(super) subcommand: Option<String>,
  pub(super) reconfigure_auth: bool,
  pub(super) play_file: Option<String>,
  pub(super) no_update: bool,
}

/// The shared bootstrap sequence: user config, runtime state, the persisted
/// playback session, client credentials, Spotify authentication (joined with
/// the auto-update check), and `App` construction. Frontend-neutral: every
/// interactive step goes through `onboarding`.
pub(super) async fn boot(
  options: BootOptions,
  onboarding: Arc<dyn Onboarding>,
  instance_lock: &mut Option<std::fs::File>,
) -> Result<Boot> {
  // Auto-update on launch: silently check, download, install, and restart.
  // Skip if a CLI subcommand is active or SPOTATUI_SKIP_UPDATE is set (prevents restart loops).
  let mut user_config = UserConfig::new();
  if let Some(config_file_path) = options.config_path {
    let path = UserConfigPaths { config_file_path };
    user_config.path_to_config.replace(path);
  }
  user_config.load_config()?;
  let system_language = if options.subcommand.is_some() {
    crate::core::i18n::Language::En
  } else {
    options
      .system_language
      .unwrap_or_else(crate::core::i18n::system_language)
  };
  let language = if options.subcommand.is_some() {
    crate::core::i18n::Language::En
  } else {
    user_config.behavior.language.resolve(system_language)
  };
  onboarding.set_language(language);
  let mut runtime_state = RuntimeState::default();
  let mut persisted_state = PersistedRuntimeState::default();
  let mut state_path = None;
  let mut should_save_initial_state = false;
  let mut can_save_initial_state = false;
  let mut first_run = false;
  let legacy_client_path = crate::core::paths::legacy_client_config_path();
  match crate::core::state::default_state_path() {
    Ok(path) => {
      let state_file_exists = path.exists();
      first_run = crate::core::first_run::is_first_run(&path, legacy_client_path.as_deref());
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
  let legacy_client_fields = match (
    legacy_client_path.as_deref(),
    user_config
      .path_to_config
      .as_ref()
      .map(|paths| paths.config_file_path.clone()),
  ) {
    (Some(client_path), Some(config_path)) => match apply_legacy_client_config_migration(
      client_path,
      &config_path,
      &mut user_config.behavior,
      &mut runtime_state,
      &persisted_state,
    ) {
      Ok(fields) => fields,
      Err(e) => {
        log::warn!("[state] failed to read the legacy client.yml: {e}");
        PersistedRuntimeState::default()
      }
    },
    _ => PersistedRuntimeState::default(),
  };
  let state = if should_save_initial_state {
    runtime_state.to_persisted()
  } else {
    let mut state = default_fields;
    state.merge_patch(&legacy_radio_fields);
    state.merge_patch(&legacy_runtime_fields);
    state.merge_patch(&legacy_client_fields);
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

  if let Some(tick_rate) = options.tick_rate {
    user_config.behavior.tick_rate_milliseconds =
      validate_tick_rate_milliseconds(tick_rate, "Tick rate")?;
  }

  // Global song counter opt-in (interactive TUI only). Asked before the source
  // picker so the choice applies no matter which source(s) the user sets up.
  if options.subcommand.is_none() {
    prompt_global_song_count_opt_in(&mut user_config, first_run, onboarding.as_ref())?;
  }

  // First-run source picker (interactive UI launch only): picking Spotify logs
  // in below; picking only free sources starts without Spotify.
  let mut picked_spotify = false;
  if first_run && options.subcommand.is_none() && !options.reconfigure_auth {
    picked_spotify = crate::core::first_run::run_first_run_picker(
      &mut user_config,
      &mut runtime_state,
      state_path.as_deref(),
      onboarding.as_ref(),
    )
    .await?;
  }

  let reconfigure_auth = options.reconfigure_auth;
  if reconfigure_auth {
    onboarding.info(onboarding.strings().status.runtime.forgetting_spotify_login);
    auth::forget_spotify_login().await;
  }
  // A build without Spotify support has nothing to log in to.
  let picked_spotify = picked_spotify && cfg!(feature = "spotify-session");
  let auth_mode = spotify_auth_mode(
    options.subcommand.as_deref(),
    reconfigure_auth,
    picked_spotify,
  );

  // The GitHub update check runs concurrently with the login: both are network
  // round trips and neither depends on the other. (An update that actually
  // installs still restarts the process, exactly as before.)
  let (logged_in, installed_update) = tokio::join!(
    spotify_login_with(
      auth_mode,
      options.subcommand.is_none(),
      onboarding.as_ref(),
      language.strings(),
    ),
    super::cli::run_auto_update(
      options.subcommand.is_some() || options.no_update,
      &user_config
    )
  );

  // Runs before the `?` below so a broken login is still allowed to restart
  // into the newer build, which may be what fixes it.
  super::cli::restart_after_update(installed_update, instance_lock)?;

  let (spotify, startup_notice) = logged_in?;
  let (sync_io_tx, sync_io_rx) = std::sync::mpsc::channel::<IoEvent>();
  info!("app state initialized");

  // Initialise app state
  let app = Arc::new(Mutex::new(App::new_with_state(
    sync_io_tx,
    user_config.clone(),
    runtime_state.clone(),
    state_path.clone(),
    spotify.is_some(),
    system_language,
  )));

  if options.subcommand.is_some() {
    app.lock().await.use_english_messages();
  }

  // `--play-file <PATH>`: queue a local file to start once the UI is up. The
  // path is canonicalised to an absolute `file://` URI so the local-files
  // dispatch can route it; an unreadable path is reported as a status message.
  if let Some(path) = options.play_file.as_deref() {
    match std::fs::canonicalize(path).ok().and_then(|abs| {
      url::Url::from_file_path(abs)
        .ok()
        .map(|url| url.to_string())
    }) {
      Some(uri) => app.lock().await.pending_play_file = Some(uri),
      None => {
        let mut app = app.lock().await;
        let message = app.strings().status.runtime.local_file_missing(path);
        app.set_status_message(message, 8);
      }
    }
  }

  Ok(Boot {
    app,
    sync_io_rx,
    user_config,
    spotify,
    runtime_state,
    restore_playback,
    restore_queue,
    initial_shuffle_enabled,
    initial_startup_behavior,
    startup_notice,
  })
}

/// The Spotify login `mode` asks for, plus a status line for the first frame.
/// A failed login on a UI launch is that line; the app starts without Spotify.
#[cfg(test)]
async fn spotify_login(
  mode: SpotifyAuthMode,
  ui_launch: bool,
  onboarding: &dyn Onboarding,
) -> Result<(Option<SpotifyLogin>, Option<String>)> {
  spotify_login_with(mode, ui_launch, onboarding, &crate::core::i18n::EN).await
}

async fn spotify_login_with(
  mode: SpotifyAuthMode,
  ui_launch: bool,
  onboarding: &dyn Onboarding,
  strings: &crate::core::i18n::Strings,
) -> Result<(Option<SpotifyLogin>, Option<String>)> {
  match mode {
    SpotifyAuthMode::Interactive => match auth::log_in_to_spotify(onboarding).await {
      Ok(login) => Ok((Some(login), None)),
      Err(e) if ui_launch => {
        log::warn!("Spotify login failed: {e}. Press `d` and pick Spotify to try again.");
        Ok((
          None,
          Some(strings.status.runtime.spotify_login_failed(&e.to_string())),
        ))
      }
      Err(e) => Err(e.context("Spotify login failed")),
    },
    SpotifyAuthMode::CachedOrFail => live_spotify_session()
      .await
      .map(|login| (Some(login), None)),
    SpotifyAuthMode::CachedOrNone => Ok((SpotifyLogin::cached(), None)),
  }
}

/// A subcommand needs Spotify now: the cached login must connect.
async fn live_spotify_session() -> Result<SpotifyLogin> {
  #[cfg(feature = "spotify-session")]
  {
    use crate::infra::spotify::session::{self, Unavailable};
    match session::current().await {
      Ok(_) => Ok(SpotifyLogin::new()),
      Err(Unavailable::NoLogin) => Err(anyhow::anyhow!(
        "Spotify is not connected. Start `spotatui`, press `d` and pick Spotify to log in."
      )),
      Err(reason) => Err(anyhow::anyhow!(reason.message())),
    }
  }
  #[cfg(not(feature = "spotify-session"))]
  Err(anyhow::anyhow!(
    "This build of spotatui has no Spotify support"
  ))
}

#[cfg(test)]
mod tests {
  use super::{
    apply_configured_runtime_defaults, global_song_counter_prompt, persist_global_song_count,
    prompt_global_song_count_opt_in, should_prompt_global_song_count, spotify_auth_mode,
    spotify_login, SpotifyAuthMode,
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
    // Genuine first run, nothing answered.
    assert!(should_prompt_global_song_count(true, false));
    // Stale config predating the setting.
    assert!(should_prompt_global_song_count(false, false));
    // A first run, even with an answer present.
    assert!(should_prompt_global_song_count(true, true));
    // Asked and answered already.
    assert!(!should_prompt_global_song_count(false, true));
  }

  #[test]
  fn first_run_prompts_carry_frontend_neutral_text() {
    let OnboardingPrompt::Confirm {
      title,
      body,
      question,
    } = global_song_counter_prompt(&crate::core::i18n::EN);
    assert_eq!(title, "Global Song Counter");
    assert_eq!(
      body,
      "spotatui can contribute to a global counter showing total songs played by all users worldwide.\n\nPrivacy: This feature is completely anonymous.\n• No personal information is collected\n• No song names, artists, or listening history\n• Only a simple increment once a song has played for 30 seconds"
    );
    assert_eq!(question, "Would you like to participate?");
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

    prompt_global_song_count_opt_in(&mut user_config, true, &onboarding).unwrap();

    assert!(user_config.behavior.enable_global_song_count);
    let persisted = std::fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("enable_global_song_count: true"));
    assert!(persisted.contains("volume_percent: 42"));
    assert!(persisted.contains("'#000000'"));
    assert!(onboarding.saw("Global Song Counter"));
    assert!(onboarding.saw("Would you like to participate?"));
    assert!(onboarding.saw("Thank you for participating!\n"));
  }

  #[test]
  fn scripted_no_answers_the_global_song_counter_and_persists_opt_out() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    let mut user_config = user_config_at(config_path.clone());
    let onboarding = ScriptedOnboarding::with_answers(&["nope"]);

    prompt_global_song_count_opt_in(&mut user_config, true, &onboarding).unwrap();

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

    prompt_global_song_count_opt_in(&mut user_config, true, &onboarding).unwrap();

    // Never enable anonymous telemetry for a user we couldn't prompt.
    assert!(!user_config.behavior.enable_global_song_count);
    let persisted = std::fs::read_to_string(&config_path).unwrap();
    assert!(persisted.contains("enable_global_song_count: false"));
    assert!(!onboarding.saw("Would you like to participate?"));
    assert!(!onboarding.saw("Opted out. You can change this anytime in Settings -> Behavior.\n"));
    assert!(!onboarding.saw("Thank you for participating!\n"));
  }

  #[test]
  fn a_failed_global_song_counter_write_does_not_abort_boot() {
    let dir = tempfile::tempdir().unwrap();
    // A missing parent fails the write on every platform, root included.
    let config_path = dir.path().join("no-such-directory").join("config.yml");
    let mut user_config = user_config_at(config_path.clone());
    let onboarding = ScriptedOnboarding::with_answers(&["y"]);

    prompt_global_song_count_opt_in(&mut user_config, true, &onboarding).unwrap();

    assert!(user_config.behavior.enable_global_song_count);
    assert!(!config_path.exists());
    assert!(onboarding.saw(&format!(
      "Could not save your answer to {} (read-only config?). Set `enable_global_song_count` under `behavior:` there to stop this prompt.\n",
      config_path.display()
    )));
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
  fn the_boot_auth_mode_follows_the_picker_and_the_subcommand() {
    let cases = [
      (None, true, false, SpotifyAuthMode::Interactive),
      (Some("play"), true, false, SpotifyAuthMode::Interactive),
      (None, false, true, SpotifyAuthMode::Interactive),
      (Some("play"), false, false, SpotifyAuthMode::CachedOrFail),
      (Some("sync"), false, false, SpotifyAuthMode::CachedOrNone),
      (None, false, false, SpotifyAuthMode::CachedOrNone),
    ];

    for (subcommand, reconfigure_auth, picked_spotify, expected) in cases {
      assert_eq!(
        spotify_auth_mode(subcommand, reconfigure_auth, picked_spotify),
        expected,
        "subcommand={subcommand:?} reconfigure_auth={reconfigure_auth} picked_spotify={picked_spotify}"
      );
    }
  }

  /// A subcommand without a session must fail with a way out; the unit-test
  /// session never connects.
  #[tokio::test]
  async fn a_subcommand_without_a_session_fails_with_the_way_to_log_in() {
    let onboarding = ScriptedOnboarding::with_answers(&[]);
    let error = spotify_login(SpotifyAuthMode::CachedOrFail, false, &onboarding)
      .await
      .unwrap_err()
      .to_string();
    assert!(
      error.contains("log in") || error.contains("no Spotify support"),
      "{error}"
    );
  }

  #[cfg(not(feature = "spotify-session"))]
  #[tokio::test]
  async fn a_failed_login_on_a_ui_launch_starts_without_spotify() {
    let onboarding = ScriptedOnboarding::with_answers(&[]);
    let (login, notice) = spotify_login(SpotifyAuthMode::Interactive, true, &onboarding)
      .await
      .unwrap();
    assert!(login.is_none());
    assert!(notice.is_some_and(|line| line.contains("Press `d`")));
  }
}
