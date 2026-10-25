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

  pub(crate) fn soloist_downloading(&self) -> bool {
    self.soloist_downloading
  }

  /// Download the official build from Spotify, one download at a time.
  pub(crate) fn download_soloist(&mut self) {
    if self.soloist_downloading {
      return;
    }
    self.soloist_downloading = true;
    self.display_revisions.bump(DisplayDomain::Settings);
    let started = self.strings().status.infra.soloist_download_started;
    self.set_status_message(started, 8);
    self.dispatch_without_spinner(IoEvent::DownloadSoloist);
  }

  pub(crate) fn soloist_download_progress(&mut self, percent: u8) {
    let message = self.strings().status.infra.soloist_downloading(percent);
    self.set_status_message(message, 5);
  }

  /// Save the new path; the start to run when the engine waits for it.
  pub(crate) fn soloist_download_finished(
    &mut self,
    result: Result<std::path::PathBuf, crate::infra::soloist::download::Failure>,
  ) -> Option<crate::infra::soloist::startup::SoloistStart> {
    use crate::infra::soloist::download::Failure;
    self.soloist_downloading = false;
    self.display_revisions.bump(DisplayDomain::Settings);
    let strings = self.strings().status.infra;
    let path = match result {
      Ok(path) => path.display().to_string(),
      Err(Failure::NoBuild) => {
        self.set_error_status_message(strings.soloist_no_build, 10);
        return None;
      }
      Err(Failure::Failed(error)) => {
        self.set_error_status_message(strings.soloist_download_failed(&error), 15);
        return None;
      }
    };
    self.user_config.behavior.soloist_path = Some(path.clone());
    if let Err(e) = self.user_config.save_config() {
      let message = self.strings().settings.save_failed(&e.to_string());
      self.handle_error_with_message(anyhow::anyhow!("Failed to save settings: {}", e), message);
      return None;
    }
    // An open Settings screen saves its staged rows later; they must not put the old path back.
    for item in self
      .settings_items
      .iter_mut()
      .chain(self.settings_saved_items.iter_mut())
      .filter(|item| item.id == "behavior.soloist_path")
    {
      item.value = SettingValue::String(path.clone());
    }
    let behavior = &self.user_config.behavior;
    let engine_on = behavior.playback_engine == crate::core::user_config::PlaybackEngine::Soloist;
    let start = (engine_on
      && behavior.enable_streaming
      && !self.native_backend_pending
      && !crate::infra::soloist::supervisor::running())
    .then(|| {
      crate::infra::soloist::startup::SoloistStart::from_behavior(
        behavior,
        self.runtime_state.volume_percent,
        None,
        false,
      )
    });
    match (engine_on, start.is_some()) {
      (false, _) => self.set_status_message(strings.soloist_downloaded_engine_off, 10),
      (true, false) => self.set_status_message(strings.soloist_downloaded_next_launch, 8),
      (true, true) => {
        self.set_status_message(strings.soloist_downloaded, 4);
        self.soloist_starting();
      }
    }
    start
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
  use crate::core::action::Action;
  use crate::core::app::test_support::*;
  use crate::core::app::{App, SettingValue};
  use crate::core::user_config::PlaybackEngine;
  use crate::infra::network::IoEvent;
  use crate::infra::soloist::download::Failure;
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

  fn app_with_config(dir: &std::path::Path) -> App {
    let mut app = make_app_simple();
    app.user_config.path_to_config = Some(crate::core::user_config::UserConfigPaths {
      config_file_path: dir.join("config.yml"),
    });
    app
  }

  fn path_row(value: &str) -> crate::core::app::SettingItem {
    crate::core::app::SettingItem {
      id: "behavior.soloist_path".to_string(),
      name: String::new(),
      description: String::new(),
      value: SettingValue::String(value.to_string()),
    }
  }

  #[test]
  fn the_download_button_dispatches_one_download_at_a_time() {
    let (mut app, rx) = session_free_app();
    let press = || Action::PressSettingButton {
      id: "behavior.soloist_path".to_string(),
    };
    app.apply(press());
    app.apply(press());
    assert!(matches!(rx.try_recv(), Ok(IoEvent::DownloadSoloist)));
    assert!(rx.try_recv().is_err());
    assert_eq!(
      app.setting_button_label("behavior.soloist_path"),
      Some(("Downloading…", true))
    );
    app.soloist_download_finished(Err(Failure::Failed("timed out".to_string())));
    assert!(app.status_message_is_error());
    assert_eq!(
      app.setting_button_label("behavior.soloist_path"),
      Some(("Download from Spotify", false))
    );
  }

  #[test]
  fn a_finished_download_saves_the_path_where_an_open_settings_screen_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config(dir.path());
    app.settings_items = vec![path_row("")];
    app.settings_saved_items = vec![path_row("")];
    let binary = dir.path().join("soloist.exe");

    let start = app.soloist_download_finished(Ok(binary.clone()));

    let path = binary.display().to_string();
    assert!(start.is_none(), "the librespot engine starts no Soloist");
    assert_eq!(
      app.user_config.behavior.soloist_path.as_deref(),
      Some(path.as_str())
    );
    assert!(std::fs::read_to_string(dir.path().join("config.yml"))
      .unwrap()
      .contains("soloist_path"));
    assert_eq!(
      app.settings_items[0].value,
      SettingValue::String(path.clone())
    );
    assert_eq!(
      app.settings_saved_items[0].value,
      SettingValue::String(path)
    );
  }

  #[test]
  fn a_download_starts_soloist_when_the_engine_waits_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config(dir.path());
    app.user_config.behavior.enable_streaming = true;
    app.user_config.behavior.playback_engine = PlaybackEngine::Soloist;

    let start = app.soloist_download_finished(Ok(dir.path().join("soloist")));

    assert!(start.is_some());
    assert!(app.native_backend_pending);
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
