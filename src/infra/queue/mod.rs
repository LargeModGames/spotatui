//! Native cross-source queue playback engine (Phase 2).
//!
//! Phase 1 built the queue *state* (`App::native_queue`, the Queue screen ops,
//! [`SuspendedContext`](crate::core::queue::SuspendedContext), persistence).
//! This module is the *engine* that consumes it: [`dispatch::route_queue_event`]
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
#[allow(dead_code)]
pub fn advance_decision(
  finished: bool,
  advancing: bool,
  has_next: bool,
  queue_len: usize,
) -> Decision {
  if !finished || advancing {
    return Decision::None;
  }
  if queue_len > 0 {
    return Decision::SuspendToQueue;
  }
  if has_next {
    Decision::AdvanceContext
  } else {
    Decision::Teardown
  }
}

/// A queued *decoded* track playing through the shared [`LocalPlayer`] sink
/// (local file, Subsonic, or YouTube). Kept separate from the per-source
/// `*_playback` structs so the underlying context is preserved for resume.
#[cfg(feature = "audio-decode")]
pub struct DecodedQueuePlayback {
  /// The output-device sink. Shared (`Arc::ptr_eq`) with the suspended context's
  /// player when there is one, so no second device is opened.
  pub player: std::sync::Arc<crate::infra::audio::LocalPlayer>,
  /// The queued track's metadata (drives the playbar / MPRIS / cover art).
  pub track: crate::core::plugin_api::TrackInfo,
  /// Guards the empty-sink window during a queue advance from being read as
  /// end-of-track by the runner tick (mirrors the per-source `advancing` guard).
  pub advancing: bool,
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
/// A slim build (neither native streaming nor a decoded source) cannot play a
/// queued track at all, so this type is gated to builds that can; the
/// `App::queue_now` field shares that gate, and every call site goes through the
/// unconditional `App::queue_owns_playback()` accessor.
#[cfg(any(feature = "streaming", feature = "audio-decode"))]
pub enum QueueNowPlaying {
  #[cfg(feature = "audio-decode")]
  Decoded(DecodedQueuePlayback),
  /// A Spotify track playing via native streaming (`player.load`, no Spirc
  /// context). Constructed in Phase 3; defined now so the enum is stable.
  #[cfg(feature = "streaming")]
  #[allow(dead_code)]
  Spotify {
    track: crate::core::plugin_api::TrackInfo,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn advance_decision_full_table() {
    // Not finished, or a change already in flight: never act.
    assert_eq!(advance_decision(false, false, true, 3), Decision::None);
    assert_eq!(advance_decision(false, true, false, 0), Decision::None);
    assert_eq!(advance_decision(true, true, true, 3), Decision::None);

    // Finished with a non-empty queue: always suspend to the queue, whether or
    // not the context has a next track.
    assert_eq!(
      advance_decision(true, false, true, 1),
      Decision::SuspendToQueue
    );
    assert_eq!(
      advance_decision(true, false, false, 1),
      Decision::SuspendToQueue
    );
    assert_eq!(
      advance_decision(true, false, true, 5),
      Decision::SuspendToQueue
    );

    // Finished with an empty queue: fall back to the source's own behavior.
    assert_eq!(
      advance_decision(true, false, true, 0),
      Decision::AdvanceContext
    );
    assert_eq!(advance_decision(true, false, false, 0), Decision::Teardown);
  }
}
