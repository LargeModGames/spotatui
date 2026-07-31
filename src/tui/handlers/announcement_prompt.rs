use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Enter | Key::Esc | Key::Char('q') | Key::Char(' ') => {
      if let Some(dismissed_id) = app.dismiss_active_announcement() {
        app.runtime_state.mark_announcement_seen(dismissed_id);
        let patch = crate::core::state::PersistedRuntimeState::announcements(
          &app.runtime_state.seen_announcement_ids,
          &app.runtime_state.dismissed_announcements,
        );
        if let Err(error) = app.save_runtime_state(&patch) {
          app.handle_error(anyhow::anyhow!(
            "Failed to persist dismissed announcement: {}",
            error
          ));
        }
      }

      if app.active_announcement.is_none() {
        app.pop_navigation_stack();
      }
    }
    _ => {}
  }
}
