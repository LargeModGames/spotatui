//! Short-lived upgrade shims for moving old on-disk shapes into current state.
//!
//! Keep each migration isolated here so it can be deleted cleanly once the
//! minimum supported upgrade path no longer includes the legacy release.

use crate::core::source::Source;
use crate::core::state::{sanitized_radio_stations, PersistedRuntimeState, RuntimeState};
use crate::core::user_config::BehaviorConfig;
use anyhow::{anyhow, Context, Result};
use serde_yaml::{Mapping, Value};
use std::{
  fs,
  path::{Path, PathBuf},
};

const LEGACY_RUNTIME_STATE_BEHAVIOR_KEYS: [&str; 4] = [
  "active_source",
  "shuffle_enabled",
  "seen_announcement_ids",
  "dismissed_announcements",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LegacyConfigCleanupTargets {
  runtime_state_keys: Vec<&'static str>,
  radio_stations: bool,
}

impl LegacyConfigCleanupTargets {
  pub(crate) fn is_empty(&self) -> bool {
    self.runtime_state_keys.is_empty() && !self.radio_stations
  }

  pub(crate) fn removes_radio_stations(&self) -> bool {
    self.radio_stations
  }
}

/// Resolve a file that now belongs under the XDG state directory, migrating the
/// legacy file from the config directory when the new location is still empty.
///
/// This intentionally never overwrites an existing state file: once the new
/// path has data, merging the old file is file-format-specific and unsafe for a
/// generic shim. Remove this after the config-to-state upgrade window closes.
pub(crate) fn state_file_path_with_legacy_config_rename(
  relative_path: impl AsRef<Path>,
) -> Result<PathBuf> {
  let relative_path = relative_path.as_ref();
  if relative_path.is_absolute() {
    return Err(anyhow!(
      "state migration path must be relative: {}",
      relative_path.display()
    ));
  }

  let state_dir = crate::core::paths::app_state_dir()
    .ok_or_else(|| anyhow!("cannot resolve the spotatui state directory"))?;
  let state_path = state_dir.join(relative_path);

  if let Some(config_dir) = crate::core::paths::app_config_dir() {
    let legacy_path = config_dir.join(relative_path);
    rename_legacy_file_if_unclaimed(&legacy_path, &state_path)?;
  }

  Ok(state_path)
}

pub(crate) fn rename_legacy_file_if_unclaimed(
  legacy_path: &Path,
  state_path: &Path,
) -> Result<bool> {
  if path_exists(state_path)? || !path_exists(legacy_path)? {
    return Ok(false);
  }

  if let Some(parent) = state_path.parent() {
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
  }

  match fs::hard_link(legacy_path, state_path) {
    Ok(()) => {
      remove_legacy_file_after_migration(legacy_path, state_path);
      log::info!(
        "migrated legacy app data from {} to {}",
        legacy_path.display(),
        state_path.display()
      );
      Ok(true)
    }
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(link_error) => {
      if !copy_legacy_file_if_unclaimed(legacy_path, state_path, &link_error)? {
        return Ok(false);
      }
      remove_legacy_file_after_migration(legacy_path, state_path);
      log::info!(
        "migrated legacy app data from {} to {}",
        legacy_path.display(),
        state_path.display()
      );
      Ok(true)
    }
  }
}

fn copy_legacy_file_if_unclaimed(
  legacy_path: &Path,
  state_path: &Path,
  link_error: &std::io::Error,
) -> Result<bool> {
  let mut source = match fs::File::open(legacy_path) {
    Ok(file) => file,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| format!("opening {}", legacy_path.display()));
    }
  };
  let mut target = match fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(state_path)
  {
    Ok(file) => file,
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| {
        format!(
          "linking {} to {} failed ({link_error}); creating copy target also failed",
          legacy_path.display(),
          state_path.display()
        )
      });
    }
  };

  if let Err(error) = std::io::copy(&mut source, &mut target) {
    let _ = fs::remove_file(state_path);
    return Err(error).with_context(|| {
      format!(
        "linking {} to {} failed ({link_error}); copying fallback also failed",
        legacy_path.display(),
        state_path.display()
      )
    });
  }

  Ok(true)
}

fn remove_legacy_file_after_migration(legacy_path: &Path, state_path: &Path) {
  match fs::remove_file(legacy_path) {
    Ok(()) => {}
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
    Err(error) => log::warn!(
      "migrated {} to {}, but failed to remove the legacy file: {}",
      legacy_path.display(),
      state_path.display(),
      error
    ),
  }
}

/// Move legacy in-app radio favorites from `config.yml` ownership into
/// `state.yml` ownership.
///
/// Remove this migration after one release cycle where users could have started
/// the app and converted pre-existing `behavior.radio_stations`.
pub(crate) fn apply_legacy_config_radio_station_migration(
  runtime_state: &mut RuntimeState,
  persisted_state: &PersistedRuntimeState,
  behavior: &BehaviorConfig,
) -> PersistedRuntimeState {
  if persisted_state.radio_stations.is_some() || behavior.radio_stations.is_empty() {
    return PersistedRuntimeState::default();
  }

  runtime_state.radio_stations = behavior.radio_stations.clone();
  PersistedRuntimeState {
    radio_stations: Some(runtime_state.radio_stations.clone()),
    ..Default::default()
  }
}

/// Seed runtime-state fields that used to live under `behavior` in `config.yml`.
///
/// This reads the raw YAML rather than `BehaviorConfigString` because those
/// fields were removed from the typed config. Remove this with the radio shim
/// after the legacy upgrade path no longer needs to be supported.
pub(crate) fn apply_legacy_config_runtime_state_migration(
  path: &Path,
  runtime_state: &mut RuntimeState,
  persisted_state: &PersistedRuntimeState,
) -> Result<PersistedRuntimeState> {
  let Some(config) = read_config_value(path)? else {
    return Ok(PersistedRuntimeState::default());
  };
  let Some(behavior) = behavior_mapping(&config) else {
    return Ok(PersistedRuntimeState::default());
  };

  let mut patch = PersistedRuntimeState::default();
  if persisted_state.active_source.is_none() {
    if let Some(source) = behavior
      .get(yaml_key("active_source"))
      .and_then(source_from_value)
    {
      runtime_state.active_source = source;
      patch.active_source = Some(source);
    }
  }
  if persisted_state.shuffle_enabled.is_none() {
    if let Some(shuffle_enabled) = behavior
      .get(yaml_key("shuffle_enabled"))
      .and_then(Value::as_bool)
    {
      runtime_state.shuffle_enabled = shuffle_enabled;
      patch.shuffle_enabled = Some(shuffle_enabled);
    }
  }
  if persisted_state.seen_announcement_ids.is_none() {
    if let Some(ids) = behavior
      .get(yaml_key("seen_announcement_ids"))
      .and_then(string_sequence)
    {
      runtime_state.seen_announcement_ids = ids.clone();
      patch.seen_announcement_ids = Some(ids);
    }
  }
  if persisted_state.dismissed_announcements.is_none() {
    if let Some(ids) = behavior
      .get(yaml_key("dismissed_announcements"))
      .and_then(string_sequence)
    {
      runtime_state.dismissed_announcements = ids.clone();
      patch.dismissed_announcements = Some(ids);
    }
  }

  Ok(patch)
}

pub(crate) fn legacy_config_cleanup_targets(
  path: &Path,
  state: &PersistedRuntimeState,
) -> Result<LegacyConfigCleanupTargets> {
  let Some(config) = read_config_value(path)? else {
    return Ok(LegacyConfigCleanupTargets::default());
  };
  let Some(behavior) = behavior_mapping(&config) else {
    return Ok(LegacyConfigCleanupTargets::default());
  };

  let mut targets = LegacyConfigCleanupTargets::default();
  for key in LEGACY_RUNTIME_STATE_BEHAVIOR_KEYS {
    if behavior.get(yaml_key(key)).is_some() && state_owns_legacy_runtime_key(key, state) {
      targets.runtime_state_keys.push(key);
    }
  }

  if let Some(config_stations) = behavior
    .get(yaml_key("radio_stations"))
    .and_then(radio_stations_from_value)
  {
    if state_owns_radio_stations(
      &config_stations,
      state.radio_stations.as_deref().unwrap_or(&[]),
    ) {
      targets.radio_stations = true;
    }
  }

  Ok(targets)
}

pub(crate) fn remove_legacy_config_fields(
  path: &Path,
  targets: &LegacyConfigCleanupTargets,
) -> Result<bool> {
  let mut keys = targets.runtime_state_keys.clone();
  if targets.radio_stations {
    keys.push("radio_stations");
  }
  remove_behavior_keys_from_config(path, &keys)
}

fn remove_behavior_keys_from_config(path: &Path, keys: &[&str]) -> Result<bool> {
  if keys.is_empty() {
    return Ok(false);
  }

  let Some(mut config) = read_config_value(path)? else {
    return Ok(false);
  };
  let Some(behavior_map) = behavior_mapping_mut(&mut config) else {
    return Ok(false);
  };

  let mut removed = false;
  for key in keys {
    removed |= behavior_map.remove(yaml_key(key)).is_some();
  }
  if !removed {
    return Ok(false);
  }

  let updated_config = serde_yaml::to_string(&config)?;
  let tmp_path = path.with_extension("yml.tmp");
  crate::core::auth::write_private_file(&tmp_path, updated_config.as_bytes())?;
  fs::rename(&tmp_path, path)?;

  Ok(true)
}

fn path_exists(path: &Path) -> Result<bool> {
  match fs::symlink_metadata(path) {
    Ok(_) => Ok(true),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(error) => Err(error).with_context(|| format!("checking {}", path.display())),
  }
}

fn read_config_value(path: &Path) -> Result<Option<Value>> {
  let config_yml = match fs::read_to_string(path) {
    Ok(config_yml) => config_yml,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(e) => return Err(e.into()),
  };
  if config_yml.trim().is_empty() {
    return Ok(None);
  }

  Ok(Some(serde_yaml::from_str(&config_yml)?))
}

fn behavior_mapping(config: &Value) -> Option<&Mapping> {
  config
    .as_mapping()
    .and_then(|map| map.get(yaml_key("behavior")))
    .and_then(Value::as_mapping)
}

fn behavior_mapping_mut(config: &mut Value) -> Option<&mut Mapping> {
  config
    .as_mapping_mut()
    .and_then(|map| map.get_mut(yaml_key("behavior")))
    .and_then(Value::as_mapping_mut)
}

fn yaml_key(key: &str) -> Value {
  Value::String(key.to_string())
}

fn source_from_value(value: &Value) -> Option<Source> {
  let source = value.as_str()?.trim();
  if source.is_empty() {
    return None;
  }

  Some(Source::from_config_str(source))
}

fn radio_stations_from_value(value: &Value) -> Option<Vec<crate::core::state::RadioStationConfig>> {
  serde_yaml::from_value(value.clone()).ok()
}

fn state_owns_legacy_runtime_key(key: &str, state: &PersistedRuntimeState) -> bool {
  match key {
    "active_source" => state.active_source.is_some(),
    "shuffle_enabled" => state.shuffle_enabled.is_some(),
    "seen_announcement_ids" => state.seen_announcement_ids.is_some(),
    "dismissed_announcements" => state.dismissed_announcements.is_some(),
    _ => false,
  }
}

fn state_owns_radio_stations(
  config_stations: &[crate::core::state::RadioStationConfig],
  state_stations: &[crate::core::state::RadioStationConfig],
) -> bool {
  let config_stations = sanitized_radio_stations(config_stations);
  if config_stations.is_empty() {
    return false;
  }

  let state_stations = sanitized_radio_stations(state_stations);
  config_stations.iter().all(|config_station| {
    state_stations.iter().any(|state_station| {
      state_station.name == config_station.name && state_station.url == config_station.url
    })
  })
}

fn string_sequence(value: &Value) -> Option<Vec<String>> {
  value.as_sequence().map(|items| {
    items
      .iter()
      .filter_map(Value::as_str)
      .map(|id| id.trim().to_string())
      .filter(|id| !id.is_empty())
      .collect()
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::source::Source;
  use crate::core::state::RadioStationConfig;
  use crate::core::user_config::UserConfig;

  #[test]
  fn legacy_file_rename_moves_file_when_state_path_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir
      .path()
      .join("config")
      .join("history")
      .join("listens.jsonl");
    let state_path = dir
      .path()
      .join("state")
      .join("history")
      .join("listens.jsonl");
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::write(&legacy_path, "legacy listens").unwrap();

    assert!(rename_legacy_file_if_unclaimed(&legacy_path, &state_path).unwrap());
    assert!(!legacy_path.exists());
    assert_eq!(
      std::fs::read_to_string(&state_path).unwrap(),
      "legacy listens"
    );
  }

  #[test]
  fn legacy_file_rename_ignores_missing_legacy_file() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("config").join("last_session.yml");
    let state_path = dir.path().join("state").join("last_session.yml");

    assert!(!rename_legacy_file_if_unclaimed(&legacy_path, &state_path).unwrap());
    assert!(!state_path.exists());
  }

  #[test]
  fn legacy_file_rename_does_not_overwrite_existing_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir
      .path()
      .join("config")
      .join("history")
      .join("last_synced_at.txt");
    let state_path = dir
      .path()
      .join("state")
      .join("history")
      .join("last_synced_at.txt");
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    std::fs::write(&legacy_path, "legacy cursor").unwrap();
    std::fs::write(&state_path, "offset:42").unwrap();

    assert!(!rename_legacy_file_if_unclaimed(&legacy_path, &state_path).unwrap());
    assert_eq!(
      std::fs::read_to_string(&legacy_path).unwrap(),
      "legacy cursor"
    );
    assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "offset:42");
  }

  #[test]
  fn legacy_config_radio_stations_seed_missing_persisted_runtime_field() {
    let mut config = UserConfig::new();
    config.behavior.radio_stations = vec![RadioStationConfig {
      name: "Groove Salad".to_string(),
      url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
    }];
    let persisted = PersistedRuntimeState {
      volume_percent: Some(42),
      ..Default::default()
    };
    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&persisted);

    let patch =
      apply_legacy_config_radio_station_migration(&mut runtime, &persisted, &config.behavior);

    assert_eq!(
      runtime.radio_stations,
      vec![RadioStationConfig {
        name: "Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      }]
    );
    assert_eq!(patch.radio_stations, Some(runtime.radio_stations.clone()));
  }

  #[test]
  fn legacy_config_radio_stations_do_not_override_existing_runtime_field() {
    let mut config = UserConfig::new();
    config.behavior.radio_stations = vec![RadioStationConfig {
      name: "Configured Groove".to_string(),
      url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
    }];
    let persisted = PersistedRuntimeState {
      radio_stations: Some(vec![RadioStationConfig {
        name: "Secret Agent".to_string(),
        url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
      }]),
      ..Default::default()
    };
    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&persisted);

    let patch =
      apply_legacy_config_radio_station_migration(&mut runtime, &persisted, &config.behavior);

    assert_eq!(
      runtime.radio_stations,
      vec![RadioStationConfig {
        name: "Secret Agent".to_string(),
        url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
      }]
    );
    assert_eq!(patch, PersistedRuntimeState::default());
  }

  #[test]
  fn legacy_runtime_state_fields_seed_missing_persisted_fields_from_raw_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  active_source: Radio
  shuffle_enabled: true
  seen_announcement_ids:
    - " seen "
    - ""
  dismissed_announcements:
    - old
"#,
    )
    .unwrap();
    let persisted = PersistedRuntimeState {
      volume_percent: Some(42),
      ..Default::default()
    };
    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&persisted);

    let patch =
      apply_legacy_config_runtime_state_migration(&path, &mut runtime, &persisted).unwrap();

    assert_eq!(runtime.active_source, Source::Radio);
    assert!(runtime.shuffle_enabled);
    assert_eq!(runtime.seen_announcement_ids, vec!["seen"]);
    assert_eq!(runtime.dismissed_announcements, vec!["old"]);
    assert_eq!(
      patch,
      PersistedRuntimeState {
        active_source: Some(Source::Radio),
        shuffle_enabled: Some(true),
        seen_announcement_ids: Some(vec!["seen".to_string()]),
        dismissed_announcements: Some(vec!["old".to_string()]),
        ..Default::default()
      }
    );
  }

  #[test]
  fn legacy_runtime_state_fields_do_not_override_existing_persisted_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  active_source: Radio
  shuffle_enabled: true
  seen_announcement_ids:
    - new
  dismissed_announcements:
    - new-dismissed
"#,
    )
    .unwrap();
    let persisted = PersistedRuntimeState {
      active_source: Some(Source::Local),
      shuffle_enabled: Some(false),
      seen_announcement_ids: Some(vec!["existing".to_string()]),
      dismissed_announcements: Some(vec!["existing-dismissed".to_string()]),
      ..Default::default()
    };
    let mut runtime = RuntimeState::default();
    runtime.apply_persisted(&persisted);

    let patch =
      apply_legacy_config_runtime_state_migration(&path, &mut runtime, &persisted).unwrap();

    assert_eq!(runtime.active_source, Source::Local);
    assert!(!runtime.shuffle_enabled);
    assert_eq!(runtime.seen_announcement_ids, vec!["existing"]);
    assert_eq!(runtime.dismissed_announcements, vec!["existing-dismissed"]);
    assert_eq!(patch, PersistedRuntimeState::default());
  }

  #[test]
  fn legacy_runtime_cleanup_removes_only_migrated_runtime_behavior_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  active_source: Radio
  shuffle_enabled: true
  seen_announcement_ids:
    - seen
  dismissed_announcements:
    - dismissed
  seek_milliseconds: 7000
  radio_stations:
    - name: Groove Salad
      url: https://ice1.somafm.com/groovesalad-128-mp3
"#,
    )
    .unwrap();

    let targets = legacy_config_cleanup_targets(
      &path,
      &PersistedRuntimeState {
        active_source: Some(Source::Radio),
        shuffle_enabled: Some(true),
        seen_announcement_ids: Some(vec!["seen".to_string()]),
        dismissed_announcements: Some(vec!["dismissed".to_string()]),
        ..Default::default()
      },
    )
    .unwrap();
    assert_eq!(
      targets.runtime_state_keys.as_slice(),
      LEGACY_RUNTIME_STATE_BEHAVIOR_KEYS
    );
    assert!(!targets.removes_radio_stations());

    assert!(remove_legacy_config_fields(&path, &targets).unwrap());

    let raw = std::fs::read_to_string(&path).unwrap();
    let config: Value = serde_yaml::from_str(&raw).unwrap();
    let behavior = behavior_mapping(&config).unwrap();
    for key in LEGACY_RUNTIME_STATE_BEHAVIOR_KEYS {
      assert!(behavior.get(yaml_key(key)).is_none());
    }
    assert_eq!(
      behavior
        .get(yaml_key("seek_milliseconds"))
        .and_then(Value::as_i64),
      Some(7000)
    );
    assert!(behavior.get(yaml_key("radio_stations")).is_some());
  }

  #[test]
  fn radio_station_config_migration_removes_only_behavior_radio_stations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  seek_milliseconds: 7000
  radio_stations:
    - name: Groove Salad
      url: https://ice1.somafm.com/groovesalad-128-mp3
keybindings:
  back: q
"#,
    )
    .unwrap();

    let targets = legacy_config_cleanup_targets(
      &path,
      &PersistedRuntimeState {
        radio_stations: Some(vec![RadioStationConfig {
          name: "Groove Salad".to_string(),
          url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
        }]),
        ..Default::default()
      },
    )
    .unwrap();
    assert!(targets.runtime_state_keys.is_empty());
    assert!(targets.removes_radio_stations());

    assert!(remove_legacy_config_fields(&path, &targets).unwrap());

    let raw = std::fs::read_to_string(&path).unwrap();
    let config: serde_yaml::Value = serde_yaml::from_str(&raw).unwrap();
    let behavior = config
      .as_mapping()
      .and_then(|map| map.get(serde_yaml::Value::String("behavior".to_string())))
      .and_then(|behavior| behavior.as_mapping())
      .unwrap();
    assert!(behavior
      .get(serde_yaml::Value::String("radio_stations".to_string()))
      .is_none());
    assert_eq!(
      behavior
        .get(serde_yaml::Value::String("seek_milliseconds".to_string()))
        .and_then(|value| value.as_i64()),
      Some(7000)
    );
    assert_eq!(
      config
        .as_mapping()
        .and_then(|map| map.get(serde_yaml::Value::String("keybindings".to_string())))
        .and_then(|keybindings| keybindings.as_mapping())
        .and_then(|keybindings| { keybindings.get(serde_yaml::Value::String("back".to_string())) })
        .and_then(|value| value.as_str()),
      Some("q")
    );
  }

  #[test]
  fn radio_station_cleanup_retries_after_state_already_owns_migrated_stations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  radio_stations:
    - name: Groove Salad
      url: https://ice1.somafm.com/groovesalad-128-mp3
"#,
    )
    .unwrap();

    let targets = legacy_config_cleanup_targets(
      &path,
      &PersistedRuntimeState {
        radio_stations: Some(vec![
          RadioStationConfig {
            name: "Groove Salad".to_string(),
            url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
          },
          RadioStationConfig {
            name: "Secret Agent".to_string(),
            url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
          },
        ]),
        ..Default::default()
      },
    )
    .unwrap();

    assert!(targets.removes_radio_stations());
  }

  #[test]
  fn radio_station_cleanup_keeps_config_owned_stations_not_present_in_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  radio_stations:
    - name: Configured Groove
      url: https://ice1.somafm.com/groovesalad-128-mp3
"#,
    )
    .unwrap();

    let targets = legacy_config_cleanup_targets(
      &path,
      &PersistedRuntimeState {
        radio_stations: Some(vec![RadioStationConfig {
          name: "Secret Agent".to_string(),
          url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
        }]),
        ..Default::default()
      },
    )
    .unwrap();

    assert!(!targets.removes_radio_stations());
    assert!(targets.is_empty());
  }
}
