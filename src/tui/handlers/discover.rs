use super::common_key_events;
use crate::core::app::{ActiveBlock, App, RouteId, TrackTableContext};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

const DISCOVER_OPTIONS_COUNT: usize = 2;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      let next_index = if app.view.discover_selected_index >= DISCOVER_OPTIONS_COUNT - 1 {
        0
      } else {
        app.view.discover_selected_index + 1
      };
      app.view.discover_selected_index = next_index;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      let next_index = if app.view.discover_selected_index == 0 {
        DISCOVER_OPTIONS_COUNT - 1
      } else {
        app.view.discover_selected_index - 1
      };
      app.view.discover_selected_index = next_index;
    }
    // Left/Right to cycle time range (for Top Tracks)
    k if common_key_events::right_event(k, &app.user_config.keys)
      && app.view.discover_selected_index == 1 =>
    {
      // Only cycle time range when Top Tracks is selected
      app.view.discover_time_range = app.view.discover_time_range.next();
      // Clear cache so it refetches with new time range
      app.discover_top_tracks.clear();
    }
    Key::Char('[') if app.view.discover_selected_index == 1 => {
      app.view.discover_time_range = app.view.discover_time_range.prev();
      app.discover_top_tracks.clear();
    }
    Key::Char(']') if app.view.discover_selected_index == 1 => {
      app.view.discover_time_range = app.view.discover_time_range.next();
      app.discover_top_tracks.clear();
    }
    Key::Enter => {
      if app.discover_loading {
        return; // Don't process Enter while loading
      }
      match app.view.discover_selected_index {
        0 => {
          // Top Artists Mix
          if app.discover_artists_mix.is_empty() {
            app.dispatch(IoEvent::GetTopArtistsMix);
          } else {
            // Mix already loaded, show it
            app.track_table.tracks = app.discover_artists_mix.clone();
            app.track_table.context = Some(TrackTableContext::DiscoverPlaylist);
            app.track_table.selected_index = 0;
            app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
          }
        }
        1 => {
          // Top Tracks - always refetch if empty or if we want fresh data
          if app.discover_top_tracks.is_empty() {
            app.dispatch(IoEvent::GetUserTopTracks(app.view.discover_time_range));
          } else {
            // Tracks already loaded, show them
            app.track_table.tracks = app.discover_top_tracks.clone();
            app.track_table.context = Some(TrackTableContext::DiscoverPlaylist);
            app.track_table.selected_index = 0;
            app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
          }
        }
        _ => {}
      }
    }
    _ if key == app.user_config.keys.add_item_to_queue => {
      // Add the first track from the selected discover list to the queue.
      let track = match app.view.discover_selected_index {
        0 => app.discover_artists_mix.first().cloned(),
        1 => app.discover_top_tracks.first().cloned(),
        _ => None,
      };
      if let Some(track) = track {
        app.add_track_to_native_queue(track);
      }
    }
    _ => {}
  }
}
