use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(albums) = &mut app.library.saved_albums.get_results(None) {
        let next_index =
          common_key_events::on_down_press_handler(&albums.items, Some(app.view.album_list_index));
        app.view.album_list_index = next_index;
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(albums) = &mut app.library.saved_albums.get_results(None) {
        let next_index =
          common_key_events::on_up_press_handler(&albums.items, Some(app.view.album_list_index));
        app.view.album_list_index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if let Some(_albums) = app.library.saved_albums.get_results(None) {
        let next_index = common_key_events::on_high_press_handler();
        app.view.album_list_index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if let Some(albums) = app.library.saved_albums.get_results(None) {
        let next_index = common_key_events::on_middle_press_handler(&albums.items);
        app.view.album_list_index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if let Some(albums) = app.library.saved_albums.get_results(None) {
        let next_index = common_key_events::on_low_press_handler(&albums.items);
        app.view.album_list_index = next_index;
      }
    }
    Key::Enter => {
      if let Some(id) = selected_saved_album_id(app) {
        app.apply(Action::Open(OpenTarget::SavedAlbum(id)));
      }
    }
    k if k == app.user_config.keys.next_page => app.get_current_user_saved_albums_next(),
    k if k == app.user_config.keys.previous_page => app.get_current_user_saved_albums_previous(),
    Key::Char('D') => {
      if let Some(id) = selected_saved_album_id(app) {
        app.apply(Action::UnsaveAlbum(id));
      }
    }
    // Open sort menu
    Key::Char(',') => {
      super::sort_menu::open_sort_menu(app, crate::core::sort::SortContext::SavedAlbums);
    }
    _ => {}
  };
}

/// The album id under the list cursor.
fn selected_saved_album_id(app: &App) -> Option<String> {
  app
    .library
    .saved_albums
    .get_results(None)?
    .items
    .get(app.view.album_list_index)?
    .album
    .id
    .clone()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::{AlbumInfo, SavedAlbumInfo, TrackInfo};
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn track(name: &str) -> TrackInfo {
    TrackInfo {
      uri: None,
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1000,
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

  fn app_with_saved_album(
    cached_tracks: usize,
    total_tracks: u32,
  ) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    app_with_saved_album_id(Some("longalbum".to_string()), cached_tracks, total_tracks)
  }

  fn app_with_saved_album_id(
    album_id: Option<String>,
    cached_tracks: usize,
    total_tracks: u32,
  ) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let album = AlbumInfo {
      id: album_id,
      uri: Some("spotify:album:longalbum".to_string()),
      name: "One Wayne G".to_string(),
      total_tracks: Some(total_tracks),
      tracks: (0..cached_tracks)
        .map(|i| track(&format!("t{}", i)))
        .collect(),
      ..AlbumInfo::default()
    };
    app.library.saved_albums.upsert_page_by_offset(Paged {
      items: vec![SavedAlbumInfo {
        album,
        added_at: String::new(),
      }],
      offset: 0,
      limit: 1,
      total: 1,
      next: None,
      previous: None,
    });
    (app, rx)
  }

  #[test]
  fn enter_on_saved_album_with_complete_tracklist_uses_cache() {
    let (mut app, rx) = app_with_saved_album(2, 2);

    handler(Key::Enter, &mut app);

    assert!(app.selected_album_full.is_some());
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::AlbumTracks
    );
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn enter_on_saved_album_with_truncated_tracklist_refetches_full_album() {
    let (mut app, rx) = app_with_saved_album(50, 199);

    handler(Key::Enter, &mut app);

    // The cached 50-track page must not be rendered; GetAlbum fetches the
    // complete tracklist and pushes the AlbumTracks route itself.
    assert!(app.selected_album_full.is_none());
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::AlbumTracks
    );
    match rx.recv().unwrap() {
      IoEvent::GetAlbum(id) => assert_eq!(id, "longalbum"),
      _ => panic!("expected GetAlbum"),
    }
  }

  #[test]
  fn enter_on_a_saved_album_without_an_id_opens_nothing() {
    // An id-less row cannot be opened by id; every Web API album has one.
    let (mut app, rx) = app_with_saved_album_id(None, 50, 199);

    handler(Key::Enter, &mut app);

    assert!(app.selected_album_full.is_none());
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::AlbumTracks
    );
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(
      Some(ActiveBlock::AlbumTracks),
      Some(ActiveBlock::AlbumTracks),
    );

    handler(Key::Left, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::Library);
  }

  #[test]
  fn on_esc() {
    let mut app = App::default();

    handler(Key::Esc, &mut app);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
  }
}
