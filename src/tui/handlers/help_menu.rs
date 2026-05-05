use super::common_key_events;
use crate::{core::app::App, tui::event::Key};

#[derive(PartialEq)]
enum Direction {
  Up,
  Down,
}

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::down_event(k) => {
      move_page(Direction::Down, app);
    }
    k if common_key_events::up_event(k) => {
      move_page(Direction::Up, app);
    }
    Key::Ctrl('d') => {
      move_page(Direction::Down, app);
    }
    Key::Ctrl('u') => {
      move_page(Direction::Up, app);
    }
    _ => {}
  };
}

fn move_page(direction: Direction, app: &mut App) {
  if direction == Direction::Up {
    if app.help_menu_page > 0 {
      app.help_menu_page -= 1;
    }
  } else if direction == Direction::Down {
    app.help_menu_page += 1;
  }
  app.calculate_help_menu_offset();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use crate::tui::handlers::handle_app;

  #[test]
  fn test_help_menu_pagination() {
    let mut app = App::default();
    app.help_docs_size = 100;
    app.help_menu_max_lines = 10;

    // Test down navigation
    handler(Key::Down, &mut app);
    assert_eq!(app.help_menu_page, 1);
    assert_eq!(app.help_menu_offset, 10);

    handler(Key::Char('j'), &mut app);
    assert_eq!(app.help_menu_page, 2);
    assert_eq!(app.help_menu_offset, 20);

    handler(Key::Ctrl('d'), &mut app);
    assert_eq!(app.help_menu_page, 3);
    assert_eq!(app.help_menu_offset, 30);

    // Test up navigation
    handler(Key::Up, &mut app);
    assert_eq!(app.help_menu_page, 2);
    assert_eq!(app.help_menu_offset, 20);

    handler(Key::Char('k'), &mut app);
    assert_eq!(app.help_menu_page, 1);
    assert_eq!(app.help_menu_offset, 10);

    handler(Key::Ctrl('u'), &mut app);
    assert_eq!(app.help_menu_page, 0);
    assert_eq!(app.help_menu_offset, 0);
  }

  #[test]
  fn test_help_menu_navigation_stack() {
    let mut app = App::default();
    // Start at Home
    assert_eq!(app.get_current_route().id, RouteId::Home);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Empty);

    // Open help menu
    handle_app(Key::Char('?'), &mut app);
    assert_eq!(app.get_current_route().id, RouteId::HelpMenu);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::HelpMenu);

    // Close help menu with Esc (uses handle_escape via handle_app)
    handle_app(Key::Esc, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::Home);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Empty);

    // Open help menu again
    handle_app(Key::Char('?'), &mut app);
    assert_eq!(app.get_current_route().id, RouteId::HelpMenu);

    // Close help menu with 'q' (simulating the back key handling in main.rs)
    let back_key = app.user_config.keys.back;
    assert_eq!(back_key, Key::Char('q'));

    let pop_result = app.pop_navigation_stack();
    assert!(pop_result.is_some());
    assert_eq!(app.get_current_route().id, RouteId::Home);
  }
}
