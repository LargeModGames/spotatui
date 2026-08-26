//! Pure per-tick decisions, split out of [`super::Driver::tick`] in the house
//! style: scalars in, plain data out. Each one used to live inline in the
//! terminal event loop, where it was untestable without an audio sink, a
//! Spotify session, or a real clock; here every decision runs against fake
//! inputs. The driver probes live state (`player.is_finished()`, `Instant`),
//! asks these functions what to do, then applies the answer.

#[cfg(feature = "art-decode")]
use crate::core::art::CoverArtStatus;
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "qobuz",
  feature = "youtube"
))]
use crate::infra::queue::{advance_decision, Decision, RepeatMode};
use std::time::{Duration, Instant};

/// How often the active non-Spotify playback session is persisted to
/// `last_session.yml` while it keeps playing.
const SESSION_SAVE_INTERVAL: Duration = Duration::from_secs(3);

/// What the session persister should do this tick.
#[derive(Debug, PartialEq)]
pub(super) enum SessionPersist {
  /// A save is due: snapshot the session and write it.
  Save,
  /// The session just ended (Some -> None transition): clear the file so a
  /// stale session is never resurrected on the next launch.
  Clear,
  None,
}

/// Throttled `last_session.yml` persistence: save at most every
/// [`SESSION_SAVE_INTERVAL`] while a session is active (immediately on the
/// first tick that has one), and clear the file exactly once when it goes
/// away.
pub(super) fn session_persist_action(
  has_session: bool,
  was_present: bool,
  last_save: Option<Instant>,
  now: Instant,
) -> SessionPersist {
  if has_session {
    let due = last_save
      .map(|t| now.duration_since(t) >= SESSION_SAVE_INTERVAL)
      .unwrap_or(true);
    if due {
      SessionPersist::Save
    } else {
      SessionPersist::None
    }
  } else if was_present {
    SessionPersist::Clear
  } else {
    SessionPersist::None
  }
}

/// Whether the native queue slot's finished track should advance the queue.
/// `advancing` is the atomic check-and-set flag that guarantees one dispatch
/// per finished track: the sink stays empty for the whole decode/download of
/// the next item, so without it every tick in that window would re-dispatch.
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "qobuz",
  feature = "youtube"
))]
pub(super) fn native_queue_advance_due(finished: bool, advancing: bool) -> bool {
  finished && !advancing
}

/// What the tick does about a decoded-source session, as plain data so the
/// glue between [`advance_decision`] and the dispatch/suspend/teardown calls
/// is testable without a sink.
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "qobuz",
  feature = "youtube"
))]
#[derive(Debug, PartialEq)]
pub(super) enum DecodedAdvance {
  /// Mark the session advancing and dispatch the track change: replay the
  /// current track (repeat-one) or advance within the context.
  Dispatch {
    replay: bool,
  },
  /// Hand the sink to the native queue.
  SuspendToQueue,
  /// Context exhausted and the queue is empty: drop the session.
  Teardown,
  None,
}

/// Decoded-source auto-advance for one tick, composing [`advance_decision`]
/// (the shared end-of-track policy) with the action the driver takes on it.
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "qobuz",
  feature = "youtube"
))]
pub(super) fn decoded_advance(
  finished: bool,
  advancing: bool,
  has_next: bool,
  queue_len: usize,
  repeat: RepeatMode,
) -> DecodedAdvance {
  match advance_decision(finished, advancing, has_next, queue_len, repeat) {
    Decision::AdvanceContext => DecodedAdvance::Dispatch { replay: false },
    Decision::RepeatTrack => DecodedAdvance::Dispatch { replay: true },
    Decision::SuspendToQueue => DecodedAdvance::SuspendToQueue,
    Decision::Teardown => DecodedAdvance::Teardown,
    Decision::None => DecodedAdvance::None,
  }
}

/// What the per-tick cover-art evaluation should do. See the call site for why
/// this re-evaluates every tick against the desired key instead of latching to
/// the track identity.
#[cfg(feature = "art-decode")]
#[derive(Debug, PartialEq)]
pub(super) enum CoverArtAction {
  /// New art resolved: latch its key, mark it loading, dispatch the fetch.
  Fetch,
  /// Same art as already requested: leave everything alone.
  Keep,
  /// No art to show (radio, art disabled, nothing playing): drop any stale
  /// image once, then show `status` in the pane.
  Drop { clear: bool, status: CoverArtStatus },
}

#[cfg(feature = "art-decode")]
pub(super) fn cover_art_action(
  desired_key: Option<&str>,
  last_requested_key: Option<&str>,
  art_loaded: bool,
  enabled: bool,
  snapshot_present: bool,
) -> CoverArtAction {
  match desired_key {
    Some(key) => {
      if last_requested_key == Some(key) {
        CoverArtAction::Keep
      } else {
        CoverArtAction::Fetch
      }
    }
    None => CoverArtAction::Drop {
      clear: last_requested_key.is_some() || art_loaded,
      status: if enabled && snapshot_present {
        CoverArtStatus::Unavailable
      } else {
        CoverArtStatus::NotStarted
      },
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn first_tick_with_a_session_saves_immediately() {
    let now = Instant::now();
    assert_eq!(
      session_persist_action(true, false, None, now),
      SessionPersist::Save
    );
  }

  #[test]
  fn session_saves_are_throttled_to_the_interval() {
    let t0 = Instant::now();
    let just_after = t0 + Duration::from_secs(1);
    let past_interval = t0 + SESSION_SAVE_INTERVAL;

    assert_eq!(
      session_persist_action(true, true, Some(t0), just_after),
      SessionPersist::None
    );
    assert_eq!(
      session_persist_action(true, true, Some(t0), past_interval),
      SessionPersist::Save
    );
  }

  #[test]
  fn ending_a_session_clears_the_file_exactly_once() {
    let now = Instant::now();
    // Some -> None transition: clear.
    assert_eq!(
      session_persist_action(false, true, Some(now), now),
      SessionPersist::Clear
    );
    // Already cleared (was_present false): nothing left to do.
    assert_eq!(
      session_persist_action(false, false, None, now),
      SessionPersist::None
    );
  }

  #[cfg(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "qobuz",
    feature = "youtube"
  ))]
  mod decoded {
    use super::*;

    #[test]
    fn a_finished_queue_track_advances_the_queue_once() {
      assert!(native_queue_advance_due(true, false));
      // The advancing latch holds the whole decode/download window.
      assert!(!native_queue_advance_due(true, true));
      assert!(!native_queue_advance_due(false, false));
    }

    #[test]
    fn a_finished_track_with_a_next_advances_the_context() {
      assert_eq!(
        decoded_advance(true, false, true, 0, RepeatMode::Off),
        DecodedAdvance::Dispatch { replay: false }
      );
    }

    #[test]
    fn repeat_one_replays_instead_of_advancing() {
      assert_eq!(
        decoded_advance(true, false, true, 0, RepeatMode::Track),
        DecodedAdvance::Dispatch { replay: true }
      );
    }

    #[test]
    fn a_waiting_queue_preempts_the_context_advance() {
      // Even under repeat-one: a queued song must not consume the repeat.
      assert_eq!(
        decoded_advance(true, false, true, 1, RepeatMode::Track),
        DecodedAdvance::SuspendToQueue
      );
    }

    #[test]
    fn an_exhausted_context_with_an_empty_queue_tears_down() {
      assert_eq!(
        decoded_advance(true, false, false, 0, RepeatMode::Off),
        DecodedAdvance::Teardown
      );
    }

    #[test]
    fn an_advance_already_in_flight_dispatches_nothing() {
      // The sink is empty for the whole decode; without the latch every tick
      // in that window would dispatch another skip.
      assert_eq!(
        decoded_advance(true, true, true, 0, RepeatMode::Off),
        DecodedAdvance::None
      );
      assert_eq!(
        decoded_advance(false, false, true, 0, RepeatMode::Off),
        DecodedAdvance::None
      );
    }
  }

  #[cfg(feature = "art-decode")]
  mod cover_art {
    use super::*;

    #[test]
    fn newly_resolved_art_fetches_and_unchanged_art_keeps() {
      assert_eq!(
        cover_art_action(Some("url-a"), None, false, true, true),
        CoverArtAction::Fetch
      );
      assert_eq!(
        cover_art_action(Some("url-a"), Some("url-a"), true, true, true),
        CoverArtAction::Keep
      );
    }

    #[test]
    fn art_resolving_late_still_fetches() {
      // Native streaming: the snapshot's image_url catches up seconds after
      // the track identity flips, so the desired key appears (or changes)
      // ticks later. The key comparison, not an identity latch, must fire.
      assert_eq!(
        cover_art_action(None, None, false, true, true),
        CoverArtAction::Drop {
          clear: false,
          status: CoverArtStatus::Unavailable
        }
      );
      assert_eq!(
        cover_art_action(Some("real-url"), None, false, true, true),
        CoverArtAction::Fetch
      );
      assert_eq!(
        cover_art_action(Some("next-url"), Some("real-url"), true, true, true),
        CoverArtAction::Fetch
      );
    }

    #[test]
    fn losing_the_art_drops_the_stale_image_once() {
      // A track with art was playing; now radio (no art) is: clear once...
      assert_eq!(
        cover_art_action(None, Some("url-a"), true, true, true),
        CoverArtAction::Drop {
          clear: true,
          status: CoverArtStatus::Unavailable
        }
      );
      // ...and quiet ticks afterwards have nothing left to clear.
      assert_eq!(
        cover_art_action(None, None, false, true, true),
        CoverArtAction::Drop {
          clear: false,
          status: CoverArtStatus::Unavailable
        }
      );
    }

    #[test]
    fn disabled_art_and_silence_read_as_not_started() {
      assert_eq!(
        cover_art_action(None, Some("url-a"), true, false, true),
        CoverArtAction::Drop {
          clear: true,
          status: CoverArtStatus::NotStarted
        }
      );
      assert_eq!(
        cover_art_action(None, None, false, true, false),
        CoverArtAction::Drop {
          clear: false,
          status: CoverArtStatus::NotStarted
        }
      );
    }
  }
}
