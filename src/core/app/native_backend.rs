use super::*;

#[cfg(feature = "streaming")]
const FRESH_NATIVE_ACTIVITY_WINDOW: Duration = Duration::from_secs(5);

/// Longer than a whole player build, so a rebuild never expires the start it
/// replays.
#[cfg(feature = "streaming")]
fn max_parked_age() -> Duration {
  crate::infra::player::player_build_timeout().saturating_add(Duration::from_secs(15))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativePlaybackOrigin {
  Context,
  #[default]
  RawList,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeTrackKind {
  #[default]
  Track,
  Episode,
}

/// Immediate track info from native player for instant UI updates
/// Used to display track info immediately when skipping, before API responds
#[derive(Clone, Debug, Default)]
pub struct NativeTrackInfo {
  pub name: String,
  /// Individual credited artist names, in order. Kept structured (not a
  /// pre-joined display string) so the LRCLIB lookup can fall back to the
  /// primary artist alone for collaborations (#410). Join with `", "` for
  /// display.
  pub artists: Vec<String>,
  #[allow(dead_code)]
  pub album: String, // Reserved for future use (e.g., displaying album in playbar)
  pub duration_ms: u32,
  pub kind: NativeTrackKind,
  /// Album art URL carried by librespot's own `TrackChanged` payload, so cover
  /// art follows the track librespot is actually decoding. The polled Spotify
  /// context is not a usable source here: it lags by seconds after a skip, and
  /// for a natively queued track (played via a direct `player.load`, which Spirc
  /// never reports) it stays on the *previous* track for the whole song. (#402)
  pub image_url: Option<String>,
}

#[cfg(feature = "streaming")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingStartPlayback {
  pub context_uri: Option<String>,
  pub uris: Option<Vec<String>>,
  pub offset: Option<usize>,
  pub parked_at: Instant,
  pub recovery_attempts: u8,
}

impl App {
  #[cfg(feature = "streaming")]
  pub fn request_native_streaming_recovery_if_disconnected(
    &mut self,
    reselect_device: bool,
  ) -> bool {
    if !self.native_should_drive() {
      return false;
    }
    let Some(player) = self.streaming_player.as_ref() else {
      return false;
    };

    if player.is_available() {
      return false;
    }

    self.force_native_streaming_recovery(reselect_device);
    true
  }

  /// Tear down the current native session — even one that still passes
  /// `is_connected` — and request recovery. Called by the disconnect check
  /// above and by the load watchdog, which catches zombie sessions (half-open
  /// TCP: `is_connected` true, Spirc commands silently dropped).
  #[cfg(feature = "streaming")]
  pub fn force_native_streaming_recovery(&mut self, reselect_device: bool) {
    if !self.native_should_drive() {
      return;
    }
    let position_ms = u32::try_from(self.song_progress_ms).unwrap_or(u32::MAX);
    let is_playing = self.native_is_playing.unwrap_or(false);
    self.prepare_native_playback_recovery(position_ms, is_playing);
    if let Some(player) = self.streaming_player.take() {
      // Stop the old spirc before dropping our reference so the dead session
      // doesn't linger as a ghost Connect device (#297).
      player.shutdown();
    }
    // Unlike disconnect recovery, the shuffle session (and any shuffled queue
    // suspension bound to its generation) is deliberately kept: the session is
    // app-owned and player-independent, so a session-driven load resumes it on
    // the replacement player.
    self.is_streaming_active = false;
    self.native_activation_pending = false;
    self.native_device_id = None;
    self.native_is_playing = Some(false);
    self.native_track_info = None;
    self.native_playback_origin = None;
    self.song_progress_ms = 0;
    self.last_track_id = None;
    self.last_device_activation = None;
    self.seek_ms = None;
    self.native_load_watchdog = None;
    // Playback requests park for replay until recovery resolves (see
    // `replay_pending_start_playback`). Only enter the pending state when the
    // recovery request was actually accepted.
    if reselect_device {
      self.current_playback_context = None;
    }

    self.set_status_message("Native streaming disconnected; attempting recovery.", 8);
    if let Some(tx) = &self.streaming_recovery_tx {
      self.native_backend_pending = tx
        .send(crate::infra::player::StreamingRecoveryRequest {
          reselect_device,
          restore_playback: true,
          continue_after_track: None,
          reacquire: false,
        })
        .is_ok();
    } else {
      self.native_backend_pending = false;
    }
    self.dispatch(IoEvent::GetCurrentPlayback);
  }

  /// Shut librespot down for another owner of the sink. A shut-down player
  /// cannot resume: the next Spotify start rebuilds it.
  #[cfg(feature = "streaming")]
  pub(crate) fn park_native_backend(&mut self) {
    let Some(player) = self.streaming_player.take() else {
      return;
    };
    self.native_parked = true;
    self
      .native_device_id
      .get_or_insert_with(|| player.device_id());
    self.native_backend_pending = false;
    self.native_restore_pending = None;
    player.shutdown();
    // The last handle joins librespot's player thread: drop it off the lock.
    tokio::task::spawn_blocking(move || drop(player));
  }

  /// Whether the park took the player and no rebuild has installed a new one.
  #[cfg(feature = "streaming")]
  pub(crate) fn native_backend_parked(&self) -> bool {
    self.native_parked && self.streaming_player.is_none()
  }

  /// Ask the recovery loop to rebuild a parked backend. True while parked, so
  /// the caller holds its start back instead of routing it to another device.
  #[cfg(feature = "streaming")]
  pub(crate) fn reacquire_parked_backend(&mut self) -> bool {
    if !self.native_backend_parked() {
      return false;
    }
    if !self.native_backend_pending {
      self.native_backend_pending = self.streaming_recovery_tx.as_ref().is_some_and(|tx| {
        tx.send(crate::infra::player::StreamingRecoveryRequest {
          reselect_device: false,
          restore_playback: true,
          continue_after_track: None,
          reacquire: true,
        })
        .is_ok()
      });
      if self.native_backend_pending {
        self.set_status_message("Reconnecting native streaming…", 6);
      }
    }
    true
  }

  /// Enter on the parked device row: rebuild the backend. When another device
  /// took the playback while parked, the rebuild comes back idle and a second
  /// Enter transfers it.
  #[cfg(feature = "streaming")]
  pub(crate) fn reacquire_parked_device(&mut self) -> bool {
    let parked_owns_playback = self.native_parked_owns_context();
    if !self.reacquire_parked_backend() {
      return false;
    }
    if !parked_owns_playback {
      self.clear_native_playback_recovery();
    }
    // Routing follows the rebuilt device, not the context from before the park.
    self.current_playback_context = None;
    true
  }

  /// Whether a finished build may be installed: not while parked under
  /// another owner of the sink. An install ends the park.
  #[cfg(feature = "streaming")]
  pub(crate) fn accept_rebuilt_native_backend(&mut self) -> bool {
    if self.native_parked && !self.native_should_drive() {
      return false;
    }
    self.native_parked = false;
    true
  }

  /// Park a StartPlayback request (string form, as `IoEvent::StartPlayback`
  /// carries it) for replay once a usable backend exists. A newer request
  /// replaces an older one — the user's latest intent wins.
  #[cfg(feature = "streaming")]
  pub fn park_start_playback(
    &mut self,
    context_uri: Option<String>,
    uris: Option<Vec<String>>,
    offset: Option<usize>,
  ) {
    let same_request = self.pending_start_playback.as_ref().is_some_and(|pending| {
      pending.context_uri == context_uri && pending.uris == uris && pending.offset == offset
    });
    if !same_request {
      self.pending_start_playback = Some(PendingStartPlayback {
        context_uri,
        uris,
        offset,
        parked_at: Instant::now(),
        recovery_attempts: 0,
      });
    }
  }

  /// Replay a parked StartPlayback through the normal dispatch path. No-op
  /// when nothing is parked.
  #[cfg(feature = "streaming")]
  pub fn replay_pending_start_playback(&mut self) {
    let Some(pending) = self.pending_start_playback.clone() else {
      return;
    };
    if pending.parked_at.elapsed() > max_parked_age() {
      self.pending_start_playback = None;
      self.set_status_message("Playback request expired during native recovery.", 6);
      return;
    }
    if !self.native_should_drive() {
      return;
    }
    self.set_status_message("Resuming playback request…", 4);
    self.dispatch(IoEvent::StartPlayback(
      pending.context_uri,
      pending.uris,
      pending.offset,
    ));
  }

  #[cfg(feature = "streaming")]
  pub fn mark_native_streaming_device_available(
    &mut self,
    device_id: String,
    device_name: String,
    volume_percent: u8,
  ) {
    self.native_device_id = Some(device_id.clone());
    self.is_streaming_active = true;
    self.native_activation_pending = false;
    // Only the polled "no active playback" answer reaches this, and that answer
    // is structurally wrong while a queued Spotify track plays: the queue starts
    // it with a direct `player.load`, which never enters Spirc's Connect state,
    // so the Web API reports the device idle over audible playback. Clearing the
    // play state here drew a paused playbar over that audio, and re-cleared it
    // on every poll after the user pressed play. The player's own events own
    // this field while the queue slot holds the sink.
    if !self.queue_now_is_spotify() {
      self.native_is_playing = Some(false);
    }

    if self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.item.as_ref())
      .is_some()
    {
      return;
    }

    self.current_playback_context = Some(CurrentPlaybackContext {
      device: Device {
        id: Some(device_id),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: device_name,
        _type: DeviceType::Computer,
        volume_percent: Some(u32::from(volume_percent)),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: self.runtime_state.shuffle_enabled,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: false,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Unknown,
      actions: Actions::default(),
    });
  }

  #[cfg(feature = "streaming")]
  pub fn has_fresh_native_activity(&self) -> bool {
    self.native_track_info.is_some()
      || self.native_is_playing == Some(true)
      || self
        .last_device_activation
        .is_some_and(|instant| instant.elapsed() < FRESH_NATIVE_ACTIVITY_WINDOW)
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;

  #[cfg(feature = "streaming")]
  #[test]
  fn parked_playback_retries_are_keyed_to_the_request() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.park_start_playback(
      Some("spotify:playlist:first".to_string()),
      Some(vec!["spotify:track:first".to_string()]),
      Some(1),
    );
    app
      .pending_start_playback
      .as_mut()
      .unwrap()
      .recovery_attempts = 2;

    app.park_start_playback(
      Some("spotify:playlist:first".to_string()),
      Some(vec!["spotify:track:first".to_string()]),
      Some(1),
    );
    assert_eq!(
      app
        .pending_start_playback
        .as_ref()
        .unwrap()
        .recovery_attempts,
      2
    );

    app.park_start_playback(Some("spotify:playlist:second".to_string()), None, None);
    assert_eq!(
      app
        .pending_start_playback
        .as_ref()
        .unwrap()
        .recovery_attempts,
      0
    );
  }

  #[test]
  fn a_decoded_owner_does_not_force_a_backend_rebuild() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.claim_decoded_sink(Source::Qobuz);
    app.is_streaming_active = true;

    app.force_native_streaming_recovery(true);

    assert!(app.is_streaming_active);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn a_parked_start_is_not_replayed_over_a_decoded_source() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.park_start_playback(Some("spotify:playlist:p".to_string()), None, None);
    app.claim_decoded_sink(Source::YouTube);

    app.replay_pending_start_playback();

    assert!(rx.try_recv().is_err());
    assert!(app.pending_start_playback.is_some());
  }

  #[test]
  fn a_refused_replay_still_expires_on_age() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.park_start_playback(Some("spotify:playlist:p".to_string()), None, None);
    if let Some(pending) = app.pending_start_playback.as_mut() {
      pending.parked_at = Instant::now() - max_parked_age() - Duration::from_secs(1);
    }
    app.claim_decoded_sink(Source::YouTube);

    app.replay_pending_start_playback();

    assert!(rx.try_recv().is_err());
    assert!(app.pending_start_playback.is_none());
  }

  #[test]
  fn a_start_parked_for_a_whole_rebuild_still_replays() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.park_start_playback(Some("spotify:playlist:p".to_string()), None, None);
    if let Some(pending) = app.pending_start_playback.as_mut() {
      pending.parked_at = Instant::now() - crate::infra::player::player_build_timeout();
    }

    app.replay_pending_start_playback();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));
  }

  #[test]
  fn a_parked_backend_sends_one_reacquire_request_per_rebuild() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let (recovery_tx, mut recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    app.streaming_recovery_tx = Some(recovery_tx);
    assert!(!app.reacquire_parked_backend());
    app.native_parked = true;

    assert!(app.reacquire_parked_backend());
    assert!(app.reacquire_parked_backend());

    let request = recovery_rx.try_recv().expect("one reacquire request");
    assert!(request.reacquire && request.restore_playback && !request.reselect_device);
    assert!(recovery_rx.try_recv().is_err());
    assert!(app.native_backend_pending);
    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some("Reconnecting native streaming…"));
  }

  #[test]
  fn a_rebuild_under_a_decoded_owner_is_refused_only_while_parked() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_parked = true;
    app.claim_decoded_sink(Source::Qobuz);

    assert!(!app.accept_rebuilt_native_backend());
    assert!(app.native_parked);

    app.release_decoded_sink_claim();
    assert!(app.accept_rebuilt_native_backend());
    assert!(!app.native_parked);

    // A decoded queue slot over a Spotify context parks nothing.
    app.claim_decoded_sink(Source::YouTube);
    assert!(app.accept_rebuilt_native_backend());
  }

  #[test]
  fn closing_the_io_channel_drops_the_app_recovery_sender() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let (recovery_tx, mut recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    app.streaming_recovery_tx = Some(recovery_tx);

    app.close_io_channel();

    assert!(matches!(
      recovery_rx.try_recv(),
      Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    ));
  }

  #[test]
  fn the_parked_device_row_restores_only_playback_the_parked_device_held() {
    use crate::core::app::test_support::{make_external_context, parked_native_app};
    let (mut app, _rx, mut recovery_rx) = parked_native_app();
    assert!(app.reacquire_parked_device());
    assert!(recovery_rx
      .try_recv()
      .is_ok_and(|request| request.reacquire));
    assert!(app.native_playback_recovery.is_some());

    // A phone took the playback while parked: the rebuild comes back idle.
    let (mut app, _rx, _recovery_rx) = parked_native_app();
    app.current_playback_context = Some(make_external_context());
    assert!(app.reacquire_parked_device());
    assert!(app.native_playback_recovery.is_none());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn fresh_native_activity_is_true_when_native_metadata_exists() {
    let mut app = App {
      native_track_info: Some(NativeTrackInfo::default()),
      ..Default::default()
    };

    assert!(app.has_fresh_native_activity());

    app.native_track_info = None;
    assert!(!app.has_fresh_native_activity());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn fresh_native_activity_is_true_when_native_is_playing() {
    let app = App {
      native_is_playing: Some(true),
      ..Default::default()
    };

    assert!(app.has_fresh_native_activity());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn fresh_native_activity_uses_recent_activation_window() {
    let mut app = App {
      last_device_activation: Some(Instant::now()),
      ..Default::default()
    };

    assert!(app.has_fresh_native_activity());

    app.last_device_activation = Some(Instant::now() - Duration::from_secs(6));

    assert!(!app.has_fresh_native_activity());
  }

  /// The polled "device is idle" answer is the only caller, and it cannot see a
  /// queued Spotify track: the queue starts it with a direct `player.load`, so
  /// the Web API reports the device idle over audible playback. Believing it
  /// drew a paused playbar over that audio, and re-drew it every poll after the
  /// user pressed play.
  #[cfg(feature = "streaming")]
  #[test]
  fn marking_the_device_available_keeps_the_queue_slots_play_state() {
    use crate::core::app::test_support::queue_track;
    use crate::infra::queue::QueueNowPlaying;

    let mut app = App {
      native_is_playing: Some(true),
      queue_now: Some(QueueNowPlaying::Spotify {
        track: queue_track(Some("spotify:track:queued"), "Queued"),
      }),
      ..Default::default()
    };

    app.mark_native_streaming_device_available("device".to_string(), "spotatui".to_string(), 70);

    assert_eq!(app.native_is_playing, Some(true));
    assert!(app.is_streaming_active);
  }

  /// With no queue slot the API's idle answer is trustworthy again.
  #[cfg(feature = "streaming")]
  #[test]
  fn marking_the_device_available_clears_the_play_state_without_a_queue_slot() {
    let mut app = App {
      native_is_playing: Some(true),
      ..Default::default()
    };

    app.mark_native_streaming_device_available("device".to_string(), "spotatui".to_string(), 70);

    assert_eq!(app.native_is_playing, Some(false));
  }
}
