//! The playback keys in Soloist's per-user prefs files.
//!
//! Undocumented internal state (the docs say Soloist reads no config file):
//! `audio.play_bitrate_enumeration=5` streams FLAC on 1.3.10, and
//! `audio.normalize_v2=false` turns loudness normalization off, as librespot
//! runs here. The official quality path is the per-device menu in the Spotify
//! mobile app.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const ENUMERATION: &str = "audio.play_bitrate_enumeration";
const NON_METERED: &str = "audio.play_bitrate_non_metered_enumeration";
const MIGRATED: &str = "audio.play_bitrate_non_metered_migrated";
const NORMALIZE: &str = "audio.normalize_v2";
/// The user dirs spotatui already seeded and restarted Soloist for.
const SEEDED_MARKER: &str = ".spotatui-prefs-seeded";

/// `existing` with spotatui's keys set, every other line kept, LF line endings.
pub fn merge_prefs(existing: &str, enumeration: u8) -> String {
  let wanted = [
    (ENUMERATION, enumeration.to_string()),
    (NON_METERED, enumeration.to_string()),
    (MIGRATED, "true".to_string()),
    (NORMALIZE, "false".to_string()),
  ];
  let mut lines: Vec<String> = existing
    .split('\n')
    .map(|line| line.trim_end_matches('\r'))
    .filter(|line| !line.is_empty())
    .filter(|line| {
      let key = line.split('=').next().unwrap_or_default();
      !wanted.iter().any(|(wanted, _)| *wanted == key)
    })
    .map(str::to_string)
    .collect();
  lines.extend(wanted.iter().map(|(key, value)| format!("{key}={value}")));
  let mut out = lines.join("\n");
  out.push('\n');
  out
}

fn users_dir(data_dir: &Path) -> PathBuf {
  data_dir.join("settings").join("Users")
}

/// The account dirs that hold a prefs file.
pub fn user_dirs(data_dir: &Path) -> BTreeSet<String> {
  let Ok(entries) = std::fs::read_dir(users_dir(data_dir)) else {
    return BTreeSet::new();
  };
  entries
    .filter_map(Result::ok)
    .filter(|entry| entry.path().join("prefs").is_file())
    .filter_map(|entry| entry.file_name().into_string().ok())
    .collect()
}

/// Write the quality keys into every account's prefs; returns how many files changed.
pub fn write_quality(data_dir: &Path, enumeration: u8) -> std::io::Result<usize> {
  let mut written = 0;
  for user in user_dirs(data_dir) {
    let path = users_dir(data_dir).join(user).join("prefs");
    let existing = std::fs::read_to_string(&path)?;
    let merged = merge_prefs(&existing, enumeration);
    if merged != existing {
      crate::core::auth::write_private_file_atomic(&path, merged.as_bytes())
        .map_err(std::io::Error::other)?;
      written += 1;
    }
  }
  Ok(written)
}

pub fn seeded(data_dir: &Path) -> BTreeSet<String> {
  std::fs::read_to_string(data_dir.join(SEEDED_MARKER))
    .map(|text| {
      text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
    })
    .unwrap_or_default()
}

pub fn mark_seeded(data_dir: &Path, users: &BTreeSet<String>) -> std::io::Result<()> {
  let text: String = users.iter().map(|user| format!("{user}\n")).collect();
  std::fs::write(data_dir.join(SEEDED_MARKER), text)
}

/// Accounts that appeared after the spawn and still need one restart to pick up the keys.
pub fn unseeded(present: &BTreeSet<String>, seeded: &BTreeSet<String>) -> BTreeSet<String> {
  present.difference(seeded).cloned().collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  const SPIKE_PREFS: &str = "audio.play_bitrate_non_metered_migrated=true\naudio.crossfade_migrated_to_ps=true\naudio.gapless_migrated_to_ps=true\n";

  #[test]
  fn merge_prefs_sets_lossless_and_turns_normalization_off() {
    let merged = merge_prefs(SPIKE_PREFS, 5);
    assert!(merged.contains("audio.play_bitrate_enumeration=5\n"));
    assert!(merged.contains("audio.play_bitrate_non_metered_enumeration=5\n"));
    assert!(merged.contains("audio.normalize_v2=false\n"));
    assert_eq!(
      merged
        .matches("audio.play_bitrate_non_metered_migrated=true")
        .count(),
      1
    );
  }

  #[test]
  fn merge_prefs_keeps_engine_owned_keys() {
    let merged = merge_prefs(SPIKE_PREFS, 5);
    assert!(merged.contains("audio.crossfade_migrated_to_ps=true\n"));
    assert!(merged.contains("audio.gapless_migrated_to_ps=true\n"));
  }

  #[test]
  fn merge_prefs_replaces_an_old_enumeration() {
    let merged = merge_prefs("audio.play_bitrate_enumeration=3\n", 5);
    assert!(!merged.contains("=3"));
    assert!(merged.contains("audio.play_bitrate_enumeration=5"));
  }

  #[test]
  fn merge_prefs_is_idempotent() {
    let once = merge_prefs(SPIKE_PREFS, 5);
    assert_eq!(merge_prefs(&once, 5), once);
  }

  #[test]
  fn merge_prefs_writes_lf_only_line_endings() {
    let merged = merge_prefs("a=1\r\nb=2\r\n", 4);
    assert!(!merged.contains('\r'));
    assert!(merged.ends_with('\n'));
  }

  #[test]
  fn a_new_account_needs_one_seeding_restart() {
    let present: BTreeSet<String> = ["old", "new"].map(String::from).into();
    let seeded: BTreeSet<String> = ["old"].map(String::from).into();
    assert_eq!(
      unseeded(&present, &seeded),
      ["new"].map(String::from).into()
    );
    assert!(unseeded(&present, &present).is_empty());
  }

  #[test]
  fn write_quality_touches_only_account_prefs() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("settings").join("Users").join("someone");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("prefs"), SPIKE_PREFS).unwrap();
    std::fs::write(
      dir.path().join("settings").join("prefs"),
      "core.clock_delta=-1\n",
    )
    .unwrap();

    assert_eq!(write_quality(dir.path(), 5).unwrap(), 1);
    assert_eq!(write_quality(dir.path(), 5).unwrap(), 0);
    assert_eq!(
      std::fs::read_to_string(dir.path().join("settings").join("prefs")).unwrap(),
      "core.clock_delta=-1\n"
    );
    mark_seeded(dir.path(), &user_dirs(dir.path())).unwrap();
    assert_eq!(seeded(dir.path()), ["someone"].map(String::from).into());
  }
}
