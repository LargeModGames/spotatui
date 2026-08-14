use super::*;

#[cfg(feature = "streaming")]
const FRESH_NATIVE_ACTIVITY_WINDOW: Duration = Duration::from_secs(5);

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
        })
        .is_ok();
    } else {
      self.native_backend_pending = false;
    }
    self.dispatch(IoEvent::GetCurrentPlayback);
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
    const MAX_PARKED_AGE: Duration = Duration::from_secs(30);
    let Some(pending) = self.pending_start_playback.clone() else {
      return;
    };
    if pending.parked_at.elapsed() > MAX_PARKED_AGE {
      self.pending_start_playback = None;
      self.set_status_message("Playback request expired during native recovery.", 6);
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
    self.native_is_playing = Some(false);

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
}
