//! The command-line surface: clap assembly, the self-update plumbing, and
//! CLI-mode dispatch of one subcommand against the network layer.

use super::bootstrap::Boot;
use crate::cli;
use crate::core::banner::BANNER;
use crate::core::user_config::UserConfig;
use crate::infra::network::Network;
use anyhow::{anyhow, Context, Result};
use clap::{Arg, ArgMatches, Command as ClapApp};
use std::sync::Arc;

pub(super) fn build_clap_app() -> ClapApp {
  // `mut` is only exercised by the feature-gated subcommand additions below.
  #[cfg_attr(
    not(any(feature = "scripting", feature = "mcp-server")),
    allow(unused_mut)
  )]
  let mut clap_app = add_self_update_cli(
    ClapApp::new(env!("CARGO_PKG_NAME"))
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about(env!("CARGO_PKG_DESCRIPTION"))
    .override_usage("Press `?` while running the app to see keybindings")
    .before_help(BANNER)
    .after_help(
      "Client authentication settings are stored in the spotatui config directory (use --reconfigure-auth to update them)",
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
      Arg::new("play-file")
        .long("play-file")
        .value_name("PATH")
        .help("Play a local audio file on startup (requires the local-files build feature)."),
    )
    .arg(
      Arg::new("completions")
        .long("completions")
        .help("Generates completions for your preferred shell")
        .value_parser(["bash", "zsh", "fish", "power-shell", "elvish"])
        .value_name("SHELL"),
    )
    .arg(
      Arg::new("debug")
        .long("debug")
        .global(true)
        .action(clap::ArgAction::SetTrue)
        .help("Enable verbose debug logging for bug reports (also SPOTATUI_LOG=debug)")
        .long_help(
          "Enable verbose debug logging for spotatui and the native streaming crates it embeds \
(equivalent to SPOTATUI_LOG=debug). At this level the log records identifying details such as \
your Spotify user id and playlist ids. SPOTATUI_LOG=trace goes further and adds raw audio packet \
activity and full API response bodies, which include your country. Access \
tokens and request headers are never logged, but read through the file before posting it \
publicly.",
        ),
    )
    // Control spotify from the command line
    .subcommand(cli::playback_subcommand())
    .subcommand(cli::play_subcommand())
    .subcommand(cli::list_subcommand())
    .subcommand(cli::history_subcommand())
    .subcommand(cli::search_subcommand())
    .subcommand(cli::sync_subcommand()),
  );

  #[cfg(feature = "scripting")]
  {
    clap_app = clap_app.subcommand(cli::plugin_subcommand());
  }

  #[cfg(feature = "mcp-server")]
  {
    clap_app = clap_app.subcommand(cli::mcp_subcommand());
  }

  clap_app
}

/// CLI mode: run one subcommand against the network layer and print its
/// result.
pub(super) async fn run_subcommand(boot: Boot, cmd: &str, matches: &ArgMatches) -> Result<()> {
  if cmd == "sync" {
    return run_sync(boot, matches).await;
  }
  let app = boot.app;
  // Held (unread) for the length of the command; see the field doc on `Boot`.
  let _sync_io_rx = boot.sync_io_rx;
  // Both `Network::new` variants share one signature, so the same call
  // compiles with or without `streaming`. CLI mode never uses streaming.
  let network = Network::new(
    boot.spotify,
    boot.client_config,
    &app,
    boot.token_cache_path,
  );
  let cli_result = cli::handle_matches(matches, cmd.to_string(), network, boot.user_config).await;
  app.lock().await.flush_state_save(true);
  println!("{}", cli_result?);
  Ok(())
}

/// The `sync` subcommand: no device probe, no Spotify requirement, its own exit signal.
async fn run_sync(boot: Boot, matches: &ArgMatches) -> Result<()> {
  let app = boot.app;
  // Held (unread) for the length of the command; see the field doc on `Boot`.
  let _sync_io_rx = boot.sync_io_rx;
  let args = cli::sync_args(matches);
  let ctx = crate::infra::playlist_sync::SyncContext::new(
    boot.spotify,
    boot.token_cache_path,
    Arc::clone(&app),
  );
  let run = crate::infra::playlist_sync::run_guarded(ctx, args.link, args.dry_run, true);
  let report = tokio::spawn(run)
    .await
    .context("playlist sync task failed")?;
  app.lock().await.flush_state_save(true);
  println!("{}", report.printable());
  if report.failed() {
    return Err(anyhow!("playlist sync failed"));
  }
  Ok(())
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
pub(super) async fn handle_self_update_command(matches: &ArgMatches) -> Result<bool> {
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
pub(super) async fn handle_self_update_command(_matches: &ArgMatches) -> Result<bool> {
  Ok(false)
}

/// Run the auto-update check, returning the new version when one was installed.
///
/// Deliberately does NOT restart here. This runs concurrently with
/// authentication, and restarting mid-authentication deadlocks startup: the
/// re-exec blocks this task in `Command::status()`, which stops the joined
/// authentication future from being polled while it still owns the OAuth
/// callback port, so the child cannot bind that port and the parent never
/// releases it. See `restart_after_update`, which the caller invokes once
/// authentication has finished.
#[cfg(feature = "self-update")]
pub(super) async fn run_auto_update(
  matches: &ArgMatches,
  user_config: &UserConfig,
) -> Option<String> {
  if matches.subcommand_name().is_some()
    || std::env::var_os("SPOTATUI_SKIP_UPDATE").is_some()
    || matches.get_flag("no-update")
    || user_config.behavior.disable_auto_update
  {
    return None;
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
    Some(cli::UpdateOutcome::Installed(new_version)) => Some(new_version),
    Some(cli::UpdateOutcome::Pending {
      version,
      secs_remaining,
    }) => {
      println!(
        "Update v{} detected — will install in {}. Run `spotatui update --install` to update now.",
        version,
        crate::core::user_config::format_update_delay_secs(secs_remaining)
      );
      None
    }
    // Up-to-date, check failed, or no update — continue normally.
    _ => None,
  }
}

#[cfg(not(feature = "self-update"))]
pub(super) async fn run_auto_update(
  _matches: &ArgMatches,
  _user_config: &UserConfig,
) -> Option<String> {
  None
}

/// Re-exec into the freshly installed binary. Never returns when an update was
/// installed.
///
/// Must be called only once startup is no longer holding resources the child
/// will need. The child repeats startup from scratch, so anything this process
/// still owns (the OAuth callback port on 8989, stdin, the terminal) it would
/// contend with. Authentication persists its token before returning, so the
/// child reuses it rather than opening a second browser login. The
/// single-instance lock is released just before the re-exec, since the child
/// takes it again while this process waits.
pub(super) fn restart_after_update(
  new_version: Option<String>,
  instance_lock: &mut Option<std::fs::File>,
) -> Result<()> {
  let Some(new_version) = new_version else {
    return Ok(());
  };

  println!("Updated to v{}! Restarting...", new_version);
  // Re-exec the current binary with the same args, skipping the update check.
  // Reported rather than panicked: the update is already installed by now, so
  // an unreadable executable path should surface as an error the user can act
  // on, not a backtrace.
  let exe = std::env::current_exe()
    .context("failed to get current executable path while restarting after an update")?;
  let args: Vec<String> = std::env::args().skip(1).collect();
  drop(instance_lock.take());
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

#[cfg(test)]
mod tests {
  use super::*;

  /// `--debug` is the flag every bug report in issue #566 is going to be asked
  /// for, and the help text is the only place a user learns that the log can
  /// carry personal data. Renaming either is a silent regression otherwise:
  /// nothing else in the suite builds the clap app.
  #[test]
  fn debug_flag_parses_and_defaults_to_off() {
    let app = build_clap_app();
    let matches = app
      .clone()
      .try_get_matches_from(["spotatui", "--debug"])
      .expect("--debug must parse");
    assert!(matches.get_flag("debug"));

    let matches = app
      .try_get_matches_from(["spotatui"])
      .expect("no args must parse");
    assert!(!matches.get_flag("debug"));
  }

  /// `--debug` needs `global(true)` to parse after a subcommand
  /// (`spotatui playback --status --debug`), not just before one. `run_cli`
  /// reads it off the root `ArgMatches` (`matches.get_flag("debug")` in
  /// `runtime/mod.rs`), and clap copies a global flag's value back up to
  /// every parent level it was propagated through, so that read keeps working
  /// once the flag is placed on the subcommand line instead of the root one.
  #[test]
  fn debug_flag_parses_after_a_subcommand() {
    let matches = build_clap_app()
      .try_get_matches_from(["spotatui", "playback", "--status", "--debug"])
      .expect("--debug after a subcommand must parse");
    assert!(matches.get_flag("debug"));
  }

  #[test]
  fn debug_flag_long_help_warns_about_personal_data() {
    let help = build_clap_app()
      .get_arguments()
      .find(|arg| arg.get_id() == "debug")
      .and_then(|arg| arg.get_long_help().or_else(|| arg.get_help()))
      .map(ToString::to_string)
      .expect("--debug must document itself");
    let help = help.to_lowercase();
    assert!(
      help.contains("before posting"),
      "the long help must tell the user to read the log before posting it:\n{help}"
    );
    assert!(
      help.contains("country"),
      "the long help must name what trace adds to the log:\n{help}"
    );
  }
}
