use super::*;

/// Domain-level representation of the playback queue.
/// Replaces `rspotify::model::CurrentUserQueue` in `App` state.
#[derive(Debug, Clone)]
pub struct QueueState {
  pub currently_playing: Option<PlayableInfo>,
  pub queue: Vec<PlayableInfo>,
}

impl App {
  /// Add a track to the native cross-source queue.
  ///
  /// Rejects tracks with no URI and radio streams (a live stream is not a finite
  /// track). Spotify tracks on an external Connect device keep today's Web-API
  /// queue behavior (there is no native sink to play them through); everything
  /// else is pushed onto [`Self::native_queue`].
  pub fn add_track_to_native_queue(&mut self, track: TrackInfo) {
    let Some(uri) = track.uri.clone() else {
      self.set_status_message("Cannot queue: track has no URI", 3);
      return;
    };
    if uri.starts_with("radio:") {
      self.set_status_message("Radio stations can't be queued", 3);
      return;
    }
    // A Spotify track controlled on an external device has no native sink to
    // play through, so fall back to the Spotify Web-API queue.
    if matches!(
      crate::core::queue::queue_item_source(&uri),
      crate::core::queue::QueueItemSource::Spotify
    ) && self.spotify_external_device_active()
    {
      self.dispatch(IoEvent::AddItemToQueue(uri));
      return;
    }
    let name = track.name.clone();
    self.native_queue.push(track);
    self.set_status_message(format!("Queued: {name}"), 3);
    // Keep the Spotify mirror queue ([`Self::queue`]) current while a native
    // Spotify context is playing: it is the snapshot source for the resume
    // target when this newly-queued item later suspends the context.
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() && !self.queue_owns_playback() {
      self.dispatch(IoEvent::GetQueue);
    }
  }

  /// The position while its URI still matches, else the first item with that
  /// URI (the queue can hold duplicates).
  fn queue_position_for(&self, uri: &str, position: usize) -> Option<usize> {
    if self
      .native_queue
      .get(position)
      .and_then(|t| t.uri.as_deref())
      == Some(uri)
    {
      return Some(position);
    }
    self
      .native_queue
      .iter()
      .position(|t| t.uri.as_deref() == Some(uri))
  }

  /// Jump to a queued item: drop everything before it and hand the sink to
  /// the queue, suspending a playing context mid-track first.
  pub(crate) fn play_queue_item(&mut self, uri: &str, position: usize) {
    let Some(skip) = self.queue_position_for(uri, position) else {
      return;
    };
    // Drop the items before the selected one so it becomes the queue head.
    self.native_queue.drain(..skip);
    if !self.queue_owns_playback() {
      self.suspend_active_decoded_context_mid_track();
      // No decoded context recorded a suspension: a native-Spotify context may be
      // playing instead — suspend it so the queue hands back to it on drain.
      #[cfg(feature = "streaming")]
      if self.queue_suspended.is_none() {
        self.suspend_native_spotify_context_mid_track();
      }
    }
    self.dispatch(IoEvent::AdvanceNativeQueue);
  }

  /// Remove one native-queue item; a no-op when it is not in the queue.
  pub(crate) fn remove_queue_item(&mut self, uri: &str, position: usize) {
    if let Some(idx) = self.queue_position_for(uri, position) {
      self.native_queue.remove(idx);
    }
  }

  /// Move one native-queue item to index `to`; a no-op when out of range.
  pub(crate) fn move_queue_item(&mut self, uri: &str, from: usize, to: usize) {
    let Some(from) = self.queue_position_for(uri, from) else {
      return;
    };
    if to >= self.native_queue.len() {
      return;
    }
    let item = self.native_queue.remove(from);
    self.native_queue.insert(to, item);
  }

  /// Whether the native queue's playback slot currently owns the output (either a
  /// decoded queued track or a native-streamed Spotify one). The single gated
  /// entry point for every queue-ownership check; reduces to `false` in a build
  /// where the slot cannot exist (slim, or one whose only decoded source is
  /// internet radio, which is never queueable).
  pub fn queue_owns_playback(&self) -> bool {
    #[cfg(any(
      feature = "streaming",
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    ))]
    {
      self.queue_now.is_some()
    }
    #[cfg(not(any(
      feature = "streaming",
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    )))]
    {
      false
    }
  }

  /// Whether the queue slot is playing a Spotify track via native streaming.
  /// While true, librespot owns the sink and any still-`Some` decoded
  /// `*_playback` struct is a *suspended* context that must stay invisible to
  /// the decoded-ownership predicates (`active_decoded_source`,
  /// `active_decoded_player`, transport routing) — otherwise a space-bar toggle
  /// or media key would resume the suspended player on top of librespot.
  pub(crate) fn queue_now_is_spotify(&self) -> bool {
    #[cfg(feature = "streaming")]
    {
      matches!(
        self.queue_now.as_ref(),
        Some(QueueNowPlaying::Spotify { .. })
      )
    }
    #[cfg(not(feature = "streaming"))]
    {
      false
    }
  }

  /// The Spotify URI of the track playing through the queue slot, when the slot
  /// holds a native-streamed Spotify track. `None` for a decoded slot, an empty
  /// slot, or a slot whose track carries no URI. This is the resolution target
  /// for track-level actions (e.g. Like) while the slot owns playback: the
  /// cached `current_playback_context` still names the suspended context's
  /// track, so it must not be consulted.
  pub(crate) fn queue_now_spotify_track_uri(&self) -> Option<String> {
    #[cfg(feature = "streaming")]
    {
      match self.queue_now.as_ref()? {
        #[cfg(any(
          feature = "local-files",
          feature = "subsonic",
          feature = "qobuz",
          feature = "youtube"
        ))]
        QueueNowPlaying::Decoded(_) => None,
        QueueNowPlaying::Spotify { track } => track.uri.clone(),
      }
    }
    #[cfg(not(feature = "streaming"))]
    {
      None
    }
  }

  /// The queue slot's player when it is playing a *decoded* queued track (local /
  /// Subsonic / Qobuz / YouTube). `None` for a Spotify slot or an empty slot. Gated
  /// on exactly those four sources: they are the decoded ones a queue item can
  /// name, so a build whose only decoded source is internet radio has no
  /// decoded slot to look up.
  #[cfg(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "qobuz",
    feature = "youtube"
  ))]
  pub fn queue_now_decoded_player(&self) -> Option<&Arc<crate::infra::audio::LocalPlayer>> {
    match self.queue_now.as_ref()? {
      QueueNowPlaying::Decoded(d) => Some(&d.player),
      #[cfg(feature = "streaming")]
      QueueNowPlaying::Spotify { .. } => None,
    }
  }

  /// Take the queue slot, returning its player when it was a decoded track (so
  /// the caller can stop it). Clears [`Self::queue_now`] either way.
  #[cfg(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "qobuz",
    feature = "youtube"
  ))]
  pub fn take_queue_now_decoded_player(&mut self) -> Option<Arc<crate::infra::audio::LocalPlayer>> {
    match self.queue_now.take() {
      Some(QueueNowPlaying::Decoded(d)) => Some(d.player),
      _ => None,
    }
  }

  /// The track currently playing through the queue slot, if any. Used by
  /// persistence to prepend it back onto the saved queue so a mid-queue quit
  /// doesn't lose the in-flight track.
  pub(super) fn queue_now_track(&self) -> Option<&TrackInfo> {
    #[cfg(any(
      feature = "streaming",
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    ))]
    {
      match self.queue_now.as_ref()? {
        #[cfg(any(
          feature = "local-files",
          feature = "subsonic",
          feature = "qobuz",
          feature = "youtube"
        ))]
        QueueNowPlaying::Decoded(d) => Some(&d.track),
        #[cfg(feature = "streaming")]
        QueueNowPlaying::Spotify { track } => Some(track),
      }
    }
    #[cfg(not(any(
      feature = "streaming",
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    )))]
    {
      None
    }
  }

  /// A `"{name} - {artists}"` label for the track playing through the queue slot,
  /// for the Queue screen's "Now playing" row. `None` when the slot is empty.
  pub fn queue_now_display(&self) -> Option<String> {
    let track = self.queue_now_track()?;
    Some(format!("{} - {}", track.name, track.artists.join(", ")))
  }

  /// Handle a native-streaming `EndOfTrack` while the native queue is in play.
  ///
  /// Returns `true` when the queue took over (an `AdvanceNativeQueue` was
  /// dispatched, so the caller must **not** fall back to
  /// `EnsurePlaybackContinues`), `false` to let the normal continue-playback path
  /// run. Three cases:
  ///
  /// - **A queued Spotify track just ended** (`queue_now_is_spotify`): clear the
  ///   slot *now* — before the advance is processed — so the Spirc self-advance
  ///   guard can't see the stale slot on the next `TrackChanged` and reissue the
  ///   finished track over the next item's download window. Pause librespot
  ///   (Spirc may already be loading its own next track) and advance the queue.
  /// - **A stray librespot `EndOfTrack` while a decoded queued track owns the
  ///   sink** (`queue_owns_playback` without a Spotify slot): consume it without
  ///   touching the queue — advancing would skip the audible decoded track, and
  ///   `EnsurePlaybackContinues` would resume Spotify over it.
  /// - **A context track ended with items waiting** (queue idle, non-empty):
  ///   snapshot the Spotify context for resume, `pause()` the streaming player to
  ///   preempt Spirc's own auto-advance, then advance the queue.
  #[cfg(feature = "streaming")]
  pub(crate) fn handle_native_spotify_track_end(&mut self) -> bool {
    if self.queue_now_is_spotify() {
      self.queue_now = None;
      self.spotify_queue_guard_reloads = 0;
      if let Some(player) = self.streaming_player.as_ref() {
        player.pause();
      }
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return true;
    }
    if self.queue_owns_playback() {
      return true;
    }
    if !self.native_queue.is_empty() {
      self.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);
      // Preempt Spirc: after a direct `player.load`, Spirc may try to advance to
      // the next context track on its own. Pausing first stops that before the
      // queue slot takes the sink.
      if let Some(player) = self.streaming_player.as_ref() {
        player.pause();
      }
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return true;
    }
    false
  }

  /// Spirc self-advance guard for the native-Spotify queue slot.
  ///
  /// A queued Spotify track plays via a direct `player.load` (no Spirc context),
  /// so Spirc may try to advance to the next context track on its own when the
  /// queued track ends. Given the base62 id of the track librespot just switched
  /// to, this returns `Some(uri)` to reissue the queued track (Spirc fought
  /// back), or `None` to leave playback alone. Bounded by
  /// [`Self::spotify_queue_guard_reloads`] so a genuinely-gone track can't wedge
  /// a reload loop; the budget resets whenever the queued track is confirmed
  /// playing. See Risk #1 in the plan — the mitigation is pending a live
  /// experiment and cannot be verified without a real Spotify session.
  #[cfg(feature = "streaming")]
  pub(crate) fn spotify_queue_guard_reload_uri(
    &mut self,
    playing_base62_id: &str,
  ) -> Option<String> {
    let queued_uri = match self.queue_now.as_ref()? {
      #[cfg(any(
        feature = "local-files",
        feature = "subsonic",
        feature = "qobuz",
        feature = "youtube"
      ))]
      QueueNowPlaying::Decoded(_) => return None,
      QueueNowPlaying::Spotify { track } => track.uri.clone(),
    }?;
    let queued_id = base62_id_of(&queued_uri);
    if queued_id == playing_base62_id {
      // The queued track is (re)confirmed playing: clear the retry budget.
      self.spotify_queue_guard_reloads = 0;
      return None;
    }
    const MAX_RELOADS: u8 = 2;
    if self.spotify_queue_guard_reloads >= MAX_RELOADS {
      return None;
    }
    self.spotify_queue_guard_reloads += 1;
    Some(queued_uri)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  #[test]
  fn add_track_to_native_queue_pushes_normal_track() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.add_track_to_native_queue(queue_track(Some("subsonic:track:1"), "Song"));
    assert_eq!(app.native_queue.len(), 1);
    assert_eq!(app.native_queue[0].name, "Song");
    // No Web-API dispatch for a non-Spotify item.
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_rejects_missing_uri() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.add_track_to_native_queue(queue_track(None, "No URI"));
    assert!(app.native_queue.is_empty());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_rejects_radio() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.add_track_to_native_queue(queue_track(Some("radio:https://example.com/live"), "Live"));
    assert!(app.native_queue.is_empty());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_spotify_no_context_pushes_natively() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // No playback context => not external => queue natively (Spotify tracks play
    // via native streaming).
    app.add_track_to_native_queue(queue_track(Some("spotify:track:abc"), "Spotify Song"));
    assert_eq!(app.native_queue.len(), 1);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_spotify_external_device_dispatches_web_api() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // A Spotify context with no native streaming device reads as external.
    app.current_playback_context = Some(make_external_context());
    app.add_track_to_native_queue(queue_track(Some("spotify:track:abc"), "Spotify Song"));
    // Routed to the Web-API queue; nothing pushed to the native queue.
    assert!(app.native_queue.is_empty());
    match rx.recv().unwrap() {
      IoEvent::AddItemToQueue(uri) => assert_eq!(uri, "spotify:track:abc"),
      _ => panic!("expected AddItemToQueue dispatch"),
    }
  }

  /// The Spirc self-advance guard compares bare ids: the queued track's own
  /// `TrackChanged` (normalized at the event boundary) reads as confirmation,
  /// not as Spirc fighting back. (Comparing against the full URI reissued every
  /// queued track up to the retry budget, restarting it from position 0.)
  #[cfg(feature = "streaming")]
  #[test]
  fn queue_guard_confirms_queued_track_from_normalized_event_id() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    let playing_uri = "spotify:track:queued".to_string();
    assert_eq!(
      app.spotify_queue_guard_reload_uri(base62_id_of(&playing_uri)),
      None,
      "the queued track's own TrackChanged must not trigger a reload"
    );
  }

  /// When the queued Spotify track ends, the slot must be cleared *before* the
  /// advance is dispatched — a stale slot lets the Spirc self-advance guard
  /// reissue the finished track over the next item's download window (heard as
  /// "the Spotify song keeps playing while the YouTube track downloads").
  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_slot_end_clears_slot_before_advancing() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });
    app.spotify_queue_guard_reloads = 1;

    assert!(app.handle_native_spotify_track_end());

    assert!(!app.queue_owns_playback(), "the ended slot is cleared");
    assert_eq!(app.spotify_queue_guard_reloads, 0);
    assert!(
      app
        .spotify_queue_guard_reload_uri("some-other-track-id")
        .is_none(),
      "an empty slot must never reissue the finished track"
    );
    assert!(matches!(rx.recv().unwrap(), IoEvent::AdvanceNativeQueue));
  }

  /// The Spirc self-advance guard reissues the queued track only when Spirc has
  /// switched away from it, and only within its bounded retry budget.
  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_queue_guard_reissues_only_on_mismatch_and_within_budget() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    // Same track (base62 id): no reissue, budget stays clear.
    assert_eq!(app.spotify_queue_guard_reload_uri("queued"), None);
    assert_eq!(app.spotify_queue_guard_reloads, 0);

    // Spirc switched away: reissue our track, up to the cap, then stop.
    assert_eq!(
      app.spotify_queue_guard_reload_uri("other").as_deref(),
      Some("spotify:track:queued")
    );
    assert_eq!(
      app.spotify_queue_guard_reload_uri("other").as_deref(),
      Some("spotify:track:queued")
    );
    assert_eq!(app.spotify_queue_guard_reload_uri("other"), None);
    assert_eq!(app.spotify_queue_guard_reloads, 2);

    // The queued track being confirmed playing again resets the budget.
    assert_eq!(app.spotify_queue_guard_reload_uri("queued"), None);
    assert_eq!(app.spotify_queue_guard_reloads, 0);
  }
}
