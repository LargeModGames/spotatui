//! YouTube search/playback routing.
//!
//! [`route_youtube_event`] is called from the runtime IoEvent pump *after* the
//! local/Subsonic/radio dispatches and *before* `handle_network_event`. When an
//! event targets the YouTube source (a search request, or a `youtube:` playback
//! URI) it is handled here and consumed; otherwise it falls through to the
//! normal Spotify dispatch.
//!
//! ## Decoupling & device ownership
//!
//! YouTube playback owns a single piece of state, [`App::youtube_playback`],
//! and never writes Spotify/librespot fields — the playbar reads progress/pause
//! live from the player. Only one backend holds the audio device at a time:
//! starting YouTube pauses librespot **and** tears down any local/Subsonic/
//! radio session; the reciprocal teardowns live in those sources' start paths.
//!
//! ## Streaming
//!
//! Each video's audio is downloaded by `yt-dlp` to a tempfile (off the `App`
//! lock — this is the slowest download window of any source), then played from
//! disk through the shared [`LocalPlayer`]. Like Subsonic, a download failure
//! tears the session down rather than skipping past, so a broken extractor or
//! dead network can't cascade error toasts across the whole queue.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::{is_youtube_uri, video_id_from_uri, YouTubePlaybackState, YouTubeSource};
use crate::core::app::{App, SearchResultBlock};
use crate::core::pagination::Paged;
use crate::core::plugin_api::TrackInfo;
use crate::core::source::Searcher;
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;
use crate::infra::queue::{advance_index, replay_file};

/// Skip direction within the YouTube queue.
#[derive(Clone, Copy)]
enum Direction {
  Next,
  Prev,
}

/// Intercept events that target the YouTube source.
///
/// Returns `true` if the event was handled (and must **not** be forwarded to
/// the Spotify network), `false` to let the normal dispatch run.
pub async fn route_youtube_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    IoEvent::GetYouTubeSearchResults(query) => {
      run_youtube_search(app, query).await;
      true
    }
    IoEvent::GetYouTubePlaylists => {
      load_playlists(app).await;
      true
    }
    IoEvent::GetYouTubeTracks(uri) => {
      load_playlist_tracks(app, uri).await;
      true
    }
    IoEvent::CreateYouTubePlaylist(name) => {
      create_playlist(app, name).await;
      true
    }
    IoEvent::DeleteYouTubePlaylist(uri) => {
      delete_playlist(app, uri).await;
      true
    }
    IoEvent::AddTrackToYouTubePlaylist(playlist_ref, video_ref) => {
      add_track_to_playlist(app, playlist_ref, video_ref).await;
      true
    }
    IoEvent::RemoveTrackFromYouTubePlaylist(playlist_ref, video_ref) => {
      remove_track_from_playlist(app, playlist_ref, video_ref).await;
      true
    }
    // Start a queue of youtube videos: queue all and start at the offset.
    IoEvent::StartPlayback(None, Some(uris), offset)
      if uris.first().is_some_and(|u| is_youtube_uri(u)) =>
    {
      start_youtube_queue(app, uris, offset.unwrap_or(0)).await;
      true
    }
    // A single youtube video with no surrounding list: a one-track queue.
    IoEvent::StartPlayback(Some(uri), _, _) if is_youtube_uri(uri) => {
      start_youtube_queue(app, std::slice::from_ref(uri), 0).await;
      true
    }
    // Bare "resume current" — ours only while YouTube owns the session.
    IoEvent::StartPlayback(None, None, None) => match player(app).await {
      Some(p) => {
        p.resume();
        true
      }
      None => false,
    },
    // Any other start is a local/Subsonic/radio/Spotify play: relinquish the
    // device, then let the normal dispatch run.
    IoEvent::StartPlayback(..) => {
      teardown_youtube(app).await;
      false
    }
    IoEvent::PausePlayback => match player(app).await {
      Some(p) => {
        p.pause();
        true
      }
      None => false,
    },
    IoEvent::Seek(position_ms) => match player(app).await {
      Some(p) => {
        let _ = p.seek(Duration::from_millis(*position_ms as u64));
        true
      }
      None => false,
    },
    IoEvent::ChangeVolume(volume) => match player(app).await {
      Some(p) => {
        p.set_volume(*volume as f32 / 100.0);
        app.lock().await.user_config.behavior.volume_percent = *volume;
        true
      }
      None => false,
    },
    IoEvent::NextTrack => skip(app, Direction::Next).await,
    IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => skip(app, Direction::Prev).await,
    // Repeat-one auto-advance: replay the current video (dispatched by the runner
    // tick only while YouTube owns the session).
    IoEvent::ReplayCurrentTrack => replay_current(app).await,
    // Explicit repeat-mode set (e.g. a plugin's `set_repeat`): store it as the
    // decoded repeat mode while a queueable decoded source owns playback (the
    // method's ownership gate excludes a context suspended under the native
    // queue), so it never reaches the Spotify context.
    IoEvent::Repeat(state) => app.lock().await.set_decoded_repeat_from_state(*state),
    _ => false,
  }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// Build a [`YouTubeSource`] from the user config (`behavior.ytdlp_path`
/// override, else `yt-dlp` on `$PATH`). Unlike Subsonic there is nothing that
/// can be "unconfigured" — a missing binary surfaces as an actionable error
/// from the first search instead.
pub(crate) async fn build_source(app: &Arc<Mutex<App>>) -> YouTubeSource {
  let ytdlp_path = app.lock().await.user_config.behavior.ytdlp_path.clone();
  YouTubeSource::new(ytdlp_path)
}

/// Run a YouTube search and populate `app.search_results`. Like the Subsonic
/// M2 search, only the songs block is populated — each row is a video; Enter
/// dispatches the `youtube:` URIs back through this module.
async fn run_youtube_search(app: &Arc<Mutex<App>>, query: &str) {
  let source = build_source(app).await;
  match source.search(query).await {
    Ok(results) => {
      let total = results.tracks.len() as u32;
      let mut app = app.lock().await;
      app.search_results.tracks = Some(Paged {
        items: results.tracks,
        total,
        ..Default::default()
      });
      app.search_results.albums = None;
      app.search_results.artists = None;
      app.search_results.playlists = None;
      app.search_results.shows = None;
      // Focus the songs block so the first hit is selectable immediately.
      app.search_results.selected_tracks_index = Some(0);
      app.search_results.hovered_block = SearchResultBlock::SongSearch;
      app.search_results.selected_block = SearchResultBlock::Empty;
    }
    Err(e) => set_error(app, format!("YouTube search failed: {e}")).await,
  }
}

// ---------------------------------------------------------------------------
// Local playlists (youtube_playlists.yml)
// ---------------------------------------------------------------------------

/// Run a load→mutate→save transaction against the playlists file on the
/// blocking pool (never on the async executor, and never under the `App`
/// lock). Returns whatever the mutation returns.
async fn with_playlists_file<T, F>(mutate: F) -> Result<T>
where
  T: Send + 'static,
  F: FnOnce(&mut super::playlists::PlaylistsFile) -> Result<T> + Send + 'static,
{
  tokio::task::spawn_blocking(move || {
    let path = super::playlists::default_playlists_path()?;
    let mut file = super::playlists::load(&path)?;
    let out = mutate(&mut file)?;
    super::playlists::save(&path, &file)?;
    Ok(out)
  })
  .await
  .context("YouTube playlists task failed")?
}

/// Read-only load of the playlists file on the blocking pool.
async fn read_playlists_file() -> Result<super::playlists::PlaylistsFile> {
  tokio::task::spawn_blocking(|| {
    let path = super::playlists::default_playlists_path()?;
    super::playlists::load(&path)
  })
  .await
  .context("YouTube playlists task failed")?
}

/// Refresh the sidebar list in `App` from an already-loaded file.
async fn publish_playlists(app: &Arc<Mutex<App>>, file: &super::playlists::PlaylistsFile) {
  let infos: Vec<_> = file
    .playlists
    .iter()
    .map(super::playlists::playlist_to_info)
    .collect();
  app.lock().await.youtube_playlists = infos;
}

/// Load `youtube_playlists.yml` into the sidebar.
async fn load_playlists(app: &Arc<Mutex<App>>) {
  match read_playlists_file().await {
    Ok(file) => publish_playlists(app, &file).await,
    Err(e) => set_error(app, format!("Cannot load YouTube playlists: {e}")).await,
  }
}

/// Open a playlist's saved tracks in the shared track table.
async fn load_playlist_tracks(app: &Arc<Mutex<App>>, uri: &str) {
  let file = match read_playlists_file().await {
    Ok(f) => f,
    Err(e) => {
      set_error(app, format!("Cannot load YouTube playlists: {e}")).await;
      return;
    }
  };
  let Some(playlist) = super::playlists::find_playlist(&file, uri) else {
    set_error(app, "That YouTube playlist no longer exists".to_string()).await;
    return;
  };
  let rows: Vec<TrackInfo> = playlist
    .tracks
    .iter()
    .map(super::playlists::stored_to_track_info)
    .collect();
  let mut guard = app.lock().await;
  guard.track_table.tracks = rows;
  guard.track_table.selected_index = 0;
  guard.track_table.context = Some(crate::core::app::TrackTableContext::YouTubePlaylist);
  guard.youtube_open_playlist = Some(uri.to_string());
}

/// Create a playlist and refresh the sidebar.
async fn create_playlist(app: &Arc<Mutex<App>>, name: &str) {
  let name_owned = name.to_string();
  let result = with_playlists_file(move |file| {
    super::playlists::create_playlist(file, &name_owned)?;
    Ok(file.clone())
  })
  .await;
  match result {
    Ok(file) => {
      publish_playlists(app, &file).await;
      let mut guard = app.lock().await;
      guard.set_status_message(format!("Created YouTube playlist \"{}\"", name.trim()), 4);
    }
    Err(e) => set_error(app, format!("Cannot create YouTube playlist: {e}")).await,
  }
}

/// Delete a playlist and refresh the sidebar.
async fn delete_playlist(app: &Arc<Mutex<App>>, uri: &str) {
  let uri_owned = uri.to_string();
  let result = with_playlists_file(move |file| {
    let deleted = super::playlists::delete_playlist(file, &uri_owned)?;
    Ok((deleted.name, file.clone()))
  })
  .await;
  match result {
    Ok((name, file)) => {
      publish_playlists(app, &file).await;
      let mut guard = app.lock().await;
      // The deleted playlist may be open in the track table; drop the marker so
      // the remove-track flow can't edit a ghost.
      if guard.youtube_open_playlist.as_deref() == Some(uri) {
        guard.youtube_open_playlist = None;
      }
      guard.set_status_message(format!("Deleted YouTube playlist \"{name}\""), 4);
    }
    Err(e) => set_error(app, format!("Cannot delete YouTube playlist: {e}")).await,
  }
}

/// Find a video's browse row by bare id or `youtube:` URI, in the search
/// results or the track table (whichever view the add originated from).
fn find_video_row(app: &App, video_ref: &str) -> Option<TrackInfo> {
  let uri = super::uri_for_video_id(video_ref);
  let matches = |t: &&TrackInfo| {
    t.id.as_deref() == Some(video_ref)
      || t.uri.as_deref() == Some(video_ref)
      || t.uri.as_deref() == Some(uri.as_str())
  };
  app
    .search_results
    .tracks
    .as_ref()
    .and_then(|p| p.items.iter().find(matches))
    .or_else(|| app.track_table.tracks.iter().find(matches))
    .cloned()
}

/// Add a video to a playlist, resolving its metadata from the browse views.
async fn add_track_to_playlist(app: &Arc<Mutex<App>>, playlist_ref: &str, video_ref: &str) {
  let row = {
    let guard = app.lock().await;
    find_video_row(&guard, video_ref)
  };
  let Some(row) = row else {
    set_error(app, "Cannot resolve that video's metadata".to_string()).await;
    return;
  };
  let stored = match super::playlists::track_info_to_stored(&row) {
    Ok(s) => s,
    Err(e) => {
      set_error(app, format!("Cannot add video to playlist: {e}")).await;
      return;
    }
  };

  let playlist_owned = playlist_ref.to_string();
  let result = with_playlists_file(move |file| {
    let added = super::playlists::add_track(file, &playlist_owned, stored)?;
    let name = super::playlists::find_playlist(file, &playlist_owned)
      .map(|p| p.name.clone())
      .unwrap_or_default();
    Ok((added, name, file.clone()))
  })
  .await;

  match result {
    Ok((added, playlist_name, file)) => {
      publish_playlists(app, &file).await;
      let msg = if added {
        format!("Added \"{}\" to {playlist_name}", row.name)
      } else {
        format!("\"{}\" is already in {playlist_name}", row.name)
      };
      app.lock().await.set_status_message(msg, 4);
    }
    Err(e) => set_error(app, format!("Cannot add video to playlist: {e}")).await,
  }
}

/// Remove a video from a playlist; refreshes the open track table when it
/// shows that playlist.
async fn remove_track_from_playlist(app: &Arc<Mutex<App>>, playlist_ref: &str, video_ref: &str) {
  // The remove dialog passes the bare video id; normalize from a URI too.
  let video_id = super::video_id_from_uri(video_ref)
    .map(str::to_owned)
    .unwrap_or_else(|_| video_ref.to_string());

  let playlist_owned = playlist_ref.to_string();
  let result = with_playlists_file(move |file| {
    let removed = super::playlists::remove_track(file, &playlist_owned, &video_id)?;
    let remaining: Vec<_> = super::playlists::find_playlist(file, &playlist_owned)
      .map(|p| {
        p.tracks
          .iter()
          .map(super::playlists::stored_to_track_info)
          .collect()
      })
      .unwrap_or_default();
    Ok((removed.title, remaining, file.clone()))
  })
  .await;

  match result {
    Ok((title, remaining, file)) => {
      publish_playlists(app, &file).await;
      let mut guard = app.lock().await;
      // Keep the open track table in sync with the file.
      if guard.youtube_open_playlist.as_deref() == Some(playlist_ref) {
        let len = remaining.len();
        guard.track_table.tracks = remaining;
        guard.track_table.selected_index =
          guard.track_table.selected_index.min(len.saturating_sub(1));
      }
      guard.set_status_message(format!("Removed \"{title}\" from playlist"), 4);
    }
    Err(e) => set_error(app, format!("Cannot remove video from playlist: {e}")).await,
  }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// The live YouTube player, if a YouTube session is active.
async fn player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app
    .lock()
    .await
    .youtube_playback
    .as_ref()
    .map(|s| Arc::clone(&s.player))
}

/// Snapshot the `TrackInfo`s for `uris`, preserving order, looking each up in
/// both browse views a play can originate from (the shared track table and the
/// search results). Any uri found in neither is dropped.
fn snapshot_tracks(
  table: &[TrackInfo],
  search: Option<&[TrackInfo]>,
  uris: &[String],
) -> Vec<TrackInfo> {
  uris
    .iter()
    .filter_map(|uri| {
      let matches = |t: &&TrackInfo| t.uri.as_deref() == Some(uri.as_str());
      table
        .iter()
        .find(matches)
        .or_else(|| search.and_then(|s| s.iter().find(matches)))
        .cloned()
    })
    .collect()
}

/// Release the other backends so only YouTube holds the output device.
async fn release_other_backends(app: &Arc<Mutex<App>>) {
  // In a youtube-only build every block below is compiled out.
  let _ = app;
  // Pause native Spotify so librespot releases the device.
  #[cfg(feature = "streaming")]
  {
    let streaming = app.lock().await.streaming_player.clone();
    if let Some(player) = streaming {
      player.pause();
    }
  }
  // Tear down any local-file session (dropping it releases its device handle).
  #[cfg(feature = "local-files")]
  {
    let local = app.lock().await.local_playback.take();
    if let Some(local) = local {
      local.player.stop();
    }
  }
  // Tear down any Subsonic session.
  #[cfg(feature = "subsonic")]
  {
    let subsonic = app.lock().await.subsonic_playback.take();
    if let Some(subsonic) = subsonic {
      subsonic.player.stop();
    }
  }
  // Tear down any radio session.
  #[cfg(feature = "internet-radio")]
  {
    let radio = app.lock().await.radio_playback.take();
    if let Some(radio) = radio {
      radio.player.stop();
    }
  }
}

/// Reuse the live YouTube player, or open a fresh output device for one. A
/// freshly opened player is **not** published to `App` here — the caller
/// publishes the session only on a successful first play.
async fn acquire_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(p) = player(app).await {
    return Some(p);
  }
  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_error(app, format!("No audio output for YouTube playback: {e}")).await;
      None
    }
    Err(e) => {
      set_error(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

/// Download a video's audio into a fresh tempfile. Must be awaited **without**
/// holding the `App` lock (this is the slowest download of any source).
async fn download_audio(source: &YouTubeSource, video_id: &str) -> Result<NamedTempFile> {
  let tmp = NamedTempFile::with_suffix(".m4a").context("creating temp file for YouTube audio")?;
  source.download_audio(video_id, tmp.path()).await?;
  Ok(tmp)
}

/// Download the audio for `uri` into a tempfile, for the native queue engine's
/// off-pump fetch (playing is the queue's job — it re-checks that the queue
/// slot is still current before touching the shared player).
pub(crate) async fn download_for_queue(source: &YouTubeSource, uri: &str) -> Result<NamedTempFile> {
  let video_id = video_id_from_uri(uri)?.to_string();
  download_audio(source, &video_id).await
}

/// Decode-failure hint: without ffmpeg on `$PATH`, yt-dlp leaves the download
/// as a fragmented DASH container some decoders reject.
fn decode_hint(e: impl std::fmt::Display) -> String {
  format!("Cannot play YouTube audio: {e} (installing ffmpeg fixes most container issues)")
}

/// Begin playing a queue of YouTube videos, taking over the session and
/// starting at `start_idx` (clamped into range).
async fn start_youtube_queue(app: &Arc<Mutex<App>>, uris: &[String], start_idx: usize) {
  // Snapshot the track metadata under one short lock, from whichever browse
  // view the request came from.
  let tracks = {
    let guard = app.lock().await;
    let search = guard
      .search_results
      .tracks
      .as_ref()
      .map(|p| p.items.as_slice());
    snapshot_tracks(&guard.track_table.tracks, search, uris)
  };
  if tracks.is_empty() {
    set_error(app, "No YouTube videos to play".to_string()).await;
    return;
  }
  let index = start_idx.min(tracks.len() - 1);

  // A livestream (duration 0 in the search row) never finishes downloading —
  // the tempfile fetch would just spin until the timeout. Refuse up front.
  if tracks[index].duration_ms == 0 {
    set_error(
      app,
      "Live streams aren't supported on the YouTube source yet".to_string(),
    )
    .await;
    return;
  }

  let video_id = match tracks[index].uri.as_deref().map(video_id_from_uri) {
    Some(Ok(id)) => id.to_string(),
    _ => {
      set_error(app, "Invalid YouTube URI".to_string()).await;
      return;
    }
  };

  let source = Arc::new(build_source(app).await);

  // Only one backend owns the device at a time.
  release_other_backends(app).await;

  let Some(player) = acquire_player(app).await else {
    return;
  };

  // yt-dlp can take a while; let the user know something is happening.
  {
    let mut guard = app.lock().await;
    guard.set_status_message(format!("Fetching {}\u{2026}", tracks[index].name), 30);
  }

  // Download off the lock, then decode on the blocking pool.
  let tmp = match download_audio(&source, &video_id).await {
    Ok(t) => t,
    Err(e) => {
      set_error(app, format!("Cannot download YouTube audio: {e}")).await;
      return;
    }
  };
  let path = tmp.path().to_path_buf();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || decode_player.play_file(&path)).await;

  match result {
    Ok(Ok(())) => {
      let volume = app.lock().await.user_config.behavior.volume_percent;
      player.set_volume(volume as f32 / 100.0);

      let display = tracks[index].name.clone();
      let mut guard = app.lock().await;
      // Publish the session exactly once, now that the source is decoding.
      let mut state = YouTubePlaybackState {
        player,
        source,
        tracks,
        index,
        advancing: false,
        tempfile: tmp,
        shuffle_backup: None,
      };
      // Honor the player-global decoded shuffle for the freshly-built queue.
      if guard.decoded_shuffle {
        state.set_shuffle(true);
      }
      guard.youtube_playback = Some(state);
      guard.set_status_message(format!("\u{266a} {display}"), 4);
    }
    Ok(Err(e)) => set_error(app, decode_hint(e)).await,
    Err(e) => set_error(app, format!("YouTube playback task failed: {e}")).await,
  }
}

/// Move the YouTube queue index in `direction` and play the new track. Returns
/// `true` if YouTube owns the session (so the event is consumed).
async fn skip(app: &Arc<Mutex<App>>, direction: Direction) -> bool {
  let target = {
    let mut guard = app.lock().await;
    // Repeat-all wraps at the ends; Off/Repeat-one clamp.
    let mode = guard.decoded_repeat;
    let Some(s) = guard.youtube_playback.as_mut() else {
      return false; // not ours
    };
    // Guard the empty-sink download window from spurious auto-advance.
    s.advancing = true;
    let forward = matches!(direction, Direction::Next);
    advance_index(s.index, s.tracks.len(), mode, forward)
  };

  match target {
    Some(idx) => play_index(app, idx).await,
    None => {
      // Queue boundary: clear the optimistic guard so it doesn't wedge
      // auto-advance off for the rest of the track.
      if let Some(s) = app.lock().await.youtube_playback.as_mut() {
        s.advancing = false;
      }
    }
  }
  true
}

/// Replay the current YouTube video (repeat-one auto-advance) via
/// [`replay_file`]. Re-decodes the **already-downloaded tempfile** — no second
/// `yt-dlp` fetch — and on failure tears the session down (rather than leaving
/// an empty sink that would re-fire replay every tick). Returns `true` if
/// YouTube owns the session.
async fn replay_current(app: &Arc<Mutex<App>>) -> bool {
  let (player, path) = {
    let mut guard = app.lock().await;
    let Some(s) = guard.youtube_playback.as_mut() else {
      return false; // not ours
    };
    s.advancing = true;
    (Arc::clone(&s.player), s.tempfile.path().to_path_buf())
  };
  if replay_file(player, path).await {
    if let Some(s) = app.lock().await.youtube_playback.as_mut() {
      s.advancing = false;
    }
  } else {
    teardown_youtube(app).await;
    set_error(app, "Cannot replay YouTube track".to_string()).await;
  }
  true
}

/// What the locked snapshot in [`play_index`] decided to do.
enum Plan {
  Play(Arc<LocalPlayer>, Arc<YouTubeSource>, String, String),
  OutOfRange,
  BadUri,
  /// The target row is a livestream (duration 0) — its download never ends.
  Live,
}

/// Play the queued track at `target`, reusing the published session. Used by
/// Next/Previous and the runner tick's auto-advance.
///
/// A download failure tears the session down (same rationale as Subsonic): on
/// a network outage or a broken extractor, walking the whole queue at tick
/// speed would spray error toasts, so one failure ends playback instead.
pub(crate) async fn play_index(app: &Arc<Mutex<App>>, target: usize) {
  let plan = {
    let guard = app.lock().await;
    match guard.youtube_playback.as_ref() {
      None => return, // session torn down between dispatch and here
      Some(s) => match s.tracks.get(target) {
        None => Plan::OutOfRange,
        Some(track) if track.duration_ms == 0 => Plan::Live,
        Some(track) => match track.uri.as_deref().map(video_id_from_uri) {
          Some(Ok(id)) => Plan::Play(
            Arc::clone(&s.player),
            Arc::clone(&s.source),
            id.to_string(),
            track.name.clone(),
          ),
          _ => Plan::BadUri,
        },
      },
    }
  };

  let (player, source, video_id, name) = match plan {
    Plan::Play(p, s, id, name) => (p, s, id, name),
    Plan::OutOfRange => {
      if let Some(s) = app.lock().await.youtube_playback.as_mut() {
        s.advancing = false;
      }
      return;
    }
    Plan::BadUri => {
      teardown_youtube(app).await;
      set_error(app, "Invalid YouTube URI".to_string()).await;
      return;
    }
    Plan::Live => {
      // Same semantics as a download failure: end the session rather than
      // leave an empty sink + unmoved index, which would make auto-advance
      // re-dispatch onto the same livestream row every tick.
      teardown_youtube(app).await;
      set_error(
        app,
        "Live streams aren't supported on the YouTube source yet".to_string(),
      )
      .await;
      return;
    }
  };

  {
    let mut guard = app.lock().await;
    guard.set_status_message(format!("Fetching {name}\u{2026}"), 30);
  }

  let tmp = match download_audio(&source, &video_id).await {
    Ok(t) => t,
    Err(e) => {
      teardown_youtube(app).await;
      set_error(app, format!("Cannot download YouTube audio: {e}")).await;
      return;
    }
  };
  let path = tmp.path().to_path_buf();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || decode_player.play_file(&path)).await;

  match result {
    Ok(Ok(())) => commit_index(app, target, tmp).await,
    Ok(Err(e)) => {
      teardown_youtube(app).await;
      set_error(app, decode_hint(e)).await;
    }
    Err(e) => {
      teardown_youtube(app).await;
      set_error(app, format!("YouTube playback task failed: {e}")).await;
    }
  }
}

/// Commit `target` as the live index, clear the auto-advance guard, and swap
/// in the new track's tempfile (dropping the previous one). Ordering is safe:
/// the blocking `play_file` already cleared the old source from the sink, so
/// rodio no longer holds the old file by the time it is dropped here.
async fn commit_index(app: &Arc<Mutex<App>>, target: usize, tmp: NamedTempFile) {
  let mut guard = app.lock().await;
  let display = if let Some(s) = guard.youtube_playback.as_mut() {
    s.index = target;
    s.advancing = false;
    s.tempfile = tmp;
    s.tracks.get(target).map(|t| t.name.clone())
  } else {
    None
  };
  if let Some(display) = display {
    guard.set_status_message(format!("\u{266a} {display}"), 4);
  }
}

/// End the YouTube session, releasing the output device and cleaning up the
/// current tempfile.
pub async fn teardown_youtube(app: &Arc<Mutex<App>>) {
  if let Some(s) = app.lock().await.youtube_playback.take() {
    s.player.stop();
    // Dropping `s` drops the tempfile (cleanup) and, if it held the last
    // reference, the keepalive thread exits and the device is released.
  }
}

async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn test_app() -> App {
    let (tx, _rx) = channel();
    App::new(tx, UserConfig::new(), Some(SystemTime::now()))
  }

  fn video(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(uri.to_string()),
      name: name.to_string(),
      artists: vec!["Channel".to_string()],
      album: "YouTube".to_string(),
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
  fn snapshot_finds_tracks_in_table_preserving_order() {
    let table = vec![video("youtube:aaa", "A"), video("youtube:bbb", "B")];
    let snap = snapshot_tracks(
      &table,
      None,
      &["youtube:bbb".to_string(), "youtube:aaa".to_string()],
    );
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].name, "B");
    assert_eq!(snap[1].name, "A");
  }

  #[test]
  fn snapshot_falls_back_to_search_results() {
    // Search->play is the primary YouTube path: the track table may hold a
    // stale Spotify playlist while the played uris come from the search view.
    let table = vec![video("spotify:track:x", "Spotify Row")];
    let search = vec![video("youtube:searched123", "Searched")];
    let snap = snapshot_tracks(&table, Some(&search), &["youtube:searched123".to_string()]);
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].name, "Searched");
  }

  #[test]
  fn snapshot_drops_unknown_uris() {
    let table = vec![video("youtube:aaa", "A")];
    let snap = snapshot_tracks(&table, None, &["youtube:missing".to_string()]);
    assert!(snap.is_empty());
  }

  /// Search/transport events that are not YouTube's must fall through.
  #[tokio::test]
  async fn foreign_events_fall_through_without_a_session() {
    let app = Arc::new(Mutex::new(test_app()));
    // No YouTube session: transport events are not ours.
    assert!(!route_youtube_event(&app, &IoEvent::PausePlayback).await);
    assert!(!route_youtube_event(&app, &IoEvent::NextTrack).await);
    assert!(!route_youtube_event(&app, &IoEvent::Seek(1000)).await);
    assert!(!route_youtube_event(&app, &IoEvent::StartPlayback(None, None, None)).await);
    // A Spotify start falls through (and there is nothing to tear down).
    assert!(
      !route_youtube_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None)
      )
      .await
    );
    // Other sources' URIs are not ours either.
    assert!(
      !route_youtube_event(
        &app,
        &IoEvent::StartPlayback(Some("radio:https://x.example/s".to_string()), None, None)
      )
      .await
    );
  }

  /// A youtube: start with no matching browse row must be consumed (it is
  /// ours) but publish nothing — and must not panic.
  #[tokio::test]
  async fn youtube_start_with_no_rows_is_consumed_without_session() {
    let app = Arc::new(Mutex::new(test_app()));
    assert!(
      route_youtube_event(
        &app,
        &IoEvent::StartPlayback(None, Some(vec!["youtube:abc123".to_string()]), Some(0))
      )
      .await
    );
    assert!(app.lock().await.youtube_playback.is_none());
  }

  /// Livestream rows (duration 0) are refused up front with a clear message —
  /// their download never finishes, so starting one would hang "Fetching…"
  /// until the timeout.
  #[tokio::test]
  async fn livestream_rows_are_refused_without_session() {
    let app = Arc::new(Mutex::new(test_app()));
    {
      let mut guard = app.lock().await;
      let mut live = video("youtube:live12345", "24/7 Lofi Radio");
      live.duration_ms = 0;
      guard.search_results.tracks = Some(Paged {
        items: vec![live],
        total: 1,
        ..Default::default()
      });
    }
    assert!(
      route_youtube_event(
        &app,
        &IoEvent::StartPlayback(None, Some(vec!["youtube:live12345".to_string()]), Some(0))
      )
      .await,
      "livestream start is ours (consumed)"
    );
    let guard = app.lock().await;
    assert!(guard.youtube_playback.is_none(), "no session published");
    assert!(
      guard
        .status_message
        .as_deref()
        .is_some_and(|m| m.contains("Live streams")),
      "user gets an actionable message, got {:?}",
      guard.status_message
    );
  }

  /// Full local-playlist lifecycle through the dispatch, exactly as the
  /// runtime pump drives it: create → list → add (metadata resolved from a
  /// fake search view) → open → duplicate-add no-op → remove → delete. No
  /// network, no audio device — the file lives in a tempdir via the
  /// `SPOTATUI_YOUTUBE_PLAYLISTS_PATH` override. Kept as ONE test because the
  /// override is process-global env.
  #[tokio::test]
  async fn playlist_lifecycle_through_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yt_playlists.yml");
    std::env::set_var(super::super::playlists::PATH_ENV, &path);

    let app = Arc::new(Mutex::new(test_app()));

    // Empty file → empty sidebar list.
    assert!(route_youtube_event(&app, &IoEvent::GetYouTubePlaylists).await);
    assert!(app.lock().await.youtube_playlists.is_empty());

    // Create a playlist; the sidebar refreshes.
    assert!(route_youtube_event(&app, &IoEvent::CreateYouTubePlaylist("Focus".to_string())).await);
    let playlist_uri = {
      let guard = app.lock().await;
      assert_eq!(guard.youtube_playlists.len(), 1);
      assert_eq!(guard.youtube_playlists[0].name, "Focus");
      guard.youtube_playlists[0].uri.clone()
    };

    // Fake a search view holding the video's metadata, then add it by id —
    // exactly what the picker dialog dispatches.
    {
      let mut guard = app.lock().await;
      guard.search_results.tracks = Some(Paged {
        items: vec![video("youtube:vid1234", "Cool Song")],
        total: 1,
        ..Default::default()
      });
      // The search row's id field carries the bare video id.
      guard.search_results.tracks.as_mut().unwrap().items[0].id = Some("vid1234".to_string());
    }
    assert!(
      route_youtube_event(
        &app,
        &IoEvent::AddTrackToYouTubePlaylist(playlist_uri.clone(), "vid1234".to_string())
      )
      .await
    );
    assert_eq!(
      app.lock().await.youtube_playlists[0].track_count,
      1,
      "sidebar count must refresh after an add"
    );

    // Adding the same video again is a friendly no-op.
    assert!(
      route_youtube_event(
        &app,
        &IoEvent::AddTrackToYouTubePlaylist(playlist_uri.clone(), "vid1234".to_string())
      )
      .await
    );
    assert_eq!(app.lock().await.youtube_playlists[0].track_count, 1);

    // Open the playlist into the shared track table.
    assert!(route_youtube_event(&app, &IoEvent::GetYouTubeTracks(playlist_uri.clone())).await);
    {
      let guard = app.lock().await;
      assert_eq!(guard.track_table.tracks.len(), 1);
      assert_eq!(guard.track_table.tracks[0].name, "Cool Song");
      assert_eq!(
        guard.track_table.tracks[0].uri.as_deref(),
        Some("youtube:vid1234")
      );
      assert_eq!(
        guard.youtube_open_playlist.as_deref(),
        Some(playlist_uri.as_str())
      );
    }

    // Remove the track; the open table refreshes in place.
    assert!(
      route_youtube_event(
        &app,
        &IoEvent::RemoveTrackFromYouTubePlaylist(playlist_uri.clone(), "vid1234".to_string())
      )
      .await
    );
    {
      let guard = app.lock().await;
      assert!(
        guard.track_table.tracks.is_empty(),
        "open table must refresh"
      );
      assert_eq!(guard.youtube_playlists[0].track_count, 0);
    }

    // Delete the playlist; the sidebar refreshes and the open marker clears.
    assert!(route_youtube_event(&app, &IoEvent::DeleteYouTubePlaylist(playlist_uri)).await);
    {
      let guard = app.lock().await;
      assert!(guard.youtube_playlists.is_empty());
      assert!(guard.youtube_open_playlist.is_none());
    }

    // And the file on disk reflects all of it (empty playlists list).
    let on_disk = super::super::playlists::load(&path).unwrap();
    assert!(on_disk.playlists.is_empty());
  }

  /// End-to-end dispatch test: drive `route_youtube_event` exactly as the
  /// runtime pump does — search, play a result, advance the queue, and tear
  /// down on a foreign start. Ignored (needs yt-dlp, network, AND an audio
  /// output device); run:
  /// `cargo test --features youtube -- --ignored live_dispatch`
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "requires yt-dlp on PATH, network, AND an audio output device"]
  async fn live_dispatch_search_play_and_advance() {
    let app = Arc::new(Mutex::new(test_app()));

    // Search populates the songs block.
    assert!(
      route_youtube_event(
        &app,
        &IoEvent::GetYouTubeSearchResults("daft punk get lucky".to_string())
      )
      .await
    );
    let uris: Vec<String> = app
      .lock()
      .await
      .search_results
      .tracks
      .as_ref()
      .expect("search populated the songs block")
      .items
      .iter()
      .filter_map(|t| t.uri.clone())
      .collect();
    assert!(uris.len() >= 2, "need >=2 results to test advance");

    // Start the queue at index 0 — downloads + plays the first video.
    assert!(
      route_youtube_event(&app, &IoEvent::StartPlayback(None, Some(uris), Some(0))).await,
      "youtube StartPlayback must be consumed"
    );
    {
      let guard = app.lock().await;
      let s = guard.youtube_playback.as_ref().expect("session published");
      assert_eq!(s.index, 0);
      assert!(!s.player.is_paused(), "should be playing");
    }

    // Pause/resume are ours while the session lives.
    assert!(route_youtube_event(&app, &IoEvent::PausePlayback).await);
    assert!(app
      .lock()
      .await
      .youtube_playback
      .as_ref()
      .is_some_and(|s| s.player.is_paused()));
    assert!(route_youtube_event(&app, &IoEvent::StartPlayback(None, None, None)).await);

    // Advance — downloads + plays the next video, moving the index.
    assert!(route_youtube_event(&app, &IoEvent::NextTrack).await);
    {
      let guard = app.lock().await;
      let s = guard
        .youtube_playback
        .as_ref()
        .expect("session still active");
      assert_eq!(s.index, 1, "Next should advance the queue index");
    }

    // A foreign (Spotify) start tears the session down and falls through.
    assert!(
      !route_youtube_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None)
      )
      .await,
      "a non-youtube start must fall through to the network"
    );
    assert!(
      app.lock().await.youtube_playback.is_none(),
      "a foreign start must tear down the YouTube session (device handoff)"
    );
  }
}
