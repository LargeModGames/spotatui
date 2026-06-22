//! Capability traits for multi-source media backends.
//!
//! Every media source (Spotify, Subsonic, local files, ...) implements [`MediaSource`].
//! Sources that support full-text search also implement [`Searcher`].
//! These traits are the stable contract between the infra layer and future UI/routing code.

// Not yet wired into the application runtime -- Wave 1 will do that.
#![allow(dead_code)]

use crate::core::plugin_api::{PlaylistInfo, TrackInfo};
use anyhow::Result;
use std::{future::Future, pin::Pin};

/// Boxed, heap-allocated future returned by trait methods so the trait remains object-safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Information about a single album.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlbumInfo {
  /// Source-specific URI (e.g. `subsonic:album:42`).
  pub uri: String,
  /// Album title.
  pub name: String,
  /// List of artist names credited on the album.
  pub artists: Vec<String>,
  /// Release year, if known.
  pub year: Option<u32>,
}

/// Information about a single artist.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtistInfo {
  /// Source-specific URI (e.g. `subsonic:artist:7`).
  pub uri: String,
  /// Artist name.
  pub name: String,
}

/// Aggregated search results across all entity types.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchResults {
  pub tracks: Vec<TrackInfo>,
  pub albums: Vec<AlbumInfo>,
  pub artists: Vec<ArtistInfo>,
  pub playlists: Vec<PlaylistInfo>,
}

/// A media source that can enumerate playlists and their tracks.
///
/// Implementors live in `src/infra/` and are constructed with source-specific
/// credentials/config at startup. The application runtime will hold a
/// `Box<dyn MediaSource + Send + Sync>` per configured source.
pub trait MediaSource: Send + Sync {
  /// Short identifier for this source, e.g. `"Subsonic"` or `"Spotify"`.
  fn name(&self) -> &str;

  /// URI scheme prefix this source owns, e.g. `"subsonic"` or `"spotify"`.
  fn scheme(&self) -> &str;

  /// Return all playlists visible to the authenticated user.
  fn playlists(&self) -> BoxFuture<'_, Result<Vec<PlaylistInfo>>>;

  /// Return every track in the playlist identified by `playlist_uri`.
  ///
  /// The URI must use the scheme returned by [`MediaSource::scheme`].
  fn tracks<'a>(&'a self, playlist_uri: &'a str) -> BoxFuture<'a, Result<Vec<TrackInfo>>>;
}

/// A media source that additionally supports full-text search.
pub trait Searcher: MediaSource {
  /// Search for tracks, albums, artists, and playlists matching `query`.
  fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<SearchResults>>;
}
