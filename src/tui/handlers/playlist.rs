use super::common_key_events;
use crate::core::app::{ActiveBlock, RouteId};
use crate::core::app::{App, DialogContext, PlaylistFolderItem, TrackTableContext};
use crate::core::source::Source;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::idtypes::PlaylistId;

/// Total items in the sidebar Playlists panel. For Spotify this is
/// playlists/folders + the "Add Playlist" entry; for Local it is the folder
/// count (no write capability, so no "Add Playlist").
fn total_display_count(app: &App) -> usize {
  match app.active_source {
    Source::Local => app.local_playlists.len(),
    Source::Subsonic => app.subsonic_playlists.len(),
    Source::Radio => app.radio_stations.len(),
    // Local YouTube playlists + the "+ New Playlist" entry.
    Source::YouTube => app.youtube_playlists.len() + 1,
    Source::Spotify => app.get_playlist_display_count() + 1,
  }
}

/// Local Files: open the highlighted folder's tracks in the shared track table.
fn open_local_folder(app: &mut App) {
  let Some(idx) = app.selected_playlist_index else {
    return;
  };
  if let Some(folder) = app.local_playlists.get(idx) {
    let uri = folder.uri.clone();
    app.track_table.tracks = Vec::new();
    app.track_table.selected_index = 0;
    app.track_table.context = Some(TrackTableContext::LocalPlaylist);
    app.dispatch(IoEvent::GetLocalTracks(uri));
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
  }
}

/// Subsonic: open the highlighted server playlist's tracks in the shared track
/// table.
fn open_subsonic_folder(app: &mut App) {
  let Some(idx) = app.selected_playlist_index else {
    return;
  };
  if let Some(playlist) = app.subsonic_playlists.get(idx) {
    let uri = playlist.uri.clone();
    app.track_table.tracks = Vec::new();
    app.track_table.selected_index = 0;
    app.track_table.context = Some(TrackTableContext::SubsonicPlaylist);
    app.dispatch(IoEvent::GetSubsonicTracks(uri));
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
  }
}

/// YouTube: open the highlighted local playlist's saved videos in the shared
/// track table, or the create-playlist form on the trailing "+ New Playlist"
/// entry.
fn open_youtube_playlist(app: &mut App) {
  let Some(idx) = app.selected_playlist_index else {
    return;
  };
  if idx == app.youtube_playlists.len() {
    // "+ New Playlist" — reuse the create form; its name stage dispatches
    // CreateYouTubePlaylist under the YouTube source.
    app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
    return;
  }
  if let Some(playlist) = app.youtube_playlists.get(idx) {
    let uri = playlist.uri.clone();
    app.track_table.tracks = Vec::new();
    app.track_table.selected_index = 0;
    app.track_table.context = Some(TrackTableContext::YouTubePlaylist);
    app.dispatch(IoEvent::GetYouTubeTracks(uri));
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
  }
}

/// Internet Radio: play the highlighted station directly. A station is a leaf,
/// not a container — there is no track list to drill into — so Enter starts the
/// stream instead of opening the track table.
fn play_radio_station(app: &mut App) {
  let Some(idx) = app.selected_playlist_index else {
    return;
  };
  if let Some(uri) = app.radio_stations.get(idx).and_then(|s| s.uri.clone()) {
    app.dispatch(IoEvent::StartPlayback(Some(uri), None, None));
  }
}

fn remove_radio_station(app: &mut App) {
  let Some(idx) = app.selected_playlist_index else {
    app.set_status_message("No radio station selected".to_string(), 4);
    return;
  };
  let Some(station) = app.radio_stations.get(idx).cloned() else {
    app.set_status_message("No radio station selected".to_string(), 4);
    return;
  };
  let Some(url) = station.uri.as_deref().and_then(super::radio_stream_url) else {
    app.set_status_message("Radio station has no stream URL".to_string(), 4);
    return;
  };

  match app.user_config.remove_radio_station_by_url(url) {
    Ok(Some(removed)) => {
      app.radio_stations.remove(idx);
      app.selected_playlist_index = if app.radio_stations.is_empty() {
        None
      } else {
        Some(idx.min(app.radio_stations.len() - 1))
      };
      app.set_status_message(format!("Removed radio station: {}", removed.name), 4);
    }
    Ok(None) => {
      app.set_status_message(
        format!("Radio station is not favorited: {}", station.name),
        4,
      );
    }
    Err(error) => {
      app.set_error_status_message(format!("Could not remove radio station: {error}"), 6);
    }
  }
}

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::right_event(k, &app.user_config.keys) => {
      common_key_events::handle_right_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      let count = total_display_count(app);
      if count > 0 {
        let current = app.selected_playlist_index.unwrap_or(0);
        app.selected_playlist_index = Some((current + 1) % count);
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      let count = total_display_count(app);
      if count > 0 {
        let current = app.selected_playlist_index.unwrap_or(0);
        app.selected_playlist_index = Some(if current == 0 { count - 1 } else { current - 1 });
      }
    }
    k if common_key_events::high_event(k) && total_display_count(app) > 0 => {
      app.selected_playlist_index = Some(0);
    }
    k if common_key_events::middle_event(k) => {
      let count = total_display_count(app);
      if count > 0 {
        let next_index = if count.is_multiple_of(2) {
          count.saturating_sub(1) / 2
        } else {
          count / 2
        };
        app.selected_playlist_index = Some(next_index);
      }
    }
    k if common_key_events::low_event(k) => {
      let count = total_display_count(app);
      if count > 0 {
        app.selected_playlist_index = Some(count - 1);
      }
    }
    Key::Enter if app.active_source == Source::Local => {
      open_local_folder(app);
    }
    Key::Enter if app.active_source == Source::Subsonic => {
      open_subsonic_folder(app);
    }
    Key::Enter if app.active_source == Source::Radio => {
      play_radio_station(app);
    }
    Key::Char('D') if app.active_source == Source::Radio => {
      remove_radio_station(app);
    }
    Key::Enter if app.active_source == Source::YouTube => {
      open_youtube_playlist(app);
    }
    // Deleting a local YouTube playlist: same confirm dialog UX as Spotify.
    Key::Char('D') if app.active_source == Source::YouTube => {
      if let Some(playlist) = app
        .selected_playlist_index
        .and_then(|idx| app.youtube_playlists.get(idx))
      {
        app.dialog = Some(playlist.name.clone());
        app.confirm = false;
        app.push_navigation_stack(
          RouteId::Dialog,
          ActiveBlock::Dialog(DialogContext::YouTubePlaylistWindow),
        );
      }
    }
    Key::Enter => {
      if let Some(selected_idx) = app.selected_playlist_index {
        let playlist_count = app.get_playlist_display_count();
        if selected_idx == playlist_count {
          // "Add Playlist" entry selected
          app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
        } else if let Some(item) = app.get_playlist_display_item_at(selected_idx) {
          match item {
            PlaylistFolderItem::Folder(folder) => {
              // Navigate into/out of folder
              app.current_playlist_folder_id = folder.target_id;
              app.selected_playlist_index = Some(0);
            }
            PlaylistFolderItem::Playlist { index, .. } => {
              // Open the playlist tracks: navigates immediately with the
              // cleared table as the loading state (see open_playlist_tracks).
              let index = *index;
              if let Some(id_str) = app.all_playlists.get(index).and_then(|p| p.id.clone()) {
                if let Ok(playlist_id) = PlaylistId::from_id(id_str.as_str()) {
                  app.active_playlist_index = Some(index);
                  app.open_playlist_tracks(
                    playlist_id.into_static(),
                    TrackTableContext::MyPlaylists,
                  );
                }
              }
            }
          }
        }
      }
    }
    // Deleting playlists is a Spotify-only (PlaylistWriter) action.
    Key::Char('D') if app.active_source == Source::Spotify => {
      if let Some(selected_idx) = app.selected_playlist_index {
        if let Some(PlaylistFolderItem::Playlist { index, .. }) =
          app.get_playlist_display_item_at(selected_idx)
        {
          if let Some(playlist) = app.all_playlists.get(*index) {
            let selected_playlist = &playlist.name;
            app.dialog = Some(selected_playlist.clone());
            app.confirm = false;

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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::TrackInfo;
  use crate::core::test_helpers::playlist_info;
  use crate::core::user_config::{RadioStationConfig, UserConfig, UserConfigPaths};
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  #[test]
  fn enter_playlist_dispatches_only_visible_page_load() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    app.selected_playlist_index = Some(0);

    handler(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(_, 0) => {}
      _ => panic!("expected playlist page fetch"),
    }

    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn enter_playlist_navigates_immediately_and_dedups_inflight_open() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    app.selected_playlist_index = Some(0);

    handler(Key::Enter, &mut app);

    // The screen opens on the press itself, not on response arrival.
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(_, 0) => {}
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
    app.selected_playlist_index = Some(0);

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
    config.behavior.radio_stations = vec![
      RadioStationConfig {
        name: "Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      },
      RadioStationConfig {
        name: "Secret Agent".to_string(),
        url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
      },
    ];

    let mut app = App::new(tx, config, Some(SystemTime::now()));
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
    app.selected_playlist_index = Some(0);

    handler(Key::Char('D'), &mut app);

    assert_eq!(app.user_config.behavior.radio_stations.len(), 1);
    assert_eq!(
      app.user_config.behavior.radio_stations[0].url,
      "https://ice1.somafm.com/secretagent-128-mp3"
    );
    assert_eq!(app.radio_stations.len(), 1);
    assert_eq!(app.radio_stations[0].name, "Secret Agent");
    assert_eq!(app.selected_playlist_index, Some(0));
    assert_eq!(
      app.status_message.as_deref(),
      Some("Removed radio station: Groove Salad")
    );
  }
}
