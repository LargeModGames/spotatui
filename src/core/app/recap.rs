use super::*;

impl App {
  /// The monthly recap popup that waits for an answer, if one is open.
  pub fn recap_prompt(&self) -> Option<&RecapPromptState> {
    self.recap_prompt.as_ref()
  }

  /// Raise the monthly recap popup for a recap that was just written.
  pub(crate) fn show_recap_prompt(&mut self, prompt: RecapPromptState) {
    self.recap_prompt = Some(prompt);
    self.push_navigation_stack(RouteId::RecapPrompt, ActiveBlock::RecapPrompt);
  }

  /// Open the recap in the browser and close the popup.
  pub(crate) fn open_recap(&mut self) {
    if let Some(prompt) = self.recap_prompt.take() {
      if let Err(e) = open::that_detached(&prompt.path) {
        log::warn!("failed to open recap in browser: {}", e);
        self.set_status_message(
          format!(
            "Recap saved at {} (couldn't open browser)",
            prompt.path.display()
          ),
          8,
        );
      }
    }
    self.pop_navigation_stack();
  }

  /// Close the popup without opening the recap ("[ESC] Later").
  pub(crate) fn dismiss_recap_prompt(&mut self) {
    self.recap_prompt = None;
    self.pop_navigation_stack();
  }

  /// Close the popup and switch the monthly prompt off in `config.yml`.
  pub(crate) fn disable_recap_prompt(&mut self) {
    self.recap_prompt = None;
    self.user_config.behavior.enable_monthly_recap_prompt = false;
    if let Err(e) = self.user_config.save_config() {
      log::warn!("failed to persist monthly recap prompt setting: {}", e);
    }
    self.set_status_message(
      "Monthly recap prompt disabled (re-enable in Settings)".to_string(),
      6,
    );
    self.pop_navigation_stack();
  }
}
