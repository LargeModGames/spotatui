use crate::{core::action::Action, core::app::App, tui::event::Key};

pub fn handler(key: Key, app: &mut App) {
  // Uppercase 'V' to cycle visualizer style (lowercase 'v' opens the analysis view)
  if key == Key::Char('V') {
    #[cfg(feature = "milkdrop")]
    {
      app.view.milkdrop = false;
    }
    app.apply(Action::CycleVisualizerStyle);
  }
  #[cfg(feature = "milkdrop")]
  crate::tui::ui::milkdrop::handle_key(key, app.view.milkdrop);
}
