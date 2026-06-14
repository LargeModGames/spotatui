use crate::core::app::{ActiveBlock, App, RouteId};
use crate::core::user_config::KeyBindings;
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
    RouteId::Artists => Some(ActiveBlock::Artists),
    RouteId::RecentlyPlayed => Some(ActiveBlock::RecentlyPlayed),
    RouteId::Search => Some(ActiveBlock::SearchResultBlock),
    RouteId::Artist => Some(ActiveBlock::ArtistBlock),
    RouteId::Home => Some(ActiveBlock::Home),
    _ => None,
  }
}

pub fn handle_right_event(app: &mut App) {
  match app.get_current_route().hovered_block {
    ActiveBlock::MyPlaylists | ActiveBlock::Library => {
      if let Some(active_block) = content_active_block_for_route(&app.get_current_route().id) {
        app.set_current_route_state(Some(active_block), Some(active_block));
      }
    }
    _ => {}
  };
}

pub fn handle_left_event(app: &mut App) {
  // TODO: This should send you back to either library or playlist based on last selection
  app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));
}

#[cfg(test)]
mod tests {
  use super::*;

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
}
