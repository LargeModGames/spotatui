use super::common_key_events;
use crate::core::app::{ActiveBlock, App, RouteId, TrackTableContext};
use crate::infra::network::IoEvent;
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
      app.local_playlists_index = common_key_events::on_down_press_handler(
        &app.local_playlists,
        Some(app.local_playlists_index),
      );
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      app.local_playlists_index = common_key_events::on_up_press_handler(
        &app.local_playlists,
        Some(app.local_playlists_index),
      );
    }
    k if common_key_events::high_event(k) => {
      app.local_playlists_index = common_key_events::on_high_press_handler();
    }
    k if common_key_events::middle_event(k) => {
      app.local_playlists_index = common_key_events::on_middle_press_handler(&app.local_playlists);
    }
    k if common_key_events::low_event(k) => {
      app.local_playlists_index = common_key_events::on_low_press_handler(&app.local_playlists);
    }
    Key::Enter => {
      if let Some(folder) = app.local_playlists.get(app.local_playlists_index) {
        let uri = folder.uri.clone();
        // Reset the shared track table and mark it local so selecting a row
        // dispatches a `file://` play; the fetch fills in the rows.
        app.track_table.tracks = Vec::new();
        app.track_table.selected_index = 0;
        app.track_table.context = Some(TrackTableContext::LocalPlaylist);
        app.dispatch(IoEvent::GetLocalTracks(uri));
        app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      }
    }
    _ => {}
  }
}
