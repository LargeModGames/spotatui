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
      // A letter whose uppercase is more than one char (`ß` → `SS`) stays as is.
      event::KeyEvent {
        code: event::KeyCode::Char(c),
        modifiers: event::KeyModifiers::SHIFT,
        ..
      } if c.is_lowercase() => Key::Char(single_char_uppercase(c).unwrap_or(c)),

      event::KeyEvent {
        code: event::KeyCode::Char(c),
        ..
      } => Key::Char(c),

      _ => Key::Unknown,
    }
  }
}

/// The uppercase form of `c` when it is exactly one char, `None` otherwise.
fn single_char_uppercase(c: char) -> Option<char> {
  let mut upper = c.to_uppercase();
  match (upper.next(), upper.next()) {
    (Some(u), None) => Some(u),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

  fn shifted(c: char) -> Key {
    Key::from(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT))
  }

  #[test]
  fn shift_with_a_lowercase_non_ascii_letter_gives_its_uppercase_char() {
    assert_eq!(shifted('ö'), Key::Char('Ö'));
    assert_eq!(shifted('é'), Key::Char('É'));
  }

  #[test]
  fn shift_with_eszett_stays_eszett_because_its_uppercase_is_two_chars() {
    assert_eq!(shifted('ß'), Key::Char('ß'));
  }

  #[test]
  fn shift_with_an_ascii_letter_still_gives_its_uppercase_char() {
    assert_eq!(shifted('p'), Key::Char('P'));
  }

  #[test]
  fn shift_with_an_uppercase_letter_leaves_it_unchanged() {
    assert_eq!(shifted('Ö'), Key::Char('Ö'));
  }
}
