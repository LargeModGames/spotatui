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
      onboarding.info(&format!("Could not save {path}: {e}"));
      false
    }
  }
}

/// A first run has neither a `state.yml` nor the `client.yml` older releases wrote.
pub(crate) fn is_first_run(state_path: &Path, legacy_client_path: Option<&Path>) -> bool {
  !state_path.exists() && !legacy_client_path.is_some_and(Path::exists)
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
    end_first_run(user_config, runtime_state, state_path, onboarding)?;
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

fn end_first_run(
  user_config: &UserConfig,
  runtime_state: &RuntimeState,
  state_path: Option<&Path>,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  // The global song counter opt-in is asked before this picker runs, so the
  // user's choice already sits on `user_config`; save_config persists it here.
  // The active source is runtime state, and its file marks the first run done.
  save_config_or_warn(user_config, onboarding);
  if let Some(state_path) = state_path {
    crate::core::state::save(
      state_path,
      &PersistedRuntimeState::active_source(runtime_state.active_source),
    )?;
  }
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
  end_first_run(user_config, runtime_state, state_path, onboarding)?;

  // Collect credentials / check prerequisites for each chosen free source.
  for source in &selections {
    if *source != Source::Spotify {
      configure_source(*source, user_config, onboarding).await?;
    }
  }

  if spotify_selected {
    if selections.len() > 1 {
      onboarding.info("\nSetting up your other sources, then we'll log in to Spotify...\n");
    }
    return Ok(true);
  }

  onboarding.info(&format!(
    "\nStarting spotatui with {} as your source. Press `d` anytime to switch or to log in to Spotify.\n",
    active.label()
  ));

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

  onboarding.info(
    "\nQobuz setup: spotatui logs in through your browser (a paid Qobuz subscription is needed).",
  );
  onboarding.progress("Fetching the Qobuz web player constants... ");
  let constants = match auth::resolve_constants(&shared_qobuz_client(), None).await {
    Ok(constants) => {
      onboarding.info("OK");
      constants
    }
    Err(e) => {
      onboarding.info(&format!("failed: {e:#}"));
      onboarding.info("Press `d` in the app and pick Qobuz to try again.");
      return Ok(());
    }
  };

  let attempt = match auth::LoginAttempt::bind(constants.clone()).await {
    Ok(attempt) => attempt,
    Err(e) => {
      onboarding.info(&format!("Qobuz login could not start: {e:#}"));
      onboarding.info("Press `d` in the app and pick Qobuz to try again.");
      return Ok(());
    }
  };
  let url = attempt.url();
  onboarding.info("\nAttempting to open this URL in your browser:");
  onboarding.info(&format!("{url}\n"));
  if let Err(e) = open::that_detached(&url) {
    onboarding.info(&format!("Failed to open browser automatically: {e}"));
    onboarding.info("Please manually open the URL above in your browser.");
  }
  onboarding.info("Waiting for the Qobuz login to complete...");

  let credentials = match attempt.wait().await {
    Ok(credentials) => credentials,
    Err(e) => {
      onboarding.info(&format!("Qobuz login failed: {e:#}"));
      onboarding.info("Press `d` in the app and pick Qobuz to try again.");
      return Ok(());
    }
  };
  auth::save_login(&credentials)?;
  onboarding.info("Logged in to Qobuz.");

  // Best-effort stream check: a failure is not fatal, the login is saved.
  onboarding.progress("Testing the stream session... ");
  let source = QobuzSource::new(
    constants.app_id,
    constants.app_secret,
    credentials.user_auth_token,
  );
  match source.session_start().await {
    Ok(_) => onboarding.info("OK"),
    Err(e) => onboarding.info(&format!("failed: {e:#}")),
  }
  Ok(())
}

#[cfg(feature = "subsonic")]
async fn configure_subsonic(
  user_config: &mut UserConfig,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  onboarding.info("\nSubsonic / Navidrome setup:");
  let url = prompt_required(
    onboarding,
    "Server URL (e.g. https://demo.navidrome.org)",
    false,
  )?;
  let username = prompt_required(onboarding, "Username", false)?;
  let password = prompt_required(onboarding, "Password", true)?;

  user_config.behavior.subsonic_url = Some(url.clone());
  user_config.behavior.subsonic_username = Some(username.clone());
  user_config.behavior.subsonic_password = Some(password.clone());
  let saved = save_config_or_warn(user_config, onboarding);

  // Best-effort connectivity check: a failure is not fatal (the server may just
  // be temporarily down), the details are already saved.
  onboarding.progress("Testing connection... ");
  let client = crate::infra::subsonic::SubsonicSource::new(url, username, password);
  match client.ping().await {
    Ok(()) => onboarding.info("OK"),
    Err(e) => {
      onboarding.info(&format!("failed: {e}"));
      if saved {
        onboarding.info(&format!(
          "Saved anyway. Fix the details in {} and relaunch if needed.",
          config_file_path_display(user_config)
        ));
      } else {
        onboarding.info("The details are kept for this session only.");
      }
    }
  }

  Ok(())
}

#[cfg(feature = "youtube")]
fn configure_youtube(user_config: &UserConfig, onboarding: &dyn Onboarding) {
  let ytdlp = user_config
    .behavior
    .ytdlp_path
    .clone()
    .unwrap_or_else(|| "yt-dlp".to_string());

  onboarding.progress("\nChecking for yt-dlp... ");
  let mut command = std::process::Command::new(&ytdlp);
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    command.creation_flags(crate::infra::youtube::CREATE_NO_WINDOW);
  }
  match command.arg("--version").output() {
    Ok(output) if output.status.success() => {
      let version = String::from_utf8_lossy(&output.stdout);
      onboarding.info(&format!("found ({})", version.trim()));
    }
    _ => {
      onboarding.info("not found");
      onboarding.info("YouTube playback needs the `yt-dlp` binary on your PATH.");
      onboarding
        .info("Install it (e.g. `pipx install yt-dlp` or your distro package) and relaunch.");
      onboarding.info(&format!(
        "If it lives at a custom path, set behavior.ytdlp_path in {}.",
        config_file_path_display(user_config)
      ));
    }
  }
}

#[cfg(feature = "local-files")]
fn configure_local(user_config: &UserConfig, onboarding: &dyn Onboarding) {
  match &user_config.behavior.local_music_path {
    Some(path) => {
      onboarding.info(&format!("\nLocal files will be read from: {path}"));
      onboarding.info(&format!(
        "(Change behavior.local_music_path in {} to use another folder.)",
        config_file_path_display(user_config)
      ));
    }
    None => {
      onboarding.info("\nNo music folder was detected automatically.");
      onboarding.info(&format!(
        "Set behavior.local_music_path in {}.",
        config_file_path_display(user_config)
      ));
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
    onboarding.info("  (required)");
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
  fn only_a_launch_with_neither_file_is_a_first_run() {
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
