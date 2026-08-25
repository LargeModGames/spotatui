use super::common_key_events;
use crate::core::app::App;
use crate::tui::event::Key;

/// Handler for the Local Files folder browser: a list of folders (one per
/// subdirectory of the configured music directory). Enter opens a folder's
/// tracks in the shared track table.
pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      app.view.local_playlists_index = common_key_events::on_down_press_handler(
        &app.local_playlists,
        Some(app.view.local_playlists_index),
      );
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      app.view.local_playlists_index = common_key_events::on_up_press_handler(
        &app.local_playlists,
        Some(app.view.local_playlists_index),
      );
    }
    k if common_key_events::high_event(k) => {
      app.view.local_playlists_index = common_key_events::on_high_press_handler();
    }
    k if common_key_events::middle_event(k) => {
      app.view.local_playlists_index =
        common_key_events::on_middle_press_handler(&app.local_playlists);
    }
    k if common_key_events::low_event(k) => {
      app.view.local_playlists_index =
        common_key_events::on_low_press_handler(&app.local_playlists);
    }
    Key::Enter => {
      let uri = app
        .local_playlists
        .get(app.view.local_playlists_index)
        .map(|folder| folder.uri.clone());
      super::playlist::open_source_playlist(app, uri);
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::playlist_info;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  #[test]
  fn enter_opens_the_selected_folder_by_its_source_uri() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let mut first = playlist_info("first", "First", "me", false);
    first.uri = "file:///music/first".to_string();
    let mut second = playlist_info("second", "Second", "me", false);
    second.uri = "file:///music/second".to_string();
    app.local_playlists = vec![first, second];
    app.view.local_playlists_index = 1;

    handler(Key::Enter, &mut app);

    match rx.try_recv() {
      Ok(IoEvent::GetLocalTracks(uri)) => assert_eq!(uri, "file:///music/second"),
      _ => panic!("expected GetLocalTracks for the selected folder"),
    }
  }
}
