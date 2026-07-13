use super::{ids, IoEvent, Network};
use crate::core::app::{
  ActiveBlock, Artist, ArtistBlock, EpisodeTableContext, RouteId, ScrollableResultPages,
  SelectedFullShow, SelectedShow,
};
use crate::core::plugin_api::{AlbumInfo, ArtistInfo, EpisodeInfo, ShowInfo, TrackInfo};
use crate::infra::network::mapping::map_page;
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
async fn fetch_album_tracks_from(
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

      let (top_tracks_res, related_artists_res) = tokio::join!(
        self.spotify_get_typed::<ArtistTopTracksResponse>(&top_tracks_path, &top_tracks_query),
        self.spotify_get_typed::<RelatedArtistsResponse>(&related_artists_path, &[])
      );

      let top_tracks = match top_tracks_res {
        Ok(res) => res.tracks,
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          return;
        }
      };
      let related_artists = match related_artists_res {
        Ok(res) => res.artists,
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          return;
        }
      };

      let mut album_items = Vec::new();
      let mut offset = 0u32;
      let limit = 50u32;
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
        top_tracks,
        related_artists,
        album_items,
      };
      self.artist_cache.put(cache_key, data.clone());
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

    // Check if the related artists are followed.
    let follow_check: Vec<String> = related_artists
      .iter()
      .map(|artist| artist.id.id().to_string())
      .collect();
    if !follow_check.is_empty() {
      app.dispatch(IoEvent::UserArtistFollowCheck(follow_check));
    }

    app.artist = Some(Artist {
      artist_id: artist_id_str,
      artist_name: input_artist_name,
      albums,
      related_artists: domain_related_artists,
      top_tracks: domain_top_tracks,
      selected_album_index: 0,
      selected_related_artist_index: 0,
      selected_top_track_index: 0,
      artist_selected_block: ArtistBlock::TopTracks,
      artist_hovered_block: ArtistBlock::TopTracks,
    });
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
          let tracks_domain = map_page(&tracks, |t| crate::core::plugin_api::TrackInfo::from(t));
          let mut app = self.app.lock().await;
          if !track_check.is_empty() {
            app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
          }
          app.selected_album_simplified = Some(crate::core::app::SelectedAlbum {
            album: album_info,
            tracks: tracks_domain,
            selected_index: 0,
          });
          app.album_table_context = crate::core::app::AlbumTableContext::Simplified;
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
        app.selected_album_full = Some(crate::core::app::SelectedFullAlbum {
          album: album_info,
          selected_index: 0,
        });
        app.album_table_context = crate::core::app::AlbumTableContext::Full;
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
        if !episodes.items.is_empty() {
          let domain_page = map_page(&episodes, |e| EpisodeInfo::from(e));
          let mut app = self.app.lock().await;
          app.library.show_episodes = ScrollableResultPages::new();
          app.library.show_episodes.add_pages(domain_page);

          app.selected_show_simplified = Some(SelectedShow { show: *show });

          app.episode_table_context = EpisodeTableContext::Simplified;

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
        if !episodes.items.is_empty() {
          let domain_page = map_page(&episodes, |e| EpisodeInfo::from(e));
          let mut app = self.app.lock().await;
          app.library.show_episodes.add_pages(domain_page);
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
        app.library.saved_artists.add_pages(domain_page);
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn user_unfollow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>) {
    let ids = artist_ids
      .iter()
      .map(|id| id.id().to_string())
      .collect::<Vec<_>>()
      .join(",");
    match self
      .spotify_api_request_json(
        Method::DELETE,
        "me/following",
        &[("type", "artist".to_string()), ("ids", ids)],
        None,
      )
      .await
    {
      Ok(_) => {
        // Handled
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn user_follow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>) {
    let ids = artist_ids
      .iter()
      .map(|id| id.id().to_string())
      .collect::<Vec<_>>()
      .join(",");
    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/following",
        &[("type", "artist".to_string()), ("ids", ids)],
        None,
      )
      .await
    {
      Ok(_) => {
        // Handled
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn user_artist_check_follow(&mut self, artist_ids: Vec<ArtistId<'static>>) {
    match self
      .spotify_get_typed::<Vec<bool>>(
        "me/following/contains",
        &[
          ("type", "artist".to_string()),
          (
            "ids",
            artist_ids
              .iter()
              .map(|id| id.id().to_string())
              .collect::<Vec<_>>()
              .join(","),
          ),
        ],
      )
      .await
    {
      Ok(is_following) => {
        let mut app = self.app.lock().await;
        for (i, is_following) in is_following.iter().enumerate() {
          if *is_following {
            app
              .followed_artist_ids_set
              .insert(artist_ids[i].id().to_string());
          }
        }
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
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
