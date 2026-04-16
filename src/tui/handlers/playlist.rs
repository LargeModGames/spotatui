use super::common_key_events;
use crate::core::app::{ActiveBlock, RouteId};
use crate::core::app::{App, DialogContext, PlaylistFolderItem, TrackTableContext};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

/// Total items = playlists/folders + the "Add Playlist" entry at the bottom
fn total_display_count(app: &App) -> usize {
  app.get_playlist_display_count() + 1
}

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::right_event(k) => common_key_events::handle_right_event(app),
    k if common_key_events::down_event(k) => {
      let count = total_display_count(app);
      if count > 0 {
        let current = app.selected_playlist_index.unwrap_or(0);
        app.selected_playlist_index = Some((current + 1) % count);
      }
    }
    k if common_key_events::up_event(k) => {
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
              // Open the playlist tracks
              if let Some(playlist) = app.all_playlists.get(*index) {
                app.active_playlist_index = Some(*index);
                let playlist_id = playlist.id.clone().into_static();
                app.reset_playlist_tracks_view(playlist_id.clone(), TrackTableContext::MyPlaylists);
                app.dispatch(IoEvent::GetPlaylistItems(
                  playlist_id.clone(),
                  app.playlist_offset,
                ));
              }
            }
          }
        }
      }
    }
    Key::Char('D') => {
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
  use crate::core::user_config::UserConfig;
  use rspotify::model::{
    idtypes::{PlaylistId, UserId},
    playlist::PlaylistTracksRef,
    SimplifiedPlaylist,
  };
  use std::collections::HashMap;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  #[allow(deprecated)]
  fn test_playlist(id: &str, name: &str) -> SimplifiedPlaylist {
    let tracks = PlaylistTracksRef {
      href: "https://example.com/playlist/tracks".to_string(),
      total: 2,
    };
    SimplifiedPlaylist {
      collaborative: false,
      external_urls: HashMap::new(),
      href: format!("https://api.spotify.com/v1/playlists/{id}"),
      id: PlaylistId::from_id(id).unwrap().into_static(),
      images: Vec::new(),
      name: name.to_string(),
      owner: rspotify::model::PublicUser {
        display_name: Some("tester".to_string()),
        external_urls: HashMap::new(),
        followers: None,
        href: "https://api.spotify.com/v1/users/spotatui-test-user".to_string(),
        id: UserId::from_id("spotatui-test-user").unwrap().into_static(),
        images: Vec::new(),
      },
      public: Some(false),
      snapshot_id: "snapshot".to_string(),
      tracks: tracks.clone(),
      items: tracks,
    }
  }

  #[test]
  fn enter_playlist_dispatches_only_visible_page_load() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.all_playlists = vec![test_playlist("37i9dQZF1DXcBWIGoYBM5M", "Test Playlist")];
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
}
