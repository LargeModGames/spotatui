//! First-run source picker.
//!
//! First launch asks which sources to set up. Picking Spotify makes boot log
//! in right after the picker; picking only free sources starts the app with
//! the first one, and Spotify can still be added later with the in-app login.
//!
//! Only sources whose Cargo feature is compiled in are offered. A build with one
//! source shows no picker, and a build with none (the slim build) asks nothing.
//!
//! This module is the selection *logic*; all presentation goes through the
//! [`Onboarding`] trait (the terminal picker itself lives in `tui/first_run.rs`).

use crate::core::onboarding::Onboarding;
use crate::core::source::Source;
use crate::core::state::{PersistedRuntimeState, RuntimeState};
use crate::core::user_config::UserConfig;
#[cfg(feature = "subsonic")]
use anyhow::anyhow;
use anyhow::Result;
use std::path::Path;

fn config_file_path_display(user_config: &UserConfig) -> String {
  user_config
    .path_to_config
    .as_ref()
    .map(|paths| paths.config_file_path.clone())
    .or_else(|| crate::core::paths::app_config_dir().map(|dir| dir.join("config.yml")))
    .map(|path| path.display().to_string())
    .unwrap_or_else(|| "config.yml".to_string())
}

/// A config that cannot be written (a read-only dotfiles link) does not abort
/// the picker; `false` when the save failed.
fn save_config_or_warn(user_config: &UserConfig, onboarding: &dyn Onboarding) -> bool {
  match user_config.save_config() {
    Ok(()) => true,
    Err(e) => {
      let path = config_file_path_display(user_config);
      log::warn!("could not save {path}: {e}");
      onboarding.info(
        &onboarding
          .strings()
          .onboarding
          .config_save_failed(&path, &e.to_string()),
      );
      false
    }
  }
}

/// Explicit completion wins. Only an absent marker uses the upgrade rule: a
/// state file, its recovery backup or an old `client.yml` means setup is done.
/// A state file written before the picker carries an explicit `false` instead.
pub(crate) fn is_first_run(state_path: &Path, legacy_client_path: Option<&Path>) -> bool {
  let Ok(state) = crate::core::state::load(state_path) else {
    return false;
  };
  !state.first_run_complete.unwrap_or_else(|| {
    state_path.exists()
      || state_path.with_extension("yml.bak").exists()
      || legacy_client_path.is_some_and(Path::exists)
  })
}

/// Run the interactive first-run source picker; the caller checks
/// [`is_first_run`] first. `true` when the user picked Spotify, so boot logs
/// in now.
pub async fn run_first_run_picker(
  user_config: &mut UserConfig,
  runtime_state: &mut RuntimeState,
  state_path: Option<&Path>,
  onboarding: &dyn Onboarding,
) -> Result<bool> {
  let options = compiled_in_sources();
  let Some(&first) = options.first() else {
    complete_first_run(user_config, runtime_state, state_path, onboarding)?;
    return Ok(false);
  };
  let selections = if options.len() == 1 {
    options
  } else {
    // Cancelled (esc / ctrl-c) or nothing checked: the first source, Spotify when compiled in.
    onboarding
      .pick_sources(&options)?
      .unwrap_or_else(|| vec![first])
  };

  apply_selections(
    selections,
    user_config,
    runtime_state,
    state_path,
    onboarding,
  )
  .await
}

pub(crate) fn complete_first_run(
  user_config: &UserConfig,
  runtime_state: &mut RuntimeState,
  state_path: Option<&Path>,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  // The global song counter opt-in is asked before this picker runs, so the
  // user's choice already sits on `user_config`; save_config persists it here.
  // Save the active source and completion together, after source setup finishes.
  save_config_or_warn(user_config, onboarding);
  if let Some(state_path) = state_path {
    crate::core::state::save(
      state_path,
      &PersistedRuntimeState {
        first_run_complete: Some(true),
        ..PersistedRuntimeState::active_source(runtime_state.active_source)
      },
    )?;
  }
  runtime_state.first_run_complete = true;
  Ok(())
}

/// Act on the sources the user chose. `active_source` is set to the first checked
/// source in display order; every checked free source has its config collected.
async fn apply_selections(
  selections: Vec<Source>,
  user_config: &mut UserConfig,
  runtime_state: &mut RuntimeState,
  state_path: Option<&Path>,
  onboarding: &dyn Onboarding,
) -> Result<bool> {
  let spotify_selected = selections.contains(&Source::Spotify);
  let active = selections[0];

  runtime_state.active_source = active;

  // Collect credentials / check prerequisites for each chosen free source.
  for source in &selections {
    if *source != Source::Spotify {
      configure_source(*source, user_config, onboarding).await?;
    }
  }
  complete_first_run(user_config, runtime_state, state_path, onboarding)?;

  if spotify_selected {
    if selections.len() > 1 {
      onboarding.info(onboarding.strings().onboarding.setting_up_sources);
    }
    return Ok(true);
  }

  let strings = onboarding.strings();
  onboarding.info(
    &strings
      .onboarding
      .starting_source(strings.sidebar.source_label(active)),
  );

  Ok(false)
}
/// The sources whose Cargo feature is compiled into this build, in display order.
/// Spotify comes first when `spotify-session` is compiled in.
pub(crate) fn compiled_in_sources() -> Vec<Source> {
  [
    (cfg!(feature = "spotify-session"), Source::Spotify),
    (cfg!(feature = "youtube"), Source::YouTube),
    (cfg!(feature = "subsonic"), Source::Subsonic),
    (cfg!(feature = "internet-radio"), Source::Radio),
    (cfg!(feature = "local-files"), Source::Local),
    (cfg!(feature = "qobuz"), Source::Qobuz),
    // Needs nothing up front: macOS asks for Automation access on first use.
    (
      cfg!(all(feature = "apple-music", target_os = "macos")),
      Source::AppleMusic,
    ),
  ]
  .into_iter()
  .filter_map(|(compiled, source)| compiled.then_some(source))
  .collect()
}

// `user_config` and `onboarding` are only read by credential/config-collecting
// sources; a build with none of them (slim, or Qobuz alone) leaves them unused.
#[cfg_attr(
  not(any(feature = "subsonic", feature = "youtube", feature = "local-files")),
  allow(unused_variables)
)]
async fn configure_source(
  source: Source,
  user_config: &mut UserConfig,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  match source {
    #[cfg(feature = "subsonic")]
    Source::Subsonic => configure_subsonic(user_config, onboarding).await?,
    #[cfg(feature = "youtube")]
    Source::YouTube => configure_youtube(user_config, onboarding),
    #[cfg(feature = "local-files")]
    Source::Local => configure_local(user_config, onboarding),
    #[cfg(feature = "qobuz")]
    Source::Qobuz => configure_qobuz(onboarding).await?,
    // Radio needs no setup; other sources are handled above when compiled in.
    _ => {}
  }
  Ok(())
}

/// Log in to Qobuz through the browser and save the credentials file.
#[cfg(feature = "qobuz")]
async fn configure_qobuz(onboarding: &dyn Onboarding) -> Result<()> {
  use crate::infra::qobuz::{auth, shared_qobuz_client, QobuzSource};

  let strings = &onboarding.strings().onboarding;
  onboarding.info(strings.qobuz_setup);
  onboarding.progress(strings.fetching_qobuz_constants);
  let constants = match auth::resolve_constants(&shared_qobuz_client(), None).await {
    Ok(constants) => {
      onboarding.info(strings.ok);
      constants
    }
    Err(e) => {
      onboarding.info(&strings.failed(&format!("{e:#}")));
      onboarding.info(strings.qobuz_retry);
      return Ok(());
    }
  };

  let attempt = match auth::LoginAttempt::bind(constants.clone()).await {
    Ok(attempt) => attempt,
    Err(e) => {
      onboarding.info(&strings.qobuz_login_start_failed(&format!("{e:#}")));
      onboarding.info(strings.qobuz_retry);
      return Ok(());
    }
  };
  let url = attempt.url();
  onboarding.info(strings.opening_browser);
  onboarding.info(&format!("{url}\n"));
  if let Err(e) = open::that_detached(&url) {
    onboarding.info(&strings.browser_open_failed(&e.to_string()));
    onboarding.info(strings.open_browser_manually);
  }
  onboarding.info(strings.qobuz_waiting);

  let credentials = match attempt.wait().await {
    Ok(credentials) => credentials,
    Err(e) => {
      onboarding.info(&strings.qobuz_login_failed(&format!("{e:#}")));
      onboarding.info(strings.qobuz_retry);
      return Ok(());
    }
  };
  auth::save_login(&credentials)?;
  onboarding.info(strings.qobuz_logged_in);

  // Best-effort stream check: a failure is not fatal, the login is saved.
  onboarding.progress(strings.testing_stream);
  let source = QobuzSource::new(
    constants.app_id,
    constants.app_secret,
    credentials.user_auth_token,
  );
  match source.session_start().await {
    Ok(_) => onboarding.info(strings.ok),
    Err(e) => onboarding.info(&strings.failed(&format!("{e:#}"))),
  }
  Ok(())
}

#[cfg(feature = "subsonic")]
async fn configure_subsonic(
  user_config: &mut UserConfig,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  let strings = &onboarding.strings().onboarding;
  onboarding.info(strings.subsonic_setup);
  let url = prompt_required(onboarding, strings.server_url, false)?;
  let username = prompt_required(onboarding, strings.username, false)?;
  let password = prompt_required(onboarding, strings.password, true)?;

  user_config.behavior.subsonic_url = Some(url.clone());
  user_config.behavior.subsonic_username = Some(username.clone());
  user_config.behavior.subsonic_password = Some(password.clone());
  let saved = save_config_or_warn(user_config, onboarding);

  // Best-effort connectivity check: a failure is not fatal (the server may just
  // be temporarily down), the details are already saved.
  onboarding.progress(strings.testing_connection);
  let client = crate::infra::subsonic::SubsonicSource::new(url, username, password);
  match client.ping().await {
    Ok(()) => onboarding.info(strings.ok),
    Err(e) => {
      onboarding.info(&strings.failed(&e.to_string()));
      if saved {
        onboarding.info(&strings.saved_anyway(&config_file_path_display(user_config)));
      } else {
        onboarding.info(strings.session_only);
      }
    }
  }

  Ok(())
}

#[cfg(feature = "youtube")]
fn configure_youtube(user_config: &UserConfig, onboarding: &dyn Onboarding) {
  let strings = &onboarding.strings().onboarding;
  let ytdlp = user_config
    .behavior
    .ytdlp_path
    .clone()
    .unwrap_or_else(|| "yt-dlp".to_string());

  onboarding.progress(strings.checking_ytdlp);
  let mut command = std::process::Command::new(&ytdlp);
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    command.creation_flags(crate::infra::youtube::CREATE_NO_WINDOW);
  }
  match command.arg("--version").output() {
    Ok(output) if output.status.success() => {
      let version = String::from_utf8_lossy(&output.stdout);
      onboarding.info(&strings.ytdlp_found(version.trim()));
    }
    _ => {
      onboarding.info(strings.ytdlp_not_found);
      onboarding.info(strings.ytdlp_required);
      onboarding.info(strings.ytdlp_install);
      onboarding.info(&strings.ytdlp_custom_path(&config_file_path_display(user_config)));
    }
  }
}

#[cfg(feature = "local-files")]
fn configure_local(user_config: &UserConfig, onboarding: &dyn Onboarding) {
  let strings = &onboarding.strings().onboarding;
  match &user_config.behavior.local_music_path {
    Some(path) => {
      onboarding.info(&strings.local_music_path(path));
      onboarding.info(&strings.change_local_music_path(&config_file_path_display(user_config)));
    }
    None => {
      onboarding.info(strings.no_local_music_path);
      onboarding.info(&strings.set_local_music_path(&config_file_path_display(user_config)));
    }
  }
}

// Only credential-collecting sources (currently Subsonic) use this.
#[cfg(feature = "subsonic")]
fn prompt_required(onboarding: &dyn Onboarding, label: &str, masked: bool) -> Result<String> {
  const MAX_RETRIES: u8 = 5;
  let mut retries = 0;
  loop {
    let prompt = format!("  {label}: ");
    let input = if masked {
      onboarding.prompt_masked(&prompt)?
    } else {
      onboarding.prompt_line(&prompt)?
    };
    let trimmed = input.trim().to_string();
    if !trimmed.is_empty() {
      return Ok(trimmed);
    }
    onboarding.info(onboarding.strings().onboarding.required);
    retries += 1;
    if retries >= MAX_RETRIES {
      return Err(anyhow!("Maximum retries ({MAX_RETRIES}) exceeded."));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::ScriptedOnboarding;

  #[test]
  fn the_first_run_picker_offers_the_same_sources_as_the_d_picker() {
    let compiled = compiled_in_sources();
    assert_eq!(
      compiled.contains(&Source::AppleMusic),
      Source::picker_sources().contains(&Source::AppleMusic)
    );
  }

  #[test]
  fn spotify_is_offered_only_with_the_spotify_session() {
    assert_eq!(
      compiled_in_sources().contains(&Source::Spotify),
      cfg!(feature = "spotify-session")
    );
  }

  #[cfg(not(any(
    feature = "spotify-session",
    feature = "audio-decode",
    all(feature = "apple-music", target_os = "macos")
  )))]
  #[tokio::test]
  async fn a_build_with_no_source_ends_the_first_run_without_a_login() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.yml");
    let mut user_config = user_config_in(dir.path());
    let mut runtime_state = RuntimeState::default();
    let onboarding = ScriptedOnboarding::with_answers(&[]);

    let log_in = run_first_run_picker(
      &mut user_config,
      &mut runtime_state,
      Some(&state_path),
      &onboarding,
    )
    .await
    .unwrap();

    assert!(!log_in);
    assert!(!is_first_run(&state_path, None));
  }

  #[cfg(feature = "subsonic")]
  #[test]
  fn a_masked_field_is_read_through_the_masked_prompt() {
    let onboarding = ScriptedOnboarding::with_answers(&["hunter2"]);

    assert_eq!(
      prompt_required(&onboarding, "Password", true).unwrap(),
      "hunter2"
    );
    assert_eq!(
      *onboarding.masked_prompts.lock().unwrap(),
      vec!["  Password: ".to_string()]
    );
  }

  #[test]
  fn saving_initial_runtime_state_keeps_the_first_run_pending() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.yml");

    crate::core::state::save(&state, &RuntimeState::default().to_persisted()).unwrap();

    assert!(is_first_run(&state, None));
  }

  #[test]
  fn an_incomplete_first_run_marker_takes_precedence_over_a_saved_source() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.yml");
    let client = dir.path().join("client.yml");
    std::fs::write(&client, "client_id: old\n").unwrap();
    std::fs::write(state.with_extension("yml.bak"), "old state\n").unwrap();
    std::fs::write(
      &state,
      "active_source: Spotify\nfirst_run_complete: false\n",
    )
    .unwrap();

    assert!(is_first_run(&state, Some(&client)));
  }

  #[test]
  fn a_legacy_state_file_without_a_marker_or_source_keeps_setup_complete() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.yml");
    for contents in ["", "volume_percent: 42\n"] {
      std::fs::write(&state, contents).unwrap();
      assert!(!is_first_run(&state, None));
    }
    std::fs::write(&state, "first_run_complete: true\n").unwrap();
    assert!(!is_first_run(&state, None));
  }

  #[test]
  fn a_recovered_state_file_keeps_setup_complete() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.yml");
    let corrupt = "volume_percent: 300\n";
    std::fs::write(&state, corrupt).unwrap();

    assert!(!is_first_run(&state, None));
    crate::core::state::save(&state, &PersistedRuntimeState::volume_percent(42)).unwrap();

    assert_eq!(
      std::fs::read_to_string(state.with_extension("yml.bak")).unwrap(),
      corrupt
    );
    assert_eq!(
      crate::core::state::load(&state).unwrap(),
      PersistedRuntimeState::volume_percent(42)
    );
    assert!(!is_first_run(&state, None));
  }

  #[test]
  fn a_state_backup_keeps_setup_complete_when_the_replacement_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.yml");
    std::fs::write(state.with_extension("yml.bak"), "volume_percent: 300\n").unwrap();

    assert!(!state.exists());
    assert!(!is_first_run(&state, None));
  }

  #[cfg(feature = "subsonic")]
  #[tokio::test]
  async fn interrupted_source_configuration_keeps_the_first_run_pending() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.yml");
    let mut user_config = user_config_in(dir.path());
    let mut runtime_state = RuntimeState::default();
    crate::core::state::save(&state_path, &runtime_state.to_persisted()).unwrap();

    assert!(apply_selections(
      vec![Source::Subsonic],
      &mut user_config,
      &mut runtime_state,
      Some(&state_path),
      &ScriptedOnboarding::with_answers(&[]),
    )
    .await
    .is_err());

    assert!(is_first_run(&state_path, None));
    assert!(!runtime_state.first_run_complete);
  }

  #[test]
  fn legacy_installs_with_a_client_or_saved_source_keep_their_completed_setup() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.yml");
    let client = dir.path().join("client.yml");

    assert!(is_first_run(&state, Some(&client)));
    assert!(is_first_run(&state, None));
    std::fs::write(&client, "client_id: x\n").unwrap();
    assert!(!is_first_run(&state, Some(&client)));
    std::fs::write(&state, "active_source: Spotify\n").unwrap();
    assert!(!is_first_run(&state, None));
  }

  /// A user config that saves into `dir`, never into the real config directory.
  fn user_config_in(dir: &Path) -> UserConfig {
    let mut user_config = UserConfig::new();
    user_config
      .path_to_config
      .replace(crate::core::user_config::UserConfigPaths {
        config_file_path: dir.join("config.yml"),
      });
    user_config
  }

  #[tokio::test]
  async fn picking_spotify_asks_boot_to_log_in_and_ends_the_first_run() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.yml");
    let mut user_config = user_config_in(dir.path());
    let mut runtime_state = RuntimeState::default();
    let onboarding = ScriptedOnboarding::with_answers(&[]);

    let log_in = apply_selections(
      vec![Source::Spotify],
      &mut user_config,
      &mut runtime_state,
      Some(&state_path),
      &onboarding,
    )
    .await
    .unwrap();

    assert!(log_in);
    assert_eq!(runtime_state.active_source, Source::Spotify);
    assert!(runtime_state.first_run_complete);
    assert_eq!(
      crate::core::state::load(&state_path)
        .unwrap()
        .first_run_complete,
      Some(true)
    );
    assert!(!is_first_run(&state_path, None));
  }

  #[cfg(feature = "internet-radio")]
  #[tokio::test]
  async fn picking_only_a_free_source_starts_without_a_spotify_login() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("state.yml");
    let mut user_config = user_config_in(dir.path());
    let mut runtime_state = RuntimeState::default();
    let onboarding = ScriptedOnboarding::with_answers(&[]);

    let log_in = apply_selections(
      vec![Source::Radio],
      &mut user_config,
      &mut runtime_state,
      Some(&state_path),
      &onboarding,
    )
    .await
    .unwrap();

    assert!(!log_in);
    assert_eq!(runtime_state.active_source, Source::Radio);
    assert!(state_path.exists());
  }

  #[test]
  fn a_failed_config_save_warns_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    // A missing parent fails the write on every platform, root included.
    let config_path = dir.path().join("no-such-directory").join("config.yml");
    let mut user_config = UserConfig::new();
    user_config
      .path_to_config
      .replace(crate::core::user_config::UserConfigPaths {
        config_file_path: config_path.clone(),
      });
    let onboarding = ScriptedOnboarding::with_answers(&[]);

    assert!(!save_config_or_warn(&user_config, &onboarding));

    assert!(!config_path.exists());
    let shown = onboarding.shown.lock().unwrap();
    assert!(shown
      .iter()
      .any(|line| line.starts_with(&format!("Could not save {}: ", config_path.display()))));
  }
}

#[cfg(test)]
mod localization_tests {
  use super::*;
  use crate::core::i18n::{Language, Strings};
  use crate::core::onboarding::{OnboardingAnswer, OnboardingPrompt};
  use crate::core::test_helpers::ScriptedOnboarding;

  struct LocalizedOnboarding {
    scripted: ScriptedOnboarding,
    strings: &'static Strings,
  }

  impl LocalizedOnboarding {
    fn new(language: Language, answers: &[&str]) -> Self {
      Self {
        scripted: ScriptedOnboarding::with_answers(answers),
        strings: language.strings(),
      }
    }
  }

  impl Onboarding for LocalizedOnboarding {
    fn strings(&self) -> &'static Strings {
      self.strings
    }

    fn is_interactive(&self) -> bool {
      self.scripted.is_interactive()
    }

    fn ask(&self, prompt: &OnboardingPrompt) -> Result<OnboardingAnswer> {
      self.scripted.ask(prompt)
    }

    fn info(&self, text: &str) {
      self.scripted.info(text);
    }

    fn progress(&self, text: &str) {
      self.scripted.progress(text);
    }

    fn prompt_line(&self, prompt: &str) -> Result<String> {
      self.scripted.prompt_line(prompt)
    }

    fn prompt_masked(&self, prompt: &str) -> Result<String> {
      self.scripted.prompt_masked(prompt)
    }

    fn pick_sources(&self, options: &[Source]) -> Result<Option<Vec<Source>>> {
      self.scripted.pick_sources(options)
    }
  }

  fn config_at(path: std::path::PathBuf) -> UserConfig {
    let mut config = UserConfig::new();
    config.path_to_config = Some(crate::core::user_config::UserConfigPaths {
      config_file_path: path,
    });
    config
  }

  #[tokio::test]
  async fn localized_source_setup_preserves_the_source_identity() {
    for (language, expected) in [
      (Language::En, "\nStarting spotatui with Internet Radio as your source. Press `d` anytime to switch or to log in to Spotify.\n"),
      (Language::ZhHans, "\n正在以 网络电台 为音源启动 spotatui。随时按 `d` 切换音源或登录 Spotify。\n"),
      (Language::ZhHant, "\n正在以 網路電台 為來源啟動 spotatui。隨時按 `d` 切換來源或登入 Spotify。\n"),
    ] {
      let dir = tempfile::tempdir().unwrap();
      let state_path = dir.path().join("state.yml");
      let mut config = config_at(dir.path().join("config.yml"));
      let mut state = RuntimeState::default();
      let onboarding = LocalizedOnboarding::new(language, &[]);

      // Radio never needs credentials or external work, even when the build
      // would not offer it in compiled_in_sources().
      assert!(!apply_selections(
        vec![Source::Radio],
        &mut config,
        &mut state,
        Some(&state_path),
        &onboarding,
      ).await.unwrap());

      assert_eq!(state.active_source, Source::Radio);
      assert!(state_path.exists());
      assert!(onboarding.scripted.saw(expected));
    }
  }

  #[test]
  fn localized_save_warning_keeps_the_path_and_raw_error() {
    for (language, prefix) in [
      (Language::ZhHans, "无法保存"),
      (Language::ZhHant, "無法儲存"),
    ] {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("missing-{error}").join("config.yml");
      let config = config_at(path.clone());
      let error = config.save_config().unwrap_err().to_string();
      let onboarding = LocalizedOnboarding::new(language, &[]);

      assert!(!save_config_or_warn(&config, &onboarding));
      assert!(onboarding
        .scripted
        .saw(&format!("{prefix} {}：{error}", path.display())));
      assert!(!path.exists());
    }
  }

  #[cfg(feature = "subsonic")]
  #[test]
  fn localized_required_password_stays_masked_and_keeps_input_literal() {
    for (language, label) in [(Language::ZhHans, "密码"), (Language::ZhHant, "密碼")] {
      let onboarding = LocalizedOnboarding::new(language, &["", "  p{max}密碼&:  "]);

      assert_eq!(
        prompt_required(&onboarding, onboarding.strings().onboarding.password, true).unwrap(),
        "p{max}密碼&:"
      );
      assert_eq!(
        *onboarding.scripted.masked_prompts.lock().unwrap(),
        vec![format!("  {label}: "); 2]
      );
      assert!(onboarding.scripted.saw("  （必填）"));
      assert!(!onboarding.scripted.saw("  (required)"));
    }
  }

  #[cfg(feature = "subsonic")]
  #[tokio::test]
  async fn localized_subsonic_setup_masks_only_the_password() {
    for (language, password_label) in [(Language::ZhHans, "密码"), (Language::ZhHant, "密碼")] {
      let dir = tempfile::tempdir().unwrap();
      let mut config = config_at(dir.path().join("config.yml"));
      let onboarding = LocalizedOnboarding::new(
        language,
        &["http://127.0.0.1:9", "用户{name}", "p{max}密碼"],
      );

      configure_subsonic(&mut config, &onboarding).await.unwrap();

      assert_eq!(
        *onboarding.scripted.masked_prompts.lock().unwrap(),
        vec![format!("  {password_label}: ")]
      );
      assert_eq!(
        config.behavior.subsonic_url.as_deref(),
        Some("http://127.0.0.1:9")
      );
      assert_eq!(
        config.behavior.subsonic_username.as_deref(),
        Some("用户{name}")
      );
      assert_eq!(
        config.behavior.subsonic_password.as_deref(),
        Some("p{max}密碼")
      );
    }
  }

  #[cfg(feature = "local-files")]
  #[test]
  fn localized_local_setup_keeps_user_paths_and_config_keys_literal() {
    for (language, prefix) in [
      (Language::ZhHans, "\n将从此文件夹读取本地文件："),
      (Language::ZhHant, "\n將從此資料夾讀取本機檔案："),
    ] {
      let dir = tempfile::tempdir().unwrap();
      let mut config = config_at(dir.path().join("{path}-config.yml"));
      let music_path = "/Music/音樂-{path}";
      config.behavior.local_music_path = Some(music_path.to_string());
      let onboarding = LocalizedOnboarding::new(language, &[]);

      configure_local(&config, &onboarding);

      assert!(onboarding.scripted.saw(&format!("{prefix}{music_path}")));
      let change = onboarding
        .strings()
        .onboarding
        .change_local_music_path(&config_file_path_display(&config));
      assert!(change.starts_with('（') && change.ends_with("。）"));
      assert!(change.contains("behavior.local_music_path"));
      assert!(change.contains("{path}-config.yml"));
      assert!(onboarding.scripted.saw(&change));
    }
  }
}
