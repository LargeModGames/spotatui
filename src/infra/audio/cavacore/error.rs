use std::fmt;

#[derive(Debug, Clone)]
pub enum Error {
  TooHighAmountBars {
    amount_bars: usize,
    sample_rate: u32,
    max_amount_bars: usize,
  },

  InvalidNoiseReduction(f64),

  EmptyFreqRange {
    start: u32,
    end: u32,
  },

  NyquistIssue {
    freq: u32,
    max_freq: u32,
  },

  // The three variants below replace upstream's bounded/non-zero parameter
  // types (see the module-level divergence notes).
  InvalidSampleRate(u32),

  ZeroBars,

  ZeroFreqStart,
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Error::TooHighAmountBars {
        amount_bars,
        sample_rate,
        max_amount_bars,
      } => write!(
        f,
        "{amount_bars} bars for a sample rate of {sample_rate} can't be more than {max_amount_bars} bars"
      ),
      Error::InvalidNoiseReduction(value) => write!(
        f,
        "Noise reduction has to be within the range [0..1]. Your value: {value}."
      ),
      Error::EmptyFreqRange { start, end } => write!(
        f,
        "Frequency start and end frequency musn't be equal but given frequency range: {start}..{end} (Hz)"
      ),
      Error::NyquistIssue { freq, max_freq } => write!(
        f,
        "Due to the Nyquist sampling theorem the highesest frequency cutoff does not exceed 'sample rate' / 2 (= {max_freq}) but you set it to {freq}"
      ),
      Error::InvalidSampleRate(rate) => write!(
        f,
        "Sample rate has to be within the range 1..=384000. Your value: {rate}."
      ),
      Error::ZeroBars => write!(f, "The amount of bars per channel mustn't be zero"),
      Error::ZeroFreqStart => write!(f, "The frequency range mustn't start at 0 Hz"),
    }
  }
}

impl std::error::Error for Error {}
