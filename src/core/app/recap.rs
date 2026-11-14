use super::*;

impl App {
  /// The monthly recap popup that waits for an answer, if one is open.
  pub fn recap_prompt(&self) -> Option<&RecapPromptState> {
    self.recap_prompt.as_ref()
  }

  /// Raise the monthly recap popup for a recap that was just written.
  pub(crate) fn show_recap_prompt(&mut self, prompt: RecapPromptState) {
    self.recap_prompt = Some(prompt);
    self.display_revisions.bump(DisplayDomain::Status);
    self.push_navigation_stack(RouteId::RecapPrompt, ActiveBlock::RecapPrompt);
  }

  /// Take the popup's state and remove its frame, also under a page that opened over it.
  fn close_recap_prompt(&mut self) -> Option<RecapPromptState> {
    let prompt = self.recap_prompt.take();
    if prompt.is_some() {
      self.display_revisions.bump(DisplayDomain::Status);
    }
    self.drop_routes(RouteId::RecapPrompt, ActiveBlock::RecapPrompt);
    prompt
  }

  /// Open the recap in the browser and close the popup.
  pub(crate) fn open_recap(&mut self) {
    if let Some(prompt) = self.close_recap_prompt() {
      if let Err(e) = open::that_detached(&prompt.path) {
        log::warn!("failed to open recap in browser: {}", e);
        self.set_status_message(
          self
            .strings()
            .status
            .app
            .recap_browser_failed(&prompt.path.display().to_string()),
          8,
        );
      }
    }
  }

  /// Close the popup without opening the recap ("[ESC] Later").
  pub(crate) fn dismiss_recap_prompt(&mut self) {
    self.close_recap_prompt();
  }

  /// Close the popup and switch the monthly prompt off in `config.yml`.
  pub(crate) fn disable_recap_prompt(&mut self) {
    self.close_recap_prompt();
    self.user_config.behavior.enable_monthly_recap_prompt = false;
    if let Err(e) = self.user_config.save_config() {
      log::warn!("failed to persist monthly recap prompt setting: {}", e);
    }
    self.set_status_message(self.strings().status.app.recap_disabled.to_string(), 6);
  }
}
