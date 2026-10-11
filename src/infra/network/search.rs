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

/// `pub(crate)` so the DJ resolver can reuse the track-search shape: unlike the
/// handlers in this module, it needs a *returned* result rather than a write into
/// `App` (see `crate::infra::dj::resolve`).
#[derive(Deserialize, Debug)]
pub(crate) struct TrackSearchResponse {
  pub(crate) tracks: Page<FullTrack>,
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

/// The `limit` ceiling of the search endpoint for Development Mode apps: new
/// apps since February 2026, existing apps since March 2026. The library
/// endpoints still take 50.
pub(crate) const SPOTIFY_SEARCH_LIMIT: u32 = 10;

fn search_limit(requested: u32) -> String {
  requested.min(SPOTIFY_SEARCH_LIMIT).to_string()
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
        ("limit", search_limit(self.small_search_limit)),
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
      ("limit", search_limit(self.small_search_limit)),
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
    if let Some(ref track_results) = track_result {
      let track_ids = track_results
        .items
        .iter()
        .filter_map(|track| track.id.as_ref().map(|id| id.id().to_string()))
        .collect();

      // Check if these tracks are liked
      app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
    }

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
    app.set_search_results(crate::core::app::SearchResult {
      tracks: track_result
        .as_ref()
        .map(|p| map_page(p, |t| TrackInfo::from(t))),
      artists: artist_result
        .as_ref()
        .map(|p| map_page(p, |a| ArtistInfo::from(a))),
      albums: album_result
        .as_ref()
        .map(|p| map_page(p, |a| AlbumInfo::from(a))),
      playlists: playlist_result
        .as_ref()
        .map(|p| map_page(p, PlaylistInfo::from_simplified)),
      shows: show_result
        .as_ref()
        .map(|p| map_page(p, |s| ShowInfo::from(s))),
      query: Some(search_term.clone()),
    });
    app
      .plugin_data_generations
      .bump(crate::core::app::PluginDataKind::Search);
  }

  async fn search_tracks_for_playlist(&mut self, search_term: String) {
    let query = vec![
      ("q", search_term),
      ("type", "track".to_string()),
      ("limit", search_limit(self.large_search_limit)),
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
    app.view.create_playlist_selected_result = 0;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::config::ClientConfig;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::metadata::tests::{read_http_request, spotify_with_access_token};
  use std::sync::Arc;
  use tokio::io::AsyncWriteExt;
  use tokio::sync::Mutex;

  #[test]
  fn a_tall_terminal_search_limit_is_capped_at_the_spotify_ceiling() {
    assert_eq!(search_limit(50), "10");
    assert_eq!(search_limit(4), "4");
  }

  #[tokio::test]
  async fn a_frontend_that_sets_no_limits_searches_at_the_spotify_ceiling() {
    crate::infra::network::requests::SKIP_PACING.with(|skip| skip.set(true));
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
      let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
      let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
      let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
      let log = seen.clone();
      let server = tokio::spawn(async move {
        loop {
          let (mut stream, _) = listener.accept().await.unwrap();
          let request = read_http_request(&mut stream).await;
          let line = request.lines().next().unwrap_or_default().to_string();
          log.lock().unwrap().push(line);
          stream
            .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
            .await
            .unwrap();
        }
      });
      let (tx, _rx) = std::sync::mpsc::channel();
      let mut app = App::new(tx, UserConfig::new(), Some(std::time::SystemTime::now()));
      let state_dir = tempfile::tempdir().unwrap();
      app.state_path = Some(state_dir.path().join("state.yml"));
      let app = Arc::new(Mutex::new(app));
      let spotify = spotify_with_access_token("test_token", base_url).await;
      let mut network = Network::new(
        Some(spotify),
        ClientConfig::new(),
        &app,
        std::path::PathBuf::new(),
      );

      network.get_search_results("queen".to_string(), None).await;
      server.abort();

      let seen = seen.lock().unwrap();
      let searches: Vec<_> = seen
        .iter()
        .filter(|line| line.contains("/v1/search"))
        .collect();
      assert_eq!(searches.len(), 5, "{seen:?}");
      assert!(
        searches.iter().all(|line| line.contains("limit=10")),
        "{seen:?}"
      );
    })
    .await
    .expect("search limit test timed out");
  }
}
