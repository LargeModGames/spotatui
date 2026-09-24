use super::*;

pub(crate) const NOTHING_PLAYING_STATUS: &str = "Nothing is playing";

/// The status for a Spotify gesture the parked native backend cannot serve.
const SPOTIFY_PARKED_STATUS: &str = "Press play to resume Spotify";

/// The status shown when a Spotify-bound request finds no session.
pub(crate) const SPOTIFY_NOT_CONNECTED_STATUS: &str =
  "Spotify not connected. Press `d` and pick Spotify to log in.";

/// Who owns the audio output, in the order the transport chains check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaybackOwner {
  /// The native queue slot (a decoded track or a queued Spotify track).
  Queue,
  /// A decoded source's sink (Local, Subsonic, Qobuz, Radio, YouTube).
  Decoded,
  /// librespot as the active Connect device.
  #[cfg_attr(not(feature = "streaming"), allow(dead_code))]
  NativeSpotify,
  /// A Spotify session with no local player: an external device or idle.
  Spotify,
  /// No player and no session.
  None,
}

impl PlaybackOwner {
  /// Whether the native queue slot or a decoded source holds the sink.
  pub(crate) fn owns_local_sink(self) -> bool {
    match self {
      PlaybackOwner::Queue | PlaybackOwner::Decoded => true,
      PlaybackOwner::NativeSpotify | PlaybackOwner::Spotify | PlaybackOwner::None => false,
    }
  }
}

/// The item a track-level action on "what is playing now" can act on.
pub(super) enum PlayingItem<'a> {
  /// Spotify owns playback and the cached context names the item. Under
  /// native streaming it lags the player after a skip until the next poll.
  Spotify(&'a PlayableItem),
  /// A Spotify track plays through the native queue slot; the cached context
  /// names the suspended context's track, so the slot's own track is the item.
  QueuedSpotify(&'a TrackInfo),
  /// A decoded source or a decoded queue item owns the sink.
  NotSpotify,
  /// No owner, or a Spotify owner with no item.
  Nothing,
}

/// The pure half of [`App::playing_item`], so every owner is testable without
/// an audio device.
fn resolve_playing_item<'a>(
  owner: PlaybackOwner,
  slot_track: Option<&'a TrackInfo>,
  slot_is_spotify: bool,
  cached_item: Option<&'a PlayableItem>,
) -> PlayingItem<'a> {
  match owner {
    PlaybackOwner::Queue => match slot_track {
      Some(track) if slot_is_spotify => PlayingItem::QueuedSpotify(track),
      _ => PlayingItem::NotSpotify,
    },
    PlaybackOwner::Decoded => PlayingItem::NotSpotify,
    PlaybackOwner::NativeSpotify | PlaybackOwner::Spotify => {
      cached_item.map_or(PlayingItem::Nothing, PlayingItem::Spotify)
    }
    PlaybackOwner::None => PlayingItem::Nothing,
  }
}

impl App {
  /// Resolve the item playing now through the ownership order.
  pub(super) fn playing_item(&self) -> PlayingItem<'_> {
    resolve_playing_item(
      self.playback_owner(),
      self.queue_now_track(),
      self.queue_now_is_spotify(),
      self
        .current_playback_context
        .as_ref()
        .and_then(|context| context.item.as_ref()),
    )
  }

  pub(crate) fn playback_owner(&self) -> PlaybackOwner {
    if self.queue_owns_playback() {
      return PlaybackOwner::Queue;
    }
    if self.active_decoded_source() {
      return PlaybackOwner::Decoded;
    }
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      return PlaybackOwner::NativeSpotify;
    }
    if self.spotify_connected {
      return PlaybackOwner::Spotify;
    }
    PlaybackOwner::None
  }

  /// Whether librespot is the right player for a command aimed at it. True
  /// under a Spotify queue slot, whose track librespot plays.
  pub(crate) fn native_should_drive(&self) -> bool {
    !self.active_decoded_source()
  }

  /// Whether a path that restores or continues the cached Spotify context may
  /// run. Also false under a queue slot, whose direct load suspended it.
  pub(crate) fn native_context_should_drive(&self) -> bool {
    self.native_should_drive() && !self.queue_owns_playback()
  }

  /// Record that `source` took the audio sink; its start path calls this
  /// before it pauses librespot.
  #[cfg(any(test, feature = "audio-decode"))]
  pub(crate) fn claim_decoded_sink(&mut self, source: Source) {
    self.decoded_sink_claim = Some(source);
  }

  /// Spotify takes the sink back: an explicit Spotify start reached the
  /// network layer.
  pub(crate) fn release_decoded_sink_claim(&mut self) {
    #[cfg(any(test, feature = "audio-decode"))]
    {
      self.decoded_sink_claim = None;
    }
  }

  /// Whether a decoded source holds the sink claim, session or not.
  pub(crate) fn decoded_sink_claimed(&self) -> bool {
    #[cfg(any(test, feature = "audio-decode"))]
    {
      self.decoded_sink_claim.is_some()
    }
    #[cfg(not(any(test, feature = "audio-decode")))]
    {
      false
    }
  }

  /// The last arm of a transport chain: the Web API when a session exists,
  /// otherwise a source-neutral status instead of the "not connected" nag. A
  /// parked native backend takes a bare resume as its rebuild and refuses the
  /// rest.
  pub(crate) fn dispatch_spotify_fallback(&mut self, event: IoEvent) {
    if self.playback_owner() == PlaybackOwner::None {
      self.set_status_message(NOTHING_PLAYING_STATUS, 4);
      return;
    }
    #[cfg(feature = "streaming")]
    if self.native_parked_here() {
      let bare_resume = matches!(event, IoEvent::StartPlayback(None, None, None));
      if !bare_resume || self.native_backend_pending {
        // A play press during the rebuild still asks the restore to play.
        if bare_resume {
          self.arm_native_play_intent();
        }
        self.refuse_parked_gesture();
      } else if self.native_playback_recovery.is_none() {
        self.set_status_message(NOTHING_PLAYING_STATUS, 4);
      } else {
        // The rebuild restores the snapshot, which now asks to play.
        self.set_native_playback_intent(true);
        self.reacquire_parked_backend();
      }
      return;
    }
    self.dispatch(event);
  }

  /// Answer a Spotify gesture the parked native backend cannot serve.
  pub(crate) fn refuse_parked_gesture(&mut self) {
    #[cfg(feature = "streaming")]
    if self.native_backend_pending {
      self.set_status_message("Reconnecting native streaming…", 5);
      return;
    }
    self.set_status_message(SPOTIFY_PARKED_STATUS, 4);
  }

  /// Whether the cached Spotify playback belongs to the parked native backend,
  /// whoever owns the sink now.
  pub(crate) fn native_parked_owns_context(&self) -> bool {
    #[cfg(feature = "streaming")]
    {
      self.native_backend_parked()
        && match self.current_playback_context.as_ref() {
          Some(ctx) => ctx.device.id.is_some() && ctx.device.id == self.native_device_id,
          None => self.native_playback_recovery.is_some(),
        }
    }
    #[cfg(not(feature = "streaming"))]
    {
      false
    }
  }

  /// Whether another Spotify Connect device plays the cached playback.
  #[cfg(feature = "streaming")]
  pub(crate) fn spotify_playing_elsewhere(&self) -> bool {
    self.current_playback_context.as_ref().is_some_and(|ctx| {
      ctx.is_playing && ctx.device.id.is_some() && ctx.device.id != self.native_device_id
    })
  }

  /// Whether Spotify transport would land on the parked native backend.
  pub(crate) fn native_parked_here(&self) -> bool {
    #[cfg(feature = "streaming")]
    {
      self.native_parked_owns_context() && self.playback_owner() == PlaybackOwner::Spotify
    }
    #[cfg(not(feature = "streaming"))]
    {
      false
    }
  }

  /// `Some(true)` when a decoded source owns the sink and plays, `Some(false)`
  /// when it owns the sink and is paused, `None` when none owns it.
  pub(crate) fn decoded_playing_state(&self) -> Option<bool> {
    #[cfg(feature = "audio-decode")]
    {
      self.active_decoded_player().map(|p| !p.is_paused())
    }
    #[cfg(not(feature = "audio-decode"))]
    {
      None
    }
  }

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
      self.current_playback_context.is_some()
        && !self.is_native_streaming_active_for_playback()
        && !self.native_parked_owns_context()
    }
    #[cfg(not(feature = "streaming"))]
    {
      self.current_playback_context.is_some()
    }
  }

  /// Whether any decoded-audio source (local file, Subsonic, internet radio, or
  /// YouTube) currently owns the playback session.
  ///
  /// Starting a non-Spotify source pauses or parks librespot; it never clears
  /// `is_streaming_active` / `current_playback_context`, so
  /// [`is_native_streaming_active_for_playback`](Self::is_native_streaming_active_for_playback)
  /// can stay true while a decoded source owns the rodio sink. The direct-control
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
    #[cfg(feature = "audio-decode-queue")]
    if self.queue_now_decoded_player().is_some() {
      return true;
    }
    // A queued Spotify track owns the sink via librespot; any remaining
    // `*_playback` below is a suspended context, not the active source.
    if self.queue_now_is_spotify() {
      return false;
    }
    // A decoded start in flight, or a source whose session died with nothing to
    // replace it, still owns the sink: librespot is paused or parked underneath.
    #[cfg(any(test, feature = "audio-decode"))]
    if self.decoded_sink_claim.is_some() {
      return true;
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

  /// Take every decoded session except `keep`'s, so one backend can own the
  /// output device, and return their players. Stop them off the `App` lock: a
  /// sink clear waits for the audio thread.
  #[cfg(feature = "audio-decode")]
  pub(crate) fn take_decoded_sessions_except(
    &mut self,
    keep: crate::core::source::Source,
  ) -> Vec<std::sync::Arc<crate::infra::audio::LocalPlayer>> {
    use crate::core::source::Source;
    use std::sync::Arc;
    let mut players = Vec::new();
    #[cfg(feature = "local-files")]
    if keep != Source::Local {
      players.extend(self.local_playback.take().map(|s| Arc::clone(&s.player)));
    }
    #[cfg(feature = "subsonic")]
    if keep != Source::Subsonic {
      players.extend(self.subsonic_playback.take().map(|s| Arc::clone(&s.player)));
    }
    #[cfg(feature = "qobuz")]
    if keep != Source::Qobuz {
      players.extend(self.qobuz_playback.take().map(|s| Arc::clone(&s.player)));
    }
    #[cfg(feature = "internet-radio")]
    if keep != Source::Radio {
      players.extend(self.radio_playback.take().map(|s| Arc::clone(&s.player)));
    }
    #[cfg(feature = "youtube")]
    if keep != Source::YouTube {
      players.extend(self.youtube_playback.take().map(|s| Arc::clone(&s.player)));
    }
    players
  }

  /// Whether the decoded source that owns the sink is between tracks: its
  /// snapshot can name the next track while the sink still plays the last one.
  pub(crate) fn decoded_change_pending(&self) -> bool {
    #[cfg(feature = "audio-decode")]
    {
      #[cfg(feature = "audio-decode-queue")]
      if let Some(crate::infra::queue::QueueNowPlaying::Decoded(d)) = self.queue_now.as_ref() {
        return d.advancing;
      }
      if self.queue_now_is_spotify() {
        return false;
      }
      #[cfg(feature = "local-files")]
      if let Some(s) = &self.local_playback {
        return s.advancing;
      }
      #[cfg(feature = "subsonic")]
      if let Some(s) = &self.subsonic_playback {
        return s.advancing;
      }
      #[cfg(feature = "qobuz")]
      if let Some(s) = &self.qobuz_playback {
        return s.advancing;
      }
      #[cfg(feature = "youtube")]
      if let Some(s) = &self.youtube_playback {
        return s.advancing;
      }
      false
    }
    #[cfg(not(feature = "audio-decode"))]
    {
      false
    }
  }

  /// The player of whichever decoded source (local file, Subsonic, Qobuz,
  /// internet radio, or YouTube) currently owns the session, or `None` when
  /// Spotify (or nothing) owns it. All five decode through the same `LocalPlayer`
  /// sink, so a single accessor covers transport/seek routing for every one.
  /// Ordering mirrors [`Self::active_decoded_source`].
  #[cfg(feature = "audio-decode")]
  pub fn active_decoded_player(&self) -> Option<&std::sync::Arc<crate::infra::audio::LocalPlayer>> {
    #[cfg(feature = "audio-decode-queue")]
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
    #[cfg(feature = "audio-decode-queue")]
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  #[cfg(feature = "streaming")]
  #[test]
  fn a_bare_resume_rebuilds_the_parked_backend_only_while_its_device_holds_the_playback() {
    let (mut app, rx, mut recovery_rx) = parked_native_app();
    app.toggle_playback();
    assert!(rx.try_recv().is_err());
    assert!(recovery_rx
      .try_recv()
      .is_ok_and(|request| request.reacquire));
    assert!(app
      .native_playback_recovery
      .as_ref()
      .is_some_and(|snapshot| snapshot.desired_playing));
    app.toggle_playback();
    assert!(recovery_rx.try_recv().is_err());

    // A phone holds the playback: the Web API route of today.
    let (mut app, rx, mut recovery_rx) = parked_native_app();
    app.current_playback_context = Some(make_external_context());
    app.is_streaming_active = false;
    app.toggle_playback();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
    assert!(recovery_rx.try_recv().is_err());

    // Nothing is known to resume: no rebuild.
    let (mut app, rx, mut recovery_rx) = parked_native_app();
    app.current_playback_context = None;
    app.native_playback_recovery = None;
    app.is_streaming_active = false;
    app.toggle_playback();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
    assert!(recovery_rx.try_recv().is_err());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn a_play_press_during_the_rebuild_asks_the_restore_to_play() {
    let (mut app, rx, mut recovery_rx) = parked_native_app();
    app.native_backend_pending = true;

    app.toggle_playback();

    assert!(app
      .native_playback_recovery
      .as_ref()
      .is_some_and(|snapshot| snapshot.desired_playing));
    assert!(recovery_rx.try_recv().is_err());
    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some("Reconnecting native streaming…"));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn a_queue_add_is_refused_while_the_parked_device_held_the_playback() {
    let (mut app, rx, _recovery_rx) = parked_native_app();
    app.claim_decoded_sink(Source::YouTube);

    let _ = app.apply(crate::core::action::Action::AddToQueue(
      "spotify:track:x".to_string(),
    ));

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some(SPOTIFY_PARKED_STATUS));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn only_a_playing_foreign_device_is_playing_elsewhere() {
    let (mut app, _rx, _recovery_rx) = parked_native_app();
    assert!(!app.spotify_playing_elsewhere());

    app.current_playback_context = Some(make_external_context());
    assert!(app.spotify_playing_elsewhere());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn queueing_a_spotify_track_over_the_parked_device_uses_the_native_queue() {
    let (mut app, rx, _recovery_rx) = parked_native_app();

    app.add_track_to_native_queue(queue_track(Some("spotify:track:x"), "X"));

    assert_eq!(app.native_queue.len(), 1);
    assert!(rx.try_recv().is_err());
    app.current_playback_context = Some(make_external_context());
    assert!(app.spotify_external_device_active());
  }

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
    assert_eq!(app.playback_owner(), PlaybackOwner::Queue);
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

  #[test]
  fn playing_item_follows_the_owner() {
    let slot = queue_track(Some("spotify:track:queued"), "Queued");
    let cached = PlayableItem::Track(full_track("0000000000000000000001", "Cached"));

    assert!(matches!(
      resolve_playing_item(PlaybackOwner::Decoded, None, false, Some(&cached)),
      PlayingItem::NotSpotify
    ));
    assert!(matches!(
      resolve_playing_item(PlaybackOwner::Queue, Some(&slot), false, Some(&cached)),
      PlayingItem::NotSpotify
    ));
    assert!(matches!(
      resolve_playing_item(PlaybackOwner::Queue, Some(&slot), true, Some(&cached)),
      PlayingItem::QueuedSpotify(track) if track.name == "Queued"
    ));
    assert!(matches!(
      resolve_playing_item(PlaybackOwner::Spotify, None, false, Some(&cached)),
      PlayingItem::Spotify(PlayableItem::Track(track)) if track.name == "Cached"
    ));
    assert!(matches!(
      resolve_playing_item(PlaybackOwner::NativeSpotify, None, false, None),
      PlayingItem::Nothing
    ));
    assert!(matches!(
      resolve_playing_item(PlaybackOwner::None, None, false, Some(&cached)),
      PlayingItem::Nothing
    ));
  }

  #[test]
  fn playback_owner_is_none_without_a_session() {
    let (app, _rx) = session_free_app();

    assert_eq!(app.playback_owner(), PlaybackOwner::None);
  }

  #[test]
  fn playback_owner_is_spotify_with_a_session_and_no_player() {
    assert_eq!(make_app_simple().playback_owner(), PlaybackOwner::Spotify);
  }

  #[test]
  fn dispatch_spotify_fallback_reports_nothing_playing_without_a_session() {
    let (mut app, rx) = session_free_app();

    app.dispatch_spotify_fallback(IoEvent::NextTrack);

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message.as_deref(), Some(NOTHING_PLAYING_STATUS));
  }

  #[test]
  fn dispatch_spotify_fallback_dispatches_with_a_session() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.dispatch_spotify_fallback(IoEvent::NextTrack);

    assert!(matches!(rx.try_recv(), Ok(IoEvent::NextTrack)));
  }

  #[test]
  fn only_the_queue_and_a_decoded_source_own_the_local_sink() {
    assert!(PlaybackOwner::Queue.owns_local_sink());
    assert!(PlaybackOwner::Decoded.owns_local_sink());
    assert!(!PlaybackOwner::NativeSpotify.owns_local_sink());
    assert!(!PlaybackOwner::Spotify.owns_local_sink());
    assert!(!PlaybackOwner::None.owns_local_sink());
  }

  #[test]
  fn a_claimed_decoded_sink_owns_playback_without_a_session() {
    let mut app = make_app_simple();
    app.claim_decoded_sink(Source::YouTube);

    assert!(app.active_decoded_source());
    assert!(app.decoded_sink_claimed());
    assert_eq!(app.playback_owner(), PlaybackOwner::Decoded);
    assert!(!app.active_queueable_decoded_source());
    assert!(app.active_source_position_ms().is_none());
  }

  #[test]
  fn releasing_the_claim_hands_the_sink_back_to_spotify() {
    let mut app = make_app_simple();
    app.claim_decoded_sink(Source::YouTube);
    app.release_decoded_sink_claim();

    assert!(!app.active_decoded_source());
    assert_eq!(app.playback_owner(), PlaybackOwner::Spotify);
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn a_spotify_queue_slot_shadows_the_claim() {
    use crate::infra::queue::QueueNowPlaying;
    let mut app = make_app_simple();
    app.claim_decoded_sink(Source::YouTube);
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    assert!(!app.active_decoded_source());
    assert_eq!(app.playback_owner(), PlaybackOwner::Queue);
  }

  #[test]
  fn a_decoded_owner_drives_neither_native_predicate() {
    let mut app = make_app_simple();
    assert!(app.native_should_drive());
    assert!(app.native_context_should_drive());

    #[cfg(feature = "streaming")]
    {
      app.queue_now = Some(crate::infra::queue::QueueNowPlaying::Spotify {
        track: queue_track(Some("spotify:track:queued"), "Queued"),
      });
      assert!(app.native_should_drive(), "librespot plays the slot");
      assert!(!app.native_context_should_drive());
      app.queue_now = None;
    }

    app.claim_decoded_sink(Source::Qobuz);
    assert!(!app.native_should_drive());
    assert!(!app.native_context_should_drive());
  }
}
