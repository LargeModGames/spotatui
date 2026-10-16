use crate::core::action::Action;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Enter | Key::Esc | Key::Char('q') | Key::Char(' ') => dismiss(app),
    _ => {}
  }
}

/// Dismiss the active announcement, or close a prompt that has none.
pub fn dismiss(app: &mut App) {
  match app.active_announcement.as_ref().map(|a| a.id.clone()) {
    Some(id) => {
      app.apply(Action::DismissAnnouncement { id });
    }
    None => {
      app.pop_navigation_stack();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, Announcement, AnnouncementLevel, RouteId};

  #[test]
  fn enter_dismisses_each_announcement_and_closes_the_prompt_after_the_last() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new_with_state(
      std::sync::mpsc::channel().0,
      crate::core::user_config::UserConfig::new(),
      crate::core::state::RuntimeState::default(),
      Some(dir.path().join("state.yml")),
      false,
      crate::core::i18n::Language::En,
    );
    let announcement = |id: &str| Announcement {
      id: id.to_string(),
      title: id.to_string(),
      body: format!("{id} body"),
      level: AnnouncementLevel::Warning,
      url: None,
      received_at: std::time::Instant::now(),
    };
    app.enqueue_announcements(vec![announcement("first"), announcement("second")]);
    app.push_navigation_stack(RouteId::AnnouncementPrompt, ActiveBlock::AnnouncementPrompt);

    handler(Key::Enter, &mut app);
    let active = app.active_announcement.as_ref().map(|a| a.id.as_str());
    assert_eq!(active, Some("second"));
    assert_eq!(app.get_current_route().id, RouteId::AnnouncementPrompt);

    handler(Key::Esc, &mut app);
    assert!(app.active_announcement.is_none());
    assert_eq!(app.runtime_state.seen_announcement_ids, ["first", "second"]);
    assert_ne!(app.get_current_route().id, RouteId::AnnouncementPrompt);
  }
}
