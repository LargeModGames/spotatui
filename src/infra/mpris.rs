//! MPRIS D-Bus interface for desktop media control integration
//!
//! Exposes spotatui as a controllable media player via D-Bus, enabling:
//! - Media key support (play/pause, next, previous)
//! - Desktop environment integration (GNOME, KDE, etc.)
//! - playerctl command-line control
//!
//! This module is only available on Linux with the `mpris` feature enabled.

use anyhow::Result;
use mpris_server::{Metadata, PlaybackStatus, Player, Time, TrackId};
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
  Seek(i64),        // Relative offset in microseconds
  SetPosition(i64), // Absolute position in microseconds
  SetShuffle(bool),
  SetLoopStatus(LoopStatusEvent),
  SetVolume(u8), // 0-100
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

/// Object-path prefix of the track ids spotatui publishes. `/org/mpris` is
/// reserved by the specification.
const TRACK_ID_PREFIX: &str = "/org/spotatui/track/";

/// The `mpris:trackid` for a track: a D-Bus object path derived from its
/// metadata, so every path that publishes the same track (the native player's
/// track change, the driver's sync with cover art) yields the same id.
/// `DefaultHasher::new()` uses fixed keys, so the id is stable for the process.
fn track_object_path(title: &str, artists: &[String], album: &str, duration_ms: u32) -> String {
  use std::hash::{DefaultHasher, Hash, Hasher};
  let mut hasher = DefaultHasher::new();
  (title, artists, album, duration_ms).hash(&mut hasher);
  format!("{TRACK_ID_PREFIX}{:016x}", hasher.finish())
}

/// The metadata published for a track, with the `mpris:trackid` clients send
/// back in SetPosition. Without it, e.g. `playerctl position 60` refuses to send
/// the call at all. The path is always a valid object path (a constant prefix and
/// hex digits); should that ever fail, the id is left out as before.
fn build_metadata(
  title: &str,
  artists: &[String],
  album: &str,
  duration_ms: u32,
  art_url: Option<&str>,
) -> Metadata {
  let mut builder = Metadata::builder();
  if let Ok(id) = TrackId::try_from(track_object_path(title, artists, album, duration_ms)) {
    builder = builder.trackid(id);
  }
  builder = builder
    .title(title)
    .artist(artists.iter().map(|s| s.as_str()).collect::<Vec<_>>())
    .album(album)
    .length(Time::from_millis(duration_ms as i64));
  if let Some(url) = art_url {
    builder = builder.art_url(url);
  }
  builder.build()
}

/// Whether a `SetPosition` call is for the current track. The specification
/// says to ignore a stale id; only an id spotatui published for another track
/// is treated as stale. Clients that send `NoTrack` or an id of their own kept
/// working before spotatui published any id, so they still do.
fn set_position_targets_current(current: Option<&str>, requested: &str) -> bool {
  match current {
    Some(current) if requested.starts_with(TRACK_ID_PREFIX) => requested == current,
    _ => true,
  }
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
          // Enable shuffle and loop status support
          .shuffle(false) // Initial state: shuffle off
          .loop_status(mpris_server::LoopStatus::None) // Initial state: no repeat
          .build()
          .await
        {
          Ok(p) => p,
          Err(e) => {
            log::error!("Failed to build MPRIS player: {}", e);
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
        player.connect_set_position(move |player, track_id, position| {
          let current = player.metadata().trackid();
          if !set_position_targets_current(
            current.as_ref().map(|id| id.as_str()),
            track_id.as_str(),
          ) {
            log::debug!("MPRIS: ignoring SetPosition for a previous track ({track_id})");
            return;
          }
          let _ = tx.send(MprisEvent::SetPosition(position.as_micros()));
        });

        let tx = event_tx.clone();
        player.connect_set_shuffle(move |_player, shuffle| {
          let _ = tx.send(MprisEvent::SetShuffle(shuffle));
        });

        let tx = event_tx.clone();
        player.connect_set_volume(move |_player, volume| {
          let percent = (volume * 100.0).round().clamp(0.0, 100.0) as u8;
          let _ = tx.send(MprisEvent::SetVolume(percent));
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
              let metadata =
                build_metadata(&title, &artists, &album, duration_ms, art_url.as_deref());

              if let Err(e) = player.set_metadata(metadata).await {
                log::warn!("MPRIS: Failed to set metadata: {}", e);
              }
            }

            MprisCommand::PlaybackStatus(is_playing) => {
              let status = if is_playing {
                PlaybackStatus::Playing
              } else {
                PlaybackStatus::Paused
              };
              if let Err(e) = player.set_playback_status(status).await {
                log::warn!("MPRIS: Failed to set playback status: {}", e);
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
                log::warn!("MPRIS: Failed to emit Seeked signal: {}", e);
              }
            }
            MprisCommand::Volume(volume_percent) => {
              let volume = (volume_percent as f64) / 100.0;
              if let Err(e) = player.set_volume(volume).await {
                log::warn!("MPRIS: Failed to set volume: {}", e);
              }
            }
            MprisCommand::Shuffle(shuffle) => {
              if let Err(e) = player.set_shuffle(shuffle).await {
                log::warn!("MPRIS: Failed to set shuffle: {}", e);
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
                log::warn!("MPRIS: Failed to set loop status: {}", e);
              }
            }
            MprisCommand::Stopped => {
              if let Err(e) = player.set_playback_status(PlaybackStatus::Stopped).await {
                log::warn!("MPRIS: Failed to set stopped status: {}", e);
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

#[cfg(test)]
mod tests {
  use super::*;

  fn artists(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
  }

  #[test]
  fn the_same_track_always_gets_the_same_id() {
    let a = track_object_path("Pave Low", &artists(&["Dame"]), "Herz gegen Fame", 248_000);
    let b = track_object_path("Pave Low", &artists(&["Dame"]), "Herz gegen Fame", 248_000);
    assert_eq!(a, b);
  }

  #[test]
  fn another_title_artist_album_or_length_gets_another_id() {
    let base = track_object_path("Pave Low", &artists(&["Dame"]), "Herz gegen Fame", 248_000);
    for other in [
      track_object_path("Low Life", &artists(&["Dame"]), "Herz gegen Fame", 248_000),
      track_object_path(
        "Pave Low",
        &artists(&["Someone"]),
        "Herz gegen Fame",
        248_000,
      ),
      track_object_path("Pave Low", &artists(&["Dame"]), "Another Album", 248_000),
      track_object_path("Pave Low", &artists(&["Dame"]), "Herz gegen Fame", 249_000),
    ] {
      assert_ne!(base, other);
    }
  }

  #[test]
  fn the_id_is_a_valid_track_id_outside_the_reserved_mpris_namespace() {
    let path = track_object_path("Ünïcödé / title?", &artists(&["A", "B"]), "", 0);
    assert!(TrackId::try_from(path.as_str()).is_ok(), "{path}");
    assert!(path.starts_with(TRACK_ID_PREFIX));
    assert!(!path.starts_with("/org/mpris"));
  }

  #[test]
  fn fields_do_not_run_into_each_other_in_the_id() {
    assert_ne!(
      track_object_path("t", &artists(&["A B"]), "x", 1),
      track_object_path("t", &artists(&["A", "B"]), "x", 1)
    );
    assert_ne!(
      track_object_path("ab", &artists(&[]), "c", 1),
      track_object_path("a", &artists(&[]), "bc", 1)
    );
  }

  #[test]
  fn published_metadata_carries_the_track_id_set_position_needs() {
    let metadata = build_metadata(
      "Pave Low",
      &artists(&["Dame"]),
      "Herz gegen Fame",
      248_000,
      Some("https://i.scdn.co/image/x"),
    );
    let expected = TrackId::try_from(track_object_path(
      "Pave Low",
      &artists(&["Dame"]),
      "Herz gegen Fame",
      248_000,
    ))
    .unwrap();
    assert_eq!(metadata.trackid(), Some(expected));
    assert_eq!(metadata.title(), Some("Pave Low"));
  }

  #[test]
  fn set_position_for_the_current_track_is_applied() {
    let current = track_object_path("Pave Low", &artists(&["Dame"]), "Album", 1);
    assert!(set_position_targets_current(Some(&current), &current));
  }

  #[test]
  fn set_position_for_a_previous_spotatui_track_is_ignored() {
    let current = track_object_path("Low Life", &artists(&["Dame"]), "Album", 1);
    let previous = track_object_path("Pave Low", &artists(&["Dame"]), "Album", 1);
    assert!(!set_position_targets_current(Some(&current), &previous));
  }

  #[test]
  fn set_position_with_a_foreign_or_missing_id_is_still_applied() {
    let current = track_object_path("Pave Low", &artists(&["Dame"]), "Album", 1);
    for requested in [
      "/org/mpris/MediaPlayer2/TrackList/NoTrack",
      "/some/client/id",
    ] {
      assert!(
        set_position_targets_current(Some(&current), requested),
        "{requested}"
      );
    }
    assert!(set_position_targets_current(None, &current));
  }
}
