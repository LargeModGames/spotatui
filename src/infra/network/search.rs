use super::{IoEvent, Network};
use crate::core::plugin_api::{AlbumInfo, ArtistInfo, PlaylistInfo, ShowInfo, TrackInfo};
use crate::infra::network::mapping::map_page;
use anyhow::anyhow;
use rspotify::model::{
  artist::FullArtist, enums::Country, page::Page, playlist::SimplifiedPlaylist,
  show::SimplifiedShow, track::FullTrack, SimplifiedAlbum,
};
use rspotify::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ArtistSearchResponse {
  artists: Page<FullArtist>,
}

#[derive(Deserialize, Debug)]
struct TrackSearchResponse {
  tracks: Page<FullTrack>,
}

#[derive(Deserialize, Debug)]
struct AlbumSearchResponse {
  albums: Page<SimplifiedAlbum>,
}

#[derive(Deserialize, Debug)]
struct PlaylistSearchResponse {
  playlists: Page<SimplifiedPlaylist>,
}

#[derive(Deserialize, Debug)]
struct ShowSearchResponse {
  shows: Page<SimplifiedShow>,
}

pub trait SearchNetwork {
  async fn get_search_results(&mut self, search_term: String, country: Option<Country>);
  async fn search_tracks_for_playlist(&mut self, search_term: String);
}

impl SearchNetwork for Network {
  async fn get_search_results(&mut self, search_term: String, country: Option<Country>) {
    // Don't pass market to search - when market is specified, Spotify doesn't return
    // available_markets field, but rspotify 0.14 models require it for tracks/albums.
    // We'll handle null playlist fields by searching playlists separately without requiring all fields.
    let _country = country;

    let base_query = |search_type: &str| {
      vec![
        ("q", search_term.clone()),
        ("type", search_type.to_string()),
        ("limit", self.small_search_limit.to_string()),
        ("offset", "0".to_string()),
      ]
    };

    let track_query = base_query("track");
    let album_query = base_query("album");
    let playlist_query = base_query("playlist");
    let show_query = base_query("show");
    let artist_query = vec![
      ("q", search_term.clone()),
      ("type", "artist".to_string()),
      ("limit", self.small_search_limit.to_string()),
      ("offset", "0".to_string()),
    ];

    let (track_search, album_search, show_search, playlist_search, artist_search) = tokio::join!(
      self.spotify_get_typed::<TrackSearchResponse>("search", &track_query),
      self.spotify_get_typed::<AlbumSearchResponse>("search", &album_query),
      self.spotify_get_typed::<ShowSearchResponse>("search", &show_query),
      self.spotify_get_typed::<PlaylistSearchResponse>("search", &playlist_query),
      self.spotify_get_typed::<ArtistSearchResponse>("search", &artist_query)
    );

    let track_result = match track_search {
      Ok(res) => Some(res.tracks),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };
    let album_result = match album_search {
      Ok(res) => Some(res.albums),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };
    let show_result = match show_search {
      Ok(res) => Some(res.shows),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    let artist_result = artist_search.ok().map(|res| res.artists);

    // Handle playlist search separately since it can fail with null fields from Spotify API
    // Silently ignore playlist errors - this is a known Spotify API issue
    let playlist_result = playlist_search.ok().map(|res| res.playlists);

    let mut app = self.app.lock().await;

    // Extract ids for follow/saved checks from raw rspotify pages before conversion.
    if let Some(ref album_results) = album_result {
      let artist_ids = album_results
        .items
        .iter()
        .flat_map(|item| {
          item
            .artists
            .iter()
            .filter_map(|artist| artist.id.as_ref().map(|id| id.id().to_string()))
        })
        .collect();

      // Check if these artists are followed
      app.dispatch(IoEvent::UserArtistFollowCheck(artist_ids));

      let album_ids = album_results
        .items
        .iter()
        .filter_map(|album| album.id.as_ref().map(|id| id.id().to_string()))
        .collect();

      // Check if these albums are saved
      app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(album_ids));
    }

    if let Some(ref show_results) = show_result {
      let show_ids = show_results
        .items
        .iter()
        .map(|show| show.id.id().to_string())
        .collect();

      // check if these shows are saved
      app.dispatch(IoEvent::CurrentUserSavedShowsContains(show_ids));
    }

    // Convert rspotify pages to domain Paged<T> before storing on App.
    app.search_results.tracks = track_result
      .as_ref()
      .map(|p| map_page(p, |t| TrackInfo::from(t)));
    app.search_results.artists = artist_result
      .as_ref()
      .map(|p| map_page(p, |a| ArtistInfo::from(a)));
    app.search_results.albums = album_result
      .as_ref()
      .map(|p| map_page(p, |a| AlbumInfo::from(a)));
    app.search_results.playlists = playlist_result
      .as_ref()
      .map(|p| map_page(p, PlaylistInfo::from_simplified));
    app.search_results.shows = show_result
      .as_ref()
      .map(|p| map_page(p, |s| ShowInfo::from(s)));

    // A replaced page can be shorter than the previous one (or empty), which
    // would otherwise leave a stale selected-index pointing past the end of
    // the new results (panic-1: unchecked `.items[selected_index]` indexing
    // downstream). Clamp/reset every selected-index field to stay in range of
    // its freshly-replaced sibling page. Each page length is read first (ending
    // the shared borrow) so the mutable index borrow that follows doesn't
    // overlap it through the `App` guard's `Deref`.
    let tracks_len = page_len(&app.search_results.tracks);
    let artists_len = page_len(&app.search_results.artists);
    let albums_len = page_len(&app.search_results.albums);
    let playlists_len = page_len(&app.search_results.playlists);
    let shows_len = page_len(&app.search_results.shows);
    clamp_selected_index(&mut app.search_results.selected_tracks_index, tracks_len);
    clamp_selected_index(&mut app.search_results.selected_artists_index, artists_len);
    clamp_selected_index(&mut app.search_results.selected_album_index, albums_len);
    clamp_selected_index(
      &mut app.search_results.selected_playlists_index,
      playlists_len,
    );
    clamp_selected_index(&mut app.search_results.selected_shows_index, shows_len);
  }

  async fn search_tracks_for_playlist(&mut self, search_term: String) {
    let query = vec![
      ("q", search_term),
      ("type", "track".to_string()),
      ("limit", self.large_search_limit.to_string()),
      ("offset", "0".to_string()),
    ];

    let tracks = match self
      .spotify_get_typed::<TrackSearchResponse>("search", &query)
      .await
    {
      Ok(res) => res
        .tracks
        .items
        .iter()
        .filter(|t| t.id.is_some())
        .map(TrackInfo::from)
        .collect::<Vec<_>>(),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    let mut app = self.app.lock().await;
    app.create_playlist_search_results = tracks;
    app.create_playlist_selected_result = 0;
  }
}

/// Keep a `search_results.selected_*_index` field in range of its
/// freshly-replaced sibling page. A new search page can be shorter than (or
/// empty relative to) the page it replaces; without this, a stale index can
/// point past the end of the new `items` Vec, which panics wherever a caller
/// indexes directly (panic-1: `src/tui/handlers/search_results.rs`, the `D`
/// / unfollow-playlist handler).
fn page_len<T>(page: &Option<crate::core::pagination::Paged<T>>) -> usize {
  page.as_ref().map(|p| p.items.len()).unwrap_or(0)
}

fn clamp_selected_index(index: &mut Option<usize>, len: usize) {
  *index = match (*index, len) {
    (_, 0) => None,
    (Some(i), len) => Some(i.min(len - 1)),
    (None, _) => None,
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::pagination::Paged;

  fn page_of(len: usize) -> Option<Paged<u32>> {
    Some(Paged {
      items: (0..len as u32).collect(),
      total: len as u32,
      ..Default::default()
    })
  }

  /// Replacing a long page (e.g. 21 playlists, selected index 20) with a
  /// shorter one (e.g. 2 playlists) must clamp the selected index into range
  /// instead of leaving it pointing past the end — this is the root cause of
  /// panic-1 (unchecked `.items[selected_index]` indexing downstream).
  #[test]
  fn clamp_selects_last_valid_index_when_new_page_is_shorter() {
    let mut index = Some(20);
    let new_page = page_of(2);
    clamp_selected_index(&mut index, page_len(&new_page));
    assert_eq!(index, Some(1));
  }

  #[test]
  fn clamp_resets_to_none_when_new_page_is_empty() {
    let mut index = Some(20);
    let empty_page: Option<Paged<u32>> = page_of(0);
    clamp_selected_index(&mut index, page_len(&empty_page));
    assert_eq!(index, None);

    let mut index = Some(0);
    let none_page: Option<Paged<u32>> = None;
    clamp_selected_index(&mut index, page_len(&none_page));
    assert_eq!(index, None);
  }

  #[test]
  fn clamp_leaves_in_range_index_untouched() {
    let mut index = Some(1);
    let new_page = page_of(5);
    clamp_selected_index(&mut index, page_len(&new_page));
    assert_eq!(index, Some(1));
  }

  #[test]
  fn clamp_leaves_none_as_none_when_page_is_nonempty() {
    let mut index = None;
    let new_page = page_of(5);
    clamp_selected_index(&mut index, page_len(&new_page));
    assert_eq!(index, None);
  }
}
