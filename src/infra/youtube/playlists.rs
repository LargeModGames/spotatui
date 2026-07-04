//! Local YouTube playlists — user-curated lists of YouTube videos stored in a
//! plain YAML file, no Google account required.
//!
//! There is no clean login API for YouTube Music (Google shut down the OAuth
//! path; cookies are fragile and risky), so instead of syncing a remote
//! library the YouTube source keeps playlists **locally**:
//! `~/.config/spotatui/youtube_playlists.yml`. Together with anonymous
//! search + playback this makes spotatui fully usable without any account.
//!
//! The file is human-editable and shareable:
//!
//! ```yaml
//! playlists:
//!   - id: k3j9x2m4p7q1
//!     name: Focus
//!     tracks:
//!       - video_id: 5NV6Rdv1a3I
//!         title: Daft Punk - Get Lucky (Official Audio)
//!         channel: Daft Punk
//!         duration_ms: 249000
//! ```
//!
//! ## URIs
//!
//! `youtube:playlist:<id>` — ids are locally generated (random alphanumeric),
//! stable across renames. Note these URIs pass [`super::is_youtube_uri`] but
//! fail [`super::video_id_from_uri`] (the `:` is rejected), so they can never
//! be mistaken for a playable video URI.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::core::plugin_api::{PlaylistInfo, TrackInfo};

use super::{uri_for_video_id, video_id_from_uri};

const PLAYLIST_PREFIX: &str = "youtube:playlist:";
const FILE_NAME: &str = "youtube_playlists.yml";

// ---------------------------------------------------------------------------
// Storage types
// ---------------------------------------------------------------------------

/// The whole on-disk file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PlaylistsFile {
  #[serde(default)]
  pub playlists: Vec<StoredPlaylist>,
}

/// One user playlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredPlaylist {
  /// Locally generated random id, stable across renames.
  pub id: String,
  pub name: String,
  #[serde(default)]
  pub tracks: Vec<StoredTrack>,
}

/// One saved video — enough metadata to render the track table without a
/// network round-trip.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredTrack {
  pub video_id: String,
  pub title: String,
  #[serde(default)]
  pub channel: String,
  #[serde(default)]
  pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// Whether a URI names a local YouTube playlist.
pub fn is_playlist_uri(uri: &str) -> bool {
  uri.starts_with(PLAYLIST_PREFIX)
}

/// `youtube:playlist:<id>` for a playlist id.
pub fn uri_for_playlist_id(id: &str) -> String {
  format!("{PLAYLIST_PREFIX}{id}")
}

/// Extract the playlist id from a `youtube:playlist:<id>` URI; also accepts a
/// bare id (callers pass whichever they have).
pub fn playlist_id_from_ref(uri_or_id: &str) -> &str {
  if is_playlist_uri(uri_or_id) {
    &uri_or_id[PLAYLIST_PREFIX.len()..]
  } else {
    uri_or_id
  }
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

/// Environment override for the playlists file location (used by tests and
/// available to users who keep their config elsewhere).
pub const PATH_ENV: &str = "SPOTATUI_YOUTUBE_PLAYLISTS_PATH";

/// Location of the playlists file: `$SPOTATUI_YOUTUBE_PLAYLISTS_PATH` when
/// set, else `<config dir>/youtube_playlists.yml` next to the app config.
pub fn default_playlists_path() -> Result<PathBuf> {
  if let Ok(path) = std::env::var(PATH_ENV) {
    return Ok(PathBuf::from(path));
  }
  crate::core::user_config::default_app_config_dir()
    .map(|dir| dir.join(FILE_NAME))
    .ok_or_else(|| anyhow!("cannot resolve the spotatui config directory"))
}

/// Load the playlists file; a missing file is an empty list, a malformed file
/// is an error (never silently overwrite a file the user hand-edited wrong).
pub fn load(path: &Path) -> Result<PlaylistsFile> {
  match std::fs::read_to_string(path) {
    Ok(contents) => serde_yaml::from_str(&contents)
      .with_context(|| format!("malformed YouTube playlists file: {}", path.display())),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PlaylistsFile::default()),
    Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
  }
}

/// Save the playlists file atomically (write a sibling tempfile, then rename)
/// so a crash mid-write can't destroy the user's playlists.
pub fn save(path: &Path, file: &PlaylistsFile) -> Result<()> {
  let yaml = serde_yaml::to_string(file).context("serializing YouTube playlists")?;
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
  }
  let tmp = path.with_extension("yml.tmp");
  std::fs::write(&tmp, yaml).with_context(|| format!("writing {}", tmp.display()))?;
  std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
  Ok(())
}

// ---------------------------------------------------------------------------
// CRUD (pure, on the in-memory file; callers load → mutate → save)
// ---------------------------------------------------------------------------

/// Generate a 12-character random alphanumeric playlist id, unique within
/// `file`.
fn generate_id(file: &PlaylistsFile) -> String {
  const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::thread_rng();
  loop {
    let id: String = (0..12)
      .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
      .collect();
    if !file.playlists.iter().any(|p| p.id == id) {
      return id;
    }
  }
}

/// Create a playlist named `name`, returning its new id. Duplicate names are
/// allowed (ids disambiguate), but blank names are not.
pub fn create_playlist(file: &mut PlaylistsFile, name: &str) -> Result<String> {
  let name = name.trim();
  if name.is_empty() {
    bail!("playlist name cannot be empty");
  }
  let id = generate_id(file);
  file.playlists.push(StoredPlaylist {
    id: id.clone(),
    name: name.to_string(),
    tracks: Vec::new(),
  });
  Ok(id)
}

/// Delete the playlist referenced by `uri_or_id`. Errors when it doesn't
/// exist (surface stale-UI bugs rather than masking them).
pub fn delete_playlist(file: &mut PlaylistsFile, uri_or_id: &str) -> Result<StoredPlaylist> {
  let id = playlist_id_from_ref(uri_or_id);
  let idx = file
    .playlists
    .iter()
    .position(|p| p.id == id)
    .ok_or_else(|| anyhow!("no such YouTube playlist: {uri_or_id}"))?;
  Ok(file.playlists.remove(idx))
}

/// Append `track` to the playlist referenced by `uri_or_id`. Returns `false`
/// (without modifying anything) when the video is already in the playlist.
pub fn add_track(file: &mut PlaylistsFile, uri_or_id: &str, track: StoredTrack) -> Result<bool> {
  let id = playlist_id_from_ref(uri_or_id);
  let playlist = file
    .playlists
    .iter_mut()
    .find(|p| p.id == id)
    .ok_or_else(|| anyhow!("no such YouTube playlist: {uri_or_id}"))?;
  if playlist.tracks.iter().any(|t| t.video_id == track.video_id) {
    return Ok(false);
  }
  playlist.tracks.push(track);
  Ok(true)
}

/// Remove the video with `video_id` from the playlist referenced by
/// `uri_or_id`. Returns the removed track.
pub fn remove_track(
  file: &mut PlaylistsFile,
  uri_or_id: &str,
  video_id: &str,
) -> Result<StoredTrack> {
  let id = playlist_id_from_ref(uri_or_id);
  let playlist = file
    .playlists
    .iter_mut()
    .find(|p| p.id == id)
    .ok_or_else(|| anyhow!("no such YouTube playlist: {uri_or_id}"))?;
  let idx = playlist
    .tracks
    .iter()
    .position(|t| t.video_id == video_id)
    .ok_or_else(|| anyhow!("video {video_id} is not in that playlist"))?;
  Ok(playlist.tracks.remove(idx))
}

/// Find a playlist by URI or bare id.
pub fn find_playlist<'a>(file: &'a PlaylistsFile, uri_or_id: &str) -> Option<&'a StoredPlaylist> {
  let id = playlist_id_from_ref(uri_or_id);
  file.playlists.iter().find(|p| p.id == id)
}

// ---------------------------------------------------------------------------
// Domain type conversions
// ---------------------------------------------------------------------------

/// Map a stored playlist onto the sidebar row type.
pub fn playlist_to_info(p: &StoredPlaylist) -> PlaylistInfo {
  PlaylistInfo {
    uri: uri_for_playlist_id(&p.id),
    name: p.name.clone(),
    owner: "local".to_string(),
    track_count: p.tracks.len() as u32,
    id: Some(p.id.clone()),
    owner_id: None,
    collaborative: false,
    public: None,
    image_url: None,
  }
}

/// Map a stored track onto the shared track-table row type.
pub fn stored_to_track_info(t: &StoredTrack) -> TrackInfo {
  TrackInfo {
    uri: Some(uri_for_video_id(&t.video_id)),
    name: t.title.clone(),
    artists: if t.channel.is_empty() {
      vec![]
    } else {
      vec![t.channel.clone()]
    },
    album: "YouTube".to_string(),
    duration_ms: t.duration_ms,
    id: Some(t.video_id.clone()),
    album_id: None,
    artist_refs: vec![],
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    // StoredTrack persists no thumbnail (on-disk format stays stable); the
    // video id alone yields a deterministic thumbnail URL.
    image_url: Some(super::thumbnail_url_for_video_id(&t.video_id)),
  }
}

/// Build a [`StoredTrack`] from a browse row (search result or track table).
/// The row's `id` holds the bare video id and its `uri` the `youtube:` URI;
/// accept either, validating through the same charset check as playback.
pub fn track_info_to_stored(t: &TrackInfo) -> Result<StoredTrack> {
  let video_id = match (&t.id, &t.uri) {
    (Some(id), _) if video_id_from_uri(&uri_for_video_id(id)).is_ok() => id.clone(),
    (_, Some(uri)) => video_id_from_uri(uri)?.to_string(),
    _ => bail!("row has no YouTube video id"),
  };
  Ok(StoredTrack {
    video_id,
    title: t.name.clone(),
    channel: t.artists.first().cloned().unwrap_or_default(),
    duration_ms: t.duration_ms,
  })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  fn track(id: &str, title: &str) -> StoredTrack {
    StoredTrack {
      video_id: id.to_string(),
      title: title.to_string(),
      channel: "Chan".to_string(),
      duration_ms: 1000,
    }
  }

  #[test]
  fn crud_round_trip() {
    let mut file = PlaylistsFile::default();
    let id = create_playlist(&mut file, "  Focus  ").unwrap();
    assert_eq!(file.playlists[0].name, "Focus", "name must be trimmed");
    assert!(create_playlist(&mut file, "   ").is_err(), "blank rejected");

    let uri = uri_for_playlist_id(&id);
    assert!(add_track(&mut file, &uri, track("aaa", "A")).unwrap());
    assert!(
      add_track(&mut file, &id, track("bbb", "B")).unwrap(),
      "bare id accepted"
    );
    assert!(
      !add_track(&mut file, &uri, track("aaa", "A again")).unwrap(),
      "duplicate video is a no-op"
    );
    assert_eq!(find_playlist(&file, &uri).unwrap().tracks.len(), 2);

    let removed = remove_track(&mut file, &uri, "aaa").unwrap();
    assert_eq!(removed.title, "A");
    assert!(
      remove_track(&mut file, &uri, "aaa").is_err(),
      "already gone"
    );

    let deleted = delete_playlist(&mut file, &uri).unwrap();
    assert_eq!(deleted.name, "Focus");
    assert!(file.playlists.is_empty());
    assert!(delete_playlist(&mut file, &uri).is_err());
  }

  #[test]
  fn save_and_load_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("youtube_playlists.yml");

    // Missing file loads as empty.
    assert_eq!(load(&path).unwrap(), PlaylistsFile::default());

    let mut file = PlaylistsFile::default();
    let id = create_playlist(&mut file, "Road Trip").unwrap();
    add_track(&mut file, &id, track("5NV6Rdv1a3I", "Get Lucky")).unwrap();
    save(&path, &file).unwrap();

    let loaded = load(&path).unwrap();
    assert_eq!(loaded, file);
    // No stray tempfile left behind by the atomic write.
    assert!(!path.with_extension("yml.tmp").exists());
  }

  #[test]
  fn malformed_file_errors_instead_of_wiping() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("youtube_playlists.yml");
    std::fs::write(&path, "playlists: [ this is not : valid").unwrap();
    assert!(load(&path).is_err());
  }

  #[test]
  fn hand_written_minimal_yaml_loads() {
    // A user hand-editing the file will omit optional fields.
    let file: PlaylistsFile = serde_yaml::from_str(
      "playlists:\n  - id: abc\n    name: Mine\n    tracks:\n      - video_id: xyz\n        title: T\n",
    )
    .unwrap();
    assert_eq!(file.playlists[0].tracks[0].channel, "");
    assert_eq!(file.playlists[0].tracks[0].duration_ms, 0);
  }

  #[test]
  fn playlist_uri_round_trip_and_video_uri_separation() {
    let uri = uri_for_playlist_id("abc123");
    assert_eq!(uri, "youtube:playlist:abc123");
    assert!(is_playlist_uri(&uri));
    assert_eq!(playlist_id_from_ref(&uri), "abc123");
    assert_eq!(playlist_id_from_ref("abc123"), "abc123");
    // A playlist URI must never pass for a playable video URI.
    assert!(video_id_from_uri(&uri).is_err());
    assert!(!is_playlist_uri("youtube:5NV6Rdv1a3I"));
  }

  #[test]
  fn conversions_round_trip_through_domain_types() {
    let mut file = PlaylistsFile::default();
    let id = create_playlist(&mut file, "Mix").unwrap();
    add_track(&mut file, &id, track("vid1", "Song")).unwrap();

    let info = playlist_to_info(&file.playlists[0]);
    assert_eq!(info.uri, format!("youtube:playlist:{id}"));
    assert_eq!(info.track_count, 1);

    let row = stored_to_track_info(&file.playlists[0].tracks[0]);
    assert_eq!(row.uri.as_deref(), Some("youtube:vid1"));
    assert_eq!(row.artists, vec!["Chan"]);

    // And back: a browse row (as search would produce it) converts to storage.
    let stored = track_info_to_stored(&row).unwrap();
    assert_eq!(stored, file.playlists[0].tracks[0]);

    // Rows with a uri but no id (defensive) still convert.
    let mut no_id = row.clone();
    no_id.id = None;
    assert_eq!(track_info_to_stored(&no_id).unwrap().video_id, "vid1");

    // Rows with neither are rejected.
    let mut neither = row;
    neither.id = None;
    neither.uri = None;
    assert!(track_info_to_stored(&neither).is_err());
  }

  #[test]
  fn generated_ids_are_unique_and_well_formed() {
    let mut file = PlaylistsFile::default();
    for i in 0..50 {
      create_playlist(&mut file, &format!("p{i}")).unwrap();
    }
    let mut ids: Vec<_> = file.playlists.iter().map(|p| p.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 50, "ids must be unique");
    for id in &ids {
      assert_eq!(id.len(), 12);
      assert!(id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
    }
  }
}
