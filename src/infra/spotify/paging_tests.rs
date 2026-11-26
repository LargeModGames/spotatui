//! Raw collection and catalogue fixtures for exercising the real paging routes offline.

use super::{metadata::Kind, session::SpotifySession};
use crate::core::{app::App, auth::SpotifyLogin, user_config::UserConfig};
use crate::infra::network::{IoEvent, Network};
use librespot_core::{SpotifyId, SpotifyUri};
use librespot_protocol::metadata as proto;
use protobuf::{Message, MessageField};
use std::sync::mpsc::{self, Receiver};
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::Mutex;

pub(crate) const PAGE_SIZE: u32 = 50;
pub(crate) const ARTIST_ID: &str = "0000000000000000000001";
pub(crate) const ALBUM_ID: &str = "0000000000000000000002";
pub(crate) const SHOW_ID: &str = "0000000000000000000003";
pub(crate) const SHOW_WITH_FIRST_EPISODE_ID: &str = "0000000000000000000006";
pub(crate) const TRACK_NAME: &str = "Available track";
pub(crate) const EPISODE_NAME: &str = "Available episode";
pub(crate) const FIRST_EPISODE_NAME: &str = "First episode";
pub(crate) const ALBUM_NAME: &str = "Available album";
pub(crate) const SHOW_NAME: &str = "Available show";

struct Fixture {
  session: Arc<SpotifySession>,
  metadata: HashMap<String, Vec<u8>>,
  collections: HashMap<&'static str, Vec<String>>,
}

tokio::task_local! {
  static FIXTURE: Fixture;
}

fn gid(id: &str) -> Vec<u8> {
  SpotifyId::from_base62(id).unwrap().to_raw().to_vec()
}

pub(crate) fn app_with_network() -> (Arc<Mutex<App>>, Network, Receiver<IoEvent>) {
  let (tx, rx) = mpsc::channel();
  let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
  let network = Network::new(Some(SpotifyLogin::new()), &app);
  (app, network, rx)
}

/// Two fully filtered windows, then one available item in the final window.
pub(crate) async fn with_filtered_pages<T>(run: impl Future<Output = T>) -> T {
  let unavailable_id = "0000000000000000000005";
  let track_id = "0000000000000000000004";
  let episodes = vec![
    proto::Episode {
      gid: Some(gid(unavailable_id)),
      ..Default::default()
    };
    (2 * PAGE_SIZE) as usize
  ];
  let episode = proto::Episode {
    gid: Some(gid(track_id)),
    name: Some(EPISODE_NAME.into()),
    ..Default::default()
  };
  let show = proto::Show {
    gid: Some(gid(SHOW_ID)),
    name: Some(SHOW_NAME.into()),
    episode: episodes.into_iter().chain([episode.clone()]).collect(),
    ..Default::default()
  };
  let first_episode_id = "0000000000000000000007";
  let first_episode = proto::Episode {
    gid: Some(gid(first_episode_id)),
    name: Some(FIRST_EPISODE_NAME.into()),
    ..Default::default()
  };
  let mut show_with_first_episode = show.clone();
  show_with_first_episode.gid = Some(gid(SHOW_WITH_FIRST_EPISODE_ID));
  show_with_first_episode.episode[0] = first_episode.clone();
  let mut album = proto::Album {
    gid: Some(gid(ALBUM_ID)),
    name: Some(ALBUM_NAME.into()),
    ..Default::default()
  };
  let track = proto::Track {
    gid: Some(gid(track_id)),
    name: Some(TRACK_NAME.into()),
    album: MessageField::some(album.clone()),
    ..Default::default()
  };
  let missing_track = proto::Track {
    gid: Some(gid(unavailable_id)),
    ..Default::default()
  };
  album.disc.push(proto::Disc {
    track: vec![missing_track; (2 * PAGE_SIZE) as usize]
      .into_iter()
      .chain([track.clone()])
      .collect(),
    ..Default::default()
  });
  let missing_album = proto::Album {
    gid: Some(gid(unavailable_id)),
    ..Default::default()
  };
  let artist = proto::Artist {
    gid: Some(gid(ARTIST_ID)),
    album_group: vec![
      proto::AlbumGroup {
        album: vec![missing_album],
        ..Default::default()
      };
      (2 * PAGE_SIZE) as usize
    ]
    .into_iter()
    .chain([proto::AlbumGroup {
      album: vec![album.clone()],
      ..Default::default()
    }])
    .collect(),
    ..Default::default()
  };
  let metadata = HashMap::from([
    (
      format!("spotify:album:{ALBUM_ID}"),
      album.write_to_bytes().unwrap(),
    ),
    (
      format!("spotify:artist:{ARTIST_ID}"),
      artist.write_to_bytes().unwrap(),
    ),
    (
      format!("spotify:track:{track_id}"),
      track.write_to_bytes().unwrap(),
    ),
    (
      format!("spotify:episode:{track_id}"),
      episode.write_to_bytes().unwrap(),
    ),
    (
      format!("spotify:show:{SHOW_ID}"),
      show.write_to_bytes().unwrap(),
    ),
    (
      format!("spotify:episode:{first_episode_id}"),
      first_episode.write_to_bytes().unwrap(),
    ),
    (
      format!("spotify:show:{SHOW_WITH_FIRST_EPISODE_ID}"),
      show_with_first_episode.write_to_bytes().unwrap(),
    ),
  ]);
  let collection = |kind: &str, id| {
    vec![format!("spotify:{kind}:{unavailable_id}"); (2 * PAGE_SIZE) as usize]
      .into_iter()
      .chain([format!("spotify:{kind}:{id}")])
      .collect()
  };
  let fixture = Fixture {
    session: Arc::new(SpotifySession::disconnected_for_test()),
    metadata,
    collections: HashMap::from([
      ("collection", collection("album", ALBUM_ID)),
      ("show", collection("show", SHOW_ID)),
    ]),
  };
  FIXTURE.scope(fixture, run).await
}

pub(super) fn session() -> Option<Arc<SpotifySession>> {
  FIXTURE.try_with(|fixture| fixture.session.clone()).ok()
}

pub(super) fn collection(set: &str) -> Option<Vec<String>> {
  FIXTURE
    .try_with(|fixture| fixture.collections.get(set).cloned().unwrap())
    .ok()
}

pub(super) fn metadata<T: Kind>(uris: &[String]) -> Option<Vec<Option<Arc<T>>>> {
  FIXTURE
    .try_with(|fixture| {
      uris
        .iter()
        .map(|uri| {
          let bytes = fixture.metadata.get(uri)?;
          let message = T::Message::parse_from_bytes(bytes).unwrap();
          Some(Arc::new(
            T::parse(&message, &SpotifyUri::from_uri(uri).unwrap()).unwrap(),
          ))
        })
        .collect()
    })
    .ok()
}
