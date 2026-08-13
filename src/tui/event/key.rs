//! Crossterm → [`Key`] conversion. The `Key` enum itself is frontend-neutral
//! and lives in `core/input.rs`; only this terminal-event mapping belongs to
//! the TUI.

use crossterm::event;

pub use crate::core::input::Key;

impl From<event::KeyEvent> for Key {
  fn from(key_event: event::KeyEvent) -> Self {
    match key_event {
      event::KeyEvent {
        code: event::KeyCode::Esc,
        ..
      } => Key::Esc,
      event::KeyEvent {
        code: event::KeyCode::Backspace,
        ..
      } => Key::Backspace,
      event::KeyEvent {
        code: event::KeyCode::Left,
        ..
      } => Key::Left,
      event::KeyEvent {
        code: event::KeyCode::Right,
        ..
      } => Key::Right,
      event::KeyEvent {
        code: event::KeyCode::Up,
        ..
      } => Key::Up,
      event::KeyEvent {
        code: event::KeyCode::Down,
        ..
      } => Key::Down,
      event::KeyEvent {
        code: event::KeyCode::Home,
        ..
      } => Key::Home,
      event::KeyEvent {
        code: event::KeyCode::End,
        ..
      } => Key::End,
      event::KeyEvent {
        code: event::KeyCode::PageUp,
        ..
      } => Key::PageUp,
      event::KeyEvent {
        code: event::KeyCode::PageDown,
        ..
      } => Key::PageDown,
      event::KeyEvent {
        code: event::KeyCode::Delete,
        ..
      } => Key::Delete,
      event::KeyEvent {
        code: event::KeyCode::Insert,
        ..
      } => Key::Ins,
      event::KeyEvent {
        code: event::KeyCode::F(n),
        ..
      } => Key::from_f(n),
      event::KeyEvent {
        code: event::KeyCode::Enter,
        ..
      } => Key::Enter,
      event::KeyEvent {
        code: event::KeyCode::Tab,
        ..
      } => Key::Tab,

      // First check for char + modifier
      event::KeyEvent {
        code: event::KeyCode::Char(c),
        modifiers: event::KeyModifiers::ALT,
        ..
      } => Key::Alt(c),
      event::KeyEvent {
        code: event::KeyCode::Char(c),
        modifiers: event::KeyModifiers::CONTROL,
        ..
      } => Key::Ctrl(c),

      // Terminals using the kitty keyboard protocol send Shift+letter as lowercase
      // char with SHIFT modifier; normalise to uppercase so Key::Char('P') works.
      event::KeyEvent {
        code: event::KeyCode::Char(c),
        modifiers: event::KeyModifiers::SHIFT,
        ..
      } if c.is_ascii_lowercase() => Key::Char(c.to_ascii_uppercase()),

      event::KeyEvent {
        code: event::KeyCode::Char(c),
        ..
      } => Key::Char(c),

      _ => Key::Unknown,
    }
  }
}
