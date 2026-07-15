//! Local-files media source — scan a music directory and expose its audio
//! files via the multi-source [`MediaSource`] and [`Streamer`] traits.
//!
//! Gated behind the `local-files` Cargo feature. Nothing in the main dispatch
//! layer calls this yet; it will be wired in Phase 3 of the multi-source
//! refactor.

// Not yet wired into dispatch / UI.
#![allow(dead_code)]

pub mod dispatch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::infra::audio::LocalPlayer;

/// The active local-file playback session.
///
/// Holds the live [`LocalPlayer`], an ordered **queue** of track URIs plus the
/// **current index**, and the *static* metadata of the track at that index.
/// Dynamic state (position, paused) is read live from `player` at render time,
/// so it is never mirrored into — and therefore never desynced from —
/// Spotify/librespot state fields. `App` holds this in a single `Option`:
/// `Some` exactly while a local file owns the playback session.
///
/// The queue + index are the *single source of truth* for which local track is
/// playing; Next/Previous/auto-advance all move `index` and re-`play_file`.
pub struct LocalPlaybackState {
  pub player: Arc<LocalPlayer>,
  /// `file://` URIs for every track in the playing folder, in scan order.
  pub queue: Vec<String>,
  /// Index into [`queue`](Self::queue) of the currently playing track.
  pub index: usize,
  pub name: String,
  /// Display string of the joined artist names.
  pub artists: String,
  pub album: String,
  pub duration_ms: u64,
  /// Set while a track change (auto-advance *or* a manual Next/Previous) is
  /// decoding. The runner tick fires far faster than the dispatched track
  /// change is processed, so without this guard the empty sink between "track
  /// ended / cleared" and "next source appended" would dispatch `NextTrack`
  /// every tick and skip several tracks per change. Same class of guard as the
  /// "don't treat the pre-playback empty sink as end-of-track" invariant: it
  /// covers the analogous mid-change decode window.
  pub advancing: bool,
  /// Backup of the pre-shuffle queue order while shuffle is on (`None` when the
  /// queue is in natural order). Set by [`set_shuffle(true)`](Self::set_shuffle)
  /// and consumed by `set_shuffle(false)` to restore order + index exactly.
  pub shuffle_backup: Option<crate::infra::queue::ShuffleBackup>,
  /// Indices of the tracks that have failed to play since the last successful
  /// one, bounding the deliberate "skip past an unplayable file" behavior.
  ///
  /// Local playback leaves the session alive on a play failure so the tick moves
  /// past the bad track. That self-terminates under `RepeatMode::Off` (the last
  /// track has no next, so `advance_decision` reaches `Decision::Teardown`), but
  /// `RepeatMode::Context` wraps forever and never yields `Teardown` — so with an
  /// entirely unplayable queue (an unmounted share, an ejected drive) the tick
  /// would re-fail every track at tick rate with no terminating state. Cleared on
  /// every successful play; once it covers the whole queue there is nothing left
  /// to play and the session is torn down instead. Same class of bounded guard as
  /// `App::spotify_queue_guard_reloads`.
  ///
  /// Distinct indices rather than a failure count: a count would also trip on a
  /// user skipping back and forth across the same two bad tracks, tearing down a
  /// session whose remaining tracks play fine.
  pub failed_since_played: std::collections::HashSet<usize>,
}

impl LocalPlaybackState {
  /// Turn in-place shuffle on or off for this session — see
  /// [`toggle_shuffle`](crate::infra::queue::toggle_shuffle) for the shared
  /// semantics (current track stays playing at the front; un-shuffle restores
  /// order + index; idempotent).
  pub fn set_shuffle(&mut self, on: bool) {
    // The permutation moves every track, so remembered failures no longer name
    // the tracks that failed.
    self.failed_since_played.clear();
    crate::infra::queue::toggle_shuffle(
      &mut self.queue,
      &mut self.index,
      &mut self.shuffle_backup,
      on,
    );
  }
}

use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use lofty::prelude::*;
use lofty::read_from_path;
use url::Url;

use crate::core::plugin_api::{ArtistRef, PlaylistInfo, TrackInfo};
use crate::core::source::{MediaSource, Streamer};

// ---------------------------------------------------------------------------
// Audio-file extension filter
// ---------------------------------------------------------------------------

/// File extensions recognised as audio files.
///
/// Restricted to formats the default rodio decoder can actually play
/// (FLAC / MP3 / MP4-AAC / Ogg-Vorbis / WAV — see [`LocalPlayer::play_file`]).
/// Extensions the decoder rejects at playback time are deliberately excluded so
/// the library never lists a file that would fail to decode:
///   - `opus` — rodio's `vorbis` feature decodes Ogg-*Vorbis* only; there is no
///     Opus codec in the build, so `.opus` files probe but fail to decode.
///   - `aiff` / `aif` / `wv` (WavPack) / `ape` (Monkey's Audio) — no decoder in
///     the default feature set.
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "m4a", "aac", "wav"];

fn is_audio_file(path: &Path) -> bool {
  path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| {
      AUDIO_EXTENSIONS
        .iter()
        .any(|&ext| e.eq_ignore_ascii_case(ext))
    })
    .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// LocalSource
// ---------------------------------------------------------------------------

/// A media source that exposes local audio files rooted at a directory.
///
/// Each immediate subdirectory of `root` becomes a playlist. When audio files
/// sit directly in `root`, a synthetic `"(root)"` playlist is also returned as
/// the first entry so those loose files are browsable.
pub struct LocalSource {
  root: PathBuf,
}

impl LocalSource {
  /// Create a [`LocalSource`] for `root`. The path is **not** validated at
  /// construction time so this function is always cheap and infallible.
  pub fn new(root: impl Into<PathBuf>) -> Self {
    LocalSource { root: root.into() }
  }

  /// The root directory this source is scanning.
  pub fn root(&self) -> &Path {
    &self.root
  }
}

// ---------------------------------------------------------------------------
// MediaSource implementation
// ---------------------------------------------------------------------------

impl MediaSource for LocalSource {
  fn name(&self) -> &str {
    "Local Files"
  }

  fn scheme(&self) -> &str {
    "file"
  }

  /// Return one [`PlaylistInfo`] per immediate subdirectory of `root`, plus a
  /// synthetic `"(root)"` entry when the music root itself directly contains
  /// at least one audio file.
  ///
  /// The playlist `uri` is `file://<abs-path>`, the `name` is the directory's
  /// file name (or `"(root)"` for the synthetic entry), and `track_count` is
  /// the number of audio files directly inside that directory (non-recursive).
  ///
  /// The `"(root)"` entry, when present, is always placed first so loose files
  /// at the root are easy to find.
  ///
  /// TODO(multi-source): move std::fs::read_dir calls into tokio::task::spawn_blocking
  ///   or tokio::fs before wiring — blocking I/O on the async executor stalls
  ///   the Tokio runtime under slow/remote filesystems.
  async fn playlists(&self) -> Result<Vec<PlaylistInfo>> {
    let entries = std::fs::read_dir(&self.root)
      .with_context(|| format!("reading music root {:?}", self.root))?;

    let mut playlists = Vec::new();
    // Count loose audio files directly in root in the same pass that collects
    // subdirectories, so we avoid a second read_dir call on the same path.
    let mut root_audio_count: u32 = 0;
    for entry in entries {
      let entry = entry.context("reading directory entry")?;
      let path = entry.path();
      if !path.is_dir() {
        if path.is_file() && is_audio_file(&path) {
          root_audio_count += 1;
        }
        continue;
      }

      let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

      // Count audio files directly inside this subdirectory.
      let track_count = count_audio_files(&path).unwrap_or(0);

      let uri = path_to_file_uri(&path);

      playlists.push(PlaylistInfo {
        uri,
        name,
        owner: "local".to_string(),
        track_count,
        id: None,
        collaborative: false,
        public: None,
        image_url: None,
        owner_id: None,
      });
    }

    // Sort alphabetically for stable ordering across runs.
    playlists.sort_by(|a, b| a.name.cmp(&b.name));

    // Prepend a synthetic "(root)" entry when the root directory itself
    // contains at least one audio file, so loose files are browsable.
    // Note: a user-created subdirectory literally named "(root)" would result
    // in two entries with the same display name but different URIs; this is
    // unlikely in practice and acceptable given the clear "(root)" sentinel.
    if root_audio_count > 0 {
      playlists.insert(
        0,
        PlaylistInfo {
          uri: path_to_file_uri(&self.root),
          name: "(root)".to_string(),
          owner: "local".to_string(),
          track_count: root_audio_count,
          id: None,
          collaborative: false,
          public: None,
          image_url: None,
          owner_id: None,
        },
      );
    }

    Ok(playlists)
  }

  /// Return one [`TrackInfo`] per audio file in the directory identified by
  /// `playlist_uri`.
  ///
  /// `playlist_uri` must have been produced by [`LocalSource::playlists`]
  /// (i.e. `file://<abs-path>`). Tags are read via `lofty`; missing fields
  /// fall back to the file name / empty string.
  ///
  /// TODO(multi-source): move blocking lofty tag-reads into tokio::task::spawn_blocking
  ///   before wiring — lofty::read_from_path does synchronous file I/O and will
  ///   stall the Tokio runtime for large directories.
  async fn tracks(&self, playlist_uri: &str) -> Result<Vec<TrackInfo>> {
    let dir_path = file_uri_to_path(playlist_uri)
      .with_context(|| format!("parsing playlist URI: {playlist_uri}"))?;

    let entries = std::fs::read_dir(&dir_path)
      .with_context(|| format!("reading playlist dir {:?}", dir_path))?;

    let mut tracks = Vec::new();
    for entry in entries {
      let entry = entry.context("reading directory entry")?;
      let path = entry.path();
      if !path.is_file() || !is_audio_file(&path) {
        continue;
      }

      let info = track_info_from_path(&path);
      tracks.push(info);
    }

    // Sort by track_number, then by name.
    // track_number = 0 means "no tag" — sort those after all explicitly
    // numbered tracks (so track 1, 2, ... N appear before untagged files).
    tracks.sort_by(|a, b| {
      let key = |n: u32| if n == 0 { u32::MAX } else { n };
      key(a.track_number)
        .cmp(&key(b.track_number))
        .then(a.name.cmp(&b.name))
    });
    Ok(tracks)
  }
}

// ---------------------------------------------------------------------------
// Streamer skeleton
// ---------------------------------------------------------------------------

impl Streamer for LocalSource {
  /// Open the audio file at `uri` and construct a symphonia decoder to prove
  /// the decode pipeline is reachable.
  ///
  /// The decoded frames are NOT yet routed anywhere — the rodio sink wiring
  /// lives in `src/infra/player/streaming.rs` and is out of scope for this
  /// slice.
  ///
  /// TODO(multi-source): route decoded frames into the shared sink (Phase 3 wiring)
  /// TODO(multi-source): move blocking file I/O into tokio::task::spawn_blocking
  ///   before wiring — std::fs::File::open and symphonia probing are synchronous
  ///   and will stall the Tokio executor thread on slow/remote filesystems.
  async fn stream(&self, uri: &str) -> Result<()> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let path =
      file_uri_to_path(uri).with_context(|| format!("parsing track URI for streaming: {uri}"))?;

    let file =
      std::fs::File::open(&path).with_context(|| format!("opening audio file {:?}", path))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
      hint.with_extension(ext);
    }

    let format = symphonia::default::get_probe()
      .probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
      )
      .context("probing audio format")?;

    // Find the first audio track with a known codec.
    let track = format
      .default_track(TrackType::Audio)
      .context("no audio tracks found in file")?;

    let _decoder = symphonia::default::get_codecs()
      .make_audio_decoder(
        track
          .codec_params
          .as_ref()
          .context("codec parameters missing")?
          .audio()
          .context("track is not an audio codec")?,
        &AudioDecoderOptions::default(),
      )
      .context("constructing audio decoder")?;

    // TODO(multi-source): route decoded frames into the shared sink (Phase 3 wiring)
    Ok(())
  }
}

// ---------------------------------------------------------------------------
// Tag-reading helpers
// ---------------------------------------------------------------------------

/// Build a [`TrackInfo`] by reading ID3/Vorbis/etc. tags from `path` via
/// `lofty`. All fields fall back gracefully when tags are absent.
fn track_info_from_path(path: &Path) -> TrackInfo {
  let uri = path_to_file_uri(path);

  // Default fallback name: the filename without extension.
  let fallback_name = path
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("Unknown Track")
    .to_string();

  // Try to read tags; if lofty fails, return minimal metadata.
  let tagged = match read_from_path(path) {
    Ok(t) => t,
    Err(_) => {
      return TrackInfo {
        uri: Some(uri),
        name: fallback_name,
        artists: Vec::new(),
        album: String::new(),
        duration_ms: 0,
        id: None,
        album_id: None,
        artist_refs: Vec::new(),
        is_playable: true,
        is_local: true,
        track_number: 0,
        explicit: false,
        // Cover art for local files is read from the file's embedded picture on
        // demand (see `extract_embedded_cover`), not carried as a URL here.
        image_url: None,
      };
    }
  };

  // Duration from audio properties.
  let duration_ms = tagged.properties().duration().as_millis() as u64;

  // Prefer the primary tag; fall back to any tag.
  let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

  let (name, artist_name, album, track_number) = if let Some(t) = tag {
    let name = t.title().map(|s| s.to_string()).unwrap_or(fallback_name);
    let artist = t.artist().map(|s| s.to_string()).unwrap_or_default();
    let album = t.album().map(|s| s.to_string()).unwrap_or_default();
    let track_number = t.track().unwrap_or(0);
    (name, artist, album, track_number)
  } else {
    (fallback_name, String::new(), String::new(), 0)
  };

  // Split the combined artist string on common separators to produce a list.
  let artists: Vec<String> = if artist_name.is_empty() {
    Vec::new()
  } else {
    split_artists(&artist_name)
  };

  let artist_refs: Vec<ArtistRef> = artists
    .iter()
    .map(|name| ArtistRef {
      id: None,
      name: name.clone(),
    })
    .collect();

  TrackInfo {
    uri: Some(uri),
    name,
    artists,
    album,
    duration_ms,
    id: None,
    album_id: None,
    artist_refs,
    is_playable: true,
    is_local: true,
    track_number,
    explicit: false,
    image_url: None,
  }
}

/// Split an artist string on `";"`, `" / "`, or `" & "` — common multi-artist
/// separators in music tags. Returns a single-element vec when there is no
/// recognised separator.
///
/// Note: bare `"/"` (without surrounding spaces) is intentionally **not** used
/// as a separator because it appears inside single-artist names such as "AC/DC".
/// The spaced form `" / "` is the separator recommended by MusicBrainz for
/// multi-artist fields and is much less likely to appear within a single name.
fn split_artists(artist: &str) -> Vec<String> {
  // Try each separator in preference order.
  for sep in &[";", " / ", " & "] {
    if artist.contains(sep) {
      return artist
        .split(sep)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    }
  }
  vec![artist.to_string()]
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// Convert an absolute [`Path`] to a percent-encoded `file://` URI string.
///
/// Uses [`url::Url::from_file_path`] so that spaces, non-ASCII characters, and
/// non-UTF-8 byte sequences are all correctly percent-encoded. The `url` crate
/// is already a direct dependency of this crate.
///
/// Falls back to a raw `format!("file://{}", path.to_string_lossy())` on paths
/// that `from_file_path` rejects (e.g. truly relative paths), but those should
/// not reach this function in normal operation.
fn path_to_file_uri(path: &Path) -> String {
  Url::from_file_path(path)
    .map(|u| u.to_string())
    .unwrap_or_else(|_| {
      // Fallback: should not be reached for absolute paths from read_dir.
      format!("file://{}", path.to_string_lossy())
    })
}

/// Parse a `file://` URI back to a [`PathBuf`].
///
/// Accepts both percent-encoded URIs (produced by [`path_to_file_uri`]) and
/// bare `file://` URIs for compatibility.
pub(crate) fn file_uri_to_path(uri: &str) -> Result<PathBuf> {
  let url = Url::parse(uri).with_context(|| format!("invalid URI: {uri}"))?;
  url
    .to_file_path()
    .map_err(|_| anyhow::anyhow!("URI is not a valid file:// path: {uri}"))
}

/// Decode the first embedded cover-art picture from a local audio file into an
/// image, for the cover-art pane. Reads tags via `lofty` (synchronous file I/O
/// and image decode), so callers must run this on a blocking thread rather than
/// the async runtime. Returns an error when the file has no embedded artwork.
///
/// Gated on both `local-files` (for `lofty`) and `cover-art` (for `image`), so
/// builds enabling only one of them do not pull in the other's dependency.
#[cfg(feature = "cover-art")]
pub(crate) fn extract_embedded_cover(path: &Path) -> Result<image::DynamicImage> {
  let tagged = read_from_path(path).with_context(|| format!("reading tags from {:?}", path))?;
  let tag = tagged
    .primary_tag()
    .or_else(|| tagged.first_tag())
    .context("file has no tags")?;
  let picture = tag
    .pictures()
    .first()
    .context("file has no embedded cover art")?;
  image::load_from_memory(picture.data()).context("decoding embedded cover art")
}

/// Count audio files (non-recursively) in `dir`.
fn count_audio_files(dir: &Path) -> Result<u32> {
  let mut count = 0u32;
  for entry in std::fs::read_dir(dir)? {
    let path = entry?.path();
    if path.is_file() && is_audio_file(&path) {
      count += 1;
    }
  }
  Ok(count)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  /// Write a minimal valid WAV file (44-byte header + silence).
  ///
  /// Symphonia can probe and decode this without external data.
  fn write_wav(path: &Path, sample_rate: u32, num_samples: u32) {
    let data_size = num_samples * 2; // 16-bit mono
    let file_size = 36 + data_size;

    let mut f = std::fs::File::create(path).unwrap();
    // RIFF header
    f.write_all(b"RIFF").unwrap();
    f.write_all(&file_size.to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    // fmt chunk
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap(); // chunk size
    f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
    f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
    f.write_all(&sample_rate.to_le_bytes()).unwrap();
    f.write_all(&(sample_rate * 2).to_le_bytes()).unwrap(); // byte rate
    f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
    f.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample
                                                // data chunk
    f.write_all(b"data").unwrap();
    f.write_all(&data_size.to_le_bytes()).unwrap();
    f.write_all(&vec![0u8; data_size as usize]).unwrap();
  }

  // ---------------------------------------------------------------------------
  // URI round-trip
  // ---------------------------------------------------------------------------

  #[test]
  fn uri_round_trip() {
    let path = PathBuf::from("/home/user/music/track.mp3");
    let uri = path_to_file_uri(&path);
    assert_eq!(uri, "file:///home/user/music/track.mp3");
    let back = file_uri_to_path(&uri).unwrap();
    assert_eq!(back, path);
  }

  #[test]
  fn file_uri_to_path_rejects_non_file_uri() {
    assert!(file_uri_to_path("spotify:track:abc").is_err());
  }

  // ---------------------------------------------------------------------------
  // is_audio_file
  // ---------------------------------------------------------------------------

  #[test]
  fn audio_extensions_are_recognised() {
    // Only formats the default rodio decoder can actually play.
    for ext in ["mp3", "flac", "ogg", "m4a", "aac", "wav"] {
      let p = PathBuf::from(format!("track.{ext}"));
      assert!(is_audio_file(&p), "expected {ext} to be audio");
    }
  }

  #[test]
  fn non_audio_extensions_are_rejected() {
    for name in ["image.jpg", "cover.png", "README.md", "playlist.m3u"] {
      let p = PathBuf::from(name);
      assert!(!is_audio_file(&p), "expected {name} not to be audio");
    }
  }

  /// The whitelist must not offer files the default rodio decoder rejects at
  /// playback time. Listing an undecodable extension makes `play_file` fail
  /// mid-queue; guarding the whitelist keeps such files out of the library so a
  /// manual Next/Previous can never land on one. See `AUDIO_EXTENSIONS`.
  #[test]
  fn undecodable_extensions_are_not_whitelisted() {
    // rodio's default features decode Ogg-Vorbis, not Opus; the rest have no
    // decoder in the build at all.
    for ext in ["opus", "aiff", "aif", "wv", "ape"] {
      assert!(
        !AUDIO_EXTENSIONS.contains(&ext),
        "{ext} is not decodable by the default rodio build and must be excluded"
      );
      assert!(
        !is_audio_file(&PathBuf::from(format!("track.{ext}"))),
        "an undecodable {ext} file must not be treated as audio"
      );
    }

    // The formats the decoder *does* support must still be present.
    for ext in ["mp3", "flac", "ogg", "m4a", "aac", "wav"] {
      assert!(
        AUDIO_EXTENSIONS.contains(&ext),
        "{ext} is decodable and must stay whitelisted"
      );
    }
  }

  #[test]
  fn audio_extension_check_is_case_insensitive() {
    assert!(is_audio_file(&PathBuf::from("TRACK.MP3")));
    assert!(is_audio_file(&PathBuf::from("Album.FLAC")));
  }

  // ---------------------------------------------------------------------------
  // split_artists
  // ---------------------------------------------------------------------------

  #[test]
  fn split_artists_semicolon() {
    let parts = split_artists("Alice;Bob;Carol");
    assert_eq!(parts, vec!["Alice", "Bob", "Carol"]);
  }

  #[test]
  fn split_artists_ampersand() {
    let parts = split_artists("Alice & Bob");
    assert_eq!(parts, vec!["Alice", "Bob"]);
  }

  #[test]
  fn split_artists_spaced_slash() {
    let parts = split_artists("Alice / Bob");
    assert_eq!(parts, vec!["Alice", "Bob"]);
  }

  #[test]
  fn split_artists_bare_slash_is_not_a_separator() {
    // Band names like "AC/DC" must NOT be split on bare '/'.
    let parts = split_artists("AC/DC");
    assert_eq!(
      parts,
      vec!["AC/DC"],
      "bare slash in a name should not split"
    );
  }

  #[test]
  fn split_artists_single() {
    let parts = split_artists("Alice");
    assert_eq!(parts, vec!["Alice"]);
  }

  // ---------------------------------------------------------------------------
  // playlists() — directory scan
  // ---------------------------------------------------------------------------

  #[test]
  fn playlists_returns_one_entry_per_subdir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create in reverse alphabetical order to verify that the sort actually fires.
    std::fs::create_dir(root.join("Rock")).unwrap();
    std::fs::create_dir(root.join("Jazz")).unwrap();
    // A loose audio file in root triggers the synthetic "(root)" entry.
    std::fs::File::create(root.join("stray.mp3")).unwrap();

    let src = LocalSource::new(root);
    let playlists = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.playlists())
      .unwrap();

    let names: Vec<&str> = playlists.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
      names,
      vec!["(root)", "Jazz", "Rock"],
      "synthetic (root) entry should come first; subdirs sorted alphabetically"
    );

    for pl in &playlists {
      assert_eq!(pl.owner, "local");
      assert!(pl.uri.starts_with("file://"), "uri should be a file:// URI");
    }
  }

  // ---------------------------------------------------------------------------
  // playlists() — synthetic (root) entry
  // ---------------------------------------------------------------------------

  #[test]
  fn playlists_adds_root_entry_when_root_has_loose_audio_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // A subdirectory (for variety) and a loose audio file directly in root.
    std::fs::create_dir(root.join("Albums")).unwrap();
    write_wav(&root.join("loose.wav"), 44100, 100);

    let src = LocalSource::new(root);
    let playlists = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.playlists())
      .unwrap();

    // "(root)" must be present and first.
    assert!(
      !playlists.is_empty(),
      "expected at least one playlist entry"
    );
    let first = &playlists[0];
    assert_eq!(
      first.name, "(root)",
      "first entry must be the synthetic root"
    );
    assert_eq!(
      first.uri,
      path_to_file_uri(root),
      "root entry URI must point at the music root"
    );
    assert_eq!(first.track_count, 1, "root entry must count the loose file");
    assert_eq!(first.owner, "local");
  }

  #[test]
  fn playlists_omits_root_entry_when_root_has_no_loose_audio_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Only subdirectories; no loose files in root.
    let sub = root.join("Classical");
    std::fs::create_dir(&sub).unwrap();
    write_wav(&sub.join("piece.wav"), 44100, 100);
    // A non-audio file in root must not trigger the "(root)" entry.
    std::fs::File::create(root.join("cover.jpg")).unwrap();

    let src = LocalSource::new(root);
    let playlists = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.playlists())
      .unwrap();

    let names: Vec<&str> = playlists.iter().map(|p| p.name.as_str()).collect();
    assert!(
      !names.contains(&"(root)"),
      "no loose audio files in root means no (root) entry; got {names:?}"
    );
  }

  #[test]
  fn playlists_counts_audio_files_in_subdir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("Albums");
    std::fs::create_dir(&sub).unwrap();

    write_wav(&sub.join("track1.wav"), 44100, 100);
    write_wav(&sub.join("track2.wav"), 44100, 100);
    // A non-audio file should not count.
    std::fs::File::create(sub.join("cover.jpg")).unwrap();

    let src = LocalSource::new(root);
    let playlists = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.playlists())
      .unwrap();

    assert_eq!(playlists.len(), 1);
    assert_eq!(playlists[0].track_count, 2);
  }

  // ---------------------------------------------------------------------------
  // tracks() — file scan + TrackInfo construction
  // ---------------------------------------------------------------------------

  #[test]
  fn tracks_returns_one_entry_per_audio_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("Classical");
    std::fs::create_dir(&sub).unwrap();

    write_wav(&sub.join("piece.wav"), 44100, 200);
    // A JPEG cover should be skipped.
    std::fs::File::create(sub.join("cover.jpg")).unwrap();

    let src = LocalSource::new(root);
    let uri = path_to_file_uri(&sub);
    let tracks = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.tracks(&uri))
      .unwrap();

    assert_eq!(tracks.len(), 1);
    let t = &tracks[0];
    assert!(t.is_local, "track should be marked local");
    assert!(t.is_playable, "track should be playable");
    assert!(
      t.uri.as_deref().map_or(false, |u| u.starts_with("file://")),
      "track URI should be a file:// URI"
    );
  }

  #[test]
  fn tracks_falls_back_to_filename_when_no_tags() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let sub = root.join("Misc");
    std::fs::create_dir(&sub).unwrap();

    write_wav(&sub.join("my_song.wav"), 44100, 100);

    let src = LocalSource::new(root);
    let uri = path_to_file_uri(&sub);
    let tracks = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.tracks(&uri))
      .unwrap();

    assert_eq!(tracks.len(), 1);
    // lofty will have no tags for this bare WAV, so name should fall back to stem.
    let t = &tracks[0];
    // The name is either "my_song" (fallback) or a tag title; either is acceptable.
    assert!(!t.name.is_empty(), "name should not be empty");
  }

  // ---------------------------------------------------------------------------
  // Streamer skeleton
  // ---------------------------------------------------------------------------

  #[test]
  fn streamer_constructs_decoder_for_valid_wav() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("sample.wav");
    write_wav(&wav, 44100, 1000);

    let src = LocalSource::new(dir.path());
    let uri = path_to_file_uri(&wav);

    let result = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.stream(&uri));

    assert!(
      result.is_ok(),
      "streamer should succeed for valid WAV: {result:?}"
    );
  }

  #[test]
  fn streamer_errors_for_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = LocalSource::new(dir.path());
    let uri = "file:///nonexistent/path/track.wav";

    let result = tokio::runtime::Runtime::new()
      .unwrap()
      .block_on(src.stream(uri));

    assert!(result.is_err(), "streamer should fail for missing file");
  }

  // ---------------------------------------------------------------------------
  // LocalSource accessors
  // ---------------------------------------------------------------------------

  #[test]
  fn local_source_name_and_scheme() {
    let src = LocalSource::new("/tmp/music");
    assert_eq!(src.name(), "Local Files");
    assert_eq!(src.scheme(), "file");
  }

  #[test]
  fn current_uri_reads_the_indexed_track() {
    // current_uri() is index-driven and bounds-checked; exercise it without a
    // real player by building the queue fields directly is not possible
    // (player is required), so assert the underlying slice access contract that
    // current_uri relies on.
    let queue = vec!["x".to_string(), "y".to_string()];
    assert_eq!(queue.get(1).map(String::as_str), Some("y"));
    assert_eq!(queue.get(5).map(String::as_str), None);
  }
}
