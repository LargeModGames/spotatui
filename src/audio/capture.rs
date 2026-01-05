use super::analyzer::{create_shared_analyzer, SharedAnalyzer, SpectrumData};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(not(target_os = "windows"))]
use cpal::BufferSize;
use cpal::{Device, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Manages audio capture from system output (loopback)
pub struct AudioCaptureManager {
  _stream: Stream,
  analyzer: SharedAnalyzer,
  active: Arc<AtomicBool>,
}

impl AudioCaptureManager {
  /// Create a new audio capture manager
  /// Returns None if no suitable audio device is found
  pub fn new() -> Option<Self> {
    let host = cpal::default_host();

    // Try to find a loopback/monitor device
    let device = Self::find_loopback_device(&host)?;

    // Device name is now stored internally for debugging if needed
    let _device_name = device
      .description()
      .ok()
      .map(|description| description.name().to_string());

    // Get a compatible config that won't interfere with playback
    let config = Self::get_compatible_config(&device)?;

    let analyzer = create_shared_analyzer();
    let active = Arc::new(AtomicBool::new(true));

    let stream = Self::build_stream(&device, &config, analyzer.clone(), active.clone())?;

    // Start the stream
    if stream.play().is_err() {
      return None;
    }

    Some(Self {
      _stream: stream,
      analyzer,
      active,
    })
  }

  /// Get the current spectrum data
  pub fn get_spectrum(&self) -> Option<SpectrumData> {
    if !self.active.load(Ordering::Relaxed) {
      return None;
    }

    if let Ok(mut analyzer) = self.analyzer.lock() {
      Some(analyzer.process())
    } else {
      None
    }
  }

  /// Check if audio capture is currently active
  pub fn is_active(&self) -> bool {
    self.active.load(Ordering::Relaxed)
  }

  /// Find a suitable loopback/monitor device
  /// Prefers: default output monitor > bluetooth > speakers > HDMI
  fn find_loopback_device(host: &cpal::Host) -> Option<Device> {
    #[cfg(target_os = "windows")]
    {
      // On Windows, WASAPI supports loopback on output devices
      host.default_output_device()
    }

    #[cfg(target_os = "linux")]
    {
      // On Linux with PipeWire/PulseAudio, look for a monitor device
      // Priority: bluetooth first, then speakers, then anything else
      if let Ok(devices) = host.input_devices() {
        let mut monitors: Vec<Device> = Vec::new();

        // Scan available input devices

        for device in devices {
          if let Ok(description) = device.description() {
            let name_lower = description.name().to_lowercase();
            if name_lower.contains("monitor") {
              monitors.push(device);
            }
          }
        }

        // If no monitors found, will fall through to default input device

        // Sort by priority: bluetooth first, then speakers, then anything else
        monitors.sort_by_key(|d| {
          let name_lower = d
            .description()
            .map(|description| description.name().to_lowercase())
            .unwrap_or_default();
          if name_lower.contains("bluez") || name_lower.contains("bluetooth") {
            return 0; // Highest priority - likely the active wireless device
          }
          if name_lower.contains("speaker") || name_lower.contains("analog") {
            return 1; // Second priority - built-in speakers
          }
          if name_lower.contains("hdmi") {
            return 3; // Low priority - usually not used for music
          }
          2 // Default priority
        });

        if let Some(device) = monitors.into_iter().next() {
          return Some(device);
        }
      }

      // Fallback: try default input device - on PipeWire systems this might
      // route correctly, but may be the microphone on pure ALSA systems.
      // User can disable audio-viz feature if this causes issues.
      if let Some(device) = host.default_input_device() {
        return Some(device);
      }

      None
    }

    #[cfg(target_os = "macos")]
    {
      // On macOS, audio loopback requires virtual audio device
      if let Some(device) = host.default_input_device() {
        return Some(device);
      }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
      host.default_input_device()
    }

    #[cfg(target_os = "macos")]
    None
  }

  /// Get a stream config that won't interfere with playback
  /// Uses default buffer size to let the audio server manage timing
  fn get_compatible_config(device: &Device) -> Option<StreamConfig> {
    #[cfg(target_os = "windows")]
    {
      if let Ok(config) = device.default_output_config() {
        return Some(config.into());
      }
    }

    #[cfg(not(target_os = "windows"))]
    {
      // Get the default config and ensure we use default buffer size
      if let Ok(config) = device.default_input_config() {
        let stream_config = StreamConfig {
          channels: config.channels(),
          sample_rate: config.sample_rate(),
          // Use Default buffer size - let PipeWire/PulseAudio manage this
          // This is critical for avoiding audio interference
          buffer_size: BufferSize::Default,
        };
        return Some(stream_config);
      }
    }

    None
  }

  /// Build the audio input stream
  fn build_stream(
    device: &Device,
    config: &StreamConfig,
    analyzer: SharedAnalyzer,
    active: Arc<AtomicBool>,
  ) -> Option<Stream> {
    let channels = config.channels as usize;
    let active_clone = active.clone();

    let data_callback = move |data: &[f32], _: &cpal::InputCallbackInfo| {
      // Convert to mono by averaging channels
      let mono_samples: Vec<f32> = data
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

      if let Ok(mut analyzer) = analyzer.lock() {
        analyzer.push_samples(&mono_samples);
      }
    };

    let error_callback = move |_err: cpal::StreamError| {
      // Silently deactivate on error to avoid corrupting TUI
      active_clone.store(false, Ordering::Relaxed);
    };

    device
      .build_input_stream(config, data_callback, error_callback, None)
      .ok()
  }
}

impl Drop for AudioCaptureManager {
  fn drop(&mut self) {
    self.active.store(false, Ordering::Relaxed);
  }
}
