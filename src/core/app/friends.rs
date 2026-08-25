use super::*;

// ── Friends feature ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct FriendEntry {
  pub id: String,
  pub name: String,
  /// Lowercased `name`, precomputed once so the per-frame search filter
  /// doesn't allocate per friend per frame.
  pub name_lower: String,
  pub is_online: bool,
  pub now_playing: Option<FriendNowPlaying>,
  /// Total listening time in milliseconds (from spotatui.com)
  #[allow(dead_code)]
  pub listening_ms: u64,
  /// Total number of listens tracked on spotatui.com
  #[allow(dead_code)]
  pub total_listens: u64,
}

#[derive(Clone, Debug)]
pub struct FriendNowPlaying {
  pub title: String,
  pub artists: String,
}

/// A user returned from the username/code search.
#[derive(Clone, Debug)]
pub struct FriendSearchResult {
  pub id: String,
  pub name: String,
  pub is_following: bool,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FriendFilter {
  #[default]
  All,
  Online,
}

/// Which tab is active in the "Add Friend" dialog.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FriendAddMode {
  #[default]
  Code,
  Search,
}

impl App {
  pub fn clear_friend_add_dialog_state(&mut self) {
    self.view.friend_add_dialog_visible = false;
    self.view.friend_add_mode = FriendAddMode::Code;
    self.view.friend_add_input.clear();
    self.view.friend_user_search_input.clear();
    self.friend_user_search_results.clear();
    self.view.friend_user_search_selected = 0;
  }

  pub fn open_friend_add_dialog(&mut self) {
    self.clear_friend_add_dialog_state();
    self.view.friend_add_dialog_visible = true;
  }

  pub fn copy_friend_code(&mut self) {
    let Some(code) = self.friend_code.clone() else {
      self.set_status_message("Friend code not loaded yet", 3);
      return;
    };

    let Some(clipboard) = &mut self.clipboard else {
      self.set_status_message("Clipboard not available", 3);
      return;
    };

    if clipboard.set_text(code.clone()).is_ok() {
      self.set_status_message(format!("Copied friend code: {}", code), 3);
    } else {
      self.set_status_message("Failed to copy to clipboard", 3);
    }
  }

  /// A query under the server's two-byte minimum clears the stale results
  /// instead of asking.
  pub(crate) fn search_friend_users(&mut self, query: String) {
    if query.len() >= 2 {
      self.dispatch(IoEvent::SearchFriendUsers(query));
    } else {
      self.friend_user_search_results.clear();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn copy_friend_code_without_a_code_reports_not_loaded() {
    let mut app = App::default();

    app.copy_friend_code();

    assert_eq!(
      app.status_message.as_deref(),
      Some("Friend code not loaded yet")
    );
  }
}
