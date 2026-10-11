//! Playback state and remote control through connect-state. A hidden observer
//! device keeps the account's device cluster, pushed by the dealer, in memory.

use super::json;
use super::metadata::get_many;
use super::router::{api_error, query_value, upstream_error};
use super::session::SpotifySession;
use anyhow::anyhow;
use futures::StreamExt;
use librespot_core::dealer::protocol::{Message, TransferOptions};
use librespot_core::session::Session;
use librespot_core::spclient::TransferRequest;
use librespot_metadata::{Episode, Track};
use librespot_protocol::connect::{
  Capabilities, Cluster, ClusterUpdate, Device, DeviceInfo, MemberType, PutStateReason,
  PutStateRequest,
};
use librespot_protocol::devices::DeviceType;
use protobuf::{EnumOrUnknown, Message as _, MessageField};
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CONNECTION_ID_TIMEOUT: Duration = Duration::from_secs(10);
const QUEUE_LENGTH: usize = 20;

pub struct Observer {
  cluster: Mutex<Cluster>,
}

impl Observer {
  pub async fn start(session: &Session) -> anyhow::Result<Arc<Self>> {
    let mut connection_ids =
      session
        .dealer()
        .listen_for("hm://pusher/v1/connections/", |message| {
          Ok(
            message
              .headers
              .get("Spotify-Connection-Id")
              .cloned()
              .unwrap_or_default(),
          )
        })?;
    let mut updates = session.dealer().listen_for(
      "hm://connect-state/v1/cluster",
      Message::from_raw::<ClusterUpdate>,
    )?;
    session.dealer().start().await?;
    let connection_id = tokio::time::timeout(CONNECTION_ID_TIMEOUT, connection_ids.next())
      .await
      .map_err(|_| anyhow!("no dealer connection id"))?
      .ok_or_else(|| anyhow!("dealer closed"))??;
    session.set_connection_id(&connection_id);
    let observer = Arc::new(Self {
      cluster: Mutex::new(register(session).await?),
    });

    let weak = Arc::downgrade(&observer);
    let session = session.clone();
    tokio::spawn(async move {
      loop {
        let cluster = tokio::select! {
          id = connection_ids.next() => match id {
            Some(Ok(id)) => {
              session.set_connection_id(&id);
              match register(&session).await {
                Ok(cluster) => Some(cluster),
                Err(e) => {
                  log::warn!("connect-state observer failed to register again: {e:#}");
                  None
                }
              }
            }
            Some(Err(_)) => None,
            None => break,
          },
          update = updates.next() => match update {
            Some(Ok(mut update)) => update.cluster.take(),
            Some(Err(_)) => None,
            None => break,
          },
        };
        let Some(observer) = weak.upgrade() else {
          break;
        };
        if let Some(cluster) = cluster {
          *observer.cluster.lock().unwrap_or_else(|e| e.into_inner()) = cluster;
        }
      }
    });
    Ok(observer)
  }

  fn cluster(&self) -> Cluster {
    self
      .cluster
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .clone()
  }

  /// A device that just got a transfer or a targeted command is active, before
  /// the dealer pushes the cluster that says so.
  fn assume_active(&self, device_id: &str, playing: Option<bool>) {
    let mut cluster = self.cluster.lock().unwrap_or_else(|e| e.into_inner());
    cluster.active_device_id = device_id.to_string();
    if let Some(playing) = playing {
      let state = cluster.player_state.mut_or_insert_default();
      state.is_playing = true;
      state.is_paused = !playing;
    }
  }
}

/// Registers this session as a hidden observer and returns the cluster.
async fn register(session: &Session) -> anyhow::Result<Cluster> {
  let request = PutStateRequest {
    member_type: EnumOrUnknown::new(MemberType::CONNECT_STATE),
    put_state_reason: EnumOrUnknown::new(PutStateReason::NEW_DEVICE),
    client_side_timestamp: chrono::Utc::now().timestamp_millis() as u64,
    device: MessageField::some(Device {
      device_info: MessageField::some(DeviceInfo {
        device_id: session.device_id().to_string(),
        name: "spotatui".into(),
        client_id: session.client_id(),
        device_type: EnumOrUnknown::new(DeviceType::OBSERVER),
        capabilities: MessageField::some(Capabilities {
          can_be_player: false,
          hidden: true,
          is_observable: true,
          supports_command_request: true,
          supports_transfer_command: true,
          supports_gzip_pushes: true,
          command_acks: true,
          ..Default::default()
        }),
        ..Default::default()
      }),
      ..Default::default()
    }),
    ..Default::default()
  };
  let bytes = session
    .spclient()
    .put_connect_state_request(&request)
    .await
    .map_err(upstream_error)?;
  Ok(Cluster::parse_from_bytes(&bytes)?)
}

fn device_type(device_type: DeviceType) -> &'static str {
  match device_type {
    DeviceType::COMPUTER | DeviceType::CHROMEBOOK => "Computer",
    DeviceType::TABLET => "Tablet",
    DeviceType::SMARTPHONE => "Smartphone",
    DeviceType::SPEAKER | DeviceType::HOME_THING => "Speaker",
    DeviceType::TV => "TV",
    DeviceType::AVR => "AVR",
    DeviceType::STB => "STB",
    DeviceType::AUDIO_DONGLE => "AudioDongle",
    DeviceType::GAME_CONSOLE => "GameConsole",
    DeviceType::CAST_VIDEO => "CastVideo",
    DeviceType::CAST_AUDIO => "CastAudio",
    DeviceType::AUTOMOBILE | DeviceType::CAR_THING => "Automobile",
    DeviceType::SMARTWATCH => "Smartwatch",
    _ => "Unknown",
  }
}

fn volume_percent(volume: u32) -> u32 {
  (volume.min(65535) * 100 + 32767) / 65535
}

/// The connect-state volume for a Web API percentage.
fn remote_volume(percent: u32) -> u16 {
  (percent.min(100) * 65535 / 100) as u16
}

fn device_json(id: &str, info: &DeviceInfo, active: bool) -> Value {
  json!({
    "id": id,
    "name": info.name,
    "type": device_type(info.device_type.enum_value_or(DeviceType::UNKNOWN)),
    "is_active": active,
    "is_private_session": info.is_private_session,
    "is_restricted": false,
    "volume_percent": volume_percent(info.volume),
  })
}

/// Devices a user can pick: not hidden, and not this session's observer.
fn visible_devices<'a>(
  cluster: &'a Cluster,
  own_id: &'a str,
) -> impl Iterator<Item = (&'a String, &'a DeviceInfo)> {
  cluster.device.iter().filter(move |(id, info)| {
    id.as_str() != own_id && !info.capabilities.as_ref().is_some_and(|c| c.hidden)
  })
}

fn context_kind(uri: &str) -> &str {
  uri.split(':').nth(1).unwrap_or("unknown")
}

fn repeat_state(cluster: &Cluster) -> &'static str {
  let options = &cluster.player_state.options;
  if options.repeating_track {
    "track"
  } else if options.repeating_context {
    "context"
  } else {
    "off"
  }
}

/// The position now, from the position the device reported and when.
fn progress_ms(cluster: &Cluster, now_ms: i64) -> i64 {
  let state = &cluster.player_state;
  let mut position = state.position_as_of_timestamp;
  if state.is_playing && !state.is_paused {
    let speed = if state.playback_speed > 0.0 {
      state.playback_speed
    } else {
      1.0
    };
    position += ((now_ms - state.timestamp).max(0) as f64 * speed) as i64;
  }
  if state.duration > 0 {
    position = position.min(state.duration);
  }
  position.max(0)
}

async fn playable(session: &SpotifySession, uri: &str) -> anyhow::Result<Option<Value>> {
  let uris = [uri.to_string()];
  Ok(if uri.starts_with("spotify:track:") {
    get_many::<Track>(session, &uris)
      .await?
      .pop()
      .flatten()
      .map(|t| json::track_full(&t))
  } else if uri.starts_with("spotify:episode:") {
    get_many::<Episode>(session, &uris)
      .await?
      .pop()
      .flatten()
      .map(|e| json::episode_full(&e))
  } else {
    None
  })
}

async fn observer(session: &SpotifySession) -> anyhow::Result<Arc<Observer>> {
  session.observer().await
}

/// `GET me/player`: no body when no device is active, as the Web API answers 204.
pub async fn playback_state(session: &SpotifySession) -> anyhow::Result<Value> {
  let cluster = observer(session).await?.cluster();
  let Some(device) = cluster.device.get(&cluster.active_device_id) else {
    return Ok(Value::Null);
  };
  let state = &cluster.player_state;
  let track_uri = state.track.uri.as_str();
  let item = playable(session, track_uri).await?;
  let kind = if track_uri.starts_with("spotify:episode:") {
    "episode"
  } else {
    "track"
  };
  let context = (!state.context_uri.is_empty()).then(|| {
    json!({
      "uri": state.context_uri,
      "href": "",
      "external_urls": {},
      "type": context_kind(&state.context_uri),
    })
  });
  Ok(json!({
    "device": device_json(&cluster.active_device_id, device, true),
    "repeat_state": repeat_state(&cluster),
    "shuffle_state": state.options.shuffling_context,
    "context": context,
    "timestamp": state.timestamp,
    "progress_ms": progress_ms(&cluster, chrono::Utc::now().timestamp_millis()),
    "is_playing": state.is_playing && !state.is_paused,
    "item": item,
    "currently_playing_type": kind,
    "actions": { "disallows": {} },
  }))
}

pub async fn devices(session: &SpotifySession) -> anyhow::Result<Value> {
  let cluster = observer(session).await?.cluster();
  let own_id = session.session().device_id().to_string();
  let devices: Vec<Value> = visible_devices(&cluster, &own_id)
    .map(|(id, info)| device_json(id, info, *id == cluster.active_device_id))
    .collect();
  Ok(json!({ "devices": devices }))
}

pub async fn queue(session: &SpotifySession) -> anyhow::Result<Value> {
  let cluster = observer(session).await?.cluster();
  let state = &cluster.player_state;
  let uris: Vec<String> = state
    .next_tracks
    .iter()
    .map(|track| track.uri.clone())
    .filter(|uri| uri.starts_with("spotify:track:") || uri.starts_with("spotify:episode:"))
    .take(QUEUE_LENGTH)
    .collect();
  Ok(json!({
    "currently_playing": playable(session, &state.track.uri).await?,
    "queue": playables(session, &uris).await?,
  }))
}

/// The tracks and episodes of `uris` in order, one batched lookup per kind.
async fn playables(session: &SpotifySession, uris: &[String]) -> anyhow::Result<Vec<Value>> {
  let (tracks, episodes): (Vec<String>, Vec<String>) = uris
    .iter()
    .cloned()
    .partition(|uri| uri.starts_with("spotify:track:"));
  let mut found: HashMap<&str, Value> = HashMap::new();
  for (uri, track) in tracks
    .iter()
    .zip(get_many::<Track>(session, &tracks).await?)
  {
    if let Some(track) = track {
      found.insert(uri, json::track_full(&track));
    }
  }
  for (uri, episode) in episodes
    .iter()
    .zip(get_many::<Episode>(session, &episodes).await?)
  {
    if let Some(episode) = episode {
      found.insert(uri, json::episode_full(&episode));
    }
  }
  Ok(
    uris
      .iter()
      .filter_map(|uri| found.get(uri.as_str()).cloned())
      .collect(),
  )
}

/// A device that refuses a command answers 403, which the Web API words as a
/// restriction; the app's error handling matches on those words.
fn command_error(error: librespot_core::Error) -> anyhow::Error {
  if error.kind == librespot_core::error::ErrorKind::PermissionDenied {
    return api_error(
      StatusCode::FORBIDDEN,
      r#"{"error":{"status":403,"message":"Player command failed: Restriction violated","reason":"UNKNOWN"}}"#,
    );
  }
  upstream_error(error)
}

fn no_active_device() -> anyhow::Error {
  api_error(
    StatusCode::NOT_FOUND,
    r#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#,
  )
}

/// The device a command goes to: `device_id` from the query, else the active one.
fn target(cluster: &Cluster, query: &[(&str, String)]) -> anyhow::Result<String> {
  match query_value(query, "device_id") {
    Some(id) if !id.is_empty() => Ok(id.to_string()),
    _ if !cluster.active_device_id.is_empty() => Ok(cluster.active_device_id.clone()),
    _ => Err(no_active_device()),
  }
}

async fn command(
  session: &SpotifySession,
  query: &[(&str, String)],
  command: Value,
) -> anyhow::Result<Value> {
  let observer = observer(session).await?;
  let to = target(&observer.cluster(), query)?;
  session
    .session()
    .spclient()
    .send_player_command(&to, command)
    .await
    .map_err(command_error)?;
  if query_value(query, "device_id").is_some() {
    observer.assume_active(&to, None);
  }
  Ok(Value::Null)
}

fn is_playing_on(cluster: &Cluster, device_id: &str) -> bool {
  let state = &cluster.player_state;
  cluster.active_device_id == device_id && state.is_playing && !state.is_paused
}

/// The `play` command for a Web API play body; `None` for a bare resume.
fn play_command(body: Option<&Value>) -> Option<Value> {
  let body = body.filter(|body| body.is_object())?;
  let offset = &body["offset"];
  let skip_to = match (offset["uri"].as_str(), offset["position"].as_u64()) {
    (Some(uri), _) => json!({ "track_uri": uri }),
    (None, Some(position)) => json!({ "track_index": position }),
    (None, None) => json!({}),
  };
  let context = if let Some(uri) = body["context_uri"].as_str() {
    json!({ "uri": uri, "url": format!("context://{uri}"), "metadata": {} })
  } else {
    let tracks: Vec<Value> = body["uris"]
      .as_array()?
      .iter()
      .filter_map(Value::as_str)
      .map(|uri| json!({ "uri": uri }))
      .collect();
    json!({ "uri": "", "url": "", "metadata": {}, "pages": [{ "tracks": tracks }] })
  };
  Some(json!({
    "endpoint": "play",
    "context": context,
    "play_origin": { "feature_identifier": "spotatui", "feature_version": env!("CARGO_PKG_VERSION") },
    "options": {
      "license": "premium",
      "skip_to": skip_to,
      "player_options_override": {},
    },
  }))
}

pub async fn play(
  session: &SpotifySession,
  query: &[(&str, String)],
  body: Option<&Value>,
) -> anyhow::Result<Value> {
  match play_command(body) {
    Some(play) => command(session, query, play).await,
    None => {
      let cluster = observer(session).await?.cluster();
      if is_playing_on(&cluster, &target(&cluster, query)?) {
        return Ok(Value::Null);
      }
      command(session, query, json!({ "endpoint": "resume" })).await
    }
  }
}

pub async fn simple(
  session: &SpotifySession,
  query: &[(&str, String)],
  endpoint: &str,
) -> anyhow::Result<Value> {
  command(session, query, json!({ "endpoint": endpoint })).await
}

pub async fn seek(session: &SpotifySession, query: &[(&str, String)]) -> anyhow::Result<Value> {
  let position: u64 = query_value(query, "position_ms")
    .and_then(|value| value.parse().ok())
    .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Missing position_ms"))?;
  command(
    session,
    query,
    json!({ "endpoint": "seek_to", "value": position, "position": position }),
  )
  .await
}

pub async fn shuffle(session: &SpotifySession, query: &[(&str, String)]) -> anyhow::Result<Value> {
  let state = query_value(query, "state") == Some("true");
  command(
    session,
    query,
    json!({ "endpoint": "set_shuffling_context", "value": state }),
  )
  .await
}

/// The repeat-context and repeat-track flags for a Web API repeat state.
fn repeat_flags(state: Option<&str>) -> (bool, bool) {
  match state {
    Some("track") => (true, true),
    Some("context") => (true, false),
    _ => (false, false),
  }
}

pub async fn repeat(session: &SpotifySession, query: &[(&str, String)]) -> anyhow::Result<Value> {
  let (context, track) = repeat_flags(query_value(query, "state"));
  command(
    session,
    query,
    json!({ "endpoint": "set_repeating_context", "value": context }),
  )
  .await?;
  command(
    session,
    query,
    json!({ "endpoint": "set_repeating_track", "value": track }),
  )
  .await
}

pub async fn volume(session: &SpotifySession, query: &[(&str, String)]) -> anyhow::Result<Value> {
  let percent: u32 = query_value(query, "volume_percent")
    .and_then(|value| value.parse().ok())
    .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Missing volume_percent"))?;
  let cluster = observer(session).await?.cluster();
  let to = target(&cluster, query)?;
  let volume = remote_volume(percent);
  session
    .session()
    .spclient()
    .set_remote_volume(&to, volume)
    .await
    .map_err(command_error)?;
  Ok(Value::Null)
}

pub async fn add_to_queue(
  session: &SpotifySession,
  query: &[(&str, String)],
) -> anyhow::Result<Value> {
  let uri =
    query_value(query, "uri").ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Missing uri"))?;
  command(
    session,
    query,
    json!({
      "endpoint": "add_to_queue",
      "track": { "uri": uri, "metadata": { "is_queued": "true" }, "provider": "queue" },
    }),
  )
  .await
}

/// The target device and the play flag of a `PUT me/player` body.
fn transfer_target(body: Option<&Value>) -> anyhow::Result<(&str, bool)> {
  let to = body
    .and_then(|body| body["device_ids"].get(0))
    .and_then(Value::as_str)
    .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "Missing device_ids"))?;
  let play = body
    .and_then(|body| body["play"].as_bool())
    .unwrap_or(false);
  Ok((to, play))
}

/// The device a transfer leaves. With nothing active, a transfer from the
/// target to itself activates it.
fn transfer_source(cluster: &Cluster, to: &str) -> String {
  if cluster.active_device_id.is_empty() {
    to.to_string()
  } else {
    cluster.active_device_id.clone()
  }
}

/// `PUT me/player` with `{"device_ids": [id], "play": bool}`.
pub async fn transfer(session: &SpotifySession, body: Option<&Value>) -> anyhow::Result<Value> {
  let (to, play) = transfer_target(body)?;
  let observer = observer(session).await?;
  let from = transfer_source(&observer.cluster(), to);
  let request = TransferRequest {
    transfer_options: TransferOptions {
      restore_paused: Some(if play { "resume" } else { "pause" }.to_string()),
      ..Default::default()
    },
  };
  session
    .session()
    .spclient()
    .transfer(&from, to, Some(&request))
    .await
    .map_err(command_error)?;
  observer.assume_active(to, Some(play));
  Ok(Value::Null)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn volume_maps_the_full_range_to_percent() {
    assert_eq!(volume_percent(0), 0);
    assert_eq!(volume_percent(65535), 100);
    assert_eq!(volume_percent(32768), 50);
  }

  #[test]
  fn a_play_body_becomes_a_play_command() {
    let context = play_command(Some(&json!({
      "context_uri": "spotify:playlist:x",
      "offset": { "uri": "spotify:track:y" },
    })))
    .unwrap();
    assert_eq!(context["context"]["uri"], "spotify:playlist:x");
    assert_eq!(
      context["options"]["skip_to"]["track_uri"],
      "spotify:track:y"
    );

    let list = play_command(Some(&json!({
      "uris": ["spotify:track:a", "spotify:track:b"],
      "offset": { "position": 1 },
    })))
    .unwrap();
    assert_eq!(
      list["context"]["pages"][0]["tracks"][1]["uri"],
      "spotify:track:b"
    );
    assert_eq!(list["options"]["skip_to"]["track_index"], 1);

    assert!(play_command(None).is_none(), "no body is a resume");
  }

  #[test]
  fn a_paused_player_does_not_advance() {
    let mut cluster = Cluster::new();
    cluster
      .player_state
      .mut_or_insert_default()
      .position_as_of_timestamp = 1000;
    cluster.player_state.mut_or_insert_default().timestamp = 0;
    cluster.player_state.mut_or_insert_default().duration = 5000;
    cluster.player_state.mut_or_insert_default().is_playing = true;
    cluster.player_state.mut_or_insert_default().is_paused = true;
    assert_eq!(progress_ms(&cluster, 3000), 1000);
    cluster.player_state.mut_or_insert_default().is_paused = false;
    assert_eq!(progress_ms(&cluster, 3000), 4000);
    assert_eq!(progress_ms(&cluster, 9000), 5000, "capped at the duration");
  }

  fn device(name: &str, device_type: DeviceType, hidden: bool) -> DeviceInfo {
    DeviceInfo {
      name: name.to_string(),
      device_type: EnumOrUnknown::new(device_type),
      volume: 65535,
      capabilities: MessageField::some(Capabilities {
        hidden,
        ..Default::default()
      }),
      ..Default::default()
    }
  }

  #[test]
  fn device_types_map_to_the_web_api_names() {
    assert_eq!(device_type(DeviceType::SMARTPHONE), "Smartphone");
    assert_eq!(device_type(DeviceType::HOME_THING), "Speaker");
    assert_eq!(device_type(DeviceType::CAR_THING), "Automobile");
    assert_eq!(device_type(DeviceType::CHROMEBOOK), "Computer");
    assert_eq!(device_type(DeviceType::OBSERVER), "Unknown");
  }

  #[test]
  fn a_device_deserializes_as_a_web_api_device() {
    let value = device_json("d1", &device("Phone", DeviceType::SMARTPHONE, false), true);
    let parsed: rspotify::model::Device = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.id.as_deref(), Some("d1"));
    assert_eq!(parsed.name, "Phone");
    assert!(parsed.is_active);
    assert_eq!(parsed.volume_percent, Some(100));
  }

  #[test]
  fn hidden_devices_and_the_own_observer_are_not_listed() {
    let mut cluster = Cluster::new();
    cluster.device.insert(
      "phone".into(),
      device("Phone", DeviceType::SMARTPHONE, false),
    );
    cluster
      .device
      .insert("ghost".into(), device("Ghost", DeviceType::SPEAKER, true));
    cluster
      .device
      .insert("me".into(), device("spotatui", DeviceType::OBSERVER, false));
    let listed: Vec<&String> = visible_devices(&cluster, "me").map(|(id, _)| id).collect();
    assert_eq!(listed, vec!["phone"]);
  }

  #[test]
  fn repeat_reads_the_track_flag_before_the_context_flag() {
    let mut cluster = Cluster::new();
    assert_eq!(repeat_state(&cluster), "off");
    let options = cluster
      .player_state
      .mut_or_insert_default()
      .options
      .mut_or_insert_default();
    options.repeating_context = true;
    assert_eq!(repeat_state(&cluster), "context");
    cluster
      .player_state
      .mut_or_insert_default()
      .options
      .mut_or_insert_default()
      .repeating_track = true;
    assert_eq!(repeat_state(&cluster), "track");
  }

  #[test]
  fn a_web_api_repeat_state_sets_both_flags() {
    assert_eq!(repeat_flags(Some("track")), (true, true));
    assert_eq!(repeat_flags(Some("context")), (true, false));
    assert_eq!(repeat_flags(Some("off")), (false, false));
    assert_eq!(repeat_flags(None), (false, false));
  }

  #[test]
  fn the_context_type_is_the_uri_kind() {
    assert_eq!(context_kind("spotify:playlist:x"), "playlist");
    assert_eq!(context_kind("spotify:album:y"), "album");
    assert_eq!(context_kind("garbage"), "unknown");
  }

  #[test]
  fn a_command_goes_to_the_query_device_then_the_active_one() {
    let mut cluster = Cluster::new();
    let query = [("device_id", "phone".to_string())];
    assert_eq!(target(&cluster, &query).unwrap(), "phone");
    let error = target(&cluster, &[]).unwrap_err();
    assert!(error.to_string().contains("NO_ACTIVE_DEVICE"), "{error}");
    cluster.active_device_id = "speaker".into();
    assert_eq!(target(&cluster, &[]).unwrap(), "speaker");
    assert_eq!(
      target(&cluster, &[("device_id", String::new())]).unwrap(),
      "speaker"
    );
  }

  #[test]
  fn only_an_unpaused_player_on_the_device_is_playing() {
    let mut cluster = Cluster::new();
    cluster.active_device_id = "phone".into();
    assert!(!is_playing_on(&cluster, "phone"));
    cluster.player_state.mut_or_insert_default().is_playing = true;
    assert!(is_playing_on(&cluster, "phone"));
    assert!(!is_playing_on(&cluster, "speaker"));
    cluster.player_state.mut_or_insert_default().is_paused = true;
    assert!(!is_playing_on(&cluster, "phone"));
  }

  #[test]
  fn a_refused_command_reads_as_a_web_api_restriction() {
    let refused = command_error(librespot_core::Error::permission_denied("no"));
    assert!(crate::infra::network::requests::is_forbidden_error(
      &refused
    ));
    assert!(
      refused.to_string().contains("Restriction violated"),
      "{refused}"
    );
    let other = command_error(librespot_core::Error::unavailable("down"));
    assert!(!crate::infra::network::requests::is_forbidden_error(&other));
  }

  #[test]
  fn a_percentage_round_trips_through_the_remote_volume() {
    assert_eq!(remote_volume(0), 0);
    assert_eq!(remote_volume(100), 65535);
    assert_eq!(remote_volume(250), 65535);
    for percent in [1, 33, 50, 99] {
      assert_eq!(volume_percent(u32::from(remote_volume(percent))), percent);
    }
  }

  #[test]
  fn a_transfer_body_names_the_device_and_the_play_flag() {
    let body = json!({ "device_ids": ["phone"], "play": true });
    assert_eq!(transfer_target(Some(&body)).unwrap(), ("phone", true));
    let body = json!({ "device_ids": ["phone"] });
    assert_eq!(transfer_target(Some(&body)).unwrap(), ("phone", false));
    assert!(transfer_target(Some(&json!({ "device_ids": [] }))).is_err());
    assert!(transfer_target(None).is_err());
  }

  #[test]
  fn with_nothing_active_a_transfer_starts_from_its_target() {
    let mut cluster = Cluster::new();
    assert_eq!(transfer_source(&cluster, "phone"), "phone");
    cluster.active_device_id = "speaker".into();
    assert_eq!(transfer_source(&cluster, "phone"), "speaker");
  }

  #[test]
  fn an_assumed_device_is_active_before_the_dealer_says_so() {
    let observer = Observer {
      cluster: Mutex::new(Cluster::new()),
    };
    observer.assume_active("phone", Some(false));
    let cluster = observer.cluster();
    assert_eq!(cluster.active_device_id, "phone");
    assert!(!is_playing_on(&cluster, "phone"), "a paused transfer");
    observer.assume_active("phone", Some(true));
    assert!(is_playing_on(&observer.cluster(), "phone"));
  }
}
