//! First-run source picker.
//!
//! Historically spotatui forced a Spotify OAuth login before the TUI could open.
//! Now that YouTube, Subsonic/Navidrome, Internet Radio, and Local Files are all
//! free, first launch instead asks which source to set up. Picking Spotify falls
//! through to the existing auth wizard; picking a free source seeds a default
//! `client.yml` (so Spotify can still be added later via in-TUI login), records
//! the choice as the active source, and collects any source-specific config.
//!
//! Only sources whose Cargo feature is compiled in are offered. A build with just
//! Spotify (the slim build) shows no picker and keeps the original
//! behavior.
//!
//! This module is the selection *logic*; all presentation goes through the
//! [`Onboarding`] trait (the terminal picker itself lives in `tui/first_run.rs`).

use crate::core::config::ClientConfig;
use crate::core::onboarding::Onboarding;
use crate::core::source::Source;
use crate::core::state::RuntimeState;
use crate::core::user_config::UserConfig;
#[cfg(feature = "subsonic")]
use anyhow::anyhow;
use anyhow::Result;

#[cfg(any(feature = "subsonic", feature = "youtube", feature = "local-files"))]
fn config_file_path_display(user_config: &UserConfig) -> String {
  user_config
    .path_to_config
    .as_ref()
    .map(|paths| paths.config_file_path.clone())
    .or_else(|| crate::core::paths::app_config_dir().map(|dir| dir.join("config.yml")))
    .map(|path| path.display().to_string())
    .unwrap_or_else(|| "config.yml".to_string())
}

/// Run the interactive first-run source picker. A no-op after the first launch
/// (detected by the presence of `client.yml`) and when only Spotify is compiled
/// in. Must be called before [`ClientConfig::load_config`], which would otherwise
/// trigger the Spotify-only auth wizard on a fresh install.
pub async fn run_first_run_picker(
  user_config: &mut UserConfig,
  runtime_state: &mut RuntimeState,
  client_config: &mut ClientConfig,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  // First run is detected by the absence of the Spotify client config file.
  let paths = client_config.get_or_build_paths()?;
  if paths.config_file_path.exists() {
    return Ok(());
  }

  let options = compiled_in_sources();

  // Only Spotify available (the slim build): keep today's behavior and let
  // `load_config` run the wizard.
  if options.len() == 1 {
    return Ok(());
  }

  let selections = match onboarding.pick_sources(&options)? {
    Some(selected) => selected,
    // Cancelled (esc / ctrl-c) or nothing checked: fall through to the Spotify
    // wizard, matching the historical default.
    None => return Ok(()),
  };

  apply_selections(
    selections,
    user_config,
    runtime_state,
    client_config,
    onboarding,
  )
  .await
}

/// Act on the sources the user chose. `active_source` is set to the first checked
/// source in display order; every checked free source has its config collected.
async fn apply_selections(
  selections: Vec<Source>,
  user_config: &mut UserConfig,
  runtime_state: &mut RuntimeState,
  client_config: &mut ClientConfig,
  onboarding: &dyn Onboarding,
) -> Result<()> {
  // Spotify only: keep today's behavior and let `load_config` run the wizard.
  if selections == [Source::Spotify] {
    return Ok(());
  }

  let spotify_selected = selections.contains(&Source::Spotify);
  let active = selections[0];

  // If Spotify wasn't chosen, seed a default `client.yml` (no OAuth) so a later
  // in-TUI Spotify login has a client id to work with. If Spotify *was* chosen we
  // leave `client.yml` absent so `load_config` runs the OAuth wizard below.
  if !spotify_selected {
    client_config.init_default_spotify_config()?;
  }
  runtime_state.active_source = active;
  // The global song counter opt-in is asked before this picker runs, so the
  // user's choice already sits on `user_config`; save_config persists it here.
  // The active source is runtime state, so save it separately.
  user_config.save_config()?;
  let state_path = crate::core::state::default_state_path()?;
  crate::core::state::save(
    &state_path,
    &crate::core::state::PersistedRuntimeState::active_source(runtime_state.active_source),
  )?;

  // Collect credentials / check prerequisites for each chosen free source.
  for source in &selections {
    if *source != Source::Spotify {
      configure_source(*source, user_config, onboarding).await?;
    }
  }

  if spotify_selected {
    // Fall through: `load_config` runs the existing Spotify auth wizard.
    onboarding.info("\nSetting up your other sources, then we'll log in to Spotify...\n");
    return Ok(());
  }

  onboarding.info(&format!(
    "\nStarting spotatui with {} as your source. Press `d` anytime to switch or to log in to Spotify.\n",
    active.label()
  ));

  Ok(())
}

/// The sources whose Cargo feature is compiled into this build, in display order.
/// Spotify is always present.
fn compiled_in_sources() -> Vec<Source> {
  // `mut` is unused in a Spotify-only (slim) build where every push is cfg'd out.
  #[cfg_attr(
    not(any(
      feature = "youtube",
      feature = "subsonic",
      feature = "internet-radio",
      feature = "local-files",
      feature = "qobuz"
    )),
    allow(unused_mut)
  )]
  let mut options = vec![Source::Spotify];
  #[cfg(feature = "youtube")]
  options.push(Source::YouTube);
  #[cfg(feature = "subsonic")]
  options.push(Source::Subsonic);
  #[cfg(feature = "internet-radio")]
  options.push(Source::Radio);
  #[cfg(feature = "local-files")]
  options.push(Source::Local);
  #[cfg(feature = "qobuz")]
  options.push(Source::Qobuz);
  options
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
  if let Err(e) = open::that(&url) {
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
  let url = prompt_required(onboarding, "Server URL (e.g. https://demo.navidrome.org)")?;
  let username = prompt_required(onboarding, "Username")?;
  let password = prompt_required(onboarding, "Password")?;

  user_config.behavior.subsonic_url = Some(url.clone());
  user_config.behavior.subsonic_username = Some(username.clone());
  user_config.behavior.subsonic_password = Some(password.clone());
  user_config.save_config()?;

  // Best-effort connectivity check: a failure is not fatal (the server may just
  // be temporarily down), the details are already saved.
  onboarding.progress("Testing connection... ");
  let client = crate::infra::subsonic::SubsonicSource::new(url, username, password);
  match client.ping().await {
    Ok(()) => onboarding.info("OK"),
    Err(e) => {
      onboarding.info(&format!("failed: {e}"));
      onboarding.info(&format!(
        "Saved anyway. Fix the details in {} and relaunch if needed.",
        config_file_path_display(user_config)
      ));
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
  match std::process::Command::new(&ytdlp).arg("--version").output() {
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
fn prompt_required(onboarding: &dyn Onboarding, label: &str) -> Result<String> {
  const MAX_RETRIES: u8 = 5;
  let mut retries = 0;
  loop {
    let input = onboarding.prompt_line(&format!("  {label}: "))?;
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
