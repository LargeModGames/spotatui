use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream};

type WsStream = tokio_tungstenite::WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
  #[default]
  HostOnly,
  SharedControl,
}

impl ControlMode {
  /// The mode a relay message names; any unknown name is host-only.
  pub fn from_wire(mode: &str) -> Self {
    match mode {
      "shared_control" => ControlMode::SharedControl,
      _ => ControlMode::HostOnly,
    }
  }
}

impl std::fmt::Display for ControlMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ControlMode::HostOnly => write!(f, "Host Only"),
      ControlMode::SharedControl => write!(f, "Shared Control"),
    }
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackAction {
  Play,
  Pause,
  NextTrack,
  PrevTrack,
  /// Step back a track even past the first seconds, which a plain previous restarts.
  ForcePrevTrack,
  Seek {
    position_ms: u64,
  },
  PlayTrack {
    uri: String,
  },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncMessage {
  RoomCreated {
    code: String,
    control_mode: String,
  },
  JoinedRoom {
    host_name: String,
    /// Absent from a relay deployed before a late joiner learned the mode.
    #[serde(default)]
    control_mode: Option<String>,
  },
  GuestJoined {
    name: String,
  },
  GuestLeft {
    name: String,
  },
  SyncState {
    track_uri: String,
    position_ms: u64,
    is_playing: bool,
    timestamp: u64,
  },
  PlaybackCommand {
    action: PlaybackAction,
    from: Option<String>,
  },
  SetControlMode {
    control_mode: String,
  },
  SetName {
    name: String,
  },
  Heartbeat,
  Error {
    message: String,
  },
  RoomClosed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PartyRole {
  Host,
  Guest,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum PartyStatus {
  #[default]
  Disconnected,
  Connecting,
  Hosting,
  Joined,
}

impl std::fmt::Display for PartyStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      PartyStatus::Disconnected => write!(f, "Disconnected"),
      PartyStatus::Connecting => write!(f, "Connecting..."),
      PartyStatus::Hosting => write!(f, "Hosting"),
      PartyStatus::Joined => write!(f, "Joined"),
    }
  }
}

#[derive(Clone, Debug)]
pub struct PartySession {
  pub role: PartyRole,
  pub code: String,
  pub guests: Vec<String>,
  pub control_mode: ControlMode,
  pub host_name: String,
  /// When a guest last got a host state it follows; the host sends one every 2 s.
  pub host_state_at: Option<std::time::Instant>,
}

pub struct PartyConnection {
  write: SplitSink<WsStream, Message>,
}

impl PartyConnection {
  pub async fn send(&mut self, msg: &SyncMessage) -> Result<(), String> {
    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    self
      .write
      .send(Message::Text(json.into()))
      .await
      .map_err(|e| e.to_string())
  }

  pub async fn close(&mut self) {
    let _ = self.write.close().await;
  }
}

pub fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis() as u64
}

pub async fn connect_to_relay(
  relay_url: &str,
  action: &str,
  params: &[(&str, &str)],
) -> Result<(PartyConnection, SplitStream<WsStream>), String> {
  let mut url = url::Url::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
  url.query_pairs_mut().append_pair("action", action);
  for (k, v) in params {
    url.query_pairs_mut().append_pair(k, v);
  }

  info!("Connecting to party relay: action={}", action);
  let (ws_stream, _) = connect_async(url.as_str())
    .await
    .map_err(|e| format!("WebSocket connection failed: {}", e))?;

  let (write, read) = ws_stream.split();
  Ok((PartyConnection { write }, read))
}

pub fn parse_sync_message(text: &str) -> Option<SyncMessage> {
  match serde_json::from_str::<SyncMessage>(text) {
    Ok(msg) => Some(msg),
    Err(e) => {
      warn!("Failed to parse sync message: {} — raw: {}", e, text);
      None
    }
  }
}

pub async fn start_party_reader(
  mut read: SplitStream<WsStream>,
  incoming_tx: tokio::sync::mpsc::UnboundedSender<SyncMessage>,
) {
  while let Some(result) = read.next().await {
    match result {
      Ok(Message::Text(text)) => {
        if let Some(msg) = parse_sync_message(&text) {
          if incoming_tx.send(msg).is_err() {
            break;
          }
        }
      }
      Ok(Message::Close(_)) => {
        let _ = incoming_tx.send(SyncMessage::RoomClosed);
        break;
      }
      Ok(_) => {}
      Err(e) => {
        error!("WebSocket read error: {}", e);
        let _ = incoming_tx.send(SyncMessage::Error {
          message: format!("Connection lost: {}", e),
        });
        break;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn joined_room_carries_the_room_control_mode() {
    let msg = parse_sync_message(
      r#"{"type":"joined_room","host_name":"H","control_mode":"shared_control"}"#,
    );
    assert!(matches!(
      msg,
      Some(SyncMessage::JoinedRoom { control_mode: Some(mode), .. }) if mode == "shared_control"
    ));
  }

  #[test]
  fn joined_room_from_an_older_relay_still_parses() {
    let msg = parse_sync_message(r#"{"type":"joined_room","host_name":"H"}"#);
    assert!(matches!(
      msg,
      Some(SyncMessage::JoinedRoom {
        control_mode: None,
        ..
      })
    ));
  }
}
