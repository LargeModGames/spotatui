//! Short-lived upgrade shims for moving old on-disk shapes into current state.
//!
//! Keep each migration isolated here so it can be deleted cleanly once the
//! minimum supported upgrade path no longer includes the legacy release.

use crate::core::source::Source;
use crate::core::state::{PersistedRuntimeState, RuntimeState};
use crate::core::user_config::BehaviorConfig;
use anyhow::{anyhow, Context, Result};
use serde_yaml::{Mapping, Value};
use std::{fs, path::Path};

const LEGACY_STATE_FILE_RELATIVE_PATHS: [&str; 4] = [
  "last_session.yml",
  "history/listens.jsonl",
  "history/last_recap_at.txt",
  "history/last_synced_at.txt",
];

/// Move app-managed files that used to live under the XDG config directory into
/// the XDG state directory.
///
/// This intentionally never overwrites an existing state file: once the new
/// path has data, merging the old file is file-format-specific and unsafe for a
/// generic shim. Run once at startup and remove after the config-to-state
/// upgrade window closes.
pub(crate) fn apply_legacy_state_file_migrations() -> Result<()> {
  let Some(config_dir) = crate::core::paths::app_config_dir() else {
    return Ok(());
  };
  let state_dir = crate::core::paths::app_state_dir()
    .ok_or_else(|| anyhow!("cannot resolve the spotatui state directory"))?;
  crate::core::paths::ensure_private_dir(&state_dir)?;

  for relative_path in LEGACY_STATE_FILE_RELATIVE_PATHS {
    migrate_legacy_path_if_unclaimed(
      &config_dir.join(relative_path),
      &state_dir.join(relative_path),
    )?;
  }

  Ok(())
}

pub(crate) fn migrate_legacy_path_if_unclaimed(
  legacy_path: &Path,
  state_path: &Path,
) -> Result<bool> {
  if path_exists(state_path)? || !path_exists(legacy_path)? {
    return Ok(false);
  }

  if let Some(parent) = state_path.parent() {
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
  }

  match fs::rename(legacy_path, state_path) {
    Ok(()) => {
      log::info!(
        "migrated legacy app data from {} to {}",
        legacy_path.display(),
        state_path.display()
      );
      Ok(true)
    }
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
    Err(rename_error) => {
      if !copy_legacy_path_if_unclaimed(legacy_path, state_path, &rename_error)? {
        return Ok(false);
      }
      remove_legacy_path_after_migration(legacy_path, state_path);
      log::info!(
        "migrated legacy app data from {} to {}",
        legacy_path.display(),
        state_path.display()
      );
      Ok(true)
    }
  }
}

fn copy_legacy_path_if_unclaimed(
  legacy_path: &Path,
  state_path: &Path,
  rename_error: &std::io::Error,
) -> Result<bool> {
  let metadata = match fs::symlink_metadata(legacy_path) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| format!("checking {}", legacy_path.display()));
    }
  };

  if metadata.is_dir() {
    return copy_legacy_dir_if_unclaimed(legacy_path, state_path, rename_error);
  }

  let mut source = match fs::File::open(legacy_path) {
    Ok(file) => file,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| format!("opening {}", legacy_path.display()));
    }
  };
  let mut target_options = fs::OpenOptions::new();
  target_options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    target_options.mode(0o600);
  }
  let mut target = match target_options.open(state_path) {
    Ok(file) => file,
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| {
        format!(
          "renaming {} to {} failed ({rename_error}); creating copy target also failed",
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
        "renaming {} to {} failed ({rename_error}); copying fallback also failed",
        legacy_path.display(),
        state_path.display()
      )
    });
  }

  Ok(true)
}

fn copy_legacy_dir_if_unclaimed(
  legacy_path: &Path,
  state_path: &Path,
  rename_error: &std::io::Error,
) -> Result<bool> {
  match fs::create_dir(state_path) {
    Ok(()) => set_private_dir_permissions(state_path)?,
    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
    Err(error) => {
      return Err(error).with_context(|| {
        format!(
          "renaming {} to {} failed ({rename_error}); creating copy target also failed",
          legacy_path.display(),
          state_path.display()
        )
      });
    }
  }

  if let Err(error) = copy_dir_contents(legacy_path, state_path) {
    let _ = fs::remove_dir_all(state_path);
    return Err(error);
  }

  Ok(true)
}

fn create_private_dir(path: &Path) -> Result<()> {
  fs::create_dir(path).with_context(|| format!("creating {}", path.display()))?;
  set_private_dir_permissions(path)
}

fn set_private_dir_permissions(path: &Path) -> Result<()> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
      .with_context(|| format!("setting private permissions on {}", path.display()))?;
  }
  // Windows scopes %APPDATA% to the user already; nothing to tighten.
  #[cfg(not(unix))]
  let _ = path;

  Ok(())
}

fn copy_dir_contents(legacy_path: &Path, state_path: &Path) -> Result<()> {
  for entry in
    fs::read_dir(legacy_path).with_context(|| format!("reading {}", legacy_path.display()))?
  {
    let entry = entry?;
    let source = entry.path();
    let target = state_path.join(entry.file_name());
    let file_type = entry
      .file_type()
      .with_context(|| format!("checking {}", source.display()))?;

    if file_type.is_symlink() {
      continue;
    }

    if file_type.is_dir() {
      create_private_dir(&target)?;
      copy_dir_contents(&source, &target)?;
    } else {
      fs::copy(&source, &target).with_context(|| {
        format!(
          "copying legacy app data from {} to {}",
          source.display(),
          target.display()
        )
      })?;
    }
  }

  Ok(())
}

fn remove_legacy_path_after_migration(legacy_path: &Path, state_path: &Path) {
  let remove_result = match fs::symlink_metadata(legacy_path) {
    Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(legacy_path),
    Ok(_) => fs::remove_file(legacy_path),
    Err(error) => Err(error),
  };

  match remove_result {
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

/// Seed legacy in-app radio favorites from `config.yml` into `state.yml` when
/// the state field does not exist yet.
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

/// Seed `config.yml` and `state.yml` from the `client.yml` older releases
/// wrote: the native-streaming settings and the saved Connect device. A key
/// the config or state already has wins, a value equal to the default is not
/// written, and `client.yml` stays on disk so a downgrade still finds it.
pub(crate) fn apply_legacy_client_config_migration(
  client_path: &Path,
  config_path: &Path,
  behavior: &mut BehaviorConfig,
  runtime_state: &mut RuntimeState,
  persisted_state: &PersistedRuntimeState,
) -> Result<PersistedRuntimeState> {
  let mut patch = PersistedRuntimeState::default();
  let Some(client) = read_config_value(client_path)? else {
    return Ok(patch);
  };
  let Some(client) = client.as_mapping() else {
    return Ok(patch);
  };

  if persisted_state.spotify_device_id.is_none() {
    if let Some(device_id) = client
      .get(yaml_key("device_id"))
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|id| !id.is_empty())
    {
      runtime_state.spotify_device_id = Some(device_id.to_string());
      patch.spotify_device_id = Some(device_id.to_string());
    }
  }

  let config = read_config_value(config_path)?;
  let configured = |key: &str| {
    config
      .as_ref()
      .and_then(behavior_mapping)
      .is_some_and(|behavior| behavior.contains_key(yaml_key(key)))
  };
  let mut copied = Mapping::new();
  let mut copy = |key: &str, value: Value| {
    copied.insert(yaml_key(key), value);
  };
  if !configured("enable_streaming") {
    if let Some(enabled) = client
      .get(yaml_key("enable_streaming"))
      .and_then(Value::as_bool)
    {
      if enabled != behavior.enable_streaming {
        behavior.enable_streaming = enabled;
        copy("enable_streaming", Value::Bool(enabled));
      }
    }
  }
  if !configured("streaming_device_name") {
    if let Some(name) = client
      .get(yaml_key("streaming_device_name"))
      .and_then(Value::as_str)
      .map(str::trim)
      .filter(|name| !name.is_empty() && *name != behavior.streaming_device_name)
    {
      behavior.streaming_device_name = name.to_string();
      copy("streaming_device_name", Value::String(name.to_string()));
    }
  }
  if !configured("streaming_bitrate") {
    if let Some(bitrate) = client
      .get(yaml_key("streaming_bitrate"))
      .and_then(Value::as_u64)
      .and_then(|bitrate| u16::try_from(bitrate).ok())
      .map(crate::core::user_config::streaming_bitrate_or_default)
      .filter(|bitrate| *bitrate != behavior.streaming_bitrate)
    {
      behavior.streaming_bitrate = bitrate;
      copy("streaming_bitrate", Value::Number(bitrate.into()));
    }
  }
  if !configured("streaming_audio_cache") {
    if let Some(cache) = client
      .get(yaml_key("streaming_audio_cache"))
      .and_then(Value::as_bool)
      .filter(|cache| *cache != behavior.streaming_audio_cache)
    {
      behavior.streaming_audio_cache = cache;
      copy("streaming_audio_cache", Value::Bool(cache));
    }
  }

  if !copied.is_empty() {
    write_behavior_keys(config_path, config, copied)?;
  }
  Ok(patch)
}

/// Adds keys under `behavior:` and leaves the rest of the file as it was.
fn write_behavior_keys(path: &Path, config: Option<Value>, keys: Mapping) -> Result<()> {
  let mut config = config.unwrap_or_else(|| Value::Mapping(Mapping::new()));
  let map = config
    .as_mapping_mut()
    .ok_or_else(|| anyhow!("{} is not a YAML mapping", path.display()))?;
  let behavior = map
    .entry(yaml_key("behavior"))
    .or_insert_with(|| Value::Mapping(Mapping::new()))
    .as_mapping_mut()
    .ok_or_else(|| anyhow!("`behavior` in {} is not a YAML mapping", path.display()))?;
  behavior.extend(keys);
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
  }
  crate::core::auth::write_private_file_atomic(path, serde_yaml::to_string(&config)?.as_bytes())
    .with_context(|| format!("writing {}", path.display()))
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

    assert!(migrate_legacy_path_if_unclaimed(&legacy_path, &state_path).unwrap());
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

    assert!(!migrate_legacy_path_if_unclaimed(&legacy_path, &state_path).unwrap());
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

    assert!(!migrate_legacy_path_if_unclaimed(&legacy_path, &state_path).unwrap());
    assert_eq!(
      std::fs::read_to_string(&legacy_path).unwrap(),
      "legacy cursor"
    );
    assert_eq!(std::fs::read_to_string(&state_path).unwrap(), "offset:42");
  }

  #[test]
  fn legacy_path_migration_moves_directory_when_target_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("config").join("streaming_cache");
    let state_path = dir.path().join("cache").join("streaming_cache");
    std::fs::create_dir_all(&legacy_path).unwrap();
    std::fs::write(legacy_path.join("credentials.json"), "legacy credentials").unwrap();
    std::fs::create_dir_all(legacy_path.join("audio")).unwrap();
    std::fs::write(legacy_path.join("audio").join("chunk"), "audio cache").unwrap();

    assert!(migrate_legacy_path_if_unclaimed(&legacy_path, &state_path).unwrap());

    assert!(!legacy_path.exists());
    assert_eq!(
      std::fs::read_to_string(state_path.join("credentials.json")).unwrap(),
      "legacy credentials"
    );
    assert_eq!(
      std::fs::read_to_string(state_path.join("audio").join("chunk")).unwrap(),
      "audio cache"
    );
  }

  #[cfg(unix)]
  #[test]
  fn legacy_directory_copy_skips_symlink_entries_before_recursing() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("config").join("streaming_cache");
    let state_path = dir.path().join("state").join("streaming_cache");
    std::fs::create_dir_all(legacy_path.join("audio")).unwrap();
    std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    std::fs::write(legacy_path.join("audio").join("chunk"), "audio cache").unwrap();
    std::os::unix::fs::symlink(&legacy_path, legacy_path.join("loop")).unwrap();
    let rename_error = std::io::Error::other("forcing copy fallback");

    assert!(copy_legacy_dir_if_unclaimed(&legacy_path, &state_path, &rename_error).unwrap());

    assert_eq!(
      std::fs::read_to_string(state_path.join("audio").join("chunk")).unwrap(),
      "audio cache"
    );
    assert!(!state_path.join("loop").exists());
  }

  #[cfg(unix)]
  #[test]
  fn legacy_directory_copy_sets_private_permissions_on_created_directories() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join("config").join("streaming_cache");
    let state_path = dir.path().join("state").join("streaming_cache");
    std::fs::create_dir_all(legacy_path.join("audio")).unwrap();
    std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    std::fs::write(legacy_path.join("audio").join("chunk"), "audio cache").unwrap();
    let rename_error = std::io::Error::other("forcing copy fallback");

    assert!(copy_legacy_dir_if_unclaimed(&legacy_path, &state_path, &rename_error).unwrap());

    let top_level_mode = std::fs::metadata(&state_path).unwrap().permissions().mode() & 0o777;
    let subdirectory_mode = std::fs::metadata(state_path.join("audio"))
      .unwrap()
      .permissions()
      .mode()
      & 0o777;
    assert_eq!(top_level_mode, 0o700);
    assert_eq!(subdirectory_mode, 0o700);
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
  fn legacy_runtime_state_migration_preserves_free_source_startup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(
      &path,
      r#"
behavior:
  active_source: Local
"#,
    )
    .unwrap();
    let persisted = PersistedRuntimeState::default();
    let mut runtime = RuntimeState::default();

    let patch =
      apply_legacy_config_runtime_state_migration(&path, &mut runtime, &persisted).unwrap();

    assert_eq!(runtime.active_source, Source::Local);
    assert_eq!(
      patch,
      PersistedRuntimeState {
        active_source: Some(Source::Local),
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

  fn client_migration(
    client_yml: &str,
    config_yml: Option<&str>,
    persisted: &PersistedRuntimeState,
  ) -> (
    tempfile::TempDir,
    Result<PersistedRuntimeState>,
    BehaviorConfig,
    RuntimeState,
  ) {
    let dir = tempfile::tempdir().unwrap();
    let client = dir.path().join("client.yml");
    std::fs::write(&client, client_yml).unwrap();
    let config = dir.path().join("config.yml");
    if let Some(config_yml) = config_yml {
      std::fs::write(&config, config_yml).unwrap();
    }
    let mut behavior = crate::core::user_config::UserConfig::new().behavior;
    let mut runtime = RuntimeState::default();
    let result = apply_legacy_client_config_migration(
      &client,
      &config,
      &mut behavior,
      &mut runtime,
      persisted,
    );
    (dir, result, behavior, runtime)
  }

  #[test]
  fn changed_streaming_settings_move_from_client_yml_to_the_config() {
    let default_streaming = cfg!(feature = "streaming");
    let (dir, result, behavior, _) = client_migration(
      &format!(
        "client_id: abc\nstreaming_device_name: Desk\nstreaming_bitrate: 160\nenable_streaming: {default_streaming}\n"
      ),
      Some("theme:\n  active: \"#ffffff\"\n"),
      &PersistedRuntimeState::default(),
    );
    result.unwrap();
    assert_eq!(behavior.streaming_device_name, "Desk");
    assert_eq!(behavior.streaming_bitrate, 160);

    let written = std::fs::read_to_string(dir.path().join("config.yml")).unwrap();
    let config: Value = serde_yaml::from_str(&written).unwrap();
    let behavior = behavior_mapping(&config).unwrap();
    assert_eq!(
      behavior.get(yaml_key("streaming_device_name")),
      Some(&yaml_key("Desk"))
    );
    assert_eq!(
      behavior.get(yaml_key("streaming_bitrate")),
      Some(&Value::from(160))
    );
    assert!(
      !behavior.contains_key(yaml_key("enable_streaming")),
      "a default value is not written"
    );
    assert!(written.contains("theme"), "the rest of the config stays");
  }

  #[test]
  fn a_setting_the_config_already_has_wins() {
    let (dir, result, behavior, _) = client_migration(
      "streaming_device_name: Old\n",
      Some("behavior:\n  streaming_device_name: New\n"),
      &PersistedRuntimeState::default(),
    );
    result.unwrap();
    assert_eq!(
      behavior.streaming_device_name, "spotatui",
      "the config load sets New later"
    );
    let written = std::fs::read_to_string(dir.path().join("config.yml")).unwrap();
    assert!(written.contains("New") && !written.contains("Old"));
  }

  #[test]
  fn default_streaming_settings_write_no_config() {
    let (dir, result, _, _) = client_migration(
      "client_id: abc\nport: 8888\nstreaming_device_name: spotatui\nstreaming_bitrate: 320\nstreaming_audio_cache: false\n",
      None,
      &PersistedRuntimeState::default(),
    );
    result.unwrap();
    assert!(!dir.path().join("config.yml").exists());
  }

  #[test]
  fn an_unknown_bitrate_is_not_migrated() {
    let (_dir, result, behavior, _) = client_migration(
      "streaming_bitrate: 999\n",
      None,
      &PersistedRuntimeState::default(),
    );
    result.unwrap();
    assert_eq!(behavior.streaming_bitrate, 320);
  }

  #[test]
  fn the_saved_device_moves_to_the_state() {
    let (_dir, result, _, runtime) = client_migration(
      "device_id: \" phone-1 \"\n",
      None,
      &PersistedRuntimeState::default(),
    );
    assert_eq!(
      result.unwrap().spotify_device_id.as_deref(),
      Some("phone-1")
    );
    assert_eq!(runtime.spotify_device_id.as_deref(), Some("phone-1"));
  }

  #[test]
  fn a_device_the_state_already_has_wins() {
    let persisted = PersistedRuntimeState::spotify_device_id("current");
    let (_dir, result, _, runtime) = client_migration("device_id: old\n", None, &persisted);
    assert!(result.unwrap().is_empty());
    assert_eq!(runtime.spotify_device_id, None);
  }

  #[test]
  fn no_client_yml_is_no_migration() {
    let dir = tempfile::tempdir().unwrap();
    let mut behavior = crate::core::user_config::UserConfig::new().behavior;
    let mut runtime = RuntimeState::default();
    let patch = apply_legacy_client_config_migration(
      &dir.path().join("client.yml"),
      &dir.path().join("config.yml"),
      &mut behavior,
      &mut runtime,
      &PersistedRuntimeState::default(),
    )
    .unwrap();
    assert!(patch.is_empty());
    assert!(!dir.path().join("config.yml").exists());
  }

  #[test]
  fn a_garbled_client_yml_is_an_error() {
    let (_dir, result, _, _) = client_migration(
      "device_id: [unclosed\n",
      None,
      &PersistedRuntimeState::default(),
    );
    assert!(result.is_err());
  }
}
