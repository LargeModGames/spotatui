//! Persisted runtime state and app-managed mutable data.
//!
//! This is the machine-owned counterpart to `config.yml`. It holds values that
//! change as the app runs, or data the app manages directly, so ordinary config
//! edits do not churn every time volume, source, layout, announcements, or radio
//! stations change.

use crate::core::{
  source::Source,
  user_config::{RadioStationConfig, UserConfig},
};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "state.yml";

/// Optional state fields exactly as stored in `state.yml`.
///
/// Fields are optional so legacy migration can overlay values from `config.yml`
/// only when `state.yml` has not already claimed ownership of that value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedRuntimeState {
  pub volume_percent: Option<u8>,
  pub shuffle_enabled: Option<bool>,
  #[serde(
    skip_serializing_if = "Option::is_none",
    serialize_with = "source_config::serialize_option",
    deserialize_with = "source_config::deserialize_option"
  )]
  pub active_source: Option<Source>,
  pub seen_announcement_ids: Option<Vec<String>>,
  pub dismissed_announcements: Option<Vec<String>>,
  pub sidebar_width_percent: Option<u8>,
  pub playbar_height_rows: Option<u16>,
  pub library_height_percent: Option<u8>,
  pub radio_stations: Option<Vec<RadioStationConfig>>,
  pub sync_token: Option<String>,
}

impl PersistedRuntimeState {
  /// Build a complete runtime-state snapshot from the in-memory config fields
  /// that are moving out of `config.yml`.
  pub fn from_user_config(config: &UserConfig) -> Self {
    let behavior = &config.behavior;
    Self {
      volume_percent: Some(behavior.volume_percent.min(100)),
      shuffle_enabled: Some(behavior.shuffle_enabled),
      active_source: Some(behavior.active_source),
      seen_announcement_ids: Some(sanitized_ids(&behavior.seen_announcement_ids)),
      dismissed_announcements: Some(sanitized_ids(&behavior.dismissed_announcements)),
      sidebar_width_percent: Some(behavior.sidebar_width_percent.min(100)),
      playbar_height_rows: Some(behavior.playbar_height_rows),
      library_height_percent: Some(behavior.library_height_percent.min(100)),
      radio_stations: Some(sanitized_radio_stations(&behavior.radio_stations)),
      sync_token: trim_to_optional_string(behavior.sync_token.clone()),
    }
  }

  /// Overlay this state onto config defaults after `config.yml` has loaded.
  ///
  /// This method intentionally updates only the fields owned by `state.yml`.
  pub fn apply_to_user_config(&self, config: &mut UserConfig) {
    if let Some(volume_percent) = self.volume_percent {
      config.behavior.volume_percent = volume_percent.min(100);
    }
    if let Some(shuffle_enabled) = self.shuffle_enabled {
      config.behavior.shuffle_enabled = shuffle_enabled;
    }
    if let Some(active_source) = self.active_source {
      config.behavior.active_source = active_source;
    }
    if let Some(seen_announcement_ids) = &self.seen_announcement_ids {
      config.behavior.seen_announcement_ids = sanitized_ids(seen_announcement_ids);
    }
    if let Some(dismissed_announcements) = &self.dismissed_announcements {
      config.behavior.dismissed_announcements = sanitized_ids(dismissed_announcements);
    }
    if let Some(sidebar_width_percent) = self.sidebar_width_percent {
      config.behavior.sidebar_width_percent = sidebar_width_percent.min(100);
    }
    if let Some(playbar_height_rows) = self.playbar_height_rows {
      config.behavior.playbar_height_rows = playbar_height_rows;
    }
    if let Some(library_height_percent) = self.library_height_percent {
      config.behavior.library_height_percent = library_height_percent.min(100);
    }
    if let Some(radio_stations) = &self.radio_stations {
      config.behavior.radio_stations = sanitized_radio_stations(radio_stations);
    }
    if self.sync_token.is_some() {
      config.behavior.sync_token = trim_to_optional_string(self.sync_token.clone());
    }
  }
}

/// Location of the app runtime-state file: `<state dir>/state.yml`.
pub fn default_state_path() -> Result<PathBuf> {
  crate::core::paths::app_state_dir()
    .map(|dir| dir.join(FILE_NAME))
    .ok_or_else(|| anyhow!("cannot resolve the spotatui state directory"))
}

/// Load state. A missing file means empty state; malformed state is returned as
/// an error so callers can log and continue with config/default values.
pub fn load(path: &Path) -> Result<PersistedRuntimeState> {
  match std::fs::read_to_string(path) {
    Ok(contents) => {
      if contents.trim().is_empty() {
        Ok(PersistedRuntimeState::default())
      } else {
        serde_yaml::from_str(&contents)
          .with_context(|| format!("malformed runtime state file: {}", path.display()))
      }
    }
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PersistedRuntimeState::default()),
    Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
  }
}

/// Save state atomically and privately. `sync_token` can be present, so this
/// file uses the same private-file helper as the token/config paths.
pub fn save(path: &Path, state: &PersistedRuntimeState) -> Result<()> {
  let yaml = serde_yaml::to_string(state).context("serializing runtime state")?;
  if let Some(dir) = path.parent() {
    ensure_private_state_dir(dir)?;
  }
  let tmp = path.with_extension("yml.tmp");
  crate::core::auth::write_private_file(&tmp, yaml.as_bytes())
    .with_context(|| format!("writing {}", tmp.display()))?;
  std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
  Ok(())
}

fn ensure_private_state_dir(dir: &Path) -> Result<()> {
  std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
      .with_context(|| format!("setting private permissions on {}", dir.display()))?;
  }

  Ok(())
}

fn sanitized_ids(ids: &[String]) -> Vec<String> {
  ids
    .iter()
    .map(|id| id.trim().to_string())
    .filter(|id| !id.is_empty())
    .collect()
}

fn sanitized_radio_stations(stations: &[RadioStationConfig]) -> Vec<RadioStationConfig> {
  stations
    .iter()
    .filter_map(|station| {
      let name = station.name.trim();
      let url = station.url.trim();
      if name.is_empty() || url.is_empty() {
        None
      } else {
        Some(RadioStationConfig {
          name: name.to_string(),
          url: url.to_string(),
        })
      }
    })
    .collect()
}

fn trim_to_optional_string(value: Option<String>) -> Option<String> {
  value.and_then(|value| {
    let trimmed = value.trim();
    if trimmed.is_empty() {
      None
    } else {
      Some(trimmed.to_string())
    }
  })
}

mod source_config {
  use super::Source;
  use serde::{Deserialize, Deserializer, Serializer};

  pub(super) fn serialize_option<S>(
    source: &Option<Source>,
    serializer: S,
  ) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match source {
      Some(source) => serializer.serialize_some(source.to_config_str()),
      None => serializer.serialize_none(),
    }
  }

  pub(super) fn deserialize_option<'de, D>(deserializer: D) -> Result<Option<Source>, D::Error>
  where
    D: Deserializer<'de>,
  {
    Option::<String>::deserialize(deserializer)
      .map(|source| source.map(|source| Source::from_config_str(&source)))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn missing_file_loads_as_empty_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");

    assert_eq!(load(&path).unwrap(), PersistedRuntimeState::default());
  }

  #[test]
  fn state_round_trips_all_runtime_attributes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("spotatui").join("state.yml");
    let state = PersistedRuntimeState {
      volume_percent: Some(42),
      shuffle_enabled: Some(true),
      active_source: Some(Source::Radio),
      seen_announcement_ids: Some(vec!["a".to_string(), "b".to_string()]),
      dismissed_announcements: Some(vec!["old".to_string()]),
      sidebar_width_percent: Some(25),
      playbar_height_rows: Some(7),
      library_height_percent: Some(35),
      radio_stations: Some(vec![RadioStationConfig {
        name: "Station".to_string(),
        url: "https://example.test/stream".to_string(),
      }]),
      sync_token: Some("secret".to_string()),
    };

    save(&path, &state).unwrap();

    let yaml = std::fs::read_to_string(&path).unwrap();
    assert!(yaml.contains("active_source: Radio"));
    assert_eq!(load(&path).unwrap(), state);

    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let dir_mode = std::fs::metadata(path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
      let file_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
      assert_eq!(dir_mode, 0o700);
      assert_eq!(file_mode, 0o600);
    }
  }

  #[test]
  fn applying_state_sanitizes_values() {
    let mut config = UserConfig::new();
    let state = PersistedRuntimeState {
      volume_percent: Some(150),
      active_source: Some(Source::Subsonic),
      seen_announcement_ids: Some(vec![" seen ".to_string(), " ".to_string()]),
      sidebar_width_percent: Some(120),
      library_height_percent: Some(101),
      radio_stations: Some(vec![
        RadioStationConfig {
          name: " Good ".to_string(),
          url: " https://example.test ".to_string(),
        },
        RadioStationConfig {
          name: "Broken".to_string(),
          url: " ".to_string(),
        },
      ]),
      sync_token: Some(" token ".to_string()),
      ..Default::default()
    };

    state.apply_to_user_config(&mut config);

    assert_eq!(config.behavior.volume_percent, 100);
    assert_eq!(config.behavior.active_source, Source::Subsonic);
    assert_eq!(config.behavior.seen_announcement_ids, vec!["seen"]);
    assert_eq!(config.behavior.sidebar_width_percent, 100);
    assert_eq!(config.behavior.library_height_percent, 100);
    assert_eq!(
      config.behavior.radio_stations,
      vec![RadioStationConfig {
        name: "Good".to_string(),
        url: "https://example.test".to_string(),
      }]
    );
    assert_eq!(config.behavior.sync_token, Some("token".to_string()));
  }
}
