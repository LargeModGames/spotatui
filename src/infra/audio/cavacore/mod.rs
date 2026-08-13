//! Vendored from the `cavacore` crate v2.0.2 — a Rust port of cava's core
//! processing engine (<https://github.com/karlstav/cava/blob/master/CAVACORE.md>).
//! Upstream: <https://github.com/TornaxO7/cavacore-rs> (archived).
//!
//! Vendored instead of depended on because upstream is archived and ships
//! correctness bugs we need fixed. Local changes, kept minimal on purpose
//! (each patch site is marked `PATCH(spotatui)`):
//! - Autosens fix: upstream only *raised* `sens` during digital silence; the C
//!   original raises it while signal is present (`if (!silence)`). Without the
//!   fix, opening the visualizer mid-song leaves bars at ~2% height forever.
//! - Cutoff fix: upstream cast the fractional `relative_cut_off` to u32 before
//!   multiplying by the FFT size at all five cutoff sites, truncating every
//!   cutoff to 0 and collapsing the log-spaced bar mapping into consecutive
//!   near-DC bins. C multiplies in float first.
//! - Input-order fix: upstream copied each new chunk into the sliding buffer
//!   in forward order; C writes it time-reversed (newest sample at index 0),
//!   which keeps the buffer free of chunk-boundary discontinuities.
//! - Dropped the `bounded-integer` and `thiserror` deps: `SampleRate` /
//!   `NonZero*` parameter types became plain integers validated in
//!   `sanity_check`, and `Error` implements `Display` by hand.
//! - `treble_buffer_size(rate)` is exposed standalone so the analyzer can cap
//!   its sample backlog to cavacore's sliding input window.
//! - EXTENSION (not in C cava): `CavaBuilder::max_sens` caps the autosens gain
//!   so near-silent input cannot be normalized up to full-scale bars; the
//!   default (infinity) preserves upstream behavior.
//!
//! Upstream license (MIT):
//!
//! ```text
//! MIT License
//!
//! Copyright (c) 2025 TornaxO7
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```
use std::sync::Arc;

use realfft::{num_complex::Complex, FftNum, RealFftPlanner, RealToComplex};

mod builder;
mod error;

pub use builder::{treble_buffer_size, CavaBuilder, DEFAULT_NOISE_REDUCTION};
pub use error::Error;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Channels {
  Mono = 1,
  Stereo,
}

pub struct Cava {
  bars_per_channel: usize,
  sample_rate: u32,
  enable_autosens: bool,
  init_sens: bool,
  sens: f64,
  max_sens: f64,
  framerate: f64,
  frame_skip: f64,
  noise_reduction: f64,

  left: AudioData<f64>,
  right: Option<AudioData<f64>>,

  bass_hann_window: Box<[f64]>,
  mid_hann_window: Box<[f64]>,
  treble_hann_window: Box<[f64]>,

  input_buffer: Box<[f64]>,
  buffer_lower_cut_off: Box<[u32]>,
  buffer_upper_cut_off: Box<[u32]>,
  eq: Box<[f64]>,

  bass_cut_off_bar: i32,
  treble_cut_off_bar: i32,

  cava_fall: Box<[f64]>,
  cava_mem: Box<[f64]>,
  cava_peak: Box<[f64]>,
  prev_cava_out: Box<[f64]>,
}

impl Cava {
  pub fn make_output(&self) -> Box<[f64]> {
    let amount_channels = if self.right.is_some() { 2 } else { 1 };
    let total_amount_bars = self.bars_per_channel * amount_channels;

    vec![0.; total_amount_bars].into_boxed_slice()
  }

  pub fn execute(&mut self, input: &[f64], output: &mut [f64]) {
    if input.is_empty() {
      self.frame_skip += 1.;
    } else {
      self.framerate -= self.framerate / 64.;

      let amount_audio_channels = if self.right.is_some() { 2. } else { 1. };

      self.framerate += (self.sample_rate as f64 * amount_audio_channels * self.frame_skip)
        / input.len() as f64
        / 64.;

      self.frame_skip = 1.;

      let input_buffer_len = self.input_buffer.len();
      self
        .input_buffer
        .copy_within(..input_buffer_len - input.len(), input.len());
      // PATCH(spotatui): C writes the new chunk time-reversed (newest sample
      // at index 0), keeping the whole buffer one continuous time-reversed
      // stream; upstream copied it forward, splicing a discontinuity into the
      // FFT windows at every chunk boundary.
      for (n, &sample) in input.iter().enumerate() {
        self.input_buffer[input.len() - n - 1] = sample;
      }
    }

    // fill the bass, mid and treble buffers
    if let Some(right) = self.right.as_mut() {
      // bass
      for i in 0..self.left.in_bass.len() {
        right.in_bass[i] = self.input_buffer[i * 2] * self.bass_hann_window[i];
        self.left.in_bass[i] = self.input_buffer[i * 2 + 1] * self.bass_hann_window[i];
      }

      // mid
      for i in 0..self.left.in_mid.len() {
        right.in_mid[i] = self.input_buffer[i * 2] * self.mid_hann_window[i];
        self.left.in_mid[i] = self.input_buffer[i * 2 + 1] * self.mid_hann_window[i];
      }

      // treble
      for i in 0..self.left.in_treble.len() {
        right.in_treble[i] = self.input_buffer[i * 2] * self.treble_hann_window[i];
        self.left.in_treble[i] = self.input_buffer[i * 2 + 1] * self.treble_hann_window[i];
      }

      right
        .p_bass
        .process(&mut right.in_bass, &mut right.out_bass)
        .unwrap();
      right
        .p_mid
        .process(&mut right.in_mid, &mut right.out_mid)
        .unwrap();
      right
        .p_treble
        .process(&mut right.in_treble, &mut right.out_treble)
        .unwrap();
    } else {
      // bass
      for i in 0..self.left.in_bass.len() {
        self.left.in_bass[i] = self.input_buffer[i] * self.bass_hann_window[i];
      }

      // mid
      for i in 0..self.left.in_mid.len() {
        self.left.in_mid[i] = self.input_buffer[i] * self.mid_hann_window[i];
      }

      // treble
      for i in 0..self.left.in_treble.len() {
        self.left.in_treble[i] = self.input_buffer[i] * self.treble_hann_window[i];
      }
    }

    // fft goes brrrrrrr
    self
      .left
      .p_bass
      .process(&mut self.left.in_bass, &mut self.left.out_bass)
      .unwrap();
    self
      .left
      .p_mid
      .process(&mut self.left.in_mid, &mut self.left.out_mid)
      .unwrap();
    self
      .left
      .p_treble
      .process(&mut self.left.in_treble, &mut self.left.out_treble)
      .unwrap();

    // separate frequency bands
    for n in 0..self.bars_per_channel {
      let mut tmp_l = 0.;
      let mut tmp_r = 0.;

      // add upp FFT values within bands
      for i in self.buffer_lower_cut_off[n] as usize..=self.buffer_upper_cut_off[n] as usize {
        if n <= self.bass_cut_off_bar as usize {
          tmp_l += self.left.out_bass[i].norm();
          if let Some(right) = &self.right {
            tmp_r += right.out_bass[i].norm();
          }
        } else if (self.bass_cut_off_bar as usize..=self.treble_cut_off_bar as usize).contains(&n) {
          tmp_l += self.left.out_mid[i].norm();
          if let Some(right) = &self.right {
            tmp_r += right.out_mid[i].norm();
          }
        } else if (self.treble_cut_off_bar as usize) < n {
          tmp_l += self.left.out_treble[i].norm();
          if let Some(right) = &self.right {
            tmp_r += right.out_treble[i].norm();
          }
        }
      }

      // getting average multiply with eq
      tmp_l /= self.buffer_upper_cut_off[n] as f64 - self.buffer_lower_cut_off[n] as f64 + 1.;
      tmp_l *= self.eq[n];
      output[n] = tmp_l;

      if self.right.is_some() {
        tmp_r /= self.buffer_upper_cut_off[n] as f64 - self.buffer_lower_cut_off[n] as f64 + 1.;
        tmp_r *= self.eq[n];
        output[n + self.bars_per_channel] = tmp_r;
      }
    }

    // applying sens or getting max value
    if self.enable_autosens {
      for val in output.iter_mut() {
        *val *= self.sens;
      }
    }

    // process [smoothing]
    let mut is_overshooting = false;
    let gravity_mod = {
      let gravity_mod = (60. / self.framerate).powf(2.5) * 1.54 / self.noise_reduction;

      gravity_mod.max(1.)
    };

    let amount_channels = if self.right.is_some() { 2 } else { 1 };
    for (n, out) in output
      .iter_mut()
      .enumerate()
      .take(self.bars_per_channel * amount_channels)
    {
      // [smoothing]: falloff
      if *out < self.prev_cava_out[n] && self.noise_reduction > 0.1 {
        *out = self.cava_peak[n] * (1. - (self.cava_fall[n].powf(2.) * gravity_mod));

        if *out < 0.0 {
          *out = 0.0;
        }
        self.cava_fall[n] += 0.028;
      } else {
        self.cava_peak[n] = *out;
        self.cava_fall[n] = 0.;
      }
      self.prev_cava_out[n] = *out;

      // [smoothing]: integral
      *out += self.cava_mem[n] * self.noise_reduction;
      self.cava_mem[n] = *out;
      if self.enable_autosens {
        // check if we overshoot target height
        if *out > 1. {
          is_overshooting = true;
        }
      }
    }

    // calculating automatic sense adjustment
    if self.enable_autosens {
      if is_overshooting {
        self.sens *= 0.98;
        self.init_sens = false;
      } else {
        // PATCH(spotatui): upstream had `if is_silent`, inverted against the C
        // original's `if (!silence)` — sens must rise while signal plays (slow
        // recovery) and duck on overshoot, not rise only during silence.
        let is_silent = input.iter().all(|&v| v == 0.);
        if !is_silent {
          self.sens *= 1.002;
          if self.init_sens {
            self.sens *= 1.1;
          }
        }
      }

      // EXTENSION(spotatui): optional autosens gain cap (upstream has none) so
      // near-silent input stops short of full-scale bars.
      if self.sens > self.max_sens {
        self.sens = self.max_sens;
      }
    }
  }
}

impl CavaBuilder {
  pub fn build(&self) -> Result<Cava, Vec<Error>> {
    self.sanity_check()?;

    let bars_per_channel = self.bars_per_channel;
    let sample_rate = self.sample_rate;
    let enable_autosens = self.enable_autosens;
    let init_sens = true;
    let sens = 1.;
    let max_sens = self.max_sens;
    let framerate = 75.;
    let frame_skip = 1.;
    let noise_reduction = self.noise_reduction;

    let treble_buffer_size = self.compute_treble_buffer_size();
    let (left, right): (AudioData<f64>, Option<AudioData<f64>>) = {
      let left = AudioData::new(treble_buffer_size);
      let right = match self.audio_channels {
        Channels::Mono => None,
        Channels::Stereo => Some(AudioData::new(treble_buffer_size)),
      };
      (left, right)
    };

    let bass_hann_window = compute_hann_window(left.in_bass.len());
    let mid_hann_window = compute_hann_window(left.in_mid.len());
    let treble_hann_window = compute_hann_window(left.in_treble.len());

    let input_buffer =
      vec![0.0; left.in_bass.len() * self.audio_channels as usize].into_boxed_slice();

    let mut buffer_lower_cut_off = vec![0; bars_per_channel + 1].into_boxed_slice();
    let mut buffer_upper_cut_off = vec![0; bars_per_channel + 1].into_boxed_slice();
    let mut eq = vec![0.; bars_per_channel + 1].into_boxed_slice();
    let mut cut_off_frequency = vec![0f64; bars_per_channel + 1].into_boxed_slice();

    let total_amount_bars = bars_per_channel * self.audio_channels as usize;
    let cava_fall = vec![0.0; total_amount_bars].into_boxed_slice();
    let cava_mem = vec![0.0; total_amount_bars].into_boxed_slice();
    let cava_peak = vec![0.0; total_amount_bars].into_boxed_slice();
    let prev_cava_out = vec![0.0; total_amount_bars].into_boxed_slice();

    // process: calculate cutoff frequencies and eq
    let bass_cut_off = 100.;
    let treble_cut_off = 500.;

    // calculate frequency constant (used to distribute bars across the frequency band)
    let frequency_constant = (self.freq_range.start as f64 / self.freq_range.end as f64).log10()
      / (1. / (self.bars_per_channel as f64 + 1.) - 1.);

    let mut relative_cut_off = vec![0.; self.bars_per_channel + 1].into_boxed_slice();
    let mut bass_cut_off_bar = -1;
    let mut treble_cut_off_bar = -1;
    let mut first_bar = true;
    let mut first_treble_bar = 0;
    let mut bar_buffer = vec![0; self.bars_per_channel + 1];

    for n in 0..bars_per_channel + 1 {
      let mut bar_distribution_coefficient = -frequency_constant;
      bar_distribution_coefficient +=
        (n as f64 + 1.) / (bars_per_channel as f64 + 1.) * frequency_constant;

      cut_off_frequency[n] = self.freq_range.end as f64 * 10f64.powf(bar_distribution_coefficient);

      if n > 0 {
        // what?
        let condition = cut_off_frequency[n - 1] >= cut_off_frequency[n]
          && cut_off_frequency[n - 1] > bass_cut_off;

        if condition {
          cut_off_frequency[n] =
            cut_off_frequency[n - 1] + (cut_off_frequency[n - 1] - cut_off_frequency[n - 2]);
        }
      }

      relative_cut_off[n] = cut_off_frequency[n] / (self.sample_rate as f64 / 2.);

      // some random magic?
      eq[n] = cut_off_frequency[n].powf(1.);
      eq[n] /= 2f64.powf(29.);
      eq[n] /= (left.in_bass.len() as f64).log2();

      // PATCH(spotatui): upstream cast relative_cut_off (a 0..1 fraction) to
      // u32 BEFORE the multiply at all five cutoff sites below, truncating
      // every cutoff to bin 0 and collapsing cava's log-spaced bar mapping
      // into consecutive near-DC bins. C multiplies in float and truncates on
      // the int assignment: `relative_cut_off[n] * (bufferSize / 2)`.
      if cut_off_frequency[n] < bass_cut_off {
        // BASS
        bar_buffer[n] = 1;
        buffer_lower_cut_off[n] = (relative_cut_off[n] * (left.in_bass.len() as f64 / 2.)) as u32;
        bass_cut_off_bar += 1;
        treble_cut_off_bar += 1;
        if bass_cut_off_bar > 0 {
          first_bar = false;
        }

        if buffer_lower_cut_off[n] > left.in_bass.len() as u32 / 2 {
          buffer_lower_cut_off[n] = left.in_bass.len() as u32 / 2;
        }
      } else if cut_off_frequency[n] > bass_cut_off && cut_off_frequency[n] < treble_cut_off {
        // MID
        bar_buffer[n] = 2;
        buffer_lower_cut_off[n] = (relative_cut_off[n] * (left.in_mid.len() as f64 / 2.)) as u32;
        treble_cut_off_bar += 1;

        if (treble_cut_off_bar - bass_cut_off_bar) == 1 {
          first_bar = true;
          if n > 0 {
            buffer_upper_cut_off[n - 1] =
              (relative_cut_off[n] * (left.in_bass.len() as f64 / 2.)) as u32;
          }
        } else {
          first_bar = false;
        }

        if buffer_lower_cut_off[n] > left.in_mid.len() as u32 / 2 {
          buffer_lower_cut_off[n] = left.in_mid.len() as u32 / 2;
        }
      } else {
        // TREBLE
        bar_buffer[n] = 3;
        buffer_lower_cut_off[n] = (relative_cut_off[n] * (left.in_treble.len() as f64 / 2.)) as u32;
        first_treble_bar += 1;
        if first_treble_bar == 1 {
          first_bar = true;
          if n > 0 {
            buffer_upper_cut_off[n - 1] =
              (relative_cut_off[n] * (left.in_mid.len() as f64 / 2.)) as u32;
          }
        } else {
          first_bar = false;
        }

        if buffer_lower_cut_off[n] > left.in_treble.len() as u32 / 2 {
          buffer_lower_cut_off[n] = left.in_treble.len() as u32 / 2;
        }
      }

      if n > 0 {
        if !first_bar {
          buffer_upper_cut_off[n - 1] = buffer_lower_cut_off[n].saturating_sub(1);

          if buffer_lower_cut_off[n] <= buffer_lower_cut_off[n - 1] {
            let mut room_for_more = false;

            if bar_buffer[n] == 1 {
              if buffer_lower_cut_off[n - 1] + 1 < left.in_bass.len() as u32 / 2 + 1 {
                room_for_more = true;
              }
            } else if bar_buffer[n] == 2 {
              if buffer_lower_cut_off[n - 1] + 1 < left.in_mid.len() as u32 / 2 + 1 {
                room_for_more = true;
              }
            } else if bar_buffer[n] == 3
              && buffer_lower_cut_off[n - 1] + 1 < left.in_treble.len() as u32 / 2 + 1
            {
              room_for_more = true;
            }

            if room_for_more {
              // push the spectrum up
              buffer_lower_cut_off[n] = buffer_lower_cut_off[n - 1] + 1;
              buffer_upper_cut_off[n - 1] = buffer_lower_cut_off[n] - 1;

              // calculate new cut off frequency
              if bar_buffer[n] == 1 {
                relative_cut_off[n] =
                  buffer_lower_cut_off[n] as f64 / (left.in_bass.len() as f64 / 2.);
              } else if bar_buffer[n] == 2 {
                relative_cut_off[n] =
                  buffer_lower_cut_off[n] as f64 / (left.in_mid.len() as f64 / 2.);
              } else if bar_buffer[n] == 3 {
                relative_cut_off[n] =
                  buffer_lower_cut_off[n] as f64 / (left.in_treble.len() as f64 / 2.);
              }

              cut_off_frequency[n] = relative_cut_off[n] * (self.sample_rate as f64 / 2.);
            }
          }
        } else if buffer_upper_cut_off[n - 1] <= buffer_lower_cut_off[n - 1] {
          buffer_upper_cut_off[n - 1] = buffer_lower_cut_off[n - 1] + 1;
        }
      }
    }

    Ok(Cava {
      bars_per_channel,
      sample_rate,
      enable_autosens,
      init_sens,
      sens,
      max_sens,
      framerate,
      frame_skip,
      noise_reduction,
      left,
      right,
      bass_hann_window,
      mid_hann_window,
      treble_hann_window,
      input_buffer,
      buffer_lower_cut_off,
      buffer_upper_cut_off,
      eq,
      bass_cut_off_bar,
      treble_cut_off_bar,
      cava_fall,
      cava_mem,
      cava_peak,
      prev_cava_out,
    })
  }
}

struct AudioData<F: FftNum> {
  p_bass: Arc<dyn RealToComplex<F>>,
  p_mid: Arc<dyn RealToComplex<F>>,
  p_treble: Arc<dyn RealToComplex<F>>,

  out_bass: Vec<Complex<F>>,
  out_mid: Vec<Complex<F>>,
  out_treble: Vec<Complex<F>>,

  in_bass: Vec<F>,
  in_mid: Vec<F>,
  in_treble: Vec<F>,
}

impl<F: FftNum> AudioData<F> {
  pub fn new(treble_buffer_size: usize) -> Self {
    let mut planner = RealFftPlanner::new();

    let bass = planner.plan_fft_forward(treble_buffer_size * 8);
    let mid = planner.plan_fft_forward(treble_buffer_size * 4);
    let treble = planner.plan_fft_forward(treble_buffer_size);

    let out_bass = bass.make_output_vec();
    let out_mid = mid.make_output_vec();
    let out_treble = treble.make_output_vec();

    let in_bass = bass.make_input_vec();
    let in_mid = mid.make_input_vec();
    let in_treble = treble.make_input_vec();

    Self {
      p_bass: bass,
      p_mid: mid,
      p_treble: treble,

      out_bass,
      out_mid,
      out_treble,

      in_bass,
      in_mid,
      in_treble,
    }
  }
}

fn compute_hann_window(buffer_size: usize) -> Box<[f64]> {
  let mut hann_window = Vec::with_capacity(buffer_size);

  for i in 0..buffer_size {
    let multiplier =
      0.5 * (1. - (2. * std::f64::consts::PI * i as f64 / (buffer_size as f64 - 1.)).cos());
    hann_window.push(multiplier);
  }

  hann_window.into_boxed_slice()
}

#[cfg(test)]
mod tests {
  mod audio_data {
    use crate::infra::audio::cavacore::AudioData;

    #[test]
    fn buffer_size() {
      let treble_buffer_size = 128 * 4;

      let audio_data: AudioData<f32> = AudioData::new(treble_buffer_size);

      assert_eq!(audio_data.in_bass.len(), treble_buffer_size * 8);
      assert_eq!(audio_data.in_mid.len(), treble_buffer_size * 4);
      assert_eq!(audio_data.in_treble.len(), treble_buffer_size);
    }
  }

  // Pins the vendored autosens patch: sens must ramp while signal is present
  // and hold during digital silence, matching the C original. Upstream 2.0.2
  // had the condition inverted, which left bars near zero when the visualizer
  // opened mid-song (ducking-only sens can never recover from a loud passage).
  mod autosens_patch {
    use crate::infra::audio::cavacore::{Cava, CavaBuilder, Channels};

    fn cava() -> Cava {
      CavaBuilder::default()
        .bars_per_channel(10)
        .sample_rate(44_100)
        .audio_channels(Channels::Mono)
        .build()
        .unwrap()
    }

    #[test]
    fn sens_ramps_while_signal_is_present() {
      let mut cava = cava();
      let mut output = cava.make_output();
      // Non-zero but far too quiet to overshoot: the ramp branch must run.
      let quiet_signal = vec![1.0; 512];

      let before = cava.sens;
      for _ in 0..10 {
        cava.execute(&quiet_signal, &mut output);
      }

      assert!(
        cava.sens > before,
        "sens must rise while signal plays: {} -> {}",
        before,
        cava.sens
      );
    }

    #[test]
    fn sens_stops_ramping_at_the_max_sens_cap() {
      let mut cava = CavaBuilder::default()
        .bars_per_channel(10)
        .sample_rate(44_100)
        .audio_channels(Channels::Mono)
        .max_sens(2.0)
        .build()
        .unwrap();
      let mut output = cava.make_output();
      // Non-zero but far too quiet to overshoot: uncapped, the init ramp
      // (x1.1 per frame) would push sens far past 2.0 within 100 frames.
      let quiet_signal = vec![1.0; 512];

      for _ in 0..100 {
        cava.execute(&quiet_signal, &mut output);
      }

      assert!(cava.sens <= 2.0, "sens exceeded the cap: {}", cava.sens);
    }

    #[test]
    fn sens_holds_during_silence() {
      let mut cava = cava();
      let mut output = cava.make_output();
      let silence = vec![0.0; 512];

      let before = cava.sens;
      for _ in 0..10 {
        cava.execute(&silence, &mut output);
      }

      assert_eq!(
        cava.sens, before,
        "sens must not ramp during digital silence"
      );
    }
  }

  // Pins the other two vendored patches (cutoff truncation, input chunk order).
  mod port_patches {
    use crate::infra::audio::cavacore::{Cava, CavaBuilder, Channels};

    fn cava() -> Cava {
      CavaBuilder::default()
        .bars_per_channel(10)
        .sample_rate(44_100)
        .audio_channels(Channels::Mono)
        .build()
        .unwrap()
    }

    // Upstream's cast-before-multiply truncated every cutoff to 0 and
    // degenerated the mapping into consecutive bins 0,1,2,... With float math
    // (matching C), the top bar of a 10-bar 44.1kHz plan reads treble bin
    // ~136 (~5.9 kHz), nowhere near bin 9.
    #[test]
    fn cutoffs_are_log_spaced_not_consecutive() {
      let cava = cava();
      assert!(
        cava.buffer_lower_cut_off[9] > 100,
        "top bar reads bin {} - cutoffs degenerated to consecutive bins",
        cava.buffer_lower_cut_off[9]
      );
      // Bar 0 starts at 50 Hz: 50/22050 * 4096 = bin 9 of the bass FFT.
      assert_eq!(cava.buffer_lower_cut_off[0], 9);
    }

    // The sliding buffer must stay one continuous time-reversed stream
    // (newest sample at index 0) across execute calls, as in C.
    #[test]
    fn input_buffer_is_a_continuous_time_reversed_stream() {
      let mut cava = cava();
      let mut output = cava.make_output();
      cava.execute(&[1., 2., 3.], &mut output);
      cava.execute(&[4., 5., 6.], &mut output);
      assert_eq!(cava.input_buffer[..6], [6., 5., 4., 3., 2., 1.]);
    }
  }
}
