//! The player bar's visualizer frames: read from the capture on a frame clock,
//! analysed here, and sent while a page shows the bar and this process plays.

use crate::core::app::App;
use crate::core::driver::Driver;
use crate::core::user_config::PlayerBarVisualizer;
use crate::gui::protocol::{self, ServerMessage};
use std::time::{Duration, Instant};

/// Whether this build captures what the computer plays: PipeWire on Linux and
/// loopback on Windows. Elsewhere the capture would open the microphone.
pub(crate) const AVAILABLE: bool = cfg!(any(
  all(target_os = "linux", feature = "audio-viz"),
  all(target_os = "windows", feature = "audio-viz-cpal")
));

/// About one frame for each refresh of the display.
pub(crate) const FRAME_EVERY: Duration = Duration::from_millis(16);
/// The capture stays open this long after the page hides the bar or the sound
/// stops, so a short pause does not open and scan the device again.
const LINGER: Duration = Duration::from_secs(10);
/// An open capture that brings no new audio this long while this process plays
/// is closed and opened again, at first after this long and then twice as long
/// each time in a row. A capture that is not open gets `OPEN_LIMIT`, which
/// covers PipeWire's setup wait and a slow device scan.
const STALL: Duration = Duration::from_secs(3);
const OPEN_LIMIT: Duration = Duration::from_secs(6);
/// After this many stalls in a row the capture rests until a new start: another
/// mode, the bar shown again, or the sound starting again.
const MAX_STALLS: u32 = 5;
/// Audio the player queued before a volume change still plays at the old volume this long.
const SETTLE: Duration = Duration::from_millis(600);
/// The waveform's points; the page draws the newest that fit its width.
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
const WAVE_POINTS: usize = 320;
/// The most of spotatui's own volume a frame takes back out: 40 dB, which
/// covers volumes down to about a third. The capture hears the whole mix after
/// the volume, so a larger undo would also lift other apps' sound.
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
const MAX_VOLUME_UNDO: f32 = 100.0;

pub(crate) struct Feed {
  mode: PlayerBarVisualizer,
  /// This process plays at a volume above zero, so the capture hears our own sound.
  sounding: bool,
  capturing: bool,
  watched: bool,
  /// When a page last showed the bar, and when this process last played.
  shown_at: Option<Instant>,
  sounded_at: Option<Instant>,
  /// When the capture last brought new audio, or when the wait for it began.
  heard_at: Option<Instant>,
  /// A stalled capture stays closed until then.
  retry_at: Option<Instant>,
  stalls: u32,
  /// The last frame sent was silent, so another silent one is not sent.
  quiet: bool,
  #[cfg_attr(
    not(any(feature = "audio-viz", feature = "audio-viz-cpal")),
    allow(dead_code)
  )]
  volume: Volume,
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  written: u64,
  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  spectrum: Option<crate::infra::audio::WinampSpectrum>,
}

/// spotatui's volume percent, and the one before it while the queue drains.
#[derive(Clone, Copy)]
struct Volume {
  percent: u32,
  before: u32,
  changed_at: Option<Instant>,
}

impl Volume {
  fn set(&mut self, percent: u32, now: Instant) {
    if percent != self.percent {
      // Changes in quick succession: the queue can still hold any step of them.
      self.before = if self.settling(now) {
        self.before.max(self.percent)
      } else {
        self.percent
      };
      self.percent = percent;
      self.changed_at = Some(now);
    }
  }

  fn settling(&self, now: Instant) -> bool {
    self
      .changed_at
      .is_some_and(|at| now.saturating_duration_since(at) < SETTLE)
  }

  /// The loudest while the queue drains: a change can only dim the bars for a moment.
  #[cfg_attr(
    not(any(feature = "audio-viz", feature = "audio-viz-cpal")),
    allow(dead_code)
  )]
  fn heard(&self, now: Instant) -> u32 {
    if self.settling(now) {
      self.percent.max(self.before)
    } else {
      self.percent
    }
  }
}

impl Feed {
  pub(crate) fn new() -> Self {
    Self {
      mode: PlayerBarVisualizer::Off,
      sounding: false,
      capturing: false,
      watched: false,
      shown_at: None,
      sounded_at: None,
      heard_at: None,
      retry_at: None,
      stalls: 0,
      quiet: true,
      volume: Volume {
        percent: 100,
        before: 100,
        changed_at: None,
      },
      #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
      written: 0,
      #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
      spectrum: None,
    }
  }

  /// Takes in what the frames follow; call it whenever the app is locked.
  pub(crate) fn note(&mut self, app: &App, playing: bool, watched: bool, now: Instant) {
    self.observe(
      app.user_config.behavior.player_bar_visualizer,
      app.audio_capture_active,
      app.desired_volume(),
      playing,
      watched,
      now,
    );
  }

  fn observe(
    &mut self,
    mode: PlayerBarVisualizer,
    capturing: bool,
    percent: u32,
    playing: bool,
    watched: bool,
    now: Instant,
  ) {
    let sounding = playing && percent > 0;
    if mode != self.mode || (watched && !self.watched) || (sounding && !self.sounding) {
      self.stalls = 0;
      self.retry_at = None;
    }
    if capturing && !self.capturing {
      // Just opened: its first audio is still on the way, and it counts from zero.
      self.heard_at = Some(now);
      #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
      {
        self.written = 0;
      }
    }
    self.mode = mode;
    self.capturing = capturing;
    self.volume.set(percent, now);
    self.sounding = sounding;
    if sounding {
      self.sounded_at = Some(now);
    }
    self.watched = watched;
    if watched {
      self.shown_at = Some(now);
    }
    self.check_stall(now);
  }

  /// Closes a capture that brings nothing while frames should flow; the driver
  /// opens it again, on the default device of that moment, after a pause.
  fn check_stall(&mut self, now: Instant) {
    if !(self.watched && self.sounding && self.wants_capture(now)) {
      self.heard_at = None;
      return;
    }
    let since = *self.heard_at.get_or_insert(now);
    let limit = if self.capturing { STALL } else { OPEN_LIMIT };
    if now.saturating_duration_since(since) < limit {
      return;
    }
    self.stalls += 1;
    self.heard_at = None;
    let wait = STALL.saturating_mul(1 << (self.stalls - 1));
    self.retry_at = Some(now + wait);
    let resting = self.stalls >= MAX_STALLS;
    let level = if self.stalls == 1 || resting {
      log::Level::Info
    } else {
      log::Level::Debug
    };
    log::log!(
      level,
      "[audio-viz] the player bar's capture brought no audio ({} in a row); {}",
      self.stalls,
      if resting {
        "resting until the next start".to_string()
      } else {
        format!("opening it again in {wait:?}")
      }
    );
  }

  /// Whether the capture stays open.
  pub(crate) fn wants_capture(&self, now: Instant) -> bool {
    let recent =
      |at: Option<Instant>| at.is_some_and(|at| now.saturating_duration_since(at) < LINGER);
    AVAILABLE
      && self.mode != PlayerBarVisualizer::Off
      && recent(self.shown_at)
      && recent(self.sounded_at)
      && self.stalls < MAX_STALLS
      && !self.retry_at.is_some_and(|at| now < at)
  }

  /// Whether the frame clock runs: a page shows the bar, and there is sound
  /// or the bars have yet to hear that it stopped.
  pub(crate) fn streams(&self, watched: bool, now: Instant) -> bool {
    watched && self.wants_capture(now) && self.capturing && (self.sounding || !self.quiet)
  }

  /// The next frame, or `None` when there is nothing new to show.
  pub(crate) fn frame(&mut self, driver: &Driver, now: Instant) -> Option<ServerMessage> {
    if !self.sounding {
      return self.silent_frame();
    }
    #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
    {
      self.frame_from(driver.recent_audio()?, now)
    }
    #[cfg(not(any(feature = "audio-viz", feature = "audio-viz-cpal")))]
    {
      let _ = (driver, now);
      None
    }
  }

  /// The empty frame that lets the bars fall, sent once.
  fn silent_frame(&mut self) -> Option<ServerMessage> {
    (!std::mem::replace(&mut self.quiet, true)).then(|| protocol::visualizer(&[], &[]))
  }

  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  fn frame_from(
    &mut self,
    mut audio: crate::infra::audio::RecentAudio,
    now: Instant,
  ) -> Option<ServerMessage> {
    use crate::infra::audio::{scope, volume_gain, WinampSpectrum};
    // Nothing new came in: the page lets the bars fall on its own.
    if audio.written == self.written {
      return None;
    }
    self.written = audio.written;
    self.heard_at = Some(now);
    self.stalls = 0;
    let undo = (1.0 / volume_gain(self.volume.heard(now).min(100) as u8)).min(MAX_VOLUME_UNDO);
    audio.samples.iter_mut().for_each(|sample| *sample *= undo);
    let (levels, wave) = match self.mode {
      PlayerBarVisualizer::Off => return None,
      PlayerBarVisualizer::Spectrum => {
        if self.spectrum.as_ref().map(WinampSpectrum::sample_rate) != Some(audio.sample_rate) {
          self.spectrum = Some(WinampSpectrum::new(audio.sample_rate));
        }
        (
          self.spectrum.as_mut()?.levels(&audio.samples).to_vec(),
          Vec::new(),
        )
      }
      PlayerBarVisualizer::Waveform => (
        Vec::new(),
        scope(&audio.samples, audio.sample_rate, WAVE_POINTS),
      ),
    };
    // Below half a step of the bytes the page gets.
    let silent = levels.iter().all(|level| *level < 0.5 / 255.0)
      && wave.iter().all(|value| value.abs() < 0.5 / 127.0);
    if silent {
      return self.silent_frame();
    }
    self.quiet = false;
    Some(protocol::visualizer(&levels, &wave))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Spectrum on, an open capture at 80% volume.
  fn seen(feed: &mut Feed, playing: bool, watched: bool, now: Instant) {
    feed.observe(
      PlayerBarVisualizer::Spectrum,
      true,
      80,
      playing,
      watched,
      now,
    );
  }

  fn empty_frame() -> serde_json::Value {
    serde_json::json!({ "kind": "visualizer", "payload": { "levels": [], "wave": [] } })
  }

  #[test]
  fn nothing_is_captured_until_this_process_plays() {
    let t0 = Instant::now();
    let mut feed = Feed::new();
    seen(&mut feed, false, true, t0);

    assert!(!feed.wants_capture(t0));
  }

  #[test]
  fn the_capture_stays_open_for_a_while_after_the_sound_stops_or_the_page_hides() {
    let t0 = Instant::now();
    let mut paused = Feed::new();
    seen(&mut paused, true, true, t0);
    seen(&mut paused, false, true, t0 + LINGER / 2);
    let mut hidden = Feed::new();
    seen(&mut hidden, true, true, t0);
    seen(&mut hidden, true, false, t0 + LINGER / 2);

    for feed in [paused, hidden] {
      assert_eq!(feed.wants_capture(t0 + LINGER / 2), AVAILABLE);
      assert!(!feed.wants_capture(t0 + LINGER));
    }
  }

  #[test]
  fn the_visualizer_turned_off_closes_the_capture_and_stops_the_frames() {
    let t0 = Instant::now();
    let mut feed = Feed::new();
    feed.observe(PlayerBarVisualizer::Off, true, 80, true, true, t0);

    assert!(!feed.wants_capture(t0));
    assert!(!feed.streams(true, t0));
  }

  #[test]
  fn a_capture_that_brings_nothing_is_closed_and_tried_again_after_a_longer_pause_each_time() {
    if !AVAILABLE {
      return;
    }
    let t0 = Instant::now();
    let mut feed = Feed::new();
    seen(&mut feed, true, true, t0);

    seen(&mut feed, true, true, t0 + STALL);
    assert!(!feed.wants_capture(t0 + STALL));
    assert!(feed.wants_capture(t0 + STALL * 2));

    seen(&mut feed, true, true, t0 + STALL * 2);
    seen(&mut feed, true, true, t0 + STALL * 3);
    assert!(!feed.wants_capture(t0 + STALL * 4));
    assert!(feed.wants_capture(t0 + STALL * 5));
  }

  #[test]
  fn a_hidden_page_or_a_pause_is_no_stall() {
    if !AVAILABLE {
      return;
    }
    let t0 = Instant::now();
    let mut feed = Feed::new();
    seen(&mut feed, true, true, t0);
    seen(&mut feed, true, false, t0 + STALL);
    seen(&mut feed, false, true, t0 + STALL * 2);

    assert!(feed.wants_capture(t0 + STALL * 2));
  }

  #[test]
  fn a_stop_sends_one_empty_frame_and_then_rests_until_the_sound_returns() {
    let t0 = Instant::now();
    let mut feed = Feed::new();
    seen(&mut feed, true, true, t0);
    feed.quiet = false;
    seen(&mut feed, false, true, t0);

    assert_eq!(feed.streams(true, t0), AVAILABLE);
    let frame = serde_json::to_value(feed.silent_frame().unwrap()).unwrap();
    assert_eq!(frame, empty_frame());
    assert!(!feed.streams(true, t0));
    assert!(feed.silent_frame().is_none());

    seen(&mut feed, true, true, t0);
    assert_eq!(feed.streams(true, t0), AVAILABLE);
  }

  #[test]
  fn a_muted_player_counts_as_silence() {
    let t0 = Instant::now();
    let mut feed = Feed::new();
    feed.observe(PlayerBarVisualizer::Spectrum, true, 0, true, true, t0);

    assert!(!feed.sounding);
    assert!(!feed.wants_capture(t0));
  }

  #[test]
  fn audio_queued_before_a_volume_change_is_heard_at_the_louder_volume_for_a_moment() {
    let t0 = Instant::now();
    let mut volume = Volume {
      percent: 80,
      before: 80,
      changed_at: None,
    };

    volume.set(30, t0);
    assert_eq!(volume.heard(t0 + SETTLE / 2), 80);
    assert_eq!(volume.heard(t0 + SETTLE), 30);
    volume.set(90, t0 + SETTLE);
    assert_eq!(volume.heard(t0 + SETTLE), 90);
  }

  #[test]
  fn changes_in_quick_succession_keep_the_loudest_step_until_the_queue_drains() {
    let t0 = Instant::now();
    let step = SETTLE / 4;
    let mut volume = Volume {
      percent: 80,
      before: 80,
      changed_at: None,
    };

    for (i, percent) in [70, 60, 50].into_iter().enumerate() {
      volume.set(percent, t0 + step * i as u32);
    }

    assert_eq!(volume.heard(t0 + step * 3), 80);
    assert_eq!(volume.heard(t0 + step * 2 + SETTLE), 50);
  }

  #[test]
  fn after_five_stalls_in_a_row_the_capture_rests_until_the_sound_starts_again() {
    if !AVAILABLE {
      return;
    }
    let mut now = Instant::now();
    let mut feed = Feed::new();
    seen(&mut feed, true, true, now);
    for _ in 0..MAX_STALLS {
      now += STALL;
      seen(&mut feed, true, true, now);
      now = feed.retry_at.unwrap();
      seen(&mut feed, true, true, now);
    }
    assert!(!feed.wants_capture(now));

    seen(&mut feed, false, true, now);
    seen(&mut feed, true, true, now);
    assert!(feed.wants_capture(now));
  }

  #[test]
  fn a_capture_that_is_not_open_gets_longer_than_one_that_is() {
    if !AVAILABLE {
      return;
    }
    let t0 = Instant::now();
    let mut feed = Feed::new();
    feed.observe(PlayerBarVisualizer::Spectrum, false, 80, true, true, t0);

    feed.observe(
      PlayerBarVisualizer::Spectrum,
      false,
      80,
      true,
      true,
      t0 + STALL,
    );
    assert!(feed.wants_capture(t0 + STALL));
    feed.observe(
      PlayerBarVisualizer::Spectrum,
      false,
      80,
      true,
      true,
      t0 + OPEN_LIMIT,
    );
    assert!(!feed.wants_capture(t0 + OPEN_LIMIT));
  }

  #[test]
  fn no_frames_go_out_while_no_page_shows_the_bar() {
    let t0 = Instant::now();
    let mut feed = Feed::new();
    seen(&mut feed, true, true, t0);

    assert!(!feed.streams(false, t0));
  }

  #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
  mod frames {
    use super::*;
    use crate::infra::audio::RecentAudio;

    fn audio(samples: Vec<f32>, written: u64) -> RecentAudio {
      RecentAudio {
        samples,
        sample_rate: 44_100,
        written,
      }
    }

    fn tone() -> Vec<f32> {
      (0..4_410)
        .map(|i| 0.5 * (std::f32::consts::TAU * 1_000.0 * i as f32 / 44_100.0).sin())
        .collect()
    }

    fn playing(mode: PlayerBarVisualizer, percent: u32, now: Instant) -> Feed {
      let mut feed = Feed::new();
      feed.observe(mode, true, percent, true, true, now);
      feed
    }

    #[test]
    fn a_frame_carries_75_levels_and_nothing_when_no_new_audio_came_in() {
      let now = Instant::now();
      let mut feed = playing(PlayerBarVisualizer::Spectrum, 100, now);

      let first = serde_json::to_value(feed.frame_from(audio(tone(), 10), now).unwrap()).unwrap();
      assert_eq!(first["payload"]["levels"].as_array().unwrap().len(), 75);
      assert!(feed.frame_from(audio(tone(), 10), now).is_none());
    }

    #[test]
    fn silence_goes_out_once_as_an_empty_frame() {
      let now = Instant::now();
      let mut feed = playing(PlayerBarVisualizer::Waveform, 100, now);
      feed.frame_from(audio(tone(), 1), now).unwrap();

      let silent =
        serde_json::to_value(feed.frame_from(audio(vec![0.0; 4_410], 2), now).unwrap()).unwrap();
      assert_eq!(silent, empty_frame());
      assert!(feed.frame_from(audio(vec![0.0; 4_410], 3), now).is_none());
    }

    #[test]
    fn the_volume_is_taken_back_out_of_the_waveform() {
      let now = Instant::now();
      let mut feed = playing(PlayerBarVisualizer::Waveform, 80, now - SETTLE);

      let frame =
        serde_json::to_value(feed.frame_from(audio(vec![0.05; 4_410], 1), now).unwrap()).unwrap();

      // 80% is about -12 dB, so 0.05 comes back as about 0.2, doubled to 0.4 of the swing.
      assert_eq!(frame["payload"]["wave"][0], 51);
    }
  }
}
