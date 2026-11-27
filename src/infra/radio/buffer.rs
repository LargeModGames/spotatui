use std::collections::VecDeque;
use std::convert::Infallible;
use std::io::{self, Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::Result;
use futures::{ready, Stream};
use stream_download::source::SourceStream;
use stream_download::storage::bounded::BoundedStorageProvider;
use stream_download::storage::memory::MemoryStorageProvider;
use stream_download::{Settings, StreamDownload, StreamPhase};
use tokio::sync::watch;
use tokio::time::Instant;

use super::stream::{HttpRadioStream, RadioChunk};

const RING_BUFFER_BYTES: usize = 512 * 1024;
const MAX_STARTUP_BYTES: u64 = 64 * 1024;
const STARTUP_SECONDS: f64 = 3.0;
// Progress is reported after a whole chunk is written into the ring. Limit
// chunks so the reported position can lag the writer by at most 8 KiB.
const CHUNK_BYTES: usize = 8 * 1024;

type Download = StreamDownload<BoundedStorageProvider<MemoryStorageProvider>>;

/// Count audio in either direction without counting the ICY length byte or
/// metadata, even when a chunk ends in the middle of a block.
struct AudioCounter {
  interval: usize,
  audio_left: usize,
  metadata_left: usize,
  total: u64,
}

impl AudioCounter {
  fn new(interval: Option<NonZeroUsize>) -> Self {
    let interval = interval.map_or(0, NonZeroUsize::get);
    Self {
      interval,
      audio_left: interval,
      metadata_left: 0,
      total: 0,
    }
  }

  fn add(&mut self, mut bytes: &[u8]) {
    if self.interval == 0 {
      self.total += bytes.len() as u64;
      return;
    }
    while !bytes.is_empty() {
      if self.audio_left > 0 {
        let count = self.audio_left.min(bytes.len());
        self.audio_left -= count;
        self.total += count as u64;
        bytes = &bytes[count..];
      } else {
        if self.metadata_left == 0 {
          self.metadata_left = usize::from(bytes[0]) * 16;
          bytes = &bytes[1..];
        }
        let count = self.metadata_left.min(bytes.len());
        self.metadata_left -= count;
        bytes = &bytes[count..];
        if self.metadata_left == 0 {
          self.audio_left = self.interval;
        }
      }
    }
  }
}

struct RadioBody {
  http: HttpRadioStream,
  prefix: VecDeque<RadioChunk>,
  chunk: RadioChunk,
  audio: AudioCounter,
  cancelled: Arc<AtomicBool>,
}

impl Stream for RadioBody {
  type Item = <HttpRadioStream as Stream>::Item;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    // End the body once cancelled, so a download loop that picks the next
    // chunk over its cancellation branch still stops and drops the connection.
    if self.cancelled.load(Ordering::Acquire) {
      return Poll::Ready(None);
    }
    while self.chunk.is_empty() {
      self.chunk = match self.prefix.pop_front() {
        Some(bytes) => bytes,
        None => match ready!(Pin::new(&mut self.http).poll_next(cx)) {
          Some(Ok(bytes)) => bytes,
          result => return Poll::Ready(result),
        },
      };
    }
    let length = self.chunk.len().min(CHUNK_BYTES);
    let bytes = self.chunk.split_to(length);
    self.audio.add(&bytes);
    Poll::Ready(Some(Ok(bytes)))
  }
}

impl SourceStream for RadioBody {
  type Params = Infallible;
  type StreamCreationError = Infallible;

  async fn create(params: Self::Params) -> Result<Self, Self::StreamCreationError> {
    match params {}
  }

  fn content_length(&self) -> Option<u64> {
    self.http.content_length()
  }

  fn supports_seek(&self) -> bool {
    false
  }

  async fn seek_range(&mut self, _: u64, _: Option<u64>) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
  }

  async fn reconnect(&mut self, position: u64) -> io::Result<()> {
    self.http.reconnect(position).await
  }
}

#[derive(Clone, Copy, Default)]
struct Progress {
  raw: u64,
  audio: u64,
  elapsed: Duration,
  complete: bool,
}

impl Progress {
  fn has_cushion(self, consumed: &Consumed, bitrate: Option<u32>) -> bool {
    let raw = self
      .raw
      .saturating_sub(consumed.raw.load(Ordering::Relaxed));
    let audio = self
      .audio
      .saturating_sub(consumed.audio.load(Ordering::Relaxed));
    let bytes_per_second = match bitrate {
      Some(rate) => f64::from(rate) * 125.0,
      None => self.audio as f64 / self.elapsed.as_secs_f64().max(0.001),
    };
    let target = (bytes_per_second * STARTUP_SECONDS).ceil().max(1.0) as u64;
    self.complete || raw >= MAX_STARTUP_BYTES || audio >= target
  }
}

#[derive(Default)]
struct Consumed {
  raw: AtomicU64,
  audio: AtomicU64,
}

pub(super) struct RadioReader {
  download: Download,
  audio: AudioCounter,
  consumed: Arc<Consumed>,
  progress: watch::Receiver<Progress>,
  cancelled: Arc<AtomicBool>,
}

impl RadioReader {
  pub(super) fn cancel(&self) -> impl FnOnce() + Send + use<> {
    let token = self.download.cancellation_token();
    let cancelled = Arc::clone(&self.cancelled);
    move || {
      cancelled.store(true, Ordering::Release);
      token.cancel();
    }
  }
}

impl Drop for RadioReader {
  /// A full ring parks the download writer until the next read, and while it
  /// waits it never sees the cancellation token, so the connection would stay
  /// open with no reader left. Cancel, then read up to one chunk that is
  /// already downloaded (so this read cannot block) to wake the writer. It
  /// finishes its chunk, and its next turn ends on the token or on the body.
  fn drop(&mut self) {
    self.cancelled.store(true, Ordering::Release);
    self.download.cancel_download();
    let unread = self
      .progress
      .borrow()
      .raw
      .saturating_sub(self.consumed.raw.load(Ordering::Relaxed));
    let length = unread.min(CHUNK_BYTES as u64) as usize;
    if length > 0 {
      let mut wake = [0; CHUNK_BYTES];
      let _ = self.download.read(&mut wake[..length]);
    }
  }
}

impl Read for RadioReader {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let position = self.consumed.raw.load(Ordering::Relaxed);
    let remaining = self
      .download
      .content_length()
      .map_or(u64::MAX, |len| len.saturating_sub(position));
    let length = remaining.min(buf.len() as u64) as usize;
    if length == 0 {
      return Ok(0);
    }
    let count = self.download.read(&mut buf[..length])?;
    self.audio.add(&buf[..count]);
    self
      .consumed
      .raw
      .store(position + count as u64, Ordering::Relaxed);
    self
      .consumed
      .audio
      .store(self.audio.total, Ordering::Relaxed);
    Ok(count)
  }
}

impl Seek for RadioReader {
  fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
    match from {
      SeekFrom::Current(0) => Ok(self.consumed.raw.load(Ordering::Relaxed)),
      _ => Err(io::ErrorKind::Unsupported.into()),
    }
  }
}

pub(super) struct StartupBuffer {
  progress: watch::Receiver<Progress>,
  consumed: Arc<Consumed>,
  bitrate: Option<u32>,
}

impl StartupBuffer {
  pub(super) async fn wait(mut self) -> Result<()> {
    loop {
      let progress = *self.progress.borrow_and_update();
      if progress.has_cushion(&self.consumed, self.bitrate) {
        return Ok(());
      }
      self.progress.changed().await?;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn icy_lengths_and_metadata_are_excluded_across_chunks() {
    let mut counter = AudioCounter::new(NonZeroUsize::new(4));
    let mut stream = b"abcd\x01".to_vec();
    stream.extend([0xa4; 16]);
    stream.extend(b"efgh\0ij");
    for chunk in stream.chunks(3) {
      counter.add(chunk);
    }
    assert_eq!(counter.total, 10);
  }

  #[test]
  fn the_timed_cushion_ignores_metadata_and_probe_reads() {
    let consumed = Consumed {
      raw: AtomicU64::new(10_000),
      audio: AtomicU64::new(5_000),
    };
    let mut progress = Progress {
      raw: 40_000,
      audio: 16_000,
      elapsed: Duration::from_secs(4),
      complete: false,
    };
    assert!(!progress.has_cushion(&consumed, Some(32)));
    assert!(!progress.has_cushion(&consumed, None));
    progress.audio += 1_000;
    assert!(progress.has_cushion(&consumed, Some(32)));
    progress.raw = 10_000 + MAX_STARTUP_BYTES;
    assert!(progress.has_cushion(&consumed, Some(320)));
  }
}

pub(super) async fn open(
  http: HttpRadioStream,
  prefix: VecDeque<RadioChunk>,
  interval: Option<NonZeroUsize>,
  bitrate: Option<u32>,
  started: Instant,
) -> Result<(RadioReader, StartupBuffer)> {
  let (tx, progress) = watch::channel(Progress::default());
  let settings =
    Settings::default()
      .prefetch_bytes(0)
      .on_progress(move |body: &RadioBody, state, _| {
        tx.send_replace(Progress {
          raw: state.current_position,
          audio: body.audio.total,
          elapsed: started.elapsed(),
          complete: matches!(state.phase, StreamPhase::Complete),
        });
      });
  let storage = BoundedStorageProvider::new(
    MemoryStorageProvider,
    NonZeroUsize::new(RING_BUFFER_BYTES).unwrap(),
  );
  let cancelled = Arc::new(AtomicBool::new(false));
  let body = RadioBody {
    http,
    prefix,
    chunk: RadioChunk::new(),
    audio: AudioCounter::new(interval),
    cancelled: Arc::clone(&cancelled),
  };
  let download = StreamDownload::from_stream(body, storage, settings)
    .await
    .map_err(|err| anyhow::anyhow!("buffering radio stream: {err}"))?;
  let consumed = Arc::new(Consumed::default());
  let reader = RadioReader {
    download,
    audio: AudioCounter::new(interval),
    consumed: Arc::clone(&consumed),
    progress: progress.clone(),
    cancelled,
  };
  Ok((
    reader,
    StartupBuffer {
      progress,
      consumed,
      bitrate: bitrate.filter(|rate| *rate > 0),
    },
  ))
}
