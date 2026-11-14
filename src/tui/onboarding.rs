//! Console implementation of the first-launch surface.
//!
//! Reproduces the pre-trait stdout byte for byte: `info` is a `println!`,
//! `progress` a flushed `print!`, `prompt_line` shows the prompt verbatim and
//! reads one raw line, and `pick_sources` is the terminal source picker in
//! `tui/first_run.rs`.

use crate::core::i18n::{Language, Strings, EN};
use crate::core::onboarding::{
  confirm_answer, confirm_console_text, Onboarding, OnboardingAnswer, OnboardingPrompt,
};
use crate::core::source::Source;
use anyhow::Result;
use std::io::{stdin, stdout, Write};
use std::sync::Mutex;

pub struct ConsoleOnboarding {
  strings: Mutex<&'static Strings>,
}

impl Default for ConsoleOnboarding {
  fn default() -> Self {
    Self {
      strings: Mutex::new(&EN),
    }
  }
}

impl Onboarding for ConsoleOnboarding {
  fn strings(&self) -> &'static Strings {
    *self
      .strings
      .lock()
      .unwrap_or_else(|error| error.into_inner())
  }

  fn set_language(&self, language: Language) {
    *self
      .strings
      .lock()
      .unwrap_or_else(|error| error.into_inner()) = language.strings();
  }

  fn is_interactive(&self) -> bool {
    use std::io::IsTerminal;
    stdin().is_terminal() && stdout().is_terminal()
  }

  fn ask(&self, prompt: &OnboardingPrompt) -> Result<OnboardingAnswer> {
    let (title, body, question) = match prompt {
      OnboardingPrompt::Confirm {
        title,
        body,
        question,
      } => (title, body, question),
    };
    println!("{}", confirm_console_text(title, body, question));
    let _ = stdout().flush();
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    Ok(confirm_answer(&input))
  }

  fn info(&self, text: &str) {
    println!("{text}");
  }

  fn progress(&self, text: &str) {
    print!("{text}");
    let _ = stdout().flush();
  }

  fn prompt_line(&self, prompt: &str) -> Result<String> {
    print!("{prompt}");
    let _ = stdout().flush();
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    Ok(input)
  }

  fn pick_sources(&self, options: &[Source]) -> Result<Option<Vec<Source>>> {
    super::first_run::pick_sources_with(options, self.strings())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn console_language_changes_only_its_own_strings() {
    let localized = ConsoleOnboarding::default();
    let other = ConsoleOnboarding::default();
    assert!(std::ptr::eq(localized.strings(), &EN));

    for language in [Language::ZhHans, Language::ZhHant, Language::En] {
      localized.set_language(language);
      assert!(std::ptr::eq(localized.strings(), language.strings()));
      assert!(std::ptr::eq(other.strings(), &EN));
    }
  }
}
