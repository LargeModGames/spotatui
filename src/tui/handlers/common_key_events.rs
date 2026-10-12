use crate::core::app::{ActiveBlock, App, PlaybarOrigin, RouteId};
use crate::core::user_config::{KeyBindings, UserConfig};
use crate::tui::event::Key;

pub fn down_event(key: Key, keys: &KeyBindings) -> bool {
  matches!(key, Key::Down | Key::Ctrl('n')) || key == keys.move_down
}

pub fn up_event(key: Key, keys: &KeyBindings) -> bool {
  matches!(key, Key::Up | Key::Ctrl('p')) || key == keys.move_up
}

pub fn left_event(key: Key, keys: &KeyBindings) -> bool {
  matches!(key, Key::Left | Key::Ctrl('b')) || key == keys.move_left
}

pub fn right_event(key: Key, keys: &KeyBindings) -> bool {
  matches!(key, Key::Right | Key::Ctrl('f')) || key == keys.move_right
}

pub fn high_event(key: Key) -> bool {
  matches!(key, Key::Char('H'))
}

pub fn middle_event(key: Key) -> bool {
  matches!(key, Key::Char('M'))
}

pub fn low_event(key: Key) -> bool {
  matches!(key, Key::Char('L'))
}

/// Rows without a URI are skipped, so the offset is recomputed over the kept rows.
pub fn uri_playback_request(
  uris: impl Iterator<Item = Option<String>>,
  selected_index: usize,
) -> (Vec<String>, Option<usize>) {
  let mut kept = Vec::with_capacity(uris.size_hint().0);
  let mut offset = None;
  for (row_index, uri) in uris.enumerate() {
    if let Some(uri) = uri {
      if row_index == selected_index {
        offset = Some(kept.len());
      }
      kept.push(uri);
    }
  }
  (kept, offset)
}

pub fn on_down_press_handler<T>(selection_data: &[T], selection_index: Option<usize>) -> usize {
  match selection_index {
    Some(selection_index) => {
      if !selection_data.is_empty() {
        let next_index = selection_index + 1;
        if next_index > selection_data.len() - 1 {
          return 0;
        } else {
          return next_index;
        }
      }
      0
    }
    None => 0,
  }
}

pub fn on_up_press_handler<T>(selection_data: &[T], selection_index: Option<usize>) -> usize {
  match selection_index {
    Some(selection_index) => {
      if !selection_data.is_empty() {
        if selection_index > 0 {
          return selection_index - 1;
        } else {
          return selection_data.len() - 1;
        }
      }
      0
    }
    None => 0,
  }
}

pub fn on_high_press_handler() -> usize {
  0
}

pub fn on_middle_press_handler<T>(selection_data: &[T]) -> usize {
  if selection_data.is_empty() {
    return 0;
  }

  let mut index = selection_data.len() / 2;
  if selection_data.len().is_multiple_of(2) {
    index -= 1;
  }
  index
}

pub fn on_low_press_handler<T>(selection_data: &[T]) -> usize {
  selection_data.len().saturating_sub(1)
}

pub fn content_active_block_for_route(route_id: &RouteId) -> Option<ActiveBlock> {
  match route_id {
    RouteId::AlbumTracks => Some(ActiveBlock::AlbumTracks),
    RouteId::TrackTable | RouteId::Recommendations => Some(ActiveBlock::TrackTable),
    RouteId::Podcasts => Some(ActiveBlock::Podcasts),
    RouteId::AlbumList => Some(ActiveBlock::AlbumList),
    RouteId::PodcastEpisodes => Some(ActiveBlock::EpisodeTable),
    RouteId::Discover => Some(ActiveBlock::Discover),
    RouteId::LocalBrowser => Some(ActiveBlock::LocalBrowser),
    RouteId::Stats => Some(ActiveBlock::Stats),
    RouteId::PlaylistSync => Some(ActiveBlock::PlaylistSync),
    // Without this the right-arrow from the sidebar silently does nothing: the
    // match has a `_ => None` fallthrough, so it compiles either way.
    #[cfg(feature = "ai-dj")]
    RouteId::AiDj => Some(ActiveBlock::AiDj),
    RouteId::Artists => Some(ActiveBlock::Artists),
    RouteId::RecentlyPlayed => Some(ActiveBlock::RecentlyPlayed),
    RouteId::Search => Some(ActiveBlock::SearchResultBlock),
    RouteId::Artist => Some(ActiveBlock::ArtistBlock),
    RouteId::Home => Some(ActiveBlock::Home),
    _ => None,
  }
}

pub fn sidebar_on_right(config: &UserConfig) -> bool {
  config.behavior.sidebar_position == "right"
}

pub fn toward_sidebar_event(key: Key, config: &UserConfig) -> bool {
  let keys = &config.keys;
  if sidebar_on_right(config) {
    right_event(key, keys)
  } else {
    left_event(key, keys)
  }
}

pub fn toward_content_event(key: Key, config: &UserConfig) -> bool {
  let keys = &config.keys;
  if sidebar_on_right(config) {
    left_event(key, keys)
  } else {
    right_event(key, keys)
  }
}

pub fn focus_sidebar(app: &mut App) {
  let target = app.view.last_sidebar_block.into(); // Library or MyPlaylists
  app.set_current_route_state(Some(ActiveBlock::Empty), Some(target));
}

pub fn focus_content(app: &mut App) {
  let hovered = app.get_current_route().hovered_block;
  if matches!(hovered, ActiveBlock::Library | ActiveBlock::MyPlaylists) {
    remember_sidebar_block(app, hovered);
    if let Some(block) = content_active_block_for_route(&app.get_current_route().id) {
      app.set_current_route_state(Some(block), Some(block));
    }
  }
}

pub fn remember_sidebar_block(app: &mut App, block: ActiveBlock) {
  if matches!(block, ActiveBlock::Library | ActiveBlock::MyPlaylists) {
    app.view.last_sidebar_block = block.into();
  }
}

pub fn playbar_on_top(config: &UserConfig) -> bool {
  config.behavior.playbar_position == "top"
}

pub fn toward_playbar_event(key: Key, config: &UserConfig) -> bool {
  let keys = &config.keys;
  if playbar_on_top(config) {
    up_event(key, keys)
  } else {
    down_event(key, keys)
  }
}

pub fn away_from_playbar_event(key: Key, config: &UserConfig) -> bool {
  let keys = &config.keys;
  if playbar_on_top(config) {
    down_event(key, keys)
  } else {
    up_event(key, keys)
  }
}

/// Unfocused content plus the sidebar panel that actually touches the playbar.
pub fn block_borders_playbar(block: ActiveBlock, config: &UserConfig) -> bool {
  match block {
    ActiveBlock::Library => playbar_on_top(config),
    ActiveBlock::MyPlaylists => !playbar_on_top(config),
    ActiveBlock::ArtistBlock
    | ActiveBlock::AlbumList
    | ActiveBlock::AlbumTracks
    | ActiveBlock::Artists
    | ActiveBlock::Podcasts
    | ActiveBlock::EpisodeTable
    | ActiveBlock::Home
    | ActiveBlock::Discover
    | ActiveBlock::RecentlyPlayed
    | ActiveBlock::TrackTable
    | ActiveBlock::Stats
    | ActiveBlock::PlaylistSync
    | ActiveBlock::LocalBrowser => true,
    _ => false,
  }
}

pub fn remember_playbar_origin(app: &mut App) {
  let route = app.get_current_route();
  let origin = if !matches!(
    route.active_block,
    ActiveBlock::Empty | ActiveBlock::PlayBar
  ) {
    PlaybarOrigin {
      block: route.active_block,
      was_focused: true,
    }
  } else if route.hovered_block != ActiveBlock::PlayBar {
    PlaybarOrigin {
      block: route.hovered_block,
      was_focused: false,
    }
  } else {
    return;
  };
  app.view.last_playbar_origin = Some(origin);
}

pub fn hover_playbar(app: &mut App) {
  remember_playbar_origin(app);
  app.set_current_route_state(None, Some(ActiveBlock::PlayBar));
}

pub fn leave_playbar(app: &mut App) {
  let origin = app.view.last_playbar_origin.unwrap_or(PlaybarOrigin {
    block: if playbar_on_top(&app.user_config) {
      ActiveBlock::Library
    } else {
      ActiveBlock::MyPlaylists
    },
    was_focused: false,
  });
  let target = match origin.block {
    ActiveBlock::Library | ActiveBlock::MyPlaylists | ActiveBlock::Empty => {
      if playbar_on_top(&app.user_config) {
        ActiveBlock::Library
      } else {
        ActiveBlock::MyPlaylists
      }
    }
    block => block,
  };
  let active = if origin.was_focused {
    target
  } else {
    ActiveBlock::Empty
  };
  app.set_current_route_state(Some(active), Some(target));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{app::SidebarBlock, source::Source};

  #[test]
  fn the_playlist_sync_route_focuses_its_block_from_the_sidebar() {
    assert_eq!(
      content_active_block_for_route(&RouteId::PlaylistSync),
      Some(ActiveBlock::PlaylistSync)
    );
  }

  #[test]
  fn the_local_files_route_focuses_its_block_from_the_sidebar() {
    assert_eq!(
      content_active_block_for_route(&RouteId::LocalBrowser),
      Some(ActiveBlock::LocalBrowser)
    );
  }

  #[test]
  fn test_on_down_press_handler() {
    let data = vec!["Choice 1", "Choice 2", "Choice 3"];

    let index = 0;
    let next_index = on_down_press_handler(&data, Some(index));

    assert_eq!(next_index, 1);

    // Selection wrap if on last item
    let index = data.len() - 1;
    let next_index = on_down_press_handler(&data, Some(index));
    assert_eq!(next_index, 0);
  }

  #[test]
  fn test_on_up_press_handler() {
    let data = vec!["Choice 1", "Choice 2", "Choice 3"];

    let index = data.len() - 1;
    let next_index = on_up_press_handler(&data, Some(index));

    assert_eq!(next_index, index - 1);

    // Selection wrap if on first item
    let index = 0;
    let next_index = on_up_press_handler(&data, Some(index));
    assert_eq!(next_index, data.len() - 1);
  }

  #[test]
  fn test_on_middle_press_handler_empty() {
    let data: Vec<&str> = vec![];
    let next_index = on_middle_press_handler(&data);
    assert_eq!(next_index, 0);
  }

  #[test]
  fn test_on_low_press_handler_empty() {
    let data: Vec<&str> = vec![];
    let next_index = on_low_press_handler(&data);
    assert_eq!(next_index, 0);
  }

  #[test]
  fn uri_playback_request_skips_uri_less_rows_and_remaps_the_offset() {
    let rows = [Some("spotify:track:one"), None, Some("spotify:track:three")];

    let (uris, offset) = uri_playback_request(rows.iter().map(|uri| uri.map(str::to_string)), 2);

    assert_eq!(uris, vec!["spotify:track:one", "spotify:track:three"]);
    assert_eq!(offset, Some(1), "the skipped row must not shift the offset");
  }

  #[test]
  fn uri_playback_request_leaves_the_offset_unset_for_a_uri_less_selection() {
    let rows = [Some("spotify:track:one"), None];

    let (uris, offset) = uri_playback_request(rows.iter().map(|uri| uri.map(str::to_string)), 1);

    assert_eq!(uris, vec!["spotify:track:one"]);
    assert_eq!(offset, None);
  }

  #[test]
  fn left_event_targets_library_for_spotify_and_playlists_for_local() {
    let mut app = App::default();
    // Spotify (default): left arrow focuses the Library list.
    app.set_current_route_state(Some(ActiveBlock::TrackTable), Some(ActiveBlock::TrackTable));
    focus_sidebar(&mut app);
    assert_eq!(app.get_current_route().hovered_block, ActiveBlock::Library);

    // Local: the Library list keeps its free rows, so it stays the top block.
    app.active_source = Source::Local;
    app.set_current_route_state(Some(ActiveBlock::TrackTable), Some(ActiveBlock::TrackTable));
    focus_sidebar(&mut app);
    assert_eq!(app.get_current_route().hovered_block, ActiveBlock::Library);
  }

  #[test]
  fn the_sidebar_key_is_left_unless_the_sidebar_is_on_the_right() {
    for (position, sidebar_key, content_key) in [
      ("left", Key::Left, Key::Right),
      ("hidden", Key::Left, Key::Right),
      ("right", Key::Right, Key::Left),
    ] {
      let mut config = UserConfig::new();
      config.behavior.sidebar_position = position.to_string();

      assert!(toward_sidebar_event(sidebar_key, &config));
      assert!(!toward_sidebar_event(content_key, &config));
      assert!(toward_content_event(content_key, &config));
      assert!(!toward_content_event(sidebar_key, &config));
    }
  }

  #[test]
  fn returning_to_the_sidebar_goes_to_the_panel_it_left() {
    let mut app = App::default();
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::MyPlaylists));

    focus_content(&mut app);
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::TrackTable
    );

    focus_sidebar(&mut app);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Empty);
    assert_eq!(
      app.get_current_route().hovered_block,
      ActiveBlock::MyPlaylists
    );
  }

  #[test]
  fn a_content_screen_does_not_overwrite_the_remembered_sidebar_panel() {
    let mut app = App::default();
    app.view.last_sidebar_block = ActiveBlock::MyPlaylists.into();
    app.set_current_route_state(Some(ActiveBlock::TrackTable), Some(ActiveBlock::TrackTable));

    focus_content(&mut app);

    assert_eq!(app.view.last_sidebar_block, SidebarBlock::Playlists);
  }
}
