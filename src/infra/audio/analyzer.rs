use std::sync::{Arc, Mutex};

use super::cavacore::{treble_buffer_size, Cava, CavaBuilder, Channels, DEFAULT_NOISE_REDUCTION};

/// Display bars produced before the first render tick reports the terminal
/// width. Display bars are the mirrored stereo total (2x bars-per-channel).
pub const DEFAULT_BANDS: usize = 12;

/// cavacore was written for 16-bit-scale samples (cava's own f32 input path
/// multiplies by USHRT_MAX); cpal/PipeWire deliver f32 in [-1, 1], so scale up
/// before the f64 cast or autosens ramps against a mis-scaled noise floor.
const SAMPLE_SCALE: f64 = 32768.0;

/// Cap on cavacore's autosens gain. Uncapped autosens (cava's behavior)
/// normalizes ANY input level to full-scale bars given a few seconds; the cap
/// (~+15 dB of boost) still evens out track-to-track loudness but keeps
/// genuinely quiet audio rendering short bars instead of climbing to 100%.
const MAX_AUTOSENS_GAIN: f64 = 6.0;

/// Spectrum data for visualization
#[derive(Clone, Debug, Default)]
pub struct SpectrumData {
  /// Normalized bars (0.0-1.0) in cava's default stereo display order: left
  /// channel reversed on the left half (treble at the outer edge, bass at the
  /// center), right channel forward on the right half.
  pub bands: Vec<f32>,
  /// Overall peak level (0.0-1.0)
  pub peak: f32,
}

/// Audio analyzer wrapping the vendored cavacore engine: log-spaced per-bar
/// cutoffs over bass/mid/treble FFTs, integral smoothing, gravity fall-off and
/// autosens (one persistent gain, fast duck / slow recovery). Runs in stereo
/// and mirrors the two channels like cava's default display.
pub struct AudioAnalyzer {
  cava: Cava,
  sample_rate: u32,
  /// Bars per audio channel; the display shows twice this, mirrored.
  bars_per_channel: usize,
  /// Interleaved stereo 16-bit-scale samples awaiting the next `process()`.
  pending: Vec<f64>,
  /// Newest samples one `execute` can consume (cavacore's sliding stereo
  /// input window); `pending` is capped here so a stalled UI can't grow it.
  max_pending: usize,
  /// Raw engine output: [left 0..bpc][right bpc..2bpc], each bass to treble.
  output: Box<[f64]>,
  spectrum: SpectrumData,
}

impl AudioAnalyzer {
  pub fn new(sample_rate: u32, display_bars: usize) -> Self {
    let sample_rate = clamp_sample_rate(sample_rate);
    let bars_per_channel = clamp_bars_per_channel(sample_rate, display_bars / 2);

    Self {
      cava: build_cava(sample_rate, bars_per_channel),
      sample_rate,
      bars_per_channel,
      pending: Vec::new(),
      max_pending: treble_buffer_size(sample_rate) * 8 * 2,
      output: vec![0.0; bars_per_channel * 2].into_boxed_slice(),
      spectrum: SpectrumData {
        bands: vec![0.0; bars_per_channel * 2],
        peak: 0.0,
      },
    }
  }

  /// Rebuild the cavacore plan for a new display bar count (terminal resize or
  /// style switch). State starts fresh, matching cava's own behavior on resize.
  pub fn set_bars(&mut self, display_bars: usize) {
    let bars_per_channel = clamp_bars_per_channel(self.sample_rate, display_bars / 2);
    if bars_per_channel == self.bars_per_channel {
      return;
    }

    self.bars_per_channel = bars_per_channel;
    self.cava = build_cava(self.sample_rate, bars_per_channel);
    self.output = vec![0.0; bars_per_channel * 2].into_boxed_slice();
    self.spectrum = SpectrumData {
      bands: vec![0.0; bars_per_channel * 2],
      peak: 0.0,
    };
  }

  /// Adopt the rate the audio server actually negotiated. Only the PipeWire
  /// backend renegotiates after construction, so cpal builds never call this.
  #[allow(dead_code)]
  pub fn set_sample_rate(&mut self, sample_rate: u32) {
    let sample_rate = clamp_sample_rate(sample_rate);
    if sample_rate == self.sample_rate {
      return;
    }

    self.sample_rate = sample_rate;
    self.bars_per_channel = clamp_bars_per_channel(sample_rate, self.bars_per_channel);
    self.max_pending = treble_buffer_size(sample_rate) * 8 * 2;
    self.pending.clear();
    self.cava = build_cava(sample_rate, self.bars_per_channel);
    self.output = vec![0.0; self.bars_per_channel * 2].into_boxed_slice();
    self.spectrum = SpectrumData {
      bands: vec![0.0; self.bars_per_channel * 2],
      peak: 0.0,
    };
  }

  /// Push interleaved stereo audio frames (left, right, left, right, ...)
  pub fn push_samples(&mut self, samples: &[f32]) {
    self
      .pending
      .extend(samples.iter().map(|&s| f64::from(s) * SAMPLE_SCALE));

    // Keep only the newest samples, matching cavacore's own sliding window.
    // Callers push whole frames, so the cap (an even number) stays aligned.
    if self.pending.len() > self.max_pending {
      let excess = self.pending.len() - self.max_pending;
      self.pending.drain(..excess);
    }
  }

  /// Process buffered samples and update spectrum
  pub fn process(&mut self) -> SpectrumData {
    // Zero new samples still executes: cavacore keeps applying gravity and the
    // integral filter to the stale window, which is what animates bars falling
    // after pause instead of freezing them.
    self.cava.execute(&self.pending, &mut self.output);
    self.pending.clear();

    mirror_stereo_bands(&self.output, &mut self.spectrum.bands);
    self.spectrum.peak = self
      .spectrum
      .bands
      .iter()
      .fold(0.0f32, |peak, &band| peak.max(band));

    self.spectrum.clone()
  }
}

/// cava.c's default stereo display order (its `p.stereo` mirroring loop with
/// `reverse = 0`): display position n < bars/2 shows the left channel at index
/// bars/2 - n - 1 (reversed), position n >= bars/2 shows the right channel at
/// index n - bars/2 (forward). Bass meets in the middle.
fn mirror_stereo_bands(output: &[f64], bands: &mut [f32]) {
  let bars_per_channel = output.len() / 2;
  for n in 0..bars_per_channel {
    bands[n] = output[bars_per_channel - n - 1].clamp(0.0, 1.0) as f32;
    bands[bars_per_channel + n] = output[bars_per_channel + n].clamp(0.0, 1.0) as f32;
  }
}

/// Floor 4 (not 1): build_cava caps `freq_end` at Nyquist but never below 2,
/// so any admitted rate must keep Nyquist >= 2 or the builder's sanity check
/// rejects the range and the expect below panics.
fn clamp_sample_rate(sample_rate: u32) -> u32 {
  sample_rate.clamp(4, 384_000)
}

fn clamp_bars_per_channel(sample_rate: u32, bars_per_channel: usize) -> usize {
  let max_bars = treble_buffer_size(sample_rate) / 2 + 1;
  bars_per_channel.clamp(1, max_bars)
}

fn build_cava(sample_rate: u32, bars_per_channel: usize) -> Cava {
  // cava's default range, with the top capped at Nyquist for low-rate devices
  // (and the start kept under the top for absurdly low ones).
  let freq_end = 10_000.min(sample_rate / 2);
  let freq_start = 50.min(freq_end - 1).max(1);

  CavaBuilder::default()
    .bars_per_channel(bars_per_channel)
    .sample_rate(sample_rate)
    .audio_channels(Channels::Stereo)
    .enable_autosens(true)
    .max_sens(MAX_AUTOSENS_GAIN)
    .noise_reduction(DEFAULT_NOISE_REDUCTION)
    .frequency_range(freq_start..freq_end)
    .build()
    .expect("cavacore build with pre-clamped parameters")
}

/// Thread-safe wrapper for AudioAnalyzer
pub type SharedAnalyzer = Arc<Mutex<AudioAnalyzer>>;

pub fn create_shared_analyzer(sample_rate: u32) -> SharedAnalyzer {
  Arc::new(Mutex::new(AudioAnalyzer::new(sample_rate, DEFAULT_BANDS)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn push_scales_samples_to_16_bit_range() {
    let mut analyzer = AudioAnalyzer::new(48_000, 12);
    analyzer.push_samples(&[1.0, -0.5]);
    assert_eq!(analyzer.pending, vec![32_768.0, -16_384.0]);
  }

  #[test]
  fn pending_backlog_keeps_the_newest_samples() {
    let mut analyzer = AudioAnalyzer::new(48_000, 12);
    analyzer.push_samples(&vec![0.0; analyzer.max_pending]);
    analyzer.push_samples(&[1.0, 1.0]);

    assert_eq!(analyzer.pending.len(), analyzer.max_pending);
    assert_eq!(analyzer.pending.last(), Some(&32_768.0));
  }

  #[test]
  fn process_drains_pending_and_reports_requested_bars() {
    let mut analyzer = AudioAnalyzer::new(48_000, 12);
    analyzer.push_samples(&[0.25; 512]);

    let spectrum = analyzer.process();

    assert!(analyzer.pending.is_empty());
    assert_eq!(spectrum.bands.len(), 12);
  }

  #[test]
  fn set_bars_rebuilds_the_plan_for_the_new_count() {
    let mut analyzer = AudioAnalyzer::new(48_000, 12);
    analyzer.set_bars(40);
    assert_eq!(analyzer.process().bands.len(), 40);
  }

  #[test]
  fn bands_are_mirrored_like_cavas_stereo_display() {
    // Engine output: [left: 0.1, 0.2, 0.3][right: 0.4, 0.5, 0.6].
    let mut bands = vec![0.0f32; 6];
    mirror_stereo_bands(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], &mut bands);

    // Left half reversed (treble at the outer edge), right half forward.
    assert_eq!(bands, vec![0.3, 0.2, 0.1, 0.4, 0.5, 0.6]);
  }

  #[test]
  fn bars_are_clamped_to_what_the_sample_rate_supports() {
    // 8 kHz -> treble buffer 128 -> at most 65 bars per channel.
    let analyzer = AudioAnalyzer::new(8_000, 512);
    assert_eq!(analyzer.bars_per_channel, 65);
    assert_eq!(analyzer.spectrum.bands.len(), 130);
  }

  #[test]
  fn degenerate_sample_rates_never_panic_the_builder() {
    for rate in [0, 1, 2, 3, 4, 7_999] {
      let mut analyzer = AudioAnalyzer::new(rate, 12);
      analyzer.set_sample_rate(5);
      let _ = analyzer.process();
    }
  }

  #[test]
  fn sample_rate_change_rebuilds_without_panicking() {
    let mut analyzer = AudioAnalyzer::new(48_000, 12);
    analyzer.push_samples(&[0.5; 256]);
    analyzer.set_sample_rate(44_100);

    let spectrum = analyzer.process();
    assert_eq!(spectrum.bands.len(), 12);
  }
}
