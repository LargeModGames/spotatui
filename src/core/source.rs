//! Capability-based media-source traits — the seam for the multi-source refactor.
//!
//! A source is addressed by its URI **scheme** (`spotify:`, `file:`, `subsonic:`),
//! mirroring Mopidy's proven dispatch model. [`MediaSource`] is the required
//! minimum every source implements; the remaining traits are optional
//! capabilities discovered at runtime, so the UI can light up per source (e.g.
//! a source without [`LibraryProvider`] shows no "liked songs" tab).
//!
//! These are definitions only — implementations land in later slices
//! (`SpotifySource` over the existing `Network`, then `infra/local/` and
//! `infra/subsonic/`). All methods speak the domain types from
//! [`crate::core::plugin_api`]; rspotify types never appear here.
//!
//! **Dispatch.** These use native `async fn` in traits (matching the existing
//! `PlaybackNetwork` convention), which is *not* `dyn`-compatible. The planned
//! by-scheme routing therefore dispatches over a **closed enum** of concrete
//! sources (one variant per backend), matching on `scheme()` — not
//! `Box<dyn MediaSource>`. If open/plugin-provided sources are ever needed, add
//! the `async-trait` crate at that point rather than reaching for `dyn` here.

// No implementors in the binary yet; the multi-source slices wire these up.
#![allow(dead_code)]

use crate::core::plugin_api::{AlbumInfo, ArtistInfo, PlaylistInfo, SearchResults, TrackInfo};
use anyhow::Result;

/// The required minimum every media source implements: browse playlists and the
/// tracks within them.
pub trait MediaSource {
  /// Human-readable source name shown in the UI (e.g. `"Spotify"`, `"Navidrome"`).
  fn name(&self) -> &str;

  /// URI scheme this source owns, without the colon (e.g. `"spotify"`, `"file"`,
  /// `"subsonic"`). Used to route a URI to the source that can handle it.
  fn scheme(&self) -> &str;

  /// The user's playlists for this source.
  async fn playlists(&self) -> Result<Vec<PlaylistInfo>>;

  /// The tracks of a playlist, identified by its source-native URI.
  async fn tracks(&self, playlist_uri: &str) -> Result<Vec<TrackInfo>>;
}

/// Optional capability: search the source's catalog.
pub trait Searcher {
  async fn search(&self, query: &str) -> Result<SearchResults>;
}

/// Optional capability: the user's saved library (liked tracks, saved albums,
/// followed artists).
pub trait LibraryProvider {
  async fn saved_tracks(&self) -> Result<Vec<TrackInfo>>;
  async fn saved_albums(&self) -> Result<Vec<AlbumInfo>>;
  async fn saved_artists(&self) -> Result<Vec<ArtistInfo>>;
}

/// Optional capability: mutate playlists (add/remove tracks by URI).
pub trait PlaylistWriter {
  async fn add_tracks(&self, playlist_uri: &str, track_uris: &[String]) -> Result<()>;
  async fn remove_tracks(&self, playlist_uri: &str, track_uris: &[String]) -> Result<()>;
}

/// Optional capability: produce a playable audio stream for a URI and route it
/// into the shared rodio sink (so the visualizer and volume control work
/// uniformly across sources).
///
/// The concrete stream/handle return type is defined in the local-files slice
/// (Phase 3), when the symphonia → rodio pipeline is wired. Until then this is
/// the marker seam that lets the dispatch layer ask "can this source stream?".
pub trait Streamer {
  /// Begin streaming the given URI. Returns once playback has started (or
  /// errors if the URI is not streamable by this source).
  async fn stream(&self, uri: &str) -> Result<()>;
}
