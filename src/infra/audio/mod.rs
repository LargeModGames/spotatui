// Audio capture and analysis module for real-time visualization
// This module provides cross-platform audio capture:
// - Linux: PipeWire native (via pipewire-rs)
// - Windows/macOS: cpal (WASAPI/CoreAudio)
//
// It also hosts the shared decoded-audio output engine (`LocalPlayer`), used by
// any source that plays through the local rodio sink (local files, Subsonic, Qobuz).

// Shared decode/output engine. Gated on `audio-decode`, which `local-files`,
// `subsonic` and `qobuz` pull in, so the player is reachable from each source.
#[cfg(feature = "audio-decode")]
mod player;
#[cfg(feature = "audio-decode")]
pub use player::LocalPlayer;
#[cfg(feature = "qobuz")]
pub use player::PreparedStream;
#[cfg(feature = "audio-decode-queue")]
pub use player::Reopen;

#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
mod analyzer;

// The GUI player bar's spectrum.
#[cfg(all(
  feature = "gui",
  any(feature = "audio-viz", feature = "audio-viz-cpal")
))]
mod winamp;
#[cfg(all(
  feature = "gui",
  any(feature = "audio-viz", feature = "audio-viz-cpal")
))]
pub use winamp::{scope, WinampSpectrum};

/// The gain for a volume percent: librespot's `VolumeCtrl::Log` mapping over
/// a 60 dB range (`1000^(p/100 - 1)`), which the native player uses by default.
/// 0% is silence, 100% is the file level, and 80% is about -12 dB.
#[cfg(any(
  feature = "audio-decode",
  all(
    feature = "gui",
    any(feature = "audio-viz", feature = "audio-viz-cpal")
  )
))]
pub fn volume_gain(percent: u8) -> f32 {
  const DB_RATIO: f64 = 1000.0;
  match percent {
    0 => 0.0,
    p if p >= 100 => 1.0,
    p => ((f64::from(p) / 100.0 - 1.0) * DB_RATIO.ln()).exp() as f32,
  }
}

// Vendored cava DSP engine (see cavacore/mod.rs for provenance and the local
// patches). Lint allowances instead of cleanups keep it diffable against
// upstream: it carries items our integration never constructs.
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
#[allow(dead_code, clippy::all)]
mod cavacore;

// Platform-specific capture backends
#[cfg(all(feature = "audio-viz", target_os = "linux"))]
mod pipewire_capture;

#[cfg(feature = "audio-viz-cpal")]
#[allow(dead_code)]
mod capture;

// Re-export the appropriate capture manager based on platform
#[cfg(all(feature = "audio-viz", target_os = "linux"))]
pub use pipewire_capture::PipeWireCapture as AudioCaptureManager;

// On Linux with audio-viz, use cpal only if audio-viz is not enabled
// This prevents conflict when --all-features is used
#[cfg(all(
  feature = "audio-viz-cpal",
  not(all(feature = "audio-viz", target_os = "linux"))
))]
pub use capture::AudioCaptureManager;

// Re-export SpectrumData
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
pub use analyzer::RecentAudio;
#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
#[allow(unused_imports)]
pub use analyzer::SpectrumData;

// Fallback types when no audio-viz feature is enabled
#[cfg(not(any(
  all(feature = "audio-viz", target_os = "linux"),
  feature = "audio-viz-cpal"
)))]
#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct SpectrumData {
  pub bands: Vec<f32>,
}

#[cfg(not(any(
  all(feature = "audio-viz", target_os = "linux"),
  feature = "audio-viz-cpal"
)))]
#[allow(dead_code)]
pub struct AudioCaptureManager;

#[cfg(not(any(
  all(feature = "audio-viz", target_os = "linux"),
  feature = "audio-viz-cpal"
)))]
#[allow(dead_code)]
impl AudioCaptureManager {
  pub fn new(_display_bars: usize) -> Option<Self> {
    None
  }

  pub fn get_spectrum(&self, _desired_bars: usize) -> Option<SpectrumData> {
    None
  }

  pub fn is_active(&self) -> bool {
    false
  }
}

#[cfg(all(
  test,
  any(
    feature = "audio-decode",
    all(
      feature = "gui",
      any(feature = "audio-viz", feature = "audio-viz-cpal")
    )
  )
))]
mod tests {
  use super::*;

  #[test]
  fn volume_gain_follows_the_native_log_curve() {
    assert_eq!(volume_gain(0), 0.0);
    assert_eq!(volume_gain(100), 1.0);
    assert_eq!(volume_gain(150), 1.0);
    let at_80 = volume_gain(80);
    assert!(
      (at_80 - 0.251).abs() < 0.001,
      "80% is about -12 dB: {at_80}"
    );
    let at_50 = volume_gain(50);
    assert!(
      (at_50 - 0.0316).abs() < 0.001,
      "50% is about -30 dB: {at_50}"
    );
    assert!(volume_gain(20) < at_50 && at_50 < at_80);
  }
}
