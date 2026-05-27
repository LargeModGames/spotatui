use super::{IoEvent, Network};
#[cfg(feature = "streaming")]
use crate::core::app::NativePlaybackOrigin;
use crate::tui::ui::util::create_artist_string;
use anyhow::anyhow;
use chrono::TimeDelta;
#[cfg(feature = "streaming")]
use log::info;
use reqwest::Method;
#[cfg(feature = "streaming")]
use rspotify::model::device::DevicePayload;
use rspotify::model::{
  context::CurrentUserQueue,
  enums::RepeatState,
  idtypes::{PlayContextId, PlayableId},
  PlayableItem,
};
use rspotify::prelude::*;
use serde_json::{json, Value};
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

fn api_playback_offset_json(
  context_uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Option<Value> {
  if let Some(first_uri) = context_uris.and_then(|uris| uris.first()) {
    return Some(json!({ "uri": first_uri.uri() }));
  }

  offset.map(|index| json!({ "position": index }))
}

fn api_playback_body(
  context_id: Option<&PlayContextId<'static>>,
  uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Option<Value> {
  match (context_id, uris) {
    (Some(context), track_uris) => {
      let mut body = json!({ "context_uri": context.uri() });
      if let Some(offset) = api_playback_offset_json(track_uris, offset) {
        body["offset"] = offset;
      }
      Some(body)
    }
    (None, Some(track_uris)) => {
      let mut body = json!({
        "uris": track_uris.iter().map(|uri| uri.uri()).collect::<Vec<_>>()
      });
      if let Some(offset) = api_playback_offset_json(None, offset) {
        body["offset"] = offset;
      }
      Some(body)
    }
    (None, None) => None,
  }
}

fn playable_item_id(item: &PlayableItem) -> Option<String> {
  match item {
    PlayableItem::Track(track) => track.id.as_ref().map(|id| id.id().to_string()),
    PlayableItem::Episode(episode) => Some(episode.id.id().to_string()),
    PlayableItem::Unknown(_) => None,
  }
}

fn playable_item_name(item: &PlayableItem) -> Option<&str> {
  match item {
    PlayableItem::Track(track) => Some(&track.name),
    PlayableItem::Episode(episode) => Some(&episode.name),
    PlayableItem::Unknown(_) => None,
  }
}

#[cfg(feature = "streaming")]
#[derive(Debug, PartialEq, Eq)]
enum NativePlaybackRoute {
  ContextApi { device_id: String },
  NativeLoad,
}

fn api_confirms_native_info_is_current(
  native_name: &str,
  item: &PlayableItem,
  last_track_id: Option<&str>,
) -> bool {
  if playable_item_name(item) == Some(native_name) {
    return true;
  }

  playable_item_id(item)
    .as_deref()
    .is_some_and(|api_id| Some(api_id) == last_track_id)
}

#[cfg(feature = "streaming")]
#[derive(Clone, Copy, Debug)]
struct StaleApiItemContext {
  native_info_present: bool,
  api_item_present: bool,
  api_confirms_native_info: bool,
  native_track_id_present: bool,
  api_item_matches_native_track: bool,
  native_streaming_was_active: bool,
  native_activation_pending: bool,
  api_device_is_native: bool,
}

#[cfg(feature = "streaming")]
fn stale_api_item_should_preserve_native_context(context: StaleApiItemContext) -> bool {
  context.api_item_present
    && !context.api_confirms_native_info
    && (context.native_info_present
      || (context.native_track_id_present && !context.api_item_matches_native_track))
    && (context.native_streaming_was_active
      || context.native_activation_pending
      || context.api_device_is_native)
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

#[cfg(feature = "streaming")]
async fn requested_native_playback_origin(
  network: &Network,
  context_id: &Option<PlayContextId<'static>>,
  uris: &Option<Vec<PlayableId<'static>>>,
) -> NativePlaybackOrigin {
  if context_id.is_some() {
    return NativePlaybackOrigin::Context;
  }

  if uris.is_some() {
    return NativePlaybackOrigin::RawList;
  }

  let app = network.app.lock().await;
  if let Some(origin) = app.native_playback_origin {
    return origin;
  }

  if app
    .current_playback_context
    .as_ref()
    .and_then(|ctx| ctx.context.as_ref())
    .is_some()
  {
    NativePlaybackOrigin::Context
  } else {
    NativePlaybackOrigin::RawList
  }
}

#[cfg(feature = "streaming")]
async fn resolve_native_playback_route(
  network: &Network,
  context_id: &Option<PlayContextId<'static>>,
) -> NativePlaybackRoute {
  if context_id.is_none() {
    return NativePlaybackRoute::NativeLoad;
  }

  let app = network.app.lock().await;
  match app.native_device_id.clone() {
    Some(device_id) => NativePlaybackRoute::ContextApi { device_id },
    None => NativePlaybackRoute::NativeLoad,
  }
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

    let context = self
      .spotify_get_typed::<Option<rspotify::model::CurrentPlaybackContext>>(
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

        #[cfg(feature = "streaming")]
        let native_streaming_was_active = app.is_streaming_active;
        #[cfg(feature = "streaming")]
        let native_activation_was_pending = app.native_activation_pending;
        let native_track_id_before_api = app.last_track_id.clone();
        #[cfg(feature = "streaming")]
        let native_track_id_present = native_track_id_before_api.is_some();
        #[cfg(feature = "streaming")]
        let api_item_matches_native_track = c
          .item
          .as_ref()
          .and_then(playable_item_id)
          .as_deref()
          .is_some_and(|api_id| Some(api_id) == native_track_id_before_api.as_deref());
        let api_item_confirms_native_info = app
          .native_track_info
          .as_ref()
          .zip(c.item.as_ref())
          .is_some_and(|(native_info, item)| {
            api_confirms_native_info_is_current(
              &native_info.name,
              item,
              native_track_id_before_api.as_deref(),
            )
          });
        #[cfg(feature = "streaming")]
        let stale_api_item_for_native =
          stale_api_item_should_preserve_native_context(StaleApiItemContext {
            native_info_present: app.native_track_info.is_some(),
            api_item_present: c.item.is_some(),
            api_confirms_native_info: api_item_confirms_native_info,
            native_track_id_present,
            api_item_matches_native_track,
            native_streaming_was_active,
            native_activation_pending: native_activation_was_pending,
            api_device_is_native: is_native_device,
          });
        #[cfg(not(feature = "streaming"))]
        let stale_api_item_for_native =
          app.native_track_info.is_some() && c.item.is_some() && !api_item_confirms_native_info;

        // Process track info before storing context (avoids cloning)
        if !stale_api_item_for_native {
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
              PlayableItem::Episode(_episode) => { /*should map this to following the podcast show*/
              }
              _ => {}
            }
          };
        }

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

        if !stale_api_item_for_native {
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
        }

        // Update is_streaming_active based on whether the current device matches native streaming
        #[cfg(feature = "streaming")]
        {
          if stale_api_item_for_native {
            app.is_streaming_active = true;
            app.native_activation_pending = false;
          } else {
            app.is_streaming_active = is_native_device;
          }

          if is_native_device {
            app.native_activation_pending = false;
          }
        }

        // Keep native metadata authoritative while the native player is active.
        // Spotify's playback endpoint can lag behind librespot by several seconds
        // and report a different item; TrackChanged/Stopped events own this field.
        #[cfg(feature = "streaming")]
        if app.native_track_info.is_some()
          && !stale_api_item_for_native
          && (!is_native_device || api_item_confirms_native_info)
        {
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

        // 404 = no active device/player; treat as idle, not an error
        if err.to_string().contains("404") || err.to_string().contains("Not Found") {
          app.current_playback_context = None;
          app.instant_since_last_current_playback_poll = Instant::now();
          app.is_fetching_current_playback = false;
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
        let requested_origin = requested_native_playback_origin(self, &context_id, &uris).await;
        let native_route = resolve_native_playback_route(self, &context_id).await;
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
          app.native_playback_origin = Some(requested_origin);
        }

        // For resume playback (no context, no uris)
        if context_id.is_none() && uris.is_none() {
          info!("starting native resume playback via direct player route");
          player.play();
          // Update UI state immediately
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
          }
          return;
        }

        if let (NativePlaybackRoute::ContextApi { device_id }, Some(context)) =
          (&native_route, context_id.clone())
        {
          info!(
            "starting native playback via Spotify context route on device {}",
            device_id
          );
          let body = api_playback_body(Some(&context), uris.as_deref(), offset);
          match self
            .spotify_api_request_json(
              Method::PUT,
              "me/player/play",
              &[("device_id", device_id.clone())],
              body,
            )
            .await
          {
            Ok(_) => {
              if let Err(e) = self
                .spotify_api_request_json(
                  Method::PUT,
                  "me/player/shuffle",
                  &[
                    ("state", desired_shuffle_state.to_string()),
                    ("device_id", device_id.clone()),
                  ],
                  None,
                )
                .await
              {
                let mut app = self.app.lock().await;
                app.handle_error(anyhow!(e));
              }

              let mut app = self.app.lock().await;
              app.instant_since_last_current_playback_poll =
                Instant::now() - Duration::from_secs(6);
              if let Some(ctx) = &mut app.current_playback_context {
                ctx.is_playing = true;
                ctx.shuffle_state = desired_shuffle_state;
              }
              app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
              app.dispatch(IoEvent::GetCurrentPlayback);
              return;
            }
            Err(e) => {
              info!(
                "native context playback via Spotify API failed; falling back to direct native load: {}",
                e
              );
            }
          }
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

        info!("starting native playback via direct load route");
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

    let body = api_playback_body(context_id.as_ref(), uris.as_deref(), offset);
    let result = self
      .spotify_api_request_json(Method::PUT, "me/player/play", &[], body)
      .await;

    match result {
      Ok(_) => {
        if let Err(e) = self
          .spotify_api_request_json(
            Method::PUT,
            "me/player/shuffle",
            &[("state", desired_shuffle_state.to_string())],
            None,
          )
          .await
        {
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

    match self
      .spotify_api_request_json(Method::PUT, "me/player/pause", &[], None)
      .await
    {
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

    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/next", &[], None)
      .await
    {
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

    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/previous", &[], None)
      .await
    {
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
    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/previous", &[], None)
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
      return;
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/previous", &[], None)
      .await
    {
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
      .spotify_api_request_json(
        Method::PUT,
        "me/player/seek",
        &[("position_ms", position_ms.to_string())],
        None,
      )
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

    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/shuffle",
        &[("state", shuffle_state.to_string())],
        None,
      )
      .await
    {
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

    let repeat_state_param: &'static str = repeat_state.into();
    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/repeat",
        &[("state", repeat_state_param.to_string())],
        None,
      )
      .await
    {
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

    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/volume",
        &[("volume_percent", volume.to_string())],
        None,
      )
      .await
    {
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
          app.native_playback_origin = None;
          app.last_device_activation = Some(Instant::now());
          app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(6);
          return;
        }
      }
    }

    if let Err(e) = self
      .spotify_api_request_json(
        Method::PUT,
        "me/player",
        &[],
        Some(json!({
          "device_ids": [device_id.clone()],
          "play": true
        })),
      )
      .await
    {
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
        app.native_playback_origin = None;
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

        match self
          .spotify_get_typed::<DevicePayload>("me/player/devices", &[])
          .await
        {
          Ok(payload) => {
            if let Some(device) = payload
              .devices
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
    let context = self
      .spotify_get_typed::<Option<rspotify::model::CurrentPlaybackContext>>("me/player", &[])
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
    match self
      .spotify_api_request_json(
        Method::POST,
        "me/player/queue",
        &[("uri", item.uri())],
        None,
      )
      .await
    {
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
    match self
      .spotify_get_typed::<CurrentUserQueue>("me/player/queue", &[])
      .await
    {
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
  use rspotify::model::{
    artist::SimplifiedArtist, idtypes::TrackId, track::FullTrack, SimplifiedAlbum,
  };
  use rspotify::prelude::Id;
  use std::collections::HashMap;

  fn playable_track(id: &str) -> PlayableId<'static> {
    PlayableId::Track(TrackId::from_id(id).unwrap().into_static())
  }

  #[allow(deprecated)]
  fn full_track(id: &str, name: &str) -> PlayableItem {
    PlayableItem::Track(FullTrack {
      album: SimplifiedAlbum {
        name: "Album".to_string(),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: TimeDelta::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: Some(TrackId::from_id(id).unwrap().into_static()),
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: name.to_string(),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    })
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

    let offset = api_playback_offset_json(Some(&uris), Some(1));

    assert_eq!(
      offset,
      Some(json!({ "uri": "spotify:track:0000000000000000000001" }))
    );
  }

  #[test]
  fn api_playback_offset_uses_position_for_uri_list_playback() {
    let offset = api_playback_offset_json(None, Some(1));

    assert_eq!(offset, Some(json!({ "position": 1 })));
  }

  #[test]
  fn api_playback_offset_falls_back_to_position_when_context_has_no_uri() {
    let offset = api_playback_offset_json(None, Some(3));

    assert_eq!(offset, Some(json!({ "position": 3 })));
  }

  #[test]
  fn api_confirms_native_info_when_names_match() {
    let item = full_track("0000000000000000000001", "Current Song");

    assert!(api_confirms_native_info_is_current(
      "Current Song",
      &item,
      Some("different-id")
    ));
  }

  #[test]
  fn api_confirms_native_info_when_current_id_matches_even_if_name_differs() {
    let item = full_track("0000000000000000000001", "Stranger Thing");

    assert!(api_confirms_native_info_is_current(
      "Greater Together",
      &item,
      Some("0000000000000000000001")
    ));
  }

  #[test]
  fn api_does_not_confirm_stale_api_item_for_different_native_track() {
    let item = full_track("0000000000000000000001", "Old API Song");

    assert!(!api_confirms_native_info_is_current(
      "New Native Song",
      &item,
      Some("0000000000000000000002")
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_api_item_keeps_native_metadata_when_native_was_active() {
    assert!(stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_api_item_keeps_native_metadata_during_activation() {
    assert!(stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: false,
        native_activation_pending: true,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_api_item_keeps_native_context_before_native_metadata_arrives() {
    assert!(stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: false,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_native_metadata_clears_after_playback_leaves_native_device() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: false,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn confirmed_api_item_no_longer_keeps_native_metadata() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: true,
        native_track_id_present: true,
        api_item_matches_native_track: true,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: true,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn matching_api_item_without_native_metadata_can_update_context() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: false,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: true,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn api_item_without_native_track_id_can_update_context() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: false,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: false,
        api_item_matches_native_track: false,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }
}
