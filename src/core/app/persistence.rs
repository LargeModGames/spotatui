use super::*;

/// Debounce window for coalescing hot-path runtime-state saves (volume,
/// shuffle) into one disk write, and the retry delay after a failed save.
const STATE_SAVE_DEBOUNCE_MS: u64 = 500;

/// Ungated copy of the `radio:` scheme: favoriting persists stations in
/// builds without the radio player too.
const RADIO_URI_PREFIX: &str = "radio:";

/// The trimmed stream URL behind a `radio:` URI.
fn radio_stream_url(uri: &str) -> Option<&str> {
  uri
    .strip_prefix(RADIO_URI_PREFIX)
    .map(str::trim)
    .filter(|url| !url.is_empty())
}

impl App {
  /// Snapshot the currently-playing non-Spotify session for persistence, or
  /// `None` when Spotify (or nothing) owns playback. Reads the live position and
  /// pause state straight from whichever source's player is active, mirroring
  /// the source-ownership order the runner tick uses. Spotify playback is not
  /// persisted here — its resume is handled by `startup_behavior` device logic.
  /// The resume point (`resume_index`, `resume_position_ms`) recorded when the
  /// active decoded context was suspended under the native queue, or `None` when
  /// no decoded context is suspended. Only one context is ever active, so this
  /// unambiguously describes it.
  #[cfg(any(
    feature = "youtube",
    feature = "subsonic",
    feature = "qobuz",
    feature = "local-files"
  ))]
  fn suspended_resume(&self) -> Option<(Option<usize>, u64)> {
    match self.queue_suspended.as_ref()? {
      #[cfg(feature = "local-files")]
      crate::core::queue::SuspendedContext::Local {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[cfg(feature = "subsonic")]
      crate::core::queue::SuspendedContext::Subsonic {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[cfg(feature = "qobuz")]
      crate::core::queue::SuspendedContext::Qobuz {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[cfg(feature = "youtube")]
      crate::core::queue::SuspendedContext::YouTube {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[allow(unreachable_patterns)]
      _ => None,
    }
  }

  pub fn current_persisted_playback(
    &self,
  ) -> Option<crate::core::persisted_playback::PersistedPlayback> {
    if !self.has_persistable_playback() {
      return None;
    }
    #[cfg(any(
      feature = "youtube",
      feature = "subsonic",
      feature = "qobuz",
      feature = "local-files",
      feature = "internet-radio"
    ))]
    use crate::core::persisted_playback::PersistedPlayback;
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_ref() {
      // While suspended under the queue, the context's player is playing a
      // *queued* track, so read the resume point from `queue_suspended` instead
      // of the (repurposed) live player. A `None` resume_index means the context
      // was exhausted — don't persist it (the queue itself still persists).
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::YouTube {
            tracks: s.tracks.clone(),
            index,
            position_ms,
            paused: false,
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::YouTube {
            tracks: s.tracks.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
      }
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_ref() {
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::Subsonic {
            tracks: s.tracks.clone(),
            index,
            position_ms,
            paused: false,
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::Subsonic {
            tracks: s.tracks.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
      }
    }
    #[cfg(feature = "qobuz")]
    if let Some(s) = self.qobuz_playback.as_ref() {
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::Qobuz {
            tracks: s.tracks.clone(),
            index,
            position_ms,
            paused: false,
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::Qobuz {
            tracks: s.tracks.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
      }
    }
    #[cfg(feature = "local-files")]
    if let Some(s) = self.local_playback.as_ref() {
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::Local {
            queue: s.queue.clone(),
            index,
            position_ms,
            paused: false,
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::Local {
            queue: s.queue.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
            repeat: self.decoded_repeat,
            shuffle_on: self.decoded_shuffle,
            shuffle: s.shuffle_backup.clone(),
          });
        }
      }
    }
    #[cfg(feature = "internet-radio")]
    if let Some(s) = self.radio_playback.as_ref() {
      return Some(PersistedPlayback::Radio {
        station: s.station.clone(),
        paused: s.player.is_paused(),
      });
    }
    None
  }

  /// Snapshot the full session to persist: the active non-Spotify playback (if
  /// any) plus the native queue. Returns `None` only when there is nothing to
  /// save (no active source *and* an empty queue), so the caller clears the
  /// session file — preserving the existing Some→None clear semantics.
  pub fn current_persisted_session(
    &self,
  ) -> Option<crate::core::persisted_playback::PersistedSession> {
    let playback = self.current_persisted_playback();
    let queue_now_track = self.queue_now_track();
    if playback.is_none() && self.native_queue.is_empty() && queue_now_track.is_none() {
      return None;
    }
    // A track playing through the queue slot has been popped off `native_queue`;
    // prepend it so a mid-queue quit resumes it (it re-enters the queue on the
    // next launch).
    let mut queue = self.native_queue.clone();
    if let Some(track) = queue_now_track {
      queue.insert(0, track.clone());
    }
    Some(crate::core::persisted_playback::PersistedSession { playback, queue })
  }

  /// Cheap presence check for `current_persisted_session`: same decision
  /// logic, no snapshot allocation. Lets the tick loop skip building the
  /// snapshot (which clones the whole native queue) unless a save is due.
  pub fn has_persistable_session(&self) -> bool {
    self.has_persistable_playback()
      || !self.native_queue.is_empty()
      || self.queue_now_track().is_some()
  }

  fn has_persistable_playback(&self) -> bool {
    // Mirrors `current_persisted_playback`: a decoded context suspended past
    // its end (resume_index == None) does not persist.
    #[cfg(any(
      feature = "youtube",
      feature = "subsonic",
      feature = "qobuz",
      feature = "local-files"
    ))]
    {
      let context_resumable = !matches!(self.suspended_resume(), Some((None, _)));
      #[cfg(feature = "youtube")]
      if self.youtube_playback.is_some() && context_resumable {
        return true;
      }
      #[cfg(feature = "subsonic")]
      if self.subsonic_playback.is_some() && context_resumable {
        return true;
      }
      #[cfg(feature = "qobuz")]
      if self.qobuz_playback.is_some() && context_resumable {
        return true;
      }
      #[cfg(feature = "local-files")]
      if self.local_playback.is_some() && context_resumable {
        return true;
      }
    }
    #[cfg(feature = "internet-radio")]
    if self.radio_playback.is_some() {
      return true;
    }
    false
  }

  /// Schedule a debounced state save. Hot paths (volume, shuffle) call this
  /// instead of `save_runtime_state()` so a held key doesn't pay
  /// disk + YAML work on every auto-repeat; the save lands once, shortly
  /// after the last change.
  pub fn schedule_state_save(&mut self, patch: PersistedRuntimeState) {
    if patch.is_empty() {
      return;
    }
    self.pending_state_save_patch.merge_patch(&patch);
    self.state_save_due = Some(Instant::now() + Duration::from_millis(STATE_SAVE_DEBOUNCE_MS));
  }

  pub fn save_runtime_state(&self, patch: &PersistedRuntimeState) -> anyhow::Result<()> {
    if patch.is_empty() {
      return Ok(());
    }
    let path = match &self.state_path {
      Some(path) => path.clone(),
      None => crate::core::state::default_state_path()?,
    };
    crate::core::state::save(&path, patch)
  }

  fn save_removed_radio_station(&self, url: &str) -> anyhow::Result<()> {
    let path = match &self.state_path {
      Some(path) => path.clone(),
      None => crate::core::state::default_state_path()?,
    };
    crate::core::state::save_removing_radio_station(&path, url)
  }

  pub fn add_radio_station(
    &mut self,
    name: impl AsRef<str>,
    url: impl AsRef<str>,
  ) -> anyhow::Result<RadioStationAddOutcome> {
    if self.is_configured_radio_station_url(url.as_ref()) {
      return Ok(RadioStationAddOutcome::AlreadyExists);
    }
    let before_len = self.runtime_state.radio_stations.len();
    let outcome = self.runtime_state.add_radio_station(name, url)?;
    if outcome == RadioStationAddOutcome::Added {
      let Some(station) = self.runtime_state.radio_stations.get(before_len).cloned() else {
        // `Added` promises the station was appended at `before_len`; if it is
        // not there, don't report success for a station that was never saved.
        self.runtime_state.radio_stations.truncate(before_len);
        return Err(anyhow!(
          "added radio station was not found at the expected position; not persisted"
        ));
      };
      if let Err(error) = self.save_runtime_state(&PersistedRuntimeState::radio_station(station)) {
        self.runtime_state.radio_stations.truncate(before_len);
        return Err(error);
      }
    }
    Ok(outcome)
  }

  pub fn is_configured_radio_station_url(&self, url: &str) -> bool {
    let url = url.trim();
    self.is_config_owned_radio_station_url(url) && !self.is_state_owned_radio_station_url(url)
  }

  pub fn is_config_owned_radio_station_url(&self, url: &str) -> bool {
    let url = url.trim();
    !url.is_empty()
      && self
        .user_config
        .behavior
        .radio_stations
        .iter()
        .any(|station| station.url.trim() == url)
  }

  pub fn is_state_owned_radio_station_url(&self, url: &str) -> bool {
    let url = url.trim();
    !url.is_empty()
      && self
        .runtime_state
        .radio_stations
        .iter()
        .any(|station| station.url.trim() == url)
  }

  pub fn remove_radio_station_by_url(
    &mut self,
    url: impl AsRef<str>,
  ) -> anyhow::Result<Option<RadioStationConfig>> {
    let url = url.as_ref().trim().to_string();
    let Some(index) = self
      .runtime_state
      .radio_stations
      .iter()
      .position(|station| station.url.trim() == url)
    else {
      return Ok(None);
    };
    let removed = self.runtime_state.remove_radio_station_by_url(&url)?;
    if let Err(error) = self.save_removed_radio_station(&url) {
      if let Some(removed) = removed {
        self.runtime_state.radio_stations.insert(index, removed);
      }
      return Err(error);
    }
    Ok(removed)
  }

  fn add_station_to_sidebar(&mut self, mut station: TrackInfo, name: String, url: &str) {
    let uri = format!("{}{url}", RADIO_URI_PREFIX);
    if self
      .radio_stations
      .iter()
      .any(|existing| existing.uri.as_deref() == Some(uri.as_str()))
    {
      return;
    }

    station.name = name;
    station.uri = Some(uri);
    self.radio_stations.push(station);
    if self.view.selected_playlist_index.is_none() {
      self.view.selected_playlist_index = Some(0);
    }
  }

  /// Favorite one radio station: persist it and mirror it into the sidebar.
  pub(crate) fn favorite_radio_station(&mut self, station: TrackInfo) {
    let Some(url) = station.uri.as_deref().and_then(radio_stream_url) else {
      self.set_status_message("Radio station has no stream URL".to_string(), 4);
      return;
    };

    let trimmed = station.name.trim();
    let name = if trimmed.is_empty() { url } else { trimmed }.to_string();
    let url = url.to_string();

    let message = match self.add_radio_station(&name, &url) {
      Ok(RadioStationAddOutcome::Added) => format!("Favorited radio station: {name}"),
      Ok(RadioStationAddOutcome::AlreadyExists) => {
        format!("Radio station already favorited: {name}")
      }
      Err(error) => {
        self.set_error_status_message(format!("Could not favorite radio station: {error}"), 6);
        return;
      }
    };

    self.add_station_to_sidebar(station, name, &url);
    self.set_status_message(message, 4);
  }

  /// Remove a saved radio station by `radio:` URI; a config.yml station is
  /// reported, not removed.
  pub(crate) fn remove_saved_radio_station(&mut self, uri: String) {
    let Some(url) = radio_stream_url(&uri) else {
      self.set_status_message("Radio station has no stream URL".to_string(), 4);
      return;
    };
    let station_name = |app: &Self| {
      app
        .radio_stations
        .iter()
        .find(|station| station.uri.as_deref() == Some(uri.as_str()))
        .map(|station| station.name.clone())
        .unwrap_or_default()
    };
    let config_owned = self.is_config_owned_radio_station_url(url);
    if config_owned && !self.is_state_owned_radio_station_url(url) {
      let name = station_name(self);
      self.set_status_message(
        format!("Radio station is configured in config.yml: {name}"),
        4,
      );
      return;
    }

    match self.remove_radio_station_by_url(url) {
      Ok(Some(removed)) => {
        if !config_owned {
          self
            .radio_stations
            .retain(|station| station.uri.as_deref() != Some(uri.as_str()));
        }
        // The saved copy's name, not the sidebar row's: a station configured in
        // config.yml keeps its configured name while its saved duplicate goes.
        self.set_status_message(format!("Removed saved radio station: {}", removed.name), 4);
      }
      Ok(None) => {
        let name = station_name(self);
        self.set_status_message(format!("Radio station is not favorited: {name}"), 4);
      }
      Err(error) => {
        self.set_error_status_message(format!("Could not remove radio station: {error}"), 6);
      }
    }
  }

  /// Flush a scheduled state save once its debounce window has passed, or
  /// immediately when `force` is set (shutdown).
  pub fn flush_state_save(&mut self, force: bool) {
    let Some(due) = self.state_save_due else {
      return;
    };
    if force || Instant::now() >= due {
      let patch = std::mem::take(&mut self.pending_state_save_patch);
      self.state_save_due = None;
      match self.save_runtime_state(&patch) {
        Ok(()) => self.state_save_error_reported = false,
        Err(e) => {
          self.pending_state_save_patch.merge_patch(&patch);
          // Keep the save scheduled: with the deadline cleared the retained patch
          // would never be retried (this fn returns early on a `None` deadline),
          // and the forced shutdown flush would silently drop it.
          self.state_save_due =
            Some(Instant::now() + Duration::from_millis(STATE_SAVE_DEBOUNCE_MS));
          // Reported to the status bar, not as an error page: the tick calls
          // this every frame and the retry above re-arms twice a second, so as
          // a modal an unwritable state dir is an unusable app, and as an
          // `api_error` it is one no frontend can clear because every retry
          // restamps the lifetime. Latched to one report per failure run, or
          // the same rate would pin the status bar into error mode and drop
          // every ordinary message for the rest of the session.
          if !self.state_save_error_reported {
            self.state_save_error_reported = true;
            self.set_error_status_message(format!("Failed to save state: {}", e), 8);
          }
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  /// An `App` whose state saves always fail: the save path's parent is a
  /// regular file, so creating the directory for it cannot succeed on any
  /// platform.
  fn app_with_unwritable_state_path() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").unwrap();

    let mut app = make_app_simple();
    app.state_path = Some(blocker.join("state.yml"));
    (app, dir)
  }

  fn schedule_a_save(app: &mut App) {
    app.pending_state_save_patch = PersistedRuntimeState {
      volume_percent: Some(42),
      ..PersistedRuntimeState::default()
    };
    app.state_save_due = Some(Instant::now());
  }

  #[test]
  fn a_failed_state_save_reports_to_the_status_bar_instead_of_the_error_page() {
    let (mut app, _dir) = app_with_unwritable_state_path();
    schedule_a_save(&mut app);

    app.flush_state_save(false);

    assert!(app
      .status_message
      .as_deref()
      .is_some_and(|message| message.starts_with("Failed to save state")));
    assert!(app.status_message_is_error);
    // Not the modal: the tick retries twice a second, so an error page here
    // is an app the user cannot escape.
    assert!(app.api_error.is_empty());
    assert_ne!(app.get_current_route().id, RouteId::Error);
  }

  // The regression the latch exists to prevent: an unlatched report re-fires
  // at the tick's retry rate, and a live error message holds
  // `status_message_is_error`, which makes `set_status_message` drop every
  // ordinary message for the rest of the session.
  //
  // Asserted through a sentinel rather than by expiring the message and
  // watching the bar recover: backdating `status_message_expires_at` opens the
  // priority guard whether or not the latch is there, so that version of this
  // test passes with the latch deleted. A second report would replace the
  // sentinel, because an error always overwrites a live error.
  #[test]
  fn repeated_state_save_failures_report_only_once() {
    let (mut app, _dir) = app_with_unwritable_state_path();
    schedule_a_save(&mut app);
    app.flush_state_save(false);
    app.set_error_status_message("sentinel", 8);

    // The retry the failed flush re-armed, as the tick would drive it.
    app.state_save_due = Some(Instant::now());
    app.flush_state_save(false);

    assert_eq!(app.status_message.as_deref(), Some("sentinel"));
  }

  #[test]
  fn a_successful_state_save_re_arms_the_failure_report() {
    let (mut app, dir) = app_with_unwritable_state_path();
    schedule_a_save(&mut app);
    app.flush_state_save(false);
    assert!(app.state_save_error_reported);

    app.state_path = Some(dir.path().join("state.yml"));
    app.state_save_due = Some(Instant::now());
    app.flush_state_save(false);

    assert!(!app.state_save_error_reported);
  }
}
