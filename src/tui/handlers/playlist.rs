use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::{ActiveBlock, RouteId};
use crate::core::app::{App, DialogContext, PlaylistFolderItem};
use crate::core::source::Source;
use crate::tui::event::Key;

/// Total rows in the sidebar Playlists panel. For Spotify this is the leading
/// "+ Add Playlist" row + playlists/folders; for Local it is the folder count
/// (no write capability, so no "Add Playlist").
pub(crate) fn total_display_count(app: &App) -> usize {
  match app.active_source {
    Source::Local => app.local_playlists.len(),
    Source::Subsonic => app.subsonic_playlists.len(),
    Source::Radio => app.radio_stations.len(),
    // Local YouTube playlists + the "+ New Playlist" entry.
    Source::YouTube => app.youtube_playlists.len() + 1,
    Source::Spotify => app.get_playlist_display_count() + 1,
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
    .and_then(|idx| app.local_playlists.get(idx))
    .map(|folder| folder.uri.clone());
  open_source_playlist(app, uri);
}

/// Subsonic: open the highlighted server playlist's tracks in the shared track
/// table.
fn open_subsonic_folder(app: &mut App) {
  let uri = app
    .view
    .selected_playlist_index
    .and_then(|idx| app.subsonic_playlists.get(idx))
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
  if idx == app.youtube_playlists.len() {
    // "+ New Playlist" — reuse the create form; its name stage dispatches
    // CreateYouTubePlaylist under the YouTube source.
    app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
    return;
  }
  let uri = app
    .youtube_playlists
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
  let uri = app.radio_stations.get(idx).and_then(|s| s.uri.clone());
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
    app.set_status_message("No radio station selected".to_string(), 4);
    return;
  };
  let uri = match app.radio_stations.get(idx) {
    None => {
      app.set_status_message("No radio station selected".to_string(), 4);
      return;
    }
    Some(station) => match station.uri.clone() {
      None => {
        app.set_status_message("Radio station has no stream URL".to_string(), 4);
        return;
      }
      Some(uri) => uri,
    },
  };
  app.apply(Action::RemoveRadioStation(uri));
  // The clamp is a no-op for every outcome that leaves the list untouched.
  app.view.selected_playlist_index = if app.radio_stations.is_empty() {
    None
  } else {
    Some(idx.min(app.radio_stations.len() - 1))
  };
}

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::right_event(k, &app.user_config.keys) => {
      common_key_events::handle_right_event(app)
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
    Key::Enter => activate_selected(app),
    Key::Char('D') if app.active_source == Source::Radio => {
      remove_radio_station(app);
    }
    // Deleting a local YouTube playlist: same confirm dialog UX as Spotify.
    Key::Char('D') if app.active_source == Source::YouTube => {
      if let Some(playlist) = app
        .view
        .selected_playlist_index
        .and_then(|idx| app.youtube_playlists.get(idx))
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
          if let Some(playlist) = app.all_playlists.get(*index) {
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
  match app.active_source {
    Source::Local => open_local_folder(app),
    Source::Subsonic => open_subsonic_folder(app),
    Source::Radio => play_radio_station(app),
    Source::YouTube => open_youtube_playlist(app),
    Source::Spotify => open_spotify_row(app),
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
          let id_str = app.all_playlists.get(index).and_then(|p| p.id.clone());
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
    }
  }

  #[test]
  fn enter_playlist_dispatches_only_visible_page_load() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // Exercise a real playlist (row 1, below the leading "+ Add Playlist" row),
    // not the pinned community entry.
    app.user_config.behavior.pin_community_playlist = false;
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Test Playlist",
      "spotatui-test-user",
      false,
    )];
    app.playlist_folder_items = vec![PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 0,
    }];
    app.view.selected_playlist_index = Some(1);

    handler(Key::Enter, &mut app);

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
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Test Playlist",
      "spotatui-test-user",
      false,
    )];
    app.playlist_folder_items = vec![PlaylistFolderItem::Playlist {
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
    app.local_playlists = vec![PlaylistInfo {
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
    app.radio_stations = vec![
      TrackInfo {
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
      },
      TrackInfo {
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
      },
    ];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert_eq!(app.runtime_state.radio_stations.len(), 1);
    assert_eq!(
      app.runtime_state.radio_stations[0].url,
      "https://ice1.somafm.com/secretagent-128-mp3"
    );
    assert_eq!(app.radio_stations.len(), 1);
    assert_eq!(app.radio_stations[0].name, "Secret Agent");
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message.as_deref(),
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
    app.radio_stations = vec![radio_station_row(
      "Configured Groove",
      "https://ice1.somafm.com/groovesalad-128-mp3",
    )];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert!(app.runtime_state.radio_stations.is_empty());
    assert_eq!(app.radio_stations.len(), 1);
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message.as_deref(),
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
    app.radio_stations = vec![radio_station_row(
      "Configured Groove",
      "https://ice1.somafm.com/groovesalad-128-mp3",
    )];
    app.view.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert!(app.runtime_state.radio_stations.is_empty());
    assert_eq!(app.radio_stations.len(), 1);
    assert_eq!(app.radio_stations[0].name, "Configured Groove");
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message.as_deref(),
      Some("Removed saved radio station: Runtime Duplicate")
    );
  }
}
