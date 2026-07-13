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

/// The full persisted session: the active non-Spotify playback (if any) plus the
/// native cross-source queue. Wraps [`PersistedPlayback`] so the queue can be
/// persisted independently of whichever source (if any) owns playback.
///
/// `deny_unknown_fields` is load-bearing: [`load`] parses this wrapper *first*,
/// and a legacy file is a bare top-level [`PersistedPlayback`] whose `source`
/// tag key would otherwise be silently ignored here (serde ignores unknown
/// fields by default), yielding an empty session and dropping the saved
/// playback. Rejecting unknown fields forces the legacy file to fail this parse
/// so [`load`] falls through to the legacy path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PersistedSession {
  #[serde(default)]
  pub playback: Option<PersistedPlayback>,
  #[serde(default)]
  pub queue: Vec<TrackInfo>,
}

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
    #[serde(default)]
    repeat: crate::infra::queue::RepeatMode,
    /// The user's shuffle *intent* (`decoded_shuffle`). Stored separately from
    /// `shuffle` because a toggle deferred while a track change is in flight can
    /// leave the two temporarily disagreeing; the restore trusts this flag and
    /// reconciles the queue order to it.
    #[serde(default)]
    shuffle_on: bool,
    /// The applied shuffle permutation, for restoring the original order on
    /// un-shuffle. May be absent while `shuffle_on` is set (deferred toggle).
    #[serde(default)]
    shuffle: Option<crate::infra::queue::ShuffleBackup>,
  },
  /// Subsonic: full track metadata (from the API) plus the queue position.
  Subsonic {
    tracks: Vec<TrackInfo>,
    index: usize,
    position_ms: u64,
    paused: bool,
    #[serde(default)]
    repeat: crate::infra::queue::RepeatMode,
    #[serde(default)]
    shuffle_on: bool,
    #[serde(default)]
    shuffle: Option<crate::infra::queue::ShuffleBackup>,
  },
  /// YouTube: full track metadata (from `yt-dlp`) plus the queue position.
  YouTube {
    tracks: Vec<TrackInfo>,
    index: usize,
    position_ms: u64,
    paused: bool,
    #[serde(default)]
    repeat: crate::infra::queue::RepeatMode,
    #[serde(default)]
    shuffle_on: bool,
    #[serde(default)]
    shuffle: Option<crate::infra::queue::ShuffleBackup>,
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
///
/// Parses the current [`PersistedSession`] wrapper first, then falls back to a
/// legacy top-level [`PersistedPlayback`] (files written before the queue
/// existed), wrapping it with an empty queue. This is an explicit two-step
/// parse — see the note on [`PersistedSession`] for why the ordering is safe.
pub fn load(path: &Path) -> Result<Option<PersistedSession>> {
  match std::fs::read_to_string(path) {
    Ok(contents) => {
      if let Ok(session) = serde_yaml::from_str::<PersistedSession>(&contents) {
        return Ok(Some(session));
      }
      serde_yaml::from_str::<PersistedPlayback>(&contents)
        .map(|playback| {
          Some(PersistedSession {
            playback: Some(playback),
            queue: Vec::new(),
          })
        })
        .with_context(|| format!("malformed session file: {}", path.display()))
    }
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
  }
}

/// Save the session atomically (write a sibling tempfile, then rename) so a
/// crash mid-write can't leave a half-written file that fails to parse.
pub fn save(path: &Path, session: &PersistedSession) -> Result<()> {
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
  use crate::infra::queue::RepeatMode;

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

  fn session(playback: Option<PersistedPlayback>, queue: Vec<TrackInfo>) -> PersistedSession {
    PersistedSession { playback, queue }
  }

  #[test]
  fn missing_file_is_no_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.yml");
    assert_eq!(load(&path).unwrap(), None);
  }

  #[test]
  fn shuffle_intent_round_trips_independently_of_the_backup() {
    // A shuffle toggled while a track change is in flight is deferred, so the
    // intent (`shuffle_on: true`) can be persisted while the permutation
    // (`shuffle`) has not been applied yet. Both must survive independently so
    // the restore trusts the flag and reconciles the order to it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    let s = session(
      Some(PersistedPlayback::Local {
        queue: vec!["file:///a.mp3".to_string(), "file:///b.mp3".to_string()],
        index: 0,
        position_ms: 0,
        paused: false,
        repeat: RepeatMode::Context,
        shuffle_on: true,
        shuffle: None,
      }),
      vec![],
    );
    save(&path, &s).unwrap();
    assert_eq!(load(&path).unwrap(), Some(s));
  }

  #[test]
  fn save_then_load_round_trips_a_youtube_playback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    let s = session(
      Some(PersistedPlayback::YouTube {
        tracks: vec![track("youtube:aaa", "A"), track("youtube:bbb", "B")],
        index: 1,
        position_ms: 42_000,
        paused: true,
        repeat: RepeatMode::Track,
        shuffle_on: false,
        shuffle: None,
      }),
      vec![],
    );
    save(&path, &s).unwrap();
    assert_eq!(load(&path).unwrap(), Some(s));
  }

  #[test]
  fn save_then_load_round_trips_playback_with_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    let s = session(
      Some(PersistedPlayback::Subsonic {
        tracks: vec![track("subsonic:track:1", "Sub")],
        index: 0,
        position_ms: 5_000,
        paused: false,
        repeat: RepeatMode::Off,
        shuffle_on: false,
        shuffle: None,
      }),
      vec![
        track("spotify:track:xyz", "Queued A"),
        track("file:///b.mp3", "Queued B"),
      ],
    );
    save(&path, &s).unwrap();
    assert_eq!(load(&path).unwrap(), Some(s));
  }

  #[test]
  fn save_then_load_round_trips_queue_only_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    // No active playback source, but the native queue has items to persist.
    let s = session(None, vec![track("spotify:track:only", "Just Queued")]);
    save(&path, &s).unwrap();
    let loaded = load(&path).unwrap().expect("should load");
    assert!(loaded.playback.is_none());
    assert_eq!(loaded.queue.len(), 1);
    assert_eq!(loaded.queue[0].name, "Just Queued");
  }

  #[test]
  fn legacy_top_level_playback_file_loads_with_empty_queue() {
    // A file written before the queue existed is a bare top-level enum with a
    // `source` tag; it must still resume playback (and load an empty queue).
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    let legacy = PersistedPlayback::YouTube {
      tracks: vec![
        track("youtube:aaa", "Legacy A"),
        track("youtube:bbb", "Legacy B"),
      ],
      index: 1,
      position_ms: 12_345,
      paused: true,
      repeat: RepeatMode::Off,
      shuffle_on: false,
      shuffle: None,
    };
    let legacy_yaml = serde_yaml::to_string(&legacy).unwrap();
    std::fs::write(&path, legacy_yaml).unwrap();

    let loaded = load(&path).unwrap().expect("legacy file should load");
    assert!(loaded.queue.is_empty());
    // The discriminating assertion: the legacy playback survives the fallback.
    assert_eq!(loaded.playback, Some(legacy));
  }

  #[test]
  fn clear_removes_the_file_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("last_session.yml");
    save(
      &path,
      &session(
        Some(PersistedPlayback::Local {
          queue: vec!["file:///music/a.mp3".to_string()],
          index: 0,
          position_ms: 0,
          paused: false,
          repeat: RepeatMode::Off,
          shuffle_on: false,
          shuffle: None,
        }),
        vec![],
      ),
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
