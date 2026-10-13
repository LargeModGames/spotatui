use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  let count = app
    .discover_shelves()
    .iter()
    .map(|shelf| shelf.playlists.len())
    .sum::<usize>();
  let index = app.view.discover_selected_index;
  let selected = app.discover_playlist(index).map(|p| p.uri.clone());
  match key {
    k if common_key_events::toward_sidebar_event(k, &app.user_config) => {
      common_key_events::focus_sidebar(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) && count > 0 => {
      app.view.discover_selected_index = (index + 1) % count;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) && count > 0 => {
      app.view.discover_selected_index = (index + count - 1) % count;
    }
    Key::Enter => {
      if let Some(id) = selected {
        app.apply(Action::Open(OpenTarget::Playlist {
          id,
          from_search: false,
        }));
      }
    }
    Key::Char('e') => {
      if let Some(uri) = selected {
        app.apply(Action::PlayContext { uri, offset: None });
      }
    }
    Key::Char('w') => {
      if let Some(uri) = selected {
        app.apply(Action::FollowPlaylist(uri));
      }
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use crate::core::test_helpers::discover_shelves;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use rspotify::prelude::Id;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn app_on_discover() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::Discover, ActiveBlock::Discover);
    app.end_discover_fetch(Some(discover_shelves()));
    (app, rx)
  }

  #[test]
  fn down_walks_across_shelves_and_wraps() {
    let (mut app, _rx) = app_on_discover();

    handler(Key::Down, &mut app);
    handler(Key::Down, &mut app);
    assert_eq!(
      app
        .discover_playlist(app.view.discover_selected_index)
        .map(|p| p.name.as_str()),
      Some("Release Radar")
    );

    handler(Key::Down, &mut app);
    assert_eq!(app.view.discover_selected_index, 0);
  }

  #[test]
  fn up_on_the_first_playlist_wraps_to_the_last() {
    let (mut app, _rx) = app_on_discover();

    handler(Key::Up, &mut app);

    assert_eq!(app.view.discover_selected_index, 2);
  }

  #[test]
  fn enter_opens_the_selected_playlist_in_the_track_table() {
    let (mut app, _rx) = app_on_discover();
    app.view.discover_selected_index = 1;

    handler(Key::Enter, &mut app);

    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    assert_eq!(
      app
        .current_playlist_track_table_id()
        .map(|id| id.id().to_string()),
      Some("37i9dQZF1E000000000002".to_string())
    );
  }

  #[test]
  fn keys_on_an_empty_page_do_nothing() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::Discover, ActiveBlock::Discover);

    for key in [
      Key::Down,
      Key::Up,
      Key::Enter,
      Key::Char('e'),
      Key::Char('w'),
    ] {
      handler(key, &mut app);
    }

    assert_eq!(app.view.discover_selected_index, 0);
    assert_eq!(app.get_current_route().id, RouteId::Discover);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn w_saves_the_selected_playlist_to_the_library() {
    let (mut app, rx) = app_on_discover();

    handler(Key::Char('w'), &mut app);

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::UserFollowPlaylist(_, uri, _)) if uri == "spotify:playlist:37i9dQZF1E000000000001"
    ));
  }
}
