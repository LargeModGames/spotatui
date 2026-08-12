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
    self.friend_add_dialog_visible = false;
    self.friend_add_mode = FriendAddMode::Code;
    self.friend_add_input.clear();
    self.friend_user_search_input.clear();
    self.friend_user_search_results.clear();
    self.friend_user_search_selected = 0;
  }

  pub fn open_friend_add_dialog(&mut self) {
    self.clear_friend_add_dialog_state();
    self.friend_add_dialog_visible = true;
  }
}
