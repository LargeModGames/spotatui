use crate::core::app::App;
use crate::core::state::PersistedRuntimeState;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    // Hide the pin: turn the setting off (persisted) and never nag again.
    Key::Char('h') | Key::Char('H') => {
      app.user_config.behavior.pin_community_playlist = false;
      if let Err(e) = app.user_config.save_config() {
        log::warn!("failed to persist community playlist pin setting: {}", e);
      }
      app.set_status_message(
        "Community playlist pin hidden (re-enable in Settings)".to_string(),
        6,
      );
      mark_shown_and_dismiss(app);
    }
    // Keep it pinned (Enter) or dismiss with Esc: either way, don't ask again.
    Key::Enter | Key::Esc => {
      mark_shown_and_dismiss(app);
    }
    _ => {}
  }
}

fn mark_shown_and_dismiss(app: &mut App) {
  app.runtime_state.community_pin_prompt_shown = true;
  if let Err(e) = app.save_runtime_state(&PersistedRuntimeState::community_pin_prompt_shown(true)) {
    log::warn!("failed to persist community pin prompt state: {}", e);
  }
  app.pop_navigation_stack();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use crate::core::user_config::UserConfigPaths;

  // Keep persistence off the real user dirs by pointing state/config at `dir`,
  // which the caller keeps alive for the test's duration.
  fn app_with_prompt(dir: &std::path::Path) -> App {
    let mut app = App::default();
    app.state_path = Some(dir.join("state.yml"));
    app.user_config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.join("config.yml"),
    });
    app.push_navigation_stack(RouteId::CommunityPinPrompt, ActiveBlock::CommunityPinPrompt);
    app
  }

  #[test]
  fn hide_disables_toggle_marks_shown_and_pops() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_prompt(dir.path());
    handler(Key::Char('h'), &mut app);
    assert!(!app.user_config.behavior.pin_community_playlist);
    assert!(app.runtime_state.community_pin_prompt_shown);
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::CommunityPinPrompt
    );
  }

  #[test]
  fn keep_leaves_toggle_on_marks_shown_and_pops() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_prompt(dir.path());
    handler(Key::Enter, &mut app);
    assert!(app.user_config.behavior.pin_community_playlist);
    assert!(app.runtime_state.community_pin_prompt_shown);
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::CommunityPinPrompt
    );
  }

  #[test]
  fn esc_leaves_toggle_on_marks_shown_and_pops() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_prompt(dir.path());
    handler(Key::Esc, &mut app);
    assert!(app.user_config.behavior.pin_community_playlist);
    assert!(app.runtime_state.community_pin_prompt_shown);
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::CommunityPinPrompt
    );
  }
}
