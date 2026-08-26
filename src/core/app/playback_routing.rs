use super::*;

impl App {
  /// Check if native streaming is the active playback device
  /// Returns true while the player is connected or reconnecting and it is the
  /// currently active device.
  #[cfg(feature = "streaming")]
  pub(super) fn is_native_streaming_active_for_playback(&self) -> bool {
    // Keep routing controls to the native backend during its bounded in-place
    // reconnect; StreamingPlayer queues Spirc-dependent commands in that window.
    let player_available = self
      .streaming_player
      .as_ref()
      .is_some_and(|p| p.is_available());

    if !player_available {
      return false;
    }

    // Get native device name from player
    let native_device_name = self
      .streaming_player
      .as_ref()
      .map(|p| p.device_name().to_lowercase());

    // If no context yet (e.g., at startup), use the app state flag which is
    // set when the native streaming device is activated/selected.
    let Some(ref ctx) = self.current_playback_context else {
      return self.is_streaming_active;
    };

    // First, check if the current playback device matches the native streaming device ID
    if let (Some(current_id), Some(native_id)) =
      (ctx.device.id.as_ref(), self.native_device_id.as_ref())
    {
      if current_id == native_id {
        return true;
      }
    }

    // Fallback: strict name match (case-insensitive), but only while we have
    // fresh native activity or a recent explicit activation. After a recovery,
    // Spotify can keep returning the old "spotatui" device while the new native
    // player is connected but stopped/not active.
    if let Some(native_name) = native_device_name.as_ref() {
      let current_device_name = ctx.device.name.to_lowercase();
      if current_device_name == native_name.as_str() && self.has_fresh_native_activity() {
        return true;
      }
    }

    // No match - not the active device
    false
  }

  /// Whether Spotify playback is happening on an *external* Connect device
  /// (i.e. a Spotify context exists and it is not our own native streaming
  /// device). When true, `z` on a Spotify track keeps today's Web-API
  /// `AddItemToQueue` behavior instead of routing to the native queue. Under a
  /// build without native streaming, any Spotify context is external by
  /// definition.
  pub fn spotify_external_device_active(&self) -> bool {
    #[cfg(feature = "streaming")]
    {
      self.current_playback_context.is_some() && !self.is_native_streaming_active_for_playback()
    }
    #[cfg(not(feature = "streaming"))]
    {
      self.current_playback_context.is_some()
    }
  }

  /// Whether any decoded-audio source (local file, Subsonic, internet radio, or
  /// YouTube) currently owns the playback session.
  ///
  /// Starting a non-Spotify source only *pauses* librespot; it never clears
  /// `is_streaming_active` / `current_playback_context`, so
  /// [`is_native_streaming_active_for_playback`](Self::is_native_streaming_active_for_playback)
  /// stays true while a decoded source owns the rodio sink. The direct-control
  /// transport methods (next/prev/volume) use this guard to route to the active
  /// source via `IoEvent` dispatch instead of driving the paused librespot.
  ///
  /// Radio is included: routing Next/volume to radio's dispatcher (which no-ops
  /// or handles it) is still correct — we must never drive librespot while a
  /// source is playing. In a build with all source features off this reduces to
  /// `false`.
  pub(crate) fn active_decoded_source(&self) -> bool {
    // The native queue slot playing a decoded track owns the sink even when no
    // per-source `*_playback` context is set (e.g. queueing from an idle app).
    #[cfg(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    ))]
    if self.queue_now_decoded_player().is_some() {
      return true;
    }
    // A queued Spotify track owns the sink via librespot; any remaining
    // `*_playback` below is a suspended context, not the active source.
    if self.queue_now_is_spotify() {
      return false;
    }
    #[cfg(feature = "local-files")]
    if self.local_playback.is_some() {
      return true;
    }
    #[cfg(feature = "subsonic")]
    if self.subsonic_playback.is_some() {
      return true;
    }
    #[cfg(feature = "qobuz")]
    if self.qobuz_playback.is_some() {
      return true;
    }
    #[cfg(feature = "internet-radio")]
    if self.radio_playback.is_some() {
      return true;
    }
    #[cfg(feature = "youtube")]
    if self.youtube_playback.is_some() {
      return true;
    }
    false
  }

  /// Whether a *queueable* decoded source (Local / Subsonic / YouTube) — one with
  /// its own track queue — currently owns playback. Unlike
  /// [`active_decoded_source`](Self::active_decoded_source) this **excludes**
  /// internet radio (an infinite stream with no queue) and the native queue slot
  /// (a suspended context is not the active source). This is the gate for the
  /// decoded repeat/shuffle controls, which only make sense over a real queue.
  /// Also gates which playbar buttons are drawn and clickable (see
  /// `playbar_supported_controls`).
  pub(crate) fn active_queueable_decoded_source(&self) -> bool {
    // The native queue owning the sink is out of scope for repeat/shuffle; any
    // per-source `*_playback` below is then a suspended context, not active.
    if self.queue_owns_playback() {
      return false;
    }
    #[cfg(feature = "local-files")]
    if self.local_playback.is_some() {
      return true;
    }
    #[cfg(feature = "subsonic")]
    if self.subsonic_playback.is_some() {
      return true;
    }
    #[cfg(feature = "qobuz")]
    if self.qobuz_playback.is_some() {
      return true;
    }
    #[cfg(feature = "youtube")]
    if self.youtube_playback.is_some() {
      return true;
    }
    false
  }

  /// The player of whichever decoded source (local file, Subsonic, internet
  /// radio, or YouTube) currently owns the session, or `None` when Spotify (or
  /// nothing) owns it. All four sources decode through the same `LocalPlayer`
  /// sink, so a single accessor covers transport/seek routing for every one.
  /// Ordering mirrors [`Self::active_decoded_source`].
  #[cfg(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "qobuz",
    feature = "internet-radio",
    feature = "youtube"
  ))]
  // Consumed only by the OS media integrations (MPRIS / macOS / Windows), so
  // builds with decoded sources but none of those integrations leave it unused.
  #[cfg_attr(
    not(any(
      all(feature = "mpris", target_os = "linux"),
      all(feature = "macos-media", target_os = "macos"),
      all(feature = "windows-media", target_os = "windows")
    )),
    allow(dead_code)
  )]
  pub fn active_decoded_player(&self) -> Option<&std::sync::Arc<crate::infra::audio::LocalPlayer>> {
    #[cfg(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    ))]
    if let Some(p) = self.queue_now_decoded_player() {
      return Some(p);
    }
    // A queued Spotify track owns the sink via librespot; any remaining
    // `*_playback` below is a suspended context, not the active source.
    if self.queue_now_is_spotify() {
      return None;
    }
    #[cfg(feature = "local-files")]
    if let Some(s) = &self.local_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = &self.subsonic_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "qobuz")]
    if let Some(s) = &self.qobuz_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "internet-radio")]
    if let Some(s) = &self.radio_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = &self.youtube_playback {
      return Some(&s.player);
    }
    None
  }

  /// The current playback position, in milliseconds, of the active *seekable*
  /// decoded source (local file, Subsonic, or YouTube).
  ///
  /// Read live from the source player's sink. Internet radio is intentionally
  /// **excluded** — a live stream is not seekable — so radio returns `None` here
  /// and seek keys become correct no-ops for radio. In a build with all seekable
  /// source features off this reduces to `None`.
  pub(super) fn active_source_position_ms(&self) -> Option<u128> {
    #[cfg(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    ))]
    if let Some(p) = self.queue_now_decoded_player() {
      return Some(p.position().as_millis());
    }
    // A queued Spotify track owns the sink; librespot events drive progress and
    // any remaining `*_playback` below is a suspended context.
    if self.queue_now_is_spotify() {
      return None;
    }
    #[cfg(feature = "local-files")]
    if let Some(local) = &self.local_playback {
      return Some(local.player.position().as_millis());
    }
    #[cfg(feature = "subsonic")]
    if let Some(subsonic) = &self.subsonic_playback {
      return Some(subsonic.player.position().as_millis());
    }
    #[cfg(feature = "qobuz")]
    if let Some(qobuz) = &self.qobuz_playback {
      return Some(qobuz.player.position().as_millis());
    }
    #[cfg(feature = "youtube")]
    if let Some(youtube) = &self.youtube_playback {
      return Some(youtube.player.position().as_millis());
    }
    None
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_queue_slot_shadows_decoded_activity_checks() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    assert!(!app.active_decoded_source());
    assert!(app.active_source_position_ms().is_none());
  }

  #[cfg(all(feature = "streaming", feature = "audio-decode"))]
  #[test]
  fn spotify_queue_slot_shadows_decoded_player_lookup() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    assert!(app.active_decoded_player().is_none());
  }
}
