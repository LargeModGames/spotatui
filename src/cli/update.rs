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

  let latest = status.get_latest_release().ok()?;
  let latest_version = latest.version.trim_start_matches('v').to_string();

  if latest_version != current_version {
    Some(UpdateInfo {
      current_version: current_version.to_string(),
      latest_version,
    })
  } else {
    None
  }
}

/// Returns the artifact prefix used in release asset names for the current platform.
/// Mirrors the `artifact_prefix` matrix in `.github/workflows/cd.yml`.
fn current_platform_prefix() -> Option<&'static str> {
  match (std::env::consts::OS, std::env::consts::ARCH) {
    ("linux", "x86_64") => Some("linux-x86_64"),
    ("macos", "x86_64") => Some("macos-x86_64"),
    ("macos", "aarch64") => Some("macos-aarch64"),
    ("windows", "x86_64") => Some("windows-x86_64"),
    _ => None,
  }
}

/// Downloads the release asset for this platform, verifies its SHA-256 against the
/// published `.sha256` sidecar, and returns `Err` if the hash does not match or the
/// sidecar is missing.  Called before `status.update()` so a compromised asset is
/// rejected before the binary is replaced.
fn verify_release_checksum(release: &self_update::update::Release) -> Result<()> {
  use sha2::Digest;

  let prefix = current_platform_prefix()
    .ok_or_else(|| anyhow!("unsupported platform, cannot verify update checksum"))?;

  let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
  let asset_name = format!("spotatui-{}.{}", prefix, ext);
  let checksum_name = format!("{}.sha256", asset_name);

  let asset = release
    .assets
    .iter()
    .find(|a| a.name == asset_name)
    .ok_or_else(|| anyhow!("release asset '{}' not found", asset_name))?;

  let checksum_asset = release
    .assets
    .iter()
    .find(|a| a.name == checksum_name)
    .ok_or_else(|| {
      anyhow!(
        "checksum asset '{}' not found — skipping update",
        checksum_name
      )
    })?;

  let client = reqwest::blocking::Client::builder()
    .timeout(std::time::Duration::from_secs(60))
    .build()?;

  let checksum_text = client
    .get(&checksum_asset.download_url)
    .send()?
    .error_for_status()?
    .text()?;

  // Extract the hex digest: shasum/sha256sum produces "<hash>  <filename>",
  // certutil (Windows) produces just the hash on a line by itself.
  let expected_hash = checksum_text
    .lines()
    .find_map(|line| {
      let token = line.split_whitespace().next()?;
      if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(token.to_string())
      } else {
        None
      }
    })
    .ok_or_else(|| anyhow!("could not parse SHA-256 hash from checksum file"))?;

  let binary_bytes = client
    .get(&asset.download_url)
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

/// Convert self_update::Status to UpdateOutcome
fn status_to_outcome(status: self_update::Status) -> UpdateOutcome {
  match status {
    self_update::Status::UpToDate(_) => UpdateOutcome::UpToDate,
    self_update::Status::Updated(v) => UpdateOutcome::Installed(v),
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

  let latest = status.get_latest_release()?;
  let latest_version = latest.version.trim_start_matches('v').to_string();

  if latest_version == current_version {
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
    let result = status.update()?;
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
    let result = status.update()?;
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

  let latest = status.get_latest_release()?;

  // Remove 'v' prefix if present for comparison
  let latest_version = latest.version.trim_start_matches('v');

  if latest_version == current_version {
    println!("✓ You are already running the latest version!");
    return Ok(());
  }

  println!("New version available: v{}", latest_version);

  if do_update {
    println!("\nVerifying checksum and installing update...");

    verify_release_checksum(&latest)?;
    let result = status.update()?;
    match result {
      self_update::Status::UpToDate(_) => {
        println!("✓ Already up to date!");
      }
      self_update::Status::Updated(v) => {
        println!("✓ Successfully updated to v{}!", v);
        println!("\nPlease restart spotatui to use the new version.");
      }
    }
  } else {
    println!("\nRun `spotatui update --install` to install the update.");
  }

  Ok(())
}
