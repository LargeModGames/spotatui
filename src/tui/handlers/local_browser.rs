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
