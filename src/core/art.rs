//! Frontend-agnostic cover art: what to fetch ([`CoverArtRequest`]), the
//! fetch/decode itself ([`fetch_and_decode`]), and the decoded image plus its
//! load status ([`CoverArtStore`]) that frontends render from. Turning the
//! decoded image into a terminal graphics protocol is the TUI's job
//! (`tui::cover_art`); nothing in here may depend on ratatui.

use anyhow::anyhow;
use log::{debug, info};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Cap on a cover-art response body, checked against both the declared and
/// the actual size. Real covers are a few hundred KiB; the cap only exists so
/// a misbehaving server cannot make the app buffer an unbounded body (the
/// decode needs it all in memory).
const MAX_COVER_ART_BYTES: u64 = 16 * 1024 * 1024;

/// Cap on either decoded dimension. [`MAX_COVER_ART_BYTES`] bounds only the
/// compressed body, and a tiny file can still declare enormous dimensions
/// (`image`'s default `max_alloc` is best-effort, not strict), so the decoder
/// gets strict width/height limits too. 4096 px is far beyond what any
/// terminal cell grid can show.
const MAX_COVER_ART_DIMENSION: u32 = 4096;

/// Total budget for decoded cover art held in RAM. Covers every cached image
/// process-wide; an image that cannot fit on its own leaves the cache inactive
/// rather than thrashing on every track change.
/// 64 MiB holds dozens of typical 640 px covers, and any 8-bit cover within
/// [`MAX_COVER_ART_DIMENSION`] fits.
const DECODED_ART_CACHE_BUDGET_BYTES: u64 = 64 * 1024 * 1024;

const LOAD_SAMPLE_WINDOW: usize = 8;

pub(crate) fn redacted_art_key(key: &str) -> &str {
  key.find(['?', '#']).map_or(key, |index| &key[..index])
}

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static DECODED_ART_CACHE: OnceLock<Mutex<DecodedArtCache>> = OnceLock::new();

struct DecodedArtCache {
  entries: VecDeque<(String, Arc<image::DynamicImage>)>,
  bytes: u64,
  budget: u64,
}

impl Default for DecodedArtCache {
  fn default() -> Self {
    Self::new(DECODED_ART_CACHE_BUDGET_BYTES)
  }
}

impl DecodedArtCache {
  fn new(budget: u64) -> Self {
    Self {
      entries: VecDeque::new(),
      bytes: 0,
      budget,
    }
  }

  fn image_bytes(image: &image::DynamicImage) -> u64 {
    u64::from(image.width())
      * u64::from(image.height())
      * u64::from(image.color().bytes_per_pixel())
  }

  fn get(&mut self, key: &str) -> Option<Arc<image::DynamicImage>> {
    let index = self
      .entries
      .iter()
      .position(|(entry_key, _)| entry_key == key)?;
    let entry = self.entries.remove(index)?;
    self
      .entries
      .push_back((entry.0.clone(), Arc::clone(&entry.1)));
    Some(entry.1)
  }

  fn put(&mut self, key: &str, image: Arc<image::DynamicImage>) {
    let bytes = Self::image_bytes(&image);
    if bytes > self.budget {
      return;
    }
    if let Some(index) = self
      .entries
      .iter()
      .position(|(entry_key, _)| entry_key == key)
    {
      let previous = self.entries.remove(index).expect("checked position");
      self.bytes = self.bytes.saturating_sub(Self::image_bytes(&previous.1));
    }
    self.entries.push_back((key.to_string(), image));
    self.bytes = self.bytes.saturating_add(bytes);
    while self.bytes > self.budget {
      let Some(evicted) = self.entries.pop_front() else {
        break;
      };
      self.bytes = self.bytes.saturating_sub(Self::image_bytes(&evicted.1));
    }
  }
}

fn decoded_art_cache() -> &'static Mutex<DecodedArtCache> {
  DECODED_ART_CACHE.get_or_init(|| Mutex::new(DecodedArtCache::default()))
}

pub(crate) fn cached_decoded_art(key: &str) -> Option<Arc<image::DynamicImage>> {
  decoded_art_cache()
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .get(key)
}

pub(crate) fn remember_decoded_art(key: &str, image: Arc<image::DynamicImage>) {
  decoded_art_cache()
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .put(key, image);
}

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
  /// thumbnail, Subsonic getCoverArt, Qobuz album art).
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverArtTarget {
  Playing,
  #[cfg(feature = "cover-art")]
  BrowsePreview,
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
  /// Cover art is switched off by configuration or the decode cache is
  /// disabled by its byte cap, so no art will be fetched for this session.
  Inactive,
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
pub async fn fetch_and_decode(url: &str) -> anyhow::Result<Arc<image::DynamicImage>> {
  if let Some(cached) = cached_decoded_art(url) {
    debug!("reusing cached cover art: {}", redacted_art_key(url));
    return Ok(cached);
  }

  info!("getting new cover art image...");

  let res = client()?
    .get(url)
    .send()
    .await
    .and_then(|r| r.error_for_status());
  let mut res = match res {
    Ok(res) => res,
    Err(e) => return Err(anyhow!(e.without_url())),
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
  while let Some(chunk) = res.chunk().await.map_err(|e| anyhow!(e.without_url()))? {
    if (file.len() + chunk.len()) as u64 > MAX_COVER_ART_BYTES {
      return Err(anyhow!(
        "cover art response exceeded the {MAX_COVER_ART_BYTES}-byte limit"
      ));
    }
    file.extend_from_slice(&chunk);
  }
  debug!("finished reading response: {} bytes", file.len());

  let image = Arc::new(decode_bounded(&file)?);
  remember_decoded_art(url, Arc::clone(&image));
  Ok(image)
}

/// Decode a downloaded cover-art body with strict dimension limits, so a
/// small compressed file cannot expand into an oversized decoded allocation.
fn decode_bounded(bytes: &[u8]) -> anyhow::Result<image::DynamicImage> {
  let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
    .with_guessed_format()
    .map_err(|e| anyhow!(e))?;
  let mut limits = image::Limits::default();
  limits.max_image_width = Some(MAX_COVER_ART_DIMENSION);
  limits.max_image_height = Some(MAX_COVER_ART_DIMENSION);
  reader.limits(limits);
  reader.decode().map_err(|e| anyhow!(e))
}

/// The decoded cover art for the current track, keyed by the request that
/// produced it, plus its load status. Lives on `App` so any frontend can read
/// it; the TUI caches its terminal protocols on [`Self::key`] and rebuilds
/// them only when the key changes.
pub struct CoverArtStore {
  art: Option<(String, Arc<image::DynamicImage>)>,
  cached_bytes: u64,
  cap_bytes: u64,
  cache_inactive: bool,
  /// Status of the current track's cover art, driving the placeholder message.
  pub status: CoverArtStatus,
  fetch_started: Option<Instant>,
  transition: Option<CoverArtTransition>,
  snap_generation: u64,
  load_samples: VecDeque<Duration>,
}

struct CoverArtTransition {
  key: String,
  started: Instant,
}

impl Default for CoverArtStore {
  fn default() -> Self {
    Self {
      art: None,
      cached_bytes: 0,
      cap_bytes: DECODED_ART_CACHE_BUDGET_BYTES,
      cache_inactive: false,
      status: CoverArtStatus::NotStarted,
      fetch_started: None,
      transition: None,
      snap_generation: 0,
      load_samples: VecDeque::new(),
    }
  }
}

impl CoverArtStore {
  /// Decode budget in bytes. Zero, or an image too large to fit on its own,
  /// leaves the cache inactive rather than thrashing on every track change.
  pub fn cache_active(&self) -> bool {
    !self.cache_inactive && self.cap_bytes > 0
  }

  /// Decoded bytes currently held by the cache, for tests and diagnostics.
  #[cfg_attr(not(test), allow(dead_code))]
  pub fn cached_bytes(&self) -> u64 {
    self.cached_bytes
  }

  #[cfg(test)]
  pub(crate) fn with_cap_bytes(cap_bytes: u64) -> Self {
    Self {
      cap_bytes,
      ..Self::default()
    }
  }

  /// The cache-identity key of the stored image (the URL or file URI it was
  /// decoded from), if any art is loaded.
  pub fn key(&self) -> Option<&str> {
    self.art.as_ref().map(|(key, _)| key.as_str())
  }

  /// The key and start time of the art transition that is currently pending.
  /// The TUI uses this to start fading the old image as soon as a new one is
  /// requested, rather than waiting for the fetch to finish.
  pub fn transition(&self) -> Option<(&str, Instant)> {
    self
      .transition
      .as_ref()
      .map(|transition| (transition.key.as_str(), transition.started))
  }

  /// Record that art for `key` is on its way, without timing a fetch. The
  /// frontend starts fading from `started` while the debounce and the load are
  /// still pending; a repeated call for the same key keeps the first start.
  pub fn begin_transition_at(&mut self, key: &str, started: Instant) {
    if self
      .transition
      .as_ref()
      .is_some_and(|transition| transition.key == key)
    {
      return;
    }
    self.transition = Some(CoverArtTransition {
      key: key.to_string(),
      started,
    });
  }

  pub fn begin_fetch(&mut self, key: &str) {
    let started = Instant::now();
    self.fetch_started = Some(started);
    self.begin_transition_at(key, started);
  }

  pub fn begin_clear(&mut self) {
    self.transition = Some(CoverArtTransition {
      key: String::new(),
      started: Instant::now(),
    });
  }

  pub fn snap_generation(&self) -> u64 {
    self.snap_generation
  }

  pub fn snap(&mut self) {
    self.transition = None;
    self.snap_generation = self.snap_generation.wrapping_add(1);
  }

  /// Share the decoded image with a renderer without copying its pixels.
  #[cfg(feature = "cover-art")]
  pub fn image_shared(&self) -> Option<Arc<image::DynamicImage>> {
    self.art.as_ref().map(|(_, image)| Arc::clone(image))
  }

  pub fn available(&self) -> bool {
    self.art.is_some()
  }

  /// Store an already-decoded image under its cache key, replacing whatever
  /// was held. Cheap: the slow fetch/decode happens in [`fetch_and_decode`],
  /// off the `App` lock.
  pub fn store_decoded(&mut self, key: String, image: impl Into<Arc<image::DynamicImage>>) {
    info!("got new cover art: {}", redacted_art_key(&key));
    let image = image.into();
    let bytes = DecodedArtCache::image_bytes(&image);
    if bytes > self.cap_bytes {
      info!(
        "cover art cache inactive: decoded image needs {bytes} bytes (cap {})",
        self.cap_bytes
      );
      self.art = None;
      self.cached_bytes = 0;
      self.cache_inactive = true;
      return;
    }
    if self.cached_bytes.saturating_add(bytes) > self.cap_bytes {
      info!(
        "cover art cache evicting {:?} ({} decoded bytes) to stay within {} bytes",
        self.key(),
        self.cached_bytes,
        self.cap_bytes
      );
      self.art = None;
      self.cached_bytes = 0;
    }
    remember_decoded_art(&key, Arc::clone(&image));
    self.art = Some((key, image));
    self.cached_bytes = bytes;
    self.cache_inactive = false;
  }

  pub fn mark_loaded(&mut self) {
    self.status = if self.cache_active() {
      CoverArtStatus::Loaded
    } else {
      CoverArtStatus::Inactive
    };
  }

  #[cfg(test)]
  pub fn note_fetch_started(&mut self) {
    self.fetch_started = Some(Instant::now());
  }

  pub fn note_fetch_finished(&mut self, success: bool) {
    let Some(started) = self.fetch_started.take() else {
      return;
    };
    if !success {
      return;
    }
    while self.load_samples.len() >= LOAD_SAMPLE_WINDOW {
      self.load_samples.pop_front();
    }
    self.load_samples.push_back(started.elapsed());
  }

  pub fn average_load_duration(&self) -> Option<Duration> {
    if self.load_samples.is_empty() {
      return None;
    }
    let total = self.load_samples.iter().copied().sum::<Duration>();
    Some(total / self.load_samples.len() as u32)
  }

  /// Drop any stored cover art so the pane renders nothing. Used when
  /// switching to a track/source with no art, or after a failed fetch, so
  /// stale art from the previous track can never linger on screen.
  pub fn clear(&mut self) {
    self.art = None;
    self.cached_bytes = 0;
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
  async fn a_failed_cover_fetch_keeps_the_query_out_of_the_error() {
    let refused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = refused.local_addr().unwrap();
    drop(refused);
    let err = fetch_and_decode(&format!(
      "http://{addr}/rest/getCoverArt.view?u=alice&t=abc"
    ))
    .await
    .unwrap_err();
    assert!(!format!("{err:#}").contains("u=alice"), "{err:#}");
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
      for _ in 0..16 {
        if stream.write_all(&chunk).is_err() {
          return;
        }
      }
      let _ = stream.write_all(&[0]);
    });

    let err = fetch_and_decode(&format!("http://{addr}/art.jpg"))
      .await
      .unwrap_err();
    assert!(err.to_string().contains("exceeded"), "got: {err}");
    let _ = server.join();
  }

  #[test]
  fn cover_art_dimensions_beyond_the_limit_are_rejected() {
    // A blank one-row PNG wider than the cap compresses to a tiny body, so it
    // passes the byte cap and must be stopped by the decoder's limits.
    let mut png = Vec::new();
    image::DynamicImage::new_rgb8(MAX_COVER_ART_DIMENSION + 1, 1)
      .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
      .unwrap();
    assert!((png.len() as u64) < MAX_COVER_ART_BYTES);

    assert!(decode_bounded(&png).is_err());
    // The same pipeline still decodes an in-bounds image.
    let mut ok_png = Vec::new();
    image::DynamicImage::new_rgb8(2, 2)
      .write_to(
        &mut std::io::Cursor::new(&mut ok_png),
        image::ImageFormat::Png,
      )
      .unwrap();
    assert!(decode_bounded(&ok_png).is_ok());
  }

  #[test]
  fn request_key_is_the_url() {
    let request = CoverArtRequest::Url("https://img.example/cover.jpg".to_string());
    assert_eq!(request.key(), "https://img.example/cover.jpg");
  }

  #[test]
  fn redacted_art_key_drops_query_and_fragment_secrets() {
    let key = "https://example.test/rest/getCoverArt.view?u=alice&t=abc#detail";
    assert_eq!(
      redacted_art_key(key),
      "https://example.test/rest/getCoverArt.view"
    );
  }

  #[test]
  fn decoded_art_cache_evicts_by_decoded_bytes_not_image_count() {
    let mut cache = DecodedArtCache::new(12);
    for i in 0..4 {
      cache.put(
        &format!("key-{i}"),
        Arc::new(image::DynamicImage::new_rgb8(1, 1)),
      );
    }
    assert!(cache.get("key-0").is_some());

    cache.put("key-4", Arc::new(image::DynamicImage::new_rgb8(1, 1)));

    assert!(cache.get("key-1").is_none());
    assert!(cache.get("key-0").is_some());
    assert!(cache.get("key-2").is_some());
    assert!(cache.get("key-4").is_some());
  }

  #[test]
  fn decoded_art_cache_refuses_an_image_larger_than_the_budget() {
    let mut cache = DecodedArtCache::new(2);
    cache.put("too-large", Arc::new(image::DynamicImage::new_rgb8(1, 1)));

    assert!(cache.get("too-large").is_none());
    assert_eq!(cache.bytes, 0);
    assert!(cache.entries.is_empty());
  }

  #[tokio::test]
  async fn fetch_reuses_a_cached_decoded_image() {
    let url = format!("http://127.0.0.1:9/cached-{}", std::process::id());
    let image = Arc::new(image::DynamicImage::new_rgb8(4, 4));
    remember_decoded_art(&url, Arc::clone(&image));

    let decoded = fetch_and_decode(&url).await.unwrap();

    assert!(Arc::ptr_eq(&decoded, &image));
  }

  #[test]
  fn storing_decoded_art_makes_it_available_for_recall() {
    let key = "https://example.test/store-recall.jpg";
    let image = Arc::new(image::DynamicImage::new_rgb8(3, 3));
    assert!(cached_decoded_art(key).is_none());

    let mut store = CoverArtStore::default();
    store.store_decoded(key.to_string(), Arc::clone(&image));

    let recalled = cached_decoded_art(key).expect("stored art should be cached");
    assert!(Arc::ptr_eq(&recalled, &image));
  }

  #[test]
  fn store_mark_loaded_sets_the_status() {
    let mut store = CoverArtStore::default();
    store.mark_loaded();
    assert_eq!(store.status, CoverArtStatus::Loaded);
  }

  #[test]
  fn store_replaces_and_clear_drops_the_held_art() {
    let mut store = CoverArtStore::default();
    assert!(!store.available());
    assert_eq!(store.key(), None);
    assert_eq!(store.cached_bytes(), 0);
    assert!(store.cache_active());

    store.store_decoded("a".to_string(), image::DynamicImage::new_rgb8(1, 1));
    assert!(store.available());
    assert_eq!(store.key(), Some("a"));
    assert_eq!(store.cached_bytes(), 3);

    store.store_decoded("b".to_string(), image::DynamicImage::new_rgb8(2, 2));
    assert_eq!(store.key(), Some("b"));
    assert_eq!(store.cached_bytes(), 12);

    store.clear();
    assert!(!store.available());
    assert_eq!(store.key(), None);
    assert_eq!(store.cached_bytes(), 0);
  }

  #[test]
  fn the_default_decode_budget_is_sixty_four_mebibytes() {
    assert_eq!(DECODED_ART_CACHE_BUDGET_BYTES, 64 * 1024 * 1024);
    assert_eq!(
      CoverArtStore::default().cap_bytes,
      DECODED_ART_CACHE_BUDGET_BYTES
    );
  }

  #[test]
  fn an_image_larger_than_the_budget_leaves_the_cache_inactive() {
    let mut store = CoverArtStore::with_cap_bytes(8);
    store.store_decoded("big".into(), image::DynamicImage::new_rgba8(3, 1));

    assert!(!store.cache_active());
    assert!(!store.available());
    assert_eq!(store.cached_bytes(), 0);
    assert_eq!(store.key(), None);
  }

  #[test]
  fn adding_an_image_over_budget_evicts_the_least_recent_entry_by_bytes() {
    let mut store = CoverArtStore::with_cap_bytes(24);
    store.store_decoded("old".into(), image::DynamicImage::new_rgba8(2, 2));
    assert_eq!(store.cached_bytes(), 16);

    store.store_decoded("new".into(), image::DynamicImage::new_rgba8(2, 2));

    assert!(store.cache_active());
    assert_eq!(store.key(), Some("new"));
    assert_eq!(store.cached_bytes(), 16);
  }

  #[test]
  fn average_load_duration_uses_a_bounded_rolling_window() {
    let mut store = CoverArtStore::default();
    assert_eq!(store.average_load_duration(), None);

    for duration in [100, 200, 300] {
      store
        .load_samples
        .push_back(Duration::from_millis(duration));
    }
    assert_eq!(
      store.average_load_duration(),
      Some(Duration::from_millis(200))
    );

    for _ in 0..LOAD_SAMPLE_WINDOW {
      store.load_samples.push_back(Duration::from_millis(50));
    }
    assert_eq!(store.load_samples.len(), LOAD_SAMPLE_WINDOW + 3);
    store.note_fetch_started();
    store.note_fetch_finished(true);
    assert_eq!(store.load_samples.len(), LOAD_SAMPLE_WINDOW);
    assert!(store.average_load_duration() <= Some(Duration::from_millis(200)));
  }

  #[test]
  fn a_transition_keeps_the_first_start_when_the_key_repeats() {
    let mut store = CoverArtStore::default();
    let started = Instant::now() - Duration::from_millis(50);

    store.begin_transition_at("new", started);
    store.begin_transition_at("new", Instant::now());

    assert_eq!(store.transition(), Some(("new", started)));
  }
}
