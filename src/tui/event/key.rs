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

      event::KeyEvent {
        code: event::KeyCode::Char(c),
        modifiers: event::KeyModifiers::SHIFT,
        ..
      } => Key::Char(shifted_char(c, cfg!(windows))),

      event::KeyEvent {
        code: event::KeyCode::Char(c),
        ..
      } => Key::Char(c),

      _ => Key::Unknown,
    }
  }
}

/// Apply Shift casing unless the input source has already done so.
fn shifted_char(c: char, shift_already_applied: bool) -> char {
  if shift_already_applied || !c.is_lowercase() {
    return c;
  }

  let mut upper = c.to_uppercase();
  match (upper.next(), upper.next()) {
    (Some(u), None) => u,
    _ => c,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn shifted_char_uppercases_lowercase_when_shift_is_not_applied() {
    assert_eq!(shifted_char('p', false), 'P');
    assert_eq!(shifted_char('ö', false), 'Ö');
    assert_eq!(shifted_char('é', false), 'É');
  }

  #[test]
  fn shifted_char_preserves_multi_char_uppercase() {
    assert_eq!(shifted_char('ß', false), 'ß');
  }

  #[test]
  fn shifted_char_preserves_console_resolved_casing() {
    assert_eq!(shifted_char('a', true), 'a');
    assert_eq!(shifted_char('ö', true), 'ö');
    assert_eq!(shifted_char('A', true), 'A');
  }

  #[test]
  fn shifted_char_preserves_already_uppercase_input() {
    assert_eq!(shifted_char('Ö', false), 'Ö');
  }
}
