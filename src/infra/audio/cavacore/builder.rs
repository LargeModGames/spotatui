use std::ops::Range;

use crate::infra::audio::cavacore::{Channels, Error};

/// The recommended default noise reduction value.
/// Is automatically set if a new [`CavaBuilder`] is created.
pub const DEFAULT_NOISE_REDUCTION: f64 = 0.77;

/// The sample rate cap upstream encoded in its `SampleRate` bounded type;
/// enforced by `sanity_check` instead (see the module-level divergence notes).
const MAX_SAMPLE_RATE: u32 = 384_000;

#[derive(Debug, Clone)]
pub struct CavaBuilder {
  pub(crate) bars_per_channel: usize,
  pub(crate) sample_rate: u32,
  pub(crate) audio_channels: Channels,
  pub(crate) enable_autosens: bool,
  pub(crate) noise_reduction: f64,
  pub(crate) freq_range: Range<u32>,
  pub(crate) max_sens: f64,
}

impl CavaBuilder {
  pub fn bars_per_channel(mut self, bars_per_channel: usize) -> Self {
    self.bars_per_channel = bars_per_channel;
    self
  }

  pub fn sample_rate(mut self, sample_rate: u32) -> Self {
    self.sample_rate = sample_rate;
    self
  }

  pub fn audio_channels(mut self, audio_channels: Channels) -> Self {
    self.audio_channels = audio_channels;
    self
  }

  pub fn enable_autosens(mut self, enable_autosens: bool) -> Self {
    self.enable_autosens = enable_autosens;
    self
  }

  /// Adjust noise reduciton filters. Has to be within the range `0..=1`.
  ///
  /// The raw visualization is very noisy, this factor adjusts the integral
  /// and gravity filters inside cavacore to keep the signal smooth:
  /// `1` will be very slow and smooth, `0` will be fast but noisy.
  pub fn noise_reduction(mut self, noise_reduction: f64) -> Self {
    self.noise_reduction = noise_reduction;
    self
  }

  pub fn frequency_range(mut self, freq_range: Range<u32>) -> Self {
    self.freq_range = freq_range;
    self
  }

  /// EXTENSION(spotatui): cap the autosens gain so near-silent input cannot be
  /// amplified all the way to full-scale bars. Upstream and C cava have no
  /// such cap (the default, infinity, keeps their behavior).
  pub fn max_sens(mut self, max_sens: f64) -> Self {
    self.max_sens = max_sens;
    self
  }

  pub fn sanity_check(&self) -> Result<(), Vec<Error>> {
    let mut errors = Vec::new();
    let treble_buffer_size = self.compute_treble_buffer_size();

    // These three checks replace upstream's NonZeroUsize / SampleRate /
    // NonZeroU32 parameter types.
    if self.bars_per_channel == 0 {
      errors.push(Error::ZeroBars);
    }

    if self.sample_rate == 0 || self.sample_rate > MAX_SAMPLE_RATE {
      errors.push(Error::InvalidSampleRate(self.sample_rate));
    }

    if self.freq_range.start == 0 {
      errors.push(Error::ZeroFreqStart);
    }

    if self.bars_per_channel > treble_buffer_size / 2 + 1 {
      errors.push(Error::TooHighAmountBars {
        amount_bars: self.bars_per_channel,
        sample_rate: self.sample_rate,
        max_amount_bars: treble_buffer_size / 2 + 1,
      });
    }

    if self.freq_range.is_empty() {
      errors.push(Error::EmptyFreqRange {
        start: self.freq_range.start,
        end: self.freq_range.end,
      });
    }

    if self.freq_range.end > self.sample_rate / 2 {
      errors.push(Error::NyquistIssue {
        freq: self.freq_range.end,
        max_freq: self.sample_rate / 2,
      });
    }

    if !errors.is_empty() {
      return Err(errors);
    }

    Ok(())
  }

  pub(crate) fn compute_treble_buffer_size(&self) -> usize {
    treble_buffer_size(self.sample_rate)
  }
}

/// The treble FFT window cavacore picks for a sample rate. The bass window (and
/// so the whole sliding input buffer for mono audio) is 8x this.
pub fn treble_buffer_size(sample_rate: u32) -> usize {
  let factor = if sample_rate <= 8_125 {
    1
  } else if sample_rate <= 16_250 {
    2
  } else if sample_rate <= 32_500 {
    4
  } else if sample_rate <= 75_000 {
    8
  } else if sample_rate <= 150_000 {
    16
  } else if sample_rate <= 300_000 {
    32
  } else {
    64
  };

  factor * 128
}

impl Default for CavaBuilder {
  fn default() -> Self {
    Self {
      bars_per_channel: 32,
      sample_rate: 44_100,
      audio_channels: Channels::Stereo,
      enable_autosens: true,
      noise_reduction: DEFAULT_NOISE_REDUCTION,
      freq_range: 50..10_000,
      max_sens: f64::INFINITY,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::CavaBuilder;

  #[test]
  fn sample_rate() {
    assert!(CavaBuilder::default()
      .sample_rate(0)
      .sanity_check()
      .is_err());
    assert!(CavaBuilder::default()
      .sample_rate(384_001)
      .sanity_check()
      .is_err());
  }

  #[test]
  fn treble_buffer_size() {
    let mut builder = CavaBuilder::default();
    builder = builder.sample_rate(8_125);

    assert_eq!(128, builder.compute_treble_buffer_size());

    builder = builder.sample_rate(16_250);
    assert_eq!(128 * 2, builder.compute_treble_buffer_size());

    builder = builder.sample_rate(32_500);
    assert_eq!(128 * 4, builder.compute_treble_buffer_size());

    builder = builder.sample_rate(75_000);
    assert_eq!(128 * 8, builder.compute_treble_buffer_size());

    builder = builder.sample_rate(150_000);
    assert_eq!(128 * 16, builder.compute_treble_buffer_size());

    builder = builder.sample_rate(300_000);
    assert_eq!(128 * 32, builder.compute_treble_buffer_size());

    builder = builder.sample_rate(300_001);
    assert_eq!(128 * 64, builder.compute_treble_buffer_size());
  }
}
