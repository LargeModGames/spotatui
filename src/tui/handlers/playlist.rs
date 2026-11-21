use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::{ActiveBlock, RouteId, SidebarBlock};
use crate::core::app::{App, DialogContext, PlaylistFolderItem};
use crate::core::requirement::{Capability, Requirement};
use crate::core::source::Source;
use crate::tui::event::Key;

/// Total rows in the sidebar Playlists panel. For Spotify this is the leading
/// "+ Add Playlist" row + playlists/folders; for Local it is the folder count
/// (no write capability, so no "Add Playlist").
pub(crate) fn total_display_count(app: &App) -> usize {
  match app.active_source {
    Source::Local => app.local_playlists().len(),
    Source::Subsonic => app.subsonic_playlists().len(),
    Source::Radio => app.radio_stations().len(),
    // Local YouTube playlists + the "+ New Playlist" entry.
    Source::YouTube => app.youtube_playlists().len() + 1,
    Source::Qobuz => app.qobuz_playlists().len(),
    Source::Spotify => app.get_playlist_display_count() + 1,
    Source::AppleMusic => app.apple_music_playlists().len(),
  }
}

/// Open a non-Spotify playlist's tracks in the shared track table.
pub(super) fn open_source_playlist(app: &mut App, uri: Option<String>) {
  if let Some(uri) = uri {
    app.apply(Action::Open(OpenTarget::SourcePlaylist(uri)));
  }
}

/// Local Files: open the highlighted folder's tracks in the shared track table.
fn open_local_folder(app: &mut App) {
  let uri = app
    .view
    .selected_playlist_index
    .and_then(|idx| app.local_playlists().get(idx))
    .map(|folder| folder.uri.clone());
  open_source_playlist(app, uri);
}

/// Subsonic: open the highlighted server playlist's tracks in the shared track
/// table.
fn open_subsonic_folder(app: &mut App) {
  let uri = app
    .view
    .selected_playlist_index
    .and_then(|idx| app.subsonic_playlists().get(idx))
    .map(|playlist| playlist.uri.clone());
  open_source_playlist(app, uri);
}

/// Qobuz: open the highlighted row (favorites, playlist, or album) in the
/// shared track table.
fn open_qobuz_folder(app: &mut App) {
  let uri = app
    .view
    .selected_playlist_index
    .and_then(|idx| app.qobuz_playlists().get(idx))
    .map(|playlist| playlist.uri.clone());
  open_source_playlist(app, uri);
}

/// YouTube: open the highlighted local playlist's saved videos in the shared
/// track table, or the create-playlist form on the trailing "+ New Playlist"
/// entry.
fn open_youtube_playlist(app: &mut App) {
  let Some(idx) = app.view.selected_playlist_index else {
    return;
  };
  if idx == app.youtube_playlists().len() {
    // "+ New Playlist" — reuse the create form; its name stage dispatches
    // CreateYouTubePlaylist under the YouTube source.
    app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
    return;
  }
  let uri = app
    .youtube_playlists()
    .get(idx)
    .map(|playlist| playlist.uri.clone());
  open_source_playlist(app, uri);
}

/// Internet Radio: play the highlighted station directly. A station is a leaf,
/// not a container — there is no track list to drill into — so Enter starts the
/// stream instead of opening the track table.
fn play_radio_station(app: &mut App) {
  let Some(idx) = app.view.selected_playlist_index else {
    return;
  };
  let uri = app.radio_stations().get(idx).and_then(|s| s.uri.clone());
  if let Some(uri) = uri {
    // A one-item URI list: the radio router starts the station for both shapes.
    app.apply(Action::PlayUris {
      uris: vec![uri],
      offset: None,
    });
  }
}

fn remove_radio_station(app: &mut App) {
  let Some(idx) = app.view.selected_playlist_index else {
    app.set_status_message(app.strings().status.app.no_radio_selected.to_string(), 4);
    return;
  };
  let Some(uri) = app
    .radio_stations()
    .get(idx)
    .map(|station| station.uri.clone())
  else {
    app.set_status_message(app.strings().status.app.no_radio_selected.to_string(), 4);
    return;
  };
  let Some(uri) = uri else {
    app.set_status_message(app.strings().status.app.radio_no_url.to_string(), 4);
    return;
  };
  app.apply(Action::RemoveRadioStation(uri));
  // The clamp is a no-op for every outcome that leaves the list untouched.
  app.view.selected_playlist_index = if app.radio_stations().is_empty() {
    None
  } else {
    Some(idx.min(app.radio_stations().len() - 1))
  };
}

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Char(',')
      if app
        .availability(Requirement::Source(Source::Spotify))
        .is_available() =>
    {
      super::sort_menu::open_sort_menu(app, crate::core::sort::SortContext::SidebarPlaylists);
    }
    k if common_key_events::toward_content_event(k, &app.user_config) => {
      common_key_events::focus_content(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => select_next(app),
    k if common_key_events::up_event(k, &app.user_config.keys) => select_previous(app),
    k if common_key_events::high_event(k) && total_display_count(app) > 0 => {
      app.view.selected_playlist_index = Some(0);
    }
    k if common_key_events::middle_event(k) => {
      let count = total_display_count(app);
      if count > 0 {
        let next_index = if count.is_multiple_of(2) {
          count.saturating_sub(1) / 2
        } else {
          count / 2
        };
        app.view.selected_playlist_index = Some(next_index);
      }
    }
    k if common_key_events::low_event(k) => {
      let count = total_display_count(app);
      if count > 0 {
        app.view.selected_playlist_index = Some(count - 1);
      }
    }
    Key::Enter => {
      common_key_events::remember_sidebar_block(app, ActiveBlock::MyPlaylists);
      activate_selected(app);
    }
    Key::Char('D') if app.active_source == Source::Radio => {
      remove_radio_station(app);
    }
    // Deleting a local YouTube playlist: same confirm dialog UX as Spotify.
    Key::Char('D') if app.active_source == Source::YouTube => {
      if let Some(playlist) = app
        .view
        .selected_playlist_index
        .and_then(|idx| app.youtube_playlists().get(idx))
      {
        app.view.dialog = Some(playlist.name.clone());
        app.view.confirm = false;
        app.push_navigation_stack(
          RouteId::Dialog,
          ActiveBlock::Dialog(DialogContext::YouTubePlaylistWindow),
        );
      }
    }
    // Deleting playlists is a Spotify-only (PlaylistWriter) action.
    Key::Char('D') if app.active_source == Source::Spotify => {
      if let Some(selected_idx) = app.view.selected_playlist_index {
        if let Some(PlaylistFolderItem::Playlist { index, .. }) = selected_idx
          .checked_sub(1)
          .and_then(|i| app.get_playlist_display_item_at(i))
        {
          if let Some(playlist) = app.all_playlists().get(*index) {
            let selected_playlist = &playlist.name;
            app.view.dialog = Some(selected_playlist.clone());
            app.view.confirm = false;

            app.push_navigation_stack(
              RouteId::Dialog,
              ActiveBlock::Dialog(DialogContext::PlaylistWindow),
            );
          }
        }
      }
    }
    Key::Char('m')
      if app
        .availability(Requirement::Capability(Capability::PlaylistSync))
        .is_available() =>
    {
      app.apply(Action::OpenPlaylistSyncPicker);
    }
    // Config files may bind pinning to an existing sidebar key. Keep navigation
    // and the other sidebar actions above it so those bindings retain priority.
    k if k == app.user_config.keys.pin_playlist
      && app
        .availability(Requirement::BrowseSource(Source::Spotify))
        .is_available() =>
    {
      let uri = app
        .view
        .selected_playlist_index
        .and_then(|row| row.checked_sub(1))
        .and_then(|index| app.get_playlist_display_item_at(index))
        .and_then(|item| app.get_playlist_for_item(item))
        .map(|playlist| playlist.uri.clone());
      if let Some(uri) = uri {
        let pinned = !app.is_playlist_pinned(&uri);
        app.apply(Action::SetPlaylistPinned { uri, pinned });
      }
    }
    _ => {}
  }
}

/// The down key's cursor move, named for the mouse wheel.
pub(super) fn select_next(app: &mut App) {
  let count = total_display_count(app);
  if count > 0 {
    let current = app.view.selected_playlist_index.unwrap_or(0);
    app.view.selected_playlist_index = Some((current + 1) % count);
  }
}

pub(super) fn select_previous(app: &mut App) {
  let count = total_display_count(app);
  if count > 0 {
    let current = app.view.selected_playlist_index.unwrap_or(0);
    app.view.selected_playlist_index = Some(if current == 0 { count - 1 } else { current - 1 });
  }
}

/// The Enter consequence, routed by the active browse source.
pub(super) fn activate_selected(app: &mut App) {
  app.view.last_sidebar_block = SidebarBlock::Playlists;
  match app.active_source {
    Source::Local => open_local_folder(app),
    Source::Subsonic => open_subsonic_folder(app),
    Source::Radio => play_radio_station(app),
    Source::YouTube => open_youtube_playlist(app),
    Source::Qobuz => open_qobuz_folder(app),
    Source::Spotify => open_spotify_row(app),
    Source::AppleMusic => {
      let uri = app
        .view
        .selected_playlist_index
        .and_then(|i| app.apple_music_playlists().get(i))
        .map(|p| p.uri.clone());
      open_source_playlist(app, uri);
    }
  }
}

fn open_spotify_row(app: &mut App) {
  if let Some(selected_idx) = app.view.selected_playlist_index {
    if selected_idx == 0 {
      // "+ Add Playlist" is the leading row (row 0).
      app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
    } else if let Some(item) = app.get_playlist_display_item_at(selected_idx - 1) {
      match item {
        PlaylistFolderItem::Folder(folder) => {
          // Navigate into/out of folder
          let target_id = folder.target_id;
          app.apply(Action::Open(OpenTarget::PlaylistFolder(target_id)));
          // Land on the first item below the leading "+ Add Playlist" row.
          // Counted after the apply: the count is folder-scoped.
          let has_items = app.get_playlist_display_count() > 0;
          app.view.selected_playlist_index = Some(if has_items { 1 } else { 0 });
        }
        PlaylistFolderItem::Playlist { index, .. } => {
          // Open the playlist tracks: navigates immediately with the
          // cleared table as the loading state (see open_playlist_tracks).
          let index = *index;
          let id_str = app.all_playlists().get(index).and_then(|p| p.id.clone());
          if let Some(id_str) = id_str {
            app.view.active_playlist_index = Some(index);
            app.apply(Action::Open(OpenTarget::Playlist {
              id: id_str,
              from_search: false,
            }));
          }
        }
        PlaylistFolderItem::CommunityPin => {
          app.view.active_playlist_index = None;
          app.apply(Action::Open(OpenTarget::Playlist {
            id: crate::core::app::COMMUNITY_PLAYLIST_ID.to_string(),
            from_search: false,
          }));
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::TrackInfo;
  use crate::core::state::RadioStationConfig;
  use crate::core::test_helpers::playlist_info;
  use crate::core::user_config::{UserConfig, UserConfigPaths};
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn radio_station_row(name: &str, url: &str) -> TrackInfo {
    TrackInfo {
      generated_name: None,
      uri: Some(format!("radio:{url}")),
      name: name.to_string(),
      artists: vec![],
      album: String::new(),
      duration_ms: 0,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
      release_date: None,
    }
  }

  fn pin_app_with(key: Key, source: Source) -> (App, tempfile::TempDir) {
    let mut config = UserConfig::new();
    config.keys.pin_playlist = key;
    pin_app_with_config(config, source, false)
  }

  fn pin_app_with_config(
    mut config: UserConfig,
    source: Source,
    spotify_connected: bool,
  ) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    let (tx, _rx) = channel();
    let mut app = App::new_with_state(
      tx,
      config,
      crate::core::state::RuntimeState::default(),
      Some(dir.path().join("state.yml")),
      spotify_connected,
      crate::core::i18n::Language::En,
    );
    app.set_active_source(source);
    *app.all_playlists_mut() = vec![playlist_info("p1", "One", "me", false)];
    *app.playlist_folder_items_mut() = vec![
      PlaylistFolderItem::Folder(crate::core::app::PlaylistFolder {
        generated_name: false,
        name: "Folder".into(),
        current_id: 0,
        target_id: 1,
      }),
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 0,
      },
    ];
    app.view.selected_playlist_index = Some(3);
    app.push_navigation_stack(RouteId::Home, ActiveBlock::MyPlaylists);
    app.set_current_route_state(
      Some(ActiveBlock::MyPlaylists),
      Some(ActiveBlock::MyPlaylists),
    );
    (app, dir)
  }

  fn selected_playlist_uri(app: &App) -> Option<&str> {
    app
      .view
      .selected_playlist_index
      .and_then(|row| row.checked_sub(1))
      .and_then(|index| app.get_playlist_display_item_at(index))
      .and_then(|item| app.get_playlist_for_item(item))
      .map(|playlist| playlist.uri.as_str())
  }

  #[test]
  fn the_pin_key_keeps_the_same_playlist_selected_when_toggled_without_a_login() {
    let (mut app, _dir) = pin_app_with(Key::Char('t'), Source::Spotify);
    let uri = "spotify:playlist:p1";
    assert_eq!(selected_playlist_uri(&app), Some(uri));

    super::super::handle_app(Key::Char('t'), &mut app);
    assert!(app.is_playlist_pinned(uri));
    assert_eq!(app.status_message(), Some("Pinned One"));
    assert_eq!(selected_playlist_uri(&app), Some(uri));
    assert_eq!(app.view.selected_playlist_index, Some(2));

    super::super::handle_app(Key::Char('t'), &mut app);
    assert!(!app.is_playlist_pinned(uri));
    assert_eq!(app.status_message(), Some("Unpinned One"));
    assert_eq!(selected_playlist_uri(&app), Some(uri));
    assert_eq!(app.view.selected_playlist_index, Some(3));
    app.flush_state_save(true);
    assert!(crate::core::state::load(app.state_path.as_ref().unwrap())
      .unwrap()
      .pinned_playlists
      .unwrap()
      .is_empty());
  }

  #[test]
  fn the_pin_key_keeps_the_same_uri_when_unpinning_with_a_community_pin_and_folder() {
    let (mut app, _dir) = pin_app_with(Key::Char('t'), Source::Spotify);
    let uri = "spotify:playlist:p1";
    app
      .all_playlists_mut()
      .push(playlist_info("p2", "Two", "me", false));
    app.playlist_folder_items_mut().insert(
      0,
      PlaylistFolderItem::Playlist {
        index: 1,
        current_id: 0,
      },
    );
    app.apply(Action::SetPlaylistPinned {
      uri: uri.to_string(),
      pinned: true,
    });
    app.view.selected_playlist_index = Some(2);
    assert!(app.community_pin_visible());
    assert!(matches!(
      app.get_playlist_display_item_at(3),
      Some(PlaylistFolderItem::Folder(_))
    ));
    assert_eq!(selected_playlist_uri(&app), Some(uri));

    super::super::handle_app(Key::Char('t'), &mut app);
    assert!(app.pinned_playlist_uris().is_empty());
    assert_eq!(selected_playlist_uri(&app), Some(uri));
    assert_eq!(app.view.selected_playlist_index, Some(4));

    super::super::handle_app(Key::Char('t'), &mut app);
    assert!(app.is_playlist_pinned(uri));
    assert!(!app.is_playlist_pinned("spotify:playlist:p2"));
    assert_eq!(selected_playlist_uri(&app), Some(uri));
    assert_eq!(app.view.selected_playlist_index, Some(2));
  }

  #[test]
  fn a_rebound_pin_key_is_the_one_that_pins() {
    let (mut app, _dir) = pin_app_with(Key::Char('u'), Source::Spotify);
    super::super::handle_app(Key::Char('t'), &mut app);
    assert!(app.pinned_playlist_uris().is_empty());
    super::super::handle_app(Key::Char('u'), &mut app);
    assert!(app.is_playlist_pinned("spotify:playlist:p1"));
  }

  #[test]
  fn rebound_up_and_down_keys_move_the_cursor_without_pinning() {
    for (up, expected_row) in [(true, 2), (false, 0)] {
      let mut config = UserConfig::new();
      if up {
        config.keys.move_up = Key::Char('t');
      } else {
        config.keys.move_down = Key::Char('t');
      }
      let (mut app, _dir) = pin_app_with_config(config, Source::Spotify, false);

      super::super::handle_app(Key::Char('t'), &mut app);

      assert_eq!(app.view.selected_playlist_index, Some(expected_row));
      assert!(app.pinned_playlist_uris().is_empty());
      assert!(!app.state_path.as_ref().unwrap().exists());
    }
  }

  #[test]
  fn rebound_content_navigation_keeps_priority_over_pinning_on_either_sidebar_side() {
    for side in ["left", "right"] {
      let mut config = UserConfig::new();
      config.behavior.sidebar_position = side.to_string();
      if side == "right" {
        config.keys.move_left = Key::Char('t');
      } else {
        config.keys.move_right = Key::Char('t');
      }
      let (mut app, _dir) = pin_app_with_config(config, Source::Spotify, false);

      super::super::handle_app(Key::Char('t'), &mut app);

      assert_eq!(app.get_current_route().active_block, ActiveBlock::Home);
      assert_eq!(app.get_current_route().hovered_block, ActiveBlock::Home);
      assert_eq!(app.view.selected_playlist_index, Some(3));
      assert!(app.pinned_playlist_uris().is_empty());
      assert!(!app.state_path.as_ref().unwrap().exists());
    }
  }

  #[test]
  fn fixed_sidebar_navigation_keeps_priority_over_a_rebound_pin_key() {
    for (key, expected_row) in [
      (Key::Up, 2),
      (Key::Down, 0),
      (Key::Ctrl('p'), 2),
      (Key::Ctrl('n'), 0),
      (Key::Char('H'), 0),
      (Key::Char('M'), 1),
      (Key::Char('L'), 3),
    ] {
      let (mut app, _dir) = pin_app_with(key, Source::Spotify);

      handler(key, &mut app);

      assert_eq!(
        app.view.selected_playlist_index,
        Some(expected_row),
        "{key:?}"
      );
      assert!(app.pinned_playlist_uris().is_empty(), "{key:?}");
      assert!(!app.state_path.as_ref().unwrap().exists());
    }
  }

  #[test]
  fn existing_sidebar_actions_keep_priority_over_a_rebound_pin_key() {
    for key in [Key::Enter, Key::Char(','), Key::Char('D'), Key::Char('m')] {
      let mut config = UserConfig::new();
      config.keys.pin_playlist = key;
      let (mut app, _dir) = pin_app_with_config(config, Source::Spotify, true);

      super::super::handle_app(key, &mut app);

      match key {
        Key::Enter => assert_eq!(app.get_current_route().id, RouteId::TrackTable),
        Key::Char(',') => {
          assert_eq!(app.get_current_route().active_block, ActiveBlock::SortMenu);
          assert!(app.view.sort_menu_visible);
        }
        Key::Char('D') => {
          assert_eq!(
            app.get_current_route().active_block,
            ActiveBlock::Dialog(DialogContext::PlaylistWindow)
          );
          assert_eq!(app.view.dialog.as_deref(), Some("One"));
        }
        Key::Char('m') => {
          if app.get_current_route().active_block
            == ActiveBlock::Dialog(DialogContext::PlaylistSyncPicker)
          {
            assert_eq!(
              app.pending_playlist_sync_master().unwrap().playlist_uri,
              "spotify:playlist:p1"
            );
          } else {
            assert_eq!(
              app.status_message(),
              Some(app.strings().status.app.no_mirror_source)
            );
          }
        }
        _ => unreachable!(),
      }
      assert!(app.pinned_playlist_uris().is_empty(), "{key:?}");
      assert!(
        crate::core::state::load(app.state_path.as_ref().unwrap())
          .unwrap()
          .pinned_playlists
          .is_none_or(|pins| pins.is_empty()),
        "{key:?}"
      );
    }
  }

  #[test]
  fn settings_refuse_conflicts_between_pinning_and_named_navigation_in_both_directions() {
    use crate::core::app::{SettingValue, SettingsCategory};

    for (id, key, unchanged, error) in [
      (
        "keys.move_up",
        Key::Char('t'),
        "k",
        "Key t is already assigned to Pin / Unpin Playlist",
      ),
      (
        "keys.pin_playlist",
        Key::Char('k'),
        "t",
        "Key k is already assigned to Move Up",
      ),
    ] {
      let (mut app, _dir) = pin_app_with(Key::Char('t'), Source::Spotify);
      app.view.settings_category = SettingsCategory::Keybindings;
      app.open_settings_screen();
      let index = app
        .settings_items
        .iter()
        .position(|item| item.id == id)
        .unwrap();
      app.view.settings_selected_index = index;

      super::super::handle_app(Key::Enter, &mut app);
      assert!(app.view.settings_edit_mode);
      super::super::handle_app(key, &mut app);

      assert_eq!(app.api_error(), error);
      assert_eq!(
        app.settings_items[index].value,
        SettingValue::Key(unchanged.to_string())
      );
      assert!(!app.view.settings_edit_mode);
      assert!(app.pinned_playlist_uris().is_empty());
    }
  }

  #[test]
  fn editing_the_pin_key_in_settings_saves_and_reloads_the_binding() {
    use crate::core::app::{SettingValue, SettingsCategory};

    let (mut app, _dir) = pin_app_with(Key::Char('t'), Source::Spotify);
    app.view.settings_category = SettingsCategory::Keybindings;
    app.open_settings_screen();
    let index = app
      .settings_items
      .iter()
      .position(|item| item.id == "keys.pin_playlist")
      .unwrap();
    app.view.settings_selected_index = index;

    super::super::handle_app(Key::Enter, &mut app);
    super::super::handle_app(Key::F6, &mut app);
    assert_eq!(
      app.settings_items[index].value,
      SettingValue::Key("f6".into())
    );
    assert_eq!(app.user_config.keys.pin_playlist, Key::Char('t'));

    let save_key = app.effective_save_settings_key();
    super::super::handle_app(save_key, &mut app);
    assert_eq!(app.user_config.keys.pin_playlist, Key::F6);
    assert!(!app.has_unsaved_settings_changes());
    let mut reloaded = UserConfig::new();
    reloaded.path_to_config = app.user_config.path_to_config.clone();
    reloaded.load_config().unwrap();
    assert_eq!(reloaded.keys.pin_playlist, Key::F6);
  }

  #[test]
  fn the_pin_key_ignores_folders_the_community_pin_add_playlist_and_other_sources() {
    let (mut app, _dir) = pin_app_with(Key::Char('t'), Source::Spotify);
    for row in [0, 1, 2] {
      app.view.selected_playlist_index = Some(row);
      handler(Key::Char('t'), &mut app);
      assert!(app.pinned_playlist_uris().is_empty());
    }
    for source in Source::ALL {
      if source == Source::Spotify {
        continue;
      }
      let (mut app, _dir) = pin_app_with(Key::Char('t'), source);
      handler(Key::Char('t'), &mut app);
      assert!(app.pinned_playlist_uris().is_empty());
    }
    assert!(!app.state_path.as_ref().unwrap().exists());
  }

  #[test]
  fn enter_playlist_dispatches_only_visible_page_load() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // Exercise a real playlist (row 1, below the leading "+ Add Playlist" row),
    // not the pinned community entry.
    app.user_config.behavior.pin_community_playlist = false;
    *app.all_playlists_mut() = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Test Playlist",
      "spotatui-test-user",
      false,
    )];
    *app.playlist_folder_items_mut() = vec![PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 0,
    }];
    app.view.selected_playlist_index = Some(1);

    handler(Key::Enter, &mut app);

    assert_eq!(app.view.last_sidebar_block, ActiveBlock::MyPlaylists.into());

    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(id, 0) => assert_eq!(id, "37i9dQZF1DXcBWIGoYBM5M"),
      _ => panic!("expected playlist page fetch"),
    }

    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn enter_playlist_navigates_immediately_and_dedups_inflight_open() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // Exercise a real playlist (row 1, below the leading "+ Add Playlist" row),
    // not the pinned community entry.
    app.user_config.behavior.pin_community_playlist = false;
    *app.all_playlists_mut() = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Test Playlist",
      "spotatui-test-user",
      false,
    )];
    *app.playlist_folder_items_mut() = vec![PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 0,
    }];
    app.view.selected_playlist_index = Some(1);

    handler(Key::Enter, &mut app);

    // The screen opens on the press itself, not on response arrival.
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(id, 0) => assert_eq!(id, "37i9dQZF1DXcBWIGoYBM5M"),
      _ => panic!("expected playlist page fetch"),
    }

    // Pressing Enter again while the same open is in flight (after navigating
    // back to the sidebar) re-opens the screen but dispatches no duplicate
    // fetch.
    app.pop_navigation_stack();
    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn enter_on_row_zero_opens_add_playlist_form() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // Row 0 is always the "+ Add Playlist" entry.
    app.view.selected_playlist_index = Some(0);

    handler(Key::Enter, &mut app);

    assert_eq!(app.get_current_route().id, RouteId::CreatePlaylist);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn enter_on_community_pin_opens_community_playlist() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // No real playlists: the pin is the only display item, at sidebar row 1
    // (row 0 is "+ Add Playlist").
    assert!(app.community_pin_visible());
    app.view.selected_playlist_index = Some(1);

    handler(Key::Enter, &mut app);

    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    // The pin is not a real sidebar selection.
    assert_eq!(app.view.active_playlist_index, None);
    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(id, 0) => {
        assert_eq!(id, crate::core::app::COMMUNITY_PLAYLIST_ID)
      }
      _ => panic!("expected community playlist fetch"),
    }
  }

  #[test]
  fn shift_d_on_community_pin_is_a_no_op() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert!(app.community_pin_visible());
    // Sidebar row 1 is the pin (row 0 is "+ Add Playlist").
    app.view.selected_playlist_index = Some(1);
    let route_before = app.get_current_route().id.clone();

    handler(Key::Char('D'), &mut app);

    assert_eq!(app.get_current_route().id, route_before);
    assert!(app.view.dialog.is_none());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn enter_on_local_folder_dispatches_get_local_tracks() {
    use crate::core::plugin_api::PlaylistInfo;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.active_source = Source::Local;
    *app.local_playlists_mut() = vec![PlaylistInfo {
      generated_name: None,
      uri: "file:///music/Jazz".to_string(),
      name: "Jazz".to_string(),
      owner: "local".to_string(),
      track_count: 0,
      id: None,
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: None,
    }];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::GetLocalTracks(uri) => assert_eq!(uri, "file:///music/Jazz"),
      _ => panic!("expected local tracks fetch"),
    }
  }

  #[test]
  fn shift_d_on_radio_station_removes_favorite_and_updates_sidebar() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    app.state_path = Some(dir.path().join("state.yml"));
    app.runtime_state.radio_stations = vec![
      RadioStationConfig {
        name: "Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      },
      RadioStationConfig {
        name: "Secret Agent".to_string(),
        url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
      },
    ];

    app.active_source = Source::Radio;
    *app.radio_stations_mut() = vec![
      TrackInfo {
        generated_name: None,
        uri: Some("radio:https://ice1.somafm.com/groovesalad-128-mp3".to_string()),
        name: "Groove Salad".to_string(),
        artists: vec![],
        album: String::new(),
        duration_ms: 0,
        id: None,
        album_id: None,
        artist_refs: vec![],
        is_playable: true,
        is_local: false,
        track_number: 0,
        explicit: false,
        image_url: None,
        release_date: None,
      },
      TrackInfo {
        generated_name: None,
        uri: Some("radio:https://ice1.somafm.com/secretagent-128-mp3".to_string()),
        name: "Secret Agent".to_string(),
        artists: vec![],
        album: String::new(),
        duration_ms: 0,
        id: None,
        album_id: None,
        artist_refs: vec![],
        is_playable: true,
        is_local: false,
        track_number: 0,
        explicit: false,
        image_url: None,
        release_date: None,
      },
    ];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert_eq!(app.runtime_state.radio_stations.len(), 1);
    assert_eq!(
      app.runtime_state.radio_stations[0].url,
      "https://ice1.somafm.com/secretagent-128-mp3"
    );
    assert_eq!(app.radio_stations().len(), 1);
    assert_eq!(app.radio_stations()[0].name, "Secret Agent");
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message(),
      Some("Removed saved radio station: Groove Salad")
    );
  }

  #[test]
  fn shift_d_on_config_only_radio_station_reports_config_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    config.behavior.radio_stations = vec![RadioStationConfig {
      name: "Configured Groove".to_string(),
      url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
    }];
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    app.state_path = Some(dir.path().join("state.yml"));
    app.active_source = Source::Radio;
    *app.radio_stations_mut() = vec![radio_station_row(
      "Configured Groove",
      "https://ice1.somafm.com/groovesalad-128-mp3",
    )];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert!(app.runtime_state.radio_stations.is_empty());
    assert_eq!(app.radio_stations().len(), 1);
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message(),
      Some("Radio station is configured in config.yml: Configured Groove")
    );
  }

  #[test]
  fn shift_d_on_config_and_state_radio_station_removes_saved_copy_only() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    config.behavior.radio_stations = vec![RadioStationConfig {
      name: "Configured Groove".to_string(),
      url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
    }];
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    app.state_path = Some(dir.path().join("state.yml"));
    app.runtime_state.radio_stations = vec![RadioStationConfig {
      name: "Runtime Duplicate".to_string(),
      url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
    }];
    app.active_source = Source::Radio;
    *app.radio_stations_mut() = vec![radio_station_row(
      "Configured Groove",
      "https://ice1.somafm.com/groovesalad-128-mp3",
    )];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert!(app.runtime_state.radio_stations.is_empty());
    assert_eq!(app.radio_stations().len(), 1);
    assert_eq!(app.radio_stations()[0].name, "Configured Groove");
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message(),
      Some("Removed saved radio station: Runtime Duplicate")
    );
  }

  #[test]
  fn m_on_a_qobuz_playlist_opens_the_mirror_picker() {
    use crate::core::plugin_api::PlaylistInfo;
    let (tx, _rx) = channel();
    let mut app =
      App::new(tx, UserConfig::new(), Some(SystemTime::now())).under_source(Source::Qobuz);
    app.qobuz_playlists_mut().push(PlaylistInfo {
      generated_name: None,
      uri: "qobuz:playlist:9".to_string(),
      name: "Mine".to_string(),
      owner: "qobuz".to_string(),
      track_count: 3,
      id: Some("9".to_string()),
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: None,
    });
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('m'), &mut app);

    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::PlaylistSyncPicker)
    );
  }

  #[test]
  fn opening_a_playlist_row_remembers_the_playlist_panel() {
    let mut app = App::default();
    app.view.last_sidebar_block = SidebarBlock::Library;
    activate_selected(&mut app);
    assert_eq!(app.view.last_sidebar_block, SidebarBlock::Playlists);
  }
}
