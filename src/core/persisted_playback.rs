//! Persist the last non-Spotify playback session so it survives restarts.
//!
//! The browse-scope [`Source`](crate::core::source::Source) is already persisted
//! in `config.yml` (see `BehaviorConfig::active_source`). What this module adds
//! is the *playback* side: which track/queue was playing, where in it, and
//! whether it was paused — so that after a restart the app can resume the exact
//! song on the same source.
//!
//! ## Why a side file and not `config.yml`
//!
//! A playback session is a machine-written, frequently-updated blob (a queue of
//! [`TrackInfo`], an index, a live position). Serializing that into the
//! hand-editable `config.yml` would bury the user's real settings under churning
//! metadata. So it lives in its own `last_session.yml` next to the app config,
//! mirroring the `youtube_playlists.yml` precedent.
//!
//! ## Per-source shape
//!
//! The sources differ in what they need to reconstruct playback, so this is a
//! tagged enum rather than one struct that fits none of them:
//! - **Local** re-reads tags from disk, so only the `file://` URI queue + index
//!   are needed.
//! - **Subsonic / YouTube** got their metadata from a remote API / `yt-dlp`; it
//!   cannot be re-derived offline, so the full [`TrackInfo`] list is stored.
//! - **Radio** is one infinite stream — no queue, just the station row to
//!   reconnect to.

use crate::core::plugin_api::TrackInfo;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "last_session.yml";

/// Environment override for the session file location (used by tests, and
/// available to users who keep their config elsewhere).
pub const PATH_ENV: &str = "SPOTATUI_LAST_SESSION_PATH";

/// A snapshot of the non-Spotify playback session, enough to resume it on the
/// next launch. One variant per source; the discriminant doubles as the source
/// marker (see [`PersistedPlayback::source`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source")]
pub enum PersistedPlayback {
  /// Local files: the `file://` URI queue is enough — tags are re-read from
  /// disk when the queue restarts.
  Local {
    queue: Vec<String>,
    index: usize,
    position_ms: u64,
    paused: bool,
  },
  /// Subsonic: full track metadata (from the API) plus the queue position.
  Subsonic {
    tracks: Vec<TrackInfo>,
    index: usize,
    position_ms: u64,
    paused: bool,
  },
  /// YouTube: full track metadata (from `yt-dlp`) plus the queue position.
  YouTube {
    tracks: Vec<TrackInfo>,
    index: usize,
    position_ms: u64,
    paused: bool,
  },
  /// Radio: a single station to reconnect to. A stream has no seekable
  /// position, so none is stored.
  Radio { station: TrackInfo, paused: bool },
}

/// Location of the session file: `$SPOTATUI_LAST_SESSION_PATH` when set, else
/// `<config dir>/last_session.yml` next to the app config.
pub fn default_session_path() -> Result<PathBuf> {
  if let Ok(path) = std::env::var(PATH_ENV) {
    return Ok(PathBuf::from(path));
  }
  crate::core::user_config::default_app_config_dir()
    .map(|dir| dir.join(FILE_NAME))
    .ok_or_else(|| anyhow!("cannot resolve the spotatui config directory"))
}

/// Load the persisted session. A missing file means "no session to resume"
/// (`Ok(None)`); a malformed file is an error the caller logs and ignores
/// (never crash startup over an auto-written file).
pub fn load(path: &Path) -> Result<Option<PersistedPlayback>> {
  match std::fs::read_to_string(path) {
    Ok(contents) => serde_yaml::from_str(&contents)
      .map(Some)
      .with_context(|| format!("malformed session file: {}", path.display())),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
  }
}

/// Save the session atomically (write a sibling tempfile, then rename) so a
/// crash mid-write can't leave a half-written file that fails to parse.
pub fn save(path: &Path, session: &PersistedPlayback) -> Result<()> {
  let yaml = serde_yaml::to_string(session).context("serializing playback session")?;
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
  }
  let tmp = path.with_extension("yml.tmp");
  std::fs::write(&tmp, yaml).with_context(|| format!("writing {}", tmp.display()))?;
  std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
  Ok(())
}

/// Remove the session file. A missing file is not an error — clearing an
/// already-absent session is a no-op.
pub fn clear(path: &Path) -> Result<()> {
  match std::fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(e).with_context(|| format!("removing {}", path.display())),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn track(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(uri.to_string()),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1000,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  #[test]
  fn missing_file_is_no_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.yml");
    assert_eq!(load(&path).unwrap(), None);
  }

  #[test]
  fn save_then_load_round_trips_a_youtube_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    let session = PersistedPlayback::YouTube {
      tracks: vec![track("youtube:aaa", "A"), track("youtube:bbb", "B")],
      index: 1,
      position_ms: 42_000,
      paused: true,
    };
    save(&path, &session).unwrap();
    assert_eq!(load(&path).unwrap(), Some(session));
  }

  #[test]
  fn clear_removes_the_file_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    save(
      &path,
      &PersistedPlayback::Local {
        queue: vec!["file:///music/a.mp3".to_string()],
        index: 0,
        position_ms: 0,
        paused: false,
      },
    )
    .unwrap();
    assert!(path.exists());
    clear(&path).unwrap();
    assert!(!path.exists());
    // Clearing an absent file is a no-op, not an error.
    clear(&path).unwrap();
  }

  #[test]
  fn malformed_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    std::fs::write(&path, "this: is: not: valid: yaml: for: our: enum").unwrap();
    assert!(load(&path).is_err());
  }
}
