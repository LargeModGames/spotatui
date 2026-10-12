//! The desktop app's self-update: check, download in the background, install on the way out.

use spotatui::{UpdateRequest, UpdateStatus};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::utils::config::BundleType;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::{mpsc, watch};

/// A downloaded update that waits for the app to quit.
#[derive(Default)]
pub struct Pending {
  ready: Mutex<Option<(Update, Vec<u8>)>>,
  /// The user asked to restart into the new version.
  restart: AtomicBool,
  /// Auto-update is on: any quit installs.
  auto: AtomicBool,
}

impl Pending {
  pub fn is_ready(&self) -> bool {
    self.lock().is_some()
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Option<(Update, Vec<u8>)>> {
    self.ready.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Installs a downloaded update when auto-update is on or the user asked;
  /// true when the app must start again itself (a failed install restarts the old version).
  /// On Windows a good install ends the process, and the installer starts the new version when asked.
  pub fn install(&self) -> bool {
    let restart = self.restart.load(SeqCst);
    if !restart && !self.auto.load(SeqCst) {
      return false;
    }
    if let Some((update, bytes)) = self.lock().take() {
      if let Err(e) = update.restart_after_install(restart).install(bytes) {
        log::warn!("could not install the update: {e}");
      }
    }
    restart
  }
}

/// Answers the app's update requests until the app drops its sender.
pub async fn serve(
  app: AppHandle,
  status: watch::Sender<UpdateStatus>,
  mut requests: mpsc::UnboundedReceiver<UpdateRequest>,
  pending: Arc<Pending>,
) {
  let mut found = None;
  while let Some(request) = requests.recv().await {
    match request {
      UpdateRequest::Check {
        download,
        delay_secs,
      } => {
        pending.auto.store(download, SeqCst);
        if pending.is_ready() {
          continue;
        }
        status.send_replace(UpdateStatus::Checking);
        match check(&app).await {
          // A release is up before its latest.json: no manifest yet is no update yet.
          Ok(None) | Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            status.send_replace(UpdateStatus::UpToDate);
          }
          Ok(Some(update)) => {
            let version = update.version.clone();
            let released = update.date.map(|date| date.unix_timestamp());
            if !installs_itself() {
              status.send_replace(UpdateStatus::Manual { version });
            } else if download && due(released, delay_secs, now_secs()) {
              fetch(update, &status, &pending).await;
            } else {
              found = Some(update);
              status.send_replace(UpdateStatus::Available { version });
            }
          }
          Err(e) => {
            log::warn!("update check failed: {e}");
            status.send_replace(UpdateStatus::Failed {
              message: e.to_string(),
            });
          }
        }
      }
      UpdateRequest::Download => {
        if let Some(update) = found.take() {
          fetch(update, &status, &pending).await;
        }
      }
      UpdateRequest::Install => pending.restart.store(true, SeqCst),
    }
  }
}

async fn check(app: &AppHandle) -> tauri_plugin_updater::Result<Option<Update>> {
  app.updater()?.check().await
}

async fn fetch(update: Update, status: &watch::Sender<UpdateStatus>, pending: &Pending) {
  let version = update.version.clone();
  let mut received = 0u64;
  let downloaded = update
    .download(
      |chunk, total| {
        received += chunk as u64;
        let next = UpdateStatus::Downloading {
          version: version.clone(),
          percent: percent(received, total),
        };
        status.send_if_modified(|current| {
          let changed = *current != next;
          *current = next;
          changed
        });
      },
      || {},
    )
    .await;
  match downloaded {
    Ok(bytes) => {
      *pending.lock() = Some((update, bytes));
      status.send_replace(UpdateStatus::Ready { version });
    }
    Err(e) => {
      log::warn!("update download failed: {e}");
      status.send_replace(UpdateStatus::Failed {
        message: e.to_string(),
      });
    }
  }
}

/// Only an installed bundle replaces itself: never a .deb or .rpm (root owns those) or a dev build.
fn installs_itself() -> bool {
  match tauri::utils::platform::bundle_type() {
    Some(BundleType::Deb | BundleType::Rpm) => false,
    Some(_) => true,
    // A macOS bundle carries no marker: its binary sits inside the .app.
    None => {
      cfg!(target_os = "macos")
        && std::env::current_exe().is_ok_and(|exe| {
          exe
            .ancestors()
            .any(|dir| dir.extension().is_some_and(|ext| ext == "app"))
        })
    }
  }
}

/// Whether a release is old enough to download; one without a date waits for a manual download.
fn due(released: Option<i64>, delay_secs: u64, now: i64) -> bool {
  delay_secs == 0
    || released.is_some_and(|released| {
      now.saturating_sub(released) >= i64::try_from(delay_secs).unwrap_or(i64::MAX)
    })
}

fn percent(received: u64, total: Option<u64>) -> Option<u8> {
  total
    .filter(|total| *total > 0)
    .map(|total| (received.saturating_mul(100) / total).min(100) as u8)
}

fn now_secs() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_release_is_due_once_its_delay_has_passed() {
    assert!(due(None, 0, 1_000));
    assert!(due(Some(1_000), 0, 1_000));
    assert!(!due(Some(1_000), 3_600, 4_599));
    assert!(due(Some(1_000), 3_600, 4_600));
    assert!(!due(None, 3_600, 1_000_000));
  }

  #[test]
  fn the_download_percent_needs_a_known_total() {
    assert_eq!(percent(50, Some(200)), Some(25));
    assert_eq!(percent(300, Some(200)), Some(100));
    assert_eq!(percent(50, None), None);
    assert_eq!(percent(50, Some(0)), None);
  }
}
