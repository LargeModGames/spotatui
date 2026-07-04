//! ICY-aware streaming reader for internet-radio playback.
//!
//! An internet-radio stream is **infinite**, so the Subsonic
//! download-to-tempfile approach cannot work — the download never finishes.
//! Instead [`open_radio_stream`] connects with `stream-download`, which
//! prefetches the live stream on the tokio runtime into a bounded in-memory
//! ring buffer and exposes it behind a blocking `Read + Seek` adapter that
//! rodio can decode (non-seekably) on the audio thread.
//!
//! ## ICY now-playing metadata
//!
//! The request carries `Icy-MetaData: 1`; when the server answers with an
//! `icy-metaint` header, in-band `StreamTitle` metadata blocks are interleaved
//! with the audio at that interval. [`IcyMetadataReader`] strips them out of
//! the byte stream (so the decoder never sees them) and publishes each title
//! into a shared cell the playbar reads every frame.
//!
//! The concrete reader stack —
//! `IcyMetadataReader<StreamDownload<BoundedStorageProvider<MemoryStorageProvider>>>`
//! — stays inside this module; callers get a boxed [`StreamReader`].

use std::io::{Read, Seek};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use icy_metadata::{IcyHeaders, IcyMetadataReader};
use stream_download::http::{reqwest, HttpStream};
use stream_download::storage::bounded::BoundedStorageProvider;
use stream_download::storage::memory::MemoryStorageProvider;
use stream_download::{Settings, StreamDownload};

/// The bounds [`LocalPlayer::play_stream`](crate::infra::audio::LocalPlayer::play_stream)
/// requires, as one nameable trait so the reader stack can be boxed.
pub trait StreamReader: Read + Seek + Send + Sync {}
impl<T: Read + Seek + Send + Sync> StreamReader for T {}

/// How many bytes to buffer before playback may start. At a typical 128 kbps
/// (16 KiB/s) this is ~4 s of audio — enough of a cushion against jitter
/// without a long tune-in delay. (stream-download's 256 KiB default would mean
/// a ~16 s tune-in.)
const PREFETCH_BYTES: u64 = 64 * 1024;

/// Size of the in-memory ring buffer holding the live stream. Must comfortably
/// exceed [`PREFETCH_BYTES`]; ~30 s of audio at 128 kbps.
const RING_BUFFER_BYTES: usize = 512 * 1024;

/// Cap on establishing the TCP+TLS connection to the station. A station that
/// never completes the handshake fails fast instead of hanging the serial pump.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Cap on the connect + header-wait phase (`HttpStream::new`). A station that
/// accepts the connection then withholds response headers would otherwise hang
/// the pump forever. This bounds only the tune-in handshake — NOT the audio body,
/// which is an infinite stream and must never be subject to a request timeout.
const HEADER_TIMEOUT: Duration = Duration::from_secs(15);

/// A successfully opened radio stream, ready to hand to
/// [`LocalPlayer::play_stream`](crate::infra::audio::LocalPlayer::play_stream).
pub struct OpenedStream {
  /// The decodable audio byte stream (ICY metadata already stripped).
  pub reader: Box<dyn StreamReader>,
  /// Live now-playing title (`StreamTitle`), updated by the reader as metadata
  /// blocks arrive. Stays `None` for streams without ICY metadata.
  pub now_playing: Arc<Mutex<Option<String>>>,
  /// `type/subtype` from the response Content-Type (e.g. `"audio/mpeg"`),
  /// used to prime rodio's format probe — a live stream has no file extension.
  pub content_type: Option<String>,
  /// Station name self-reported via the `icy-name` header.
  pub station_name: Option<String>,
}

/// Connect to a radio stream URL and start buffering it.
///
/// Async (the prefetch task runs on the current tokio runtime) and slow on bad
/// networks — await it **without** holding the `App` lock. Dropping the
/// returned reader cancels the background download, so teardown is just
/// "stop the sink" (rodio drops the decoder, the decoder drops the reader).
pub async fn open_radio_stream(url: &str) -> Result<OpenedStream> {
  let mut headers = reqwest::header::HeaderMap::new();
  // Ask the server to interleave ICY metadata into the stream.
  icy_metadata::add_icy_metadata_header(&mut headers);
  let client = reqwest::Client::builder()
    // radio-browser.info asks clients to identify themselves; icecast servers
    // occasionally reject UA-less requests too.
    .user_agent(concat!("spotatui/", env!("CARGO_PKG_VERSION")))
    .default_headers(headers)
    // Bound the connect phase so a station that never completes the handshake
    // fails fast. Deliberately NO blanket `.timeout()`: the audio body is an
    // infinite stream, and a request timeout would kill playback mid-song. Only
    // the connect + header phase is bounded (via `HEADER_TIMEOUT` below).
    .connect_timeout(CONNECT_TIMEOUT)
    .build()
    .context("building radio stream HTTP client")?;

  let parsed: reqwest::Url = url
    .parse()
    .with_context(|| format!("invalid radio stream URL: {url}"))?;
  // `HttpStream::new` connects and waits for response headers. A station that
  // accepts the connection then withholds headers would hang the serial pump
  // forever, so bound the header-wait. This wrapper covers ONLY the tune-in
  // handshake; once `HttpStream` is returned, the infinite body is read without
  // any request timeout.
  let stream = tokio::time::timeout(HEADER_TIMEOUT, HttpStream::new(client, parsed))
    .await
    .map_err(|_| anyhow!("timed out waiting for radio stream headers from {url}"))?
    .map_err(|e| anyhow!("connecting to radio stream {url}: {e}"))?;

  let icy_headers = IcyHeaders::parse_from_headers(stream.headers());
  let station_name = icy_headers.name().map(str::to_owned);
  let content_type = stream
    .content_type()
    .as_ref()
    .map(|ct| format!("{}/{}", ct.r#type, ct.subtype));

  let settings = Settings::default().prefetch_bytes(PREFETCH_BYTES);
  let storage = BoundedStorageProvider::new(
    MemoryStorageProvider,
    NonZeroUsize::new(RING_BUFFER_BYTES).expect("ring buffer size is non-zero"),
  );
  let download = StreamDownload::from_stream(stream, storage, settings)
    .await
    .map_err(|e| anyhow!("buffering radio stream: {e}"))?;

  let now_playing: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
  let reader: Box<dyn StreamReader> = match icy_headers.metadata_interval() {
    Some(metaint) => {
      let cell = Arc::clone(&now_playing);
      Box::new(IcyMetadataReader::new(
        download,
        Some(metaint),
        move |meta| {
          // Parse errors on one metadata block are transient — keep the last
          // good title rather than blanking the playbar.
          if let Ok(meta) = meta {
            let title = meta.stream_title().map(str::to_owned);
            if let Ok(mut np) = cell.lock() {
              *np = title;
            }
          }
        },
      ))
    }
    // No `icy-metaint`: a plain audio stream, pass it through untouched.
    None => Box::new(download),
  };

  Ok(OpenedStream {
    reader,
    now_playing,
    content_type,
    station_name,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Spike/smoke test for the load-bearing seam: request ICY metadata through
  /// `stream-download`, read `icy-metaint` off its response headers, and pull
  /// actual audio bytes through the `IcyMetadataReader` stack. SomaFM Groove
  /// Salad is a stable public 128 kbps MP3 ICY stream. Ignored (network); run:
  /// `cargo test --features internet-radio -- --ignored live_stream_opens`
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "hits the live SomaFM stream"]
  async fn live_stream_opens_with_icy_metadata() {
    let opened = open_radio_stream("https://ice1.somafm.com/groovesalad-128-mp3")
      .await
      .expect("stream should open");

    assert_eq!(
      opened.content_type.as_deref(),
      Some("audio/mpeg"),
      "SomaFM serves MP3"
    );
    assert!(
      opened.station_name.is_some(),
      "SomaFM sends an icy-name header"
    );

    // Reading must yield audio bytes (prefetch already buffered some).
    let mut reader = opened.reader;
    let bytes = tokio::task::spawn_blocking(move || {
      let mut buf = vec![0u8; 32 * 1024];
      let mut filled = 0;
      while filled < buf.len() {
        let n = reader.read(&mut buf[filled..]).expect("read stream bytes");
        assert!(n > 0, "live stream must not EOF");
        filled += n;
      }
      buf
    })
    .await
    .unwrap();
    // MP3 audio with the ICY blocks stripped: an ID3 tag or an MPEG frame sync
    // somewhere near the start (frame boundaries need not align with byte 0).
    let looks_like_mp3 = bytes.starts_with(b"ID3")
      || bytes
        .windows(2)
        .take(4096)
        .any(|w| w[0] == 0xFF && (w[1] & 0xE0) == 0xE0);
    assert!(looks_like_mp3, "expected MP3 data after ICY stripping");
  }

  /// End-to-end: open the live stream and decode it through the shared rodio
  /// sink, asserting playback advances and ICY now-playing arrives. Ignored
  /// (needs network **and** an audio output device); run:
  /// `cargo test --features internet-radio -- --ignored live_stream_plays`
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "hits the live SomaFM stream AND requires an audio output device"]
  async fn live_stream_plays_through_sink() {
    use crate::infra::audio::LocalPlayer;
    use std::time::Duration;

    let opened = open_radio_stream("https://ice1.somafm.com/groovesalad-128-mp3")
      .await
      .expect("stream should open");
    let now_playing = Arc::clone(&opened.now_playing);

    let player = Arc::new(LocalPlayer::new().expect("open default output device"));
    let decode_player = Arc::clone(&player);
    let (reader, mime) = (opened.reader, opened.content_type);
    tokio::task::spawn_blocking(move || decode_player.play_stream(reader, mime.as_deref()))
      .await
      .unwrap()
      .expect("stream should decode and play");

    assert!(!player.is_paused(), "should be playing after play_stream");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
      player.position() >= Duration::from_millis(500),
      "live playback position should advance, got {:?}",
      player.position()
    );
    assert!(
      !player.is_finished(),
      "an infinite stream must not report finished"
    );

    // SomaFM interleaves StreamTitle at short intervals; give it a few seconds.
    let mut got_title = false;
    for _ in 0..20 {
      if now_playing.lock().unwrap().is_some() {
        got_title = true;
        break;
      }
      tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(got_title, "ICY StreamTitle should arrive while playing");

    player.stop();
    assert!(player.is_finished(), "stop should clear the sink");
  }
}
