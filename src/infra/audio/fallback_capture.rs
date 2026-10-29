// Linux with both capture backends: PipeWire's sink monitor first, and cpal's
// ALSA capture when no PipeWire server answers (plain PulseAudio or ALSA, or
// a proot container on Android that reaches PulseAudio through ALSA).

use super::analyzer::{RecentAudio, SpectrumData};
use super::capture::AudioCaptureManager as CpalCapture;
use super::pipewire_capture::PipeWireCapture;

pub enum AudioCaptureManager {
  PipeWire(PipeWireCapture),
  Cpal(CpalCapture),
}

impl AudioCaptureManager {
  /// Opens PipeWire capture, or cpal capture when PipeWire fails to start.
  pub fn new(display_bars: usize) -> Option<Self> {
    PipeWireCapture::new(display_bars)
      .map(Self::PipeWire)
      .or_else(|| {
        log::info!("[audio-viz] trying the ALSA capture instead of PipeWire");
        CpalCapture::new(display_bars).map(Self::Cpal)
      })
  }

  pub fn get_spectrum(&self, desired_bars: usize) -> Option<SpectrumData> {
    match self {
      Self::PipeWire(capture) => capture.get_spectrum(desired_bars),
      Self::Cpal(capture) => capture.get_spectrum(desired_bars),
    }
  }

  #[cfg_attr(not(feature = "gui"), allow(dead_code))]
  pub fn recent_audio(&self) -> Option<RecentAudio> {
    match self {
      Self::PipeWire(capture) => capture.recent_audio(),
      Self::Cpal(capture) => capture.recent_audio(),
    }
  }

  pub fn is_active(&self) -> bool {
    match self {
      Self::PipeWire(capture) => capture.is_active(),
      Self::Cpal(capture) => capture.is_active(),
    }
  }
}
