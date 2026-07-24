use std::{
  ffi::OsString,
  path::PathBuf,
};

const APP_CONFIG_DIR: &str = "spotatui";
const FALLBACK_CONFIG_DIR: &str = ".config";
const XDG_CONFIG_HOME_ENV: &str = "XDG_CONFIG_HOME";

/// Resolve spotatui's app config directory.
///
/// Uses `$XDG_CONFIG_HOME/spotatui` when XDG_CONFIG_HOME is set to an absolute
/// path, otherwise preserves the historical `$HOME/.config/spotatui` fallback.
pub(crate) fn app_config_dir() -> Option<PathBuf> {
  app_config_dir_from(std::env::var_os(XDG_CONFIG_HOME_ENV), dirs::home_dir())
}

fn app_config_dir_from(xdg_config_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
  xdg_config_home
    .and_then(valid_xdg_config_home)
    .or_else(|| home.map(|home| home.join(FALLBACK_CONFIG_DIR)))
    .map(|config_dir| config_dir.join(APP_CONFIG_DIR))
}

fn valid_xdg_config_home(value: OsString) -> Option<PathBuf> {
  if value.is_empty() {
    return None;
  }

  let path = PathBuf::from(value);
  if path.is_absolute() {
    Some(path)
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::Path;

  #[test]
  fn app_config_dir_uses_absolute_xdg_config_home() {
    let path = app_config_dir_from(
      Some(OsString::from("/tmp/xdg-config")),
      Some(Path::new("/home/alice").to_path_buf()),
    );

    assert_eq!(path, Some(Path::new("/tmp/xdg-config/spotatui").to_path_buf()));
  }

  #[test]
  fn app_config_dir_falls_back_to_home_when_xdg_is_unset() {
    let path = app_config_dir_from(None, Some(Path::new("/home/alice").to_path_buf()));

    assert_eq!(path, Some(Path::new("/home/alice/.config/spotatui").to_path_buf()));
  }

  #[test]
  fn app_config_dir_ignores_empty_or_relative_xdg_config_home() {
    let home = Some(Path::new("/home/alice").to_path_buf());

    assert_eq!(
      app_config_dir_from(Some(OsString::new()), home.clone()),
      Some(Path::new("/home/alice/.config/spotatui").to_path_buf())
    );
    assert_eq!(
      app_config_dir_from(Some(OsString::from("relative")), home),
      Some(Path::new("/home/alice/.config/spotatui").to_path_buf())
    );
  }
}
