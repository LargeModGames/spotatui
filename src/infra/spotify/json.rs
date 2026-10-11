//! Web API JSON shapes built from internal metadata, kept to what rspotify's
//! models require and what the app reads.

use librespot_core::SpotifyUri;
use librespot_metadata::album::AlbumType;
use librespot_metadata::artist::Artists;
use librespot_metadata::image::Images;
use librespot_metadata::{Album, Artist, Episode, Show, Track};
use serde_json::{json, Value};

pub fn image_url(file_id: &librespot_core::FileId) -> String {
  format!("https://i.scdn.co/image/{}", file_id.to_base16())
}

/// Largest first, as the Web API lists them.
pub fn images(images: &Images) -> Value {
  let mut sorted: Vec<_> = images.iter().collect();
  sorted.sort_by_key(|image| std::cmp::Reverse(image.width));
  Value::Array(
    sorted
      .into_iter()
      .map(|image| {
        json!({
          "url": image_url(&image.id),
          "width": (image.width > 0).then_some(image.width),
          "height": (image.height > 0).then_some(image.height),
        })
      })
      .collect(),
  )
}

fn kind_of(uri: &SpotifyUri) -> &'static str {
  match uri {
    SpotifyUri::Track { .. } => "track",
    SpotifyUri::Album { .. } => "album",
    SpotifyUri::Artist { .. } => "artist",
    SpotifyUri::Episode { .. } => "episode",
    SpotifyUri::Show { .. } => "show",
    SpotifyUri::Playlist { .. } => "playlist",
    _ => "unknown",
  }
}

/// The `id`, `uri`, `type`, `href` and `external_urls` every object carries.
fn identity(uri: &SpotifyUri) -> serde_json::Map<String, Value> {
  let id = uri.to_id();
  let kind = kind_of(uri);
  let mut object = serde_json::Map::new();
  object.insert("id".into(), json!(id));
  object.insert("uri".into(), json!(uri.to_uri()));
  object.insert("type".into(), json!(kind));
  object.insert(
    "href".into(),
    json!(format!("https://api.spotify.com/v1/{kind}s/{id}")),
  );
  object.insert(
    "external_urls".into(),
    json!({ "spotify": format!("https://open.spotify.com/{kind}/{id}") }),
  );
  object
}

fn with(mut object: serde_json::Map<String, Value>, fields: Value) -> Value {
  if let Value::Object(fields) = fields {
    object.extend(fields);
  }
  Value::Object(object)
}

pub fn artist_ref(uri: &SpotifyUri, name: &str) -> Value {
  with(identity(uri), json!({ "name": name }))
}

pub fn artist_refs(artists: &Artists) -> Value {
  Value::Array(
    artists
      .iter()
      .map(|artist| artist_ref(&artist.id, &artist.name))
      .collect(),
  )
}

pub fn album_type(album_type: AlbumType) -> &'static str {
  match album_type {
    AlbumType::SINGLE | AlbumType::EP => "single",
    AlbumType::COMPILATION => "compilation",
    _ => "album",
  }
}

/// Only the year is reliable: a missing month or day reads as January or the 1st.
fn release_year(date: &librespot_core::date::Date) -> String {
  format!("{:04}", date.year().max(0))
}

pub fn album_simplified(album: &Album) -> Value {
  with(
    identity(&album.id),
    json!({
      "name": album.name,
      "album_type": album_type(album.album_type),
      "artists": artist_refs(&album.artists),
      "images": images(&album.covers),
      "release_date": release_year(&album.date),
      "release_date_precision": "year",
      "total_tracks": album.tracks().count(),
    }),
  )
}

pub fn track_simplified(track: &Track) -> Value {
  with(
    identity(&track.id),
    json!({
      "name": track.name,
      "artists": artist_refs(&track.artists),
      "disc_number": track.disc_number,
      "track_number": track.number,
      "duration_ms": i64::from(track.duration.max(0)),
      "explicit": track.is_explicit,
      "is_local": false,
    }),
  )
}

pub fn track_full(track: &Track) -> Value {
  let isrc = track
    .external_ids
    .iter()
    .find(|external| external.external_type == "isrc")
    .map(|external| external.id.clone());
  let mut value = track_simplified(track);
  if let Value::Object(object) = &mut value {
    object.insert("album".into(), album_simplified(&track.album));
    object.insert("popularity".into(), json!(track.popularity.max(0)));
    object.insert(
      "external_ids".into(),
      isrc.map_or_else(|| json!({}), |isrc| json!({ "isrc": isrc })),
    );
  }
  value
}

pub fn artist_full(artist: &Artist) -> Value {
  let portraits = if artist.portraits.is_empty() {
    &artist.portrait_group
  } else {
    &artist.portraits
  };
  with(
    identity(&artist.id),
    json!({
      "name": artist.name,
      "images": images(portraits),
      "genres": [],
      "popularity": artist.popularity.max(0),
      "followers": { "href": null, "total": 0 },
    }),
  )
}

pub fn album_full(album: &Album, tracks: Value) -> Value {
  let mut value = album_simplified(album);
  if let Value::Object(object) = &mut value {
    object.insert("tracks".into(), tracks);
    object.insert("genres".into(), json!([]));
    object.insert("copyrights".into(), json!([]));
    object.insert("label".into(), json!(album.label));
    object.insert("popularity".into(), json!(album.popularity.max(0)));
  }
  value
}

pub fn episode_simplified(episode: &Episode) -> Value {
  let published = &episode.publish_time;
  with(
    identity(&episode.id),
    json!({
      "name": episode.name,
      "description": episode.description,
      "duration_ms": i64::from(episode.duration.max(0)),
      "explicit": episode.is_explicit,
      "images": images(&episode.covers),
      "is_externally_hosted": false,
      "is_playable": true,
      "language": episode.language,
      "languages": [episode.language],
      "release_date": format!(
        "{:04}-{:02}-{:02}",
        published.year().max(0),
        published.month() as u8,
        published.day()
      ),
      "release_date_precision": "day",
    }),
  )
}

/// The episode metadata names its show but carries no show id or images.
pub fn episode_full(episode: &Episode) -> Value {
  let mut value = episode_simplified(episode);
  value["show"] = json!({
    "id": "",
    "href": "",
    "name": episode.show_name,
    "description": "",
    "publisher": "",
    "explicit": false,
    "external_urls": {},
    "images": [],
    "languages": [],
    "media_type": "audio",
    "copyrights": [],
  });
  value
}

pub fn show_simplified(show: &Show) -> Value {
  with(
    identity(&show.id),
    json!({
      "name": show.name,
      "description": show.description,
      "publisher": show.publisher,
      "explicit": show.is_explicit,
      "images": images(&show.covers),
      "languages": [show.language],
      "media_type": "audio",
      "copyrights": [],
      "is_externally_hosted": false,
      "total_episodes": show.episodes.len(),
    }),
  )
}

pub fn show_full(show: &Show, episodes: Value) -> Value {
  let mut value = show_simplified(show);
  if let Value::Object(object) = &mut value {
    object.insert("episodes".into(), episodes);
  }
  value
}

/// A Web API page. `next` is set while items remain, as callers loop on it.
pub fn page(items: Vec<Value>, offset: usize, limit: usize, total: usize) -> Value {
  let next = (offset + items.len() < total && !items.is_empty())
    .then(|| format!("internal://page?offset={}", offset + items.len()));
  json!({
    "href": "",
    "items": items,
    "limit": limit,
    "offset": offset,
    "total": total,
    "next": next,
    "previous": null,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_page_has_a_next_link_only_while_items_remain() {
    let first = page(vec![json!(1), json!(2)], 0, 2, 3);
    assert!(first["next"].is_string());
    let last = page(vec![json!(3)], 2, 2, 3);
    assert!(last["next"].is_null());
    let empty = page(vec![], 4, 2, 9);
    assert!(empty["next"].is_null(), "an empty page must end the loop");
  }

  #[test]
  fn an_ep_maps_to_a_single_and_unknown_types_to_an_album() {
    assert_eq!(album_type(AlbumType::EP), "single");
    assert_eq!(album_type(AlbumType::SINGLE), "single");
    assert_eq!(album_type(AlbumType::COMPILATION), "compilation");
    assert_eq!(album_type(AlbumType::PODCAST), "album");
  }

  #[test]
  fn an_artist_ref_deserializes_as_a_simplified_artist() {
    let uri = SpotifyUri::from_uri("spotify:artist:4tZwfgrHOc3mvqYlEYSvVi").unwrap();
    let artist: rspotify::model::SimplifiedArtist =
      serde_json::from_value(artist_ref(&uri, "Daft Punk")).unwrap();
    assert_eq!(artist.name, "Daft Punk");
    assert!(artist.id.is_some());
  }
}
