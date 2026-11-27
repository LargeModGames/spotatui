//! Bounded references to Subsonic covers handed to the page, never its login URLs.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

const MAX_URL_BYTES: usize = 8 * 1024;
pub(super) const COVER_PREFIX: &str = "subsonic-cover:";

#[derive(Default)]
pub(super) struct CoverLinks(Mutex<Registry>);

#[derive(Default)]
struct Registry {
  next: u64,
  urls: BTreeMap<u64, String>,
  ids: HashMap<String, u64>,
  channels: HashMap<String, ChannelCovers>,
}

struct ChannelCovers {
  revision: u64,
  ids: HashSet<u64>,
}

impl CoverLinks {
  /// Resolve references returned in actions. A page cannot enroll its own private URLs.
  pub(super) fn restore(&self, value: &mut Value) {
    match value {
      Value::Array(items) => {
        for item in items {
          self.restore(item);
        }
      }
      Value::Object(fields) => {
        let subsonic = fields
          .get("uri")
          .and_then(Value::as_str)
          .is_some_and(|uri| uri.starts_with("subsonic:"));
        if let Some(Value::String(image)) = fields.get("image_url") {
          let reference = image.strip_prefix("/cover?").and_then(|query| {
            url::form_urlencoded::parse(query.as_bytes())
              .find(|(key, _)| key == "uri")
              .map(|(_, uri)| uri.into_owned())
          });
          let reference = reference.as_deref().unwrap_or(image);
          let private = url::Url::parse(image).ok().is_some_and(|url| {
            url
              .query_pairs()
              .any(|(key, _)| matches!(&*key, "u" | "t" | "s" | "p"))
          });
          if subsonic || private || reference.starts_with(COVER_PREFIX) {
            let restored = if subsonic {
              self.resolve(reference)
            } else {
              None
            };
            fields.insert("image_url".to_string(), restored.into());
          }
        }
        for field in fields.values_mut() {
          self.restore(field);
        }
      }
      _ => {}
    }
  }

  /// Replace private cover URLs at the serialization boundary, including nested rows.
  pub(super) fn rewrite(&self, value: &mut Value) {
    let kind = value
      .get("kind")
      .and_then(Value::as_str)
      .unwrap_or_default()
      .to_owned();
    let revision = value.get("rev").and_then(Value::as_u64).unwrap_or_default();
    let mut registry = self.0.lock().unwrap_or_else(PoisonError::into_inner);
    // A socket's resync can be encoded after a newer broadcast.
    let stale = registry
      .channels
      .get(&kind)
      .is_some_and(|channel| revision < channel.revision);
    let mut ids = HashSet::new();
    registry.rewrite(value, &mut ids, stale);
    if !stale {
      registry.replace_channel(kind, revision, ids);
    }
  }

  pub(super) fn resolve(&self, reference: &str) -> Option<String> {
    let id = reference.strip_prefix(COVER_PREFIX)?.parse::<u64>().ok()?;
    self
      .0
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .urls
      .get(&id)
      .cloned()
  }
}

impl Registry {
  fn rewrite(&mut self, value: &mut Value, ids: &mut HashSet<u64>, stale: bool) {
    match value {
      Value::Array(items) => {
        for item in items {
          self.rewrite(item, ids, stale);
        }
      }
      Value::Object(fields) => {
        let subsonic = fields
          .get("uri")
          .and_then(Value::as_str)
          .is_some_and(|uri| uri.starts_with("subsonic:"));
        if let Some(Value::String(image)) = fields.get_mut("image_url") {
          let url = url::Url::parse(image).ok();
          let private = url.as_ref().is_some_and(|url| {
            url
              .query_pairs()
              .any(|(key, _)| matches!(&*key, "u" | "t" | "s" | "p"))
          });
          if subsonic || private {
            // Never proxy a caller-selected endpoint, even if a restored row is malformed.
            let reference = url
              .filter(|url| {
                matches!(url.scheme(), "http" | "https")
                  && url.path().ends_with("/rest/getCoverArt.view")
                  && image.len() <= MAX_URL_BYTES
              })
              .and_then(|_| {
                if stale {
                  return self.ids.get(image).copied();
                }
                let id = self.register(image)?;
                ids.insert(id);
                Some(id)
              })
              .map(|id| format!("{COVER_PREFIX}{id}"));
            fields.insert("image_url".to_string(), reference.into());
          }
        }
        for field in fields.values_mut() {
          self.rewrite(field, ids, stale);
        }
      }
      _ => {}
    }
  }

  fn replace_channel(&mut self, kind: String, revision: u64, ids: HashSet<u64>) {
    // Only the replaced channel can release covers. Other channels keep their rows.
    let Some(previous) = self
      .channels
      .insert(kind, ChannelCovers { revision, ids })
      .filter(|channel| !channel.ids.is_empty())
    else {
      return;
    };
    let released = previous.ids.into_iter().filter(|id| {
      !self
        .channels
        .values()
        .any(|channel| channel.ids.contains(id))
    });
    for id in released {
      if let Some(url) = self.urls.remove(&id) {
        self.ids.remove(&url);
      }
    }
    self.ids.shrink_to(self.urls.len());
  }

  fn register(&mut self, url: &str) -> Option<u64> {
    let id = match self.ids.get(url) {
      Some(id) => *id,
      None => {
        self.next = self.next.checked_add(1)?;
        self.urls.insert(self.next, url.to_string());
        self.ids.insert(url.to_string(), self.next);
        self.next
      }
    };
    Some(id)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  const PRIVATE: &str =
    "https://music.example/rest/getCoverArt.view?u=alice&t=secret&s=salt&id=album";

  #[test]
  fn nested_covers_get_references_without_changing_public_covers() {
    let covers = CoverLinks::default();
    let mut value = json!({"rows": [
      {"uri": "subsonic:track:1", "image_url": PRIVATE},
      {"tracks": [{"image_url": PRIVATE}]},
      {"image_url": "https://i.scdn.co/image/album"}
    ]});
    covers.rewrite(&mut value);
    let reference = value["rows"][0]["image_url"].as_str().unwrap();
    assert!(reference.starts_with(COVER_PREFIX));
    assert_eq!(covers.resolve(reference).as_deref(), Some(PRIVATE));
    assert_eq!(value["rows"][1]["tracks"][0]["image_url"], reference);
    assert_eq!(
      value["rows"][2]["image_url"],
      "https://i.scdn.co/image/album"
    );
    assert!(!value.to_string().contains("secret"));
  }

  #[test]
  fn password_auth_and_encoded_auth_keys_never_reach_the_page() {
    let covers = CoverLinks::default();
    for query in ["p=enc:secret", "%75=alice&%74=secret&%73=salt"] {
      let url = format!("https://music.example/rest/getCoverArt.view?{query}&id=a");
      let mut value = json!({"image_url": url});
      covers.rewrite(&mut value);
      let reference = value["image_url"].as_str().unwrap();
      assert_eq!(covers.resolve(reference), Some(url));
      assert!(!value.to_string().contains("secret"));
    }
  }

  #[test]
  fn unknown_references_and_urls_cannot_select_an_upstream() {
    let covers = CoverLinks::default();
    for reference in [
      "subsonic-cover:1",
      "subsonic:track:1",
      PRIVATE,
      "file:///etc/passwd",
    ] {
      assert!(covers.resolve(reference).is_none());
    }
    for image in [
      "https://music.example/rest/deletePlaylist.view?u=alice&p=secret&id=1".to_string(),
      "file:///rest/getCoverArt.view?u=alice&p=secret".to_string(),
      format!("{PRIVATE}&padding={}", "x".repeat(MAX_URL_BYTES)),
    ] {
      let mut value = json!({"uri": "subsonic:track:1", "image_url": image});
      covers.rewrite(&mut value);
      assert!(value["image_url"].is_null());
    }
  }

  #[test]
  fn page_actions_restore_known_covers_and_cannot_enroll_private_urls() {
    let covers = CoverLinks::default();
    let mut row = json!({"uri": "subsonic:track:1", "image_url": PRIVATE});
    covers.rewrite(&mut row);
    let reference = row["image_url"].as_str().unwrap().to_string();
    row["image_url"] = format!("/cover?uri={reference}&t=loopback-token").into();
    covers.restore(&mut row);
    assert_eq!(row["image_url"], PRIVATE);

    for uri in ["subsonic:track:1", "spotify:track:1"] {
      let mut row = json!({"uri": uri, "image_url": PRIVATE});
      covers.restore(&mut row);
      covers.rewrite(&mut row);
      assert!(row["image_url"].is_null());
    }
  }

  #[test]
  fn non_subsonic_rows_drop_registered_cover_references() {
    let covers = CoverLinks::default();
    let mut issued = json!({"uri": "subsonic:track:1", "image_url": PRIVATE});
    covers.rewrite(&mut issued);
    let reference = issued["image_url"].as_str().unwrap();
    assert_eq!(covers.resolve(reference).as_deref(), Some(PRIVATE));

    for row in [
      json!({"uri": "radio:https://radio.example/live"}),
      json!({"uri": "spotify:track:1"}),
      json!({"uri": null}),
      json!({}),
    ] {
      for image in [
        reference.to_string(),
        format!("/cover?uri={reference}&t=session"),
        format!("/cover?uri={}&t=session", reference.replace(':', "%3A")),
      ] {
        let mut row = row.clone();
        row["image_url"] = image.into();
        covers.restore(&mut row);
        assert!(row["image_url"].is_null(), "{row}");
        assert!(!row.to_string().contains(PRIVATE));
      }
    }
  }

  #[test]
  fn small_cover_pushes_keep_the_large_tables_oldest_row_restorable() {
    let covers = CoverLinks::default();
    let tracks: Vec<_> = (0..5000)
      .map(|id| {
        json!({
          "uri": format!("subsonic:track:{id}"),
          "image_url": format!("{PRIVATE}&row={id}"),
        })
      })
      .collect();
    let oldest = tracks[0].clone();
    let table = push(&covers, "track_table", 1, json!({"tracks": tracks}));
    for (kind, key, image) in [
      ("queue", "native", format!("{PRIVATE}&row=4999")),
      ("search", "tracks", format!("{PRIVATE}&new=1")),
    ] {
      push(&covers, kind, 1, json!({(key): [{"image_url": image}]}));
      let mut queued = table["payload"]["tracks"][0].clone();
      covers.restore(&mut queued);
      assert_eq!(queued, oldest, "after the one-row {kind} push");
    }
    for row in table["payload"]["tracks"].as_array().unwrap() {
      assert!(covers.resolve(row["image_url"].as_str().unwrap()).is_some());
    }
  }

  #[test]
  fn replacing_a_list_releases_only_covers_unused_by_other_channels() {
    for replacement in [
      json!([]),
      json!([{"image_url": "https://i.scdn.co/image/a"}]),
    ] {
      let covers = CoverLinks::default();
      let shared = format!("{PRIVATE}&shared=1");
      let table = push(
        &covers,
        "track_table",
        1,
        json!({"tracks": [
          {"image_url": PRIVATE},
          {"image_url": shared},
        ]}),
      );
      let queue = push(
        &covers,
        "queue",
        1,
        json!({"native": [{"image_url": shared}]}),
      );
      let expired_ref = table["payload"]["tracks"][0]["image_url"].as_str().unwrap();
      let shared_ref = table["payload"]["tracks"][1]["image_url"].as_str().unwrap();
      assert_eq!(queue["payload"]["native"][0]["image_url"], shared_ref);

      push(&covers, "source", 2, json!({"active": "Spotify"}));
      assert_eq!(covers.resolve(expired_ref).as_deref(), Some(PRIVATE));
      push(&covers, "track_table", 2, json!({"tracks": replacement}));
      assert!(covers.resolve(expired_ref).is_none());
      assert_eq!(covers.resolve(shared_ref).as_deref(), Some(shared.as_str()));
      push(&covers, "queue", 2, json!({"native": []}));
      {
        let registry = covers.0.lock().unwrap();
        assert!(registry.urls.is_empty());
        assert!(registry.ids.is_empty());
      }

      let table = push(
        &covers,
        "track_table",
        3,
        json!({"tracks": [{"image_url": PRIVATE}]}),
      );
      assert_ne!(table["payload"]["tracks"][0]["image_url"], expired_ref);
      assert!(covers.resolve(expired_ref).is_none());
    }
  }

  #[test]
  fn stale_list_revisions_neither_register_nor_release_covers() {
    let covers = CoverLinks::default();
    let table = push(
      &covers,
      "track_table",
      2,
      json!({"tracks": [{"image_url": PRIVATE}]}),
    );
    let current = table["payload"]["tracks"][0]["image_url"].as_str().unwrap();
    let older = push(
      &covers,
      "track_table",
      1,
      json!({"tracks": [
        {"image_url": PRIVATE},
        {"image_url": format!("{PRIVATE}&old=1")},
      ]}),
    );
    assert_eq!(older["payload"]["tracks"][0]["image_url"], current);
    assert!(older["payload"]["tracks"][1]["image_url"].is_null());
    push(&covers, "track_table", 1, json!({"tracks": []}));
    assert_eq!(covers.resolve(current).as_deref(), Some(PRIVATE));
    push(&covers, "track_table", 3, json!({"tracks": []}));
    assert!(covers.resolve(current).is_none());

    push(&covers, "search", 2, json!({"tracks": []}));
    let older = push(
      &covers,
      "search",
      1,
      json!({"tracks": [{"image_url": PRIVATE}]}),
    );
    assert!(older["payload"]["tracks"][0]["image_url"].is_null());
    let registry = covers.0.lock().unwrap();
    assert!(registry.urls.is_empty());
    assert!(registry.ids.is_empty());
  }

  fn push(covers: &CoverLinks, kind: &str, rev: u64, payload: Value) -> Value {
    let mut message = json!({"kind": kind, "rev": rev, "payload": payload});
    covers.rewrite(&mut message);
    message
  }
}
