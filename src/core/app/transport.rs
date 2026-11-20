use super::*;

#[cfg(feature = "qobuz")]
#[derive(Clone, Copy)]
enum QobuzTransportCommand {
  Play,
  Pause,
  Toggle,
}

/// Decide the Qobuz pause target, independently of the playback snapshot.
///
/// Each cell describes pause intent / sink state. `P` requests false and resumes;
/// `S` requests true and pauses. `L` flips the live sink's pause state, then saves
/// and applies it. `R` flips the staging record's pause state (live sink if no
/// record exists), then saves and applies it. `-` delegates to the current owner
/// without changing Qobuz intent or issuing a Qobuz sink command.
///
/// | Session | Play | Pause | TogglePlayback | StartPlayback(None, None, None) | PausePlayback |
/// |---|---|---|---|---|---|
/// | No session | - | - | - | - | - |
/// | Pending fetch (advancing, no tempfile) | P | S | L | P | S |
/// | Staging (advancing, tempfile) | P | S | R | P | S |
/// | Playing (not advancing) | P | S | S | P | S |
/// | Paused (not advancing) | P | S | P | P | S |
/// | Owned by another source or queue | - | - | - | - | - |
///
/// `set_paused` preserves any saved seek position. Pending fetch creates a
/// record if needed; staging has one before the blocking decoder starts. A
/// live session updates an existing recovery record but does not create one.
/// Staging can temporarily pause the sink again; its commit applies the latest
/// saved intent. Apple Music, a queue slot, Local and Subsonic take precedence
/// over Qobuz. Browse scope does not affect playback ownership.
#[cfg(feature = "qobuz")]
fn qobuz_transport_paused(
  command: QobuzTransportCommand,
  owns_session: bool,
  advancing: bool,
  has_tempfile: bool,
  saved_paused: Option<bool>,
  player_paused: bool,
) -> Option<bool> {
  if !owns_session {
    return None;
  }
  Some(match command {
    QobuzTransportCommand::Play => false,
    QobuzTransportCommand::Pause => true,
    QobuzTransportCommand::Toggle if advancing && has_tempfile => {
      !saved_paused.unwrap_or(player_paused)
    }
    QobuzTransportCommand::Toggle => !player_paused,
  })
}

impl App {
  pub(crate) fn devices(&self) -> Option<&DevicePayload> {
    self.devices.as_ref()
  }

  pub(crate) fn set_devices(&mut self, payload: DevicePayload) {
    self.devices = Some(payload);
    self.display_revisions.bump(DisplayDomain::Devices);
  }

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
    #[cfg(feature = "soloist")]
    if let Some(link) = &self.soloist_link {
      link.pause();
    }
    if let Some(ctx) = &mut self.current_playback_context {
      ctx.is_playing = false;
    }
    self.native_is_playing = Some(false);
  }

  /// Resume native streaming playback with the bookkeeping the resume branch
  /// of [`toggle_playback`](Self::toggle_playback) does: refuse while the
  /// unavailable-track guard holds playback, arm the play intent, resume the
  /// player, and flip the UI playing state without waiting for librespot's
  /// Playing event.
  #[cfg(feature = "streaming")]
  pub fn resume_native_playback(&mut self) {
    if self.refuse_resume_after_unavailable_halt() {
      return;
    }
    // librespot has nothing to resume, so ask Spotify for the last playback,
    // like the TUI's play/pause.
    if self.native_has_nothing_loaded() {
      self.dispatch(IoEvent::StartPlayback(None, None, None));
      return;
    }
    self.arm_native_play_intent();
    if let Some(player) = &self.streaming_player {
      player.play();
    }
    if let Some(ctx) = &mut self.current_playback_context {
      ctx.is_playing = true;
    }
    self.native_is_playing = Some(true);
  }

  /// No track is loaded for the native player, e.g. right after startup.
  #[cfg(feature = "streaming")]
  fn native_has_nothing_loaded(&self) -> bool {
    self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.item.as_ref())
      .is_none()
  }

  /// Hand the sink to a decoded source: park librespot when it owned the
  /// sink, else only pause it.
  #[cfg(feature = "streaming")]
  pub(crate) fn release_native_for_decoded(&mut self) {
    // Before the pause: the device check falls back on the play state.
    let owned_sink = self.queue_now_is_spotify()
      || self.is_native_streaming_active_for_playback()
      || self
        .streaming_player
        .as_ref()
        .is_some_and(|player| !player.is_available());
    self.pause_native_playback();
    if !owned_sink {
      return;
    }
    // A bare resume after the park restores from the snapshot.
    if self.native_playback_recovery.is_none() {
      let position_ms = u32::try_from(self.song_progress_ms).unwrap_or(u32::MAX);
      self.prepare_native_playback_recovery(position_ms, false);
    }
    self.clear_native_shuffle_session();
    self.native_track_info = None;
    self.last_track_id = None;
    self.native_playback_origin = None;
    self.native_activation_pending = false;
    self.last_device_activation = None;
    self.seek_ms = None;
    self.park_native_backend();
  }

  /// Apply an absolute command while Qobuz owns the session, including fetches.
  pub(crate) fn set_qobuz_playing(&mut self, playing: bool) -> bool {
    #[cfg(feature = "qobuz")]
    {
      self.qobuz_transport(if playing {
        QobuzTransportCommand::Play
      } else {
        QobuzTransportCommand::Pause
      })
    }
    #[cfg(not(feature = "qobuz"))]
    {
      let _ = playing;
      false
    }
  }

  #[cfg(feature = "qobuz")]
  fn qobuz_transport(&mut self, command: QobuzTransportCommand) -> bool {
    let owns_session = !self.apple_music_owns_playback() && !self.queue_owns_playback();
    #[cfg(feature = "local-files")]
    let owns_session = owns_session && self.local_playback.is_none();
    #[cfg(feature = "subsonic")]
    let owns_session = owns_session && self.subsonic_playback.is_none();
    let Some(qobuz) = self.qobuz_playback.as_mut() else {
      return false;
    };
    let Some(paused) = qobuz_transport_paused(
      command,
      owns_session,
      qobuz.advancing,
      qobuz.tempfile.is_some(),
      qobuz.resume_at.map(|resume| resume.paused),
      qobuz.player.is_paused(),
    ) else {
      return false;
    };
    qobuz.set_paused(paused);
    true
  }

  pub fn toggle_playback(&mut self) {
    if self.relay_party_toggle() {
      return;
    }
    if self.apple_music_owns_playback() {
      self.toggle_apple_music();
      return;
    }
    // The native queue slot owns the sink: toggle its player directly (covers the
    // idle-app case where no per-source context is set).
    #[cfg(feature = "audio-decode-queue")]
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
      #[cfg(feature = "streaming")]
      if !is_playing && self.refuse_resume_after_unavailable_halt() {
        return;
      }
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

    #[cfg(feature = "qobuz")]
    if self.qobuz_transport(QobuzTransportCommand::Toggle) {
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

    // A decoded start in flight, or a source that lost its sink: nothing to
    // toggle, and the paused librespot underneath is not the player the user
    // means.
    if self.decoded_sink_claimed() {
      self.set_status_message(self.strings().status.app.nothing_playing, 4);
      return;
    }

    // A pulled-back playback plays before Spotify confirms this device (#693).
    #[cfg(feature = "streaming")]
    if self.native_handed_off() && self.native_is_playing == Some(true) {
      self.pause_native_playback();
      return;
    }

    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      if self.native_has_nothing_loaded() {
        self.dispatch(IoEvent::StartPlayback(None, None, None));
        return;
      }

      if self.streaming_player.is_some() {
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
          self.resume_native_playback();
        }
        return;
      }
    }

    // Fallback to API-based playback control for external devices. A parked
    // backend plays nothing, whatever a stale poll says.
    let is_playing = if self.native_parked_here() {
      false
    } else if self.is_streaming_active {
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
      self.dispatch_spotify_fallback(IoEvent::PausePlayback);
    } else {
      // When no offset or uris are passed, spotify will resume current playback
      self.dispatch_spotify_fallback(IoEvent::StartPlayback(None, None, None));
    }
  }

  pub fn previous_track(&mut self) {
    if self.relay_party_transport(PlaybackAction::PrevTrack) {
      return;
    }
    if self.apple_music_owns_playback() {
      if self.apple_music_refuses_while_switching() {
        return;
      }
      // Past the first 3s Music's own previous restarts the track, which
      // works; earlier, step back in the list the track was started from.
      let restart = self.apple_music_position_ms() > 3_000 && !self.apple_music_just_stepped();
      if restart || !self.step_apple_music(false) {
        self.dispatch(IoEvent::PreviousTrack);
      }
      return;
    }
    info!("playing previous track or restarting current track");
    // A skip drops a waiting start, except one that waits for the rebuild of
    // a parked backend: the skip is refused there.
    #[cfg(feature = "streaming")]
    if !self.native_parked_here() {
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
          self.record_native_seek(0);
          return;
        }
      }

      // Fallback for external devices
      self.dispatch_spotify_fallback(IoEvent::Seek(0));
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
          player.resume_after_skip(false);
          return;
        }
      }

      // Fallback for external devices
      self.dispatch_spotify_fallback(IoEvent::PreviousTrack);
    }
  }

  pub fn force_previous_track(&mut self) {
    if self.relay_party_transport(PlaybackAction::ForcePrevTrack) {
      return;
    }
    if self.apple_music_owns_playback() {
      if self.apple_music_refuses_while_switching() {
        return;
      }
      if !self.step_apple_music(false) {
        self.dispatch(IoEvent::ForcePreviousTrack);
      }
      return;
    }
    info!("force skipping to previous track");
    // A skip drops a waiting start, except one that waits for the rebuild of
    // a parked backend: the skip is refused there.
    #[cfg(feature = "streaming")]
    if !self.native_parked_here() {
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
        player.resume_after_skip(true);
        return;
      }
    }

    self.song_progress_ms = 0;
    self.dispatch_spotify_fallback(IoEvent::ForcePreviousTrack);
  }

  pub fn next_track(&mut self) {
    if self.relay_party_transport(PlaybackAction::NextTrack) {
      return;
    }
    if self.apple_music_owns_playback() {
      if self.apple_music_refuses_while_switching() {
        return;
      }
      // A track started by itself has no queue in Music, so its own next
      // does nothing: start the next track of the list spotatui kept.
      if !self.step_apple_music(true) {
        self.dispatch(IoEvent::NextTrack);
      }
      return;
    }
    info!("skipping to next track");
    // A skip drops a waiting start, except one that waits for the rebuild of
    // a parked backend: the skip is refused there, unless queued items take
    // the sink (their hand-over drops the start).
    #[cfg(feature = "streaming")]
    if !self.native_parked_here() {
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
    // A native-Spotify context, live or parked, with items waiting in the
    // queue: suspend it (skip semantics) and hand the sink to the queue instead
    // of Spirc-advancing the context. (`queue_owns_playback` is already handled
    // above, so here the context, not a queued track, is playing.)
    #[cfg(feature = "streaming")]
    if (self.is_native_streaming_active_for_playback() || self.native_parked_here())
      && !self.native_queue.is_empty()
    {
      self.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::ManualSkip);
      self.pause_native_playback();
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return;
    }
    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      // A manual Next advances the shuffle session even under repeat-one.
      self.mark_native_shuffle_manual_skip(true);
      if let Some(ref player) = self.streaming_player {
        player.activate();
        player.next();
        // Reset progress immediately for UI feedback
        self.song_progress_ms = 0;
        player.resume_after_skip(false);
        return;
      }
    }

    // Fallback for external devices
    self.dispatch_spotify_fallback(IoEvent::NextTrack);
  }

  /// Start playback of an explicit list of playable URIs, optionally from an
  /// offset into that list. The single place the shared action vocabulary
  /// builds a URI-list `StartPlayback`; ownership routing happens in the
  /// pump's source routers, exactly as for the equivalent keybinding.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub(crate) fn start_playback_uris(&mut self, uris: Vec<String>, offset: Option<usize>) {
    // No URIs: nothing to start, and an empty start would only tear the current player down.
    if uris.is_empty() {
      return;
    }
    // One Music track at a time: there is no cross-source queue for Apple Music.
    if uris.iter().all(|uri| uri.starts_with("applemusic:")) {
      let offset = offset.unwrap_or(0);
      if let Some(uri) = uris.get(offset) {
        let occurrence = uris[..offset].iter().filter(|u| *u == uri).count();
        self.play_apple_music_track_at(uri.clone(), occurrence);
      }
      return;
    }
    self.dispatch(IoEvent::StartPlayback(None, Some(uris), offset));
  }

  /// Start playback of a Spotify context URI, optionally from a 0-based track
  /// offset. The context twin of [`Self::start_playback_uris`].
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub(crate) fn start_playback_context(&mut self, context_uri: String, offset: Option<usize>) {
    self.dispatch(IoEvent::StartPlayback(Some(context_uri), None, offset));
  }

  /// Start playback of one track URI as the first track inside a Spotify
  /// context URI. The third start shape beside the two twins above: the
  /// network layer deliberately does not trim the uri list when a context is
  /// present, which is what keeps the selected track first under shuffle.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub(crate) fn start_playback_track_in_context(&mut self, context_uri: String, track_uri: String) {
    self.dispatch(IoEvent::StartPlayback(
      Some(context_uri),
      Some(vec![track_uri]),
      Some(0),
    ));
  }

  /// Transfer Spotify playback to a Connect device, refused while another
  /// source owns the sink.
  pub(crate) fn transfer_playback_to_device(&mut self, device_id: String, persist: bool) {
    if self.apple_music_owns_playback() || self.active_decoded_source() {
      self.set_status_message(self.strings().status.app.another_source_playing, 4);
      return;
    }
    self.dispatch(IoEvent::TransferPlaybackToDevice(device_id, persist));
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
                self.handle_error_with_message(
                  anyhow!("failed to set clipboard content: {}", e),
                  self
                    .strings()
                    .status
                    .app
                    .clipboard_set_failed(&e.to_string()),
                );
              }
            }
            _ => {
              self.handle_error_with_message(
                anyhow!("Track has no ID"),
                self.strings().status.app.track_no_id.to_string(),
              );
            }
          }
        }
        PlayableItem::Episode(episode) => {
          let episode_id = episode.id.id().to_string();
          if let Err(e) =
            clipboard.set_text(format!("https://open.spotify.com/episode/{}", episode_id))
          {
            self.handle_error_with_message(
              anyhow!("failed to set clipboard content: {}", e),
              self
                .strings()
                .status
                .app
                .clipboard_set_failed(&e.to_string()),
            );
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
                self.handle_error_with_message(
                  anyhow!("failed to set clipboard content: {}", e),
                  self
                    .strings()
                    .status
                    .app
                    .clipboard_set_failed(&e.to_string()),
                );
              }
            }
            _ => {
              self.handle_error_with_message(
                anyhow!("Album has no ID"),
                self.strings().status.app.album_no_id.to_string(),
              );
            }
          }
        }
        PlayableItem::Episode(episode) => {
          let show_id = episode.show.id.id().to_string();
          if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/show/{}", show_id)) {
            self.handle_error_with_message(
              anyhow!("failed to set clipboard content: {}", e),
              self
                .strings()
                .status
                .app
                .clipboard_set_failed(&e.to_string()),
            );
          }
        }
        _ => {}
      }
    }
  }
}

#[cfg(all(test, feature = "qobuz"))]
mod qobuz_tests {
  use super::*;
  use crate::core::action::Action;
  use crate::core::test_helpers::queued_track;
  use crate::infra::audio::LocalPlayer;
  use crate::infra::qobuz::{QobuzPlaybackState, ResumePoint};

  fn app_with_qobuz() -> App {
    let mut app = App::default();
    app.qobuz_playback = Some(QobuzPlaybackState {
      player: Arc::new(LocalPlayer::without_device()),
      tracks: vec![queued_track("qobuz:track:1", "Staging")],
      index: 0,
      advancing: true,
      tempfile: Some(tempfile::NamedTempFile::new().unwrap()),
      complete: Default::default(),
      quality: None,
      shuffle_backup: None,
      fetch_id: 1,
      resume_at: Some(ResumePoint {
        position_ms: 1234,
        paused: false,
      }),
      fetch: None,
    });
    app
  }

  #[derive(Clone, Copy, Debug)]
  enum TransportEntry {
    Play,
    Pause,
    TogglePlayback,
    StartPlayback,
    PausePlayback,
  }

  impl TransportEntry {
    async fn send(self, app: &Arc<tokio::sync::Mutex<App>>) -> Option<bool> {
      use crate::infra::qobuz::dispatch::route_qobuz_event;

      let action = match self {
        Self::Play => Action::Play,
        Self::Pause => Action::Pause,
        Self::TogglePlayback => Action::TogglePlayback,
        Self::StartPlayback => {
          return Some(route_qobuz_event(app, &IoEvent::StartPlayback(None, None, None)).await);
        }
        Self::PausePlayback => {
          return Some(route_qobuz_event(app, &IoEvent::PausePlayback).await);
        }
      };
      app.lock().await.apply(action);
      None
    }
  }

  #[tokio::test]
  async fn qobuz_actions_and_media_commands_follow_the_full_transport_matrix() {
    use crate::infra::queue::DecodedQueuePlayback;

    #[derive(Clone, Copy, Debug)]
    enum Owner {
      Qobuz,
      NoSession,
      Queue,
      AppleMusic,
      #[cfg(feature = "local-files")]
      Local,
      #[cfg(feature = "subsonic")]
      Subsonic,
    }

    let owners = [
      Owner::Qobuz,
      Owner::NoSession,
      Owner::Queue,
      Owner::AppleMusic,
      #[cfg(feature = "local-files")]
      Owner::Local,
      #[cfg(feature = "subsonic")]
      Owner::Subsonic,
    ];
    let mut checks = 0;
    for owner in owners {
      for (phase, advancing, has_tempfile) in [
        ("pending fetch", true, false),
        ("staging", true, true),
        ("live", false, true),
      ] {
        for live_paused in [false, true] {
          for saved_paused in [None, Some(false), Some(true)] {
            let controls_qobuz = matches!(owner, Owner::Qobuz);
            let toggle_paused = if controls_qobuz && advancing && has_tempfile {
              !saved_paused.unwrap_or(live_paused)
            } else {
              !live_paused
            };
            for (entry, target_paused) in [
              (TransportEntry::Play, false),
              (TransportEntry::Pause, true),
              (TransportEntry::TogglePlayback, toggle_paused),
              (TransportEntry::StartPlayback, false),
              (TransportEntry::PausePlayback, true),
            ] {
              let mut app = app_with_qobuz();
              // Browse scope must not choose the transport owner.
              app.active_source = Source::Spotify;
              let session = app.qobuz_playback.as_mut().unwrap();
              session.advancing = advancing;
              if !has_tempfile {
                session.tempfile = None;
              }
              let original_resume = saved_paused.map(|paused| ResumePoint {
                position_ms: 1234,
                paused,
              });
              session.resume_at = original_resume;
              let qobuz_player = Arc::clone(&session.player);
              let owner_player = Arc::new(LocalPlayer::without_device());
              for player in [&qobuz_player, &owner_player] {
                if live_paused {
                  player.pause();
                } else {
                  player.resume();
                }
              }
              match owner {
                Owner::Qobuz => {}
                Owner::NoSession => app.qobuz_playback = None,
                Owner::Queue => {
                  app.queue_now = Some(QueueNowPlaying::Decoded(DecodedQueuePlayback {
                    player: Arc::clone(&owner_player),
                    track: queued_track("qobuz:track:2", "Queued"),
                    advancing: false,
                    fetch_id: 2,
                    tempfile: None,
                    abort_handle: None,
                    quality: None,
                  }));
                }
                Owner::AppleMusic => {
                  app.apple_music.claimed = true;
                  app.apple_music.desired_playing = !live_paused;
                }
                #[cfg(feature = "local-files")]
                Owner::Local => {
                  app.local_playback = Some(crate::infra::local::LocalPlaybackState {
                    player: Arc::clone(&owner_player),
                    queue: vec!["file:///test.flac".to_string()],
                    index: 0,
                    name: "Local".to_string(),
                    generated_name: None,
                    artists: String::new(),
                    album: String::new(),
                    duration_ms: 1000,
                    advancing: false,
                    shuffle_backup: None,
                    failed_since_played: Default::default(),
                    resume_at: None,
                  });
                }
                #[cfg(feature = "subsonic")]
                Owner::Subsonic => {
                  use crate::infra::subsonic::{SubsonicPlaybackState, SubsonicSource};

                  app.subsonic_playback = Some(SubsonicPlaybackState {
                    player: Arc::clone(&owner_player),
                    source: Arc::new(SubsonicSource::new("http://127.0.0.1:1", "test", "test")),
                    tracks: vec![queued_track("subsonic:track:1", "Subsonic")],
                    index: 0,
                    advancing: false,
                    tempfile: tempfile::NamedTempFile::new().unwrap(),
                    shuffle_backup: None,
                    resume_at: None,
                  });
                }
              }
              let app = Arc::new(tokio::sync::Mutex::new(app));
              // Check every call: repeated absolute commands are idempotent,
              // while successive toggles must alternate.
              for paused in [
                target_paused,
                if matches!(entry, TransportEntry::TogglePlayback) {
                  !target_paused
                } else {
                  target_paused
                },
              ] {
                let routed = entry.send(&app).await;
                let context = format!("{owner:?}/{phase}/{live_paused}/{saved_paused:?}/{entry:?}");
                if let Some(handled) = routed {
                  assert_eq!(handled, controls_qobuz, "{context}");
                }
                let guard = app.lock().await;
                assert_eq!(
                  guard.qobuz_playback.is_some(),
                  !matches!(owner, Owner::NoSession),
                  "{context}"
                );
                if let Some(session) = &guard.qobuz_playback {
                  let expected_resume =
                    if controls_qobuz && (saved_paused.is_some() || (advancing && !has_tempfile)) {
                      Some(ResumePoint {
                        position_ms: original_resume.map_or(0, |r| r.position_ms),
                        paused,
                      })
                    } else {
                      original_resume
                    };
                  assert_eq!(session.resume_at, expected_resume, "{context}");
                  assert_eq!(session.advancing, advancing, "{context}");
                  assert_eq!(session.tempfile.is_some(), has_tempfile, "{context}");
                  assert_eq!(session.fetch_id, 1, "{context}");
                }
                assert_eq!(
                  qobuz_player.is_paused(),
                  if controls_qobuz { paused } else { live_paused },
                  "{context}"
                );
                if !matches!(owner, Owner::Qobuz | Owner::NoSession) {
                  // A direct call to the Qobuz router delegates untouched;
                  // Actions continue through the current owner's transport.
                  let owner_paused = if routed.is_none() {
                    paused
                  } else {
                    live_paused
                  };
                  if matches!(owner, Owner::AppleMusic) {
                    assert_eq!(guard.apple_music_is_playing(), !owner_paused, "{context}");
                  } else {
                    assert_eq!(owner_player.is_paused(), owner_paused, "{context}");
                  }
                }
                checks += 1;
              }
            }
          }
        }
      }
    }
    assert_eq!(checks, owners.len() * 3 * 2 * 3 * 5 * 2);
    eprintln!("Qobuz transport matrix: {checks} state/command checks passed");
  }

  #[test]
  fn a_play_action_during_qobuz_staging_preserves_the_saved_play_intent() {
    let mut app = app_with_qobuz();
    app.qobuz_playback.as_ref().unwrap().player.pause();

    for _ in 0..2 {
      app.apply(Action::Play);
      assert_eq!(
        app.qobuz_playback.as_ref().unwrap().resume_at,
        Some(ResumePoint {
          position_ms: 1234,
          paused: false,
        })
      );
    }
  }

  #[test]
  fn a_play_action_during_qobuz_staging_overrides_the_saved_pause_intent() {
    let mut app = app_with_qobuz();
    let session = app.qobuz_playback.as_mut().unwrap();
    session.resume_at.as_mut().unwrap().paused = true;
    session.player.pause();

    for _ in 0..2 {
      app.apply(Action::Play);
      assert_eq!(
        app.qobuz_playback.as_ref().unwrap().resume_at,
        Some(ResumePoint {
          position_ms: 1234,
          paused: false,
        })
      );
    }
  }

  #[test]
  fn a_pause_action_during_qobuz_staging_overrides_the_saved_play_intent() {
    let mut app = app_with_qobuz();
    app.qobuz_playback.as_ref().unwrap().player.pause();

    for _ in 0..2 {
      app.apply(Action::Pause);
      assert_eq!(
        app.qobuz_playback.as_ref().unwrap().resume_at,
        Some(ResumePoint {
          position_ms: 1234,
          paused: true,
        })
      );
    }
  }

  #[test]
  fn a_toggle_action_during_qobuz_staging_flips_the_saved_intent() {
    let mut app = app_with_qobuz();
    app.qobuz_playback.as_ref().unwrap().player.pause();

    for paused in [true, false, true] {
      app.apply(Action::TogglePlayback);
      assert_eq!(
        app.qobuz_playback.as_ref().unwrap().resume_at,
        Some(ResumePoint {
          position_ms: 1234,
          paused,
        })
      );
    }
  }

  #[test]
  fn absolute_actions_during_a_qobuz_fetch_set_the_saved_intent_and_sink() {
    for paused in [false, true] {
      for saved_pause in [false, true] {
        for play in [false, true] {
          let mut app = app_with_qobuz();
          let session = app.qobuz_playback.as_mut().unwrap();
          session.tempfile = None;
          session.set_paused(paused);
          session.resume_at.as_mut().unwrap().paused = saved_pause;

          app.apply(if play { Action::Play } else { Action::Pause });

          let session = app.qobuz_playback.as_ref().unwrap();
          assert_eq!(session.player.is_paused(), !play);
          assert_eq!(
            session.resume_at,
            Some(ResumePoint {
              position_ms: 1234,
              paused: !play,
            })
          );
        }
      }
    }
  }

  #[test]
  fn absolute_actions_leave_a_staged_qobuz_context_untouched_under_a_queue_slot() {
    use crate::infra::queue::DecodedQueuePlayback;

    for paused in [false, true] {
      let mut app = app_with_qobuz();
      let session = app.qobuz_playback.as_ref().unwrap();
      let resume = session.resume_at;
      session.player.pause();
      if paused {
        session.player.resume();
      }
      app.queue_now = Some(QueueNowPlaying::Decoded(DecodedQueuePlayback {
        player: Arc::clone(&session.player),
        track: queued_track("qobuz:track:2", "Queued"),
        advancing: false,
        fetch_id: 2,
        tempfile: None,
        abort_handle: None,
        quality: None,
      }));

      app.apply(if paused { Action::Pause } else { Action::Play });

      let session = app.qobuz_playback.as_ref().unwrap();
      assert_eq!(session.resume_at, resume);
      assert_eq!(session.player.is_paused(), paused);
    }
  }

  #[tokio::test]
  async fn routed_play_and_pause_during_qobuz_staging_set_the_saved_intent() {
    use crate::infra::qobuz::dispatch::route_qobuz_event;

    let app = Arc::new(tokio::sync::Mutex::new(app_with_qobuz()));
    for paused in [false, false, true, true, false] {
      let event = if paused {
        IoEvent::PausePlayback
      } else {
        IoEvent::StartPlayback(None, None, None)
      };
      assert!(route_qobuz_event(&app, &event).await);
      let guard = app.lock().await;
      assert_eq!(
        guard.qobuz_playback.as_ref().unwrap().resume_at,
        Some(ResumePoint {
          position_ms: 1234,
          paused,
        })
      );
    }
  }

  #[test]
  fn a_toggle_during_qobuz_staging_flips_the_saved_intent() {
    let mut app = app_with_qobuz();
    app.qobuz_playback.as_ref().unwrap().player.pause();

    app.toggle_playback();
    let session = app.qobuz_playback.as_ref().unwrap();
    assert_eq!(
      session.resume_at,
      Some(ResumePoint {
        position_ms: 1234,
        paused: true,
      })
    );
    assert!(session.player.is_paused());

    app.toggle_playback();
    let session = app.qobuz_playback.as_ref().unwrap();
    assert!(!session.resume_at.unwrap().paused);
    assert!(!session.player.is_paused());

    // Once staging commits, the live player is authoritative again.
    let session = app.qobuz_playback.as_mut().unwrap();
    session.advancing = false;
    session.resume_at = None;
    app.toggle_playback();
    let session = app.qobuz_playback.as_ref().unwrap();
    assert!(session.player.is_paused());
    assert_eq!(session.resume_at, None);
  }

  #[test]
  fn a_toggle_during_a_qobuz_fetch_still_reads_the_live_player() {
    let mut app = app_with_qobuz();
    let session = app.qobuz_playback.as_mut().unwrap();
    session.tempfile = None;
    session.player.pause();

    app.toggle_playback();
    let session = app.qobuz_playback.as_ref().unwrap();
    assert!(!session.resume_at.unwrap().paused);
    assert!(!session.player.is_paused());
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  #[cfg(feature = "streaming")]
  #[test]
  fn space_pauses_a_pulled_back_playback_before_spotify_confirms_the_device() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.mark_native_handed_off();
    app.native_is_playing = Some(true);

    app.toggle_playback();

    assert_eq!(app.native_is_playing, Some(false));
    assert!(rx.try_recv().is_err());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn space_after_a_handoff_still_starts_when_nothing_plays_here() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.mark_native_handed_off();
    app.native_is_playing = Some(false);

    app.toggle_playback();

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
  }

  /// The media-key fallback for another device (#797) leans on this: the idle
  /// local player says "not playing", but the toggle reads the other device and
  /// pauses it instead of starting it.
  #[test]
  fn toggle_pauses_another_device_that_plays_while_the_local_player_is_idle() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.spotify_connected = true;
    app.native_is_playing = Some(false);
    app.current_playback_context = Some(make_external_context());
    assert_eq!(app.playback_owner(), PlaybackOwner::Spotify);

    app.toggle_playback();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
  }

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

  /// A session another client moved to spotatui has no snapshot: the release
  /// makes one, so a bare resume after the park has something to restore.
  #[test]
  fn releasing_the_native_sink_keeps_a_snapshot_for_the_resume() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let (recovery_tx, mut recovery_rx) = tokio::sync::mpsc::unbounded_channel();
    app.streaming_recovery_tx = Some(recovery_tx);
    // A Spotify slot makes librespot own the sink with no player to construct.
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });
    app.current_playback_context = Some(playing_track_context(full_track(
      "0000000000000000000001",
      "T",
    )));
    app.song_progress_ms = 42_000;
    app.native_track_info = Some(NativeTrackInfo::default());
    let generation = app.native_shuffle_generation;

    app.release_native_for_decoded();

    assert!(app
      .native_playback_recovery
      .as_ref()
      .is_some_and(|snapshot| snapshot.current_track_uri.as_deref()
        == Some("spotify:track:0000000000000000000001")
        && snapshot.position_ms == 42_000
        && !snapshot.desired_playing));
    assert!(app.native_track_info.is_none());
    assert_ne!(app.native_shuffle_generation, generation);
    assert!(recovery_rx.try_recv().is_err());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn next_on_the_parked_device_sends_nothing_to_the_web_api() {
    let (mut app, rx, _recovery_rx) = parked_native_app();

    app.next_track();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some("Press play to resume Spotify"));
  }

  #[test]
  fn next_on_the_parked_device_hands_the_waiting_queue_the_sink() {
    use crate::core::queue::SuspendedContext;
    let (mut app, rx, _recovery_rx) = parked_native_app();
    if let Some(snapshot) = app.native_playback_recovery.as_mut() {
      snapshot.current_track_uri = Some("spotify:track:0000000000000000000001".to_string());
    }
    app
      .native_queue
      .push(queue_track(Some("spotify:track:queued"), "Queued"));

    app.next_track();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::AdvanceNativeQueue)));
    // No mirror queue while parked: the interrupted track is the resume target.
    assert!(matches!(
      app.queue_suspended,
      Some(SuspendedContext::Spotify {
        context_uri: Some(ref uri),
        resume_track_uri: Some(ref track),
      }) if uri == "spotify:playlist:parked"
        && track == "spotify:track:0000000000000000000001"
    ));
  }

  #[test]
  fn a_refused_skip_keeps_the_start_waiting_for_the_parked_rebuild() {
    let (mut app, rx, _recovery_rx) = parked_native_app();
    app.native_backend_pending = true;
    app.park_start_playback(Some("spotify:playlist:new".to_string()), None, None);

    app.next_track();

    assert!(app.pending_start_playback.is_some());
    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message(), Some("Reconnecting native streaming…"));
  }

  #[test]
  fn a_stale_playing_answer_for_the_parked_device_still_resumes_on_play() {
    let (mut app, rx, mut recovery_rx) = parked_native_app();
    let mut context = playing_track_context(full_track("0000000000000000000001", "Parked"));
    context.device.id = Some("spotatui".to_string());
    app.current_playback_context = Some(context);
    app.is_streaming_active = false;

    app.toggle_playback();

    assert!(rx.try_recv().is_err());
    assert!(recovery_rx.try_recv().is_ok());
  }

  #[test]
  fn a_release_with_no_native_backend_only_pauses() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_track_info = Some(NativeTrackInfo::default());
    app.park_start_playback(Some("spotify:playlist:p".to_string()), None, None);

    app.release_native_for_decoded();

    assert!(!app.native_backend_parked());
    assert!(app.native_track_info.is_some());
    assert!(app.pending_start_playback.is_none());
    assert!(rx.try_recv().is_err());
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

  #[cfg(feature = "youtube")]
  #[test]
  fn toggle_under_a_claim_with_no_session_dispatches_nothing() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.claim_decoded_sink(Source::YouTube);

    app.toggle_playback();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.status_message.as_deref(), Some(NOTHING_PLAYING_STATUS));
  }

  /// MPRIS Pause/Play call these directly (#985): the playbar and the MPRIS
  /// status must follow at once, not wait for a librespot event.
  #[test]
  fn pause_and_resume_flip_the_play_state_without_a_player_event() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(playing_track_context(full_track(
      "0000000000000000000001",
      "T",
    )));
    app.native_is_playing = Some(true);

    app.pause_native_playback();
    assert_eq!(app.native_is_playing, Some(false));

    app.resume_native_playback();
    assert_eq!(app.native_is_playing, Some(true));
    assert!(rx.try_recv().is_err());
  }

  /// MPRIS Play right after startup: librespot has no track, so the resume
  /// asks Spotify for the last playback instead of showing Playing in silence.
  #[test]
  fn resume_with_nothing_loaded_asks_spotify_for_the_last_playback() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.resume_native_playback();

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
    assert_ne!(app.native_is_playing, Some(true));
  }

  #[test]
  fn resume_is_refused_while_the_unavailable_track_guard_holds() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.halt_native_after_unavailable();

    app.resume_native_playback();

    assert_eq!(app.native_is_playing, Some(false));
    assert!(app.status_message.is_some());
  }
}

#[cfg(test)]
mod devices_tests {
  use super::*;

  #[test]
  fn caching_a_device_list_bumps_the_devices_revision() {
    let mut app = App::default();
    let before = app.display_revisions().get(DisplayDomain::Devices);

    app.set_devices(DevicePayload { devices: vec![] });

    assert_eq!(app.devices().map(|p| p.devices.len()), Some(0));
    assert_eq!(
      app.display_revisions().get(DisplayDomain::Devices),
      before + 1
    );
  }
}
