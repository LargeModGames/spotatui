//! rspotify → domain mapping layer.
//!
//! This is the **only** place (besides the pre-existing helpers in
//! `core::plugin_api`, which relocate here in a later slice) where rspotify
//! model types are converted into the source-agnostic domain types from
//! [`crate::core::plugin_api`]. Keeping every conversion behind this boundary
//! is what makes the end-state criterion — *zero `rspotify::model` imports
//! outside `src/infra/network/`* — achievable.
//!
//! Conventions:
//! - We never read fields rspotify marks `#[deprecated]` (Spotify removed them),
//!   so this module stays clean under `-D warnings`.
//! - URIs/ids are produced via the [`Id`] trait: `.id()` → base62, `.uri()` →
//!   full `spotify:…` URI.

// Conversions are consumed by the per-screen migration slices, not yet by the
// binary itself.
#![allow(dead_code)]

use crate::core::pagination::{CursorPaged, Paged};
use crate::core::plugin_api::{
  AlbumInfo, ArtistInfo, ArtistRef, EpisodeInfo, PlayableInfo, PlaylistInfo, ResumePointInfo,
  SavedAlbumInfo, SearchResults, ShowInfo, TrackInfo,
};
use rspotify::model::{
  album::{FullAlbum, SimplifiedAlbum},
  artist::{FullArtist, SimplifiedArtist},
  page::{CursorBasedPage, Page},
  playlist::SimplifiedPlaylist,
  show::{FullEpisode, FullShow, SimplifiedEpisode, SimplifiedShow},
  track::{FullTrack, SimplifiedTrack},
  PlayableItem,
};
use rspotify::prelude::Id;

/// First image URL of an `images` list, if any.
fn first_image(images: &[rspotify::model::image::Image]) -> Option<String> {
  images.first().map(|img| img.url.clone())
}

/// Convert a chrono `Duration` to non-negative milliseconds.
fn duration_ms(duration: chrono::Duration) -> u64 {
  duration.num_milliseconds().max(0) as u64
}

/// A single artist reference (id + display name).
pub fn artist_ref(artist: &SimplifiedArtist) -> ArtistRef {
  ArtistRef {
    id: artist.id.as_ref().map(|id| id.id().to_string()),
    name: artist.name.clone(),
  }
}

fn artist_refs(artists: &[SimplifiedArtist]) -> Vec<ArtistRef> {
  artists.iter().map(artist_ref).collect()
}

// --- Artists ---------------------------------------------------------------

impl From<&FullArtist> for ArtistInfo {
  fn from(a: &FullArtist) -> Self {
    ArtistInfo {
      id: Some(a.id.id().to_string()),
      uri: Some(a.id.uri()),
      name: a.name.clone(),
      image_url: first_image(&a.images),
    }
  }
}

impl From<&SimplifiedArtist> for ArtistInfo {
  fn from(a: &SimplifiedArtist) -> Self {
    ArtistInfo {
      id: a.id.as_ref().map(|id| id.id().to_string()),
      uri: a.id.as_ref().map(|id| id.uri()),
      name: a.name.clone(),
      image_url: None,
    }
  }
}

// --- Tracks ----------------------------------------------------------------

impl From<&FullTrack> for TrackInfo {
  fn from(t: &FullTrack) -> Self {
    TrackInfo {
      uri: t.id.as_ref().map(|id| id.uri()),
      name: t.name.clone(),
      artists: t.artists.iter().map(|a| a.name.clone()).collect(),
      album: t.album.name.clone(),
      duration_ms: duration_ms(t.duration),
      id: t.id.as_ref().map(|id| id.id().to_string()),
      album_id: t.album.id.as_ref().map(|id| id.id().to_string()),
      artist_refs: artist_refs(&t.artists),
      is_playable: t.is_playable.unwrap_or(true),
      is_local: t.is_local,
      track_number: t.track_number,
      explicit: t.explicit,
      // Spotify cover art is resolved separately from album images, not here.
      image_url: None,
    }
  }
}

impl From<&SimplifiedTrack> for TrackInfo {
  fn from(t: &SimplifiedTrack) -> Self {
    TrackInfo {
      uri: t.id.as_ref().map(|id| id.uri()),
      name: t.name.clone(),
      artists: t.artists.iter().map(|a| a.name.clone()).collect(),
      album: t
        .album
        .as_ref()
        .map(|al| al.name.clone())
        .unwrap_or_default(),
      duration_ms: duration_ms(t.duration),
      id: t.id.as_ref().map(|id| id.id().to_string()),
      album_id: t
        .album
        .as_ref()
        .and_then(|al| al.id.as_ref())
        .map(|id| id.id().to_string()),
      artist_refs: artist_refs(&t.artists),
      is_playable: t.is_playable.unwrap_or(true),
      is_local: t.is_local,
      track_number: t.track_number,
      explicit: t.explicit,
      // Spotify cover art is resolved separately from album images, not here.
      image_url: None,
    }
  }
}

// --- Albums ----------------------------------------------------------------

impl From<&SimplifiedAlbum> for AlbumInfo {
  fn from(a: &SimplifiedAlbum) -> Self {
    AlbumInfo {
      id: a.id.as_ref().map(|id| id.id().to_string()),
      uri: a.id.as_ref().map(|id| id.uri()),
      name: a.name.clone(),
      artists: artist_refs(&a.artists),
      album_type: a.album_type.clone(),
      release_date: a.release_date.clone(),
      total_tracks: None,
      image_url: first_image(&a.images),
      tracks: Vec::new(),
    }
  }
}

impl From<&FullAlbum> for AlbumInfo {
  fn from(a: &FullAlbum) -> Self {
    let album_id = a.id.id().to_string();
    // Child tracks come back as SimplifiedTrack without their parent album set;
    // backfill the album name/id from this full album so each row is renderable.
    let tracks = a
      .tracks
      .items
      .iter()
      .map(|t| {
        let mut info = TrackInfo::from(t);
        if info.album.is_empty() {
          info.album = a.name.clone();
        }
        if info.album_id.is_none() {
          info.album_id = Some(album_id.clone());
        }
        info
      })
      .collect();
    AlbumInfo {
      id: Some(album_id),
      uri: Some(a.id.uri()),
      name: a.name.clone(),
      artists: artist_refs(&a.artists),
      // `AlbumType` derives strum `IntoStaticStr` with snake_case, so this
      // yields the same wire string ("album"/"single"/"appears_on"/
      // "compilation") as the raw `SimplifiedAlbum::album_type` API field —
      // keeping both mapping paths consistent.
      album_type: Some(<&'static str>::from(a.album_type).to_string()),
      release_date: Some(a.release_date.clone()),
      total_tracks: Some(a.tracks.total),
      image_url: first_image(&a.images),
      tracks,
    }
  }
}

// --- Shows / episodes ------------------------------------------------------

impl From<&SimplifiedShow> for ShowInfo {
  // `publisher` is `#[deprecated]` in rspotify (a field Spotify dropped), but the
  // podcast list still renders it as the "by …" attribution, so we read it here
  // behind the boundary rather than dropping a column.
  #[allow(deprecated)]
  fn from(s: &SimplifiedShow) -> Self {
    ShowInfo {
      id: Some(s.id.id().to_string()),
      uri: Some(s.id.uri()),
      name: s.name.clone(),
      description: s.description.clone(),
      publisher: s.publisher.clone(),
      image_url: first_image(&s.images),
    }
  }
}

impl From<&FullShow> for ShowInfo {
  #[allow(deprecated)]
  fn from(s: &FullShow) -> Self {
    ShowInfo {
      id: Some(s.id.id().to_string()),
      uri: Some(s.id.uri()),
      name: s.name.clone(),
      description: s.description.clone(),
      publisher: s.publisher.clone(),
      image_url: first_image(&s.images),
    }
  }
}

/// Map rspotify's `ResumePoint` (chrono `Duration` position) to the domain
/// [`ResumePointInfo`] (ms).
fn resume_point_info(rp: &rspotify::model::show::ResumePoint) -> ResumePointInfo {
  ResumePointInfo {
    fully_played: rp.fully_played,
    resume_position_ms: duration_ms(rp.resume_position),
  }
}

impl From<&SimplifiedEpisode> for EpisodeInfo {
  fn from(e: &SimplifiedEpisode) -> Self {
    EpisodeInfo {
      id: Some(e.id.id().to_string()),
      uri: Some(e.id.uri()),
      name: e.name.clone(),
      // SimplifiedEpisode carries no parent show; it is only ever listed within
      // a show's own episode view, where the show name is shown separately.
      show_name: String::new(),
      duration_ms: duration_ms(e.duration),
      description: e.description.clone(),
      release_date: e.release_date.clone(),
      is_playable: e.is_playable,
      resume_point: e.resume_point.as_ref().map(resume_point_info),
      image_url: first_image(&e.images),
    }
  }
}

impl From<&FullEpisode> for EpisodeInfo {
  fn from(e: &FullEpisode) -> Self {
    EpisodeInfo {
      id: Some(e.id.id().to_string()),
      uri: Some(e.id.uri()),
      name: e.name.clone(),
      show_name: e.show.name.clone(),
      duration_ms: duration_ms(e.duration),
      description: e.description.clone(),
      release_date: e.release_date.clone(),
      is_playable: e.is_playable,
      resume_point: e.resume_point.as_ref().map(resume_point_info),
      image_url: first_image(&e.images),
    }
  }
}

/// Map a rspotify `SavedAlbum` to the domain [`SavedAlbumInfo`], preserving
/// `added_at` (as an RFC 3339 UTC string) for the "Date Added" sort.
pub fn saved_album_info(sa: &rspotify::model::album::SavedAlbum) -> SavedAlbumInfo {
  SavedAlbumInfo {
    album: AlbumInfo::from(&sa.album),
    added_at: sa.added_at.to_rfc3339(),
  }
}

// --- Playable items (queue / playback context) -----------------------------

/// Map a rspotify `PlayableItem` to a domain [`PlayableInfo`]. The `Unknown`
/// (unparseable) variant yields `None`.
pub fn playable_info(item: &PlayableItem) -> Option<PlayableInfo> {
  match item {
    PlayableItem::Track(t) => Some(PlayableInfo::Track(TrackInfo::from(t))),
    PlayableItem::Episode(e) => Some(PlayableInfo::Episode(EpisodeInfo::from(e))),
    PlayableItem::Unknown(_) => None,
  }
}

/// Convert a page of `PlaylistItem` into a domain page of
/// `(absolute_position, PlayableInfo)`. Each item's inner `track`
/// (`Option<PlayableItem>`) is mapped via [`playable_info`]; items that are
/// `None` or `Unknown` are dropped from the result, but their slot is still
/// counted so the surviving items keep their **absolute playlist position**
/// (`page.offset + raw slot index`). That position drives removal-by-position
/// and play-from-here offsets, so it must reflect the real playlist index, not
/// the compacted one. Paging metadata is copied straight from the source page.
pub fn playlist_items_page(
  page: &Page<rspotify::model::playlist::PlaylistItem>,
) -> Paged<(u32, PlayableInfo)> {
  Paged {
    items: page
      .items
      .iter()
      .enumerate()
      .filter_map(|(idx, item)| {
        let info = item.item.as_ref().and_then(playable_info)?;
        Some((page.offset + idx as u32, info))
      })
      .collect(),
    offset: page.offset,
    limit: page.limit,
    total: page.total,
    next: page.next.clone(),
    previous: page.previous.clone(),
  }
}

// --- Search ----------------------------------------------------------------

/// Assemble domain [`SearchResults`] from whichever rspotify result pages are
/// present (the `App` holds each category as an independent `Option<Page<…>>`).
pub fn search_results_from_pages(
  tracks: Option<&Page<FullTrack>>,
  albums: Option<&Page<SimplifiedAlbum>>,
  artists: Option<&Page<FullArtist>>,
  playlists: Option<&Page<SimplifiedPlaylist>>,
  shows: Option<&Page<SimplifiedShow>>,
) -> SearchResults {
  SearchResults {
    tracks: tracks
      .map(|p| p.items.iter().map(TrackInfo::from).collect())
      .unwrap_or_default(),
    albums: albums
      .map(|p| p.items.iter().map(AlbumInfo::from).collect())
      .unwrap_or_default(),
    artists: artists
      .map(|p| p.items.iter().map(ArtistInfo::from).collect())
      .unwrap_or_default(),
    playlists: playlists
      .map(|p| p.items.iter().map(PlaylistInfo::from_simplified).collect())
      .unwrap_or_default(),
    shows: shows
      .map(|p| p.items.iter().map(ShowInfo::from).collect())
      .unwrap_or_default(),
  }
}

// --- Pagination ------------------------------------------------------------

/// Convert an rspotify offset-based [`Page<U>`] into a domain
/// [`Paged<T>`], mapping each item with `f`. Drops the API `href`.
///
/// Pass a **closure**, not a bare `From::from` fn-item — e.g.
/// `map_page(&page, |t| TrackInfo::from(t))`. A bare `TrackInfo::from` trips
/// Rust's higher-ranked-lifetime inference (`Fn` not general enough).
pub fn map_page<U, T>(page: &Page<U>, f: impl Fn(&U) -> T) -> Paged<T>
where
  U: serde::de::DeserializeOwned,
{
  Paged {
    items: page.items.iter().map(f).collect(),
    offset: page.offset,
    limit: page.limit,
    total: page.total,
    next: page.next.clone(),
    previous: page.previous.clone(),
  }
}

/// Convert an rspotify [`CursorBasedPage<U>`] into a domain
/// [`CursorPaged<T>`], mapping each item with `f`. Pass a closure (see
/// [`map_page`]).
pub fn map_cursor_page<U, T>(page: &CursorBasedPage<U>, f: impl Fn(&U) -> T) -> CursorPaged<T> {
  CursorPaged {
    items: page.items.iter().map(f).collect(),
    limit: page.limit,
    next: page.next.clone(),
    cursor_after: page.cursors.as_ref().and_then(|c| c.after.clone()),
    total: page.total,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::full_track;
  use rspotify::model::{album::SimplifiedAlbum, artist::SimplifiedArtist, idtypes::ArtistId};

  fn artist_with_id(id: &str, name: &str) -> SimplifiedArtist {
    SimplifiedArtist {
      id: Some(ArtistId::from_id(id).unwrap().into_static()),
      name: name.to_string(),
      ..Default::default()
    }
  }

  #[test]
  fn full_track_maps_core_and_extended_fields() {
    let ft = full_track("4uLU6hMCjMI75M1A2tKUQC", "Never Gonna Give You Up");
    let info = TrackInfo::from(&ft);

    // Scripting-contract fields (must keep working for api_version = 4).
    assert_eq!(info.name, "Never Gonna Give You Up");
    assert_eq!(info.artists, vec!["Test Artist".to_string()]);
    assert_eq!(info.album, "Test Album");
    assert_eq!(info.duration_ms, 180_000);
    assert_eq!(
      info.uri.as_deref(),
      Some("spotify:track:4uLU6hMCjMI75M1A2tKUQC")
    );

    // Extended fields.
    assert_eq!(info.id.as_deref(), Some("4uLU6hMCjMI75M1A2tKUQC"));
    assert_eq!(info.album_id, None); // test fixture album has no id
    assert_eq!(info.artist_refs.len(), 1);
    assert_eq!(info.artist_refs[0].name, "Test Artist");
    assert!(info.is_playable);
    assert!(!info.is_local);
    assert_eq!(info.track_number, 1);
    assert!(!info.explicit);
  }

  #[test]
  fn simplified_artist_maps_id_and_uri() {
    let artist = artist_with_id("2WX2uTcsvV5OnS0inACecP", "Survive Said The Prophet");
    let info = ArtistInfo::from(&artist);
    assert_eq!(info.id.as_deref(), Some("2WX2uTcsvV5OnS0inACecP"));
    assert_eq!(
      info.uri.as_deref(),
      Some("spotify:artist:2WX2uTcsvV5OnS0inACecP")
    );
    assert_eq!(info.name, "Survive Said The Prophet");
    assert_eq!(info.image_url, None);
  }

  #[test]
  fn simplified_album_maps_without_tracks() {
    let album = SimplifiedAlbum {
      name: "Inhuman".to_string(),
      album_type: Some("album".to_string()),
      artists: vec![artist_with_id("2WX2uTcsvV5OnS0inACecP", "SSTP")],
      ..Default::default()
    };
    let info = AlbumInfo::from(&album);
    assert_eq!(info.name, "Inhuman");
    assert_eq!(info.album_type.as_deref(), Some("album"));
    assert_eq!(info.artists.len(), 1);
    assert_eq!(info.artists[0].name, "SSTP");
    assert_eq!(info.total_tracks, None);
    assert!(info.tracks.is_empty());
  }

  #[test]
  fn search_aggregator_collects_present_categories() {
    let tracks = Page {
      href: String::new(),
      items: vec![full_track("4uLU6hMCjMI75M1A2tKUQC", "A")],
      limit: 1,
      next: None,
      offset: 0,
      previous: None,
      total: 1,
    };
    let results = search_results_from_pages(Some(&tracks), None, None, None, None);
    assert_eq!(results.tracks.len(), 1);
    assert_eq!(results.tracks[0].name, "A");
    assert!(results.albums.is_empty());
    assert!(results.artists.is_empty());
    assert!(results.playlists.is_empty());
    assert!(results.shows.is_empty());
  }

  #[test]
  fn map_page_converts_items_and_preserves_paging() {
    let page = Page {
      href: "https://api/x".to_string(),
      items: vec![
        full_track("4uLU6hMCjMI75M1A2tKUQC", "A"),
        full_track("1301WleyT98MSxVHPZCA6M", "B"),
      ],
      limit: 20,
      next: Some("https://api/x?offset=20".to_string()),
      offset: 0,
      previous: None,
      total: 42,
    };
    let mapped = map_page(&page, |t| TrackInfo::from(t));
    assert_eq!(mapped.items.len(), 2);
    assert_eq!(mapped.items[0].name, "A");
    assert_eq!(mapped.items[1].name, "B");
    assert_eq!(mapped.offset, 0);
    assert_eq!(mapped.limit, 20);
    assert_eq!(mapped.total, 42);
    assert!(mapped.has_next());
  }

  #[test]
  fn track_info_serializes_with_api_v4_contract_keys() {
    // Plugins built against api_version = 4 read these exact keys; growth must
    // only ever ADD keys, never remove/rename these.
    let info = TrackInfo::from(&full_track("4uLU6hMCjMI75M1A2tKUQC", "A"));
    let json = serde_json::to_value(&info).unwrap();
    let obj = json.as_object().unwrap();
    for key in ["uri", "name", "artists", "album", "duration_ms"] {
      assert!(obj.contains_key(key), "missing contract key `{key}`");
    }
  }

  #[test]
  #[allow(deprecated)] // PlaylistItem struct literal must set the deprecated `track` field
  fn playlist_items_page_keeps_absolute_position_when_slots_are_dropped() {
    use rspotify::model::playlist::PlaylistItem;

    // A `None` slot before the second track must still advance the index so the
    // surviving track keeps its true playlist position (removal-by-position
    // depends on it).
    let track_item = |id: &str| PlaylistItem {
      added_at: None,
      added_by: None,
      is_local: false,
      track: None,
      item: Some(PlayableItem::Track(full_track(id, id))),
    };
    let empty_item = PlaylistItem {
      added_at: None,
      added_by: None,
      is_local: false,
      track: None,
      item: None,
    };

    let page = Page {
      href: String::new(),
      items: vec![
        track_item("4uLU6hMCjMI75M1A2tKUQC"), // raw index 0
        empty_item,                           // raw index 1 (dropped)
        track_item("1301WleyT98MSxVHPZCA6M"), // raw index 2
      ],
      limit: 20,
      next: None,
      offset: 40,
      previous: None,
      total: 43,
    };

    let mapped = playlist_items_page(&page);
    // Two playables survive; positions reflect raw slots offset by the page offset.
    assert_eq!(mapped.items.len(), 2);
    assert_eq!(mapped.items[0].0, 40);
    assert_eq!(mapped.items[1].0, 42);
    // Paging metadata is copied verbatim.
    assert_eq!(mapped.offset, 40);
    assert_eq!(mapped.total, 43);
  }
}
