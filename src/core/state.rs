//! Persisted runtime state and app-managed mutable data.
//!
//! This is the machine-owned counterpart to `config.yml`. It holds values that
//! change as the app runs, or data the app manages directly, so ordinary config
//! edits do not churn every time volume, source, pane sizes, announcements, or
//! radio stations change.

use crate::core::source::Source;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "state.yml";

/// One saved internet-radio station: a display name plus the direct stream URL.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RadioStationConfig {
  pub name: String,
  pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioStationAddOutcome {
  Added,
  AlreadyExists,
}

/// Live runtime/app-managed state.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeState {
  pub volume_percent: u8,
  pub shuffle_enabled: bool,
  pub active_source: Source,
  pub seen_announcement_ids: Vec<String>,
  pub dismissed_announcements: Vec<String>,
  pub sidebar_width_percent: u8,
  pub playbar_height_rows: u16,
  pub library_height_percent: u8,
  pub radio_stations: Vec<RadioStationConfig>,
}

impl Default for RuntimeState {
  fn default() -> Self {
    Self {
      volume_percent: 100,
      shuffle_enabled: false,
      active_source: Source::default(),
      seen_announcement_ids: Vec::new(),
      dismissed_announcements: Vec::new(),
      sidebar_width_percent: 20,
      playbar_height_rows: 6,
      library_height_percent: 30,
      radio_stations: Vec::new(),
    }
  }
}

impl RuntimeState {
  pub fn apply_persisted(&mut self, state: &PersistedRuntimeState) {
    if let Some(volume_percent) = state.volume_percent {
      self.volume_percent = volume_percent.min(100);
    }
    if let Some(shuffle_enabled) = state.shuffle_enabled {
      self.shuffle_enabled = shuffle_enabled;
    }
    if let Some(active_source) = state.active_source {
      self.active_source = active_source;
    }
    if let Some(seen_announcement_ids) = &state.seen_announcement_ids {
      self.seen_announcement_ids = sanitized_ids(seen_announcement_ids);
    }
    if let Some(dismissed_announcements) = &state.dismissed_announcements {
      self.dismissed_announcements = sanitized_ids(dismissed_announcements);
    }
    if let Some(sidebar_width_percent) = state.sidebar_width_percent {
      self.sidebar_width_percent = sidebar_width_percent.min(100);
    }
    if let Some(playbar_height_rows) = state.playbar_height_rows {
      self.playbar_height_rows = playbar_height_rows;
    }
    if let Some(library_height_percent) = state.library_height_percent {
      self.library_height_percent = library_height_percent.min(100);
    }
    if let Some(radio_stations) = &state.radio_stations {
      self.radio_stations = sanitized_radio_stations(radio_stations);
    }
  }

  pub fn to_persisted(&self) -> PersistedRuntimeState {
    PersistedRuntimeState {
      volume_percent: Some(self.volume_percent.min(100)),
      shuffle_enabled: Some(self.shuffle_enabled),
      active_source: Some(self.active_source),
      seen_announcement_ids: Some(sanitized_ids(&self.seen_announcement_ids)),
      dismissed_announcements: Some(sanitized_ids(&self.dismissed_announcements)),
      sidebar_width_percent: Some(self.sidebar_width_percent.min(100)),
      playbar_height_rows: Some(self.playbar_height_rows),
      library_height_percent: Some(self.library_height_percent.min(100)),
      radio_stations: Some(sanitized_radio_stations(&self.radio_stations)),
    }
  }

  pub fn add_radio_station(
    &mut self,
    name: impl AsRef<str>,
    url: impl AsRef<str>,
  ) -> Result<RadioStationAddOutcome> {
    let name = name.as_ref().trim();
    let url = url.as_ref().trim();

    if name.is_empty() {
      return Err(anyhow!("Radio station name is empty"));
    }
    if url.is_empty() {
      return Err(anyhow!("Radio station URL is empty"));
    }

    if self
      .radio_stations
      .iter()
      .any(|station| station.url.trim() == url)
    {
      return Ok(RadioStationAddOutcome::AlreadyExists);
    }

    self.radio_stations.push(RadioStationConfig {
      name: name.to_string(),
      url: url.to_string(),
    });

    Ok(RadioStationAddOutcome::Added)
  }

  pub fn remove_radio_station_by_url(
    &mut self,
    url: impl AsRef<str>,
  ) -> Result<Option<RadioStationConfig>> {
    let url = url.as_ref().trim();
    if url.is_empty() {
      return Err(anyhow!("Radio station URL is empty"));
    }

    let Some(index) = self
      .radio_stations
      .iter()
      .position(|station| station.url.trim() == url)
    else {
      return Ok(None);
    };

    Ok(Some(self.radio_stations.remove(index)))
  }

  pub fn mark_announcement_seen(&mut self, announcement_id: impl Into<String>) {
    let id = announcement_id.into();
    if id.is_empty() {
      return;
    }

    if !self.seen_announcement_ids.iter().any(|seen| seen == &id) {
      self.seen_announcement_ids.push(id);
    }
  }
}

/// Optional state fields exactly as stored in `state.yml`.
///
/// Fields are optional so sparse or older `state.yml` files can overlay only the
/// values they contain onto runtime defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedRuntimeState {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volume_percent: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub shuffle_enabled: Option<bool>,
  #[serde(
    skip_serializing_if = "Option::is_none",
    serialize_with = "source_config::serialize_option",
    deserialize_with = "source_config::deserialize_option"
  )]
  pub active_source: Option<Source>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub seen_announcement_ids: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dismissed_announcements: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sidebar_width_percent: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub playbar_height_rows: Option<u16>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub library_height_percent: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub radio_stations: Option<Vec<RadioStationConfig>>,
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

/// Save state atomically and privately.
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

/// Ensure a state directory exists and is owner-only where supported.
pub(crate) fn ensure_private_state_dir(dir: &Path) -> Result<()> {
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

pub(crate) fn sanitized_radio_stations(stations: &[RadioStationConfig]) -> Vec<RadioStationConfig> {
  let mut seen_urls = std::collections::HashSet::new();

  stations
    .iter()
    .filter_map(|station| {
      let name = station.name.trim();
      let url = station.url.trim();
      if name.is_empty() || url.is_empty() {
        return None;
      }

      let url = url.to_string();
      if !seen_urls.insert(url.clone()) {
        return None;
      }

      Some(RadioStationConfig {
        name: name.to_string(),
        url,
      })
    })
    .collect()
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
  fn runtime_state_applies_and_sanitizes_persisted_values() {
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
          name: "Duplicate".to_string(),
          url: "https://example.test".to_string(),
        },
        RadioStationConfig {
          name: "Broken".to_string(),
          url: " ".to_string(),
        },
      ]),
      ..Default::default()
    };

    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&state);

    assert_eq!(runtime.volume_percent, 100);
    assert_eq!(runtime.active_source, Source::Subsonic);
    assert_eq!(runtime.seen_announcement_ids, vec!["seen"]);
    assert_eq!(runtime.sidebar_width_percent, 100);
    assert_eq!(runtime.library_height_percent, 100);
    assert_eq!(
      runtime.radio_stations,
      vec![RadioStationConfig {
        name: "Good".to_string(),
        url: "https://example.test".to_string(),
      }]
    );
  }

  #[test]
  fn radio_station_helpers_trim_dedupe_and_remove() {
    let mut runtime = RuntimeState::default();

    assert_eq!(
      runtime
        .add_radio_station(" Groove ", " https://example.test/stream ")
        .unwrap(),
      RadioStationAddOutcome::Added
    );
    assert_eq!(runtime.radio_stations[0].name, "Groove");
    assert_eq!(runtime.radio_stations[0].url, "https://example.test/stream");
    assert_eq!(
      runtime
        .add_radio_station("Duplicate", "https://example.test/stream")
        .unwrap(),
      RadioStationAddOutcome::AlreadyExists
    );

    let removed = runtime
      .remove_radio_station_by_url(" https://example.test/stream ")
      .unwrap();
    assert_eq!(
      removed.map(|station| station.name),
      Some("Groove".to_string())
    );
    assert!(runtime.radio_stations.is_empty());
  }
}
