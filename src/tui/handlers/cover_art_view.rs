use crate::core::action::Action;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Char('s') => {
      app.apply(Action::ToggleSaveCurrentItem);
    }
    k if k == app.user_config.keys.back => {
      app.pop_navigation_stack();
    }
    _ => {}
  }
}
