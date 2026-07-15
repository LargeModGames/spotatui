//! Native cross-source queue playback engine.
//!
//! [`App::native_queue`](crate::core::app::App::native_queue) holds the queue
//! state, while this module is the engine that consumes it:
//! [`dispatch::route_queue_event`]
//! plays queued tracks through the shared decoded-audio sink, overlaying the
//! per-source playback contexts without mutating them, and resumes the
//! underlying context once the queue drains.
//!
//! ## Playback slot vs. suspended context
//!
//! [`QueueNowPlaying`] is what the queue slot is *currently* playing. It never
//! touches the per-source `*_playback` structs — those are the context to
//! resume, recorded in `App::queue_suspended`. When the suspended context is a
//! decoded source, the queue slot **reuses that context's `Arc<LocalPlayer>`**
//! (the sink is reloaded with the queued track); only when Spotify or nothing is
//! suspended does it open a fresh player. This keeps the "never two live players
//! on one output device" invariant.

pub mod dispatch;

/// The runner-tick decision at a decoded source's auto-advance point, once the
/// native queue is in the picture.
///
/// Pure data so the full decision table is unit-testable without an audio
/// device. `#[allow(dead_code)]` because a slim build (no source features) never
/// calls it — clippy's slim run does not compile the tests that do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Decision {
  /// Nothing to do (still playing, or a track change is already in flight).
  None,
  /// Advance within the source's own context (the existing `NextTrack` path).
  AdvanceContext,
  /// Suspend the context and hand the sink to the native queue.
  SuspendToQueue,
  /// Context exhausted and the queue is empty: tear the session down.
  Teardown,
  /// Repeat-one is active: replay the current track instead of advancing.
  RepeatTrack,
}

/// Repeat mode for a decoded (non-Spotify) source's auto-advance / skip logic.
///
/// The canonical repeat state for the decoded sources (`App::decoded_repeat`),
/// kept source-neutral here so the pure queue/source modules never name an
/// rspotify type (translated to `RepeatState` only at the Spotify-shaped edges,
/// e.g. the media-metadata snapshot). `#[allow(dead_code)]` for the same reason
/// as [`Decision`]: a slim build never constructs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
pub enum RepeatMode {
  /// No repeat: advance then tear down at the end of the context.
  #[default]
  Off,
  /// Repeat the whole context: wrap around at the ends, never tear down.
  Context,
  /// Repeat the current track: replay it when it ends.
  Track,
}

impl RepeatMode {
  /// The next mode in the user-facing cycle, mirroring Spotify's
  /// Off -> Repeat All -> Repeat One -> Off.
  #[allow(dead_code)]
  pub fn next(self) -> Self {
    match self {
      RepeatMode::Off => RepeatMode::Context,
      RepeatMode::Context => RepeatMode::Track,
      RepeatMode::Track => RepeatMode::Off,
    }
  }
}

/// Decide what a decoded source should do when its current track ends.
///
/// - `finished` / `advancing`: the source's live end-of-track + in-flight guard.
/// - `has_next`: whether the source's own context has a following track.
/// - `queue_len`: the number of items waiting in the native queue.
///
/// The native queue takes priority: whenever it is non-empty and a track just
/// ended, the context is suspended (regardless of whether it has a next track;
/// `resume_index` is computed separately and is `None` when the context is
/// exhausted). Only with an empty queue does the source's own advance / teardown
/// behavior apply.
///
/// With an empty queue, `repeat` decides the fallback:
/// - `Track` — replay the current track ([`Decision::RepeatTrack`]).
/// - `Context` — always advance; the actual wrap-around to the first track is
///   handled by [`advance_index`] on the ensuing `NextTrack`, so this never
///   tears down.
/// - `Off` — advance if there is a next track, else tear down (the original
///   behavior).
#[allow(dead_code)]
pub fn advance_decision(
  finished: bool,
  advancing: bool,
  has_next: bool,
  queue_len: usize,
  repeat: RepeatMode,
) -> Decision {
  if !finished || advancing {
    return Decision::None;
  }
  if queue_len > 0 {
    return Decision::SuspendToQueue;
  }
  match repeat {
    RepeatMode::Track => Decision::RepeatTrack,
    RepeatMode::Context => Decision::AdvanceContext,
    RepeatMode::Off => {
      if has_next {
        Decision::AdvanceContext
      } else {
        Decision::Teardown
      }
    }
  }
}

/// The index to move to when skipping/advancing within a decoded source's queue.
///
/// `forward` selects Next (`true`) vs Previous (`false`). Under
/// [`RepeatMode::Context`] the ends wrap around (last→first, first→last); under
/// `Off`/`Track` the index clamps and returns `None` at a boundary (a manual
/// skip past the boundary is then a no-op — repeat-one only affects *auto*
/// advance, which replays via [`Decision::RepeatTrack`], never this helper).
#[allow(dead_code)]
pub fn advance_index(
  current: usize,
  len: usize,
  repeat: RepeatMode,
  forward: bool,
) -> Option<usize> {
  match repeat {
    RepeatMode::Context if len > 0 => Some(if forward {
      (current + 1) % len
    } else {
      (current + len - 1) % len
    }),
    // Off / Track / empty queue: clamp at the boundaries.
    _ => {
      if forward {
        if len == 0 || current + 1 >= len {
          None
        } else {
          Some(current + 1)
        }
      } else if len == 0 || current == 0 {
        None
      } else {
        Some(current - 1)
      }
    }
  }
}

/// Why a decoded context was handed off to the native queue, which is what
/// decides where it resumes once the queue drains.
///
/// The distinction only matters under [`RepeatMode::Track`], where the repo-wide
/// rule is that repeat-one affects *auto* advance but not a *manual* skip (see
/// [`advance_index`]). Both handoffs otherwise resume the context's next track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SuspendCause {
  /// The context's track ended on its own and the queue preempted the context's
  /// own advance.
  AutoAdvance,
  /// The user pressed Next while items were waiting in the queue.
  ManualSkip,
}

/// The index a decoded context should resume at once the native queue drains,
/// having been suspended with skip semantics (resume at position 0).
///
/// Differs from [`advance_index`] in exactly one case: an [`SuspendCause::AutoAdvance`]
/// handoff under [`RepeatMode::Track`] resumes *the same track*, because the
/// context is repeating it and a queued song must not consume the repeat.
/// [`advance_decision`] returns [`Decision::SuspendToQueue`] before it ever
/// consults `repeat`, so this is the only thing keeping repeat-one alive across
/// the queue: without it the repeated track is skipped, and on the last track
/// `advance_index` returns `None`, which reads as "context exhausted" and tears
/// the whole context down.
///
/// A [`SuspendCause::ManualSkip`] handoff advances even under repeat-one — the
/// user asked to move on, and the per-source skip paths treat repeat-one as a
/// normal clamp/advance. Every other mode defers to `advance_index` either way:
/// `Context` wraps last→first, `Off` clamps to `None` at the end.
#[allow(dead_code)]
pub fn resume_index_after_queue(
  current: usize,
  len: usize,
  repeat: RepeatMode,
  cause: SuspendCause,
) -> Option<usize> {
  match cause {
    // Replay the repeated track. Guard `current < len` so an out-of-range index
    // falls through to the clamping path instead of resuming a track that isn't
    // there.
    SuspendCause::AutoAdvance if repeat == RepeatMode::Track && current < len => Some(current),
    _ => advance_index(current, len, repeat, true),
  }
}

/// The index of the track after `current` in a queue of `len` tracks, clamped
/// at the end: [`advance_index`] under [`RepeatMode::Off`], going forward.
///
/// Returns `None` when `current` is already the last track (or the queue is
/// empty), signalling "no next track" — used to compute `has_next` for
/// [`advance_decision`] and the resume index when suspending to the native
/// queue (both of which ignore repeat wrap-around by design).
#[allow(dead_code)]
pub fn next_index(current: usize, len: usize) -> Option<usize> {
  advance_index(current, len, RepeatMode::Off, true)
}

/// The permutation applied by an in-place shuffle, letting a later un-shuffle
/// restore the exact original order and index. `perm[i]` is the original index
/// of the track now at shuffled position `i`, so restoring needs no equality
/// checks (correct even with duplicate tracks) and no copy of the queue.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
pub struct ShuffleBackup {
  perm: Vec<usize>,
}

impl ShuffleBackup {
  /// Whether `perm` is a complete permutation of `0..len`: exactly `len` entries,
  /// each in range and appearing once. A backup produced by [`shuffle_in_place`]
  /// is always valid, but one deserialized from a hand-edited or corrupted
  /// `last_session.yml` may not be — [`restore_in_place`] checks this before its
  /// unchecked indexing so a bad file degrades gracefully instead of panicking.
  #[allow(dead_code)]
  fn is_valid_for(&self, len: usize) -> bool {
    if self.perm.len() != len {
      return false;
    }
    let mut seen = vec![false; len];
    for &p in &self.perm {
      if p >= len || seen[p] {
        return false;
      }
      seen[p] = true;
    }
    true
  }
}

/// Turn in-place shuffle on or off for a decoded source's queue — the single
/// shared implementation behind every source's `set_shuffle`. Turning it on
/// reorders `items` keeping the track at `*index` at the front (`*index`
/// becomes `0`), so the currently-playing track is unaffected and playback
/// continues uninterrupted; turning it off restores the original order and
/// points `*index` back at the current track. Idempotent (a redundant on/off
/// is a no-op).
#[allow(dead_code)]
pub fn toggle_shuffle<T>(
  items: &mut Vec<T>,
  index: &mut usize,
  backup: &mut Option<ShuffleBackup>,
  on: bool,
) {
  if on {
    if backup.is_none() {
      *backup = Some(shuffle_in_place(items, *index));
      *index = 0;
    }
  } else if let Some(b) = backup.take() {
    *index = restore_in_place(items, &b, *index);
  }
}

/// Shuffle `items` in place, moving the track at `current` to the front and
/// randomizing the rest. This is the seam for a future native shuffle
/// algorithm: change the permutation built here and every source picks it up.
#[allow(dead_code)]
fn shuffle_in_place<T>(items: &mut Vec<T>, current: usize) -> ShuffleBackup {
  use rand::seq::SliceRandom;
  let len = items.len();
  // Permutation over original indices: move `current` to the front, shuffle the rest.
  let mut perm: Vec<usize> = (0..len).collect();
  if current < len {
    perm.swap(0, current);
  }
  if len > 1 {
    perm[1..].shuffle(&mut rand::rng());
  }
  // Reorder by moving items out of their old slots — no clones.
  let mut slots: Vec<Option<T>> = std::mem::take(items).into_iter().map(Some).collect();
  *items = perm
    .iter()
    .map(|&i| slots[i].take().expect("perm is a permutation"))
    .collect();
  ShuffleBackup { perm }
}

/// Invert the shuffle permutation in place. `current` is the live index in the
/// *shuffled* queue; the returned index is the same track's position in the
/// restored original order.
#[allow(dead_code)]
fn restore_in_place<T>(items: &mut Vec<T>, backup: &ShuffleBackup, current: usize) -> usize {
  if !backup.is_valid_for(items.len()) {
    // Either the queue was replaced/resized since the backup (shouldn't happen —
    // a new queue starts with a fresh backup) or the permutation was corrupted /
    // hand-edited in `last_session.yml`. Keep the current order rather than index
    // out of range or leave an unfilled slot for the `expect` below.
    return current;
  }
  let index = backup.perm.get(current).copied().unwrap_or(0);
  let mut slots: Vec<Option<T>> = (0..items.len()).map(|_| None).collect();
  for (i, item) in std::mem::take(items).into_iter().enumerate() {
    slots[backup.perm[i]] = Some(item);
  }
  *items = slots
    .into_iter()
    .map(|slot| slot.expect("perm is a permutation"))
    .collect();
  index
}

/// Re-decode `path` into `player`'s sink, off the async runtime (repeat-one
/// replay: a drained rodio sink cannot seek, so the audio is decoded again).
/// Returns whether playback restarted. On `false` the caller must tear its
/// session down — an empty sink left in place would read as end-of-track and
/// re-fire replay every runner tick.
///
/// Gated on the *queueable* decoded sources rather than `audio-decode`: replay
/// is the repeat-one path of a finite track list. Internet radio decodes audio
/// too, but a live stream has no track to re-decode.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
pub async fn replay_file(
  player: std::sync::Arc<crate::infra::audio::LocalPlayer>,
  path: std::path::PathBuf,
) -> bool {
  matches!(
    tokio::task::spawn_blocking(move || player.play_file(&path)).await,
    Ok(Ok(()))
  )
}

/// A queued *decoded* track playing through the shared [`LocalPlayer`] sink
/// (local file, Subsonic, or YouTube). Kept separate from the per-source
/// `*_playback` structs so the underlying context is preserved for resume.
///
/// Gated on exactly those three sources, not `audio-decode`: they are the ones
/// [`dispatch::try_play_queued`] can play. Internet radio pulls `audio-decode`
/// in as well, but a live stream is never a queue item, so a radio-only build
/// can never construct this.
#[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
pub struct DecodedQueuePlayback {
  /// The output-device sink. Shared (`Arc::ptr_eq`) with the suspended context's
  /// player when there is one, so no second device is opened.
  pub player: std::sync::Arc<crate::infra::audio::LocalPlayer>,
  /// The queued track's metadata (drives the playbar / MPRIS / cover art).
  pub track: crate::core::plugin_api::TrackInfo,
  /// Guards the empty-sink window during a queue advance from being read as
  /// end-of-track by the runner tick (mirrors the per-source `advancing` guard).
  pub advancing: bool,
  /// Generation stamp for the slot's background fetch. A Subsonic/YouTube
  /// download runs off the IoEvent pump (so it never blocks other events); its
  /// completion only plays and finalizes when the slot still carries the same
  /// stamp — a skip or teardown in the meantime republished the slot with a new
  /// one, and the stale result is silently discarded. Only *read* by the
  /// Subsonic/YouTube fetch-completion path, so a build with neither (e.g. a
  /// local-files-only build) writes it without reading it.
  #[cfg_attr(not(any(feature = "subsonic", feature = "youtube")), allow(dead_code))]
  pub fetch_id: u64,
  /// The tempfile backing a downloaded track (Subsonic / YouTube). `None` for a
  /// local file, which is played straight from disk. Held purely to keep the
  /// file alive on disk for the duration of playback (dropped with the slot), so
  /// it is never read back.
  #[cfg(any(feature = "subsonic", feature = "youtube"))]
  #[allow(dead_code)]
  pub tempfile: Option<tempfile::NamedTempFile>,
}

/// What the native queue's playback slot is currently playing.
///
/// A build with no *queueable* source (neither native streaming nor a decoded
/// source that owns a finite track list) cannot play a queued track at all, so
/// this type is gated to builds that can; the `App::queue_now` field shares that
/// gate, and every call site goes through the unconditional
/// `App::queue_owns_playback()` accessor. Note internet radio is deliberately
/// absent from both halves of the gate: it decodes audio, but a `radio:` URI is
/// never a queue item, so it can neither fill nor own this slot.
#[cfg(any(
  feature = "streaming",
  feature = "local-files",
  feature = "subsonic",
  feature = "youtube"
))]
pub enum QueueNowPlaying {
  #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
  Decoded(DecodedQueuePlayback),
  /// A Spotify track playing via native streaming (`player.load`, no Spirc
  /// context).
  #[cfg(feature = "streaming")]
  Spotify {
    track: crate::core::plugin_api::TrackInfo,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn advance_decision_full_table() {
    use RepeatMode::Off;
    // Not finished, or a change already in flight: never act.
    assert_eq!(advance_decision(false, false, true, 3, Off), Decision::None);
    assert_eq!(advance_decision(false, true, false, 0, Off), Decision::None);
    assert_eq!(advance_decision(true, true, true, 3, Off), Decision::None);

    // Finished with a non-empty queue: always suspend to the queue, whether or
    // not the context has a next track.
    assert_eq!(
      advance_decision(true, false, true, 1, Off),
      Decision::SuspendToQueue
    );
    assert_eq!(
      advance_decision(true, false, false, 1, Off),
      Decision::SuspendToQueue
    );
    assert_eq!(
      advance_decision(true, false, true, 5, Off),
      Decision::SuspendToQueue
    );

    // Finished with an empty queue: fall back to the source's own behavior.
    assert_eq!(
      advance_decision(true, false, true, 0, Off),
      Decision::AdvanceContext
    );
    assert_eq!(
      advance_decision(true, false, false, 0, Off),
      Decision::Teardown
    );
  }

  #[test]
  fn advance_decision_repeat_modes() {
    use RepeatMode::{Context, Track};
    // Repeat-one replays the current track whether or not a next track exists.
    assert_eq!(
      advance_decision(true, false, true, 0, Track),
      Decision::RepeatTrack
    );
    assert_eq!(
      advance_decision(true, false, false, 0, Track),
      Decision::RepeatTrack
    );
    // Repeat-context always advances (never tears down); the wrap is in advance_index.
    assert_eq!(
      advance_decision(true, false, true, 0, Context),
      Decision::AdvanceContext
    );
    assert_eq!(
      advance_decision(true, false, false, 0, Context),
      Decision::AdvanceContext
    );
    // The native queue keeps priority regardless of repeat mode.
    assert_eq!(
      advance_decision(true, false, false, 2, Track),
      Decision::SuspendToQueue
    );
    assert_eq!(
      advance_decision(true, false, false, 2, Context),
      Decision::SuspendToQueue
    );
    // The in-flight guard still short-circuits regardless of repeat mode.
    assert_eq!(advance_decision(true, true, true, 0, Track), Decision::None);
  }

  #[test]
  fn advance_index_clamps_off_and_track() {
    use RepeatMode::{Off, Track};
    for mode in [Off, Track] {
      // Forward clamps at the last track.
      assert_eq!(advance_index(1, 3, mode, true), Some(2));
      assert_eq!(advance_index(2, 3, mode, true), None);
      // Backward clamps at the first track.
      assert_eq!(advance_index(1, 3, mode, false), Some(0));
      assert_eq!(advance_index(0, 3, mode, false), None);
      // Empty queue.
      assert_eq!(advance_index(0, 0, mode, true), None);
      assert_eq!(advance_index(0, 0, mode, false), None);
    }
  }

  #[test]
  fn advance_index_wraps_context() {
    use RepeatMode::Context;
    // Forward wraps last -> first.
    assert_eq!(advance_index(2, 3, Context, true), Some(0));
    assert_eq!(advance_index(1, 3, Context, true), Some(2));
    // Backward wraps first -> last.
    assert_eq!(advance_index(0, 3, Context, false), Some(2));
    assert_eq!(advance_index(2, 3, Context, false), Some(1));
    // Single-track queue loops to itself; empty queue yields None.
    assert_eq!(advance_index(0, 1, Context, true), Some(0));
    assert_eq!(advance_index(0, 1, Context, false), Some(0));
    assert_eq!(advance_index(0, 0, Context, true), None);
  }

  #[test]
  fn auto_advance_handoff_replays_the_repeat_one_track() {
    use RepeatMode::Track;
    use SuspendCause::AutoAdvance;
    // Regression: a queued song preempts playback via `Decision::SuspendToQueue`
    // *before* `advance_decision` consults `repeat`, so the resume index is the
    // only thing keeping Repeat One alive across the queue. It must replay the
    // same track, not advance past it.
    assert_eq!(resume_index_after_queue(5, 10, Track, AutoAdvance), Some(5));
    // The boundary case that used to lose the context entirely: on the last
    // track, advancing would yield `None` and tear the whole context down.
    assert_eq!(resume_index_after_queue(9, 10, Track, AutoAdvance), Some(9));
    // Single-track context repeats itself.
    assert_eq!(resume_index_after_queue(0, 1, Track, AutoAdvance), Some(0));
    // Degenerate input still clamps rather than resuming a track that isn't there.
    assert_eq!(resume_index_after_queue(0, 0, Track, AutoAdvance), None);
    assert_eq!(resume_index_after_queue(7, 3, Track, AutoAdvance), None);
  }

  #[test]
  fn manual_skip_handoff_advances_even_under_repeat_one() {
    use RepeatMode::Track;
    use SuspendCause::ManualSkip;
    // Repeat-one only replays on *auto* advance: an explicit Next moves on, so
    // the suspended context must not resume the track the user just skipped.
    // The per-source skip paths (local/subsonic/youtube dispatch) already treat
    // repeat-one as a normal clamp/advance; this keeps the queue handoff in step
    // with them instead of resurrecting the skipped track once the queue drains.
    assert_eq!(resume_index_after_queue(5, 10, Track, ManualSkip), Some(6));
    // At the boundary a manual skip clamps, exactly like `advance_index`.
    assert_eq!(resume_index_after_queue(9, 10, Track, ManualSkip), None);
    assert_eq!(resume_index_after_queue(0, 1, Track, ManualSkip), None);
  }

  #[test]
  fn resume_after_queue_matches_advance_for_the_other_modes() {
    use RepeatMode::{Context, Off};
    // Outside repeat-one the cause makes no difference: both handoffs advance.
    for cause in [SuspendCause::AutoAdvance, SuspendCause::ManualSkip] {
      // Repeat All still wraps last -> first, so the context resumes rather than
      // reading as exhausted.
      assert_eq!(resume_index_after_queue(2, 3, Context, cause), Some(0));
      assert_eq!(resume_index_after_queue(0, 3, Context, cause), Some(1));
      // Off advances, and clamps to None at the end (context exhausted -> teardown).
      assert_eq!(resume_index_after_queue(0, 3, Off, cause), Some(1));
      assert_eq!(resume_index_after_queue(2, 3, Off, cause), None);
      assert_eq!(resume_index_after_queue(0, 0, Off, cause), None);
    }
  }

  #[test]
  fn repeat_mode_cycles_like_spotify() {
    // Off -> Repeat All -> Repeat One -> Off.
    assert_eq!(RepeatMode::Off.next(), RepeatMode::Context);
    assert_eq!(RepeatMode::Context.next(), RepeatMode::Track);
    assert_eq!(RepeatMode::Track.next(), RepeatMode::Off);
  }

  #[test]
  fn next_index_clamps_at_end_and_handles_empty() {
    // A 3-track queue: 0 -> 1 -> 2 -> (end).
    assert_eq!(next_index(0, 3), Some(1));
    assert_eq!(next_index(1, 3), Some(2));
    assert_eq!(
      next_index(2, 3),
      None,
      "advancing past the last track signals end-of-queue"
    );
    assert_eq!(next_index(0, 1), None, "single-track queue has no next");
    assert_eq!(next_index(0, 0), None, "empty queue has no next");
    // A defensively out-of-range index still yields None rather than panicking.
    assert_eq!(next_index(9, 3), None);
  }

  #[test]
  fn toggle_shuffle_round_trips() {
    let original = vec![10, 20, 30, 40, 50];
    let current = 2; // track "30" is playing

    let mut queue = original.clone();
    let mut index = current;
    let mut backup = None;
    toggle_shuffle(&mut queue, &mut index, &mut backup, true);

    // The result is a permutation of the input with the current track at the
    // front, and the live index follows it there.
    assert_eq!(index, 0);
    assert_eq!(queue[0], original[current]);
    let mut sorted = queue.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, original);

    // A redundant second toggle-on is a no-op (no re-shuffle).
    let shuffled = queue.clone();
    toggle_shuffle(&mut queue, &mut index, &mut backup, true);
    assert_eq!(queue, shuffled);

    // Un-shuffle restores the original order + the right index.
    toggle_shuffle(&mut queue, &mut index, &mut backup, false);
    assert_eq!(queue, original);
    assert_eq!(index, current);
    // ... and a redundant toggle-off stays a no-op.
    toggle_shuffle(&mut queue, &mut index, &mut backup, false);
    assert_eq!(queue, original);
    assert_eq!(index, current);
  }

  #[test]
  fn restore_ignores_a_corrupt_persisted_permutation() {
    // A `perm` deserialized from a hand-edited `last_session.yml` may have the
    // right length but an out-of-range or duplicate index. `restore_in_place`
    // (via `set_shuffle(false)`) must degrade to a no-op, never panic.
    let original = vec![10, 20, 30];
    for bad_perm in [
      vec![0, 3, 1], // 3 is out of range for len 3
      vec![0, 0, 1], // duplicate index, slot 2 left unfilled
      vec![0, 1],    // wrong length
    ] {
      let mut queue = original.clone();
      let mut index = 1;
      let mut backup = Some(ShuffleBackup { perm: bad_perm });
      toggle_shuffle(&mut queue, &mut index, &mut backup, false);
      // Degraded safely: order and index preserved, no panic.
      assert_eq!(queue, original);
      assert_eq!(index, 1);
    }
  }

  #[test]
  fn toggle_shuffle_restores_after_skipping() {
    let original = vec!['a', 'b', 'c', 'd'];
    let mut queue = original.clone();
    let mut index = 0;
    let mut backup = None;
    toggle_shuffle(&mut queue, &mut index, &mut backup, true);
    // Simulate the user skipping forward to shuffled position 2, then
    // un-shuffling: the restored index must point at whatever track sat at
    // shuffled slot 2 (duplicate-safe: positions, not equality).
    index = 2;
    let played = queue[2];
    toggle_shuffle(&mut queue, &mut index, &mut backup, false);
    assert_eq!(queue, original);
    assert_eq!(queue[index], played);
  }
}
