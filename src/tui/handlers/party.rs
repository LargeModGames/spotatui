use crate::core::action::Action;
use crate::core::app::App;
use crate::infra::network::sync::PartyStatus;
use crate::tui::event::Key;

const PARTY_CODE_LEN: usize = 6;
const PARTY_NAME_MAX_LEN: usize = 32;

pub fn handler(key: Key, app: &mut App) {
  match app.party_status {
    PartyStatus::Disconnected | PartyStatus::Connecting => {
      handle_disconnected_menu(key, app);
    }
    PartyStatus::Hosting => {
      handle_hosting_menu(key, app);
    }
    PartyStatus::Joined => {
      handle_joined_menu(key, app);
    }
  }
}

fn handle_disconnected_menu(key: Key, app: &mut App) {
  if app.view.party_input.is_empty()
    && app.view.party_join_name.is_empty()
    && !app.party_status.eq(&PartyStatus::Connecting)
  {
    match key {
      Key::Esc => {
        app.pop_navigation_stack();
      }
      Key::Char('1') | Key::Char('h') => {
        app.apply(Action::StartParty);
      }
      Key::Char('2') | Key::Char('j') | Key::Char('J') => {
        // Switch to "Enter code" view (one space so the code-entry UI is shown).
        app.view.party_input = vec![' '];
        app.view.party_input_idx = 0;
        app.view.party_join_name.clear();
      }
      Key::Enter => {
        app.apply(Action::StartParty);
      }
      _ => {}
    }
  } else {
    handle_code_input(key, app);
  }
}

fn code_alphanumeric_len(party_input: &[char]) -> usize {
  party_input.iter().filter(|c| c.is_alphanumeric()).count()
}

fn normalized_guest_name(guest_name: &[char]) -> String {
  guest_name.iter().collect::<String>().trim().to_string()
}

fn handle_code_input(key: Key, app: &mut App) {
  match key {
    Key::Esc => {
      app.view.party_input.clear();
      app.view.party_input_idx = 0;
      app.view.party_join_name.clear();
    }
    Key::Enter => {
      let code: String = app
        .view
        .party_input
        .iter()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
      let name = normalized_guest_name(&app.view.party_join_name);
      if code.len() == PARTY_CODE_LEN && !name.is_empty() {
        app.apply(Action::JoinParty { code, name });
      }
    }
    Key::Backspace => {
      if !app.view.party_join_name.is_empty() {
        app.view.party_join_name.pop();
      } else if app.view.party_input_idx > 0 {
        app.view.party_input_idx -= 1;
        app.view.party_input.remove(app.view.party_input_idx);
      }
    }
    Key::Char(c) => {
      if c.is_alphanumeric() && code_alphanumeric_len(&app.view.party_input) < PARTY_CODE_LEN {
        app
          .view
          .party_input
          .insert(app.view.party_input_idx, c.to_ascii_uppercase());
        app.view.party_input_idx += 1;
      } else if code_alphanumeric_len(&app.view.party_input) == PARTY_CODE_LEN
        && (c.is_ascii_graphic() || c == ' ')
        && app.view.party_join_name.len() < PARTY_NAME_MAX_LEN
      {
        app.view.party_join_name.push(c);
      }
    }
    _ => {}
  }
}

fn handle_hosting_menu(key: Key, app: &mut App) {
  match key {
    Key::Esc => {
      app.pop_navigation_stack();
    }
    Key::Char('l') | Key::Char('L') => {
      app.apply(Action::LeaveParty);
    }
    Key::Char('c') | Key::Char('C') => {
      app.apply(Action::TogglePartyControlMode);
    }
    _ => {}
  }
}

fn handle_joined_menu(key: Key, app: &mut App) {
  match key {
    Key::Esc => {
      app.pop_navigation_stack();
    }
    Key::Char('l') | Key::Char('L') => {
      app.apply(Action::LeaveParty);
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};

  /// Open the code-entry view and type `keys` into it, all through key presses.
  fn app_typing(keys: &str) -> App {
    let mut app = App::default();
    handler(Key::Char('2'), &mut app);
    for c in keys.chars() {
      handler(Key::Char(c), &mut app);
    }
    app
  }

  #[test]
  fn party_code_entry_uppercases_each_typed_character() {
    let app = app_typing("abcdef");

    // The trailing space is the sentinel that makes the popup render the code form.
    assert_eq!(app.view.party_input.iter().collect::<String>(), "ABCDEF ");
    assert_eq!(app.view.party_input_idx, 6);
    assert!(app.view.party_join_name.is_empty());
  }

  #[test]
  fn party_name_entry_starts_once_the_code_is_complete() {
    let app = app_typing("abcdefGuest");

    assert_eq!(app.view.party_input.iter().collect::<String>(), "ABCDEF ");
    assert_eq!(app.view.party_join_name.iter().collect::<String>(), "Guest");
  }

  #[test]
  fn party_backspace_deletes_the_name_before_the_code() {
    let mut app = app_typing("abcdefX");

    handler(Key::Backspace, &mut app);
    assert!(app.view.party_join_name.is_empty());
    assert_eq!(app.view.party_input.iter().collect::<String>(), "ABCDEF ");

    handler(Key::Backspace, &mut app);
    assert_eq!(app.view.party_input.iter().collect::<String>(), "ABCDE ");
  }

  #[test]
  fn party_enter_with_an_incomplete_code_keeps_the_buffers() {
    let mut app = app_typing("abcde");

    handler(Key::Enter, &mut app);

    assert_eq!(app.view.party_input.iter().collect::<String>(), "ABCDE ");
    assert_eq!(app.view.party_input_idx, 5);
  }

  #[test]
  fn esc_in_code_entry_clears_the_buffers_without_leaving_the_popup() {
    let mut app = App::default();
    app.push_navigation_stack(RouteId::Party, ActiveBlock::Party);
    handler(Key::Char('2'), &mut app);
    handler(Key::Char('a'), &mut app);

    handler(Key::Esc, &mut app);

    assert!(app.view.party_input.is_empty());
    assert_eq!(app.view.party_input_idx, 0);
    assert!(app.view.party_join_name.is_empty());
    assert_eq!(app.get_current_route().id, RouteId::Party);
  }
}
