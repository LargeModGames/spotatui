use crate::core::action::Action;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Enter | Key::Char('o') => {
      app.apply(Action::OpenRecap);
    }
    Key::Char('d') => {
      app.apply(Action::DisableRecapPrompt);
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RecapPromptState};
  use std::path::PathBuf;

  fn app_with_prompt() -> App {
    let mut app = App::default();
    app.show_recap_prompt(RecapPromptState {
      path: PathBuf::from("recap.html"),
      listens: 42,
    });
    app
  }

  #[test]
  fn d_disables_the_prompt_and_pops() {
    let mut app = app_with_prompt();
    handler(Key::Char('d'), &mut app);
    assert!(!app.user_config.behavior.enable_monthly_recap_prompt);
    assert!(app.recap_prompt().is_none());
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::RecapPrompt
    );
  }

  #[test]
  fn other_keys_keep_the_prompt_open() {
    let mut app = app_with_prompt();
    handler(Key::Char('x'), &mut app);
    assert!(app.recap_prompt().is_some());
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::RecapPrompt
    );
  }
}
