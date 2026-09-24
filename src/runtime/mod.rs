//! Bootstrap and the processes every frontend shares.
//!
//! `run_cli()` is the console entry point: it wires logging, parses the
//! command line, runs the shared `bootstrap::boot()` sequence (config, state,
//! auth, `App`), and then either executes one CLI subcommand or launches the
//! terminal UI. The IoEvent pump every frontend drives lives in `pump`, and
//! the native-streaming startup every frontend shares lives in `streaming`.

mod bootstrap;
mod cli;
#[cfg(feature = "tui")]
mod instance;
mod logging;
mod pump;
#[cfg(feature = "tui")]
mod startup;
#[cfg(any(feature = "streaming", test))]
mod streaming;

use crate::core::migrations::apply_legacy_state_file_migrations;
use anyhow::{anyhow, Result};
use clap_complete::{generate, Shell};
use log::info;
use std::io;
use std::sync::Arc;

/// Console implementation of the first-launch surface for builds without the
/// terminal frontend. CLI subcommands can still reach the Spotify auth wizard
/// (`ClientConfig::load_config`), whose prompts are plain stdin/stdout;
/// mirrors `ConsoleOnboarding` minus the interactive source picker, which is
/// unreachable here (the picker only runs on a UI launch, and headless builds
/// bail out before boot in that case).
#[cfg(not(feature = "tui"))]
struct HeadlessOnboarding;

#[cfg(not(feature = "tui"))]
impl crate::core::onboarding::Onboarding for HeadlessOnboarding {
  fn info(&self, text: &str) {
    println!("{text}");
  }

  fn progress(&self, text: &str) {
    use std::io::Write;
    print!("{text}");
    let _ = io::stdout().flush();
  }

  fn prompt_line(&self, prompt: &str) -> Result<String> {
    use std::io::Write;
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
  }

  fn is_interactive(&self) -> bool {
    use std::io::IsTerminal;
    io::stdin().is_terminal() && io::stdout().is_terminal()
  }

  fn ask(
    &self,
    prompt: &crate::core::onboarding::OnboardingPrompt,
  ) -> Result<crate::core::onboarding::OnboardingAnswer> {
    use crate::core::onboarding::{confirm_answer, OnboardingPrompt, BANNER_RULE};
    // Both bootstrap prompts are UI-launch-only and headless builds bail out
    // before boot there; keep a plain console fallback if this is reached.
    let OnboardingPrompt::Confirm {
      title,
      body,
      question,
    } = prompt;
    println!("\n{BANNER_RULE}\n{title}\n{BANNER_RULE}\n{body}");
    println!("{question}");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(confirm_answer(&input))
  }

  fn pick_sources(
    &self,
    _options: &[crate::core::source::Source],
  ) -> Result<Option<Vec<crate::core::source::Source>>> {
    // Only a UI launch runs the first-run picker, and headless builds return
    // before boot in that case; fall through to the Spotify wizard if this is
    // ever reached anyway.
    Ok(None)
  }
}

pub async fn run_cli() -> Result<()> {
  let result = run_cli_inner().await;
  // A failing run is the one that gets reported, so it needs the log path
  // most — and `?` inside carries every failure straight past the notice at
  // the bottom. Checked rather than assumed: `setup_logging` is itself one of
  // the steps that can fail, and pointing at a file that was never created
  // sends the reporter looking for something that is not there.
  if result.is_err() {
    let path = crate::core::paths::app_log_path();
    if path.is_file() {
      eprintln!(
        "{}",
        logging::exit_log_notice(&path, log::max_level() >= log::LevelFilter::Debug)
      );
    }
  }
  result
}

async fn run_cli_inner() -> Result<()> {
  let mut clap_app = cli::build_clap_app();

  let matches = clap_app.clone().get_matches();

  // Logging depends on the parsed flags (`--debug`), so it moves here from
  // being the very first statement. Accepted consequence: a clap usage error
  // above (bad flag, `--help`, `--version`) exits before any log file exists.
  let debug_flag = matches.get_flag("debug");
  let env_log_value = std::env::var("SPOTATUI_LOG").ok();
  let (log_level, log_level_warning) =
    logging::resolve_log_level(debug_flag, env_log_value.as_deref());
  bootstrap::setup_logging(log_level, &logging::target_levels(log_level))?;
  if let Some(warning) = log_level_warning {
    log::warn!("{warning}");
  }

  // Always on, so a bug report has version/platform/build context in every
  // log, not just `--debug` ones.
  info!(
    "{}",
    logging::startup_header(
      env!("CARGO_PKG_VERSION"),
      std::env::consts::OS,
      std::env::consts::ARCH,
      &logging::compiled_features(),
      std::env::var("TERM").ok().as_deref(),
      std::env::var("TERM_PROGRAM").ok().as_deref(),
      std::env::var_os("WT_SESSION").is_some(),
    )
  );

  info!("spotatui {} starting up", env!("CARGO_PKG_VERSION"));
  bootstrap::init_audio_backend();
  info!("audio backend initialized");

  bootstrap::install_panic_hook();
  info!("panic hook configured");

  // Shell completions don't need any spotify work
  if let Some(s) = matches.get_one::<String>("completions") {
    let shell = match s.as_str() {
      "fish" => Shell::Fish,
      "bash" => Shell::Bash,
      "zsh" => Shell::Zsh,
      "power-shell" => Shell::PowerShell,
      "elvish" => Shell::Elvish,
      _ => return Err(anyhow!("no completions available for '{}'", s)),
    };
    generate(shell, &mut clap_app, "spotatui", &mut io::stdout());
    return Ok(());
  }

  // Handle self-update command (doesn't need Spotify auth)
  if cli::handle_self_update_command(&matches).await? {
    return Ok(());
  }

  let mut instance_lock = None;
  #[cfg(feature = "tui")]
  if instance::takes_lock(matches.subcommand_name()) {
    instance_lock = instance::acquire()?;
  }

  if let Err(e) = apply_legacy_state_file_migrations() {
    log::warn!("[state] failed to migrate legacy app data files: {e}");
  }

  if let Some(history_matches) = matches.subcommand_matches("history") {
    println!("{}", crate::cli::handle_history_matches(history_matches)?);
    return Ok(());
  }

  // The MCP server owns stdout for the protocol, so it must return before any
  // other startup path can print to it — and it needs no Spotify auth of its
  // own, since the running TUI holds the session.
  #[cfg(feature = "mcp-server")]
  if let Some(mcp_matches) = matches.subcommand_matches("mcp") {
    // `status` is the safe probe an agent (or a human) can run; bare `mcp` is the
    // server and blocks on stdin until the client closes it.
    if let Some(status_matches) = mcp_matches.subcommand_matches("status") {
      let (report, code) = crate::infra::mcp::run_status(status_matches.get_flag("json")).await;
      println!("{report}");
      std::process::exit(code);
    }
    crate::infra::mcp::run_relay().await?;
    return Ok(());
  }

  // Plugin management is pure git + filesystem work; it must not require Spotify auth.
  #[cfg(feature = "scripting")]
  if let Some(plugin_matches) = matches.subcommand_matches("plugin") {
    crate::cli::handle_plugin_command(plugin_matches)?;
    return Ok(());
  }

  // Without the terminal frontend there is no interactive UI to launch; only
  // the CLI subcommands work. Bail before any first-run prompt would fire.
  #[cfg(not(feature = "tui"))]
  if matches.subcommand_name().is_none() {
    return Err(anyhow!(
      "this spotatui build has no terminal UI (compiled without the `tui` feature); run a CLI subcommand instead"
    ));
  }

  // The console implementation of the first-launch surface; core and infra
  // only ever see the `Onboarding` trait. `Arc` so the blocking streaming
  // credential task in the UI launch can hold its own handle.
  #[cfg(feature = "tui")]
  let onboarding: Arc<dyn crate::core::onboarding::Onboarding> =
    Arc::new(crate::tui::onboarding::ConsoleOnboarding);
  #[cfg(not(feature = "tui"))]
  let onboarding: Arc<dyn crate::core::onboarding::Onboarding> = Arc::new(HeadlessOnboarding);

  let boot = bootstrap::boot(&matches, onboarding, &mut instance_lock).await?;

  // Work with the cli (not really async)
  if let Some(cmd) = matches.subcommand_name() {
    info!("running in cli mode with command: {}", cmd);
    // Safe, because we checked if the subcommand is present at runtime
    let m = matches.subcommand_matches(cmd).unwrap();
    cli::run_subcommand(boot, cmd, m).await?;
  // Launch the UI (async)
  } else {
    #[cfg(feature = "tui")]
    startup::launch_ui(boot).await?;
    #[cfg(not(feature = "tui"))]
    unreachable!("headless builds reject a UI launch before boot");
  }

  // On stderr, same reasoning as the "Logging to:" notice in `setup_logging`:
  // stdout is reserved for program output (history HTML, MCP JSON-RPC). Only
  // reached by the CLI-subcommand and UI-launch paths above; `--completions`,
  // `update`, `history`, `mcp`, `plugin`, and the headless no-subcommand error
  // all return earlier and skip it. The failure case is covered by `run_cli`.
  eprintln!(
    "{}",
    logging::exit_log_notice(
      &crate::core::paths::app_log_path(),
      log::max_level() >= log::LevelFilter::Debug,
    )
  );

  Ok(())
}
