use super::requests::is_rate_limited_error;
use super::Network;
use crate::core::app::{ActiveBlock, DiscoverTimeRange, RouteId};
use anyhow::anyhow;

use rand::seq::SliceRandom;
use rspotify::model::{
  artist::FullArtist,
  device::DevicePayload,
  page::{CursorBasedPage, Page},
  playing::PlayHistory,
  track::FullTrack,
  user::PrivateUser,
};
use rspotify::prelude::*;
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct ArtistTopTracksResponse {
  tracks: Vec<FullTrack>,
}

pub trait UserNetwork {
  async fn get_user(&mut self);
  async fn get_devices(&mut self);
  async fn get_user_top_tracks(&mut self, time_range: DiscoverTimeRange);
  async fn get_top_artists_mix(&mut self);
  #[allow(dead_code)]
  async fn get_recently_played(&mut self);
}

impl UserNetwork for Network {
  async fn get_user(&mut self) {
    match self.spotify_get_typed::<PrivateUser>("me", &[]).await {
      Ok(user) => {
        let mut app = self.app.lock().await;
        app.user = Some(user);
      }
      Err(e) => {
        let err = anyhow!(e);
        if is_rate_limited_error(&err) {
          let mut app = self.app.lock().await;
          app.status_message = Some(
            "Spotify rate limit hit while loading profile. Retrying automatically.".to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(6));
          return;
        }
        self.handle_error(err).await;
      }
    }
  }

  async fn get_devices(&mut self) {
    if let Ok(result) = self
      .spotify_get_typed::<DevicePayload>("me/player/devices", &[])
      .await
    {
      let mut app = self.app.lock().await;
      app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
      if !result.devices.is_empty() {
        app.devices = Some(result);
        // Select the first device in the list
        app.selected_device_index = Some(0);
      }
    }
  }

  async fn get_user_top_tracks(&mut self, time_range: DiscoverTimeRange) {
    let range_str = match time_range {
      DiscoverTimeRange::Short => "short_term",
      DiscoverTimeRange::Medium => "medium_term",
      DiscoverTimeRange::Long => "long_term",
    };

    // Set loading state
    {
      let mut app = self.app.lock().await;
      app.discover_loading = true;
    }

    match self
      .spotify_get_typed::<Page<FullTrack>>(
        "me/top/tracks",
        &[
          ("time_range", range_str.to_string()),
          ("limit", "50".to_string()),
        ],
      )
      .await
    {
      Ok(page) => {
        let mut app = self.app.lock().await;
        app.discover_top_tracks = page.items;
        app.discover_loading = false;
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.discover_loading = false;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn get_top_artists_mix(&mut self) {
    // Set loading state
    {
      let mut app = self.app.lock().await;
      app.discover_loading = true;
    }

    // 1. Get top artists
    let artists_res = self
      .spotify_get_typed::<Page<FullArtist>>(
        "me/top/artists",
        &[("limit", "5".to_string())], // Get top 5 artists
      )
      .await;

    let artists = match artists_res {
      Ok(page) => page.items,
      Err(e) => {
        let mut app = self.app.lock().await;
        app.discover_loading = false;
        app.handle_error(anyhow!(e));
        return;
      }
    };

    let mut all_tracks = Vec::new();

    // 2. Get top tracks for each artist
    for artist in artists {
      let path = format!("artists/{}/top-tracks", artist.id.id());
      if let Ok(res) = self
        .spotify_get_typed::<ArtistTopTracksResponse>(&path, &[])
        .await
      {
        all_tracks.extend(res.tracks);
      }
    }

    // 3. Shuffle
    {
      let mut rng = rand::thread_rng();
      all_tracks.shuffle(&mut rng);
    }

    // 4. Update state
    let mut app = self.app.lock().await;
    app.discover_artists_mix = all_tracks;
    app.discover_loading = false;
  }

  async fn get_recently_played(&mut self) {
    let limit = self.large_search_limit;
    match self
      .spotify_get_typed::<CursorBasedPage<PlayHistory>>(
        "me/player/recently-played",
        &[("limit", limit.to_string())],
      )
      .await
    {
      Ok(recently_played) => {
        let mut app = self.app.lock().await;
        app.recently_played.result = Some(recently_played);
        app.push_navigation_stack(RouteId::RecentlyPlayed, ActiveBlock::RecentlyPlayed);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }
}
