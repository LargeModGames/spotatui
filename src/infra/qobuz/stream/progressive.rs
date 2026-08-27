//! Progressive delivery: the track plays while it downloads.
//!
//! [`SegmentStream`] is a `stream-download` source over the segment transport.
//! It yields the codec header, then each decrypted audio segment in order, and
//! restarts at any byte offset when the decoder seeks past the downloaded
//! part. `stream-download` writes the bytes into the session's tempfile
//! ([`TempfileStorage`]), blocks the decoder's reads until they exist, and
//! stops the download when the reader is dropped. The session keeps the
//! `NamedTempFile`, so the finished file outlives the reader (repeat-one).

use std::convert::Infallible;
use std::fs::File;
use std::future::Future;
use std::io::{self, BufReader};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::{anyhow, Context as _, Result};
use bytes::Bytes;
use futures::Stream;
use reqwest::Client;
use stream_download::source::SourceStream;
use stream_download::storage::StorageProvider;
use stream_download::{Settings, StreamDownload};
use tempfile::NamedTempFile;
use tokio::task::JoinHandle;

use super::cmaf::InitSegment;
use super::download::fetch_audio_segment;

/// Consecutive failed fetches of one segment before the stream fails.
const MAX_ATTEMPTS: u32 = 3;
/// Bytes downloaded before the first read returns: a cushion against jitter
/// that costs well under a second on a normal connection.
const PREFETCH_BYTES: u64 = 512 * 1024;
/// Time without a chunk before `stream-download` asks for a reconnect. Above
/// the HTTP client's request timeout, so a stalled segment fails there first
/// and counts as an attempt.
const STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// The reader the decoder pulls from; dropping it cancels the download.
pub type TrackReader = StreamDownload<TempfileStorage>;

/// Start the download into `file` and return the decoder's reader.
pub async fn open(stream: SegmentStream, file: &NamedTempFile) -> Result<TrackReader> {
  let storage = TempfileStorage::new(file).context("reopening stream file")?;
  let settings = Settings::default()
    .prefetch_bytes(PREFETCH_BYTES)
    .retry_timeout(STALL_TIMEOUT);
  StreamDownload::from_stream(stream, storage, settings)
    .await
    .map_err(|e| anyhow!("{e}"))
}

/// Storage over the session's own tempfile: one reopened handle for the
/// download's writes and one for the decoder's reads.
pub struct TempfileStorage {
  reader: File,
  writer: File,
}

impl TempfileStorage {
  fn new(file: &NamedTempFile) -> io::Result<Self> {
    Ok(Self {
      reader: file.reopen()?,
      writer: file.reopen()?,
    })
  }
}

impl StorageProvider for TempfileStorage {
  type Reader = BufReader<File>;
  type Writer = File;

  fn into_reader_writer(
    self,
    _content_length: Option<u64>,
  ) -> io::Result<(Self::Reader, Self::Writer)> {
    Ok((BufReader::new(self.reader), self.writer))
  }
}

/// A track as a byte stream: chunk 0 is the codec header, chunk `i >= 1` is
/// the decrypted audio segment `i`.
pub struct SegmentStream {
  http: Client,
  url_template: String,
  content_key: [u8; 16],
  header: Bytes,
  /// `starts[i]` is the byte offset of chunk `i`; the last entry is the total.
  starts: Vec<u64>,
  /// The next chunk to yield.
  next: usize,
  /// Bytes to drop from the front of the next chunk (a seek into a chunk).
  skip: usize,
  /// Chunks at or past this index are not yielded until the next seek.
  end: usize,
  in_flight: Option<JoinHandle<Result<Bytes>>>,
  attempts: u32,
  failure: Option<String>,
}

impl SegmentStream {
  pub fn new(
    http: Client,
    url_template: String,
    content_key: [u8; 16],
    init: &InitSegment,
  ) -> Self {
    let mut starts = Vec::with_capacity(init.segment_lengths.len() + 2);
    let mut position = 0u64;
    starts.push(position);
    position += init.header.len() as u64;
    starts.push(position);
    for &len in &init.segment_lengths {
      position += u64::from(len);
      starts.push(position);
    }
    let chunks = starts.len() - 1;
    Self {
      http,
      url_template,
      content_key,
      header: Bytes::copy_from_slice(&init.header),
      starts,
      next: 0,
      skip: 0,
      end: chunks,
      in_flight: None,
      attempts: 0,
      failure: None,
    }
  }

  fn chunk_count(&self) -> usize {
    self.starts.len() - 1
  }

  /// The size of the finished file.
  pub fn total_bytes(&self) -> u64 {
    self.starts[self.chunk_count()]
  }

  /// The chunk that holds byte `position` and the offset inside that chunk;
  /// `(chunk_count, 0)` at or past the end.
  fn locate(&self, position: u64) -> (usize, usize) {
    let chunks = self.chunk_count();
    let index = self.starts[1..].partition_point(|&end| end <= position);
    if index >= chunks {
      (chunks, 0)
    } else {
      (index, (position - self.starts[index]) as usize)
    }
  }

  /// The exclusive chunk bound that covers the exclusive byte bound `end`.
  fn chunk_bound(&self, end: Option<u64>) -> usize {
    match end {
      None => self.chunk_count(),
      Some(0) => 0,
      Some(end) => (self.locate(end - 1).0 + 1).min(self.chunk_count()),
    }
  }

  /// Continue from byte `start`, yielding chunks below `end_chunk` only.
  fn restart(&mut self, start: u64, end_chunk: usize) -> io::Result<()> {
    if let Some(message) = &self.failure {
      return Err(io::Error::other(message.clone()));
    }
    self.abort_in_flight();
    let (chunk, skip) = self.locate(start);
    self.next = chunk;
    self.skip = skip;
    self.end = end_chunk;
    self.attempts = 0;
    Ok(())
  }

  fn abort_in_flight(&mut self) {
    if let Some(task) = self.in_flight.take() {
      task.abort();
    }
  }

  fn spawn_fetch(&self, index: u32) -> JoinHandle<Result<Bytes>> {
    let http = self.http.clone();
    let template = self.url_template.clone();
    let key = self.content_key;
    tokio::spawn(async move { fetch_audio_segment(&http, &template, index, &key).await })
  }
}

impl Drop for SegmentStream {
  fn drop(&mut self) {
    self.abort_in_flight();
  }
}

impl Stream for SegmentStream {
  type Item = Result<Bytes>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = &mut *self;
    if this.failure.is_some() || this.next >= this.end {
      return Poll::Ready(None);
    }
    let chunk = if this.next == 0 {
      this.header.clone()
    } else {
      let index = this.next as u32;
      if this.in_flight.is_none() {
        let task = this.spawn_fetch(index);
        this.in_flight = Some(task);
      }
      let task = this.in_flight.as_mut().expect("fetch task was just set");
      let outcome = match Pin::new(task).poll(cx) {
        Poll::Pending => return Poll::Pending,
        Poll::Ready(Ok(outcome)) => outcome,
        Poll::Ready(Err(join)) => Err(anyhow!("segment {index} fetch task: {join}")),
      };
      this.in_flight = None;
      match outcome {
        Ok(bytes) => bytes,
        Err(e) => {
          // `stream-download` logs the error and polls again; the failure
          // flag ends the stream after the last attempt.
          this.attempts += 1;
          if this.attempts >= MAX_ATTEMPTS {
            this.failure = Some(format!("{e:#}"));
          }
          return Poll::Ready(Some(Err(e)));
        }
      }
    };
    let skip = this.skip.min(chunk.len());
    this.next += 1;
    this.skip = 0;
    this.attempts = 0;
    Poll::Ready(Some(Ok(chunk.slice(skip..))))
  }
}

impl SourceStream for SegmentStream {
  type Params = Self;
  type StreamCreationError = Infallible;

  async fn create(params: Self) -> Result<Self, Infallible> {
    Ok(params)
  }

  fn content_length(&self) -> Option<u64> {
    Some(self.total_bytes())
  }

  async fn seek_range(&mut self, start: u64, end: Option<u64>) -> io::Result<()> {
    let bound = self.chunk_bound(end);
    self.restart(start, bound)
  }

  async fn reconnect(&mut self, current_position: u64) -> io::Result<()> {
    let bound = self.end;
    self.restart(current_position, bound)
  }

  fn supports_seek(&self) -> bool {
    true
  }
}

#[cfg(test)]
mod tests {
  use std::io::{Read, Seek, SeekFrom};

  use futures::StreamExt;

  use super::super::download::fetch_init;
  use super::super::download::test_support::*;
  use super::*;

  fn stream_of(lengths: &[u32]) -> SegmentStream {
    let init = InitSegment {
      header: vec![0; 10],
      segment_lengths: lengths.to_vec(),
      sample_rate: 44_100,
      bits_per_sample: 16,
    };
    SegmentStream::new(
      Client::new(),
      "http://127.0.0.1:1/$SEGMENT$".to_string(),
      KEY,
      &init,
    )
  }

  async fn fixture_stream(segments: Vec<Vec<u8>>) -> SegmentStream {
    let template = serve(segments).await;
    let http = Client::new();
    let init = fetch_init(&http, &template).await.unwrap();
    SegmentStream::new(http, template, KEY, &init)
  }

  async fn collect(stream: &mut SegmentStream) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
      out.extend_from_slice(&chunk.unwrap());
    }
    out
  }

  #[tokio::test]
  async fn locate_maps_offsets_to_chunks() {
    let s = stream_of(&[70, 20]);
    assert_eq!(s.total_bytes(), 100);
    assert_eq!(s.locate(0), (0, 0));
    assert_eq!(s.locate(9), (0, 9));
    assert_eq!(s.locate(10), (1, 0));
    assert_eq!(s.locate(45), (1, 35));
    assert_eq!(s.locate(80), (2, 0));
    assert_eq!(s.locate(99), (2, 19));
    assert_eq!(s.locate(100), (3, 0));
    assert_eq!(s.locate(500), (3, 0));
  }

  #[tokio::test]
  async fn chunk_bound_covers_the_chunk_that_holds_the_last_byte() {
    let s = stream_of(&[70, 20]);
    assert_eq!(s.chunk_bound(None), 3);
    assert_eq!(s.chunk_bound(Some(0)), 0);
    assert_eq!(s.chunk_bound(Some(10)), 1);
    assert_eq!(s.chunk_bound(Some(11)), 2);
    assert_eq!(s.chunk_bound(Some(100)), 3);
    assert_eq!(s.chunk_bound(Some(1000)), 3);
  }

  #[tokio::test]
  async fn stream_yields_header_then_decrypted_segments() {
    let (segments, expected) = fixture_track();
    let mut stream = fixture_stream(segments).await;
    assert_eq!(stream.total_bytes(), expected.len() as u64);
    assert_eq!(collect(&mut stream).await, expected);
  }

  #[tokio::test]
  async fn seek_range_restarts_inside_a_segment_and_honors_the_end_bound() {
    let (segments, expected) = fixture_track();
    let mut stream = fixture_stream(segments).await;
    stream.seek_range(45, None).await.unwrap();
    assert_eq!(collect(&mut stream).await, expected[45..]);
    // Bytes 0..50 lie in the header and segment 1 (0..112): both are yielded.
    stream.seek_range(0, Some(50)).await.unwrap();
    assert_eq!(collect(&mut stream).await, expected[..112]);
    stream.seek_range(112, None).await.unwrap();
    assert_eq!(collect(&mut stream).await, expected[112..]);
  }

  #[tokio::test]
  async fn missing_segment_fails_after_the_last_attempt() {
    let (mut segments, expected) = fixture_track();
    segments.pop();
    let mut stream = fixture_stream(segments).await;
    let mut out = Vec::new();
    let mut errors = 0;
    while let Some(chunk) = stream.next().await {
      match chunk {
        Ok(bytes) => out.extend_from_slice(&bytes),
        Err(_) => errors += 1,
      }
    }
    assert_eq!(out, expected[..112]);
    assert_eq!(errors, MAX_ATTEMPTS);
    assert!(stream.seek_range(112, None).await.is_err());
  }

  #[tokio::test]
  async fn reader_returns_the_track_and_fills_the_tempfile() {
    let (segments, expected) = fixture_track();
    let stream = fixture_stream(segments).await;
    let tmp = NamedTempFile::new().unwrap();
    let mut reader = open(stream, &tmp).await.unwrap();
    let bytes = tokio::task::spawn_blocking(move || {
      let mut out = Vec::new();
      reader.read_to_end(&mut out).map(|_| out)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(bytes, expected);
    assert_eq!(std::fs::read(tmp.path()).unwrap(), expected);
  }

  #[tokio::test]
  async fn reader_seeks_within_the_track() {
    let (segments, expected) = fixture_track();
    let stream = fixture_stream(segments).await;
    let tmp = NamedTempFile::new().unwrap();
    let mut reader = open(stream, &tmp).await.unwrap();
    let tail_start = expected.len() - 15;
    let (tail, all) = tokio::task::spawn_blocking(move || {
      reader.seek(SeekFrom::Start(tail_start as u64)).unwrap();
      let mut tail = Vec::new();
      reader.read_to_end(&mut tail).unwrap();
      reader.seek(SeekFrom::Start(0)).unwrap();
      let mut all = Vec::new();
      reader.read_to_end(&mut all).unwrap();
      (tail, all)
    })
    .await
    .unwrap();
    assert_eq!(tail, expected[tail_start..]);
    assert_eq!(all, expected);
  }

  #[tokio::test]
  async fn reader_fails_when_a_segment_is_missing() {
    let (mut segments, _) = fixture_track();
    segments.pop();
    let stream = fixture_stream(segments).await;
    let tmp = NamedTempFile::new().unwrap();
    let mut reader = open(stream, &tmp).await.unwrap();
    let result = tokio::task::spawn_blocking(move || {
      let mut out = Vec::new();
      reader.read_to_end(&mut out)
    })
    .await
    .unwrap();
    assert!(result.is_err());
  }
}
