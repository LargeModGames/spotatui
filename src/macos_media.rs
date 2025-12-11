//! macOS Now Playing / Media Key integration
//!
//! Exposes spotatui as a controllable media player via macOS's MediaPlayer framework, enabling:
//! - Media key support (play/pause, next, previous)
//! - Control Center / Touch Bar Now Playing widget
//! - Headphone button controls
//!
//! This module is only available on macOS with the `macos-media` feature enabled.

use anyhow::Result;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
  MPNowPlayingInfoCenter, MPNowPlayingPlaybackState, MPRemoteCommandCenter,
  MPRemoteCommandHandlerStatus,
};
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc;

/// Events that can be received from external macOS media controls (media keys, Control Center, etc.)
#[derive(Debug, Clone)]
pub enum MacMediaEvent {
  PlayPause,
  Play,
  Pause,
  Next,
  Previous,
  Stop,
}

/// Commands to send TO the Now Playing center to update its state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MacMediaCommand {
  SetMetadata {
    title: String,
    artists: Vec<String>,
    album: String,
    duration_ms: u32,
  },
  SetPlaybackStatus(bool), // true = playing, false = paused
  SetPosition(u64),        // position in milliseconds
  SetVolume(u8),           // 0-100 (not directly supported by Now Playing, but kept for API parity)
  SetStopped,
}

/// Manager for the macOS Now Playing integration
pub struct MacMediaManager {
  event_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<MacMediaEvent>>>,
  command_tx: mpsc::UnboundedSender<MacMediaCommand>,
}

impl MacMediaManager {
  /// Create and start the macOS media integration
  ///
  /// Registers command handlers with MPRemoteCommandCenter and sets up Now Playing info
  /// The handler runs in a dedicated thread because it requires the main run loop
  pub fn new() -> Result<Self> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<MacMediaCommand>();

    // Clone event_tx for use in callbacks
    let event_tx = Arc::new(event_tx);

    // Spawn the macOS integration in a dedicated thread
    // The command center and now playing center must be accessed from the main thread
    // in a real app, but for a terminal app we use a background thread with its own run loop
    thread::spawn(move || {
      // Get the shared command center
      let command_center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };

      // Set up play command handler
      let tx = Arc::clone(&event_tx);
      let play_handler = RcBlock::new(move |_event: &AnyObject| -> isize {
        let _ = tx.send(MacMediaEvent::Play);
        MPRemoteCommandHandlerStatus::Success.0 as isize
      });
      unsafe {
        command_center
          .playCommand()
          .addTargetWithHandler(&play_handler);
      }

      // Set up pause command handler
      let tx = Arc::clone(&event_tx);
      let pause_handler = RcBlock::new(move |_event: &AnyObject| -> isize {
        let _ = tx.send(MacMediaEvent::Pause);
        MPRemoteCommandHandlerStatus::Success.0 as isize
      });
      unsafe {
        command_center
          .pauseCommand()
          .addTargetWithHandler(&pause_handler);
      }

      // Set up toggle play/pause command handler
      let tx = Arc::clone(&event_tx);
      let toggle_handler = RcBlock::new(move |_event: &AnyObject| -> isize {
        let _ = tx.send(MacMediaEvent::PlayPause);
        MPRemoteCommandHandlerStatus::Success.0 as isize
      });
      unsafe {
        command_center
          .togglePlayPauseCommand()
          .addTargetWithHandler(&toggle_handler);
      }

      // Set up next track command handler
      let tx = Arc::clone(&event_tx);
      let next_handler = RcBlock::new(move |_event: &AnyObject| -> isize {
        let _ = tx.send(MacMediaEvent::Next);
        MPRemoteCommandHandlerStatus::Success.0 as isize
      });
      unsafe {
        command_center
          .nextTrackCommand()
          .addTargetWithHandler(&next_handler);
      }

      // Set up previous track command handler
      let tx = Arc::clone(&event_tx);
      let prev_handler = RcBlock::new(move |_event: &AnyObject| -> isize {
        let _ = tx.send(MacMediaEvent::Previous);
        MPRemoteCommandHandlerStatus::Success.0 as isize
      });
      unsafe {
        command_center
          .previousTrackCommand()
          .addTargetWithHandler(&prev_handler);
      }

      // Set up stop command handler
      let tx = Arc::clone(&event_tx);
      let stop_handler = RcBlock::new(move |_event: &AnyObject| -> isize {
        let _ = tx.send(MacMediaEvent::Stop);
        MPRemoteCommandHandlerStatus::Success.0 as isize
      });
      unsafe {
        command_center
          .stopCommand()
          .addTargetWithHandler(&stop_handler);
      }

      // Get the now playing info center
      let info_center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };

      // Create a simple runtime for handling commands
      let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create macOS media runtime");

      rt.block_on(async move {
        while let Some(cmd) = command_rx.recv().await {
          match cmd {
            MacMediaCommand::SetMetadata {
              title,
              artists,
              album,
              duration_ms,
            } => {
              // Build the now playing info dictionary
              // Note: This is a simplified implementation - full implementation would use
              // proper NSMutableDictionary with correct keys
              let _ = (title, artists, album, duration_ms);
              unsafe {
                // Create metadata dictionary with track info
                let title_key = NSString::from_str("MPMediaItemPropertyTitle");
                let title_value = NSString::from_str(&title);
                let artist_key = NSString::from_str("MPMediaItemPropertyArtist");
                let artist_value = NSString::from_str(&artists.join(", "));
                let album_key = NSString::from_str("MPMediaItemPropertyAlbumTitle");
                let album_value = NSString::from_str(&album);
                let duration_key = NSString::from_str("MPMediaItemPropertyPlaybackDuration");
                let duration_value = NSNumber::new_f64(duration_ms as f64 / 1000.0);

                let keys: [&NSString; 4] = [&title_key, &artist_key, &album_key, &duration_key];
                let objects: [&AnyObject; 4] = [
                  Retained::as_ref(&title_value).as_ref(),
                  Retained::as_ref(&artist_value).as_ref(),
                  Retained::as_ref(&album_value).as_ref(),
                  Retained::as_ref(&duration_value).as_ref(),
                ];

                let dict = NSDictionary::from_retained_objects(
                  &keys as &[&NSString],
                  &objects as &[&AnyObject],
                );
                info_center.setNowPlayingInfo(Some(&dict));
              }
            }
            MacMediaCommand::SetPlaybackStatus(is_playing) => unsafe {
              let state = if is_playing {
                MPNowPlayingPlaybackState::Playing
              } else {
                MPNowPlayingPlaybackState::Paused
              };
              info_center.setPlaybackState(state);
            },
            MacMediaCommand::SetPosition(_position_ms) => {
              // Position updates would require updating the nowPlayingInfo dict
              // with MPNowPlayingInfoPropertyElapsedPlaybackTime
              // Simplified for now
            }
            MacMediaCommand::SetVolume(_volume_percent) => {
              // Volume is not directly supported by Now Playing center
              // but kept for API parity with MPRIS
            }
            MacMediaCommand::SetStopped => unsafe {
              info_center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            },
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
  pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<MacMediaEvent>> {
    self.event_rx.lock().ok()?.take()
  }

  /// Update track metadata
  pub fn set_metadata(&self, title: &str, artists: &[String], album: &str, duration_ms: u32) {
    let _ = self.command_tx.send(MacMediaCommand::SetMetadata {
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
      .send(MacMediaCommand::SetPlaybackStatus(is_playing));
  }

  /// Update playback position
  #[allow(dead_code)]
  pub fn set_position(&self, position_ms: u64) {
    let _ = self
      .command_tx
      .send(MacMediaCommand::SetPosition(position_ms));
  }

  /// Update volume (0-100) - kept for API parity with MPRIS
  #[allow(dead_code)]
  pub fn set_volume(&self, volume_percent: u8) {
    let _ = self
      .command_tx
      .send(MacMediaCommand::SetVolume(volume_percent));
  }

  /// Mark playback as stopped
  pub fn set_stopped(&self) {
    let _ = self.command_tx.send(MacMediaCommand::SetStopped);
  }
}
