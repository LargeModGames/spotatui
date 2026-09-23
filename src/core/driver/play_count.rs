//! The global song counter: counts plays of every source off the same
//! source-neutral snapshot the track-change detector reads. Only audio heard
//! counts: the position has to move forward past the furthest point of a play.

use crate::core::app::App;
use crate::infra::media_metadata::PlaybackSnapshot;
use crate::infra::network::IoEvent;

/// Play after which a track counts, the same rule Spotify uses for a stream.
const PLAY_THRESHOLD_MS: u128 = 30_000;
/// Live radio has no track boundaries, so it counts one song per this much play.
const RADIO_SONG_MS: u128 = 150_000;
/// A jump longer than this is a seek or a restage, not play.
const MAX_STEP_MS: u128 = 3_000;
/// A jump back to this near the top is a replay once it plays on past this point.
const RESTART_WITHIN_MS: u128 = 5_000;
/// Two ids of one recording (a relinked Spotify track) differ in length by no more than this.
const SAME_LENGTH_WITHIN_MS: u32 = 2_000;
/// Earlier plays kept, so a track resumed after queued ones is not counted again.
const HISTORY_LEN: usize = 32;

#[derive(Clone, Copy)]
struct PlayingItem<'a> {
  key: &'a str,
  title: &'a str,
  duration_ms: u32,
  progress_ms: u128,
  is_live: bool,
  playing: bool,
}

struct Play {
  key: String,
  title: String,
  duration_ms: u32,
  last_ms: u128,
  /// Furthest position of this pass. Only play past it is credited, so a
  /// position that jumps back and forth adds nothing.
  high_ms: u128,
  /// Where a jump back near the top landed, while it can still be a replay.
  rewind_ms: Option<u128>,
  played_ms: u128,
  counted: u32,
}

impl Play {
  fn new(item: &PlayingItem<'_>) -> Self {
    Play {
      key: item.key.to_string(),
      title: item.title.to_string(),
      duration_ms: item.duration_ms,
      last_ms: item.progress_ms,
      high_ms: item.progress_ms,
      rewind_ms: None,
      played_ms: 0,
      counted: 0,
    }
  }

  fn same_recording(&self, item: &PlayingItem<'_>) -> bool {
    self.title == item.title && self.duration_ms.abs_diff(item.duration_ms) <= SAME_LENGTH_WITHIN_MS
  }

  fn continues_at(&self, item: &PlayingItem<'_>) -> bool {
    self.last_ms.abs_diff(item.progress_ms) <= MAX_STEP_MS
  }

  /// Whether `item` is still this play: the same key, or a relinked id met at the same position.
  fn is_of(&self, item: &PlayingItem<'_>) -> bool {
    self.key == item.key || (self.same_recording(item) && self.continues_at(item))
  }

  /// Whether `item` picks this earlier play up where it was left.
  fn resumed_by(&self, item: &PlayingItem<'_>) -> bool {
    (self.key == item.key || self.same_recording(item)) && (item.is_live || self.continues_at(item))
  }

  fn at_end(&self, position: u128) -> bool {
    self.duration_ms > 0 && position + MAX_STEP_MS >= u128::from(self.duration_ms)
  }

  fn threshold_ms(&self) -> u128 {
    match u128::from(self.duration_ms) {
      0 => PLAY_THRESHOLD_MS,
      duration => PLAY_THRESHOLD_MS.min(duration / 2),
    }
  }

  /// Credits the move to `item`'s position and returns the plays that became due.
  fn advance(&mut self, item: &PlayingItem<'_>) -> u32 {
    let position = item.progress_ms;
    let last = std::mem::replace(&mut self.last_ms, position);
    if position < last {
      if last - position > MAX_STEP_MS {
        self.high_ms = position;
        self.rewind_ms = None;
        if !item.is_live && self.at_end(last) {
          self.played_ms = 0;
          self.counted = 0;
        } else if !item.is_live && position < RESTART_WITHIN_MS {
          self.rewind_ms = Some(position);
        }
      }
      return 0;
    }
    if position - last > MAX_STEP_MS {
      self.high_ms = self.high_ms.max(position);
      self.rewind_ms = None;
      return 0;
    }
    if let Some(start) = self.rewind_ms {
      self.high_ms = position;
      if position < RESTART_WITHIN_MS {
        return 0;
      }
      self.rewind_ms = None;
      self.counted = 0;
      self.played_ms = if item.playing { position - start } else { 0 };
    } else if item.playing && position > self.high_ms {
      self.played_ms += position - self.high_ms;
      self.high_ms = position;
    } else {
      self.high_ms = self.high_ms.max(position);
    }
    let due = if item.is_live {
      (self.played_ms / RADIO_SONG_MS) as u32
    } else {
      u32::from(self.played_ms >= self.threshold_ms())
    };
    let new = due.saturating_sub(self.counted);
    self.counted = self.counted.max(due);
    new
  }
}

#[derive(Default)]
pub(super) struct PlayCounter {
  current: Option<Play>,
  /// Earlier plays, newest last.
  history: Vec<Play>,
}

impl PlayCounter {
  /// Credits the play `snapshot` shows and sends the plays that became due.
  pub(super) fn tick(&mut self, app: &mut App, snapshot: Option<&PlaybackSnapshot>) {
    if app.decoded_change_pending() {
      return;
    }
    let enabled = app.user_config.behavior.enable_global_song_count;
    let item = snapshot.map(|snapshot| PlayingItem {
      key: snapshot
        .item_uri
        .as_deref()
        .unwrap_or(&snapshot.metadata.title),
      title: &snapshot.metadata.title,
      duration_ms: snapshot.metadata.duration_ms,
      progress_ms: snapshot.progress_ms,
      is_live: snapshot.is_live,
      playing: snapshot.is_playing && enabled,
    });
    for _ in 0..self.observe(item) {
      app.dispatch(IoEvent::IncrementGlobalSongCount);
    }
  }

  fn observe(&mut self, item: Option<PlayingItem<'_>>) -> u32 {
    let Some(item) = item else {
      return 0;
    };
    if !self.current.as_ref().is_some_and(|play| play.is_of(&item)) {
      self.switch_to(&item);
    }
    let play = self
      .current
      .as_mut()
      .expect("switch_to sets the current play");
    if play.key != item.key {
      play.key = item.key.to_string();
    }
    if play.title != item.title {
      play.title = item.title.to_string();
    }
    play.duration_ms = item.duration_ms;
    play.advance(&item)
  }

  fn switch_to(&mut self, item: &PlayingItem<'_>) {
    let resumed = self
      .history
      .iter()
      .rposition(|play| play.resumed_by(item))
      .map(|index| self.history.remove(index));
    let next = resumed.unwrap_or_else(|| Play::new(item));
    if let Some(previous) = self.current.replace(next) {
      if self.history.len() == HISTORY_LEN {
        self.history.remove(0);
      }
      self.history.push(previous);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{NativeTrackInfo, NativeTrackKind};
  use crate::core::user_config::UserConfig;
  use crate::infra::media_metadata::current_playback_snapshot;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::{Duration, SystemTime};

  const TICK_MS: u128 = 250;

  fn song(key: &'static str, title: &'static str, secs: u32) -> PlayingItem<'static> {
    PlayingItem {
      key,
      title,
      duration_ms: secs * 1000,
      progress_ms: 0,
      is_live: false,
      playing: true,
    }
  }

  fn radio() -> PlayingItem<'static> {
    PlayingItem {
      key: "radio:station",
      title: "Station",
      duration_ms: 0,
      progress_ms: 0,
      is_live: true,
      playing: true,
    }
  }

  fn at(item: PlayingItem<'static>, secs: u128) -> PlayingItem<'static> {
    PlayingItem {
      progress_ms: secs * 1000,
      ..item
    }
  }

  /// Observes `item` once per tick for `secs`, its position moving while it plays.
  fn listen(counter: &mut PlayCounter, item: PlayingItem<'_>, secs: u128) -> u32 {
    let mut item = item;
    let mut sent = 0;
    for _ in 0..secs * 1000 / TICK_MS {
      sent += counter.observe(Some(item));
      if item.playing {
        item.progress_ms += TICK_MS;
      }
    }
    sent
  }

  #[test]
  fn a_track_counts_once_after_thirty_seconds_of_play() {
    let mut counter = PlayCounter::default();
    let track = song("spotify:track:a", "A", 180);

    assert_eq!(listen(&mut counter, track, 29), 0);
    assert_eq!(listen(&mut counter, at(track, 29), 120), 1);
  }

  #[test]
  fn a_skip_before_thirty_seconds_does_not_count() {
    let mut counter = PlayCounter::default();

    let sent = listen(&mut counter, song("spotify:track:a", "A", 180), 20)
      + listen(&mut counter, song("spotify:track:b", "B", 180), 20);

    assert_eq!(sent, 0);
  }

  #[test]
  fn paused_time_does_not_count() {
    let mut counter = PlayCounter::default();
    let track = song("file:///a.flac", "A", 180);
    let paused = PlayingItem {
      playing: false,
      ..track
    };

    assert_eq!(listen(&mut counter, paused, 60), 0);
    assert_eq!(listen(&mut counter, track, 31), 1);
  }

  #[test]
  fn a_position_that_stands_still_adds_nothing() {
    let mut counter = PlayCounter::default();
    let track = song("youtube:a", "A", 180);

    let mut sent = 0;
    for _ in 0..600 {
      sent += counter.observe(Some(track));
    }
    sent += listen(&mut counter, track, 29);
    assert_eq!(sent, 0);
    assert_eq!(listen(&mut counter, at(track, 29), 2), 1);
  }

  #[test]
  fn a_position_that_jumps_back_and_forth_adds_nothing() {
    let mut counter = PlayCounter::default();
    let track = song("spotify:track:a", "A", 180);

    let mut sent = 0;
    for _ in 0..300 {
      for step in 0..5 {
        let item = PlayingItem {
          progress_ms: 10_000 + step * TICK_MS,
          ..track
        };
        sent += counter.observe(Some(item));
      }
    }

    assert_eq!(sent, 0);
  }

  #[test]
  fn a_seek_forward_is_not_play() {
    let mut counter = PlayCounter::default();
    let track = song("subsonic:a", "A", 180);

    counter.observe(Some(track));
    assert_eq!(listen(&mut counter, at(track, 120), 29), 0);
    assert_eq!(listen(&mut counter, at(track, 149), 2), 1);
  }

  #[test]
  fn a_short_track_counts_after_half_its_length() {
    let mut counter = PlayCounter::default();

    assert_eq!(
      listen(&mut counter, song("youtube:a", "Interlude", 20), 11),
      1
    );
  }

  #[test]
  fn a_restart_from_the_top_counts_again() {
    let mut counter = PlayCounter::default();
    let track = song("subsonic:a", "A", 180);

    assert_eq!(listen(&mut counter, track, 180), 1);
    assert_eq!(listen(&mut counter, track, 31), 1);
  }

  #[test]
  fn a_brief_zero_before_a_restage_is_not_a_replay() {
    let mut counter = PlayCounter::default();
    let track = song("file:///a.flac", "A", 180);

    let mut sent = listen(&mut counter, track, 90);
    sent += counter.observe(Some(track)) + counter.observe(Some(track));
    sent += listen(&mut counter, at(track, 90), 60);

    assert_eq!(sent, 1);
  }

  #[test]
  fn radio_counts_one_song_per_two_and_a_half_minutes() {
    let mut counter = PlayCounter::default();

    assert_eq!(listen(&mut counter, radio(), 149), 0);
    assert_eq!(listen(&mut counter, at(radio(), 149), 452), 4);
  }

  #[test]
  fn a_relinked_id_of_the_same_recording_stays_one_play() {
    let mut counter = PlayCounter::default();

    let sent = listen(&mut counter, song("spotify:track:a", "Song", 180), 40)
      + listen(
        &mut counter,
        at(song("spotify:track:relinked", "Song", 180), 40),
        40,
      );

    assert_eq!(sent, 1);
  }

  #[test]
  fn a_different_track_with_the_same_title_is_a_new_play() {
    let mut counter = PlayCounter::default();

    let sent = listen(
      &mut counter,
      song("spotify:track:a", "Hallelujah", 413),
      413,
    ) + listen(&mut counter, song("spotify:track:b", "Hallelujah", 279), 5);

    assert_eq!(sent, 1);
  }

  #[test]
  fn every_new_track_counts() {
    let mut counter = PlayCounter::default();

    let sent = listen(&mut counter, song("qobuz:a", "A", 180), 40)
      + listen(&mut counter, song("qobuz:b", "B", 180), 40);

    assert_eq!(sent, 2);
  }

  #[test]
  fn a_track_resumed_after_many_queued_ones_is_not_counted_again() {
    const QUEUED: [&str; 10] = [
      "youtube:q0",
      "youtube:q1",
      "youtube:q2",
      "youtube:q3",
      "youtube:q4",
      "youtube:q5",
      "youtube:q6",
      "youtube:q7",
      "youtube:q8",
      "youtube:q9",
    ];
    let mut counter = PlayCounter::default();
    let track = song("file:///a.flac", "A", 180);

    let mut sent = listen(&mut counter, track, 60);
    for key in QUEUED {
      sent += listen(&mut counter, song(key, key, 180), 40);
    }
    sent += listen(&mut counter, at(track, 60), 40);

    assert_eq!(sent, 11);
  }

  #[test]
  fn a_repeat_lap_first_seen_late_still_counts() {
    let mut counter = PlayCounter::default();
    let track = song("spotify:track:a", "A", 180);

    assert_eq!(listen(&mut counter, track, 180), 1);
    assert_eq!(listen(&mut counter, at(track, 8), 31), 1);
  }

  #[test]
  fn radio_time_carries_over_a_queued_song() {
    let mut counter = PlayCounter::default();

    let sent = listen(&mut counter, radio(), 120)
      + listen(&mut counter, song("youtube:q", "Queued", 180), 40)
      + listen(&mut counter, radio(), 31);

    assert_eq!(sent, 2);
  }

  #[test]
  fn a_gap_with_nothing_playing_keeps_the_current_play() {
    let mut counter = PlayCounter::default();
    let track = song("spotify:track:a", "A", 180);

    assert_eq!(listen(&mut counter, track, 40), 1);
    assert_eq!(counter.observe(None), 0);
    assert_eq!(listen(&mut counter, at(track, 40), 40), 0);
  }

  fn app_playing_natively() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.is_streaming_active = true;
    app.native_is_playing = Some(true);
    app.last_track_id = Some("4uLU6hMCjMI75M1A2tKUQC".to_string());
    app.native_track_info = Some(NativeTrackInfo {
      name: "Song".to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 180_000,
      kind: NativeTrackKind::Track,
      image_url: None,
    });
    (app, rx)
  }

  fn tick_for(counter: &mut PlayCounter, app: &mut App, time: Duration) {
    for _ in 0..time.as_millis() / TICK_MS {
      let snapshot = current_playback_snapshot(app);
      counter.tick(app, snapshot.as_ref());
      app.song_progress_ms += TICK_MS;
    }
  }

  fn counts_sent(rx: &Receiver<IoEvent>) -> usize {
    rx.try_iter()
      .filter(|event| matches!(event, IoEvent::IncrementGlobalSongCount))
      .count()
  }

  #[test]
  fn a_native_track_sends_one_count_after_thirty_seconds() {
    let (mut app, rx) = app_playing_natively();
    let mut counter = PlayCounter::default();

    tick_for(&mut counter, &mut app, Duration::from_secs(29));
    assert_eq!(counts_sent(&rx), 0);
    tick_for(&mut counter, &mut app, Duration::from_secs(60));
    assert_eq!(counts_sent(&rx), 1);
  }

  #[test]
  fn a_disabled_counter_sends_nothing() {
    let (mut app, rx) = app_playing_natively();
    app.user_config.behavior.enable_global_song_count = false;
    let mut counter = PlayCounter::default();

    tick_for(&mut counter, &mut app, Duration::from_secs(60));

    assert_eq!(counts_sent(&rx), 0);
  }

  #[test]
  fn time_before_the_counter_is_enabled_does_not_count() {
    let (mut app, rx) = app_playing_natively();
    app.user_config.behavior.enable_global_song_count = false;
    let mut counter = PlayCounter::default();

    tick_for(&mut counter, &mut app, Duration::from_secs(29));
    app.user_config.behavior.enable_global_song_count = true;
    tick_for(&mut counter, &mut app, Duration::from_secs(2));

    assert_eq!(counts_sent(&rx), 0);
  }
}
