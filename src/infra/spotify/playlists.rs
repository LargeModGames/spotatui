//! Playlists from playlist/v2: the user's list, contents and edits.

use super::json;
use super::metadata::get_many;
use super::router::{api_error, query_value, retry_read, upstream_error, Paging};
use super::session::SpotifySession;
use chrono::{DateTime, Utc};
use librespot_core::SpotifyId;
use librespot_metadata::{Episode, Track};
use librespot_protocol::playlist4_external::{
  op::Kind, Add, ChangeInfo, Delta, Item, ItemAttributes, ListAttributes,
  ListAttributesPartialState, ListChanges, MetaItem, Op, Rem, SelectedListContent,
  UpdateListAttributes,
};
use protobuf::{Message, MessageField};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const ROOTLIST_TTL: Duration = Duration::from_secs(30);
const ROOTLIST_LENGTH: usize = 100_000;
/// The base revision of a playlist that does not exist yet.
const NEW_PLAYLIST_REVISION: &[u8] = b"\0\0\0\0root";

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_ms() -> i64 {
  Utc::now().timestamp_millis()
}

fn rootlist_cache() -> &'static Mutex<Option<(SelectedListContent, Instant)>> {
  static CACHE: OnceLock<Mutex<Option<(SelectedListContent, Instant)>>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(None))
}

async fn rootlist(session: &SpotifySession, fresh: bool) -> anyhow::Result<SelectedListContent> {
  let mut cache = rootlist_cache().lock().await;
  if let Some((content, fetched)) = cache.as_ref() {
    if !fresh && fetched.elapsed() < ROOTLIST_TTL {
      return Ok(content.clone());
    }
  }
  let spclient = session.session().spclient();
  let bytes = retry_read(|| spclient.get_rootlist(0, Some(ROOTLIST_LENGTH)))
    .await
    .map_err(upstream_error)?;
  let content = SelectedListContent::parse_from_bytes(&bytes)?;
  *cache = Some((content.clone(), Instant::now()));
  Ok(content)
}

async fn forget_rootlist() {
  *rootlist_cache().lock().await = None;
}

/// Playlist uris in the user's list, in sidebar order, without folder markers.
pub async fn listed_uris(session: &SpotifySession) -> anyhow::Result<Vec<String>> {
  let content = rootlist(session, false).await?;
  Ok(
    content
      .contents
      .items
      .iter()
      .map(|item| item.uri().to_string())
      .filter(|uri| uri.starts_with("spotify:playlist:"))
      .collect(),
  )
}

fn playlist_images(attributes: &ListAttributes) -> Value {
  if !attributes.picture_size.is_empty() {
    let size = |name: &str| match name {
      "xlarge" => 1280,
      "large" => 640,
      "default" => 300,
      _ => 64,
    };
    let mut sizes: Vec<_> = attributes.picture_size.iter().collect();
    sizes.sort_by_key(|picture| std::cmp::Reverse(size(picture.target_name())));
    return Value::Array(
      sizes
        .into_iter()
        .map(|picture| json!({ "url": picture.url(), "width": size(picture.target_name()), "height": size(picture.target_name()) }))
        .collect(),
    );
  }
  if attributes.has_picture() && !attributes.picture().is_empty() {
    return json!([{ "url": format!("https://i.scdn.co/image/{}", hex(attributes.picture())) }]);
  }
  json!([])
}

fn user_ref(username: &str) -> Value {
  json!({
    "id": username,
    "display_name": username,
    "type": "user",
    "uri": format!("spotify:user:{username}"),
    "href": format!("https://api.spotify.com/v1/users/{username}"),
    "external_urls": { "spotify": format!("https://open.spotify.com/user/{username}") },
  })
}

fn simplified_playlist(
  uri: &str,
  attributes: &ListAttributes,
  length: i32,
  owner: &str,
  revision: &[u8],
) -> Value {
  let id = uri.rsplit(':').next().unwrap_or_default();
  json!({
    "id": id,
    "uri": uri,
    "type": "playlist",
    "href": format!("https://api.spotify.com/v1/playlists/{id}"),
    "external_urls": { "spotify": format!("https://open.spotify.com/playlist/{id}") },
    "name": attributes.name(),
    "description": attributes.description(),
    "collaborative": attributes.collaborative(),
    "public": null,
    "images": playlist_images(attributes),
    "owner": user_ref(owner),
    "snapshot_id": hex(revision),
    "items": { "href": "", "total": length.max(0) },
  })
}

fn meta_playlist(uri: &str, meta: &MetaItem) -> Value {
  simplified_playlist(
    uri,
    &meta.attributes,
    meta.length(),
    meta.owner_username(),
    meta.revision(),
  )
}

/// `GET me/playlists`.
pub async fn user_playlists(
  session: &SpotifySession,
  query: &[(&str, String)],
) -> anyhow::Result<Value> {
  let content = rootlist(session, false).await?;
  let playlists: Vec<Value> = content
    .contents
    .items
    .iter()
    .enumerate()
    .filter(|(index, item)| {
      let viewable = content
        .contents
        .meta_items
        .get(*index)
        .is_none_or(|meta| meta.status_code() < 400);
      item.uri().starts_with("spotify:playlist:") && viewable
    })
    .map(
      |(index, item)| match content.contents.meta_items.get(index) {
        Some(meta) => meta_playlist(item.uri(), meta),
        None => simplified_playlist(item.uri(), &ListAttributes::new(), 0, "", &[]),
      },
    )
    .collect();
  let paging = Paging::from_query(query, 20);
  let items = paging.window(&playlists).to_vec();
  Ok(json::page(
    items,
    paging.offset,
    paging.limit,
    playlists.len(),
  ))
}

async fn fetch_playlist(
  session: &SpotifySession,
  id: &str,
  from: usize,
  length: usize,
) -> anyhow::Result<SelectedListContent> {
  let endpoint = format!(
    "/playlist/v2/playlist/{id}?decorate=revision,attributes,length,owner&from={from}&length={length}"
  );
  let spclient = session.session().spclient();
  let bytes = retry_read(|| spclient.request(&Method::GET, &endpoint, None, None))
    .await
    .map_err(upstream_error)?;
  Ok(SelectedListContent::parse_from_bytes(&bytes)?)
}

/// `GET playlists/{id}`.
pub async fn playlist(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let content = fetch_playlist(session, id, 0, 1).await?;
  let mut value = simplified_playlist(
    &format!("spotify:playlist:{id}"),
    &content.attributes,
    content.length(),
    content.owner_username(),
    content.revision(),
  );
  value["tracks"] = json!({ "href": "", "total": content.length().max(0) });
  Ok(value)
}

fn local_track(uri: &str) -> Value {
  let decode = |part: &str| {
    url::form_urlencoded::parse(format!("x={part}").as_bytes())
      .next()
      .map(|(_, value)| value.into_owned())
      .unwrap_or_default()
  };
  let parts: Vec<&str> = uri.splitn(6, ':').collect();
  let part = |index: usize| {
    parts
      .get(index)
      .map(|part| decode(part))
      .unwrap_or_default()
  };
  let seconds: i64 = part(5).parse().unwrap_or(0);
  json!({
    "id": null,
    "uri": uri,
    "type": "track",
    "name": part(4),
    "artists": [{ "name": part(2), "external_urls": {} }],
    "album": { "name": part(3), "artists": [], "images": [], "external_urls": {} },
    "duration_ms": seconds * 1000,
    "disc_number": 0,
    "track_number": 0,
    "explicit": false,
    "is_local": true,
    "external_urls": {},
  })
}

/// `GET playlists/{id}/items`. With a `fields` filter the items carry only
/// `item`, as the filtered Web API answer does.
pub async fn playlist_items(
  session: &SpotifySession,
  id: &str,
  query: &[(&str, String)],
) -> anyhow::Result<Value> {
  let paging = Paging::from_query(query, 100);
  let filtered = query_value(query, "fields").is_some();
  let content = fetch_playlist(session, id, paging.offset, paging.limit).await?;
  let items = &content.contents.items;
  let uris: Vec<String> = items.iter().map(|item| item.uri().to_string()).collect();
  let track_uris: Vec<String> = uris
    .iter()
    .filter(|uri| uri.starts_with("spotify:track:"))
    .cloned()
    .collect();
  let episode_uris: Vec<String> = uris
    .iter()
    .filter(|uri| uri.starts_with("spotify:episode:"))
    .cloned()
    .collect();
  let mut objects: HashMap<String, Value> = HashMap::new();
  for (uri, track) in track_uris
    .iter()
    .zip(get_many::<Track>(session, &track_uris).await?)
  {
    if let Some(track) = track {
      objects.insert(uri.clone(), json::track_full(&track));
    }
  }
  for (uri, episode) in episode_uris
    .iter()
    .zip(get_many::<Episode>(session, &episode_uris).await?)
  {
    if let Some(episode) = episode {
      objects.insert(uri.clone(), json::episode_full(&episode));
    }
  }

  let entries = items
    .iter()
    .map(|item| {
      let uri = item.uri();
      let is_local = uri.starts_with("spotify:local:");
      let object = if is_local {
        local_track(uri)
      } else {
        objects.get(uri).cloned().unwrap_or(Value::Null)
      };
      if filtered {
        return json!({ "item": object, "is_local": is_local });
      }
      let mut entry = json!({ "item": object, "is_local": is_local });
      let attributes = &item.attributes;
      if attributes.timestamp() > 0 {
        entry["added_at"] = json!(
          DateTime::<Utc>::from_timestamp_millis(attributes.timestamp())
            .unwrap_or_default()
            .to_rfc3339()
        );
      }
      if !attributes.added_by().is_empty() {
        entry["added_by"] = user_ref(attributes.added_by());
      }
      entry
    })
    .collect();
  Ok(json::page(
    entries,
    paging.offset,
    paging.limit,
    content.length().max(0) as usize,
  ))
}

fn list_changes(session: &SpotifySession, base_revision: &[u8], ops: Vec<Op>) -> ListChanges {
  let mut info = ChangeInfo::new();
  info.set_user(session.username());
  info.set_timestamp(now_ms());
  let mut changes = ListChanges::new();
  changes.set_base_revision(base_revision.to_vec());
  changes.deltas.push(Delta {
    ops,
    info: MessageField::some(info),
    ..Default::default()
  });
  changes.set_want_resulting_revisions(true);
  changes.set_want_sync_result(true);
  changes
}

fn item(uri: &str, timestamp: bool, public: Option<bool>) -> Item {
  let mut item = Item::new();
  item.set_uri(uri.to_string());
  let mut attributes = ItemAttributes::new();
  if timestamp {
    attributes.set_timestamp(now_ms());
  }
  if let Some(public) = public {
    attributes.set_public(public);
  }
  item.attributes = MessageField::some(attributes);
  item
}

fn add_op(items: Vec<Item>, from_index: Option<i32>) -> Op {
  let mut add = Add::new();
  add.items = items;
  match from_index {
    Some(index) => add.set_from_index(index),
    None => add.set_add_last(true),
  }
  let mut op = Op::new();
  op.set_kind(Kind::ADD);
  op.add = MessageField::some(add);
  op
}

fn remove_op(index: i32, uri: &str) -> Op {
  let mut rem = Rem::new();
  rem.set_from_index(index);
  rem.set_length(1);
  rem.items.push(item(uri, false, None));
  let mut op = Op::new();
  op.set_kind(Kind::REM);
  op.rem = MessageField::some(rem);
  op
}

fn rename_op(name: Option<&str>, description: Option<&str>) -> Op {
  let mut values = ListAttributes::new();
  if let Some(name) = name {
    values.set_name(name.to_string());
  }
  if let Some(description) = description {
    values.set_description(description.to_string());
  }
  let mut op = Op::new();
  op.set_kind(Kind::UPDATE_LIST_ATTRIBUTES);
  op.update_list_attributes = MessageField::some(UpdateListAttributes {
    new_attributes: MessageField::some(ListAttributesPartialState {
      values: MessageField::some(values),
      ..Default::default()
    }),
    ..Default::default()
  });
  op
}

fn playlist_id(id: &str) -> anyhow::Result<SpotifyId> {
  SpotifyId::from_base62(id).map_err(|_| api_error(StatusCode::BAD_REQUEST, "Invalid playlist id"))
}

async fn post_changes(
  session: &SpotifySession,
  id: &str,
  changes: &ListChanges,
) -> anyhow::Result<Value> {
  let reply = session
    .session()
    .spclient()
    .post_playlist_changes(&playlist_id(id)?, changes)
    .await
    .map_err(upstream_error)?;
  Ok(json!({ "snapshot_id": hex(reply.revision()) }))
}

fn body_uris(body: Option<&Value>, key: &str) -> Vec<Value> {
  body
    .and_then(|body| body[key].as_array())
    .cloned()
    .unwrap_or_default()
}

/// `POST playlists/{id}/items` with `{"uris": [...]}`, added at the end.
pub async fn add_items(
  session: &SpotifySession,
  id: &str,
  body: Option<&Value>,
) -> anyhow::Result<Value> {
  let uris: Vec<String> = body_uris(body, "uris")
    .iter()
    .filter_map(|uri| uri.as_str().map(str::to_string))
    .collect();
  if uris.is_empty() {
    return Err(api_error(StatusCode::BAD_REQUEST, "No uris"));
  }
  let current = fetch_playlist(session, id, 0, 1).await?;
  let items = uris.iter().map(|uri| item(uri, true, None)).collect();
  let changes = list_changes(session, current.revision(), vec![add_op(items, None)]);
  post_changes(session, id, &changes).await
}

/// The positions to remove, last first so earlier removals keep later
/// positions valid. An entry without positions removes every occurrence; a
/// given position that holds another track is skipped.
fn removal_positions(entries: &[Value], contents: &[String]) -> Vec<(i32, String)> {
  let mut positions = Vec::new();
  for entry in entries {
    let Some(uri) = entry["uri"].as_str() else {
      continue;
    };
    match entry["positions"].as_array() {
      Some(given) if !given.is_empty() => positions.extend(
        given
          .iter()
          .filter_map(Value::as_u64)
          .filter(|position| {
            usize::try_from(*position)
              .ok()
              .and_then(|position| contents.get(position))
              .is_some_and(|listed| listed == uri)
          })
          .map(|position| (position as i32, uri.to_string())),
      ),
      _ => positions.extend(
        contents
          .iter()
          .enumerate()
          .filter(|(_, listed)| *listed == uri)
          .map(|(position, _)| (position as i32, uri.to_string())),
      ),
    }
  }
  positions.sort_by_key(|(position, _)| std::cmp::Reverse(*position));
  positions.dedup();
  positions
}

/// `DELETE playlists/{id}/items` with `{"items": [{"uri", "positions"?}]}`.
pub async fn remove_items(
  session: &SpotifySession,
  id: &str,
  body: Option<&Value>,
) -> anyhow::Result<Value> {
  let mut entries = body_uris(body, "items");
  if entries.is_empty() {
    entries = body_uris(body, "tracks");
  }
  // The whole list, so a given position can be checked against its track.
  let first = fetch_playlist(session, id, 0, 1).await?;
  let total = first.length().max(0) as usize;
  let current = fetch_playlist(session, id, 0, total.max(1)).await?;
  let contents: Vec<String> = current
    .contents
    .items
    .iter()
    .map(|item| item.uri().to_string())
    .collect();
  let ops: Vec<Op> = removal_positions(&entries, &contents)
    .into_iter()
    .map(|(position, uri)| remove_op(position, &uri))
    .collect();
  if ops.is_empty() {
    // A position that holds another track now means the list moved under the page.
    let positioned = entries.iter().any(|entry| {
      entry["positions"]
        .as_array()
        .is_some_and(|given| !given.is_empty())
    });
    anyhow::ensure!(
      !positioned,
      "The playlist changed since it loaded; open it again and retry"
    );
    return Ok(json!({ "snapshot_id": hex(current.revision()) }));
  }
  let changes = list_changes(session, current.revision(), ops);
  post_changes(session, id, &changes).await
}

/// `POST me/playlists`: a new playlist, put at the top of the user's list.
pub async fn create(session: &SpotifySession, body: Option<&Value>) -> anyhow::Result<Value> {
  let name = body
    .and_then(|body| body["name"].as_str())
    .unwrap_or("New Playlist");
  let description = body
    .and_then(|body| body["description"].as_str())
    .filter(|description| !description.is_empty());
  let public = body
    .and_then(|body| body["public"].as_bool())
    .unwrap_or(true);
  let mut raw = [0u8; 16];
  rand::Rng::fill_bytes(&mut rand::rng(), &mut raw);
  let id = SpotifyId::from_raw(&raw)
    .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "Invalid new id"))?
    .to_base62();
  let changes = list_changes(
    session,
    NEW_PLAYLIST_REVISION,
    vec![rename_op(Some(name), description)],
  );
  post_changes(session, &id, &changes).await?;
  let uri = format!("spotify:playlist:{id}");
  add_to_rootlist(session, &uri, Some(public)).await?;
  let mut attributes = ListAttributes::new();
  attributes.set_name(name.to_string());
  Ok(simplified_playlist(
    &uri,
    &attributes,
    0,
    &session.username(),
    &[],
  ))
}

async fn add_to_rootlist(
  session: &SpotifySession,
  uri: &str,
  public: Option<bool>,
) -> anyhow::Result<()> {
  let current = rootlist(session, true).await?;
  let changes = list_changes(
    session,
    current.revision(),
    vec![add_op(vec![item(uri, true, public)], Some(0))],
  );
  session
    .session()
    .spclient()
    .post_rootlist_changes(&changes)
    .await
    .map_err(upstream_error)?;
  forget_rootlist().await;
  Ok(())
}

/// Follows a playlist: adds it to the user's list.
pub async fn list(session: &SpotifySession, uri: &str) -> anyhow::Result<()> {
  add_to_rootlist(session, uri, Some(true)).await
}

/// Unfollows a playlist: removes it from the user's list.
pub async fn unlist(session: &SpotifySession, uri: &str) -> anyhow::Result<()> {
  let current = rootlist(session, true).await?;
  let Some(index) = current
    .contents
    .items
    .iter()
    .position(|item| item.uri() == uri)
  else {
    return Ok(());
  };
  let changes = list_changes(
    session,
    current.revision(),
    vec![remove_op(index as i32, uri)],
  );
  session
    .session()
    .spclient()
    .post_rootlist_changes(&changes)
    .await
    .map_err(upstream_error)?;
  forget_rootlist().await;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn removals_run_from_the_last_position() {
    let contents = vec!["a".to_string(), "b".to_string(), "a".to_string()];
    let entries = vec![
      json!({ "uri": "a" }),
      json!({ "uri": "b", "positions": [1] }),
    ];
    assert_eq!(
      removal_positions(&entries, &contents),
      vec![
        (2, "a".to_string()),
        (1, "b".to_string()),
        (0, "a".to_string())
      ]
    );
  }

  #[test]
  fn a_position_that_holds_another_track_is_not_removed() {
    let contents = vec!["a".to_string(), "b".to_string()];
    let entries = vec![json!({ "uri": "a", "positions": [1, 0, 7] })];
    assert_eq!(
      removal_positions(&entries, &contents),
      vec![(0, "a".to_string())]
    );
  }

  #[test]
  fn a_local_uri_becomes_a_local_track() {
    let track = local_track("spotify:local:Some+Artist:Album:My+Song%21:215");
    assert_eq!(track["name"], "My Song!");
    assert_eq!(track["artists"][0]["name"], "Some Artist");
    assert_eq!(track["duration_ms"], 215_000);
    let parsed: rspotify::model::FullTrack = serde_json::from_value({
      let mut value = track;
      value["external_ids"] = json!({});
      value
    })
    .unwrap();
    assert!(parsed.is_local);
  }

  #[test]
  fn a_playlist_deserializes_as_a_simplified_playlist() {
    let mut attributes = ListAttributes::new();
    attributes.set_name("Mix".into());
    let value = simplified_playlist(
      "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
      &attributes,
      7,
      "jay",
      &[1, 2],
    );
    let playlist: rspotify::model::SimplifiedPlaylist = serde_json::from_value(value).unwrap();
    assert_eq!(playlist.name, "Mix");
    assert_eq!(playlist.items.total, 7);
  }

  #[test]
  fn a_revision_is_written_as_lowercase_hex() {
    assert_eq!(hex(&[0x00, 0x0f, 0xab]), "000fab");
    assert_eq!(hex(&[]), "");
  }

  #[test]
  fn sized_pictures_are_listed_largest_first() {
    let mut attributes = ListAttributes::new();
    for (name, url) in [("small", "s"), ("xlarge", "x"), ("default", "d")] {
      let mut picture = librespot_protocol::playlist4_external::PictureSize::new();
      picture.set_target_name(name.to_string());
      picture.set_url(url.to_string());
      attributes.picture_size.push(picture);
    }
    let images = playlist_images(&attributes);
    let urls: Vec<&str> = images
      .as_array()
      .unwrap()
      .iter()
      .map(|image| image["url"].as_str().unwrap())
      .collect();
    assert_eq!(urls, vec!["x", "d", "s"]);
    assert_eq!(images[0]["width"], 1280);
  }

  #[test]
  fn a_bare_picture_id_becomes_a_cdn_image_and_none_is_empty() {
    let mut attributes = ListAttributes::new();
    assert_eq!(playlist_images(&attributes), json!([]));
    attributes.set_picture(vec![0xab, 0xcd]);
    assert_eq!(
      playlist_images(&attributes)[0]["url"],
      "https://i.scdn.co/image/abcd"
    );
  }

  #[test]
  fn an_owner_deserializes_as_a_public_user() {
    let user: rspotify::model::PublicUser = serde_json::from_value(user_ref("jay")).unwrap();
    assert_eq!(rspotify::prelude::Id::id(&user.id), "jay");
    assert_eq!(user.display_name.as_deref(), Some("jay"));
  }

  #[test]
  fn an_add_goes_to_the_end_or_to_an_index() {
    let at_end = add_op(vec![item("spotify:track:a", false, None)], None);
    assert_eq!(at_end.kind(), Kind::ADD);
    assert!(at_end.add.add_last());
    assert!(!at_end.add.has_from_index());
    let at_top = add_op(vec![], Some(0));
    assert!(at_top.add.has_from_index());
    assert_eq!(at_top.add.from_index(), 0);
    assert_eq!(at_end.add.items[0].uri(), "spotify:track:a");
  }

  #[test]
  fn a_removal_takes_one_item_at_its_position() {
    let op = remove_op(4, "spotify:track:a");
    assert_eq!(op.kind(), Kind::REM);
    assert_eq!(op.rem.from_index(), 4);
    assert_eq!(op.rem.length(), 1);
    assert_eq!(op.rem.items[0].uri(), "spotify:track:a");
  }

  #[test]
  fn a_rename_sets_only_the_given_attributes() {
    let op = rename_op(Some("Road Trip"), None);
    assert_eq!(op.kind(), Kind::UPDATE_LIST_ATTRIBUTES);
    let values = &op.update_list_attributes.new_attributes.values;
    assert_eq!(values.name(), "Road Trip");
    assert!(!values.has_description());
  }

  #[test]
  fn body_uris_are_an_array_or_nothing() {
    let body = json!({ "uris": ["spotify:track:a"], "items": "not a list" });
    assert_eq!(
      body_uris(Some(&body), "uris"),
      vec![json!("spotify:track:a")]
    );
    assert!(body_uris(Some(&body), "items").is_empty());
    assert!(body_uris(None, "uris").is_empty());
  }

  #[test]
  fn a_playlist_id_must_be_base62() {
    assert!(playlist_id("37i9dQZF1DXcBWIGoYBM5M").is_ok());
    assert!(playlist_id("not-an-id!").is_err());
  }
}
