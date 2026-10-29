use super::*;

/// The parts of Spotify's audio analysis that the pitch view draws, with times in ms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackAnalysis {
  pub sections: Vec<AnalysisSection>,
  pub segments: Vec<AnalysisSegment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisSection {
  pub start_ms: u32,
  pub tempo: f32,
  pub tempo_confidence: f32,
  /// The pitch class of the key, 0 (C) to 11 (B); `None` when no key was detected.
  pub key: Option<u8>,
  pub key_confidence: f32,
  /// `Some(true)` for minor, `Some(false)` for major, `None` when not detected.
  pub minor: Option<bool>,
  pub time_signature: u8,
  pub time_signature_confidence: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisSegment {
  pub start_ms: u32,
  /// Loudness in dB at the start.
  pub loudness_start: f32,
  /// Peak loudness in dB, `loudness_max_ms` after the start.
  pub loudness_max: f32,
  pub loudness_max_ms: u32,
  /// The strength of each pitch class, C to B, from 0 to 1.
  pub pitches: [f32; 12],
}

/// The audio analysis of the playing Spotify track, by base62 id.
#[derive(Debug, Default, PartialEq)]
pub enum AnalysisState {
  #[default]
  Idle,
  Loading(String),
  Ready(String, TrackAnalysis),
  /// Spotify has no analysis for the track.
  Unavailable(String),
  /// The fetch failed at this instant; it is tried again 10 s later.
  Failed(String, Instant),
}

/// What one fetch of an audio analysis found.
pub enum AnalysisFetch {
  Found(TrackAnalysis),
  Missing,
  Failed,
}

/// The wait before a failed fetch is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(10);

impl AnalysisState {
  fn track_id(&self) -> Option<&str> {
    match self {
      Self::Idle => None,
      Self::Loading(id) | Self::Ready(id, _) | Self::Unavailable(id) | Self::Failed(id, _) => {
        Some(id)
      }
    }
  }
}

/// Pitch class names, C to B.
pub const PITCH_NAMES: [&str; 12] = [
  "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// The loudness that the bars show as empty.
const QUIET_DB: f32 = -40.0;

/// The longest time the pitch view moves on past the last position update.
const MAX_CLOCK_AHEAD: Duration = Duration::from_millis(1500);

/// The time over which a position correction is blended in.
const BLEND: Duration = Duration::from_millis(400);

/// A larger correction is a seek or a new track and applies at once.
const MAX_BLEND_MS: f64 = 1000.0;

/// How fast a level rises toward a higher target, per second.
const RISE_RATE: f32 = 35.0;

/// The fall acceleration of a level, in full heights per second squared.
const GRAVITY: f32 = 4.0;

/// The pitch view's clock: the last position update, and the correction it blends in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AnalysisClock {
  base: u128,
  start_ms: f64,
  error_ms: f64,
  at: Instant,
  playing: bool,
}

impl AnalysisClock {
  fn position_ms(&self, now: Instant) -> f64 {
    if !self.playing {
      return self.start_ms + self.error_ms;
    }
    let elapsed = now.saturating_duration_since(self.at);
    let ahead = elapsed.min(MAX_CLOCK_AHEAD).as_secs_f64() * 1000.0;
    let blend = (elapsed.as_secs_f64() / BLEND.as_secs_f64()).min(1.0);
    self.start_ms + ahead + self.error_ms * blend
  }
}

/// The last item in `items` (sorted by `start`) that started at or before `position_ms`.
fn started<T>(items: &[T], position_ms: u32, start: impl Fn(&T) -> u32) -> Option<&T> {
  let count = items.partition_point(|item| start(item) <= position_ms);
  count.checked_sub(1).map(|index| &items[index])
}

/// How far `position_ms` is from `before` to `after`, 0 to 1.
fn progress(position_ms: u32, before: u32, after: u32) -> f32 {
  if after > before {
    position_ms.saturating_sub(before).min(after - before) as f32 / (after - before) as f32
  } else {
    1.0
  }
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
  from + (to - from) * t
}

/// The loudness in dB at `position_ms`: up to the peak of the segment, then down to the next one.
fn loudness_at(current: &AnalysisSegment, next: Option<&AnalysisSegment>, position_ms: u32) -> f32 {
  let peak_ms = current.start_ms.saturating_add(current.loudness_max_ms);
  if position_ms < peak_ms {
    let t = progress(position_ms, current.start_ms, peak_ms);
    return lerp(current.loudness_start, current.loudness_max, t);
  }
  match next {
    Some(next) => {
      let t = progress(position_ms, peak_ms, next.start_ms);
      lerp(current.loudness_max, next.loudness_start, t)
    }
    None => current.loudness_max,
  }
}

/// The bars at `position_ms`: the pitch shape gliding between segments, scaled by the loudness.
pub fn pitch_bars(analysis: &TrackAnalysis, position_ms: u32) -> [f32; 12] {
  let segments = &analysis.segments;
  let index = segments.partition_point(|segment| segment.start_ms <= position_ms);
  let Some(current) = index.checked_sub(1).map(|index| &segments[index]) else {
    return [0.0; 12];
  };
  let next = segments.get(index);
  let pitches = match next {
    Some(next) => {
      let t = progress(position_ms, current.start_ms, next.start_ms);
      std::array::from_fn(|k| lerp(current.pitches[k], next.pitches[k], t))
    }
    None => current.pitches,
  };
  let level = ((loudness_at(current, next, position_ms) - QUIET_DB) / -QUIET_DB).clamp(0.0, 1.0);
  pitches.map(|pitch| pitch.clamp(0.0, 1.0) * level)
}

/// The section at `position_ms`; the first one before it starts.
pub fn section_at(analysis: &TrackAnalysis, position_ms: u32) -> Option<&AnalysisSection> {
  started(&analysis.sections, position_ms, |section| section.start_ms).or(analysis.sections.first())
}

/// "F# minor", "C major", or "?" when no key was detected.
#[cfg(test)]
pub fn key_label(key: Option<u8>, minor: Option<bool>) -> String {
  let Some(name) = key.and_then(|key| PITCH_NAMES.get(usize::from(key))) else {
    return "?".to_string();
  };
  match minor {
    Some(true) => format!("{name} minor"),
    Some(false) => format!("{name} major"),
    None => name.to_string(),
  }
}

impl App {
  pub fn audio_analysis(&self) -> &AnalysisState {
    &self.audio_analysis
  }

  /// Whether the analysis screen shows the Spotify pitch view instead of a capture style.
  pub fn pitch_view_shows(&self) -> bool {
    self.pitch_view_available() && !self.view.pitch_dismissed
  }

  /// Whether `V` can pick the pitch view: the analysis screen or its pane shows and a Spotify track plays.
  pub(crate) fn pitch_view_available(&self) -> bool {
    self.spotify_track_playing
      && (self.get_current_route().active_block == ActiveBlock::Analysis
        || self.pane_open(Pane::Visualizer))
  }

  /// The tick's decision for the playing Spotify track; an analysis stays while its track plays.
  pub(crate) fn sync_audio_analysis(&mut self, track: Option<&str>) {
    self.spotify_track_playing = track.is_some();
    if self.pitch_view_shows() {
      self.want_audio_analysis(track);
    } else if self.audio_analysis.track_id() != track {
      self.want_audio_analysis(None);
    }
  }

  /// Starts the clock from a new position: a small correction is blended in, a seek is not.
  pub(crate) fn note_analysis_clock(&mut self, playing: bool, now: Instant) {
    let base = self.song_progress_ms;
    if self
      .analysis_clock
      .is_some_and(|clock| clock.base == base && clock.playing == playing)
    {
      return;
    }
    let predicted = self
      .analysis_clock
      .filter(|clock| clock.playing && playing)
      .map(|clock| clock.position_ms(now));
    let (start_ms, error_ms) = match predicted {
      Some(predicted) if (base as f64 - predicted).abs() < MAX_BLEND_MS => {
        (predicted, base as f64 - predicted)
      }
      _ => (base as f64, 0.0),
    };
    self.analysis_clock = Some(AnalysisClock {
      base,
      start_ms,
      error_ms,
      at: now,
      playing,
    });
  }

  /// The position for the pitch view, moved on between the native player's 1 s updates.
  pub fn analysis_position_ms(&self, now: Instant) -> Option<u32> {
    let base = self.playback_position_ms()?;
    let position = match self.analysis_clock {
      Some(clock) if clock.base == base => clock.position_ms(now),
      _ => base as f64,
    };
    Some(position.max(0.0).round() as u32)
  }

  /// Moves the shown levels toward the analysis as cava moves its bars: up fast, down with gravity.
  pub(crate) fn step_pitch_levels(&mut self, now: Instant, elapsed: Duration) {
    let target = match (&self.audio_analysis, self.analysis_position_ms(now)) {
      (AnalysisState::Ready(_, analysis), Some(position)) => pitch_bars(analysis, position),
      _ => [0.0; 12],
    };
    let dt = elapsed.as_secs_f32().min(0.1);
    let rise = 1.0 - (-RISE_RATE * dt).exp();
    for ((level, fall), goal) in self
      .pitch_levels
      .iter_mut()
      .zip(self.pitch_fall.iter_mut())
      .zip(target)
    {
      if goal >= *level {
        *level += (goal - *level) * rise;
        *fall = 0.0;
      } else {
        *fall += GRAVITY * dt;
        *level = (*level - *fall * dt).max(goal);
      }
    }
  }

  pub fn pitch_levels(&self) -> [f32; 12] {
    self.pitch_levels
  }

  /// Fetches the analysis of `track_id` once, or again after a failure; `None` drops it.
  pub(crate) fn want_audio_analysis(&mut self, track_id: Option<&str>) {
    let Some(id) = track_id else {
      self.audio_analysis = AnalysisState::Idle;
      return;
    };
    let retry_due = matches!(
      self.audio_analysis,
      AnalysisState::Failed(_, at) if at.elapsed() >= RETRY_AFTER
    );
    if self.audio_analysis.track_id() == Some(id) && !retry_due {
      return;
    }
    self.audio_analysis = AnalysisState::Loading(id.to_string());
    self.dispatch_without_spinner(IoEvent::GetAudioAnalysis(id.to_string()));
  }

  pub(crate) fn audio_analysis_pending(&self, track_id: &str) -> bool {
    matches!(&self.audio_analysis, AnalysisState::Loading(id) if id == track_id)
  }

  /// Stores a fetch result, unless another track plays by now.
  pub(crate) fn set_audio_analysis(&mut self, track_id: &str, fetch: AnalysisFetch) {
    if !self.audio_analysis_pending(track_id) {
      return;
    }
    let id = track_id.to_string();
    self.audio_analysis = match fetch {
      AnalysisFetch::Found(analysis) => AnalysisState::Ready(id, analysis),
      AnalysisFetch::Missing => AnalysisState::Unavailable(id),
      AnalysisFetch::Failed => AnalysisState::Failed(id, Instant::now()),
    };
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::mpsc::{channel, Receiver};

  fn app_with_events() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    (App::new(tx, UserConfig::new(), None), rx)
  }

  fn app_on_the_analysis_screen() -> (App, Receiver<IoEvent>) {
    let (mut app, rx) = app_with_events();
    app.get_audio_analysis();
    (app, rx)
  }

  #[test]
  fn wanting_a_track_fetches_its_analysis_once() {
    let (mut app, rx) = app_with_events();

    app.want_audio_analysis(Some("track1"));
    app.want_audio_analysis(Some("track1"));

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAudioAnalysis(id)) if id == "track1"));
    assert!(rx.try_recv().is_err());
    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Loading("track1".into())
    );
  }

  #[test]
  fn a_track_without_an_analysis_is_not_fetched_again() {
    let (mut app, rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));
    let _ = rx.try_recv();

    app.set_audio_analysis("track1", AnalysisFetch::Missing);
    app.want_audio_analysis(Some("track1"));

    assert!(rx.try_recv().is_err());
    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Unavailable("track1".into())
    );
  }

  #[test]
  fn a_failed_fetch_is_tried_again_only_after_the_wait() {
    let (mut app, rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));
    app.set_audio_analysis("track1", AnalysisFetch::Failed);
    let _ = rx.try_recv();

    app.want_audio_analysis(Some("track1"));
    assert!(rx.try_recv().is_err());

    app.audio_analysis = AnalysisState::Failed("track1".into(), Instant::now() - RETRY_AFTER);
    app.want_audio_analysis(Some("track1"));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAudioAnalysis(_))));
  }

  #[test]
  fn a_reply_for_a_track_no_longer_wanted_is_dropped() {
    let (mut app, _rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));
    app.want_audio_analysis(Some("track2"));

    app.set_audio_analysis("track1", AnalysisFetch::Found(TrackAnalysis::default()));

    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Loading("track2".into())
    );
  }

  #[test]
  fn a_reply_for_the_wanted_track_is_stored() {
    let (mut app, _rx) = app_with_events();
    app.want_audio_analysis(Some("track1"));

    app.set_audio_analysis("track1", AnalysisFetch::Found(TrackAnalysis::default()));

    assert_eq!(
      *app.audio_analysis(),
      AnalysisState::Ready("track1".into(), TrackAnalysis::default())
    );
  }

  fn segment(start_ms: u32, loudness: (f32, f32, u32), first_pitch: f32) -> AnalysisSegment {
    let mut pitches = [0.0; 12];
    pitches[0] = first_pitch;
    AnalysisSegment {
      start_ms,
      loudness_start: loudness.0,
      loudness_max: loudness.1,
      loudness_max_ms: loudness.2,
      pitches,
    }
  }

  fn section(start_ms: u32, tempo: f32) -> AnalysisSection {
    AnalysisSection {
      start_ms,
      tempo,
      tempo_confidence: 1.0,
      key: Some(0),
      key_confidence: 1.0,
      minor: Some(false),
      time_signature: 4,
      time_signature_confidence: 1.0,
    }
  }

  /// Two segments: a quiet one (-40 dB to a -20 dB peak at 100 ms) and a loud one at 0 dB.
  fn analysis() -> TrackAnalysis {
    TrackAnalysis {
      sections: vec![section(0, 100.0), section(10_000, 120.0)],
      segments: vec![
        segment(1000, (-40.0, -20.0, 100), 0.5),
        segment(2000, (0.0, 0.0, 0), 1.0),
      ],
    }
  }

  fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
  }

  #[test]
  fn pitch_bars_rise_to_the_loudness_peak_of_the_segment() {
    let analysis = analysis();
    assert_close(pitch_bars(&analysis, 1000)[0], 0.0);
    // The peak at -20 dB is half of the scale; the pitch glided 10% toward 1.0.
    assert_close(pitch_bars(&analysis, 1100)[0], 0.55 * 0.5);
  }

  #[test]
  fn pitch_bars_follow_the_loudness_and_pitch_toward_the_next_segment() {
    // Halfway from the peak to the next segment (-10 dB); the pitch glided 55% toward 1.0.
    assert_close(pitch_bars(&analysis(), 1550)[0], 0.775 * 0.75);
  }

  #[test]
  fn pitch_bars_never_jump_inside_the_track() {
    let analysis = analysis();
    for at in [1100, 1500, 2000] {
      let step = pitch_bars(&analysis, at)[0] - pitch_bars(&analysis, at - 1)[0];
      assert!(step.abs() < 0.01, "jump of {step} at {at} ms");
    }
  }

  #[test]
  fn pitch_bars_are_empty_before_the_first_segment() {
    assert_eq!(pitch_bars(&analysis(), 500), [0.0; 12]);
  }

  #[test]
  fn the_section_follows_the_position_and_the_last_one_holds_to_the_end() {
    let analysis = analysis();
    assert_eq!(section_at(&analysis, 9_999).unwrap().tempo, 100.0);
    assert_eq!(section_at(&analysis, 500_000).unwrap().tempo, 120.0);
  }

  #[test]
  fn an_undetected_key_reads_as_unknown_not_c() {
    assert_eq!(key_label(None, Some(true)), "?");
    assert_eq!(key_label(Some(6), Some(true)), "F# minor");
    assert_eq!(key_label(Some(0), Some(false)), "C major");
  }

  #[test]
  fn a_spotify_track_gets_the_pitch_view_on_the_analysis_screen() {
    let (mut app, rx) = app_with_events();

    app.sync_audio_analysis(Some("track1"));
    assert!(!app.pitch_view_shows());
    assert!(rx.try_recv().is_err());

    app.get_audio_analysis();
    assert!(app.pitch_view_shows());
    app.sync_audio_analysis(Some("track1"));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAudioAnalysis(_))));
  }

  #[test]
  fn an_analysis_stays_while_its_track_plays_and_goes_when_the_track_changes() {
    let (mut app, _rx) = app_on_the_analysis_screen();
    app.sync_audio_analysis(Some("track1"));
    app.set_audio_analysis("track1", AnalysisFetch::Found(TrackAnalysis::default()));

    app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
    app.sync_audio_analysis(Some("track1"));
    assert!(matches!(app.audio_analysis(), AnalysisState::Ready(..)));

    app.sync_audio_analysis(Some("track2"));
    assert_eq!(*app.audio_analysis(), AnalysisState::Idle);
  }

  #[test]
  fn v_cycles_every_style_and_the_pitch_view_while_a_spotify_track_plays() {
    let (mut app, _rx) = app_on_the_analysis_screen();
    app.sync_audio_analysis(Some("track1"));

    app.cycle_visualizer_style();
    assert!(!app.pitch_view_shows());
    assert_eq!(
      app.user_config.behavior.visualizer_style,
      VisualizerStyle::Cava
    );

    app.cycle_visualizer_style();
    assert_eq!(
      app.user_config.behavior.visualizer_style,
      VisualizerStyle::BarGraph
    );

    for style in &VisualizerStyle::all()[2..] {
      app.cycle_visualizer_style();
      assert_eq!(app.user_config.behavior.visualizer_style, *style);
    }

    app.cycle_visualizer_style();
    assert!(app.pitch_view_shows());
    assert_eq!(
      app.user_config.behavior.visualizer_style,
      VisualizerStyle::Cassette
    );
  }

  #[test]
  fn v_skips_the_pitch_view_without_a_spotify_track() {
    let (mut app, _rx) = app_on_the_analysis_screen();
    app.user_config.behavior.visualizer_style = VisualizerStyle::Cassette;
    app.sync_audio_analysis(None);

    app.cycle_visualizer_style();

    assert_eq!(
      app.user_config.behavior.visualizer_style,
      VisualizerStyle::Cava
    );
    assert!(!app.view.pitch_dismissed);
  }

  #[test]
  fn the_style_action_still_cycles_away_from_the_analysis_screen() {
    let (mut app, _rx) = app_with_events();
    app.user_config.behavior.visualizer_style = VisualizerStyle::Cassette;
    app.sync_audio_analysis(Some("track1"));

    app.cycle_visualizer_style();

    assert_eq!(
      app.user_config.behavior.visualizer_style,
      VisualizerStyle::Cava
    );
  }

  #[test]
  fn reopening_the_analysis_screen_brings_the_pitch_view_back() {
    let (mut app, _rx) = app_on_the_analysis_screen();
    app.sync_audio_analysis(Some("track1"));
    app.cycle_visualizer_style();
    app.pop_navigation_stack();

    app.get_audio_analysis();

    assert!(app.pitch_view_shows());
  }

  #[test]
  fn analysis_position_moves_on_between_native_position_updates() {
    let (mut app, _rx) = app_with_events();
    let start = Instant::now();
    app.song_progress_ms = 10_000;
    app.note_analysis_clock(true, start);

    assert_eq!(
      app.analysis_position_ms(start + Duration::from_millis(400)),
      Some(10_400)
    );
    assert_eq!(
      app.analysis_position_ms(start + Duration::from_secs(5)),
      Some(11_500)
    );
  }

  #[test]
  fn a_small_position_correction_is_blended_in_instead_of_jumped() {
    let (mut app, _rx) = app_with_events();
    let start = Instant::now();
    app.song_progress_ms = 10_000;
    app.note_analysis_clock(true, start);
    let update = start + Duration::from_secs(1);
    app.song_progress_ms = 11_150;
    app.note_analysis_clock(true, update);

    assert_eq!(app.analysis_position_ms(update), Some(11_000));
    assert_eq!(app.analysis_position_ms(update + BLEND), Some(11_550));
  }

  #[test]
  fn a_seek_moves_the_clock_at_once() {
    let (mut app, _rx) = app_with_events();
    let start = Instant::now();
    app.song_progress_ms = 10_000;
    app.note_analysis_clock(true, start);
    let seek = start + Duration::from_secs(1);
    app.song_progress_ms = 60_000;
    app.note_analysis_clock(true, seek);

    assert_eq!(app.analysis_position_ms(seek), Some(60_000));
  }

  #[test]
  fn pitch_levels_rise_fast_and_fall_with_gravity() {
    let (mut app, _rx) = app_with_events();
    let loud = segment(0, (0.0, 0.0, 0), 1.0);
    app.audio_analysis = AnalysisState::Ready(
      "track1".into(),
      TrackAnalysis {
        sections: Vec::new(),
        segments: vec![loud],
      },
    );
    let tick = Duration::from_millis(16);
    for _ in 0..6 {
      app.step_pitch_levels(Instant::now(), tick);
    }
    assert!(app.pitch_levels()[0] > 0.9, "{:?}", app.pitch_levels());

    app.audio_analysis = AnalysisState::Idle;
    app.step_pitch_levels(Instant::now(), tick);
    assert!(app.pitch_levels()[0] > 0.9, "{:?}", app.pitch_levels());

    for _ in 0..50 {
      app.step_pitch_levels(Instant::now(), tick);
    }
    assert_eq!(app.pitch_levels()[0], 0.0);
  }

  #[test]
  fn analysis_position_holds_while_paused() {
    let (mut app, _rx) = app_with_events();
    let start = Instant::now();
    app.song_progress_ms = 10_000;
    app.note_analysis_clock(false, start);

    assert_eq!(
      app.analysis_position_ms(start + Duration::from_millis(400)),
      Some(10_000)
    );
  }
}
