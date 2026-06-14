use super::common_key_events;
use crate::core::app::{App, FriendAddMode, FriendFilter};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use crate::tui::ui::friends::filtered_friends;

pub fn handler(key: Key, app: &mut App) {
  // When the add-friend dialog is open, route all keys there.
  if app.friend_add_dialog_visible {
    handle_add_dialog(key, app);
    return;
  }

  // When the search input has focus (non-empty), handle character input inline.
  if !app.friend_search_input.is_empty() {
    match key {
      Key::Esc => {
        // Clear search and return focus to the list
        app.friend_search_input.clear();
        return;
      }
      Key::Backspace => {
        app.friend_search_input.pop();
        // Reset selected index when the list changes
        app.friend_selected_index = 0;
        return;
      }
      Key::Char(c) if c != '\n' => {
        app.friend_search_input.push(c);
        app.friend_selected_index = 0;
        return;
      }
      _ => {}
    }
  }

  match key {
    // Navigation
    k if common_key_events::down_event(k, &app.user_config.keys) => move_down(app),
    k if common_key_events::up_event(k, &app.user_config.keys) => move_up(app),
    k if common_key_events::high_event(k) => app.friend_selected_index = 0,
    k if common_key_events::low_event(k) => {
      let count = filtered_count(app);
      if count > 0 {
        app.friend_selected_index = count - 1;
      }
    }

    // Copy own friend code to clipboard
    Key::Char('c') => copy_friend_code(app),

    // Open add-friend dialog
    Key::Char('a') => app.open_friend_add_dialog(),

    // Unfollow selected friend (no confirm for now — status message acts as feedback)
    Key::Char('u') => unfollow_selected(app),

    // Tab: cycle between All / Online filter
    Key::Tab => {
      app.friend_filter = match app.friend_filter {
        FriendFilter::All => FriendFilter::Online,
        FriendFilter::Online => FriendFilter::All,
      };
      app.friend_selected_index = 0;
    }

    // Type directly into search when idle (any unbound character filters the list)
    Key::Char(c) if c != '\n' => {
      app.friend_search_input.push(c);
      app.friend_selected_index = 0;
    }

    // Backspace clears last search character
    Key::Backspace if !app.friend_search_input.is_empty() => {
      app.friend_search_input.pop();
      app.friend_selected_index = 0;
    }

    // Esc: pop navigation (handled upstream, but guard in case)
    Key::Esc => {
      app.friend_search_input.clear();
      app.pop_navigation_stack();
    }

    _ => {}
  }
}

// ── Add-friend dialog handler ─────────────────────────────────────────────────

fn handle_add_dialog(key: Key, app: &mut App) {
  match key {
    // Close dialog
    Key::Esc => close_dialog(app),

    // Switch between Code / Search tabs
    Key::Tab => {
      app.friend_add_mode = match app.friend_add_mode {
        FriendAddMode::Code => FriendAddMode::Search,
        FriendAddMode::Search => FriendAddMode::Code,
      };
    }

    // Submit
    Key::Enter => match app.friend_add_mode {
      FriendAddMode::Code => {
        let code: String = app.friend_add_input.iter().collect();
        let code = code.trim().to_string();
        if !code.is_empty() {
          app.dispatch(IoEvent::AddFriendByCode(code));
          app.clear_friend_add_dialog_state();
        }
      }
      FriendAddMode::Search => {
        let idx = app.friend_user_search_selected;
        if let Some(result) = app.friend_user_search_results.get(idx) {
          let user_id = result.id.clone();
          app.dispatch(IoEvent::AddFriendByUserId(user_id));
          app.clear_friend_add_dialog_state();
        }
      }
    },

    Key::Backspace => match app.friend_add_mode {
      FriendAddMode::Code => {
        app.friend_add_input.pop();
      }
      FriendAddMode::Search => {
        app.friend_user_search_input.pop();
        let query: String = app.friend_user_search_input.iter().collect();
        if query.len() >= 2 {
          app.dispatch(IoEvent::SearchFriendUsers(query));
        } else {
          app.friend_user_search_results.clear();
        }
      }
    },

    // Navigate search results
    k if app.friend_add_mode == FriendAddMode::Search
      && common_key_events::down_event(k, &app.user_config.keys) =>
    {
      let count = app.friend_user_search_results.len();
      if count > 0 {
        app.friend_user_search_selected = (app.friend_user_search_selected + 1).min(count - 1);
      }
    }

    k if app.friend_add_mode == FriendAddMode::Search
      && common_key_events::up_event(k, &app.user_config.keys)
      && app.friend_user_search_selected > 0 =>
    {
      app.friend_user_search_selected -= 1;
    }

    Key::Char(c) if c != '\n' => match app.friend_add_mode {
      FriendAddMode::Code => {
        app.friend_add_input.push(c);
      }
      FriendAddMode::Search => {
        app.friend_user_search_input.push(c);
        let query: String = app.friend_user_search_input.iter().collect();
        if query.len() >= 2 {
          app.dispatch(IoEvent::SearchFriendUsers(query));
        }
      }
    },

    _ => {}
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn close_dialog(app: &mut App) {
  app.clear_friend_add_dialog_state();
}

fn filtered_count(app: &App) -> usize {
  filtered_friends(app).len()
}

fn move_down(app: &mut App) {
  let count = filtered_count(app);
  if count == 0 {
    return;
  }
  app.friend_selected_index = (app.friend_selected_index + 1).min(count - 1);
}

fn move_up(app: &mut App) {
  if app.friend_selected_index > 0 {
    app.friend_selected_index -= 1;
  }
}

fn copy_friend_code(app: &mut App) {
  let Some(code) = app.friend_code.clone() else {
    app.set_status_message("Friend code not loaded yet", 3);
    return;
  };

  let Some(clipboard) = &mut app.clipboard else {
    app.set_status_message("Clipboard not available", 3);
    return;
  };

  if clipboard.set_text(code.clone()).is_ok() {
    app.set_status_message(format!("Copied friend code: {}", code), 3);
  } else {
    app.set_status_message("Failed to copy to clipboard", 3);
  }
}

fn unfollow_selected(app: &mut App) {
  let filtered = filtered_friends(app);
  if let Some(friend) = filtered.get(app.friend_selected_index) {
    let user_id = friend.id.clone();
    app.dispatch(IoEvent::UnfollowFriend(user_id));
  }
}
