use super::*;

/// Track identity for recovery matching: the bare id for `spotify:<kind>:<id>`
/// URIs, the full string otherwise. Delegates to [`base62_id_of`] so
/// `spotify:local:` URIs keep their full identity (their last segment is a
/// duration shared across unrelated local tracks).
#[cfg(feature = "streaming")]
fn spotify_item_identity(value: &str) -> &str {
  base62_id_of(value)
}

#[cfg(feature = "streaming")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePlaybackRecoverySnapshot {
  pub generation: u64,
  pub context_uri: Option<String>,
  pub uris: Option<Vec<String>>,
  pub offset: Option<usize>,
  pub current_track_uri: Option<String>,
  pub loading_track_uri: Option<String>,
  pub track_duration_ms: Option<u32>,
  pub position_ms: u32,
  pub desired_playing: bool,
  pub shuffle: bool,
  pub repeat: RepeatState,
  pub recovery_attempts: u8,
}

#[cfg(feature = "streaming")]
impl NativePlaybackRecoverySnapshot {
  pub fn expected_track_uri(&self) -> Option<&str> {
    self
      .loading_track_uri
      .as_deref()
      .or(self.current_track_uri.as_deref())
      .or_else(|| {
        let uris = self.uris.as_ref()?;
        if self.context_uri.is_some() {
          uris.first().map(String::as_str)
        } else {
          uris
            .get(self.offset.unwrap_or(0))
            .or_else(|| uris.first())
            .map(String::as_str)
        }
      })
  }

  pub fn restore_position_ms(&self) -> u32 {
    if self.loading_track_uri.is_some() {
      0
    } else {
      self
        .track_duration_ms
        .map_or(self.position_ms, |duration_ms| {
          if duration_ms == 0 {
            self.position_ms
          } else {
            self.position_ms.min(duration_ms - 1)
          }
        })
    }
  }

  pub fn completed_track_uri(&self) -> Option<&str> {
    let duration_ms = self.track_duration_ms?;
    if self.desired_playing
      && self.loading_track_uri.is_none()
      && duration_ms > 0
      && self.position_ms >= duration_ms
    {
      self.current_track_uri.as_deref()
    } else {
      None
    }
  }

  fn canonical_track_uri(&self, observed_track_id: &str, kind: Option<NativeTrackKind>) -> String {
    if observed_track_id.starts_with("spotify:") {
      return observed_track_id.to_string();
    }
    if let Some(uri) = self
      .loading_track_uri
      .iter()
      .chain(self.current_track_uri.iter())
      .chain(self.uris.iter().flatten())
      .find(|uri| spotify_item_identity(uri) == spotify_item_identity(observed_track_id))
    {
      return uri.clone();
    }

    let kind = kind.unwrap_or_else(|| {
      if self
        .current_track_uri
        .as_deref()
        .is_some_and(|uri| uri.starts_with("spotify:episode:"))
      {
        NativeTrackKind::Episode
      } else {
        NativeTrackKind::Track
      }
    });
    let item_type = match kind {
      NativeTrackKind::Track => "track",
      NativeTrackKind::Episode => "episode",
    };
    format!("spotify:{item_type}:{observed_track_id}")
  }

  fn update_offset_for_track(&mut self, track_uri: &str) {
    let Some(uris) = self.uris.as_ref() else {
      return;
    };
    if let Some(index) = uris
      .iter()
      .position(|uri| spotify_item_identity(uri) == spotify_item_identity(track_uri))
    {
      self.offset = Some(index);
    }
  }

  pub(super) fn next_raw_list_request(
    &self,
    previous_track_uri: &str,
  ) -> Option<PendingStartPlayback> {
    if self.context_uri.is_some() {
      return None;
    }
    let uris = self.uris.as_ref()?;
    if uris.is_empty() {
      return None;
    }

    let current_index = uris
      .iter()
      .position(|uri| spotify_item_identity(uri) == spotify_item_identity(previous_track_uri))
      .or(self.offset)
      .unwrap_or(0)
      .min(uris.len() - 1);
    let next_index = match self.repeat {
      RepeatState::Track => current_index,
      RepeatState::Context if current_index + 1 >= uris.len() => 0,
      _ if current_index + 1 < uris.len() => current_index + 1,
      _ => return None,
    };

    Some(PendingStartPlayback {
      context_uri: None,
      uris: Some(uris.clone()),
      offset: Some(next_index),
      parked_at: Instant::now(),
      recovery_attempts: 0,
    })
  }
}

#[cfg(feature = "streaming")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePlaybackRestoreAttempt {
  pub generation: u64,
  pub expected_track_uri: Option<String>,
  pub desired_playing: bool,
}

impl App {
  #[cfg(feature = "streaming")]
  fn next_native_playback_generation(&mut self) -> u64 {
    self.native_playback_generation = self.native_playback_generation.wrapping_add(1);
    self.native_playback_generation
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn record_native_playback_request(
    &mut self,
    context_uri: Option<String>,
    uris: Option<Vec<String>>,
    offset: Option<usize>,
    desired_playing: bool,
    shuffle: bool,
    repeat: RepeatState,
  ) -> u64 {
    let generation = self.next_native_playback_generation();
    let current_track_uri = uris.as_ref().and_then(|items| {
      if context_uri.is_some() {
        items.first().cloned()
      } else {
        items
          .get(offset.unwrap_or(0))
          .or_else(|| items.first())
          .cloned()
      }
    });
    self.native_playback_recovery = Some(NativePlaybackRecoverySnapshot {
      generation,
      context_uri,
      uris,
      offset,
      current_track_uri,
      loading_track_uri: None,
      track_duration_ms: None,
      position_ms: 0,
      desired_playing,
      shuffle,
      repeat,
      recovery_attempts: 0,
    });
    self.native_restore_pending = None;
    self.native_load_watchdog = None;
    generation
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn set_native_playback_intent(&mut self, desired_playing: bool) {
    if self.native_playback_recovery.is_none() {
      self.native_restore_pending = None;
      return;
    }
    let generation = self.next_native_playback_generation();
    if let Some(snapshot) = self.native_playback_recovery.as_mut() {
      snapshot.generation = generation;
      snapshot.desired_playing = desired_playing;
      snapshot.recovery_attempts = 0;
    }
    self.native_restore_pending = None;
    self.native_load_watchdog = None;
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn set_native_recovery_position(&mut self, position_ms: u32) {
    if let Some(snapshot) = self.native_playback_recovery.as_mut() {
      snapshot.position_ms = position_ms;
    }
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn set_native_recovery_shuffle(&mut self, shuffle: bool) {
    if let Some(snapshot) = self.native_playback_recovery.as_mut() {
      snapshot.shuffle = shuffle;
    }
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn set_native_recovery_repeat(&mut self, repeat: RepeatState) {
    if let Some(snapshot) = self.native_playback_recovery.as_mut() {
      snapshot.repeat = repeat;
    }
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn prepare_native_playback_recovery(
    &mut self,
    position_ms: u32,
    is_playing: bool,
  ) -> Option<u64> {
    if self.native_playback_recovery.is_none() {
      let context_uri = self
        .current_playback_context
        .as_ref()
        .and_then(|ctx| ctx.context.as_ref())
        .map(|ctx| ctx.uri.clone());
      let current_track_uri = self
        .current_playback_context
        .as_ref()
        .and_then(|ctx| ctx.item.as_ref())
        .and_then(|item| match item {
          PlayableItem::Track(track) => track.id.as_ref().map(|id| id.uri()),
          PlayableItem::Episode(episode) => Some(episode.id.uri()),
          PlayableItem::Unknown(_) => None,
        })
        .or_else(|| {
          self.last_track_id.as_ref().map(|id| {
            if id.starts_with("spotify:") {
              id.clone()
            } else {
              let item_type = match self.native_track_info.as_ref().map(|info| info.kind) {
                Some(NativeTrackKind::Episode) => "episode",
                _ => "track",
              };
              format!("spotify:{item_type}:{id}")
            }
          })
        });
      if context_uri.is_some() || current_track_uri.is_some() {
        let uris = context_uri
          .is_none()
          .then(|| current_track_uri.clone().into_iter().collect::<Vec<_>>());
        let generation = self.next_native_playback_generation();
        self.native_playback_recovery = Some(NativePlaybackRecoverySnapshot {
          generation,
          context_uri,
          uris,
          offset: Some(0),
          current_track_uri,
          loading_track_uri: None,
          track_duration_ms: self.native_track_info.as_ref().map(|info| info.duration_ms),
          position_ms,
          desired_playing: is_playing,
          shuffle: self.runtime_state.shuffle_enabled,
          repeat: self
            .current_playback_context
            .as_ref()
            .map_or(RepeatState::Off, |ctx| ctx.repeat_state),
          recovery_attempts: 0,
        });
      }
    }

    let track_duration_ms = self.native_track_info.as_ref().map(|info| info.duration_ms);
    let snapshot = self.native_playback_recovery.as_mut()?;
    snapshot.position_ms = position_ms;
    if snapshot.loading_track_uri.is_none() && snapshot.track_duration_ms.is_none() {
      snapshot.track_duration_ms = track_duration_ms;
    }
    Some(snapshot.generation)
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn begin_native_playback_restore(
    &mut self,
    generation: u64,
  ) -> Option<NativePlaybackRecoverySnapshot> {
    let snapshot = self.native_playback_recovery.as_mut()?;
    if snapshot.generation != generation {
      return None;
    }
    snapshot.recovery_attempts = snapshot.recovery_attempts.saturating_add(1);
    let attempt = NativePlaybackRestoreAttempt {
      generation,
      expected_track_uri: snapshot.expected_track_uri().map(str::to_string),
      desired_playing: snapshot.desired_playing,
    };
    self.native_restore_pending = Some(attempt);
    Some(snapshot.clone())
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn native_playback_restore_generation(&self) -> Option<u64> {
    self
      .native_playback_recovery
      .as_ref()
      .map(|snapshot| snapshot.generation)
  }

  #[cfg(feature = "streaming")]
  fn native_restore_event_matches(&self, track_uri: &str) -> bool {
    self
      .native_restore_pending
      .as_ref()
      .and_then(|attempt| attempt.expected_track_uri.as_deref())
      .is_none_or(|expected| spotify_item_identity(expected) == spotify_item_identity(track_uri))
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn observe_native_loading(&mut self, track_uri: String, position_ms: u32) {
    if self.native_restore_pending.is_some() && !self.native_restore_event_matches(&track_uri) {
      log::warn!(
        "ignoring stale native Loading event during restore: expected {:?}, got {}",
        self
          .native_restore_pending
          .as_ref()
          .and_then(|attempt| attempt.expected_track_uri.as_deref()),
        track_uri
      );
      return;
    }
    let canonical_track_uri = self
      .native_playback_recovery
      .as_ref()
      .map_or(track_uri.clone(), |snapshot| {
        snapshot.canonical_track_uri(&track_uri, None)
      });
    if let Some(snapshot) = self.native_playback_recovery.as_mut() {
      snapshot.loading_track_uri = Some(canonical_track_uri);
      snapshot.track_duration_ms = None;
      snapshot.position_ms = position_ms;
    }
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn observe_native_track_changed(
    &mut self,
    track_uri: String,
    kind: NativeTrackKind,
    duration_ms: u32,
  ) {
    if self.native_restore_pending.is_some() && !self.native_restore_event_matches(&track_uri) {
      log::warn!(
        "ignoring stale native TrackChanged event during restore: expected {:?}, got {}",
        self
          .native_restore_pending
          .as_ref()
          .and_then(|attempt| attempt.expected_track_uri.as_deref()),
        track_uri
      );
      return;
    }
    let canonical_track_uri = self
      .native_playback_recovery
      .as_ref()
      .map_or(track_uri.clone(), |snapshot| {
        snapshot.canonical_track_uri(&track_uri, Some(kind))
      });
    if let Some(snapshot) = self.native_playback_recovery.as_mut() {
      snapshot.current_track_uri = Some(canonical_track_uri.clone());
      snapshot.loading_track_uri = None;
      snapshot.track_duration_ms = Some(duration_ms);
      snapshot.position_ms = 0;
      snapshot.update_offset_for_track(&canonical_track_uri);
    }
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn observe_native_playback_state(
    &mut self,
    track_uri: String,
    position_ms: u32,
    is_playing: bool,
  ) -> bool {
    let restore_matches = self.native_restore_event_matches(&track_uri);
    let restore_confirmed = self.native_restore_pending.as_ref().is_some_and(|attempt| {
      attempt.generation
        == self
          .native_playback_recovery
          .as_ref()
          .map_or(u64::MAX, |snapshot| snapshot.generation)
        && restore_matches
        && attempt.desired_playing == is_playing
    });
    let canonical_track_uri = self
      .native_playback_recovery
      .as_ref()
      .map_or(track_uri.clone(), |snapshot| {
        snapshot.canonical_track_uri(&track_uri, None)
      });

    if self.native_restore_pending.is_none() || restore_matches {
      if let Some(snapshot) = self.native_playback_recovery.as_mut() {
        snapshot.current_track_uri = Some(canonical_track_uri.clone());
        snapshot.loading_track_uri = None;
        snapshot.position_ms = position_ms;
        snapshot.update_offset_for_track(&canonical_track_uri);
        // A transport failure can emit Paused immediately before the event
        // stream closes. Keep desired intent independent from observed state:
        // explicit pause controls update it before issuing the command, while
        // a successful Playing event may safely confirm play intent.
        if self.native_restore_pending.is_none() && is_playing {
          snapshot.desired_playing = true;
        }
        if restore_confirmed {
          snapshot.recovery_attempts = 0;
        }
      }
    }

    if restore_confirmed {
      self.native_restore_pending = None;
      self.native_load_watchdog = None;
    }
    restore_confirmed
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn native_raw_list_next_request(
    &self,
    previous_track_uri: &str,
  ) -> Option<PendingStartPlayback> {
    let snapshot = self.native_playback_recovery.as_ref()?;
    if snapshot
      .loading_track_uri
      .as_deref()
      .is_some_and(|loading| {
        spotify_item_identity(loading) != spotify_item_identity(previous_track_uri)
      })
      || snapshot
        .current_track_uri
        .as_deref()
        .is_some_and(|current| {
          spotify_item_identity(current) != spotify_item_identity(previous_track_uri)
        })
    {
      return None;
    }
    snapshot.next_raw_list_request(previous_track_uri)
  }

  /// True when native playback is a raw URI list (no context) that has no track
  /// after `previous_track_uri`: the list ran out with repeat off. Ending here
  /// is a legitimate stop, not a stall.
  #[cfg(feature = "streaming")]
  pub(crate) fn native_raw_list_playback_exhausted(&self, previous_track_uri: &str) -> bool {
    let Some(snapshot) = self.native_playback_recovery.as_ref() else {
      return false;
    };
    snapshot.context_uri.is_none()
      && snapshot.uris.as_ref().is_some_and(|uris| !uris.is_empty())
      && !self.native_transition_has_advanced(previous_track_uri)
      && self
        .native_raw_list_next_request(previous_track_uri)
        .is_none()
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn native_transition_has_advanced(&self, previous_track_uri: &str) -> bool {
    let Some(snapshot) = self.native_playback_recovery.as_ref() else {
      return false;
    };
    snapshot
      .loading_track_uri
      .as_deref()
      .is_some_and(|loading| {
        spotify_item_identity(loading) != spotify_item_identity(previous_track_uri)
      })
      || snapshot
        .current_track_uri
        .as_deref()
        .is_some_and(|current| {
          spotify_item_identity(current) != spotify_item_identity(previous_track_uri)
        })
  }

  #[cfg(feature = "streaming")]
  pub(crate) fn clear_native_playback_recovery(&mut self) {
    self.native_playback_recovery = None;
    self.native_restore_pending = None;
    self.native_load_watchdog = None;
  }
}

#[cfg(all(test, feature = "streaming"))]
mod tests {
  use super::*;

  #[cfg(feature = "streaming")]
  #[test]
  fn native_recovery_prefers_in_flight_loading_track() {
    let mut app = App::default();
    let generation = app.record_native_playback_request(
      None,
      Some(vec![
        "spotify:track:current".to_string(),
        "spotify:track:next".to_string(),
      ]),
      Some(0),
      true,
      false,
      RepeatState::Off,
    );
    app.observe_native_playback_state("current".to_string(), 175_000, true);
    app.observe_native_loading("next".to_string(), 0);

    let snapshot = app.begin_native_playback_restore(generation).unwrap();

    assert_eq!(snapshot.expected_track_uri(), Some("spotify:track:next"));
    assert_eq!(snapshot.restore_position_ms(), 0);
    assert!(snapshot.desired_playing);
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_native_event_does_not_confirm_restore_generation() {
    let mut app = App::default();
    let generation = app.record_native_playback_request(
      None,
      Some(vec!["spotify:track:expected".to_string()]),
      Some(0),
      true,
      false,
      RepeatState::Off,
    );
    app.begin_native_playback_restore(generation).unwrap();

    assert!(!app.observe_native_playback_state("spotify:track:stale".to_string(), 0, true));
    assert!(app.native_restore_pending.is_some());

    assert!(app.observe_native_playback_state("expected".to_string(), 12_000, true));
    assert!(app.native_restore_pending.is_none());
    assert_eq!(
      app.native_playback_recovery.as_ref().unwrap().position_ms,
      12_000
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn transport_pause_does_not_overwrite_explicit_play_intent() {
    let mut app = App::default();
    app.record_native_playback_request(
      None,
      Some(vec!["spotify:track:current".to_string()]),
      Some(0),
      true,
      false,
      RepeatState::Off,
    );

    app.observe_native_playback_state("current".to_string(), 20_000, false);
    app.prepare_native_playback_recovery(20_000, false);

    assert!(
      app
        .native_playback_recovery
        .as_ref()
        .unwrap()
        .desired_playing
    );

    app.set_native_playback_intent(false);
    assert!(
      !app
        .native_playback_recovery
        .as_ref()
        .unwrap()
        .desired_playing
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn completed_track_during_transport_loss_is_detected_for_recovery() {
    let mut app = App::default();
    app.record_native_playback_request(
      None,
      Some(vec![
        "spotify:track:finished".to_string(),
        "spotify:track:next".to_string(),
      ]),
      Some(0),
      true,
      false,
      RepeatState::Off,
    );
    app.observe_native_track_changed("finished".to_string(), NativeTrackKind::Track, 389_094);

    app.prepare_native_playback_recovery(395_562, false);

    assert_eq!(
      app
        .native_playback_recovery
        .as_ref()
        .and_then(NativePlaybackRecoverySnapshot::completed_track_uri),
      Some("spotify:track:finished")
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_raw_list_next_request_matches_uri_and_base62_forms() {
    let mut app = App::default();
    app.record_native_playback_request(
      None,
      Some(vec![
        "spotify:track:first".to_string(),
        "spotify:track:second".to_string(),
      ]),
      Some(0),
      true,
      false,
      RepeatState::Off,
    );

    let request = app.native_raw_list_next_request("first").unwrap();

    assert_eq!(request.offset, Some(1));
    assert_eq!(
      request.uris.as_deref(),
      Some(
        [
          "spotify:track:first".to_string(),
          "spotify:track:second".to_string(),
        ]
        .as_slice()
      )
    );
  }

  #[cfg(feature = "streaming")]
  fn raw_list_app(offset: usize, repeat: RepeatState) -> App {
    let mut app = App::default();
    app.record_native_playback_request(
      None,
      Some(vec![
        "spotify:track:first".to_string(),
        "spotify:track:second".to_string(),
      ]),
      Some(offset),
      true,
      false,
      repeat,
    );
    app
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn raw_list_is_exhausted_after_the_last_track_with_repeat_off() {
    let app = raw_list_app(1, RepeatState::Off);
    assert!(app.native_raw_list_playback_exhausted("second"));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn raw_list_is_not_exhausted_mid_list_or_while_repeating() {
    let app = raw_list_app(0, RepeatState::Off);
    assert!(!app.native_raw_list_playback_exhausted("first"));

    let app = raw_list_app(1, RepeatState::Context);
    assert!(!app.native_raw_list_playback_exhausted("second"));

    let app = raw_list_app(1, RepeatState::Track);
    assert!(!app.native_raw_list_playback_exhausted("second"));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn raw_list_is_not_exhausted_for_context_playback_or_advanced_transitions() {
    let mut app = App::default();
    app.record_native_playback_request(
      Some("spotify:playlist:ctx".to_string()),
      Some(vec!["spotify:track:first".to_string()]),
      Some(0),
      true,
      false,
      RepeatState::Off,
    );
    assert!(!app.native_raw_list_playback_exhausted("first"));

    // A transition onto another track means playback advanced; the list is in
    // use, not exhausted.
    let mut app = raw_list_app(1, RepeatState::Off);
    app.observe_native_loading("spotify:track:other".to_string(), 0);
    assert!(!app.native_raw_list_playback_exhausted("second"));
  }
}
