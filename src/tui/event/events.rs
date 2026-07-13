use super::key::Key;
use crate::core::user_config::{normalize_tick_rate_milliseconds, DEFAULT_TICK_RATE_MILLISECONDS};
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind, MouseEvent, MouseEventKind};
use std::{
  sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc,
  },
  thread,
  time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
/// Configuration for event handling.
pub struct EventConfig {
  /// The key that is used to exit the application.
  #[allow(dead_code)]
  pub exit_key: Key,
  /// The tick rate at which the application will sent an tick event.
  pub tick_rate: Duration,
}

impl Default for EventConfig {
  fn default() -> EventConfig {
    EventConfig {
      exit_key: Key::Ctrl('c'),
      tick_rate: Duration::from_millis(DEFAULT_TICK_RATE_MILLISECONDS),
    }
  }
}

/// An occurred event.
pub enum Event {
  /// An input event occurred.
  Input(Key),
  /// A mouse event occurred.
  Mouse(MouseEvent),
  /// A tick event occurred.
  Tick(Duration),
}

/// A small event handler that wrap crossterm input and tick event. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct Events {
  rx: mpsc::Receiver<Event>,
  tick_rate_milliseconds: Arc<AtomicU64>,
  // Need to be kept around to prevent disposing the sender side.
  _tx: mpsc::Sender<Event>,
}

impl Events {
  /// Constructs an new instance of `Events` with the default config.
  pub fn new(tick_rate: u64) -> Events {
    Events::with_config(EventConfig {
      tick_rate: Duration::from_millis(tick_rate),
      ..Default::default()
    })
  }

  /// Constructs an new instance of `Events` from given config.
  pub fn with_config(config: EventConfig) -> Events {
    let (tx, rx) = mpsc::channel();
    let tick_rate_milliseconds = Arc::new(AtomicU64::new(normalize_tick_rate_milliseconds(
      config.tick_rate.as_millis().try_into().unwrap_or(i64::MAX),
    )));

    let event_tx = tx.clone();
    let event_tick_rate_milliseconds = tick_rate_milliseconds.clone();
    thread::spawn(move || {
      let mut last_tick = Instant::now();
      loop {
        let tick_rate = Duration::from_millis(event_tick_rate_milliseconds.load(Ordering::Relaxed));

        // Poll only until the next tick is due, so input events don't push the
        // tick schedule back.
        let poll_timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(poll_timeout).unwrap() {
          match event::read().unwrap() {
            // Only process key press events, not release or repeat.
            // This fixes duplicate key events on Windows where both
            // Press and Release events are sent for each key press.
            CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
              let key = Key::from(key);
              // If send fails, the receiver has been dropped (app is closing)
              if event_tx.send(Event::Input(key)).is_err() {
                break;
              }
            }
            CrosstermEvent::Mouse(mouse)
              if matches!(
                mouse.kind,
                MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
              ) && event_tx.send(Event::Mouse(mouse)).is_err() =>
            {
              break;
            }
            _ => {}
          }
        }

        // Only send a tick when one is actually due. Ticks used to be sent
        // unconditionally here, so every keypress enqueued an extra Tick right
        // after its Input, and the main loop drew two full frames per
        // keystroke. Input events keep their own immediate redraw; the tick
        // cadence stays fixed at tick_rate regardless of input.
        let elapsed = last_tick.elapsed();
        if elapsed >= tick_rate {
          last_tick = Instant::now();
          // If send fails, the receiver has been dropped (app is closing)
          if event_tx.send(Event::Tick(elapsed)).is_err() {
            break;
          }
        }
      }
    });

    Events {
      rx,
      tick_rate_milliseconds,
      _tx: tx,
    }
  }

  pub fn set_tick_rate(&self, tick_rate: u64) {
    let tick_rate = normalize_tick_rate_milliseconds(tick_rate as i64);
    if self.tick_rate_milliseconds.load(Ordering::Relaxed) != tick_rate {
      self
        .tick_rate_milliseconds
        .store(tick_rate, Ordering::Relaxed);
    }
  }

  /// Attempts to read an event.
  /// This function will block the current thread.
  pub fn next(&self) -> Result<Event, mpsc::RecvError> {
    self.rx.recv()
  }
}
