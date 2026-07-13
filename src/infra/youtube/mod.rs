//! YouTube media source (via the external `yt-dlp` binary).
//!
//! Deliberately the thinnest possible wrapper: no YouTube crates, no API keys —
//! [`YouTubeSource`] shells out to `yt-dlp`, which owns the extraction
//! treadmill (PO tokens, signature ciphers, throttling). Search uses
//! `--flat-playlist --dump-json ytsearchN:`; playback downloads the AAC/M4A
//! audio-only format (itag 140, decodable by rodio's default `mp4` feature) to
//! a tempfile and plays it through the shared [`LocalPlayer`] — the same
//! download-then-play model as the Subsonic source.
//!
//! This source is **unofficial-fragile by design** (see plan-multi-source.md):
//! when YouTube hardens extraction, updating the `yt-dlp` binary is the fix,
//! not a spotatui release.
//!
//! ## URIs
//!
//! `youtube:<video-id>` — the suffix is the 11-character video id. Ids are
//! charset-validated before being passed to `yt-dlp` (defense against argument
//! injection; they are embedded in a full watch URL, never passed bare).

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::core::plugin_api::{SearchResults, TrackInfo};
use crate::core::source::Searcher;
use crate::infra::audio::LocalPlayer;

pub mod dispatch;
pub mod playlists;
mod types;

use types::YtVideo;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const YOUTUBE_PREFIX: &str = "youtube:";

/// The `yt-dlp` binary to invoke when the user has not configured
/// `behavior.ytdlp_path` — resolved through `$PATH`.
const DEFAULT_YTDLP: &str = "yt-dlp";

/// Result page size for searches. Each flat-search page is one remote request
/// for yt-dlp, so this is also the dominant term in search latency (~3-6 s).
const SEARCH_LIMIT: usize = 20;

/// Audio format selector: itag 140 (AAC/M4A ~128 kbps) first — the format the
/// bundled symphonia decoder is known to handle — then any m4a, then whatever
/// audio exists (a webm/opus fallback will fail to decode, but a clear decode
/// error beats no audio at all when 140 disappears someday).
const FORMAT_SELECTOR: &str = "140/bestaudio[ext=m4a]/bestaudio";

/// Cap on a search invocation. yt-dlp searches normally return in a few
/// seconds; a hung process must not wedge the IoEvent pump forever.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(45);

/// Cap on a download invocation (a ~4 MiB audio file, but allow slow links).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);

// ---------------------------------------------------------------------------
// YouTubePlaybackState
// ---------------------------------------------------------------------------

/// The active YouTube playback session — same decoupled shape as the Subsonic
/// one: it owns the live [`LocalPlayer`], the queue of searched videos with
/// the current index, and the downloaded tempfile for the playing track. It
/// never writes Spotify/librespot fields; the playbar reads progress/pause
/// live from `player`.
pub struct YouTubePlaybackState {
  pub player: Arc<LocalPlayer>,
  /// Source handle, reused to download each track on Next/advance.
  pub source: Arc<YouTubeSource>,
  /// The queued videos (with search metadata) in order.
  pub tracks: Vec<TrackInfo>,
  /// Index into [`tracks`](Self::tracks) of the currently playing video.
  pub index: usize,
  /// Auto-advance guard — set while a track change is downloading/decoding so
  /// the runner tick does not mistake the empty sink for end-of-track. The
  /// yt-dlp download window is even longer than Subsonic's, so this guard is
  /// load-bearing.
  pub advancing: bool,
  /// The downloaded audio for the current track, kept alive while it plays.
  pub tempfile: tempfile::NamedTempFile,
  /// Backup of the pre-shuffle track order while shuffle is on (`None` in natural
  /// order). Set by [`set_shuffle(true)`](Self::set_shuffle); restored exactly by
  /// `set_shuffle(false)`.
  pub shuffle_backup: Option<crate::infra::queue::ShuffleBackup>,
}

impl YouTubePlaybackState {
  /// The currently playing video, if `index` is in range.
  pub fn current(&self) -> Option<&TrackInfo> {
    self.tracks.get(self.index)
  }

  /// Turn in-place shuffle on or off — see
  /// [`toggle_shuffle`](crate::infra::queue::toggle_shuffle) for the shared
  /// semantics (current track stays playing at the front; un-shuffle restores
  /// order + index; idempotent).
  pub fn set_shuffle(&mut self, on: bool) {
    crate::infra::queue::toggle_shuffle(
      &mut self.tracks,
      &mut self.index,
      &mut self.shuffle_backup,
      on,
    );
  }
}

// ---------------------------------------------------------------------------
// URI helpers
// ---------------------------------------------------------------------------

/// Whether a URI is owned by the YouTube source.
pub fn is_youtube_uri(uri: &str) -> bool {
  uri.starts_with(YOUTUBE_PREFIX)
}

/// Strip the `youtube:` prefix and return the video id, validating its
/// charset. The validation is load-bearing: the id is interpolated into a
/// `yt-dlp` argument, so a crafted "id" must not be able to smuggle flags or
/// URL structure.
pub fn video_id_from_uri(uri: &str) -> Result<&str> {
  let id = uri
    .strip_prefix(YOUTUBE_PREFIX)
    .ok_or_else(|| anyhow!("Not a YouTube URI: {}", uri))?;
  if id.is_empty()
    || !id
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
  {
    bail!("Invalid YouTube video id: {id:?}");
  }
  Ok(id)
}

/// Build the `youtube:` URI for a video id.
pub fn uri_for_video_id(id: &str) -> String {
  format!("{YOUTUBE_PREFIX}{id}")
}

// ---------------------------------------------------------------------------
// YouTubeSource — yt-dlp wrapper
// ---------------------------------------------------------------------------

/// Client for YouTube backed by an external `yt-dlp` binary.
pub struct YouTubeSource {
  /// Path or name of the `yt-dlp` binary (name is resolved via `$PATH`).
  ytdlp: String,
}

impl YouTubeSource {
  /// `ytdlp_path` is the user's `behavior.ytdlp_path` override, if any.
  pub fn new(ytdlp_path: Option<String>) -> Self {
    YouTubeSource {
      ytdlp: ytdlp_path.unwrap_or_else(|| DEFAULT_YTDLP.to_string()),
    }
  }

  /// Search YouTube by keywords, returning up to [`SEARCH_LIMIT`] videos.
  ///
  /// Flat extraction (`--flat-playlist`) returns one JSON object per line
  /// without resolving each video page, keeping the search to one remote
  /// round-trip.
  pub async fn search_videos(&self, query: &str) -> Result<Vec<YtVideo>> {
    let stdout = self
      .run(
        &[
          "--flat-playlist",
          "--dump-json",
          "--no-warnings",
          &format!("ytsearch{SEARCH_LIMIT}:{query}"),
        ],
        SEARCH_TIMEOUT,
      )
      .await?;

    // One JSON object per line; skip lines that fail to parse rather than
    // failing the whole search (yt-dlp occasionally interleaves notices).
    Ok(
      stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<YtVideo>(line).ok())
        .filter(|v| !v.id.is_empty())
        .collect(),
    )
  }

  /// Download a video's audio (itag 140 AAC/M4A preferred) to `dest`.
  ///
  /// When `ffmpeg` is on `$PATH`, yt-dlp remuxes the DASH fragments into a
  /// plain MP4 container ("FixupM4a"); without it the file stays fragmented,
  /// which the bundled decoder may reject — the dispatch surfaces an
  /// install-ffmpeg hint on decode failure for that case.
  pub async fn download_audio(&self, video_id: &str, dest: &Path) -> Result<()> {
    // Belt-and-braces: the dispatch already validated the id, but this method
    // is also callable directly (tests, future callers).
    if !video_id
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
      bail!("Invalid YouTube video id: {video_id:?}");
    }
    let dest = dest
      .to_str()
      .ok_or_else(|| anyhow!("Non-UTF-8 tempfile path"))?;

    self
      .run(
        &[
          "-f",
          FORMAT_SELECTOR,
          "--no-playlist",
          // The NamedTempFile already exists (0 bytes); overwrite it.
          "--force-overwrites",
          // Write straight to dest instead of a .part file next to it.
          "--no-part",
          "--no-warnings",
          "--no-progress",
          "--quiet",
          "-o",
          dest,
          &format!("https://www.youtube.com/watch?v={video_id}"),
        ],
        DOWNLOAD_TIMEOUT,
      )
      .await?;
    Ok(())
  }

  /// Run `yt-dlp` with `args`, returning stdout. Fails with the tail of
  /// stderr on a non-zero exit, a clear hint when the binary is missing, and
  /// kills the process on timeout (so a wedged extractor can't leak).
  async fn run(&self, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = Command::new(&self.ytdlp)
      .args(args)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true)
      .spawn()
      .map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
          anyhow!(
            "`{}` not found — install yt-dlp (or set behavior.ytdlp_path)",
            self.ytdlp
          )
        } else {
          anyhow!("failed to spawn `{}`: {e}", self.ytdlp)
        }
      })?;

    // Read both pipes concurrently with wait(); reading stdout after wait()
    // would deadlock once the pipe buffer fills on large outputs.
    let mut stdout_pipe = child.stdout.take().context("yt-dlp stdout not piped")?;
    let mut stderr_pipe = child.stderr.take().context("yt-dlp stderr not piped")?;
    let io = async {
      let (mut out, mut err) = (String::new(), String::new());
      let _ = tokio::join!(
        stdout_pipe.read_to_string(&mut out),
        stderr_pipe.read_to_string(&mut err),
      );
      (out, err)
    };

    let ((stdout, stderr), status) =
      tokio::time::timeout(timeout, async { tokio::join!(io, child.wait()) })
        .await
        .map_err(|_| anyhow!("yt-dlp timed out after {}s", timeout.as_secs()))?;

    let status = status.context("waiting for yt-dlp")?;
    if !status.success() {
      // yt-dlp's real error is the last stderr lines ("ERROR: ...").
      let tail: String = stderr
        .lines()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
      bail!(
        "yt-dlp failed ({}): {}",
        status,
        if tail.is_empty() {
          "no error output"
        } else {
          tail.as_str()
        }
      );
    }
    Ok(stdout)
  }
}

impl Searcher for YouTubeSource {
  async fn search(&self, query: &str) -> Result<SearchResults> {
    let videos = self.search_videos(query).await?;
    Ok(SearchResults {
      tracks: videos.iter().map(video_to_track_info).collect(),
      albums: vec![],
      artists: vec![],
      playlists: vec![],
      shows: vec![],
    })
  }
}

// ---------------------------------------------------------------------------
// Domain type conversions
// ---------------------------------------------------------------------------

/// Map a search row onto the shared [`TrackInfo`] the track table, search
/// results and playbar already render.
///
/// - `uri` = `youtube:<video-id>`.
/// - `artists` (the subtitle column) = the channel name.
/// - `album` = a `"YouTube • 863M views"` style summary.
/// - `duration_ms` = 0 for livestreams (the playbar's LIVE sentinel).
fn video_to_track_info(v: &YtVideo) -> TrackInfo {
  let channel = v
    .channel
    .as_deref()
    .or(v.uploader.as_deref())
    .unwrap_or_default()
    .trim()
    .to_string();

  TrackInfo {
    uri: Some(uri_for_video_id(&v.id)),
    name: v.title.trim().to_string(),
    artists: if channel.is_empty() {
      vec![]
    } else {
      vec![channel]
    },
    album: video_summary(v.view_count),
    duration_ms: v.duration.map(|s| (s * 1000.0) as u64).unwrap_or(0),
    id: Some(v.id.clone()),
    album_id: None,
    artist_refs: vec![],
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    // Flat extraction often omits the top-level `thumbnail`; every video id has
    // a deterministic thumbnail URL, so fall back to that.
    image_url: v
      .thumbnail
      .clone()
      .or_else(|| Some(thumbnail_url_for_video_id(&v.id))),
  }
}

/// Deterministic YouTube thumbnail URL for a video id. `hqdefault.jpg` exists
/// for every video (unlike `maxresdefault`), so it serves as the cover-art
/// fallback when yt-dlp's flat rows or a stored playlist carry no thumbnail.
pub(crate) fn thumbnail_url_for_video_id(video_id: &str) -> String {
  format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

/// `"YouTube • 863M views"`-style one-liner for the album column.
fn video_summary(view_count: Option<u64>) -> String {
  match view_count {
    Some(views) => format!("YouTube \u{2022} {} views", humanize_count(views)),
    None => "YouTube".to_string(),
  }
}

/// `1234` -> `"1.2K"`, `863781229` -> `"863.8M"` — coarse is fine for a
/// subtitle column.
fn humanize_count(n: u64) -> String {
  match n {
    0..=999 => n.to_string(),
    1_000..=999_999 => format!("{:.1}K", n as f64 / 1_000.0),
    1_000_000..=999_999_999 => format!("{:.1}M", n as f64 / 1_000_000.0),
    _ => format!("{:.1}B", n as f64 / 1_000_000_000.0),
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;

  // Two representative flat-search lines (trimmed to the fields we read) plus
  // one junk line that must be skipped.
  const SEARCH_LINES: &str = concat!(
    r#"{"id": "5NV6Rdv1a3I", "title": " Daft Punk - Get Lucky (Official Audio) ", "channel": "Daft Punk", "uploader": "Daft Punk", "duration": 249.0, "view_count": 863781229}"#,
    "\n",
    "not json at all\n",
    r#"{"id": "dQw4w9WgXcQ", "title": "Live now", "uploader": "Some Channel", "duration": null}"#,
    "\n",
  );

  #[test]
  fn parse_search_lines_and_map_to_track_info() {
    let videos: Vec<YtVideo> = SEARCH_LINES
      .lines()
      .filter_map(|l| serde_json::from_str(l).ok())
      .collect();
    assert_eq!(videos.len(), 2, "junk lines must be skipped");

    let row = video_to_track_info(&videos[0]);
    assert_eq!(row.uri.as_deref(), Some("youtube:5NV6Rdv1a3I"));
    assert_eq!(row.name, "Daft Punk - Get Lucky (Official Audio)");
    assert_eq!(row.artists, vec!["Daft Punk"]);
    assert_eq!(row.album, "YouTube \u{2022} 863.8M views");
    assert_eq!(row.duration_ms, 249_000);
    assert_eq!(row.id.as_deref(), Some("5NV6Rdv1a3I"));
    assert!(row.is_playable);

    // No duration (livestream) -> the LIVE-style 0 sentinel; channel falls
    // back to uploader.
    let live = video_to_track_info(&videos[1]);
    assert_eq!(live.duration_ms, 0);
    assert_eq!(live.artists, vec!["Some Channel"]);
    assert_eq!(live.album, "YouTube");
  }

  #[test]
  fn youtube_uri_round_trip() {
    let uri = uri_for_video_id("5NV6Rdv1a3I");
    assert_eq!(uri, "youtube:5NV6Rdv1a3I");
    assert!(is_youtube_uri(&uri));
    assert_eq!(video_id_from_uri(&uri).unwrap(), "5NV6Rdv1a3I");
    // Ids may contain - and _.
    assert_eq!(video_id_from_uri("youtube:a-b_c123").unwrap(), "a-b_c123");
  }

  #[test]
  fn video_id_rejects_injection_shaped_input() {
    // Flags, URL structure, spaces, and other schemes must all be rejected.
    assert!(video_id_from_uri("youtube:--exec=rm").is_err());
    assert!(video_id_from_uri("youtube:id&x=1").is_err());
    assert!(video_id_from_uri("youtube:a b").is_err());
    assert!(video_id_from_uri("youtube:").is_err());
    assert!(video_id_from_uri("spotify:track:x").is_err());
    assert!(!is_youtube_uri("radio:https://x.example/s"));
    // A leading dash alone is caught by charset? No — '-' is allowed in ids,
    // so bare "-flag" survives charset checks. It is defused structurally:
    // the id is embedded in a watch URL, never passed as its own argument.
    assert!(video_id_from_uri("youtube:-abcdefghij").is_ok());
  }

  #[test]
  fn summary_humanizes_view_counts() {
    assert_eq!(video_summary(None), "YouTube");
    assert_eq!(video_summary(Some(999)), "YouTube \u{2022} 999 views");
    assert_eq!(video_summary(Some(1_500)), "YouTube \u{2022} 1.5K views");
    assert_eq!(
      video_summary(Some(863_781_229)),
      "YouTube \u{2022} 863.8M views"
    );
    assert_eq!(
      video_summary(Some(1_200_000_000)),
      "YouTube \u{2022} 1.2B views"
    );
  }

  #[test]
  fn missing_binary_yields_actionable_error() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let source = YouTubeSource::new(Some("definitely-not-a-real-binary".into()));
    let err = rt
      .block_on(source.search_videos("test"))
      .expect_err("missing binary must error");
    assert!(
      err.to_string().contains("install yt-dlp"),
      "error should tell the user what to do, got: {err}"
    );
  }

  /// Live search against real YouTube through yt-dlp. Ignored by default
  /// (network + binary); run with:
  /// `cargo test --features youtube -- --ignored live_search`
  #[tokio::test]
  #[ignore = "requires yt-dlp on PATH and hits live YouTube"]
  async fn live_search_returns_playable_rows() {
    let source = YouTubeSource::new(None);
    let videos = source
      .search_videos("daft punk get lucky")
      .await
      .expect("search should succeed");
    assert!(!videos.is_empty(), "search should match videos");
    for v in &videos {
      let row = video_to_track_info(v);
      let uri = row.uri.expect("row must carry a uri");
      assert!(is_youtube_uri(&uri));
      video_id_from_uri(&uri).expect("id must round-trip");
    }
  }

  /// Live download + decode + play through the shared sink. Ignored (network,
  /// yt-dlp, and an audio output device); run:
  /// `cargo test --features youtube -- --ignored live_download`
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "requires yt-dlp on PATH, network, AND an audio output device"]
  async fn live_download_and_play_through_sink() {
    let source = YouTubeSource::new(None);
    let tmp = tempfile::NamedTempFile::new().unwrap();
    source
      .download_audio("5NV6Rdv1a3I", tmp.path())
      .await
      .expect("download should succeed");
    let len = std::fs::metadata(tmp.path()).unwrap().len();
    assert!(
      len > 100_000,
      "audio file should be non-trivial, got {len}B"
    );

    let player = Arc::new(LocalPlayer::new().expect("open default output device"));
    let path = tmp.path().to_path_buf();
    let decode_player = Arc::clone(&player);
    tokio::task::spawn_blocking(move || decode_player.play_file(&path))
      .await
      .unwrap()
      .expect("m4a should decode and play");

    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
      player.position() >= Duration::from_millis(500),
      "playback position should advance, got {:?}",
      player.position()
    );
    player.stop();
  }
}
