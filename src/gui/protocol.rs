//! The JSON the server and the page exchange over the socket. A channel message
//! carries that channel's whole state; a page applies it when its `rev` is at
//! least the last one it applied for that channel. Hello's revisions are informational.

use crate::core::action::Action;
use crate::core::app::{App, DisplayDomain, DisplayRevisions};
use crate::core::plugin_api::{
  device_list, route_name, DeviceInfo, PlaybackState, QueueItemSnapshot, QueueSnapshot, TrackInfo,
};
use crate::core::theme::{resolve, Color, Palette, Theme, ThemeField};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Message {
  Hello {
    payload: HelloPayload,
  },
  /// The playback position in ms, or `null` while the tick loop is stale.
  Tick {
    payload: Option<u64>,
  },
  Playback {
    rev: u64,
    payload: PlaybackPayload,
  },
  Queue {
    rev: u64,
    payload: Box<QueuePayload>,
  },
  Status {
    rev: u64,
    payload: StatusPayload,
  },
  /// Field name to RGB; `null` is `Reset`, the page's own default.
  Theme {
    rev: u64,
    payload: BTreeMap<String, Option<[u8; 3]>>,
  },
  Devices {
    rev: u64,
    payload: Vec<DeviceInfo>,
  },
  Route {
    rev: u64,
    payload: String,
  },
}

#[derive(Serialize)]
pub(crate) struct HelloPayload {
  version: &'static str,
  /// Present only on the connect that traded a launch code.
  token: Option<String>,
  revisions: DisplayRevisions,
}

#[derive(Serialize)]
pub(crate) struct PlaybackPayload {
  item: Option<NowPlaying>,
  volume: u32,
  device: Option<String>,
  liked: bool,
}

#[derive(Serialize)]
pub(crate) struct NowPlaying {
  title: String,
  artists: Vec<String>,
  album: String,
  image_url: Option<String>,
  duration_ms: u32,
  uri: Option<String>,
  is_playing: bool,
  is_live: bool,
  shuffle: bool,
  repeat: String,
}

#[derive(Serialize)]
pub(crate) struct QueuePayload {
  spotify: QueueSnapshot,
  native: Vec<TrackInfo>,
  now: Option<TrackInfo>,
}

#[derive(Serialize)]
pub(crate) struct StatusPayload {
  message: Option<String>,
  is_error: bool,
  api_error: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClientMessage {
  Action { action: Box<Action> },
  Quit,
}

pub(crate) fn hello(revisions: DisplayRevisions, token: Option<String>) -> Message {
  Message::Hello {
    payload: HelloPayload {
      version: env!("CARGO_PKG_VERSION"),
      token,
      revisions,
    },
  }
}

pub(crate) fn tick(app: &App) -> Message {
  Message::Tick {
    payload: app.playback_position_ms().map(|ms| ms as u64),
  }
}

/// Every channel whose revision moved since `sent`.
pub(crate) fn diff(sent: &DisplayRevisions, app: &App) -> Vec<Message> {
  let now = app.display_revisions();
  DisplayDomain::ALL
    .into_iter()
    .filter(|domain| now.get(*domain) != sent.get(*domain))
    .filter_map(|domain| channel_message(app, domain, now.get(domain)))
    .collect()
}

/// Every channel once.
pub(crate) fn resync(app: &App) -> Vec<Message> {
  let now = app.display_revisions();
  DisplayDomain::ALL
    .into_iter()
    .filter_map(|domain| channel_message(app, domain, now.get(domain)))
    .collect()
}

pub(crate) fn encode(message: &Message) -> String {
  serde_json::to_string(message).expect("a protocol message serializes")
}

fn channel_message(app: &App, domain: DisplayDomain, rev: u64) -> Option<Message> {
  Some(match domain {
    DisplayDomain::Route => Message::Route {
      rev,
      payload: route_name(app.get_current_route()),
    },
    DisplayDomain::Status => Message::Status {
      rev,
      payload: StatusPayload {
        message: app.status_message().map(str::to_string),
        is_error: app.status_message_is_error(),
        api_error: Some(app.api_error())
          .filter(|error| !error.is_empty())
          .map(str::to_string),
      },
    },
    DisplayDomain::Theme => Message::Theme {
      rev,
      payload: theme_colors(&app.user_config.theme),
    },
    DisplayDomain::Playback => Message::Playback {
      rev,
      payload: playback(app),
    },
    DisplayDomain::Devices => Message::Devices {
      rev,
      payload: device_list(app),
    },
    DisplayDomain::Queue => {
      let (spotify, native, now) = app.queue_view();
      let spotify = spotify
        .as_ref()
        .map(|queue| QueueSnapshot {
          currently_playing: queue
            .currently_playing
            .as_ref()
            .map(QueueItemSnapshot::from_playable),
          items: queue
            .queue
            .iter()
            .map(QueueItemSnapshot::from_playable)
            .collect(),
        })
        .unwrap_or_default();
      Message::Queue {
        rev,
        payload: Box::new(QueuePayload {
          spotify,
          native: native.clone(),
          now: now.clone(),
        }),
      }
    }
    DisplayDomain::Source
    | DisplayDomain::Party
    | DisplayDomain::Search
    | DisplayDomain::Lyrics
    | DisplayDomain::Artist
    | DisplayDomain::Library => return None,
  })
}

fn playback(app: &App) -> PlaybackPayload {
  let (snapshot, volume, device, liked) = app.playback_view();
  PlaybackPayload {
    item: snapshot.as_ref().map(|snapshot| NowPlaying {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      image_url: snapshot.metadata.image_url.clone(),
      duration_ms: snapshot.metadata.duration_ms,
      uri: snapshot.item_uri.clone(),
      is_playing: snapshot.is_playing,
      is_live: snapshot.is_live,
      shuffle: snapshot.shuffle,
      repeat: snapshot
        .repeat
        .map(PlaybackState::repeat_from)
        .unwrap_or_else(|| "off".to_string()),
    }),
    volume: *volume,
    device: device.clone(),
    liked: *liked,
  }
}

fn theme_colors(theme: &Theme) -> BTreeMap<String, Option<[u8; 3]>> {
  let palette = Palette::default();
  ThemeField::ALL
    .into_iter()
    .map(|field| {
      let color = theme.get(field);
      (
        field.name().to_string(),
        (color != Color::Reset).then(|| resolve(color, &palette)),
      )
    })
    .collect()
}

/// The first message of that kind, as JSON; panics when none was produced.
#[cfg(test)]
pub(crate) fn pushed(messages: &[Message], kind: &str) -> serde_json::Value {
  messages
    .iter()
    .map(|message| serde_json::to_value(message).unwrap())
    .find(|value| value["kind"] == kind)
    .unwrap_or_else(|| panic!("no {kind} message"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::action::NavTarget;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::Receiver;
  use std::time::SystemTime;

  fn app() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = std::sync::mpsc::channel();
    (App::new(tx, UserConfig::new(), Some(SystemTime::now())), rx)
  }

  fn apply_from_page(app: &mut App, text: &str) -> Vec<Message> {
    let before = app.display_revisions();
    let ClientMessage::Action { action } = serde_json::from_str(text).unwrap() else {
      panic!("not an action");
    };
    app.apply(*action);
    diff(&before, app)
  }

  fn action_frame(action: Action) -> String {
    serde_json::json!({ "type": "action", "action": action }).to_string()
  }

  #[test]
  fn resync_sends_every_channel_once_in_domain_order() {
    let (app, _rx) = app();

    let kinds: Vec<_> = resync(&app)
      .iter()
      .map(|message| serde_json::to_value(message).unwrap()["kind"].clone())
      .collect();

    assert_eq!(
      kinds,
      ["route", "status", "theme", "playback", "devices", "queue"]
    );
    let revisions = serde_json::to_value(app.display_revisions()).unwrap();
    assert_eq!(
      revisions.as_object().unwrap().len(),
      DisplayDomain::ALL.len()
    );
  }

  #[test]
  fn a_track_queued_from_the_page_pushes_the_queue_channel() {
    let (mut app, _rx) = app();
    let track = TrackInfo {
      uri: Some("subsonic:track:1".to_string()),
      name: "One".to_string(),
      artists: vec![],
      album: String::new(),
      duration_ms: 1000,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    };

    let messages = apply_from_page(&mut app, &action_frame(Action::QueueTrack(track)));

    let queue = pushed(&messages, "queue");
    assert_eq!(queue["payload"]["native"][0]["name"], "One");
    assert!(queue["rev"].as_u64().unwrap() > 0);
  }

  #[test]
  fn a_theme_set_from_the_page_pushes_rgb_and_leaves_reset_to_the_page() {
    let (mut app, _rx) = app();

    let messages = apply_from_page(
      &mut app,
      r#"{"type":"action","action":{"SetTheme":[["Active",{"Rgb":[1,2,3]}]]}}"#,
    );

    let theme = pushed(&messages, "theme");
    assert_eq!(theme["payload"]["active"], serde_json::json!([1, 2, 3]));
    assert!(theme["payload"]["text"].is_null());
  }

  #[test]
  fn a_navigation_from_the_page_pushes_the_route_name() {
    let (mut app, _rx) = app();

    let messages = apply_from_page(&mut app, &action_frame(Action::Navigate(NavTarget::Queue)));

    assert_eq!(pushed(&messages, "route")["payload"], "queue");
  }

  #[test]
  #[allow(deprecated)]
  fn a_cached_device_list_pushes_the_devices_channel() {
    use rspotify::model::device::{Device, DevicePayload};
    use rspotify::model::DeviceType;
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.set_devices(DevicePayload {
      devices: vec![Device {
        id: Some("device-1".to_string()),
        is_active: false,
        is_private_session: false,
        is_restricted: false,
        name: "Kitchen".to_string(),
        _type: DeviceType::Speaker,
        volume_percent: Some(40),
      }],
    });

    assert_eq!(
      pushed(&diff(&before, &app), "devices")["payload"][0]["name"],
      "Kitchen"
    );
  }

  #[test]
  fn a_tick_carries_the_position_and_no_revision() {
    let (mut app, _rx) = app();
    app.song_progress_ms = 1_234;

    assert_eq!(
      serde_json::to_value(tick(&app)).unwrap(),
      serde_json::json!({ "kind": "tick", "payload": 1234 })
    );
  }
}
