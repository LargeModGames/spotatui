//! Console implementation of the first-launch surface.
//!
//! Reproduces the pre-trait stdout byte for byte: `info` is a `println!`,
//! `progress` a flushed `print!`, `prompt_line` shows the prompt verbatim and
//! reads one raw line, and `pick_sources` is the terminal source picker in
//! `tui/first_run.rs`.

use crate::core::onboarding::{
  confirm_answer, Onboarding, OnboardingAnswer, OnboardingPrompt, BANNER_RULE,
};
use crate::core::source::Source;
use anyhow::Result;
use std::io::{stdin, stdout, Write};

pub struct ConsoleOnboarding;

impl Onboarding for ConsoleOnboarding {
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
    println!("\n{BANNER_RULE}\n{title}\n{BANNER_RULE}\n{body}");
    println!("{question}");
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
    super::first_run::pick_sources(options)
  }
}
