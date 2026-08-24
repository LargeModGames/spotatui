use super::common_key_events;
use crate::core::action::{Action, DiscoverTarget};
use crate::core::app::App;
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
      let target = match app.view.discover_selected_index {
        0 => DiscoverTarget::ArtistsMix,
        1 => DiscoverTarget::TopTracks(app.view.discover_time_range),
        _ => return,
      };
      app.apply(Action::OpenDiscover(target));
    }
    _ if key == app.user_config.keys.add_item_to_queue => {
      // Add the first track from the selected discover list to the queue.
      let track = match app.view.discover_selected_index {
        0 => app.discover_artists_mix.first().cloned(),
        1 => app.discover_top_tracks.first().cloned(),
        _ => None,
      };
      if let Some(track) = track {
        app.apply(Action::QueueTrack(track));
      }
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::DiscoverTimeRange;
  use crate::core::plugin_api::TrackInfo;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn app_with_channel() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    (app, rx)
  }

  fn top_track() -> TrackInfo {
    TrackInfo {
      uri: Some("spotify:track:one".to_string()),
      name: "One".to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1_000,
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
  fn enter_on_top_tracks_fetches_with_the_cycled_time_range() {
    let (mut app, rx) = app_with_channel();
    app.view.discover_selected_index = 1;

    handler(Key::Char(']'), &mut app);
    handler(Key::Enter, &mut app);

    assert_eq!(app.view.discover_time_range, DiscoverTimeRange::Long);
    assert!(
      matches!(
        rx.try_recv(),
        Ok(IoEvent::GetUserTopTracks(range)) if range == DiscoverTimeRange::Long
      ),
      "the empty cache triggered a fetch for the cycled range"
    );
  }

  #[test]
  fn cycling_the_time_range_invalidates_the_top_tracks_cache() {
    let (mut app, _rx) = app_with_channel();
    app.view.discover_selected_index = 1;
    app.discover_top_tracks = vec![top_track()];

    handler(Key::Char(']'), &mut app);

    assert_eq!(app.view.discover_time_range, DiscoverTimeRange::Long);
    assert!(
      app.discover_top_tracks.is_empty(),
      "a new time range invalidates the cached page"
    );
  }

  #[test]
  fn time_range_keys_do_nothing_on_the_artists_mix_row() {
    let (mut app, _rx) = app_with_channel();
    let before = app.view.discover_time_range;

    handler(Key::Char('['), &mut app);
    handler(Key::Char(']'), &mut app);

    assert_eq!(app.view.discover_selected_index, 0);
    assert_eq!(app.view.discover_time_range, before);
  }

  #[test]
  fn down_wraps_across_the_two_rows() {
    let (mut app, _rx) = app_with_channel();

    handler(Key::Down, &mut app);
    assert_eq!(app.view.discover_selected_index, 1);

    handler(Key::Down, &mut app);
    assert_eq!(app.view.discover_selected_index, 0);
  }
}
