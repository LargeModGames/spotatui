use rspotify::model::device::Device;
use std::time::{Duration, Instant};

pub const SONOS_DEVICE_ID_PREFIX: &str = "sonos:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SonosRoom {
  pub uuid: String,
  pub name: String,
  pub location: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaybackTarget {
  Spotify {
    id: String,
    name: String,
    is_active: bool,
  },
  Sonos {
    room: SonosRoom,
    is_selected: bool,
  },
}

#[derive(Clone, Debug)]
pub struct SonosNowPlaying {
  pub room_uuid: String,
  pub title: Option<String>,
  pub artist: Option<String>,
  pub album: Option<String>,
  pub track_uri: Option<String>,
  pub duration_ms: Option<u32>,
  pub position_ms: u32,
  pub is_playing: bool,
  pub volume_percent: Option<u8>,
  pub fetched_at: Instant,
}

impl SonosNowPlaying {
  pub fn is_for_room(&self, room_uuid: &str) -> bool {
    self.room_uuid == room_uuid
  }

  pub fn is_fresh(&self, max_age: Duration) -> bool {
    self.fetched_at.elapsed() <= max_age
  }
}

impl PlaybackTarget {
  pub fn label(&self) -> String {
    match self {
      PlaybackTarget::Spotify {
        name, is_active, ..
      } => {
        if *is_active {
          format!("{name} (Spotify, active)")
        } else {
          format!("{name} (Spotify)")
        }
      }
      PlaybackTarget::Sonos { room, is_selected } => {
        if *is_selected {
          format!("{} (Sonos, selected)", room.name)
        } else {
          format!("{} (Sonos)", room.name)
        }
      }
    }
  }
}

pub fn sonos_persisted_id(uuid: &str) -> String {
  format!("{SONOS_DEVICE_ID_PREFIX}{uuid}")
}

pub fn parse_sonos_persisted_id(device_id: &str) -> Option<&str> {
  device_id.strip_prefix(SONOS_DEVICE_ID_PREFIX)
}

pub fn spotify_target_from_device(device: &Device) -> Option<PlaybackTarget> {
  Some(PlaybackTarget::Spotify {
    id: device.id.clone()?,
    name: device.name.clone(),
    is_active: device.is_active,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn round_trips_sonos_persisted_id() {
    let persisted = sonos_persisted_id("RINCON_123");

    assert_eq!(persisted, "sonos:RINCON_123");
    assert_eq!(parse_sonos_persisted_id(&persisted), Some("RINCON_123"));
    assert_eq!(parse_sonos_persisted_id("spotify-device"), None);
  }
}
