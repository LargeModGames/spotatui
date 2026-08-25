//! Sort menu handler for context sorting
//!
//! Handles keyboard input for the sort menu popup

use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::{ActiveBlock, App};
use crate::core::sort::SortContext;
use crate::tui::event::Key;

/// Handle input when the sort menu is active
pub fn handler(key: Key, app: &mut App) {
  let context = match app.view.sort_context {
    Some(ctx) => ctx,
    None => {
      // No context, close menu
      close_sort_menu(app);
      return;
    }
  };
  let available_fields = context.available_fields();

  match key {
    Key::Esc | Key::Char(',') => {
      close_sort_menu(app);
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if app.view.sort_menu_selected > 0 {
        app.view.sort_menu_selected -= 1;
      } else {
        app.view.sort_menu_selected = available_fields.len().saturating_sub(1);
      }
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if app.view.sort_menu_selected < available_fields.len().saturating_sub(1) {
        app.view.sort_menu_selected += 1;
      } else {
        app.view.sort_menu_selected = 0;
      }
    }
    Key::Enter => {
      if let Some(field) = available_fields.get(app.view.sort_menu_selected) {
        app.apply(Action::Sort {
          context,
          field: *field,
        });
      }
      close_sort_menu(app);
    }
    // Quick select by shortcut character (lowercase = ascending, uppercase = descending)
    Key::Char(c) => {
      // Find field matching this shortcut
      for field in available_fields {
        if let Some(shortcut) = field.shortcut() {
          if c == shortcut || c == shortcut.to_ascii_uppercase() {
            app.apply(Action::Sort {
              context,
              field: *field,
            });
            // Toggle order if uppercase
            if c.is_ascii_uppercase() {
              app.apply(Action::ToggleSortOrder(context));
            }
            close_sort_menu(app);
            return;
          }
        }
      }
    }
    _ => {}
  }
}

/// Open the sort menu for a given context
pub fn open_sort_menu(app: &mut App, context: SortContext) {
  app.view.sort_context = Some(context);
  app.view.sort_menu_visible = true;
  app.view.sort_menu_selected = 0;

  // Find current sort field in the available fields to highlight it
  let current_field = app.sort_state(context).field;

  let available = context.available_fields();
  for (i, field) in available.iter().enumerate() {
    if *field == current_field {
      app.view.sort_menu_selected = i;
      break;
    }
  }

  app.set_current_route_state(Some(ActiveBlock::SortMenu), None);
}

fn close_sort_menu(app: &mut App) {
  app.view.sort_menu_visible = false;
  app.view.sort_context = None;
  app.set_current_route_state(Some(ActiveBlock::Empty), None);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::sort::{SortField, SortOrder};

  fn app_with_open_menu(context: SortContext) -> App {
    let mut app = App::default();
    open_sort_menu(&mut app, context);
    app
  }

  #[test]
  fn enter_applies_the_highlighted_field_and_closes_the_menu() {
    let mut app = app_with_open_menu(SortContext::SavedArtists);
    // Available fields for artists: [Default, Name].
    app.view.sort_menu_selected = 1;

    handler(Key::Enter, &mut app);

    assert_eq!(app.artist_sort.field, SortField::Name);
    assert_eq!(app.artist_sort.order, SortOrder::Ascending);
    assert!(!app.view.sort_menu_visible);
    assert!(app.view.sort_context.is_none());
  }

  #[test]
  fn an_uppercase_shortcut_records_the_reversed_order() {
    let mut app = app_with_open_menu(SortContext::SavedArtists);

    handler(Key::Char('N'), &mut app);

    assert_eq!(app.artist_sort.field, SortField::Name);
    assert_eq!(app.artist_sort.order, SortOrder::Descending);
    assert!(!app.view.sort_menu_visible);
  }

  #[test]
  fn a_character_without_a_shortcut_leaves_the_menu_open() {
    let mut app = app_with_open_menu(SortContext::SavedArtists);

    handler(Key::Char('x'), &mut app);

    assert_eq!(app.artist_sort.field, SortField::Default);
    assert!(app.view.sort_menu_visible);
  }
}
