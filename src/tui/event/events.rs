use super::key::Key;
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind, MouseEvent, MouseEventKind};
use std::{sync::mpsc, thread, time::Duration};

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
      tick_rate: Duration::from_millis(250),
    }
  }
}

/// An occurred event.
pub enum Event {
  /// An input event occurred.
  Input(Key),
  /// A mouse event occurred.
  Mouse(MouseEvent),
  /// An tick event occurred.
  Tick,
}

/// A small event handler that wrap crossterm input and tick event. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct Events {
  rx: mpsc::Receiver<Event>,
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

    let event_tx = tx.clone();
    thread::spawn(move || {
      loop {
        // poll for tick rate duration, if no event, sent tick event.
        if event::poll(config.tick_rate).unwrap() {
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

        // If send fails, the receiver has been dropped (app is closing)
        if event_tx.send(Event::Tick).is_err() {
          break;
        }
      }
    });

    Events { rx, _tx: tx }
  }

  /// Attempts to read an event.
  /// This function will block the current thread.
  pub fn next(&self) -> Result<Event, mpsc::RecvError> {
    self.rx.recv()
  }
}
