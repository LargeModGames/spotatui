//! Own librespot output sink for Windows, replacing the fork's rodio one.
//!
//! The fork's `RodioSink` has three faults when the output device is unplugged:
//! - rodio's default error callback `eprintln!`s into the TUI
//! - `write` spins on `Sink::len() > 26` forever (a dead device never drains
//! - `stop` calls `sleep_until_end()`. This sink stays silent, notices `DeviceNotAvailable`
//!   and bounds every wait, so `RecoveringSink` sees an `Err` and recovers.

use librespot_playback::{
  audio_backend::{Sink, SinkError, SinkResult},
  config::AudioFormat,
  convert::Converter,
  decoder::AudioPacket,
  NUM_CHANNELS, SAMPLE_RATE,
};
use log::{debug, info, warn};
use rodio::cpal::traits::HostTrait;
use rodio::cpal::{Device, SampleFormat, StreamError, SupportedStreamConfig};
use rodio::{ChannelCount, DeviceSinkBuilder, DeviceTrait, Player};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Queued buffers above which `write` waits.
const MAX_QUEUED: usize = 26;
/// How long the queue may sit without shrinking before `write` gives up.
const WRITE_STALL_LIMIT: Duration = Duration::from_secs(2);
const STOP_STALL_LIMIT: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(10);
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// What a bounded wait on the queue should do next.
#[derive(Debug, PartialEq, Eq)]
enum WaitDecision {
  /// The queue is at or under the threshold.
  Done,
  KeepWaiting,
  /// The device was removed.
  Lost,
  /// No progress for the whole stall limit.
  Stalled,
}

fn wait_decision(
  queued: usize,
  threshold: usize,
  lost: bool,
  stalled_for: Duration,
  stall_limit: Duration,
) -> WaitDecision {
  if lost {
    WaitDecision::Lost
  } else if queued <= threshold {
    WaitDecision::Done
  } else if stalled_for >= stall_limit {
    WaitDecision::Stalled
  } else {
    WaitDecision::KeepWaiting
  }
}

/// Wait until at most `threshold` buffers are queued. Never blocks for longer
/// than `stall_limit` without the queue shrinking.
fn wait_for_queue(
  player: &Player,
  lost: &AtomicBool,
  threshold: usize,
  stall_limit: Duration,
) -> WaitDecision {
  let mut best = usize::MAX;
  let mut progress_at = Instant::now();
  loop {
    let queued = player.len();
    if queued < best {
      best = queued;
      progress_at = Instant::now();
    }
    let decision = wait_decision(
      queued,
      threshold,
      lost.load(Ordering::Relaxed),
      progress_at.elapsed(),
      stall_limit,
    );
    if decision != WaitDecision::KeepWaiting {
      return decision;
    }
    std::thread::sleep(POLL);
  }
}

pub struct SpotatuiSink {
  player: Player,
  lost: Arc<AtomicBool>,
  /// Dropping this lets the device thread release the stream.
  _keepalive: mpsc::Sender<()>,
}

/// Stands in when the device cannot be opened, so the failure reaches
/// `RecoveringSink` as an error instead of a panic.
struct UnavailableSink(String);

impl Sink for UnavailableSink {
  fn start(&mut self) -> SinkResult<()> {
    Err(SinkError::ConnectionRefused(self.0.clone()))
  }
  fn write(&mut self, _: AudioPacket, _: &mut Converter) -> SinkResult<()> {
    Err(SinkError::NotConnected(self.0.clone()))
  }
}

/// Builder with the signature of `audio_backend::SinkBuilder`.
pub fn mk_spotatui_sink(device: Option<String>, format: AudioFormat) -> Box<dyn Sink> {
  match SpotatuiSink::open(device, format) {
    Ok(sink) => Box::new(sink),
    Err(e) => {
      warn!("Could not open audio output: {e}");
      Box::new(UnavailableSink(e))
    }
  }
}

fn sample_format(format: AudioFormat) -> SampleFormat {
  match format {
    AudioFormat::F64 => SampleFormat::F64,
    AudioFormat::F32 => SampleFormat::F32,
    AudioFormat::S32 => SampleFormat::I32,
    AudioFormat::S24 | AudioFormat::S24_3 => SampleFormat::I24,
    AudioFormat::S16 => SampleFormat::I16,
  }
}

/// Picks the output device and the config to open it with. Called on the audio
/// thread so the lookups are bounded by `OPEN_TIMEOUT` too.
fn select_device_and_config(
  requested: Option<&str>,
) -> Result<(Device, SupportedStreamConfig), String> {
  let host = rodio::cpal::default_host();
  let device = match requested {
    Some(name) => host
      .output_devices()
      .map_err(|e| format!("cannot list audio devices: {e}"))?
      .find(|d| d.description().is_ok_and(|desc| desc.name() == name))
      .ok_or_else(|| format!("audio device \"{name}\" is not available"))?,
    None => host
      .default_output_device()
      .ok_or_else(|| "no audio output device available".to_string())?,
  };
  info!(
    "Using audio device: {}",
    device
      .description()
      .map(|d| d.name().to_string())
      .unwrap_or_else(|_| "[unknown name]".into())
  );

  // Native stereo 44.1 kHz first, then the device's default rate (rodio
  // resamples), then whatever the default config is.
  let default_config = device
    .default_output_config()
    .map_err(|e| format!("no default output config: {e}"))?;
  let config = device
    .supported_output_configs()
    .map_err(|e| format!("cannot read output configs: {e}"))?
    .find(|c| c.channels() == u16::from(NUM_CHANNELS))
    .and_then(|c| {
      c.try_with_sample_rate(SAMPLE_RATE)
        .or_else(|| c.try_with_sample_rate(default_config.sample_rate()))
    })
    .unwrap_or(default_config);
  Ok((device, config))
}

impl SpotatuiSink {
  fn open(requested: Option<String>, format: AudioFormat) -> Result<Self, String> {
    let lost = Arc::new(AtomicBool::new(false));
    let on_error = {
      let lost = Arc::clone(&lost);
      move |err: StreamError| {
        // rodio's default callback `eprintln!`s, which corrupts the TUI.
        if matches!(err, StreamError::DeviceNotAvailable) {
          lost.store(true, Ordering::Relaxed);
          warn!("Audio output device was removed");
        } else {
          debug!("audio stream error: {err}");
        }
      }
    };

    let (init_tx, init_rx) = mpsc::channel::<Result<Player, String>>();
    let (keepalive_tx, keepalive_rx) = mpsc::channel::<()>();
    // The device lookup and config queries also run on this thread, so
    // OPEN_TIMEOUT bounds the entire open. cpal streams are not `Send` on
    // WASAPI: open and own it on its own thread.
    std::thread::Builder::new()
      .name("spotatui-stream-audio".to_string())
      .spawn(move || {
        let (device, config) = match select_device_and_config(requested.as_deref()) {
          Ok(selected) => selected,
          Err(e) => {
            let _ = init_tx.send(Err(e));
            return;
          }
        };
        let opened = DeviceSinkBuilder::from_device(device)
          .map(|b| {
            b.with_config(&config.config())
              .with_sample_format(sample_format(format))
              .with_error_callback(on_error)
          })
          .and_then(|b| b.open_sink_or_fallback());
        match opened {
          Ok(mut stream) => {
            // Its drop warning goes to stderr by default.
            stream.log_on_drop(false);
            let player = Player::connect_new(stream.mixer());
            if init_tx.send(Ok(player)).is_err() {
              return;
            }
            let _ = keepalive_rx.recv();
          }
          Err(e) => {
            let _ = init_tx.send(Err(e.to_string()));
          }
        }
      })
      .map_err(|e| format!("spawning audio thread: {e}"))?;

    let player = init_rx
      .recv_timeout(OPEN_TIMEOUT)
      .map_err(|_| "audio output thread did not open a device".to_string())??;
    debug!("Streaming audio sink was created");
    Ok(Self {
      player,
      lost,
      _keepalive: keepalive_tx,
    })
  }

  fn removed() -> SinkError {
    SinkError::OnWrite("audio output device was removed".to_string())
  }
}

impl Sink for SpotatuiSink {
  fn start(&mut self) -> SinkResult<()> {
    if self.lost.load(Ordering::Relaxed) {
      return Err(Self::removed());
    }
    self.player.play();
    Ok(())
  }

  fn stop(&mut self) -> SinkResult<()> {
    // Let the tail play out, but never wait on a device that cannot.
    wait_for_queue(&self.player, &self.lost, 0, STOP_STALL_LIMIT);
    self.player.pause();
    Ok(())
  }

  fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
    if self.lost.load(Ordering::Relaxed) {
      return Err(Self::removed());
    }
    let samples = packet
      .samples()
      .map_err(|e| SinkError::OnWrite(e.to_string()))?;
    let samples_f32: Vec<f32> = converter.f64_to_f32(samples).to_vec();
    let (Some(channels), Some(rate)) = (
      ChannelCount::new(u16::from(NUM_CHANNELS)),
      std::num::NonZero::new(SAMPLE_RATE),
    ) else {
      return Err(SinkError::InvalidParams("bad audio format".to_string()));
    };
    self.player.append(rodio::buffer::SamplesBuffer::new(
      channels,
      rate,
      samples_f32,
    ));

    match wait_for_queue(&self.player, &self.lost, MAX_QUEUED, WRITE_STALL_LIMIT) {
      WaitDecision::Lost => Err(Self::removed()),
      WaitDecision::Stalled => Err(SinkError::OnWrite(
        "audio output stopped consuming samples".to_string(),
      )),
      WaitDecision::Done | WaitDecision::KeepWaiting => Ok(()),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const LIMIT: Duration = Duration::from_secs(2);

  #[test]
  fn a_queue_under_the_threshold_is_done() {
    assert_eq!(
      wait_decision(5, 26, false, Duration::ZERO, LIMIT),
      WaitDecision::Done
    );
  }

  #[test]
  fn a_full_queue_keeps_waiting_while_progress_is_recent() {
    assert_eq!(
      wait_decision(30, 26, false, Duration::from_millis(500), LIMIT),
      WaitDecision::KeepWaiting
    );
  }

  #[test]
  fn a_removed_device_errors_at_once_even_with_a_full_queue() {
    assert_eq!(
      wait_decision(30, 26, true, Duration::ZERO, LIMIT),
      WaitDecision::Lost
    );
  }

  #[test]
  fn a_full_queue_with_no_progress_for_the_limit_stalls() {
    assert_eq!(
      wait_decision(30, 26, false, LIMIT, LIMIT),
      WaitDecision::Stalled
    );
  }

  #[test]
  fn an_empty_queue_is_done_for_the_stop_drain() {
    assert_eq!(
      wait_decision(0, 0, false, Duration::from_secs(9), LIMIT),
      WaitDecision::Done
    );
  }

  #[test]
  fn an_unopenable_device_sink_fails_start_instead_of_panicking() {
    let mut sink = UnavailableSink("gone".into());
    assert!(sink.start().is_err());
  }

  #[test]
  #[ignore = "requires an audio output device"]
  fn the_default_device_opens_and_stops_without_hanging() {
    let mut sink = SpotatuiSink::open(None, AudioFormat::default()).expect("open default device");
    sink.start().unwrap();
    sink.stop().unwrap();
  }
}
