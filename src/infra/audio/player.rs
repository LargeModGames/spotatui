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
//! `rodio` drives the default output device on all three platforms, CoreAudio
//! included. macOS was gated off here until the rodio 0.22 / cpal 0.17 output
//! path was measured on a Mac (the two `#[ignore]`d tests below): the crashes
//! behind that gate (issues #9/#20) were librespot's own `rodio-backend`, an
//! older rodio on the pre-`DeviceSinkBuilder` API. librespot still uses
//! `portaudio-backend` on macOS for that reason; this player does not share
//! that code path and does not need it.
//!
//! ## Losing the output device
//!
//! Losing the device mid-track takes two shapes, and only one of them is an
//! error anybody reports. cpal notices the device being *removed* and reports
//! `DeviceNotAvailable`. It cannot notice the far more common case: the OS
//! moving its **default output** somewhere else — headphones unplugged, AirPods
//! back in their case — which leaves the stream happily bound to a device
//! nobody is listening to. So the sink also remembers which device it opened
//! and [`device_lost`](LocalPlayer::device_lost) compares that against the
//! current default.
//!
//! Either way no audio callback runs again, and that matters far beyond
//! silence: rodio's
//! `clear` and `try_seek` *wait on that callback* (`sleep_until_end`, and the
//! seek feedback channel, neither with a timeout), and the transport methods
//! here are called straight from the serial IoEvent pump. A blocked one wedges
//! the pump, and with it every unrelated thing behind it — searches included.
//!
//! So every method that would wait on the audio thread refuses once the device
//! is gone, and — because a detector that misses one day would freeze the app
//! again — the wait itself is bounded: an audio thread that does not answer
//! *is* the last-resort detector. The driver's tick notices, calls
//! [`LocalPlayer::reopen`] to rebuild the output on the new default device, and
//! restages the track there.
//!
//! ## Threading
//!
//! `rodio::MixerDeviceSink` is `!Send`, so it cannot live on the shared player
//! struct (which is held behind an `Arc` across async tasks). Instead a
//! dedicated thread owns the `MixerDeviceSink` and keeps it alive; the player
//! holds only the `Send + Sync` [`rodio::Player`] plus a keepalive channel whose
//! drop tells the thread to release the device.

use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rodio::{Decoder, Player};

/// An audio player for local files, driving the system default output device.
///
/// Cheap to hold behind an `Arc`: the heavy `MixerDeviceSink` lives on a
/// dedicated thread, and the `Player` here is a lightweight `Arc`-backed handle.
pub struct LocalPlayer {
  /// Replaced wholesale by [`reopen`](LocalPlayer::reopen). Only ever locked to
  /// clone the `Arc` out, never held across a rodio call: `clear` waits on the
  /// audio thread while the render tick reads `position()` from another one.
  sink: Mutex<Sink>,
  /// The last percent handed to [`set_volume`](LocalPlayer::set_volume),
  /// re-applied to a reopened sink so a device change is not also a volume jump.
  volume_percent: AtomicU8,
  /// The cached answer of the default-output compare in
  /// [`device_lost`](LocalPlayer::device_lost).
  device_check: Mutex<Option<DeviceCheck>>,
  /// Reopen attempts since the device was lost.
  reopen: Mutex<ReopenState>,
}

/// One default-output read, keyed on the name the sink opened so a swapped
/// sink is never answered from the old sink's cache.
struct DeviceCheck {
  checked_at: Instant,
  opened: String,
  moved: bool,
}

#[derive(Default)]
struct ReopenState {
  attempts: u8,
  last_attempt: Option<Instant>,
}

/// What [`LocalPlayer::recover_device`] did this call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reopen {
  /// A fresh sink is in place, paused and empty: restage onto it.
  Reopened,
  /// An attempt just failed; another follows after [`DEVICE_REOPEN_RETRY_AFTER`].
  Retrying,
  /// Nothing happened: the next attempt is not due yet, or the device was
  /// already given up on.
  Waiting,
  /// The last attempt failed. Reported once; the caller ends the session.
  GaveUp,
}

/// One open output: rodio's control handle, the audio thread's keepalive, and
/// the flag cpal's error callback raises when the device disappears.
struct Sink {
  player: Arc<Player>,
  /// Dropping this sender signals the audio thread to drop its `OutputStream`
  /// and release the audio device. Held for the sink's lifetime.
  _keepalive: mpsc::Sender<()>,
  /// Raised by our cpal error callback when the device is *removed*.
  lost: Arc<AtomicBool>,
  /// The device this sink was opened on, as cpal names it, so a default-output
  /// change can be spotted (see module docs). `None` when the name could not be
  /// read, which only disables the comparison — never fakes a change.
  device_name: Option<String>,
}

/// How often a wait on the audio thread stops to re-ask whether the device is
/// still there. Well above a healthy `clear` (one buffer, tens of milliseconds),
/// short enough that a real disconnect is noticed while the user still connects
/// it to what they just did.
const AUDIO_THREAD_POLL: Duration = Duration::from_secs(3);

/// The absolute ceiling on such a wait. Above Qobuz's 60s stream stall so a
/// slow network is never mistaken for a dead device, and finite so an audio
/// thread that wedges for a reason nothing here can name still cannot take the
/// app with it.
const AUDIO_THREAD_CEILING: Duration = Duration::from_secs(90);

/// How long to wait for the audio thread to hand back an opened device.
/// `reopen` runs from the driver's tick, on the UI thread under the `App` lock,
/// so this wait cannot be unbounded either. A healthy open is tens of
/// milliseconds; this is a backstop, not a deadline anybody should reach.
const DEVICE_OPEN_TIMEOUT: Duration = Duration::from_secs(5);

/// How often [`LocalPlayer::device_lost`] re-reads the default output device.
/// The tick asks on every frame, and the read is a full device enumeration.
const DEVICE_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// The spacing between reopen attempts, and how many are made before the
/// device is given up on. A Bluetooth output takes a few seconds to negotiate
/// after it comes back, so one failed open is not the answer.
const DEVICE_REOPEN_RETRY_AFTER: Duration = Duration::from_secs(2);
const DEVICE_REOPEN_ATTEMPTS: u8 = 5;

/// Whether another reopen attempt is due: not within
/// [`DEVICE_REOPEN_RETRY_AFTER`] of the last one, and never past
/// [`DEVICE_REOPEN_ATTEMPTS`].
fn reopen_due(attempts: u8, since_last: Option<Duration>) -> bool {
  attempts < DEVICE_REOPEN_ATTEMPTS && since_last.is_none_or(|d| d >= DEVICE_REOPEN_RETRY_AFTER)
}

/// A decoded stream, ready for [`LocalPlayer::play_prepared`].
#[cfg(any(feature = "internet-radio", feature = "qobuz"))]
pub struct PreparedStream(Box<dyn rodio::Source + Send>);

impl LocalPlayer {
  /// Open the default audio output device and return a ready player.
  ///
  /// **Blocking:** waits for the audio thread to open the device. Call once at
  /// setup, off any latency-sensitive path. Returns an error if no output
  /// device is available (e.g. headless CI).
  pub fn new() -> Result<Self> {
    let sink = open_sink()?;
    // Start silent; nothing is queued until the first `play_file`.
    sink.player.pause();
    Ok(Self {
      sink: Mutex::new(sink),
      volume_percent: AtomicU8::new(100),
      device_check: Mutex::new(None),
      reopen: Mutex::new(ReopenState::default()),
    })
  }

  /// The rodio handle, for the calls that cannot block (they only touch
  /// atomics and short-lived mutexes, never the audio thread).
  fn player(&self) -> Arc<Player> {
    Arc::clone(&self.sink.lock().unwrap().player)
  }

  /// The rodio handle for calls that *do* wait on the audio thread, refusing
  /// once the device is gone — see the module docs: waiting there never
  /// returns, and would wedge the serial IoEvent pump rather than fall silent.
  fn live_player(&self) -> Result<Arc<Player>> {
    let sink = self.sink.lock().unwrap();
    if sink.lost.load(Ordering::Relaxed) {
      anyhow::bail!("audio output device disconnected");
    }
    Ok(Arc::clone(&sink.player))
  }

  /// Whether cpal reported the device *removed* (headphones unplugged, a USB
  /// DAC pulled), as against the OS merely moving its default output somewhere
  /// else. Recovery pauses only for removal — that is what macOS itself does,
  /// and a device the user just *plugged in* should keep playing.
  #[cfg_attr(
    not(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    )),
    allow(dead_code)
  )]
  pub fn device_removed(&self) -> bool {
    self.sink.lock().unwrap().lost.load(Ordering::Relaxed)
  }

  /// Whether the sink is still connected to something the user can hear. The
  /// driver's tick polls this to decide when to [`reopen`](Self::reopen).
  ///
  /// Two ways to fail (see module docs): the device was removed and cpal told
  /// us, or the OS quietly moved its default output elsewhere and nobody did.
  pub fn device_lost(&self) -> bool {
    // Read the sink's answer and release its lock: the default-output read
    // below enumerates devices, and the render path waits on this mutex.
    let (removed, opened) = {
      let sink = self.sink.lock().unwrap();
      (sink.lost.load(Ordering::Relaxed), sink.device_name.clone())
    };
    if removed {
      return true;
    }
    // Only a name read successfully on *both* sides is evidence: a missing one
    // means "cannot tell", which must not read as "changed" — mid-switch there
    // is briefly no default device at all, and tearing playback down for that
    // would be worse than the silence this exists to catch.
    let Some(opened) = opened else {
      return false;
    };
    let mut check = self.device_check.lock().unwrap();
    if let Some(cached) = check.as_ref() {
      if cached.opened == opened && cached.checked_at.elapsed() < DEVICE_CHECK_INTERVAL {
        return cached.moved;
      }
    }
    let moved = default_output_name().is_some_and(|current| current != opened);
    *check = Some(DeviceCheck {
      checked_at: Instant::now(),
      opened,
      moved,
    });
    moved
  }

  /// Wait on the audio thread, but never forever.
  ///
  /// rodio's `clear` and `try_seek` block until the audio callback answers, and
  /// they run on the serial IoEvent pump: one that never returns takes every
  /// unrelated request behind it down too. `device_lost` normally catches a
  /// dead device before we get here — this is the backstop for a way of losing
  /// one that neither cpal nor the default-output check saw.
  ///
  /// A plain timeout would be wrong: a dead device and a source stalled on the
  /// network look identical from here (Qobuz gives its stream 60 seconds), and
  /// cutting the second one short would break slow playback to fix a freeze. So
  /// the wait re-*asks the device* every [`AUDIO_THREAD_POLL`] instead of
  /// guessing, gives up at once when it really went away, and only past
  /// [`AUDIO_THREAD_CEILING`] — nothing identifiably wrong, still no answer —
  /// declares it lost anyway, because a pump that never returns is worse than
  /// a track that never plays.
  ///
  /// A call that truly never returns leaves its worker parked for the life of
  /// the process. That is the cheap half of this trade.
  fn bounded<T: Send + 'static>(&self, op: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    // The sink this wait belongs to. `reopen` swaps in a whole new `Sink`, so
    // every check below must be about *this* one: a poll that read the fresh
    // sink would find it healthy, wait out the full ceiling, and then mark the
    // live sink lost.
    let (waited_on, lost) = {
      let sink = self.sink.lock().unwrap();
      (Arc::clone(&sink.player), Arc::clone(&sink.lost))
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
      let _ = tx.send(op());
    });

    let started = std::time::Instant::now();
    loop {
      match rx.recv_timeout(AUDIO_THREAD_POLL) {
        Ok(value) => return Some(value),
        // The worker vanished without answering; nothing left to wait for.
        Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        Err(mpsc::RecvTimeoutError::Timeout) => {
          // Reopened under us: the caller's result would land on a sink nobody
          // plays through any more, and the driver is already restaging.
          if !Arc::ptr_eq(&waited_on, &self.player()) {
            return None;
          }
          if self.device_lost() {
            return None;
          }
          if started.elapsed() >= AUDIO_THREAD_CEILING {
            lost.store(true, Ordering::Relaxed);
            return None;
          }
        }
      }
    }
  }

  /// Rebuild the output on the *current* default device and drop the dead one.
  ///
  /// The new sink starts paused and empty, at the volume the old one had, so
  /// the caller restages whatever was playing (see the driver's tick). Dropping
  /// the old `Sink` releases its keepalive, which lets the old audio thread
  /// exit; `Player`'s own `Drop` only sets a flag, so nothing here blocks.
  ///
  /// **Blocking:** opens a device, like [`new`](Self::new).
  pub fn reopen(&self) -> Result<()> {
    let fresh = open_sink()?;
    // Configure before publishing: once swapped in, other threads can see it.
    fresh.player.pause();
    fresh
      .player
      .set_volume(volume_gain(self.volume_percent.load(Ordering::Relaxed)));
    *self.sink.lock().unwrap() = fresh;
    *self.device_check.lock().unwrap() = None;
    Ok(())
  }

  /// [`reopen`](Self::reopen) with retries, for the driver's tick: an attempt
  /// every [`DEVICE_REOPEN_RETRY_AFTER`] up to [`DEVICE_REOPEN_ATTEMPTS`], so a
  /// device that is still negotiating gets its chance, and one that never
  /// comes back is given up on exactly once.
  ///
  /// **Blocking:** an attempt opens a device, like [`new`](Self::new).
  ///
  /// Radio has no track to restage, so a build with just `internet-radio`
  /// never recovers a device.
  #[cfg_attr(
    not(any(
      feature = "local-files",
      feature = "subsonic",
      feature = "qobuz",
      feature = "youtube"
    )),
    allow(dead_code)
  )]
  pub fn recover_device(&self) -> Reopen {
    let attempts = {
      let mut state = self.reopen.lock().unwrap();
      if !reopen_due(state.attempts, state.last_attempt.map(|t| t.elapsed())) {
        return Reopen::Waiting;
      }
      state.attempts += 1;
      state.last_attempt = Some(Instant::now());
      state.attempts
    };
    match self.reopen() {
      Ok(()) => {
        *self.reopen.lock().unwrap() = ReopenState::default();
        Reopen::Reopened
      }
      Err(e) => {
        log::warn!("[audio] reopen attempt {attempts}: {e:#}");
        if attempts >= DEVICE_REOPEN_ATTEMPTS {
          Reopen::GaveUp
        } else {
          Reopen::Retrying
        }
      }
    }
  }

  /// Decode the file at `path` and play it, replacing whatever was playing.
  ///
  /// The format is detected from the file's content (FLAC/MP3/MP4-AAC/Vorbis/
  /// WAV are supported by default). Returns an error if the file cannot be
  /// opened or its format is unsupported.
  ///
  /// Only the tempfile-based sources play files; a build with just
  /// `internet-radio` uses [`prepare_stream`](Self::prepare_stream) plus
  /// [`play_prepared`](Self::play_prepared) instead, and Qobuz stages.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic", feature = "youtube")),
    allow(dead_code)
  )]
  pub fn play_file(&self, path: &Path) -> Result<()> {
    self.stage_file(path)?;
    self.resume();
    Ok(())
  }

  /// [`play_file`](Self::play_file) without the final play: the track is
  /// queued and the sink left paused, so a caller that must seek first, or
  /// stay paused, never plays a burst of the track's start.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic", feature = "qobuz")),
    allow(dead_code)
  )]
  pub fn stage_file(&self, path: &Path) -> Result<()> {
    let sink = self.live_player()?;
    // Stop whatever is currently playing *before* any fallible step (open or
    // decode), so a failure here can never leave the previous track audible. A
    // manual Next/Previous into a missing or undecodable file must fall silent:
    // `play_index`'s failure arm relies on the sink draining here so the runner
    // tick's `is_finished()` fires and auto-advance skips past the bad file
    // instead of dead-ending on a stale, still-playing track.
    let clearing = Arc::clone(&sink);
    if self.bounded(move || clearing.clear()).is_none() {
      anyhow::bail!("audio output device stopped responding");
    }

    let file = std::fs::File::open(path)
      .with_context(|| format!("opening audio file {}", path.display()))?;

    // On decode error we return with the sink already empty (no old track
    // playing); on success we append and start the new source below.
    let decoder = Decoder::new(BufReader::new(file))
      .with_context(|| format!("decoding audio file {}", path.display()))?;

    // The decode is long enough for the driver's tick to have reopened the
    // device under us; appending to the old sink would play into nothing.
    if !Arc::ptr_eq(&sink, &self.player()) {
      anyhow::bail!("audio output device changed while decoding");
    }

    // The clear above paused the sink; `append` leaves it so.
    sink.append(decoder);
    Ok(())
  }

  /// Build the decoder for a stream without a sink change, so the caller can
  /// decide under its own lock if the stream is still wanted.
  ///
  /// With `byte_len` the reader is seekable and the decoder knows the total
  /// size (a progressive download of a known file). Without it the reader is
  /// treated as non-seekable: the decoder is built with `with_seekable(false)`
  /// so the symphonia probe never issues the `Seek` that breaks on an infinite
  /// HTTP (internet-radio) stream. The `Seek` bound is only there to satisfy
  /// rodio's type signature. A stream has no filename, so format detection is
  /// primed from `mime_type` (e.g. `"audio/mpeg"`) when available.
  ///
  /// **Blocking:** the probe reads from the network reader; call it off the
  /// async runtime (e.g. `spawn_blocking`) like `play_file`.
  #[cfg(any(feature = "internet-radio", feature = "qobuz"))]
  pub fn prepare_stream<R>(
    reader: R,
    mime_type: Option<&str>,
    byte_len: Option<u64>,
  ) -> Result<PreparedStream>
  where
    R: std::io::Read + std::io::Seek + Send + Sync + 'static,
  {
    let mut builder = Decoder::builder()
      .with_data(reader)
      .with_seekable(byte_len.is_some());
    if let Some(len) = byte_len {
      builder = builder.with_byte_len(len);
    }
    if let Some(mime) = mime_type {
      builder = builder.with_mime_type(mime);
    }
    let decoder = builder
      .build()
      .map_err(|e| anyhow::anyhow!("decoding audio stream: {e}"))?;
    Ok(PreparedStream(Box::new(decoder)))
  }

  /// Play a prepared stream, replacing whatever was playing. The clear waits
  /// for the audio thread to drop the previous source: call it off the `App`
  /// lock (see `stop_detached`). Fails once the device is gone.
  #[cfg(any(feature = "internet-radio", feature = "qobuz"))]
  #[cfg_attr(not(feature = "internet-radio"), allow(dead_code))]
  pub fn play_prepared(&self, stream: PreparedStream) -> Result<()> {
    self.stage_prepared(stream)?;
    self.resume();
    Ok(())
  }

  /// [`play_prepared`](Self::play_prepared) without the final play, like
  /// [`stage_file`](Self::stage_file).
  #[cfg(any(feature = "internet-radio", feature = "qobuz"))]
  pub fn stage_prepared(&self, stream: PreparedStream) -> Result<()> {
    let sink = self.live_player()?;
    let clearing = Arc::clone(&sink);
    if self.bounded(move || clearing.clear()).is_none() {
      anyhow::bail!("audio output device stopped responding");
    }
    // Reopened under the clear: the stream would play into a discarded sink,
    // and nothing would notice, since the fresh one reports no lost device.
    if !Arc::ptr_eq(&sink, &self.player()) {
      anyhow::bail!("audio output device changed");
    }
    sink.append(stream.0);
    Ok(())
  }

  /// Pause playback, keeping the current position.
  pub fn pause(&self) {
    self.player().pause();
  }

  /// Resume playback from the current position.
  pub fn resume(&self) {
    self.player().play();
  }

  /// Whether playback is currently paused.
  pub fn is_paused(&self) -> bool {
    self.player().is_paused()
  }

  /// Stop playback and discard the current source.
  ///
  /// After this, [`is_finished`](Self::is_finished) reports `true`. A no-op
  /// once the device is gone — the sink it would drain is already dead.
  pub fn stop(&self) {
    let Ok(sink) = self.live_player() else { return };
    self.bounded(move || sink.clear());
  }

  /// Stop on the blocking pool and return at once. `stop` waits for the audio
  /// thread, which a stalled network source holds until its stall timeout.
  pub fn stop_detached(self: std::sync::Arc<Self>) {
    tokio::task::spawn_blocking(move || self.stop());
  }

  /// [`stop_detached`](Self::stop_detached) with `session` kept alive until
  /// the stop returns, so a tempfile the sink still reads is not deleted
  /// under it.
  pub fn stop_detached_holding<S: Send + 'static>(self: std::sync::Arc<Self>, session: S) {
    tokio::task::spawn_blocking(move || {
      self.stop();
      drop(session);
    });
  }

  /// Set the output volume from the user's percent, on the same logarithmic
  /// curve as native streaming, so one setting is equally loud on every source.
  pub fn set_volume(&self, percent: u8) {
    self.volume_percent.store(percent, Ordering::Relaxed);
    self.player().set_volume(volume_gain(percent));
  }

  /// The playback position of the current source.
  pub fn position(&self) -> Duration {
    self.player().get_pos()
  }

  /// Whether the sink has no source playing — either nothing was ever played,
  /// or the current track played to completion (used to advance to the next
  /// track).
  ///
  /// Radio never polls this (an infinite stream has no end-of-track), so it is
  /// dead code in a build with just `internet-radio`.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic", feature = "qobuz")),
    allow(dead_code)
  )]
  pub fn is_finished(&self) -> bool {
    self.player().empty()
  }

  /// Seek to an absolute position within the current source.
  ///
  /// Radio consumes `Seek` as a no-op (nothing to seek within a live stream),
  /// so this is dead code in a build with just `internet-radio`.
  #[cfg_attr(
    not(any(feature = "local-files", feature = "subsonic", feature = "qobuz")),
    allow(dead_code)
  )]
  pub fn seek(&self, pos: Duration) -> Result<()> {
    let sink = self.live_player()?;
    match self.bounded(move || sink.try_seek(pos)) {
      Some(result) => result.map_err(|e| anyhow::anyhow!("seeking local audio: {e}")),
      None => anyhow::bail!("audio output device stopped responding"),
    }
  }
}

// ---------------------------------------------------------------------------
// Platform-specific output construction
// ---------------------------------------------------------------------------

/// Open the default output device on a dedicated thread and return the live
/// [`Sink`] (dropping its keepalive releases the device).
fn open_sink() -> Result<Sink> {
  use rodio::cpal::traits::HostTrait;
  use rodio::cpal::StreamError;
  use rodio::DeviceSinkBuilder;

  // Pick the device here rather than letting rodio do it, so the sink can
  // remember which one it got and notice the OS moving on from it later.
  let device = rodio::cpal::default_host()
    .default_output_device()
    .context("no audio output device available")?;
  let device_name = device_name(&device);

  let lost = Arc::new(AtomicBool::new(false));
  let on_error = {
    let lost = Arc::clone(&lost);
    move |err: StreamError| {
      // Ours only because rodio's default callback `eprintln!`s (its `tracing`
      // feature is off here) and raw stderr corrupts the TUI. The one error
      // worth recording is the device going away: cpal pauses the stream and
      // reports `DeviceNotAvailable`, after which no audio callback runs again
      // and every rodio call that waits on one would hang (see module docs).
      if matches!(err, StreamError::DeviceNotAvailable) {
        lost.store(true, Ordering::Relaxed);
      }
    }
  };

  let (init_tx, init_rx) = mpsc::channel::<std::result::Result<Player, String>>();
  let (keepalive_tx, keepalive_rx) = mpsc::channel::<()>();

  std::thread::Builder::new()
    .name("spotatui-local-audio".to_string())
    .spawn(move || {
      // `open_default_sink()` would install rodio's `eprintln!` callback and
      // pick the device itself, so build the same thing by hand: the default
      // device, falling back to its other supported configs. Unlike rodio's
      // helper this does not then sweep every *other* output device — a machine
      // whose default output cannot be opened gets a clear error instead of
      // audio from a surprise device.
      let opened = DeviceSinkBuilder::from_device(device)
        .map(|builder| builder.with_error_callback(on_error))
        .and_then(|builder| builder.open_sink_or_fallback());
      match opened {
        Ok(mut stream) => {
          // rodio prints a drop warning for the `MixerDeviceSink` to stderr by
          // default (it has no `tracing` feature enabled here). Raw stderr
          // output corrupts the TUI, and the drop is deliberate anyway (device
          // handoff between sources tears the player down) — silence it.
          stream.log_on_drop(false);
          let sink = Player::connect_new(stream.mixer());
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

  let player = init_rx
    .recv_timeout(DEVICE_OPEN_TIMEOUT)
    .context("local audio output thread did not open a device")?
    .map_err(|e| anyhow::anyhow!("opening default audio output device: {e}"))?;

  Ok(Sink {
    player: Arc::new(player),
    _keepalive: keepalive_tx,
    lost,
    device_name,
  })
}

/// A device's name as cpal reports it. `None` when it cannot be read — the
/// caller must treat that as "cannot tell", never as a change.
fn device_name(device: &rodio::Device) -> Option<String> {
  use rodio::DeviceTrait;
  device.description().ok().map(|d| d.name().to_string())
}

/// The name of the output device the OS currently calls its default, which is
/// what a sink opened earlier is compared against.
fn default_output_name() -> Option<String> {
  use rodio::cpal::traits::HostTrait;
  rodio::cpal::default_host()
    .default_output_device()
    .as_ref()
    .and_then(device_name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The gain for a volume percent: librespot's `VolumeCtrl::Log` mapping over
/// a 60 dB range (`1000^(p/100 - 1)`), which the native player uses by default.
/// 0% is silence, 100% is the file level, and 80% is about -12 dB.
pub fn volume_gain(percent: u8) -> f32 {
  const DB_RATIO: f64 = 1000.0;
  match percent {
    0 => 0.0,
    p if p >= 100 => 1.0,
    p => ((f64::from(p) / 100.0 - 1.0) * DB_RATIO.ln()).exp() as f32,
  }
}

#[cfg(test)]
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

  #[test]
  fn the_first_reopen_is_due_at_once() {
    assert!(reopen_due(0, None));
  }

  #[test]
  fn reopens_are_spaced_out() {
    assert!(!reopen_due(1, Some(DEVICE_REOPEN_RETRY_AFTER / 2)));
    assert!(reopen_due(1, Some(DEVICE_REOPEN_RETRY_AFTER)));
  }

  #[test]
  fn reopens_stop_after_the_attempt_budget() {
    let long_ago = Some(Duration::from_secs(60));
    assert!(reopen_due(DEVICE_REOPEN_ATTEMPTS - 1, long_ago));
    assert!(!reopen_due(DEVICE_REOPEN_ATTEMPTS, long_ago));
  }
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

  /// The anti-hang guarantee. Once the device is gone, the calls that wait on
  /// the audio thread must refuse rather than block: they run on the serial
  /// IoEvent pump, so one that never returns takes the whole app down with it,
  /// searches included. A regression does not fail loudly — it deadlocks — so
  /// the assertion is a timeout around a worker thread.
  #[test]
  #[ignore = "requires an audio output device"]
  fn a_lost_device_refuses_the_calls_that_wait_on_the_audio_thread() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("sample.wav");
    write_wav(&wav, 44_100, 44_100 * 30); // still playing when the device drops

    let player = Arc::new(LocalPlayer::new().expect("open default output device"));
    player.play_file(&wav).expect("play wav");

    // Raise exactly the flag cpal's error callback raises on a disconnect.
    player
      .sink
      .lock()
      .unwrap()
      .lost
      .store(true, Ordering::Relaxed);
    assert!(player.device_lost());

    let (tx, rx) = mpsc::channel();
    let worker = Arc::clone(&player);
    let path = wav.clone();
    std::thread::spawn(move || {
      let refused = (
        worker.play_file(&path).is_err(),
        worker.seek(Duration::from_secs(1)).is_err(),
      );
      worker.stop(); // must return instead of draining a sink nothing feeds
      let _ = tx.send(refused);
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
      Ok((play_refused, seek_refused)) => {
        assert!(play_refused, "play_file should refuse a lost device");
        assert!(seek_refused, "seek should refuse a lost device");
      }
      Err(_) => panic!("a transport call blocked on the dead audio thread"),
    }
  }

  /// The detector that matters most in practice. cpal reports nothing when the
  /// OS moves its default output elsewhere — the stream stays bound to a device
  /// nobody can hear — so comparing against the current default is what catches
  /// headphones unplugged or AirPods going back in their case.
  #[test]
  #[ignore = "requires an audio output device"]
  fn a_default_output_change_reads_as_a_lost_device() {
    let player = LocalPlayer::new().expect("open default output device");
    assert!(
      !player.device_lost(),
      "a sink freshly opened on the current default is not lost"
    );

    // Stand in for the OS switching away: the sink names a device that is no
    // longer the default.
    player.sink.lock().unwrap().device_name = Some("a device that went away".to_string());
    assert!(
      player.device_lost(),
      "a default that moved is a lost device"
    );

    // A name we could not read means "cannot tell", never "changed": mid-switch
    // there is briefly no default at all, and tearing playback down for that
    // would be worse than the silence this exists to catch.
    player.sink.lock().unwrap().device_name = None;
    assert!(!player.device_lost(), "an unreadable name is not evidence");

    player.reopen().expect("reopen on the default device");
    assert!(
      !player.device_lost(),
      "reopen re-anchors the sink to the new default"
    );
  }

  /// The backstop, on the path that matters: the audio thread stops answering
  /// *because* the device went away. The wait must notice at its first check
  /// rather than sit out a stall timeout meant for slow networks — this is the
  /// difference between a stutter and the frozen app the user reported.
  #[test]
  #[ignore = "requires an audio output device"]
  fn a_wait_gives_up_as_soon_as_the_device_is_gone() {
    let player = LocalPlayer::new().expect("open default output device");
    // The device moved out from under the sink, as it does when headphones go.
    player.sink.lock().unwrap().device_name = Some("a device that went away".to_string());

    let started = std::time::Instant::now();
    let answered = player.bounded(|| std::thread::sleep(Duration::from_secs(120)));

    assert!(answered.is_none(), "the wait must give up, not block");
    assert!(
      started.elapsed() < AUDIO_THREAD_POLL + Duration::from_secs(2),
      "it must give up at the first check, got {:?}",
      started.elapsed()
    );
  }

  /// The reopen window. `reopen` swaps in a whole `Sink`, and a wait already
  /// running belongs to the old one: it must give up at once rather than sit
  /// out the ceiling and then mark the *live* sink lost — the driver is
  /// already restaging onto that sink.
  #[test]
  #[ignore = "requires an audio output device"]
  fn a_wait_gives_up_when_the_sink_is_reopened_under_it() {
    let player = Arc::new(LocalPlayer::new().expect("open default output device"));

    let waiting = Arc::clone(&player);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
      let gave_up = waiting
        .bounded(|| std::thread::sleep(Duration::from_secs(120)))
        .is_none();
      let _ = tx.send(gave_up);
    });

    // Reopen while that wait is parked, before its first poll.
    std::thread::sleep(AUDIO_THREAD_POLL / 2);
    player.reopen().expect("reopen on the default device");

    match rx.recv_timeout(AUDIO_THREAD_POLL + Duration::from_secs(2)) {
      Ok(gave_up) => assert!(gave_up, "a wait on a replaced sink must give up"),
      Err(_) => panic!("the wait outlived the sink it belonged to"),
    }
    assert!(
      !player.device_lost(),
      "the ceiling must never be charged to the fresh sink"
    );
  }

  /// Recovery: a reopened sink is alive, empty, paused and at the volume the
  /// dead one had, ready for the driver to restage the track onto.
  #[test]
  #[ignore = "requires an audio output device"]
  fn reopening_gives_a_live_empty_sink_at_the_same_volume() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("sample.wav");
    write_wav(&wav, 44_100, 44_100);

    let player = LocalPlayer::new().expect("open default output device");
    player.set_volume(40);
    player.play_file(&wav).expect("play wav");
    player
      .sink
      .lock()
      .unwrap()
      .lost
      .store(true, Ordering::Relaxed);

    player.reopen().expect("reopen on the default device");

    assert!(!player.device_lost(), "the fresh sink starts alive");
    assert!(player.is_finished(), "the fresh sink starts empty");
    assert!(player.is_paused(), "the fresh sink waits to be restaged");
    let gain = player.player().volume();
    assert!(
      (gain - volume_gain(40)).abs() < 0.0001,
      "a device change should not also be a volume jump: {gain}"
    );
    // And it is usable: the restage the driver dispatches must land.
    player
      .play_file(&wav)
      .expect("reopened sink accepts a track");
  }

  #[test]
  #[ignore = "requires an audio output device"]
  fn position_advances_while_playing() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("sample.wav");
    write_wav(&wav, 44_100, 44_100 * 3); // ~3s

    let player = LocalPlayer::new().expect("open default output device");
    let start = player.position();
    player.play_file(&wav).expect("play wav");

    std::thread::sleep(Duration::from_millis(600));
    let after = player.position();

    assert!(
      after >= Duration::from_millis(300),
      "position should advance to roughly playback time, got {after:?} (started at {start:?})"
    );
    assert!(
      after < Duration::from_secs(3),
      "position should not exceed the track duration, got {after:?}"
    );
  }
}
