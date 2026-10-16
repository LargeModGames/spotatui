//! The visualizer's audio capture, opened off the tick. Opening one waits on
//! the audio system (PipeWire's setup handshake, cpal's device scan), and the
//! tick runs under the `App` lock every frame, so the open runs on its own
//! thread and the tick only polls for the result. Backend-neutral: the
//! PipeWire and cpal capture managers both come through here.

use crate::core::app::App;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

/// Shown once per visit to the visualizer when the capture cannot open.
#[cfg(test)]
pub(super) const AUDIO_CAPTURE_UNAVAILABLE: &str =
  "Visualizer can't capture system audio - see the log for details.";
const AUDIO_CAPTURE_UNAVAILABLE_TTL_SECS: u64 = 8;

enum Slot<C> {
  /// The visualizer is closed, or open with nothing tried yet.
  Closed,
  /// The open is running on its thread; it sends exactly one result.
  Opening(Receiver<Option<C>>),
  Open(C),
  /// Opening failed during this visit to the visualizer. Not retried until
  /// the visualizer closes; `reported` once the message is on screen.
  Failed {
    reported: bool,
  },
}

/// Clears the open flag when its open ends, by return or by panic.
struct OpenEnded(Arc<AtomicBool>);

impl Drop for OpenEnded {
  fn drop(&mut self) {
    self.0.store(false, Ordering::Release);
  }
}

/// One capture per visit to the visualizer: opened once, in the background,
/// and dropped when the visualizer closes.
pub(super) struct VizCapture<C> {
  slot: Slot<C>,
  /// An open thread still runs, maybe one a close abandoned. A device that
  /// hangs its open would otherwise pile up one stuck thread per try.
  opening: Arc<AtomicBool>,
}

impl<C: Send + 'static> VizCapture<C> {
  pub(super) fn new() -> Self {
    Self {
      slot: Slot::Closed,
      opening: Arc::new(AtomicBool::new(false)),
    }
  }

  /// One tick while the visualizer shows; never blocks. Starts `open` on a
  /// background thread if nothing was tried yet this visit, installs its
  /// result once it arrives, and with `report` keeps offering the failure
  /// message until the status bar takes it (a live error status drops plain
  /// messages).
  pub(super) fn poll(
    &mut self,
    app: &mut App,
    report: bool,
    open: impl FnOnce() -> Option<C> + Send + 'static,
  ) -> Option<&C> {
    if matches!(self.slot, Slot::Closed) && !self.opening.load(Ordering::Acquire) {
      let (tx, rx) = mpsc::sync_channel(1);
      let opening = Arc::clone(&self.opening);
      opening.store(true, Ordering::Release);
      let spawned = thread::Builder::new()
        .name("audio-viz-open".into())
        .spawn(move || {
          // Cleared before the result goes out, so a poll that has it can
          // open again; also cleared when `open` panics.
          let capture = {
            let _ended = OpenEnded(opening);
            open()
          };
          // The receiver is gone if the visualizer closed meanwhile; the
          // capture is then dropped here.
          let _ = tx.send(capture);
        });
      self.slot = match spawned {
        Ok(_) => Slot::Opening(rx),
        Err(e) => {
          self.opening.store(false, Ordering::Release);
          log::error!("[audio-viz] could not start the capture thread: {e}");
          Slot::Failed { reported: false }
        }
      };
    }

    if let Slot::Opening(rx) = &self.slot {
      // A capture that does not report is tried again by its frontend; its
      // failures would fill the log.
      let level = if report {
        log::Level::Warn
      } else {
        log::Level::Debug
      };
      match rx.try_recv() {
        Ok(Some(capture)) => self.slot = Slot::Open(capture),
        // Logged once, on the transition: the status message points at the
        // log, and the cpal backend returns None without logging anything.
        Ok(None) => {
          log::log!(
            level,
            "[audio-viz] the audio capture could not be opened; the visualizer has no audio"
          );
          self.slot = Slot::Failed { reported: false };
        }
        Err(TryRecvError::Disconnected) => {
          log::log!(
            level,
            "[audio-viz] the capture thread ended without a result; the visualizer has no audio"
          );
          self.slot = Slot::Failed { reported: false };
        }
        Err(TryRecvError::Empty) => {}
      }
    }

    if let Slot::Failed { reported: false } = self.slot {
      let shown = !report || {
        app.set_status_message(
          app.strings().status.app.capture_unavailable,
          AUDIO_CAPTURE_UNAVAILABLE_TTL_SECS,
        );
        app.status_message() == Some(app.strings().status.app.capture_unavailable)
          && !app.status_message_is_error()
      };
      self.slot = Slot::Failed { reported: shown };
    }

    self.current()
  }

  /// The visualizer closed: drop the capture (or abandon a pending open) and
  /// forget a failure, so the next visit tries again. True when a capture
  /// was open.
  pub(super) fn close(&mut self) -> bool {
    matches!(
      std::mem::replace(&mut self.slot, Slot::Closed),
      Slot::Open(_)
    )
  }

  /// The capture, once it is open.
  pub(super) fn current(&self) -> Option<&C> {
    match &self.slot {
      Slot::Open(capture) => Some(capture),
      Slot::Closed | Slot::Opening(_) | Slot::Failed { .. } => None,
    }
  }

  #[cfg(test)]
  fn is_opening(&self) -> bool {
    matches!(self.slot, Slot::Opening(_))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::sync::mpsc::channel;
  use std::sync::Arc;
  use std::time::{Duration, Instant};

  #[test]
  fn audio_capture_status_fixture_matches_the_english_catalog() {
    assert_eq!(
      AUDIO_CAPTURE_UNAVAILABLE,
      crate::core::i18n::EN.status.app.capture_unavailable
    );
  }

  fn app() -> App {
    let (tx, _rx) = channel();
    App::new(tx, UserConfig::new(), None)
  }

  /// An opener that counts its runs and answers `result`.
  fn opener(
    attempts: &Arc<AtomicUsize>,
    result: Option<()>,
  ) -> impl FnOnce() -> Option<()> + Send + 'static {
    let attempts = attempts.clone();
    move || {
      attempts.fetch_add(1, Ordering::SeqCst);
      result
    }
  }

  /// Polls until the background open has answered.
  fn settle(
    capture: &mut VizCapture<()>,
    app: &mut App,
    attempts: &Arc<AtomicUsize>,
    result: Option<()>,
  ) {
    let deadline = Instant::now() + Duration::from_secs(5);
    capture.poll(app, true, opener(attempts, result));
    while capture.is_opening() {
      assert!(Instant::now() < deadline, "the open never answered");
      thread::sleep(Duration::from_millis(1));
      capture.poll(app, true, opener(attempts, result));
    }
  }

  #[test]
  fn a_capture_that_fails_to_open_is_reported_once_and_not_retried() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));

    settle(&mut capture, &mut app, &attempts, None);
    for _ in 0..3 {
      assert!(capture
        .poll(&mut app, true, opener(&attempts, None))
        .is_none());
    }

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(app.status_message(), Some(AUDIO_CAPTURE_UNAVAILABLE));
    assert!(!app.status_message_is_error());
  }

  #[test]
  fn a_capture_that_does_not_report_fails_without_a_status_message() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let deadline = Instant::now() + Duration::from_secs(5);

    capture.poll(&mut app, false, opener(&attempts, None));
    while capture.is_opening() {
      assert!(Instant::now() < deadline, "the open never answered");
      thread::sleep(Duration::from_millis(1));
      capture.poll(&mut app, false, opener(&attempts, None));
    }
    capture.poll(&mut app, false, opener(&attempts, None));

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(app.status_message(), None);
  }

  #[test]
  fn the_failure_message_waits_for_a_live_error_to_clear() {
    let mut app = app();
    app.set_error_status_message("Something else failed", 60);
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));

    settle(&mut capture, &mut app, &attempts, None);
    capture.poll(&mut app, true, opener(&attempts, None));
    assert_eq!(app.status_message(), Some("Something else failed"));

    // Stands in for the error's lifetime running out.
    app.clear_status_message();
    capture.poll(&mut app, true, opener(&attempts, None));

    assert_eq!(app.status_message(), Some(AUDIO_CAPTURE_UNAVAILABLE));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
  }

  #[test]
  fn closing_and_reopening_the_visualizer_retries_a_failed_capture() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));

    settle(&mut capture, &mut app, &attempts, None);
    assert!(!capture.close());
    settle(&mut capture, &mut app, &attempts, None);

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
  }

  #[test]
  fn an_open_capture_is_not_opened_again() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));

    settle(&mut capture, &mut app, &attempts, Some(()));
    for _ in 0..3 {
      assert!(capture
        .poll(&mut app, true, opener(&attempts, Some(())))
        .is_some());
    }

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(capture.close());
  }

  #[test]
  fn a_slow_open_does_not_block_the_tick() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let (release_tx, release_rx) = channel::<()>();

    let started = Instant::now();
    capture.poll(&mut app, true, move || {
      // A wedged audio system: answers only once the test releases it.
      release_rx.recv_timeout(Duration::from_secs(5)).ok()
    });
    let poll_took = started.elapsed();
    // Unblocks the open thread; fails harmlessly if it already gave up.
    let _ = release_tx.send(());

    assert!(
      poll_took < Duration::from_secs(1),
      "poll blocked for {poll_took:?}"
    );
    assert!(capture.is_opening());
  }

  #[test]
  fn a_new_open_waits_until_an_abandoned_one_answers() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let (release_tx, release_rx) = channel::<()>();
    let hung = Arc::clone(&attempts);
    capture.poll(&mut app, false, move || {
      hung.fetch_add(1, Ordering::SeqCst);
      release_rx.recv_timeout(Duration::from_secs(5)).ok()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while attempts.load(Ordering::SeqCst) == 0 {
      assert!(Instant::now() < deadline, "the open never started");
      thread::sleep(Duration::from_millis(1));
    }

    assert!(!capture.close());
    for _ in 0..3 {
      assert!(capture
        .poll(&mut app, false, opener(&attempts, Some(())))
        .is_none());
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 1);

    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while capture.opening.load(Ordering::Acquire) {
      assert!(Instant::now() < deadline, "the abandoned open never ended");
      thread::sleep(Duration::from_millis(1));
    }
    settle(&mut capture, &mut app, &attempts, Some(()));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(capture.current().is_some());
  }

  #[test]
  fn an_open_that_panics_does_not_block_the_next_visit() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let deadline = Instant::now() + Duration::from_secs(5);

    capture.poll(&mut app, false, || -> Option<()> {
      panic!("the audio system broke");
    });
    while capture.is_opening() {
      assert!(Instant::now() < deadline, "the open never answered");
      thread::sleep(Duration::from_millis(1));
      capture.poll(&mut app, false, opener(&attempts, Some(())));
    }
    capture.close();
    settle(&mut capture, &mut app, &attempts, Some(()));

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(capture.current().is_some());
  }
}
