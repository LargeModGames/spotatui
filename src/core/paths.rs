use std::{ffi::OsString, path::PathBuf};

const APP_DIR: &str = "spotatui";
const FALLBACK_CONFIG_DIR: &str = ".config";
#[cfg(any(feature = "streaming", test))]
const FALLBACK_CACHE_DIR: &str = ".cache";
const FALLBACK_STATE_DIR: &str = ".local/state";
#[cfg(feature = "streaming")]
const XDG_CACHE_HOME_ENV: &str = "XDG_CACHE_HOME";
const XDG_CONFIG_HOME_ENV: &str = "XDG_CONFIG_HOME";
const XDG_STATE_HOME_ENV: &str = "XDG_STATE_HOME";

/// Resolve spotatui's app config directory.
///
/// Uses `$XDG_CONFIG_HOME/spotatui` when XDG_CONFIG_HOME is set to an absolute
/// path, otherwise preserves the historical `$HOME/.config/spotatui` fallback.
pub(crate) fn app_config_dir() -> Option<PathBuf> {
  app_dir_from(
    std::env::var_os(XDG_CONFIG_HOME_ENV),
    dirs::home_dir(),
    FALLBACK_CONFIG_DIR,
  )
}

/// Resolve spotatui's app cache directory.
///
/// Uses `$XDG_CACHE_HOME/spotatui` when XDG_CACHE_HOME is set to an absolute
/// path, otherwise falls back to `$HOME/.cache/spotatui`.
#[cfg(feature = "streaming")]
pub(crate) fn app_cache_dir() -> Option<PathBuf> {
  app_dir_from(
    std::env::var_os(XDG_CACHE_HOME_ENV),
    dirs::home_dir(),
    FALLBACK_CACHE_DIR,
  )
}

/// Resolve spotatui's app state directory.
///
/// Uses `$XDG_STATE_HOME/spotatui` when XDG_STATE_HOME is set to an absolute
/// path, otherwise falls back to `$HOME/.local/state/spotatui`.
pub(crate) fn app_state_dir() -> Option<PathBuf> {
  app_dir_from(
    std::env::var_os(XDG_STATE_HOME_ENV),
    dirs::home_dir(),
    FALLBACK_STATE_DIR,
  )
}

fn app_dir_from(
  xdg_home: Option<OsString>,
  home: Option<PathBuf>,
  fallback_dir: &str,
) -> Option<PathBuf> {
  xdg_home
    .and_then(valid_xdg_home)
    .or_else(|| home.map(|home| home.join(fallback_dir)))
    .map(|dir| dir.join(APP_DIR))
}

fn valid_xdg_home(value: OsString) -> Option<PathBuf> {
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
    let path = app_dir_from(
      Some(OsString::from("/tmp/xdg-config")),
      Some(Path::new("/home/alice").to_path_buf()),
      FALLBACK_CONFIG_DIR,
    );

    assert_eq!(
      path,
      Some(Path::new("/tmp/xdg-config/spotatui").to_path_buf())
    );
  }

  #[test]
  fn app_config_dir_falls_back_to_home_when_xdg_is_unset() {
    let path = app_dir_from(
      None,
      Some(Path::new("/home/alice").to_path_buf()),
      FALLBACK_CONFIG_DIR,
    );

    assert_eq!(
      path,
      Some(Path::new("/home/alice/.config/spotatui").to_path_buf())
    );
  }

  #[test]
  fn app_config_dir_ignores_empty_or_relative_xdg_config_home() {
    let home = Some(Path::new("/home/alice").to_path_buf());

    assert_eq!(
      app_dir_from(Some(OsString::new()), home.clone(), FALLBACK_CONFIG_DIR),
      Some(Path::new("/home/alice/.config/spotatui").to_path_buf())
    );
    assert_eq!(
      app_dir_from(Some(OsString::from("relative")), home, FALLBACK_CONFIG_DIR),
      Some(Path::new("/home/alice/.config/spotatui").to_path_buf())
    );
  }

  #[test]
  fn app_cache_dir_uses_xdg_cache_home_or_home_fallback() {
    assert_eq!(
      app_dir_from(
        Some(OsString::from("/tmp/xdg-cache")),
        Some(Path::new("/home/alice").to_path_buf()),
        FALLBACK_CACHE_DIR,
      ),
      Some(Path::new("/tmp/xdg-cache/spotatui").to_path_buf())
    );
    assert_eq!(
      app_dir_from(
        None,
        Some(Path::new("/home/alice").to_path_buf()),
        FALLBACK_CACHE_DIR,
      ),
      Some(Path::new("/home/alice/.cache/spotatui").to_path_buf())
    );
  }

  #[test]
  fn app_state_dir_uses_xdg_state_home_or_home_fallback() {
    assert_eq!(
      app_dir_from(
        Some(OsString::from("/tmp/xdg-state")),
        Some(Path::new("/home/alice").to_path_buf()),
        FALLBACK_STATE_DIR,
      ),
      Some(Path::new("/tmp/xdg-state/spotatui").to_path_buf())
    );
    assert_eq!(
      app_dir_from(
        None,
        Some(Path::new("/home/alice").to_path_buf()),
        FALLBACK_STATE_DIR,
      ),
      Some(Path::new("/home/alice/.local/state/spotatui").to_path_buf())
    );
  }
}
