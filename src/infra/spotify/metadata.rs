//! Catalogue metadata from extended-metadata, many items per request, cached.

use super::router::{retry_read, upstream_error};
use super::session::SpotifySession;
use librespot_core::SpotifyUri;
use librespot_metadata::{Album, Artist, Episode, Metadata, Show, Track};
use librespot_protocol::extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery};
use librespot_protocol::extension_kind::ExtensionKind;
use protobuf::{EnumOrUnknown, Message};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Spotify answers larger batches, but a failed one then costs more.
const BATCH_SIZE: usize = 100;
const CACHE_CAPACITY: usize = 4000;

pub trait Kind: Metadata + Send + Sync {
  const EXTENSION: ExtensionKind;
  fn cache() -> &'static Mutex<HashMap<String, Arc<Self>>>;
}

macro_rules! kind {
  ($type:ty, $extension:ident) => {
    impl Kind for $type {
      const EXTENSION: ExtensionKind = ExtensionKind::$extension;
      fn cache() -> &'static Mutex<HashMap<String, Arc<Self>>> {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<$type>>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
      }
    }
  };
}

kind!(Track, TRACK_V4);
kind!(Album, ALBUM_V4);
kind!(Artist, ARTIST_V4);
kind!(Episode, EPISODE_V4);
kind!(Show, SHOW_V4);

/// Metadata for each uri, in the order of `uris`. An item Spotify does not
/// know is `None`.
pub async fn get_many<T: Kind>(
  session: &SpotifySession,
  uris: &[String],
) -> anyhow::Result<Vec<Option<Arc<T>>>> {
  let mut found: HashMap<String, Arc<T>> = HashMap::new();
  let mut missing = Vec::new();
  {
    let cache = T::cache().lock().unwrap_or_else(|e| e.into_inner());
    for uri in uris {
      match cache.get(uri) {
        Some(item) => {
          found.insert(uri.clone(), item.clone());
        }
        None if !missing.contains(uri) => missing.push(uri.clone()),
        None => {}
      }
    }
  }

  for batch in missing.chunks(BATCH_SIZE) {
    let fetched = fetch_batch::<T>(session, batch).await?;
    let mut cache = T::cache().lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() + fetched.len() > CACHE_CAPACITY {
      cache.clear();
    }
    for (uri, item) in fetched {
      cache.insert(uri.clone(), item.clone());
      found.insert(uri, item);
    }
  }

  Ok(uris.iter().map(|uri| found.get(uri).cloned()).collect())
}

pub async fn get_one<T: Kind>(session: &SpotifySession, uri: &str) -> anyhow::Result<Arc<T>> {
  get_many::<T>(session, &[uri.to_string()])
    .await?
    .pop()
    .flatten()
    .ok_or_else(|| super::router::api_error(reqwest::StatusCode::NOT_FOUND, "Not found"))
}

async fn fetch_batch<T: Kind>(
  session: &SpotifySession,
  uris: &[String],
) -> anyhow::Result<Vec<(String, Arc<T>)>> {
  let request = BatchedEntityRequest {
    entity_request: uris
      .iter()
      .map(|uri| EntityRequest {
        entity_uri: uri.clone(),
        query: vec![ExtensionQuery {
          extension_kind: EnumOrUnknown::new(T::EXTENSION),
          ..Default::default()
        }],
        ..Default::default()
      })
      .collect(),
    ..Default::default()
  };
  let spclient = session.session().spclient();
  let response = retry_read(|| spclient.get_extended_metadata(request.clone()))
    .await
    .map_err(upstream_error)?;

  let mut items = Vec::new();
  for array in response.extended_metadata {
    for data in array.extension_data {
      let Some(any) = data.extension_data.as_ref() else {
        continue;
      };
      let Ok(uri) = SpotifyUri::from_uri(&data.entity_uri) else {
        continue;
      };
      let Ok(message) = T::Message::parse_from_bytes(&any.value) else {
        continue;
      };
      if let Ok(item) = T::parse(&message, &uri) {
        items.push((data.entity_uri.clone(), Arc::new(item)));
      }
    }
  }
  Ok(items)
}
