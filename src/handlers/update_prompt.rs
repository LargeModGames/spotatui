use super::super::app::App;
use crate::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Enter | Key::Esc | Key::Char('q') | Key::Char(' ') => {
      app.update_prompt_acknowledged = true;
      app.pop_navigation_stack();
    }
    _ => {}
  }
}
