//! The first-launch interaction surface.
//!
//! Core and infra never talk to the console: every interactive first-run flow
//! (the source picker, Spotify client setup, OAuth URL display, the manual
//! redirect paste, source credential prompts) goes through this trait so the
//! frontend decides how to present it. The terminal frontend's
//! `ConsoleOnboarding` (`tui/onboarding.rs`) reproduces the historical stdout
//! byte for byte; a windowed frontend can implement the same surface with
//! dialogs instead.
//!
//! The retry loops, validation, and persistence stay with the callers - this
//! trait is only the presentation boundary.

use crate::core::source::Source;
use anyhow::Result;

pub trait Onboarding: Send + Sync {
  /// Show one informational message, terminated like a line (a `println!` on
  /// the console). Multi-line text is passed whole.
  fn info(&self, text: &str);

  /// Show the beginning of an in-progress line ("Testing connection... "),
  /// completed by a later [`Self::info`] (a flushed `print!` on the console).
  // Only the source-configuration flows emit progress fragments; builds
  // without those sources have no caller.
  #[cfg_attr(not(any(feature = "subsonic", feature = "youtube")), allow(dead_code))]
  fn progress(&self, text: &str);

  /// Show `prompt` verbatim (nothing appended) and read one line of input,
  /// returned exactly as read - trailing newline included, as
  /// [`std::io::BufRead::read_line`] produces it. Callers trim where they
  /// mean to.
  fn prompt_line(&self, prompt: &str) -> Result<String>;

  /// First-run source picker over the compiled-in `options`. `None` means the
  /// user cancelled or confirmed with nothing selected (the caller falls
  /// through to the Spotify wizard, matching the historical default).
  fn pick_sources(&self, options: &[Source]) -> Result<Option<Vec<Source>>>;
}
