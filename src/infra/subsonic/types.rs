//! Serde types for the Subsonic / OpenSubsonic REST API JSON response envelope.
//!
//! Every endpoint wraps its payload in:
//! ```json
//! { "subsonic-response": { "status": "ok", "version": "1.16.1", "<method>": { … } } }
//! ```
//!
//! The structs here are **private** to the subsonic module; callers work with
//! the domain types in [`crate::core::plugin_api`].

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Top-level envelope
// ---------------------------------------------------------------------------

/// Outer JSON wrapper: `{ "subsonic-response": <SubsonicResponse> }`.
#[derive(Debug, Deserialize)]
pub struct SubsonicEnvelope {
  #[serde(rename = "subsonic-response")]
  pub response: SubsonicResponse,
}

/// The `subsonic-response` object. A `status` of `"ok"` means the request
/// succeeded; `"failed"` means the nested `error` field is populated.
#[derive(Debug, Deserialize)]
pub struct SubsonicResponse {
  pub status: String,
  pub version: String,
  pub error: Option<SubsonicError>,
  // Payload fields — only one is populated per response.
  pub playlists: Option<PlaylistsWrapper>,
  pub playlist: Option<PlaylistDetail>,
  #[serde(rename = "searchResult3")]
  pub search_result3: Option<SearchResult3>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicError {
  pub code: u32,
  pub message: String,
}

// ---------------------------------------------------------------------------
// getPlaylists
// ---------------------------------------------------------------------------

/// `getPlaylists` → `playlists.playlist[]`
#[derive(Debug, Deserialize)]
pub struct PlaylistsWrapper {
  pub playlist: Option<Vec<SubsonicPlaylist>>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicPlaylist {
  pub id: String,
  pub name: String,
  /// Display name of the playlist owner.
  #[serde(default)]
  pub owner: String,
  #[serde(rename = "songCount", default)]
  pub song_count: u32,
  #[serde(default)]
  pub public: Option<bool>,
  #[serde(rename = "coverArt", default)]
  pub cover_art: Option<String>,
}

// ---------------------------------------------------------------------------
// getPlaylist
// ---------------------------------------------------------------------------

/// `getPlaylist` → `playlist` (single object with embedded `entry[]`).
#[derive(Debug, Deserialize)]
pub struct PlaylistDetail {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub owner: String,
  #[serde(rename = "songCount", default)]
  pub song_count: u32,
  #[serde(default)]
  pub public: Option<bool>,
  #[serde(rename = "coverArt", default)]
  pub cover_art: Option<String>,
  /// The actual tracks. Named `entry` in the Subsonic spec.
  #[serde(default)]
  pub entry: Vec<SubsonicSong>,
}

// ---------------------------------------------------------------------------
// search3
// ---------------------------------------------------------------------------

/// `search3` → `searchResult3`
#[derive(Debug, Default, Deserialize)]
pub struct SearchResult3 {
  #[serde(default)]
  pub song: Vec<SubsonicSong>,
  #[serde(default)]
  pub album: Vec<SubsonicAlbum>,
  #[serde(default)]
  pub artist: Vec<SubsonicArtist>,
}

// ---------------------------------------------------------------------------
// Shared media item structs
// ---------------------------------------------------------------------------

/// A single song/track as returned by Subsonic (`/rest/getPlaylist.view`,
/// `/rest/search3.view`, etc.).
#[derive(Debug, Deserialize)]
pub struct SubsonicSong {
  pub id: String,
  pub title: String,
  #[serde(default)]
  pub artist: Option<String>,
  #[serde(rename = "artistId", default)]
  pub artist_id: Option<String>,
  #[serde(default)]
  pub album: Option<String>,
  #[serde(rename = "albumId", default)]
  pub album_id: Option<String>,
  /// Duration in seconds (Subsonic uses seconds, not milliseconds).
  #[serde(default)]
  pub duration: Option<u64>,
  #[serde(rename = "trackNumber", default)]
  pub track_number: Option<u32>,
  #[serde(default)]
  pub year: Option<u32>,
  #[serde(rename = "coverArt", default)]
  pub cover_art: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicAlbum {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub artist: Option<String>,
  #[serde(rename = "artistId", default)]
  pub artist_id: Option<String>,
  #[serde(rename = "songCount", default)]
  pub song_count: Option<u32>,
  #[serde(default)]
  pub year: Option<u32>,
  #[serde(rename = "coverArt", default)]
  pub cover_art: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicArtist {
  pub id: String,
  pub name: String,
  #[serde(rename = "coverArt", default)]
  pub cover_art: Option<String>,
}
