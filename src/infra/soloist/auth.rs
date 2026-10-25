//! The Soloist API key: `SPOTATUI_SOLOIST_API_KEY`, else `soloist_credentials.yml`.

use std::path::Path;

use serde::Deserialize;

pub const KEY_ENV: &str = "SPOTATUI_SOLOIST_API_KEY";

/// A Soloist API key; `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
  pub fn expose(&self) -> &str {
    &self.0
  }
}

impl std::fmt::Debug for ApiKey {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("ApiKey(***)")
  }
}

#[derive(Deserialize)]
struct Saved {
  api_key: String,
}

fn read_saved(path: &Path) -> Option<String> {
  let text = std::fs::read_to_string(path).ok()?;
  serde_yaml::from_str::<Saved>(&text)
    .ok()
    .map(|saved| saved.api_key)
}

/// The env value first, then the saved one; blank counts as missing.
pub fn key_with(env: Option<String>, saved: Option<String>) -> Option<ApiKey> {
  [env, saved]
    .into_iter()
    .flatten()
    .map(|key| key.trim().to_string())
    .find(|key| !key.is_empty())
    .map(ApiKey)
}

pub fn current_key() -> Option<ApiKey> {
  key_with(
    std::env::var(KEY_ENV).ok(),
    crate::core::paths::soloist_credentials_path().and_then(|path| read_saved(&path)),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn env_key_overrides_the_saved_key() {
    let key = key_with(Some("env".to_string()), Some("saved".to_string()));
    assert_eq!(key.unwrap().expose(), "env");
  }

  #[test]
  fn empty_keys_count_as_missing() {
    assert_eq!(key_with(Some("  ".to_string()), Some(String::new())), None);
    let key = key_with(Some(" ".to_string()), Some(" saved ".to_string()));
    assert_eq!(key.unwrap().expose(), "saved");
  }

  #[test]
  fn debug_redacts_the_key() {
    let key = key_with(Some("secret".to_string()), None).unwrap();
    assert_eq!(format!("{key:?}"), "ApiKey(***)");
  }

  #[test]
  fn the_saved_file_holds_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("soloist_credentials.yml");
    std::fs::write(&path, "api_key: abc\n").unwrap();
    assert_eq!(read_saved(&path).as_deref(), Some("abc"));
  }
}
