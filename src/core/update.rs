//! What a desktop shell's updater knows and what the app asks of it.

use serde::{Deserialize, Serialize};

/// The updater's state, as the page shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateStatus {
  #[default]
  Idle,
  Checking,
  UpToDate,
  /// Found, not downloaded: auto-update is off or its delay has not passed.
  Available {
    version: String,
  },
  Downloading {
    version: String,
    percent: Option<u8>,
  },
  /// Downloaded; it installs on the next quit.
  Ready {
    version: String,
  },
  /// This install cannot update itself (a .deb, a .rpm or a dev build): a new download updates it.
  Manual {
    version: String,
  },
  Failed {
    message: String,
  },
}

/// What the page asks the updater to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum UpdateAsk {
  Check,
  Download,
  /// Quit, install the downloaded update and start the new version.
  Install,
}

/// What the app asks the shell's updater to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateRequest {
  /// Look for an update; download it when `download` is set and the release is older than `delay_secs`.
  Check {
    download: bool,
    delay_secs: u64,
  },
  Download,
  /// The app quits next; install on the way out and start the new version.
  Install,
}
