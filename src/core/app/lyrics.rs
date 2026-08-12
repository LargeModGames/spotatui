use super::*;

#[derive(Clone, PartialEq, Debug, Default)]
pub enum LyricsStatus {
  #[default]
  NotStarted,
  Loading,
  Found,
  NotFound,
}

/// UI state for the lyrics view: the eased scroll position plus the manual
/// browsing mode. The lyric data itself lives in `App::lyrics`.
#[derive(Default)]
pub struct LyricsViewState {
  /// Animated scroll position as a fractional lyric-line index.
  pub scroll_pos: f64,
  /// `Some(i)` while the user browses manually; auto-follow is paused and the
  /// view centers on line `i` until follow resumes.
  pub manual_index: Option<usize>,
  /// When the last manual scroll input happened, for the auto-resnap timeout.
  pub last_manual_input: Option<Instant>,
  /// User nudge applied to the playback position when matching lyric
  /// timestamps, for correcting misaligned LRC files. Positive shows lyrics
  /// earlier. Reset on track change.
  pub timing_offset_ms: i64,
  /// Position the current glide started from.
  anim_from: f64,
  /// Line index the current glide is heading to.
  anim_target: f64,
  /// Progress through the current glide, 0.0..=1.0.
  anim_progress: f64,
}

/// How long one scroll glide takes. Row steps quantize evenly across this
/// window thanks to the ease-in-out curve.
const LYRICS_SCROLL_ANIM: Duration = Duration::from_millis(300);

/// Symmetric acceleration/deceleration so the whole-row steps a terminal can
/// show land evenly spaced in time (a first-frame jump followed by a slow
/// tail reads as stutter, not smoothness).
pub(crate) fn ease_in_out_cubic(t: f64) -> f64 {
  if t < 0.5 {
    4.0 * t * t * t
  } else {
    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
  }
}

impl LyricsViewState {
  pub fn reset(&mut self) {
    *self = Self::default();
  }

  /// Glide the scroll position toward `target` over a fixed duration. A new
  /// target mid-glide restarts the curve from the current position, so
  /// motion stays continuous under rapid manual scrolling.
  pub fn ease_toward(&mut self, target: f64, elapsed: Duration) {
    if target != self.anim_target {
      self.anim_from = self.scroll_pos;
      self.anim_target = target;
      self.anim_progress = 0.0;
    }
    if self.anim_progress >= 1.0 {
      return;
    }
    self.anim_progress =
      (self.anim_progress + elapsed.as_secs_f64() / LYRICS_SCROLL_ANIM.as_secs_f64()).min(1.0);
    let eased = ease_in_out_cubic(self.anim_progress);
    self.scroll_pos = self.anim_from + (self.anim_target - self.anim_from) * eased;
  }

  /// Jump straight to `target` with no animation (used while the view is not
  /// visible so it opens already in position).
  pub fn snap_to(&mut self, target: f64) {
    self.scroll_pos = target;
    self.anim_from = target;
    self.anim_target = target;
    self.anim_progress = 1.0;
  }
}

/// Index of the lyric line currently playing: the last line whose timestamp is
/// at or before `progress_ms`.
pub fn active_lyric_index(lyrics: &[(u128, String)], progress_ms: u128) -> usize {
  let mut active_idx = 0;
  for (i, (time, _)) in lyrics.iter().enumerate() {
    if *time <= progress_ms {
      active_idx = i;
    } else {
      break;
    }
  }
  active_idx
}

impl App {
  /// Playback progress adjusted by the user's lyric timing nudge, for
  /// matching against lyric timestamps.
  pub fn lyric_progress_ms(&self) -> u128 {
    let adjusted = self.song_progress_ms as i128 + i128::from(self.lyrics_view.timing_offset_ms);
    adjusted.max(0) as u128
  }

  /// Advance the lyrics view scroll animation: glide toward the focused line
  /// while the view is open, snap when it is not, and drop back to
  /// auto-follow after the browsing timeout.
  pub(super) fn advance_lyrics_scroll(&mut self, elapsed: Duration) {
    const BROWSE_RESNAP_AFTER: Duration = Duration::from_secs(8);
    let Some(lyrics) = &self.lyrics else {
      return;
    };
    if lyrics.is_empty() {
      return;
    }
    if self.lyrics_view.manual_index.is_some()
      && self
        .lyrics_view
        .last_manual_input
        .is_some_and(|at| at.elapsed() >= BROWSE_RESNAP_AFTER)
    {
      self.lyrics_view.manual_index = None;
    }
    let active = active_lyric_index(lyrics, self.lyric_progress_ms());
    let target = self.lyrics_view.manual_index.unwrap_or(active) as f64;
    if self.get_current_route().id == RouteId::LyricsView {
      self.lyrics_view.ease_toward(target, elapsed);
    } else {
      self.lyrics_view.snap_to(target);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn active_lyric_index_picks_last_started_line() {
    let lyrics = vec![
      (1_000u128, "a".to_string()),
      (5_000, "b".to_string()),
      (10_000, "c".to_string()),
    ];
    assert_eq!(active_lyric_index(&lyrics, 0), 0);
    assert_eq!(active_lyric_index(&lyrics, 4_999), 0);
    assert_eq!(active_lyric_index(&lyrics, 5_000), 1);
    assert_eq!(active_lyric_index(&lyrics, 99_000), 2);
  }

  #[test]
  fn lyrics_scroll_eases_monotonically_and_snaps_on_target() {
    let mut state = LyricsViewState::default();
    let mut last = 0.0;
    for _ in 0..200 {
      state.ease_toward(10.0, Duration::from_millis(16));
      assert!(state.scroll_pos > last && state.scroll_pos <= 10.0);
      last = state.scroll_pos;
      if state.scroll_pos == 10.0 {
        break;
      }
    }
    assert_eq!(state.scroll_pos, 10.0, "easing never converged");
  }

  #[test]
  fn browsing_resnaps_to_follow_after_timeout() {
    let mut app = App::default();
    app.lyrics = Some(vec![(0, "a".to_string()), (5_000, "b".to_string())]);
    app.lyrics_status = LyricsStatus::Found;
    app.lyrics_view.manual_index = Some(1);
    app.lyrics_view.last_manual_input = Instant::now().checked_sub(Duration::from_secs(9));
    app.advance_lyrics_scroll(Duration::from_millis(16));
    assert_eq!(app.lyrics_view.manual_index, None);
  }

  #[test]
  fn lyric_progress_applies_offset_and_clamps_at_zero() {
    let mut app = App::default();
    app.song_progress_ms = 10_000;
    app.lyrics_view.timing_offset_ms = 2_500;
    assert_eq!(app.lyric_progress_ms(), 12_500);
    app.lyrics_view.timing_offset_ms = -15_000;
    assert_eq!(app.lyric_progress_ms(), 0);
  }

  #[test]
  fn browsing_persists_while_input_is_recent() {
    let mut app = App::default();
    app.lyrics = Some(vec![(0, "a".to_string()), (5_000, "b".to_string())]);
    app.lyrics_status = LyricsStatus::Found;
    app.lyrics_view.manual_index = Some(1);
    app.lyrics_view.last_manual_input = Some(Instant::now());
    app.advance_lyrics_scroll(Duration::from_millis(16));
    assert_eq!(app.lyrics_view.manual_index, Some(1));
  }
}
