use super::{ids, IoEvent, Network};
use crate::core::app::{
  ActiveBlock, AnalysisFetch, App, Artist, ArtistBlock, EpisodeTableContext, RouteId,
  SelectedFullShow, SelectedShow,
};
use crate::core::plugin_api::{AlbumInfo, ArtistInfo, EpisodeInfo, ShowInfo, TrackInfo};
use crate::infra::network::mapping::{self, map_page};
use crate::infra::network::requests::{
  is_forbidden_error, is_not_found_error, spotify_api_request_json_for,
};
use anyhow::anyhow;
use reqwest::Method;
use rspotify::model::{
  album::{FullAlbum, SimplifiedAlbum},
  artist::FullArtist,
  enums::Country,
  idtypes::{AlbumId, ArtistId, ShowId, TrackId},
  page::{CursorBasedPage, Page},
  track::{FullTrack, SimplifiedTrack},
};
use rspotify::prelude::*;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Minimal id-keyed TTL cache for metadata responses, so re-entering the same
/// album/artist from a list within the TTL doesn't pay a fresh round trip plus
/// the pacing tax. Deliberately tiny: no LRU crate, oldest-entry eviction at
/// capacity, entries expire on read.
pub(super) struct MetadataTtlCache<T> {
  entries: std::collections::HashMap<String, (std::time::Instant, T)>,
}

const METADATA_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);
const METADATA_CACHE_CAP: usize = 32;

impl<T> Default for MetadataTtlCache<T> {
  fn default() -> Self {
    Self {
      entries: std::collections::HashMap::new(),
    }
  }
}

impl<T: Clone> MetadataTtlCache<T> {
  pub(super) fn get(&mut self, key: &str) -> Option<T> {
    match self.entries.get(key) {
      Some((stored_at, value)) if stored_at.elapsed() < METADATA_CACHE_TTL => Some(value.clone()),
      Some(_) => {
        self.entries.remove(key);
        None
      }
      None => None,
    }
  }

  pub(super) fn put(&mut self, key: String, value: T) {
    if self.entries.len() >= METADATA_CACHE_CAP && !self.entries.contains_key(&key) {
      if let Some(oldest) = self
        .entries
        .iter()
        .min_by_key(|(_, (stored_at, _))| *stored_at)
        .map(|(k, _)| k.clone())
      {
        self.entries.remove(&oldest);
      }
    }
    self.entries.insert(key, (std::time::Instant::now(), value));
  }
}

/// The raw responses `get_artist` assembles, cached as one unit.
#[derive(Clone)]
pub(super) struct CachedArtistData {
  image_url: Option<String>,
  top_tracks: Vec<FullTrack>,
  related_artists: Vec<FullArtist>,
  album_items: Vec<SimplifiedAlbum>,
}

#[derive(Deserialize)]
struct ArtistTopTracksResponse {
  tracks: Vec<FullTrack>,
}

#[derive(Deserialize)]
struct RelatedArtistsResponse {
  artists: Vec<FullArtist>,
}

#[derive(Deserialize)]
struct FollowedArtistsResponse {
  artists: CursorBasedPage<FullArtist>,
}

fn country_code(country: Country) -> String {
  let code: &'static str = country.into();
  code.to_string()
}

/// Fetch an album's tracklist from `offset` onward, following pagination until
/// the API reports no further page. Spotify caps each response at 50 tracks,
/// so albums longer than that need this loop to come back complete.
pub(super) async fn fetch_album_tracks_from(
  network: &Network,
  album_id: &str,
  mut offset: u32,
) -> anyhow::Result<Vec<SimplifiedTrack>> {
  let path = format!("albums/{}/tracks", album_id);
  let limit = 50u32;
  let mut items = Vec::new();
  loop {
    let page = network
      .spotify_get_typed::<Page<SimplifiedTrack>>(
        &path,
        &[("limit", limit.to_string()), ("offset", offset.to_string())],
      )
      .await?;
    let has_next = page.next.is_some();
    items.extend(page.items);
    if !has_next {
      break;
    }
    offset += limit;
  }
  Ok(items)
}

pub trait MetadataNetwork {
  async fn get_artist(
    &mut self,
    artist_id: ArtistId<'static>,
    input_artist_name: String,
    country: Option<Country>,
  );
  async fn get_album_tracks(&mut self, album: Box<SimplifiedAlbum>);
  async fn get_album(&mut self, album_id: AlbumId<'static>);
  async fn get_show_episodes(&mut self, show: Box<ShowInfo>);
  async fn get_show(&mut self, show_id: ShowId<'static>);
  async fn get_current_show_episodes(&mut self, show_id: ShowId<'static>, offset: Option<u32>);
  async fn get_followed_artists(&mut self, after: Option<ArtistId<'static>>);
  async fn user_unfollow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>);
  async fn user_follow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>);
  async fn user_artist_check_follow(&mut self, artist_ids: Vec<ArtistId<'static>>);
  #[allow(dead_code)]
  async fn get_album_for_track(&mut self, track_id: TrackId<'static>);
}

impl MetadataNetwork for Network {
  async fn get_artist(
    &mut self,
    artist_id: ArtistId<'static>,
    input_artist_name: String,
    country: Option<Country>,
  ) {
    let artist_id_str = artist_id.id().to_string();
    let cache_key = format!("{}:{:?}", artist_id_str, country);

    let cached = self.artist_cache.get(&cache_key);
    let CachedArtistData {
      image_url,
      top_tracks,
      related_artists,
      album_items,
    } = if let Some(data) = cached {
      data
    } else {
      let artist_path = format!("artists/{}", artist_id.id());
      let top_tracks_path = format!("{}/top-tracks", artist_path);
      let related_artists_path = format!("{}/related-artists", artist_path);
      let mut top_tracks_query = Vec::new();
      if let Some(country) = country {
        top_tracks_query.push(("market", country_code(country)));
      }

      // A refused or missing half leaves its list empty and keeps the other.
      let (artist_res, top_tracks_res, related_artists_res) = tokio::join!(
        self.spotify_get_typed::<FullArtist>(&artist_path, &[]),
        self.spotify_get_typed::<ArtistTopTracksResponse>(&top_tracks_path, &top_tracks_query),
        self.spotify_get_typed::<RelatedArtistsResponse>(&related_artists_path, &[])
      );
      // The portrait is decoration: a failed fetch only leaves it out, and is not cached.
      let artist_fetched = artist_res.is_ok();
      let image_url = artist_res
        .ok()
        .and_then(|artist| ArtistInfo::from(&artist).image_url);

      let top_tracks = match top_tracks_res {
        Ok(res) => res.tracks,
        Err(e) if is_not_found_error(&e) || is_forbidden_error(&e) => Vec::new(),
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          return;
        }
      };

      let related_artists = match related_artists_res {
        Ok(res) => res.artists,
        Err(e) if is_not_found_error(&e) || is_forbidden_error(&e) => Vec::new(),
        Err(e) => {
          self.handle_error(e).await;
          return;
        }
      };

      let mut album_items = Vec::new();
      let mut offset = 0u32;
      // Spotify rejects a limit above 10 here with 400 "Invalid limit".
      let limit = 10u32;
      loop {
        let mut query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];
        if let Some(country) = country {
          query.push(("market", country_code(country)));
        }
        let page = match self
          .spotify_get_typed::<Page<SimplifiedAlbum>>(&format!("{}/albums", artist_path), &query)
          .await
        {
          Ok(page) => page,
          Err(e) => {
            self.handle_error(anyhow!(e)).await;
            return;
          }
        };
        let next = page.next.is_some();
        album_items.extend(page.items);
        if !next {
          break;
        }
        offset += limit;
      }

      let data = CachedArtistData {
        image_url,
        top_tracks,
        related_artists,
        album_items,
      };
      if artist_fetched {
        self.artist_cache.put(cache_key, data.clone());
      }
      data
    };

    // Convert rspotify types to domain types before storing — conversion stays
    // inside infra/network per the multi-source boundary contract.
    let albums_rspotify_page = Page {
      items: album_items,
      href: String::new(),
      limit: 50,
      next: None,
      offset: 0,
      previous: None,
      total: 0,
    };
    let albums = map_page(&albums_rspotify_page, |a| AlbumInfo::from(a));
    let domain_related_artists: Vec<ArtistInfo> =
      related_artists.iter().map(ArtistInfo::from).collect();
    let domain_top_tracks: Vec<TrackInfo> = top_tracks.iter().map(TrackInfo::from).collect();

    let mut app = self.app.lock().await;

    // Check if the top tracks are liked.
    let track_check = ids::track_check_ids(top_tracks.iter().map(|t| t.id.as_ref()));
    if !track_check.is_empty() {
      app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
    }

    // Check if the artist's albums are saved.
    let album_check: Vec<String> = albums_rspotify_page
      .items
      .iter()
      .filter_map(|album| album.id.as_ref().map(|id| id.id().to_string()))
      .collect();
    if !album_check.is_empty() {
      app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(album_check));
    }

    // Check if the artist and the related artists are followed.
    let follow_check: Vec<String> = std::iter::once(artist_id_str.clone())
      .chain(
        related_artists
          .iter()
          .map(|artist| artist.id.id().to_string()),
      )
      .collect();
    app.dispatch(IoEvent::UserArtistFollowCheck(follow_check));

    app.artist = Some(Artist {
      artist_id: artist_id_str,
      artist_name: input_artist_name,
      image_url,
      albums,
      related_artists: domain_related_artists,
      top_tracks: domain_top_tracks,
      selected_album_index: 0,
      selected_related_artist_index: 0,
      selected_top_track_index: 0,
      artist_selected_block: if top_tracks.is_empty() {
        ArtistBlock::Albums
      } else {
        ArtistBlock::TopTracks
      },
      artist_hovered_block: if top_tracks.is_empty() {
        ArtistBlock::Albums
      } else {
        ArtistBlock::TopTracks
      },
    });
    app.bump_display(crate::core::app::DisplayDomain::Artist);
    app.push_navigation_stack(RouteId::Artist, ActiveBlock::ArtistBlock);
  }

  async fn get_album_tracks(&mut self, album: Box<SimplifiedAlbum>) {
    let album_id = album.id.clone();
    if let Some(id) = album_id {
      let fetched = match self.album_tracks_cache.get(id.id()) {
        Some(tracks) => Ok(tracks),
        None => match fetch_album_tracks_from(self, id.id(), 0).await {
          Ok(tracks) => {
            self
              .album_tracks_cache
              .put(id.id().to_string(), tracks.clone());
            Ok(tracks)
          }
          Err(e) => Err(e),
        },
      };
      match fetched {
        Ok(track_items) => {
          let album_info = crate::core::plugin_api::AlbumInfo::from(album.as_ref());
          // Check if these tracks are liked (before `track_items` is consumed).
          let track_check = ids::track_check_ids(track_items.iter().map(|t| t.id.as_ref()));
          let total = track_items.len() as u32;
          let tracks = Page {
            items: track_items,
            href: String::new(),
            limit: total,
            next: None,
            offset: 0,
            previous: None,
            total,
          };
          let mut tracks_domain =
            map_page(&tracks, |t| crate::core::plugin_api::TrackInfo::from(t));
          // The album-tracks endpoint omits each track's album, so the
          // release-year column reads the date of the album opened here.
          for track in &mut tracks_domain.items {
            if track.release_date.is_none() {
              track.release_date = album_info.release_date.clone();
            }
          }
          let mut app = self.app.lock().await;
          if !track_check.is_empty() {
            app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
          }
          app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(vec![id
            .id()
            .to_string()]));
          app.selected_album_simplified = Some(crate::core::app::SelectedAlbum {
            album: album_info,
            tracks: tracks_domain,
            selected_index: 0,
          });
          app.album_table_context = crate::core::app::AlbumTableContext::Simplified;
          app.bump_display(crate::core::app::DisplayDomain::Album);
          app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
        }
        Err(e) => self.handle_error(anyhow!(e)).await,
      }
    }
  }

  async fn get_album(&mut self, album_id: AlbumId<'static>) {
    let fetched = if let Some(album) = self.album_cache.get(album_id.id()) {
      Ok(album)
    } else {
      self
        .spotify_get_typed::<FullAlbum>(&format!("albums/{}", album_id.id()), &[])
        .await
    };
    match fetched {
      Ok(mut album) => {
        // A full album embeds only the first page of its tracklist (50 tracks
        // max); fetch the rest so longer albums render completely.
        if album.tracks.next.is_some() {
          let offset = album.tracks.items.len() as u32;
          match fetch_album_tracks_from(self, album_id.id(), offset).await {
            Ok(rest) => {
              album.tracks.items.extend(rest);
              // Mark the tracklist complete so a cache hit doesn't re-page.
              album.tracks.next = None;
            }
            Err(e) => {
              self.handle_error(anyhow!(e)).await;
              return;
            }
          }
        }
        self
          .album_cache
          .put(album_id.id().to_string(), album.clone());
        let album_info = crate::core::plugin_api::AlbumInfo::from(&album);
        let mut app = self.app.lock().await;
        // Check if these tracks are liked.
        let track_check = ids::track_check_ids(album.tracks.items.iter().map(|t| t.id.as_ref()));
        if !track_check.is_empty() {
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
        }
        app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(vec![album_id
          .id()
          .to_string()]));
        app.selected_album_full = Some(crate::core::app::SelectedFullAlbum {
          album: album_info,
          selected_index: 0,
        });
        app.album_table_context = crate::core::app::AlbumTableContext::Full;
        app.bump_display(crate::core::app::DisplayDomain::Album);
        app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn get_show_episodes(&mut self, show: Box<ShowInfo>) {
    let Some(show_id) = show.id.as_deref().and_then(|id| ShowId::from_id(id).ok()) else {
      return;
    };
    let path = format!("shows/{}/episodes", show_id.id());
    let query = vec![
      ("limit", self.large_search_limit.to_string()),
      ("offset", "0".to_string()),
    ];
    match self
      .spotify_get_typed::<Page<rspotify::model::show::SimplifiedEpisode>>(&path, &query)
      .await
    {
      Ok(episodes) => {
        let domain_page = map_page(&episodes, |e| EpisodeInfo::from(e));
        let mut app = self.app.lock().await;
        app.dispatch(IoEvent::CurrentUserSavedShowsContains(vec![show_id
          .id()
          .to_string()]));
        let empty = domain_page.items.is_empty();
        app.store_show_episodes_page(domain_page, true);
        app.selected_show_simplified = Some(SelectedShow { show: *show });
        app.episode_table_context = EpisodeTableContext::Simplified;
        if !empty {
          app.push_navigation_stack(RouteId::PodcastEpisodes, ActiveBlock::EpisodeTable);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_show(&mut self, show_id: ShowId<'static>) {
    let path = format!("shows/{}", show_id.id());
    match self
      .spotify_get_typed::<rspotify::model::show::FullShow>(&path, &[])
      .await
    {
      Ok(show) => {
        let selected_show = SelectedFullShow {
          show: ShowInfo::from(&show),
        };

        let mut app = self.app.lock().await;

        app.selected_show_full = Some(selected_show);

        app.episode_table_context = EpisodeTableContext::Full;
        // The episodes in the cache belong to the show opened before this one.
        let id = show_id.id().to_string();
        app.clear_show_episodes();
        app.dispatch(IoEvent::GetCurrentShowEpisodes(id.clone(), None));
        app.dispatch(IoEvent::CurrentUserSavedShowsContains(vec![id]));
        app.push_navigation_stack(RouteId::PodcastEpisodes, ActiveBlock::EpisodeTable);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_show_episodes(&mut self, show_id: ShowId<'static>, offset: Option<u32>) {
    let path = format!("shows/{}/episodes", show_id.id());
    let mut query = vec![("limit", self.large_search_limit.to_string())];
    if let Some(offset) = offset {
      query.push(("offset", offset.to_string()));
    }

    match self
      .spotify_get_typed::<Page<rspotify::model::show::SimplifiedEpisode>>(&path, &query)
      .await
    {
      Ok(episodes) => {
        let first = offset.unwrap_or(0) == 0;
        // An empty first page still lands, so a show with no episodes stops loading.
        if first || !episodes.items.is_empty() {
          let domain_page = map_page(&episodes, |e| EpisodeInfo::from(e));
          let mut app = self.app.lock().await;
          // Another show opened while this page was in flight.
          if app.selected_show_id() != Some(show_id.id()) {
            return;
          }
          app.store_show_episodes_page(domain_page, first);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_followed_artists(&mut self, after: Option<ArtistId<'static>>) {
    let limit = self.large_search_limit;
    let mut query = vec![("type", "artist".to_string()), ("limit", limit.to_string())];
    if let Some(after) = after.as_ref() {
      query.push(("after", after.id().to_string()));
    }

    match self
      .spotify_get_typed::<FollowedArtistsResponse>("me/following", &query)
      .await
    {
      Ok(res) => {
        let domain_page =
          crate::infra::network::mapping::map_cursor_page(&res.artists, |a| ArtistInfo::from(a));
        let mut app = self.app.lock().await;
        let artists = &mut app.library_mut().saved_artists;
        if after.is_none() {
          artists.clear();
        }
        artists.add_pages(domain_page);
        app.set_saved_artists_to_table_continuous();
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.clear_saved_artists_in_flight();
        drop(app);
        self.handle_error(anyhow!(e)).await;
      }
    }
  }
  async fn user_unfollow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>) {
    let uris: Vec<String> = artist_ids.iter().map(|id| id.uri()).collect();
    if let Err(e) = self.library_remove_uris(&uris).await {
      self.handle_error(e).await;
      return;
    }
    let mut app = self.app.lock().await;
    let followed = app.followed_artist_ids_set_mut();
    for id in &artist_ids {
      followed.remove(id.id());
    }
    app.dispatch(IoEvent::GetFollowedArtists(None));
  }

  async fn user_follow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>) {
    let uris: Vec<String> = artist_ids.iter().map(|id| id.uri()).collect();
    if let Err(e) = self.library_save_uris(&uris).await {
      self.handle_error(e).await;
      return;
    }
    let mut app = self.app.lock().await;
    let followed = app.followed_artist_ids_set_mut();
    for id in &artist_ids {
      followed.insert(id.id().to_string());
    }
    app.dispatch(IoEvent::GetFollowedArtists(None));
  }

  async fn user_artist_check_follow(&mut self, artist_ids: Vec<ArtistId<'static>>) {
    let uris: Vec<String> = artist_ids.iter().map(|id| id.uri()).collect();
    match self.library_contains_uris(&uris).await {
      Ok(is_following) => {
        let mut app = self.app.lock().await;
        let followed = app.followed_artist_ids_set_mut();
        for (id, is_following) in artist_ids.iter().zip(is_following) {
          if is_following {
            followed.insert(id.id().to_string());
          } else {
            followed.remove(id.id());
          }
        }
      }
      // Bookkeeping for the follow marker: a failed check must not take over
      // the screen.
      Err(e) => log::debug!("artist follow check failed: {e}"),
    }
  }

  async fn get_album_for_track(&mut self, track_id: TrackId<'static>) {
    match self
      .spotify_get_typed::<FullTrack>(&format!("tracks/{}", track_id.id()), &[])
      .await
    {
      Ok(track) => {
        // FullTrack.album is SimplifiedAlbum (not Option) in rspotify 0.14
        let album = track.album;
        self.get_album_tracks(Box::new(album)).await;
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }
}

/// Fetches the audio analysis of `track_id` off the pump. Never the error screen:
/// the pitch view shows its own empty state.
pub(super) async fn fetch_audio_analysis(app: Arc<Mutex<App>>, track_id: String) {
  if !app.lock().await.audio_analysis_pending(&track_id) {
    return;
  }
  let path = format!("audio-analysis/{track_id}");
  let fetch = match spotify_api_request_json_for(Method::GET, &path, &[], None).await {
    Ok(value) => match mapping::track_analysis(value) {
      Ok(analysis) => AnalysisFetch::Found(analysis),
      Err(e) => {
        log::warn!("unreadable audio analysis for {track_id}: {e:#}");
        AnalysisFetch::Missing
      }
    },
    Err(e) if is_not_found_error(&e) => AnalysisFetch::Missing,
    Err(_) => AnalysisFetch::Failed,
  };
  app.lock().await.set_audio_analysis(&track_id, fetch);
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use crate::core::app::{App, RouteId};
  use crate::core::user_config::UserConfig;
  use rspotify::model::idtypes::ArtistId;
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::TcpListener;
  use tokio::sync::Mutex;

  /// A login whose Spotify calls go to `base_url` while the guard lives.
  pub(crate) fn fake_spotify_login(
    base_url: String,
  ) -> (
    crate::core::auth::SpotifyLogin,
    crate::infra::network::requests::fake::FakeSpotify,
  ) {
    (
      crate::core::auth::SpotifyLogin::new(),
      crate::infra::network::requests::fake::use_fake_spotify(base_url),
    )
  }
  pub(crate) async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut buf = vec![0; 4096];
    let n = stream.read(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf[..n]).to_string()
  }

  const ARTIST_JSON: &str = r#"{"external_urls":{},"followers":{"href":null,"total":0},"genres":[],"href":"","id":"artist","images":[{"url":"https://i.scdn.co/image/portrait","width":640,"height":640}],"name":"Test Artist","popularity":0}"#;

  async fn get_artist_recovers_from_related_artists(
    related_status: &'static str,
    related_status_code: u16,
  ) {
    // Generous: every HTTP test shares the API pacing limiter.
    tokio::time::timeout(Duration::from_secs(30), async {
      let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
      let local_addr = listener.local_addr().unwrap();
      let base_url = format!("http://{local_addr}/v1/");

      let server = tokio::spawn(async move {
        for _ in 0..4 {
          let (mut stream, _) = listener.accept().await.unwrap();
          let request = read_http_request(&mut stream).await;

          let (status, body) = if request.contains("/top-tracks") {
            ("200 OK", r#"{"tracks":[]}"#.to_string())
          } else if request.contains("/related-artists") {
            (
              related_status,
              format!(
                r#"{{"error":{{"status":{related_status_code},"message":"mock error"}}}}"#
              ),
            )
          } else if request.contains("/albums") {
            (
              "200 OK",
              r#"{"items":[],"total":0,"limit":50,"offset":0,"href":"","next":null,"previous":null}"#
                .to_string(),
            )
          } else if request.lines().next().unwrap_or_default().contains("/artists/artist ") {
            ("200 OK", ARTIST_JSON.to_string())
          } else {
            panic!("unexpected request: {request}");
          };

          let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
          );
          stream.write_all(response.as_bytes()).await.unwrap();
        }
      });

      let (tx, _rx) = std::sync::mpsc::channel();
      let app = Arc::new(Mutex::new(App::new(
        tx,
        UserConfig::new(),
        Some(std::time::SystemTime::now()),
      )));

      let (spotify, _fake) = fake_spotify_login(base_url);

      let mut network = Network::new(Some(spotify), &app);

      let artist_id = ArtistId::from_id("artist").unwrap();
      let before = app
        .lock()
        .await
        .display_revisions()
        .get(crate::core::app::DisplayDomain::Artist);

      network
        .get_artist(artist_id, "Test Artist".to_string(), None)
        .await;

      server.await.unwrap();

      let app_guard = app.lock().await;

      let artist = app_guard
        .artist
        .as_ref()
        .expect("app.artist should be populated for a recoverable related-artists error");
      assert_eq!(artist.artist_name, "Test Artist");
      assert_eq!(
        artist.image_url.as_deref(),
        Some("https://i.scdn.co/image/portrait")
      );
      assert!(
        app_guard
          .display_revisions()
          .get(crate::core::app::DisplayDomain::Artist)
          > before
      );
      assert!(
        artist.related_artists.is_empty(),
        "related_artists should be empty when error is recovered"
      );

      assert_eq!(app_guard.get_current_route().id, RouteId::Artist);
      assert_ne!(app_guard.get_current_route().id, RouteId::Error);
    })
      .await
      .expect("test timed out");
  }

  #[tokio::test]
  async fn get_artist_recovers_from_related_artists_404() {
    get_artist_recovers_from_related_artists("404 Not Found", 404).await;
  }

  #[tokio::test]
  async fn get_artist_recovers_from_related_artists_403() {
    get_artist_recovers_from_related_artists("403 Forbidden", 403).await;
  }

  fn album_json(n: u32) -> String {
    format!(
      r#"{{"album_type":"album","artists":[],"external_urls":{{}},"href":null,"id":"album{n}","images":[],"name":"Album {n}"}}"#
    )
  }

  #[tokio::test]
  async fn get_artist_pages_albums_ten_at_a_time_and_keeps_every_album() {
    tokio::time::timeout(Duration::from_secs(10), async {
      let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
      let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());

      let server = tokio::spawn(async move {
        for _ in 0..5 {
          let (mut stream, _) = listener.accept().await.unwrap();
          let request = read_http_request(&mut stream).await;
          let request_line = request.lines().next().unwrap_or_default().to_string();

          let (status, body) = if request_line.contains("/top-tracks") {
            ("200 OK", r#"{"tracks":[]}"#.to_string())
          } else if request_line.contains("/related-artists") {
            ("200 OK", r#"{"artists":[]}"#.to_string())
          } else if request_line.contains("/albums") {
            let query = request_line
              .split_whitespace()
              .nth(1)
              .and_then(|path| path.split_once('?'))
              .map_or("", |(_, query)| query);
            let param = |key: &str| {
              query
                .split('&')
                .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
            };
            match (param("limit"), param("offset")) {
              (Some("10"), Some("0")) => {
                let items: Vec<String> = (0..10).map(album_json).collect();
                (
                  "200 OK",
                  format!(
                    r#"{{"items":[{}],"total":12,"limit":10,"offset":0,"href":"","next":"more","previous":null}}"#,
                    items.join(",")
                  ),
                )
              }
              (Some("10"), Some("10")) => {
                let items: Vec<String> = (10..12).map(album_json).collect();
                (
                  "200 OK",
                  format!(
                    r#"{{"items":[{}],"total":12,"limit":10,"offset":10,"href":"","next":null,"previous":null}}"#,
                    items.join(",")
                  ),
                )
              }
              _ => (
                "400 Bad Request",
                r#"{"error":{"status":400,"message":"Invalid limit"}}"#.to_string(),
              ),
            }
          } else if request_line.contains("/artists/artist ") {
            ("200 OK", ARTIST_JSON.to_string())
          } else {
            panic!("unexpected request: {request}");
          };

          let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
          );
          stream.write_all(response.as_bytes()).await.unwrap();
        }
      });

      let (tx, _rx) = std::sync::mpsc::channel();
      let app = Arc::new(Mutex::new(App::new(
        tx,
        UserConfig::new(),
        Some(std::time::SystemTime::now()),
      )));

      let (spotify, _fake) = fake_spotify_login(base_url);
      let mut network = Network::new(Some(spotify), &app);

      network
        .get_artist(ArtistId::from_id("artist").unwrap(), "Test Artist".to_string(), None)
        .await;

      server.await.unwrap();

      let app_guard = app.lock().await;
      assert_eq!(app_guard.get_current_route().id, RouteId::Artist);
      let artist = app_guard
        .artist
        .as_ref()
        .expect("the artist page must open");
      assert_eq!(artist.albums.items.len(), 12);
    })
    .await
    .expect("test timed out");
  }

  /// Fetches the analysis of `track1` from a server that answers once with `status` and `body`.
  async fn audio_analysis_from(status: &'static str, body: &'static str) -> App {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let request = read_http_request(&mut stream).await;
      assert!(request.contains("/audio-analysis/track1"), "{request}");
      let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    });
    let (tx, _rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::new(
      tx,
      UserConfig::new(),
      Some(std::time::SystemTime::now()),
    )));
    app.lock().await.want_audio_analysis(Some("track1"));
    let (_spotify, _fake) = fake_spotify_login(base_url);

    fetch_audio_analysis(Arc::clone(&app), "track1".to_string()).await;

    server.await.unwrap();
    Arc::try_unwrap(app).ok().unwrap().into_inner()
  }

  #[tokio::test]
  async fn an_audio_analysis_reply_lands_for_the_wanted_track() {
    let app = tokio::time::timeout(
      Duration::from_secs(30),
      audio_analysis_from("200 OK", r#"{"sections":[],"segments":[{"start":0.5}]}"#),
    )
    .await
    .expect("test timed out");

    match app.audio_analysis() {
      crate::core::app::AnalysisState::Ready(id, analysis) => {
        assert_eq!(id, "track1");
        assert_eq!(analysis.segments[0].start_ms, 500);
      }
      other => panic!("expected the analysis, got {other:?}"),
    }
  }

  #[tokio::test]
  async fn a_missing_audio_analysis_marks_the_track_without_opening_the_error_screen() {
    let app = tokio::time::timeout(
      Duration::from_secs(30),
      audio_analysis_from(
        "404 Not Found",
        r#"{"error":{"status":404,"message":"not found"}}"#,
      ),
    )
    .await
    .expect("test timed out");

    assert_eq!(
      *app.audio_analysis(),
      crate::core::app::AnalysisState::Unavailable("track1".into())
    );
    assert!(app.api_error().is_empty());
    assert_ne!(app.get_current_route().id, RouteId::Error);
  }

  #[tokio::test]
  async fn a_failed_audio_analysis_fetch_is_not_read_as_a_missing_analysis() {
    let app = tokio::time::timeout(
      Duration::from_secs(30),
      audio_analysis_from(
        "503 Service Unavailable",
        r#"{"error":{"status":503,"message":"down"}}"#,
      ),
    )
    .await
    .expect("test timed out");

    assert!(matches!(
      app.audio_analysis(),
      crate::core::app::AnalysisState::Failed(id, _) if id == "track1"
    ));
    assert!(app.api_error().is_empty());
  }
}
