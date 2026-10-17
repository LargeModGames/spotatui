use super::*;

impl App {
  pub(crate) fn visualizer_look(&self) -> &crate::core::visualizer_look::VisualizerLook {
    &self.visualizer_look
  }

  /// Takes a read of `visualizer.yml`; an unreadable file keeps the look, and the first problem shows as an error.
  pub(crate) fn apply_visualizer_look(
    &mut self,
    loaded: Result<(crate::core::visualizer_look::VisualizerLook, Vec<String>), String>,
  ) {
    let problems = match loaded {
      Ok((look, problems)) => {
        if look != self.visualizer_look {
          self.visualizer_look = look;
          self.display_revisions.bump(DisplayDomain::Settings);
        }
        problems
      }
      Err(error) => vec![error],
    };
    for problem in &problems {
      log::warn!("visualizer.yml: {problem}");
    }
    if let Some(first) = problems.first() {
      let message = self.strings().status.app.visualizer_config(first);
      self.set_error_status_message(message, 8);
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::core::app::test_support::*;
  use crate::core::app::DisplayDomain;
  use crate::core::visualizer_look::VisualizerLook;

  #[test]
  fn the_same_look_again_does_not_resend_the_settings() {
    let mut app = make_app_simple();
    let before = app.display_revisions().get(DisplayDomain::Settings);
    app.apply_visualizer_look(Ok((VisualizerLook::default(), Vec::new())));
    assert_eq!(app.display_revisions().get(DisplayDomain::Settings), before);
    assert!(app.status_message().is_none());
  }

  #[test]
  fn a_new_look_resends_the_settings_once() {
    let mut app = make_app_simple();
    let before = app.display_revisions().get(DisplayDomain::Settings);
    let (look, _) = VisualizerLook::parse("count: 2\n").unwrap();
    app.apply_visualizer_look(Ok((look.clone(), Vec::new())));
    app.apply_visualizer_look(Ok((look, Vec::new())));
    assert_eq!(
      app.display_revisions().get(DisplayDomain::Settings),
      before + 1
    );
    assert_eq!(app.visualizer_look().count, 2);
  }

  #[test]
  fn a_problem_shows_as_an_error_and_a_broken_file_keeps_the_look() {
    let mut app = make_app_simple();
    let (look, problems) = VisualizerLook::parse("count: 2\norder: rainbow\n").unwrap();
    app.apply_visualizer_look(Ok((look, problems)));
    assert!(app.status_message_is_error());
    assert!(app
      .status_message()
      .is_some_and(|message| message.starts_with("visualizer.yml: order:")));
    app.apply_visualizer_look(Err("bad yaml".to_string()));
    assert_eq!(app.visualizer_look().count, 2);
    assert_eq!(app.status_message(), Some("visualizer.yml: bad yaml"));
  }
}
