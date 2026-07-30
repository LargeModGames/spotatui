use anyhow::{Context, Result};
use std::{
  ffi::OsString,
  path::{Path, PathBuf},
};

const APP_DIR: &str = "spotatui";
const FALLBACK_CONFIG_DIR: [&str; 1] = [".config"];
#[cfg(any(feature = "streaming", test))]
const FALLBACK_CACHE_DIR: [&str; 1] = [".cache"];
const FALLBACK_STATE_DIR: [&str; 2] = [".local", "state"];
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
    &FALLBACK_CONFIG_DIR,
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
    &FALLBACK_CACHE_DIR,
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
    &FALLBACK_STATE_DIR,
  )
}

/// Ensure a directory that stores credentials or other private app data exists
/// and is owner-only where supported.
pub(crate) fn ensure_private_dir(dir: &Path) -> Result<()> {
  std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
      .with_context(|| format!("setting private permissions on {}", dir.display()))?;
  }

  Ok(())
}

fn app_dir_from(
  xdg_home: Option<OsString>,
  home: Option<PathBuf>,
  fallback_dir: &[&str],
) -> Option<PathBuf> {
  xdg_home
    .and_then(valid_xdg_home)
    .or_else(|| {
      home.map(|home| {
        fallback_dir
          .iter()
          .fold(home, |home, component| home.join(component))
      })
    })
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

  fn test_home() -> PathBuf {
    std::env::temp_dir().join("spotatui-home-alice")
  }

  fn test_xdg_home(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
  }

  #[test]
  fn app_config_dir_uses_absolute_xdg_config_home() {
    let xdg_home = test_xdg_home("xdg-config");
    let path = app_dir_from(
      Some(xdg_home.clone().into_os_string()),
      Some(test_home()),
      &FALLBACK_CONFIG_DIR,
    );

    assert_eq!(path, Some(xdg_home.join(APP_DIR)));
  }

  #[test]
  fn app_config_dir_falls_back_to_home_when_xdg_is_unset() {
    let home = test_home();
    let path = app_dir_from(None, Some(home.clone()), &FALLBACK_CONFIG_DIR);

    assert_eq!(path, Some(home.join(".config").join(APP_DIR)));
  }

  #[test]
  fn app_config_dir_ignores_empty_or_relative_xdg_config_home() {
    let home = test_home();

    assert_eq!(
      app_dir_from(
        Some(OsString::new()),
        Some(home.clone()),
        &FALLBACK_CONFIG_DIR
      ),
      Some(home.clone().join(".config").join(APP_DIR))
    );
    assert_eq!(
      app_dir_from(
        Some(OsString::from("relative")),
        Some(home.clone()),
        &FALLBACK_CONFIG_DIR
      ),
      Some(home.join(".config").join(APP_DIR))
    );
  }

  #[test]
  fn app_cache_dir_uses_xdg_cache_home_or_home_fallback() {
    let xdg_home = test_xdg_home("xdg-cache");
    assert_eq!(
      app_dir_from(
        Some(xdg_home.clone().into_os_string()),
        Some(test_home()),
        &FALLBACK_CACHE_DIR,
      ),
      Some(xdg_home.join(APP_DIR))
    );

    let home = test_home();
    assert_eq!(
      app_dir_from(None, Some(home.clone()), &FALLBACK_CACHE_DIR,),
      Some(home.join(".cache").join(APP_DIR))
    );
  }

  #[test]
  fn app_state_dir_uses_xdg_state_home_or_home_fallback() {
    let xdg_home = test_xdg_home("xdg-state");
    assert_eq!(
      app_dir_from(
        Some(xdg_home.clone().into_os_string()),
        Some(test_home()),
        &FALLBACK_STATE_DIR,
      ),
      Some(xdg_home.join(APP_DIR))
    );

    let home = test_home();
    assert_eq!(
      app_dir_from(None, Some(home.clone()), &FALLBACK_STATE_DIR,),
      Some(home.join(".local").join("state").join(APP_DIR))
    );
  }
}
