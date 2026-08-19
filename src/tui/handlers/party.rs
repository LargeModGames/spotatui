use crate::core::app::App;
use crate::infra::network::sync::{ControlMode, PartyStatus};
use crate::infra::network::IoEvent;
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
        app.dispatch(IoEvent::StartParty(ControlMode::HostOnly));
      }
      Key::Char('2') | Key::Char('j') | Key::Char('J') => {
        // Switch to "Enter code" view (one space so the code-entry UI is shown).
        app.view.party_input = vec![' '];
        app.view.party_input_idx = 0;
        app.view.party_join_name.clear();
      }
      Key::Enter => {
        app.dispatch(IoEvent::StartParty(ControlMode::HostOnly));
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
        app.dispatch(IoEvent::JoinParty { code, name });
        app.view.party_input.clear();
        app.view.party_input_idx = 0;
        app.view.party_join_name.clear();
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
      app.dispatch(IoEvent::LeaveParty);
    }
    Key::Char('c') | Key::Char('C') => {
      let new_mode = if let Some(session) = &mut app.party_session {
        let updated_mode = match session.control_mode {
          ControlMode::HostOnly => ControlMode::SharedControl,
          ControlMode::SharedControl => ControlMode::HostOnly,
        };
        session.control_mode = updated_mode.clone();
        Some(updated_mode)
      } else {
        None
      };

      if let Some(updated_mode) = new_mode {
        app.dispatch(IoEvent::SetPartyControlMode(updated_mode));
      }
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
      app.dispatch(IoEvent::LeaveParty);
    }
    _ => {}
  }
}
