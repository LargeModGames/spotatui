use super::*;

impl App {
  /// Pause native streaming playback with the full bookkeeping the pause
  /// branch of [`toggle_playback`](Self::toggle_playback) does: clear any
  /// parked StartPlayback and the load watchdog (either would resume or force
  /// recovery against a backend we just gave up on), mark the playback intent
  /// as paused (so a recovery snapshot doesn't resume into that backend
  /// either), pause the player, and flip the UI playing state.
  #[cfg(feature = "streaming")]
  pub fn pause_native_playback(&mut self) {
    self.pending_start_playback = None;
    self.native_load_watchdog = None;
    self.set_native_playback_intent(false);
    if let Some(player) = &self.streaming_player {
      player.pause();
    }
    if let Some(ctx) = &mut self.current_playback_context {
      ctx.is_playing = false;
    }
    self.native_is_playing = Some(false);
  }

  pub fn toggle_playback(&mut self) {
    // The native queue slot owns the sink: toggle its player directly (covers the
    // idle-app case where no per-source context is set).
    #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
    if let Some(player) = self.queue_now_decoded_player() {
      if player.is_paused() {
        player.resume();
      } else {
        player.pause();
      }
      return;
    }

    // Spotify queue playback has no item in `current_playback_context`; route
    // the intended transport action using the librespot state instead. Flip the
    // state optimistically so rapid toggles alternate instead of both reading
    // the stale pre-router value and dispatching the same action twice.
    if self.queue_now_is_spotify() {
      let is_playing = self.native_is_playing.unwrap_or(false);
      self.native_is_playing = Some(!is_playing);
      if is_playing {
        self.dispatch(IoEvent::PausePlayback);
      } else {
        self.dispatch(IoEvent::StartPlayback(None, None, None));
      }
      return;
    }

    // Local-file playback owns the session: toggle the local sink directly. The
    // playbar reads pause state live from the player, so nothing else to update.
    #[cfg(feature = "local-files")]
    if let Some(local) = &self.local_playback {
      if local.player.is_paused() {
        local.player.resume();
      } else {
        local.player.pause();
      }
      return;
    }

    // Subsonic playback owns the session the same way: toggle its sink directly.
    #[cfg(feature = "subsonic")]
    if let Some(subsonic) = &self.subsonic_playback {
      if subsonic.player.is_paused() {
        subsonic.player.resume();
      } else {
        subsonic.player.pause();
      }
      return;
    }

    // YouTube playback owns the session the same way: toggle its sink directly.
    #[cfg(feature = "youtube")]
    if let Some(youtube) = &self.youtube_playback {
      if youtube.player.is_paused() {
        youtube.player.resume();
      } else {
        youtube.player.pause();
      }
      return;
    }

    // Internet-radio playback owns the session the same way: toggle its sink
    // directly. Without this branch radio falls through to the streaming path,
    // which only ever emits a bare resume — so Play/Pause could resume radio but
    // never pause it.
    #[cfg(feature = "internet-radio")]
    if let Some(radio) = &self.radio_playback {
      if radio.player.is_paused() {
        radio.player.resume();
      } else {
        radio.player.pause();
      }
      return;
    }

    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      if self
        .current_playback_context
        .as_ref()
        .and_then(|ctx| ctx.item.as_ref())
        .is_none()
      {
        self.dispatch(IoEvent::StartPlayback(None, None, None));
        return;
      }

      if let Some(player) = self.streaming_player.clone() {
        let is_playing = self
          .native_is_playing
          .or_else(|| self.current_playback_context.as_ref().map(|c| c.is_playing))
          .unwrap_or(false);
        info!(
          "toggling playback: {}",
          if is_playing { "paused" } else { "playing" }
        );
        if is_playing {
          self.pause_native_playback();
        } else {
          self.set_native_playback_intent(true);
          player.play();
          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.is_playing = true;
          }
          self.native_is_playing = Some(true);
        }
        return;
      }
    }

    // Fallback to API-based playback control for external devices
    let is_playing = if self.is_streaming_active {
      self
        .native_is_playing
        .or_else(|| self.current_playback_context.as_ref().map(|c| c.is_playing))
        .unwrap_or(false)
    } else {
      self
        .current_playback_context
        .as_ref()
        .map(|c| c.is_playing)
        .unwrap_or(false)
    };

    if is_playing {
      self.dispatch(IoEvent::PausePlayback);
    } else {
      // When no offset or uris are passed, spotify will resume current playback
      self.dispatch(IoEvent::StartPlayback(None, None, None));
    }
  }

  pub fn previous_track(&mut self) {
    info!("playing previous track or restarting current track");
    #[cfg(feature = "streaming")]
    {
      self.pending_start_playback = None;
      self.native_load_watchdog = None;
    }
    // The native queue owns playback: a forward-only queue has no "previous",
    // so restart the current queued track. The queue router intercepts the
    // dispatched event for both decoded and Spotify queue slots.
    if self.queue_owns_playback() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::PreviousTrack);
      return;
    }
    // A decoded source owns the session: route to its dispatcher, never to the
    // paused librespot. Preserve the ">= 3s restarts current, else previous"
    // semantics (radio no-ops both Seek and PreviousTrack).
    if self.active_decoded_source() {
      if self.song_progress_ms >= 3_000 {
        self.dispatch(IoEvent::Seek(0));
      } else {
        self.dispatch(IoEvent::PreviousTrack);
      }
      self.song_progress_ms = 0;
      return;
    }
    if self.song_progress_ms >= 3_000 {
      // If more than 3 seconds into the song, restart from beginning
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(player) = self.streaming_player.clone() {
          player.seek(0);
          self.song_progress_ms = 0;
          self.seek_ms = None;
          self.set_native_recovery_position(0);
          return;
        }
      }

      // Fallback for external devices
      self.dispatch(IoEvent::Seek(0));
    } else {
      // If less than 3 seconds in, go to previous track
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        // A manual Previous advances the shuffle session even under repeat-one.
        self.mark_native_shuffle_manual_skip(false);
        if let Some(ref player) = self.streaming_player {
          player.activate();
          player.prev();
          // Reset progress immediately for UI feedback
          self.song_progress_ms = 0;
          // librespot can occasionally land in a paused state after a skip.
          // Schedule a short delayed resume to avoid racing the track transition.
          let player = std::sync::Arc::clone(player);
          std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            player.activate();
            player.play();
          });
          return;
        }
      }

      // Fallback for external devices
      self.dispatch(IoEvent::PreviousTrack);
    }
  }

  pub fn force_previous_track(&mut self) {
    info!("force skipping to previous track");
    #[cfg(feature = "streaming")]
    {
      self.pending_start_playback = None;
      self.native_load_watchdog = None;
    }
    // The native queue owns playback: restart the current queued track (the
    // queue router intercepts the event for both slot kinds).
    if self.queue_owns_playback() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::ForcePreviousTrack);
      return;
    }
    // A decoded source owns the session: route to its dispatcher, never to the
    // paused librespot. The source handles or no-ops ForcePreviousTrack.
    if self.active_decoded_source() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::ForcePreviousTrack);
      return;
    }
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      // A manual Previous advances the shuffle session even under repeat-one.
      self.mark_native_shuffle_manual_skip(false);
      if let Some(ref player) = self.streaming_player {
        player.activate();
        // First prev() restarts the current track (if past Spotify's ~3s threshold).
        // After a short delay the second prev() actually skips to the previous track,
        // since the position is now back at 0.
        player.prev();
        self.song_progress_ms = 0;
        let player = std::sync::Arc::clone(player);
        std::thread::spawn(move || {
          std::thread::sleep(std::time::Duration::from_millis(500));
          player.prev();
          std::thread::sleep(std::time::Duration::from_millis(300));
          player.activate();
          player.play();
        });
        return;
      }
    }

    self.song_progress_ms = 0;
    self.dispatch(IoEvent::ForcePreviousTrack);
  }

  pub fn next_track(&mut self) {
    info!("skipping to next track");
    #[cfg(feature = "streaming")]
    {
      self.pending_start_playback = None;
      self.native_load_watchdog = None;
    }
    // The native queue owns playback: skip to the next queued item (or resume
    // the suspended context when the queue drains).
    if self.queue_owns_playback() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return;
    }
    // A decoded context is playing with items waiting in the queue: suspend it
    // (skip semantics — resume at the context's next track) and start the queue.
    // An explicit Next advances the context even under Repeat One, matching the
    // per-source skip paths: repeat-one only replays on *auto* advance.
    if self.active_decoded_source() && !self.native_queue.is_empty() {
      self.suspend_active_decoded_context_for_skip(crate::infra::queue::SuspendCause::ManualSkip);
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return;
    }
    // A decoded source (local/subsonic/radio/youtube) owns the session: route to
    // its dispatcher, never to the paused librespot. The source handles or
    // no-ops NextTrack (radio has no queue).
    if self.active_decoded_source() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::NextTrack);
      return;
    }
    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      // A native-Spotify context is playing with items waiting in the queue:
      // suspend it (skip semantics) and hand the sink to the queue instead of
      // Spirc-advancing the context. (`queue_owns_playback` is already handled
      // above, so here the context, not a queued track, is playing.)
      if !self.native_queue.is_empty() {
        self
          .suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::ManualSkip);
        if let Some(player) = self.streaming_player.as_ref() {
          player.pause();
        }
        self.song_progress_ms = 0;
        self.dispatch(IoEvent::AdvanceNativeQueue);
        return;
      }
      // A manual Next advances the shuffle session even under repeat-one.
      self.mark_native_shuffle_manual_skip(true);
      if let Some(ref player) = self.streaming_player {
        player.activate();
        player.next();
        // Reset progress immediately for UI feedback
        self.song_progress_ms = 0;
        // librespot can occasionally land in a paused state after a skip.
        // Schedule a short delayed resume to avoid racing the track transition.
        let player = std::sync::Arc::clone(player);
        std::thread::spawn(move || {
          std::thread::sleep(std::time::Duration::from_millis(300));
          player.activate();
          player.play();
        });
        return;
      }
    }

    // Fallback for external devices
    self.dispatch(IoEvent::NextTrack);
  }

  pub fn copy_song_url(&mut self) {
    info!("copying song url to clipboard");
    let clipboard = match &mut self.clipboard {
      Some(ctx) => ctx,
      None => return,
    };

    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      match item {
        PlayableItem::Track(track) => {
          let track_id = track.id.as_ref().map(|id| id.id().to_string());

          match track_id {
            Some(id) if !id.is_empty() => {
              if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/track/{}", id)) {
                self.handle_error(anyhow!("failed to set clipboard content: {}", e));
              }
            }
            _ => {
              self.handle_error(anyhow!("Track has no ID"));
            }
          }
        }
        PlayableItem::Episode(episode) => {
          let episode_id = episode.id.id().to_string();
          if let Err(e) =
            clipboard.set_text(format!("https://open.spotify.com/episode/{}", episode_id))
          {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
        _ => {}
      }
    }
  }

  pub fn copy_album_url(&mut self) {
    info!("copying album url to clipboard");
    let clipboard = match &mut self.clipboard {
      Some(ctx) => ctx,
      None => return,
    };

    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      match item {
        PlayableItem::Track(track) => {
          let album_id = track.album.id.as_ref().map(|id| id.id().to_string());

          match album_id {
            Some(id) if !id.is_empty() => {
              if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/album/{}", id)) {
                self.handle_error(anyhow!("failed to set clipboard content: {}", e));
              }
            }
            _ => {
              self.handle_error(anyhow!("Album has no ID"));
            }
          }
        }
        PlayableItem::Episode(episode) => {
          let show_id = episode.show.id.id().to_string();
          if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/show/{}", show_id)) {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
        _ => {}
      }
    }
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  /// When the native queue slot owns playback, `next_track` advances the queue
  /// instead of driving the streaming player's own `next`.
  #[cfg(feature = "streaming")]
  #[test]
  fn next_track_advances_native_queue_when_queue_owns_playback() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.next_track();

    // The first dispatched event is the queue advance, not a Spotify NextTrack.
    assert!(
      matches!(rx.recv().unwrap(), IoEvent::AdvanceNativeQueue),
      "expected AdvanceNativeQueue to be dispatched first"
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn toggle_playback_with_spotify_queue_slot_does_not_panic() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });
    app.native_is_playing = Some(true);

    app.toggle_playback();

    assert!(app.queue_now_is_spotify());
    assert!(matches!(rx.recv().unwrap(), IoEvent::PausePlayback));
    assert_eq!(app.native_is_playing, Some(false));

    // A second toggle before the router echoes back the new state must read
    // the optimistically flipped value and dispatch the opposite action.
    app.toggle_playback();

    assert!(matches!(
      rx.recv().unwrap(),
      IoEvent::StartPlayback(None, None, None)
    ));
    assert_eq!(app.native_is_playing, Some(true));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn previous_track_restarts_native_queue_when_queue_owns_playback() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.previous_track();

    assert!(
      matches!(rx.recv().unwrap(), IoEvent::PreviousTrack),
      "expected PreviousTrack to be dispatched for the queue router"
    );
  }
}
