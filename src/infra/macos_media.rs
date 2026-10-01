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
use log::info;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::AnyThread;
use objc2_app_kit::NSImage;
use objc2_foundation::{NSData, NSDate, NSMutableDictionary, NSNumber, NSRunLoop, NSString};
use objc2_media_player::{
  MPMediaItemArtwork, MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyArtist,
  MPMediaItemPropertyArtwork, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle,
  MPNowPlayingInfoCenter, MPNowPlayingInfoPropertyElapsedPlaybackTime,
  MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommandCenter,
  MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use std::ptr::NonNull;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
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
#[allow(dead_code, clippy::enum_variant_names)]
pub enum MacMediaCommand {
  SetMetadata {
    title: String,
    artists: Vec<String>,
    album: String,
    duration_ms: u32,
    art_url: Option<String>,
  },
  SetPlaybackStatus(bool), // true = playing, false = paused
  SetPosition(u64),        // position in milliseconds
  SetVolume(u8),           // 0-100 (not directly supported by Now Playing, but kept for API parity)
  SetStopped,
  SetRemoteOwned(bool),
}

/// Manager for the macOS Now Playing integration
pub struct MacMediaManager {
  event_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<MacMediaEvent>>>,
  command_tx: mpsc::UnboundedSender<MacMediaCommand>,
  remote_owned: tokio::sync::watch::Sender<bool>,
}

impl MacMediaManager {
  /// Create and start the macOS media integration
  ///
  /// Registers command handlers with MPRemoteCommandCenter and sets up Now Playing info
  /// The handler runs in a dedicated thread because it requires the main run loop
  pub fn new() -> Result<Self> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel::<MacMediaCommand>();

    let (remote_owned, mut remote_on_thread) = tokio::sync::watch::channel(false);
    // Clone event_tx for use in callbacks
    let event_tx = Arc::new(event_tx);

    // MPRemoteCommandCenter requires an initialized NSApplication to route media key events;
    // without one, macOS ignores the handlers and falls through to Music.app.
    thread::spawn(move || {
      // Initialize NSApplication with raw msg_send because objc2-app-kit's
      // sharedApplication() requires MainThreadMarker (unavailable in CLI apps).
      unsafe {
        let cls = AnyClass::get(c"NSApplication").expect("NSApplication class not found");
        let app: objc2::rc::Retained<AnyObject> = msg_send![cls, sharedApplication];
        // NSApplicationActivationPolicyProhibited = 2 (no Dock icon, no menu bar)
        let _activation_policy_set: bool = msg_send![&app, setActivationPolicy: 2isize];
      }
      info!("macos media: NSApplication initialized with Prohibited activation policy");

      // Get the shared command center
      let command_center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };

      // Set up play command handler
      let tx = Arc::clone(&event_tx);
      let play_handler: RcBlock<
        dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
      > = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        info!("macos media: received Play event");
        let _ = tx.send(MacMediaEvent::Play);
        MPRemoteCommandHandlerStatus::Success
      });
      unsafe {
        command_center
          .playCommand()
          .addTargetWithHandler(&play_handler);
      }

      // Set up pause command handler
      let tx = Arc::clone(&event_tx);
      let pause_handler: RcBlock<
        dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
      > = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        info!("macos media: received Pause event");
        let _ = tx.send(MacMediaEvent::Pause);
        MPRemoteCommandHandlerStatus::Success
      });
      unsafe {
        command_center
          .pauseCommand()
          .addTargetWithHandler(&pause_handler);
      }

      // Set up toggle play/pause command handler
      let tx = Arc::clone(&event_tx);
      let toggle_handler: RcBlock<
        dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
      > = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        info!("macos media: received PlayPause event");
        let _ = tx.send(MacMediaEvent::PlayPause);
        MPRemoteCommandHandlerStatus::Success
      });
      unsafe {
        command_center
          .togglePlayPauseCommand()
          .addTargetWithHandler(&toggle_handler);
      }

      // Set up next track command handler
      let tx = Arc::clone(&event_tx);
      let next_handler: RcBlock<
        dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
      > = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        info!("macos media: received Next event");
        let _ = tx.send(MacMediaEvent::Next);
        MPRemoteCommandHandlerStatus::Success
      });
      unsafe {
        command_center
          .nextTrackCommand()
          .addTargetWithHandler(&next_handler);
      }

      // Set up previous track command handler
      let tx = Arc::clone(&event_tx);
      let prev_handler: RcBlock<
        dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
      > = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        info!("macos media: received Previous event");
        let _ = tx.send(MacMediaEvent::Previous);
        MPRemoteCommandHandlerStatus::Success
      });
      unsafe {
        command_center
          .previousTrackCommand()
          .addTargetWithHandler(&prev_handler);
      }

      // Set up stop command handler
      let tx = Arc::clone(&event_tx);
      let stop_handler: RcBlock<
        dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus,
      > = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        info!("macos media: received Stop event");
        let _ = tx.send(MacMediaEvent::Stop);
        MPRemoteCommandHandlerStatus::Success
      });
      unsafe {
        command_center
          .stopCommand()
          .addTargetWithHandler(&stop_handler);
      }

      info!("macos media: remote command handlers registered");

      // Get the now playing info center
      let info_center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };

      // Interleave command processing with NSRunLoop ticks so macOS can deliver
      // MPRemoteCommandCenter events to our handler blocks.
      let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create macOS media runtime");

      rt.block_on(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
          tokio::select! {
            Some(cmd) = command_rx.recv() => {
              handle_now_playing_command(&cmd, &info_center, &mut remote_on_thread).await;
            }
            _ = interval.tick() => {
              NSRunLoop::currentRunLoop()
                .runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.01));
            }
          }
        }
      });
    });

    Ok(Self {
      event_rx: std::sync::Mutex::new(Some(event_rx)),
      command_tx,
      remote_owned,
    })
  }

  /// Suppress every publisher, including an artwork fetch already in flight.
  /// Music owns the system entry while spotatui is its remote.
  pub(crate) fn set_remote_owned(&self, owned: bool) {
    if self.remote_owned.send_replace(owned) != owned {
      let _ = self.command_tx.send(MacMediaCommand::SetRemoteOwned(owned));
    }
  }

  /// Take the event receiver for handling external control requests
  ///
  /// This can only be called once; subsequent calls return None
  pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<MacMediaEvent>> {
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
    let _ = self.command_tx.send(MacMediaCommand::SetMetadata {
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
      .send(MacMediaCommand::SetPlaybackStatus(is_playing));
  }

  /// Update playback position
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

/// Process a single Now Playing command, updating the info center state.
/// Must be called from the dedicated macOS media thread that owns `info_center`.
async fn handle_now_playing_command(
  cmd: &MacMediaCommand,
  info_center: &MPNowPlayingInfoCenter,
  remote_owned: &mut tokio::sync::watch::Receiver<bool>,
) {
  if let MacMediaCommand::SetRemoteOwned(owned) = cmd {
    if *owned {
      unsafe {
        info_center.setNowPlayingInfo(None);
        info_center.setPlaybackState(MPNowPlayingPlaybackState::Unknown);
      }
    }
    return;
  }
  if *remote_owned.borrow() {
    return;
  }
  match cmd {
    MacMediaCommand::SetRemoteOwned(_) => {}
    MacMediaCommand::SetMetadata {
      title,
      artists,
      album,
      duration_ms,
      art_url,
    } => {
      let artwork = match art_url.as_deref() {
        Some(url) => tokio::select! {
          biased;
          // A slow old artwork fetch must not delay removing our OS entry
          // when Music starts. No Apple metadata is published here.
          _ = remote_owned.wait_for(|owned| *owned) => None,
          artwork = fetch_artwork_from_url(url) => artwork,
        },
        None => None,
      };

      if *remote_owned.borrow() {
        unsafe {
          info_center.setNowPlayingInfo(None);
          info_center.setPlaybackState(MPNowPlayingPlaybackState::Unknown);
        }
        return;
      }
      unsafe {
        let dict: objc2::rc::Retained<NSMutableDictionary<NSString, AnyObject>> =
          NSMutableDictionary::new();

        let title_ns = NSString::from_str(title);
        dict.insert(MPMediaItemPropertyTitle, &*title_ns);

        let artist_ns = NSString::from_str(&artists.join(", "));
        dict.insert(MPMediaItemPropertyArtist, &*artist_ns);

        let album_ns = NSString::from_str(album);
        dict.insert(MPMediaItemPropertyAlbumTitle, &*album_ns);

        let duration = NSNumber::numberWithDouble(f64::from(*duration_ms) / 1000.0);
        dict.insert(MPMediaItemPropertyPlaybackDuration, &*duration);

        let rate = NSNumber::numberWithDouble(1.0);
        dict.insert(MPNowPlayingInfoPropertyPlaybackRate, &*rate);

        if let Some(artwork) = artwork.as_ref() {
          dict.insert(MPMediaItemPropertyArtwork, &**artwork);
        }

        info_center.setNowPlayingInfo(Some(&dict));
      }
    }
    MacMediaCommand::SetPlaybackStatus(is_playing) => unsafe {
      let state = if *is_playing {
        MPNowPlayingPlaybackState::Playing
      } else {
        MPNowPlayingPlaybackState::Paused
      };
      info_center.setPlaybackState(state);

      // Update playback rate in the existing nowPlayingInfo so macOS
      // knows whether to advance the elapsed time counter.
      if let Some(existing) = info_center.nowPlayingInfo() {
        let dict: objc2::rc::Retained<NSMutableDictionary<NSString, AnyObject>> =
          NSMutableDictionary::dictionaryWithDictionary(&existing);
        let rate = NSNumber::numberWithDouble(if *is_playing { 1.0 } else { 0.0 });
        dict.insert(MPNowPlayingInfoPropertyPlaybackRate, &*rate);
        info_center.setNowPlayingInfo(Some(&dict));
      }
    },
    MacMediaCommand::SetPosition(position_ms) => unsafe {
      // Update elapsed playback time in the existing nowPlayingInfo dict
      if let Some(existing) = info_center.nowPlayingInfo() {
        let dict: objc2::rc::Retained<NSMutableDictionary<NSString, AnyObject>> =
          NSMutableDictionary::dictionaryWithDictionary(&existing);
        let elapsed = NSNumber::numberWithDouble(*position_ms as f64 / 1000.0);
        dict.insert(MPNowPlayingInfoPropertyElapsedPlaybackTime, &*elapsed);
        info_center.setNowPlayingInfo(Some(&dict));
      }
    },
    MacMediaCommand::SetVolume(_) => {
      // Volume is not directly supported by Now Playing center
    }
    MacMediaCommand::SetStopped => unsafe {
      info_center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
      info_center.setNowPlayingInfo(None);
    },
  }
}

async fn fetch_artwork_from_url(art_url: &str) -> Option<objc2::rc::Retained<MPMediaItemArtwork>> {
  let response = reqwest::get(art_url).await.ok()?;
  if !response.status().is_success() {
    return None;
  }

  let bytes = response.bytes().await.ok()?;
  if bytes.is_empty() {
    return None;
  }

  unsafe {
    let data = NSData::dataWithBytes_length(bytes.as_ptr().cast(), bytes.len());
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    let image_for_handler = image.clone();
    let request_handler =
      RcBlock::new(move |_requested_size| NonNull::from(image_for_handler.as_ref()));

    Some(MPMediaItemArtwork::initWithBoundsSize_requestHandler(
      MPMediaItemArtwork::alloc(),
      image.size(),
      &request_handler,
    ))
  }
}
