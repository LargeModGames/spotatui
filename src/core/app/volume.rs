use super::*;

impl App {
  pub fn flush_pending_volume(&mut self) {
    if self.is_volume_change_in_flight {
      return; // previous request still processing
    }
    if let Some(volume) = self.pending_volume {
      if self.last_dispatched_volume == Some(volume) {
        return; // already dispatched this value, waiting for API to confirm
      }
      self.is_volume_change_in_flight = true;
      self.last_dispatched_volume = Some(volume);
      self.dispatch(IoEvent::ChangeVolume(volume));
    }
  }

  /// Returns the volume the UI should show and volume-up/down should use as a base.
  ///
  /// If the user just pressed a volume key, we show their input (not what the API
  /// says) because Spotify can be slow to reflect the change. Without this, you'd
  /// see the percentage jump back to the old value for a split second before
  /// correcting — especially noticeable when spamming volume up/down.
  pub fn desired_volume(&self) -> u32 {
    if let Some(pending) = self.pending_volume {
      return pending as u32;
    }
    self
      .current_playback_context
      .as_ref()
      .and_then(|c| c.device.volume_percent)
      // No Spotify device volume (e.g. a decoded source is playing, or the slim
      // build has no context): fall back to the configured volume, not 0, so the
      // playbar and volume-up/down base math stay correct for every source.
      .unwrap_or(self.runtime_state.volume_percent as u32)
  }

  /// Set volume to an absolute percentage (0-100). Routes through the same
  /// native-streaming fast path and API coalescing logic as the keyboard
  /// volume keys, so Lua actions behave identically to keypresses.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn set_volume_percent(&mut self, volume: u8) {
    let next_volume = volume.min(100);
    let current_volume = self.desired_volume() as u8;

    if next_volume != current_volume {
      info!("setting volume to {}", next_volume);
      // A decoded source owns the sink: route the volume change to its
      // dispatcher (which sets the rodio sink's gain), never to the paused
      // librespot. The dispatcher converts the u8 percentage to a float.
      if self.active_decoded_source() {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
        self.runtime_state.volume_percent = next_volume;
        self.schedule_state_save(PersistedRuntimeState::volume_percent(next_volume));
        self.pending_volume = Some(next_volume);
        return;
      }
      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.set_volume(next_volume);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.device.volume_percent = Some(next_volume.into());
          }
          self.runtime_state.volume_percent = next_volume;
          self.schedule_state_save(PersistedRuntimeState::volume_percent(next_volume));
          self.pending_volume = Some(next_volume);

          // Notify MPRIS clients of the change (VolumeChanged is never emitted by
          // librespot for local mixer changes, so this is the only way the
          // Volume D-Bus property stays in sync)
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_volume(next_volume);
          }
          return;
        }
      }

      // Fallback to API-based volume control for external devices
      // Coalesce: only dispatch if no request is already in flight
      self.pending_volume = Some(next_volume);
      if !self.is_volume_change_in_flight {
        self.is_volume_change_in_flight = true;
        self.dispatch(IoEvent::ChangeVolume(next_volume));
      }
    }
  }

  /// Bump volume up. Uses `desired_volume()` as the base so rapid presses
  /// don't accidentally calculate from a stale API value.
  pub fn increase_volume(&mut self) {
    let current_volume = self.desired_volume() as u8;
    let next_volume = min(
      current_volume + self.user_config.behavior.volume_increment,
      100,
    );

    if next_volume != current_volume {
      info!("increasing volume: {} -> {}", current_volume, next_volume);
      // A decoded source owns the sink: route the volume change to its
      // dispatcher (which sets the rodio sink's gain), never to the paused
      // librespot. The dispatcher converts the u8 percentage to a float.
      if self.active_decoded_source() {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
        self.runtime_state.volume_percent = next_volume;
        self.schedule_state_save(PersistedRuntimeState::volume_percent(next_volume));
        self.pending_volume = Some(next_volume);
        return;
      }
      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.set_volume(next_volume);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.device.volume_percent = Some(next_volume.into());
          }
          self.runtime_state.volume_percent = next_volume;
          self.schedule_state_save(PersistedRuntimeState::volume_percent(next_volume));
          self.pending_volume = Some(next_volume);

          // Notify MPRIS clients of the change (VolumeChanged is never emitted by
          // librespot for local mixer changes, so this is the only way the
          // Volume D-Bus property stays in sync)
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_volume(next_volume);
          }
          return;
        }
      }

      // Fallback to API-based volume control for external devices
      // Coalesce: only dispatch if no request is already in flight
      self.pending_volume = Some(next_volume);
      if !self.is_volume_change_in_flight {
        self.is_volume_change_in_flight = true;
        self.dispatch(IoEvent::ChangeVolume(next_volume));
      }
    }
  }

  /// Bump volume down. Uses `desired_volume()` as the base so rapid presses
  /// don't accidentally calculate from a stale API value.
  pub fn decrease_volume(&mut self) {
    let current_volume = self.desired_volume() as i8;
    let next_volume = max(
      current_volume - self.user_config.behavior.volume_increment as i8,
      0,
    );

    if next_volume != current_volume {
      let next_volume_u8 = next_volume as u8;
      info!(
        "decreasing volume: {} -> {}",
        current_volume, next_volume_u8
      );

      // A decoded source owns the sink: route the volume change to its
      // dispatcher (which sets the rodio sink's gain), never to the paused
      // librespot. The dispatcher converts the u8 percentage to a float.
      if self.active_decoded_source() {
        self.dispatch(IoEvent::ChangeVolume(next_volume_u8));
        self.runtime_state.volume_percent = next_volume_u8;
        self.schedule_state_save(PersistedRuntimeState::volume_percent(next_volume_u8));
        self.pending_volume = Some(next_volume_u8);
        return;
      }
      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.set_volume(next_volume_u8);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.device.volume_percent = Some(next_volume_u8.into());
          }
          self.runtime_state.volume_percent = next_volume_u8;
          self.schedule_state_save(PersistedRuntimeState::volume_percent(next_volume_u8));
          self.pending_volume = Some(next_volume_u8);

          // Notify MPRIS clients of the change (VolumeChanged is never emitted by
          // librespot for local mixer changes, so this is the only way the
          // Volume D-Bus property stays in sync)
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_volume(next_volume_u8);
          }
          return;
        }
      }

      // Fallback to API-based volume control for external devices
      // Coalesce: only dispatch if no request is already in flight
      self.pending_volume = Some(next_volume_u8);
      if !self.is_volume_change_in_flight {
        self.is_volume_change_in_flight = true;
        self.dispatch(IoEvent::ChangeVolume(next_volume_u8));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::core::app::test_support::*;

  // Regression for transport-4: with no Spotify device volume and no pending
  // volume (the state while a decoded source plays, and the whole slim build),
  // `desired_volume` must fall back to the configured volume, not 0. The old
  // `.unwrap_or(0)` made volume-down a dead no-op and the first volume-up snap
  // to the increment. This is the only hardware-free guard for that fix — a
  // source-active transport test needs a real audio device (see report).
  #[test]
  fn desired_volume_falls_back_to_config_when_no_context() {
    let mut app = make_app_simple();
    app.current_playback_context = None;
    app.pending_volume = None;
    app.runtime_state.volume_percent = 42;

    assert_eq!(
      app.desired_volume(),
      42,
      "with no device volume and no pending volume, base volume must come from config, not 0"
    );
  }
}
