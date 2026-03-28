use super::requests::spotify_get_typed_compat_for;
use super::{IoEvent, Network};
use anyhow::anyhow;
use rspotify::model::{
  artist::FullArtist,
  enums::{Country, Market, SearchType},
  idtypes::AlbumId,
  page::Page,
  search::SearchResult,
};
use rspotify::prelude::*;
use serde::Deserialize;
use tokio::try_join;

#[derive(Deserialize, Debug)]
pub struct ArtistSearchResponse {
  artists: Page<FullArtist>,
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
    let _market = country.map(Market::Country);

    let search_track = self.spotify.search(
      &search_term,
      SearchType::Track,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_album = self.spotify.search(
      &search_term,
      SearchType::Album,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_playlist = self.spotify.search(
      &search_term,
      SearchType::Playlist,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_show = self.spotify.search(
      &search_term,
      SearchType::Show,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let artist_query = vec![
      ("q", search_term.clone()),
      ("type", "artist".to_string()),
      ("limit", self.small_search_limit.to_string()),
      ("offset", "0".to_string()),
    ];

    // Run all futures concurrently
    let (main_search, playlist_search, artist_search) = tokio::join!(
      async { try_join!(search_track, search_album, search_show) },
      search_playlist,
      spotify_get_typed_compat_for::<ArtistSearchResponse>(&self.spotify, "search", &artist_query)
    );

    // Handle main search results
    let (track_result, album_result, show_result) = match main_search {
      Ok((
        SearchResult::Tracks(tracks),
        SearchResult::Albums(albums),
        SearchResult::Shows(shows),
      )) => (Some(tracks), Some(albums), Some(shows)),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
      _ => return,
    };

    let artist_result = artist_search.ok().map(|res| res.artists);

    // Handle playlist search separately since it can fail with null fields from Spotify API
    // Silently ignore playlist errors - this is a known Spotify API issue
    let playlist_result = match playlist_search {
      Ok(SearchResult::Playlists(playlists)) => Some(playlists),
      Err(_) => None,
      _ => None,
    };

    let mut app = self.app.lock().await;

    if let Some(ref album_results) = album_result {
      let artist_ids = album_results
        .items
        .iter()
        .flat_map(|item| {
          item
            .artists
            .iter()
            .filter_map(|artist| artist.id.as_ref().map(|id| id.to_owned().into_static()))
        })
        .collect();

      // Check if these artists are followed
      app.dispatch(IoEvent::UserArtistFollowCheck(artist_ids));

      let album_ids = album_results
        .items
        .iter()
        .filter_map(|album| {
          album
            .id
            .as_ref()
            .map(|id| AlbumId::from_id(id.id()).unwrap().into_static())
        })
        .collect();

      // Check if these albums are saved
      app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(album_ids));
    }

    if let Some(ref show_results) = show_result {
      let show_ids = show_results
        .items
        .iter()
        .map(|show| show.id.clone().into_static())
        .collect();

      // check if these shows are saved
      app.dispatch(IoEvent::CurrentUserSavedShowsContains(show_ids));
    }

    app.search_results.tracks = track_result;
    app.search_results.artists = artist_result;
    app.search_results.albums = album_result;
    app.search_results.playlists = playlist_result;
    app.search_results.shows = show_result;
  }

  async fn search_tracks_for_playlist(&mut self, search_term: String) {
    let result = self
      .spotify
      .search(
        &search_term,
        SearchType::Track,
        None,
        None,
        Some(self.large_search_limit),
        Some(0),
      )
      .await;

    let tracks = match result {
      Ok(SearchResult::Tracks(page)) => page
        .items
        .into_iter()
        .filter_map(|t| if t.id.is_some() { Some(t) } else { None })
        .collect::<Vec<_>>(),
      _ => return,
    };

    let mut app = self.app.lock().await;
    app.create_playlist_search_results = tracks;
    app.create_playlist_selected_result = 0;
  }
}
