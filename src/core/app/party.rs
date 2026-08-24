use super::*;

impl App {
  pub(crate) fn start_party(&mut self) {
    self.dispatch(IoEvent::StartParty(ControlMode::HostOnly));
  }

  pub(crate) fn join_party(&mut self, code: String, name: String) {
    self.dispatch(IoEvent::JoinParty { code, name });
  }

  pub(crate) fn leave_party(&mut self) {
    self.dispatch(IoEvent::LeaveParty);
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
