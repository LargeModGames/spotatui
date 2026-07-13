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
//! Spotify (slim / current macOS release) shows no picker and keeps the original
//! behavior.

use crate::core::config::ClientConfig;
use crate::core::source::Source;
use crate::core::user_config::UserConfig;
use anyhow::{anyhow, Result};
use crossterm::{
  cursor,
  event::{read, Event, KeyCode, KeyEventKind, KeyModifiers},
  execute,
  style::Stylize,
  terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
  tty::IsTty,
};
use std::io::{stdin, stdout, Write};

/// Run the interactive first-run source picker. A no-op after the first launch
/// (detected by the presence of `client.yml`) and when only Spotify is compiled
/// in. Must be called before [`ClientConfig::load_config`], which would otherwise
/// trigger the Spotify-only auth wizard on a fresh install.
pub async fn run_first_run_picker(
  user_config: &mut UserConfig,
  client_config: &mut ClientConfig,
) -> Result<()> {
  // First run is detected by the absence of the Spotify client config file.
  let paths = client_config.get_or_build_paths()?;
  if paths.config_file_path.exists() {
    return Ok(());
  }

  let options = compiled_in_sources();

  // Only Spotify available (slim build / macOS release without the source
  // features): keep today's behavior and let `load_config` run the wizard.
  if options.len() == 1 {
    return Ok(());
  }

  // Collect the chosen sources. Interactive terminals get the checkbox picker;
  // piped / non-interactive stdin falls back to the numbered single-select prompt
  // so headless and scripted runs never hang on a raw-mode read.
  let selections = if stdout().is_tty() && stdin().is_tty() {
    match interactive_multiselect(&options)? {
      Some(selected) => selected,
      // Cancelled (esc / ctrl-c) or nothing checked: fall through to the Spotify
      // wizard, matching the historical default.
      None => return Ok(()),
    }
  } else {
    numbered_fallback(&options)?
  };

  apply_selections(selections, user_config, client_config).await
}

/// Act on the sources the user chose. `active_source` is set to the first checked
/// source in display order; every checked free source has its config collected.
async fn apply_selections(
  selections: Vec<Source>,
  user_config: &mut UserConfig,
  client_config: &mut ClientConfig,
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
  user_config.behavior.active_source = active;
  // The global song counter opt-in is asked before this picker runs, so the
  // user's choice already sits on `user_config`; save_config persists it here.
  user_config.save_config()?;

  // Collect credentials / check prerequisites for each chosen free source.
  for source in &selections {
    if *source != Source::Spotify {
      configure_source(*source, user_config).await?;
    }
  }

  if spotify_selected {
    // Fall through: `load_config` runs the existing Spotify auth wizard.
    println!("\nSetting up your other sources, then we'll log in to Spotify...\n");
    return Ok(());
  }

  println!(
    "\nStarting spotatui with {} as your source. Press `d` anytime to switch or to log in to Spotify.\n",
    active.label()
  );

  Ok(())
}

/// Interactive checkbox picker: arrow keys / j,k to move, space to toggle, enter
/// to confirm, esc to skip. Returns the checked sources in display order, or
/// `None` when the user cancels or confirms with nothing selected.
///
/// Restores the terminal via a [`RawModeGuard`] on every exit path (early return,
/// `?`, or panic) so a mid-selection error never leaves the terminal in raw mode.
fn interactive_multiselect(options: &[Source]) -> Result<Option<Vec<Source>>> {
  println!("\nWelcome to spotatui! Choose your music sources:");
  println!("You can add or switch sources anytime from the `d` menu.\n");

  enable_raw_mode()?;
  let _guard = RawModeGuard;

  let mut checked = vec![false; options.len()];
  let mut hover = 0usize;
  // Option lines + a blank spacer + the instructions line.
  let line_count = (options.len() + 2) as u16;
  let mut out = stdout();
  let mut first_draw = true;

  loop {
    if !first_draw {
      execute!(
        out,
        cursor::MoveToPreviousLine(line_count),
        Clear(ClearType::FromCursorDown)
      )?;
    }
    first_draw = false;

    for (index, source) in options.iter().enumerate() {
      let pointer = if index == hover { ">" } else { " " };
      let checkbox = if checked[index] { "[x]" } else { "[ ]" };
      let line = format!(
        "  {pointer} {checkbox} {}{}",
        source.label(),
        source_note(*source)
      );
      if index == hover {
        print!("{}\r\n", line.cyan().bold());
      } else {
        print!("{line}\r\n");
      }
    }
    print!("\r\n");
    print!(
      "  {}\r\n",
      "↑/↓ move · space select · enter confirm · esc skip".dark_grey()
    );
    out.flush()?;

    let event = read()?;
    let key = match event {
      // Ignore key-release / repeat events (Windows emits them) and non-key events.
      Event::Key(key) if key.kind == KeyEventKind::Press => key,
      _ => continue,
    };

    match key.code {
      KeyCode::Up | KeyCode::Char('k') => {
        hover = if hover == 0 {
          options.len() - 1
        } else {
          hover - 1
        };
      }
      KeyCode::Down | KeyCode::Char('j') => {
        hover = (hover + 1) % options.len();
      }
      KeyCode::Char(' ') => checked[hover] = !checked[hover],
      KeyCode::Enter => {
        let selected: Vec<Source> = options
          .iter()
          .zip(&checked)
          .filter_map(|(source, &on)| on.then_some(*source))
          .collect();
        return Ok(if selected.is_empty() {
          None
        } else {
          Some(selected)
        });
      }
      KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
      KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
      _ => {}
    }
  }
}

/// Restores cooked terminal mode when dropped, so any exit path out of the
/// interactive picker (return, `?`, panic) leaves the terminal usable.
struct RawModeGuard;

impl Drop for RawModeGuard {
  fn drop(&mut self) {
    let _ = disable_raw_mode();
  }
}

/// Non-interactive fallback (piped stdin): the original numbered single-select
/// prompt. Returns a single-element `Vec` for a uniform downstream path.
fn numbered_fallback(options: &[Source]) -> Result<Vec<Source>> {
  println!("\nWelcome to spotatui! Choose your music source:\n");
  for (index, source) in options.iter().enumerate() {
    println!(
      "  {}) {}{}",
      index + 1,
      source.label(),
      source_note(*source)
    );
  }
  println!("\nYou can add or switch sources anytime from the `d` menu.");

  let choice = prompt_choice(options.len())?;
  Ok(vec![options[choice - 1]])
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
      feature = "local-files"
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
  options
}

fn source_note(source: Source) -> &'static str {
  match source {
    Source::Spotify => " (needs login)",
    Source::YouTube => " (free, needs the yt-dlp binary)",
    Source::Subsonic => " (free, needs a Subsonic/Navidrome server)",
    Source::Radio => " (free)",
    Source::Local => " (free)",
  }
}

// `user_config` is only read by credential/config-collecting sources; a build
// with none of them (slim) leaves it unused.
#[cfg_attr(
  not(any(feature = "subsonic", feature = "youtube", feature = "local-files")),
  allow(unused_variables)
)]
async fn configure_source(source: Source, user_config: &mut UserConfig) -> Result<()> {
  match source {
    #[cfg(feature = "subsonic")]
    Source::Subsonic => configure_subsonic(user_config).await?,
    #[cfg(feature = "youtube")]
    Source::YouTube => configure_youtube(user_config),
    #[cfg(feature = "local-files")]
    Source::Local => configure_local(user_config),
    // Radio needs no setup; other sources are handled above when compiled in.
    _ => {}
  }
  Ok(())
}

#[cfg(feature = "subsonic")]
async fn configure_subsonic(user_config: &mut UserConfig) -> Result<()> {
  println!("\nSubsonic / Navidrome setup:");
  let url = prompt_line("Server URL (e.g. https://demo.navidrome.org)")?;
  let username = prompt_line("Username")?;
  let password = prompt_line("Password")?;

  user_config.behavior.subsonic_url = Some(url.clone());
  user_config.behavior.subsonic_username = Some(username.clone());
  user_config.behavior.subsonic_password = Some(password.clone());
  user_config.save_config()?;

  // Best-effort connectivity check: a failure is not fatal (the server may just
  // be temporarily down), the details are already saved.
  print!("Testing connection... ");
  let _ = stdout().flush();
  let client = crate::infra::subsonic::SubsonicSource::new(url, username, password);
  match client.ping().await {
    Ok(()) => println!("OK"),
    Err(e) => {
      println!("failed: {e}");
      println!(
        "Saved anyway. Fix the details in ~/.config/spotatui/config.yml and relaunch if needed."
      );
    }
  }

  Ok(())
}

#[cfg(feature = "youtube")]
fn configure_youtube(user_config: &UserConfig) {
  let ytdlp = user_config
    .behavior
    .ytdlp_path
    .clone()
    .unwrap_or_else(|| "yt-dlp".to_string());

  print!("\nChecking for yt-dlp... ");
  let _ = stdout().flush();
  match std::process::Command::new(&ytdlp).arg("--version").output() {
    Ok(output) if output.status.success() => {
      let version = String::from_utf8_lossy(&output.stdout);
      println!("found ({})", version.trim());
    }
    _ => {
      println!("not found");
      println!("YouTube playback needs the `yt-dlp` binary on your PATH.");
      println!("Install it (e.g. `pipx install yt-dlp` or your distro package) and relaunch.");
      println!(
        "If it lives at a custom path, set behavior.ytdlp_path in ~/.config/spotatui/config.yml."
      );
    }
  }
}

#[cfg(feature = "local-files")]
fn configure_local(user_config: &UserConfig) {
  match &user_config.behavior.local_music_path {
    Some(path) => {
      println!("\nLocal files will be read from: {path}");
      println!("(Change behavior.local_music_path in config.yml to use another folder.)");
    }
    None => {
      println!("\nNo music folder was detected automatically.");
      println!("Set behavior.local_music_path in ~/.config/spotatui/config.yml.");
    }
  }
}

fn prompt_choice(max: usize) -> Result<usize> {
  const MAX_RETRIES: u8 = 5;
  let mut retries = 0;
  loop {
    print!("\nChoose (1-{max}): ");
    let _ = stdout().flush();
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    match input.trim().parse::<usize>() {
      Ok(n) if (1..=max).contains(&n) => return Ok(n),
      _ => {
        println!("Invalid choice. Please enter a number between 1 and {max}.");
        retries += 1;
        if retries >= MAX_RETRIES {
          return Err(anyhow!("Maximum retries ({MAX_RETRIES}) exceeded."));
        }
      }
    }
  }
}

// Only credential-collecting sources (currently Subsonic) use this.
#[cfg_attr(not(feature = "subsonic"), allow(dead_code))]
fn prompt_line(label: &str) -> Result<String> {
  const MAX_RETRIES: u8 = 5;
  let mut retries = 0;
  loop {
    print!("  {label}: ");
    let _ = stdout().flush();
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_string();
    if !trimmed.is_empty() {
      return Ok(trimmed);
    }
    println!("  (required)");
    retries += 1;
    if retries >= MAX_RETRIES {
      return Err(anyhow!("Maximum retries ({MAX_RETRIES}) exceeded."));
    }
  }
}
