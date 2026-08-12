use super::*;

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
  #[cfg(any(feature = "youtube", feature = "subsonic", feature = "local-files"))]
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
    #[cfg(any(feature = "youtube", feature = "subsonic", feature = "local-files"))]
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
    const STATE_SAVE_DEBOUNCE_MS: u64 = 500;
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
        return Ok(outcome);
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

  /// Flush a scheduled state save once its debounce window has passed, or
  /// immediately when `force` is set (shutdown).
  pub fn flush_state_save(&mut self, force: bool) {
    let Some(due) = self.state_save_due else {
      return;
    };
    if force || Instant::now() >= due {
      let patch = std::mem::take(&mut self.pending_state_save_patch);
      self.state_save_due = None;
      if let Err(e) = self.save_runtime_state(&patch) {
        self.pending_state_save_patch.merge_patch(&patch);
        self.handle_error(anyhow!("Failed to save state: {}", e));
      }
    }
  }
}
