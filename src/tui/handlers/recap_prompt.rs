use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Enter | Key::Char('o') => {
      if let Some(prompt) = app.recap_prompt.take() {
        if let Err(e) = open::that(&prompt.path) {
          log::warn!("failed to open recap in browser: {}", e);
          app.set_status_message(
            format!(
              "Recap saved at {} (couldn't open browser)",
              prompt.path.display()
            ),
            8,
          );
        }
      }
      app.pop_navigation_stack();
    }
    Key::Char('d') => {
      app.recap_prompt = None;
      app.user_config.behavior.enable_monthly_recap_prompt = false;
      if let Err(e) = app.user_config.save_config() {
        log::warn!("failed to persist monthly recap prompt setting: {}", e);
      }
      app.set_status_message(
        "Monthly recap prompt disabled (re-enable in Settings)".to_string(),
        6,
      );
      app.pop_navigation_stack();
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RecapPromptState, RouteId};
  use std::path::PathBuf;

  fn app_with_prompt() -> App {
    let mut app = App::default();
    app.recap_prompt = Some(RecapPromptState {
      path: PathBuf::from("recap.html"),
      listens: 42,
    });
    app.push_navigation_stack(RouteId::RecapPrompt, ActiveBlock::RecapPrompt);
    app
  }

  #[test]
  fn d_disables_the_prompt_and_pops() {
    let mut app = app_with_prompt();
    handler(Key::Char('d'), &mut app);
    assert!(!app.user_config.behavior.enable_monthly_recap_prompt);
    assert!(app.recap_prompt.is_none());
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::RecapPrompt
    );
  }

  #[test]
  fn other_keys_keep_the_prompt_open() {
    let mut app = app_with_prompt();
    handler(Key::Char('x'), &mut app);
    assert!(app.recap_prompt.is_some());
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::RecapPrompt
    );
  }
}
