use super::*;

const PARTY_NEEDS_SPOTIFY: &str =
  "Listening Party needs Spotify. Press `d` and pick Spotify to log in.";

impl App {
  /// Host a party; needs a Spotify session, the relay drives Spotify playback.
  pub(crate) fn start_party(&mut self) {
    if !self.spotify_connected {
      self.set_status_message(PARTY_NEEDS_SPOTIFY, 6);
      return;
    }
    self.dispatch(IoEvent::StartParty(ControlMode::HostOnly));
  }

  /// Join a party; same session requirement as `start_party`.
  pub(crate) fn join_party(&mut self, code: String, name: String) {
    if !self.spotify_connected {
      self.set_status_message(PARTY_NEEDS_SPOTIFY, 6);
      return;
    }
    self.dispatch(IoEvent::JoinParty { code, name });
    // The typed code and name are consumed only by a join that went out.
    self.view.party_input.clear();
    self.view.party_input_idx = 0;
    self.view.party_join_name.clear();
  }

  /// The local write is optimistic on purpose: the relay handler never writes
  /// the session back, so the popup's "Control" label renders from it.
  pub(crate) fn toggle_party_control_mode(&mut self) {
    let Some(session) = &mut self.party_session else {
      return;
    };
    let updated_mode = match session.control_mode {
      ControlMode::HostOnly => ControlMode::SharedControl,
      ControlMode::SharedControl => ControlMode::HostOnly,
    };
    session.control_mode = updated_mode.clone();
    self.dispatch(IoEvent::SetPartyControlMode(updated_mode));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::action::Action;
  use crate::core::app::test_support::*;

  #[test]
  fn start_party_without_a_session_dispatches_nothing_and_says_why() {
    let (mut app, rx) = session_free_app();

    app.apply(Action::StartParty);

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message.as_deref(), Some(PARTY_NEEDS_SPOTIFY));
  }

  #[test]
  fn join_party_without_a_session_dispatches_nothing_and_keeps_the_input() {
    let (mut app, rx) = session_free_app();
    app.view.party_input = "ABC123".chars().collect();
    app.view.party_input_idx = 6;
    app.view.party_join_name = "Guest".chars().collect();

    app.apply(Action::JoinParty {
      code: "ABC123".to_string(),
      name: "Guest".to_string(),
    });

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message.as_deref(), Some(PARTY_NEEDS_SPOTIFY));
    assert_eq!(app.view.party_input.iter().collect::<String>(), "ABC123");
    assert_eq!(app.view.party_input_idx, 6);
    assert_eq!(app.view.party_join_name.iter().collect::<String>(), "Guest");
  }

  #[test]
  fn join_party_with_a_session_dispatches_and_clears_the_input() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.view.party_input = "ABC123".chars().collect();
    app.view.party_input_idx = 6;
    app.view.party_join_name = "Guest".chars().collect();

    app.apply(Action::JoinParty {
      code: "ABC123".to_string(),
      name: "Guest".to_string(),
    });

    assert!(matches!(rx.try_recv(), Ok(IoEvent::JoinParty { .. })));
    assert!(app.view.party_input.is_empty());
    assert_eq!(app.view.party_input_idx, 0);
    assert!(app.view.party_join_name.is_empty());
  }
}
