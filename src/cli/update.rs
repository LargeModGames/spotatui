use anyhow::Result;
use self_update::cargo_crate_version;

/// Information about an available update
#[derive(Clone, Debug)]
pub struct UpdateInfo {
  pub current_version: String,
  pub latest_version: String,
}

/// Check for updates in the background (non-blocking)
/// Returns Some(UpdateInfo) if an update is available, None if up to date
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
    println!("\nDownloading and installing update...");

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
