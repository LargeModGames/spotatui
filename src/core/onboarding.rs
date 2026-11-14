//! The first-launch interaction surface.
//!
//! Core and infra never talk to the console: every interactive first-run flow
//! (the source picker, the Spotify pairing code, source credential prompts)
//! goes through this trait so the
//! frontend decides how to present it. The terminal frontend's
//! `ConsoleOnboarding` (`tui/onboarding.rs`) reproduces the historical stdout
//! byte for byte; a windowed frontend can implement the same surface with
//! dialogs instead.
//!
//! The retry loops, validation, and persistence stay with the callers - this
//! trait is only the presentation boundary.

use crate::core::i18n::{Language, Strings, EN};
use crate::core::source::Source;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// The 58-character rule line both first-run banners draw.
pub(crate) const BANNER_RULE: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";

/// A typed question a caller needs answered before it can continue. The
/// frontend owns presentation and input parsing; callers never interpret
/// prompt prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnboardingPrompt {
  /// A yes/no question. The terminal frontend draws the banner rule around
  /// `title`, then `body`, then asks `question` on its own line.
  Confirm {
    title: String,
    body: String,
    question: String,
  },
}

/// The typed answer to [`OnboardingPrompt::Confirm`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnboardingAnswer {
  Yes,
  No,
}

/// The console text of an [`OnboardingPrompt::Confirm`]: the banner, the body, and the question with its `(Y/n)` hint.
pub(crate) fn confirm_console_text(title: &str, body: &str, question: &str) -> String {
  format!("\n{BANNER_RULE}\n{title}\n{BANNER_RULE}\n\n{body}\n\n{question} (Y/n): ")
}

/// Parse one raw console line for [`OnboardingPrompt::Confirm`]: empty input,
/// `y`, or `yes` (case-insensitive) means yes; anything else means no.
pub(crate) fn confirm_answer(raw: &str) -> OnboardingAnswer {
  match raw.trim().to_lowercase().as_str() {
    "" | "y" | "yes" => OnboardingAnswer::Yes,
    _ => OnboardingAnswer::No,
  }
}

pub trait Onboarding: Send + Sync {
  /// UI text for this frontend's resolved language. Scripted and headless
  /// callers keep the historical English surface unless they opt in.
  fn strings(&self) -> &'static Strings {
    &EN
  }

  /// Select the resolved language after configuration has been loaded.
  /// Frontends with their own language state may keep that selection instead.
  fn set_language(&self, _language: Language) {}

  /// Whether this frontend can ask questions at all. The terminal frontend
  /// checks stdin and stdout; a windowed frontend answers true while its
  /// event loop runs.
  fn is_interactive(&self) -> bool;

  /// Ask one typed question and wait for the answer. Blocking on the
  /// terminal frontend; a windowed frontend bridges to its event loop from
  /// a blocking thread.
  fn ask(&self, prompt: &OnboardingPrompt) -> Result<OnboardingAnswer>;

  /// Show one informational message, terminated like a line (a `println!` on
  /// the console). Multi-line text is passed whole.
  fn info(&self, text: &str);

  /// Show the beginning of an in-progress line ("Testing connection... "),
  /// completed by a later [`Self::info`] (a flushed `print!` on the console).
  // Only the source-configuration flows emit progress fragments; builds
  // without those sources have no caller.
  #[cfg_attr(
    not(any(feature = "subsonic", feature = "qobuz", feature = "youtube")),
    allow(dead_code)
  )]
  fn progress(&self, text: &str);

  /// Show `prompt` verbatim (nothing appended) and read one line of input,
  /// returned exactly as read - trailing newline included, as
  /// [`std::io::BufRead::read_line`] produces it. Callers trim where they
  /// mean to.
  fn prompt_line(&self, prompt: &str) -> Result<String>;

  /// [`Self::prompt_line`] for a value the frontend must not echo.
  #[cfg_attr(not(feature = "subsonic"), allow(dead_code))]
  fn prompt_masked(&self, prompt: &str) -> Result<String> {
    self.prompt_line(prompt)
  }

  /// First-run source picker over the compiled-in `options`. `None` means the
  /// user cancelled or confirmed with nothing selected (the caller falls
  /// through to the Spotify wizard, matching the historical default).
  fn pick_sources(&self, options: &[Source]) -> Result<Option<Vec<Source>>>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_console_confirm_keeps_the_banner_layout_and_adds_the_yes_no_hint() {
    let text = confirm_console_text("Title", "Body.", "Join?");
    assert_eq!(
      text,
      format!("\n{BANNER_RULE}\nTitle\n{BANNER_RULE}\n\nBody.\n\nJoin? (Y/n): ")
    );
  }

  #[test]
  fn confirm_answer_treats_empty_y_and_yes_as_yes() {
    assert_eq!(confirm_answer(""), OnboardingAnswer::Yes);
    assert_eq!(confirm_answer("\n"), OnboardingAnswer::Yes);
    assert_eq!(confirm_answer(" y "), OnboardingAnswer::Yes);
    assert_eq!(confirm_answer("YES\n"), OnboardingAnswer::Yes);
    assert_eq!(confirm_answer("n"), OnboardingAnswer::No);
    assert_eq!(confirm_answer("no"), OnboardingAnswer::No);
    assert_eq!(confirm_answer("banana"), OnboardingAnswer::No);
  }
}
