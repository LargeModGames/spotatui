//! Local-file audio playback engine.
//!
//! [`LocalPlayer`] decodes an audio file and plays it through the system's
//! default output device, independent of librespot. The decoded audio reaches
//! the same default device the visualizer's loopback/monitor captures, so the
//! spectrum visualizer works for local playback without any extra wiring.
//!
//! ## Encapsulation
//!
//! All `rodio` types are contained inside this module. The public surface is a
//! small transport API (`play_file`/`pause`/`resume`/`seek`/`position`/
//! `is_finished`/`set_volume`) that speaks `std` types only. This keeps the
//! runtime free of `rodio` and makes the platform-specific output swap below a
//! single-file change.
//!
//! ## Cross-platform output
//!
//! `rodio` is the project-validated output backend on **Linux and Windows**
//! (librespot itself uses `rodio-backend` there). On **macOS** rodio crashes on
//! CoreAudio/Bluetooth devices (the reason librespot uses `portaudio-backend`
//! there; see issues #9/#20), so [`LocalPlayer::new`] refuses to construct an
//! output on macOS and returns a clear error. A macOS output path (portaudio or
//! a direct CoreAudio backend) is a tracked follow-up that needs a Mac to test.
//!
//! ## Threading
//!
//! `rodio::OutputStream` is `!Send`, so it cannot live on the shared player
//! struct (which is held behind an `Arc` across async tasks). Instead a
//! dedicated thread owns the `OutputStream` and keeps it alive; the player holds
//! only the `Send + Sync` [`rodio::Sink`] plus a keepalive channel whose drop
//! tells the thread to release the device.

use std::io::BufReader;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use rodio::{Decoder, Sink};

/// An audio player for local files, driving the system default output device.
///
/// Cheap to hold behind an `Arc`: the heavy `OutputStream` lives on a dedicated
/// thread, and the `Sink` here is a lightweight `Arc`-backed handle.
pub struct LocalPlayer {
  sink: Sink,
  /// Dropping this sender signals the audio thread to drop its `OutputStream`
  /// and release the audio device. Held for the player's lifetime.
  _keepalive: mpsc::Sender<()>,
}

impl LocalPlayer {
  /// Open the default audio output device and return a ready player.
  ///
  /// **Blocking:** waits for the audio thread to open the device. Call once at
  /// setup, off any latency-sensitive path. Returns an error if no output
  /// device is available (e.g. headless CI) or on macOS (see module docs).
  pub fn new() -> Result<Self> {
    let (sink, keepalive) = open_sink()?;
    // Start silent; nothing is queued until the first `play_file`.
    sink.pause();
    Ok(Self {
      sink,
      _keepalive: keepalive,
    })
  }

  /// Decode the file at `path` and play it, replacing whatever was playing.
  ///
  /// The format is detected from the file's content (FLAC/MP3/MP4-AAC/Vorbis/
  /// WAV are supported by default). Returns an error if the file cannot be
  /// opened or its format is unsupported.
  ///
  /// Only the tempfile-based sources play files; a build with just
  /// `internet-radio` uses [`play_stream`](Self::play_stream) instead.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic")),
    allow(dead_code)
  )]
  pub fn play_file(&self, path: &Path) -> Result<()> {
    // Stop whatever is currently playing *before* any fallible step (open or
    // decode), so a failure here can never leave the previous track audible. A
    // manual Next/Previous into a missing or undecodable file must fall silent:
    // `play_index`'s failure arm relies on the sink draining here so the runner
    // tick's `is_finished()` fires and auto-advance skips past the bad file
    // instead of dead-ending on a stale, still-playing track.
    self.sink.clear();

    let file = std::fs::File::open(path)
      .with_context(|| format!("opening audio file {}", path.display()))?;

    // On decode error we return with the sink already empty (no old track
    // playing); on success we append and start the new source below.
    let decoder = Decoder::new(BufReader::new(file))
      .with_context(|| format!("decoding audio file {}", path.display()))?;

    self.sink.append(decoder);
    self.sink.play();
    Ok(())
  }

  /// Decode an already-opened **live stream** and play it, replacing whatever
  /// was playing.
  ///
  /// Unlike [`play_file`](Self::play_file) the reader is treated as
  /// non-seekable: the decoder is built with `with_seekable(false)` so the
  /// symphonia probe never issues the `Seek` that breaks on an infinite HTTP
  /// (internet-radio) stream — the `Seek` bound is only there to satisfy
  /// rodio's type signature. A live stream has no filename, so format
  /// detection is primed from `mime_type` (e.g. `"audio/mpeg"` from the ICY
  /// response's Content-Type) when available.
  ///
  /// **Blocking:** the probe reads from the network reader; call it off the
  /// async runtime (e.g. `spawn_blocking`) like `play_file`.
  #[cfg(feature = "internet-radio")]
  pub fn play_stream<R>(&self, reader: R, mime_type: Option<&str>) -> Result<()>
  where
    R: std::io::Read + std::io::Seek + Send + Sync + 'static,
  {
    let mut builder = Decoder::builder().with_data(reader).with_seekable(false);
    if let Some(mime) = mime_type {
      builder = builder.with_mime_type(mime);
    }
    let decoder = builder
      .build()
      .map_err(|e| anyhow::anyhow!("decoding audio stream: {e}"))?;

    self.sink.clear();
    self.sink.append(decoder);
    self.sink.play();
    Ok(())
  }

  /// Pause playback, keeping the current position.
  pub fn pause(&self) {
    self.sink.pause();
  }

  /// Resume playback from the current position.
  pub fn resume(&self) {
    self.sink.play();
  }

  /// Whether playback is currently paused.
  pub fn is_paused(&self) -> bool {
    self.sink.is_paused()
  }

  /// Stop playback and discard the current source.
  ///
  /// After this, [`is_finished`](Self::is_finished) reports `true`.
  pub fn stop(&self) {
    self.sink.clear();
  }

  /// Set the output volume. `volume` is a linear gain clamped to `0.0..=1.0`
  /// (1.0 = original file level).
  pub fn set_volume(&self, volume: f32) {
    self.sink.set_volume(volume.clamp(0.0, 1.0));
  }

  /// The playback position of the current source.
  pub fn position(&self) -> Duration {
    self.sink.get_pos()
  }

  /// Whether the sink has no source playing — either nothing was ever played,
  /// or the current track played to completion (used to advance to the next
  /// track).
  ///
  /// Radio never polls this (an infinite stream has no end-of-track), so it is
  /// dead code in a build with just `internet-radio`.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic")),
    allow(dead_code)
  )]
  pub fn is_finished(&self) -> bool {
    self.sink.empty()
  }

  /// Seek to an absolute position within the current source.
  ///
  /// Radio consumes `Seek` as a no-op (nothing to seek within a live stream),
  /// so this is dead code in a build with just `internet-radio`.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic")),
    allow(dead_code)
  )]
  pub fn seek(&self, pos: Duration) -> Result<()> {
    self
      .sink
      .try_seek(pos)
      .map_err(|e| anyhow::anyhow!("seeking local audio: {e}"))
  }
}

// ---------------------------------------------------------------------------
// Platform-specific output construction
// ---------------------------------------------------------------------------

/// Open the default output device on a dedicated thread and return a control
/// `Sink` plus a keepalive sender (dropping it releases the device).
#[cfg(not(target_os = "macos"))]
fn open_sink() -> Result<(Sink, mpsc::Sender<()>)> {
  use rodio::OutputStreamBuilder;

  let (init_tx, init_rx) = mpsc::channel::<std::result::Result<Sink, String>>();
  let (keepalive_tx, keepalive_rx) = mpsc::channel::<()>();

  std::thread::Builder::new()
    .name("spotatui-local-audio".to_string())
    .spawn(move || {
      match OutputStreamBuilder::open_default_stream() {
        Ok(mut stream) => {
          // rodio `eprintln!`s a "Dropping OutputStream" warning on drop by
          // default (it has no `tracing` feature enabled here). Raw stderr
          // output corrupts the TUI, and the drop is deliberate anyway (device
          // handoff between sources tears the player down) — silence it.
          stream.log_on_drop(false);
          let sink = Sink::connect_new(stream.mixer());
          if init_tx.send(Ok(sink)).is_err() {
            return; // player was dropped before init completed
          }
          // Keep `stream` (and the device) alive until the player drops its
          // keepalive sender, at which point `recv` returns `Err` and we fall
          // through, dropping `stream` and releasing the device.
          let _ = keepalive_rx.recv();
        }
        Err(e) => {
          let _ = init_tx.send(Err(e.to_string()));
        }
      }
    })
    .context("spawning local audio output thread")?;

  let sink = init_rx
    .recv()
    .context("local audio output thread exited before initialising")?
    .map_err(|e| anyhow::anyhow!("opening default audio output device: {e}"))?;

  Ok((sink, keepalive_tx))
}

/// macOS output is not yet supported — see module docs.
#[cfg(target_os = "macos")]
fn open_sink() -> Result<(Sink, mpsc::Sender<()>)> {
  anyhow::bail!(
    "Local audio playback is not yet supported on macOS: rodio output crashes on \
     CoreAudio/Bluetooth devices (see issues #9/#20). A macOS output path is a tracked follow-up."
  )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;

  /// Write a minimal valid WAV file (44-byte header + silence) that symphonia
  /// can decode. Mirrors the helper in the parent module's tests.
  fn write_wav(path: &Path, sample_rate: u32, num_samples: u32) {
    let data_size = num_samples * 2; // 16-bit mono
    let file_size = 36 + data_size;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&file_size.to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&sample_rate.to_le_bytes()).unwrap();
    f.write_all(&(sample_rate * 2).to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_size.to_le_bytes()).unwrap();
    f.write_all(&vec![0u8; data_size as usize]).unwrap();
  }

  /// End-to-end smoke test: open the default device, play a generated WAV, and
  /// confirm the transport responds.
  ///
  /// `#[ignore]` because it requires a real audio output device, which is
  /// absent in CI / headless sandboxes. Run locally with:
  /// `cargo test --features local-files -- --ignored plays_wav`
  #[test]
  #[ignore = "requires an audio output device"]
  #[cfg(not(target_os = "macos"))]
  fn plays_wav_through_sink() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("sample.wav");
    write_wav(&wav, 44_100, 44_100); // ~1s of silence

    let player = LocalPlayer::new().expect("open default output device");
    player.play_file(&wav).expect("play wav");

    assert!(
      !player.is_paused(),
      "playback should be running after play_file"
    );
    assert!(
      !player.is_finished(),
      "a freshly started ~1s track should not be finished immediately"
    );

    player.pause();
    assert!(player.is_paused(), "pause should take effect");

    player.stop();
    assert!(
      player.is_finished(),
      "stop should clear the source so the sink reports finished"
    );
  }

  #[test]
  #[ignore = "requires an audio output device"]
  #[cfg(not(target_os = "macos"))]
  fn position_advances_while_playing() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("sample.wav");
    write_wav(&wav, 44_100, 44_100 * 3); // ~3s

    let player = LocalPlayer::new().expect("open default output device");
    let start = player.position();
    eprintln!("position at start: {start:?}");
    player.play_file(&wav).expect("play wav");

    std::thread::sleep(Duration::from_millis(600));
    let after = player.position();
    eprintln!("position after ~600ms: {after:?}");

    assert!(
      after >= Duration::from_millis(300),
      "position should advance to roughly playback time, got {after:?}"
    );
    assert!(
      after < Duration::from_secs(3),
      "position should not exceed the track duration, got {after:?}"
    );
  }
}
