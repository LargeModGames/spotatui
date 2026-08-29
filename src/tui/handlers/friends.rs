use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::{App, FriendAddMode, FriendFilter};
use crate::tui::event::Key;
use crate::tui::ui::friends::filtered_friends;

pub fn handler(key: Key, app: &mut App) {
  // When the add-friend dialog is open, route all keys there.
  if app.view.friend_add_dialog_visible {
    handle_add_dialog(key, app);
    return;
  }

  // When the search input has focus (non-empty), handle character input inline.
  if !app.view.friend_search_input.is_empty() {
    match key {
      Key::Esc => {
        // Clear search and return focus to the list
        app.view.friend_search_input.clear();
        return;
      }
      Key::Backspace => {
        app.view.friend_search_input.pop();
        // Reset selected index when the list changes
        app.view.friend_selected_index = 0;
        return;
      }
      Key::Char(c) if c != '\n' => {
        app.view.friend_search_input.push(c);
        app.view.friend_selected_index = 0;
        return;
      }
      _ => {}
    }
  }

  match key {
    // Navigation
    k if common_key_events::down_event(k, &app.user_config.keys) => move_down(app),
    k if common_key_events::up_event(k, &app.user_config.keys) => move_up(app),
    k if common_key_events::high_event(k) => app.view.friend_selected_index = 0,
    k if common_key_events::low_event(k) => {
      let count = filtered_count(app);
      if count > 0 {
        app.view.friend_selected_index = count - 1;
      }
    }

    // Copy own friend code to clipboard
    Key::Char('c') => app.copy_friend_code(),

    // Open add-friend dialog
    Key::Char('a') => app.open_friend_add_dialog(),

    // Unfollow selected friend (no confirm for now — status message acts as feedback)
    Key::Char('u') => unfollow_selected(app),

    // Tab: cycle between All / Online filter
    Key::Tab => {
      app.view.friend_filter = match app.view.friend_filter {
        FriendFilter::All => FriendFilter::Online,
        FriendFilter::Online => FriendFilter::All,
      };
      app.view.friend_selected_index = 0;
    }

    // Type directly into search when idle (any unbound character filters the list)
    Key::Char(c) if c != '\n' => {
      app.view.friend_search_input.push(c);
      app.view.friend_selected_index = 0;
    }

    // Backspace clears last search character
    Key::Backspace if !app.view.friend_search_input.is_empty() => {
      app.view.friend_search_input.pop();
      app.view.friend_selected_index = 0;
    }

    // Esc: pop navigation (handled upstream, but guard in case)
    Key::Esc => {
      app.view.friend_search_input.clear();
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
      app.view.friend_add_mode = match app.view.friend_add_mode {
        FriendAddMode::Code => FriendAddMode::Search,
        FriendAddMode::Search => FriendAddMode::Code,
      };
    }

    // Submit
    Key::Enter => match app.view.friend_add_mode {
      FriendAddMode::Code => {
        let code: String = app.view.friend_add_input.iter().collect();
        let code = code.trim().to_string();
        if !code.is_empty() {
          app.apply(Action::AddFriendByCode(code));
          app.clear_friend_add_dialog_state();
        }
      }
      FriendAddMode::Search => {
        let idx = app.view.friend_user_search_selected;
        if let Some(result) = app.friend_user_search_results.get(idx) {
          let user_id = result.id.clone();
          app.apply(Action::AddFriendById(user_id));
          app.clear_friend_add_dialog_state();
        }
      }
    },

    Key::Backspace => match app.view.friend_add_mode {
      FriendAddMode::Code => {
        app.view.friend_add_input.pop();
      }
      FriendAddMode::Search => {
        app.view.friend_user_search_input.pop();
        let query: String = app.view.friend_user_search_input.iter().collect();
        app.apply(Action::SearchFriendUsers(query));
      }
    },

    // Navigate search results
    k if app.view.friend_add_mode == FriendAddMode::Search
      && common_key_events::down_event(k, &app.user_config.keys) =>
    {
      let count = app.friend_user_search_results.len();
      if count > 0 {
        app.view.friend_user_search_selected =
          (app.view.friend_user_search_selected + 1).min(count - 1);
      }
    }

    k if app.view.friend_add_mode == FriendAddMode::Search
      && common_key_events::up_event(k, &app.user_config.keys)
      && app.view.friend_user_search_selected > 0 =>
    {
      app.view.friend_user_search_selected -= 1;
    }

    Key::Char(c) if c != '\n' => match app.view.friend_add_mode {
      FriendAddMode::Code => {
        app.view.friend_add_input.push(c);
      }
      FriendAddMode::Search => {
        app.view.friend_user_search_input.push(c);
        let query: String = app.view.friend_user_search_input.iter().collect();
        app.apply(Action::SearchFriendUsers(query));
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
  app.view.friend_selected_index = (app.view.friend_selected_index + 1).min(count - 1);
}

fn move_up(app: &mut App) {
  if app.view.friend_selected_index > 0 {
    app.view.friend_selected_index -= 1;
  }
}

fn unfollow_selected(app: &mut App) {
  let user_id = filtered_friends(app)
    .get(app.view.friend_selected_index)
    .map(|friend| friend.id.clone());
  if let Some(user_id) = user_id {
    app.apply(Action::UnfollowFriend(user_id));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn app_on_the_friends_screen() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::Friends, ActiveBlock::Friends);
    (app, rx)
  }

  #[test]
  fn esc_first_clears_the_filter_then_leaves_the_screen() {
    let (mut app, _rx) = app_on_the_friends_screen();
    handler(Key::Char('z'), &mut app);
    assert_eq!(app.view.friend_search_input, vec!['z']);

    handler(Key::Esc, &mut app);

    assert!(app.view.friend_search_input.is_empty());
    assert_eq!(
      app.get_current_route().id,
      RouteId::Friends,
      "the first Esc only clears the filter"
    );

    handler(Key::Esc, &mut app);

    assert_eq!(app.get_current_route().id, RouteId::Home);
  }

  #[test]
  fn unfollow_without_a_selected_friend_does_nothing() {
    let (mut app, rx) = app_on_the_friends_screen();

    handler(Key::Char('u'), &mut app);

    assert!(app.friends.is_empty());
    assert_eq!(app.get_current_route().id, RouteId::Friends);
    assert!(rx.try_recv().is_err(), "expected nothing dispatched");
  }

  #[test]
  fn a_short_user_search_query_asks_for_nothing() {
    let (mut app, rx) = app_on_the_friends_screen();
    app.open_friend_add_dialog();
    handler(Key::Tab, &mut app);

    handler(Key::Char('a'), &mut app);

    assert_eq!(app.view.friend_user_search_input, vec!['a']);
    assert!(rx.try_recv().is_err(), "below the server's minimum");
  }
}
