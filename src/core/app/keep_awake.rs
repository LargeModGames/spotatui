use super::*;

/// The sleep inhibitor, held on its own thread so a D-Bus call that never returns cannot block the tick.
#[derive(Default)]
pub(super) struct KeepAwake {
  tx: Option<Sender<bool>>,
  requested: bool,
}

impl KeepAwake {
  pub(super) fn request(&mut self, on: bool) {
    self.request_with(on, || {
      keepawake::Builder::default()
        .idle(true)
        .sleep(true)
        .display(true)
        .reason("Playing music")
        .app_name("spotatui")
        .create()
        .ok()
    });
  }

  fn request_with<T: 'static>(
    &mut self,
    on: bool,
    create: impl Fn() -> Option<T> + Send + 'static,
  ) {
    if on == self.requested {
      return;
    }
    let sent = self.tx.as_ref().is_some_and(|tx| tx.send(on).is_ok()) || {
      // No worker yet, or a failed release panicked and ended it.
      let tx = spawn_worker(create);
      let sent = tx.send(on).is_ok();
      self.tx = Some(tx);
      sent
    };
    // A worker that could not start takes the next request again.
    if sent {
      self.requested = on;
    }
  }
}

/// Creating and dropping on one thread matters on Windows, where the request is per thread.
fn spawn_worker<T: 'static>(create: impl Fn() -> Option<T> + Send + 'static) -> Sender<bool> {
  let (tx, rx) = std::sync::mpsc::channel::<bool>();
  let spawned = std::thread::Builder::new()
    .name("keepawake".into())
    .spawn(move || {
      let mut held: Option<T> = None;
      while let Ok(mut want) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
          want = newer;
        }
        if !want {
          held = None;
        } else if held.is_none() {
          held = create();
        }
      }
    });
  if let Err(e) = spawned {
    log::warn!("keepawake: could not start the worker thread: {e}");
  }
  tx
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::sync::Arc;

  #[test]
  fn a_hung_inhibitor_does_not_block_the_request() {
    let (block_tx, block_rx) = std::sync::mpsc::channel::<()>();
    let mut keep = KeepAwake::default();
    let started = Instant::now();
    keep.request_with(true, move || {
      let _ = block_rx.recv();
      None::<()>
    });
    keep.request_with(false, || None::<()>);
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(block_tx);
  }

  #[test]
  fn a_failed_inhibitor_is_tried_once_per_play_edge() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let create = {
      let calls = Arc::clone(&calls);
      move || {
        calls.fetch_add(1, Ordering::SeqCst);
        let _ = done_tx.send(());
        None::<()>
      }
    };
    let mut keep = KeepAwake::default();
    keep.tx = Some(spawn_worker(create));
    let settled = |done_rx: &std::sync::mpsc::Receiver<()>| {
      done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
      // A second attempt would arrive well inside this wait.
      assert!(done_rx.recv_timeout(Duration::from_millis(200)).is_err());
    };
    for _ in 0..10 {
      keep.request_with(true, || None::<()>);
    }
    settled(&done_rx);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    keep.request_with(false, || None::<()>);
    keep.request_with(true, || None::<()>);
    settled(&done_rx);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
  }

  #[test]
  fn a_worker_that_ended_is_started_again() {
    let (dead_tx, dead_rx) = std::sync::mpsc::channel::<bool>();
    drop(dead_rx);
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let mut keep = KeepAwake::default();
    keep.tx = Some(dead_tx);

    keep.request_with(true, move || {
      let _ = done_tx.send(());
      None::<()>
    });

    done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
  }
}
