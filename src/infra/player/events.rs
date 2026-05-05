use crate::core::app::{self, App};
use crate::core::config::ClientConfig;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::IoEvent;
use crate::infra::player::{get_default_cache_path, PlayerEvent, StreamingConfig, StreamingPlayer};
use log::info;
use std::sync::{
  atomic::{AtomicBool, AtomicU64, Ordering},
  Arc,
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Default)]
pub struct StreamingRecoveryRequest {
  pub reselect_device: bool,
}

/// Bundled context for player event handling tasks.
/// Groups all shared state and managers needed by event handlers.
pub struct PlayerEventContext {
  pub player: Arc<StreamingPlayer>,
  pub app: Arc<Mutex<App>>,
  pub shared_position: Arc<AtomicU64>,
  pub shared_is_playing: Arc<AtomicBool>,
  pub recovery_tx: tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
}

pub struct StreamingRecoveryContext {
  pub app: Arc<Mutex<App>>,
  pub shared_position: Arc<AtomicU64>,
  pub shared_is_playing: Arc<AtomicBool>,
  pub recovery_rx: tokio::sync::mpsc::UnboundedReceiver<StreamingRecoveryRequest>,
  pub recovery_tx: tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  pub client_config: ClientConfig,
  pub redirect_uri: String,
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  pub macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,
}

pub fn spawn_streaming_recovery_handler(ctx: StreamingRecoveryContext) {
  tokio::spawn(async move {
    handle_streaming_recovery(ctx).await;
  });
}

async fn handle_streaming_recovery(mut ctx: StreamingRecoveryContext) {
  while let Some(mut request) = ctx.recovery_rx.recv().await {
    while let Ok(next_request) = ctx.recovery_rx.try_recv() {
      request.reselect_device |= next_request.reselect_device;
    }

    if active_streaming_player(&ctx.app).await.is_some() {
      continue;
    }

    let initial_volume = {
      let app = ctx.app.lock().await;
      app.user_config.behavior.volume_percent
    };

    let streaming_config = StreamingConfig {
      device_name: ctx.client_config.streaming_device_name.clone(),
      bitrate: ctx.client_config.streaming_bitrate,
      audio_cache: ctx.client_config.streaming_audio_cache,
      cache_path: get_default_cache_path(),
      initial_volume,
    };

    info!("attempting native streaming recovery");

    match StreamingPlayer::new_cache_only(
      &ctx.client_config.client_id,
      &ctx.redirect_uri,
      streaming_config,
    )
    .await
    {
      Ok(recovered_player) => {
        let recovered_player = Arc::new(recovered_player);
        {
          let mut app = ctx.app.lock().await;
          app.streaming_player = Some(Arc::clone(&recovered_player));
          app.set_status_message("Native streaming recovered.", 6);
          if request.reselect_device {
            app.dispatch(IoEvent::AutoSelectStreamingDevice(
              ctx.client_config.streaming_device_name.clone(),
              false,
            ));
          }
        }

        spawn_player_event_handler(PlayerEventContext {
          player: recovered_player,
          app: Arc::clone(&ctx.app),
          shared_position: Arc::clone(&ctx.shared_position),
          shared_is_playing: Arc::clone(&ctx.shared_is_playing),
          recovery_tx: ctx.recovery_tx.clone(),
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          mpris_manager: ctx.mpris_manager.clone(),
          #[cfg(all(feature = "macos-media", target_os = "macos"))]
          macos_media_manager: ctx.macos_media_manager.clone(),
        });
      }
      Err(e) => {
        info!("native streaming recovery failed: {}", e);
        let mut app = ctx.app.lock().await;
        app.set_status_message(format!("Native recovery failed: {}", e), 8);
      }
    }
  }
}

/// Get the currently active streaming player (if any).
pub async fn active_streaming_player(app: &Arc<Mutex<App>>) -> Option<Arc<StreamingPlayer>> {
  let app_lock = app.lock().await;
  app_lock.streaming_player.clone()
}

pub fn spawn_player_event_handler(ctx: PlayerEventContext) {
  let event_rx = ctx.player.get_event_channel();
  info!("spawning native player event handler");

  let player = ctx.player.clone();
  let app = Arc::clone(&ctx.app);
  let shared_position = Arc::clone(&ctx.shared_position);
  let shared_is_playing = Arc::clone(&ctx.shared_is_playing);
  let recovery_tx = ctx.recovery_tx.clone();
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  let mpris_manager = ctx.mpris_manager.clone();
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  let macos_media_manager = ctx.macos_media_manager.clone();

  tokio::spawn(async move {
    handle_player_events(
      event_rx,
      player,
      app,
      shared_position,
      shared_is_playing,
      recovery_tx,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager,
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      macos_media_manager,
    )
    .await;
  });
}

/// Handle player events from librespot and update app state directly.
/// This bypasses the Spotify Web API for instant UI updates.
async fn handle_player_events(
  mut event_rx: librespot_playback::player::PlayerEventChannel,
  player: Arc<StreamingPlayer>,
  app: Arc<Mutex<App>>,
  shared_position: Arc<AtomicU64>,
  shared_is_playing: Arc<AtomicBool>,
  recovery_tx: tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  #[cfg(all(feature = "mpris", target_os = "linux"))] mpris_manager: Option<
    Arc<mpris::MprisManager>,
  >,
  #[cfg(all(feature = "macos-media", target_os = "macos"))] macos_media_manager: Option<
    Arc<macos_media::MacMediaManager>,
  >,
) {
  use chrono::TimeDelta;

  while let Some(event) = event_rx.recv().await {
    if !is_current_streaming_player(&app, &player).await {
      continue;
    }

    match event {
      PlayerEvent::Playing {
        play_request_id: _,
        track_id,
        position_ms,
      } => {
        shared_is_playing.store(true, Ordering::Relaxed);

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(true);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_playback_status(true);
        }

        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(true);
        }

        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = true;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }

          app.instant_since_last_current_playback_poll = std::time::Instant::now();

          let track_id_str = track_id.to_string();
          if app.last_track_id.as_ref() != Some(&track_id_str) {
            app.last_track_id = Some(track_id_str);
            app.dispatch(IoEvent::GetCurrentPlayback);
          }
          if app.pending_stop_after_track {
            app.pending_stop_after_track = false;
            if let Some(ref mut ctx) = app.current_playback_context {
              ctx.is_playing = false;
            }
            app.dispatch(IoEvent::PausePlayback);
          }
        }
      }
      PlayerEvent::Paused {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        shared_is_playing.store(false, Ordering::Relaxed);

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(false);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_playback_status(false);
        }

        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(false);
        }

        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::Seeked {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_position(position_ms as u64);
        }

        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;
          app.seek_ms = None;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::TrackChanged { audio_item } => {
        use librespot_metadata::audio::UniqueFields;

        let (artists, album) = match &audio_item.unique_fields {
          UniqueFields::Track { artists, album, .. } => {
            let artist_names: Vec<String> = artists.0.iter().map(|a| a.name.clone()).collect();
            (artist_names, album.clone())
          }
          UniqueFields::Episode { show_name, .. } => (vec![show_name.clone()], String::new()),
          UniqueFields::Local { artists, album, .. } => {
            let artist_vec = artists
              .as_ref()
              .map(|a| vec![a.clone()])
              .unwrap_or_default();
            let album_str = album.clone().unwrap_or_default();
            (artist_vec, album_str)
          }
        };

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms,
            None,
          );
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms,
            None,
          );
        }

        let mut app = app.lock().await;
        app.native_track_info = Some(app::NativeTrackInfo {
          name: audio_item.name.clone(),
          artists_display: artists.join(", "),
          album: album.clone(),
          duration_ms: audio_item.duration_ms,
        });

        app.song_progress_ms = 0;
        app.last_track_id = Some(audio_item.track_id.to_string());
        app.instant_since_last_current_playback_poll = std::time::Instant::now();
        app.dispatch(IoEvent::GetCurrentPlayback);
      }
      PlayerEvent::Stopped { .. } => {
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_stopped();
        }

        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
          app.native_track_info = None;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if let Ok(mut app) = app.try_lock() {
          app.dispatch(IoEvent::GetCurrentPlayback);
        }
      }
      PlayerEvent::EndOfTrack { track_id, .. } => {
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_stopped();
        }

        if let Ok(mut app) = app.try_lock() {
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
          app.native_track_info = None;
          if app.user_config.behavior.stop_after_current_track {
            app.pending_stop_after_track = true;
          }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(mut app) = app.try_lock() {
          if !app.user_config.behavior.stop_after_current_track {
            app.dispatch(IoEvent::EnsurePlaybackContinues(track_id.to_string()));
          }
        }
      }
      PlayerEvent::VolumeChanged { volume } => {
        let volume_percent = ((volume as f64 / 65535.0) * 100.0).round() as u8;
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_volume(volume_percent);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_volume(volume_percent);
        }

        if let Ok(mut app) = app.try_lock() {
          if let Some(pending) = app.pending_volume {
            if volume_percent == pending {
              app.pending_volume = None;
              app.last_dispatched_volume = None;
            }
          } else {
            if let Some(ref mut ctx) = app.current_playback_context {
              ctx.device.volume_percent = Some(volume_percent as u32);
            }
            app.user_config.behavior.volume_percent = volume_percent.min(100);
            let _ = app.user_config.save_config();
          }
        }
      }
      PlayerEvent::PositionChanged {
        play_request_id: _,
        track_id: _,
        position_ms,
      } => {
        shared_position.store(position_ms as u64, Ordering::Relaxed);

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_position(position_ms as u64);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_position(position_ms as u64);
        }
      }
      PlayerEvent::SessionDisconnected { .. } => {
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_stopped();
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_stopped();
        }

        if let Some(request) = disconnect_streaming_player(
          &app,
          &player,
          &shared_position,
          &shared_is_playing,
          "Native streaming disconnected; attempting recovery.",
        )
        .await
        {
          let _ = recovery_tx.send(request);
        }
        return;
      }
      _ => {}
    }
  }

  if let Some(request) = disconnect_streaming_player(
    &app,
    &player,
    &shared_position,
    &shared_is_playing,
    "Native streaming stopped; attempting recovery.",
  )
  .await
  {
    let _ = recovery_tx.send(request);
  }
}

async fn is_current_streaming_player(app: &Arc<Mutex<App>>, player: &Arc<StreamingPlayer>) -> bool {
  let app_lock = app.lock().await;
  app_lock
    .streaming_player
    .as_ref()
    .is_some_and(|current| Arc::ptr_eq(current, player))
}

fn current_playback_matches_native(app: &App, player: &StreamingPlayer) -> bool {
  let Some(ctx) = app.current_playback_context.as_ref() else {
    return app.is_streaming_active;
  };

  if let Some(native_id) = app.native_device_id.as_ref() {
    if ctx.device.id.as_ref() == Some(native_id) {
      return true;
    }
  }

  ctx.device.name.eq_ignore_ascii_case(player.device_name())
}

async fn disconnect_streaming_player(
  app: &Arc<Mutex<App>>,
  player: &Arc<StreamingPlayer>,
  shared_position: &Arc<AtomicU64>,
  shared_is_playing: &Arc<AtomicBool>,
  status_message: &str,
) -> Option<StreamingRecoveryRequest> {
  let mut app_lock = app.lock().await;
  let current_player = app_lock.streaming_player.as_ref()?;
  if !Arc::ptr_eq(current_player, player) {
    return None;
  }

  let reselect_device = current_playback_matches_native(&app_lock, player);

  app_lock.streaming_player = None;
  app_lock.is_streaming_active = false;
  app_lock.native_activation_pending = false;
  app_lock.native_device_id = None;
  app_lock.native_is_playing = Some(false);
  app_lock.native_track_info = None;
  app_lock.song_progress_ms = 0;
  app_lock.last_track_id = None;
  app_lock.last_device_activation = None;
  app_lock.seek_ms = None;
  if reselect_device {
    app_lock.current_playback_context = None;
  }
  app_lock.set_status_message(status_message, 8);
  app_lock.dispatch(IoEvent::GetCurrentPlayback);

  shared_position.store(0, Ordering::Relaxed);
  shared_is_playing.store(false, Ordering::Relaxed);

  Some(StreamingRecoveryRequest { reselect_device })
}
