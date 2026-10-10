//! Music.app remote protocol. Parsing and ownership data have no Apple dependency.
//! The only process/Apple Event boundary is the opt-in macOS `macos` module.
#![cfg_attr(
  not(all(feature = "apple-music", target_os = "macos")),
  allow(dead_code)
)]

#[cfg(any(test, all(feature = "apple-music", target_os = "macos")))]
pub(crate) mod dispatch;
#[cfg(all(feature = "apple-music", target_os = "macos"))]
mod macos;
#[cfg(any(test, all(feature = "apple-music", target_os = "macos")))]
mod process;

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use std::time::Instant;

use crate::core::plugin_api::{PlaylistInfo, TrackInfo};

pub(crate) const LIBRARY_URI: &str = "applemusic:library";
pub(crate) const PAGE_SIZE: usize = 100;

/// Failures that prove the requested Apple Event never reached Music. Other
/// helper failures (especially timeouts) leave the outcome unknown.
#[derive(Debug)]
pub(crate) enum CommandError {
  AutomationDenied,
  HelperSpawn(std::io::Error),
  LaunchFailed(anyhow::Error),
}

impl std::fmt::Display for CommandError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::AutomationDenied => f.write_str("Music automation was denied. Allow your terminal/spotatui to control Music in System Settings > Privacy & Security > Automation"),
      Self::HelperSpawn(_) => f.write_str("Cannot start Music helper"),
      Self::LaunchFailed(error) => write!(f, "Cannot launch Music: {error}"),
    }
  }
}

impl std::error::Error for CommandError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::HelperSpawn(error) => Some(error),
      Self::LaunchFailed(error) => Some(error.as_ref()),
      Self::AutomationDenied => None,
    }
  }
}

pub(crate) fn event_not_delivered(error: &anyhow::Error) -> bool {
  error.downcast_ref::<CommandError>().is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Browse {
  Playlists,
  Tracks(String),
  Search(String),
}

/// Persistent IDs are library-local 64-bit hexadecimal identifiers. Keep them
/// as strings: JavaScript numbers would round them above 2^53.
pub(crate) fn persistent_id(value: &str) -> Result<String> {
  ensure!(
    value.len() == 16 && value.bytes().all(|c| c.is_ascii_hexdigit()),
    "Invalid Apple Music persistent ID"
  );
  Ok(value.to_ascii_uppercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MusicUri {
  Track(String),
  Playlist(String),
  Library,
}

pub(crate) fn parse_uri(uri: &str) -> Result<MusicUri> {
  let value = uri
    .strip_prefix("applemusic:")
    .context("Not an Apple Music URI")?;
  if value == "library" {
    Ok(MusicUri::Library)
  } else if let Some(id) = value.strip_prefix("playlist:") {
    Ok(MusicUri::Playlist(persistent_id(id)?))
  } else {
    Ok(MusicUri::Track(persistent_id(value)?))
  }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
  Snapshot,
  Browse(Browse, usize),
  Play {
    container: MusicUri,
    track: Option<String>,
    offset: usize,
  },
  Resume,
  Pause,
  Next,
  Previous,
  Seek(u32),
  Volume(u8),
}

impl Command {
  /// All caller data is argv data, never source code or a shell command.
  pub(crate) fn arguments(&self) -> Result<Vec<String>> {
    let args: Vec<String> = match self {
      Self::Snapshot => vec!["snapshot".into()],
      Self::Resume => vec!["resume".into()],
      Self::Pause => vec!["pause".into()],
      Self::Next => vec!["next".into()],
      Self::Previous => vec!["previous".into()],
      Self::Seek(ms) => vec!["seek".into(), ms.to_string()],
      Self::Volume(volume) => vec!["volume".into(), volume.min(&100).to_string()],
      Self::Browse(Browse::Playlists, offset) => vec!["playlists".into(), offset.to_string()],
      Self::Browse(Browse::Tracks(uri), offset) => {
        let target = match parse_uri(uri)? {
          MusicUri::Playlist(id) => id,
          MusicUri::Library => "library".into(),
          MusicUri::Track(_) => bail!("Expected an Apple Music playlist"),
        };
        vec!["tracks".into(), target, offset.to_string()]
      }
      Self::Browse(Browse::Search(query), offset) => {
        ensure!(
          !query.contains('\0') && query.len() <= 4096,
          "Apple Music search is too long or contains NUL"
        );
        vec!["search".into(), query.clone(), offset.to_string()]
      }
      Self::Play {
        container,
        track,
        offset,
      } => {
        let (kind, id) = match container {
          MusicUri::Track(id) => ("track", persistent_id(id)?),
          MusicUri::Playlist(id) => ("playlist", persistent_id(id)?),
          MusicUri::Library => ("playlist", "library".into()),
        };
        vec![
          "play".into(),
          kind.into(),
          id,
          track
            .as_deref()
            .map(persistent_id)
            .transpose()?
            .unwrap_or_default(),
          offset.to_string(),
        ]
      }
    };
    Ok(args)
  }

  pub(crate) fn may_launch(&self) -> bool {
    !matches!(self, Self::Snapshot | Self::Pause)
  }
}

#[derive(Debug, Deserialize)]
struct WireTrack {
  id: String,
  name: String,
  #[serde(default)]
  artist: String,
  #[serde(default)]
  album: String,
  duration: f64,
  /// False for a track Music cannot play (gone from the catalogue, or a
  /// file track whose file is missing). Absent means playable.
  #[serde(default = "playable_by_default")]
  playable: bool,
}

fn playable_by_default() -> bool {
  true
}

fn milliseconds(seconds: f64) -> Result<u32> {
  ensure!(
    seconds.is_finite() && seconds >= 0.0 && seconds <= f64::from(u32::MAX) / 1000.0,
    "Invalid Apple Music time"
  );
  Ok((seconds * 1000.0).round() as u32)
}

impl WireTrack {
  fn into_track(self) -> Result<TrackInfo> {
    Ok(TrackInfo {
      uri: Some(format!("applemusic:{}", persistent_id(&self.id)?)),
      name: self.name,
      artists: if self.artist.is_empty() {
        vec![]
      } else {
        vec![self.artist]
      },
      album: self.album,
      duration_ms: milliseconds(self.duration)? as u64,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: self.playable,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
      release_date: None,
    })
  }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Snapshot {
  pub running: bool,
  pub playing: bool,
  pub track: Option<TrackInfo>,
  pub position_ms: u32,
  pub volume: u8,
  /// Music's own shuffle setting, which spotatui's next follows.
  pub shuffle: bool,
  /// After a start or a resume: whether Music really began playing. `None`
  /// for every other command.
  pub started: Option<bool>,
}

pub(crate) fn parse_snapshot(json: &str) -> Result<Snapshot> {
  #[derive(Deserialize)]
  struct Wire {
    running: bool,
    playing: bool,
    track: Option<WireTrack>,
    position: f64,
    volume: u8,
    #[serde(default)]
    shuffle: bool,
    #[serde(default)]
    started: Option<bool>,
  }
  let wire: Wire = serde_json::from_str(json).context("Invalid Music response")?;
  ensure!(wire.volume <= 100, "Invalid Music volume");
  ensure!(
    wire.running || (!wire.playing && wire.track.is_none()),
    "Inconsistent Music state"
  );
  let track = wire.track.map(WireTrack::into_track).transpose()?;
  let position_ms =
    milliseconds(wire.position)?.min(track.as_ref().map_or(0, |t| t.duration_ms as u32));
  Ok(Snapshot {
    running: wire.running,
    playing: wire.playing,
    track,
    position_ms,
    volume: wire.volume,
    shuffle: wire.shuffle,
    started: wire.started,
  })
}

#[derive(Debug)]
pub(crate) struct Page<T> {
  pub items: Vec<T>,
  pub offset: usize,
  pub total: usize,
  /// Where the next page starts: past any entries the helper skipped.
  pub next: usize,
}

#[derive(Deserialize)]
struct WirePage<T> {
  items: Vec<T>,
  offset: usize,
  total: usize,
  /// Absent from older helpers, which never skipped an entry.
  next: Option<usize>,
}

impl<T> WirePage<T> {
  fn next(&self) -> usize {
    self.next.unwrap_or(self.offset + self.items.len())
  }
}

fn validate_page<T>(page: &WirePage<T>) -> Result<()> {
  let next = page.next();
  ensure!(
    page.items.len() <= PAGE_SIZE
      && page.offset <= next
      && next <= page.total
      && next - page.offset <= PAGE_SIZE
      && page.items.len() <= next - page.offset,
    "Invalid Music page bounds"
  );
  ensure!(
    next > page.offset || page.offset == page.total,
    "Music returned an empty incomplete page"
  );
  Ok(())
}

pub(crate) fn parse_tracks(json: &str) -> Result<Page<TrackInfo>> {
  let wire: WirePage<WireTrack> = serde_json::from_str(json).context("Invalid Music tracks")?;
  validate_page(&wire)?;
  let next = wire.next();
  Ok(Page {
    items: wire
      .items
      .into_iter()
      .map(WireTrack::into_track)
      .collect::<Result<_>>()?,
    next,
    offset: wire.offset,
    total: wire.total,
  })
}

pub(crate) fn parse_playlists(json: &str) -> Result<Page<PlaylistInfo>> {
  #[derive(Deserialize)]
  struct Playlist {
    id: String,
    name: String,
  }
  let wire: WirePage<Playlist> = serde_json::from_str(json).context("Invalid Music playlists")?;
  validate_page(&wire)?;
  let next = wire.next();
  let items = wire
    .items
    .into_iter()
    .map(|p| {
      Ok(PlaylistInfo {
        uri: format!("applemusic:playlist:{}", persistent_id(&p.id)?),
        name: p.name,
        owner: "Music".into(),
        track_count: 0,
        id: None,
        owner_id: None,
        collaborative: false,
        public: None,
        image_url: None,
      })
    })
    .collect::<Result<_>>()?;
  Ok(Page {
    items,
    next,
    offset: wire.offset,
    total: wire.total,
  })
}

#[derive(Default)]
pub(crate) struct RemoteState {
  pub claimed: bool,
  pub switching: bool,
  /// A failed handoff proved no event reached Music. Its held start can be
  /// released once, unless a newer explicit start supersedes this token.
  pub recovered_handoff: Option<u64>,
  pub desired_playing: bool,
  /// Bumped whenever the user asks for play or pause. A start that Music
  /// ignored only clears `desired_playing` when no newer request came since.
  pub intent_revision: u64,
  pub generation: u64,
  pub snapshot: Option<Snapshot>,
  pub observed_at: Option<Instant>,
  /// Transport commands queued for Music and not yet run. A snapshot read
  /// meanwhile predates them, so it must not undo what they will set.
  pub pending_commands: u32,
  /// When the last transport command finished: Music can still report the
  /// old play state for a moment after a pause (about 0.35s measured).
  pub commanded_at: Option<Instant>,
  /// Latest volume while the throttle or an earlier volume command holds it.
  pub pending_volume: Option<u8>,
  pub last_volume: Option<Instant>,
  pub volume_in_flight: Option<u8>,
  /// When a read first found Music not running. The claim is let go only
  /// once later reads still find it gone, see `accept_apple_music_snapshot`.
  pub quit_seen: Option<Instant>,
  /// Wall-clock time of the last accepted read. Unlike `observed_at` it
  /// counts time asleep, so a track that ended while the Mac slept or while
  /// reads failed is still recognised as ended.
  pub read_at: Option<std::time::SystemTime>,
  pub browse_generation: u64,
  /// The sidebar's playlists load in their own slot, see `browse_apple_music`.
  pub playlists_generation: u64,
  pub browse: Option<Browse>,
  pub tracks: Vec<TrackInfo>,
  pub playlists: Vec<PlaylistInfo>,
  pub playing_list: Option<PlayingList>,
  /// Music's shuffle setting as last read while a track was loaded. Stopped,
  /// Music reports it off whatever the user chose, so that read is ignored.
  pub shuffle: bool,
}

/// The list a Music track was started from in spotatui. Music gives a track
/// started by itself no queue: its next-track command does nothing and
/// playback stops at the end of the track (measured on macOS 26.5 and 27;
/// only playing a whole playlist builds a queue). So spotatui keeps the list
/// and starts the neighbouring track by id: on next/previous, and when the
/// track ends.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlayingList {
  pub context: String,
  pub uris: Vec<String>,
  pub index: usize,
  /// When spotatui last started a track: snapshots read right after a start
  /// can still name the previous track, so they do not move `index` yet.
  pub stepped_at: Instant,
  /// Indexes played before `index`, most recent last, so previous retraces a
  /// shuffled next instead of taking the row above.
  pub history: Vec<usize>,
  /// The shuffled tracks still to play in this round, next one last. Filled
  /// when empty with every track but the current one, so no track repeats
  /// before the whole list played. Only used while Music's shuffle is on.
  pub upcoming: Vec<usize>,
  /// The browse the list was taken from while its later pages still load:
  /// they are added to the list as they arrive.
  pub browse_generation: Option<u64>,
  /// A start was dropped or refused, so `index` may name a track Music is
  /// not playing: follow the next snapshot at once.
  pub resync: bool,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn apple_music_ids_never_become_code_or_imprecise_numbers() {
    assert_eq!(
      parse_uri("applemusic:ffffffffffffffff").unwrap(),
      MusicUri::Track("FFFFFFFFFFFFFFFF".into())
    );
    for bad in [
      "applemusic:",
      "applemusic:1",
      "applemusic:playlist:bad",
      "applemusic:1234567890ABCDEF;quit",
      "spotify:track:x",
    ] {
      assert!(parse_uri(bad).is_err());
    }
    let query = "\"; Application('Finder').activate(); //\n歌\\曲";
    assert_eq!(
      Command::Browse(Browse::Search(query.into()), 0)
        .arguments()
        .unwrap(),
      vec!["search", query, "0"]
    );
  }

  #[test]
  fn apple_music_json_round_trips_quotes_newlines_and_unicode() {
    let json = serde_json::json!({"running":true,"playing":true,"volume":42,"position":1.25,
      "track":{"id":"0123456789ABCDEF","name":"A\t\"B\"\n歌","artist":"合作者\\","album":"Album","duration":2.5}}).to_string();
    let snapshot = parse_snapshot(&json).unwrap();
    assert_eq!(snapshot.position_ms, 1250);
    let track = snapshot.track.unwrap();
    assert_eq!(track.name, "A\t\"B\"\n歌");
    assert_eq!(track.duration_ms, 2500);
    assert_eq!(track.id, None);
  }

  #[test]
  fn apple_music_rejects_malformed_times_pages_and_partial_json() {
    for time in [-1.0, f64::NAN, f64::INFINITY, 1e20] {
      assert!(milliseconds(time).is_err());
    }
    assert!(parse_snapshot("{\"running\":true}").is_err());
    assert!(parse_tracks(r#"{"items":[],"offset":0,"total":1}"#).is_err());
    assert!(parse_playlists(r#"{"items":[],"offset":2,"total":1}"#).is_err());
    assert!(parse_tracks(r#"{"items":[],"offset":0,"total":0}"#).is_ok());
    assert!(!Command::Pause.may_launch());
    assert!(!Command::Snapshot.may_launch());
  }

  #[test]
  fn apple_music_pages_move_past_skipped_entries() {
    // Every entry of the page was unreadable and skipped: still progress.
    let page = parse_tracks(r#"{"items":[],"offset":0,"total":150,"next":100}"#).unwrap();
    assert_eq!((page.items.len(), page.next), (0, 100));
    // A next start beyond the list, or a page wider than a page, is bogus.
    assert!(parse_tracks(r#"{"items":[],"offset":0,"total":50,"next":100}"#).is_err());
    assert!(parse_tracks(r#"{"items":[],"offset":0,"total":500,"next":200}"#).is_err());
    // Older helpers send no next: it follows the rows.
    let page =
      parse_playlists(r#"{"items":[{"id":"0123456789ABCDEF","name":"x"}],"offset":3,"total":9}"#)
        .unwrap();
    assert_eq!(page.next, 4);
  }
}
