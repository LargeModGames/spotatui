use crate::core::app::{self, App, NativeTrackKind};
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
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  pub windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
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
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  pub windows_media_manager: Option<Arc<smtc_tokio::WindowsMediaManager>>,
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
      // A live player already exists (e.g. a queued duplicate request): the
      // pending window is over, so replay anything parked against it.
      let mut app = ctx.app.lock().await;
      app.native_backend_pending = false;
      app.replay_pending_start_playback();
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
          // A disconnected old player may still be referenced here; shut its
          // spirc down before replacing it so it can't leave a ghost device (#297).
          if let Some(old) = app.streaming_player.take() {
            if !Arc::ptr_eq(&old, &recovered_player) {
              old.shutdown();
            }
          }
          app.streaming_player = Some(Arc::clone(&recovered_player));
          app.set_status_message("Native streaming recovered.", 6);
          if request.reselect_device {
            app.dispatch(IoEvent::AutoSelectStreamingDevice(
              ctx.client_config.streaming_device_name.clone(),
              false,
            ));
          }
          // Replay the request that triggered (or arrived during) recovery.
          // Dispatched after AutoSelectStreamingDevice, so the serial pump
          // completes the device selection before the playback starts.
          app.native_backend_pending = false;
          app.replay_pending_start_playback();
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
          #[cfg(all(feature = "windows-media", target_os = "windows"))]
          windows_media_manager: ctx.windows_media_manager.clone(),
        });
      }
      Err(e) => {
        info!("native streaming recovery failed: {}", e);
        let mut app = ctx.app.lock().await;
        app.native_backend_pending = false;
        if app.pending_start_playback.take().is_some() {
          app.set_status_message(
            format!("Native recovery failed; playback request dropped: {}", e),
            8,
          );
        } else {
          app.set_status_message(format!("Native recovery failed: {}", e), 8);
        }
      }
    }
  }
}

/// Get the currently active streaming player (if any).
pub async fn active_streaming_player(app: &Arc<Mutex<App>>) -> Option<Arc<StreamingPlayer>> {
  let app_lock = app.lock().await;
  app_lock
    .streaming_player
    .as_ref()
    .filter(|player| player.is_connected())
    .cloned()
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
  #[cfg(all(feature = "windows-media", target_os = "windows"))]
  let windows_media_manager = ctx.windows_media_manager.clone();

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
      #[cfg(all(feature = "windows-media", target_os = "windows"))]
      windows_media_manager,
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
  #[cfg(all(feature = "windows-media", target_os = "windows"))] windows_media_manager: Option<
    Arc<smtc_tokio::WindowsMediaManager>,
  >,
) {
  use chrono::TimeDelta;

  // Count consecutive failed (Unavailable) loads so we can escalate the message
  // when an account is hit by the upstream Spotify audio-key block (#282). A
  // single genuinely-unavailable track only trips the mild message and resets on
  // the next successful Playing.
  let mut consecutive_unavailable: u32 = 0;
  const UNAVAILABLE_ESCALATION_THRESHOLD: u32 = 3;

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
        // While the native queue is mid-handoff or playing a *decoded* track,
        // librespot must stay paused. The handoff pauses Spirc, but a
        // self-advance load (or a stale-slot reissue) already in flight at that
        // moment can complete afterwards and start audio over the queue slot —
        // re-pause instead of accepting the state update. Librespot playing is
        // legitimate here only when the slot itself is a Spotify track; with a
        // decoded slot, or with a context suspended under the queue and no
        // Spotify slot (the between-items window: the old slot is cleared, the
        // next one not yet published), it never is. One-shot: a paused Spirc
        // emits no further Playing events, so this can't ping-pong.
        {
          let stray_over_queue = {
            let guard = app.lock().await;
            let decoded_slot = {
              #[cfg(any(feature = "local-files", feature = "subsonic", feature = "youtube"))]
              {
                guard.queue_now_decoded_player().is_some()
              }
              // Without a queueable decoded source the slot can never be
              // decoded (internet radio enables `audio-decode` but is never
              // queued), so there is nothing to shadow librespot here.
              #[cfg(not(any(feature = "local-files", feature = "subsonic", feature = "youtube")))]
              {
                false
              }
            };
            !guard.queue_now_is_spotify() && (decoded_slot || guard.queue_suspended.is_some())
          };
          if stray_over_queue {
            player.pause();
            continue;
          }
        }

        // Playback is actually working: reset the failure streak.
        consecutive_unavailable = 0;
        shared_is_playing.store(true, Ordering::Relaxed);

        #[cfg(all(feature = "mpris", target_os = "linux"))]
        if let Some(ref mpris) = mpris_manager {
          mpris.set_playback_status(true);
        }

        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if let Some(ref macos_media) = macos_media_manager {
          macos_media.set_playback_status(true);
        }

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_playback_status(true);
        }

        {
          let mut app_lock = app.lock().await;
          app_lock.native_is_playing = Some(true);
          // A real Playing event proves the session is alive: disarm the load
          // watchdog and drop any request parked for its potential recovery.
          app_lock.native_load_watchdog = None;
          app_lock.pending_start_playback = None;
          app_lock.native_backend_pending = false;
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_playback_status(false);
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_position(position_ms as u64);
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

        let (artists, album, kind) = match &audio_item.unique_fields {
          UniqueFields::Track { artists, album, .. } => {
            let artist_names: Vec<String> = artists.0.iter().map(|a| a.name.clone()).collect();
            (artist_names, album.clone(), NativeTrackKind::Track)
          }
          UniqueFields::Episode { show_name, .. } => (
            vec![show_name.clone()],
            String::new(),
            NativeTrackKind::Episode,
          ),
          UniqueFields::Local { artists, album, .. } => {
            let artist_vec = artists
              .as_ref()
              .map(|a| vec![a.clone()])
              .unwrap_or_default();
            let album_str = album.clone().unwrap_or_default();
            (artist_vec, album_str, NativeTrackKind::Track)
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_metadata(
            &audio_item.name,
            &artists,
            &album,
            audio_item.duration_ms as u64,
            None,
          );
        }

        let mut app = app.lock().await;
        // A TrackChanged proves the session is processing loads: disarm the
        // zombie-session watchdog.
        app.native_load_watchdog = None;
        app.pending_start_playback = None;
        app.native_backend_pending = false;
        app.native_track_info = Some(app::NativeTrackInfo {
          name: audio_item.name.clone(),
          artists_display: artists.join(", "),
          album: album.clone(),
          duration_ms: audio_item.duration_ms,
          kind,
        });

        app.song_progress_ms = 0;
        let playing_id = audio_item.track_id.to_string();
        app.last_track_id = Some(playing_id.clone());
        app.instant_since_last_current_playback_poll = std::time::Instant::now();
        app.dispatch(IoEvent::GetCurrentPlayback);

        // Spirc self-advance guard: a queued Spotify track plays via a direct
        // `player.load` (no Spirc context), so Spirc can switch to the next
        // context track on its own. If that happened, reissue the queued track
        // (bounded). NOTE: pending the live experiment in the plan (Risk #1),
        // this mitigation is unverified without a real Spotify session.
        let reload_uri = app.spotify_queue_guard_reload_uri(&playing_id);
        drop(app);
        if let Some(uri) = reload_uri {
          info!("spirc advanced off the queued track; reissuing {}", uri);
          if let Err(e) = player.play_uri(&uri).await {
            info!("failed to reissue queued Spotify track: {}", e);
          }
        }
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_stopped();
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_stopped();
        }

        // Full `lock().await`, not `try_lock`: this arm decides whether the
        // native queue takes over, and a dropped decision here strands the
        // queue (nothing advances, nothing continues). The render loop holds
        // the app mutex a large fraction of the time, so a single try_lock
        // attempt loses this race routinely.
        {
          let mut app = app.lock().await;
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
        {
          let mut app = app.lock().await;
          if !app.user_config.behavior.stop_after_current_track {
            // The native queue takes priority: a queued Spotify track that just
            // ended advances the queue; a context track that ended while items
            // wait suspends the context (preempting Spirc's self-advance) and
            // hands off to the queue. Only when neither applies do we fall back
            // to the normal continue-playback path.
            if !app.handle_native_spotify_track_end() {
              app.dispatch(IoEvent::EnsurePlaybackContinues(track_id.to_string()));
            }
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_position(position_ms as u64);
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

        #[cfg(all(feature = "windows-media", target_os = "windows"))]
        if let Some(ref windows_media) = windows_media_manager {
          windows_media.set_stopped();
        }

        if let Some(request) = disconnect_streaming_player(
          &app,
          &player,
          &shared_position,
          &shared_is_playing,
          "Native streaming disconnected; attempting recovery.",
          false,
        )
        .await
        {
          let _ = recovery_tx.send(request);
        }
        return;
      }
      PlayerEvent::Unavailable { track_id, .. } => {
        // librespot emits Unavailable when a track can't be loaded — including
        // when Spotify rejects the audio key (`error audio key 0 1`), which makes
        // decryption fail. This was previously dropped by the `_` arm, so the
        // failure was completely silent (#282). Surface it to the user.
        consecutive_unavailable += 1;

        // Clear the ghost native track and the request/watchdog together so an
        // unavailable track cannot be replayed as a supposed session failure.
        {
          let mut app = app.lock().await;
          app.song_progress_ms = 0;
          app.native_track_info = None;
          app.native_load_watchdog = None;
          app.pending_start_playback = None;
          app.native_backend_pending = false;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
        }

        info!(
          "native playback unavailable (track {}, consecutive {})",
          track_id, consecutive_unavailable
        );

        // Once several consecutive tracks fail to load (genuinely unavailable,
        // e.g. region-locked or the audio-key block), halt playback instead of
        // letting librespot auto-skip through the entire queue at machine speed;
        // that stampede hammers Spotify and can get the account rate-limited.
        if consecutive_unavailable >= UNAVAILABLE_ESCALATION_THRESHOLD {
          player.pause();
        }

        // Emit on the threshold transitions only (== not >=) so we don't spam the
        // same message on every auto-skip during an account-wide failure.
        if consecutive_unavailable == 1 {
          let mut app = app.lock().await;
          app.set_status_message(
            "Couldn't play this track natively (unavailable or blocked); skipping.",
            6,
          );
        } else if consecutive_unavailable == UNAVAILABLE_ESCALATION_THRESHOLD {
          let mut app = app.lock().await;
          app.set_status_message(
            "Several tracks in a row couldn't be played natively, so playback was stopped. They may be unavailable on your account or region. Press 'd' to switch to an official Spotify Connect device.",
            20,
          );
        }
      }
      PlayerEvent::Loading { .. } | PlayerEvent::Preloading { .. } => {
        let mut app = app.lock().await;
        if app.native_load_watchdog.is_some() {
          app.native_load_watchdog = Some(std::time::Instant::now());
        }
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
    true,
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

  ctx.device.name.eq_ignore_ascii_case(player.device_name()) && app.has_fresh_native_activity()
}

async fn disconnect_streaming_player(
  app: &Arc<Mutex<App>>,
  player: &Arc<StreamingPlayer>,
  shared_position: &Arc<AtomicU64>,
  shared_is_playing: &Arc<AtomicBool>,
  status_message: &str,
  allow_reselect_device: bool,
) -> Option<StreamingRecoveryRequest> {
  let mut app_lock = app.lock().await;
  let current_player = app_lock.streaming_player.as_ref()?;
  if !Arc::ptr_eq(current_player, player) {
    return None;
  }

  // Spotify Connect sends SessionDisconnected when the user intentionally moves
  // playback to another device. At that point the API context can still be the
  // old native device, so only reselect native for non-Connect-disconnect paths.
  let reselect_device = allow_reselect_device && current_playback_matches_native(&app_lock, player);

  app_lock.streaming_player = None;
  // Stop the old Connect session so it doesn't linger as a ghost device (#297).
  player.shutdown();
  app_lock.is_streaming_active = false;
  app_lock.native_activation_pending = false;
  app_lock.native_device_id = None;
  app_lock.native_is_playing = Some(false);
  app_lock.native_track_info = None;
  app_lock.native_playback_origin = None;
  app_lock.song_progress_ms = 0;
  app_lock.last_track_id = None;
  app_lock.last_device_activation = None;
  app_lock.seek_ms = None;
  // The cached API context may still point at the stale native session; the
  // dispatch below repopulates it if Spotify has already moved to another device.
  app_lock.current_playback_context = None;
  app_lock.set_status_message(status_message, 8);
  app_lock.dispatch(IoEvent::GetCurrentPlayback);

  shared_position.store(0, Ordering::Relaxed);
  shared_is_playing.store(false, Ordering::Relaxed);

  Some(StreamingRecoveryRequest { reselect_device })
}
