//! The Soloist engine's state on the App: the link, its events, the notices.

use super::*;

impl App {
  pub(crate) fn soloist_link(&self) -> Option<Arc<crate::infra::soloist::SoloistLink>> {
    self.soloist_link.clone()
  }

  pub(crate) fn is_current_soloist(&self, link: &Arc<crate::infra::soloist::SoloistLink>) -> bool {
    self
      .soloist_link
      .as_ref()
      .is_some_and(|current| Arc::ptr_eq(current, link))
  }

  /// Starts wait for the engine, as they wait for librespot's deferred start.
  pub(crate) fn soloist_starting(&mut self) {
    self.native_backend_pending = true;
  }

  /// The engine gave up: starts stop waiting, and a parked one goes out now.
  pub(crate) fn soloist_failed(&mut self, notice: Option<crate::infra::soloist::Notice>) {
    if let Some(notice) = notice {
      self.soloist_notice(notice);
    }
    self.native_backend_pending = false;
    self.set_soloist_pairing(false);
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
    use crate::infra::soloist::protocol::Event;
    if !self.is_current_soloist(link) {
      return;
    }
    link.apply(event);
    let snapshot = link.snapshot();
    if snapshot.logged_in && self.soloist_pairing {
      self.set_soloist_pairing(false);
      let paired = self.strings().status.infra.soloist_paired;
      self.set_status_message(paired, 6);
    }
    match event {
      Event::Track { .. } | Event::Context(_) | Event::Device { .. } => self.poll_playback_soon(),
      Event::Volume(volume) if snapshot.volume == Some(*volume) => {
        self.runtime_state.volume_percent = *volume;
        self.schedule_state_save(PersistedRuntimeState::volume_percent(*volume));
      }
      _ => {}
    }
    // A phone cannot start Soloist over a decoded source or Music that owns the sink.
    if matches!(
      self.playback_owner(),
      PlaybackOwner::Decoded | PlaybackOwner::AppleMusic | PlaybackOwner::Queue
    ) {
      link.pause();
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
    let engine_on = self.user_config.behavior.playback_engine
      == crate::core::user_config::PlaybackEngine::Soloist;
    let start = self.soloist_start_now();
    match (engine_on, start.is_some()) {
      (false, _) => self.set_status_message(strings.soloist_downloaded_engine_off, 10),
      (true, false) => self.set_status_message(strings.soloist_downloaded_next_launch, 8),
      (true, true) => self.set_status_message(strings.soloist_downloaded, 4),
    }
    start
  }

  /// The start to run when the Soloist engine is on and nothing starts or runs it.
  pub(crate) fn soloist_start_now(
    &mut self,
  ) -> Option<crate::infra::soloist::startup::SoloistStart> {
    let behavior = &self.user_config.behavior;
    if behavior.playback_engine != crate::core::user_config::PlaybackEngine::Soloist
      || !behavior.enable_streaming
      || self.native_backend_pending
      || crate::infra::soloist::supervisor::running()
    {
      return None;
    }
    let start = crate::infra::soloist::startup::SoloistStart::from_behavior(
      behavior,
      self.runtime_state.volume_percent,
      None,
      false,
    );
    self.soloist_starting();
    Some(start)
  }

  pub(crate) fn stand_by_librespot(
    &mut self,
    recovery_tx: tokio::sync::mpsc::UnboundedSender<crate::infra::player::StreamingRecoveryRequest>,
  ) {
    self.librespot_standby = Some(recovery_tx);
  }

  /// The settings name Soloist, and this system has a Soloist build.
  pub(crate) fn soloist_engine_selected(&self) -> bool {
    self.user_config.behavior.playback_engine == crate::core::user_config::PlaybackEngine::Soloist
      && !cfg!(all(target_os = "macos", target_arch = "x86_64"))
  }

  /// Playback Engine changed in Settings: switch now, without a restart.
  pub(crate) fn playback_engine_changed(&mut self) {
    if !self.user_config.behavior.enable_streaming {
      return;
    }
    let strings = self.strings().status.infra;
    let message = if self.soloist_engine_selected() {
      strings.engine_switching_soloist
    } else {
      strings.engine_switching_librespot
    };
    self.set_status_message(message, 4);
    self.dispatch_without_spinner(IoEvent::ApplyPlaybackEngine);
  }

  /// After Soloist stopped: give the device to the engine the settings name; the Soloist start, if any.
  pub(crate) fn apply_playback_engine(
    &mut self,
  ) -> Option<crate::infra::soloist::startup::SoloistStart> {
    if self.soloist_engine_selected() {
      self.stop_librespot_for_soloist();
      return self.soloist_start_now();
    }
    if let Some(recovery_tx) = self.librespot_standby.take() {
      self.streaming_recovery_tx = Some(recovery_tx);
    }
    if self.streaming_player.is_some() || !self.user_config.behavior.enable_streaming {
      return None;
    }
    // Soloist's id; librespot registers its own.
    self.native_device_id = None;
    self.native_backend_pending = self.streaming_recovery_tx.as_ref().is_some_and(|tx| {
      tx.send(crate::infra::player::StreamingRecoveryRequest::default())
        .is_ok()
    });
    if !self.native_backend_pending {
      let message = self.strings().status.infra.engine_next_launch;
      self.set_status_message(message, 8);
    }
    None
  }

  /// Shut librespot down before Soloist registers the same device name; keep its recovery sender.
  fn stop_librespot_for_soloist(&mut self) {
    if let Some(recovery_tx) = self.streaming_recovery_tx.take() {
      self.librespot_standby = Some(recovery_tx);
    }
    self.pause_native_playback();
    if let Some(player) = self.streaming_player.take() {
      player.shutdown();
      // The last handle joins librespot's player thread: drop it off the lock.
      tokio::task::spawn_blocking(move || drop(player));
    }
    self.native_parked = false;
    self.native_backend_pending = false;
    self.native_playback_recovery = None;
    self.native_load_watchdog = None;
    self.is_streaming_active = false;
    self.native_activation_pending = false;
    self.native_device_id = None;
    self.native_is_playing = Some(false);
    self.native_track_info = None;
    self.display_revisions.bump(DisplayDomain::Playback);
  }

  /// `soloist_credentials.yml`, beside `config.yml`.
  fn soloist_key_file(&self) -> Option<std::path::PathBuf> {
    let config = &self.user_config.path_to_config.as_ref()?.config_file_path;
    Some(config.parent()?.join("soloist_credentials.yml"))
  }

  pub(crate) fn saved_soloist_key(&self) -> String {
    self
      .soloist_key_file()
      .and_then(|path| crate::infra::soloist::auth::read_saved(&path))
      .unwrap_or_default()
  }

  /// Write a changed API key row, then restart the engine on it so a bad key shows at once;
  /// true when that restart is dispatched.
  pub(crate) fn save_soloist_key_row(&mut self) -> bool {
    let env_key = std::env::var(crate::infra::soloist::auth::KEY_ENV).ok();
    self.save_soloist_key_row_with(env_key)
  }

  fn save_soloist_key_row_with(&mut self, env_key: Option<String>) -> bool {
    use crate::infra::soloist::auth;
    let Some(key) = self
      .settings_items
      .iter()
      .find_map(|item| match &item.value {
        SettingValue::Secret(key) if item.id == "behavior.soloist_api_key" => {
          Some(key.trim().to_string())
        }
        _ => None,
      })
    else {
      return false;
    };
    let Some(path) = self.soloist_key_file() else {
      return false;
    };
    if key == self.saved_soloist_key().trim() {
      return false;
    }
    let strings = self.strings().status.infra;
    if let Err(e) = auth::save_key(&path, &key) {
      self.set_error_status_message(strings.soloist_key_save_failed(&e.to_string()), 10);
      return false;
    }
    if auth::key_with(env_key, None).is_some() {
      self.set_status_message(strings.soloist_key_env_wins, 10);
      return false;
    }
    if key.is_empty()
      || !self.soloist_engine_selected()
      || !self.user_config.behavior.enable_streaming
    {
      self.set_status_message(strings.soloist_key_saved, 4);
      return false;
    }
    self.set_status_message(strings.soloist_key_restarting, 4);
    self.dispatch_without_spinner(IoEvent::ApplyPlaybackEngine);
    true
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
      Notice::NotPaired => {
        self.set_status_message(strings.soloist_not_paired, 12);
        self.set_soloist_pairing(true);
      }
      Notice::Offline => self.set_status_message(strings.soloist_offline, 8),
      Notice::BadKey => self.set_error_status_message(strings.soloist_bad_key, 15),
      Notice::Expired => self.set_error_status_message(strings.soloist_expired, 20),
      Notice::ExpiresSoon { count, unit } => {
        self.set_status_message(
          strings.soloist_expires_soon(count, strings.soloist_expiry_unit(&unit)),
          10,
        );
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

  fn with_config(app: &mut App, dir: &std::path::Path) {
    app.user_config.path_to_config = Some(crate::core::user_config::UserConfigPaths {
      config_file_path: dir.join("config.yml"),
    });
  }

  fn app_with_config(dir: &std::path::Path) -> App {
    let mut app = make_app_simple();
    with_config(&mut app, dir);
    app
  }

  fn path_row(value: &str) -> crate::core::app::SettingItem {
    setting(
      "behavior.soloist_path",
      SettingValue::String(value.to_string()),
    )
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

  fn key_row(value: &str) -> crate::core::app::SettingItem {
    setting(
      "behavior.soloist_api_key",
      SettingValue::Secret(value.to_string()),
    )
  }

  fn soloist_app_with_config(dir: &std::path::Path) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (mut app, rx) = session_free_app();
    with_config(&mut app, dir);
    app.user_config.behavior.enable_streaming = true;
    app.user_config.behavior.playback_engine = PlaybackEngine::Soloist;
    (app, rx)
  }

  #[test]
  fn a_new_key_is_saved_beside_the_config_and_restarts_soloist() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, rx) = soloist_app_with_config(dir.path());
    app.settings_items = vec![key_row(" new-key ")];

    app.save_soloist_key_row_with(None);

    let file = dir.path().join("soloist_credentials.yml");
    assert_eq!(
      crate::infra::soloist::auth::read_saved(&file).as_deref(),
      Some("new-key")
    );
    assert_eq!(app.saved_soloist_key(), "new-key");
    assert!(matches!(rx.try_recv(), Ok(IoEvent::ApplyPlaybackEngine)));
  }

  #[test]
  fn an_unchanged_key_or_an_env_key_restarts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, rx) = soloist_app_with_config(dir.path());
    std::fs::write(
      dir.path().join("soloist_credentials.yml"),
      "api_key: same\n",
    )
    .unwrap();
    app.settings_items = vec![key_row("same")];
    app.save_soloist_key_row_with(None);
    assert!(rx.try_recv().is_err());

    app.settings_items = vec![key_row("other")];
    app.save_soloist_key_row_with(Some("env-key".to_string()));
    assert_eq!(app.saved_soloist_key(), "other");
    assert!(rx.try_recv().is_err());
    assert_eq!(
      app.status_message(),
      Some(app.strings().status.infra.soloist_key_env_wins)
    );
  }

  #[test]
  fn the_key_row_offers_the_dashboard_button() {
    let app = make_app_simple();
    assert_eq!(
      app.setting_button_label("behavior.soloist_api_key"),
      Some(("Get a key", false))
    );
  }

  #[test]
  fn the_pairing_guide_shows_until_soloist_logs_in() {
    let mut app = make_app_simple();
    let (link, _rx) = SoloistLink::test_pair("soloist-device");
    app.install_soloist_link(link.clone());
    app.soloist_notice(crate::infra::soloist::Notice::NotPaired);
    let device = app.user_config.behavior.streaming_device_name.clone();
    assert_eq!(app.soloist_pairing_device(), Some(device.as_str()));

    let auth = |logged_in| Event::Auth {
      logged_in,
      is_active: false,
    };
    app.apply_soloist_event(&link, &auth(false));
    assert!(app.soloist_pairing_device().is_some());
    app.apply_soloist_event(&link, &auth(true));
    assert_eq!(app.soloist_pairing_device(), None);
    assert_eq!(
      app.status_message(),
      Some(app.strings().status.infra.soloist_paired)
    );
  }

  #[test]
  fn an_engine_change_in_settings_dispatches_the_switch() {
    let (mut app, rx) = session_free_app();
    app.user_config.behavior.enable_streaming = true;
    app.settings_items = vec![setting(
      "behavior.playback_engine",
      SettingValue::Cycle(
        PlaybackEngine::Soloist.name().to_string(),
        PlaybackEngine::options(),
      ),
    )];

    app.apply_settings_changes();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::ApplyPlaybackEngine)));
    assert_eq!(
      app.status_message(),
      Some(app.strings().status.infra.engine_switching_soloist)
    );
  }

  #[test]
  fn a_switch_to_soloist_keeps_the_librespot_sender_aside_and_starts_soloist() {
    let mut app = make_app_simple();
    let (tx, _recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    app.streaming_recovery_tx = Some(tx);
    app.native_device_id = Some("librespot-device".to_string());
    app.user_config.behavior.enable_streaming = true;
    app.user_config.behavior.playback_engine = PlaybackEngine::Soloist;

    let start = app.apply_playback_engine();

    assert!(start.is_some());
    assert!(app.streaming_recovery_tx.is_none());
    assert!(app.librespot_standby.is_some());
    assert_eq!(app.native_device_id, None);
    assert!(app.native_backend_pending);
    assert!(!app.accept_rebuilt_native_backend());
  }

  #[test]
  fn a_switch_to_librespot_asks_the_standby_supervisor_for_an_idle_player() {
    let mut app = make_app_simple();
    let (tx, mut recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    app.stand_by_librespot(tx);
    app.native_device_id = Some("soloist-device".to_string());
    app.user_config.behavior.enable_streaming = true;

    assert!(app.apply_playback_engine().is_none());

    let request = recovery_rx.try_recv().unwrap();
    assert!(!request.restore_playback && !request.reselect_device && !request.reacquire);
    assert!(app.streaming_recovery_tx.is_some());
    assert_eq!(app.native_device_id, None);
    assert!(app.native_backend_pending);
  }

  #[test]
  fn the_soloist_offer_shows_once_and_set_up_opens_the_playback_engine_row() {
    let mut app = make_app_simple();
    app.offer_soloist_setup();
    assert!(app.soloist_offer_shows());

    app.apply(Action::AnswerSoloistOffer { set_up: true });

    assert!(!app.soloist_offer_shows());
    assert_eq!(
      app.get_current_route().id,
      crate::core::app::RouteId::Settings
    );
    assert_eq!(
      app.settings_items[app.view.settings_selected_index].id,
      "behavior.playback_engine"
    );
    app.offer_soloist_setup();
    assert!(!app.soloist_offer_shows());
  }

  #[test]
  fn soloist_is_not_offered_when_it_is_already_the_engine() {
    let mut app = make_app_simple();
    app.user_config.behavior.playback_engine = PlaybackEngine::Soloist;
    app.offer_soloist_setup();
    assert!(!app.soloist_offer_shows());
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
