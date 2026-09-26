//! The playlist-sync state file: `playlist_sync.yml` in the app state dir.
//!
//! Whole-file load and save. A missing or blank file is an empty link list; a
//! malformed one is an error the caller reports, and the file is left exactly
//! as the user wrote it. The save writes a sibling tempfile and renames it, so
//! a crash mid-write cannot leave a half-written file behind.

use super::{Endpoint, Link, Mirror};
use anyhow::{anyhow, Context, Result};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "playlist_sync.yml";

/// Environment override for the playlist-sync file location (used by tests, and
/// available to users who keep their state elsewhere).
pub const PATH_ENV: &str = "SPOTATUI_PLAYLIST_SYNC_PATH";

/// The schema this build writes, and the highest it can read.
fn file_version() -> u32 {
  1
}

/// The whole on-disk file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistSyncFile {
  #[serde(default = "file_version")]
  pub version: u32,
  #[serde(default)]
  pub links: Vec<Link>,
}

impl Default for PlaylistSyncFile {
  fn default() -> Self {
    PlaylistSyncFile {
      version: file_version(),
      links: Vec::new(),
    }
  }
}

impl PlaylistSyncFile {
  /// Register `mirror` under `master`: on the link that already has this master,
  /// else on a new link. Returns the link id.
  pub fn link(&mut self, master: Endpoint, mirror: Endpoint) -> String {
    if let Some(existing) = self.links.iter_mut().find(|link| {
      link.master.source == master.source && link.master.playlist_uri == master.playlist_uri
    }) {
      existing.mirrors.push(Mirror::new(mirror));
      return existing.id.clone();
    }
    let id = self.new_link_id();
    self.links.push(Link {
      id: id.clone(),
      master,
      mirrors: vec![Mirror::new(mirror)],
    });
    id
  }

  /// Twelve lowercase base-36 characters no other link uses.
  fn new_link_id(&self) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    loop {
      let id: String = (0..12)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect();
      if !self.links.iter().any(|link| link.id == id) {
        return id;
      }
    }
  }

  /// Drop the link with `id`; `false` when there is none.
  pub fn remove_link(&mut self, id: &str) -> bool {
    let before = self.links.len();
    self.links.retain(|link| link.id != id);
    self.links.len() != before
  }
}

/// Location of the file: `$SPOTATUI_PLAYLIST_SYNC_PATH` when set, else
/// `<state dir>/playlist_sync.yml`.
pub fn default_path() -> Result<PathBuf> {
  default_path_with(
    std::env::var(PATH_ENV).ok(),
    crate::core::paths::app_state_dir(),
  )
}

/// [`default_path`] with the environment value and the state dir passed in.
fn default_path_with(env_value: Option<String>, state_dir: Option<PathBuf>) -> Result<PathBuf> {
  if let Some(path) = env_value.filter(|value| !value.trim().is_empty()) {
    return Ok(PathBuf::from(path));
  }
  state_dir
    .map(|dir| dir.join(FILE_NAME))
    .ok_or_else(|| anyhow!("cannot resolve the spotatui state directory"))
}

/// Load the file: missing or blank is an empty link list, malformed is an error.
pub fn load(path: &Path) -> Result<PlaylistSyncFile> {
  let contents = match std::fs::read_to_string(path) {
    Ok(contents) => contents,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(PlaylistSyncFile::default()),
    Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
  };
  if contents.trim().is_empty() {
    return Ok(PlaylistSyncFile::default());
  }
  let file: PlaylistSyncFile = serde_yaml::from_str(&contents)
    .with_context(|| format!("malformed playlist sync file: {}", path.display()))?;
  if file.version > file_version() {
    return Err(anyhow!(
      "{} was written by a newer spotatui (file version {})",
      path.display(),
      file.version
    ));
  }
  Ok(file)
}

/// Save the whole file through a process-unique tempfile and a rename.
pub fn save(path: &Path, file: &PlaylistSyncFile) -> Result<()> {
  let yaml = serde_yaml::to_string(file).context("serializing playlist sync links")?;
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
  }
  crate::core::auth::write_private_file_atomic(path, yaml.as_bytes())
    .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::playlist_sync::{Endpoint, Mirror, UnmatchReason, Unmatched};
  use crate::core::source::Source;
  use std::collections::BTreeMap;

  fn endpoint(source: Source, uri: &str) -> Endpoint {
    Endpoint {
      source,
      playlist_uri: uri.to_string(),
      name: "Road Trip".to_string(),
    }
  }

  fn sample_link() -> Link {
    let mut mirror = Mirror {
      endpoint: Endpoint {
        source: Source::Qobuz,
        playlist_uri: "qobuz:playlist:99".to_string(),
        name: "Road Trip".to_string(),
      },
      matches: BTreeMap::new(),
      unmatched: Vec::new(),
      last_run: None,
    };
    mirror.record_match("spotify:track:a", "1234");
    mirror.finish_run(
      vec![
        Unmatched {
          master_key: "spotify:track:b".to_string(),
          title: "Creep".to_string(),
          artist: "Radiohead".to_string(),
          reason: UnmatchReason::NoCandidate,
        },
        Unmatched {
          master_key: "spotify:track:c".to_string(),
          title: "Nude".to_string(),
          artist: "Radiohead".to_string(),
          reason: UnmatchReason::SearchFailed("429 rate limited".to_string()),
        },
      ],
      "2026-09-16T10:00:00Z".to_string(),
    );
    Link {
      id: "abc123".to_string(),
      master: Endpoint {
        source: Source::Spotify,
        playlist_uri: "spotify:playlist:1".to_string(),
        name: "Road Trip".to_string(),
      },
      mirrors: vec![mirror],
    }
  }

  #[test]
  fn missing_file_is_an_empty_link_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    assert_eq!(load(&path).unwrap(), PlaylistSyncFile::default());
    assert!(load(&path).unwrap().links.is_empty());
  }

  #[test]
  fn a_blank_file_is_an_empty_link_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, "   \n").unwrap();
    assert_eq!(load(&path).unwrap(), PlaylistSyncFile::default());
  }

  #[test]
  fn save_then_load_round_trips_a_link_with_matches_and_unmatched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state").join(FILE_NAME);
    let file = PlaylistSyncFile {
      version: file_version(),
      links: vec![sample_link()],
    };

    save(&path, &file).unwrap();
    assert_eq!(load(&path).unwrap(), file);
    assert_eq!(
      std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
      1
    );
  }

  #[test]
  fn malformed_file_is_an_error_and_leaves_the_file_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let broken = "links: [ this is not : valid";
    std::fs::write(&path, broken).unwrap();

    assert!(load(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
  }

  #[test]
  fn a_file_from_a_newer_version_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    std::fs::write(&path, "version: 99\nlinks: []\n").unwrap();
    assert!(load(&path).is_err());
  }

  #[test]
  fn a_hand_written_file_without_a_version_loads_as_version_one() {
    // A hand-edited file omits the version and every field a run fills in.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE_NAME);
    let yaml = r#"
links:
  - id: abc
    master:
      source: Spotify
      playlist_uri: "spotify:playlist:1"
      name: Mine
    mirrors:
      - endpoint:
          source: Qobuz
          playlist_uri: "qobuz:playlist:9"
          name: Mine
  - id: def
    master:
      source: Subsonic
      playlist_uri: "subsonic:playlist:7"
      name: Other
"#;
    std::fs::write(&path, yaml).unwrap();

    let file = load(&path).unwrap();
    assert_eq!(file.version, file_version());
    let mirror = &file.links[0].mirrors[0];
    assert!(mirror.matches.is_empty());
    assert!(mirror.unmatched.is_empty());
    assert_eq!(mirror.last_run, None);
    assert!(file.links[1].mirrors.is_empty());
  }

  #[test]
  fn link_appends_a_mirror_to_the_existing_master_or_opens_a_new_link() {
    let mut file = PlaylistSyncFile::default();
    let master = endpoint(Source::Spotify, "spotify:playlist:1");
    let other = endpoint(Source::Spotify, "spotify:playlist:2");
    let qobuz = endpoint(Source::Qobuz, "qobuz:playlist:9");
    let subsonic = endpoint(Source::Subsonic, "subsonic:playlist:7");

    let first = file.link(master.clone(), qobuz);
    let second = file.link(master, subsonic);
    let third = file.link(other, endpoint(Source::Qobuz, "qobuz:playlist:8"));

    assert_eq!(first, second);
    assert_ne!(first, third);
    assert_eq!(first.len(), 12);
    assert_eq!(third.len(), 12);
    assert_eq!(file.links.len(), 2);
    assert_eq!(file.links[0].mirrors.len(), 2);
    assert_eq!(file.links[1].mirrors.len(), 1);
    assert!(file.links[0].mirrors[0].matches.is_empty());
    assert_eq!(file.links[0].mirrors[0].last_run, None);
  }

  #[test]
  fn remove_link_drops_only_the_named_link() {
    let mut file = PlaylistSyncFile {
      version: file_version(),
      links: vec![sample_link()],
    };

    assert!(!file.remove_link("no-such-link"));
    assert_eq!(file.links.len(), 1);
    assert!(file.remove_link("abc123"));
    assert!(file.links.is_empty());
  }

  #[test]
  fn the_env_override_wins_and_a_blank_one_falls_back_to_the_state_dir() {
    let state = PathBuf::from("state-dir");

    let overridden = default_path_with(Some("other/links.yml".to_string()), Some(state.clone()));
    assert_eq!(overridden.unwrap(), PathBuf::from("other/links.yml"));

    let blank = default_path_with(Some("   ".to_string()), Some(state.clone()));
    assert_eq!(blank.unwrap(), state.join(FILE_NAME));

    let unset = default_path_with(None, Some(state.clone()));
    assert_eq!(unset.unwrap(), state.join(FILE_NAME));

    assert!(default_path_with(None, None).is_err());
  }
}
