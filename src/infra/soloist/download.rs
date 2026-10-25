//! Gets the official Soloist build for this system from Spotify's CDN when the user asks.
//! spotatui never hosts or bundles the binary.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::core::app::App;

const CDN: &str = "https://soloist-builds.spotifycdn.com/";
const BINARY: &str = if cfg!(windows) {
  "soloist.exe"
} else {
  "soloist"
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
  /// Spotify publishes no build for this OS and CPU.
  NoBuild,
  Failed(String),
}

fn failed(error: impl std::fmt::Display) -> Failure {
  Failure::Failed(error.to_string())
}

/// The archive Spotify publishes for `os` and `arch`, in `std::env::consts` names.
pub fn archive_name(os: &str, arch: &str) -> Option<&'static str> {
  match (os, arch) {
    ("linux", "x86_64") => Some("soloist_release_x86_64.tar.gz"),
    ("linux", "aarch64") => Some("soloist_release_arm64.tar.gz"),
    ("linux", "arm") => Some("soloist_release_arm32.tar.gz"),
    ("macos", "aarch64") => Some("soloist_release_osx_arm64.tar.gz"),
    ("windows", "x86_64") => Some("soloist_release_windows_x86_64.zip"),
    _ => None,
  }
}

/// The share of `total` in whole percent; `None` when the size is unknown.
fn percent(received: u64, total: Option<u64>) -> Option<u8> {
  let total = total.filter(|total| *total > 0)?;
  Some((received.min(total) * 100 / total) as u8)
}

/// The binary in an extracted archive, at most three folders down.
fn find_binary(dir: &Path, depth: u8) -> Option<PathBuf> {
  let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
  if let Some(entry) = entries
    .iter()
    .find(|entry| entry.file_name() == BINARY && entry.path().is_file())
  {
    return Some(entry.path());
  }
  if depth == 0 {
    return None;
  }
  entries
    .iter()
    .filter(|entry| entry.path().is_dir())
    .find_map(|entry| find_binary(&entry.path(), depth - 1))
}

/// Download, extract, and return the new binary; the App then saves its path.
pub async fn run(app: Arc<Mutex<App>>) {
  let result = install(&app).await;
  if let Err(Failure::Failed(error)) = &result {
    log::warn!("soloist: download failed: {error}");
  }
  let start = app.lock().await.soloist_download_finished(result);
  if let Some(start) = start {
    super::startup::deferred_soloist_startup(app, start);
  }
}

async fn install(app: &Arc<Mutex<App>>) -> Result<PathBuf, Failure> {
  let name = archive_name(std::env::consts::OS, std::env::consts::ARCH).ok_or(Failure::NoBuild)?;
  let root = crate::core::paths::soloist_bin_dir().ok_or_else(|| failed("no state directory"))?;
  let stamp = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|since| since.as_secs())
    .unwrap_or_default();
  // A new folder per build: Windows cannot replace a running soloist.exe.
  let dir = root.join(stamp.to_string());
  tokio::fs::create_dir_all(&dir).await.map_err(failed)?;
  let archive = root.join(format!(".{name}"));
  let result = async {
    fetch(app, &format!("{CDN}{name}"), &archive).await?;
    extract(&archive, &dir).await?;
    find_binary(&dir, 3).ok_or_else(|| failed(format!("no {BINARY} in {name}")))
  }
  .await;
  let _ = tokio::fs::remove_file(&archive).await;
  match &result {
    Ok(binary) => {
      log::info!("soloist: downloaded {name} to {}", binary.display());
      remove_other_builds(&root, &dir);
    }
    Err(_) => {
      let _ = tokio::fs::remove_dir_all(&dir).await;
    }
  }
  result
}

async fn fetch(app: &Arc<Mutex<App>>, url: &str, to: &Path) -> Result<(), Failure> {
  let client = reqwest::Client::builder()
    .user_agent(concat!("spotatui/", env!("CARGO_PKG_VERSION")))
    .connect_timeout(Duration::from_secs(15))
    .read_timeout(Duration::from_secs(30))
    .build()
    .map_err(failed)?;
  let mut response = client
    .get(url)
    .send()
    .await
    .and_then(reqwest::Response::error_for_status)
    .map_err(failed)?;
  let total = response.content_length();
  let mut file = tokio::fs::File::create(to).await.map_err(failed)?;
  let mut received = 0;
  let mut shown = 0;
  while let Some(chunk) = response.chunk().await.map_err(failed)? {
    file.write_all(&chunk).await.map_err(failed)?;
    received += chunk.len() as u64;
    if let Some(now) = percent(received, total).filter(|now| *now >= shown + 10) {
      shown = now;
      app.lock().await.soloist_download_progress(now);
    }
  }
  file.flush().await.map_err(failed)
}

/// The system tar reads both archive kinds: Windows 10 and later ship bsdtar, which reads zip.
async fn extract(archive: &Path, into: &Path) -> Result<(), Failure> {
  // A GNU tar earlier on PATH (Git for Windows) cannot read zip.
  #[cfg(windows)]
  let program = std::env::var_os("SystemRoot")
    .map(|root| Path::new(&root).join("System32").join("tar.exe"))
    .unwrap_or_else(|| PathBuf::from("tar"));
  #[cfg(not(windows))]
  let program = PathBuf::from("tar");
  let mut command = tokio::process::Command::new(program);
  command
    .arg("-xf")
    .arg(archive)
    .arg("-C")
    .arg(into)
    .stdin(Stdio::null())
    .kill_on_drop(true);
  #[cfg(windows)]
  command.creation_flags(super::supervisor::CREATE_NO_WINDOW);
  let output = command.output().await.map_err(failed)?;
  if output.status.success() {
    return Ok(());
  }
  let stderr = String::from_utf8_lossy(&output.stderr);
  Err(failed(format!("tar: {}", stderr.trim())))
}

/// Older downloads; one that still runs on Windows stays until the next download.
fn remove_other_builds(root: &Path, keep: &Path) {
  let Ok(entries) = std::fs::read_dir(root) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    if path != keep && path.is_dir() {
      let _ = std::fs::remove_dir_all(&path);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn each_published_build_maps_to_its_archive() {
    assert_eq!(
      archive_name("windows", "x86_64"),
      Some("soloist_release_windows_x86_64.zip")
    );
    assert_eq!(
      archive_name("macos", "aarch64"),
      Some("soloist_release_osx_arm64.tar.gz")
    );
    assert_eq!(
      archive_name("linux", "arm"),
      Some("soloist_release_arm32.tar.gz")
    );
    assert_eq!(archive_name("macos", "x86_64"), None);
    assert_eq!(archive_name("windows", "aarch64"), None);
  }

  #[test]
  fn progress_needs_a_known_size_and_stops_at_one_hundred() {
    assert_eq!(percent(50, Some(200)), Some(25));
    assert_eq!(percent(300, Some(200)), Some(100));
    assert_eq!(percent(50, None), None);
    assert_eq!(percent(50, Some(0)), None);
  }

  #[test]
  fn the_binary_is_found_at_the_top_or_in_a_folder() {
    let top = tempfile::tempdir().unwrap();
    std::fs::write(top.path().join("CHANGELOG.md"), "").unwrap();
    std::fs::write(top.path().join(BINARY), "").unwrap();
    assert_eq!(find_binary(top.path(), 3), Some(top.path().join(BINARY)));

    let nested = tempfile::tempdir().unwrap();
    let folder = nested.path().join("soloist_release_x86_64");
    std::fs::create_dir(&folder).unwrap();
    std::fs::create_dir(nested.path().join(BINARY)).unwrap();
    std::fs::write(folder.join(BINARY), "").unwrap();
    assert_eq!(find_binary(nested.path(), 3), Some(folder.join(BINARY)));
    assert_eq!(find_binary(nested.path(), 0), None);
  }

  #[test]
  fn other_builds_are_removed_and_the_new_one_stays() {
    let root = tempfile::tempdir().unwrap();
    let old = root.path().join("100");
    let new = root.path().join("200");
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(&new).unwrap();
    remove_other_builds(root.path(), &new);
    assert!(!old.exists());
    assert!(new.exists());
  }
}
