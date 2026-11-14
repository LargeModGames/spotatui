//! Pure decisions around the Soloist child: its argv, spawn errors, exits, restarts.

use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use super::auth::ApiKey;

pub struct SpawnPlan<'a> {
  pub device_name: &'a str,
  pub key: &'a ApiKey,
  pub data_dir: &'a Path,
  pub cache_dir: &'a Path,
  pub initial_volume: u8,
}

/// The cache cap in MB; FLAC fills the cache about five times faster than 160 kbps.
const CACHE_MB: u32 = 2048;

/// Never log this: it holds the API key.
pub fn args(plan: &SpawnPlan) -> Vec<OsString> {
  vec![
    "-n".into(),
    plan.device_name.into(),
    "-k".into(),
    plan.key.expose().into(),
    "-D".into(),
    plan.data_dir.into(),
    "-C".into(),
    plan.cache_dir.into(),
    "-z".into(),
    CACHE_MB.to_string().into(),
    "-i".into(),
    plan.initial_volume.min(100).to_string().into(),
    "-w".into(),
    "127.0.0.1:0".into(),
    "-v".into(),
  ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnFailure {
  Missing,
  NotExecutable,
  WrongCpu,
  Other,
}

/// `ENOEXEC` is 8 on Linux and macOS; macOS reports a foreign CPU as `EBADARCH` (86).
pub fn classify_spawn_error(kind: std::io::ErrorKind, raw_os_error: Option<i32>) -> SpawnFailure {
  match (kind, raw_os_error) {
    (std::io::ErrorKind::NotFound, _) => SpawnFailure::Missing,
    (_, Some(8)) if cfg!(unix) => SpawnFailure::WrongCpu,
    (_, Some(86)) if cfg!(target_os = "macos") => SpawnFailure::WrongCpu,
    (_, Some(193)) if cfg!(windows) => SpawnFailure::WrongCpu,
    (std::io::ErrorKind::PermissionDenied, _) => SpawnFailure::NotExecutable,
    _ => SpawnFailure::Other,
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
  /// spotatui stopped it.
  Stopped,
  Expired,
  DataDirInUse,
  Crashed(Option<i32>),
}

pub fn classify_exit(
  code: Option<i32>,
  shutting_down: bool,
  saw_another_session: bool,
  saw_expired: bool,
) -> Exit {
  if shutting_down {
    return Exit::Stopped;
  }
  if code == Some(10) || saw_expired {
    return Exit::Expired;
  }
  if code == Some(1) && saw_another_session {
    return Exit::DataDirInUse;
  }
  Exit::Crashed(code)
}

const RESTART_WINDOW: Duration = Duration::from_secs(600);

/// The wait before the next restart, given the earlier exits; `None` after three in ten minutes.
pub fn restart_delay(exits: &[Instant], now: Instant) -> Option<Duration> {
  let recent = exits
    .iter()
    .filter(|exit| now.saturating_duration_since(**exit) < RESTART_WINDOW)
    .count();
  match recent {
    0 | 1 => Some(Duration::from_secs(2)),
    2 => Some(Duration::from_secs(5)),
    3 => Some(Duration::from_secs(15)),
    _ => None,
  }
}

/// The pid in `soloist ctl status` output (`running (pid 1234)`), when it runs.
pub fn parse_ctl_status(text: &str) -> Option<u32> {
  let start = text.find("running (pid ")? + "running (pid ".len();
  let rest = &text[start..];
  rest[..rest.find(')')?].trim().parse().ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn key() -> ApiKey {
    super::super::auth::key_with(Some("secret".to_string()), None).unwrap()
  }

  #[test]
  fn soloist_args_bind_the_websocket_to_loopback_port_zero() {
    let key = key();
    let args = args(&SpawnPlan {
      device_name: "spotatui",
      key: &key,
      data_dir: Path::new("data"),
      cache_dir: Path::new("cache"),
      initial_volume: 140,
    });
    let args: Vec<String> = args
      .iter()
      .map(|a| a.to_string_lossy().into_owned())
      .collect();
    let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    assert_eq!(after("-w"), "127.0.0.1:0");
    assert_eq!(after("-n"), "spotatui");
    assert_eq!(after("-D"), "data");
    assert_eq!(after("-C"), "cache");
    assert_eq!(after("-z"), "2048");
    assert_eq!(after("-i"), "100");
    assert!(args.contains(&"-v".to_string()));
  }

  #[test]
  fn spawn_errors_name_the_cause() {
    use std::io::ErrorKind;
    assert_eq!(
      classify_spawn_error(ErrorKind::NotFound, Some(2)),
      SpawnFailure::Missing
    );
    assert_eq!(
      classify_spawn_error(ErrorKind::Other, Some(12345)),
      SpawnFailure::Other
    );
    #[cfg(unix)]
    assert_eq!(
      classify_spawn_error(ErrorKind::PermissionDenied, Some(13)),
      SpawnFailure::NotExecutable
    );
    #[cfg(unix)]
    assert_eq!(
      classify_spawn_error(ErrorKind::Other, Some(8)),
      SpawnFailure::WrongCpu
    );
    #[cfg(windows)]
    assert_eq!(
      classify_spawn_error(ErrorKind::Other, Some(193)),
      SpawnFailure::WrongCpu
    );
  }

  #[test]
  fn exit_code_ten_means_build_expired() {
    assert_eq!(classify_exit(Some(10), false, false, false), Exit::Expired);
    assert_eq!(classify_exit(Some(1), false, false, true), Exit::Expired);
  }

  #[test]
  fn exit_code_one_with_another_session_means_data_dir_in_use() {
    assert_eq!(
      classify_exit(Some(1), false, true, false),
      Exit::DataDirInUse
    );
    assert_eq!(
      classify_exit(Some(1), false, false, false),
      Exit::Crashed(Some(1))
    );
  }

  #[test]
  fn exit_after_shutdown_started_is_not_a_crash() {
    assert_eq!(classify_exit(None, true, false, false), Exit::Stopped);
    assert_eq!(classify_exit(Some(10), true, false, true), Exit::Stopped);
  }

  #[test]
  fn restart_delay_stops_after_three_exits_in_ten_minutes() {
    let now = Instant::now() + Duration::from_secs(3600);
    let at = |ago: u64| now - Duration::from_secs(ago);
    assert_eq!(restart_delay(&[at(1)], now), Some(Duration::from_secs(2)));
    assert_eq!(
      restart_delay(&[at(60), at(1)], now),
      Some(Duration::from_secs(5))
    );
    assert_eq!(
      restart_delay(&[at(120), at(60), at(1)], now),
      Some(Duration::from_secs(15))
    );
    assert_eq!(restart_delay(&[at(180), at(120), at(60), at(1)], now), None);
    assert_eq!(
      restart_delay(&[at(1200), at(900), at(700), at(1)], now),
      Some(Duration::from_secs(2))
    );
  }

  #[test]
  fn ctl_status_reads_the_running_pid() {
    assert_eq!(parse_ctl_status("running (pid 17984)\n"), Some(17984));
    assert_eq!(parse_ctl_status("not running\n"), None);
  }
}
