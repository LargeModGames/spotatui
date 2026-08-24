use super::*;

/// The public "spotatui community" playlist pinned to the top of the Spotify
/// playlists sidebar.
pub const COMMUNITY_PLAYLIST_ID: &str = "0tjRxKAUgoz95pWeW17wYx";

/// A node in the playlist folder hierarchy from Spotify's rootlist
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlaylistFolderNodeType {
  Folder,
  Playlist,
}

/// A node in the playlist folder hierarchy from Spotify's rootlist
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PlaylistFolderNode {
  pub name: Option<String>,
  pub node_type: PlaylistFolderNodeType,
  pub uri: String,
  pub children: Vec<PlaylistFolderNode>,
}

/// A folder entry for navigation in the playlist panel
#[derive(Clone, Debug)]
pub struct PlaylistFolder {
  pub name: String,
  /// Folder ID this item is visible in (which folder "page" it appears on)
  pub current_id: usize,
  /// Folder ID this item navigates to when selected
  pub target_id: usize,
}

/// A flattened item for display in the playlist panel
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum PlaylistFolderItem {
  Folder(PlaylistFolder),
  Playlist {
    /// Index into app.all_playlists
    index: usize,
    /// Folder ID this playlist is visible in
    current_id: usize,
  },
  /// The pinned "spotatui community" playlist. Injected at display time only
  /// (never stored in `playlist_folder_items`); opens the community playlist
  /// when selected.
  CommunityPin,
}

impl App {
  pub fn is_playlist_item_visible_in_current_folder(&self, item: &PlaylistFolderItem) -> bool {
    match item {
      PlaylistFolderItem::Folder(f) => f.current_id == self.current_playlist_folder_id,
      PlaylistFolderItem::Playlist { current_id, .. } => {
        *current_id == self.current_playlist_folder_id
      }
      PlaylistFolderItem::CommunityPin => false,
    }
  }

  /// Whether the user already follows the community playlist (its id is present
  /// in the loaded playlists), in which case the pin is suppressed.
  pub fn follows_community_playlist(&self) -> bool {
    self
      .all_playlists
      .iter()
      .any(|p| p.id.as_deref() == Some(COMMUNITY_PLAYLIST_ID))
  }

  /// Whether the community-playlist pin should be shown as row 0 of the Spotify
  /// playlists sidebar.
  pub fn community_pin_visible(&self) -> bool {
    self.active_source == Source::Spotify
      && self.user_config.behavior.pin_community_playlist
      && self.current_playlist_folder_id == 0
      && !self.follows_community_playlist()
  }

  /// Get the number of items visible in the current folder level.
  pub fn get_playlist_display_count(&self) -> usize {
    self.get_playlist_display_items().len()
  }

  /// Get a visible item by display index in the current folder.
  pub fn get_playlist_display_item_at(&self, display_index: usize) -> Option<&PlaylistFolderItem> {
    self
      .get_playlist_display_items()
      .into_iter()
      .nth(display_index)
  }

  /// Get visible playlist items in the current folder (used by UI rendering).
  ///
  /// Single source of truth for the visible order: rendering and index-based
  /// selection (keyboard + mouse) both go through it, so they can never
  /// disagree. When `group_folders_first` is set, folders are hoisted to the
  /// top via a stable sort, preserving each group's relative order.
  pub fn get_playlist_display_items(&self) -> Vec<&PlaylistFolderItem> {
    let mut items: Vec<&PlaylistFolderItem> = self
      .playlist_folder_items
      .iter()
      .filter(|item| self.is_playlist_item_visible_in_current_folder(item))
      .collect();
    if self.user_config.behavior.group_folders_first {
      items.sort_by_key(|item| !matches!(item, PlaylistFolderItem::Folder(_)));
    }
    // Injected after sorting so the pin is always row 0, regardless of grouping.
    if self.community_pin_visible() {
      items.insert(0, &self.community_pin_item);
    }
    items
  }

  /// Scope the playlist sidebar to the rootlist folder `target_id`.
  pub(crate) fn open_playlist_folder(&mut self, target_id: usize) {
    self.current_playlist_folder_id = target_id;
  }

  /// Get the playlist for a PlaylistFolderItem::Playlist variant
  #[allow(dead_code)]
  pub fn get_playlist_for_item(&self, item: &PlaylistFolderItem) -> Option<&PlaylistInfo> {
    match item {
      PlaylistFolderItem::Playlist { index, .. } => self.all_playlists.get(*index),
      PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => None,
    }
  }

  /// Get the currently selected playlist id in the visible playlist list.
  #[allow(dead_code)]
  pub fn get_selected_playlist_id(&self) -> Option<String> {
    let selected_index = self.view.selected_playlist_index?;
    // Row 0 is the "+ Add Playlist" entry; display items start at row 1.
    let display_index = selected_index.checked_sub(1)?;
    match self.get_playlist_display_item_at(display_index) {
      Some(PlaylistFolderItem::Playlist { index, .. }) => {
        return self.all_playlists.get(*index).and_then(|p| p.id.clone());
      }
      // The pin is not a stored playlist; don't let the raw-page fallback below
      // return an unrelated playlist's id for it.
      Some(PlaylistFolderItem::CommunityPin) => return None,
      Some(PlaylistFolderItem::Folder(_)) | None => {}
    }

    // In the raw-page fallback the pin is also rendered (row 1, above the pages),
    // so a row past it maps to `items[display_index - 1]`. The CommunityPin guard
    // above means this only runs at display_index >= 1 when the pin is visible.
    let raw_index = if self.community_pin_visible() {
      display_index.checked_sub(1)?
    } else {
      display_index
    };
    self
      .playlists
      .as_ref()
      .and_then(|playlists| playlists.items.get(raw_index))
      .and_then(|playlist| playlist.id.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn group_folders_first_hoists_folders_stably_and_only_when_enabled() {
    fn folder(name: &str) -> PlaylistFolderItem {
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: name.to_string(),
        current_id: 0,
        target_id: 1,
      })
    }
    fn playlist(index: usize) -> PlaylistFolderItem {
      PlaylistFolderItem::Playlist {
        index,
        current_id: 0,
      }
    }
    // Interleaved: playlist, folder A, playlist, folder B (all at root level).
    let mut app = App {
      playlist_folder_items: vec![playlist(0), folder("A"), playlist(1), folder("B")],
      ..Default::default()
    };
    // Keep this test focused on folder hoisting; the community pin is exercised
    // separately.
    app.user_config.behavior.pin_community_playlist = false;

    // Off (default): order is untouched.
    app.user_config.behavior.group_folders_first = false;
    let names: Vec<&str> = app
      .get_playlist_display_items()
      .iter()
      .map(|i| match i {
        PlaylistFolderItem::Folder(f) => f.name.as_str(),
        PlaylistFolderItem::Playlist { .. } => "P",
        PlaylistFolderItem::CommunityPin => "C",
      })
      .collect();
    assert_eq!(names, vec!["P", "A", "P", "B"]);

    // On: folders float to the top; both groups keep their relative order.
    app.user_config.behavior.group_folders_first = true;
    let names: Vec<&str> = app
      .get_playlist_display_items()
      .iter()
      .map(|i| match i {
        PlaylistFolderItem::Folder(f) => f.name.as_str(),
        PlaylistFolderItem::Playlist { .. } => "P",
        PlaylistFolderItem::CommunityPin => "C",
      })
      .collect();
    assert_eq!(names, vec!["A", "B", "P", "P"]);
    // Selection index resolves against the same reordered list.
    assert!(matches!(
      app.get_playlist_display_item_at(0),
      Some(PlaylistFolderItem::Folder(_))
    ));
  }

  #[test]
  fn community_pin_is_first_display_item_when_visible() {
    let app = App::default();
    // Default Spotify source + default-on toggle + root folder + not following.
    // The pin is the first *display* item (sidebar row 1, below "+ Add Playlist").
    assert!(app.community_pin_visible());
    assert!(matches!(
      app.get_playlist_display_item_at(0),
      Some(PlaylistFolderItem::CommunityPin)
    ));
    assert_eq!(app.get_playlist_display_count(), 1);
  }

  #[test]
  fn community_pin_hidden_outside_root_folder() {
    let mut app = App::default();
    assert!(app.community_pin_visible());
    app.current_playlist_folder_id = 3;
    assert!(!app.community_pin_visible());
  }

  #[test]
  fn community_pin_hidden_when_toggle_off() {
    let mut app = App::default();
    app.user_config.behavior.pin_community_playlist = false;
    assert!(!app.community_pin_visible());
  }

  #[test]
  fn community_pin_hidden_when_already_following() {
    let mut app = App::default();
    assert!(app.community_pin_visible());
    app.all_playlists = vec![playlist_info(
      COMMUNITY_PLAYLIST_ID,
      "spotatui community",
      "spotatui",
      false,
    )];
    assert!(app.follows_community_playlist());
    assert!(!app.community_pin_visible());
  }

  #[test]
  fn community_pin_hidden_under_non_spotify_source() {
    let mut app = App::default();
    app.active_source = Source::Local;
    assert!(!app.community_pin_visible());
  }

  #[test]
  fn selected_playlist_id_offsets_past_pin_in_raw_fallback() {
    // Folder-aware items not yet initialized, so the sidebar falls back to the
    // raw playlist pages. Rendered rows: [+ Add Playlist, pin, First, Second].
    let mut app = App::default();
    assert!(app.community_pin_visible());
    assert!(app.playlist_folder_items.is_empty());
    app.playlists = Some(Paged {
      items: vec![
        playlist_info("00000000000000000000a0", "First", "me", false),
        playlist_info("00000000000000000000a1", "Second", "me", false),
      ],
      total: 2,
      ..Default::default()
    });

    // Row 3 is the second raw page item, not the off-by-one neighbor.
    app.view.selected_playlist_index = Some(3);
    assert_eq!(
      app.get_selected_playlist_id().as_deref(),
      Some("00000000000000000000a1")
    );
    // Row 2 is the first raw page item.
    app.view.selected_playlist_index = Some(2);
    assert_eq!(
      app.get_selected_playlist_id().as_deref(),
      Some("00000000000000000000a0")
    );
  }
}
