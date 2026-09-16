use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Station {
  pub name: String,
  pub url: String,
}

#[derive(Default, Deserialize)]
struct ConfigFile {
  #[serde(default)]
  behavior: Behavior,
  #[serde(default)]
  theme: ThemeSettings,
}

#[derive(Default, Deserialize)]
struct Behavior {
  #[serde(default)]
  radio_stations: Vec<Station>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeSettings {
  #[serde(alias = "active")]
  pub focused_border: String,
  #[serde(alias = "header")]
  pub now_playing: String,
  #[serde(alias = "selected")]
  pub selection: String,
  pub favorite: String,
}

impl Default for ThemeSettings {
  fn default() -> Self {
    Self {
      focused_border: "Cyan".to_owned(),
      now_playing: "Green".to_owned(),
      selection: "Cyan".to_owned(),
      favorite: "Magenta".to_owned(),
    }
  }
}

#[derive(Default, Deserialize, Serialize)]
struct StateFile {
  #[serde(default = "default_volume")]
  volume_percent: u8,
  #[serde(default)]
  radio_stations: Vec<Station>,
  #[serde(default)]
  theme: Option<ThemeSettings>,
}

fn default_volume() -> u8 {
  80
}

pub struct LoadedConfig {
  pub stations: Vec<Station>,
  pub volume_percent: u8,
  pub theme: ThemeSettings,
}

pub fn default_config_path() -> Result<PathBuf> {
  dirs::config_dir()
    .map(|dir| dir.join("degen-radio/config.yml"))
    .context("cannot resolve the user configuration directory")
}

pub fn default_state_path() -> Result<PathBuf> {
  dirs::state_dir()
    .map(|dir| dir.join("degen-radio/state.yml"))
    .context("cannot resolve the user state directory")
}

fn legacy_config_path() -> Result<PathBuf> {
  dirs::config_dir()
    .map(|dir| dir.join("spotatui/config.yml"))
    .context("cannot resolve the user configuration directory")
}

fn legacy_state_path() -> Result<PathBuf> {
  dirs::state_dir()
    .map(|dir| dir.join("spotatui/state.yml"))
    .context("cannot resolve the user state directory")
}

fn prefer_existing(primary: PathBuf, legacy: PathBuf) -> PathBuf {
  if primary.exists() || !legacy.exists() {
    primary
  } else {
    legacy
  }
}

pub fn load(config_override: Option<&Path>) -> Result<LoadedConfig> {
  let config_path = match config_override {
    Some(path) => path.to_path_buf(),
    None => prefer_existing(default_config_path()?, legacy_config_path()?),
  };
  let state_path = prefer_existing(default_state_path()?, legacy_state_path()?);

  let config: ConfigFile = read_yaml_if_present(&config_path)
    .with_context(|| format!("loading {}", config_path.display()))?;
  let state: StateFile = read_yaml_if_present(&state_path)
    .with_context(|| format!("loading {}", state_path.display()))?;

  let mut seen = HashSet::new();
  let stations = config
    .behavior
    .radio_stations
    .into_iter()
    .chain(state.radio_stations)
    .filter_map(|station| {
      let name = station.name.trim().to_owned();
      let url = station.url.trim().to_owned();
      if name.is_empty() || url.is_empty() || !seen.insert(url.clone()) {
        return None;
      }
      Some(Station { name, url })
    })
    .collect();

  Ok(LoadedConfig {
    stations,
    volume_percent: state.volume_percent.min(100),
    theme: state.theme.unwrap_or(config.theme),
  })
}

pub fn save_preferences(
  stations: &[Station],
  volume_percent: u8,
  theme: &ThemeSettings,
) -> Result<()> {
  save_state_to(&default_state_path()?, stations, volume_percent, theme)
}

fn save_state_to(
  path: &Path,
  stations: &[Station],
  volume_percent: u8,
  theme: &ThemeSettings,
) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
  }
  let content = serde_yaml::to_string(&StateFile {
    volume_percent: volume_percent.min(100),
    radio_stations: stations.to_vec(),
    theme: Some(theme.clone()),
  })
  .context("serializing radio preferences")?;
  let temporary = path.with_extension("yml.tmp");
  std::fs::write(&temporary, content)
    .with_context(|| format!("writing {}", temporary.display()))?;
  std::fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
  Ok(())
}

fn read_yaml_if_present<T>(path: &Path) -> Result<T>
where
  T: Default + for<'de> Deserialize<'de>,
{
  match std::fs::read_to_string(path) {
    Ok(content) if content.trim().is_empty() => Ok(T::default()),
    Ok(content) => serde_yaml::from_str(&content).context("parsing YAML"),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn merge_keeps_config_order_and_deduplicates_urls() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    std::fs::write(
      &config_path,
      "behavior:\n  radio_stations:\n    - name: Configured\n      url: https://example.com/live\n",
    )
    .unwrap();

    let parsed: ConfigFile = read_yaml_if_present(&config_path).unwrap();
    assert_eq!(parsed.behavior.radio_stations[0].name, "Configured");
  }

  #[test]
  fn favorites_round_trip_through_state_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.yml");
    let stations = vec![Station {
      name: "Saved FM".to_owned(),
      url: "https://example.com/live".to_owned(),
    }];

    save_state_to(&path, &stations, 73, &ThemeSettings::default()).unwrap();
    let state: StateFile = read_yaml_if_present(&path).unwrap();

    assert_eq!(state.radio_stations, stations);
    assert_eq!(state.volume_percent, 73);
    assert_eq!(state.theme, Some(ThemeSettings::default()));
  }
}
