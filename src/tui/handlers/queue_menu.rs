use super::common_key_events;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      move_selection(1, app);
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      move_selection(-1, app);
    }
    _ => {}
  }
}

fn move_selection(delta: i32, app: &mut App) {
  let len = app.queue.as_ref().map_or(0, |q| {
    let now = if q.currently_playing.is_some() { 1 } else { 0 };
    now + q.queue.len()
  });
  if len == 0 {
    return;
  }
  let max_index = len.saturating_sub(1);
  let current = app.queue_selected_index;
  let next = match delta {
    -1 => current.saturating_sub(1),
    _ => (current + 1).min(max_index),
  };
  app.queue_selected_index = next;
}
