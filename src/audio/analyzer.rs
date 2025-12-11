use std::sync::{Arc, Mutex};

/// Frequency bands for visualization (12 bands like chromatic scale)
pub const NUM_BANDS: usize = 12;

/// Smoothing factor for spectrum display (0.0 = no smoothing, 1.0 = infinite smoothing)
/// Higher = smoother but slower response, lower = faster but jittery
const SMOOTHING: f32 = 0.5;

/// Base gain for visualization amplitude
const GAIN: f32 = 0.85;

/// FFT window size (power of 2, ~46ms at 44.1kHz)
const FFT_SIZE: usize = 2048;

/// Spectrum data for visualization
#[derive(Clone, Debug)]
pub struct SpectrumData {
  /// Normalized frequency bands (0.0-1.0)
  pub bands: [f32; NUM_BANDS],
  /// Overall peak level (0.0-1.0)
  pub peak: f32,
}

impl Default for SpectrumData {
  fn default() -> Self {
    Self {
      bands: [0.0; NUM_BANDS],
      peak: 0.0,
    }
  }
}

/// Audio analyzer that performs FFT on incoming samples
pub struct AudioAnalyzer {
  fft: Arc<dyn realfft::RealToComplex<f32>>,
  sample_buffer: Vec<f32>,
  fft_input: Vec<f32>,
  fft_output: Vec<realfft::num_complex::Complex<f32>>,
  spectrum: SpectrumData,
  write_pos: usize,
}

impl AudioAnalyzer {
  pub fn new() -> Self {
    let mut planner = realfft::RealFftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let fft_output_len = FFT_SIZE / 2 + 1;

    Self {
      fft,
      sample_buffer: vec![0.0; FFT_SIZE],
      fft_input: vec![0.0; FFT_SIZE],
      fft_output: vec![realfft::num_complex::Complex::default(); fft_output_len],
      spectrum: SpectrumData::default(),
      write_pos: 0,
    }
  }

  /// Push audio samples into the analyzer
  pub fn push_samples(&mut self, samples: &[f32]) {
    for &sample in samples {
      self.sample_buffer[self.write_pos] = sample;
      self.write_pos = (self.write_pos + 1) % FFT_SIZE;
    }
  }

  /// Process buffered samples and update spectrum
  pub fn process(&mut self) -> SpectrumData {
    // Copy samples to FFT input buffer (in order from write position)
    for i in 0..FFT_SIZE {
      let idx = (self.write_pos + i) % FFT_SIZE;
      // Apply Hann window
      let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos());
      self.fft_input[i] = self.sample_buffer[idx] * window;
    }

    // Perform FFT
    if self
      .fft
      .process(&mut self.fft_input, &mut self.fft_output)
      .is_ok()
    {
      self.update_spectrum();
    }

    self.spectrum.clone()
  }

  /// Map FFT bins to frequency bands
  fn update_spectrum(&mut self) {
    let bin_count = self.fft_output.len();

    // Frequency band boundaries (logarithmic scale, roughly musical)
    // Maps to approximately: C(~32Hz) to B(~16kHz) in octave bands
    let band_edges: [usize; NUM_BANDS + 1] = [
      1,
      2,
      4,
      8,
      16,
      32,
      64,
      128,
      256,
      384,
      512,
      768,
      bin_count - 1,
    ];

    let mut new_bands = [0.0f32; NUM_BANDS];
    let mut max_magnitude = 0.0f32;

    for band in 0..NUM_BANDS {
      let start = band_edges[band];
      let end = band_edges[band + 1].min(bin_count);

      if start < end {
        let mut sum = 0.0f32;
        for i in start..end {
          let magnitude = self.fft_output[i].norm();
          sum += magnitude;
          max_magnitude = max_magnitude.max(magnitude);
        }
        new_bands[band] = sum / (end - start) as f32;
      }
    }

    // Per-band gain for visualization - boosted highs to compensate for low energy
    const BAND_GAINS: [f32; NUM_BANDS] = [
      0.7, // Sub   - reduce sub rumble
      0.8, // Bass  - visible but not overwhelming
      0.9, // Low   - building up
      1.0, // LMid  - full
      1.0, // Mid   - peak visibility (voice frequencies)
      1.0, // UMid  - peak visibility
      1.1, // High  - boost slightly
      1.2, // HiMd  - boost more
      1.3, // Pres  - boost more
      1.4, // Bril  - boost more
      1.6, // Air   - significant boost
      2.0, // Ultra - major boost (very little energy here)
    ];

    // Normalize and apply pleasing visual curve
    if max_magnitude > 0.0 {
      for (i, band) in new_bands.iter_mut().enumerate() {
        // Normalize, apply per-band curve, then global gain
        let normalized = (*band / max_magnitude) * BAND_GAINS[i] * GAIN;
        // Square root gives more pleasing response (dB-like scaling)
        let scaled = normalized.sqrt();
        // Gentle limiting - cap at 85% so bars never hit the top
        *band = scaled.min(0.85);
      }
    }

    // Apply smoothing for butter-smooth animation
    for (i, new_band) in new_bands.iter().enumerate() {
      self.spectrum.bands[i] = self.spectrum.bands[i] * SMOOTHING + *new_band * (1.0 - SMOOTHING);
      // Noise gate: treat very low values as zero
      if self.spectrum.bands[i] < 0.005 {
        self.spectrum.bands[i] = 0.0;
      }
    }

    // Update peak with smoothing and noise gate
    let current_peak = new_bands.iter().cloned().fold(0.0f32, f32::max);
    self.spectrum.peak = self.spectrum.peak * SMOOTHING + current_peak * (1.0 - SMOOTHING);
    if self.spectrum.peak < 0.005 {
      self.spectrum.peak = 0.0;
    }
  }
}

/// Thread-safe wrapper for AudioAnalyzer
pub type SharedAnalyzer = Arc<Mutex<AudioAnalyzer>>;

pub fn create_shared_analyzer() -> SharedAnalyzer {
  Arc::new(Mutex::new(AudioAnalyzer::new()))
}
