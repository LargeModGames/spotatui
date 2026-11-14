//! Crossterm → [`Key`] conversion. The `Key` enum itself is frontend-neutral
//! and lives in `core/input.rs`; only this terminal-event mapping belongs to
//! the TUI.

use crossterm::event;

use crate::core::input::Arrow;
pub use crate::core::input::Key;

impl From<event::KeyEvent> for Key {
  fn from(key_event: event::KeyEvent) -> Self {
    if let Some(key) = alt_arrow(&key_event) {
      return key;
    }
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
        modifiers,
        ..
      } if modifiers == event::KeyModifiers::ALT | event::KeyModifiers::SHIFT => {
        Key::Alt(shifted_char(c, cfg!(windows)))
      }
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

/// An arrow with Alt, or with Alt and Shift; any other modifier leaves it a plain arrow.
fn alt_arrow(key_event: &event::KeyEvent) -> Option<Key> {
  let arrow = match key_event.code {
    event::KeyCode::Left => Arrow::Left,
    event::KeyCode::Right => Arrow::Right,
    event::KeyCode::Up => Arrow::Up,
    event::KeyCode::Down => Arrow::Down,
    _ => return None,
  };
  let modifiers = key_event.modifiers;
  if modifiers.difference(event::KeyModifiers::SHIFT) != event::KeyModifiers::ALT {
    return None;
  }
  Some(if modifiers.contains(event::KeyModifiers::SHIFT) {
    Key::AltShiftArrow(arrow)
  } else {
    Key::AltArrow(arrow)
  })
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
  fn alt_arrows_keep_their_modifiers_and_ctrl_alt_arrows_stay_plain() {
    let arrow = |modifiers| Key::from(event::KeyEvent::new(event::KeyCode::Left, modifiers));

    assert_eq!(arrow(event::KeyModifiers::ALT), Key::AltArrow(Arrow::Left));
    assert_eq!(
      arrow(event::KeyModifiers::ALT | event::KeyModifiers::SHIFT),
      Key::AltShiftArrow(Arrow::Left)
    );
    assert_eq!(
      arrow(event::KeyModifiers::ALT | event::KeyModifiers::CONTROL),
      Key::Left
    );
  }

  #[test]
  fn alt_shift_letter_is_an_uppercase_alt_key() {
    let event = event::KeyEvent::new(
      event::KeyCode::Char('H'),
      event::KeyModifiers::ALT | event::KeyModifiers::SHIFT,
    );

    assert_eq!(Key::from(event), Key::Alt('H'));
  }

  #[test]
  fn shifted_char_preserves_already_uppercase_input() {
    assert_eq!(shifted_char('Ö', false), 'Ö');
  }
}
