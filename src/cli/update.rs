use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use self_update::cargo_crate_version;
use serde::{Deserialize, Serialize};

/// Information about an available update
#[allow(dead_code)]
pub struct UpdateInfo {
  pub current_version: String,
  pub latest_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdatePendingState {
  pending_version: String,
  detected_at_secs: u64,
}

fn update_state_file_path() -> Option<PathBuf> {
  use crate::core::user_config::UserConfig;
  UserConfig::get_app_config_dir().map(|dir| dir.join("update_pending.json"))
}

pub enum UpdateOutcome {
  /// Update was installed; contains the new version string
  Installed(String),
  /// Update is available but waiting for the delay to elapse
  Pending {
    version: String,
    secs_remaining: u64,
  },
  /// Already up to date
  UpToDate,
}

/// Check for updates in the background (non-blocking)
/// Returns Some(UpdateInfo) if an update is available, None if up to date
#[allow(dead_code)]
pub fn check_for_update_silent() -> Option<UpdateInfo> {
  // ============ TESTING: Uncomment below to simulate update ============
  // return Some(UpdateInfo {
  //   current_version: env!("CARGO_PKG_VERSION").to_string(),
  //   latest_version: "99.0.0".to_string(),
  // });
  // =====================================================================

  let current_version = cargo_crate_version!();

  let status = self_update::backends::github::Update::configure()
    .repo_owner("LargeModGames")
    .repo_name("spotatui")
    .bin_name("spotatui")
    .current_version(current_version)
    .build()
    .ok()?;

  let latest = latest_release(&status).ok()?;
  let latest_version = latest.version().trim_start_matches('v').to_string();

  if latest_version != current_version {
    Some(UpdateInfo {
      current_version: current_version.to_string(),
      latest_version,
    })
  } else {
    None
  }
}

/// Returns the release archive name for the current platform.
fn current_asset_name() -> Option<String> {
  asset_name(std::env::consts::OS, std::env::consts::ARCH)
}

/// The archive cd.yml publishes for (OS, arch), named after its `artifact_prefix` matrix.
fn asset_name(os: &str, arch: &str) -> Option<String> {
  let ext = if os == "windows" { "zip" } else { "tar.gz" };
  platform_prefix(os, arch).map(|prefix| format!("spotatui-{prefix}.{ext}"))
}

/// Maps (OS, arch) to the release artifact prefix published by cd.yml; must
/// cover every target that workflow builds.
fn platform_prefix(os: &str, arch: &str) -> Option<&'static str> {
  match (os, arch) {
    ("linux", "x86_64") => Some("linux-x86_64"),
    ("linux", "aarch64") => Some("linux-aarch64"),
    ("macos", "x86_64") => Some("macos-x86_64"),
    ("macos", "aarch64") => Some("macos-aarch64"),
    ("windows", "x86_64") => Some("windows-x86_64"),
    _ => None,
  }
}

/// The release file that lists the SHA-256 of every download, in `sha256sum` format.
const CHECKSUMS_ASSET: &str = "checksums.txt";

/// Finds the hash of `asset_name` in a `sha256sum` listing, text or binary mode.
fn checksum_for(listing: &str, asset_name: &str) -> Option<String> {
  listing.lines().find_map(|line| {
    let (hash, name) = line.trim_end().split_once(char::is_whitespace)?;
    let name = name.trim_start().trim_start_matches('*');
    (name == asset_name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
      .then(|| hash.to_ascii_lowercase())
  })
}

/// The pinned update skips self_update's own version check, so an rc or dev build must not be downgraded.
fn is_newer(current: &str, latest: &str) -> bool {
  self_update::version::bump_is_greater(current, latest).unwrap_or(false)
}

fn latest_release(
  updater: &self_update::backends::github::Update,
) -> Result<self_update::update::Release> {
  updater
    .get_latest_release()?
    .latest()
    .cloned()
    .ok_or_else(|| anyhow!("no release found"))
}

/// Pinned to `tag`, because an unpinned self_update can pick a prerelease nobody checked.
fn pinned_updater(
  tag: &str,
  current_version: &str,
  interactive: bool,
) -> Result<self_update::backends::github::Update> {
  // self_update matches `darwin` for macOS, but cd.yml publishes `macos` archives.
  let asset_name =
    current_asset_name().ok_or_else(|| anyhow!("unsupported platform, cannot update"))?;
  Ok(
    self_update::backends::github::Update::configure()
      .repo_owner("LargeModGames")
      .repo_name("spotatui")
      .bin_name("spotatui")
      .show_download_progress(interactive)
      .no_confirm(!interactive)
      .current_version(current_version)
      .release_tag(format!("v{}", tag.trim_start_matches('v')))
      .asset_matcher(move |assets| assets.iter().find(|a| a.name() == asset_name).cloned())
      .build()?,
  )
}

/// Downloads the release asset for this platform, verifies its SHA-256 against the
/// release's `checksums.txt`, and returns `Err` if the hash does not match or is not
/// listed.  Called before the update so a compromised asset is rejected before the
/// binary is replaced.
fn verify_release_checksum(release: &self_update::update::Release) -> Result<()> {
  use sha2::Digest;

  let asset_name = current_asset_name()
    .ok_or_else(|| anyhow!("unsupported platform, cannot verify update checksum"))?;

  let asset = release
    .assets()
    .iter()
    .find(|a| a.name() == asset_name)
    .ok_or_else(|| anyhow!("release asset '{}' not found", asset_name))?;

  let checksum_asset = release
    .assets()
    .iter()
    .find(|a| a.name() == CHECKSUMS_ASSET)
    .ok_or_else(|| anyhow!("'{}' not found, skipping update", CHECKSUMS_ASSET))?;

  // GitHub's API rejects requests without a User-Agent with 403, and asset URLs
  // return JSON metadata instead of the file unless Accept is application/octet-stream.
  let client = reqwest::blocking::Client::builder()
    .user_agent(concat!("spotatui/", env!("CARGO_PKG_VERSION")))
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

  let checksum_text = client
    .get(checksum_asset.download_url())
    .header(reqwest::header::ACCEPT, "application/octet-stream")
    .send()?
    .error_for_status()?
    .text()?;

  let expected_hash = checksum_for(&checksum_text, &asset_name)
    .ok_or_else(|| anyhow!("'{}' has no SHA-256 for '{}'", CHECKSUMS_ASSET, asset_name))?;

  let binary_bytes = client
    .get(asset.download_url())
    .header(reqwest::header::ACCEPT, "application/octet-stream")
    .send()?
    .error_for_status()?
    .bytes()?;

  let actual_hash = hex::encode(sha2::Sha256::digest(&binary_bytes));

  if actual_hash != expected_hash {
    bail!(
      "checksum mismatch for '{}': expected {}, got {}",
      asset_name,
      expected_hash,
      actual_hash
    );
  }

  Ok(())
}

/// Convert self_update::VersionStatus to UpdateOutcome
fn status_to_outcome(status: self_update::VersionStatus) -> UpdateOutcome {
  match status {
    self_update::VersionStatus::Updated(v) => UpdateOutcome::Installed(v),
    _ => UpdateOutcome::UpToDate,
  }
}

/// Silently check for, download, and install an update, respecting an optional delay.
/// Returns Ok(UpdateOutcome::Installed(v)) if updated, Ok(UpdateOutcome::Pending{..}) if
/// a new version was detected but the delay hasn't elapsed yet, Ok(UpdateOutcome::UpToDate)
/// if already current, or Err on failure.
pub fn install_update_silent(delay_secs: u64) -> Result<UpdateOutcome> {
  let current_version = cargo_crate_version!();

  let status = self_update::backends::github::Update::configure()
    .repo_owner("LargeModGames")
    .repo_name("spotatui")
    .bin_name("spotatui")
    .show_download_progress(false)
    .no_confirm(true)
    .current_version(current_version)
    .build()?;

  let latest = latest_release(&status)?;
  let latest_version = latest.version().trim_start_matches('v').to_string();

  if !is_newer(current_version, &latest_version) {
    // Up to date — clear any stale pending state
    if let Some(path) = update_state_file_path() {
      let _ = std::fs::remove_file(path);
    }
    return Ok(UpdateOutcome::UpToDate);
  }

  // New version available — apply delay logic
  if delay_secs == 0 {
    // No delay: verify checksum then install immediately
    verify_release_checksum(&latest)?;
    let result = pinned_updater(latest.version(), current_version, false)?.update()?;
    return Ok(status_to_outcome(result));
  }

  // Delay > 0: check state file
  let now_secs = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs();

  let state_path = update_state_file_path();
  let mut pending = state_path.as_ref().and_then(|p| {
    std::fs::read_to_string(p)
      .ok()
      .and_then(|s| serde_json::from_str::<UpdatePendingState>(&s).ok())
  });

  // If pending is for a different version (newer release arrived), reset the timer
  if let Some(ref p) = pending {
    if p.pending_version != latest_version {
      pending = None;
    }
  }

  let state = match pending {
    Some(p) => p,
    None => {
      // First detection: write state file and notify
      let s = UpdatePendingState {
        pending_version: latest_version.clone(),
        detected_at_secs: now_secs,
      };
      if let Some(ref path) = state_path {
        if let Some(parent) = path.parent() {
          let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
          path,
          serde_json::to_string(&s).expect("failed to serialize update state"),
        );
      }
      s
    }
  };

  let elapsed = now_secs.saturating_sub(state.detected_at_secs);
  if elapsed >= delay_secs {
    // Delay elapsed — verify checksum then install
    verify_release_checksum(&latest)?;
    let result = pinned_updater(latest.version(), current_version, false)?.update()?;
    // Clean up state file
    if let Some(ref path) = state_path {
      let _ = std::fs::remove_file(path);
    }
    Ok(status_to_outcome(result))
  } else {
    let secs_remaining = delay_secs - elapsed;
    Ok(UpdateOutcome::Pending {
      version: latest_version,
      secs_remaining,
    })
  }
}

/// Check for updates and optionally install the latest version
pub fn check_for_update(do_update: bool) -> Result<()> {
  let current_version = cargo_crate_version!();

  println!("Current version: v{}", current_version);
  println!("Checking for updates...");

  let status = self_update::backends::github::Update::configure()
    .repo_owner("LargeModGames")
    .repo_name("spotatui")
    .bin_name("spotatui")
    .show_download_progress(true)
    .current_version(current_version)
    .no_confirm(false)
    .build()?;

  let latest = latest_release(&status)?;

  // Remove 'v' prefix if present for comparison
  let latest_version = latest.version().trim_start_matches('v');

  if !is_newer(current_version, latest_version) {
    println!("✓ You are already running the latest version!");
    return Ok(());
  }

  println!("New version available: v{}", latest_version);

  if do_update {
    println!("\nVerifying checksum and installing update...");

    verify_release_checksum(&latest)?;
    let result = pinned_updater(latest.version(), current_version, true)?.update()?;
    match result {
      self_update::VersionStatus::Updated(v) => {
        println!("✓ Successfully updated to v{}!", v);
        println!("\nPlease restart spotatui to use the new version.");
      }
      _ => {
        println!("✓ Already up to date!");
      }
    }
  } else {
    println!("\nRun `spotatui update --install` to install the update.");
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::{asset_name, checksum_for, is_newer, platform_prefix};

  #[test]
  fn an_rc_or_dev_build_ahead_of_the_latest_release_is_not_downgraded() {
    assert!(is_newer("0.45.0", "0.46.0"));
    assert!(!is_newer("0.46.0", "0.46.0"));
    assert!(!is_newer("0.47.0-rc1", "0.46.0"));
    assert!(!is_newer("0.47.0", "0.46.0"));
  }

  const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const B: &str = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

  #[test]
  fn checksum_for_picks_the_line_of_the_named_asset_not_the_first_hash() {
    let listing =
      format!("{A}  spotatui-linux-aarch64.tar.gz\n{B}  spotatui-linux-x86_64.tar.gz\n");
    assert_eq!(
      checksum_for(&listing, "spotatui-linux-x86_64.tar.gz"),
      Some(B.to_ascii_lowercase())
    );
  }

  #[test]
  fn checksum_for_reads_binary_mode_and_crlf_lines() {
    let listing = format!("{A} *spotatui-windows-x86_64.zip\r\n");
    assert_eq!(
      checksum_for(&listing, "spotatui-windows-x86_64.zip"),
      Some(A.to_string())
    );
  }

  #[test]
  fn checksum_for_rejects_a_missing_asset_and_a_name_that_only_starts_the_same() {
    let listing = format!(
      "{A}  spotatui-linux-x86_64.tar.gz.sha256\nnot-a-hash  spotatui-linux-x86_64.tar.gz\n"
    );
    assert_eq!(checksum_for(&listing, "spotatui-linux-x86_64.tar.gz"), None);
    assert_eq!(
      checksum_for(&listing, "spotatui-macos-aarch64.tar.gz"),
      None
    );
  }

  #[test]
  fn every_published_release_target_resolves_a_checksum_prefix() {
    // One row per cd.yml artifact_prefix; a target published without a row
    // here makes self-update abort with "unsupported platform" (#440).
    for (os, arch, prefix) in [
      ("linux", "x86_64", "linux-x86_64"),
      ("linux", "aarch64", "linux-aarch64"),
      ("macos", "x86_64", "macos-x86_64"),
      ("macos", "aarch64", "macos-aarch64"),
      ("windows", "x86_64", "windows-x86_64"),
    ] {
      assert_eq!(platform_prefix(os, arch), Some(prefix), "{os}/{arch}");
    }
    assert_eq!(platform_prefix("linux", "arm"), None);
  }

  #[test]
  fn every_release_target_names_the_archive_cd_publishes() {
    for (os, arch, name) in [
      ("linux", "x86_64", "spotatui-linux-x86_64.tar.gz"),
      ("linux", "aarch64", "spotatui-linux-aarch64.tar.gz"),
      ("macos", "x86_64", "spotatui-macos-x86_64.tar.gz"),
      ("macos", "aarch64", "spotatui-macos-aarch64.tar.gz"),
      ("windows", "x86_64", "spotatui-windows-x86_64.zip"),
    ] {
      assert_eq!(asset_name(os, arch).as_deref(), Some(name), "{os}/{arch}");
    }
    assert_eq!(asset_name("linux", "arm"), None);
  }
}
