use super::*;

#[allow(deprecated)]
pub(super) fn full_track(id: &str, name: &str) -> FullTrack {
  FullTrack {
    album: SimplifiedAlbum {
      name: format!("{name} Album"),
      ..Default::default()
    },
    artists: vec![SimplifiedArtist {
      name: "Artist".to_string(),
      ..Default::default()
    }],
    available_markets: Vec::new(),
    disc_number: 1,
    duration: ChronoDuration::milliseconds(180_000),
    explicit: false,
    external_ids: HashMap::new(),
    external_urls: HashMap::new(),
    href: None,
    id: Some(TrackId::from_id(id).unwrap().into_static()),
    is_local: false,
    is_playable: Some(true),
    linked_from: None,
    restrictions: None,
    name: name.to_string(),
    popularity: 50,
    preview_url: None,
    track_number: 1,
    r#type: rspotify::model::Type::Track,
  }
}

pub(super) fn queue_track(uri: Option<&str>, name: &str) -> TrackInfo {
  TrackInfo {
    uri: uri.map(|u| u.to_string()),
    name: name.to_string(),
    artists: vec!["Artist".to_string()],
    album: "Album".to_string(),
    duration_ms: 1000,
    id: None,
    album_id: None,
    artist_refs: vec![],
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    image_url: None,
  }
}

#[allow(deprecated)]
pub(super) fn make_external_context() -> CurrentPlaybackContext {
  use rspotify::model::{context::Actions, CurrentlyPlayingType, Device, DeviceType, RepeatState};
  CurrentPlaybackContext {
    device: Device {
      id: Some("external".to_string()),
      is_active: true,
      is_private_session: false,
      is_restricted: false,
      name: "Phone".to_string(),
      _type: DeviceType::Smartphone,
      volume_percent: Some(50),
    },
    repeat_state: RepeatState::Off,
    shuffle_state: false,
    context: None,
    timestamp: Utc::now(),
    progress: None,
    is_playing: true,
    item: None,
    currently_playing_type: CurrentlyPlayingType::Track,
    actions: Actions::default(),
  }
}

pub(super) fn saved_track(id: &str, name: &str) -> SavedTrack {
  SavedTrack {
    added_at: Utc::now(),
    track: full_track(id, name),
  }
}

pub(super) fn saved_tracks_page(
  offset: u32,
  total: u32,
  ids: &[&str],
  has_next: bool,
) -> Page<SavedTrack> {
  Page {
    href: "https://example.com/me/tracks".to_string(),
    items: ids
      .iter()
      .enumerate()
      .map(|(index, id)| saved_track(id, &format!("Track {offset}-{index}")))
      .collect(),
    limit: ids.len() as u32,
    next: has_next.then(|| "https://example.com/me/tracks?next".to_string()),
    offset,
    previous: None,
    total,
  }
}

pub(super) fn make_app_simple() -> App {
  let (tx, _rx) = channel();
  App::new(tx, UserConfig::new(), Some(SystemTime::now()))
}

/// An `App` with no Spotify session; the receiver keeps `dispatch` observable.
pub(super) fn session_free_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
  let (tx, rx) = channel();
  (App::new(tx, UserConfig::new(), None), rx)
}

/// A playback context whose item is `track`: the suspended Spotify track the
/// queue-slot tests seed under the slot.
#[cfg(feature = "streaming")]
pub(super) fn playing_track_context(track: FullTrack) -> CurrentPlaybackContext {
  CurrentPlaybackContext {
    item: Some(PlayableItem::Track(track)),
    ..make_external_context()
  }
}

/// A session whose native backend was parked while its device held the paused
/// track, with the receivers for dispatched events and rebuild requests.
#[cfg(feature = "streaming")]
#[allow(deprecated)]
pub(super) fn parked_native_app() -> (
  App,
  std::sync::mpsc::Receiver<IoEvent>,
  tokio::sync::mpsc::UnboundedReceiver<crate::infra::player::StreamingRecoveryRequest>,
) {
  let (tx, rx) = channel();
  let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
  let (recovery_tx, recovery_rx) = tokio::sync::mpsc::unbounded_channel();
  app.streaming_recovery_tx = Some(recovery_tx);
  app.native_parked = true;
  app.native_device_id = Some("spotatui".to_string());
  app.native_is_playing = Some(false);
  let mut context = playing_track_context(full_track("0000000000000000000001", "Parked"));
  context.device.id = Some("spotatui".to_string());
  context.is_playing = false;
  context.context = Some(rspotify::model::context::Context {
    uri: "spotify:playlist:parked".to_string(),
    href: String::new(),
    external_urls: HashMap::new(),
    _type: rspotify::model::Type::Playlist,
  });
  app.current_playback_context = Some(context);
  app.record_native_playback_request(
    Some("spotify:playlist:parked".to_string()),
    None,
    None,
    false,
    false,
    RepeatState::Off,
  );
  (app, rx, recovery_rx)
}
