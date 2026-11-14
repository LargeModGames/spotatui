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
  /// The server omitted its name; a user title with the same text stays literal.
  pub generated_name: bool,
  pub name: String,
  /// Folder ID this item is visible in (which folder "page" it appears on)
  pub current_id: usize,
  /// Folder ID this item navigates to when selected
  pub target_id: usize,
}

impl PlaylistFolder {
  pub fn display_name<'a>(
    &'a self,
    strings: &crate::core::i18n::Strings,
  ) -> std::borrow::Cow<'a, str> {
    if !self.generated_name {
      return std::borrow::Cow::Borrowed(&self.name);
    }
    let name = strings.status.infra.unnamed_folder;
    if self.current_id > self.target_id {
      std::borrow::Cow::Owned(format!("← {name}"))
    } else {
      std::borrow::Cow::Borrowed(name)
    }
  }
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

  /// Record in `state.yml` that the pin prompt was shown, so it never asks again.
  pub(crate) fn mark_community_pin_prompt_shown(&mut self) {
    self.runtime_state.community_pin_prompt_shown = true;
    if let Err(e) =
      self.save_runtime_state(&PersistedRuntimeState::community_pin_prompt_shown(true))
    {
      log::warn!("failed to persist community pin prompt state: {}", e);
    }
  }

  /// Answer the pin prompt: hide the pin (persisted) or keep it, then close it.
  pub(crate) fn answer_community_pin_prompt(&mut self, keep: bool) {
    if !keep {
      self.user_config.behavior.pin_community_playlist = false;
      if let Err(e) = self.user_config.save_config() {
        log::warn!("failed to persist community playlist pin setting: {}", e);
      }
      self.set_status_message(
        self.strings().status.app.community_pin_hidden.to_string(),
        6,
      );
    }
    self.mark_community_pin_prompt_shown();
    self.pop_navigation_stack();
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
    self.sort_sidebar_playlist_slots(&mut items, |item| match item {
      PlaylistFolderItem::Playlist { index, .. } => self.all_playlists.get(*index),
      PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => None,
    });
    // Injected after sorting so the pin is always row 0, regardless of grouping.
    if self.community_pin_visible() {
      items.insert(0, &self.community_pin_item);
    }
    items
  }

  /// Shared ordering for one level of the TUI or GUI playlist tree.
  pub(crate) fn sort_sidebar_playlist_slots<'a, T: Clone>(
    &self,
    items: &mut [T],
    playlist: impl Fn(&T) -> Option<&'a PlaylistInfo>,
  ) {
    sort_playlist_slots(
      items,
      self.runtime_state.playlist_sidebar_order,
      &self.playlist_last_listens,
      playlist,
    );
  }

  pub(super) fn set_playlist_sidebar_order(&mut self, order: SidebarPlaylistOrder) {
    if self.runtime_state.playlist_sidebar_order == order {
      return;
    }
    let selected = self.selected_sidebar_item();
    self.runtime_state.playlist_sidebar_order = order;
    self.restore_sidebar_item(selected);
    self.schedule_state_save(PersistedRuntimeState::playlist_sidebar_order(order));
    self.display_revisions.bump(DisplayDomain::Library);
  }

  /// Only the collector writes this cache. Its startup load lands before its
  /// incremental records, and display order is always read from current state,
  /// so changing the sort or refreshing the rootlist cannot make a load stale.
  pub(crate) fn set_playlist_last_listens(&mut self, listens: PlaylistLastListens) {
    if self.playlist_last_listens == listens {
      return;
    }
    let selected = self.selected_sidebar_item();
    self.playlist_last_listens = listens;
    self.restore_sidebar_item(selected);
    if self.runtime_state.playlist_sidebar_order == SidebarPlaylistOrder::RecentlyPlayed {
      self.display_revisions.bump(DisplayDomain::Library);
    }
  }

  pub(crate) fn record_playlist_listen(&mut self, record: &crate::infra::history::ListenRecord) {
    let Some(uri) = record
      .context_uri
      .as_deref()
      .filter(|uri| uri.starts_with("spotify:playlist:"))
    else {
      return;
    };
    if self
      .playlist_last_listens
      .get(uri)
      .is_some_and(|last| *last >= record.ended_at)
    {
      return;
    }
    let selected = self.selected_sidebar_item();
    self
      .playlist_last_listens
      .insert(uri.to_string(), record.ended_at);
    self.restore_sidebar_item(selected);
    if self.runtime_state.playlist_sidebar_order == SidebarPlaylistOrder::RecentlyPlayed {
      self.display_revisions.bump(DisplayDomain::Library);
    }
  }

  fn selected_sidebar_item(&self) -> Option<PlaylistFolderItem> {
    if self.active_source != Source::Spotify {
      return None;
    }
    self
      .view
      .selected_playlist_index
      .and_then(|index| index.checked_sub(1))
      .and_then(|index| self.get_playlist_display_item_at(index))
      .cloned()
  }

  fn restore_sidebar_item(&mut self, selected: Option<PlaylistFolderItem>) {
    let Some(selected) = selected else { return };
    if let Some(index) =
      self
        .get_playlist_display_items()
        .iter()
        .position(|item| match (&selected, item) {
          (
            PlaylistFolderItem::Playlist { index: a, .. },
            PlaylistFolderItem::Playlist { index: b, .. },
          ) => a == b,
          (PlaylistFolderItem::Folder(a), PlaylistFolderItem::Folder(b)) => {
            a.target_id == b.target_id
          }
          (PlaylistFolderItem::CommunityPin, PlaylistFolderItem::CommunityPin) => true,
          _ => false,
        })
    {
      self.view.selected_playlist_index = Some(index + 1);
    }
  }

  /// Changes current playlist and resets playlist scroll offset
  pub fn open_playlist_folder(&mut self, target_id: usize) {
    self.current_playlist_folder_id = target_id;
    self.view.playlist_scroll_offset.set(0);
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

  fn sidebar_app() -> App {
    fn playlist(index: usize, current_id: usize) -> PlaylistFolderItem {
      PlaylistFolderItem::Playlist { index, current_id }
    }
    fn folder(name: &str, current_id: usize, target_id: usize) -> PlaylistFolderItem {
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
        name: name.to_string(),
        current_id,
        target_id,
      })
    }
    App {
      all_playlists: [
        "Zulu",
        "beta",
        "Alpha",
        "ALPHA",
        "Inner Zulu",
        "inner alpha",
      ]
      .iter()
      .enumerate()
      .map(|(i, name)| playlist_info(&format!("p{i}"), name, "me", false))
      .collect(),
      playlist_folder_items: vec![
        playlist(0, 0),
        folder("Folder", 0, 1),
        playlist(1, 0),
        playlist(2, 0),
        playlist(3, 0),
        folder("← Back", 1, 0),
        playlist(4, 1),
        folder("Nested", 1, 2),
        playlist(5, 1),
      ],
      ..App::default()
    }
  }

  fn sidebar_names(app: &App) -> Vec<String> {
    app
      .get_playlist_display_items()
      .into_iter()
      .map(|item| match item {
        PlaylistFolderItem::CommunityPin => "pin".to_string(),
        PlaylistFolderItem::Folder(folder) => folder.name.clone(),
        PlaylistFolderItem::Playlist { index, .. } => app.all_playlists[*index].name.clone(),
      })
      .collect()
  }

  #[test]
  fn sidebar_playlist_name_sort_keeps_folder_slots_pin_and_stable_ties() {
    let mut app = sidebar_app();
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::Name);
    assert_eq!(
      sidebar_names(&app),
      ["pin", "Alpha", "Folder", "ALPHA", "beta", "Zulu"]
    );
    // Repeated selection cannot reverse the fixed Name order or its ties.
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::Name);
    app.toggle_sort_order(SortContext::SidebarPlaylists);
    assert_eq!(
      sidebar_names(&app),
      ["pin", "Alpha", "Folder", "ALPHA", "beta", "Zulu"]
    );
    app.open_playlist_folder(1);
    assert_eq!(
      sidebar_names(&app),
      ["← Back", "inner alpha", "Nested", "Inner Zulu"]
    );
    app.open_playlist_folder(2);
    assert!(sidebar_names(&app).is_empty());
  }

  #[test]
  fn sidebar_playlist_recent_order_keeps_never_played_tail_and_folder_scope() {
    let mut app = sidebar_app();
    let older = chrono::DateTime::from_timestamp(1, 0).unwrap();
    let newer = chrono::DateTime::from_timestamp(2, 0).unwrap();
    app.set_playlist_last_listens(
      [
        (app.all_playlists[2].uri.clone(), newer),
        (app.all_playlists[1].uri.clone(), older),
        (app.all_playlists[5].uri.clone(), newer),
      ]
      .into(),
    );
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::RecentlyPlayed);
    assert_eq!(
      sidebar_names(&app),
      ["pin", "Alpha", "Folder", "beta", "Zulu", "ALPHA"]
    );
    app.open_playlist_folder(1);
    assert_eq!(
      sidebar_names(&app),
      ["← Back", "inner alpha", "Nested", "Inner Zulu"]
    );
    // The Spotify order is recovered from rootlist data, never a previously sorted list.
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::Default);
    assert_eq!(
      sidebar_names(&app),
      ["← Back", "Inner Zulu", "Nested", "inner alpha"]
    );
    app.open_playlist_folder(0);
    assert_eq!(
      sidebar_names(&app),
      ["pin", "Zulu", "Folder", "beta", "Alpha", "ALPHA"]
    );
  }

  #[test]
  fn sidebar_playlist_order_changes_keep_selected_playlist_and_sparse_state_patch() {
    let mut app = sidebar_app();
    app.view.selected_playlist_index = Some(2); // Zulu, after Add and pin.
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::Name);
    assert_eq!(app.view.selected_playlist_index, Some(6));
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p0"));
    assert!(matches!(
      app.get_playlist_display_item_at(5),
      Some(PlaylistFolderItem::Playlist { index: 0, .. })
    ));
    assert_eq!(
      app.pending_state_save_patch,
      PersistedRuntimeState::playlist_sidebar_order(SidebarPlaylistOrder::Name)
    );
    app.set_playlist_last_listens(
      [(
        app.all_playlists[3].uri.clone(),
        chrono::DateTime::from_timestamp(3, 0).unwrap(),
      )]
      .into(),
    );
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::RecentlyPlayed);
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p0"));
    assert_eq!(app.view.selected_playlist_index, Some(4));
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::Default);
    assert_eq!(app.view.selected_playlist_index, Some(2));
  }

  #[test]
  fn sidebar_playlist_cache_updates_preserve_folder_and_pin_selections() {
    let mut app = sidebar_app();
    app.apply_sort_field(SortContext::SidebarPlaylists, SortField::RecentlyPlayed);
    for (row, name) in [(1, "pin"), (3, "Folder")] {
      app.view.selected_playlist_index = Some(row);
      app.set_playlist_last_listens(
        [(
          app.all_playlists[2].uri.clone(),
          chrono::DateTime::from_timestamp(row as i64, 0).unwrap(),
        )]
        .into(),
      );
      assert_eq!(app.view.selected_playlist_index, Some(row));
      assert_eq!(sidebar_names(&app)[row - 1], name);
    }
    // A source switch must not move that source's playlist cursor.
    app.active_source = Source::Local;
    app.view.selected_playlist_index = Some(7);
    app.set_playlist_last_listens(PlaylistLastListens::new());
    assert_eq!(app.view.selected_playlist_index, Some(7));
  }

  #[test]
  fn group_folders_first_hoists_folders_stably_and_only_when_enabled() {
    fn folder(name: &str) -> PlaylistFolderItem {
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
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
    let app = App {
      active_source: Source::Local,
      ..Default::default()
    };
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
