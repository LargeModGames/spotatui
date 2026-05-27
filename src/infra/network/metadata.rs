use super::Network;
use crate::core::app::{
  ActiveBlock, Artist, ArtistBlock, EpisodeTableContext, RouteId, ScrollableResultPages,
  SelectedFullShow, SelectedShow,
};
use anyhow::anyhow;
use reqwest::Method;
use rspotify::model::{
  album::{FullAlbum, SimplifiedAlbum},
  artist::FullArtist,
  enums::Country,
  idtypes::{AlbumId, ArtistId, ShowId, TrackId},
  page::{CursorBasedPage, Page},
  show::SimplifiedShow,
  track::FullTrack,
};
use rspotify::prelude::*;
use serde::Deserialize;

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

pub trait MetadataNetwork {
  async fn get_artist(
    &mut self,
    artist_id: ArtistId<'static>,
    input_artist_name: String,
    country: Option<Country>,
  );
  async fn get_album_tracks(&mut self, album: Box<SimplifiedAlbum>);
  async fn get_album(&mut self, album_id: AlbumId<'static>);
  async fn get_show_episodes(&mut self, show: Box<SimplifiedShow>);
  async fn get_show(&mut self, show_id: ShowId<'static>);
  async fn get_current_show_episodes(&mut self, show_id: ShowId<'static>, offset: Option<u32>);
  async fn get_followed_artists(&mut self, after: Option<ArtistId<'static>>);
  async fn user_unfollow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>);
  async fn user_follow_artists(&mut self, artist_ids: Vec<ArtistId<'static>>);
  async fn user_artist_check_follow(&mut self, artist_ids: Vec<ArtistId<'static>>);
  async fn set_artists_to_table(&mut self, artists: Vec<FullArtist>);
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

    let albums = Page {
      items: album_items,
      href: String::new(),
      limit,
      next: None,
      offset: 0,
      previous: None,
      total: 0,
    };

    let mut app = self.app.lock().await;
    app.artist = Some(Artist {
      artist_id: artist_id_str,
      artist_name: input_artist_name,
      albums,
      related_artists,
      top_tracks,
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
      let path = format!("albums/{}/tracks", id.id());
      // TODO: Handle pagination for albums with > 50 tracks
      match self
        .spotify_get_typed::<Page<rspotify::model::track::SimplifiedTrack>>(
          &path,
          &[("limit", "50".to_string()), ("offset", "0".to_string())],
        )
        .await
      {
        Ok(tracks) => {
          let mut app = self.app.lock().await;
          app.selected_album_simplified = Some(crate::core::app::SelectedAlbum {
            album: *album,
            tracks,
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
    match self
      .spotify_get_typed::<FullAlbum>(&format!("albums/{}", album_id.id()), &[])
      .await
    {
      Ok(album) => {
        let mut app = self.app.lock().await;
        app.selected_album_full = Some(crate::core::app::SelectedFullAlbum {
          album,
          selected_index: 0,
        });
        app.album_table_context = crate::core::app::AlbumTableContext::Full;
        app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn get_show_episodes(&mut self, show: Box<SimplifiedShow>) {
    let show_id = show.id.clone();
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
          let mut app = self.app.lock().await;
          app.library.show_episodes = ScrollableResultPages::new();
          app.library.show_episodes.add_pages(episodes);

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
        let selected_show = SelectedFullShow { show };

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
          let mut app = self.app.lock().await;
          app.library.show_episodes.add_pages(episodes);
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
        let mut app = self.app.lock().await;
        app.library.saved_artists.add_pages(res.artists);
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

  async fn set_artists_to_table(&mut self, artists: Vec<FullArtist>) {
    let mut app = self.app.lock().await;
    app.artists = artists;
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
