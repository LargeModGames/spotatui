use super::requests::spotify_get_typed_compat_for;
use super::{IoEvent, Network};
use crate::tui::ui::util::create_artist_string;
use anyhow::anyhow;
use chrono::Duration as ChronoDuration;
use chrono::TimeDelta;
use rspotify::model::{
  enums::RepeatState,
  idtypes::{PlayContextId, PlayableId},
  Offset, PlayableItem,
};
use rspotify::prelude::*;
use std::time::{Duration, Instant};

#[cfg(feature = "streaming")]
use librespot_connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
#[cfg(feature = "streaming")]
use std::sync::Arc;

const MAX_API_PLAYBACK_URIS: usize = 100;

pub trait PlaybackNetwork {
  async fn get_current_playback(&mut self);
  async fn start_playback(
    &mut self,
    context_id: Option<PlayContextId<'static>>,
    uris: Option<Vec<PlayableId<'static>>>,
    offset: Option<usize>,
  );
  async fn pause_playback(&mut self);
  async fn next_track(&mut self);
  async fn previous_track(&mut self);
  async fn force_previous_track(&mut self);
  async fn seek(&mut self, position_ms: u32);
  async fn shuffle(&mut self, shuffle_state: bool);
  async fn repeat(&mut self, repeat_state: RepeatState);
  async fn change_volume(&mut self, volume: u8);
  async fn transfert_playback_to_device(&mut self, device_id: String, persist_device_id: bool);
  #[cfg(feature = "streaming")]
  async fn auto_select_streaming_device(&mut self, device_name: String, persist_device_id: bool);
  async fn ensure_playback_continues(&mut self, previous_track_id: String);
  #[allow(dead_code)]
  async fn add_item_to_queue(&mut self, item: PlayableId<'static>);
  async fn get_queue(&mut self);
}

fn trim_api_playback_uris(
  track_uris: Vec<PlayableId<'static>>,
  offset: Option<usize>,
) -> (Vec<PlayableId<'static>>, Option<usize>) {
  if track_uris.len() <= MAX_API_PLAYBACK_URIS {
    return (track_uris, offset);
  }

  let selected_index = offset.unwrap_or(0).min(track_uris.len().saturating_sub(1));
  let preferred_history = MAX_API_PLAYBACK_URIS / 5;
  let mut start = selected_index.saturating_sub(preferred_history);
  let end = (start + MAX_API_PLAYBACK_URIS).min(track_uris.len());

  if end - start < MAX_API_PLAYBACK_URIS {
    start = end.saturating_sub(MAX_API_PLAYBACK_URIS);
  }

  // Spotify rejects oversized URI payloads, so URI-list playback is capped
  // to a window that still contains the selected track.
  let trimmed_uris = track_uris[start..end]
    .iter()
    .map(PlayableId::clone_static)
    .collect::<Vec<_>>();

  (trimmed_uris, Some(selected_index - start))
}

fn api_playback_offset(
  context_uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Option<Offset> {
  if let Some(first_uri) = context_uris.and_then(|uris| uris.first()) {
    return Some(Offset::Uri(first_uri.uri()));
  }

  offset.map(|index| Offset::Position(ChronoDuration::milliseconds(index as i64)))
}

/// Get the currently active streaming player, if any.
/// Note: This logic is duplicated in `main.rs` as `active_streaming_player()`.
/// Both are identical; the difference is input type (Network vs. App Arc).
/// A future refactor could consolidate to a shared location like `src/core/app.rs`.
#[cfg(feature = "streaming")]
async fn current_streaming_player(
  network: &Network,
) -> Option<Arc<crate::infra::player::StreamingPlayer>> {
  let app = network.app.lock().await;
  app.streaming_player.clone()
}

#[cfg(feature = "streaming")]
async fn is_native_streaming_active_for_playback(network: &Network) -> bool {
  let app = network.app.lock().await;
  let streaming_player = app.streaming_player.clone();
  let player_connected = streaming_player.as_ref().is_some_and(|p| p.is_connected());

  if !player_connected {
    return false;
  }

  let native_device_name = streaming_player
    .as_ref()
    .map(|p| p.device_name().to_lowercase());

  // If no context yet (e.g., at startup), use the app state flag which is
  // set when the native streaming device is activated/selected.
  let Some(ref ctx) = app.current_playback_context else {
    return app.is_streaming_active;
  };

  // First, check if the current playback device matches the native streaming device ID
  if let (Some(current_id), Some(native_id)) =
    (ctx.device.id.as_ref(), app.native_device_id.as_ref())
  {
    if current_id == native_id {
      return true;
    }
  }

  // Fallback: strict name match (case-insensitive)
  if let Some(native_name) = native_device_name.as_ref() {
    let current_device_name = ctx.device.name.to_lowercase();
    if current_device_name == native_name.as_str() {
      return true;
    }
  }

  // No match - not the active device
  false
}

impl PlaybackNetwork for Network {
  async fn get_current_playback(&mut self) {
    // When using native streaming, the Spotify API returns stale server-side state
    // that doesn't reflect recent local changes (volume, shuffle, repeat, play/pause).
    // We need to preserve these local states and restore them after getting the API response.
    #[cfg(feature = "streaming")]
    let streaming_player = current_streaming_player(self).await;
    #[cfg(feature = "streaming")]
    // Check if native streaming is active by examining the pre-fetched player
    // (avoids redundant lock call from is_native_streaming_active)
    let local_state: Option<(Option<u8>, bool, rspotify::model::RepeatState, Option<bool>)> =
      if streaming_player.as_ref().is_some_and(|p| p.is_connected()) {
        let app = self.app.lock().await;
        if let Some(ref ctx) = app.current_playback_context {
          let volume = streaming_player.as_ref().map(|p| p.get_volume());
          Some((
            volume,
            ctx.shuffle_state,
            ctx.repeat_state,
            app.native_is_playing,
          ))
        } else {
          None
        }
      } else {
        None
      };

    let context = spotify_get_typed_compat_for::<Option<rspotify::model::CurrentPlaybackContext>>(
      &self.spotify,
      "me/player",
      &[("additional_types", "episode,track".to_string())],
    )
    .await;

    let mut app = self.app.lock().await;

    match context {
      #[allow(unused_mut)]
      Ok(Some(mut c)) => {
        app.instant_since_last_current_playback_poll = Instant::now();

        // Detect whether the native spotatui streaming device is the active Spotify device.
        #[cfg(feature = "streaming")]
        let is_native_device = streaming_player.as_ref().is_some_and(|p| {
          if let (Some(current_id), Some(native_id)) =
            (c.device.id.as_ref(), app.native_device_id.as_ref())
          {
            return current_id == native_id;
          }
          let native_name = p.device_name().to_lowercase();
          c.device.name.to_lowercase() == native_name
        });

        #[cfg(feature = "streaming")]
        if is_native_device && app.native_device_id.is_none() {
          if let Some(id) = c.device.id.clone() {
            app.native_device_id = Some(id);
          }
        }

        // Process track info before storing context (avoids cloning)
        if let Some(ref item) = c.item {
          match item {
            PlayableItem::Track(track) => {
              if let Some(ref track_id) = track.id {
                let track_id_str = track_id.id().to_string();

                // Check if this is a new track
                if app.last_track_id.as_ref() != Some(&track_id_str) {
                  if app.user_config.behavior.enable_global_song_count {
                    app.dispatch(IoEvent::IncrementGlobalSongCount);
                  }

                  // Trigger lyrics fetch
                  let duration_secs = track.duration.num_seconds() as f64;
                  app.dispatch(IoEvent::GetLyrics(
                    track.name.clone(),
                    create_artist_string(&track.artists),
                    duration_secs,
                  ));

                  app.dispatch(IoEvent::CurrentUserSavedTracksContains(vec![track_id
                    .clone()
                    .into_static()]));
                }

                app.last_track_id = Some(track_id_str);
              };
            }
            PlayableItem::Episode(_episode) => { /*should map this to following the podcast show*/ }
            _ => {}
          }
        };

        // Preserve local streaming states (API returns stale server-side state)
        #[cfg(feature = "streaming")]
        if is_native_device {
          if let Some((volume, shuffle, repeat, native_is_playing)) = local_state {
            if let Some(vol) = volume {
              c.device.volume_percent = Some(vol.into());
            }
            c.shuffle_state = shuffle;
            c.repeat_state = repeat;
            // Preserve play/pause state from native player events when available.
            if let Some(is_playing) = native_is_playing {
              c.is_playing = is_playing;
            }
          }
        }

        // Check if Spotify finally caught up to the user's volume change.
        // If the API now returns what the user asked for, we can clear pending_volume
        // and let the API take over again. If not, this response is stale — ignore it.
        if let Some(pending) = app.pending_volume {
          let api_vol = c.device.volume_percent.unwrap_or(0) as u8;
          if api_vol == pending {
            app.pending_volume = None;
            app.last_dispatched_volume = None;
          } else {
            // API hasn't caught up yet — keep showing the user's intended value
            if let Some(ctx) = app.current_playback_context.as_ref() {
              c.device.volume_percent = ctx.device.volume_percent;
            }
          }
        }

        // On first load with native streaming AND native device is active,
        // override API shuffle with saved preference.
        #[cfg(feature = "streaming")]
        if local_state.is_none() && is_native_device {
          c.shuffle_state = app.user_config.behavior.shuffle_enabled;
          // Proactively set native shuffle on first load to keep backend in sync
          if let Some(ref player) = streaming_player {
            let _ = player.set_shuffle(app.user_config.behavior.shuffle_enabled);
          }
        }

        // Get album/episode cover art
        #[cfg(feature = "cover-art")]
        if app
          .user_config
          .do_draw_cover_art(app.cover_art.full_image_support())
        {
          if let Some(playable) = &c.item {
            let image = match playable {
              PlayableItem::Track(t) => t.album.images.first(),
              PlayableItem::Episode(e) => e.images.first(),
              _ => None,
            };

            if let Some(image) = image {
              if let anyhow::Result::Err(err) = app.cover_art.refresh(image).await {
                drop(app);
                self.handle_error(err).await;
                return;
              }
            }
          }
        }

        app.current_playback_context = Some(c);

        // Update is_streaming_active based on whether the current device matches native streaming
        #[cfg(feature = "streaming")]
        {
          app.is_streaming_active = is_native_device;
          if is_native_device {
            app.native_activation_pending = false;
          }
        }

        // Only clear native track info if API data matches the native player's track
        if let Some(ref native_info) = app.native_track_info {
          if let Some(ref ctx) = app.current_playback_context {
            if let Some(ref item) = ctx.item {
              let api_track_name = match item {
                PlayableItem::Track(t) => &t.name,
                PlayableItem::Episode(e) => &e.name,
                _ => return,
              };
              // Only clear if names match (API caught up to native player)
              if api_track_name == &native_info.name {
                app.native_track_info = None;
              }
            }
          }
        } else {
          app.native_track_info = None;
        }
      }
      Ok(None) => {
        app.instant_since_last_current_playback_poll = Instant::now();
      }
      Err(e) => {
        app.is_fetching_current_playback = false;

        let err = anyhow!(e);

        if err.to_string().contains("429")
          || err.to_string().contains("Too Many Requests")
          || err.to_string().contains("Too many requests")
        {
          app.status_message = Some(
            "Spotify rate limit hit. Retrying automatically; please wait a few seconds."
              .to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(6));
          app.instant_since_last_current_playback_poll = Instant::now();
          return;
        }

        if err
          .to_string()
          .to_lowercase()
          .contains("error sending request for url")
          || err.to_string().contains("connection reset")
          || err.to_string().contains("connection refused")
          || err.to_string().contains("timed out")
          || err.to_string().contains("temporary failure")
          || err.to_string().contains("dns")
        {
          app.status_message = Some(
            "Temporary Spotify network error while polling playback; retrying automatically."
              .to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(5));
          app.instant_since_last_current_playback_poll = Instant::now();
          return;
        }

        if err.to_string().contains("504")
          || err.to_string().contains("503")
          || err.to_string().contains("502")
          || err.to_string().contains("Gateway Timeout")
          || err.to_string().contains("Service Unavailable")
          || err.to_string().contains("Bad Gateway")
        {
          app.status_message = Some(
            "Spotify server temporarily unavailable (5xx); retrying automatically.".to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(10));
          app.instant_since_last_current_playback_poll = Instant::now();
          return;
        }

        app.handle_error(err);
        return;
      }
    }

    app.seek_ms.take();
    app.is_fetching_current_playback = false;
  }

  async fn start_playback(
    &mut self,
    context_id: Option<PlayContextId<'static>>,
    uris: Option<Vec<PlayableId<'static>>>,
    offset: Option<usize>,
  ) {
    let (uris, offset) = if context_id.is_none() {
      match uris {
        Some(track_uris) => {
          let (trimmed_uris, trimmed_offset) = trim_api_playback_uris(track_uris, offset);
          (Some(trimmed_uris), trimmed_offset)
        }
        None => (None, offset),
      }
    } else {
      (uris, offset)
    };

    let desired_shuffle_state = {
      let app = self.app.lock().await;
      app
        .current_playback_context
        .as_ref()
        .map(|ctx| ctx.shuffle_state)
        .unwrap_or(app.user_config.behavior.shuffle_enabled)
    };

    // Check if we should use native streaming for playback
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        let activation_time = Instant::now();
        let should_transfer = {
          let app = self.app.lock().await;
          let activation_pending = app.native_activation_pending;
          let recent_activation = app
            .last_device_activation
            .is_some_and(|instant| instant.elapsed() < Duration::from_secs(5));
          if activation_pending {
            !recent_activation
          } else {
            !app.is_streaming_active && !recent_activation
          }
        };

        if should_transfer {
          let _ = player.transfer(None);
        }

        player.activate();
        {
          let mut app = self.app.lock().await;
          app.is_streaming_active = true;
          app.last_device_activation = Some(activation_time);
          app.native_activation_pending = false;
        }

        // For resume playback (no context, no uris)
        if context_id.is_none() && uris.is_none() {
          player.play();
          // Update UI state immediately
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
          }
          return;
        }

        // For URI-based or context playback, use Spirc load directly.
        let mut options = LoadRequestOptions {
          start_playing: true,
          seek_to: 0,
          context_options: None,
          playing_track: None,
        };

        let request = match (context_id, uris) {
          (Some(context), Some(track_uris)) => {
            if let Some(first_uri) = track_uris.first() {
              options.playing_track = Some(PlayingTrack::Uri(first_uri.uri()));
            } else if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
              options.playing_track = Some(PlayingTrack::Index(i));
            }
            LoadRequest::from_context_uri(context.uri(), options)
          }
          (Some(context), None) => {
            if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
              options.playing_track = Some(PlayingTrack::Index(i));
            }
            LoadRequest::from_context_uri(context.uri(), options)
          }
          (None, Some(track_uris)) => {
            if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
              options.playing_track = Some(PlayingTrack::Index(i));
            }
            let uris = track_uris.into_iter().map(|u| u.uri()).collect::<Vec<_>>();
            LoadRequest::from_tracks(uris, options)
          }
          (None, None) => {
            player.play();
            let mut app = self.app.lock().await;
            if let Some(ctx) = &mut app.current_playback_context {
              ctx.is_playing = true;
            }
            return;
          }
        };

        if let Err(e) = player.load(request) {
          let mut app = self.app.lock().await;
          app.handle_error(anyhow!("Failed to start native playback: {}", e));
        } else {
          let _ = player.set_shuffle(desired_shuffle_state);
          // Optimistic UI update
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
            ctx.shuffle_state = desired_shuffle_state;
          }
          app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
        }
        return;
      }
    }

    let result = match (context_id, uris) {
      (Some(context), track_uris) => {
        let offset_struct = api_playback_offset(track_uris.as_deref(), offset);
        self
          .spotify
          .start_context_playback(
            context,
            None, // device_id
            offset_struct,
            None, // position
          )
          .await
      }
      (None, Some(track_uris)) => {
        let offset_struct = api_playback_offset(None, offset);
        self
          .spotify
          .start_uris_playback(
            track_uris,
            None, // device_id
            offset_struct,
            None, // position
          )
          .await
      }
      (None, None) => self.spotify.resume_playback(None, None).await,
    };

    match result {
      Ok(_) => {
        if let Err(e) = self.spotify.shuffle(desired_shuffle_state, None).await {
          let mut app = self.app.lock().await;
          app.handle_error(anyhow!(e));
        }

        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = true;
          ctx.shuffle_state = desired_shuffle_state;
        }
        app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn pause_playback(&mut self) {
    // Check if using native streaming
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        player.pause();
        // Update UI state immediately
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = false;
        }
        return;
      }
    }

    match self.spotify.pause_playback(None).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = false;
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn next_track(&mut self) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        player.next();
        return;
      }
    }

    if let Err(e) = self.spotify.next_track(None).await {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn previous_track(&mut self) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        player.prev();
        return;
      }
    }

    if let Err(e) = self.spotify.previous_track(None).await {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn force_previous_track(&mut self) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        player.prev();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        player.prev();
        return;
      }
    }

    // First previous_track restarts the current track (if past Spotify's ~3s
    // threshold). After a short delay the second call actually skips to the
    // previous track, since the position is now back at 0.
    if let Err(e) = self.spotify.previous_track(None).await {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
      return;
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    if let Err(e) = self.spotify.previous_track(None).await {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn seek(&mut self, position_ms: u32) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        player.seek(position_ms);
        return;
      }
    }

    if let Err(e) = self
      .spotify
      .seek_track(ChronoDuration::milliseconds(position_ms as i64), None)
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn shuffle(&mut self, shuffle_state: bool) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        let _ = player.set_shuffle(shuffle_state);
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.shuffle_state = shuffle_state;
        }
        return;
      }
    }

    match self.spotify.shuffle(shuffle_state, None).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.shuffle_state = shuffle_state;
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn repeat(&mut self, repeat_state: RepeatState) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        let _ = player.set_repeat(repeat_state);
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.repeat_state = repeat_state;
        }
        return;
      }
    }

    match self.spotify.repeat(repeat_state, None).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.repeat_state = repeat_state;
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  /// Sends the volume change to Spotify, either through the native streaming
  /// player or the Web API depending on which device is active.
  ///
  /// On success we clear the in-flight flag but keep `pending_volume` around.
  /// It only gets cleared when `get_current_playback` comes back with a matching
  /// volume — that's our signal that Spotify actually caught up.
  ///
  /// On error we bail and clear everything so the UI falls back to whatever
  /// the API last reported.
  async fn change_volume(&mut self, volume: u8) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      if let Some(player) = current_streaming_player(self).await {
        player.set_volume(volume);
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.device.volume_percent = Some(volume.into());
        }
        app.is_volume_change_in_flight = false;
        app.last_dispatched_volume = Some(volume);
        // Keep pending_volume set — cleared when API confirms the value matches
        return;
      }
    }

    match self.spotify.volume(volume, None).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.device.volume_percent = Some(volume.into());
        }
        app.is_volume_change_in_flight = false;
        app.last_dispatched_volume = Some(volume);
        // Keep pending_volume set — cleared when get_current_playback confirms
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.is_volume_change_in_flight = false;
        app.pending_volume = None;
        app.last_dispatched_volume = None;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn transfert_playback_to_device(&mut self, device_id: String, persist_device_id: bool) {
    #[cfg(feature = "streaming")]
    {
      let streaming_player = current_streaming_player(self).await;
      let is_native_transfer = if let Some(ref player) = streaming_player {
        let native_name = player.device_name().to_lowercase();
        let app = self.app.lock().await;
        let matches_cached_device = app.devices.as_ref().is_some_and(|payload| {
          payload
            .devices
            .iter()
            .any(|d| d.id.as_ref() == Some(&device_id) && d.name.to_lowercase() == native_name)
        });
        matches_cached_device || app.native_device_id.as_ref() == Some(&device_id)
      } else {
        false
      };

      if is_native_transfer {
        if let Some(ref player) = streaming_player {
          let _ = player.transfer(None);
          player.activate();
          let mut app = self.app.lock().await;
          app.is_streaming_active = true;
          app.native_activation_pending = true;
          app.last_device_activation = Some(Instant::now());
          app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(6);
          return;
        }
      }
    }

    if let Err(e) = self.spotify.transfer_playback(&device_id, Some(true)).await {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    } else {
      let mut app = self.app.lock().await;
      if persist_device_id {
        // Update via client_config helper to save to file
        if let Err(e) = self.client_config.set_device_id(device_id) {
          app.handle_error(anyhow!(e));
        }
      }
      app.current_playback_context = None;

      #[cfg(feature = "streaming")]
      {
        // If transferring away from native, update flag
        app.is_streaming_active = false;
      }
    }
  }

  #[cfg(feature = "streaming")]
  async fn auto_select_streaming_device(&mut self, device_name: String, persist_device_id: bool) {
    tokio::time::sleep(Duration::from_millis(200)).await;

    if let Some(player) = current_streaming_player(self).await {
      let activation_time = Instant::now();
      let should_transfer = {
        let app = self.app.lock().await;
        let recent_activation = app
          .last_device_activation
          .is_some_and(|instant| instant.elapsed() < Duration::from_secs(5));
        !app.native_activation_pending && !app.is_streaming_active && !recent_activation
      };

      {
        let mut app = self.app.lock().await;
        app.is_streaming_active = true;
        app.native_activation_pending = true;
        app.last_device_activation = Some(activation_time);
        app.instant_since_last_current_playback_poll = activation_time - Duration::from_secs(6);
      }

      if should_transfer {
        let _ = player.transfer(None);
      }
      player.activate();

      {
        let mut app = self.app.lock().await;
        app.is_streaming_active = true;
        app.native_activation_pending = false;
        app.last_device_activation = Some(activation_time);
        app.instant_since_last_current_playback_poll = activation_time - Duration::from_secs(6);
      }

      for attempt in 0..2 {
        if attempt > 0 {
          tokio::time::sleep(Duration::from_millis(200)).await;
        }

        match self.spotify.device().await {
          Ok(devices) => {
            if let Some(device) = devices
              .iter()
              .find(|d| d.name.to_lowercase() == device_name.to_lowercase())
            {
              if let Some(id) = &device.id {
                if persist_device_id {
                  let _ = self.client_config.set_device_id(id.clone());
                }
                let mut app = self.app.lock().await;
                app.native_device_id = Some(id.clone());
                return;
              }
            }
          }
          Err(_) => continue,
        }
      }
    }
  }

  async fn ensure_playback_continues(&mut self, previous_track_id: String) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      // Native player handles queue automatically
      return;
    }

    // Check if we are paused/stopped
    let context = spotify_get_typed_compat_for::<Option<rspotify::model::CurrentPlaybackContext>>(
      &self.spotify,
      "me/player",
      &[],
    )
    .await;

    if let Ok(Some(ctx)) = context {
      if !ctx.is_playing {
        // If we are stopped but shouldn't be (e.g. track finished), try to skip to next
        // Use a heuristic: if the current item is the SAME as the previous one and we are at 0:00,
        // it might mean Spotify stopped. Or if we are just null.
        if let Some(item) = ctx.item {
          let current_id = match item {
            PlayableItem::Track(t) => t.id.map(|id| id.id().to_string()),
            PlayableItem::Episode(e) => Some(e.id.id().to_string()),
            _ => None,
          };

          if current_id == Some(previous_track_id)
            && ctx
              .progress
              .map(|d: TimeDelta| d.num_milliseconds())
              .unwrap_or(0)
              == 0
          {
            self.next_track().await;
          }
        }
      }
    }
  }

  async fn add_item_to_queue(&mut self, item: PlayableId<'static>) {
    match self.spotify.add_item_to_queue(item, None).await {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.status_message = Some("Added to queue".to_string());
        app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(3));
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn get_queue(&mut self) {
    match self.spotify.current_user_queue().await {
      Ok(q) => {
        let mut app = self.app.lock().await;
        app.queue = Some(q);
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.queue = None;
        app.status_message = Some("Could not load queue (no active device?)".to_string());
        app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(3));
        log::warn!("get_queue failed: {}", e);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rspotify::model::idtypes::TrackId;
  use rspotify::prelude::Id;

  fn playable_track(id: &str) -> PlayableId<'static> {
    PlayableId::Track(TrackId::from_id(id).unwrap().into_static())
  }

  #[test]
  fn trim_api_playback_uris_leaves_small_requests_unchanged() {
    let uris = vec![
      playable_track("0000000000000000000001"),
      playable_track("0000000000000000000002"),
    ];

    let (trimmed, offset) = trim_api_playback_uris(uris.clone(), Some(1));

    assert_eq!(trimmed, uris);
    assert_eq!(offset, Some(1));
  }

  #[test]
  fn trim_api_playback_uris_keeps_selected_track_inside_window() {
    let uris = (0..150)
      .map(|index| playable_track(&format!("{index:022}")))
      .collect::<Vec<_>>();

    let (trimmed, offset) = trim_api_playback_uris(uris.clone(), Some(60));

    assert_eq!(trimmed.len(), MAX_API_PLAYBACK_URIS);
    assert_eq!(offset, Some(20));
    assert_eq!(trimmed[offset.unwrap()].uri(), uris[60].uri());
  }

  #[test]
  fn trim_api_playback_uris_slides_window_near_end() {
    let uris = (0..150)
      .map(|index| playable_track(&format!("{index:022}")))
      .collect::<Vec<_>>();

    let (trimmed, offset) = trim_api_playback_uris(uris.clone(), Some(149));

    assert_eq!(trimmed.len(), MAX_API_PLAYBACK_URIS);
    assert_eq!(offset, Some(99));
    assert_eq!(trimmed[offset.unwrap()].uri(), uris[149].uri());
  }

  #[test]
  fn api_playback_offset_uses_track_uri_for_context_playback() {
    let uris = vec![
      playable_track("0000000000000000000001"),
      playable_track("0000000000000000000002"),
    ];

    let offset = api_playback_offset(Some(&uris), Some(1));

    assert_eq!(
      offset,
      Some(Offset::Uri(
        "spotify:track:0000000000000000000001".to_string()
      ))
    );
  }

  #[test]
  fn api_playback_offset_uses_position_for_uri_list_playback() {
    let offset = api_playback_offset(None, Some(1));

    assert_eq!(
      offset,
      Some(Offset::Position(ChronoDuration::milliseconds(1)))
    );
  }

  #[test]
  fn api_playback_offset_falls_back_to_position_when_context_has_no_uri() {
    let offset = api_playback_offset(None, Some(3));

    assert_eq!(
      offset,
      Some(Offset::Position(ChronoDuration::milliseconds(3)))
    );
  }
}
