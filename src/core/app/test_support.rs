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
