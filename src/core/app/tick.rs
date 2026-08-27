use super::*;

/// Grace window after the last tick before playback-position reads go stale.
const STALE_TICK_AFTER: Duration = Duration::from_secs(2);

// The window must cover at least two of the slowest allowed ticks, so one
// missed frame can never read as stale. The tick rate is bounded on every
// path (config load rejects out-of-range values, the settings screen clamps),
// and this assertion keeps the two constants coupled if either ever moves.
const _: () = assert!(
  STALE_TICK_AFTER.as_millis() >= 2 * crate::core::user_config::MAX_TICK_RATE_MILLISECONDS as u128
);

/// Whether the machine must stay awake. The audible player answers first: a
/// decoded source that owns the sink overrides the suspended librespot flag and
/// the stale Spotify context it left behind.
fn playing_for_keepawake(
  decoded: Option<bool>,
  native: Option<bool>,
  spotify: Option<bool>,
) -> bool {
  decoded.or(native).or(spotify).unwrap_or(false)
}

impl App {
  /// Milliseconds into the current track, or `None` when no tick has run for
  /// over [`STALE_TICK_AFTER`]. A frontend that stops driving
  /// `core::driver::Driver::tick` (or never starts) would otherwise show a
  /// silently frozen playbar; `None` is the loud version of that failure,
  /// rendered as an explicit stall by the playbar.
  pub fn playback_position_ms(&self) -> Option<u128> {
    (self.last_tick_at.elapsed() <= STALE_TICK_AFTER).then_some(self.song_progress_ms)
  }

  fn poll_current_playback(&mut self) {
    // No Spotify session (free-source launch): the poll would hit the auth gate
    // and re-flash a "connect Spotify" status message every interval. Free
    // sources drive their own playback state, so skip the Spotify poll entirely.
    if !self.spotify_connected {
      return;
    }

    // Poll interval depends on playback mode:
    // - Native streaming: configurable (default 5s; real-time events provide
    //   updates between polls).
    // - External players (spotifyd, etc.): 1 second (no events, need faster
    //   polling for smooth playbar) — stays hardcoded, not a preference.
    let poll_interval_ms: u128 = if self.is_streaming_active {
      self.user_config.behavior.playback_poll_seconds as u128 * 1000
    } else {
      1_000
    };

    let elapsed = self
      .instant_since_last_current_playback_poll
      .elapsed()
      .as_millis();

    if !self.is_fetching_current_playback && elapsed >= poll_interval_ms {
      self.is_fetching_current_playback = true;
      // Trigger the seek if the user has set a new position
      match self.seek_ms {
        Some(seek_ms) => self.apply_seek(seek_ms as u32),
        None => self.dispatch(IoEvent::GetCurrentPlayback),
      }
    }
  }

  pub fn update_on_tick(&mut self, elapsed: Duration) {
    self.last_tick_at = Instant::now();

    // Increment global animation tick (wraps after ~9.4 quintillion ticks, effectively never)
    self.view.animation_tick = self.view.animation_tick.wrapping_add(1);

    // Advance an adaptive-theme fade. Real elapsed time, not tick count, so
    // the fade speed is independent of the configured tick rates.
    #[cfg(feature = "art-decode")]
    if let Some(transition) = self.theme_transition.as_mut() {
      transition.advance(elapsed);
      self.user_config.theme = transition.current();
      if transition.is_complete() {
        self.theme_transition = None;
        // A finished fade-out means the user's own theme is back in place.
        if let crate::core::cover_theme::CoverThemeState::Restoring { .. } = self.cover_theme_state
        {
          self.cover_theme_state = crate::core::cover_theme::CoverThemeState::Inactive;
        }
      }
    }

    // Periodic party sync: host broadcasts state about every 2 seconds.
    // Keep this before early-return paths so sync still happens during native-streaming fast paths.
    if self.party_status == PartyStatus::Hosting
      && self.last_party_sync_at.elapsed() >= Duration::from_secs(2)
    {
      self.last_party_sync_at = Instant::now();
      self.dispatch(IoEvent::SyncPlayback);
    }

    // Periodic friends refresh: re-fetch when the Friends screen is active, every 30 seconds.
    if self.get_current_route().id == RouteId::Friends
      && self.last_friends_refresh_at.elapsed() >= Duration::from_secs(30)
      && !self.friends_loading
      && self.user_config.behavior.sync_token.is_some()
    {
      self.last_friends_refresh_at = Instant::now();
      self.dispatch(IoEvent::GetFriends);
    }

    if let Some(expires_at) = self.status_message_expires_at {
      if Instant::now() >= expires_at {
        self.status_message = None;
        self.status_message_expires_at = None;
        self.status_message_is_error = false;
      }
    }

    // Must stay above the early returns further down this fn, or the backstop
    // would silently stop running during playback.
    self.expire_api_error();

    if let Some(frame) = self.view.liked_song_animation_frame {
      if frame > 0 {
        self.view.liked_song_animation_frame = Some(frame - 1);
      } else {
        self.view.liked_song_animation_frame = None;
      }
    }

    self.advance_lyrics_scroll(elapsed);

    // Load watchdog: a native load that produces no Playing/TrackChanged
    // event within the window means the session is a zombie (passes
    // `is_connected`, drops Spirc commands) — force recovery; the parked
    // request replays once the new session is up.
    #[cfg(feature = "streaming")]
    {
      const NATIVE_LOAD_WATCHDOG: Duration = Duration::from_secs(5);
      let restore_attempts = self
        .native_restore_pending
        .as_ref()
        .and_then(|attempt| {
          self
            .native_playback_recovery
            .as_ref()
            .filter(|snapshot| snapshot.generation == attempt.generation)
        })
        .map_or(0, |snapshot| snapshot.recovery_attempts.saturating_sub(1));
      let watchdog_window = NATIVE_LOAD_WATCHDOG.saturating_mul(
        u32::from(
          self
            .pending_start_playback
            .as_ref()
            .map_or(restore_attempts, |pending| pending.recovery_attempts),
        ) + 1,
      );
      let native_reconnecting = self
        .streaming_player
        .as_ref()
        .is_some_and(|player| player.is_recovering());
      if native_reconnecting {
        // A command accepted during the bounded fast-reconnect window is
        // intentionally deferred by StreamingPlayer. Start its response window
        // after the replacement Spirc exists, not while it is still connecting.
        if let Some(watchdog) = self.native_load_watchdog.as_mut() {
          *watchdog = Instant::now();
        }
      } else if self
        .native_load_watchdog
        .is_some_and(|armed| armed.elapsed() >= watchdog_window)
      {
        self.native_load_watchdog = None;
        const MAX_RECOVERY_ATTEMPTS: u8 = 2;
        if let Some(pending) = self.pending_start_playback.as_mut() {
          if pending.recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
            self.pending_start_playback = None;
            self.set_status_message(
              "Native playback did not respond after recovery; request dropped.",
              8,
            );
          } else {
            pending.recovery_attempts += 1;
            log::warn!(
              "no player event within {}s of native load; forcing recovery attempt {}",
              NATIVE_LOAD_WATCHDOG.as_secs(),
              pending.recovery_attempts
            );
            self.force_native_streaming_recovery(true);
          }
        } else if let Some(attempt) = self.native_restore_pending.clone() {
          let recovery_attempts = self
            .native_playback_recovery
            .as_ref()
            .filter(|snapshot| snapshot.generation == attempt.generation)
            .map_or(0, |snapshot| snapshot.recovery_attempts);
          if recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
            self.native_restore_pending = None;
            self.set_status_message(
              "Native connection recovered, but playback could not be restored.",
              8,
            );
          } else {
            log::warn!(
              "native restore generation {} produced no matching player event; forcing recovery attempt {}",
              attempt.generation,
              recovery_attempts + 1
            );
            self.force_native_streaming_recovery(true);
          }
        }
      }
    }

    self.poll_current_playback();
    let playing_now = self.user_config.behavior.keepawake_enabled
      && playing_for_keepawake(
        self.decoded_playing_state(),
        self.native_is_playing,
        self.current_playback_context.as_ref().map(|c| c.is_playing),
      );
    match (playing_now, self.keepawake.is_some()) {
      (true, false) => {
        self.keepawake = keepawake::Builder::default()
          .idle(true)
          .sleep(true)
          .display(true)
          .reason("Playing music")
          .app_name("spotatui")
          .create()
          .ok();
      }
      (false, true) => self.keepawake = None,
      _ => {}
    }

    if let Some(CurrentPlaybackContext {
      item: Some(item),
      progress,
      is_playing,
      ..
    }) = &self.current_playback_context
    {
      // When native streaming is active, skip API-based progress calculation
      // The native player's PositionChanged events update song_progress_ms directly
      if self.is_streaming_active {
        let ms_since_poll = self
          .instant_since_last_current_playback_poll
          .elapsed()
          .as_millis();
        if ms_since_poll < 2000 {
          return; // Recent native update - don't overwrite
        }
        // No recent native update - fall through to API-based calculation as fallback
      }

      let ms_since_poll = self
        .instant_since_last_current_playback_poll
        .elapsed()
        .as_millis();

      // Skip position updates if we recently seeked (let UI show our target position)
      let recently_seeked = self
        .last_api_seek
        .is_some_and(|t| t.elapsed().as_millis() < SEEK_POSITION_IGNORE_MS);

      if recently_seeked {
        return; // Don't overwrite our seek target
      }

      // Resync from fresh API data (within 300ms of poll) to correct drift
      if ms_since_poll < 300 {
        self.song_progress_ms = progress
          .as_ref()
          .map(|p| p.num_milliseconds() as u128)
          .unwrap_or(0);
      } else if *is_playing {
        // Smooth incremental updates between API polls
        let elapsed_ms = elapsed.as_millis();
        let duration_ms = match item {
          PlayableItem::Track(track) => track.duration.num_milliseconds() as u128,
          PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u128,
          _ => return,
        };

        self.song_progress_ms = (self.song_progress_ms + elapsed_ms).min(duration_ms);
      }
      // When paused, keep song_progress_ms unchanged
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn poll_current_playback_skips_when_spotify_disconnected() {
    let (tx, rx) = channel();
    // No Spotify session (free-source launch): spotify_connected == false.
    let mut app = App::new(tx, UserConfig::new(), None);
    assert!(!app.spotify_connected);
    // Force the poll interval to have elapsed so only the connection gate matters.
    app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(10);

    app.poll_current_playback();

    // Nothing dispatched: no per-tick "connect Spotify" auth-spam for free sources.
    assert!(rx.try_recv().is_err());
    assert!(!app.is_fetching_current_playback);
  }

  #[test]
  fn playback_position_goes_stale_without_ticks_and_recovers_on_one() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.song_progress_ms = 1234;
    assert_eq!(app.playback_position_ms(), Some(1234));

    // A frontend that stops ticking: the position must read as stale rather
    // than freeze at its last value.
    app.last_tick_at = Instant::now() - Duration::from_secs(3);
    assert_eq!(app.playback_position_ms(), None);

    app.update_on_tick(Duration::from_millis(500));
    assert!(app.playback_position_ms().is_some());
  }

  #[test]
  fn a_live_error_survives_a_tick() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.handle_error(anyhow!("boom"));

    app.update_on_tick(Duration::from_millis(500));

    assert_eq!(app.api_error, "boom");
    assert_eq!(app.get_current_route().id, RouteId::Error);
  }

  #[test]
  fn an_expired_error_is_retired_by_the_tick_and_handed_to_the_status_bar() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.handle_error(anyhow!("boom"));
    app.api_error_expires_at = Some(Instant::now() - Duration::from_secs(1));

    app.update_on_tick(Duration::from_millis(500));

    assert!(app.api_error.is_empty());
    assert!(app.api_error_expires_at.is_none());
    assert_ne!(app.get_current_route().id, RouteId::Error);
    assert_eq!(app.status_message.as_deref(), Some("boom"));
    assert!(app.status_message_is_error);
  }

  // The load-bearing case for a frontend that never dismisses: a buried error
  // frame must not survive the expiry, or it resurfaces later rendering an
  // error page with nothing on it. It expires silently, because a toast about
  // a minute-old failure on a screen the user already left is noise.
  #[test]
  fn expiring_an_error_removes_an_error_route_left_under_another_screen() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.handle_error(anyhow!("boom"));
    app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
    app.api_error_expires_at = Some(Instant::now() - Duration::from_secs(1));

    app.update_on_tick(Duration::from_millis(500));

    assert!(app.api_error.is_empty());
    assert_eq!(app.get_current_route().id, RouteId::SelectedDevice);
    assert!(app.status_message.is_none());

    app.pop_navigation_stack();
    assert_eq!(app.get_current_route().id, RouteId::Home);
  }

  #[test]
  fn poll_current_playback_dispatches_when_spotify_connected() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert!(app.spotify_connected);
    app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(10);

    app.poll_current_playback();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetCurrentPlayback)));
    assert!(app.is_fetching_current_playback);
  }

  #[test]
  fn a_paused_decoded_source_overrides_the_suspended_spotify_state() {
    // The Spotify-to-decoded handoff only pauses librespot, so both the native
    // flag and the context it left behind can still read as playing.
    assert!(!playing_for_keepawake(Some(false), Some(true), Some(true)));
  }

  #[test]
  fn a_playing_decoded_source_keeps_the_machine_awake() {
    assert!(playing_for_keepawake(Some(true), Some(false), Some(false)));
  }

  #[test]
  fn without_a_decoded_owner_the_native_flag_decides() {
    assert!(playing_for_keepawake(None, Some(true), Some(false)));
    assert!(!playing_for_keepawake(None, Some(false), Some(true)));
  }

  #[test]
  fn without_a_decoded_owner_or_a_native_flag_spotify_decides() {
    assert!(playing_for_keepawake(None, None, Some(true)));
    assert!(!playing_for_keepawake(None, None, Some(false)));
  }

  #[test]
  fn nothing_playing_lets_the_machine_sleep() {
    assert!(!playing_for_keepawake(None, None, None));
  }
}
