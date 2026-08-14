use super::*;

impl App {
  /// Suspend the active decoded context with **skip** semantics (resume at the
  /// context's *next* track, position 0) and latch its `advancing` guard so the
  /// runner tick leaves it alone. Radio is torn down (a live stream can't share
  /// the sink) and its station stashed for reconnect. A no-op when no decoded
  /// context is active. Called before handing the sink to the native queue.
  ///
  /// `cause` decides where the context resumes under Repeat One: an auto-advance
  /// handoff replays the repeated track, a manual Next advances past it. See
  /// [`crate::infra::queue::resume_index_after_queue`].
  pub(crate) fn suspend_active_decoded_context_for_skip(
    &mut self,
    #[cfg_attr(
      not(any(feature = "local-files", feature = "subsonic", feature = "youtube")),
      allow(unused_variables)
    )]
    cause: crate::infra::queue::SuspendCause,
  ) {
    // `resume_index_after_queue` applies the per-mode wrap/clamp: Repeat All
    // wraps at the end (last -> first) so a skip at the final track resumes the
    // first one rather than reading as exhausted (`None`); Repeat One resumes
    // the *same* track on an auto-advance (a queued song must not consume the
    // repeat) but advances on a manual skip; Off clamps to `None` at the boundary.
    #[cfg_attr(
      not(any(feature = "local-files", feature = "subsonic", feature = "youtube")),
      allow(unused_variables)
    )]
    let repeat = self.decoded_repeat;
    #[cfg(feature = "local-files")]
    if let Some(local) = self.local_playback.as_mut() {
      let resume_index = crate::infra::queue::resume_index_after_queue(
        local.index,
        local.queue.len(),
        repeat,
        cause,
      );
      local.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Local {
        resume_index,
        resume_position_ms: 0,
      });
      return;
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_mut() {
      let resume_index =
        crate::infra::queue::resume_index_after_queue(s.index, s.tracks.len(), repeat, cause);
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Subsonic {
        resume_index,
        resume_position_ms: 0,
      });
      return;
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_mut() {
      let resume_index =
        crate::infra::queue::resume_index_after_queue(s.index, s.tracks.len(), repeat, cause);
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::YouTube {
        resume_index,
        resume_position_ms: 0,
      });
      return;
    }
    #[cfg(feature = "internet-radio")]
    if let Some(radio) = self.radio_playback.take() {
      radio.player.stop();
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Radio {
        station: radio.station,
      });
    }
  }

  /// Suspend the active decoded context with **mid-track** semantics (resume the
  /// *same* track at its live position) — the Enter-jump path. Radio has no
  /// seekable position, so it is stashed for reconnect like the skip path.
  pub(crate) fn suspend_active_decoded_context_mid_track(&mut self) {
    #[cfg(feature = "local-files")]
    if let Some(local) = self.local_playback.as_mut() {
      let position_ms = local.player.position().as_millis() as u64;
      let index = local.index;
      local.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Local {
        resume_index: Some(index),
        resume_position_ms: position_ms,
      });
      return;
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_mut() {
      let position_ms = s.player.position().as_millis() as u64;
      let index = s.index;
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Subsonic {
        resume_index: Some(index),
        resume_position_ms: position_ms,
      });
      return;
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_mut() {
      let position_ms = s.player.position().as_millis() as u64;
      let index = s.index;
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::YouTube {
        resume_index: Some(index),
        resume_position_ms: position_ms,
      });
      return;
    }
    #[cfg(feature = "internet-radio")]
    if let Some(radio) = self.radio_playback.take() {
      radio.player.stop();
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Radio {
        station: radio.station,
      });
    }
  }

  /// Snapshot how to resume the underlying native-Spotify context once the
  /// native queue drains, and record it in [`Self::queue_suspended`]. Skip
  /// semantics: `resume_track_uri` is the head of the Spotify mirror queue
  /// ([`Self::queue`]) — i.e. the *next* track Spirc would have played — so the
  /// context resumes at its next track, matching Spotify's own queue behavior.
  /// Either field is `None` when the corresponding state is unknown; the resume
  /// handler degrades gracefully (context-only, or track-only, or "finished").
  #[cfg(feature = "streaming")]
  pub(crate) fn suspend_native_spotify_context_for_queue(
    &mut self,
    cause: crate::infra::queue::SuspendCause,
  ) {
    // A client-side shuffle session resumes by index into its app-owned order
    // (no context reload, no reshuffle) — the whole point of the session is
    // that a queue interruption cannot regenerate the remaining shuffle order.
    if let Some(session) = self.native_spotify_shuffle.as_ref() {
      // A lazily-seeded session whose context fetch failed for good has only
      // its seed track — a shuffled resume would dead-end in "Queue finished",
      // so fall through and snapshot the Spotify context instead.
      if !(session.fetch_failed && session.order.len() <= 1) {
        let generation = session.generation;
        let resume_index = crate::infra::queue::resume_index_after_queue(
          session.index,
          session.order.len(),
          self.native_shuffle_repeat_mode(),
          cause,
        );
        let (context_uri, resume_track_uri) = self.spotify_context_snapshot_parts();
        self.queue_suspended = Some(crate::core::queue::SuspendedContext::SpotifyShuffled {
          resume_index,
          generation,
          context_uri,
          resume_track_uri,
        });
        return;
      }
    }
    self.queue_suspended = Some(self.spotify_context_suspend_snapshot());
  }

  /// Snapshot the current Spotify context as a queue suspension: the resume
  /// target is the mirror queue's head (the context's next track). Used by the
  /// suspend fall-through above.
  #[cfg(feature = "streaming")]
  pub(crate) fn spotify_context_suspend_snapshot(&self) -> crate::core::queue::SuspendedContext {
    let (context_uri, resume_track_uri) = self.spotify_context_snapshot_parts();
    crate::core::queue::SuspendedContext::Spotify {
      context_uri,
      resume_track_uri,
    }
  }

  /// The live context snapshot pieces: the playing context's uri and the
  /// Spotify mirror queue's head (the context's next track).
  #[cfg(feature = "streaming")]
  fn spotify_context_snapshot_parts(&self) -> (Option<String>, Option<String>) {
    let context_uri = self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.context.as_ref())
      .map(|c| c.uri.clone());
    let resume_track_uri = self
      .queue
      .as_ref()
      .and_then(|q| q.queue.first())
      .and_then(|item| match item {
        crate::core::plugin_api::PlayableInfo::Track(t) => t.uri.clone(),
        crate::core::plugin_api::PlayableInfo::Episode(e) => e.uri.clone(),
      });
    if context_uri.is_some() || resume_track_uri.is_some() {
      return (context_uri, resume_track_uri);
    }
    // Restored/direct-loaded playback has no server-side context and often an
    // empty mirror queue, which used to snapshot `(None, None)` and dead-end
    // the queue drain in "Queue finished" (no active playback after a queued
    // song). The recovery snapshot still knows what was playing; consult it
    // here, synchronously at suspend time, because the queued track's own
    // direct load moves the recovery observer afterwards.
    let Some(snapshot) = self.native_playback_recovery.as_ref() else {
      return (None, None);
    };
    let next_track_uri = snapshot
      .current_track_uri
      .as_deref()
      .and_then(|current| snapshot.next_raw_list_request(current))
      .and_then(|request| {
        let index = request.offset?;
        request.uris?.into_iter().nth(index)
      });
    (snapshot.context_uri.clone(), next_track_uri)
  }

  /// Convert a shuffled queue suspension into a context snapshot. Called when
  /// the shuffle session it indexes into is invalidated while suspended
  /// (disconnect recovery, failed context fetch): the shuffled resume would be
  /// a silent no-op and the queued tracks would drain into nothing. Prefers
  /// the context captured at suspension time — by conversion time
  /// `current_playback_context` may describe the queued track, not the
  /// suspended context — and falls back to the live snapshot per field.
  /// `only_generation` restricts the conversion to a suspension bound to that
  /// session generation.
  #[cfg(feature = "streaming")]
  pub(crate) fn convert_shuffled_suspension_to_context(&mut self, only_generation: Option<u64>) {
    let Some(crate::core::queue::SuspendedContext::SpotifyShuffled {
      generation,
      context_uri,
      resume_track_uri,
      ..
    }) = &self.queue_suspended
    else {
      return;
    };
    if only_generation.is_some_and(|g| *generation != g) {
      return;
    }
    let stored = (context_uri.clone(), resume_track_uri.clone());
    let live = self.spotify_context_snapshot_parts();
    self.queue_suspended = Some(crate::core::queue::SuspendedContext::Spotify {
      context_uri: stored.0.or(live.0),
      resume_track_uri: stored.1.or(live.1),
    });
  }

  /// Suspend the native-Spotify context with **mid-track** semantics (the
  /// Enter-jump path): resume at the track that was playing when the user
  /// jumped, not the context's next one. Position is not preserved — the
  /// Spotify resume path restarts the track. Pauses the streaming player so the
  /// queued track doesn't play over it. A no-op unless native streaming is the
  /// active playback device.
  #[cfg(feature = "streaming")]
  pub(crate) fn suspend_native_spotify_context_mid_track(&mut self) {
    if !self.is_native_streaming_active_for_playback() {
      return;
    }
    // Mid-track semantics for a client-side shuffle session: resume the track
    // that was playing, at its position in the app-owned order.
    if let Some(session) = self.native_spotify_shuffle.as_ref() {
      // Same stranded-session fallback as the queue suspend: a seed-only
      // session with a failed fetch would replay the seed and then dead-end,
      // so fall through and resume through the context route instead.
      if !(session.fetch_failed && session.order.len() <= 1) {
        let generation = session.generation;
        let resume_index = if session.index < session.order.len() {
          Some(session.index)
        } else {
          None
        };
        let (context_uri, resume_track_uri) = self.spotify_context_snapshot_parts();
        self.queue_suspended = Some(crate::core::queue::SuspendedContext::SpotifyShuffled {
          resume_index,
          generation,
          context_uri,
          resume_track_uri,
        });
        if let Some(player) = self.streaming_player.as_ref() {
          player.pause();
        }
        return;
      }
    }
    let context_uri = self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.context.as_ref())
      .map(|c| c.uri.clone());
    // Resume target: the *current* item. Fall back to the mirror queue's head
    // (the context's next track) when the current item is unknown.
    let resume_track_uri = self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.item.as_ref())
      .and_then(|item| match item {
        PlayableItem::Track(t) => t.id.as_ref().map(|id| id.uri()),
        PlayableItem::Episode(e) => Some(e.id.uri()),
        _ => None,
      })
      .or_else(|| {
        self
          .queue
          .as_ref()
          .and_then(|q| q.queue.first())
          .and_then(|item| match item {
            crate::core::plugin_api::PlayableInfo::Track(t) => t.uri.clone(),
            crate::core::plugin_api::PlayableInfo::Episode(e) => e.uri.clone(),
          })
      });
    self.queue_suspended = Some(crate::core::queue::SuspendedContext::Spotify {
      context_uri,
      resume_track_uri,
    });
    if let Some(player) = self.streaming_player.as_ref() {
      player.pause();
    }
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  #[cfg(feature = "streaming")]
  #[allow(deprecated)]
  fn context_playing(context_uri: &str) -> CurrentPlaybackContext {
    use rspotify::model::{context::Context, Type};
    let mut ctx = make_external_context();
    ctx.context = Some(Context {
      uri: context_uri.to_string(),
      href: String::new(),
      external_urls: HashMap::new(),
      _type: Type::Playlist,
    });
    ctx
  }

  /// The suspension snapshot records the context's uri and, as the resume target,
  /// the head of the Spotify mirror queue (the *next* track Spirc would play).
  #[cfg(feature = "streaming")]
  #[test]
  fn suspend_native_spotify_context_snapshots_context_and_next_track() {
    use crate::core::plugin_api::PlayableInfo;
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context_playing("spotify:playlist:ctx123"));
    app.queue = Some(QueueState {
      currently_playing: Some(PlayableInfo::Track(queue_track(
        Some("spotify:track:current"),
        "Current",
      ))),
      queue: vec![
        PlayableInfo::Track(queue_track(Some("spotify:track:next1"), "Next One")),
        PlayableInfo::Track(queue_track(Some("spotify:track:next2"), "Next Two")),
      ],
    });

    app.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);

    match app.queue_suspended {
      Some(SuspendedContext::Spotify {
        context_uri,
        resume_track_uri,
      }) => {
        assert_eq!(context_uri.as_deref(), Some("spotify:playlist:ctx123"));
        assert_eq!(resume_track_uri.as_deref(), Some("spotify:track:next1"));
      }
      other => panic!("expected a Spotify suspension, got {other:?}"),
    }
  }

  /// With no mirror queue or context, the snapshot degrades to all-None (the
  /// resume handler then finishes the queue rather than panicking).
  #[cfg(feature = "streaming")]
  #[test]
  fn suspend_native_spotify_context_degrades_to_none_without_state() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);

    match app.queue_suspended {
      Some(SuspendedContext::Spotify {
        context_uri,
        resume_track_uri,
      }) => {
        assert!(context_uri.is_none());
        assert!(resume_track_uri.is_none());
      }
      other => panic!("expected a Spotify suspension, got {other:?}"),
    }
  }

  /// A suspended shuffle resume is bound to its session's generation, and the
  /// suspend cause is honored under repeat-one: an auto advance replays the
  /// current track (keeps the repeat alive across the queue) while a manual Next
  /// advances past it.
  #[cfg(feature = "streaming")]
  #[test]
  fn suspend_native_shuffle_binds_generation_and_honors_cause() {
    use crate::core::queue::SuspendedContext;
    use crate::infra::queue::SuspendCause;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // Repeat-one is the only mode where AutoAdvance and ManualSkip diverge.
    let mut ctx = context_playing("spotify:playlist:ctx");
    ctx.repeat_state = RepeatState::Track;
    app.current_playback_context = Some(ctx);
    let session = || NativeSpotifyShuffleSession {
      order: vec![
        "spotify:track:a".to_string(),
        "spotify:track:b".to_string(),
        "spotify:track:c".to_string(),
      ],
      original: Vec::new(),
      index: 1,
      shuffled: true,
      fetch_complete: true,
      fetch_failed: false,
      generation: 42,
      pending_reload_index: None,
      pending_manual_skip: None,
    };

    // Auto advance under repeat-one replays the current index in place.
    app.native_spotify_shuffle = Some(session());
    app.suspend_native_spotify_context_for_queue(SuspendCause::AutoAdvance);
    match app.queue_suspended {
      Some(SuspendedContext::SpotifyShuffled {
        resume_index,
        generation,
        ..
      }) => {
        assert_eq!(
          resume_index,
          Some(1),
          "auto advance replays the current track"
        );
        assert_eq!(generation, 42, "resume is bound to the session generation");
      }
      other => panic!("expected a shuffled suspension, got {other:?}"),
    }

    // A manual Next advances past the current track even under repeat-one.
    app.native_spotify_shuffle = Some(session());
    app.suspend_native_spotify_context_for_queue(SuspendCause::ManualSkip);
    match app.queue_suspended {
      Some(SuspendedContext::SpotifyShuffled { resume_index, .. }) => {
        assert_eq!(
          resume_index,
          Some(2),
          "manual skip advances past the current track"
        );
      }
      other => panic!("expected a shuffled suspension, got {other:?}"),
    }
  }

  /// Queue resume targets the track *after* the one that was playing: with the
  /// index synced to 2, an auto-advance suspend under repeat Off stores resume
  /// index 3. (The frozen index always produced 1, replaying or rewinding to
  /// the second shuffled track after every queue drain.)
  #[cfg(feature = "streaming")]
  #[test]
  fn queue_resume_targets_the_track_after_the_synced_index() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
      order: vec![
        "spotify:track:a".to_string(),
        "spotify:track:b".to_string(),
        "spotify:track:c".to_string(),
        "spotify:track:d".to_string(),
      ],
      original: Vec::new(),
      index: 0,
      shuffled: true,
      fetch_complete: true,
      fetch_failed: false,
      generation: 7,
      pending_reload_index: None,
      pending_manual_skip: None,
    });

    let playing_uri = "spotify:track:c".to_string();
    app.sync_native_shuffle_index(base62_id_of(&playing_uri));
    app.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);

    match app.queue_suspended {
      Some(SuspendedContext::SpotifyShuffled { resume_index, .. }) => {
        assert_eq!(resume_index, Some(3));
      }
      other => panic!("expected a shuffled suspension, got {other:?}"),
    }
  }

  /// A playlist shuffle session whose background context fetch failed for good
  /// is stuck on its seed track. Suspending for the queue must snapshot the
  /// Spotify context (resumable through the context route) instead of storing
  /// a shuffled resume that dead-ends in "Queue finished".
  #[cfg(feature = "streaming")]
  #[test]
  fn stranded_seed_only_shuffle_session_suspends_to_the_context() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context_playing("spotify:playlist:ctx"));
    app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
      order: vec!["spotify:track:seed".to_string()],
      original: vec!["spotify:track:seed".to_string()],
      index: 0,
      shuffled: true,
      fetch_complete: true,
      fetch_failed: true,
      generation: 1,
      pending_reload_index: None,
      pending_manual_skip: None,
    });

    app.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);

    match app.queue_suspended {
      Some(SuspendedContext::Spotify { context_uri, .. }) => {
        assert_eq!(context_uri.as_deref(), Some("spotify:playlist:ctx"));
      }
      other => panic!("expected a context suspension, got {other:?}"),
    }
  }

  /// A genuinely one-track context (fetch succeeded, nothing more to play)
  /// still suspends as a shuffled session: its `None` resume correctly means
  /// "context exhausted" rather than a stranded fetch.
  #[cfg(feature = "streaming")]
  #[test]
  fn exhausted_one_track_shuffle_session_still_suspends_shuffled() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
      order: vec!["spotify:track:only".to_string()],
      original: vec!["spotify:track:only".to_string()],
      index: 0,
      shuffled: true,
      fetch_complete: true,
      fetch_failed: false,
      generation: 2,
      pending_reload_index: None,
      pending_manual_skip: None,
    });

    app.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);

    match app.queue_suspended {
      Some(SuspendedContext::SpotifyShuffled { resume_index, .. }) => {
        assert_eq!(resume_index, None, "one-track context is exhausted");
      }
      other => panic!("expected a shuffled suspension, got {other:?}"),
    }
  }

  /// Disconnect recovery clears the shuffle session, which orphans a shuffled
  /// queue suspension (the generation check turns the resume into a silent
  /// no-op and the queued tracks vanish). Converting it to a context snapshot
  /// first keeps the drain resumable through the context route.
  #[cfg(feature = "streaming")]
  #[test]
  fn shuffled_suspension_converts_to_context_when_session_is_cleared() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context_playing("spotify:playlist:ctx"));
    app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
      order: vec!["spotify:track:a".to_string(), "spotify:track:b".to_string()],
      original: Vec::new(),
      index: 0,
      shuffled: true,
      fetch_complete: true,
      fetch_failed: false,
      generation: 5,
      pending_reload_index: None,
      pending_manual_skip: None,
    });
    // The suspension carries the context captured when it was created; by
    // disconnect time the live context may describe the queued track instead.
    app.queue_suspended = Some(SuspendedContext::SpotifyShuffled {
      resume_index: Some(1),
      generation: 5,
      context_uri: Some("spotify:playlist:captured".to_string()),
      resume_track_uri: None,
    });

    // What disconnect recovery does, in order.
    app.convert_shuffled_suspension_to_context(None);
    app.clear_native_shuffle_session();

    match app.queue_suspended.take() {
      Some(SuspendedContext::Spotify { context_uri, .. }) => {
        assert_eq!(
          context_uri.as_deref(),
          Some("spotify:playlist:captured"),
          "the suspension-time context wins over the live one"
        );
      }
      other => panic!("expected a context suspension, got {other:?}"),
    }

    // Without a captured context the conversion falls back to the live one.
    app.queue_suspended = Some(SuspendedContext::SpotifyShuffled {
      resume_index: Some(1),
      generation: 6,
      context_uri: None,
      resume_track_uri: None,
    });
    app.convert_shuffled_suspension_to_context(None);
    match app.queue_suspended {
      Some(SuspendedContext::Spotify { context_uri, .. }) => {
        assert_eq!(context_uri.as_deref(), Some("spotify:playlist:ctx"));
      }
      other => panic!("expected a context suspension, got {other:?}"),
    }
  }

  /// Restored/direct-loaded playback has no live context and no mirror queue;
  /// the suspend fallback consults the recovery snapshot so the queue drain
  /// resumes the next raw-list track instead of dead-ending in "Queue
  /// finished" (observed as "No active playback" after a queued song).
  #[cfg(feature = "streaming")]
  #[test]
  fn empty_live_state_suspension_falls_back_to_the_recovery_snapshot() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_playback_recovery = Some(NativePlaybackRecoverySnapshot {
      generation: 1,
      context_uri: None,
      uris: Some(vec![
        "spotify:track:a".to_string(),
        "spotify:track:b".to_string(),
        "spotify:track:c".to_string(),
      ]),
      offset: Some(0),
      current_track_uri: Some("spotify:track:a".to_string()),
      loading_track_uri: None,
      track_duration_ms: None,
      position_ms: 0,
      desired_playing: true,
      shuffle: false,
      repeat: RepeatState::Off,
      recovery_attempts: 0,
    });

    app.suspend_native_spotify_context_for_queue(crate::infra::queue::SuspendCause::AutoAdvance);

    match app.queue_suspended {
      Some(SuspendedContext::Spotify {
        context_uri,
        resume_track_uri,
      }) => {
        assert_eq!(context_uri, None);
        assert_eq!(
          resume_track_uri.as_deref(),
          Some("spotify:track:b"),
          "the drain must resume the track after the one that was playing"
        );
      }
      other => panic!("expected a context suspension, got {other:?}"),
    }
  }
}
