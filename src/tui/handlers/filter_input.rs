//! Key handling shared by the inline filter inputs (Help rows, Settings rows).
//!
//! Both screens open a filter with `keys.search` and then edit it with the same
//! keys, so the buffer edits live here once. What a screen does *around* the
//! edit - which rows to recompute, where to move the selection - stays in that
//! screen's handler, which is why this returns an outcome instead of touching
//! `App`.

use crate::tui::event::Key;

/// What one key press did to a filter buffer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum FilterEdit {
  /// Esc: the screen should abandon the filter.
  Cancel,
  /// Enter: the screen should keep the filter but stop editing it.
  Confirm,
  /// The buffer changed; the screen should refresh its filtered view.
  Changed,
  /// Not a key this input consumes.
  Ignored,
}

/// Apply `key` to `filter` and report what the screen has to do next.
pub(super) fn apply(key: Key, filter: &mut String) -> FilterEdit {
  match key {
    Key::Esc => FilterEdit::Cancel,
    Key::Enter => FilterEdit::Confirm,
    Key::Backspace | Key::Ctrl('h') => {
      filter.pop();
      FilterEdit::Changed
    }
    Key::Ctrl('u') | Key::Ctrl('l') => {
      filter.clear();
      FilterEdit::Changed
    }
    Key::Ctrl('w') => {
      delete_last_word(filter);
      FilterEdit::Changed
    }
    Key::Char(c) => {
      filter.push(c);
      FilterEdit::Changed
    }
    _ => FilterEdit::Ignored,
  }
}

fn delete_last_word(filter: &mut String) {
  while filter.ends_with(char::is_whitespace) {
    filter.pop();
  }
  while filter
    .chars()
    .next_back()
    .is_some_and(|c| !c.is_whitespace())
  {
    filter.pop();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn after(start: &str, keys: &[Key]) -> String {
    let mut filter = start.to_string();
    for key in keys {
      apply(*key, &mut filter);
    }
    filter
  }

  #[test]
  fn editing_keys_change_the_buffer() {
    assert_eq!(after("", &[Key::Char('v'), Key::Char('o')]), "vo");
    assert_eq!(after("vol", &[Key::Backspace]), "vo");
    assert_eq!(after("vol", &[Key::Ctrl('h')]), "vo");
    assert_eq!(after("", &[Key::Backspace]), "");
    assert_eq!(after("volume", &[Key::Ctrl('u')]), "");
    assert_eq!(after("volume", &[Key::Ctrl('l')]), "");
    // Ctrl+W drops the trailing word along with the whitespace after it.
    assert_eq!(after("volume inc", &[Key::Ctrl('w')]), "volume ");
    assert_eq!(after("volume ", &[Key::Ctrl('w')]), "");
  }

  #[test]
  fn esc_enter_and_unrelated_keys_report_without_touching_the_buffer() {
    let mut filter = String::from("volume");
    assert_eq!(apply(Key::Esc, &mut filter), FilterEdit::Cancel);
    assert_eq!(apply(Key::Enter, &mut filter), FilterEdit::Confirm);
    assert_eq!(apply(Key::Tab, &mut filter), FilterEdit::Ignored);
    assert_eq!(filter, "volume");
  }
}
