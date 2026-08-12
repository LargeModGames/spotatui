use super::*;

/// How long to ignore position updates after a seek (ms)
/// This prevents the UI from jumping back to old positions while the seek completes
pub const SEEK_POSITION_IGNORE_MS: u128 = 500;

impl App {
  pub(super) fn apply_seek(&mut self, seek_ms: u32) {
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
        _ => return,
      };

      let event = if seek_ms < duration_ms {
        IoEvent::Seek(seek_ms)
      } else {
        IoEvent::NextTrack
      };

      self.dispatch(event);
    }
  }

  pub fn seek_forwards(&mut self) {
    info!(
      "seeking forwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    // A seekable decoded source (local/subsonic/youtube) owns the session: seek
    // relative to *its* live position, never from the stale/foreign Spotify
    // progress. Radio returns None here, so its seek keys are correct no-ops.
    // The source player clamps to the track duration internally, so no upper
    // clamp is needed (and we must not read the stale Spotify context duration).
    if let Some(pos) = self.active_source_position_ms() {
      let new_progress = (pos as u32).saturating_add(self.user_config.behavior.seek_milliseconds);
      self.song_progress_ms = new_progress as u128;
      self.seek_ms = None;
      self.dispatch(IoEvent::Seek(new_progress));
      return;
    }
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
        _ => return,
      };

      let old_progress = match self.seek_ms {
        Some(seek_ms) => seek_ms,
        None => self.song_progress_ms,
      };

      let new_progress = min(
        old_progress as u32 + self.user_config.behavior.seek_milliseconds,
        duration_ms,
      );

      self.seek_ms = Some(new_progress as u128);

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() && self.streaming_player.is_some() {
        // Always update UI immediately
        self.song_progress_ms = new_progress as u128;
        self.seek_ms = None;

        // Throttle actual seeks to avoid overwhelming librespot (max ~20/sec)
        const SEEK_THROTTLE_MS: u128 = 50;
        let should_seek_now = self
          .last_native_seek
          .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

        if should_seek_now {
          self.execute_native_seek(new_progress);
        } else {
          // Queue the seek - will be flushed by tick loop or next seek
          self.pending_native_seek = Some(new_progress);
        }
        return;
      }

      // Fallback: API-based seek for external devices (with throttling)
      self.queue_api_seek(new_progress);
    }
  }

  pub fn seek_backwards(&mut self) {
    info!(
      "seeking backwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    // A seekable decoded source (local/subsonic/youtube) owns the session: seek
    // relative to *its* live position, never from the stale/foreign Spotify
    // progress. Radio returns None here, so its seek keys are correct no-ops.
    if let Some(pos) = self.active_source_position_ms() {
      let new_progress = (pos as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
      self.song_progress_ms = new_progress as u128;
      self.seek_ms = None;
      self.dispatch(IoEvent::Seek(new_progress));
      return;
    }
    let old_progress = match self.seek_ms {
      Some(seek_ms) => seek_ms,
      None => self.song_progress_ms,
    };
    let new_progress =
      (old_progress as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
    self.seek_ms = Some(new_progress as u128);

    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() && self.streaming_player.is_some() {
      // Always update UI immediately
      self.song_progress_ms = new_progress as u128;
      self.seek_ms = None;

      // Throttle actual seeks to avoid overwhelming librespot (max ~20/sec)
      const SEEK_THROTTLE_MS: u128 = 50;
      let should_seek_now = self
        .last_native_seek
        .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

      if should_seek_now {
        self.execute_native_seek(new_progress);
      } else {
        // Queue the seek - will be flushed by tick loop or next seek
        self.pending_native_seek = Some(new_progress);
      }
      return;
    }

    // Fallback: API-based seek for external devices (with throttling)
    self.queue_api_seek(new_progress);
  }

  /// Seek to an absolute position within the current track (e.g. from clicking or
  /// dragging on the playbar progress line). The target is clamped to the track
  /// duration. Mirrors the dispatch logic of [`Self::seek_forwards`].
  pub fn seek_to(&mut self, position_ms: u32) {
    // A seekable decoded source (local/subsonic/youtube) owns the session: seek
    // it to the absolute target directly (the source player clamps to the track
    // duration internally). Radio returns None here, so its playbar drags are
    // correct no-ops. Never read the stale Spotify context duration for a source.
    if self.active_source_position_ms().is_some() {
      // Decoded `.seek()` can re-decode forward for many codecs, so a mouse
      // drag must not dispatch one seek per drag event; coalesce to the last
      // target with the same throttle-and-flush pattern as the other backends.
      self.queue_source_seek(position_ms);
      return;
    }
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
        _ => return,
      };

      let new_progress = position_ms.min(duration_ms);
      self.seek_ms = Some(new_progress as u128);

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() && self.streaming_player.is_some() {
        // Always update UI immediately
        self.song_progress_ms = new_progress as u128;
        self.seek_ms = None;

        // Throttle actual seeks to avoid overwhelming librespot (max ~20/sec)
        const SEEK_THROTTLE_MS: u128 = 50;
        let should_seek_now = self
          .last_native_seek
          .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

        if should_seek_now {
          self.execute_native_seek(new_progress);
        } else {
          // Queue the seek - will be flushed by tick loop or next seek
          self.pending_native_seek = Some(new_progress);
        }
        return;
      }

      // Fallback: API-based seek for external devices (with throttling)
      self.queue_api_seek(new_progress);
    }
  }

  /// Queue a decoded-source (local/Subsonic/YouTube) seek with throttling
  fn queue_source_seek(&mut self, position_ms: u32) {
    // Always update UI immediately
    self.song_progress_ms = position_ms as u128;
    self.seek_ms = None;

    const SOURCE_SEEK_THROTTLE_MS: u128 = 50;
    let should_seek_now = self
      .last_source_seek
      .is_none_or(|t| t.elapsed().as_millis() >= SOURCE_SEEK_THROTTLE_MS);

    if should_seek_now {
      self.execute_source_seek(position_ms);
    } else {
      // Queue the seek - will be flushed by tick loop or next seek
      self.pending_source_seek = Some(position_ms);
    }
  }

  /// Execute a decoded-source seek and update tracking state
  fn execute_source_seek(&mut self, position_ms: u32) {
    self.pending_source_seek = None;
    self.last_source_seek = Some(Instant::now());
    self.dispatch(IoEvent::Seek(position_ms));
  }

  /// Flush any pending decoded-source seek (called from tick loop)
  pub fn flush_pending_source_seek(&mut self) {
    if let Some(position) = self.pending_source_seek {
      const SOURCE_SEEK_THROTTLE_MS: u128 = 50;
      let should_flush = self
        .last_source_seek
        .is_none_or(|t| t.elapsed().as_millis() >= SOURCE_SEEK_THROTTLE_MS);

      if should_flush {
        self.execute_source_seek(position);
      }
    }
  }

  /// Queue an API-based seek with throttling (for external device control)
  fn queue_api_seek(&mut self, position_ms: u32) {
    // Always update UI immediately
    self.song_progress_ms = position_ms as u128;
    self.seek_ms = None;

    // Start the ignore window immediately when the user requests a seek
    // This prevents position updates from overwriting our target while waiting
    let now = Instant::now();

    // Mark poll data as stale so resync won't happen after ignore window
    self.instant_since_last_current_playback_poll = now;

    // Throttle API calls (max ~5/sec to respect rate limits)
    const API_SEEK_THROTTLE_MS: u128 = 200;
    let should_seek_now = self
      .last_api_seek
      .is_none_or(|t| t.elapsed().as_millis() >= API_SEEK_THROTTLE_MS);

    // Update last_api_seek for BOTH the ignore window AND throttling
    // This ensures the ignore window starts immediately on any seek request
    self.last_api_seek = Some(now);

    if should_seek_now {
      self.execute_api_seek(position_ms);
    } else {
      // Queue the seek - will be flushed by tick loop
      self.pending_api_seek = Some(position_ms);
    }
  }

  /// Execute an API-based seek
  fn execute_api_seek(&mut self, position_ms: u32) {
    self.pending_api_seek = None;
    self.apply_seek(position_ms);
  }

  /// Flush any pending API seek (called from tick loop)
  pub fn flush_pending_api_seek(&mut self) {
    if let Some(position) = self.pending_api_seek {
      const API_SEEK_THROTTLE_MS: u128 = 200;
      let should_flush = self
        .last_api_seek
        .is_none_or(|t| t.elapsed().as_millis() >= API_SEEK_THROTTLE_MS);

      if should_flush {
        self.execute_api_seek(position);
      }
    }
  }

  /// Execute a native seek and update tracking state
  #[cfg(feature = "streaming")]
  fn execute_native_seek(&mut self, position_ms: u32) {
    if let Some(player) = self.streaming_player.clone() {
      player.seek(position_ms);
      self.last_native_seek = Some(Instant::now());
      self.pending_native_seek = None;
      self.set_native_recovery_position(position_ms);

      // Notify MPRIS clients that position jumped
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      if let Some(ref mpris) = self.mpris_manager {
        mpris.emit_seeked(position_ms as u64);
      }
    }
  }

  /// Flush any pending native seek (called from tick loop)
  #[cfg(feature = "streaming")]
  pub fn flush_pending_native_seek(&mut self) {
    if let Some(position) = self.pending_native_seek {
      // Only flush if enough time has passed since last seek
      const SEEK_THROTTLE_MS: u128 = 50;
      let should_flush = self
        .last_native_seek
        .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

      if should_flush {
        self.execute_native_seek(position);
      }
    }
  }
}
