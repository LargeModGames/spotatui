use super::common_key_events;
use crate::{
  core::app::{ActiveBlock, App, RouteId},
  tui::{event::Key, ui::help::get_filtered_help_docs},
};

#[derive(PartialEq)]
enum Direction {
  Up,
  Down,
}

pub(super) fn open(app: &mut App) {
  clear_filter(app);
  app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
}

pub(super) fn clear_filter(app: &mut App) {
  app.help_filter.clear();
  app.help_filter_editing = false;
  refresh_filtered_rows(app);
}

pub fn handler(key: Key, app: &mut App) {
  if app.help_filter_editing {
    handle_filter_input(key, app);
    return;
  }

  match key {
    k if k == app.user_config.keys.search => begin_filter(app),
    Key::Esc if !app.help_filter.is_empty() => clear_filter(app),
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      move_page(Direction::Down, app);
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
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

fn begin_filter(app: &mut App) {
  app.help_filter.clear();
  app.help_filter_editing = true;
  refresh_filtered_rows(app);
}

fn handle_filter_input(key: Key, app: &mut App) {
  match key {
    Key::Esc => clear_filter(app),
    Key::Enter => {
      if app.help_filter.split_whitespace().next().is_none() {
        clear_filter(app);
      } else {
        app.help_filter_editing = false;
        reset_page(app);
      }
    }
    Key::Backspace | Key::Ctrl('h') => {
      app.help_filter.pop();
      refresh_filtered_rows(app);
    }
    Key::Ctrl('u') | Key::Ctrl('l') => {
      app.help_filter.clear();
      refresh_filtered_rows(app);
    }
    Key::Ctrl('w') => {
      while app.help_filter.ends_with(char::is_whitespace) {
        app.help_filter.pop();
      }
      while app
        .help_filter
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_whitespace())
      {
        app.help_filter.pop();
      }
      refresh_filtered_rows(app);
    }
    Key::Char(c) => {
      app.help_filter.push(c);
      refresh_filtered_rows(app);
    }
    _ => {}
  }
}

fn refresh_filtered_rows(app: &mut App) {
  app.help_docs_size = get_filtered_help_docs(app).len() as u32;
  reset_page(app);
}

fn reset_page(app: &mut App) {
  app.help_menu_page = 0;
  app.help_menu_offset = 0;
}

fn move_page(direction: Direction, app: &mut App) {
  if direction == Direction::Up {
    if app.help_menu_page > 0 {
      app.help_menu_page -= 1;
    }
  } else if direction == Direction::Down {
    app.help_menu_page = app.help_menu_page.saturating_add(1);
  }
  app.calculate_help_menu_offset();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    core::source::Source,
    tui::{handlers::handle_app, ui::help::get_help_docs},
  };

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
  fn pagination_does_not_open_an_empty_last_page() {
    let mut app = App::default();
    app.help_docs_size = 20;
    app.help_menu_max_lines = 10;
    app.help_menu_page = 1;
    app.calculate_help_menu_offset();

    handler(Key::Down, &mut app);

    assert_eq!(app.help_menu_page, 1);
    assert_eq!(app.help_menu_offset, 10);
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

    // Close help menu with 'q' (simulating the back key handling in the runner)
    let back_key = app.user_config.keys.back;
    assert_eq!(back_key, Key::Char('q'));

    let pop_result = app.pop_navigation_stack();
    assert!(pop_result.is_some());
    assert_eq!(app.get_current_route().id, RouteId::Home);
  }

  #[test]
  fn search_key_starts_a_local_help_filter_and_captures_global_keys() {
    let mut app = App::default();
    app.active_source = Source::Local;
    handle_app(app.user_config.keys.help, &mut app);

    handle_app(app.user_config.keys.search, &mut app);
    handle_app(Key::Char('d'), &mut app);
    handle_app(Key::Char('q'), &mut app);

    assert_eq!(app.get_current_route().id, RouteId::HelpMenu);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::HelpMenu);
    assert!(app.help_filter_editing);
    assert_eq!(app.help_filter, "dq");
  }

  #[test]
  fn confirmed_filter_stays_applied_until_escape_clears_it() {
    let mut app = App::default();
    handle_app(app.user_config.keys.help, &mut app);
    let unfiltered_size = get_help_docs(&app).len() as u32;

    handle_app(app.user_config.keys.search, &mut app);
    for c in "volume".chars() {
      handle_app(Key::Char(c), &mut app);
    }

    assert!(app.help_filter_editing);
    assert!(app.help_docs_size > 0);
    assert!(app.help_docs_size < unfiltered_size);

    handle_app(Key::Enter, &mut app);
    assert!(!app.help_filter_editing);
    assert_eq!(app.help_filter, "volume");

    handle_app(Key::Esc, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::HelpMenu);
    assert!(app.help_filter.is_empty());
    assert_eq!(app.help_docs_size, unfiltered_size);

    handle_app(Key::Esc, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::Home);
  }

  #[test]
  fn help_filter_uses_the_configured_search_binding() {
    let mut app = App::default();
    app.user_config.keys.search = Key::Char('f');
    handle_app(app.user_config.keys.help, &mut app);

    handle_app(Key::Char('f'), &mut app);

    assert!(app.help_filter_editing);
    assert!(app.help_filter.is_empty());
  }
}
