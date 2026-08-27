//! Console presentation of the first-run source picker.
//!
//! The selection *logic* (which sources are compiled in, what happens with the
//! choice) lives in `core/first_run.rs`; this module is only the terminal UI it
//! is presented with: a crossterm checkbox picker on a real terminal, a
//! numbered single-select prompt when stdin is piped.

use crate::core::source::Source;
use anyhow::{anyhow, Result};
use crossterm::{
  cursor,
  event::{read, Event, KeyCode, KeyEventKind, KeyModifiers},
  execute,
  style::Stylize,
  terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::io::{stdin, stdout, IsTerminal, Write};

/// Console source picker. Interactive terminals get the checkbox picker; piped
/// / non-interactive stdin falls back to the numbered single-select prompt so
/// headless and scripted runs never hang on a raw-mode read. `None` means the
/// user cancelled or confirmed with nothing selected.
pub fn pick_sources(options: &[Source]) -> Result<Option<Vec<Source>>> {
  if stdout().is_terminal() && stdin().is_terminal() {
    interactive_multiselect(options)
  } else {
    numbered_fallback(options).map(Some)
  }
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

fn source_note(source: Source) -> &'static str {
  match source {
    Source::Spotify => " (needs login)",
    Source::YouTube => " (free, needs the yt-dlp binary)",
    Source::Subsonic => " (free, needs a Subsonic/Navidrome server)",
    Source::Radio => " (free)",
    Source::Local => " (free)",
    Source::Qobuz => " (paid subscription, logs in through the browser)",
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
