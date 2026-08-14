//! Console implementation of the first-launch surface.
//!
//! Reproduces the pre-trait stdout byte for byte: `info` is a `println!`,
//! `progress` a flushed `print!`, `prompt_line` shows the prompt verbatim and
//! reads one raw line, and `pick_sources` is the terminal source picker in
//! `tui/first_run.rs`.

use crate::core::onboarding::Onboarding;
use crate::core::source::Source;
use anyhow::Result;
use std::io::{stdin, stdout, Write};

pub struct ConsoleOnboarding;

impl Onboarding for ConsoleOnboarding {
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
