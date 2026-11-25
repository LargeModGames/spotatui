use std::time::Duration;

/// What a bounded wait on the queue should do next.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WaitDecision {
  /// The queue is at or under the threshold.
  Done,
  KeepWaiting,
  /// The device was removed.
  Lost,
  /// No progress for the whole stall limit.
  Stalled,
}

pub(super) fn wait_decision(
  queued: usize,
  threshold: usize,
  lost: bool,
  stalled_for: Duration,
  stall_limit: Duration,
) -> WaitDecision {
  if lost {
    WaitDecision::Lost
  } else if queued <= threshold {
    WaitDecision::Done
  } else if stalled_for >= stall_limit {
    WaitDecision::Stalled
  } else {
    WaitDecision::KeepWaiting
  }
}

/// Whether the system default output moved off the opened device. A name that
/// cannot be read is no evidence of a move.
pub(super) fn default_moved(opened: Option<&str>, current: Option<&str>) -> bool {
  matches!((opened, current), (Some(a), Some(b)) if a != b)
}

#[cfg(test)]
mod tests {
  use super::*;

  const LIMIT: Duration = Duration::from_secs(2);

  #[test]
  fn a_default_output_change_counts_only_when_both_names_read() {
    assert!(default_moved(Some("Speakers"), Some("Headphones")));
    assert!(!default_moved(Some("Speakers"), Some("Speakers")));
    assert!(!default_moved(Some("Speakers"), None));
    assert!(!default_moved(None, Some("Headphones")));
  }

  #[test]
  fn a_queue_under_the_threshold_is_done() {
    assert_eq!(
      wait_decision(5, 26, false, Duration::ZERO, LIMIT),
      WaitDecision::Done
    );
  }

  #[test]
  fn a_full_queue_keeps_waiting_while_progress_is_recent() {
    assert_eq!(
      wait_decision(30, 26, false, Duration::from_millis(500), LIMIT),
      WaitDecision::KeepWaiting
    );
  }

  #[test]
  fn a_removed_device_errors_at_once_even_with_a_full_queue() {
    assert_eq!(
      wait_decision(30, 26, true, Duration::ZERO, LIMIT),
      WaitDecision::Lost
    );
  }

  #[test]
  fn a_full_queue_with_no_progress_for_the_limit_stalls() {
    assert_eq!(
      wait_decision(30, 26, false, LIMIT, LIMIT),
      WaitDecision::Stalled
    );
  }

  #[test]
  fn an_empty_queue_is_done_for_the_stop_drain() {
    assert_eq!(
      wait_decision(0, 0, false, Duration::from_secs(9), LIMIT),
      WaitDecision::Done
    );
  }
}
