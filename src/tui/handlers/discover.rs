use super::common_key_events;
use crate::core::app::{ActiveBlock, App, RouteId, TrackTableContext};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::PlayableId;

const DISCOVER_OPTIONS_COUNT: usize = 2;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k) => common_key_events::handle_left_event(app),
    k if common_key_events::down_event(k) => {
      let next_index = if app.discover_selected_index >= DISCOVER_OPTIONS_COUNT - 1 {
        0
      } else {
        app.discover_selected_index + 1
      };
      app.discover_selected_index = next_index;
    }
    k if common_key_events::up_event(k) => {
      let next_index = if app.discover_selected_index == 0 {
        DISCOVER_OPTIONS_COUNT - 1
      } else {
        app.discover_selected_index - 1
      };
      app.discover_selected_index = next_index;
    }
    // Left/Right to cycle time range (for Top Tracks)
    k if common_key_events::right_event(k) && app.discover_selected_index == 1 => {
      // Only cycle time range when Top Tracks is selected
      app.discover_time_range = app.discover_time_range.next();
      // Clear cache so it refetches with new time range
      app.discover_top_tracks.clear();
    }
    Key::Char('[') if app.discover_selected_index == 1 => {
      app.discover_time_range = app.discover_time_range.prev();
      app.discover_top_tracks.clear();
    }
    Key::Char(']') if app.discover_selected_index == 1 => {
      app.discover_time_range = app.discover_time_range.next();
      app.discover_top_tracks.clear();
    }
    Key::Enter => {
      if app.discover_loading {
        return; // Don't process Enter while loading
      }
      match app.discover_selected_index {
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
            app.dispatch(IoEvent::GetUserTopTracks(app.discover_time_range));
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
      // Add selected track from top tracks to queue if available
      let tracks = match app.discover_selected_index {
        0 => &app.discover_artists_mix,
        1 => &app.discover_top_tracks,
        _ => return,
      };
      if let Some(track) = tracks.first() {
        if let Some(track_id) = &track.id {
          app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
            track_id.clone_static(),
          )));
        }
      }
    }
    _ => {}
  }
}
