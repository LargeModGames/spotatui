//! Persisted runtime state and app-managed mutable data.
//!
//! This is the machine-owned counterpart to `config.yml`. It holds values that
//! change as the app runs, or data the app manages directly, so ordinary config
//! edits do not churn every time volume, source, pane sizes, announcements, or
//! radio stations change.

use crate::core::{limits::MAX_PLAYBAR_ROWS, source::Source};
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

/// The Qobuz web-player constants, cached by bundle version (public values only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QobuzBundleCache {
  pub bundle_version: String,
  pub app_id: String,
  pub app_secret: String,
  pub oauth_key: String,
}

impl QobuzBundleCache {
  fn is_complete(&self) -> bool {
    !self.bundle_version.is_empty()
      && !self.app_id.is_empty()
      && !self.app_secret.is_empty()
      && !self.oauth_key.is_empty()
  }
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
  /// Whether the one-time community-playlist-pin prompt has been shown.
  pub community_pin_prompt_shown: bool,
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
      community_pin_prompt_shown: false,
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
      self.playbar_height_rows = playbar_height_rows.min(MAX_PLAYBAR_ROWS);
    }
    if let Some(library_height_percent) = state.library_height_percent {
      self.library_height_percent = library_height_percent.min(100);
    }
    if let Some(radio_stations) = &state.radio_stations {
      self.radio_stations = sanitized_radio_stations(radio_stations);
    }
    if let Some(community_pin_prompt_shown) = state.community_pin_prompt_shown {
      self.community_pin_prompt_shown = community_pin_prompt_shown;
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
      playbar_height_rows: Some(self.playbar_height_rows.min(MAX_PLAYBAR_ROWS)),
      library_height_percent: Some(self.library_height_percent.min(100)),
      radio_stations: Some(sanitized_radio_stations(&self.radio_stations)),
      community_pin_prompt_shown: Some(self.community_pin_prompt_shown),
      qobuz_bundle_cache: None,
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
  #[serde(skip_serializing_if = "Option::is_none")]
  pub community_pin_prompt_shown: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub qobuz_bundle_cache: Option<QobuzBundleCache>,
}

impl PersistedRuntimeState {
  pub fn volume_percent(volume_percent: u8) -> Self {
    Self {
      volume_percent: Some(volume_percent.min(100)),
      ..Default::default()
    }
  }

  pub fn shuffle_enabled(shuffle_enabled: bool) -> Self {
    Self {
      shuffle_enabled: Some(shuffle_enabled),
      ..Default::default()
    }
  }

  pub fn active_source(active_source: Source) -> Self {
    Self {
      active_source: Some(active_source),
      ..Default::default()
    }
  }

  pub fn announcements(
    seen_announcement_ids: &[String],
    dismissed_announcements: &[String],
  ) -> Self {
    Self {
      seen_announcement_ids: Some(sanitized_ids(seen_announcement_ids)),
      dismissed_announcements: Some(sanitized_ids(dismissed_announcements)),
      ..Default::default()
    }
  }

  pub fn sidebar_width_percent(sidebar_width_percent: u8) -> Self {
    Self {
      sidebar_width_percent: Some(sidebar_width_percent.min(100)),
      ..Default::default()
    }
  }

  pub fn playbar_height_rows(playbar_height_rows: u16) -> Self {
    Self {
      playbar_height_rows: Some(playbar_height_rows.min(MAX_PLAYBAR_ROWS)),
      ..Default::default()
    }
  }

  pub fn library_height_percent(library_height_percent: u8) -> Self {
    Self {
      library_height_percent: Some(library_height_percent.min(100)),
      ..Default::default()
    }
  }

  pub fn radio_station(radio_station: RadioStationConfig) -> Self {
    Self {
      radio_stations: Some(vec![radio_station]),
      ..Default::default()
    }
  }

  pub fn community_pin_prompt_shown(community_pin_prompt_shown: bool) -> Self {
    Self {
      community_pin_prompt_shown: Some(community_pin_prompt_shown),
      ..Default::default()
    }
  }

  #[cfg_attr(not(feature = "qobuz"), allow(dead_code))]
  pub fn qobuz_bundle_cache(qobuz_bundle_cache: QobuzBundleCache) -> Self {
    Self {
      qobuz_bundle_cache: Some(qobuz_bundle_cache),
      ..Default::default()
    }
  }

  pub fn merge_patch(&mut self, patch: &Self) {
    merge_state_patch(self, patch);
  }

  pub fn is_empty(&self) -> bool {
    self.volume_percent.is_none()
      && self.shuffle_enabled.is_none()
      && self.active_source.is_none()
      && self.seen_announcement_ids.is_none()
      && self.dismissed_announcements.is_none()
      && self.sidebar_width_percent.is_none()
      && self.playbar_height_rows.is_none()
      && self.library_height_percent.is_none()
      && self.radio_stations.is_none()
      && self.community_pin_prompt_shown.is_none()
      && self.qobuz_bundle_cache.is_none()
  }
}

/// Location of the app runtime-state file: `<state dir>/state.yml`.
pub fn default_state_path() -> Result<PathBuf> {
  crate::core::paths::app_state_dir()
    .map(|dir| dir.join(FILE_NAME))
    .ok_or_else(|| anyhow!("cannot resolve the spotatui state directory"))
}

/// Outcome of reading `state.yml` off disk, before deciding how to react to a
/// bad file. Loading (startup) surfaces a malformed file as an error, while
/// saving heals it; both share this classification so the read logic lives in
/// one place.
enum StateRead {
  /// The file is absent or blank; treat as empty state.
  Empty,
  /// The file parsed cleanly.
  Parsed(PersistedRuntimeState),
  /// The file was readable but its contents did not parse.
  Malformed(anyhow::Error),
  /// The file could not be read (permissions, other I/O error).
  Unreadable(anyhow::Error),
}

fn read_state(path: &Path) -> StateRead {
  // Read raw bytes so non-UTF-8 contents count as a corrupt file we can heal on
  // save, not an I/O error to propagate. `read_to_string` would fold invalid
  // UTF-8 (e.g. a write truncated mid-multibyte-character) into `ErrorKind`,
  // leaving it wedged forever.
  let bytes = match std::fs::read(path) {
    Ok(bytes) => bytes,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return StateRead::Empty,
    Err(error) => {
      return StateRead::Unreadable(
        anyhow::Error::new(error).context(format!("reading {}", path.display())),
      )
    }
  };

  let contents = match std::str::from_utf8(&bytes) {
    Ok(contents) => contents,
    Err(error) => {
      return StateRead::Malformed(
        anyhow::Error::new(error)
          .context(format!("malformed runtime state file: {}", path.display())),
      )
    }
  };

  if contents.trim().is_empty() {
    return StateRead::Empty;
  }

  match serde_yaml::from_str::<PersistedRuntimeState>(contents) {
    Ok(state) => StateRead::Parsed(sanitized_persisted_state(&state)),
    Err(error) => StateRead::Malformed(
      anyhow::Error::new(error)
        .context(format!("malformed runtime state file: {}", path.display())),
    ),
  }
}

/// Load state. A missing file means empty state; malformed state is returned as
/// an error so callers can log and continue with config/default values.
pub fn load(path: &Path) -> Result<PersistedRuntimeState> {
  match read_state(path) {
    StateRead::Empty => Ok(PersistedRuntimeState::default()),
    StateRead::Parsed(state) => Ok(state),
    StateRead::Malformed(error) | StateRead::Unreadable(error) => Err(error),
  }
}

/// Load existing state for a read-modify-write save, healing a corrupt file.
///
/// `save` re-reads `state.yml` before merging, so a malformed file would
/// otherwise wedge every future save forever and surface a UI error on each
/// runtime change (volume nudge, layout tweak). Mirror the `config.yml`
/// fallback: move the unreadable file aside to `state.yml.bak` (best effort),
/// warn, and continue from empty state. A genuine read failure is still
/// propagated so a transient error never discards good data by overwriting it
/// with defaults.
fn load_for_save(path: &Path) -> Result<PersistedRuntimeState> {
  match read_state(path) {
    StateRead::Empty => Ok(PersistedRuntimeState::default()),
    StateRead::Parsed(state) => Ok(state),
    StateRead::Malformed(error) => {
      let backup = path.with_extension("yml.bak");
      match std::fs::rename(path, &backup) {
        Ok(()) => log::warn!(
          "[state] {error:#}; backed up to {} and starting from defaults",
          backup.display()
        ),
        Err(rename_error) => log::warn!(
          "[state] {error:#}; could not back up to {} ({rename_error}); overwriting with defaults",
          backup.display()
        ),
      }
      Ok(PersistedRuntimeState::default())
    }
    StateRead::Unreadable(error) => Err(error),
  }
}

/// Save a state patch atomically and privately.
///
/// Runtime saves are intentionally read-modify-write: most save call sites only
/// own one field, so blindly writing a whole in-memory snapshot would clobber
/// updates made by another running instance.
pub fn save(path: &Path, state: &PersistedRuntimeState) -> Result<()> {
  let mut merged = load_for_save(path)?;
  merge_state_patch(&mut merged, state);
  write_state(path, &merged)
}

/// Remove a radio station without sending a stale full station list.
pub fn save_removing_radio_station(path: &Path, url: &str) -> Result<()> {
  let mut merged = load_for_save(path)?;
  if let Some(stations) = &mut merged.radio_stations {
    let url = url.trim();
    stations.retain(|station| station.url.trim() != url);
    *stations = sanitized_radio_stations(stations);
  } else {
    merged.radio_stations = Some(Vec::new());
  }
  write_state(path, &merged)
}

fn write_state(path: &Path, state: &PersistedRuntimeState) -> Result<()> {
  let state = sanitized_persisted_state(state);
  let yaml = serde_yaml::to_string(&state).context("serializing runtime state")?;
  if let Some(dir) = path.parent() {
    crate::core::paths::ensure_private_dir(dir)?;
  }
  crate::core::auth::write_private_file_atomic(path, yaml.as_bytes())
    .with_context(|| format!("writing {}", path.display()))?;
  Ok(())
}

fn sanitized_persisted_state(state: &PersistedRuntimeState) -> PersistedRuntimeState {
  PersistedRuntimeState {
    volume_percent: state.volume_percent.map(|volume| volume.min(100)),
    shuffle_enabled: state.shuffle_enabled,
    active_source: state.active_source,
    seen_announcement_ids: state.seen_announcement_ids.as_deref().map(sanitized_ids),
    dismissed_announcements: state.dismissed_announcements.as_deref().map(sanitized_ids),
    sidebar_width_percent: state.sidebar_width_percent.map(|width| width.min(100)),
    playbar_height_rows: state
      .playbar_height_rows
      .map(|rows| rows.min(MAX_PLAYBAR_ROWS)),
    library_height_percent: state.library_height_percent.map(|height| height.min(100)),
    radio_stations: state
      .radio_stations
      .as_deref()
      .map(sanitized_radio_stations),
    community_pin_prompt_shown: state.community_pin_prompt_shown,
    qobuz_bundle_cache: state
      .qobuz_bundle_cache
      .clone()
      .filter(QobuzBundleCache::is_complete),
  }
}

fn merge_state_patch(merged: &mut PersistedRuntimeState, patch: &PersistedRuntimeState) {
  if let Some(volume_percent) = patch.volume_percent {
    merged.volume_percent = Some(volume_percent.min(100));
  }
  if let Some(shuffle_enabled) = patch.shuffle_enabled {
    merged.shuffle_enabled = Some(shuffle_enabled);
  }
  if let Some(active_source) = patch.active_source {
    merged.active_source = Some(active_source);
  }
  if let Some(seen_announcement_ids) = &patch.seen_announcement_ids {
    merged.seen_announcement_ids = Some(merged_ids(
      merged.seen_announcement_ids.as_deref().unwrap_or(&[]),
      seen_announcement_ids,
    ));
  }
  if let Some(dismissed_announcements) = &patch.dismissed_announcements {
    merged.dismissed_announcements = Some(merged_ids(
      merged.dismissed_announcements.as_deref().unwrap_or(&[]),
      dismissed_announcements,
    ));
  }
  if let Some(sidebar_width_percent) = patch.sidebar_width_percent {
    merged.sidebar_width_percent = Some(sidebar_width_percent.min(100));
  }
  if let Some(playbar_height_rows) = patch.playbar_height_rows {
    merged.playbar_height_rows = Some(playbar_height_rows.min(MAX_PLAYBAR_ROWS));
  }
  if let Some(library_height_percent) = patch.library_height_percent {
    merged.library_height_percent = Some(library_height_percent.min(100));
  }
  if let Some(radio_stations) = &patch.radio_stations {
    merged.radio_stations = Some(merged_radio_stations(
      merged.radio_stations.as_deref().unwrap_or(&[]),
      radio_stations,
    ));
  }
  if let Some(community_pin_prompt_shown) = patch.community_pin_prompt_shown {
    merged.community_pin_prompt_shown = Some(community_pin_prompt_shown);
  }
  if let Some(qobuz_bundle_cache) = &patch.qobuz_bundle_cache {
    merged.qobuz_bundle_cache = Some(qobuz_bundle_cache.clone());
  }
}

fn sanitized_ids(ids: &[String]) -> Vec<String> {
  ids
    .iter()
    .map(|id| id.trim().to_string())
    .filter(|id| !id.is_empty())
    .collect()
}

fn merged_ids(existing: &[String], incoming: &[String]) -> Vec<String> {
  let mut seen = std::collections::HashSet::new();
  existing
    .iter()
    .chain(incoming.iter())
    .filter_map(|id| {
      let id = id.trim();
      if id.is_empty() || !seen.insert(id.to_string()) {
        return None;
      }
      Some(id.to_string())
    })
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

pub(crate) fn merged_radio_stations(
  existing: &[RadioStationConfig],
  incoming: &[RadioStationConfig],
) -> Vec<RadioStationConfig> {
  let mut stations = sanitized_radio_stations(existing);
  let mut seen_urls = stations
    .iter()
    .map(|station| station.url.clone())
    .collect::<std::collections::HashSet<_>>();

  for station in sanitized_radio_stations(incoming) {
    if seen_urls.insert(station.url.clone()) {
      stations.push(station);
    }
  }

  stations
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
      community_pin_prompt_shown: Some(true),
      qobuz_bundle_cache: Some(QobuzBundleCache {
        bundle_version: "8.2.0-b034".to_string(),
        app_id: "123456789".to_string(),
        app_secret: "s".repeat(32),
        oauth_key: "k".to_string(),
      }),
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
  fn community_pin_prompt_shown_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");

    save(
      &path,
      &PersistedRuntimeState::community_pin_prompt_shown(true),
    )
    .unwrap();

    assert_eq!(load(&path).unwrap().community_pin_prompt_shown, Some(true));
  }

  #[test]
  fn incomplete_qobuz_bundle_cache_is_dropped_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");

    save(
      &path,
      &PersistedRuntimeState::qobuz_bundle_cache(QobuzBundleCache {
        bundle_version: "8.2.0-b034".to_string(),
        app_id: String::new(),
        app_secret: "s".to_string(),
        oauth_key: "k".to_string(),
      }),
    )
    .unwrap();

    assert_eq!(load(&path).unwrap().qobuz_bundle_cache, None);
  }

  #[test]
  fn state_load_and_save_caps_playbar_height_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");
    std::fs::write(
      &path,
      format!("playbar_height_rows: {}\n", MAX_PLAYBAR_ROWS + 10),
    )
    .unwrap();

    assert_eq!(
      load(&path).unwrap().playbar_height_rows,
      Some(MAX_PLAYBAR_ROWS)
    );

    save(
      &path,
      &PersistedRuntimeState {
        volume_percent: Some(40),
        ..Default::default()
      },
    )
    .unwrap();

    assert_eq!(
      load(&path).unwrap().playbar_height_rows,
      Some(MAX_PLAYBAR_ROWS)
    );
  }

  #[test]
  fn state_save_merges_sparse_patches_without_clobbering_unmentioned_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");

    let existing = PersistedRuntimeState {
      volume_percent: Some(20),
      active_source: Some(Source::Radio),
      seen_announcement_ids: Some(vec!["first".to_string()]),
      radio_stations: Some(vec![RadioStationConfig {
        name: "Alpha".to_string(),
        url: "https://example.test/alpha".to_string(),
      }]),
      ..Default::default()
    };
    save(&path, &existing).unwrap();

    let patch = PersistedRuntimeState {
      shuffle_enabled: Some(true),
      seen_announcement_ids: Some(vec!["second".to_string()]),
      radio_stations: Some(vec![RadioStationConfig {
        name: "Beta".to_string(),
        url: "https://example.test/beta".to_string(),
      }]),
      ..Default::default()
    };
    save(&path, &patch).unwrap();

    assert_eq!(
      load(&path).unwrap(),
      PersistedRuntimeState {
        volume_percent: Some(20),
        shuffle_enabled: Some(true),
        active_source: Some(Source::Radio),
        seen_announcement_ids: Some(vec!["first".to_string(), "second".to_string()]),
        radio_stations: Some(vec![
          RadioStationConfig {
            name: "Alpha".to_string(),
            url: "https://example.test/alpha".to_string(),
          },
          RadioStationConfig {
            name: "Beta".to_string(),
            url: "https://example.test/beta".to_string(),
          },
        ]),
        ..Default::default()
      }
    );
  }

  #[test]
  fn save_heals_a_malformed_state_file_and_keeps_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");
    let garbage = "this: is: not: valid: yaml: [\n";
    std::fs::write(&path, garbage).unwrap();

    // A malformed file makes `load` fail, but `save` must still succeed.
    assert!(load(&path).is_err());
    save(&path, &PersistedRuntimeState::volume_percent(42)).unwrap();

    // The corrupt bytes are preserved next to the healed file, untouched.
    let backup = path.with_extension("yml.bak");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), garbage);

    // The rewritten file parses and reflects the patch, starting from defaults.
    assert_eq!(
      load(&path).unwrap(),
      PersistedRuntimeState {
        volume_percent: Some(42),
        ..Default::default()
      }
    );
  }

  #[test]
  fn save_heals_a_non_utf8_state_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");
    // A write truncated mid-multibyte-character leaves invalid UTF-8 on disk.
    let garbage = b"volume_percent: 5\xff\xfe";
    std::fs::write(&path, garbage).unwrap();

    assert!(load(&path).is_err());
    save(&path, &PersistedRuntimeState::volume_percent(42)).unwrap();

    let backup = path.with_extension("yml.bak");
    assert_eq!(std::fs::read(&backup).unwrap(), garbage);
    assert_eq!(
      load(&path).unwrap(),
      PersistedRuntimeState {
        volume_percent: Some(42),
        ..Default::default()
      }
    );
  }

  #[test]
  fn save_leaves_no_temporary_files_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");

    save(&path, &PersistedRuntimeState::volume_percent(30)).unwrap();

    let has_temp = std::fs::read_dir(dir.path())
      .unwrap()
      .filter_map(Result::ok)
      .any(|entry| entry.file_name().to_string_lossy().contains(".tmp"));
    assert!(!has_temp, "atomic save left a temporary file behind");
  }

  #[test]
  fn radio_station_removal_preserves_other_runtime_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");

    save(
      &path,
      &PersistedRuntimeState {
        volume_percent: Some(55),
        radio_stations: Some(vec![
          RadioStationConfig {
            name: "Alpha".to_string(),
            url: "https://example.test/alpha".to_string(),
          },
          RadioStationConfig {
            name: "Beta".to_string(),
            url: "https://example.test/beta".to_string(),
          },
        ]),
        ..Default::default()
      },
    )
    .unwrap();

    save_removing_radio_station(&path, " https://example.test/alpha ").unwrap();

    assert_eq!(
      load(&path).unwrap(),
      PersistedRuntimeState {
        volume_percent: Some(55),
        radio_stations: Some(vec![RadioStationConfig {
          name: "Beta".to_string(),
          url: "https://example.test/beta".to_string(),
        }]),
        ..Default::default()
      }
    );
  }

  #[test]
  fn persisted_runtime_state_builds_sparse_patches() {
    let mut patch = PersistedRuntimeState::volume_percent(120);
    patch.merge_patch(&PersistedRuntimeState::active_source(Source::Subsonic));
    patch.merge_patch(&PersistedRuntimeState::playbar_height_rows(
      MAX_PLAYBAR_ROWS + 10,
    ));

    assert_eq!(
      patch,
      PersistedRuntimeState {
        volume_percent: Some(100),
        active_source: Some(Source::Subsonic),
        playbar_height_rows: Some(MAX_PLAYBAR_ROWS),
        ..Default::default()
      }
    );
  }

  #[test]
  fn runtime_state_applies_and_sanitizes_persisted_values() {
    let state = PersistedRuntimeState {
      volume_percent: Some(150),
      active_source: Some(Source::Subsonic),
      seen_announcement_ids: Some(vec![" seen ".to_string(), " ".to_string()]),
      sidebar_width_percent: Some(120),
      playbar_height_rows: Some(MAX_PLAYBAR_ROWS + 10),
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
    assert_eq!(runtime.playbar_height_rows, MAX_PLAYBAR_ROWS);
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
