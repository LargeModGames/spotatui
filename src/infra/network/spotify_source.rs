//! `SpotifySource`: implementation of the capability traits in
//! [`crate::core::source`] backed by the rspotify `AuthCodePkceSpotify`
//! client.
//!
//! This is **additive dead code** until the multi-source dispatch layer is
//! wired up in a later slice. All items are annotated with `#[allow(dead_code)]`
//! at the module level so the slim (no-streaming) CI build stays clean under
//! `-D warnings`.
//!
//! # Design
//! - Holds the rspotify client directly by value — no `App` reference.
//! - Uses rspotify's high-level typed `_manual` paginators; no custom HTTP.
//! - All API errors propagate via `?` (no silent swallowing).
//! - Pagination: methods that accept a single result page carry a
//!   `// TODO(multi-source): paginate` comment.
//!
//! # Known limitations
//! - `playlists()` and `tracks()` call `current_user_playlists_manual` /
//!   `playlist_items_manual`, which use rspotify's typed deserializer. That
//!   path hits a known Spotify API bug where the response JSON contains a
//!   duplicate `"items"` key, causing a deserializer crash. The existing
//!   `Network` implementation works around this with a raw-JSON compat path
//!   (see `library.rs`). Fix this before wiring `SpotifySource` live.
//!   See: `src/infra/network/library.rs` → `get_current_user_playlists`.
//! - `search()` uses `search_multiple`, which deserializes all five result
//!   types (tracks/albums/artists/playlists/shows) in a single
//!   `convert_result` call. If Spotify returns null fields in any category
//!   (the playlist type is a known offender — see `search.rs` comments), the
//!   entire search call fails rather than degrading gracefully per-category.
//!   The existing `Network::get_search_results` isolates each type so that a
//!   playlist parse failure leaves tracks/albums/artists intact. Fix before
//!   wiring `SpotifySource` live: either replicate the per-type isolation or
//!   use a raw JSON fallback for the playlist category.

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use rspotify::{
  model::{
    enums::types::SearchType,
    idtypes::{PlaylistId, TrackId},
    PlayableItem,
  },
  prelude::{BaseClient, OAuthClient},
  AuthCodePkceSpotify,
};

use crate::core::{
  plugin_api::{AlbumInfo, ArtistInfo, PlaylistInfo, SearchResults, TrackInfo},
  source::{LibraryProvider, MediaSource, PlaylistWriter, Searcher},
};
use crate::infra::network::mapping;

// ---------------------------------------------------------------------------
// Core struct
// ---------------------------------------------------------------------------

/// A [`MediaSource`] (and optional capability traits) backed by the Spotify
/// Web API via `AuthCodePkceSpotify`.
pub struct SpotifySource {
  spotify: AuthCodePkceSpotify,
}

impl SpotifySource {
  /// Construct a new `SpotifySource` from an already-authenticated client.
  pub fn new(spotify: AuthCodePkceSpotify) -> Self {
    Self { spotify }
  }
}

// ---------------------------------------------------------------------------
// MediaSource
// ---------------------------------------------------------------------------

impl MediaSource for SpotifySource {
  fn name(&self) -> &str {
    "Spotify"
  }

  fn scheme(&self) -> &str {
    "spotify"
  }

  /// Fetch all of the user's playlists, paginating until exhausted.
  ///
  /// # Known limitation
  /// Uses rspotify's typed deserializer, which crashes on the duplicate
  /// `"items"` key that Spotify's API sometimes returns. Fix before wiring
  /// live. See module-level doc.
  async fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
    let limit = 50u32;
    let mut offset = 0u32;
    let mut all_playlists = Vec::new();

    loop {
      let page = self
        .spotify
        .current_user_playlists_manual(Some(limit), Some(offset))
        .await
        .map_err(|e| anyhow!(e))?;

      let is_last = page.next.is_none() || page.items.is_empty();
      for p in &page.items {
        all_playlists.push(PlaylistInfo::from_simplified(p));
      }
      if is_last {
        break;
      }
      offset += limit;
    }

    Ok(all_playlists)
  }

  /// Fetch all tracks in a playlist, identified by its `spotify:playlist:…` URI.
  ///
  /// Episodes and unknown playable items are silently dropped (the trait
  /// contract returns `Vec<TrackInfo>`, not a mixed-media list).
  ///
  /// # Known limitation
  /// Uses rspotify's typed deserializer; see module-level doc for the
  /// duplicate-key caveat.
  async fn tracks(&self, playlist_uri: &str) -> Result<Vec<TrackInfo>> {
    let playlist_id = playlist_id_from_uri(playlist_uri)?;

    let limit = 50u32;
    let mut offset = 0u32;
    let mut all_tracks = Vec::new();

    loop {
      let page = self
        .spotify
        .playlist_items_manual(playlist_id.as_ref(), None, None, Some(limit), Some(offset))
        .await
        .map_err(|e| anyhow!(e))?;

      let is_last = page.next.is_none() || page.items.is_empty();
      for item in &page.items {
        if let Some(PlayableItem::Track(track)) = item.item.as_ref() {
          all_tracks.push(TrackInfo::from(track));
        }
      }
      if is_last {
        break;
      }
      offset += limit;
    }

    Ok(all_tracks)
  }
}

// ---------------------------------------------------------------------------
// Searcher
// ---------------------------------------------------------------------------

impl Searcher for SpotifySource {
  /// Search the Spotify catalog using a single batched API call for tracks,
  /// albums, artists, playlists, and shows simultaneously.
  ///
  /// TODO(multi-source): paginate - first page only. Limit is 10, which is
  /// the current Spotify API maximum for multi-type search as of 2026-02.
  async fn search(&self, query: &str) -> Result<SearchResults> {
    let result = self
      .spotify
      .search_multiple(
        query,
        [
          SearchType::Track,
          SearchType::Album,
          SearchType::Artist,
          SearchType::Playlist,
          SearchType::Show,
        ],
        None,
        None,
        Some(10),
        Some(0),
      )
      .await
      .map_err(|e| anyhow!(e))?;

    Ok(mapping::search_results_from_pages(
      result.tracks.as_ref(),
      result.albums.as_ref(),
      result.artists.as_ref(),
      result.playlists.as_ref(),
      result.shows.as_ref(),
    ))
  }
}

// ---------------------------------------------------------------------------
// LibraryProvider
// ---------------------------------------------------------------------------

impl LibraryProvider for SpotifySource {
  /// Fetch all saved (liked) tracks, paginating until exhausted.
  async fn saved_tracks(&self) -> Result<Vec<TrackInfo>> {
    let limit = 50u32;
    let mut offset = 0u32;
    let mut all_tracks = Vec::new();

    loop {
      let page = self
        .spotify
        .current_user_saved_tracks_manual(None, Some(limit), Some(offset))
        .await
        .map_err(|e| anyhow!(e))?;

      let is_last = page.next.is_none() || page.items.is_empty();
      for saved in &page.items {
        all_tracks.push(TrackInfo::from(&saved.track));
      }
      if is_last {
        break;
      }
      offset += limit;
    }

    Ok(all_tracks)
  }

  /// Fetch all saved albums, paginating until exhausted.
  async fn saved_albums(&self) -> Result<Vec<AlbumInfo>> {
    let limit = 50u32;
    let mut offset = 0u32;
    let mut all_albums = Vec::new();

    loop {
      let page = self
        .spotify
        .current_user_saved_albums_manual(None, Some(limit), Some(offset))
        .await
        .map_err(|e| anyhow!(e))?;

      let is_last = page.next.is_none() || page.items.is_empty();
      for saved in &page.items {
        all_albums.push(AlbumInfo::from(&saved.album));
      }
      if is_last {
        break;
      }
      offset += limit;
    }

    Ok(all_albums)
  }

  /// Fetch followed artists.
  ///
  /// TODO(multi-source): paginate - first page only. The cursor-based
  /// `current_user_followed_artists` API requires threading the `after`
  /// cursor through successive calls; only the first page is returned here.
  async fn saved_artists(&self) -> Result<Vec<ArtistInfo>> {
    let page = self
      .spotify
      .current_user_followed_artists(None, Some(50))
      .await
      .map_err(|e| anyhow!(e))?;

    Ok(page.items.iter().map(ArtistInfo::from).collect())
  }
}

// ---------------------------------------------------------------------------
// PlaylistWriter
// ---------------------------------------------------------------------------

impl PlaylistWriter for SpotifySource {
  /// Append `track_uris` to a playlist. Accepts both `spotify:track:…` URIs
  /// and bare base62 track IDs.
  async fn add_tracks(&self, playlist_uri: &str, track_uris: &[String]) -> Result<()> {
    if track_uris.is_empty() {
      return Ok(());
    }
    let playlist_id = playlist_id_from_uri(playlist_uri)?;
    let playable_ids: Result<Vec<_>> = track_uris
      .iter()
      .map(|uri| {
        TrackId::from_id_or_uri(uri)
          .map(|id| rspotify::model::idtypes::PlayableId::Track(id.into_static()))
          .map_err(|e| anyhow!("invalid track URI/id {:?}: {}", uri, e))
      })
      .collect();

    self
      .spotify
      .playlist_add_items(playlist_id.as_ref(), playable_ids?, None)
      .await
      .map_err(|e| anyhow!(e))?;

    Ok(())
  }

  /// Remove all occurrences of `track_uris` from a playlist. Accepts both
  /// `spotify:track:…` URIs and bare base62 track IDs.
  async fn remove_tracks(&self, playlist_uri: &str, track_uris: &[String]) -> Result<()> {
    if track_uris.is_empty() {
      return Ok(());
    }
    let playlist_id = playlist_id_from_uri(playlist_uri)?;
    let playable_ids: Result<Vec<_>> = track_uris
      .iter()
      .map(|uri| {
        TrackId::from_id_or_uri(uri)
          .map(|id| rspotify::model::idtypes::PlayableId::Track(id.into_static()))
          .map_err(|e| anyhow!("invalid track URI/id {:?}: {}", uri, e))
      })
      .collect();

    self
      .spotify
      .playlist_remove_all_occurrences_of_items(playlist_id.as_ref(), playable_ids?, None)
      .await
      .map_err(|e| anyhow!(e))?;

    Ok(())
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a Spotify playlist URI (`spotify:playlist:<id>`) or bare base62 id
/// into a `PlaylistId`. Centralised here so both `MediaSource::tracks` and
/// `PlaylistWriter` share the same error path.
fn playlist_id_from_uri(uri_or_id: &str) -> Result<PlaylistId<'static>> {
  PlaylistId::from_id_or_uri(uri_or_id)
    .map(|id| id.into_static())
    .map_err(|e| anyhow!("invalid playlist URI/id {:?}: {}", uri_or_id, e))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use rspotify::prelude::Id;

  // ---------------------------------------------------------------------------
  // MediaSource::name / scheme
  // ---------------------------------------------------------------------------

  fn dummy_spotify() -> AuthCodePkceSpotify {
    AuthCodePkceSpotify::with_config(
      rspotify::Credentials::new_pkce("test_client_id"),
      rspotify::OAuth {
        redirect_uri: "http://localhost:8888/callback".to_string(),
        ..Default::default()
      },
      rspotify::Config::default(),
    )
  }

  #[test]
  fn name_returns_spotify() {
    let src = SpotifySource::new(dummy_spotify());
    assert_eq!(src.name(), "Spotify");
  }

  #[test]
  fn scheme_returns_spotify() {
    let src = SpotifySource::new(dummy_spotify());
    assert_eq!(src.scheme(), "spotify");
  }

  // ---------------------------------------------------------------------------
  // playlist_id_from_uri
  // ---------------------------------------------------------------------------

  #[test]
  fn playlist_id_from_full_uri() {
    let id = playlist_id_from_uri("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M").unwrap();
    assert_eq!(id.id(), "37i9dQZF1DXcBWIGoYBM5M");
  }

  #[test]
  fn playlist_id_from_bare_id() {
    let id = playlist_id_from_uri("37i9dQZF1DXcBWIGoYBM5M").unwrap();
    assert_eq!(id.id(), "37i9dQZF1DXcBWIGoYBM5M");
  }

  #[test]
  fn playlist_id_from_invalid_returns_error() {
    let result = playlist_id_from_uri("not-a-valid-id!!!@@@");
    assert!(result.is_err());
  }

  // ---------------------------------------------------------------------------
  // Mapping round-trip: TrackInfo from FullTrack (via test_helpers)
  // ---------------------------------------------------------------------------

  #[test]
  fn track_info_from_full_track_round_trip() {
    use crate::core::test_helpers::full_track;

    let ft = full_track("4uLU6hMCjMI75M1A2tKUQC", "Never Gonna Give You Up");
    let info = TrackInfo::from(&ft);

    assert_eq!(info.name, "Never Gonna Give You Up");
    assert_eq!(
      info.uri.as_deref(),
      Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC")
    );
    assert_eq!(info.id.as_deref(), Some("4uLU6hMCjMI75M1A2tKUQC"));
    assert!(!info.artists.is_empty());
  }
}
