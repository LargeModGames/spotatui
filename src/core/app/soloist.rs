//! The Soloist engine's state on the App: the link, its events, the notices.

use super::*;

impl App {
  pub(crate) fn soloist_link(&self) -> Option<Arc<crate::infra::soloist::SoloistLink>> {
    self.soloist_link.clone()
  }

  fn is_current_soloist(&self, link: &Arc<crate::infra::soloist::SoloistLink>) -> bool {
    self
      .soloist_link
      .as_ref()
      .is_some_and(|current| Arc::ptr_eq(current, link))
  }

  /// Starts wait for the engine, as they wait for librespot's deferred start.
  pub(crate) fn soloist_starting(&mut self) {
    self.native_backend_pending = true;
  }

  pub(crate) fn soloist_restarting(&mut self) {
    self.native_backend_pending = true;
  }

  /// The engine gave up: starts stop waiting, and a parked one goes out now.
  pub(crate) fn soloist_failed(&mut self) {
    self.native_backend_pending = false;
    self.replay_pending_start_playback();
  }

  pub(crate) fn install_soloist_link(&mut self, link: Arc<crate::infra::soloist::SoloistLink>) {
    self.native_device_id = Some(link.device_id().to_string());
    self.soloist_link = Some(link);
    self.native_backend_pending = false;
  }

  pub(crate) fn uninstall_soloist_link(&mut self, link: &Arc<crate::infra::soloist::SoloistLink>) {
    if self.is_current_soloist(link) {
      self.soloist_link = None;
      self.display_revisions.bump(DisplayDomain::Playback);
    }
  }

  /// The first login of this launch: the configured startup play or pause, then a parked start.
  pub(crate) fn soloist_logged_in(
    &mut self,
    behavior: Option<crate::core::user_config::StartupBehavior>,
    shuffle: bool,
  ) {
    self.run_spotify_startup_behavior(behavior, shuffle);
    self.replay_pending_start_playback();
  }

  pub(crate) fn apply_soloist_event(
    &mut self,
    link: &Arc<crate::infra::soloist::SoloistLink>,
    event: &crate::infra::soloist::protocol::Event,
  ) {
    use crate::infra::soloist::protocol::{Command, Event};
    if !self.is_current_soloist(link) {
      return;
    }
    link.apply(event);
    let snapshot = link.snapshot();
    match event {
      Event::Track { .. } | Event::Context(_) | Event::Device { .. } => self.poll_playback_soon(),
      Event::Volume(volume) if snapshot.volume == Some(*volume) => {
        self.runtime_state.volume_percent = *volume;
        self.schedule_state_save(PersistedRuntimeState::volume_percent(*volume));
      }
      _ => {}
    }
    // A phone cannot start Soloist over a decoded source or Music that owns the sink.
    if snapshot.status.is_playing()
      && matches!(
        self.playback_owner(),
        PlaybackOwner::Decoded | PlaybackOwner::AppleMusic | PlaybackOwner::Queue
      )
    {
      link.send(Command::Pause);
    }
    self.overlay_soloist_snapshot();
  }

  /// Report the stream Soloist plays: once per engine start, and again on a drop from lossless.
  pub(crate) fn note_soloist_quality(
    &mut self,
    link: &Arc<crate::infra::soloist::SoloistLink>,
    previous: Option<crate::infra::soloist::StreamQuality>,
    quality: &crate::infra::soloist::StreamQuality,
  ) {
    if !self.is_current_soloist(link) {
      return;
    }
    self.display_revisions.bump(DisplayDomain::Playback);
    // Podcasts are always lossy; they say nothing about the quality setting.
    if link.snapshot().item_is_episode {
      return;
    }
    let strings = self.strings().status.infra;
    let label = quality.label();
    match previous {
      None if quality.is_lossless() => self.set_status_message(strings.soloist_streams(&label), 4),
      None => self.set_status_message(strings.soloist_lossy_hint(&label), 10),
      Some(previous) if previous.is_lossless() && !quality.is_lossless() => {
        self.set_status_message(strings.soloist_dropped(&label), 8);
      }
      Some(_) => {}
    }
  }

  pub(crate) fn soloist_notice(&mut self, notice: crate::infra::soloist::Notice) {
    use crate::infra::soloist::Notice;
    let strings = self.strings().status.infra;
    match notice {
      Notice::PathUnset => self.set_error_status_message(strings.soloist_path_unset, 15),
      Notice::Missing(path) => {
        self.set_error_status_message(strings.soloist_missing(&path.display().to_string()), 15);
      }
      Notice::NotExecutable(path) => {
        let path = path.display().to_string();
        self.set_error_status_message(strings.soloist_not_executable(&path), 15);
      }
      Notice::WrongCpu => self.set_error_status_message(strings.soloist_wrong_cpu, 15),
      Notice::StartFailed(error) => {
        self.set_error_status_message(strings.soloist_start_failed(&error), 15);
      }
      Notice::KeyMissing(path) => {
        let path = path.display().to_string();
        self.set_error_status_message(strings.soloist_key_missing(&path), 20);
      }
      Notice::NotPaired => self.set_status_message(strings.soloist_not_paired, 12),
      Notice::Offline => self.set_status_message(strings.soloist_offline, 8),
      Notice::BadKey => self.set_error_status_message(strings.soloist_bad_key, 15),
      Notice::Expired => self.set_error_status_message(strings.soloist_expired, 20),
      Notice::ExpiresSoon { count, unit } => {
        self.set_status_message(strings.soloist_expires_soon(count, &unit), 10);
      }
      Notice::DirInUse => self.set_error_status_message(strings.soloist_dir_in_use, 15),
      Notice::KeepsExiting => self.set_error_status_message(strings.soloist_keeps_exiting, 15),
      Notice::NoWebSocket => self.set_error_status_message(strings.soloist_no_websocket, 15),
      Notice::Restarting => self.set_status_message(strings.soloist_restarting, 4),
      Notice::RestartingForQuality => self.set_status_message(strings.soloist_restart_quality, 4),
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::core::app::test_support::*;
  use crate::infra::soloist::protocol::{Command, Event, Status};
  use crate::infra::soloist::SoloistLink;

  #[test]
  fn install_sets_the_native_device_id_and_ends_the_wait() {
    let mut app = make_app_simple();
    app.soloist_starting();
    let (link, _rx) = SoloistLink::test_pair("soloist-device");
    app.install_soloist_link(link);
    assert_eq!(app.native_device_id.as_deref(), Some("soloist-device"));
    assert!(!app.native_backend_pending);
    assert!(app.soloist_engine_installed());
  }

  #[test]
  fn stale_soloist_link_writes_nothing() {
    let mut app = make_app_simple();
    let (old, _old_rx) = SoloistLink::test_pair("old");
    let (new, _new_rx) = SoloistLink::test_pair("new");
    app.install_soloist_link(old.clone());
    app.install_soloist_link(new);
    app.apply_soloist_event(&old, &Event::Volume(70));
    assert_eq!(old.snapshot().volume, None);
    app.uninstall_soloist_link(&old);
    assert!(app.soloist_engine_installed());
  }

  #[test]
  fn pause_native_playback_pauses_soloist() {
    let mut app = make_app_simple();
    let (link, mut rx) = SoloistLink::test_pair("soloist-device");
    app.install_soloist_link(link.clone());
    app.apply_soloist_event(&link, &Event::Playback(Status::Playing));
    app.pause_native_playback();
    assert_eq!(rx.try_recv().ok(), Some(Command::Pause));
  }

  #[test]
  fn queue_refuses_a_decoded_item_while_soloist_plays() {
    let mut app = make_app_simple();
    let (link, _rx) = SoloistLink::test_pair("soloist-device");
    app.install_soloist_link(link.clone());
    app.apply_soloist_event(
      &link,
      &Event::Auth {
        logged_in: true,
        is_active: true,
      },
    );
    app.add_track_to_native_queue(queue_track(Some("file:///music/a.flac"), "a"));
    assert!(app.native_queue.is_empty());
  }
}
