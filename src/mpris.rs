//! MPRIS D-Bus interface for desktop media control integration
//!
//! Exposes spotatui as a controllable media player via D-Bus, enabling:
//! - Media key support (play/pause, next, previous)
//! - Desktop environment integration (GNOME, KDE, etc.)
//! - playerctl command-line control
//!
//! This module is only available on Linux with the `mpris` feature enabled.

use anyhow::Result;
use mpris_server::{Metadata, PlaybackStatus, Player, Time};
use std::thread;
use tokio::sync::mpsc;

/// Events that can be received from external MPRIS clients (e.g., media keys, playerctl)
#[derive(Debug, Clone)]
pub enum MprisEvent {
  PlayPause,
  Play,
  Pause,
  Next,
  Previous,
  Stop,
  Seek(i64), // Offset in microseconds
}

/// Commands to send TO the MPRIS server to update its state
#[derive(Debug, Clone)]
#[allow(dead_code)] // SetPosition kept for future use
pub enum MprisCommand {
  Metadata {
    title: String,
    artists: Vec<String>,
    album: String,
    duration_ms: u32,
  },
  PlaybackStatus(bool), // true = playing, false = paused
  Position(u64),        // position in milliseconds (for future use)
  Volume(u8),           // 0-100
  Stopped,
}

/// Manager for the MPRIS D-Bus server
pub struct MprisManager {
  event_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<MprisEvent>>>,
  command_tx: mpsc::UnboundedSender<MprisCommand>,
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
            } => {
              let metadata = Metadata::builder()
                .title(&title)
                .artist(artists.iter().map(|s| s.as_str()).collect::<Vec<_>>())
                .album(&album)
                .length(Time::from_millis(duration_ms as i64))
                .build();

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
              player.set_position(Time::from_millis(position_ms as i64));
            }
            MprisCommand::Volume(volume_percent) => {
              let volume = (volume_percent as f64) / 100.0;
              if let Err(e) = player.set_volume(volume).await {
                eprintln!("MPRIS: Failed to set volume: {}", e);
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
  pub fn set_metadata(&self, title: &str, artists: &[String], album: &str, duration_ms: u32) {
    let _ = self.command_tx.send(MprisCommand::Metadata {
      title: title.to_string(),
      artists: artists.to_vec(),
      album: album.to_string(),
      duration_ms,
    });
  }

  /// Update playback status
  pub fn set_playback_status(&self, is_playing: bool) {
    let _ = self
      .command_tx
      .send(MprisCommand::PlaybackStatus(is_playing));
  }

  /// Update playback position
  #[allow(dead_code)] // Kept for future use
  pub fn set_position(&self, position_ms: u64) {
    let _ = self.command_tx.send(MprisCommand::Position(position_ms));
  }

  /// Update volume (0-100)
  pub fn set_volume(&self, volume_percent: u8) {
    let _ = self.command_tx.send(MprisCommand::Volume(volume_percent));
  }

  /// Mark playback as stopped
  pub fn set_stopped(&self) {
    let _ = self.command_tx.send(MprisCommand::Stopped);
  }
}
