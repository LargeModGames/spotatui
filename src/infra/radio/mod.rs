//! Internet-radio media source.
//!
//! Plays direct HTTP(S) icecast/shoutcast-style streams (SomaFM, most stations
//! in the radio-browser.info directory). Stations come from two places:
//!
//! - the user's config list (`behavior.radio_stations`, name + URL pairs),
//!   shown in the sidebar when the Radio source is active;
//! - in-app search of the community [radio-browser.info](https://api.radio-browser.info)
//!   directory (30k+ stations), via [`RadioSource`].
//!
//! A station is a **leaf, not a container**: it has no playlists or tracks and
//! plays forever, so [`MediaSource`]'s browse model does not apply — the source
//! implements only [`Searcher`]. Playback state ([`RadioPlaybackState`]) has no
//! queue/index/advance; Next/Prev are meaningless on a live stream.
//!
//! ## URIs
//!
//! `radio:<stream-url>` — the suffix is the verbatim stream URL (which itself
//! contains `:` and `/`; never re-split on `:`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rand::seq::SliceRandom;

use crate::core::plugin_api::{SearchResults, TrackInfo};
use crate::core::source::Searcher;
use crate::infra::audio::LocalPlayer;

pub mod dispatch;
mod stream;
mod types;

pub use stream::open_radio_stream;

use types::RbStation;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const RADIO_PREFIX: &str = "radio:";

/// Known radio-browser.info API mirrors, tried in shuffled order per call.
/// The project recommends DNS discovery of `all.api.radio-browser.info`, but a
/// vetted static mirror list avoids resolving+reverse-mapping IPs to TLS
/// hostnames; these are the long-lived first-party mirrors.
const MIRRORS: [&str; 3] = [
  "https://de1.api.radio-browser.info",
  "https://de2.api.radio-browser.info",
  "https://fi1.api.radio-browser.info",
];

/// Descriptive User-Agent, required by the radio-browser.info usage policy.
const USER_AGENT: &str = concat!("spotatui/", env!("CARGO_PKG_VERSION"));

/// Result page size for directory searches — matches the subsonic search's
/// songCount and roughly one screen of the songs block.
const SEARCH_LIMIT: usize = 30;

/// Per-mirror wall-clock cap. The mirror walk is on the serial IoEvent pump, so
/// a mirror that connects then stalls the body must not hold up the whole walk
/// (or every source's transport). Bounds each `get_from_any_mirror` attempt so a
/// dead mirror really does "only cost one timeout" and the walk advances to the
/// next mirror. Blankets both the connect and the body phases via a hard wrap.
const MIRROR_TIMEOUT: Duration = Duration::from_secs(6);

// ---------------------------------------------------------------------------
// RadioPlaybackState
// ---------------------------------------------------------------------------

/// The active internet-radio playback session.
///
/// Mirrors the local/subsonic decoupling: it owns the live [`LocalPlayer`] and
/// never writes Spotify/librespot fields — the playbar reads pause state and
/// elapsed time live from `player`. Unlike those sources there is **no queue**:
/// a station is infinite, so there is no index, no advance guard and no
/// end-of-track. `player.is_finished()` flipping true means the stream died.
pub struct RadioPlaybackState {
  pub player: Arc<LocalPlayer>,
  /// The playing station's row (name, `radio:` URI, tags/summary) as
  /// snapshotted from the sidebar or search results at start.
  pub station: TrackInfo,
  /// Live ICY `StreamTitle` ("Artist - Title"), shared with the stream reader
  /// which updates it as metadata blocks arrive. `None` until the first block
  /// (or forever, for streams without ICY metadata).
  pub now_playing: Arc<Mutex<Option<String>>>,
}

impl RadioPlaybackState {
  /// The current ICY now-playing title, if one has arrived.
  pub fn now_playing_title(&self) -> Option<String> {
    self.now_playing.lock().ok().and_then(|np| np.clone())
  }
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// Whether a URI is owned by the radio source.
pub fn is_radio_uri(uri: &str) -> bool {
  uri.starts_with(RADIO_PREFIX)
}

/// Strip the `radio:` prefix and return the verbatim stream URL.
pub fn stream_url_from_uri(uri: &str) -> Result<&str> {
  uri
    .strip_prefix(RADIO_PREFIX)
    .ok_or_else(|| anyhow!("Not a radio URI: {}", uri))
}

/// Build the `radio:` URI for a stream URL.
pub fn uri_for_stream_url(url: &str) -> String {
  format!("{RADIO_PREFIX}{url}")
}

// ---------------------------------------------------------------------------
// RadioSource — radio-browser.info directory client
// ---------------------------------------------------------------------------

/// Client for the radio-browser.info community directory.
///
/// Stateless besides the HTTP client: each call walks the [`MIRRORS`] in a
/// fresh shuffled order and returns the first success, so a dead mirror only
/// costs one timeout and load spreads across the pool (per the project's
/// usage guidelines).
pub struct RadioSource {
  http: reqwest::Client,
}

impl RadioSource {
  pub fn new() -> Self {
    RadioSource {
      http: reqwest::Client::builder()
        .user_agent(USER_AGENT)
        // Blanket per-request timeout so a mirror that connects then stalls the
        // body can never hang the pump. The per-mirror `tokio::time::timeout` in
        // `get_from_any_mirror` is the belt to this suspenders — it also bounds
        // any phase reqwest's own timeout might not (DNS, connect races).
        .timeout(MIRROR_TIMEOUT)
        .build()
        // Falls back to a default client only if TLS init fails, which would
        // break every other request in the app too.
        .unwrap_or_default(),
    }
  }

  /// Search the directory by station name, most-voted first, skipping stations
  /// whose last connectivity check failed.
  pub async fn search_stations(&self, query: &str) -> Result<Vec<RbStation>> {
    let path = format!(
      "/json/stations/search?name={}&limit={}&hidebroken=true&order=votes&reverse=true",
      url_encode(query),
      SEARCH_LIMIT,
    );
    let body = self.get_from_any_mirror(&path).await?;
    let stations: Vec<RbStation> =
      serde_json::from_str(&body).context("deserializing radio-browser search response")?;
    Ok(
      stations
        .into_iter()
        .filter(|s| s.lastcheckok == 1)
        .collect(),
    )
  }

  /// Count a station click, as the radio-browser usage policy asks clients to
  /// do when a user starts a station. Best-effort: failures are ignored (the
  /// ping must never block or fail playback).
  pub async fn click(&self, stationuuid: &str) {
    let path = format!("/json/url/{}", url_encode(stationuuid));
    let _ = self.get_from_any_mirror(&path).await;
  }

  /// GET `path` from the first responding mirror, in shuffled order.
  async fn get_from_any_mirror(&self, path: &str) -> Result<String> {
    let mut mirrors = MIRRORS;
    mirrors.shuffle(&mut rand::thread_rng());

    let mut last_err = anyhow!("no radio-browser mirrors configured");
    for base in mirrors {
      let url = format!("{base}{path}");
      // Hard-bound each mirror attempt: even if reqwest's own timeout somehow
      // does not fire (e.g. a stall in a phase it does not cover), this ensures
      // one dead mirror costs at most `MIRROR_TIMEOUT` before the walk advances.
      match tokio::time::timeout(MIRROR_TIMEOUT, self.get(&url)).await {
        Ok(Ok(body)) => return Ok(body),
        Ok(Err(e)) => last_err = e,
        Err(_) => last_err = anyhow!("mirror {base} timed out after {MIRROR_TIMEOUT:?}"),
      }
    }
    Err(last_err.context("all radio-browser mirrors failed"))
  }

  async fn get(&self, url: &str) -> Result<String> {
    self
      .http
      .get(url)
      .send()
      .await
      .context("HTTP request to radio-browser failed")?
      .error_for_status()
      .context("radio-browser returned an HTTP error")?
      .text()
      .await
      .context("failed to read radio-browser response body")
  }
}

impl Default for RadioSource {
  fn default() -> Self {
    Self::new()
  }
}

impl Searcher for RadioSource {
  async fn search(&self, query: &str) -> Result<SearchResults> {
    let stations = self.search_stations(query).await?;
    Ok(SearchResults {
      tracks: stations.iter().map(station_to_track_info).collect(),
      albums: vec![],
      artists: vec![],
      playlists: vec![],
      shows: vec![],
    })
  }
}

// ---------------------------------------------------------------------------
// Domain type conversions
// ---------------------------------------------------------------------------

/// Map a directory station onto the shared [`TrackInfo`] row the track table,
/// search results and playbar already render.
///
/// - `uri` = `radio:<stream-url>` (prefers `url_resolved`, which the directory
///   has already followed through `.m3u`/`.pls` playlist pointers).
/// - `artists` (the subtitle column) = genre tags.
/// - `album` = a "US • MP3 128 kbps" style summary.
/// - `duration_ms` = 0 — the LIVE sentinel; a radio stream has no duration and
///   the playbar renders a LIVE badge instead of a seek bar.
/// - `id` = the directory `stationuuid`, so play can send the click ping.
fn station_to_track_info(s: &RbStation) -> TrackInfo {
  let stream_url = if s.url_resolved.is_empty() {
    &s.url
  } else {
    &s.url_resolved
  };

  let tags: Vec<String> = s
    .tags
    .split(',')
    .map(str::trim)
    .filter(|t| !t.is_empty())
    .take(3)
    .map(str::to_owned)
    .collect();

  TrackInfo {
    uri: Some(uri_for_stream_url(stream_url)),
    name: s.name.trim().to_string(),
    artists: tags,
    album: station_summary(&s.countrycode, &s.codec, s.bitrate),
    duration_ms: 0,
    id: if s.stationuuid.is_empty() {
      None
    } else {
      Some(s.stationuuid.clone())
    },
    album_id: None,
    artist_refs: vec![],
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    image_url: None,
  }
}

/// Map a config-file station (name + URL, nothing else known) onto a row.
pub fn config_station_to_track_info(name: &str, url: &str) -> TrackInfo {
  TrackInfo {
    uri: Some(uri_for_stream_url(url.trim())),
    name: name.trim().to_string(),
    artists: vec![],
    album: String::new(),
    duration_ms: 0,
    id: None, // no stationuuid — config stations don't get click pings
    album_id: None,
    artist_refs: vec![],
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    image_url: None,
  }
}

/// `"US • MP3 128 kbps"`-style one-liner from whatever fields are present.
fn station_summary(countrycode: &str, codec: &str, bitrate: u32) -> String {
  let mut parts: Vec<String> = Vec::new();
  if !countrycode.is_empty() {
    parts.push(countrycode.to_string());
  }
  if !codec.is_empty() {
    parts.push(codec.to_uppercase());
  }
  if bitrate > 0 {
    parts.push(format!("{bitrate} kbps"));
  }
  parts.join(" \u{2022} ")
}

// ---------------------------------------------------------------------------
// Minimal URL encoding for query strings (mirrors infra::subsonic::url_encode,
// which is private to that feature-gated module)
// ---------------------------------------------------------------------------

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

  /// Live directory search against the real mirror pool. Ignored by default
  /// (network); run with:
  /// `cargo test --features internet-radio -- --ignored live_directory_search`
  #[tokio::test]
  #[ignore = "hits the live radio-browser.info directory"]
  async fn live_directory_search_returns_stations() {
    let source = RadioSource::new();
    let stations = source
      .search_stations("soma")
      .await
      .expect("directory search should succeed");
    assert!(!stations.is_empty(), "'soma' should match SomaFM stations");
    // Every returned station must map to a playable radio: row.
    for s in &stations {
      let row = station_to_track_info(s);
      let uri = row.uri.expect("station row must carry a uri");
      assert!(is_radio_uri(&uri));
      let url = stream_url_from_uri(&uri).unwrap();
      assert!(
        url.starts_with("http://") || url.starts_with("https://"),
        "stream url should be http(s), got {url}"
      );
    }
  }

  // Representative directory response (two stations, one failing its check).
  const SEARCH_JSON: &str = r#"
  [
    {
      "stationuuid": "960e57c5-0601-11e8-ae97-52543be04c81",
      "name": "SomaFM Groove Salad",
      "url": "http://ice1.somafm.com/groovesalad-128-mp3",
      "url_resolved": "https://ice1.somafm.com/groovesalad-128-mp3",
      "tags": "ambient,chillout,downtempo",
      "countrycode": "US",
      "codec": "MP3",
      "bitrate": 128,
      "lastcheckok": 1
    },
    {
      "stationuuid": "dead-beef",
      "name": "Broken FM",
      "url": "http://broken.example.com/stream",
      "url_resolved": "",
      "tags": "",
      "countrycode": "",
      "codec": "",
      "bitrate": 0,
      "lastcheckok": 0
    }
  ]"#;

  #[test]
  fn parse_search_response_and_map_to_track_info() {
    let stations: Vec<RbStation> = serde_json::from_str(SEARCH_JSON).unwrap();
    assert_eq!(stations.len(), 2);

    let row = station_to_track_info(&stations[0]);
    assert_eq!(
      row.uri.as_deref(),
      Some("radio:https://ice1.somafm.com/groovesalad-128-mp3"),
      "must prefer url_resolved"
    );
    assert_eq!(row.name, "SomaFM Groove Salad");
    assert_eq!(row.artists, vec!["ambient", "chillout", "downtempo"]);
    assert_eq!(row.album, "US \u{2022} MP3 \u{2022} 128 kbps");
    assert_eq!(row.duration_ms, 0, "0 is the LIVE sentinel");
    assert_eq!(
      row.id.as_deref(),
      Some("960e57c5-0601-11e8-ae97-52543be04c81")
    );
    assert!(row.is_playable);
  }

  #[test]
  fn station_falls_back_to_raw_url_when_unresolved() {
    let stations: Vec<RbStation> = serde_json::from_str(SEARCH_JSON).unwrap();
    let row = station_to_track_info(&stations[1]);
    assert_eq!(
      row.uri.as_deref(),
      Some("radio:http://broken.example.com/stream")
    );
    assert!(row.artists.is_empty(), "no tags -> no subtitle entries");
    assert!(row.album.is_empty(), "nothing known -> empty summary");
  }

  #[test]
  fn parse_tolerates_sparse_records() {
    // Records with most fields missing must still deserialize (serde defaults).
    let sparse: Vec<RbStation> =
      serde_json::from_str(r#"[{"name": "Bare", "url": "http://x.example/s"}]"#).unwrap();
    assert_eq!(sparse[0].name, "Bare");
    assert_eq!(sparse[0].lastcheckok, 0);
  }

  #[test]
  fn radio_uri_round_trip_preserves_url_with_colons_and_query() {
    let url = "https://example.com:8000/stream.mp3?token=a:b&x=1";
    let uri = uri_for_stream_url(url);
    assert_eq!(uri, format!("radio:{url}"));
    assert!(is_radio_uri(&uri));
    assert_eq!(stream_url_from_uri(&uri).unwrap(), url);
  }

  #[test]
  fn stream_url_from_uri_rejects_other_schemes() {
    assert!(stream_url_from_uri("spotify:track:x").is_err());
    assert!(stream_url_from_uri("subsonic:track:1").is_err());
    assert!(!is_radio_uri("file:///music/a.flac"));
  }

  #[test]
  fn config_station_maps_to_playable_row() {
    let row = config_station_to_track_info(" SomaFM ", " https://ice1.somafm.com/g-128 ");
    assert_eq!(row.name, "SomaFM");
    assert_eq!(
      row.uri.as_deref(),
      Some("radio:https://ice1.somafm.com/g-128")
    );
    assert_eq!(row.duration_ms, 0);
    assert!(row.id.is_none());
  }

  #[test]
  fn summary_skips_missing_fields() {
    assert_eq!(
      station_summary("US", "mp3", 128),
      "US \u{2022} MP3 \u{2022} 128 kbps"
    );
    assert_eq!(station_summary("", "aac", 0), "AAC");
    assert_eq!(station_summary("", "", 0), "");
  }
}
