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

/// The source the UI is currently scoped to — which catalog the sidebar, search
/// and capability gating reflect.
///
/// This is a **browse-scope** marker only: it never changes playback routing
/// (that stays URI-scheme driven via `route_local_event` + `App::local_playback`),
/// so switching the active source never interrupts what is currently playing.
///
/// The enum is deliberately unconditional — every variant compiles in every
/// build (including the slim `telemetry`-only CI build) so handlers and UI code
/// never need `#[cfg]`. Only the per-source *data loading* is gated behind that
/// source's feature (`local-files`, `subsonic`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Source {
  #[default]
  Spotify,
  Local,
  Subsonic,
  Radio,
  YouTube,
}

impl Source {
  /// Every selectable source, in display order. Add new sources here.
  pub const ALL: [Source; 5] = [
    Source::Spotify,
    Source::Local,
    Source::Subsonic,
    Source::Radio,
    Source::YouTube,
  ];

  /// Human-readable label shown in the source picker.
  pub fn label(&self) -> &'static str {
    match self {
      Source::Spotify => "Spotify",
      Source::Local => "Local Files",
      Source::Subsonic => "Subsonic",
      Source::Radio => "Internet Radio",
      Source::YouTube => "YouTube",
    }
  }

  /// Config-file token used to persist the active source.
  /// Distinct from `label()` so the config key stays stable even if the
  /// display name changes.
  pub fn to_config_str(self) -> &'static str {
    match self {
      Source::Spotify => "Spotify",
      Source::Local => "Local",
      Source::Subsonic => "Subsonic",
      Source::Radio => "Radio",
      Source::YouTube => "YouTube",
    }
  }

  /// Parse the config-file token back to a `Source`.
  /// Unknown strings fall back to `Spotify` so old or hand-edited configs
  /// never break startup.
  pub fn from_config_str(s: &str) -> Self {
    match s {
      "Local" => Source::Local,
      "Subsonic" => Source::Subsonic,
      "Radio" => Source::Radio,
      "YouTube" => Source::YouTube,
      _ => Source::Spotify,
    }
  }

  /// Whether this source can search its catalog (implements [`Searcher`]).
  pub fn supports_search(&self) -> bool {
    matches!(
      self,
      Source::Spotify | Source::Subsonic | Source::Radio | Source::YouTube
    )
  }

  /// Whether this source exposes a saved library — liked songs, saved albums,
  /// followed artists (implements [`LibraryProvider`]).
  pub fn supports_library(&self) -> bool {
    matches!(self, Source::Spotify)
  }

  /// Whether this source can mutate playlists (implements [`PlaylistWriter`]).
  pub fn supports_playlist_write(&self) -> bool {
    matches!(self, Source::Spotify)
  }

  /// Whether this source supports liking / saving the currently-playing track
  /// (requires a [`LibraryProvider`] that can accept writes). Currently
  /// equivalent to [`supports_library`](Self::supports_library).
  pub fn supports_like(&self) -> bool {
    matches!(self, Source::Spotify)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn spotify_supports_all_capabilities() {
    assert!(Source::Spotify.supports_search());
    assert!(Source::Spotify.supports_library());
    assert!(Source::Spotify.supports_playlist_write());
    assert!(Source::Spotify.supports_like());
  }

  #[test]
  fn local_supports_no_spotify_only_capabilities() {
    assert!(!Source::Local.supports_search());
    assert!(!Source::Local.supports_library());
    assert!(!Source::Local.supports_playlist_write());
    assert!(!Source::Local.supports_like());
  }

  #[test]
  fn subsonic_supports_search_only() {
    // Subsonic speaks `search3`, so search is on; library/playlist-write/like
    // depend on `LibraryProvider`/`PlaylistWriter` impls that are deferred
    // follow-ups, so they stay off for now.
    assert!(Source::Subsonic.supports_search());
    assert!(!Source::Subsonic.supports_library());
    assert!(!Source::Subsonic.supports_playlist_write());
    assert!(!Source::Subsonic.supports_like());
  }

  #[test]
  fn radio_supports_search_only() {
    // The radio-browser.info directory is searchable; there is no library,
    // no playlists to write, and no like/star concept for a live stream.
    assert!(Source::Radio.supports_search());
    assert!(!Source::Radio.supports_library());
    assert!(!Source::Radio.supports_playlist_write());
    assert!(!Source::Radio.supports_like());
  }

  #[test]
  fn youtube_supports_search_only() {
    // yt-dlp searches the catalog; there is no library, no writable playlists,
    // and no like/star concept without a Google account.
    assert!(Source::YouTube.supports_search());
    assert!(!Source::YouTube.supports_library());
    assert!(!Source::YouTube.supports_playlist_write());
    assert!(!Source::YouTube.supports_like());
  }

  #[test]
  fn config_str_round_trips_every_source() {
    for source in Source::ALL {
      assert_eq!(Source::from_config_str(source.to_config_str()), source);
    }
  }
}

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
