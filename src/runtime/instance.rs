//! Keeps a second UI launch from running beside the first one.

use anyhow::{Context, Result};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

/// The refusal of a second UI launch: expected, so not reported as a crash.
#[derive(Debug)]
pub(super) struct AlreadyRunning {
  holder: String,
}

impl std::fmt::Display for AlreadyRunning {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "spotatui is already running{}. Quit it first; Spotify playback can still be controlled with `spotatui playback`.",
      self.holder
    )
  }
}

impl std::error::Error for AlreadyRunning {}

/// Only a UI launch (no subcommand) locks, so the CLI keeps working beside a running UI.
pub(super) fn takes_lock(subcommand: Option<&str>) -> bool {
  subcommand.is_none()
}

/// The single-instance lock for a UI launch, held for as long as the file lives.
/// `None` when this system cannot lock; the launch then goes ahead unlocked.
pub(super) fn acquire() -> Result<Option<File>> {
  let Some(dir) = crate::core::paths::app_state_dir() else {
    log::warn!("[instance] no state directory, running without the single-instance lock");
    return Ok(None);
  };
  acquire_in(&dir)
}

fn acquire_in(dir: &Path) -> Result<Option<File>> {
  let opened = crate::core::paths::ensure_private_dir(dir).and_then(|()| {
    OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(false)
      .open(dir.join("instance.lock"))
      .context("opening the instance lock")
  });
  let file = match opened {
    Ok(file) => file,
    Err(e) => {
      log::warn!("[instance] running without the single-instance lock: {e:#}");
      return Ok(None);
    }
  };
  match file.try_lock() {
    Ok(()) => {
      let _ = std::fs::write(dir.join("instance.pid"), std::process::id().to_string());
      Ok(Some(file))
    }
    Err(TryLockError::WouldBlock) => {
      let holder = std::fs::read_to_string(dir.join("instance.pid"))
        .ok()
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .map(|pid| format!(" (pid {pid})"))
        .unwrap_or_default();
      Err(AlreadyRunning { holder }.into())
    }
    Err(TryLockError::Error(e)) => {
      log::warn!("[instance] running without the single-instance lock: {e}");
      Ok(None)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_a_ui_launch_takes_the_lock() {
    assert!(takes_lock(None));
    assert!(!takes_lock(Some("playback")));
    assert!(!takes_lock(Some("sync")));
  }

  #[test]
  fn a_second_launch_is_refused_while_the_first_holds_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let _first = acquire_in(dir.path())
      .unwrap()
      .expect("the first launch takes the lock");

    let error = acquire_in(dir.path()).unwrap_err();

    assert!(error.is::<AlreadyRunning>());
    let error = error.to_string();
    assert!(
      error.contains(&format!("pid {}", std::process::id())),
      "{error}"
    );
  }

  #[test]
  fn a_lock_file_left_by_an_exited_holder_does_not_block_the_next_launch() {
    let dir = tempfile::tempdir().unwrap();
    drop(acquire_in(dir.path()).unwrap());

    assert!(acquire_in(dir.path()).unwrap().is_some());
  }
}
