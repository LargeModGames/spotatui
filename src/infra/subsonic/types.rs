//! Serde structs mirroring the Subsonic JSON envelope.
//!
//! Only the fields required by spotatui are captured; unknown fields are
//! silently ignored (all structs use `#[serde(default)]` or optional fields).
//!
//! Conversion methods on each struct translate Subsonic shapes into the
//! source-agnostic domain types defined in [`crate::core`].

// Not yet wired into the application runtime -- Wave 1 will do that.
#![allow(dead_code)]

use serde::Deserialize;

use crate::core::{
  plugin_api::{PlaylistInfo, TrackInfo},
  source::{AlbumInfo, ArtistInfo},
};

// ---------------------------------------------------------------------------
// Top-level response envelope
// ---------------------------------------------------------------------------

/// Outer wrapper: every Subsonic JSON response is `{"subsonic-response": { ... }}`.
#[derive(Debug, Deserialize)]
pub struct SubsonicResponseWrapper {
  #[serde(rename = "subsonic-response")]
  pub subsonic_response: SubsonicResponse,
}

/// The `subsonic-response` object contained in every reply.
#[derive(Debug, Deserialize)]
pub struct SubsonicResponse {
  /// `"ok"` on success, `"failed"` on error.
  pub status: String,
  /// Protocol version the server is speaking, e.g. `"1.16.1"`.
  pub version: String,
  /// Present on error responses.
  pub error: Option<SubsonicError>,

  // ---- endpoint-specific payloads ----
  /// Returned by `getPlaylists.view`.
  pub playlists: Option<PlaylistsContainer>,
  /// Returned by `getPlaylist.view`.
  pub playlist: Option<SubsonicPlaylistDetail>,
  /// Returned by `search3.view`.
  #[serde(rename = "searchResult3")]
  pub search_result3: Option<SearchResult3>,
}

/// Subsonic error descriptor.
#[derive(Debug, Deserialize)]
pub struct SubsonicError {
  pub code: u32,
  pub message: String,
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

/// Container returned by `getPlaylists`.
#[derive(Debug, Deserialize)]
pub struct PlaylistsContainer {
  pub playlist: Option<Vec<SubsonicPlaylist>>,
}

/// A single playlist entry from `getPlaylists`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubsonicPlaylist {
  pub id: String,
  pub name: String,
  /// Some servers (e.g. Funkwhale) omit this field; default to empty string.
  #[serde(default)]
  pub owner: String,
  #[serde(rename = "songCount", default)]
  pub song_count: u32,
  #[serde(default)]
  pub duration: u64,
  #[serde(default)]
  pub public: Option<bool>,
  #[serde(default)]
  pub comment: Option<String>,
}

impl SubsonicPlaylist {
  /// Convert into the source-agnostic [`PlaylistInfo`] domain type.
  pub fn into_playlist_info(self) -> PlaylistInfo {
    PlaylistInfo {
      uri: format!("subsonic:playlist:{}", self.id),
      name: self.name,
      owner: self.owner,
      track_count: self.song_count,
    }
  }
}

// ---------------------------------------------------------------------------
// Playlist detail (getPlaylist)
// ---------------------------------------------------------------------------

/// Returned by `getPlaylist.view`; contains the playlist metadata plus entries.
#[derive(Debug, Deserialize)]
pub struct SubsonicPlaylistDetail {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub owner: String,
  #[serde(rename = "songCount", default)]
  pub song_count: u32,
  /// The list of song entries within this playlist.
  pub entry: Option<Vec<SubsonicSong>>,
}

// ---------------------------------------------------------------------------
// Songs
// ---------------------------------------------------------------------------

/// A single song/track as returned by `getPlaylist` or `search3`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubsonicSong {
  pub id: String,
  pub title: String,
  #[serde(default)]
  pub artist: Option<String>,
  #[serde(default)]
  pub album: Option<String>,
  /// Duration in **seconds** (Subsonic convention).
  #[serde(default)]
  pub duration: u64,
  #[serde(default)]
  pub track: Option<u32>,
  #[serde(default)]
  pub year: Option<u32>,
  #[serde(rename = "coverArt", default)]
  pub cover_art: Option<String>,
}

impl SubsonicSong {
  /// Convert into the source-agnostic [`TrackInfo`] domain type.
  pub fn into_track_info(self) -> TrackInfo {
    TrackInfo {
      uri: Some(format!("subsonic:track:{}", self.id)),
      name: self.title,
      artists: self.artist.into_iter().collect(),
      album: self.album.unwrap_or_default(),
      // Subsonic sends duration in seconds; domain type uses milliseconds.
      duration_ms: self.duration * 1_000,
    }
  }
}

// ---------------------------------------------------------------------------
// Search results (search3)
// ---------------------------------------------------------------------------

/// The `searchResult3` object returned by `search3.view`.
#[derive(Debug, Deserialize)]
pub struct SearchResult3 {
  pub song: Option<Vec<SubsonicSong>>,
  pub album: Option<Vec<SubsonicAlbum>>,
  pub artist: Option<Vec<SubsonicArtist>>,
}

/// A single album as returned by `search3`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubsonicAlbum {
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub artist: Option<String>,
  #[serde(default)]
  pub year: Option<u32>,
}

impl SubsonicAlbum {
  /// Convert into the source-agnostic [`AlbumInfo`] domain type.
  pub fn into_album_info(self) -> AlbumInfo {
    AlbumInfo {
      uri: format!("subsonic:album:{}", self.id),
      name: self.name,
      artists: self.artist.into_iter().collect(),
      year: self.year,
    }
  }
}

/// A single artist as returned by `search3`.
#[derive(Debug, Clone, Deserialize)]
pub struct SubsonicArtist {
  pub id: String,
  pub name: String,
}

impl SubsonicArtist {
  /// Convert into the source-agnostic [`ArtistInfo`] domain type.
  pub fn into_artist_info(self) -> ArtistInfo {
    ArtistInfo {
      uri: format!("subsonic:artist:{}", self.id),
      name: self.name,
    }
  }
}
