//! Frontend-neutral key representation.
//!
//! `Key` is the domain vocabulary for keybindings: config parsing
//! (`core/user_config.rs`), `App` state, and every handler speak this type.
//! The conversion from crossterm's `KeyEvent` lives with the terminal
//! frontend in `tui/event/key.rs`, so nothing here depends on a terminal.

use std::fmt;

/// Represents an key.
#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Key {
  /// Both Enter (or Return) and numpad Enter
  Enter,
  /// Tabulation key
  Tab,
  /// Backspace key
  Backspace,
  /// Escape key
  Esc,

  /// Left arrow
  Left,
  /// Right arrow
  Right,
  /// Up arrow
  Up,
  /// Down arrow
  Down,

  /// Insert key
  Ins,
  /// Delete key
  Delete,
  /// Home key
  Home,
  /// End key
  End,
  /// Page Up key
  PageUp,
  /// Page Down key
  PageDown,

  /// F0 key
  F0,
  /// F1 key
  F1,
  /// F2 key
  F2,
  /// F3 key
  F3,
  /// F4 key
  F4,
  /// F5 key
  F5,
  /// F6 key
  F6,
  /// F7 key
  F7,
  /// F8 key
  F8,
  /// F9 key
  F9,
  /// F10 key
  F10,
  /// F11 key
  F11,
  /// F12 key
  F12,
  Char(char),
  Ctrl(char),
  Alt(char),
  Unknown,
}

impl Key {
  /// Returns the function key corresponding to the given number
  ///
  /// 1 -> F1, etc... Unmapped keys (F13+) return `Unknown`.
  pub fn from_f(n: u8) -> Key {
    match n {
      0 => Key::F0,
      1 => Key::F1,
      2 => Key::F2,
      3 => Key::F3,
      4 => Key::F4,
      5 => Key::F5,
      6 => Key::F6,
      7 => Key::F7,
      8 => Key::F8,
      9 => Key::F9,
      10 => Key::F10,
      11 => Key::F11,
      12 => Key::F12,
      _ => Key::Unknown,
    }
  }
}

impl fmt::Display for Key {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match *self {
      Key::Alt(' ') => write!(f, "<Alt+Space>"),
      Key::Ctrl(' ') => write!(f, "<Ctrl+Space>"),
      Key::Char(' ') => write!(f, "<Space>"),
      Key::Alt(c) => write!(f, "<Alt+{}>", c),
      Key::Ctrl(c) => write!(f, "<Ctrl+{}>", c),
      Key::Char(c) => write!(f, "{}", c),
      Key::Left | Key::Right | Key::Up | Key::Down => write!(f, "<{:?} Arrow Key>", self),
      Key::Enter
      | Key::Tab
      | Key::Backspace
      | Key::Esc
      | Key::Ins
      | Key::Delete
      | Key::Home
      | Key::End
      | Key::PageUp
      | Key::PageDown => write!(f, "<{:?}>", self),
      _ => write!(f, "{:?}", self),
    }
  }
}
