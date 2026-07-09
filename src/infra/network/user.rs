use super::requests::is_rate_limited_error;
use super::{ids, IoEvent, Network};
use crate::core::app::{ActiveBlock, DiscoverTimeRange, RouteId, UserInfo};
use crate::core::plugin_api::TrackInfo;
use anyhow::anyhow;

use crate::infra::network::mapping::map_cursor_page;
use rand::seq::SliceRandom;
use rspotify::model::{
  artist::FullArtist,
  device::DevicePayload,
  page::{CursorBasedPage, Page},
  playing::PlayHistory,
  track::FullTrack,
  user::PrivateUser,
};
#[cfg(feature = "streaming")]
use rspotify::model::{enums::DeviceType, Device};
use rspotify::prelude::*;
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct ArtistTopTracksResponse {
  tracks: Vec<FullTrack>,
}

#[cfg(feature = "streaming")]
fn include_native_streaming_device(app: &crate::core::app::App, payload: &mut DevicePayload) {
  let Some(player) = app.streaming_player.as_ref() else {
    return;
  };

  if !player.is_connected() {
    return;
  }

  let device_name = player.device_name();
  let device_id = app
    .native_device_id
    .clone()
    .unwrap_or_else(|| player.device_id());

  if let Some(device) = payload
    .devices
    .iter_mut()
    .find(|device| device.name.eq_ignore_ascii_case(device_name))
  {
    if device.id.is_none() {
      device.id = Some(device_id);
    }
    return;
  }

  payload.devices.push(Device {
    id: Some(device_id),
    is_active: app.is_streaming_active,
    is_private_session: false,
    is_restricted: false,
    name: device_name.to_string(),
    _type: DeviceType::Computer,
    volume_percent: Some(player.get_volume().into()),
  });
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
        // `PrivateUser::country` is deprecated upstream but still the only
        // market signal available; mirror the existing read in `get_user_country`.
        #[allow(deprecated)]
        let country = user.country.map(|c| <&'static str>::from(c).to_string());
        app.user = Some(UserInfo {
          id: user.id.id().to_string(),
          display_name: user.display_name.clone(),
          // Store the ISO 3166-1 alpha-2 code as a plain string so no rspotify
          // type leaks into App state; `App::get_user_country` re-derives it.
          country,
        });
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
    match self
      .spotify_get_typed::<DevicePayload>("me/player/devices", &[])
      .await
    {
      Ok(result) => {
        let mut app = self.app.lock().await;
        app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);

        #[cfg(feature = "streaming")]
        let mut result = result;
        #[cfg(feature = "streaming")]
        {
          let recovering = app.request_native_streaming_recovery_if_disconnected(true);
          if !recovering {
            include_native_streaming_device(&app, &mut result);
          }
        }

        app.selected_device_index = if result.devices.is_empty() {
          None
        } else {
          app
            .selected_device_index
            .filter(|index| *index < result.devices.len())
            .or(Some(0))
        };
        app.devices = Some(result);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
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
        // Check if these tracks are liked.
        let track_check = ids::track_check_ids(page.items.iter().map(|t| t.id.as_ref()));
        if !track_check.is_empty() {
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
        }
        app.discover_top_tracks = page.items.iter().map(TrackInfo::from).collect();
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
    // Check if these tracks are liked.
    let track_check = ids::track_check_ids(all_tracks.iter().map(|t| t.id.as_ref()));
    if !track_check.is_empty() {
      app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
    }
    app.discover_artists_mix = all_tracks.iter().map(TrackInfo::from).collect();
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
        let domain_page = map_cursor_page(&recently_played, |ph| TrackInfo::from(&ph.track));
        let mut app = self.app.lock().await;
        // Check if these tracks are liked.
        let track_check =
          ids::track_check_ids(recently_played.items.iter().map(|ph| ph.track.id.as_ref()));
        if !track_check.is_empty() {
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
        }
        app.recently_played.result = Some(domain_page);
        app.sort_recently_played_items();
        app.push_navigation_stack(RouteId::RecentlyPlayed, ActiveBlock::RecentlyPlayed);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }
}
