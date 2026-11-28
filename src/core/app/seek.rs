use super::*;

/// How long to ignore position updates after a seek (ms)
/// This prevents the UI from jumping back to old positions while the seek completes
pub const SEEK_POSITION_IGNORE_MS: u128 = 500;

const API_SEEK_THROTTLE: Duration = Duration::from_millis(200);
#[cfg(feature = "streaming")]
const NATIVE_SEEK_THROTTLE: Duration = Duration::from_millis(50);

pub(super) struct ApiSeek {
  request: Arc<SpotifySeekRequest>,
  at: Instant,
  pending: bool,
}

impl App {
  pub(super) fn apply_seek(&mut self, seek_ms: u32) {
    // The audible track's duration decides between a seek and a skip; a
    // decoded owner seeks through its own latch and gets nothing here.
    let Some(duration_ms) = self.playing_duration_ms() else {
      return;
    };

    let command = if seek_ms < duration_ms {
      SpotifySeekCommand::Position(seek_ms)
    } else {
      SpotifySeekCommand::NextTrack
    };
    if let Some(target) = self.spotify_seek_target() {
      self.queue_api_seek(target, command, seek_ms, false);
    } else {
      self.try_dispatch(match command {
        SpotifySeekCommand::Position(position_ms) => IoEvent::Seek(position_ms),
        SpotifySeekCommand::NextTrack => IoEvent::NextTrack,
      });
    }
  }

  /// The length of the Spotify item that plays, or `None` under another owner.
  pub(super) fn playing_duration_ms(&self) -> Option<u32> {
    match self.playing_item() {
      PlayingItem::Spotify(PlayableItem::Track(track)) => {
        Some(track.duration.num_milliseconds() as u32)
      }
      PlayingItem::Spotify(PlayableItem::Episode(episode)) => {
        Some(episode.duration.num_milliseconds() as u32)
      }
      PlayingItem::QueuedSpotify(track) => {
        Some(u32::try_from(track.duration_ms).unwrap_or(u32::MAX))
      }
      PlayingItem::Spotify(_) | PlayingItem::NotSpotify | PlayingItem::Nothing => None,
    }
  }

  pub fn seek_forwards(&mut self) {
    if self.apple_music_owns_playback() {
      self.seek_to(
        self
          .apple_music_position_ms()
          .saturating_add(self.user_config.behavior.seek_milliseconds),
      );
      return;
    }
    info!(
      "seeking forwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    // A decoded source owns the session: seek relative to *its* live position,
    // never from the stale/foreign Spotify progress. The source player clamps
    // to the track duration internally, so no upper clamp is needed (and we
    // must not read the stale Spotify context duration). Radio has no position
    // (not seekable): stop there instead of repositioning the paused Spotify
    // player underneath it.
    if self.active_decoded_source() {
      if let Some(pos) = self.active_source_position_ms() {
        let new_progress = (pos as u32).saturating_add(self.user_config.behavior.seek_milliseconds);
        self.song_progress_ms = new_progress as u128;
        self.seek_ms = None;
        self.dispatch(IoEvent::Seek(new_progress));
      }
      return;
    }
    if let Some(duration_ms) = self.playing_duration_ms() {
      let old_progress = self.seek_ms.unwrap_or(self.song_progress_ms);
      let new_progress = min(
        (old_progress as u32).saturating_add(self.user_config.behavior.seek_milliseconds),
        duration_ms,
      );
      self.seek_spotify(new_progress);
    }
  }

  pub fn seek_backwards(&mut self) {
    if self.apple_music_owns_playback() {
      self.seek_to(
        self
          .apple_music_position_ms()
          .saturating_sub(self.user_config.behavior.seek_milliseconds),
      );
      return;
    }
    info!(
      "seeking backwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    // A decoded source owns the session: seek relative to *its* live position,
    // never from the stale/foreign Spotify progress. Radio has no position (not
    // seekable): stop there instead of repositioning the paused Spotify player
    // underneath it.
    if self.active_decoded_source() {
      if let Some(pos) = self.active_source_position_ms() {
        let new_progress = (pos as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
        self.song_progress_ms = new_progress as u128;
        self.seek_ms = None;
        self.dispatch(IoEvent::Seek(new_progress));
      }
      return;
    }
    let old_progress = self.seek_ms.unwrap_or(self.song_progress_ms);
    let new_progress =
      (old_progress as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
    self.seek_spotify(new_progress);
  }

  /// Seek to an absolute position within the current track (e.g. from clicking or
  /// dragging on the playbar progress line). The target is clamped to the track
  /// duration. Mirrors the dispatch logic of [`Self::seek_forwards`].
  pub fn seek_to(&mut self, position_ms: u32) {
    if self.apple_music_owns_playback() {
      self.seek_apple_music(position_ms);
      return;
    }
    // A decoded source owns the session: seek it to the absolute target directly
    // (the source player clamps to the track duration internally). Never read
    // the stale Spotify context duration for a source. Radio has no position
    // (not seekable): stop there instead of repositioning the paused Spotify
    // player underneath it.
    if self.active_decoded_source() {
      if self.active_source_position_ms().is_some() {
        // Decoded `.seek()` can re-decode forward for many codecs, so a mouse
        // drag must not dispatch one seek per drag event; coalesce to the last
        // target with the same throttle-and-flush pattern as the other backends.
        self.queue_source_seek(position_ms);
      }
      return;
    }
    if let Some(duration_ms) = self.playing_duration_ms() {
      self.seek_spotify(position_ms.min(duration_ms));
    }
  }

  fn seek_spotify(&mut self, position_ms: u32) {
    self.seek_ms = None;
    #[cfg(feature = "streaming")]
    if self.queue_now_is_spotify() || self.is_native_streaming_active_for_playback() {
      self.song_progress_ms = position_ms as u128;
      if self
        .last_native_seek
        .is_none_or(|at| at.elapsed() >= NATIVE_SEEK_THROTTLE)
      {
        self.execute_native_seek(position_ms);
      } else {
        self.pending_native_seek = Some(position_ms);
      }
      return;
    }
    if self.native_parked_here() {
      self.refuse_parked_gesture();
      return;
    }
    self.apply_seek(position_ms);
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
    // Queued for a decoded owner; once that owner is gone the value would
    // reach whoever holds the sink now.
    if self.apple_music_owns_playback() {
      self.flush_apple_music_seek();
      return;
    }
    if self.pending_source_seek.is_some() && !self.active_decoded_source() {
      self.pending_source_seek = None;
      return;
    }
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

  /// Explicitly restarting a track supersedes any drag and sends at once.
  pub(super) fn restart_spotify_track(&mut self) {
    self.api_seek = None;
    if let Some(target) = self.spotify_seek_target() {
      self.queue_api_seek(target, SpotifySeekCommand::Position(0), 0, true);
    } else {
      self.dispatch_spotify_fallback(IoEvent::Seek(0));
    }
  }

  /// The request's identity travels with it even after it leaves the App.
  fn queue_api_seek(
    &mut self,
    target: SpotifySeekTarget,
    command: SpotifySeekCommand,
    position_ms: u32,
    immediate: bool,
  ) {
    self.discard_stale_api_seek();
    self.seek_ms = None;
    self.song_progress_ms = position_ms as u128;
    let now = Instant::now();
    let should_seek_now = immediate
      || self
        .api_seek
        .as_ref()
        .is_none_or(|seek| now.duration_since(seek.at) >= API_SEEK_THROTTLE);
    let request = Arc::new(SpotifySeekRequest { target, command });
    // Start the position guard at input time, even if dispatch has to wait.
    self.api_seek = Some(ApiSeek {
      request: Arc::clone(&request),
      at: now,
      pending: !should_seek_now,
    });
    if should_seek_now {
      self.try_dispatch(IoEvent::SpotifySeek(request));
    }
  }

  pub(crate) fn spotify_seek_request_is_current(&self, request: &Arc<SpotifySeekRequest>) -> bool {
    self
      .api_seek
      .as_ref()
      .is_some_and(|seek| Arc::ptr_eq(&seek.request, request))
      && self.spotify_seek_is_current(&request.target)
  }

  pub(super) fn discard_stale_api_seek(&mut self) {
    if self
      .api_seek
      .as_ref()
      .is_some_and(|seek| !self.spotify_seek_is_current(&seek.request.target))
    {
      self.api_seek = None;
    }
  }

  pub(super) fn api_seek_settles_at(&self) -> Option<Instant> {
    self
      .api_seek
      .as_ref()
      .map(|seek| seek.at + Duration::from_millis(SEEK_POSITION_IGNORE_MS as u64))
  }

  #[cfg(feature = "soloist")]
  pub(super) fn settling_api_seek_at(&self) -> Option<Instant> {
    self
      .api_seek
      .as_ref()
      .filter(|seek| seek.at.elapsed().as_millis() < SEEK_POSITION_IGNORE_MS)
      .map(|seek| seek.at)
  }

  /// Flush a debounced seek only while its original item and device are current.
  pub fn flush_pending_api_seek(&mut self) {
    self.discard_stale_api_seek();
    if let Some(seek) = self
      .api_seek
      .as_mut()
      .filter(|seek| seek.pending && seek.at.elapsed() >= API_SEEK_THROTTLE)
    {
      seek.pending = false;
      seek.at = Instant::now();
      let request = Arc::clone(&seek.request);
      self.try_dispatch(IoEvent::SpotifySeek(request));
    }
  }

  /// Execute a native seek and update tracking state
  #[cfg(feature = "streaming")]
  fn execute_native_seek(&mut self, position_ms: u32) {
    if self.queue_now_is_spotify() {
      self.last_native_seek = Some(Instant::now());
      self.pending_native_seek = None;
      self.dispatch(IoEvent::Seek(position_ms));
      return;
    }
    if let Some(player) = self.streaming_player.clone() {
      player.seek(position_ms);
      self.pending_native_seek = None;
      self.record_native_seek(position_ms);

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
    // Queued for librespot; a decoded source that took the sink since leaves
    // librespot paused, and a seek would move the wrong player.
    if self.pending_native_seek.is_some() && self.active_decoded_source() {
      self.pending_native_seek = None;
      return;
    }
    if let Some(position) = self.pending_native_seek {
      // Only flush if enough time has passed since last seek
      let should_flush = self
        .last_native_seek
        .is_none_or(|t| t.elapsed() >= NATIVE_SEEK_THROTTLE);

      if should_flush {
        self.execute_native_seek(position);
      }
    }
  }
}

#[cfg(test)]
mod remote_seek_tests {
  use super::*;
  use crate::core::test_helpers::playing_with_art;

  const TRACK_ID: &str = "0000000000000000000001";

  fn context(track_id: &str) -> CurrentPlaybackContext {
    playing_with_art(track_id, "https://example.com/cover")
  }

  fn app_with_debounced_seek(position_ms: u32) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context(TRACK_ID));
    app.seek_to(5_000);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::SpotifySeek(request))
      if matches!(request.command, SpotifySeekCommand::Position(5_000))));
    app.seek_to(position_ms);
    assert!(rx.try_recv().is_err());
    app.api_seek.as_mut().unwrap().at -= API_SEEK_THROTTLE;
    (app, rx)
  }

  #[test]
  fn a_debounced_remote_seek_does_not_move_the_next_track() {
    // A seek to the old track's end would have dispatched a skip on flush.
    for position in [45_000, 180_000] {
      let (mut app, rx) = app_with_debounced_seek(position);
      app.current_playback_context = Some(context("0000000000000000000002"));

      app.flush_pending_api_seek();

      assert!(rx.try_recv().is_err());
      assert!(app.api_seek.is_none());
    }
  }

  #[test]
  fn a_debounced_remote_seek_does_not_follow_playback_to_another_device() {
    let (mut app, rx) = app_with_debounced_seek(45_000);
    let mut context = context(TRACK_ID);
    context.device.id = Some("another-device".to_string());
    app.set_polled_playback(context, Instant::now());

    app.flush_pending_api_seek();

    assert!(rx.try_recv().is_err());
    assert!(app.api_seek.is_none());
  }

  #[test]
  fn a_debounced_remote_seek_survives_a_fresh_poll_of_the_same_track() {
    let (mut app, rx) = app_with_debounced_seek(45_000);
    app.set_polled_playback(context(TRACK_ID), Instant::now());

    app.flush_pending_api_seek();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::SpotifySeek(request))
      if matches!(request.command, SpotifySeekCommand::Position(45_000))));
  }

  #[test]
  fn a_skip_cancels_a_debounced_remote_seek_before_the_next_poll() {
    let (mut app, rx) = app_with_debounced_seek(45_000);

    app.next_track();
    app.flush_pending_api_seek();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::NextTrack)));
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn restarting_a_remote_track_discards_the_waiting_drag_position() {
    let (mut app, rx) = app_with_debounced_seek(45_000);

    app.previous_track();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::SpotifySeek(request))
      if matches!(request.command, SpotifySeekCommand::Position(0))));
    app.flush_pending_api_seek();

    assert!(rx.try_recv().is_err(), "the drag overwrote the restart");
  }

  #[test]
  fn a_new_drag_supersedes_a_sent_seek_even_before_the_new_seek_flushes() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context(TRACK_ID));
    app.seek_to(5_000);
    let Ok(IoEvent::SpotifySeek(first)) = rx.try_recv() else {
      panic!("expected the first seek");
    };
    assert!(app.spotify_seek_request_is_current(&first));

    app.seek_to(45_000);

    assert!(rx.try_recv().is_err());
    assert!(!app.spotify_seek_request_is_current(&first));
    app.api_seek.as_mut().unwrap().at -= API_SEEK_THROTTLE;
    app.flush_pending_api_seek();
    let Ok(IoEvent::SpotifySeek(latest)) = rx.try_recv() else {
      panic!("expected the last seek");
    };
    assert!(app.spotify_seek_request_is_current(&latest));
    assert!(matches!(
      latest.command,
      SpotifySeekCommand::Position(45_000)
    ));
  }

  #[test]
  fn a_remote_seek_to_the_end_still_throttles_followup_input() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context(TRACK_ID));

    app.seek_to(180_000);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::SpotifySeek(request))
      if matches!(request.command, SpotifySeekCommand::NextTrack)));
    app.seek_to(180_000);

    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn a_progress_sample_received_during_a_seek_does_not_rewind_after_it_settles() {
    let (mut app, rx) = app_with_debounced_seek(60_000);
    app.flush_pending_api_seek();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::SpotifySeek(request))
      if matches!(request.command, SpotifySeekCommand::Position(60_000))));
    app.user_config.behavior.keepawake_enabled = false;
    let now = Instant::now();
    app.api_seek.as_mut().unwrap().at = now - Duration::from_millis(550);
    let received_at = now - Duration::from_millis(100);
    let mut context = context(TRACK_ID);
    context.progress = Some(chrono::TimeDelta::milliseconds(10_000));
    app.instant_since_last_current_playback_poll = received_at;
    app.set_polled_playback(context, received_at);

    app.update_on_tick(Duration::from_millis(250));

    assert_eq!(app.song_progress_ms, 60_250);
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn an_api_seek_queued_under_spotify_is_dropped_once_the_queue_slot_owns_playback() {
    let (mut app, rx) = app_with_debounced_seek(5_000);
    app.queue_now = Some(crate::infra::queue::QueueNowPlaying::Spotify {
      track: crate::core::app::test_support::queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.flush_pending_api_seek();

    assert!(app.api_seek.is_none());
    assert!(rx.try_recv().is_err());
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;
  use crate::infra::queue::QueueNowPlaying;

  #[test]
  fn a_seek_on_the_parked_device_sends_no_web_api_seek() {
    let (mut app, rx, _recovery_rx) = parked_native_app();

    app.seek_to(30_000);

    assert!(rx.try_recv().is_err());
    assert!(app.seek_ms.is_none());
    assert_eq!(app.status_message(), Some("Press play to resume Spotify"));
  }

  fn app_with_spotify_queue_slot() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });
    (app, rx)
  }

  #[test]
  fn seek_to_uses_the_spotify_queue_slots_duration_without_a_cached_item() {
    let (mut app, rx) = app_with_spotify_queue_slot();

    app.seek_to(500);

    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(500))));

    app.last_native_seek = Some(Instant::now() - NATIVE_SEEK_THROTTLE);
    app.seek_to(2_000);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(1_000))));
  }

  #[test]
  fn a_source_seek_is_dropped_without_a_decoded_owner() {
    let (mut app, rx) = app_with_spotify_queue_slot();
    app.pending_source_seek = Some(5_000);

    app.flush_pending_source_seek();

    assert!(app.pending_source_seek.is_none());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn a_throttled_seek_reaches_the_spotify_queue_slot() {
    let (mut app, rx) = app_with_spotify_queue_slot();
    app.seek_to(100);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(100))));
    app.seek_to(500);
    assert!(rx.try_recv().is_err());

    app.last_native_seek = Some(Instant::now() - NATIVE_SEEK_THROTTLE);
    app.flush_pending_native_seek();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(500))));
  }

  #[test]
  fn apply_seek_under_a_spotify_queue_slot_uses_the_slot_track_duration() {
    let (mut app, rx) = app_with_spotify_queue_slot();
    app.current_playback_context = Some(playing_track_context(full_track("ctx", "Context")));

    // Past the slot track's 1000 ms but inside the suspended context's track.
    app.apply_seek(2_000);

    assert!(matches!(rx.try_recv(), Ok(IoEvent::NextTrack)));
  }
}
