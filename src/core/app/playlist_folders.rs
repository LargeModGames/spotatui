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
    let at_spotify_root =
      self.active_source == Source::Spotify && self.current_playlist_folder_id == 0;
    let pinned: Vec<_> = if at_spotify_root {
      present_pins(self.pinned_playlist_uris(), |uri| {
        self.playlist_folder_items.iter().any(|item| {
          self
            .get_playlist_for_item(item)
            .is_some_and(|p| p.uri == uri)
        })
      })
      .into_iter()
      .filter_map(|uri| {
        self.playlist_folder_items.iter().find(|item| {
          self
            .get_playlist_for_item(item)
            .is_some_and(|p| p.uri == uri)
        })
      })
      .collect()
    } else {
      Vec::new()
    };
    // Remove pins before sorting, so their former slots cannot shift folders.
    items = pinned_first(Vec::new(), items, |item| {
      at_spotify_root
        && self
          .get_playlist_for_item(item)
          .is_some_and(|p| self.is_playlist_pinned(&p.uri))
    });
    if self.user_config.behavior.group_folders_first {
      items.sort_by_key(|item| !matches!(item, PlaylistFolderItem::Folder(_)));
    }
    self.sort_sidebar_playlist_slots(&mut items, |item| match item {
      PlaylistFolderItem::Playlist { index, .. } => self.all_playlists.get(*index),
      PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => None,
    });
    items.splice(0..0, pinned);
    // Injected after sorting so the pin is always row 0, regardless of grouping.
    if self.community_pin_visible() {
      items.insert(0, &self.community_pin_item);
    }
    items
  }

  pub(crate) fn pinned_playlist_uris(&self) -> &[String] {
    &self.runtime_state.pinned_playlists
  }

  pub(crate) fn is_playlist_pinned(&self, uri: &str) -> bool {
    self.pinned_playlist_uris().iter().any(|pin| pin == uri)
  }

  /// Local pin intent is saved immediately; it must never enter the debouncer.
  pub(crate) fn set_playlist_pinned(&mut self, uri: &str, pinned: bool) {
    if self.is_playlist_pinned(uri) == pinned {
      return;
    }
    let Some(playlist) = self.all_playlists.iter().find(|p| p.uri == uri) else {
      return;
    };
    let name = playlist.name.clone();
    let selected = self.selected_sidebar_item();
    let result = if pinned {
      let mut patch = PersistedRuntimeState::pinned_playlist(uri);
      let Some(pins) = patch
        .pinned_playlists
        .as_ref()
        .filter(|pins| !pins.is_empty())
      else {
        return;
      };
      if self.runtime_state.pinned_playlists.len() >= crate::core::state::MAX_PINNED_PLAYLISTS {
        self.set_error_status_message(
          self
            .strings()
            .status
            .app
            .playlist_pin_limit(crate::core::state::MAX_PINNED_PLAYLISTS),
          4,
        );
        return;
      }
      self
        .runtime_state
        .pinned_playlists
        .extend(pins.iter().cloned());
      if let Some(account) = self
        .user
        .as_ref()
        .map(|user| &user.id)
        .filter(|id| !id.is_empty())
      {
        self
          .runtime_state
          .playlist_pin_accounts
          .insert(uri.to_string(), account.clone());
        patch.playlist_pin_accounts = Some([(uri.to_string(), account.clone())].into());
      }
      self.save_runtime_state(&patch)
    } else {
      self.runtime_state.pinned_playlists.retain(|pin| pin != uri);
      self.runtime_state.playlist_pin_accounts.remove(uri);
      self.save_removed_pinned_playlist(uri)
    };
    if pinned && result.is_err() {
      self.runtime_state.pinned_playlists.retain(|pin| pin != uri);
      self.runtime_state.playlist_pin_accounts.remove(uri);
    }
    self.restore_sidebar_item(selected);
    self.clamp_playlist_pin_cursor();
    self.display_revisions.bump(DisplayDomain::Library);
    if let Err(error) = result {
      log::warn!("failed to persist playlist pin: {error}");
      let message = if error.is::<crate::core::state::PlaylistPinLimitReached>() {
        self
          .strings()
          .status
          .app
          .playlist_pin_limit(crate::core::state::MAX_PINNED_PLAYLISTS)
      } else {
        self
          .strings()
          .status
          .app
          .state_save_failed(&error.to_string())
      };
      self.handle_error_with_message(error, message);
    } else {
      let message = if pinned {
        self.strings().status.app.playlist_pinned(&name)
      } else {
        self.strings().status.app.playlist_unpinned(&name)
      };
      self.set_status_message(message, 4);
    }
  }

  /// Unfollowing and a complete rootlist refresh remove obsolete pins silently.
  pub(crate) fn forget_playlist_pin(&mut self, uri: &str) {
    if !self.is_playlist_pinned(uri) {
      return;
    }
    let selected = self.selected_sidebar_item();
    self.runtime_state.pinned_playlists.retain(|pin| pin != uri);
    self.runtime_state.playlist_pin_accounts.remove(uri);
    if let Err(error) = self.save_removed_pinned_playlist(uri) {
      log::warn!("failed to remove playlist pin: {error}");
      let message = self
        .strings()
        .status
        .app
        .state_save_failed(&error.to_string());
      self.handle_error_with_message(error, message);
    }
    self.restore_sidebar_item(selected);
    self.clamp_playlist_pin_cursor();
    self.display_revisions.bump(DisplayDomain::Library);
  }

  /// Remove pins absent from both the complete rootlist and the refreshed list.
  /// Unresolved rootlist entries and followed playlists keep their pins.
  pub(crate) fn prune_playlist_pins(
    &mut self,
    nodes: &[PlaylistFolderNode],
    account: Option<&str>,
  ) {
    let Some(account) = account.filter(|account| !account.is_empty()) else {
      return;
    };
    fn contains(nodes: &[PlaylistFolderNode], uri: &str) -> bool {
      nodes
        .iter()
        .any(|node| node.uri == uri || contains(&node.children, uri))
    }
    let selected = self.selected_sidebar_item();
    let removed: Vec<_> = self
      .pinned_playlist_uris()
      .iter()
      .filter(|uri| {
        self
          .runtime_state
          .playlist_pin_accounts
          .get(uri.as_str())
          .is_some_and(|owner| owner == account)
          && !contains(nodes, uri)
          && !self
            .all_playlists
            .iter()
            .any(|playlist| playlist.uri == uri.as_str())
      })
      .cloned()
      .collect();
    if removed.is_empty() {
      return;
    }
    self
      .runtime_state
      .pinned_playlists
      .retain(|uri| !removed.contains(uri));
    self
      .runtime_state
      .playlist_pin_accounts
      .retain(|uri, _| !removed.contains(uri));
    if let Err(error) = self.save_pruned_playlist_pins(&removed, account) {
      log::warn!("failed to prune playlist pins: {error}");
      let message = self
        .strings()
        .status
        .app
        .state_save_failed(&error.to_string());
      self.handle_error_with_message(error, message);
    }
    self.restore_sidebar_item(selected);
    self.clamp_playlist_pin_cursor();
    self.display_revisions.bump(DisplayDomain::Library);
  }

  fn clamp_playlist_pin_cursor(&mut self) {
    if self.active_source == Source::Spotify {
      if let Some(index) = self.view.selected_playlist_index {
        // Add Playlist is row zero, so the item count is the last valid row.
        self.view.selected_playlist_index = Some(index.min(self.get_playlist_display_count()));
      }
    }
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

  #[test]
  fn pinned_playlists_sit_under_the_community_pin_in_pin_order() {
    let mut app = sidebar_app();
    app.runtime_state.pinned_playlists = vec![
      app.all_playlists[2].uri.clone(),
      app.all_playlists[0].uri.clone(),
    ];
    assert_eq!(
      sidebar_names(&app),
      ["pin", "Alpha", "Zulu", "Folder", "beta", "ALPHA"]
    );
    assert_eq!(app.get_playlist_display_count(), 6);
  }

  #[test]
  fn pinned_playlists_ignore_the_sidebar_order_and_folders_first() {
    for order in [
      SidebarPlaylistOrder::Spotify,
      SidebarPlaylistOrder::RecentlyPlayed,
      SidebarPlaylistOrder::Name,
    ] {
      for folders_first in [false, true] {
        let mut app = sidebar_app();
        app.runtime_state.playlist_sidebar_order = order;
        app.user_config.behavior.group_folders_first = folders_first;
        app.runtime_state.pinned_playlists = vec![
          app.all_playlists[1].uri.clone(),
          app.all_playlists[3].uri.clone(),
        ];
        app.playlist_last_listens.insert(
          app.all_playlists[2].uri.clone(),
          chrono::DateTime::from_timestamp(2, 0).unwrap(),
        );
        let names = sidebar_names(&app);
        assert_eq!(
          &names[..3],
          ["pin", "beta", "ALPHA"],
          "{order:?} folders_first={folders_first}"
        );
        let tail: &[&str] = match (folders_first, order) {
          (true, SidebarPlaylistOrder::Spotify) => &["Folder", "Zulu", "Alpha"],
          (true, SidebarPlaylistOrder::Name | SidebarPlaylistOrder::RecentlyPlayed) => {
            &["Folder", "Alpha", "Zulu"]
          }
          (false, SidebarPlaylistOrder::Spotify) => &["Zulu", "Folder", "Alpha"],
          (false, SidebarPlaylistOrder::Name | SidebarPlaylistOrder::RecentlyPlayed) => {
            &["Alpha", "Folder", "Zulu"]
          }
        };
        assert_eq!(&names[3..], tail);
      }
    }
  }

  #[test]
  fn a_playlist_pinned_inside_a_folder_is_lifted_and_stays_in_its_folder() {
    let mut app = sidebar_app();
    app.runtime_state.pinned_playlists = vec![
      app.all_playlists[5].uri.clone(),
      app.all_playlists[0].uri.clone(),
      "spotify:playlist:gone".into(),
      app.all_playlists[0].uri.clone(),
    ];
    assert_eq!(
      sidebar_names(&app),
      [
        "pin",
        "inner alpha",
        "Zulu",
        "Folder",
        "beta",
        "Alpha",
        "ALPHA"
      ]
    );
    app.view.selected_playlist_index = Some(2);
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p5"));
    app.open_playlist_folder(1);
    assert_eq!(
      sidebar_names(&app),
      ["← Back", "Inner Zulu", "Nested", "inner alpha"]
    );
    assert_eq!(
      sidebar_names(&app)
        .iter()
        .filter(|name| *name == "inner alpha")
        .count(),
      1
    );
    app.active_source = Source::Local;
    app.open_playlist_folder(0);
    assert_eq!(
      sidebar_names(&app),
      ["Zulu", "Folder", "beta", "Alpha", "ALPHA"]
    );
  }

  #[test]
  fn the_unstructured_fallback_does_not_inject_local_pins() {
    let mut app = sidebar_app();
    app.runtime_state.pinned_playlists = vec![app.all_playlists[0].uri.clone()];
    app.playlist_folder_items.clear();
    assert_eq!(sidebar_names(&app), ["pin"]);
  }

  #[test]
  fn forgetting_a_pin_preserves_the_selected_item_after_the_row_moves() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    app.state_path = Some(dir.path().join("state.yml"));
    let uri = app.all_playlists[3].uri.clone();
    app.set_playlist_pinned(&uri, true);
    app.view.selected_playlist_index = Some(2);
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p3"));
    app.forget_playlist_pin(&uri);
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p3"));
    assert_eq!(app.view.selected_playlist_index, Some(6));
  }

  #[test]
  fn a_failed_pin_save_rolls_back_and_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    app.state_path = Some(blocked.join("state.yml"));
    app.user = Some(UserInfo {
      id: "account-a".into(),
      ..Default::default()
    });
    let uri = app.all_playlists[2].uri.clone();
    app.view.selected_playlist_index = Some(5);

    app.set_playlist_pinned(&uri, true);

    assert!(app.pinned_playlist_uris().is_empty());
    assert!(app.runtime_state.playlist_pin_accounts.is_empty());
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p2"));

    let path = dir.path().join("state.yml");
    app.state_path = Some(path.clone());
    app.clear_api_error();
    app.set_playlist_pinned(&uri, true);

    assert!(app.is_playlist_pinned(&uri));
    let saved = crate::core::state::load(&path).unwrap();
    assert_eq!(saved.pinned_playlists.unwrap(), [uri.clone()]);
    assert_eq!(
      saved.playlist_pin_accounts.unwrap(),
      [(uri, "account-a".into())].into()
    );
  }

  #[test]
  fn failed_playlist_pin_writes_use_the_error_route() {
    for operation in ["pin", "unpin", "forget", "prune"] {
      let dir = tempfile::tempdir().unwrap();
      let mut app = sidebar_app();
      let blocked = dir.path().join("blocked");
      std::fs::write(&blocked, "not a directory").unwrap();
      app.state_path = Some(blocked.join("state.yml"));
      let uri = app.all_playlists[0].uri.clone();
      if operation != "pin" {
        app.runtime_state.pinned_playlists = vec![uri.clone()];
        app.runtime_state.playlist_pin_accounts = [(uri.clone(), "account-a".into())].into();
      }
      match operation {
        "pin" => app.set_playlist_pinned(&uri, true),
        "unpin" => app.set_playlist_pinned(&uri, false),
        "forget" => app.forget_playlist_pin(&uri),
        "prune" => {
          app.all_playlists.clear();
          app.prune_playlist_pins(&[], Some("account-a"));
        }
        _ => unreachable!(),
      }

      assert!(
        app.api_error().starts_with("Failed to save state: "),
        "{operation}: {}",
        app.api_error()
      );
      assert!(app.api_error().contains("state.yml"), "{operation}");
      assert_eq!(app.get_current_route().id, RouteId::Error, "{operation}");
      assert!(!app.is_playlist_pinned(&uri), "{operation}");
    }
  }

  #[test]
  fn the_pin_limit_reports_an_error_without_adding_an_unsaved_pin() {
    let mut app = sidebar_app();
    app.runtime_state.pinned_playlists = (0..100)
      .map(|i| format!("spotify:playlist:pin{i}"))
      .collect();
    let uri = app.all_playlists[0].uri.clone();
    app.set_playlist_pinned(&uri, true);
    assert!(!app.is_playlist_pinned(&uri));
    assert_eq!(app.pinned_playlist_uris().len(), 100);
    assert!(app.status_message_is_error());
    assert_eq!(
      app.status_message(),
      Some("You can pin up to 100 playlists")
    );
  }

  #[test]
  fn pins_added_by_another_instance_cannot_make_a_new_pin_silently_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    let path = dir.path().join("state.yml");
    app.state_path = Some(path.clone());
    let disk_pins = (0..100)
      .map(|i| format!("spotify:playlist:pin{i}"))
      .collect::<Vec<_>>();
    app.runtime_state.pinned_playlists = disk_pins[..99].to_vec();
    crate::core::state::save(
      &path,
      &PersistedRuntimeState {
        pinned_playlists: Some(disk_pins.clone()),
        ..Default::default()
      },
    )
    .unwrap();
    let uri = app.all_playlists[0].uri.clone();
    app.user = Some(UserInfo {
      id: "account-a".into(),
      ..Default::default()
    });
    app.set_playlist_pinned(&uri, true);
    assert!(!app.is_playlist_pinned(&uri));
    assert!(!app.runtime_state.playlist_pin_accounts.contains_key(&uri));
    assert_eq!(app.api_error(), "You can pin up to 100 playlists");
    assert_eq!(app.get_current_route().id, RouteId::Error);
    assert_eq!(
      crate::core::state::load(&path)
        .unwrap()
        .pinned_playlists
        .unwrap(),
      disk_pins
    );
    // Once the other instance frees a slot, the same request can be retried.
    crate::core::state::save_removing_pinned_playlist(&path, &disk_pins[99]).unwrap();
    app.clear_api_error();
    app.set_playlist_pinned(&uri, true);
    assert!(app.is_playlist_pinned(&uri));
    assert!(crate::core::state::load(&path)
      .unwrap()
      .pinned_playlists
      .unwrap()
      .contains(&uri));
  }

  #[test]
  fn pinning_and_unpinning_preserve_the_selected_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    app.state_path = Some(dir.path().join("state.yml"));
    let uri = app.all_playlists[2].uri.clone();
    app.view.selected_playlist_index = Some(5);
    app.set_playlist_pinned(&uri, true);
    assert_eq!(app.view.selected_playlist_index, Some(2));
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p2"));
    // Sort still restores the selected playlist through the pinned row mapping.
    app.set_playlist_sidebar_order(SidebarPlaylistOrder::Name);
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p2"));
    let inner = app.all_playlists[5].uri.clone();
    app.set_playlist_pinned(&inner, true);
    app.view.selected_playlist_index = Some(app.get_playlist_display_count());
    app.set_playlist_pinned(&inner, false);
    assert_eq!(
      app.view.selected_playlist_index,
      Some(app.get_playlist_display_count())
    );
    assert_eq!(app.runtime_state.pinned_playlists, [uri]);
  }

  #[test]
  fn pruning_keeps_followed_playlists_and_unresolved_rootlist_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    app.state_path = Some(dir.path().join("state.yml"));
    app.runtime_state.pinned_playlists = vec![
      "spotify:playlist:gone".into(),
      app.all_playlists[3].uri.clone(), // Followed, but omitted from the rootlist.
      "spotify:playlist:unresolved".into(),
      app.all_playlists[0].uri.clone(),
    ];
    app.runtime_state.playlist_pin_accounts = app
      .pinned_playlist_uris()
      .iter()
      .map(|uri| (uri.clone(), "account-a".to_string()))
      .collect();
    app
      .save_runtime_state(&PersistedRuntimeState {
        pinned_playlists: Some(app.runtime_state.pinned_playlists.clone()),
        playlist_pin_accounts: Some(app.runtime_state.playlist_pin_accounts.clone()),
        ..Default::default()
      })
      .unwrap();
    app.view.selected_playlist_index = Some(3);
    app.prune_playlist_pins(
      &[
        PlaylistFolderNode {
          name: None,
          node_type: PlaylistFolderNodeType::Folder,
          uri: "folder:1".into(),
          children: vec![PlaylistFolderNode {
            name: None,
            node_type: PlaylistFolderNodeType::Playlist,
            uri: "spotify:playlist:unresolved".into(),
            children: vec![],
          }],
        },
        PlaylistFolderNode {
          name: None,
          node_type: PlaylistFolderNodeType::Playlist,
          uri: app.all_playlists[0].uri.clone(),
          children: vec![],
        },
      ],
      Some("account-a"),
    );
    assert_eq!(
      app.pinned_playlist_uris(),
      [
        "spotify:playlist:p3",
        "spotify:playlist:unresolved",
        "spotify:playlist:p0"
      ]
    );
    assert_eq!(app.get_selected_playlist_id().as_deref(), Some("p0"));
    let saved = crate::core::state::load(app.state_path.as_ref().unwrap()).unwrap();
    assert_eq!(saved.pinned_playlists.unwrap(), app.pinned_playlist_uris());
    assert_eq!(
      saved.playlist_pin_accounts.unwrap(),
      app.runtime_state.playlist_pin_accounts
    );
  }

  #[test]
  fn switching_accounts_keeps_pins_until_their_own_account_removes_the_playlist() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    let path = dir.path().join("state.yml");
    app.state_path = Some(path.clone());
    app.user = Some(UserInfo {
      id: "account-a".into(),
      ..Default::default()
    });
    let a = app.all_playlists[0].uri.clone();
    let b = app.all_playlists[1].uri.clone();
    app.set_playlist_pinned(&a, true);

    // A new process loads A's pins, then signs in as B and pins its own row.
    let mut other = sidebar_app();
    other.state_path = Some(path.clone());
    other
      .runtime_state
      .apply_persisted(&crate::core::state::load(&path).unwrap());
    other.user = Some(UserInfo {
      id: "account-b".into(),
      ..Default::default()
    });
    other.all_playlists.retain(|playlist| playlist.uri != a);
    other.prune_playlist_pins(&[], Some("account-b"));
    assert_eq!(other.pinned_playlist_uris(), [a.clone()]);
    other.set_playlist_pinned(&b, true);
    other.prune_playlist_pins(&[], Some("account-a"));
    assert_eq!(other.pinned_playlist_uris(), [b.clone()]);
    let saved = crate::core::state::load(&path).unwrap();
    assert_eq!(saved.pinned_playlists.unwrap(), [b.clone()]);
    assert_eq!(
      saved.playlist_pin_accounts.unwrap(),
      [(b.clone(), "account-b".into())].into()
    );
    other.all_playlists.clear();
    other.prune_playlist_pins(&[], Some("account-b"));
    assert!(other.pinned_playlist_uris().is_empty());
    assert!(crate::core::state::load(&path)
      .unwrap()
      .pinned_playlists
      .unwrap()
      .is_empty());
  }

  #[test]
  fn unknown_accounts_and_legacy_pins_never_allow_automatic_pruning() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = sidebar_app();
    app.state_path = Some(dir.path().join("state.yml"));
    let uri = app.all_playlists[0].uri.clone();
    app.set_playlist_pinned(&uri, true);
    app.all_playlists.clear();
    app.prune_playlist_pins(&[], Some("account-a"));
    assert!(app.is_playlist_pinned(&uri));
    app
      .runtime_state
      .playlist_pin_accounts
      .insert(uri.clone(), "account-a".into());
    for unknown in [None, Some("")] {
      app.prune_playlist_pins(&[], unknown);
      assert!(app.is_playlist_pinned(&uri));
    }
  }

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
