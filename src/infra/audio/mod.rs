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

#[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
mod analyzer;

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
