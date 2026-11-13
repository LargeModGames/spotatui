//! Bounded references to Subsonic covers handed to the page, never its login URLs.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, PoisonError};

const MAX_COVERS: usize = 4096;
const MAX_URL_BYTES: usize = 8 * 1024;
pub(super) const COVER_PREFIX: &str = "subsonic-cover:";

#[derive(Default)]
pub(super) struct CoverLinks(Mutex<Registry>);

#[derive(Default)]
struct Registry {
  next: u64,
  urls: BTreeMap<u64, String>,
  ids: HashMap<String, u64>,
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
    match value {
      Value::Array(items) => {
        for item in items {
          self.rewrite(item);
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
                self
                  .0
                  .lock()
                  .unwrap_or_else(PoisonError::into_inner)
                  .register(image)
              });
            fields.insert("image_url".to_string(), reference.into());
          }
        }
        for field in fields.values_mut() {
          self.rewrite(field);
        }
      }
      _ => {}
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
  fn register(&mut self, url: &str) -> Option<String> {
    let id = match self.ids.get(url) {
      Some(id) => *id,
      None => {
        self.next = self.next.checked_add(1)?;
        if self.urls.len() == MAX_COVERS {
          let (_, old) = self.urls.pop_first()?;
          self.ids.remove(&old);
        }
        self.urls.insert(self.next, url.to_string());
        self.ids.insert(url.to_string(), self.next);
        self.next
      }
    };
    Some(format!("{COVER_PREFIX}{id}"))
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
  fn the_registry_evicts_old_covers_without_reusing_their_references() {
    let covers = CoverLinks::default();
    let mut first = json!({"image_url": PRIVATE});
    covers.rewrite(&mut first);
    let first = first["image_url"].as_str().unwrap().to_string();
    let mut last = Value::Null;
    for id in 0..MAX_COVERS {
      last = json!({"image_url": format!("{PRIVATE}{id}")});
      covers.rewrite(&mut last);
    }
    assert!(covers.resolve(&first).is_none());
    assert!(covers
      .resolve(last["image_url"].as_str().unwrap())
      .is_some());
    let registry = covers.0.lock().unwrap();
    assert_eq!(registry.urls.len(), MAX_COVERS);
    assert_eq!(registry.ids.len(), MAX_COVERS);
  }
}
