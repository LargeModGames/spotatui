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
}

#[derive(Default, Deserialize)]
struct Behavior {
  #[serde(default)]
  radio_stations: Vec<Station>,
}

#[derive(Default, Deserialize)]
struct StateFile {
  #[serde(default = "default_volume")]
  volume_percent: u8,
  #[serde(default)]
  radio_stations: Vec<Station>,
}

fn default_volume() -> u8 {
  80
}

pub struct LoadedConfig {
  pub stations: Vec<Station>,
  pub volume_percent: u8,
}

pub fn default_config_path() -> Result<PathBuf> {
  dirs::config_dir()
    .map(|dir| dir.join("spotatui/config.yml"))
    .context("cannot resolve the user configuration directory")
}

pub fn default_state_path() -> Result<PathBuf> {
  dirs::state_dir()
    .map(|dir| dir.join("spotatui/state.yml"))
    .context("cannot resolve the user state directory")
}

pub fn load(config_override: Option<&Path>) -> Result<LoadedConfig> {
  let config_path = match config_override {
    Some(path) => path.to_path_buf(),
    None => default_config_path()?,
  };
  let state_path = default_state_path()?;

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
  })
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
}
