use super::playbar;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  if key == app.user_config.keys.back {
    app.pop_navigation_stack();
    return;
  }

  playbar::handle_action_key(key, app);
}
