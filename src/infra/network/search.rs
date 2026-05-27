use super::{IoEvent, Network};
use anyhow::anyhow;
use rspotify::model::{
  artist::FullArtist, enums::Country, idtypes::AlbumId, page::Page, playlist::SimplifiedPlaylist,
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
        .into_iter()
        .filter_map(|t| if t.id.is_some() { Some(t) } else { None })
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
