use super::common_key_events;
use crate::core::app::{ActiveBlock, App, RouteId, LIBRARY_OPTIONS};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::right_event(k, &app.user_config.keys) => {
      common_key_events::handle_right_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      let next_index = common_key_events::on_down_press_handler(
        &LIBRARY_OPTIONS,
        Some(app.library.selected_index),
      );
      app.library.selected_index = next_index;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      let next_index =
        common_key_events::on_up_press_handler(&LIBRARY_OPTIONS, Some(app.library.selected_index));
      app.library.selected_index = next_index;
    }
    k if common_key_events::high_event(k) => {
      let next_index = common_key_events::on_high_press_handler();
      app.library.selected_index = next_index;
    }
    k if common_key_events::middle_event(k) => {
      let next_index = common_key_events::on_middle_press_handler(&LIBRARY_OPTIONS);
      app.library.selected_index = next_index;
    }
    k if common_key_events::low_event(k) => {
      let next_index = common_key_events::on_low_press_handler(&LIBRARY_OPTIONS);
      app.library.selected_index = next_index
    }
    // `library` should probably be an array of structs with enums rather than just using indexes
    // like this
    Key::Enter => match app.library.selected_index {
      // Discover
      0 => {
        app.push_navigation_stack(RouteId::Discover, ActiveBlock::Discover);
      }
      // Recently Played
      1 => {
        app.dispatch(IoEvent::GetRecentlyPlayed);
        app.push_navigation_stack(RouteId::RecentlyPlayed, ActiveBlock::RecentlyPlayed);
      }
      // Friends
      2 => {
        app.push_navigation_stack(RouteId::Friends, ActiveBlock::Friends);
        // Load friend code + friends list on first open (or if empty)
        if app.friend_code.is_none() {
          app.dispatch(IoEvent::GetFriendCode);
        }
        if app.friends.is_empty() && !app.friends_loading {
          app.dispatch(IoEvent::GetFriends);
        }
        app.last_friends_refresh_at = std::time::Instant::now();
      }
      // Liked Songs
      3 => {
        app.reset_saved_tracks_view();
        app.dispatch(IoEvent::GetCurrentSavedTracks(None));
        app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      }
      // Albums
      4 => {
        app.dispatch(IoEvent::GetCurrentUserSavedAlbums(None));
        app.push_navigation_stack(RouteId::AlbumList, ActiveBlock::AlbumList);
      }
      // Artists
      5 => {
        app.dispatch(IoEvent::GetFollowedArtists(None));
        app.push_navigation_stack(RouteId::Artists, ActiveBlock::Artists);
      }
      // Podcasts
      6 => {
        app.dispatch(IoEvent::GetCurrentUserSavedShows(None));
        app.push_navigation_stack(RouteId::Podcasts, ActiveBlock::Podcasts);
      }
      // This is required because Rust can't tell if this pattern is exhaustive
      _ => {}
    },
    _ => (),
  };
}
