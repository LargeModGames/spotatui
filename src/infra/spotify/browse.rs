//! Spotify's Made For You page through Pathfinder's persisted `browsePage` query.

use super::router::api_error;
use super::session::SpotifySession;
use librespot_core::error::ErrorKind;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Persisted query hashes, newest first; a retired one answers HTTP 412.
const BROWSE_PAGE_HASHES: &[&str] =
  &["f5c4e6d668f5716464a231c1cc8b22c1cbf6ad68b09929fd7de813a30581298b"];
const MADE_FOR_YOU_PAGE: &str = "spotify:page:0JQ5DAt0tbjZptfcdMSKl3";
/// The AI DJ: listed on the page, but librespot cannot play it.
const DJ_PLAYLIST: &str = "spotify:playlist:37i9dQZF1EYkqdzj48dyYq";

static WORKING_HASH: AtomicUsize = AtomicUsize::new(0);

/// `GET views/made-for-x`: `{sections: [{title, playlists: [{uri, name, description, image_url}]}]}`.
pub async fn made_for_you(session: &SpotifySession) -> anyhow::Result<Value> {
  let variables = json!({
    "uri": MADE_FOR_YOU_PAGE,
    "pagePagination": { "offset": 0, "limit": 20 },
    "sectionPagination": { "offset": 0, "limit": 20 },
    "browseEndUserIntegration": "INTEGRATION_WEB_PLAYER",
    "includeEpisodeContentRatingsV2": false,
  });
  let start = WORKING_HASH.load(Ordering::Relaxed);
  let mut refused = false;
  for step in 0..BROWSE_PAGE_HASHES.len() {
    let index = (start + step) % BROWSE_PAGE_HASHES.len();
    match session
      .session()
      .spclient()
      .pathfinder_query("browsePage", BROWSE_PAGE_HASHES[index], variables.clone())
      .await
    {
      // A failed page still answers HTTP 200, with a `GenericError` typename.
      Ok(answer) if answer["data"]["browse"]["__typename"] == "BrowseSectionContainer" => {
        WORKING_HASH.store(index, Ordering::Relaxed);
        return Ok(json!({ "sections": sections(&answer["data"]["browse"]) }));
      }
      Ok(answer) => {
        refused = true;
        log::warn!(
          "browsePage gave no page: {} {}",
          answer["data"]["browse"]["__typename"],
          answer["errors"]
        );
      }
      Err(e) if e.kind == ErrorKind::FailedPrecondition => {
        log::warn!(
          "browsePage hash {} is retired",
          &BROWSE_PAGE_HASHES[index][..8]
        );
      }
      Err(e) => return Err(super::router::upstream_error(e)),
    }
  }
  Err(api_error(
    StatusCode::SERVICE_UNAVAILABLE,
    if refused {
      "Spotify did not send the Made For You page"
    } else {
      "Made For You is unavailable: every known browsePage hash is retired"
    },
  ))
}

fn sections(page: &Value) -> Vec<Value> {
  page["sections"]["items"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|section| {
      let title = &section["data"]["title"];
      let title = title["transformedLabel"]
        .as_str()
        .or_else(|| title["text"].as_str())?;
      let playlists: Vec<Value> = section["sectionItems"]["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| playlist(&item["content"]["data"]))
        .collect();
      (!playlists.is_empty()).then(|| json!({ "title": title, "playlists": playlists }))
    })
    .collect()
}

fn playlist(data: &Value) -> Option<Value> {
  if data["__typename"] != "Playlist" {
    return None;
  }
  let uri = data["uri"].as_str()?;
  if !uri.starts_with("spotify:playlist:") || uri == DJ_PLAYLIST {
    return None;
  }
  let description = data["description"]
    .as_str()
    .map(plain_text)
    .filter(|text| !text.is_empty());
  Some(json!({
    "uri": uri,
    "name": data["name"].as_str().unwrap_or_default(),
    "description": description,
    "image_url": data["images"]["items"][0]["sources"][0]["url"],
  }))
}

/// Descriptions link artists as `<a href=…>Name</a>` and escape a few characters.
fn plain_text(html: &str) -> String {
  let mut text = String::with_capacity(html.len());
  let mut in_tag = false;
  for c in html.chars() {
    match c {
      '<' => in_tag = true,
      '>' if in_tag => in_tag = false,
      _ if !in_tag => text.push(c),
      _ => {}
    }
  }
  let mut decoded = String::with_capacity(text.len());
  let mut rest = text.as_str();
  while let Some(at) = rest.find('&') {
    decoded.push_str(&rest[..at]);
    rest = &rest[at..];
    let entity = rest[1..]
      .find(';')
      .filter(|end| *end <= 8)
      .and_then(|end| Some((entity(&rest[1..=end])?, end + 2)));
    match entity {
      Some((c, len)) => {
        decoded.push(c);
        rest = &rest[len..];
      }
      None => {
        decoded.push('&');
        rest = &rest[1..];
      }
    }
  }
  decoded.push_str(rest);
  decoded.trim().to_string()
}

fn entity(name: &str) -> Option<char> {
  match name {
    "amp" => Some('&'),
    "quot" => Some('"'),
    "apos" => Some('\''),
    "lt" => Some('<'),
    "gt" => Some('>'),
    "nbsp" => Some(' '),
    _ => {
      let number = name.strip_prefix('#')?;
      let code = match number.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => number.parse().ok()?,
      };
      char::from_u32(code)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn item(typename: &str, uri: &str, name: &str, description: &str) -> Value {
    json!({ "content": { "data": {
      "__typename": typename,
      "uri": uri,
      "name": name,
      "description": description,
      "images": { "items": [{ "sources": [{ "url": "https://i.scdn.co/image/a" }] }] },
    }}})
  }

  #[test]
  fn shelves_keep_their_playlists_and_drop_what_cannot_play() {
    let page = json!({ "sections": { "items": [
      { "data": { "title": { "transformedLabel": "Uniquely yours" } }, "sectionItems": { "items": [
        item("Playlist", DJ_PLAYLIST, "DJ", ""),
        item("Playlist", "spotify:playlist:daylist", "daylist", "Your day in a playlist."),
        item("NotFound", "", "", ""),
      ]}},
      { "data": { "title": { "transformedLabel": "Made For Us" } }, "sectionItems": { "items": [
        { "content": { "data": { "__typename": "BrowseClientFeature" } } },
      ]}},
      { "data": { "title": { "text": "Your Daily Mixes" } }, "sectionItems": { "items": [
        item("Playlist", "spotify:playlist:mix1", "Daily Mix 1", ""),
      ]}},
    ]}});

    assert_eq!(
      Value::Array(sections(&page)),
      json!([
        { "title": "Uniquely yours", "playlists": [{
          "uri": "spotify:playlist:daylist",
          "name": "daylist",
          "description": "Your day in a playlist.",
          "image_url": "https://i.scdn.co/image/a",
        }]},
        { "title": "Your Daily Mixes", "playlists": [{
          "uri": "spotify:playlist:mix1",
          "name": "Daily Mix 1",
          "description": null,
          "image_url": "https://i.scdn.co/image/a",
        }]},
      ])
    );
  }

  #[test]
  fn descriptions_lose_their_links_and_escapes() {
    assert_eq!(
      plain_text("<a href=spotify:playlist:x>Kygo</a>, <a href=\"y\">Avicii</a> &amp; more"),
      "Kygo, Avicii & more"
    );
    assert_eq!(
      plain_text("Lo-fi &#x2F; chill&#33; R&B &amp;amp; &unknown;"),
      "Lo-fi / chill! R&B &amp; &unknown;"
    );
  }
}
