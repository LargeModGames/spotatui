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
  pub(crate) fn mark_native_handed_off(&mut self) {
    self.native_handed_off = true;
    self.native_handoff_resume_tried = false;
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn clear_native_handoff(&mut self) {
    self.native_handed_off = false;
    self.native_handoff_resume_tried = false;
  }

  /// One sent transfer per handoff: a later resume takes the normal start path.
  #[cfg(feature = "streaming")]
  pub(crate) fn native_handoff_resume_pending(&self) -> bool {
    self.native_handed_off && !self.native_handoff_resume_tried
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn mark_native_handoff_resume_sent(&mut self) {
    self.native_handoff_resume_tried = true;
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn native_handed_off(&self) -> bool {
    self.native_handed_off
  }

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

    self.set_status_message(self.strings().status.app.native_disconnected, 8);
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
        self.set_status_message(self.strings().status.app.native_reconnecting, 6);
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
    // A rebuild may have started before Music took over. Park it, so an
    // explicit Spotify start later reacquires it instead of it resuming now.
    if self.apple_music_owns_playback() {
      self.park_native_backend();
      self.native_parked = true;
      self.native_backend_pending = false;
      return false;
    }
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

  /// The configured Spotify startup Play/Pause, sent once deferred native
  /// init is over (before that, the device transfer had not completed and
  /// these 404'd onto the Error screen). A start the user parked during init
  /// is newer, and so is Music started meanwhile: routed now, the pause or
  /// play would reach Music.
  #[cfg(feature = "streaming")]
  pub(crate) fn run_spotify_startup_behavior(
    &mut self,
    behavior: Option<crate::core::user_config::StartupBehavior>,
    shuffle: bool,
  ) {
    use crate::core::user_config::StartupBehavior;
    if self.pending_start_playback.is_some() || self.apple_music_owns_playback() {
      return;
    }
    match behavior {
      Some(StartupBehavior::Play) => {
        self.dispatch(IoEvent::Shuffle(shuffle));
        self.dispatch(IoEvent::StartPlayback(None, None, None));
      }
      Some(StartupBehavior::Pause) => self.dispatch(IoEvent::PausePlayback),
      Some(StartupBehavior::Continue) | None => {}
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
      self.set_status_message(self.strings().status.app.native_request_expired, 6);
      return;
    }
    if !self.native_should_drive() {
      return;
    }
    self.set_status_message(self.strings().status.app.resuming_request, 4);
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
        _type: if cfg!(target_os = "android") {
          DeviceType::Smartphone
        } else {
          DeviceType::Computer
        },
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

/// librespot reports about once a second; past this gap the clock waits for the next report.
#[cfg(feature = "streaming")]
const NATIVE_CLOCK_REACH: Duration = Duration::from_secs(2);

/// A position librespot reported, and when; `running` is librespot's own play state,
/// never the optimistic `native_is_playing`.
#[cfg(feature = "streaming")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeClock {
  ms: u32,
  at: Instant,
  running: bool,
}

/// A report this far from the clock is a jump (a seek elsewhere, a stall), not jitter.
#[cfg(feature = "streaming")]
const NATIVE_CLOCK_JUMP_MS: i64 = 1_500;

/// The clock after librespot reports `report_ms`. The decoder works in bursts, so a
/// report near the clock is jitter: half of it is taken, and never as a backward step.
#[cfg(feature = "streaming")]
fn observed_clock(
  clock: Option<NativeClock>,
  report_ms: u32,
  running: bool,
  now: Instant,
) -> NativeClock {
  let at_report = NativeClock {
    ms: report_ms,
    at: now,
    running,
  };
  let Some(clock) = clock else {
    return at_report;
  };
  let shown = native_position_at(clock, now, 0);
  let error = i64::from(report_ms) - i64::from(shown);
  if error.abs() >= NATIVE_CLOCK_JUMP_MS {
    return at_report;
  }
  let (ms, at) = if !(running && clock.running) || error == 0 {
    (shown, now)
  } else if error > 0 {
    (shown.saturating_add((error / 2) as u32), now)
  } else {
    (shown, now + Duration::from_millis((-error / 2) as u64))
  };
  NativeClock { ms, at, running }
}

/// The native position at `now`: the report plus the time since it while audio runs.
#[cfg(feature = "streaming")]
fn native_position_at(clock: NativeClock, now: Instant, duration_ms: u32) -> u32 {
  let ms = if clock.running {
    let ahead = now
      .saturating_duration_since(clock.at)
      .min(NATIVE_CLOCK_REACH);
    clock.ms.saturating_add(ahead.as_millis() as u32)
  } else {
    clock.ms
  };
  if duration_ms > 0 {
    ms.min(duration_ms)
  } else {
    ms
  }
}

#[cfg(feature = "streaming")]
impl App {
  /// Records that librespot is at `position_ms` now.
  pub(crate) fn anchor_native_position(&mut self, position_ms: u32, running: bool) {
    self.native_clock = Some(NativeClock {
      ms: position_ms,
      at: Instant::now(),
      running,
    });
  }

  /// Takes a librespot position report in the play state it gives.
  pub(crate) fn observe_native_position(&mut self, position_ms: u32, running: bool) {
    self.native_clock = Some(observed_clock(
      self.native_clock,
      position_ms,
      running,
      Instant::now(),
    ));
  }

  /// A local seek: librespot plays from `position_ms` now, and its reports for a moment are older.
  pub(crate) fn record_native_seek(&mut self, position_ms: u32) {
    self.last_native_seek = Some(Instant::now());
    self.set_native_recovery_position(position_ms);
    self.jump_native_position(position_ms);
  }

  /// Moves the clock to `position_ms` at once, in the play state it had.
  pub(crate) fn jump_native_position(&mut self, position_ms: u32) {
    let running = self.native_clock.is_some_and(|clock| clock.running);
    self.anchor_native_position(position_ms, running);
  }

  /// Whether a local seek is recent enough that a librespot report can predate it.
  pub(crate) fn native_seek_settling(&self) -> bool {
    self
      .last_native_seek
      .is_some_and(|at| at.elapsed().as_millis() < SEEK_POSITION_IGNORE_MS)
  }

  pub(crate) fn clear_native_position(&mut self) {
    self.native_clock = None;
  }

  /// The native position at `now`, or `None` before librespot reported one.
  /// A seek the throttle still holds is where the playbar already is.
  pub(crate) fn native_position_ms(&self, now: Instant) -> Option<u32> {
    let clock = self.native_clock?;
    if let Some(pending) = self.pending_native_seek {
      return Some(pending);
    }
    let duration_ms = self
      .native_track_info
      .as_ref()
      .map(|info| info.duration_ms)
      .or_else(|| self.playing_duration_ms())
      .unwrap_or(0);
    Some(native_position_at(clock, now, duration_ms))
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;

  fn clock(ms: u32, at: Instant, running: bool) -> NativeClock {
    NativeClock { ms, at, running }
  }

  #[test]
  fn a_repeated_native_report_still_advances_the_position() {
    let t0 = Instant::now();
    let report = clock(37_900, t0, true);
    let shown: Vec<u32> = [0, 250, 500, 750]
      .map(|ms| native_position_at(report, t0 + Duration::from_millis(ms), 180_000))
      .to_vec();
    assert_eq!(shown, vec![37_900, 38_150, 38_400, 38_650]);
  }

  #[test]
  fn the_native_position_holds_until_librespot_runs() {
    let t0 = Instant::now();
    let later = t0 + Duration::from_millis(900);
    assert_eq!(
      native_position_at(clock(37_900, t0, false), later, 180_000),
      37_900
    );
  }

  #[test]
  fn the_native_position_stops_two_seconds_past_the_last_report() {
    let t0 = Instant::now();
    let later = t0 + Duration::from_secs(10);
    assert_eq!(
      native_position_at(clock(37_900, t0, true), later, 180_000),
      39_900
    );
  }

  #[test]
  fn the_native_position_never_passes_the_track_duration() {
    let t0 = Instant::now();
    let later = t0 + Duration::from_millis(1_500);
    assert_eq!(
      native_position_at(clock(179_000, t0, true), later, 180_000),
      180_000
    );
  }

  #[test]
  fn a_report_a_little_behind_holds_the_clock_instead_of_stepping_back() {
    let t0 = Instant::now();
    let now = t0 + Duration::from_millis(1_000);
    let clock = observed_clock(Some(clock(10_000, t0, true)), 10_800, true, now);
    assert_eq!(native_position_at(clock, now, 0), 11_000);
    assert_eq!(
      native_position_at(clock, now + Duration::from_millis(100), 0),
      11_000
    );
    assert_eq!(
      native_position_at(clock, now + Duration::from_millis(300), 0),
      11_200
    );
  }

  #[test]
  fn a_report_a_little_ahead_moves_the_clock_half_way() {
    let t0 = Instant::now();
    let now = t0 + Duration::from_millis(1_000);
    let clock = observed_clock(Some(clock(10_000, t0, true)), 11_300, true, now);
    assert_eq!(native_position_at(clock, now, 0), 11_150);
  }

  #[test]
  fn a_report_far_from_the_clock_is_taken_at_once() {
    let t0 = Instant::now();
    let now = t0 + Duration::from_millis(1_000);
    let clock = observed_clock(Some(clock(10_000, t0, true)), 60_000, true, now);
    assert_eq!(native_position_at(clock, now, 0), 60_000);
  }

  #[test]
  fn a_pause_report_stops_the_clock_where_it_shows() {
    let t0 = Instant::now();
    let now = t0 + Duration::from_millis(1_000);
    let clock = observed_clock(Some(clock(10_000, t0, true)), 10_900, false, now);
    assert_eq!(
      native_position_at(clock, now + Duration::from_secs(5), 0),
      11_000
    );
  }

  #[test]
  fn a_native_seek_moves_the_clock_the_tick_reads() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.anchor_native_position(37_900, true);
    app.record_native_seek(90_000);
    let shown = app
      .native_position_ms(Instant::now() + Duration::from_millis(600))
      .unwrap();
    assert!((90_600..92_000).contains(&shown), "{shown}");
  }

  #[test]
  fn a_local_seek_settles_before_librespot_reports_count_again() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert!(!app.native_seek_settling());
    app.record_native_seek(45_000);
    assert!(app.native_seek_settling());
    app.last_native_seek = Some(Instant::now() - Duration::from_secs(1));
    assert!(!app.native_seek_settling());
  }

  #[test]
  fn a_seek_while_paused_keeps_the_clock_still() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.anchor_native_position(37_900, false);
    app.record_native_seek(90_000);
    let later = Instant::now() + Duration::from_millis(600);
    assert_eq!(app.native_position_ms(later), Some(90_000));
  }

  #[test]
  fn an_optimistic_resume_does_not_run_the_clock_before_librespot_plays() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.anchor_native_position(60_000, false);
    app.native_is_playing = Some(true);
    let later = Instant::now() + Duration::from_secs(5);
    assert_eq!(app.native_position_ms(later), Some(60_000));
  }

  #[test]
  fn a_throttled_seek_is_the_position_until_it_runs() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.anchor_native_position(15_000, true);
    app.pending_native_seek = Some(20_000);
    assert_eq!(app.native_position_ms(Instant::now()), Some(20_000));
  }

  #[test]
  fn spotify_startup_play_or_pause_never_reaches_music() {
    use crate::core::user_config::StartupBehavior;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.run_spotify_startup_behavior(Some(StartupBehavior::Pause), false);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
    // Music started while Spotify was still starting up: nothing is sent.
    app.claim_apple_music();
    app.run_spotify_startup_behavior(Some(StartupBehavior::Pause), false);
    app.run_spotify_startup_behavior(Some(StartupBehavior::Play), true);
    assert!(rx.try_recv().is_err());
  }

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

  #[test]
  fn a_handoff_is_marked_until_cleared() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert!(!app.native_handed_off());

    app.mark_native_handed_off();
    assert!(app.native_handed_off());

    app.clear_native_handoff();
    assert!(!app.native_handed_off());
  }

  #[test]
  fn only_the_first_sent_transfer_after_a_handoff_uses_up_the_resume() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert!(!app.native_handoff_resume_pending());

    app.mark_native_handed_off();
    // A transfer that could not be sent leaves the resume pending.
    assert!(app.native_handoff_resume_pending());
    assert!(app.native_handoff_resume_pending());

    app.mark_native_handoff_resume_sent();
    assert!(!app.native_handoff_resume_pending());
    assert!(app.native_handed_off());

    app.mark_native_handed_off();
    assert!(app.native_handoff_resume_pending());
  }
}
