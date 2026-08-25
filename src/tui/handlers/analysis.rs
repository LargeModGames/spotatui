use crate::{core::action::Action, core::app::App, tui::event::Key};

pub fn handler(key: Key, app: &mut App) {
  // Uppercase 'V' to cycle visualizer style (lowercase 'v' opens the analysis view)
  if key == Key::Char('V') {
    app.apply(Action::CycleVisualizerStyle);
  }
}
