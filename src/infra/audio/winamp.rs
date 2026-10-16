//! Winamp's classic spectrum for a frontend that draws its own bars: 75 bands
//! spaced in semitones from 86 Hz to 22 kHz, read on a fixed scale, so loud
//! music fills a bar and quiet music stays low. It keeps nothing between frames;
//! the frontend owns the rise, the fall and the peak caps.

use realfft::{num_complex::Complex, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub const BANDS: usize = 75;

/// Winamp reads 512 samples at 44.1 kHz. Other rates read the same span, so a
/// bin keeps Winamp's width of about 86 Hz.
const WINAMP_RATE: f32 = 44_100.0;
const WINAMP_WINDOW: f32 = 512.0;
/// A band whose bins add up to this fills its bar.
const FULL_BAR: f32 = 15.0;
/// The waveform takes every seventh sample at 44.1 kHz.
const SCOPE_STRIDE: f32 = 7.0;

/// A share of a band: the spectrum read at `position` (in bins), times `weight`.
struct Tap {
  position: f32,
  weight: f32,
}

pub struct WinampSpectrum {
  sample_rate: u32,
  fft: Arc<dyn RealToComplex<f32>>,
  window: Vec<f32>,
  input: Vec<f32>,
  output: Vec<Complex<f32>>,
  magnitudes: Vec<f32>,
  bands: Vec<Vec<Tap>>,
  /// A longer window sums more samples into each bin; this takes it back to Winamp's scale.
  scale: f32,
}

impl WinampSpectrum {
  pub fn new(sample_rate: u32) -> Self {
    let len = window_len(sample_rate);
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(len);
    Self {
      sample_rate,
      input: fft.make_input_vec(),
      output: fft.make_output_vec(),
      magnitudes: vec![0.0; len / 2 + 1],
      window: (0..len).map(|i| hann(i, len)).collect(),
      bands: band_taps(len, sample_rate),
      scale: WINAMP_WINDOW / len as f32,
      fft,
    }
  }

  pub fn sample_rate(&self) -> u32 {
    self.sample_rate
  }

  /// Each band from 0 to 1 for the newest of `samples` (mono, -1 to 1).
  pub fn levels(&mut self, samples: &[f32]) -> [f32; BANDS] {
    let newest = &samples[samples.len().saturating_sub(self.window.len())..];
    self.input.fill(0.0);
    let start = self.input.len() - newest.len();
    for ((slot, sample), weight) in self.input[start..]
      .iter_mut()
      .zip(newest)
      .zip(&self.window[start..])
    {
      *slot = sample * weight;
    }
    if self.fft.process(&mut self.input, &mut self.output).is_err() {
      return [0.0; BANDS];
    }
    for (magnitude, bin) in self.magnitudes.iter_mut().zip(&self.output) {
      *magnitude = bin.norm() * self.scale;
    }
    let mut levels = [0.0; BANDS];
    for (level, taps) in levels.iter_mut().zip(&self.bands) {
      let sum: f32 = taps
        .iter()
        .map(|tap| tap.weight * read_between_bins(&self.magnitudes, tap.position))
        .sum();
      *level = sum.min(FULL_BAR) / FULL_BAR;
    }
    levels
  }
}

/// Winamp's span at `sample_rate`.
fn window_len(sample_rate: u32) -> usize {
  ((WINAMP_WINDOW * sample_rate as f32 / WINAMP_RATE).round() as usize).max(4)
}

fn hann(i: usize, len: usize) -> f32 {
  0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / len as f32).cos()
}

/// Where band `edge` starts, in Winamp's bins: semitone steps that start at bin 1
/// and end in band 75 at bin 255.
fn winamp_edge(edge: usize) -> f32 {
  let top = 2f32.powf(BANDS as f32 / 12.0);
  1.0 + (2f32.powf(edge as f32 / 12.0) - 1.0) * 255.0 / top
}

/// Each band's taps for a `len`-sample window at `sample_rate`: the part-bin at
/// its low edge, its whole bins, and the part-bin at its high edge.
fn band_taps(len: usize, sample_rate: u32) -> Vec<Vec<Tap>> {
  let to_bins = WINAMP_RATE / WINAMP_WINDOW * len as f32 / sample_rate as f32;
  let nyquist = (len / 2) as f32;
  (0..BANDS)
    .map(|band| {
      let low = winamp_edge(band) * to_bins;
      let high = (winamp_edge(band + 1) * to_bins).min(nyquist);
      if low >= high {
        return Vec::new();
      }
      let first = low.floor();
      let last = high.floor();
      if first == last {
        return vec![Tap {
          position: low,
          weight: high - low,
        }];
      }
      let mut taps = vec![Tap {
        position: low,
        weight: first + 1.0 - low,
      }];
      taps.extend((first as usize + 1..last as usize).map(|bin| Tap {
        position: bin as f32,
        weight: 1.0,
      }));
      taps.push(Tap {
        position: last,
        weight: high - last,
      });
      taps
    })
    .collect()
}

/// The spectrum at a fractional bin, on a Catmull-Rom curve through the four nearest bins.
fn read_between_bins(magnitudes: &[f32], position: f32) -> f32 {
  let bin = position.floor() as isize;
  let t = position - bin as f32;
  let at = |offset: isize| {
    magnitudes
      .get((bin + offset).max(0) as usize)
      .copied()
      .unwrap_or(0.0)
  };
  let (p0, p1, p2, p3) = (at(-1), at(0), at(1), at(2));
  p1 + 0.5
    * t
    * ((p2 - p0) + t * ((2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) + t * (3.0 * (p1 - p2) + p3 - p0)))
}

/// Every seventh sample at 44.1 kHz: the same span at other rates.
fn scope_stride(sample_rate: u32) -> usize {
  ((SCOPE_STRIDE * sample_rate as f32 / WINAMP_RATE).round() as usize).max(1)
}

/// The waveform of the newest of `samples`: `points` values from -1 to 1,
/// oldest first, doubled so that half of full scale reaches the edge.
pub fn scope(samples: &[f32], sample_rate: u32, points: usize) -> Vec<f32> {
  let stride = scope_stride(sample_rate);
  let span = points * stride;
  let missing = span.saturating_sub(samples.len());
  let newest = &samples[samples.len().saturating_sub(span)..];
  (0..points)
    .map(|point| {
      (point * stride)
        .checked_sub(missing)
        .and_then(|index| newest.get(index))
        .map_or(0.0, |sample| (sample * 2.0).clamp(-1.0, 1.0))
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tone(hz: f32, amplitude: f32, sample_rate: u32, len: usize) -> Vec<f32> {
    (0..len)
      .map(|i| amplitude * (std::f32::consts::TAU * hz * i as f32 / sample_rate as f32).sin())
      .collect()
  }

  /// The band whose edges hold `hz`.
  fn band_of(hz: f32) -> usize {
    let bin_hz = WINAMP_RATE / WINAMP_WINDOW;
    (0..BANDS)
      .find(|band| winamp_edge(*band) * bin_hz <= hz && hz < winamp_edge(band + 1) * bin_hz)
      .unwrap()
  }

  fn loudest(levels: &[f32; BANDS]) -> usize {
    (0..BANDS)
      .max_by(|a, b| levels[*a].total_cmp(&levels[*b]))
      .unwrap()
  }

  #[test]
  fn the_bands_run_from_86_hz_to_about_22_khz() {
    let bin_hz = WINAMP_RATE / WINAMP_WINDOW;
    assert!((winamp_edge(0) * bin_hz - 86.1).abs() < 0.1);
    assert!((winamp_edge(BANDS) * bin_hz - 21_761.0).abs() < 5.0);
  }

  #[test]
  fn a_tone_lights_the_band_that_holds_its_frequency() {
    for sample_rate in [44_100, 48_000, 96_000] {
      let mut spectrum = WinampSpectrum::new(sample_rate);
      // Quiet enough that no band reaches the top, where they would tie.
      let samples = tone(2_000.0, 0.05, sample_rate, window_len(sample_rate));

      let levels = spectrum.levels(&samples);

      assert_eq!(loudest(&levels), band_of(2_000.0), "at {sample_rate} Hz");
    }
  }

  #[test]
  fn a_loud_tone_fills_its_bar_and_a_quiet_one_stays_low() {
    let mut spectrum = WinampSpectrum::new(44_100);
    let len = window_len(44_100);

    let loud = spectrum.levels(&tone(1_000.0, 0.5, 44_100, len));
    let quiet = spectrum.levels(&tone(1_000.0, 0.01, 44_100, len));

    assert_eq!(loud[band_of(1_000.0)], 1.0);
    assert!(quiet[band_of(1_000.0)] < 0.5, "{}", quiet[band_of(1_000.0)]);
  }

  #[test]
  fn silence_and_a_short_capture_read_as_empty_bands() {
    let mut spectrum = WinampSpectrum::new(48_000);

    assert_eq!(spectrum.levels(&[]), [0.0; BANDS]);
    assert_eq!(spectrum.levels(&vec![0.0; 4_800]), [0.0; BANDS]);
  }

  #[test]
  fn bands_above_half_the_sample_rate_stay_empty() {
    let mut spectrum = WinampSpectrum::new(16_000);
    let noise: Vec<f32> = (0..window_len(16_000))
      .map(|i| if i % 2 == 0 { 0.9 } else { -0.9 })
      .collect();

    let levels = spectrum.levels(&noise);

    assert_eq!(levels[band_of(9_000.0)..], [0.0; BANDS][band_of(9_000.0)..]);
  }

  #[test]
  fn the_scope_takes_every_seventh_sample_at_44_1_khz_and_every_eighth_at_48() {
    let samples: Vec<f32> = (0..32).map(|i| i as f32 / 100.0).collect();

    assert_eq!(scope(&samples, 44_100, 3), vec![0.22, 0.36, 0.5]);
    assert_eq!(scope(&samples, 48_000, 3), vec![0.16, 0.32, 0.48]);
  }

  #[test]
  fn the_scope_doubles_clips_and_pads_a_short_capture_with_silence() {
    assert_eq!(scope(&[0.75, -0.3], 6_300, 4), vec![0.0, 0.0, 1.0, -0.6]);
  }
}
