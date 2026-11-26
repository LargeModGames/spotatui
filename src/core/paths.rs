use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::{
  ffi::OsString,
  path::{Path, PathBuf},
};

const APP_DIR: &str = "spotatui";
const FALLBACK_CONFIG_DIR: [&str; 1] = [".config"];
#[cfg(any(feature = "spotify-session", test))]
const FALLBACK_CACHE_DIR: [&str; 1] = [".cache"];
const FALLBACK_STATE_DIR: [&str; 2] = [".local", "state"];
#[cfg(any(feature = "spotify-session", test))]
const XDG_CACHE_HOME_ENV: &str = "XDG_CACHE_HOME";
const XDG_CONFIG_HOME_ENV: &str = "XDG_CONFIG_HOME";
const XDG_STATE_HOME_ENV: &str = "XDG_STATE_HOME";

/// Resolve spotatui's app config directory.
///
/// Uses `$XDG_CONFIG_HOME/spotatui` when XDG_CONFIG_HOME is set to an absolute
/// path, otherwise preserves the historical `$HOME/.config/spotatui` fallback.
/// Unit tests always use a process-private temporary directory instead.
pub(crate) fn app_config_dir() -> Option<PathBuf> {
  app_dir(XDG_CONFIG_HOME_ENV, &FALLBACK_CONFIG_DIR)
}

/// Resolve spotatui's app cache directory.
///
/// Uses `$XDG_CACHE_HOME/spotatui` when XDG_CACHE_HOME is set to an absolute
/// path, otherwise falls back to `$HOME/.cache/spotatui`.
/// Unit tests always use a process-private temporary directory instead.
#[cfg(any(feature = "spotify-session", test))]
pub(crate) fn app_cache_dir() -> Option<PathBuf> {
  app_dir(XDG_CACHE_HOME_ENV, &FALLBACK_CACHE_DIR)
}

/// Resolve spotatui's app state directory.
///
/// Uses `$XDG_STATE_HOME/spotatui` when XDG_STATE_HOME is set to an absolute
/// path, otherwise falls back to `$HOME/.local/state/spotatui`.
/// Unit tests always use a process-private temporary directory instead.
pub(crate) fn app_state_dir() -> Option<PathBuf> {
  app_dir(XDG_STATE_HOME_ENV, &FALLBACK_STATE_DIR)
}

/// Tests never inherit file locations from the shell. File I/O tests pass
/// their own temporary paths directly instead.
pub(crate) fn path_override(env_name: &str) -> Option<OsString> {
  if cfg!(test) {
    None
  } else {
    std::env::var_os(env_name)
  }
}

fn app_dir(xdg_home_env: &str, fallback_dir: &[&str]) -> Option<PathBuf> {
  #[cfg(test)]
  let home = Some(test_root().to_path_buf());
  #[cfg(not(test))]
  let home = dirs::home_dir();

  app_dir_from(path_override(xdg_home_env), home, fallback_dir)
}

/// All implicit test paths share one process-private root, including calls
/// from worker threads and `App::default()`. Never fall back to the real home
/// if creating it fails. Explicit path injection remains available to tests.
#[cfg(test)]
fn test_root() -> &'static Path {
  static ROOT: std::sync::OnceLock<TestRoot> = std::sync::OnceLock::new();
  ROOT
    .get_or_init(|| {
      #[cfg(unix)]
      let parent = {
        // SAFETY: geteuid has no preconditions and does not change process state.
        let uid = unsafe { libc::geteuid() };
        std::env::temp_dir().join(format!("spotatui-tests-{uid}"))
      };
      #[cfg(not(unix))]
      let parent = std::env::temp_dir().join("spotatui-tests");
      std::fs::create_dir_all(&parent).expect("creating the spotatui test parent");
      assert!(
        parent.symlink_metadata().unwrap().is_dir(),
        "the spotatui test parent must be a directory, not a symlink"
      );
      ensure_private_dir(&parent).expect("securing the spotatui test parent");
      clean_stale_test_roots(&parent, std::time::SystemTime::now());
      create_test_root(&parent)
    })
    .dir
    .path()
}

#[cfg(test)]
struct TestRoot {
  // Release the lock before a locally owned TempDir is dropped on Windows.
  _lease: std::fs::File,
  dir: tempfile::TempDir,
}

#[cfg(test)]
fn create_test_root(parent: &Path) -> TestRoot {
  let dir = tempfile::Builder::new()
    .prefix("run-")
    .tempdir_in(parent)
    .expect("creating isolated spotatui test directories");
  let lease = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .create_new(true)
    .open(dir.path().join("active.lock"))
    .expect("creating the spotatui test lease");
  lease.lock().expect("locking the spotatui test lease");
  TestRoot { _lease: lease, dir }
}

/// Statics are not dropped at exit. Reclaim old runs on first use instead,
/// but keep a lease locked for the process lifetime so even a day-long test
/// run survives cleanup by another process. Unreadable entries and unavailable
/// locks are skipped.
#[cfg(test)]
fn clean_stale_test_roots(parent: &Path, now: std::time::SystemTime) {
  let Ok(entries) = std::fs::read_dir(parent) else {
    return;
  };
  for entry in entries.flatten() {
    if !entry.file_name().to_string_lossy().starts_with("run-")
      || !entry.file_type().is_ok_and(|kind| kind.is_dir())
    {
      continue;
    }
    let path = entry.path();
    let marker = path.join("active.lock");
    if !marker.symlink_metadata().is_ok_and(|meta| meta.is_file()) {
      continue;
    }
    let Ok(lease) = std::fs::OpenOptions::new()
      .read(true)
      .write(true)
      .open(marker)
    else {
      continue;
    };
    let age = lease
      .metadata()
      .and_then(|meta| meta.modified())
      .ok()
      .and_then(|modified| now.duration_since(modified).ok());
    if !age.is_some_and(|age| age > std::time::Duration::from_secs(24 * 60 * 60))
      || lease.try_lock().is_err()
    {
      continue;
    }
    // Windows cannot remove the marker while it is open. A released run is
    // never reused; racing cleaners can safely attempt the same removal.
    drop(lease);
    let _ = std::fs::remove_dir_all(path);
  }
}

/// The Spotify app settings file older releases wrote; read once by the migration.
pub(crate) fn legacy_client_config_path() -> Option<PathBuf> {
  app_config_dir().map(|dir| dir.join("client.yml"))
}

/// The Qobuz credentials file: `<config dir>/qobuz_credentials.yml`.
#[cfg(feature = "qobuz")]
pub(crate) fn qobuz_credentials_path() -> Option<PathBuf> {
  app_config_dir().map(|dir| dir.join("qobuz_credentials.yml"))
}

/// The Soloist API key: `<config dir>/soloist_credentials.yml`.
#[cfg(feature = "soloist")]
pub(crate) fn soloist_credentials_path() -> Option<PathBuf> {
  app_config_dir().map(|dir| dir.join("soloist_credentials.yml"))
}

/// Soloist's own data dir (device id, paired session, prefs): `<state dir>/soloist`.
#[cfg(feature = "soloist")]
pub(crate) fn soloist_data_dir() -> Option<PathBuf> {
  app_state_dir().map(|dir| dir.join("soloist"))
}

/// The Soloist builds spotatui downloaded from Spotify: `<state dir>/soloist-bin`.
#[cfg(feature = "soloist")]
pub(crate) fn soloist_bin_dir() -> Option<PathBuf> {
  app_state_dir().map(|dir| dir.join("soloist-bin"))
}

/// Soloist's playback cache: `<cache dir>/soloist`.
#[cfg(feature = "soloist")]
pub(crate) fn soloist_cache_dir() -> Option<PathBuf> {
  app_cache_dir().map(|dir| dir.join("soloist"))
}

/// The user's own MilkDrop presets, scanned at start: `<config dir>/milkdrop`.
#[cfg(feature = "milkdrop")]
pub(crate) fn milkdrop_presets_dir() -> Option<PathBuf> {
  app_config_dir().map(|dir| dir.join("milkdrop"))
}

/// The player-bar visualizer's look: `<config dir>/visualizer.yml`, never written by the app.
#[cfg(feature = "gui")]
pub(crate) fn visualizer_config_path() -> Option<PathBuf> {
  app_config_dir().map(|dir| dir.join("visualizer.yml"))
}

/// Directory holding this run's log file.
///
/// The OS temp directory rather than the state dir: a log file is written per
/// process id, so they accumulate, and temp is the one location the platform
/// clears on its own. Resolved through `std::env::temp_dir` so Windows lands in
/// `%TEMP%` instead of the literal `/tmp` this used to hard-code, which is
/// drive-relative there and left Windows users looking for a path the app
/// reported but their shell could not find.
pub(crate) fn app_log_dir() -> PathBuf {
  static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
  #[cfg(test)]
  let temp = test_root().to_path_buf();
  #[cfg(not(test))]
  let temp = std::env::temp_dir();
  DIR.get_or_init(|| log_dir_in(&temp)).clone()
}

/// `spotatui_logs` when this user owns it, else a fresh one: another account can take that name.
fn log_dir_in(temp: &Path) -> PathBuf {
  let shared = temp.join("spotatui_logs");
  // A symlink first: `ensure_private_dir` follows one and would chmod its target.
  let is_symlink = shared
    .symlink_metadata()
    .is_ok_and(|meta| meta.file_type().is_symlink());
  let owned = !is_symlink
    && ensure_private_dir(&shared).is_ok()
    && shared
      .symlink_metadata()
      .is_ok_and(|meta| meta.file_type().is_dir());
  if owned {
    return shared;
  }
  fresh_private_dir(temp, "spotatui_logs").unwrap_or(shared)
}

/// A new, uniquely named, owner-only directory under `temp`, or `None` when
/// none could be made. A non-recursive create refuses an existing entry, a
/// symlink included, so the result is always one this process made. It is not
/// removed on exit: temp is the one location the platform clears on its own.
pub(crate) fn fresh_private_dir(temp: &Path, prefix: &str) -> Option<PathBuf> {
  // Owner-only from the start, whatever the umask.
  #[cfg(unix)]
  let builder = {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder
  };
  #[cfg(not(unix))]
  let builder = std::fs::DirBuilder::new();
  let pid = std::process::id();
  (0..8).find_map(|attempt| {
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|since| since.subsec_nanos())
      .unwrap_or(0);
    let dir = temp.join(format!("{prefix}-{pid}-{nanos}-{attempt}"));
    builder.create(&dir).ok().map(|()| dir)
  })
}

fn format_log_file_name(now: DateTime<Utc>) -> String {
  // Timestamp-only is an acceptable tradeoff of log collision for code simplicity
  // and UX ease of use
  now.format("spotatui_%Y-%m-%d_%H-%M-%SZ.log").to_string()
}

/// Path of this process's log file. Callers on both the writing side
/// (`setup_logging`) and the reporting side (the help screen) use this so the
/// path shown to a user is always the path actually written.
pub(crate) fn app_log_path() -> PathBuf {
  static LOG_FILENAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
  let filename = LOG_FILENAME.get_or_init(|| {
    let now = Utc::now();
    format_log_file_name(now)
  });
  app_log_dir().join(filename)
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
  use chrono::TimeZone;

  #[cfg(unix)]
  #[test]
  fn unix_test_roots_have_a_user_scoped_private_parent() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let root = test_root();
    let parent = root.parent().unwrap();
    let uid = root.metadata().unwrap().uid();
    assert_eq!(
      parent,
      std::env::temp_dir().join(format!("spotatui-tests-{uid}"))
    );
    let metadata = parent.symlink_metadata().unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.uid(), uid);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
  }

  #[test]
  fn implicit_paths_stay_under_the_test_root_and_outside_production_dirs() {
    // Resolve production paths directly, without the cfg(test) override.
    // Check home fallbacks too, even when this invocation has XDG overrides.
    let production_dirs: Vec<_> = [
      (XDG_CONFIG_HOME_ENV, FALLBACK_CONFIG_DIR.as_slice()),
      (XDG_STATE_HOME_ENV, FALLBACK_STATE_DIR.as_slice()),
      (XDG_CACHE_HOME_ENV, FALLBACK_CACHE_DIR.as_slice()),
    ]
    .into_iter()
    .flat_map(|(env, fallback)| {
      [
        app_dir_from(std::env::var_os(env), dirs::home_dir(), fallback),
        app_dir_from(None, dirs::home_dir(), fallback),
      ]
    })
    .flatten()
    .chain([std::env::temp_dir().join("spotatui_logs")])
    .map(|dir| dir.canonicalize().unwrap_or(dir))
    .collect();
    let root = test_root().canonicalize().unwrap();
    let paths = [
      app_config_dir().unwrap(),
      app_state_dir().unwrap(),
      app_cache_dir().unwrap(),
      app_log_dir(),
      crate::core::state::default_state_path().unwrap(),
      crate::core::persisted_playback::default_session_path().unwrap(),
      crate::core::playlist_sync::store::default_path().unwrap(),
      #[cfg(feature = "youtube")]
      crate::infra::youtube::playlists::default_playlists_path().unwrap(),
      #[cfg(feature = "mcp-server")]
      crate::infra::mcp::control::handshake_path().unwrap(),
    ];
    for path in paths {
      assert!(path.starts_with(test_root()), "{}", path.display());
      let resolved = root.join(path.strip_prefix(test_root()).unwrap());
      for production in &production_dirs {
        assert!(
          !resolved.starts_with(production),
          "{} overlaps production directory {}",
          resolved.display(),
          production.display()
        );
      }
    }
  }

  #[test]
  fn stale_test_roots_are_removed_only_after_their_lease_is_released() {
    let parent = tempfile::tempdir().unwrap();
    let TestRoot { _lease: lease, dir } = create_test_root(parent.path());
    let path = dir.keep();
    let now = std::time::SystemTime::now();
    lease
      .set_modified(now - std::time::Duration::from_secs(2 * 24 * 60 * 60))
      .unwrap();
    std::fs::write(path.join("state.yml"), b"test state").unwrap();

    clean_stale_test_roots(parent.path(), now);
    assert!(path.join("state.yml").is_file());

    drop(lease);
    clean_stale_test_roots(parent.path(), now);
    assert!(!path.exists());
  }

  #[test]
  fn fresh_test_roots_and_unrelated_entries_survive_cleanup() {
    let parent = tempfile::tempdir().unwrap();
    let TestRoot { _lease: lease, dir } = create_test_root(parent.path());
    let fresh = dir.keep();
    let now = lease.metadata().unwrap().modified().unwrap();
    drop(lease);
    let unrelated = parent.path().join("unrelated");
    std::fs::create_dir(&unrelated).unwrap();
    std::fs::write(parent.path().join("run-file"), b"keep").unwrap();
    std::fs::create_dir(parent.path().join("run-without-lease")).unwrap();

    clean_stale_test_roots(
      parent.path(),
      now + std::time::Duration::from_secs(24 * 60 * 60),
    );
    assert!(fresh.is_dir());
    assert!(unrelated.is_dir());
    assert!(parent.path().join("run-file").is_file());
    assert!(parent.path().join("run-without-lease").is_dir());
  }

  #[cfg(unix)]
  #[test]
  fn symlinks_and_their_targets_survive_test_root_cleanup() {
    let parent = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let marker = std::fs::File::create(target.path().join("active.lock")).unwrap();
    let now = std::time::SystemTime::now();
    marker
      .set_modified(now - std::time::Duration::from_secs(2 * 24 * 60 * 60))
      .unwrap();
    std::os::unix::fs::symlink(target.path(), parent.path().join("run-symlink")).unwrap();
    let run = parent.path().join("run-symlink-lease");
    std::fs::create_dir(&run).unwrap();
    std::os::unix::fs::symlink(target.path().join("active.lock"), run.join("active.lock")).unwrap();

    clean_stale_test_roots(parent.path(), now);
    assert!(parent.path().join("run-symlink").is_symlink());
    assert!(run.join("active.lock").is_symlink());
    assert!(target.path().join("active.lock").is_file());
  }

  #[test]
  fn an_owned_log_dir_keeps_the_shared_name() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(log_dir_in(temp.path()), temp.path().join("spotatui_logs"));
  }

  #[test]
  fn a_log_dir_name_that_is_taken_falls_back_to_a_fresh_dir() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("spotatui_logs"), b"").unwrap();
    let dir = log_dir_in(temp.path());
    assert_ne!(dir, temp.path().join("spotatui_logs"));
    assert!(dir.is_dir());
  }

  #[cfg(unix)]
  #[test]
  fn a_planted_log_dir_symlink_leaves_its_target_alone() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(&target, temp.path().join("spotatui_logs")).unwrap();
    let dir = log_dir_in(temp.path());
    assert_ne!(dir, temp.path().join("spotatui_logs"));
    let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755);
  }

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

  #[test]
  fn test_log_filename_format_specifiers() {
    // Use a fixed timestamp where year, month, day, hour, minute, and second
    // are all distinct numbers to catch flipped specifiers (like %m vs %M).
    let fixed_time = Utc.with_ymd_and_hms(2026, 11, 20, 14, 45, 59).unwrap();
    let filename = format_log_file_name(fixed_time);

    // Regress against typos:
    // - %m (month = 11) vs %M (minute = 45)
    // - Missing %S (seconds = 59)
    // - Missing 'Z' UTC indicator
    assert_eq!(filename, "spotatui_2026-11-20_14-45-59Z.log");
  }

  #[test]
  fn test_app_log_path_is_idempotent_and_valid() {
    let path1 = app_log_path();
    let path2 = app_log_path();

    // Must return the exact same PathBuf across calls (OnceLock check)
    assert_eq!(path1, path2);

    // Verify basic structure
    let file_name = path1.file_name().unwrap().to_str().unwrap();
    assert!(file_name.starts_with("spotatui_"));
    assert!(file_name.ends_with(".log"));

    // Verify the timestamp portion actually parses cleanly back to UTC
    let date_part = file_name
      .strip_prefix("spotatui_")
      .unwrap()
      .strip_suffix(".log")
      .unwrap();

    let parsed = chrono::NaiveDateTime::parse_from_str(date_part, "%Y-%m-%d_%H-%M-%SZ");
    assert!(
      parsed.is_ok(),
      "Filename timestamp '{date_part}' does not match expected format: {:?}",
      parsed.err()
    );
  }
}
