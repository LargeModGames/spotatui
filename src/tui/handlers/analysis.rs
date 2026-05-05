use crate::{core::app::App, tui::event::Key};

pub fn handler(key: Key, app: &mut App) {
  // Uppercase 'V' to cycle visualizer style (lowercase 'v' opens the analysis view)
  if key == Key::Char('V') {
    app.user_config.behavior.visualizer_style = app.user_config.behavior.visualizer_style.next();
    // Save the config so the preference persists
    let _ = app.user_config.save_config();
  }
}
