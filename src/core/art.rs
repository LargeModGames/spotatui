//! Frontend-agnostic cover art: what to fetch ([`CoverArtRequest`]), the
//! fetch/decode itself ([`fetch_and_decode`]), and the decoded image plus its
//! load status ([`CoverArtStore`]) that frontends render from. Turning the
//! decoded image into a terminal graphics protocol is the TUI's job
//! (`tui::cover_art`); nothing in here may depend on ratatui.

use anyhow::anyhow;
use log::{debug, info};
use std::sync::OnceLock;

/// Cap on a cover-art response body, checked against both the declared and
/// the actual size. Real covers are a few hundred KiB; the cap only exists so
/// a misbehaving server cannot make the app buffer an unbounded body (the
/// decode needs it all in memory).
const MAX_COVER_ART_BYTES: u64 = 16 * 1024 * 1024;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// The process-wide cover-art HTTP client, so keep-alive connections to the
/// art CDN are reused across track changes. Built fallibly *outside*
/// `get_or_init` so a TLS setup failure surfaces as a fetch error instead of
/// a panic; a lost build race just discards the extra client.
fn client() -> anyhow::Result<&'static reqwest::Client> {
  if let Some(client) = CLIENT.get() {
    return Ok(client);
  }
  let client = reqwest::Client::builder()
    .connect_timeout(std::time::Duration::from_secs(10))
    .timeout(std::time::Duration::from_secs(30))
    .build()
    .map_err(|e| anyhow!(e))?;
  Ok(CLIENT.get_or_init(|| client))
}

/// What to fetch for the current track's cover art. Built once per track change
/// by the shared detector and handled off the `App` lock in the network layer.
#[derive(Clone, Debug)]
pub enum CoverArtRequest {
  /// Download and decode an image from a URL (Spotify album art, YouTube
  /// thumbnail, Subsonic getCoverArt).
  Url(String),
  /// Read the embedded cover picture out of a local audio file. `key` is the
  /// track's `file://` URI (used as the cache identity); `path` is the resolved
  /// filesystem path handed to the blocking tag reader.
  #[cfg(feature = "local-files")]
  LocalFile {
    key: String,
    path: std::path::PathBuf,
  },
}

impl CoverArtRequest {
  /// The cache-identity key for this request: the image URL, or the file URI.
  /// Used to skip re-fetching art already held for the same track.
  pub fn key(&self) -> &str {
    match self {
      CoverArtRequest::Url(url) => url,
      #[cfg(feature = "local-files")]
      CoverArtRequest::LocalFile { key, .. } => key,
    }
  }
}

/// Status of the currently-playing track's cover art, mirroring `LyricsStatus`.
/// Drives the placeholder message shown when art can't be displayed, so a
/// missing image reads as an explicit state rather than silently showing
/// nothing (or, worse, the previous track's art).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CoverArtStatus {
  /// Nothing is playing / no art has been requested yet.
  #[default]
  NotStarted,
  /// A fetch/decode for the current track is in flight.
  Loading,
  /// Art for the current track is loaded and rendering.
  Loaded,
  /// The current source has no cover art to show (e.g. internet radio, or a
  /// local file with no embedded picture).
  Unavailable,
  /// A fetch/decode was attempted for the current track but failed.
  Failed,
}

/// Downloads and decodes the cover-art image at `url`, off any external lock.
///
/// This is a **free function** on purpose: it borrows no `App` state, so its
/// `.await` points can be reached with the `App` mutex fully dropped. The
/// network fetch and the (synchronous, CPU-bound) image decode are the
/// expensive parts and must never run while the `App` guard is held, or the
/// render loop — which locks the same mutex every frame — freezes for the
/// whole CDN round-trip (#142).
///
/// The shared client carries explicit timeouts so a hung CDN cannot stall the
/// fetch forever even off-lock (`reqwest::get` uses a default client with
/// none), and the body is read through a bounded loop: `content_length()` is
/// only a hint and `bytes()` would buffer an arbitrarily large body, so the
/// [`MAX_COVER_ART_BYTES`] cap is enforced on the declared size first and
/// again while accumulating the actual bytes.
pub async fn fetch_and_decode(url: &str) -> anyhow::Result<image::DynamicImage> {
  info!("getting new cover art image...");

  let res = client()?
    .get(url)
    .send()
    .await
    .and_then(|r| r.error_for_status());
  let mut res = match res {
    Ok(res) => res,
    Err(e) => return Err(anyhow!(e)),
  };

  if let Some(declared) = res.content_length() {
    if declared > MAX_COVER_ART_BYTES {
      return Err(anyhow!(
        "cover art response declares {declared} bytes (limit {MAX_COVER_ART_BYTES})"
      ));
    }
  }

  // The declared size only pre-allocates; the cap above bounds it.
  let mut file = Vec::with_capacity(res.content_length().unwrap_or(0) as usize);
  while let Some(chunk) = res.chunk().await.map_err(|e| anyhow!(e))? {
    if (file.len() + chunk.len()) as u64 > MAX_COVER_ART_BYTES {
      return Err(anyhow!(
        "cover art response exceeded the {MAX_COVER_ART_BYTES}-byte limit"
      ));
    }
    file.extend_from_slice(&chunk);
  }
  debug!("finished reading response: {} bytes", file.len());

  image::load_from_memory(&file).map_err(|e| anyhow!(e))
}

/// The decoded cover art for the current track, keyed by the request that
/// produced it, plus its load status. Lives on `App` so any frontend can read
/// it; the TUI caches its terminal protocols on [`Self::key`] and rebuilds
/// them only when the key changes.
#[derive(Default)]
pub struct CoverArtStore {
  art: Option<(String, image::DynamicImage)>,
  /// Status of the current track's cover art, driving the placeholder message.
  pub status: CoverArtStatus,
}

impl CoverArtStore {
  /// The cache-identity key of the stored image (the URL or file URI it was
  /// decoded from), if any art is loaded.
  pub fn key(&self) -> Option<&str> {
    self.art.as_ref().map(|(key, _)| key.as_str())
  }

  /// The decoded image, if any art is loaded. Read by whichever frontend
  /// renders it; a decode-only build (`art-decode` without `cover-art`)
  /// fetches art solely for the adaptive theme and has no reader.
  #[cfg_attr(not(feature = "cover-art"), allow(dead_code))]
  pub fn image(&self) -> Option<&image::DynamicImage> {
    self.art.as_ref().map(|(_, image)| image)
  }

  pub fn available(&self) -> bool {
    self.art.is_some()
  }

  /// Store an already-decoded image under its cache key, replacing whatever
  /// was held. Cheap: the slow fetch/decode happens in [`fetch_and_decode`],
  /// off the `App` lock.
  pub fn store_decoded(&mut self, key: String, image: image::DynamicImage) {
    info!("got new cover art: {key}");
    self.art = Some((key, image));
  }

  /// Drop any stored cover art so the pane renders nothing. Used when
  /// switching to a track/source with no art, or after a failed fetch, so
  /// stale art from the previous track can never linger on screen.
  pub fn clear(&mut self) {
    self.art = None;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{Read, Write};

  /// Serve one hand-written HTTP response on a real loopback listener, so the
  /// fetch path is exercised without any mock layer (house HTTP-test pattern).
  fn one_shot_server(
    response: impl FnOnce(&mut std::net::TcpStream) + Send + 'static,
  ) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
      let (mut stream, _) = listener.accept().unwrap();
      // Read (some of) the request so the client sees a well-formed exchange.
      let mut buf = [0u8; 1024];
      let _ = stream.read(&mut buf);
      response(&mut stream);
    });
    (addr, handle)
  }

  #[tokio::test]
  async fn oversized_declared_cover_art_body_is_rejected() {
    let (addr, server) = one_shot_server(|stream| {
      let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\nContent-Type: image/jpeg\r\n\r\n",
      );
    });

    let err = fetch_and_decode(&format!("http://{addr}/art.jpg"))
      .await
      .unwrap_err();
    assert!(err.to_string().contains("declares"), "got: {err}");
    server.join().unwrap();
  }

  #[tokio::test]
  async fn cover_art_body_exceeding_the_cap_is_rejected_mid_read() {
    let (addr, server) = one_shot_server(|stream| {
      // No Content-Length, close-delimited body: the declared-size check
      // cannot fire, so the accumulation cap has to. Write errors are
      // expected once the client bails mid-body.
      let _ = stream
        .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: image/jpeg\r\n\r\n");
      let chunk = vec![0u8; 1024 * 1024];
      for _ in 0..17 {
        if stream.write_all(&chunk).is_err() {
          break;
        }
      }
    });

    let err = fetch_and_decode(&format!("http://{addr}/art.jpg"))
      .await
      .unwrap_err();
    assert!(err.to_string().contains("exceeded"), "got: {err}");
    let _ = server.join();
  }

  #[test]
  fn request_key_is_the_url() {
    let request = CoverArtRequest::Url("https://img.example/cover.jpg".to_string());
    assert_eq!(request.key(), "https://img.example/cover.jpg");
  }

  #[test]
  fn store_replaces_and_clear_drops_the_held_art() {
    let mut store = CoverArtStore::default();
    assert!(!store.available());
    assert_eq!(store.key(), None);

    store.store_decoded("a".to_string(), image::DynamicImage::new_rgb8(1, 1));
    assert!(store.available());
    assert_eq!(store.key(), Some("a"));
    assert!(store.image().is_some());

    store.store_decoded("b".to_string(), image::DynamicImage::new_rgb8(2, 2));
    assert_eq!(store.key(), Some("b"));

    store.clear();
    assert!(!store.available());
    assert_eq!(store.key(), None);
    assert!(store.image().is_none());
  }
}
