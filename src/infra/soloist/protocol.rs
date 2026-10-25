//! The Soloist WebSocket protocol: command JSON out, events in.
//!
//! The envelope is flat: `{"type":"command","command":"seek","position_ms":0}`.
//! A nested `{"command":{"type":..}}` is rejected by Soloist.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
  Play,
  Pause,
  SkipNext,
  SkipPrev,
  Seek(u64),
  SetVolume(u8),
  SetShuffle(bool),
  SetRepeatContext(bool),
  SetRepeatTrack(bool),
  Activate,
  Deactivate,
}

impl Command {
  pub fn to_json(self) -> String {
    let mut message = json!({ "type": "command" });
    let (name, field) = match self {
      Command::Play => ("play", None),
      Command::Pause => ("pause", None),
      Command::SkipNext => ("skip_next", None),
      Command::SkipPrev => ("skip_prev", None),
      Command::Seek(position) => ("seek", Some(("position_ms", json!(position)))),
      Command::SetVolume(volume) => ("set_volume", Some(("volume", json!(volume.min(100))))),
      Command::SetShuffle(on) => ("set_shuffle", Some(("enabled", json!(on)))),
      Command::SetRepeatContext(on) => ("set_repeat_context", Some(("enabled", json!(on)))),
      Command::SetRepeatTrack(on) => ("set_repeat_track", Some(("enabled", json!(on)))),
      Command::Activate => ("activate", None),
      Command::Deactivate => ("deactivate", None),
    };
    message["command"] = json!(name);
    if let Some((key, value)) = field {
      message[key] = value;
    }
    message.to_string()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
  #[default]
  Off,
  Context,
  Track,
}

impl Repeat {
  fn parse(text: &str) -> Option<Self> {
    match text {
      "off" => Some(Repeat::Off),
      "context" => Some(Repeat::Context),
      "track" => Some(Repeat::Track),
      _ => None,
    }
  }
}

/// The two flags for a repeat target, the one that turns off first.
pub fn repeat_commands(target: rspotify::model::RepeatState) -> [Command; 2] {
  use rspotify::model::RepeatState;
  match target {
    RepeatState::Off => [
      Command::SetRepeatTrack(false),
      Command::SetRepeatContext(false),
    ],
    RepeatState::Context => [
      Command::SetRepeatTrack(false),
      Command::SetRepeatContext(true),
    ],
    RepeatState::Track => [
      Command::SetRepeatContext(false),
      Command::SetRepeatTrack(true),
    ],
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
  #[default]
  Idle,
  Playing,
  Paused,
  Buffering,
}

impl Status {
  fn parse(text: &str) -> Option<Self> {
    match text {
      "idle" => Some(Status::Idle),
      "playing" => Some(Status::Playing),
      "paused" => Some(Status::Paused),
      "buffering" => Some(Status::Buffering),
      _ => None,
    }
  }

  pub fn is_playing(self) -> bool {
    matches!(self, Status::Playing | Status::Buffering)
  }
}

/// A position at a wall-clock instant (ms since the Unix epoch), moving at `speed`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionAnchor {
  pub position_ms: u64,
  pub timestamp_ms: i64,
  pub speed: f64,
}

impl PositionAnchor {
  fn parse(value: &Value) -> Option<Self> {
    Some(Self {
      position_ms: value["position_ms"].as_u64()?,
      timestamp_ms: value["timestamp_ms"].as_i64()?,
      speed: value["speed"].as_f64().unwrap_or(1.0),
    })
  }

  pub fn at(&self, now_ms: i64) -> u64 {
    let elapsed = (now_ms - self.timestamp_ms).max(0) as f64 * self.speed.max(0.0);
    self.position_ms.saturating_add(elapsed as u64)
  }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaybackState {
  pub status: Status,
  pub item_uri: Option<String>,
  pub item_is_episode: bool,
  pub context_uri: Option<String>,
  pub position: Option<PositionAnchor>,
  pub volume: Option<u8>,
  pub is_active: bool,
  pub shuffle: bool,
  pub repeat: Repeat,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
  Auth {
    logged_in: bool,
    is_active: bool,
  },
  State(Box<PlaybackState>),
  Track {
    uri: Option<String>,
    is_episode: bool,
  },
  Playback(Status),
  Volume(u8),
  Device {
    is_active: bool,
  },
  Context(Option<String>),
  Options {
    shuffle: Option<bool>,
    repeat: Option<Repeat>,
  },
  Position(PositionAnchor),
  Error(String),
}

fn item(value: &Value) -> (Option<String>, bool) {
  (
    value["uri"].as_str().map(str::to_string),
    value["entity_type"].as_str() == Some("episode"),
  )
}

fn volume(value: &Value) -> Option<u8> {
  value.as_u64().map(|volume| volume.min(100) as u8)
}

fn options(value: &Value) -> (Option<bool>, Option<Repeat>) {
  (
    value["shuffle"].as_bool(),
    value["repeat"].as_str().and_then(Repeat::parse),
  )
}

/// One WebSocket text frame; `None` for types spotatui does not use.
pub fn parse_event(text: &str) -> Option<Event> {
  let value: Value = serde_json::from_str(text).ok()?;
  match value["type"].as_str()? {
    "auth_state" => Some(Event::Auth {
      logged_in: value["logged_in"].as_bool().unwrap_or(false),
      is_active: value["is_active"].as_bool().unwrap_or(false),
    }),
    "playback_state" => {
      let (item_uri, item_is_episode) = item(&value["item"]);
      let (shuffle, repeat) = options(&value["options"]);
      Some(Event::State(Box::new(PlaybackState {
        status: value["status"]
          .as_str()
          .and_then(Status::parse)
          .unwrap_or_default(),
        item_uri,
        item_is_episode,
        context_uri: value["context"]["uri"].as_str().map(str::to_string),
        position: PositionAnchor::parse(&value["position"]),
        volume: volume(&value["volume"]),
        is_active: value["is_active"].as_bool().unwrap_or(false),
        shuffle: shuffle.unwrap_or(false),
        repeat: repeat.unwrap_or_default(),
      })))
    }
    "track_changed" => {
      let (uri, is_episode) = item(&value["item"]);
      Some(Event::Track { uri, is_episode })
    }
    "playback_changed" => value["status"]
      .as_str()
      .and_then(Status::parse)
      .map(Event::Playback),
    "volume_changed" => volume(&value["volume"]).map(Event::Volume),
    "device_changed" => Some(Event::Device {
      is_active: value["is_active"].as_bool().unwrap_or(false),
    }),
    "context_changed" => Some(Event::Context(
      value["context"]["uri"].as_str().map(str::to_string),
    )),
    "options_changed" => {
      let (shuffle, repeat) = options(&value["options"]);
      Some(Event::Options { shuffle, repeat })
    }
    "position_sync" => PositionAnchor::parse(&value["position"]).map(Event::Position),
    "error" => Some(Event::Error(
      value["message"].as_str().unwrap_or_default().to_string(),
    )),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const EXPERIMENT_STATE: &str = r#"{"type":"playback_state","status":"playing","item":{"uri":"spotify:track:35pbQNMzooB4Qg6mRPt37b","entity_type":"track","decorations":{"identity":{"name":"This Dream Of You"},"playback":{"duration_ms":286406,"content_ratings":[]}}},"context":{"uri":"spotify:playlist:0N9wg7UZwjb6UO0MBzUscR","entity_type":"playlist"},"position":{"position_ms":21344,"timestamp_ms":1792921722717,"speed":1},"volume":80,"is_active":true,"options":{"shuffle":true,"repeat":"off","playback_speed":1,"modes":{"context_enhancement":"NONE"}},"available_actions":{"pause":{},"seek":{}}}"#;

  #[test]
  fn commands_use_the_flat_envelope() {
    let pause: Value = serde_json::from_str(&Command::Pause.to_json()).unwrap();
    assert_eq!(pause, json!({ "type": "command", "command": "pause" }));
  }

  #[test]
  fn seek_and_set_volume_put_fields_at_top_level() {
    let seek: Value = serde_json::from_str(&Command::Seek(1500).to_json()).unwrap();
    assert_eq!(seek["command"], "seek");
    assert_eq!(seek["position_ms"], 1500);
    let volume: Value = serde_json::from_str(&Command::SetVolume(140).to_json()).unwrap();
    assert_eq!(volume["volume"], 100);
  }

  #[test]
  fn repeat_commands_turn_the_other_flag_off_first() {
    assert_eq!(
      repeat_commands(rspotify::model::RepeatState::Track),
      [
        Command::SetRepeatContext(false),
        Command::SetRepeatTrack(true)
      ]
    );
    assert_eq!(
      repeat_commands(rspotify::model::RepeatState::Context),
      [
        Command::SetRepeatTrack(false),
        Command::SetRepeatContext(true)
      ]
    );
    assert_eq!(
      repeat_commands(rspotify::model::RepeatState::Off),
      [
        Command::SetRepeatTrack(false),
        Command::SetRepeatContext(false)
      ]
    );
  }

  #[test]
  fn parse_event_reads_the_experiment_playback_state() {
    let Some(Event::State(state)) = parse_event(EXPERIMENT_STATE) else {
      panic!("expected a playback state");
    };
    assert_eq!(state.status, Status::Playing);
    assert_eq!(
      state.item_uri.as_deref(),
      Some("spotify:track:35pbQNMzooB4Qg6mRPt37b")
    );
    assert!(!state.item_is_episode);
    assert_eq!(
      state.context_uri.as_deref(),
      Some("spotify:playlist:0N9wg7UZwjb6UO0MBzUscR")
    );
    assert_eq!(state.volume, Some(80));
    assert!(state.is_active && state.shuffle);
    assert_eq!(state.repeat, Repeat::Off);
  }

  #[test]
  fn position_anchor_moves_with_the_wall_clock() {
    let Some(Event::State(state)) = parse_event(EXPERIMENT_STATE) else {
      panic!("expected a playback state");
    };
    let anchor = state.position.unwrap();
    assert_eq!(anchor.at(1792921722717), 21344);
    assert_eq!(anchor.at(1792921723717), 22344);
    assert_eq!(anchor.at(1792921720000), 21344);
  }

  #[test]
  fn parse_event_reads_the_auth_and_change_events() {
    assert_eq!(
      parse_event(
        r#"{"type":"auth_state","logged_in":true,"is_active":false,"device_name":"spotatui"}"#
      ),
      Some(Event::Auth {
        logged_in: true,
        is_active: false
      })
    );
    assert_eq!(
      parse_event(r#"{"type":"playback_changed","status":"paused"}"#),
      Some(Event::Playback(Status::Paused))
    );
    assert_eq!(
      parse_event(r#"{"type":"options_changed","options":{"shuffle":false,"repeat":"track"}}"#),
      Some(Event::Options {
        shuffle: Some(false),
        repeat: Some(Repeat::Track)
      })
    );
  }

  #[test]
  fn parse_event_ignores_unknown_types() {
    assert_eq!(
      parse_event(r#"{"type":"command_result","command":"pause"}"#),
      None
    );
    assert_eq!(
      parse_event(r#"{"type":"queue_changed","previous":[]}"#),
      None
    );
    assert_eq!(parse_event("not json"), None);
  }
}
