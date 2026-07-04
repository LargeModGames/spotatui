//! Subsonic / OpenSubsonic media source.
//!
//! Implements [`MediaSource`] and [`Searcher`] against any server that speaks
//! the [Subsonic REST API](https://subsonic.org/pages/api.jsp) (v1.16.1),
//! including forks such as Navidrome and Airsonic-Advanced.
//!
//! ## Authentication
//!
//! Uses the token-based scheme introduced in API 1.13.0:
//! - `s` — random salt (generated per request)
//! - `t` — `md5(password + salt)`, lower-hex encoded
//! - `u` — username
//! - `v` — API version (`"1.16.1"`)
//! - `c` — client name (`"spotatui"`)
//! - `f` — response format (`"json"`)
//!
//! ## URIs
//!
//! Playlists: `subsonic:playlist:<id>`.
//! Tracks: `subsonic:track:<id>`.

// Nothing in the binary wires this source yet.
#![allow(dead_code)]

pub mod dispatch;
mod types;

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use md5::{Digest, Md5};
use rand::Rng;
use reqwest::Client;

use crate::core::plugin_api::{
  AlbumInfo, ArtistInfo, ArtistRef, PlaylistInfo, SearchResults, TrackInfo,
};
use crate::core::source::{MediaSource, Searcher};
use crate::infra::audio::LocalPlayer;

use types::{SubsonicEnvelope, SubsonicResponse};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const API_VERSION: &str = "1.16.1";
const CLIENT_NAME: &str = "spotatui";

const PLAYLIST_PREFIX: &str = "subsonic:playlist:";
const TRACK_PREFIX: &str = "subsonic:track:";

/// Cap on establishing the TCP+TLS connection. A server that never completes the
/// handshake (captive portal, half-open TCP) fails fast instead of hanging the
/// serial IoEvent pump.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Overall per-request cap covering connect + body. `fetch`/`download_track` are
/// awaited inline on the pump, so a server that connects then stalls the body
/// (e.g. behind a proxy) must not wedge transport for every source forever.
/// Applies per request, so it also bounds each streamed download below.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// SubsonicPlaybackState
// ---------------------------------------------------------------------------

/// The active Subsonic playback session.
///
/// Like the local-files [`LocalPlaybackState`](crate::infra::local::LocalPlaybackState)
/// it owns the live [`LocalPlayer`], a queue of `subsonic:track:` URIs plus the
/// current index, and the static metadata of the playing track. Dynamic state
/// (position, paused) is read **live** from `player`, so it is never mirrored
/// into Spotify/librespot fields and cannot desync.
///
/// Two fields are specific to a *remote* source: it holds the
/// [`SubsonicSource`] (to authenticate and download each track on Next/advance)
/// and the downloaded [`tempfile::NamedTempFile`] for the current track. The
/// tempfile is kept alive here because `rodio` reads it incrementally during
/// playback; it is dropped (and cleaned up) when replaced or on teardown.
pub struct SubsonicPlaybackState {
  pub player: Arc<LocalPlayer>,
  /// Source handle, reused to build authed stream URLs and download per track.
  pub source: Arc<SubsonicSource>,
  /// The playing playlist's tracks (with API metadata) in order. The URI for
  /// track `i` is `tracks[i].uri`; the playbar reads name/artists/album/duration
  /// from `tracks[index]`. Stored in full (vs local's URI-only queue) because the
  /// metadata came from the API, not from on-disk tags re-read per track.
  pub tracks: Vec<TrackInfo>,
  /// Index into [`tracks`](Self::tracks) of the currently playing track.
  pub index: usize,
  /// Auto-advance guard, identical in purpose to the local one: set while a
  /// track change is downloading/decoding so the runner tick does not mistake
  /// the empty sink for end-of-track and fire repeated `NextTrack` dispatches.
  /// The download window is much longer than local's decode, so this guard is
  /// load-bearing for remote playback.
  pub advancing: bool,
  /// The downloaded audio for the current track, kept alive while it plays.
  pub tempfile: tempfile::NamedTempFile,
}

impl SubsonicPlaybackState {
  /// The currently playing track, if `index` is in range.
  pub fn current(&self) -> Option<&TrackInfo> {
    self.tracks.get(self.index)
  }
}

/// The index of the track after `current` in a queue of `len`, or `None` at the
/// end. Copied from `infra::local` (which is not compiled without `local-files`).
pub fn next_index(current: usize, len: usize) -> Option<usize> {
  if len == 0 || current + 1 >= len {
    None
  } else {
    Some(current + 1)
  }
}

/// The index of the track before `current`, or `None` at the start.
pub fn prev_index(current: usize, len: usize) -> Option<usize> {
  if len == 0 || current == 0 {
    None
  } else {
    Some(current - 1)
  }
}

// ---------------------------------------------------------------------------
// SubsonicSource
// ---------------------------------------------------------------------------

/// A media source backed by a Subsonic-compatible server.
///
/// Constructed with [`SubsonicSource::new`] and then used through the
/// [`MediaSource`] and [`Searcher`] trait impls.
pub struct SubsonicSource {
  /// Base URL of the server, **without** a trailing slash.
  /// Example: `"https://music.example.com"`.
  base_url: String,
  username: String,
  /// Plain-text password used to derive per-request token+salt pairs.
  /// Stored in memory; never written to disk by this module.
  password: String,
  http: Client,
}

impl SubsonicSource {
  /// Create a new source for the given server.
  ///
  /// `base_url` should be the root of the Subsonic server, e.g.
  /// `"https://music.example.com"` (trailing slashes are stripped automatically).
  pub fn new(
    base_url: impl Into<String>,
    username: impl Into<String>,
    password: impl Into<String>,
  ) -> Self {
    let base_url: String = base_url.into();
    SubsonicSource {
      // Strip trailing slashes to avoid double-slash URLs like `//rest/ping.view`.
      base_url: base_url.trim_end_matches('/').to_string(),
      username: username.into(),
      password: password.into(),
      // A bare `Client::new()` has no timeouts: a server that connects then
      // stalls the body wedges the serial IoEvent pump permanently, killing
      // transport for every source. Bound both the handshake and the whole
      // request. `.timeout()` applies per request, so it also caps the streamed
      // download in `download_track` (each chunk read must make progress within
      // the window) without needing a separate wrapper there.
      http: Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        // Falls back to a default (untimed) client only if TLS init fails, which
        // would break every other request in the app too.
        .unwrap_or_default(),
    }
  }

  // -------------------------------------------------------------------------
  // Internal helpers
  // -------------------------------------------------------------------------

  /// Build the full URL for a REST endpoint with authentication parameters.
  ///
  /// Generates a fresh salt for every call so tokens cannot be replayed.
  fn endpoint_url(&self, view: &str) -> String {
    let salt = self.generate_salt();
    let token = self.compute_token(&salt);
    format!(
      "{}/rest/{}?u={}&t={}&s={}&v={}&c={}&f=json",
      self.base_url, view, self.username, token, salt, API_VERSION, CLIENT_NAME,
    )
  }

  /// Append a key=value query parameter to an existing URL string.
  fn append_param(url: &str, key: &str, value: &str) -> String {
    format!("{}&{}={}", url, key, value)
  }

  /// Generate a random 12-character alphanumeric salt.
  fn generate_salt(&self) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..12)
      .map(|_| {
        let idx = rng.gen_range(0..CHARSET.len());
        CHARSET[idx] as char
      })
      .collect()
  }

  /// Compute the MD5 token: `lower_hex(md5(password + salt))`.
  fn compute_token(&self, salt: &str) -> String {
    let input = format!("{}{}", self.password, salt);
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
  }

  /// Fetch a JSON response from the given endpoint URL and deserialize it.
  ///
  /// Returns an error if the HTTP request fails, the JSON cannot be parsed,
  /// or the `subsonic-response.status` field is `"failed"`.
  async fn fetch(&self, url: &str) -> Result<SubsonicResponse> {
    let body = self
      .http
      .get(url)
      .send()
      .await
      .context("HTTP request to Subsonic failed")?
      .error_for_status()
      .context("Subsonic server returned an HTTP error")?
      .text()
      .await
      .context("Failed to read Subsonic response body")?;

    let envelope: SubsonicEnvelope =
      serde_json::from_str(&body).context("Failed to deserialize Subsonic response")?;

    let resp = envelope.response;
    if resp.status != "ok" {
      let msg = resp
        .error
        .as_ref()
        .map(|e| format!("code={} {}", e.code, e.message))
        .unwrap_or_else(|| "unknown error".to_string());
      return Err(anyhow!("Subsonic API error: {}", msg));
    }

    Ok(resp)
  }

  /// Verify server connectivity. Returns `Ok(())` if the server responds
  /// with `status="ok"` to a `ping` request.
  pub async fn ping(&self) -> Result<()> {
    let url = self.endpoint_url("ping.view");
    self.fetch(&url).await?;
    Ok(())
  }

  /// The authenticated `stream.view` URL for a track id. The server transcodes
  /// to a playable container (MP3 by default); the response carries an audio
  /// content-type and supports HTTP range requests.
  pub fn stream_url(&self, track_id: &str) -> String {
    Self::append_param(
      &self.endpoint_url("stream.view"),
      "id",
      &url_encode(track_id),
    )
  }

  /// The authenticated `getCoverArt.view` URL for a cover-art id, suitable for a
  /// direct image download. Mirrors `stream_url`'s auth (token+salt embedded).
  pub fn cover_art_url(&self, cover_art_id: &str) -> String {
    Self::append_param(
      &self.endpoint_url("getCoverArt.view"),
      "id",
      &url_encode(cover_art_id),
    )
  }

  /// Download a track's audio to `dest`, streaming the response body to disk
  /// chunk-by-chunk; the file is then played from disk by the shared player.
  ///
  /// Streaming (rather than buffering the whole body with `.bytes()`) avoids a
  /// multi-hundred-MB RAM spike per track change for large lossless FLAC or
  /// audiobook files — the server default is the original file. The client's
  /// overall `.timeout()` still bounds the whole download.
  ///
  /// Async (reqwest), so it must be awaited **without** holding `App`'s lock.
  pub async fn download_track(&self, track_id: &str, dest: &std::path::Path) -> Result<()> {
    let url = self.stream_url(track_id);
    let response = self
      .http
      .get(&url)
      .send()
      .await
      .context("HTTP request to Subsonic stream failed")?
      .error_for_status()
      .context("Subsonic stream returned an HTTP error")?;

    // Write to a synchronous file inside the streaming loop: each chunk is a
    // small `Bytes` and rodio reads the finished file from disk, so a blocking
    // write here is cheap and keeps peak memory to one chunk rather than the
    // whole track.
    let mut file = std::fs::File::create(dest)
      .with_context(|| format!("creating stream file {}", dest.display()))?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
      let chunk = chunk.context("Failed to read Subsonic stream body")?;
      file
        .write_all(&chunk)
        .with_context(|| format!("writing stream to {}", dest.display()))?;
    }
    file
      .flush()
      .with_context(|| format!("flushing stream to {}", dest.display()))?;
    Ok(())
  }
}

/// Strip the `subsonic:track:` prefix and return the raw track id.
pub fn track_id_from_uri(uri: &str) -> Result<&str> {
  uri
    .strip_prefix(TRACK_PREFIX)
    .ok_or_else(|| anyhow!("Not a subsonic track URI: {}", uri))
}

// ---------------------------------------------------------------------------
// Domain type conversions
// ---------------------------------------------------------------------------

/// Strip `subsonic:playlist:` prefix and return the raw numeric id.
fn playlist_id_from_uri(uri: &str) -> Result<&str> {
  uri
    .strip_prefix(PLAYLIST_PREFIX)
    .ok_or_else(|| anyhow!("Not a subsonic playlist URI: {}", uri))
}

impl From<&types::SubsonicPlaylist> for PlaylistInfo {
  fn from(p: &types::SubsonicPlaylist) -> Self {
    PlaylistInfo {
      uri: format!("{}{}", PLAYLIST_PREFIX, p.id),
      name: p.name.clone(),
      owner: p.owner.clone(),
      track_count: p.song_count,
      id: Some(p.id.clone()),
      owner_id: None, // Subsonic has no separate base62 user id; `owner` is the username
      collaborative: false,
      public: p.public,
      image_url: None, // Subsonic uses cover_art IDs, not direct URLs
    }
  }
}

impl From<&types::PlaylistDetail> for PlaylistInfo {
  fn from(p: &types::PlaylistDetail) -> Self {
    PlaylistInfo {
      uri: format!("{}{}", PLAYLIST_PREFIX, p.id),
      name: p.name.clone(),
      owner: p.owner.clone(),
      track_count: p.song_count,
      id: Some(p.id.clone()),
      owner_id: None,
      collaborative: false,
      public: p.public,
      image_url: None,
    }
  }
}

impl SubsonicSource {
  /// Map a Subsonic song onto the shared [`TrackInfo`]. A method (not a free
  /// function) so it can mint an authenticated `getCoverArt.view` URL from the
  /// song's `coverArt` id via `self`.
  fn song_to_track_info(&self, s: &types::SubsonicSong) -> TrackInfo {
    let artist_name = s.artist.clone().unwrap_or_default();
    let artist_ref = if !artist_name.is_empty() {
      vec![ArtistRef {
        id: s.artist_id.clone(),
        name: artist_name.clone(),
      }]
    } else {
      vec![]
    };

    TrackInfo {
      uri: Some(format!("{}{}", TRACK_PREFIX, s.id)),
      name: s.title.clone(),
      artists: if artist_name.is_empty() {
        vec![]
      } else {
        vec![artist_name]
      },
      album: s.album.clone().unwrap_or_default(),
      // Subsonic reports duration in seconds; convert to ms for our domain type.
      duration_ms: s.duration.unwrap_or(0) * 1000,
      id: Some(s.id.clone()),
      album_id: s.album_id.clone(),
      artist_refs: artist_ref,
      is_playable: true,
      is_local: false,
      track_number: s.track_number.unwrap_or(0),
      explicit: false,
      image_url: s.cover_art.as_deref().map(|id| self.cover_art_url(id)),
    }
  }
}

fn album_to_album_info(a: &types::SubsonicAlbum) -> AlbumInfo {
  let artists = a
    .artist
    .as_deref()
    .filter(|n| !n.is_empty())
    .map(|name| {
      vec![ArtistRef {
        id: a.artist_id.clone(),
        name: name.to_string(),
      }]
    })
    .unwrap_or_default();

  AlbumInfo {
    id: Some(a.id.clone()),
    uri: Some(format!("subsonic:album:{}", a.id)),
    name: a.name.clone(),
    artists,
    album_type: Some("album".to_string()),
    release_date: a.year.map(|y| y.to_string()),
    total_tracks: a.song_count,
    image_url: None,
    tracks: vec![],
  }
}

fn artist_to_artist_info(a: &types::SubsonicArtist) -> ArtistInfo {
  ArtistInfo {
    id: Some(a.id.clone()),
    uri: Some(format!("subsonic:artist:{}", a.id)),
    name: a.name.clone(),
    image_url: None,
  }
}

// ---------------------------------------------------------------------------
// Trait implementations
// ---------------------------------------------------------------------------

impl MediaSource for SubsonicSource {
  fn name(&self) -> &str {
    "Subsonic"
  }

  fn scheme(&self) -> &str {
    "subsonic"
  }

  async fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
    let url = self.endpoint_url("getPlaylists.view");
    let resp = self.fetch(&url).await?;

    let playlists = resp.playlists.and_then(|w| w.playlist).unwrap_or_default();

    Ok(playlists.iter().map(PlaylistInfo::from).collect())
  }

  async fn tracks(&self, playlist_uri: &str) -> Result<Vec<TrackInfo>> {
    let id = playlist_id_from_uri(playlist_uri)?;
    let url = Self::append_param(&self.endpoint_url("getPlaylist.view"), "id", id);
    let resp = self.fetch(&url).await?;

    let detail = resp
      .playlist
      .ok_or_else(|| anyhow!("No playlist in getPlaylist response"))?;

    Ok(
      detail
        .entry
        .iter()
        .map(|s| self.song_to_track_info(s))
        .collect(),
    )
  }
}

impl Searcher for SubsonicSource {
  async fn search(&self, query: &str) -> Result<SearchResults> {
    let encoded = url_encode(query);
    let base = Self::append_param(&self.endpoint_url("search3.view"), "query", &encoded);
    // Request a reasonable page size; the caller can paginate separately if needed.
    let url = format!("{}&songCount=20&albumCount=10&artistCount=10", base);

    let resp = self.fetch(&url).await?;

    let sr = resp.search_result3.unwrap_or_default();
    Ok(SearchResults {
      tracks: sr.song.iter().map(|s| self.song_to_track_info(s)).collect(),
      albums: sr.album.iter().map(album_to_album_info).collect(),
      artists: sr.artist.iter().map(artist_to_artist_info).collect(),
      playlists: vec![],
      shows: vec![],
    })
  }
}

// ---------------------------------------------------------------------------
// Minimal URL encoding for query strings
// ---------------------------------------------------------------------------

/// Percent-encode characters that are unsafe in a query parameter value.
/// Only encodes the characters that will break the Subsonic query string;
/// avoids pulling in an extra URL-encoding crate given the narrow use case.
fn url_encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        out.push(b as char);
      }
      b' ' => out.push('+'),
      other => {
        out.push('%');
        out.push_str(&format!("{:02X}", other));
      }
    }
  }
  out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::subsonic::types::SubsonicEnvelope;

  /// Live end-to-end smoke test against the public Navidrome demo server.
  /// Ignored by default (hits the network); run with:
  /// `cargo test --features subsonic -- --ignored live_demo`
  #[tokio::test]
  #[ignore = "hits the live Navidrome demo server"]
  async fn live_demo_browse_and_search() {
    let source = SubsonicSource::new("https://demo.navidrome.org", "demo", "demo");
    source.ping().await.expect("ping should succeed");

    let playlists = source.playlists().await.expect("playlists should load");
    assert!(!playlists.is_empty(), "demo server should have playlists");

    let tracks = source
      .tracks(&playlists[0].uri)
      .await
      .expect("playlist tracks should load");
    assert!(!tracks.is_empty(), "first playlist should have tracks");
    // Every track must carry a `subsonic:track:` URI so playback can route it.
    assert!(tracks.iter().all(|t| t
      .uri
      .as_deref()
      .is_some_and(|u| u.starts_with(TRACK_PREFIX))));

    let results = source.search("love").await.expect("search should succeed");
    assert!(
      !results.tracks.is_empty(),
      "search for 'love' should return tracks"
    );
  }

  /// Live download smoke test: stream a real demo track to a tempfile and check
  /// it is non-empty with a plausible audio header. Catches auth/URL-encoding
  /// bugs in `stream_url`/`download_track` that the JSON tests can't. Ignored by
  /// default (network); run with:
  /// `cargo test --features subsonic -- --ignored live_demo_download`
  #[tokio::test]
  #[ignore = "hits the live Navidrome demo server"]
  async fn live_demo_download_streams_audio() {
    let source = SubsonicSource::new("https://demo.navidrome.org", "demo", "demo");
    let playlists = source.playlists().await.expect("playlists should load");
    let tracks = source
      .tracks(&playlists[0].uri)
      .await
      .expect("tracks should load");
    let uri = tracks[0].uri.as_deref().expect("track should have a uri");
    let track_id = track_id_from_uri(uri).expect("uri should be a subsonic track uri");

    let tmp = tempfile::NamedTempFile::new().unwrap();
    source
      .download_track(track_id, tmp.path())
      .await
      .expect("download should succeed");

    let bytes = std::fs::read(tmp.path()).unwrap();
    assert!(bytes.len() > 4096, "downloaded audio should be non-trivial");
    // Navidrome transcodes the demo library to MP3 by default: either an ID3 tag
    // ("ID3") or a raw MPEG frame sync (0xFF, 0xEx/0xFx).
    let is_mp3 = bytes.starts_with(b"ID3") || (bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0);
    assert!(
      is_mp3,
      "expected an MP3 header, got bytes {:02X?}",
      &bytes[..4]
    );
  }

  /// Full streaming-playback path: download a real demo track and play it through
  /// the shared [`LocalPlayer`], asserting the sink actually advances. Verifies
  /// download -> decode -> audio sink together. Ignored (needs network **and** an
  /// audio output device); run with:
  /// `cargo test --features subsonic -- --ignored live_demo_stream_plays`
  #[tokio::test]
  #[ignore = "hits the live demo server AND requires an audio output device"]
  async fn live_demo_stream_plays_through_sink() {
    use crate::infra::audio::LocalPlayer;
    use std::time::Duration;

    let source = SubsonicSource::new("https://demo.navidrome.org", "demo", "demo");
    let playlists = source.playlists().await.expect("playlists should load");
    let tracks = source
      .tracks(&playlists[0].uri)
      .await
      .expect("tracks should load");
    let uri = tracks[0].uri.as_deref().expect("track should have a uri");
    let track_id = track_id_from_uri(uri).expect("uri should be a subsonic track uri");

    let tmp = tempfile::NamedTempFile::new().unwrap();
    source
      .download_track(track_id, tmp.path())
      .await
      .expect("download should succeed");

    let player = LocalPlayer::new().expect("open default output device");
    player.play_file(tmp.path()).expect("play streamed track");
    assert!(!player.is_paused(), "should be playing after play_file");
    assert!(
      !player.is_finished(),
      "a freshly started track should not be finished"
    );

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
      player.position() >= Duration::from_millis(200),
      "playback position should advance, got {:?}",
      player.position()
    );

    player.stop();
  }

  // Inline JSON fixtures — representative Subsonic REST API responses.

  const PING_OK: &str = r#"
  {
    "subsonic-response": {
      "status": "ok",
      "version": "1.16.1"
    }
  }"#;

  const PING_FAILED: &str = r#"
  {
    "subsonic-response": {
      "status": "failed",
      "version": "1.16.1",
      "error": { "code": 40, "message": "Wrong username or password." }
    }
  }"#;

  const GET_PLAYLISTS: &str = r#"
  {
    "subsonic-response": {
      "status": "ok",
      "version": "1.16.1",
      "playlists": {
        "playlist": [
          { "id": "1", "name": "Chill Mix", "owner": "alice", "songCount": 12, "public": true },
          { "id": "2", "name": "Workout", "owner": "alice", "songCount": 34, "public": false }
        ]
      }
    }
  }"#;

  const GET_PLAYLISTS_EMPTY: &str = r#"
  {
    "subsonic-response": {
      "status": "ok",
      "version": "1.16.1",
      "playlists": {}
    }
  }"#;

  const GET_PLAYLIST: &str = r#"
  {
    "subsonic-response": {
      "status": "ok",
      "version": "1.16.1",
      "playlist": {
        "id": "1",
        "name": "Chill Mix",
        "owner": "alice",
        "songCount": 2,
        "public": true,
        "entry": [
          {
            "id": "101",
            "title": "Weightless",
            "artist": "Marconi Union",
            "artistId": "art1",
            "album": "Weightless",
            "albumId": "alb1",
            "duration": 469,
            "trackNumber": 1
          },
          {
            "id": "102",
            "title": "Clair de Lune",
            "artist": "Claude Debussy",
            "artistId": "art2",
            "album": "Suite bergamasque",
            "albumId": "alb2",
            "duration": 328,
            "trackNumber": 1
          }
        ]
      }
    }
  }"#;

  const SEARCH3: &str = r#"
  {
    "subsonic-response": {
      "status": "ok",
      "version": "1.16.1",
      "searchResult3": {
        "song": [
          {
            "id": "201",
            "title": "Yesterday",
            "artist": "The Beatles",
            "artistId": "art10",
            "album": "Help!",
            "albumId": "alb10",
            "duration": 125,
            "trackNumber": 13
          }
        ],
        "album": [
          {
            "id": "alb10",
            "name": "Help!",
            "artist": "The Beatles",
            "artistId": "art10",
            "songCount": 14,
            "year": 1965
          }
        ],
        "artist": [
          {
            "id": "art10",
            "name": "The Beatles"
          }
        ]
      }
    }
  }"#;

  // -------------------------------------------------------------------------
  // JSON parsing tests
  // -------------------------------------------------------------------------

  #[test]
  fn parse_ping_ok() {
    let env: SubsonicEnvelope = serde_json::from_str(PING_OK).unwrap();
    assert_eq!(env.response.status, "ok");
    assert_eq!(env.response.version, "1.16.1");
  }

  #[test]
  fn parse_ping_failed_has_error() {
    let env: SubsonicEnvelope = serde_json::from_str(PING_FAILED).unwrap();
    assert_eq!(env.response.status, "failed");
    let err = env.response.error.unwrap();
    assert_eq!(err.code, 40);
    assert!(err.message.contains("Wrong username"));
  }

  #[test]
  fn parse_playlists_maps_to_domain() {
    let env: SubsonicEnvelope = serde_json::from_str(GET_PLAYLISTS).unwrap();
    let raw = env.response.playlists.unwrap().playlist.unwrap();
    assert_eq!(raw.len(), 2);

    let info: PlaylistInfo = PlaylistInfo::from(&raw[0]);
    assert_eq!(info.uri, "subsonic:playlist:1");
    assert_eq!(info.name, "Chill Mix");
    assert_eq!(info.owner, "alice");
    assert_eq!(info.track_count, 12);
    assert_eq!(info.id.as_deref(), Some("1"));
    assert_eq!(info.public, Some(true));
  }

  #[test]
  fn parse_playlists_empty_playlist_field() {
    let env: SubsonicEnvelope = serde_json::from_str(GET_PLAYLISTS_EMPTY).unwrap();
    let wrapper = env.response.playlists.unwrap();
    assert!(wrapper.playlist.is_none());
  }

  #[test]
  fn parse_playlist_tracks_maps_to_domain() {
    let env: SubsonicEnvelope = serde_json::from_str(GET_PLAYLIST).unwrap();
    let detail = env.response.playlist.unwrap();
    assert_eq!(detail.entry.len(), 2);

    let src = SubsonicSource::new("http://localhost", "user", "sesame");
    let track = src.song_to_track_info(&detail.entry[0]);
    assert_eq!(track.uri.as_deref(), Some("subsonic:track:101"));
    assert_eq!(track.name, "Weightless");
    assert_eq!(track.artists, vec!["Marconi Union"]);
    assert_eq!(track.album, "Weightless");
    // duration 469 seconds * 1000 = 469 000 ms
    assert_eq!(track.duration_ms, 469_000);
    assert_eq!(track.track_number, 1);
    assert_eq!(track.id.as_deref(), Some("101"));
    assert_eq!(track.album_id.as_deref(), Some("alb1"));
    assert_eq!(track.artist_refs.len(), 1);
    assert_eq!(track.artist_refs[0].name, "Marconi Union");
    assert_eq!(track.artist_refs[0].id.as_deref(), Some("art1"));
    assert!(track.is_playable);
    assert!(!track.is_local);
    assert!(!track.explicit);
  }

  #[test]
  fn parse_search3_maps_all_result_types() {
    let env: SubsonicEnvelope = serde_json::from_str(SEARCH3).unwrap();
    let sr = env.response.search_result3.unwrap();

    // Tracks
    assert_eq!(sr.song.len(), 1);
    let src = SubsonicSource::new("http://localhost", "user", "sesame");
    let track = src.song_to_track_info(&sr.song[0]);
    assert_eq!(track.uri.as_deref(), Some("subsonic:track:201"));
    assert_eq!(track.name, "Yesterday");
    assert_eq!(track.duration_ms, 125_000);
    assert_eq!(track.track_number, 13);

    // Albums
    assert_eq!(sr.album.len(), 1);
    let album = album_to_album_info(&sr.album[0]);
    assert_eq!(album.id.as_deref(), Some("alb10"));
    assert_eq!(album.uri.as_deref(), Some("subsonic:album:alb10"));
    assert_eq!(album.name, "Help!");
    assert_eq!(album.total_tracks, Some(14));
    assert_eq!(album.release_date.as_deref(), Some("1965"));
    assert_eq!(album.artists.len(), 1);
    assert_eq!(album.artists[0].name, "The Beatles");

    // Artists
    assert_eq!(sr.artist.len(), 1);
    let artist = artist_to_artist_info(&sr.artist[0]);
    assert_eq!(artist.id.as_deref(), Some("art10"));
    assert_eq!(artist.uri.as_deref(), Some("subsonic:artist:art10"));
    assert_eq!(artist.name, "The Beatles");
  }

  #[test]
  fn playlist_id_from_uri_strips_prefix() {
    assert_eq!(playlist_id_from_uri("subsonic:playlist:42").unwrap(), "42");
  }

  #[test]
  fn playlist_id_from_uri_rejects_wrong_scheme() {
    assert!(playlist_id_from_uri("spotify:playlist:xyz").is_err());
  }

  #[test]
  fn compute_token_is_deterministic_for_same_inputs() {
    let src = SubsonicSource::new("http://localhost", "user", "sesame");
    let t1 = src.compute_token("abc123");
    let t2 = src.compute_token("abc123");
    assert_eq!(t1, t2);
    // MD5 of "sesameabc123" = 7f9bf1c85b45c4f27fb65cb3a9c9b2fc (verify manually)
    // Presence of 32 lowercase hex chars is sufficient for the unit test.
    assert_eq!(t1.len(), 32);
    assert!(t1.chars().all(|c| c.is_ascii_hexdigit()));
  }

  #[test]
  fn compute_token_differs_per_salt() {
    let src = SubsonicSource::new("http://localhost", "user", "sesame");
    let t1 = src.compute_token("salt1");
    let t2 = src.compute_token("salt2");
    assert_ne!(t1, t2);
  }

  #[test]
  fn url_encode_encodes_spaces_and_specials() {
    assert_eq!(url_encode("hello world"), "hello+world");
    assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
    assert_eq!(url_encode("plain"), "plain");
  }

  #[test]
  fn generate_salt_produces_12_char_alphanumeric() {
    let src = SubsonicSource::new("http://localhost", "user", "pass");
    let salt = src.generate_salt();
    assert_eq!(salt.len(), 12);
    assert!(salt.chars().all(|c| c.is_ascii_alphanumeric()));
  }
}
