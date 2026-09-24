use super::*;

impl App {
  pub(crate) fn is_spotify_development_app(&self) -> bool {
    self.is_dev_app
  }

  pub(crate) fn mark_spotify_development_app(&mut self, client_id: Option<&str>) {
    self.is_dev_app = true;

    let Some(client_id) = client_id.map(str::trim).filter(|id| !id.is_empty()) else {
      return;
    };
    if self.runtime_state.is_dev_client_id(client_id) {
      return;
    }

    self
      .runtime_state
      .dev_client_ids
      .push(client_id.to_string());
    let dev_client_ids = self.runtime_state.dev_client_ids.clone();
    self.schedule_state_save(PersistedRuntimeState {
      dev_client_ids: Some(dev_client_ids),
      ..PersistedRuntimeState::default()
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn marking_development_app_records_and_persists_the_client_id_once() {
    let mut app = App::default();

    app.mark_spotify_development_app(Some(" client-a "));
    app.mark_spotify_development_app(Some("client-a"));

    assert!(app.is_spotify_development_app());
    assert_eq!(app.runtime_state.dev_client_ids, vec!["client-a"]);
    assert_eq!(
      app.pending_state_save_patch.dev_client_ids,
      Some(vec!["client-a".to_string()])
    );
  }
}
