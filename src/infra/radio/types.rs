//! Serde types for the radio-browser.info JSON API.
//!
//! Only the station fields spotatui consumes are modeled; the API returns many
//! more. Every field is defaulted because the community directory contains
//! sparse records (missing codec, zero bitrate, empty tags) and one bad record
//! must not fail the whole response.

use serde::Deserialize;

/// One station record from `/json/stations/search` (and the other station
/// listing endpoints, which share the same shape).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RbStation {
  /// Stable station identifier, used for the click-count ping.
  pub stationuuid: String,
  pub name: String,
  /// The stream URL as submitted. May be a playlist (`.m3u`/`.pls`) pointer.
  pub url: String,
  /// The URL after the directory resolved playlists/redirects — prefer this.
  pub url_resolved: String,
  /// Comma-separated genre tags, e.g. `"ambient,chillout"`.
  pub tags: String,
  /// ISO 3166-1 alpha-2 country code, e.g. `"US"`.
  pub countrycode: String,
  /// Audio codec as reported by the directory, e.g. `"MP3"`, `"AAC"`.
  pub codec: String,
  /// Bitrate in kbps; `0` when unknown.
  pub bitrate: u32,
  /// `1` when the directory's last connectivity check succeeded.
  pub lastcheckok: u8,
}
