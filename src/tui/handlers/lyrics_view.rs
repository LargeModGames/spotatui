use super::common_key_events;
use crate::core::app::{active_lyric_index, App, LyricsStatus};
use crate::tui::event::Key;
use std::time::Instant;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Char('f') => {
      app.lyrics_view.manual_index = None;
    }
    k if k == app.user_config.keys.back => {
      app.pop_navigation_stack();
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => scroll_by(app, -1),
    k if common_key_events::down_event(k, &app.user_config.keys) => scroll_by(app, 1),
    k if common_key_events::left_event(k, &app.user_config.keys) => nudge_timing(app, -500),
    k if common_key_events::right_event(k, &app.user_config.keys) => nudge_timing(app, 500),
    k if k == app.user_config.keys.next_page => scroll_by(app, 5),
    k if k == app.user_config.keys.previous_page => scroll_by(app, -5),
    k if k == app.user_config.keys.jump_to_start => jump_to(app, 0),
    k if k == app.user_config.keys.jump_to_end => jump_to(app, usize::MAX),
    _ => {}
  }
}

/// Scroll the browsed line by `delta`, entering manual mode from the
/// currently playing line when auto-follow was active.
pub(super) fn scroll_by(app: &mut App, delta: i64) {
  if app.lyrics_status != LyricsStatus::Found {
    return;
  }
  let Some(lyrics) = &app.lyrics else {
    return;
  };
  if lyrics.is_empty() {
    return;
  }
  let from = app
    .lyrics_view
    .manual_index
    .unwrap_or_else(|| active_lyric_index(lyrics, app.lyric_progress_ms()));
  let target = (from as i64 + delta).clamp(0, lyrics.len() as i64 - 1) as usize;
  app.lyrics_view.manual_index = Some(target);
  app.lyrics_view.last_manual_input = Some(Instant::now());
}

/// Shift lyric timing relative to playback, for correcting misaligned LRC
/// files. Positive delta shows lyrics earlier.
fn nudge_timing(app: &mut App, delta_ms: i64) {
  if app.lyrics_status != LyricsStatus::Found {
    return;
  }
  app.lyrics_view.timing_offset_ms += delta_ms;
}

fn jump_to(app: &mut App, index: usize) {
  if app.lyrics_status != LyricsStatus::Found {
    return;
  }
  let Some(lyrics) = &app.lyrics else {
    return;
  };
  if lyrics.is_empty() {
    return;
  }
  app.lyrics_view.manual_index = Some(index.min(lyrics.len() - 1));
  app.lyrics_view.last_manual_input = Some(Instant::now());
}
