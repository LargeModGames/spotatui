//! Frontend-agnostic cover art: what to fetch ([`CoverArtRequest`]), the
//! fetch/decode itself ([`fetch_and_decode`]), and the decoded image plus its
//! load status ([`CoverArtStore`]) that frontends render from. Turning the
//! decoded image into a terminal graphics protocol is the TUI's job
//! (`tui::cover_art`); nothing in here may depend on ratatui.

use anyhow::anyhow;
use log::{debug, info};

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
/// The reqwest client is built with explicit timeouts so a hung CDN cannot
/// stall the fetch forever even off-lock (`reqwest::get` uses a default client
/// with none).
pub async fn fetch_and_decode(url: &str) -> anyhow::Result<image::DynamicImage> {
  info!("getting new cover art image...");

  let client = reqwest::Client::builder()
    .connect_timeout(std::time::Duration::from_secs(10))
    .timeout(std::time::Duration::from_secs(30))
    .build()
    .map_err(|e| anyhow!(e))?;

  let res = client
    .get(url)
    .send()
    .await
    .and_then(|r| r.error_for_status());

  let file = match res {
    Ok(res) => {
      // Allocate Vec "file" with capacity if content_length is provided
      let mut file = match res.content_length() {
        Some(s) => Vec::with_capacity(s as usize),
        None => Vec::new(),
      };

      let bytes = res.bytes().await?;
      file.extend_from_slice(&bytes);

      debug!("finished reading response: {} bytes", file.len());
      file
    }
    Err(e) => return Err(anyhow!(e)),
  };

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
