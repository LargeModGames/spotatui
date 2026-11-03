use super::common_key_events;
use crate::core::app::{active_lyric_index, App, LyricsStatus};
use crate::tui::event::Key;
use std::time::Instant;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Char('f') => {
      app.view.lyrics_view.manual_index = None;
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
    k if k == app.user_config.keys.submit || k == Key::Enter => seek_to_focused(app),
    _ => {}
  }
}

/// Scroll the browsed line by `delta`, entering manual mode from the
/// currently playing line when auto-follow was active.
pub(super) fn scroll_by(app: &mut App, delta: i64) {
  if app.lyrics_status() != LyricsStatus::Found {
    return;
  }
  let Some(lyrics) = app.lyrics() else {
    return;
  };
  if lyrics.is_empty() {
    return;
  }
  let from = app
    .view
    .lyrics_view
    .manual_index
    .unwrap_or_else(|| active_lyric_index(lyrics, app.lyric_progress_ms()));
  let target = (from as i64 + delta).clamp(0, lyrics.len() as i64 - 1) as usize;
  app.view.lyrics_view.manual_index = Some(target);
  app.view.lyrics_view.last_manual_input = Some(Instant::now());
}

/// Shift lyric timing relative to playback, for correcting misaligned LRC
/// files. Positive delta shows lyrics earlier.
fn nudge_timing(app: &mut App, delta_ms: i64) {
  if app.lyrics_status() != LyricsStatus::Found {
    return;
  }
  app.view.lyrics_view.timing_offset_ms += delta_ms;
}

fn jump_to(app: &mut App, index: usize) {
  if app.lyrics_status() != LyricsStatus::Found {
    return;
  }
  let Some(lyrics) = app.lyrics() else {
    return;
  };
  if lyrics.is_empty() {
    return;
  }
  app.view.lyrics_view.manual_index = Some(index.min(lyrics.len() - 1));
  app.view.lyrics_view.last_manual_input = Some(Instant::now());
}

fn seek_to_focused(app: &mut App) {
  if app.lyrics_status() != LyricsStatus::Found || !app.lyrics_synced() {
    return;
  }
  let Some(lyrics) = app.lyrics() else {
    return;
  };
  if lyrics.is_empty() {
    return;
  }
  let target_index = app
    .view
    .lyrics_view
    .manual_index
    .unwrap_or_else(|| active_lyric_index(lyrics, app.lyric_progress_ms()))
    .min(lyrics.len() - 1);

  let target_time_ms = lyrics[target_index].0;
  let seek_ms = (target_time_ms as i128 - i128::from(app.view.lyrics_view.timing_offset_ms)).max(0);
  let target_u32 = u32::try_from(seek_ms).unwrap_or(u32::MAX);

  app.apply(crate::core::action::Action::SeekTo(target_u32));
  app.view.lyrics_view.manual_index = None;
}

#[cfg(test)]
mod tests {
  use super::*;

  fn playing_context() -> rspotify::model::CurrentPlaybackContext {
    use rspotify::model::{
      context::Actions, CurrentlyPlayingType, Device, DeviceType, PlayableItem, RepeatState,
    };
    rspotify::model::CurrentPlaybackContext {
      device: Device {
        id: Some("dev".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Test Device".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(50),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: chrono::Utc::now(),
      progress: Some(chrono::Duration::milliseconds(0)),
      is_playing: true,
      item: Some(PlayableItem::Track(crate::core::test_helpers::full_track(
        "4uLU6hMCjMI75M1A2tKUQC",
        "Test Song",
      ))),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  fn app_with_sample_lyrics() -> App {
    let mut app = App::default();
    app.current_playback_context = Some(playing_context());
    app.set_lyrics(
      LyricsStatus::Found,
      Some(vec![
        (0, "Line 0".to_string()),
        (5_000, "Line 1".to_string()),
        (10_000, "Line 2".to_string()),
      ]),
      true,
    );
    app
  }

  #[test]
  fn enter_seeks_to_browsed_lyric_and_resumes_follow() {
    let mut app = app_with_sample_lyrics();
    app.view.lyrics_view.timing_offset_ms = 500;
    // Browse to Line 2 (at 10_000 ms)
    scroll_by(&mut app, 2);
    assert_eq!(app.view.lyrics_view.manual_index, Some(2));

    handler(Key::Enter, &mut app);

    // 10_000 ms browsed lyric minus 500 ms timing offset
    assert_eq!(app.song_progress_ms, 9_500);
    // Manual browsing should be cleared (auto-follow resumes)
    assert_eq!(app.view.lyrics_view.manual_index, None);
  }

  #[test]
  fn enter_without_browsing_seeks_to_active_lyric() {
    let mut app = app_with_sample_lyrics();
    app.song_progress_ms = 6_000;
    assert_eq!(app.view.lyrics_view.manual_index, None);

    handler(Key::Enter, &mut app);

    // Active line at 6_000 ms is Line 1 (5_000 ms)
    assert_eq!(app.song_progress_ms, 5_000);
    assert_eq!(app.view.lyrics_view.manual_index, None);
  }

  #[test]
  fn enter_does_nothing_when_lyrics_unsynced() {
    let mut app = App::default();
    app.current_playback_context = Some(playing_context());
    app.song_progress_ms = 1_000;
    app.set_lyrics(
      LyricsStatus::Found,
      Some(vec![(0, "Unsynced text".to_string())]),
      false,
    );
    app.view.lyrics_view.manual_index = Some(0);

    handler(Key::Enter, &mut app);

    assert_eq!(app.song_progress_ms, 1_000);
    assert_eq!(app.view.lyrics_view.manual_index, Some(0));
  }

  #[test]
  fn enter_does_nothing_when_lyrics_not_found() {
    let mut app = App::default();
    app.current_playback_context = Some(playing_context());
    app.song_progress_ms = 1_000;
    app.set_lyrics(LyricsStatus::NotFound, None, false);

    handler(Key::Enter, &mut app);

    assert_eq!(app.song_progress_ms, 1_000);
  }
}
