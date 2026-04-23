//! MPRIS D-Bus interface for desktop media control integration
//!
//! Exposes spotatui as a controllable media player via D-Bus, enabling:
//! - Media key support (play/pause, next, previous)
//! - Desktop environment integration (GNOME, KDE, etc.)
//! - playerctl command-line control
//!
//! This module is only available on Linux with the `mpris` feature enabled.

use crate::core::app::App;
use crate::core::playback_metadata::extract_playable_metadata;
use crate::infra::network::IoEvent;
use anyhow::Result;
use mpris_server::{Metadata, PlaybackStatus, Player, Time};
use rspotify::model::enums::RepeatState;
use std::{
  sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
  },
  thread,
};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

#[cfg(feature = "streaming")]
pub type StreamingPlayerHandle = Option<Arc<crate::infra::player::StreamingPlayer>>;
#[cfg(not(feature = "streaming"))]
pub type StreamingPlayerHandle = Option<()>;

#[derive(Default, PartialEq)]
struct MprisMetadata {
  title: String,
  artists: Vec<String>,
  album: String,
  duration_ms: u32,
  art_url: Option<String>,
}

type MprisMetadataTuple = (String, Vec<String>, String, u32, Option<String>);

#[derive(Default)]
pub struct MprisState {
  last_metadata: Option<MprisMetadata>,
  last_is_playing: Option<bool>,
  last_shuffle: Option<bool>,
  last_loop: Option<LoopStatusEvent>,
  last_position_ms: u64,
}

/// Events that can be received from external MPRIS clients (e.g., media keys, playerctl)
#[derive(Debug, Clone)]
pub enum MprisEvent {
  PlayPause,
  Play,
  Pause,
  Next,
  Previous,
  Stop,
  Seek(i64),        // Relative offset in microseconds
  SetPosition(i64), // Absolute position in microseconds
  SetShuffle(bool),
  SetLoopStatus(LoopStatusEvent),
}

/// Loop status from MPRIS (matches mpris_server::LoopStatus)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoopStatusEvent {
  None,
  Track,
  Playlist,
}

/// Commands to send TO the MPRIS server to update its state
#[derive(Debug, Clone)]
pub enum MprisCommand {
  Metadata {
    title: String,
    artists: Vec<String>,
    album: String,
    duration_ms: u32,
    art_url: Option<String>,
  },
  PlaybackStatus(bool),        // true = playing, false = paused
  Position(u64),               // position in milliseconds (silent update)
  Seeked(u64),                 // position in milliseconds (emits Seeked signal to notify clients)
  Volume(u8),                  // 0-100
  Shuffle(bool),               // shuffle state
  LoopStatus(LoopStatusEvent), // loop/repeat state
  Stopped,
}

/// Manager for the MPRIS D-Bus server
pub struct MprisManager {
  event_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<MprisEvent>>>,
  command_tx: mpsc::UnboundedSender<MprisCommand>,
}

pub fn update_state(manager: &MprisManager, state: &mut MprisState, app: &App) {
  if let Some((title, artists, album, duration_ms, art_url)) = metadata_from_app(app) {
    let new_metadata = MprisMetadata {
      title: title.clone(),
      artists: artists.clone(),
      album: album.clone(),
      duration_ms,
      art_url: art_url.clone(),
    };

    if state.last_metadata.as_ref() != Some(&new_metadata) {
      manager.set_metadata(&title, &artists, &album, duration_ms, art_url);
      state.last_metadata = Some(new_metadata);
    }

    let is_playing = app.native_is_playing.unwrap_or_else(|| {
      app
        .current_playback_context
        .as_ref()
        .map(|context| context.is_playing)
        .unwrap_or(false)
    });
    if state.last_is_playing != Some(is_playing) {
      manager.set_playback_status(is_playing);
      state.last_is_playing = Some(is_playing);
    }

    let position_ms = app.song_progress_ms as u64;
    if position_ms.abs_diff(state.last_position_ms) > 1000 {
      manager.set_position(position_ms);
      state.last_position_ms = position_ms;
    }

    let shuffle = app
      .current_playback_context
      .as_ref()
      .map(|context| context.shuffle_state)
      .unwrap_or(app.user_config.behavior.shuffle_enabled);
    if state.last_shuffle != Some(shuffle) {
      manager.set_shuffle(shuffle);
      state.last_shuffle = Some(shuffle);
    }

    if let Some(repeat_state) = app
      .current_playback_context
      .as_ref()
      .map(|context| context.repeat_state)
    {
      let loop_status = match repeat_state {
        RepeatState::Off => LoopStatusEvent::None,
        RepeatState::Track => LoopStatusEvent::Track,
        RepeatState::Context => LoopStatusEvent::Playlist,
      };
      if state.last_loop != Some(loop_status) {
        manager.set_loop_status(loop_status);
        state.last_loop = Some(loop_status);
      }
    }
  } else if state.last_metadata.is_some() {
    manager.set_stopped();
    state.last_metadata = None;
    state.last_is_playing = None;
  }
}

pub async fn handle_events(
  mut event_rx: mpsc::UnboundedReceiver<MprisEvent>,
  streaming_player: StreamingPlayerHandle,
  shared_is_playing: Arc<AtomicBool>,
  shared_position: Arc<AtomicU64>,
  mpris_manager: Arc<MprisManager>,
  app: Arc<Mutex<App>>,
) {
  #[cfg(not(feature = "streaming"))]
  let _ = (&streaming_player, &shared_is_playing, &shared_position);

  while let Some(event) = event_rx.recv().await {
    match event {
      MprisEvent::PlayPause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          if shared_is_playing.load(Ordering::Relaxed) {
            player.pause();
          } else {
            player.play();
          }
          continue;
        }

        let mut app_lock = app.lock().await;
        let is_playing = app_lock.native_is_playing.unwrap_or_else(|| {
          app_lock
            .current_playback_context
            .as_ref()
            .map(|context| context.is_playing)
            .unwrap_or(false)
        });
        if is_playing {
          app_lock.dispatch(IoEvent::PausePlayback);
        } else {
          app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
        }
      }
      MprisEvent::Play => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.play();
          continue;
        }

        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::StartPlayback(None, None, None));
      }
      MprisEvent::Pause => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.pause();
          continue;
        }

        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      MprisEvent::Next => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.activate();
          player.next();
          player.play();
          continue;
        }

        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::NextTrack);
      }
      MprisEvent::Previous => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.activate();
          player.prev();
          player.play();
          continue;
        }

        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PreviousTrack);
      }
      MprisEvent::Stop => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.stop();
          continue;
        }

        let mut app_lock = app.lock().await;
        app_lock.dispatch(IoEvent::PausePlayback);
      }
      MprisEvent::Seek(offset_micros) => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          let current_ms = shared_position.load(Ordering::Relaxed) as i64;
          let offset_ms = offset_micros / 1000;
          let new_position_ms = (current_ms + offset_ms).max(0) as u32;
          player.seek(new_position_ms);
          shared_position.store(new_position_ms as u64, Ordering::Relaxed);
          if let Ok(mut app_lock) = app.try_lock() {
            app_lock.song_progress_ms = new_position_ms as u128;
          }
          mpris_manager.emit_seeked(new_position_ms as u64);
          continue;
        }

        let mut app_lock = app.lock().await;
        let current_ms = app_lock.song_progress_ms as i64;
        let offset_ms = offset_micros / 1000;
        let new_position_ms = (current_ms + offset_ms).max(0) as u32;
        app_lock.song_progress_ms = new_position_ms as u128;
        app_lock.dispatch(IoEvent::Seek(new_position_ms));
        drop(app_lock);
        mpris_manager.emit_seeked(new_position_ms as u64);
      }
      MprisEvent::SetPosition(position_micros) => {
        let new_position_ms = (position_micros / 1000).max(0) as u32;

        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          player.seek(new_position_ms);
          shared_position.store(new_position_ms as u64, Ordering::Relaxed);
          if let Ok(mut app_lock) = app.try_lock() {
            app_lock.song_progress_ms = new_position_ms as u128;
          }
          mpris_manager.emit_seeked(new_position_ms as u64);
          continue;
        }

        let mut app_lock = app.lock().await;
        app_lock.song_progress_ms = new_position_ms as u128;
        app_lock.dispatch(IoEvent::Seek(new_position_ms));
        drop(app_lock);
        mpris_manager.emit_seeked(new_position_ms as u64);
      }
      MprisEvent::SetShuffle(shuffle) => {
        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          if let Err(error) = player.set_shuffle(shuffle) {
            eprintln!("MPRIS: Failed to set shuffle: {}", error);
          } else {
            mpris_manager.set_shuffle(shuffle);
            let mut app_lock = app.lock().await;
            if let Some(ref mut context) = app_lock.current_playback_context {
              context.shuffle_state = shuffle;
            }
            app_lock.user_config.behavior.shuffle_enabled = shuffle;
          }
          continue;
        }

        mpris_manager.set_shuffle(shuffle);
        let mut app_lock = app.lock().await;
        if let Some(ref mut context) = app_lock.current_playback_context {
          context.shuffle_state = shuffle;
        }
        app_lock.user_config.behavior.shuffle_enabled = shuffle;
        app_lock.dispatch(IoEvent::Shuffle(shuffle));
      }
      MprisEvent::SetLoopStatus(loop_status) => {
        let repeat_state = match loop_status {
          LoopStatusEvent::None => RepeatState::Off,
          LoopStatusEvent::Track => RepeatState::Track,
          LoopStatusEvent::Playlist => RepeatState::Context,
        };

        #[cfg(feature = "streaming")]
        if let Some(ref player) = streaming_player {
          if let Err(error) = player.set_repeat_mode(repeat_state) {
            eprintln!("MPRIS: Failed to set repeat mode: {}", error);
          } else {
            mpris_manager.set_loop_status(loop_status);
            let mut app_lock = app.lock().await;
            if let Some(ref mut context) = app_lock.current_playback_context {
              context.repeat_state = repeat_state;
            }
          }
          continue;
        }

        mpris_manager.set_loop_status(loop_status);
        let mut app_lock = app.lock().await;
        if let Some(ref mut context) = app_lock.current_playback_context {
          context.repeat_state = repeat_state;
        }
        app_lock.dispatch(IoEvent::Repeat(repeat_state));
      }
    }
  }
}

impl MprisManager {
  /// Create and start the MPRIS server
  ///
  /// Registers spotatui as `org.mpris.MediaPlayer2.spotatui` on D-Bus
  /// The MPRIS server runs in a dedicated thread with its own runtime
  /// because player.run() returns a !Send future that requires LocalSet
  pub fn new() -> Result<Self> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<MprisCommand>();

    // Spawn MPRIS server in a dedicated thread with its own LocalSet runtime
    // This is required because mpris_server::Player uses Rc internally (not Send)
    thread::spawn(move || {
      let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create MPRIS runtime");

      let local = tokio::task::LocalSet::new();
      local.block_on(&rt, async move {
        // Build the MPRIS player
        let player = match Player::builder("spotatui")
          .identity("spotatui")
          .desktop_entry("spotatui")
          .can_play(true)
          .can_pause(true)
          .can_go_next(true)
          .can_go_previous(true)
          .can_seek(true)
          .can_control(true)
          .can_quit(false)
          .can_raise(false)
          .can_set_fullscreen(false)
          // Enable shuffle and loop status support
          .shuffle(false) // Initial state: shuffle off
          .loop_status(mpris_server::LoopStatus::None) // Initial state: no repeat
          .build()
          .await
        {
          Ok(p) => p,
          Err(e) => {
            eprintln!("Failed to build MPRIS player: {}", e);
            return;
          }
        };

        // Set up event handlers for external control requests
        let tx = event_tx.clone();
        player.connect_play_pause(move |_player| {
          let _ = tx.send(MprisEvent::PlayPause);
        });

        let tx = event_tx.clone();
        player.connect_play(move |_player| {
          let _ = tx.send(MprisEvent::Play);
        });

        let tx = event_tx.clone();
        player.connect_pause(move |_player| {
          let _ = tx.send(MprisEvent::Pause);
        });

        let tx = event_tx.clone();
        player.connect_next(move |_player| {
          let _ = tx.send(MprisEvent::Next);
        });

        let tx = event_tx.clone();
        player.connect_previous(move |_player| {
          let _ = tx.send(MprisEvent::Previous);
        });

        let tx = event_tx.clone();
        player.connect_stop(move |_player| {
          let _ = tx.send(MprisEvent::Stop);
        });

        let tx = event_tx.clone();
        player.connect_seek(move |_player, offset| {
          let _ = tx.send(MprisEvent::Seek(offset.as_micros()));
        });

        let tx = event_tx.clone();
        player.connect_set_position(move |_player, _track_id, position| {
          let _ = tx.send(MprisEvent::SetPosition(position.as_micros()));
        });

        let tx = event_tx.clone();
        player.connect_set_shuffle(move |_player, shuffle| {
          let _ = tx.send(MprisEvent::SetShuffle(shuffle));
        });

        let tx = event_tx.clone();
        player.connect_set_loop_status(move |_player, loop_status| {
          use mpris_server::LoopStatus;
          let status = match loop_status {
            LoopStatus::None => LoopStatusEvent::None,
            LoopStatus::Track => LoopStatusEvent::Track,
            LoopStatus::Playlist => LoopStatusEvent::Playlist,
          };
          let _ = tx.send(MprisEvent::SetLoopStatus(status));
        });

        // Spawn the player event loop
        tokio::task::spawn_local(player.run());

        // Handle commands from the main application
        while let Some(cmd) = command_rx.recv().await {
          match cmd {
            MprisCommand::Metadata {
              title,
              artists,
              album,
              duration_ms,
              art_url,
            } => {
              let mut builder = Metadata::builder()
                .title(&title)
                .artist(artists.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .album(&album)
                .length(Time::from_millis(duration_ms as i64));

              if let Some(url) = &art_url {
                builder = builder.art_url(url);
              }

              let metadata = builder.build();

              if let Err(e) = player.set_metadata(metadata).await {
                eprintln!("MPRIS: Failed to set metadata: {}", e);
              }
            }

            MprisCommand::PlaybackStatus(is_playing) => {
              let status = if is_playing {
                PlaybackStatus::Playing
              } else {
                PlaybackStatus::Paused
              };
              if let Err(e) = player.set_playback_status(status).await {
                eprintln!("MPRIS: Failed to set playback status: {}", e);
              }
            }
            MprisCommand::Position(position_ms) => {
              // Silent position update (for regular playback progress)
              player.set_position(Time::from_millis(position_ms as i64));
            }
            MprisCommand::Seeked(position_ms) => {
              // Update position AND emit Seeked signal so clients know to refresh
              let time = Time::from_millis(position_ms as i64);
              player.set_position(time);
              if let Err(e) = player.seeked(time).await {
                eprintln!("MPRIS: Failed to emit Seeked signal: {}", e);
              }
            }
            MprisCommand::Volume(volume_percent) => {
              let volume = (volume_percent as f64) / 100.0;
              if let Err(e) = player.set_volume(volume).await {
                eprintln!("MPRIS: Failed to set volume: {}", e);
              }
            }
            MprisCommand::Shuffle(shuffle) => {
              if let Err(e) = player.set_shuffle(shuffle).await {
                eprintln!("MPRIS: Failed to set shuffle: {}", e);
              }
            }
            MprisCommand::LoopStatus(loop_status) => {
              use mpris_server::LoopStatus;
              let status = match loop_status {
                LoopStatusEvent::None => LoopStatus::None,
                LoopStatusEvent::Track => LoopStatus::Track,
                LoopStatusEvent::Playlist => LoopStatus::Playlist,
              };
              if let Err(e) = player.set_loop_status(status).await {
                eprintln!("MPRIS: Failed to set loop status: {}", e);
              }
            }
            MprisCommand::Stopped => {
              if let Err(e) = player.set_playback_status(PlaybackStatus::Stopped).await {
                eprintln!("MPRIS: Failed to set stopped status: {}", e);
              }
            }
          }
        }
      });
    });

    Ok(Self {
      event_rx: std::sync::Mutex::new(Some(event_rx)),
      command_tx,
    })
  }

  /// Take the event receiver for handling external control requests
  ///
  /// This can only be called once; subsequent calls return None
  pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<MprisEvent>> {
    self.event_rx.lock().ok()?.take()
  }

  /// Update track metadata
  pub fn set_metadata(
    &self,
    title: &str,
    artists: &[String],
    album: &str,
    duration_ms: u32,
    art_url: Option<String>,
  ) {
    let _ = self.command_tx.send(MprisCommand::Metadata {
      title: title.to_string(),
      artists: artists.to_vec(),
      album: album.to_string(),
      duration_ms,
      art_url,
    });
  }

  /// Update playback status
  pub fn set_playback_status(&self, is_playing: bool) {
    let _ = self
      .command_tx
      .send(MprisCommand::PlaybackStatus(is_playing));
  }

  /// Update playback position (silent, no signal emitted)
  pub fn set_position(&self, position_ms: u64) {
    let _ = self.command_tx.send(MprisCommand::Position(position_ms));
  }

  /// Update position AND emit Seeked signal (use when position jumps due to seeking)
  pub fn emit_seeked(&self, position_ms: u64) {
    let _ = self.command_tx.send(MprisCommand::Seeked(position_ms));
  }

  /// Update volume (0-100)
  pub fn set_volume(&self, volume_percent: u8) {
    let _ = self.command_tx.send(MprisCommand::Volume(volume_percent));
  }

  /// Mark playback as stopped
  pub fn set_stopped(&self) {
    let _ = self.command_tx.send(MprisCommand::Stopped);
  }

  /// Update shuffle state
  pub fn set_shuffle(&self, shuffle: bool) {
    let _ = self.command_tx.send(MprisCommand::Shuffle(shuffle));
  }

  /// Update loop/repeat status
  pub fn set_loop_status(&self, status: LoopStatusEvent) {
    let _ = self.command_tx.send(MprisCommand::LoopStatus(status));
  }
}

fn metadata_from_app(app: &App) -> Option<MprisMetadataTuple> {
  if let Some(native_info) = &app.native_track_info {
    let art_url = app
      .current_playback_context
      .as_ref()
      .and_then(|context| context.item.as_ref())
      .and_then(extract_playable_metadata)
      .and_then(|m| m.art_url);
    return Some((
      native_info.name.clone(),
      vec![native_info.artists_display.clone()],
      native_info.album.clone(),
      native_info.duration_ms,
      art_url,
    ));
  }

  if let Some(context) = &app.current_playback_context {
    let item = context.item.as_ref()?;
    let m = extract_playable_metadata(item)?;
    Some((m.title, vec![m.artist], m.album, m.duration_ms, m.art_url))
  } else {
    None
  }
}
