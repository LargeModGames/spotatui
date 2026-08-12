use super::*;

impl App {
  /// Apply the current [`decoded_shuffle`](Self::decoded_shuffle) flag to whatever
  /// queueable decoded source owns playback: reorder its queue when turning on,
  /// or restore the original order when turning off. The currently-playing track
  /// is unaffected (it stays at the front), so audio continues uninterrupted.
  /// Call only when [`active_queueable_decoded_source`](Self::active_queueable_decoded_source)
  /// holds, so exactly one branch applies.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic", feature = "youtube")),
    allow(unused_variables)
  )]
  fn apply_decoded_shuffle(&mut self) {
    let on = self.decoded_shuffle;
    // While a track change is in flight (`advancing`), an async `play_index`
    // has captured a numeric target it will commit later; reordering the queue
    // now would make that target identify a different track. Defer the reorder —
    // [`reconcile_decoded_shuffle`](Self::reconcile_decoded_shuffle), driven by
    // the runner tick, applies it once the advance commits.
    #[cfg(feature = "local-files")]
    if let Some(s) = self.local_playback.as_mut() {
      if !s.advancing {
        s.set_shuffle(on);
      }
      return;
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_mut() {
      if !s.advancing {
        s.set_shuffle(on);
      }
      return;
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_mut() {
      if !s.advancing {
        s.set_shuffle(on);
      }
    }
  }

  /// Re-sync the active queueable source's queue order to `decoded_shuffle` when
  /// a shuffle toggle was deferred by [`apply_decoded_shuffle`](Self::apply_decoded_shuffle)
  /// because a track change was in flight. Driven by the runner tick, so it lands
  /// as soon as the advance commits (`advancing` clears). A no-op whenever the
  /// order already matches `decoded_shuffle`, so it is cheap to call every tick.
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  pub(crate) fn reconcile_decoded_shuffle(&mut self) {
    // The native queue owns the sink: any per-source struct is a suspended
    // context whose order is reconciled when it resumes, not now.
    if self.queue_owns_playback() {
      return;
    }
    let on = self.decoded_shuffle;
    #[cfg(feature = "local-files")]
    if let Some(s) = self.local_playback.as_mut() {
      if !s.advancing && s.shuffle_backup.is_some() != on {
        s.set_shuffle(on);
      }
      return;
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_mut() {
      if !s.advancing && s.shuffle_backup.is_some() != on {
        s.set_shuffle(on);
      }
      return;
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_mut() {
      if !s.advancing && s.shuffle_backup.is_some() != on {
        s.set_shuffle(on);
      }
    }
  }

  /// Store a Spotify `RepeatState` as the decoded repeat mode. Used by the
  /// source routers to honor an explicit `IoEvent::Repeat` (e.g. a plugin's
  /// `set_repeat`) while a queueable decoded source owns playback, so it updates
  /// `decoded_repeat` instead of falling through to the Spotify context. The
  /// keyboard / MPRIS / `cycle_repeat` paths already update it directly.
  ///
  /// Returns whether it consumed the event: `false` (leaving it to the Spotify
  /// handler) when the native queue owns playback or no queueable decoded source
  /// is active — the same ownership gate as the MPRIS mode setters, so a
  /// suspended context under the queue is never touched.
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  pub(crate) fn set_decoded_repeat_from_state(
    &mut self,
    state: rspotify::model::enums::RepeatState,
  ) -> bool {
    // `RepeatState` is only imported under `streaming`, but this method compiles
    // for decoded-only builds too, so refer to it via a local alias.
    use rspotify::model::enums::RepeatState as Rs;
    if !self.active_queueable_decoded_source() {
      return false;
    }
    self.decoded_repeat = match state {
      Rs::Off => RepeatMode::Off,
      Rs::Context => RepeatMode::Context,
      Rs::Track => RepeatMode::Track,
    };
    true
  }

  /// Set the decoded shuffle to an explicit value from an external media
  /// controller (MPRIS), reordering the active source's queue. Returns whether a
  /// queueable decoded source consumed it. A `false` return does **not** mean
  /// "hand this to Spotify": the caller rejects the request and corrects the
  /// client's property instead, because reaching this method at all means a
  /// decoded source (radio, or the queue slot) owns playback and the Spotify
  /// context is not what the user is listening to.
  #[cfg(all(
    feature = "mpris",
    target_os = "linux",
    any(
      feature = "local-files",
      feature = "subsonic",
      feature = "internet-radio",
      feature = "youtube"
    )
  ))]
  pub fn set_decoded_shuffle(&mut self, on: bool) -> bool {
    if !self.active_queueable_decoded_source() {
      return false;
    }
    if self.decoded_shuffle != on {
      self.decoded_shuffle = on;
      self.apply_decoded_shuffle();
    }
    true
  }

  /// Set the decoded repeat mode to an explicit value from an external media
  /// controller (MPRIS). Returns whether a queueable decoded source consumed it
  /// (see [`set_decoded_shuffle`](Self::set_decoded_shuffle)).
  #[cfg(all(
    feature = "mpris",
    target_os = "linux",
    any(
      feature = "local-files",
      feature = "subsonic",
      feature = "internet-radio",
      feature = "youtube"
    )
  ))]
  pub fn set_decoded_repeat(&mut self, mode: RepeatMode) -> bool {
    if !self.active_queueable_decoded_source() {
      return false;
    }
    self.decoded_repeat = mode;
    true
  }

  pub fn shuffle(&mut self) {
    // Decoded (non-Spotify) sources: toggle the player-global decoded shuffle and
    // reorder the owning source's queue in place (the current track stays at the
    // front, so playback continues uninterrupted).
    if self.active_queueable_decoded_source() {
      self.decoded_shuffle = !self.decoded_shuffle;
      self.apply_decoded_shuffle();
      let label = if self.decoded_shuffle {
        "Shuffle: On"
      } else {
        "Shuffle: Off"
      };
      self.set_status_message(label, 2);
      return;
    }

    // A decoded source owns playback but has no queue to shuffle: internet radio
    // is an endless stream, and the native queue slot plays an explicit list over
    // a suspended context. Both hide the shuffle button and blank shuffle in the
    // MPRIS snapshot, so the key must no-op to match. Falling through would flip
    // shuffle on the user's real Spotify device for a source they are not
    // listening to, invisibly: the playbar on screen is the decoded one, so
    // nothing would reflect the change.
    if self.active_decoded_source() || self.queue_owns_playback() {
      self.set_status_message("Shuffle does not apply to this source", 2);
      return;
    }

    if let Some(shuffle_state) = self
      .current_playback_context
      .as_ref()
      .map(|context| context.shuffle_state)
    {
      let new_shuffle_state = !shuffle_state;
      info!("toggling shuffle: {}", new_shuffle_state);

      // Native streaming: the network handler reorders the client-side shuffle
      // session and reloads it (building one mid-playback when it can); it
      // falls back to Spirc shuffle for contexts the session doesn't cover.
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() && self.streaming_player.is_some() {
        // Remember the desired state for a seamless-recovery replay, then let
        // the network handler reorder/reload the client-side session.
        self.set_native_recovery_shuffle(new_shuffle_state);
        self.dispatch(IoEvent::ToggleNativeShuffleSession(new_shuffle_state));

        // Update UI state immediately
        if let Some(ctx) = &mut self.current_playback_context {
          ctx.shuffle_state = new_shuffle_state;
        }
        self.runtime_state.shuffle_enabled = new_shuffle_state;
        self.schedule_state_save(PersistedRuntimeState::shuffle_enabled(new_shuffle_state));

        // Notify MPRIS clients of the change
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = self.mpris_manager {
          mpris.set_shuffle(new_shuffle_state);
        }
        return;
      }

      // Fallback to API-based shuffle for external devices
      self.dispatch(IoEvent::Shuffle(new_shuffle_state));
    };
  }

  pub fn repeat(&mut self) {
    // Decoded (non-Spotify) sources have no `current_playback_context`; cycle the
    // player-global decoded repeat instead, mirroring Spotify's
    // Off -> Repeat All -> Repeat One -> Off.
    if self.active_queueable_decoded_source() {
      self.decoded_repeat = self.decoded_repeat.next();
      let label = match self.decoded_repeat {
        RepeatMode::Off => "Repeat: Off",
        RepeatMode::Context => "Repeat: All",
        RepeatMode::Track => "Repeat: One",
      };
      self.set_status_message(label, 2);
      return;
    }

    // See `shuffle`: radio and the queue slot have no repeat of their own, and
    // falling through would cycle repeat on the user's real Spotify device with
    // nothing on screen to show for it.
    if self.active_decoded_source() || self.queue_owns_playback() {
      self.set_status_message("Repeat does not apply to this source", 2);
      return;
    }

    if let Some(current_repeat_state) = self
      .current_playback_context
      .as_ref()
      .map(|context| context.repeat_state)
    {
      info!("toggling repeat mode: {:?}", current_repeat_state);

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(player) = self.streaming_player.clone() {
          // Try to set repeat on the native player (pass current state, not next)
          let _ = player.set_repeat(current_repeat_state);

          // Calculate next state for UI update
          let next_repeat_state = match current_repeat_state {
            RepeatState::Off => RepeatState::Context,
            RepeatState::Context => RepeatState::Track,
            RepeatState::Track => RepeatState::Off,
          };
          self.set_native_recovery_repeat(next_repeat_state);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.repeat_state = next_repeat_state;
          }

          // Notify MPRIS clients of the change
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            use crate::infra::mpris::LoopStatusEvent;
            let loop_status = match next_repeat_state {
              RepeatState::Off => LoopStatusEvent::None,
              RepeatState::Context => LoopStatusEvent::Playlist,
              RepeatState::Track => LoopStatusEvent::Track,
            };
            mpris.set_loop_status(loop_status);
          }
          return;
        }
      }

      // Fallback to API-based repeat for external devices
      self.dispatch(IoEvent::Repeat(current_repeat_state));
    }
  }
}
