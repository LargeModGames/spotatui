use super::*;

/// App-owned play order for native-Spotify shuffle. When shuffle is on and a
/// Spotify context (playlist/album/Liked Songs) starts on the native streaming
/// device, the app builds this session and loads a flat, pre-shuffled URI list
/// into Spirc (`LoadRequest::from_tracks`) instead of delegating shuffle to
/// Spirc/Spotify — so every track plays exactly once per pass and a queue
/// suspend/resume never reshuffles the remaining order.
#[cfg(feature = "streaming")]
pub struct NativeSpotifyShuffleSession {
  /// The URI list currently loaded into Spirc, in play order (shuffled while
  /// [`Self::shuffled`] is true).
  pub order: Vec<String>,
  /// The context's original order — grows to the full context when the
  /// background fetch completes. Restored on shuffle-off.
  pub original: Vec<String>,
  /// Index into `order` of the currently-playing track (synced from
  /// `PlayerEvent::TrackChanged`).
  pub index: usize,
  /// Whether `order` is currently shuffled.
  pub shuffled: bool,
  /// False while the background full-context fetch is still running.
  pub fetch_complete: bool,
  /// The background full-context fetch failed for good (retries exhausted):
  /// `order` never grew past its seed and the queue-suspend path must fall
  /// back to the context route instead of a shuffled resume.
  pub fetch_failed: bool,
  /// Stamp guarding stale background fetches from writing into a newer session.
  pub generation: u64,
  /// Set to the index we just loaded into Spirc whenever the session issues a
  /// `from_tracks` reload; consumed by the next `TrackChanged` so a reload is
  /// confirmed in place rather than mistaken for a forward advance (which would
  /// mis-map a duplicate track id onto a later occurrence).
  pub pending_reload_index: Option<usize>,
  /// Set when the user issues a manual skip so the next `TrackChanged` is read
  /// as an explicit advance in that direction (`Some(true)` = Next, `Some(false)`
  /// = Previous) rather than a repeat-one auto replay, which stays put.
  /// Distinguishes an explicit skip to a duplicate track id from a replay of it
  /// and resolves the duplicate in the skipped direction.
  pub pending_manual_skip: Option<bool>,
}

/// The bare base62 id of a `spotify:<kind>:<id>` URI. librespot 0.8's
/// `SpotifyUri` Display prints the full URI while every app-side comparison
/// uses the bare id, so player-event handlers normalize through this once at
/// the event boundary. Anything else passes through unchanged: a bare id, and
/// notably `spotify:local:<artist>:<album>:<title>:<duration>` URIs, whose
/// only unique identity is the full string (the last segment is a duration
/// shared across unrelated local tracks).
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn base62_id_of(uri_or_id: &str) -> &str {
  let mut parts = uri_or_id.split(':');
  match (parts.next(), parts.next(), parts.next(), parts.next()) {
    (Some("spotify"), Some(kind), Some(id), None) if kind != "local" => id,
    _ => uri_or_id,
  }
}

/// Whether a `spotify:track:<id>` URI refers to the given bare base62 track id.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn uri_matches_base62_id(uri: &str, base62_id: &str) -> bool {
  base62_id_of(uri) == base62_id
}

/// How playback moved to the track a `TrackChanged` reports, so the wrap-search
/// resolves a duplicate id to the right occurrence.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShuffleStep {
  /// Sequential forward move (auto-advance or a manual Next): the next
  /// occurrence at or after `prev + 1`.
  Forward,
  /// A manual Previous: the previous occurrence at or before `prev - 1`.
  Backward,
  /// A repeat-one auto replay: stay on the current occurrence (`prev` first).
  Stay,
}

/// Resolve which index of `order` a `TrackChanged` for `playing_base62_id`
/// refers to, given the previously-known index `prev` and how playback moved
/// ([`ShuffleStep`]).
///
/// `prev` is always searched last, so a duplicate track resolves to the
/// upcoming (`Forward`) or previous (`Backward`) copy, while a unique-track
/// re-fire still lands on `prev`. `Stay` checks `prev` first, so a repeat-one
/// replay keeps its index. Returns `None` when the id is not in `order`.
#[cfg_attr(not(feature = "streaming"), allow(dead_code))]
pub(crate) fn shuffle_advance_index(
  order: &[String],
  prev: usize,
  playing_base62_id: &str,
  step: ShuffleStep,
) -> Option<usize> {
  let len = order.len();
  if len == 0 {
    return None;
  }
  let prev = prev.min(len - 1);
  (0..len).find_map(|k| {
    let i = match step {
      ShuffleStep::Stay => (prev + k) % len,
      ShuffleStep::Forward => (prev + 1 + k) % len,
      ShuffleStep::Backward => (prev + len - 1 - k) % len,
    };
    uri_matches_base62_id(&order[i], playing_base62_id).then_some(i)
  })
}

impl App {
  /// Drop the native-Spotify shuffle session and invalidate any in-flight
  /// background fetch for it.
  #[cfg(feature = "streaming")]
  pub(crate) fn clear_native_shuffle_session(&mut self) {
    self.native_shuffle_generation = self.native_shuffle_generation.wrapping_add(1);
    self.native_spotify_shuffle = None;
  }

  /// The generation stamp for a new shuffle session (also invalidates fetches
  /// for any previous session).
  #[cfg(feature = "streaming")]
  pub(crate) fn next_native_shuffle_generation(&mut self) -> u64 {
    self.native_shuffle_generation = self.native_shuffle_generation.wrapping_add(1);
    self.native_shuffle_generation
  }

  /// The current repeat state as the source-neutral [`RepeatMode`] used by the
  /// queue engine's index math.
  #[cfg(feature = "streaming")]
  pub(crate) fn native_shuffle_repeat_mode(&self) -> crate::infra::queue::RepeatMode {
    match self
      .current_playback_context
      .as_ref()
      .map(|ctx| ctx.repeat_state)
    {
      Some(RepeatState::Context) => crate::infra::queue::RepeatMode::Context,
      Some(RepeatState::Track) => crate::infra::queue::RepeatMode::Track,
      _ => crate::infra::queue::RepeatMode::Off,
    }
  }

  /// Whether native playback is currently in the playing (not paused) state.
  /// A client-side shuffle reload (`from_tracks`) must preserve this instead of
  /// unconditionally starting playback, so toggling shuffle while paused does
  /// not resume it. `native_is_playing` is the authoritative native state
  /// (updated by player events); the context flag is only a fallback, matching
  /// how the media-metadata snapshot reads it.
  #[cfg(feature = "streaming")]
  pub(crate) fn native_shuffle_is_playing(&self) -> bool {
    self
      .native_is_playing
      .or_else(|| {
        self
          .current_playback_context
          .as_ref()
          .map(|ctx| ctx.is_playing)
      })
      .unwrap_or(true)
  }

  /// Flag the client-side shuffle session so the next `TrackChanged` is read as
  /// an explicit skip (`forward` = Next, else Previous) rather than a repeat-one
  /// auto replay. Called from the native Next/Previous transport paths before
  /// driving Spirc, so a duplicate track resolves in the skipped direction.
  #[cfg(feature = "streaming")]
  pub(crate) fn mark_native_shuffle_manual_skip(&mut self, forward: bool) {
    if let Some(session) = self.native_spotify_shuffle.as_mut() {
      session.pending_manual_skip = Some(forward);
    }
  }

  /// Keep the shuffle session's index in step with what Spirc is actually
  /// playing (called from `PlayerEvent::TrackChanged` with librespot's base62
  /// track id). Also detects a completed repeat-all lap (last track wrapping
  /// back to the first) and dispatches a fresh lap reshuffle, matching
  /// Spotify's own per-lap reshuffle behavior.
  #[cfg(feature = "streaming")]
  pub(crate) fn sync_native_shuffle_index(&mut self, playing_base62_id: &str) {
    // While a queued track owns playback the session is suspended; a queued
    // track that also appears in the playlist must not move the session index.
    if self.queue_owns_playback() {
      return;
    }
    // Repeat-one replays the current track in place (no reload), so an *auto*
    // TrackChanged for it must keep the index on the current occurrence — but a
    // *manual* skip (below) advances even under repeat-one.
    let repeat_one = self.native_shuffle_repeat_mode() == crate::infra::queue::RepeatMode::Track;
    let lap_wrapped = {
      let Some(session) = self.native_spotify_shuffle.as_mut() else {
        return;
      };
      let len = session.order.len();
      if len == 0 {
        return;
      }
      let prev = session.index.min(len - 1);
      // A reload we just issued (`playing_track = Index(k)`) makes Spirc emit a
      // TrackChanged for `order[k]`; confirm that index rather than treating it
      // as a forward advance (which would mis-map a duplicate id to a later
      // copy). A reload is never a lap wrap. This is checked before the skip
      // logic even when a manual skip is also pending: Spirc emits the reload's
      // event before the skip's, so confirming it here and leaving the skip flag
      // for the skip's own (next) event keeps both in order.
      //
      // Residual, intentionally unhandled: a reload's confirmation event and a
      // manual-skip event to an *adjacent duplicate* of the reloaded track are
      // indistinguishable by track id, and whether Spirc emits one event or two
      // is an internal timing race it does not expose. The mis-mapping is a
      // wrong occurrence of the *same* track, negligible in practice (a human
      // skip lands well after the reload event) and self-correcting on the next
      // non-duplicate transition — so it is not worth sub-event bookkeeping.
      if let Some(k) = session.pending_reload_index.take() {
        if k < len && uri_matches_base62_id(&session.order[k], playing_base62_id) {
          session.index = k;
          return;
        }
      }
      // A manual skip carries its direction and always advances (even under
      // repeat-one); otherwise an auto replay stays put under repeat-one and a
      // normal auto-advance moves forward.
      let step = match session.pending_manual_skip.take() {
        Some(true) => ShuffleStep::Forward,
        Some(false) => ShuffleStep::Backward,
        None if repeat_one => ShuffleStep::Stay,
        None => ShuffleStep::Forward,
      };
      let Some(new_index) = shuffle_advance_index(&session.order, prev, playing_base62_id, step)
      else {
        return;
      };
      session.index = new_index;
      session.shuffled && len > 1 && prev == len - 1 && new_index == 0
    };
    if lap_wrapped && self.native_shuffle_repeat_mode() == crate::infra::queue::RepeatMode::Context
    {
      self.dispatch(IoEvent::ReshuffleNativeShuffleLap);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn track_uris(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| format!("spotify:track:{id}")).collect()
  }

  #[test]
  fn shuffle_advance_moves_onto_the_next_track_including_a_duplicate() {
    let order = track_uris(&["a", "b", "b", "c"]);
    // From "a" (0) the next "b" is the occurrence at index 1.
    assert_eq!(
      shuffle_advance_index(&order, 0, "b", ShuffleStep::Forward),
      Some(1)
    );
    // Playing the *next* "b" (a sequential advance from index 1) must resolve
    // to index 2, not stick on the current occurrence at index 1.
    assert_eq!(
      shuffle_advance_index(&order, 1, "b", ShuffleStep::Forward),
      Some(2)
    );
    assert_eq!(
      shuffle_advance_index(&order, 2, "c", ShuffleStep::Forward),
      Some(3)
    );
  }

  #[test]
  fn shuffle_advance_wraps_and_reports_missing_ids() {
    let order = track_uris(&["a", "b", "c"]);
    // Last -> first wrap (repeat-all lap).
    assert_eq!(
      shuffle_advance_index(&order, 2, "a", ShuffleStep::Forward),
      Some(0)
    );
    assert_eq!(
      shuffle_advance_index(&order, 1, "z", ShuffleStep::Forward),
      None
    );
    assert_eq!(
      shuffle_advance_index(&[], 0, "a", ShuffleStep::Forward),
      None
    );
  }

  #[test]
  fn shuffle_advance_stays_on_current_for_repeat_one() {
    let order = track_uris(&["a", "b", "a", "c"]);
    // Repeat-one replays index 0's "a" in place; it must not jump to the
    // duplicate "a" at index 2.
    assert_eq!(
      shuffle_advance_index(&order, 0, "a", ShuffleStep::Stay),
      Some(0)
    );
    // A unique track under repeat-one likewise keeps its index.
    assert_eq!(
      shuffle_advance_index(&order, 1, "b", ShuffleStep::Stay),
      Some(1)
    );
  }

  #[test]
  fn shuffle_advance_backward_resolves_the_previous_duplicate() {
    // Manual Previous through duplicates: from index 3 ("b"), Previous plays
    // "a" and must resolve to index 2 (the previous occurrence), not index 0.
    let order = track_uris(&["a", "b", "a", "b"]);
    assert_eq!(
      shuffle_advance_index(&order, 3, "a", ShuffleStep::Backward),
      Some(2)
    );
    // Forward from the same spot would instead find the earlier copy.
    assert_eq!(
      shuffle_advance_index(&order, 3, "a", ShuffleStep::Forward),
      Some(0)
    );
    // First -> last wrap going backward.
    assert_eq!(
      shuffle_advance_index(&track_uris(&["a", "b", "c"]), 0, "c", ShuffleStep::Backward),
      Some(2)
    );
  }

  #[test]
  fn base62_id_of_strips_the_uri_prefix() {
    assert_eq!(
      base62_id_of("spotify:track:6q02DsdnysAHuwmxFuI20c"),
      "6q02DsdnysAHuwmxFuI20c"
    );
    assert_eq!(base62_id_of("spotify:episode:abc"), "abc");
    // A bare id passes through unchanged.
    assert_eq!(
      base62_id_of("6q02DsdnysAHuwmxFuI20c"),
      "6q02DsdnysAHuwmxFuI20c"
    );
    // A Spotify local-track URI's only unique identity is the full string;
    // stripping to the last segment would collide tracks by duration.
    assert_eq!(
      base62_id_of("spotify:local:Artist:Album:Title:213"),
      "spotify:local:Artist:Album:Title:213"
    );
  }

  /// Regression for the frozen shuffle index: librespot 0.8's `TrackChanged`
  /// reports the full `spotify:track:<id>` URI, and feeding that through
  /// unnormalized made every order lookup miss, so `session.index` stayed at 0
  /// for the whole session. The event boundary now normalizes to the bare id;
  /// an auto advance must move the index to the playing track.
  #[cfg(feature = "streaming")]
  #[test]
  fn sync_native_shuffle_index_advances_on_auto_track_changed() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.native_spotify_shuffle = Some(NativeSpotifyShuffleSession {
      order: vec![
        "spotify:track:a".to_string(),
        "spotify:track:b".to_string(),
        "spotify:track:c".to_string(),
      ],
      original: Vec::new(),
      index: 0,
      shuffled: true,
      fetch_complete: true,
      fetch_failed: false,
      generation: 1,
      pending_reload_index: None,
      pending_manual_skip: None,
    });

    // What the event boundary hands over for librespot's full-URI form.
    let playing_uri = "spotify:track:b".to_string();
    app.sync_native_shuffle_index(base62_id_of(&playing_uri));

    assert_eq!(app.native_spotify_shuffle.as_ref().unwrap().index, 1);
  }
}
