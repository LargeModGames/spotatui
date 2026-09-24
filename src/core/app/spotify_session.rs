use super::*;

impl App {
  pub(crate) fn is_spotify_development_app(&self) -> bool {
    self.is_dev_app
  }

  pub(crate) fn mark_spotify_development_app(&mut self) {
    self.is_dev_app = true
  }
}
