use crate::core::app::{self, App, NativeTrackKind};
use crate::core::config::ClientConfig;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::IoEvent;
use crate::infra::player::{
  get_default_cache_path, PlayerEvent, StreamingConfig, StreamingConnectionState, StreamingPlayer,
};
use log::info;
use std::sync::{
  atomic::{AtomicBool, AtomicU64, Ordering},
  Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const STALLED_PLAYBACK_RECOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const END_OF_TRACK_CONTINUATION_DELAY: Duration = Duration::from_millis(500);
const END_OF_TRACK_RECONNECT_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone, Default)]
pub struct StreamingRecoveryRequest {
  pub reselect_device: bool,
  pub continue_after_track: Option<String>,
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
      if next_request.continue_after_track.is_some() {
        request.continue_after_track = next_request.continue_after_track;
      }
    }

    if active_streaming_player(&ctx.app).await.is_some() {
      // A live player already exists (e.g. a queued duplicate request): the
      // pending window is over, so replay anything parked against it.
      let mut app = ctx.app.lock().await;
      app.native_backend_pending = false;
      if app.pending_start_playback.is_some() {
        app.replay_pending_start_playback();
      } else if let Some(previous_track_id) = request.continue_after_track {
        if app.native_transition_has_advanced(&previous_track_id) {
          if let Some(generation) = app.native_playback_restore_generation() {
            app.dispatch(IoEvent::RestoreNativePlayback(generation));
          }
        } else {
          app.dispatch(IoEvent::EnsurePlaybackContinues(previous_track_id));
        }
      } else if request.reselect_device {
        if let Some(generation) = app.native_playback_restore_generation() {
          app.dispatch(IoEvent::RestoreNativePlayback(generation));
        }
      }
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
          app.native_backend_pending = false;
        }

        spawn_player_event_handler(PlayerEventContext {
          player: Arc::clone(&recovered_player),
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

        let mut app = ctx.app.lock().await;
        if request.reselect_device {
          app.dispatch(IoEvent::AutoSelectStreamingDevice(
            ctx.client_config.streaming_device_name.clone(),
            false,
          ));
        }
        // A new explicit playback request wins over restoring older intent.
        // Both paths are queued after device selection on the serial IoEvent pump.
        if app.pending_start_playback.is_some() {
          app.replay_pending_start_playback();
        } else if let Some(previous_track_id) = request.continue_after_track {
          if app.native_transition_has_advanced(&previous_track_id) {
            if let Some(generation) = app.native_playback_restore_generation() {
              app.dispatch(IoEvent::RestoreNativePlayback(generation));
            }
          } else {
            app.dispatch(IoEvent::EnsurePlaybackContinues(previous_track_id));
          }
        } else if request.reselect_device {
          if let Some(generation) = app.native_playback_restore_generation() {
            app.dispatch(IoEvent::RestoreNativePlayback(generation));
          } else {
            app.set_status_message("Native streaming recovered.", 6);
          }
        } else {
          app.set_status_message("Native streaming recovered.", 6);
        }
      }
      Err(e) => {
        info!("native streaming recovery failed: {}", e);
        let mut app = ctx.app.lock().await;
        app.native_backend_pending = false;
        app.native_restore_pending = None;
        app.native_load_watchdog = None;
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
    .filter(|player| player.is_available())
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

  // The StreamingPlayer first reconnects its Session/Spirc in place, retaining
  // the Player and buffered audio. Only replace the whole backend when that
  // attempt fails or desired playback stops making progress.
  let mut connection_state_rx = player.connection_state_receiver();
  let mut session_lost = false;
  let mut fast_reconnect_failed = false;
  let mut progress_watchdog_armed = false;
  let mut transport_recovery_pending = false;
  let mut pending_end_of_track = None;
  let mut observed_connection_generation = 0_u64;
  let mut audibly_playing = shared_is_playing.load(Ordering::Relaxed);
  let mut last_position = shared_position.load(Ordering::Relaxed);
  let mut last_progress_at = Instant::now();
  let mut watchdog = tokio::time::interval(Duration::from_secs(1));
  watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let playback_transition_generation = Arc::new(AtomicU64::new(0));

  loop {
    let event = tokio::select! {
      maybe_event = event_rx.recv() => match maybe_event {
        Some(event) => event,
        None => break,
      },
      changed = connection_state_rx.changed() => {
        if changed.is_err() {
          return;
        }
        let connection_state = *connection_state_rx.borrow_and_update();
        match connection_state {
          StreamingConnectionState::Reconnecting { generation } => {
            observed_connection_generation = generation;
            session_lost = true;
            fast_reconnect_failed = false;
            progress_watchdog_armed = true;
            transport_recovery_pending = true;
            last_progress_at = Instant::now();
            info!("native streaming fast reconnect generation {} started", generation);
            let mut app = app.lock().await;
            app.native_backend_pending = true;
            app.set_status_message("Native streaming connection lost; reconnecting.", 6);
          }
          StreamingConnectionState::Connected { generation }
            if session_lost || generation != observed_connection_generation =>
          {
            observed_connection_generation = generation;
            session_lost = false;
            fast_reconnect_failed = false;
            progress_watchdog_armed = true;
            transport_recovery_pending = true;
            last_progress_at = Instant::now();
            info!("native streaming fast reconnect generation {} completed", generation);
            let mut app = app.lock().await;
            app.native_backend_pending = false;
            if app.native_load_watchdog.is_some() {
              app.native_load_watchdog = Some(Instant::now());
            }
            app.set_status_message("Native streaming connection recovered.", 5);
          }
          StreamingConnectionState::Connected { .. } => {}
          StreamingConnectionState::Failed { generation } => {
            session_lost = true;
            fast_reconnect_failed = true;
            progress_watchdog_armed = true;
            transport_recovery_pending = true;
            info!("native streaming fast reconnect generation {} failed", generation);
            if audibly_playing {
              continue;
            }
            request_full_streaming_recovery(
              &app,
              &player,
              &shared_position,
              &shared_is_playing,
              &recovery_tx,
              "Native streaming connection lost; attempting recovery.",
              pending_end_of_track.clone(),
            )
            .await;
            return;
          }
          StreamingConnectionState::Shutdown => return,
        }
        continue;
      }
      _ = watchdog.tick(), if progress_watchdog_armed => {
        let position = shared_position.load(Ordering::Relaxed);
        if position != last_position {
          last_position = position;
          last_progress_at = Instant::now();
        }
        let desired_playing = {
          let app = app.lock().await;
          app
            .native_playback_recovery
            .as_ref()
            .map_or_else(
              || shared_is_playing.load(Ordering::Relaxed),
              |snapshot| snapshot.desired_playing,
            )
        };
        if !desired_playing && !session_lost {
          progress_watchdog_armed = false;
          transport_recovery_pending = false;
          continue;
        }
        if recovery_watchdog_should_escalate(desired_playing, last_progress_at.elapsed()) {
          info!("native playback made no progress for {}s during session recovery; replacing backend", STALLED_PLAYBACK_RECOVERY_TIMEOUT.as_secs());
          request_full_streaming_recovery(
            &app,
            &player,
            &shared_position,
            &shared_is_playing,
            &recovery_tx,
            "Native streaming stalled; attempting recovery.",
            pending_end_of_track.clone(),
          )
          .await;
          return;
        }
        continue;
      }
    };

    if !is_current_streaming_player(&app, &player).await {
      continue;
    }

    match &event {
      PlayerEvent::Playing { .. } => {
        audibly_playing = true;
        pending_end_of_track = None;
        last_progress_at = Instant::now();
        if !session_lost {
          progress_watchdog_armed = false;
          transport_recovery_pending = false;
        }
      }
      PlayerEvent::Paused { .. } | PlayerEvent::Stopped { .. } | PlayerEvent::EndOfTrack { .. } => {
        audibly_playing = false;
      }
      PlayerEvent::Seeked { .. }
      | PlayerEvent::TrackChanged { .. }
      | PlayerEvent::Loading { .. } => {
        last_progress_at = Instant::now();
      }
      PlayerEvent::PositionChanged { position_ms, .. } => {
        let position = u64::from(*position_ms);
        if position != last_position {
          last_position = position;
          pending_end_of_track = None;
          last_progress_at = Instant::now();
          if !session_lost {
            progress_watchdog_armed = false;
            transport_recovery_pending = false;
          }
        }
      }
      _ => {}
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
        let track_uri = track_id.to_string();

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
          let restore_confirmed =
            app_lock.observe_native_playback_state(track_uri.clone(), position_ms, true);
          if app_lock.native_restore_pending.is_none() {
            app_lock.native_load_watchdog = None;
          }
          app_lock.pending_start_playback = None;
          app_lock.native_backend_pending = false;
          if restore_confirmed {
            app_lock.set_status_message("Native playback restored.", 5);
          }
        }

        if let Ok(mut app) = app.try_lock() {
          app.song_progress_ms = position_ms as u128;

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = true;
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }

          app.instant_since_last_current_playback_poll = std::time::Instant::now();

          let track_id_str = app::base62_id_of(&track_uri).to_string();
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
        track_id,
        position_ms,
      } => {
        shared_is_playing.store(false, Ordering::Relaxed);
        let track_uri = track_id.to_string();

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
          let restore_confirmed =
            app_lock.observe_native_playback_state(track_uri, position_ms, false);
          if app_lock.native_restore_pending.is_none() {
            app_lock.native_load_watchdog = None;
          }
          if restore_confirmed {
            app_lock.set_status_message("Native playback restored in paused state.", 5);
          }
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
          app.set_native_recovery_position(position_ms);

          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.progress = Some(TimeDelta::milliseconds(position_ms as i64));
          }
          app.instant_since_last_current_playback_poll = std::time::Instant::now();
        }
      }
      PlayerEvent::TrackChanged { audio_item } => {
        playback_transition_generation.fetch_add(1, Ordering::Relaxed);
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
        if app.native_restore_pending.is_none() {
          app.native_load_watchdog = None;
        }
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
        // librespot 0.8's `SpotifyUri` Display is the full `spotify:track:<id>`
        // URI; app-side ids (shuffle index sync, the queue guard,
        // `last_track_id`) are bare base62, so normalize at the event boundary.
        // The restore observer keeps the full URI (it canonicalizes internally).
        let playing_uri = audio_item.track_id.to_string();
        app.observe_native_track_changed(playing_uri.clone(), kind);
        let playing_id = app::base62_id_of(&playing_uri).to_string();
        app.last_track_id = Some(playing_id.clone());
        // Keep the client-side shuffle session pointed at what Spirc actually
        // plays (also detects a completed repeat-all lap for the reshuffle).
        app.sync_native_shuffle_index(&playing_id);
        app.instant_since_last_current_playback_poll = std::time::Instant::now();
        app.dispatch(IoEvent::GetCurrentPlayback);

        // Spirc self-advance guard: a queued Spotify track plays via a direct
        // `player.load` (no Spirc context), so Spirc can switch to the next
        // context track on its own. If that happened, reissue the queued track
        // (bounded). NOTE: pending the live experiment in the plan (Risk #1),
        // this mitigation is unverified without a real Spotify session.
        let reload_uri = app.spotify_queue_guard_reload_uri(&playing_id);
        let end_of_track_transition_advanced = pending_end_of_track
          .as_deref()
          .is_some_and(|previous| app.native_transition_has_advanced(previous));
        drop(app);
        if end_of_track_transition_advanced {
          pending_end_of_track = None;
        }
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
        // queue (nothing advances, nothing continues).
        let should_ensure_playback = {
          let mut app = app.lock().await;
          if let Some(ref mut ctx) = app.current_playback_context {
            ctx.is_playing = false;
          }
          app.song_progress_ms = 0;
          app.last_track_id = None;
          app.native_track_info = None;
          if app.user_config.behavior.stop_after_current_track {
            app.pending_stop_after_track = true;
            app.set_native_playback_intent(false);
            false
          } else {
            // The native queue takes priority: a queued Spotify track that just
            // ended advances the queue; a context track that ended while items
            // wait suspends the context (preempting Spirc's self-advance) and
            // hands off to the queue. Only when neither applies do we fall back
            // to the normal continue-playback path.
            !app.handle_native_spotify_track_end()
          }
        };

        if should_ensure_playback {
          // Recovery and queue continuation compare against the bare Web API
          // id, while librespot formats this event as a full Spotify URI.
          let ended_uri = track_id.to_string();
          let previous_track_id = app::base62_id_of(&ended_uri).to_string();
          pending_end_of_track = Some(previous_track_id.clone());
          progress_watchdog_armed = true;
          last_progress_at = Instant::now();
          spawn_end_of_track_continuation(
            Arc::clone(&app),
            Arc::clone(&player),
            previous_track_id,
            Arc::clone(&playback_transition_generation),
          );
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
        if let Ok(mut app) = app.try_lock() {
          app.set_native_recovery_position(position_ms);
        }

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

        let unexpected_disconnect = !player.is_connected();
        if let Some(request) = disconnect_streaming_player(
          &app,
          &player,
          &shared_position,
          &shared_is_playing,
          "Native streaming disconnected; attempting recovery.",
          unexpected_disconnect,
        )
        .await
        {
          let _ = recovery_tx.send(request);
        }
        return;
      }
      PlayerEvent::Unavailable { track_id, .. } => {
        if unavailable_is_transport_related(transport_recovery_pending, player.connection_state()) {
          info!(
            "native playback unavailable for track {} while the session transport is down; deferring to connection recovery",
            track_id
          );
          continue;
        }

        // librespot emits Unavailable when a track can't be loaded — including
        // when Spotify rejects the audio key (`error audio key 0 1`), which makes
        // decryption fail. This was previously dropped by the `_` arm, so the
        // failure was completely silent (#282). Surface it to the user.
        consecutive_unavailable += 1;
        last_progress_at = Instant::now();

        // Clear the ghost native track and the request/watchdog together so an
        // unavailable track cannot be replayed as a supposed session failure.
        {
          let mut app = app.lock().await;
          app.song_progress_ms = 0;
          app.native_track_info = None;
          app.native_load_watchdog = None;
          app.native_restore_pending = None;
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
          progress_watchdog_armed = false;
          pending_end_of_track = None;
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
      PlayerEvent::Loading {
        track_id,
        position_ms,
        ..
      } => {
        let mut app = app.lock().await;
        app.observe_native_loading(track_id.to_string(), position_ms);
        if app.native_load_watchdog.is_some() {
          app.native_load_watchdog = Some(std::time::Instant::now());
        }
      }
      PlayerEvent::Preloading { .. } => {}
      _ => {}
    }

    // A failed in-place reconnect owns no viable Spirc. Runs after event
    // bookkeeping so EndOfTrack still records queue/stop-after-current state.
    if session_lost && fast_reconnect_failed && !audibly_playing {
      info!("buffered playback ended after fast reconnect failure; replacing backend");
      request_full_streaming_recovery(
        &app,
        &player,
        &shared_position,
        &shared_is_playing,
        &recovery_tx,
        "Native streaming connection lost; attempting recovery.",
        pending_end_of_track.clone(),
      )
      .await;
      return;
    }
  }

  if matches!(
    player.connection_state(),
    StreamingConnectionState::Shutdown
  ) {
    return;
  }

  request_full_streaming_recovery(
    &app,
    &player,
    &shared_position,
    &shared_is_playing,
    &recovery_tx,
    "Native streaming stopped; attempting recovery.",
    pending_end_of_track,
  )
  .await;
}

fn recovery_watchdog_should_escalate(desired_playing: bool, stalled_for: Duration) -> bool {
  desired_playing && stalled_for >= STALLED_PLAYBACK_RECOVERY_TIMEOUT
}

fn unavailable_is_transport_related(
  transport_recovery_pending: bool,
  connection_state: StreamingConnectionState,
) -> bool {
  transport_recovery_pending
    || !matches!(connection_state, StreamingConnectionState::Connected { .. })
}

async fn request_full_streaming_recovery(
  app: &Arc<Mutex<App>>,
  player: &Arc<StreamingPlayer>,
  shared_position: &Arc<AtomicU64>,
  shared_is_playing: &Arc<AtomicBool>,
  recovery_tx: &tokio::sync::mpsc::UnboundedSender<StreamingRecoveryRequest>,
  status_message: &str,
  continue_after_track: Option<String>,
) {
  if let Some(mut request) = disconnect_streaming_player(
    app,
    player,
    shared_position,
    shared_is_playing,
    status_message,
    true,
  )
  .await
  {
    request.continue_after_track = continue_after_track;
    let _ = recovery_tx.send(request);
  }
}

fn spawn_end_of_track_continuation(
  app: Arc<Mutex<App>>,
  player: Arc<StreamingPlayer>,
  previous_track_id: String,
  playback_transition_generation: Arc<AtomicU64>,
) {
  let observed_transition_generation = playback_transition_generation.load(Ordering::Relaxed);
  let mut connection_state_rx = player.connection_state_receiver();

  tokio::spawn(async move {
    let wait_for_stable_connection = async {
      loop {
        let connection_state = *connection_state_rx.borrow_and_update();
        match connection_state {
          StreamingConnectionState::Connected { generation } => {
            tokio::time::sleep(END_OF_TRACK_CONTINUATION_DELAY).await;
            if matches!(
              *connection_state_rx.borrow(),
              StreamingConnectionState::Connected {
                generation: current_generation
              } if current_generation == generation
            ) {
              return true;
            }
          }
          StreamingConnectionState::Reconnecting { .. } => {
            if connection_state_rx.changed().await.is_err() {
              return false;
            }
          }
          StreamingConnectionState::Failed { .. } | StreamingConnectionState::Shutdown => {
            return false;
          }
        }
      }
    };

    if !tokio::time::timeout(END_OF_TRACK_RECONNECT_TIMEOUT, wait_for_stable_connection)
      .await
      .unwrap_or(false)
    {
      return;
    }

    // TrackChanged is proof that Spirc already advanced. The fallback must only
    // run when no transition was observed, otherwise it skips twice. Loading
    // alone is not sufficient: a dead connection can stall after that event.
    if playback_transition_generation.load(Ordering::Relaxed) != observed_transition_generation
      || !is_current_streaming_player(&app, &player).await
    {
      return;
    }

    app
      .lock()
      .await
      .dispatch(IoEvent::EnsurePlaybackContinues(previous_track_id));
  });
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
  if reselect_device {
    let position_ms = u32::try_from(shared_position.load(Ordering::Relaxed)).unwrap_or(u32::MAX);
    let is_playing = shared_is_playing.load(Ordering::Relaxed);
    app_lock.prepare_native_playback_recovery(position_ms, is_playing);
  } else {
    // An intentional Spotify Connect transfer must not be undone by a later
    // native recovery attempt.
    app_lock.clear_native_playback_recovery();
  }

  app_lock.streaming_player = None;
  // Stop the old Connect session so it doesn't linger as a ghost device (#297).
  player.shutdown();
  app_lock.is_streaming_active = false;
  app_lock.native_activation_pending = false;
  app_lock.native_device_id = None;
  app_lock.native_is_playing = Some(false);
  app_lock.native_track_info = None;
  app_lock.native_playback_origin = None;
  // Clearing the session below bumps its generation, which would turn a
  // shuffled queue suspension into a silent no-op at resume time; convert it
  // to a context snapshot first (while the cached context is still around).
  app_lock.convert_shuffled_suspension_to_context(None);
  app_lock.clear_native_shuffle_session();
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

  Some(StreamingRecoveryRequest {
    reselect_device,
    continue_after_track: None,
  })
}

#[cfg(test)]
mod tests {
  use super::{
    recovery_watchdog_should_escalate, unavailable_is_transport_related, StreamingConnectionState,
    STALLED_PLAYBACK_RECOVERY_TIMEOUT,
  };
  use std::time::Duration;

  #[test]
  fn recovery_watchdog_waits_for_the_full_stall_window() {
    assert!(!recovery_watchdog_should_escalate(
      true,
      STALLED_PLAYBACK_RECOVERY_TIMEOUT - Duration::from_millis(1)
    ));
    assert!(recovery_watchdog_should_escalate(
      true,
      STALLED_PLAYBACK_RECOVERY_TIMEOUT
    ));
  }

  #[test]
  fn recovery_watchdog_never_rebuilds_explicitly_paused_playback() {
    assert!(!recovery_watchdog_should_escalate(
      false,
      STALLED_PLAYBACK_RECOVERY_TIMEOUT * 2
    ));
  }

  #[test]
  fn unavailable_is_transport_related_until_recovery_progress_is_verified() {
    assert!(unavailable_is_transport_related(
      true,
      StreamingConnectionState::Connected { generation: 1 }
    ));
    assert!(unavailable_is_transport_related(
      false,
      StreamingConnectionState::Reconnecting { generation: 2 }
    ));
    assert!(!unavailable_is_transport_related(
      false,
      StreamingConnectionState::Connected { generation: 2 }
    ));
  }
}
