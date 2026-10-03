//! The visualizer's audio capture, opened off the tick. Opening one waits on
//! the audio system (PipeWire's setup handshake, cpal's device scan), and the
//! tick runs under the `App` lock every frame, so the open runs on its own
//! thread and the tick only polls for the result. Backend-neutral: the
//! PipeWire and cpal capture managers both come through here.

use crate::core::app::App;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

/// Shown once per visit to the visualizer when the capture cannot open.
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

/// One capture per visit to the visualizer: opened once, in the background,
/// and dropped when the visualizer closes.
pub(super) struct VizCapture<C> {
  slot: Slot<C>,
}

impl<C: Send + 'static> VizCapture<C> {
  pub(super) fn new() -> Self {
    Self { slot: Slot::Closed }
  }

  /// One tick while the visualizer shows; never blocks. Starts `open` on a
  /// background thread if nothing was tried yet this visit, installs its
  /// result once it arrives, and keeps offering the failure message until
  /// the status bar takes it (a live error status drops plain messages).
  pub(super) fn poll(
    &mut self,
    app: &mut App,
    open: impl FnOnce() -> Option<C> + Send + 'static,
  ) -> Option<&C> {
    if matches!(self.slot, Slot::Closed) {
      let (tx, rx) = mpsc::sync_channel(1);
      let spawned = thread::Builder::new()
        .name("audio-viz-open".into())
        .spawn(move || {
          // The receiver is gone if the visualizer closed meanwhile; the
          // capture is then dropped here.
          let _ = tx.send(open());
        });
      self.slot = match spawned {
        Ok(_) => Slot::Opening(rx),
        Err(e) => {
          log::error!("[audio-viz] could not start the capture thread: {e}");
          Slot::Failed { reported: false }
        }
      };
    }

    if let Slot::Opening(rx) = &self.slot {
      match rx.try_recv() {
        Ok(Some(capture)) => self.slot = Slot::Open(capture),
        Ok(None) | Err(TryRecvError::Disconnected) => self.slot = Slot::Failed { reported: false },
        Err(TryRecvError::Empty) => {}
      }
    }

    if let Slot::Failed { reported: false } = self.slot {
      app.set_status_message(
        AUDIO_CAPTURE_UNAVAILABLE,
        AUDIO_CAPTURE_UNAVAILABLE_TTL_SECS,
      );
      let shown =
        app.status_message() == Some(AUDIO_CAPTURE_UNAVAILABLE) && !app.status_message_is_error();
      self.slot = Slot::Failed { reported: shown };
    }

    match &self.slot {
      Slot::Open(capture) => Some(capture),
      Slot::Closed | Slot::Opening(_) | Slot::Failed { .. } => None,
    }
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
    capture.poll(app, opener(attempts, result));
    while capture.is_opening() {
      assert!(Instant::now() < deadline, "the open never answered");
      thread::sleep(Duration::from_millis(1));
      capture.poll(app, opener(attempts, result));
    }
  }

  #[test]
  fn a_capture_that_fails_to_open_is_reported_once_and_not_retried() {
    let mut app = app();
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));

    settle(&mut capture, &mut app, &attempts, None);
    for _ in 0..3 {
      assert!(capture.poll(&mut app, opener(&attempts, None)).is_none());
    }

    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(app.status_message(), Some(AUDIO_CAPTURE_UNAVAILABLE));
    assert!(!app.status_message_is_error());
  }

  #[test]
  fn the_failure_message_waits_for_a_live_error_to_clear() {
    let mut app = app();
    app.set_error_status_message("Something else failed", 60);
    let mut capture = VizCapture::new();
    let attempts = Arc::new(AtomicUsize::new(0));

    settle(&mut capture, &mut app, &attempts, None);
    capture.poll(&mut app, opener(&attempts, None));
    assert_eq!(app.status_message(), Some("Something else failed"));

    // Stands in for the error's lifetime running out.
    app.clear_status_message();
    capture.poll(&mut app, opener(&attempts, None));

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
        .poll(&mut app, opener(&attempts, Some(())))
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
    capture.poll(&mut app, move || {
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
}
