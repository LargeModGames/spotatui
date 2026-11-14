use super::*;

/// The host sends its state every 2 s; a guest that heard none for longer stops relaying.
const HOST_STATE_FRESH: Duration = Duration::from_secs(5);

#[cfg(test)]
const PARTY_NEEDS_SPOTIFY: &str =
  "Listening Party needs Spotify. Press `d` and pick Spotify to log in.";

impl App {
  pub(crate) fn party_status(&self) -> &PartyStatus {
    &self.party_status
  }

  pub(crate) fn party_session(&self) -> Option<&PartySession> {
    self.party_session.as_ref()
  }

  pub(crate) fn set_party_status(&mut self, status: PartyStatus) {
    self.party_status = status;
    self.display_revisions.bump(DisplayDomain::Party);
  }

  pub(crate) fn set_party_session(&mut self, session: Option<PartySession>) {
    self.party_session = session;
    self.display_revisions.bump(DisplayDomain::Party);
  }

  /// The live session for an in-place update; bumps the party revision when one exists.
  pub(crate) fn party_session_mut(&mut self) -> Option<&mut PartySession> {
    let session = self.party_session.as_mut()?;
    self.display_revisions.bump(DisplayDomain::Party);
    Some(session)
  }

  /// Host a party; needs a Spotify session, the relay drives Spotify playback.
  pub(crate) fn start_party(&mut self) {
    if !self.spotify_connected {
      self.set_status_message(self.strings().status.app.party_needs_spotify, 6);
      return;
    }
    self.dispatch(IoEvent::StartParty(ControlMode::HostOnly));
  }

  /// Join a party; same session requirement as `start_party`.
  pub(crate) fn join_party(&mut self, code: String, name: String) {
    if !self.spotify_connected {
      self.set_status_message(self.strings().status.app.party_needs_spotify, 6);
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
    self.display_revisions.bump(DisplayDomain::Party);
    self.dispatch(IoEvent::SetPartyControlMode(updated_mode));
  }

  /// The party follows Spotify transport only. Coarser than the transport
  /// guard on purpose: a queued Spotify track keeps librespot, but a guest must
  /// not drive the host's queue slot. A parked native backend has no Spotify
  /// playback to relay or follow.
  pub(crate) fn party_yields_to_local_playback(&self) -> bool {
    self.apple_music_owns_playback()
      || self.playback_owner().owns_local_sink()
      || self.native_parked_here()
  }

  /// A guest heard the host publish playback it can follow. No revision bump: nothing shows it.
  pub(crate) fn note_host_state(&mut self) {
    if let Some(session) = self.party_session.as_mut() {
      session.host_state_at = Some(Instant::now());
    }
  }

  /// A shared-control guest sends its transport to the host instead of playing it,
  /// while the host publishes playback the party follows.
  pub(crate) fn relay_party_transport(&mut self, action: PlaybackAction) -> bool {
    let relays = self.party_session().is_some_and(|session| {
      session.role == PartyRole::Guest
        && session.control_mode == ControlMode::SharedControl
        && session
          .host_state_at
          .is_some_and(|at| at.elapsed() < HOST_STATE_FRESH)
    }) && !self.party_yields_to_local_playback();
    if relays {
      self.dispatch(IoEvent::PartyPlaybackCommand(action));
    }
    relays
  }

  /// An OS play (`Some(true)`), pause (`Some(false)`) or play/pause (`None`) key for a relaying guest.
  #[cfg_attr(
    not(any(
      all(feature = "mpris", target_os = "linux"),
      all(feature = "macos-media", target_os = "macos"),
      all(feature = "windows-media", target_os = "windows")
    )),
    allow(dead_code)
  )]
  pub(crate) fn relay_party_play_key(&mut self, play: Option<bool>) -> bool {
    match play {
      None => self.relay_party_toggle(),
      Some(true) => self.relay_party_transport(PlaybackAction::Play),
      Some(false) => self.relay_party_transport(PlaybackAction::Pause),
    }
  }

  /// Play or pause for a relaying guest, flipped at once so quick presses alternate.
  pub(crate) fn relay_party_toggle(&mut self) -> bool {
    let playing = crate::core::plugin_api::playback_state(self).is_some_and(|p| p.is_playing);
    let action = if playing {
      PlaybackAction::Pause
    } else {
      PlaybackAction::Play
    };
    if !self.relay_party_transport(action) {
      return false;
    }
    if self.native_is_playing.is_some() {
      self.native_is_playing = Some(!playing);
    }
    true
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

  fn app_in_party(
    role: PartyRole,
    control_mode: ControlMode,
  ) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.set_party_session(Some(PartySession {
      role,
      code: "ABC123".to_string(),
      guests: Vec::new(),
      control_mode,
      host_name: "Host".to_string(),
      host_state_at: Some(Instant::now()),
    }));
    (app, rx)
  }

  fn relayed(rx: &std::sync::mpsc::Receiver<IoEvent>) -> Vec<PlaybackAction> {
    rx.try_iter()
      .filter_map(|event| match event {
        IoEvent::PartyPlaybackCommand(action) => Some(action),
        _ => None,
      })
      .collect()
  }

  #[test]
  fn a_shared_control_guest_sends_its_transport_to_the_host() {
    let (mut app, rx) = app_in_party(PartyRole::Guest, ControlMode::SharedControl);
    // The host's sync has the guest's native player playing.
    app.current_playback_context = Some(make_external_context());
    app.is_streaming_active = true;
    app.native_track_info = Some(NativeTrackInfo::default());
    app.native_is_playing = Some(true);

    app.apply(Action::TogglePlayback);
    app.apply(Action::TogglePlayback);
    app.apply(Action::NextTrack);
    app.apply(Action::PreviousTrack);
    app.apply(Action::ForcePreviousTrack);

    assert_eq!(
      relayed(&rx),
      vec![
        PlaybackAction::Pause,
        PlaybackAction::Play,
        PlaybackAction::NextTrack,
        PlaybackAction::PrevTrack,
        PlaybackAction::ForcePrevTrack,
      ]
    );
  }

  #[test]
  fn a_shared_control_guest_sends_os_play_and_pause_keys_to_the_host() {
    let (mut app, rx) = app_in_party(PartyRole::Guest, ControlMode::SharedControl);

    assert!(app.relay_party_play_key(Some(false)));
    assert!(app.relay_party_play_key(Some(true)));

    assert_eq!(
      relayed(&rx),
      vec![PlaybackAction::Pause, PlaybackAction::Play]
    );
  }

  #[test]
  fn a_guest_whose_host_publishes_nothing_to_follow_keeps_its_transport_local() {
    let (mut app, rx) = app_in_party(PartyRole::Guest, ControlMode::SharedControl);
    if let Some(session) = app.party_session.as_mut() {
      session.host_state_at = None;
    }

    app.apply(Action::NextTrack);

    assert!(relayed(&rx).is_empty());
  }

  #[test]
  fn a_host_only_guest_keeps_its_transport_local() {
    let (mut app, rx) = app_in_party(PartyRole::Guest, ControlMode::HostOnly);

    app.apply(Action::NextTrack);

    assert!(relayed(&rx).is_empty());
  }

  #[test]
  fn a_host_never_relays_its_own_transport() {
    let (mut app, rx) = app_in_party(PartyRole::Host, ControlMode::SharedControl);

    app.apply(Action::NextTrack);
    app.apply(Action::TogglePlayback);

    assert!(relayed(&rx).is_empty());
  }
}
