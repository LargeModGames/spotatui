use super::*;

impl App {
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub(crate) fn install_macos_media_manager(
    &mut self,
    manager: Option<Arc<crate::infra::macos_media::MacMediaManager>>,
  ) {
    self.macos_media_manager = manager;
  }

  pub(crate) fn apple_music_state(&self) -> &crate::infra::apple_music::RemoteState {
    &self.apple_music
  }

  pub(crate) fn apple_music_owns_playback(&self) -> bool {
    self.apple_music.claimed
  }

  pub(crate) fn apple_music_position_ms(&self) -> u32 {
    let Some(snapshot) = &self.apple_music.snapshot else {
      return 0;
    };
    let elapsed = self
      .apple_music
      .observed_at
      .map_or(0, |at| at.elapsed().as_millis());
    let extra = if snapshot.playing {
      elapsed.min(10_000)
    } else {
      0
    };
    (u128::from(snapshot.position_ms) + extra)
      .min(snapshot.track.as_ref().map_or(0, |t| t.duration_ms) as u128) as u32
  }

  pub(crate) fn apple_music_is_playing(&self) -> bool {
    self.apple_music.desired_playing
  }

  pub(crate) fn apple_music_volume(&self) -> u8 {
    self
      .apple_music
      .pending_volume
      .or(self.apple_music.volume_in_flight)
      .or_else(|| self.apple_music.snapshot.as_ref().map(|s| s.volume))
      .unwrap_or(self.runtime_state.volume_percent)
  }

  pub(crate) fn toggle_apple_music(&mut self) {
    if self.apple_music.switching {
      self.set_status_message("Waiting for Music to pause before switching source", 4);
      return;
    }
    let playing = self.apple_music_is_playing();
    self.apple_music.desired_playing = !playing;
    self.apple_music.intent_revision = self.apple_music.intent_revision.wrapping_add(1);
    if let Some(snapshot) = &mut self.apple_music.snapshot {
      snapshot.playing = !playing;
    }
    self.dispatch(if playing {
      IoEvent::PausePlayback
    } else {
      IoEvent::StartPlayback(None, None, None)
    });
  }

  /// Play or pause Music outright, for the gestures that name the state (a
  /// media key's play or pause, a plugin's `Play`): unlike a toggle, the
  /// answer must not depend on a snapshot that may not have arrived yet.
  pub(crate) fn set_apple_music_playing(&mut self, playing: bool) {
    if self.apple_music.switching {
      self.set_status_message("Waiting for Music to pause before switching source", 4);
      return;
    }
    self.apple_music.desired_playing = playing;
    self.apple_music.intent_revision = self.apple_music.intent_revision.wrapping_add(1);
    if let Some(snapshot) = &mut self.apple_music.snapshot {
      snapshot.playing = playing;
    }
    self.dispatch(if playing {
      IoEvent::StartPlayback(None, None, None)
    } else {
      IoEvent::PausePlayback
    });
  }

  /// A skip while handing off would be queued behind the handoff and reach
  /// the next source: refuse it with a message instead.
  pub(crate) fn apple_music_refuses_while_switching(&mut self) -> bool {
    if self.apple_music.switching {
      self.set_status_message("Waiting for Music to pause before switching source", 4);
    }
    self.apple_music.switching
  }

  /// Where the last read track would be now, for telling whether it ended.
  /// Not capped like the displayed position: after a long gap in the reads
  /// (Music busy, the Mac asleep) the track may well have run out.
  fn apple_music_end_position_ms(&self) -> u32 {
    let Some(snapshot) = &self.apple_music.snapshot else {
      return 0;
    };
    if !snapshot.playing {
      return snapshot.position_ms;
    }
    let monotonic = self
      .apple_music
      .observed_at
      .map_or(0, |at| at.elapsed().as_millis());
    let wall = self
      .apple_music
      .read_at
      .and_then(|at| at.elapsed().ok())
      .map_or(0, |gap| gap.as_millis());
    (u128::from(snapshot.position_ms) + monotonic.max(wall)).min(u128::from(u32::MAX)) as u32
  }

  pub(crate) fn set_apple_music_volume(&mut self, value: u8) {
    if self.apple_music_refuses_while_switching() {
      return;
    }
    // Never 0: at 0 Music answers the next start with a dialog in its own
    // window and plays nothing until someone dismisses it. The script raises
    // a level Music stores as 0 (1 on some versions) to the next that sticks.
    if value == 0 {
      self.set_status_message(
        "Apple Music: the volume stops just above 0 (Music refuses to play at 0)",
        4,
      );
    }
    let value = value.clamp(1, 100);
    if value == self.apple_music_volume() {
      return;
    }
    if let Some(snapshot) = &mut self.apple_music.snapshot {
      snapshot.volume = value;
    }
    self.apple_music.pending_volume = Some(value);
    self.flush_apple_music_volume();
  }

  /// A seek or volume change is one helper run: coalesce rapid gestures.
  const APPLE_MUSIC_CONTROL_THROTTLE_MS: u128 = 250;

  /// Keep only the latest volume while a helper is queued or running. The
  /// tick sends it once that helper finishes and the throttle permits it.
  pub(crate) fn flush_apple_music_volume(&mut self) {
    if !self.apple_music.claimed
      || self.apple_music.switching
      || self.apple_music.volume_in_flight.is_some()
      || self
        .apple_music
        .last_volume
        .is_some_and(|at| at.elapsed().as_millis() < Self::APPLE_MUSIC_CONTROL_THROTTLE_MS)
    {
      return;
    }
    if let Some(volume) = self.apple_music.pending_volume.take() {
      self.apple_music.last_volume = Some(Instant::now());
      self.apple_music.volume_in_flight = Some(volume);
      self.dispatch(IoEvent::AppleMusicVolume {
        generation: self.apple_music.generation,
        volume,
      });
    }
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn finish_apple_music_volume(&mut self, generation: u64) {
    if self.apple_music.generation == generation {
      self.apple_music.volume_in_flight = None;
    }
  }

  fn cancel_apple_music_volume(&mut self) {
    self.apple_music.pending_volume = None;
    self.apple_music.volume_in_flight = None;
    self.apple_music.last_volume = None;
  }

  /// Seek Music. The position is applied locally at once, so quick repeated
  /// seeks add up instead of all starting from the last snapshot.
  pub(crate) fn seek_apple_music(&mut self, position_ms: u32) {
    // Handing off: a seek held back by the throttle would reach the next
    // source instead.
    if self.apple_music.switching {
      return;
    }
    let Some(snapshot) = self.apple_music.snapshot.as_mut() else {
      self.dispatch(IoEvent::Seek(position_ms));
      return;
    };
    let duration = snapshot.track.as_ref().map_or(0, |t| t.duration_ms as u32);
    let position_ms = if duration > 0 {
      position_ms.min(duration)
    } else {
      position_ms
    };
    snapshot.position_ms = position_ms;
    self.apple_music.observed_at = Some(Instant::now());
    self.song_progress_ms = position_ms as u128;
    self.seek_ms = None;
    self.pending_source_seek = Some(position_ms);
    self.flush_apple_music_seek();
  }

  /// Send the waiting seek once the throttle allows; the tick calls this too.
  pub(crate) fn flush_apple_music_seek(&mut self) {
    if self.apple_music.switching {
      return;
    }
    let Some(position_ms) = self.pending_source_seek else {
      return;
    };
    if self
      .last_source_seek
      .is_some_and(|t| t.elapsed().as_millis() < Self::APPLE_MUSIC_CONTROL_THROTTLE_MS)
    {
      return;
    }
    self.pending_source_seek = None;
    self.last_source_seek = Some(Instant::now());
    self.dispatch(IoEvent::Seek(position_ms));
  }

  /// Music took a start or a resume and did not begin playing. The usual
  /// reason is a dialog waiting in the Music window, which the terminal
  /// cannot show.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn note_apple_music_did_not_start(&mut self, generation: u64, revision: u64) {
    if self.apple_music.generation != generation {
      return;
    }
    // A pause or resume asked for while this start waited is newer than it:
    // keep that intent, the commands queued behind this one carry it out.
    if self.apple_music.intent_revision == revision {
      self.apple_music.desired_playing = false;
      if let Some(snapshot) = self.apple_music.snapshot.as_mut() {
        snapshot.playing = false;
      }
    }
    self.resync_apple_music_list();
    self.set_error_status_message(
      "Apple Music: Music did not start playing. Check the Music window for a dialog or an unavailable track",
      10,
    );
  }

  /// A transport command was queued for the Music worker.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn note_apple_music_command_queued(&mut self) {
    self.apple_music.pending_commands = self.apple_music.pending_commands.saturating_add(1);
  }

  /// The Music worker finished (or dropped) a queued transport command.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn finish_apple_music_command(&mut self) {
    self.apple_music.pending_commands = self.apple_music.pending_commands.saturating_sub(1);
    self.apple_music.commanded_at = Some(Instant::now());
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn claim_apple_music(&mut self) -> u64 {
    // Claim first: a media key during launch/permission handling must never
    // resume the player we are handing off from.
    // Taking over plays. Already claimed, the intent was set when the start
    // was asked for (`note_apple_music_start`), and a pause pressed while it
    // waited in the pump is newer: keep it.
    if !self.apple_music.claimed {
      self.apple_music.desired_playing = true;
      self.apple_music.intent_revision = self.apple_music.intent_revision.wrapping_add(1);
    }
    self.apple_music.claimed = true;
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    if let Some(manager) = &self.macos_media_manager {
      manager.set_remote_owned(true);
    }
    self.apple_music.switching = false;
    self.apple_music.recovered_handoff = None;
    self.apple_music.generation = self.apple_music.generation.wrapping_add(1);
    self.apple_music.snapshot = None;
    self.apple_music.observed_at = None;
    self.apple_music.quit_seen = None;
    self.cancel_volume_change();
    self.cancel_apple_music_volume();
    self.pending_api_seek = None;
    self.pending_source_seek = None;
    self.seek_ms = None;
    #[cfg(feature = "streaming")]
    {
      self.pending_native_seek = None;
      self.release_native_for_decoded();
    }
    #[cfg(feature = "audio-decode")]
    {
      #[allow(unused_mut)]
      let mut players = self.take_decoded_sessions_except(Source::AppleMusic);
      #[cfg(feature = "audio-decode-queue")]
      players.extend(self.take_queue_now_decoded_player());
      for player in players {
        player.pause();
        player.stop_detached();
      }
    }
    #[cfg(feature = "queue")]
    {
      self.queue_now = None;
      self.queue_suspended = None;
      self.queue_slot_desired_playing = false;
    }
    self.release_decoded_sink_claim();
    self.apple_music.generation
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn begin_apple_music_handoff(&mut self) -> u64 {
    self.apple_music.generation = self.apple_music.generation.wrapping_add(1);
    self.apple_music.switching = true;
    // A seek or volume still held back would be queued behind the handoff
    // and reach the next source.
    self.pending_source_seek = None;
    self.cancel_apple_music_volume();
    self.apple_music.generation
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn apple_music_handoff_is_current(&self, generation: u64) -> bool {
    self.apple_music.claimed
      && self.apple_music.switching
      && self.apple_music.generation == generation
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn acknowledge_apple_music_pause(
    &mut self,
    generation: u64,
    snapshot: crate::infra::apple_music::Snapshot,
  ) {
    if self.apple_music.claimed
      && self.apple_music.switching
      && self.apple_music.generation == generation
    {
      self.apple_music.desired_playing = false;
      self.apple_music.snapshot = Some(snapshot);
      self.apple_music.observed_at = Some(Instant::now());
    }
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn finish_apple_music_handoff(&mut self, generation: u64) -> bool {
    if self.apple_music.recovered_handoff == Some(generation)
      && self.apple_music.generation == generation
    {
      self.apple_music.recovered_handoff = None;
      return true;
    }
    if !self.apple_music_handoff_is_current(generation) {
      return false;
    }
    self.apple_music.claimed = false;
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    if let Some(manager) = &self.macos_media_manager {
      manager.set_remote_owned(false);
    }
    self.apple_music.switching = false;
    self.apple_music.snapshot = None;
    self.apple_music.observed_at = None;
    // A Music seek still held back by the throttle shares this slot with the
    // decoded sources: left set, the next tick would send it to them.
    self.pending_source_seek = None;
    self.cancel_apple_music_volume();
    true
  }

  /// A proven undelivered pause released the claim. Keep a token for its held
  /// start, so a newer start can invalidate the replay before the pump sees it.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn defer_recovered_apple_music_handoff(&mut self) -> u64 {
    let generation = self.apple_music.generation;
    self.apple_music.recovered_handoff = Some(generation);
    generation
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn cancel_recovered_apple_music_handoff(&mut self) {
    self.apple_music.recovered_handoff = None;
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn accept_apple_music_snapshot(
    &mut self,
    generation: u64,
    snapshot: crate::infra::apple_music::Snapshot,
  ) {
    if self.apple_music.claimed
      && self.apple_music.generation == generation
      && !self.apple_music.switching
    {
      let mut snapshot = snapshot;
      // Music's own report, read before the command window below adjusts it:
      // stopped, Music reports shuffle as off whatever the user chose.
      let reported_active = snapshot.playing || snapshot.track.is_some();
      let just_commanded = self.apple_music.pending_commands > 0
        || self
          .apple_music
          .commanded_at
          .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(1_500));
      // Music quit (or crashed) under spotatui: let go of the claim, so the
      // keys stop relaunching Music and the other sources play again. Only
      // once reads 2s apart, outside any command, still find it gone: a start
      // can be launching it. Until then nothing of this read is kept: it is
      // neither the end of a track nor a state to show.
      if !snapshot.running {
        if !just_commanded {
          match self.apple_music.quit_seen {
            Some(at) if at.elapsed() >= std::time::Duration::from_secs(2) => {
              self.release_apple_music();
              self.set_status_message("Apple Music: Music quit", 5);
            }
            Some(_) => {}
            None => self.apple_music.quit_seen = Some(Instant::now()),
          }
        }
        return;
      }
      self.apple_music.quit_seen = None;
      // A track started by itself has no queue behind it in Music: at its end
      // Music stops with no current track. That is the moment to start the
      // next track of the list ourselves. A pause keeps the current track, and
      // a stop in the middle of a track is the user's, so neither counts.
      // Judged on Music's own report, not on the state kept below while a
      // command settles: a volume change or a seek in a track's last seconds
      // must not hide its end. A track spotatui just started is excluded
      // instead, so the old track's end cannot skip the new one.
      let ended = !self.apple_music_just_stepped()
        && !snapshot.playing
        && snapshot.track.is_none()
        && self.apple_music.snapshot.as_ref().is_some_and(|previous| {
          previous.playing
            && previous.track.as_ref().is_some_and(|track| {
              self.apple_music_end_position_ms().saturating_add(5_000) >= track.duration_ms as u32
            })
        });
      // Stopped with no track inside that start guard: a track shorter than
      // the guard, or a seek to the end of the new one. Held like a read
      // near a command, the track is kept, so a read after the guard still
      // sees the end.
      let held_end = !snapshot.playing
        && snapshot.track.is_none()
        && self.apple_music_just_stepped()
        && self
          .apple_music
          .snapshot
          .as_ref()
          .is_some_and(|previous| previous.playing && previous.track.is_some());
      if (just_commanded || held_end) && !ended {
        // A read taken before a queued command ran, or right after one, can
        // still show the old state: keep the state spotatui asked for. That is
        // the intent, not the last snapshot, which a first start has not got.
        snapshot.playing = self.apple_music.desired_playing;
        if let Some(previous) = self.apple_music.snapshot.as_ref() {
          snapshot.volume = previous.volume;
          // Kept as playing, the track stays too: stored without one, the
          // next read could no longer tell that it ended.
          let lost_track = snapshot.track.is_none() && previous.track.is_some();
          if lost_track {
            snapshot.track = previous.track.clone();
          }
          // A seek still waiting in the queue: keep where spotatui put the
          // position instead of jumping back to where Music still is.
          if lost_track
            || self.apple_music.pending_commands > 0
            || self.pending_source_seek.is_some()
          {
            snapshot.position_ms = self.apple_music_position_ms();
          }
        }
      }
      // A coalesced volume may still be waiting for the next helper, even
      // after the last command's optimistic-state window has expired.
      if let Some(volume) = self
        .apple_music
        .pending_volume
        .or(self.apple_music.volume_in_flight)
      {
        snapshot.volume = volume;
      }
      if reported_active {
        self.apple_music.shuffle = snapshot.shuffle;
      }
      self.song_progress_ms = snapshot.position_ms as u128;
      self.apple_music.desired_playing = snapshot.playing;
      self.apple_music.snapshot = Some(snapshot);
      self.apple_music.observed_at = Some(Instant::now());
      self.apple_music.read_at = Some(std::time::SystemTime::now());
      self.sync_apple_music_list();
      if ended && self.step_apple_music(true) {
        self.apple_music.desired_playing = true;
      }
      self.note_display_changes();
    }
  }

  /// Drop a claim when Music is gone or an error proves the event was not
  /// delivered. Invalidate queued work and any held handoff start together.
  fn release_apple_music(&mut self) {
    self.apple_music.claimed = false;
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    if let Some(manager) = &self.macos_media_manager {
      manager.set_remote_owned(false);
    }
    self.apple_music.switching = false;
    self.apple_music.recovered_handoff = None;
    self.apple_music.desired_playing = false;
    self.apple_music.intent_revision = self.apple_music.intent_revision.wrapping_add(1);
    // Results of commands sent before the release are dropped.
    self.apple_music.generation = self.apple_music.generation.wrapping_add(1);
    self.apple_music.snapshot = None;
    self.apple_music.observed_at = None;
    self.apple_music.playing_list = None;
    self.apple_music.quit_seen = None;
    self.pending_source_seek = None;
    self.cancel_apple_music_volume();
    self.note_display_changes();
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn apple_music_failed(&mut self, generation: u64, error: anyhow::Error) -> bool {
    if self.apple_music.generation != generation {
      return false;
    }
    if self.apple_music.claimed && crate::infra::apple_music::event_not_delivered(&error) {
      self.release_apple_music();
      self.report_apple_music_error(&error);
      return true;
    }
    // A timeout or an unknown failure may have executed the Apple Event.
    // Retain the claim until a pause is acknowledged in that case.
    self.apple_music.switching = false;
    // The last snapshot stays until the next read replaces it: cleared, a
    // track ending meanwhile could no longer be recognised.
    // Keep the last intent: a failed Play may already be audible, so the next
    // toggle must request Pause rather than accidentally issuing another Play.
    self.report_apple_music_error(&error);
    false
  }

  /// Music failures are status messages, like the other sources': the error
  /// screen's Spotify device advice does not apply, and no CLI command
  /// reaches Music, so nothing reads `api_error` for them.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn report_apple_music_error(&mut self, error: &anyhow::Error) {
    self.set_error_status_message(format!("Apple Music: {error}"), 8);
  }

  pub(crate) fn apple_music_playlists(&self) -> &[PlaylistInfo] {
    &self.apple_music.playlists
  }

  /// Forget the track list or search being loaded. The sidebar's playlists
  /// load in a slot of their own and are not affected.
  pub(crate) fn cancel_apple_music_browse(&mut self) {
    self.apple_music.browse_generation = self.apple_music.browse_generation.wrapping_add(1);
    self.apple_music.browse = None;
  }

  /// Load a Music list page by page. The sidebar opens with a synthetic
  /// "All songs" row so the library is reachable before any playlist arrives.
  ///
  /// The sidebar (`Browse::Playlists`) and the list in the main pane (a
  /// playlist's tracks or a search) are separate slots with their own
  /// generation: opening a playlist must not stop the sidebar from loading,
  /// and reloading the sidebar must not drop the list a track is played from.
  pub(crate) fn browse_apple_music(&mut self, request: crate::infra::apple_music::Browse) {
    if !cfg!(all(feature = "apple-music", target_os = "macos")) {
      self.set_status_message("Apple Music requires macOS and the apple-music feature", 5);
      return;
    }
    if request == crate::infra::apple_music::Browse::Playlists {
      self.apple_music.playlists_generation = self.apple_music.playlists_generation.wrapping_add(1);
      self.apple_music.playlists = vec![PlaylistInfo {
        uri: crate::infra::apple_music::LIBRARY_URI.into(),
        name: "All songs".into(),
        owner: "Music".into(),
        track_count: 0,
        id: None,
        owner_id: None,
        collaborative: false,
        public: None,
        image_url: None,
      }];
      self.display_revisions.bump(DisplayDomain::Library);
      self.dispatch_without_spinner(IoEvent::AppleMusicPage {
        request,
        offset: 0,
        generation: self.apple_music.playlists_generation,
      });
      return;
    }
    self.cancel_apple_music_browse();
    self.apple_music.browse = Some(request.clone());
    self.apple_music.tracks.clear();
    self.dispatch_without_spinner(IoEvent::AppleMusicPage {
      request,
      offset: 0,
      generation: self.apple_music.browse_generation,
    });
  }

  /// A page is only accepted for the list still on screen: same source, same
  /// request, same generation, and for a playlist the Music track table.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn apple_music_browse_is_current(
    &self,
    request: &crate::infra::apple_music::Browse,
    generation: u64,
  ) -> bool {
    if *request == crate::infra::apple_music::Browse::Playlists {
      return self.active_source == Source::AppleMusic
        && self.apple_music.playlists_generation == generation;
    }
    self.active_source == Source::AppleMusic
      && self.apple_music.browse_generation == generation
      && self.apple_music.browse.as_ref() == Some(request)
      && (!matches!(request, crate::infra::apple_music::Browse::Tracks(_))
        || self.track_table.context == Some(TrackTableContext::AppleMusicPlaylist))
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn append_apple_music_tracks(&mut self, tracks: Vec<TrackInfo>, search: bool) {
    // A list played before all its pages arrived grows with them, so the
    // continuation does not stop at the end of the first page.
    let browse_generation = Some(self.apple_music.browse_generation);
    if let Some(list) = self
      .apple_music
      .playing_list
      .as_mut()
      .filter(|list| list.browse_generation == browse_generation)
    {
      let start = list.uris.len();
      list.uris.extend(
        tracks
          .iter()
          .filter(|t| t.is_playable)
          .filter_map(|t| t.uri.clone()),
      );
      // A shuffle round already dealt takes the new tracks in too, so they
      // play before anything repeats.
      if !list.upcoming.is_empty() {
        use rand::seq::SliceRandom;
        list.upcoming.extend(start..list.uris.len());
        list.upcoming.shuffle(&mut rand::rng());
      }
    }
    self.apple_music.tracks.extend(tracks);
    let tracks = self.apple_music.tracks.clone();
    if search {
      self.set_search_results(SearchResult {
        tracks: Some(Paged {
          total: tracks.len() as u32,
          items: tracks,
          ..Default::default()
        }),
        ..Default::default()
      });
      self.view.search_hovered_block = SearchResultBlock::SongSearch;
    } else {
      self.replace_track_table_tracks(tracks);
    }
  }

  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub(crate) fn append_apple_music_playlists(&mut self, playlists: Vec<PlaylistInfo>) {
    self.apple_music.playlists.extend(playlists);
    self.display_revisions.bump(DisplayDomain::Library);
  }

  #[cfg(test)]
  pub(crate) fn play_apple_music_track(&mut self, uri: String) {
    self.play_apple_music_track_at(uri, 0);
  }

  /// Start one Music track, inside the playlist on screen when it came from
  /// one, so Music's own next/previous stay within that playlist. It is the
  /// `occurrence`-th row holding `uri` (from 0): a playlist can hold a song
  /// twice, and next must go on from the row that was chosen.
  pub(crate) fn play_apple_music_track_at(&mut self, uri: String, occurrence: usize) {
    if self.apple_music.claimed && self.apple_music_refuses_while_switching() {
      return;
    }
    use crate::infra::apple_music::{Browse, PlayingList, LIBRARY_URI};
    // A track Music cannot play is refused here: sent on, Music would put a
    // dialog up in its own window that blocks every later start.
    let unplayable = self
      .track_table
      .tracks
      .iter()
      .chain(self.apple_music.tracks.iter())
      .any(|t| t.uri.as_deref() == Some(uri.as_str()) && !t.is_playable);
    if unplayable {
      self.set_error_status_message(
        "Apple Music: Music cannot play this track (it is no longer available, or its file is missing)",
        6,
      );
      return;
    }
    // Next, previous and the end-of-track continuation only step through
    // tracks Music can play, for the same reason.
    let uris_of = |tracks: &[TrackInfo]| -> Vec<String> {
      tracks
        .iter()
        .filter(|t| t.is_playable)
        .filter_map(|t| t.uri.clone())
        .collect()
    };
    // The list on screen the track came from: a playlist's table plays in
    // that playlist, search results play in the library.
    let list = match self.apple_music.browse.as_ref() {
      Some(Browse::Tracks(context))
        if self.track_table.context == Some(TrackTableContext::AppleMusicPlaylist) =>
      {
        Some((context.clone(), uris_of(&self.track_table.tracks)))
      }
      Some(Browse::Search(_)) => Some((LIBRARY_URI.to_string(), uris_of(&self.apple_music.tracks))),
      _ => None,
    };
    // Pages still loading for this list are added to it as they arrive.
    let browse_generation = self
      .apple_music
      .browse
      .is_some()
      .then_some(self.apple_music.browse_generation);
    match list.and_then(|(context, uris)| {
      let mut rows = uris.iter().enumerate().filter(|(_, u)| **u == uri);
      let index = rows
        .clone()
        .nth(occurrence)
        .or_else(|| rows.next())
        .map(|(i, _)| i)?;
      Some((context, uris, index))
    }) {
      Some((context, uris, index)) => {
        self.apple_music.playing_list = Some(PlayingList {
          context: context.clone(),
          uris,
          index,
          stepped_at: Instant::now(),
          history: Vec::new(),
          upcoming: Vec::new(),
          browse_generation,
          resync: false,
        });
        self.note_apple_music_start();
        self.start_playback_track_in_context(context, uri);
      }
      None => {
        self.apple_music.playing_list = None;
        self.note_apple_music_start();
        self.dispatch(IoEvent::StartPlayback(None, Some(vec![uri]), None));
      }
    }
  }

  /// Start the next (`forward`) or previous track of the list the current
  /// track was started from, following Music's shuffle for next. Returns
  /// false when there is no such list or no neighbour, so the caller falls
  /// back to Music's own command.
  /// Whether spotatui started a track of the list in the last 3s. The
  /// snapshot can still carry the old track's position then, which must not
  /// turn a quick previous into a restart.
  pub(crate) fn apple_music_just_stepped(&self) -> bool {
    self
      .apple_music
      .playing_list
      .as_ref()
      .is_some_and(|l| l.stepped_at.elapsed() < std::time::Duration::from_secs(3))
  }

  pub(crate) fn step_apple_music(&mut self, forward: bool) -> bool {
    // Handing off: a start now would cancel the switch to the other source.
    // Music's own command, the fallback, is refused with a message instead.
    if self.apple_music.switching {
      return false;
    }
    let shuffle = self.apple_music.shuffle;
    let Some(list) = self.apple_music.playing_list.as_mut() else {
      return false;
    };
    let len = list.uris.len();
    let next = match (forward, shuffle) {
      (true, true) if len > 1 => {
        // A real shuffle: deal the rest of the list in random order and only
        // reshuffle once every track played, so nothing repeats in a round.
        if list.upcoming.is_empty() {
          use rand::seq::SliceRandom;
          list.upcoming = (0..len).filter(|i| *i != list.index).collect();
          list.upcoming.shuffle(&mut rand::rng());
        }
        list.upcoming.pop()
      }
      (true, _) => (list.index + 1 < len).then_some(list.index + 1),
      // Retrace what was played first, then fall back to the row above.
      (false, _) => list
        .history
        .last()
        .copied()
        .or_else(|| list.index.checked_sub(1)),
    };
    let Some(next) = next else {
      return false;
    };
    if forward {
      list.history.push(list.index);
    } else {
      if list.history.last() == Some(&next) {
        list.history.pop();
      }
      // The track being left comes back on the next forward step, so
      // previous then next returns to it and the round still plays it.
      if shuffle {
        list.upcoming.retain(|i| *i != list.index);
        list.upcoming.push(list.index);
      }
    }
    list.upcoming.retain(|i| *i != next);
    list.index = next;
    list.stepped_at = Instant::now();
    let (context, uri) = (list.context.clone(), list.uris[next].clone());
    self.note_apple_music_start();
    self.start_playback_track_in_context(context, uri);
    true
  }

  /// A Music start was asked for: it plays. Recorded now rather than when
  /// the router takes the start, so a pause pressed in between wins.
  fn note_apple_music_start(&mut self) {
    if self.apple_music.claimed {
      self.apple_music.desired_playing = true;
      self.apple_music.intent_revision = self.apple_music.intent_revision.wrapping_add(1);
    }
  }

  /// A start of a list track never reached Music, or Music refused it:
  /// `index` already moved, so take it from the next snapshot instead.
  pub(crate) fn resync_apple_music_list(&mut self) {
    if let Some(list) = self.apple_music.playing_list.as_mut() {
      list.resync = true;
    }
  }

  /// Follow Music when it moved on by itself (the track ended), so the next
  /// step starts from the track actually playing.
  fn sync_apple_music_list(&mut self) {
    let Some(uri) = self
      .apple_music
      .snapshot
      .as_ref()
      .and_then(|s| s.track.as_ref())
      .and_then(|t| t.uri.clone())
    else {
      return;
    };
    if let Some(list) = self.apple_music.playing_list.as_mut() {
      if !list.resync && list.stepped_at.elapsed() < std::time::Duration::from_secs(4) {
        return;
      }
      // After a dropped start `index` names a track that never played: it
      // does not go into the history previous retraces.
      let played = !std::mem::take(&mut list.resync);
      // Already there: a song in the list twice must not jump back to its
      // first row.
      if list.uris.get(list.index) == Some(&uri) {
        return;
      }
      if let Some(index) = list.uris.iter().position(|u| *u == uri) {
        if index != list.index {
          if played {
            list.history.push(list.index);
          } else if list.history.last() == Some(&index) {
            list.history.pop();
          }
          list.upcoming.retain(|i| *i != index);
          list.index = index;
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn apple_music_owner_routes_without_spotify_or_a_decoded_player() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    assert_eq!(app.playback_owner(), PlaybackOwner::AppleMusic);
    assert!(!app.active_decoded_source());
    assert!(!app.native_should_drive());
    app.toggle_playback();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
    app.toggle_playback();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
    app.next_track();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::NextTrack)));
    app.force_previous_track();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::ForcePreviousTrack)));
    app.seek_to(5000);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(5000))));
    app.set_volume_percent(23);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicVolume { generation: queued, volume: 23 }) if queued == generation
    ));
    let newer = app.begin_apple_music_handoff();
    assert!(!app.finish_apple_music_handoff(generation));
    assert!(app.finish_apple_music_handoff(newer));
    assert!(!app.apple_music_owns_playback());
  }

  #[test]
  fn apple_music_claim_survives_failure_and_a_source_switch() {
    let mut app = App::default();
    let generation = app.claim_apple_music();
    app.apple_music_failed(generation, anyhow!("timeout"));
    assert_eq!(app.playback_owner(), PlaybackOwner::AppleMusic);
    assert!(app.apple_music_is_playing());
    // Browse scope has no bearing on the actual playback owner.
    app.active_source = Source::Local;
    assert_eq!(app.playback_owner(), PlaybackOwner::AppleMusic);
    assert!(!PlaybackOwner::AppleMusic.owns_local_sink());
  }

  #[test]
  fn apple_music_undelivered_errors_release_only_the_current_claim() {
    use crate::infra::apple_music::CommandError;
    let errors = [
      anyhow::Error::new(CommandError::AutomationDenied),
      anyhow::Error::new(CommandError::HelperSpawn(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "helper missing",
      ))),
    ];
    for error in errors {
      let mut app = App::default();
      let generation = app.claim_apple_music();
      app.apple_music.pending_volume = Some(42);
      app.pending_source_seek = Some(1_000);
      assert!(app.apple_music_failed(generation, error));
      assert!(!app.apple_music_owns_playback());
      assert!(!app.apple_music_is_playing());
      assert_ne!(app.apple_music.generation, generation);
      assert!(app.apple_music.pending_volume.is_none());
      assert!(app.pending_source_seek.is_none());
      assert!(app.status_message_is_error());

      let newer = app.claim_apple_music();
      assert!(!app.apple_music_failed(generation, CommandError::AutomationDenied.into()));
      assert!(app.apple_music_owns_playback());
      assert_eq!(app.apple_music.generation, newer);
    }
  }

  #[test]
  fn apple_music_recovered_handoff_is_one_shot_and_new_starts_cancel_it() {
    use crate::infra::apple_music::CommandError;
    let mut app = App::default();
    for replacement in 0..3 {
      app.claim_apple_music();
      let generation = app.begin_apple_music_handoff();
      assert!(app.apple_music_failed(generation, CommandError::AutomationDenied.into()));
      let recovered = app.defer_recovered_apple_music_handoff();
      assert!(!app.apple_music_owns_playback());
      match replacement {
        0 => assert!(app.finish_apple_music_handoff(recovered)),
        1 => app.cancel_recovered_apple_music_handoff(),
        _ => {
          app.claim_apple_music();
        }
      }
      assert!(!app.finish_apple_music_handoff(recovered));
    }
  }

  #[test]
  fn apple_music_stale_handoff_and_snapshot_cannot_replace_a_new_start() {
    let mut app = App::default();
    let old = app.claim_apple_music();
    let handoff = app.begin_apple_music_handoff();
    let current = app.claim_apple_music();
    let snapshot = crate::infra::apple_music::parse_snapshot(r#"{"running":true,"playing":true,"track":{"id":"0123456789ABCDEF","name":"Music track","artist":"Artist","album":"Album","duration":100},"position":1,"volume":23}"#).unwrap();
    app.accept_apple_music_snapshot(old, snapshot.clone());
    assert!(app.apple_music_state().snapshot.is_none());
    assert!(!app.finish_apple_music_handoff(handoff));
    app.accept_apple_music_snapshot(current, snapshot);
    let metadata = crate::infra::media_metadata::current_playback_snapshot(&app).unwrap();
    assert_eq!(
      metadata.source,
      crate::infra::media_metadata::PlaybackSource::AppleMusic
    );
    assert_eq!(metadata.metadata.album, "Album");
    assert_eq!(metadata.metadata.title, "Music track");
    let device = crate::core::plugin_api::playback_state(&app)
      .unwrap()
      .device
      .unwrap();
    assert_eq!(device.name, "Music.app");
    assert_eq!(device.volume_percent, Some(23));
  }

  #[test]
  fn apple_music_queue_requests_never_reach_spotify_or_the_native_queue() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.add_to_spotify_queue("applemusic:0123456789ABCDEF".into());
    assert!(rx.try_recv().is_err());
    app.claim_apple_music();
    app.add_to_spotify_queue("spotify:track:other".into());
    assert!(rx.try_recv().is_err());
    assert!(app.native_queue.is_empty());
  }

  #[test]
  fn apple_music_transport_keys_go_to_music_and_not_to_spotify() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.claim_apple_music();
    // No snapshot yet, so the claim's desired state reads as playing.
    app.toggle_playback();
    app.next_track();
    app.previous_track();
    app.transfer_playback_to_device("device".into(), false);
    let sent: Vec<IoEvent> = rx.try_iter().collect();
    assert!(matches!(
      sent.as_slice(),
      [
        IoEvent::PausePlayback,
        IoEvent::NextTrack,
        IoEvent::PreviousTrack
      ]
    ));
    assert!(!app.apple_music_is_playing());
    app.toggle_playback();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
  }

  #[test]
  fn apple_music_next_and_previous_start_neighbours_of_the_started_list() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.claim_apple_music();
    let uris: Vec<String> = ["A", "B", "C"]
      .iter()
      .map(|c| format!("applemusic:{}", c.repeat(16)))
      .collect();
    app.apple_music.playing_list = Some(crate::infra::apple_music::PlayingList {
      context: "applemusic:playlist:0123456789ABCDEF".into(),
      uris: uris.clone(),
      index: 0,
      stepped_at: Instant::now(),
      history: Vec::new(),
      upcoming: Vec::new(),
      browse_generation: None,
      resync: false,
    });
    let started = |rx: &std::sync::mpsc::Receiver<IoEvent>| match rx.try_recv() {
      Ok(IoEvent::StartPlayback(Some(_), Some(uris), Some(0))) => uris[0].clone(),
      _other => panic!("expected a start inside the playlist"),
    };

    app.next_track();
    assert_eq!(started(&rx), uris[1]);
    app.next_track();
    assert_eq!(started(&rx), uris[2]);
    // The end of the list falls back to Music's own command.
    app.next_track();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::NextTrack)));
    // Within the first 3s previous steps back instead of restarting.
    app.previous_track();
    assert_eq!(started(&rx), uris[1]);
  }

  #[test]
  fn apple_music_a_stale_read_cannot_undo_a_queued_pause_or_volume() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    let playing = |volume: u8| {
      crate::infra::apple_music::parse_snapshot(&format!(
        r#"{{"running":true,"playing":true,"track":null,"position":0,"volume":{volume}}}"#
      ))
      .unwrap()
    };
    app.accept_apple_music_snapshot(generation, playing(80));
    app.toggle_playback();
    app.increase_volume();
    assert_eq!(rx.try_iter().count(), 2);
    // The router queued both; a poll that read Music before they ran returns.
    app.note_apple_music_command_queued();
    app.note_apple_music_command_queued();
    app.accept_apple_music_snapshot(generation, playing(80));
    assert!(!app.apple_music_is_playing());
    assert_eq!(app.apple_music_volume(), 90);
    // So a second Space resumes instead of pausing again.
    app.toggle_playback();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
  }

  #[test]
  fn apple_music_a_search_result_plays_in_the_library_and_next_follows_the_results() {
    use crate::infra::apple_music::{Browse, LIBRARY_URI};
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let uris: Vec<String> = ["1111111111111111", "2222222222222222"]
      .iter()
      .map(|id| format!("applemusic:{id}"))
      .collect();
    app.apple_music.browse = Some(Browse::Search("q".into()));
    app.apple_music.tracks = uris
      .iter()
      .map(|u| crate::core::app::test_support::queue_track(Some(u), "Song"))
      .collect();
    app.play_apple_music_track(uris[0].clone());
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), Some(_), Some(0))) if context == LIBRARY_URI
    ));
    app.claim_apple_music();
    app.next_track();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(_), Some(ref next), Some(0))) if next[..] == uris[1..]
    ));
  }

  #[test]
  fn apple_music_continues_the_list_when_a_track_ends_but_not_on_a_pause_or_stop() {
    use crate::infra::apple_music::{parse_snapshot, PlayingList};
    let uris: Vec<String> = ["1111111111111111", "2222222222222222"]
      .iter()
      .map(|id| format!("applemusic:{id}"))
      .collect();
    let at = |playing: bool, position: f64, track: bool| {
      let track = if track {
        r#"{"id":"1111111111111111","name":"n","artist":"a","album":"b","duration":100}"#
      } else {
        "null"
      };
      parse_snapshot(&format!(
        r#"{{"running":true,"playing":{playing},"track":{track},"position":{position},"volume":50}}"#
      ))
      .unwrap()
    };
    let setup = |position: f64| {
      let (tx, rx) = channel();
      let mut app = App::new(tx, UserConfig::new(), None);
      let generation = app.claim_apple_music();
      app.apple_music.playing_list = Some(PlayingList {
        context: "applemusic:playlist:0123456789ABCDEF".into(),
        uris: uris.clone(),
        index: 0,
        // Started a while ago: an end right after a start is the old track's.
        stepped_at: Instant::now() - std::time::Duration::from_secs(10),
        history: Vec::new(),
        upcoming: Vec::new(),
        browse_generation: None,
        resync: false,
      });
      app.accept_apple_music_snapshot(generation, at(true, position, true));
      (app, rx, generation)
    };

    // Reached the end: Music stops with no current track, the next one starts.
    let (mut app, rx, generation) = setup(98.0);
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(_), Some(ref next), Some(0))) if next[..] == uris[1..]
    ));
    assert!(app.apple_music_is_playing());

    // Stopped in the middle of the track: the user's doing, nothing starts.
    let (mut app, rx, generation) = setup(20.0);
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(rx.try_recv().is_err());

    // Paused near the end keeps its current track: not an end.
    let (mut app, rx, generation) = setup(98.0);
    app.accept_apple_music_snapshot(generation, at(false, 98.0, true));
    assert!(rx.try_recv().is_err());
    assert!(!app.apple_music_is_playing());

    // A volume change in the last seconds: the read right after it still
    // counts as the end.
    let (mut app, rx, generation) = setup(98.0);
    app.note_apple_music_command_queued();
    app.finish_apple_music_command();
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(_), Some(ref next), Some(0))) if next[..] == uris[1..]
    ));

    // Right after spotatui started a track, the old track's end is not the
    // new one's, but the track is kept so a later read still sees an end.
    let (mut app, rx, generation) = setup(98.0);
    if let Some(list) = app.apple_music.playing_list.as_mut() {
      list.stepped_at = Instant::now();
    }
    app.note_apple_music_command_queued();
    app.finish_apple_music_command();
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(rx.try_recv().is_err());
    let kept = app.apple_music.snapshot.as_ref().unwrap();
    assert!(kept.track.is_some() && kept.playing);
    // Still inside the start guard but past the command window (a track
    // shorter than the guard): the track is still kept.
    app.apple_music.commanded_at = None;
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(rx.try_recv().is_err());
    assert!(app.apple_music.snapshot.as_ref().unwrap().track.is_some());
    // Once the start is old, the next read finds that end.
    if let Some(list) = app.apple_music.playing_list.as_mut() {
      list.stepped_at = Instant::now() - std::time::Duration::from_secs(10);
    }
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));

    // Reads stopped for 20s from 94s into the track (Music busy, the Mac
    // asleep): the track ran out meanwhile, and that still counts.
    let (mut app, rx, generation) = setup(94.0);
    app.apple_music.observed_at = Some(Instant::now() - std::time::Duration::from_secs(20));
    app.apple_music.read_at =
      Some(std::time::SystemTime::now() - std::time::Duration::from_secs(20));
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));
    // The Mac asleep: the monotonic clock did not move, the wall clock did.
    let (mut app, rx, generation) = setup(94.0);
    app.apple_music.read_at =
      Some(std::time::SystemTime::now() - std::time::Duration::from_secs(20));
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));

    // A command that failed in the last seconds keeps the track too.
    let (mut app, rx, generation) = setup(98.0);
    app.finish_apple_music_command();
    app.apple_music_failed(generation, anyhow!("Music helper failed"));
    app.accept_apple_music_snapshot(generation, at(false, 0.0, false));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));

    // Music quitting near the end of a track, with a command in flight, is
    // not an end: nothing starts, and nothing of that read is shown.
    let (mut app, rx, generation) = setup(98.0);
    app.note_apple_music_command_queued();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(r#"{"running":false,"playing":false,"track":null,"position":0,"volume":0}"#)
        .unwrap(),
    );
    assert!(rx.try_recv().is_err());
    let kept = app.apple_music.snapshot.as_ref().unwrap();
    assert!(kept.running && kept.track.is_some());
  }

  #[test]
  fn apple_music_quitting_music_releases_the_claim_but_not_while_a_command_runs() {
    use crate::infra::apple_music::parse_snapshot;
    let quit = || {
      parse_snapshot(r#"{"running":false,"playing":false,"track":null,"position":0,"volume":0}"#)
        .unwrap()
    };
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    // A start that is launching Music can read it not running yet.
    app.note_apple_music_command_queued();
    app.accept_apple_music_snapshot(generation, quit());
    assert!(app.apple_music_owns_playback());
    app.finish_apple_music_command();
    app.apple_music.commanded_at = None;
    // One read is not enough: Music may still be starting.
    app.accept_apple_music_snapshot(generation, quit());
    assert!(app.apple_music_owns_playback());
    // Still gone 2s later: Music quit.
    app.apple_music.quit_seen = Some(Instant::now() - std::time::Duration::from_secs(3));
    app.accept_apple_music_snapshot(generation, quit());
    assert!(!app.apple_music_owns_playback());
    assert!(!app.apple_music_is_playing());
    assert_eq!(app.status_message(), Some("Apple Music: Music quit"));
  }

  #[test]
  fn apple_music_a_pause_during_the_first_start_is_kept() {
    use crate::infra::apple_music::parse_snapshot;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    app.note_apple_music_command_queued();
    // No snapshot yet: the pause cannot be written into one.
    app.toggle_playback();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
    app.note_apple_music_command_queued();
    // The start finishes first and reports playing.
    app.finish_apple_music_command();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(r#"{"running":true,"playing":true,"track":null,"position":0,"volume":50}"#)
        .unwrap(),
    );
    assert!(!app.apple_music_is_playing());
    // So Space resumes rather than pausing a second time.
    app.toggle_playback();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
  }

  #[test]
  fn apple_music_a_pause_after_choosing_a_track_survives_the_router_taking_it() {
    use crate::infra::apple_music::Browse;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.claim_apple_music();
    app.toggle_playback();
    let uri = format!("applemusic:{}", "A".repeat(16));
    app.apple_music.browse = Some(Browse::Search("q".into()));
    app.apple_music.tracks = vec![crate::core::app::test_support::queue_track(
      Some(&uri),
      "Song",
    )];
    // Choosing a track asks for play at once.
    app.play_apple_music_track(uri);
    assert!(app.apple_music_is_playing());
    // Paused while the start still waits in the pump.
    app.toggle_playback();
    // The router takes the start: the pause is newer and stays.
    app.claim_apple_music();
    assert!(!app.apple_music_is_playing());
    assert_eq!(rx.try_iter().count(), 3);
  }

  #[test]
  fn apple_music_play_and_pause_actions_follow_the_intent_without_a_snapshot() {
    use crate::core::action::Action;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.claim_apple_music();
    app.apply(Action::Pause);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
    assert!(!app.apple_music_is_playing());
    app.apply(Action::Play);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(None, None, None))
    ));
    assert!(app.apple_music_is_playing());
  }

  #[test]
  fn apple_music_a_held_back_seek_does_not_reach_the_next_source() {
    use crate::infra::apple_music::parse_snapshot;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":true,"track":{"id":"1111111111111111","name":"n","artist":"a","album":"b","duration":100},"position":10,"volume":50}"#,
      )
      .unwrap(),
    );
    app.apple_music.playing_list = Some(crate::infra::apple_music::PlayingList {
      context: "applemusic:playlist:0123456789ABCDEF".into(),
      uris: vec![
        "applemusic:1111111111111111".into(),
        "applemusic:2222222222222222".into(),
      ],
      index: 0,
      stepped_at: Instant::now(),
      history: Vec::new(),
      upcoming: Vec::new(),
      browse_generation: None,
      resync: false,
    });
    app.seek_to(20_000);
    app.seek_to(30_000);
    assert!(app.pending_source_seek.is_some());
    let handoff = app.begin_apple_music_handoff();
    // The held-back seek is dropped as the handoff starts: the tick cannot
    // queue it behind the handoff.
    assert_eq!(app.pending_source_seek, None);
    app.flush_pending_source_seek();
    // A drag during the handoff is not queued either.
    app.seek_to(40_000);
    assert_eq!(app.pending_source_seek, None);
    // Nor do next, previous or a volume change: queued behind the handoff
    // they would reach the next source.
    let volume = app.apple_music_volume();
    app.next_track();
    app.previous_track();
    app.force_previous_track();
    app.set_apple_music_volume(10);
    assert_eq!(app.apple_music_volume(), volume);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(20_000))));
    assert!(rx.try_recv().is_err());
    assert!(app.finish_apple_music_handoff(handoff));
    assert_eq!(app.pending_source_seek, None);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn apple_music_a_song_twice_in_a_playlist_goes_on_from_the_chosen_row() {
    use crate::infra::apple_music::Browse;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let [a, b, c] = ["A", "B", "C"].map(|l| format!("applemusic:{}", l.repeat(16)));
    let rows = vec![a.clone(), b.clone(), a.clone(), c.clone()];
    app.apple_music.browse = Some(Browse::Search("q".into()));
    app.apple_music.tracks = rows
      .iter()
      .map(|u| crate::core::app::test_support::queue_track(Some(u), "Song"))
      .collect();
    app.start_playback_uris(rows.clone(), Some(2));
    assert_eq!(app.apple_music.playing_list.as_ref().unwrap().index, 2);
    let _ = rx.try_recv();
    app.claim_apple_music();
    // The read of the second A keeps that row.
    if let Some(list) = app.apple_music.playing_list.as_mut() {
      list.resync = true;
    }
    app.apple_music.snapshot = Some(
      crate::infra::apple_music::parse_snapshot(
        r#"{"running":true,"playing":true,"track":{"id":"AAAAAAAAAAAAAAAA","name":"n","artist":"a","album":"b","duration":100},"position":1,"volume":50}"#,
      )
      .unwrap(),
    );
    app.sync_apple_music_list();
    assert_eq!(app.apple_music.playing_list.as_ref().unwrap().index, 2);
    app.next_track();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(_), Some(ref next), Some(0))) if next[0] == c
    ));
  }

  #[test]
  fn apple_music_later_pages_join_the_list_already_playing() {
    use crate::infra::apple_music::Browse;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let track = |id: &str| {
      let uri = format!("applemusic:{}", id.repeat(16));
      crate::core::app::test_support::queue_track(Some(&uri), "Song")
    };
    app.apple_music.browse = Some(Browse::Search("q".into()));
    app.append_apple_music_tracks(vec![track("A")], true);
    app.play_apple_music_track(format!("applemusic:{}", "A".repeat(16)));
    app.append_apple_music_tracks(vec![track("B"), track("C")], true);
    assert_eq!(app.apple_music.playing_list.as_ref().unwrap().uris.len(), 3);
    // A shuffle round already dealt takes later pages in.
    if let Some(list) = app.apple_music.playing_list.as_mut() {
      list.upcoming = vec![1, 2];
    }
    app.append_apple_music_tracks(vec![track("E")], true);
    let mut upcoming = app
      .apple_music
      .playing_list
      .as_ref()
      .unwrap()
      .upcoming
      .clone();
    upcoming.sort();
    assert_eq!(upcoming, vec![1, 2, 3]);
    // A different list on screen no longer feeds it.
    app.cancel_apple_music_browse();
    app.append_apple_music_tracks(vec![track("D")], true);
    assert_eq!(app.apple_music.playing_list.as_ref().unwrap().uris.len(), 4);
  }

  #[test]
  fn apple_music_a_dropped_start_takes_the_position_from_music() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    let uris: Vec<String> = ["1111111111111111", "2222222222222222"]
      .iter()
      .map(|id| format!("applemusic:{id}"))
      .collect();
    app.apple_music.playing_list = Some(crate::infra::apple_music::PlayingList {
      context: "applemusic:playlist:0123456789ABCDEF".into(),
      uris,
      index: 0,
      stepped_at: Instant::now(),
      history: Vec::new(),
      upcoming: Vec::new(),
      browse_generation: None,
      resync: false,
    });
    app.next_track();
    assert_eq!(app.apple_music.playing_list.as_ref().unwrap().index, 1);
    assert_eq!(
      app.apple_music.playing_list.as_ref().unwrap().history,
      vec![0]
    );
    // Music refused it (or the router found Music busy and dropped it):
    // Music still plays the first.
    let revision = app.apple_music.intent_revision;
    app.note_apple_music_did_not_start(generation, revision);
    assert!(app.apple_music.playing_list.as_ref().unwrap().resync);
    app.accept_apple_music_snapshot(
      generation,
      crate::infra::apple_music::parse_snapshot(
        r#"{"running":true,"playing":true,"track":{"id":"1111111111111111","name":"n","artist":"a","album":"b","duration":100},"position":5,"volume":50}"#,
      )
      .unwrap(),
    );
    let list = app.apple_music.playing_list.as_ref().unwrap();
    assert_eq!(list.index, 0);
    // The track that never played is not in what previous retraces.
    assert!(list.history.is_empty());
  }

  #[cfg(all(feature = "apple-music", target_os = "macos"))]
  #[test]
  fn apple_music_sidebar_and_list_loads_do_not_cancel_each_other() {
    use crate::infra::apple_music::{Browse, LIBRARY_URI};
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.set_active_source(Source::AppleMusic);
    app.load_source_sidebar(Source::AppleMusic);
    let sidebar = match rx.try_recv() {
      Ok(IoEvent::AppleMusicPage {
        request: Browse::Playlists,
        generation,
        ..
      }) => generation,
      _other => panic!("expected the sidebar load"),
    };

    // Opening a list while the sidebar is still loading keeps the sidebar's
    // later pages welcome.
    let tracks = Browse::Tracks(LIBRARY_URI.into());
    app.open_source_playlist_tracks(LIBRARY_URI.into());
    let list = match rx.try_recv() {
      Ok(IoEvent::AppleMusicPage { generation, .. }) => generation,
      _other => panic!("expected the track list load"),
    };
    assert!(app.apple_music_browse_is_current(&Browse::Playlists, sidebar));
    assert!(app.apple_music_browse_is_current(&tracks, list));

    // Reloading the sidebar keeps the list on screen and what it holds, so a
    // row started from it still plays inside the list.
    let track = "applemusic:FEDCBA9876543210".to_string();
    app.append_apple_music_tracks(
      vec![crate::core::app::test_support::queue_track(
        Some(&track),
        "Song",
      )],
      false,
    );
    app.load_source_sidebar(Source::AppleMusic);
    assert!(!app.apple_music_browse_is_current(&Browse::Playlists, sidebar));
    assert!(app.apple_music_browse_is_current(&tracks, list));
    while rx.try_recv().is_ok() {}
    app.play_apple_music_track(track);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), Some(_), Some(0))) if context == LIBRARY_URI
    ));
  }

  #[test]
  fn apple_music_quick_seeks_add_up_and_a_drag_is_throttled() {
    use crate::infra::apple_music::parse_snapshot;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":false,"track":{"id":"1111111111111111","name":"n","artist":"a","album":"b","duration":100},"position":10,"volume":50}"#,
      )
      .unwrap(),
    );
    let step = app.user_config.behavior.seek_milliseconds;
    app.seek_forwards();
    app.seek_forwards();
    app.seek_forwards();
    // Three presses move three steps, not one step three times.
    assert_eq!(app.apple_music_position_ms(), 10_000 + 3 * step);
    // Only the first went out at once; the rest wait for the throttle.
    assert!(matches!(rx.try_recv(), Ok(IoEvent::Seek(p)) if p == 10_000 + step));
    assert!(rx.try_recv().is_err());
    assert_eq!(app.pending_source_seek, Some(10_000 + 3 * step));
    // Past the end clamps to the track.
    app.seek_to(500_000);
    assert_eq!(app.apple_music_position_ms(), 100_000);
  }

  #[test]
  fn apple_music_volume_never_goes_to_zero() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      crate::infra::apple_music::parse_snapshot(
        r#"{"running":true,"playing":true,"track":null,"position":0,"volume":5}"#,
      )
      .unwrap(),
    );
    // One step down from 5% would be 0: Music refuses to play there.
    app.decrease_volume();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicVolume { generation: queued, volume: 1 }) if queued == generation
    ));
    assert_eq!(app.apple_music_volume(), 1);
    assert!(app
      .status_message()
      .is_some_and(|m| m.contains("just above 0")));
  }

  #[test]
  fn apple_music_unchanged_volume_and_boundary_repeats_send_nothing() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    for (volume, increase) in [(100, true), (1, false)] {
      app.accept_apple_music_snapshot(
        generation,
        crate::infra::apple_music::parse_snapshot(&format!(
          r#"{{"running":true,"playing":false,"track":null,"position":0,"volume":{volume}}}"#
        ))
        .unwrap(),
      );
      for _ in 0..100 {
        app.set_volume_percent(volume);
        if increase {
          app.increase_volume();
        } else {
          app.decrease_volume();
        }
      }
      assert_eq!(app.apple_music_volume(), volume);
      assert!(rx.try_recv().is_err());
      assert!(app.apple_music.pending_volume.is_none());
      assert!(app.apple_music.volume_in_flight.is_none());
    }
  }

  #[test]
  fn apple_music_volume_coalesces_while_busy_and_throttles_the_latest_value() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    app.runtime_state.volume_percent = 50;
    // No snapshot yet: the next key still adds to the optimistic volume.
    app.increase_volume();
    let first = app.apple_music_volume();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicVolume { generation: queued, volume })
        if queued == generation && volume == first
    ));
    app.increase_volume();
    assert_eq!(
      app.apple_music_volume(),
      first + app.user_config.behavior.volume_increment
    );
    app.set_volume_percent(73);
    app.set_volume_percent(89);
    assert_eq!(app.apple_music.pending_volume, Some(89));
    assert!(rx.try_recv().is_err());
    // A slow helper does not turn the 250ms throttle into an unbounded queue.
    app.apple_music.last_volume = Some(Instant::now() - std::time::Duration::from_secs(8));
    app.flush_apple_music_volume();
    assert!(rx.try_recv().is_err());
    // Even a late snapshot must not replace the latest value still waiting.
    app.accept_apple_music_snapshot(
      generation,
      crate::infra::apple_music::parse_snapshot(
        r#"{"running":true,"playing":false,"track":null,"position":0,"volume":55}"#,
      )
      .unwrap(),
    );
    assert_eq!(app.apple_music_volume(), 89);
    assert_eq!(app.apple_music.snapshot.as_ref().unwrap().volume, 89);
    app.finish_apple_music_volume(generation);
    app.apple_music.last_volume = Some(Instant::now());
    app.flush_apple_music_volume();
    assert!(rx.try_recv().is_err());
    app.apple_music.last_volume = Some(Instant::now() - std::time::Duration::from_millis(251));
    app.flush_apple_music_volume();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicVolume { generation: queued, volume: 89 }) if queued == generation
    ));
    assert!(rx.try_recv().is_err());
    assert!(app.apple_music.pending_volume.is_none());
  }

  #[test]
  fn apple_music_volume_handoff_and_new_claim_drop_old_pending_values() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let old = app.claim_apple_music();
    app.set_volume_percent(23);
    app.set_volume_percent(42);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicVolume { generation: queued, volume: 23 }) if queued == old
    ));
    let handoff = app.begin_apple_music_handoff();
    assert!(app.apple_music.pending_volume.is_none());
    assert!(app.apple_music.volume_in_flight.is_none());
    app.set_volume_percent(99);
    app.flush_apple_music_volume();
    assert!(rx.try_recv().is_err());
    assert!(app.finish_apple_music_handoff(handoff));
    app.flush_apple_music_volume();
    assert!(rx.try_recv().is_err());

    // A new claim has no old throttle; an old helper completion cannot clear
    // the new claim's in-flight latch or let another volume flood the queue.
    let current = app.claim_apple_music();
    app.set_volume_percent(67);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicVolume { generation: queued, volume: 67 }) if queued == current
    ));
    app.finish_apple_music_volume(old);
    assert_eq!(app.apple_music.volume_in_flight, Some(67));
    app.set_volume_percent(72);
    app.finish_apple_music_volume(current);
    assert!(app.apple_music.volume_in_flight.is_none());
    assert_eq!(app.apple_music.pending_volume, Some(72));
    app.claim_apple_music();
    assert!(app.apple_music.pending_volume.is_none());
    assert!(app.apple_music.last_volume.is_none());
  }

  #[test]
  fn apple_music_position_freezes_after_ten_seconds_without_jumping_back() {
    let mut app = App::default();
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      crate::infra::apple_music::parse_snapshot(
        r#"{"running":true,"playing":true,"track":{"id":"1111111111111111","name":"n","artist":"a","album":"b","duration":100},"position":10,"volume":50}"#,
      )
      .unwrap(),
    );
    for gap in [10, 11, 20] {
      app.apple_music.observed_at = Some(Instant::now() - std::time::Duration::from_secs(gap));
      assert_eq!(app.apple_music_position_ms(), 20_000);
    }
    // The same cap still respects the track duration and a paused snapshot.
    app.apple_music.snapshot.as_mut().unwrap().position_ms = 95_000;
    assert_eq!(app.apple_music_position_ms(), 100_000);
    app.apple_music.snapshot.as_mut().unwrap().playing = false;
    assert_eq!(app.apple_music_position_ms(), 95_000);
  }

  #[test]
  fn apple_music_unplayable_tracks_are_refused_and_skipped() {
    use crate::infra::apple_music::{parse_tracks, Browse, LIBRARY_URI};
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let page = parse_tracks(
      r#"{"items":[
        {"id":"1111111111111111","name":"a","duration":10},
        {"id":"2222222222222222","name":"gone","duration":10,"playable":false},
        {"id":"3333333333333333","name":"c","duration":10,"playable":true}
      ],"offset":0,"total":3,"next":3}"#,
    )
    .unwrap();
    assert_eq!(
      page.items.iter().map(|t| t.is_playable).collect::<Vec<_>>(),
      [true, false, true]
    );
    app.apple_music.browse = Some(Browse::Tracks(LIBRARY_URI.into()));
    app.set_track_table(page.items, TrackTableContext::AppleMusicPlaylist);
    app.apple_music.browse = Some(Browse::Tracks(LIBRARY_URI.into()));

    // Enter on the unplayable row: nothing is sent to Music, the user is told.
    app.play_apple_music_track("applemusic:2222222222222222".into());
    assert!(rx.try_recv().is_err());
    assert!(app.status_message_is_error());

    // From the first row, next goes to the third: the dead row is not a stop.
    app.play_apple_music_track("applemusic:1111111111111111".into());
    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));
    app.claim_apple_music();
    app.next_track();
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(_), Some(ref next), Some(0)))
        if next[0] == "applemusic:3333333333333333"
    ));
  }

  #[test]
  fn apple_music_a_start_music_ignored_is_reported() {
    let mut app = App::default();
    let generation = app.claim_apple_music();
    let snapshot = crate::infra::apple_music::parse_snapshot(
      r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50,"started":false}"#,
    )
    .unwrap();
    assert_eq!(snapshot.started, Some(false));
    let revision = app.apple_music_state().intent_revision;
    app.accept_apple_music_snapshot(generation, snapshot);
    app.note_apple_music_did_not_start(generation, revision);
    assert!(!app.apple_music_is_playing());
    assert!(app.status_message_is_error());
    assert!(app
      .status_message()
      .is_some_and(|m| m.contains("did not start playing")));
    // A stale report from before a newer start is ignored.
    let mut app = App::default();
    let old = app.claim_apple_music();
    app.claim_apple_music();
    let revision = app.apple_music_state().intent_revision;
    app.note_apple_music_did_not_start(old, revision);
    assert!(app.status_message().is_none());
  }

  #[test]
  fn apple_music_a_failed_resume_keeps_a_newer_pause_and_resume() {
    use crate::infra::apple_music::parse_snapshot;
    let mut app = App::default();
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50}"#)
        .unwrap(),
    );
    // Resume is queued and runs; meanwhile the user pauses and resumes again.
    app.toggle_apple_music();
    let resume_revision = app.apple_music_state().intent_revision;
    app.note_apple_music_command_queued();
    app.toggle_apple_music();
    app.note_apple_music_command_queued();
    app.toggle_apple_music();
    app.note_apple_music_command_queued();
    assert!(app.apple_music_is_playing());
    // The first resume comes back: Music did not start.
    app.finish_apple_music_command();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50,"started":false}"#,
      )
      .unwrap(),
    );
    app.note_apple_music_did_not_start(generation, resume_revision);
    // The newer resume still stands, so Space sends Pause next.
    assert!(app.apple_music_is_playing());
    assert!(app.status_message_is_error());
  }

  #[test]
  fn apple_music_shuffle_is_not_read_from_a_stopped_player() {
    use crate::infra::apple_music::parse_snapshot;
    let mut app = App::default();
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":true,"track":null,"position":0,"volume":50,"shuffle":true}"#,
      )
      .unwrap(),
    );
    assert!(app.apple_music.shuffle);
    // Stopped, Music says shuffle is off whatever the user chose.
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50,"shuffle":false}"#,
      )
      .unwrap(),
    );
    assert!(app.apple_music.shuffle);
    // Nor right after a command, where the read is held to the asked-for
    // playing state and the last track: Music itself still reported stopped.
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":true,"track":{"id":"1111111111111111","name":"n","artist":"a","album":"b","duration":100},"position":1,"volume":50,"shuffle":true}"#,
      )
      .unwrap(),
    );
    app.note_apple_music_command_queued();
    app.finish_apple_music_command();
    app.accept_apple_music_snapshot(
      generation,
      parse_snapshot(
        r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50,"shuffle":false}"#,
      )
      .unwrap(),
    );
    assert!(app.apple_music.shuffle);
  }

  #[test]
  fn apple_music_shuffle_plays_every_track_once_before_any_repeats() {
    use std::collections::HashSet;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    app.accept_apple_music_snapshot(
      generation,
      crate::infra::apple_music::parse_snapshot(
        r#"{"running":true,"playing":true,"track":null,"position":0,"volume":50,"shuffle":true}"#,
      )
      .unwrap(),
    );
    let uris: Vec<String> = (0..6)
      .map(|i| format!("applemusic:{:016X}", i + 1))
      .collect();
    app.apple_music.playing_list = Some(crate::infra::apple_music::PlayingList {
      context: "applemusic:playlist:0123456789ABCDEF".into(),
      uris: uris.clone(),
      index: 2,
      stepped_at: Instant::now(),
      history: Vec::new(),
      upcoming: Vec::new(),
      browse_generation: None,
      resync: false,
    });
    let next = |app: &mut App| {
      app.next_track();
      match rx.try_recv() {
        Ok(IoEvent::StartPlayback(Some(_), Some(uris), Some(0))) => uris[0].clone(),
        _other => panic!("expected a start inside the playlist"),
      }
    };

    // One round: the five other tracks, each exactly once.
    let round: Vec<String> = (0..5).map(|_| next(&mut app)).collect();
    let distinct: HashSet<&String> = round.iter().collect();
    assert_eq!(distinct.len(), 5);
    assert!(!round.contains(&uris[2]));
    // The next round starts on another track than the one just played, and
    // again covers the other five without a repeat.
    let second: Vec<String> = (0..5).map(|_| next(&mut app)).collect();
    assert_ne!(second[0], round[4]);
    assert_eq!(second.iter().collect::<HashSet<_>>().len(), 5);
    assert!(!second.contains(&round[4]));

    // Previous then next comes back to the track that was left.
    let left = second[4].clone();
    app.previous_track();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::StartPlayback(..))));
    assert_eq!(next(&mut app), left);
  }

  #[test]
  fn apple_music_previous_retraces_a_shuffled_next() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    let generation = app.claim_apple_music();
    let snapshot = crate::infra::apple_music::parse_snapshot(
      r#"{"running":true,"playing":true,"track":null,"position":0,"volume":50,"shuffle":true}"#,
    )
    .unwrap();
    app.accept_apple_music_snapshot(generation, snapshot);
    let uris: Vec<String> = (0..8)
      .map(|i| format!("applemusic:{:016X}", i + 1))
      .collect();
    app.apple_music.playing_list = Some(crate::infra::apple_music::PlayingList {
      context: "applemusic:playlist:0123456789ABCDEF".into(),
      uris: uris.clone(),
      index: 3,
      stepped_at: Instant::now(),
      history: Vec::new(),
      upcoming: Vec::new(),
      browse_generation: None,
      resync: false,
    });
    let started = |rx: &std::sync::mpsc::Receiver<IoEvent>| match rx.try_recv() {
      Ok(IoEvent::StartPlayback(Some(_), Some(uris), Some(0))) => uris[0].clone(),
      _other => panic!("expected a start inside the playlist"),
    };

    app.next_track();
    let shuffled = started(&rx);
    assert_ne!(shuffled, uris[3]);
    app.next_track();
    started(&rx);
    // Previous walks back through what played, not the rows above.
    app.previous_track();
    assert_eq!(started(&rx), shuffled);
    app.previous_track();
    assert_eq!(started(&rx), uris[3]);
  }

  #[test]
  fn apple_music_late_pages_are_rejected_once_the_list_moved_on() {
    use crate::infra::apple_music::{Browse, LIBRARY_URI};
    let mut app = App::default();
    app.active_source = Source::AppleMusic;
    let request = Browse::Tracks(LIBRARY_URI.into());
    app.apple_music.browse = Some(request.clone());
    app.track_table.context = Some(TrackTableContext::AppleMusicPlaylist);
    assert!(app.apple_music_browse_is_current(&request, 0));
    // A newer browse bumps the generation.
    app.cancel_apple_music_browse();
    assert!(!app.apple_music_browse_is_current(&request, 0));
    // Another table replaced the Music one.
    app.apple_music.browse = Some(request.clone());
    app.set_track_table(Vec::new(), TrackTableContext::SavedTracks);
    assert!(!app.apple_music_browse_is_current(&request, 2));
    // The user switched sources.
    app.apple_music.browse = Some(request.clone());
    app.track_table.context = Some(TrackTableContext::AppleMusicPlaylist);
    app.set_active_source(Source::Local);
    assert!(!app.apple_music_browse_is_current(&request, 3));
  }

  #[cfg(all(feature = "apple-music", target_os = "macos"))]
  #[test]
  fn apple_music_sidebar_starts_with_all_songs_and_a_track_plays_in_its_playlist() {
    use crate::infra::apple_music::{Browse, LIBRARY_URI};
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), None);
    app.set_active_source(Source::AppleMusic);
    app.load_source_sidebar(Source::AppleMusic);
    assert_eq!(app.apple_music_playlists()[0].uri, LIBRARY_URI);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicPage {
        request: Browse::Playlists,
        offset: 0,
        ..
      })
    ));

    let playlist = "applemusic:playlist:0123456789ABCDEF".to_string();
    let track = "applemusic:FEDCBA9876543210".to_string();
    app.open_source_playlist_tracks(playlist.clone());
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::AppleMusicPage { request: Browse::Tracks(ref uri), .. }) if *uri == playlist
    ));
    app.append_apple_music_tracks(
      vec![crate::core::app::test_support::queue_track(
        Some(&track),
        "Song",
      )],
      false,
    );
    app.start_playback_uris(vec![track.clone()], Some(0));
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::StartPlayback(Some(ref context), Some(ref uris), Some(0)))
        if *context == playlist && uris[..] == [track]
    ));
  }
}
