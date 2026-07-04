//! Subsonic browse/search/playback routing.
//!
//! The seam that keeps the Spotify [`Network`](crate::infra::network)
//! Spotify-only: [`route_subsonic_event`] is called from the runtime IoEvent
//! pump *after* the local-files dispatch and *before* `handle_network_event`.
//! When an event targets the Subsonic source (a browse/search request, or a
//! `subsonic:` playback URI) it is handled here and consumed; otherwise it falls
//! through to the normal Spotify dispatch.
//!
//! Unlike the local-files source, Subsonic's [`MediaSource`]/[`Searcher`] methods
//! are genuinely async (reqwest REST calls), so they are awaited directly rather
//! than run on the blocking pool.
//!
//! ## Decoupling & device ownership
//!
//! Subsonic playback owns a single piece of state, [`App::subsonic_playback`],
//! and never writes Spotify/librespot fields — the playbar reads progress/pause
//! live from the player. Only one backend holds the audio device at a time:
//! starting Subsonic pauses librespot **and** tears down any local session; the
//! reciprocal teardown lives in the local and network start paths.
//!
//! ## Streaming
//!
//! Each track is downloaded from `stream.view` to a tempfile (off the `App`
//! lock), then played from disk through the shared [`LocalPlayer`]. The download
//! window is much longer than a local decode, so the `advancing` guard is
//! load-bearing, and a download failure tears the session down rather than
//! skipping past (avoids cascading the whole queue when the network is down).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;
use tokio::sync::Mutex;

use super::{next_index, prev_index, track_id_from_uri, SubsonicPlaybackState, SubsonicSource};
use crate::core::app::{App, SearchResultBlock, TrackTableContext};
use crate::core::pagination::Paged;
use crate::core::plugin_api::TrackInfo;
use crate::core::source::{MediaSource, Searcher};
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;

/// Environment variable that overrides the configured Subsonic password. Prefer
/// it over the plaintext config field so the secret is never written to disk.
const PASSWORD_ENV: &str = "SPOTATUI_SUBSONIC_PASSWORD";

/// Whether a URI is owned by the Subsonic source.
fn is_subsonic_uri(uri: &str) -> bool {
  uri.starts_with("subsonic:")
}

/// Skip direction within the subsonic queue.
#[derive(Clone, Copy)]
enum Direction {
  Next,
  Prev,
}

/// Intercept events that target the Subsonic source.
///
/// Returns `true` if the event was handled (and must **not** be forwarded to the
/// Spotify network), `false` to let the normal dispatch run.
pub async fn route_subsonic_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    IoEvent::GetSubsonicPlaylists => {
      load_subsonic_playlists(app).await;
      true
    }
    IoEvent::GetSubsonicTracks(uri) => {
      load_subsonic_tracks(app, uri).await;
      true
    }
    IoEvent::GetSubsonicSearchResults(query) => {
      run_subsonic_search(app, query).await;
      true
    }
    // Start a playlist of subsonic tracks: queue all and start at the offset.
    IoEvent::StartPlayback(None, Some(uris), offset)
      if uris.first().is_some_and(|u| is_subsonic_uri(u)) =>
    {
      start_subsonic_queue(app, uris, offset.unwrap_or(0)).await;
      true
    }
    // A single subsonic track with no surrounding playlist: a one-track queue.
    IoEvent::StartPlayback(Some(uri), _, _) if is_subsonic_uri(uri) => {
      start_subsonic_queue(app, std::slice::from_ref(uri), 0).await;
      true
    }
    // Bare "resume current" — ours only while subsonic owns the session.
    IoEvent::StartPlayback(None, None, None) => match player(app).await {
      Some(p) => {
        p.resume();
        true
      }
      None => false,
    },
    // Any other start is a local/Spotify play: relinquish the device, then let
    // the normal dispatch run.
    IoEvent::StartPlayback(..) => {
      teardown_subsonic(app).await;
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
    _ => false,
  }
}

// ---------------------------------------------------------------------------
// Browse + search
// ---------------------------------------------------------------------------

/// Build a [`SubsonicSource`] from the saved server config, with the password
/// taken from the `SPOTATUI_SUBSONIC_PASSWORD` env var when set. Returns `None`
/// (after surfacing a status message) when no server URL is configured.
async fn build_source(app: &Arc<Mutex<App>>) -> Option<SubsonicSource> {
  let (url, username, config_password) = {
    let guard = app.lock().await;
    let behavior = &guard.user_config.behavior;
    (
      behavior.subsonic_url.clone(),
      behavior.subsonic_username.clone(),
      behavior.subsonic_password.clone(),
    )
  };

  let Some(url) = url else {
    set_error(
      app,
      "No Subsonic server configured (set behavior.subsonic_url)".to_string(),
    )
    .await;
    return None;
  };

  // Env override takes precedence over the plaintext config field.
  let password = std::env::var(PASSWORD_ENV)
    .ok()
    .or(config_password)
    .unwrap_or_default();

  Some(SubsonicSource::new(
    url,
    username.unwrap_or_default(),
    password,
  ))
}

/// Fetch the user's server playlists into `app.subsonic_playlists`.
async fn load_subsonic_playlists(app: &Arc<Mutex<App>>) {
  let Some(source) = build_source(app).await else {
    return;
  };
  match source.playlists().await {
    Ok(playlists) => {
      let mut app = app.lock().await;
      app.subsonic_playlists = playlists;
      app.subsonic_playlists_index = 0;
    }
    Err(e) => set_error(app, format!("Cannot load Subsonic playlists: {e}")).await,
  }
}

/// Fetch a playlist's tracks into the shared track table, tagged
/// [`TrackTableContext::SubsonicPlaylist`] so selecting a row plays it.
async fn load_subsonic_tracks(app: &Arc<Mutex<App>>, playlist_uri: &str) {
  let Some(source) = build_source(app).await else {
    return;
  };
  match source.tracks(playlist_uri).await {
    Ok(tracks) => {
      let mut app = app.lock().await;
      app.track_table.tracks = tracks;
      app.track_table.selected_index = 0;
      app.track_table.context = Some(TrackTableContext::SubsonicPlaylist);
    }
    Err(e) => set_error(app, format!("Cannot load Subsonic playlist: {e}")).await,
  }
}

/// Run a catalog search and populate `app.search_results`.
///
/// M2 populates **only** the songs block: album/artist search-result Enter
/// dispatches rspotify-bound `GetAlbum`/`GetArtist` events that fail for Subsonic
/// ids, so those blocks are left `None` to avoid dead rows. Album/artist
/// drill-down is a tracked follow-up.
async fn run_subsonic_search(app: &Arc<Mutex<App>>, query: &str) {
  let Some(source) = build_source(app).await else {
    return;
  };
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
    Err(e) => set_error(app, format!("Subsonic search failed: {e}")).await,
  }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// The live subsonic player, if a subsonic session is active.
async fn player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app
    .lock()
    .await
    .subsonic_playback
    .as_ref()
    .map(|s| Arc::clone(&s.player))
}

/// Snapshot the `TrackInfo`s for `uris`, preserving order, looking each up in
/// **both** browse views: the track table (browse→play) and the search results
/// (search→play). A playback request can originate from either — the track-table
/// Enter sends `track_table.tracks` uris, the search-result Enter sends
/// `search_results.tracks` uris — so the metadata may live in either. Any uri
/// found in neither is dropped.
fn snapshot_tracks(
  table: &[TrackInfo],
  search: Option<&[TrackInfo]>,
  uris: &[String],
) -> Vec<TrackInfo> {
  uris
    .iter()
    .filter_map(|uri| find_track(table, search, uri).cloned())
    .collect()
}

/// Find a track's metadata by URI in the track table, falling back to the search
/// results.
fn find_track<'a>(
  table: &'a [TrackInfo],
  search: Option<&'a [TrackInfo]>,
  uri: &str,
) -> Option<&'a TrackInfo> {
  let matches = |t: &&TrackInfo| t.uri.as_deref() == Some(uri);
  table
    .iter()
    .find(matches)
    .or_else(|| search.and_then(|s| s.iter().find(matches)))
}

/// Release the other two backends so only subsonic holds the output device.
async fn release_other_backends(app: &Arc<Mutex<App>>) {
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
  // Tear down any radio session (the `!handled_subsonic` short-circuit in the
  // runtime means the radio dispatch never sees this subsonic: start).
  #[cfg(feature = "internet-radio")]
  {
    let radio = app.lock().await.radio_playback.take();
    if let Some(radio) = radio {
      radio.player.stop();
    }
  }
  // Tear down any YouTube session, for the same short-circuit reason.
  #[cfg(feature = "youtube")]
  {
    let youtube = app.lock().await.youtube_playback.take();
    if let Some(youtube) = youtube {
      youtube.player.stop();
    }
  }
}

/// Reuse the live subsonic player, or open a fresh output device for one. A
/// freshly opened player is **not** published to `App` here — the caller
/// publishes the session only on a successful first play.
async fn acquire_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(p) = player(app).await {
    return Some(p);
  }
  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_error(app, format!("No audio output for Subsonic playback: {e}")).await;
      None
    }
    Err(e) => {
      set_error(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

/// Download a track's audio into a fresh tempfile. Must be awaited **without**
/// holding the `App` lock (it can take seconds).
async fn download_track(source: &SubsonicSource, track_id: &str) -> Result<NamedTempFile> {
  let tmp = NamedTempFile::new().context("creating temp file for Subsonic stream")?;
  source.download_track(track_id, tmp.path()).await?;
  Ok(tmp)
}

/// Begin playing a queue of subsonic tracks, taking over the session and
/// starting at `start_idx` (clamped into range).
async fn start_subsonic_queue(app: &Arc<Mutex<App>>, uris: &[String], start_idx: usize) {
  // Snapshot the track metadata under one short lock, from whichever browse view
  // the request came from (track table for browse, search results for search).
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
    set_error(app, "No Subsonic tracks to play".to_string()).await;
    return;
  }
  let index = start_idx.min(tracks.len() - 1);

  let track_id = match tracks[index].uri.as_deref().map(track_id_from_uri) {
    Some(Ok(id)) => id.to_string(),
    _ => {
      set_error(app, "Invalid Subsonic track URI".to_string()).await;
      return;
    }
  };

  let Some(source) = build_source(app).await else {
    return;
  };
  let source = Arc::new(source);

  // Only one backend owns the device at a time.
  release_other_backends(app).await;

  let Some(player) = acquire_player(app).await else {
    return;
  };

  // Download off the lock, then decode on the blocking pool.
  let tmp = match download_track(&source, &track_id).await {
    Ok(t) => t,
    Err(e) => {
      set_error(app, format!("Cannot download Subsonic track: {e}")).await;
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
      guard.subsonic_playback = Some(SubsonicPlaybackState {
        player,
        source,
        tracks,
        index,
        advancing: false,
        tempfile: tmp,
      });
      guard.set_status_message(format!("\u{266a} {display}"), 4);
    }
    Ok(Err(e)) => set_error(app, format!("Cannot play Subsonic track: {e}")).await,
    Err(e) => set_error(app, format!("Subsonic playback task failed: {e}")).await,
  }
}

/// Move the subsonic queue index in `direction` and play the new track. Returns
/// `true` if subsonic owns the session (so the event is consumed).
async fn skip(app: &Arc<Mutex<App>>, direction: Direction) -> bool {
  let target = {
    let mut guard = app.lock().await;
    let Some(s) = guard.subsonic_playback.as_mut() else {
      return false; // not ours
    };
    // Guard the empty-sink download window from spurious auto-advance.
    s.advancing = true;
    match direction {
      Direction::Next => next_index(s.index, s.tracks.len()),
      Direction::Prev => prev_index(s.index, s.tracks.len()),
    }
  };

  match target {
    Some(idx) => play_index(app, idx).await,
    None => {
      // Queue boundary: clear the optimistic guard so it doesn't wedge
      // auto-advance off for the rest of the track.
      if let Some(s) = app.lock().await.subsonic_playback.as_mut() {
        s.advancing = false;
      }
    }
  }
  true
}

/// What the locked snapshot in [`play_index`] decided to do.
enum Plan {
  Play(Arc<LocalPlayer>, Arc<SubsonicSource>, String),
  OutOfRange,
  BadUri,
}

/// Play the queued track at `target`, reusing the published session. Used by
/// Next/Previous and the runner tick's auto-advance.
///
/// A download failure tears the session down (deliberate divergence from the
/// local source's skip-past): on a network outage, walking the whole queue at
/// tick speed would spray error toasts, so one failure ends playback instead.
async fn play_index(app: &Arc<Mutex<App>>, target: usize) {
  let plan = {
    let guard = app.lock().await;
    match guard.subsonic_playback.as_ref() {
      None => return, // session torn down between dispatch and here
      Some(s) => match s.tracks.get(target) {
        None => Plan::OutOfRange,
        Some(track) => match track.uri.as_deref().map(track_id_from_uri) {
          Some(Ok(id)) => Plan::Play(Arc::clone(&s.player), Arc::clone(&s.source), id.to_string()),
          _ => Plan::BadUri,
        },
      },
    }
  };

  let (player, source, track_id) = match plan {
    Plan::Play(p, s, id) => (p, s, id),
    Plan::OutOfRange => {
      if let Some(s) = app.lock().await.subsonic_playback.as_mut() {
        s.advancing = false;
      }
      return;
    }
    Plan::BadUri => {
      teardown_subsonic(app).await;
      set_error(app, "Invalid Subsonic track URI".to_string()).await;
      return;
    }
  };

  let tmp = match download_track(&source, &track_id).await {
    Ok(t) => t,
    Err(e) => {
      teardown_subsonic(app).await;
      set_error(app, format!("Cannot download Subsonic track: {e}")).await;
      return;
    }
  };
  let path = tmp.path().to_path_buf();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || decode_player.play_file(&path)).await;

  match result {
    Ok(Ok(())) => commit_index(app, target, tmp).await,
    Ok(Err(e)) => {
      teardown_subsonic(app).await;
      set_error(app, format!("Cannot play Subsonic track: {e}")).await;
    }
    Err(e) => {
      teardown_subsonic(app).await;
      set_error(app, format!("Subsonic playback task failed: {e}")).await;
    }
  }
}

/// Commit `target` as the live index, clear the auto-advance guard, and swap in
/// the new track's tempfile (dropping the previous one). Ordering is safe: the
/// blocking `play_file` already cleared the old source from the sink, so rodio
/// no longer holds the old file by the time it is dropped here.
async fn commit_index(app: &Arc<Mutex<App>>, target: usize, tmp: NamedTempFile) {
  let mut guard = app.lock().await;
  let display = if let Some(s) = guard.subsonic_playback.as_mut() {
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

/// End the subsonic session, releasing the output device and cleaning up the
/// current tempfile.
async fn teardown_subsonic(app: &Arc<Mutex<App>>) {
  if let Some(s) = app.lock().await.subsonic_playback.take() {
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
  fn snapshot_finds_tracks_in_table_preserving_order() {
    let table = vec![
      track("subsonic:track:a", "A"),
      track("subsonic:track:b", "B"),
    ];
    let snap = snapshot_tracks(
      &table,
      None,
      &[
        "subsonic:track:b".to_string(),
        "subsonic:track:a".to_string(),
      ],
    );
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].name, "B");
    assert_eq!(snap[1].name, "A");
  }

  #[test]
  fn snapshot_falls_back_to_search_results_for_search_to_play() {
    // The track table holds a previously-browsed playlist; the played uris come
    // from the search results. The lookup must consult both. (Regression: the
    // search->play path looked only at the table and found nothing.)
    let table = vec![track("subsonic:track:browsed", "Browsed")];
    let search = vec![track("subsonic:track:searched", "Searched")];
    let snap = snapshot_tracks(
      &table,
      Some(&search),
      &["subsonic:track:searched".to_string()],
    );
    assert_eq!(snap.len(), 1, "search-sourced uri must resolve");
    assert_eq!(snap[0].name, "Searched");
  }

  #[test]
  fn snapshot_drops_unknown_uris() {
    let table = vec![track("subsonic:track:a", "A")];
    let snap = snapshot_tracks(&table, None, &["subsonic:track:missing".to_string()]);
    assert!(snap.is_empty());
  }

  /// End-to-end dispatch test: drive `route_subsonic_event` exactly as the
  /// runtime pump does — browse, play, advance the queue, and tear down on a
  /// foreign start. Exercises the dispatch glue/state machine the direct-client
  /// smoke tests bypass. Ignored (needs network **and** an audio device); run:
  /// `cargo test --features subsonic -- --ignored live_dispatch`
  #[tokio::test]
  #[ignore = "hits the live demo server AND requires an audio output device"]
  async fn live_dispatch_browse_play_and_advance() {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user_config.behavior.subsonic_url = Some("https://demo.navidrome.org".to_string());
    app.user_config.behavior.subsonic_username = Some("demo".to_string());
    app.user_config.behavior.subsonic_password = Some("demo".to_string());
    let app = Arc::new(Mutex::new(app));

    // Browse playlists, then a playlist's tracks into the shared table.
    assert!(route_subsonic_event(&app, &IoEvent::GetSubsonicPlaylists).await);
    let playlist_uri = app
      .lock()
      .await
      .subsonic_playlists
      .first()
      .expect("demo has playlists")
      .uri
      .clone();
    assert!(route_subsonic_event(&app, &IoEvent::GetSubsonicTracks(playlist_uri)).await);
    let uris: Vec<String> = app
      .lock()
      .await
      .track_table
      .tracks
      .iter()
      .filter_map(|t| t.uri.clone())
      .collect();
    assert!(
      uris.len() >= 2,
      "need a multi-track playlist to test advance"
    );

    // Start the queue at index 0 — downloads + plays the first track.
    assert!(
      route_subsonic_event(&app, &IoEvent::StartPlayback(None, Some(uris), Some(0))).await,
      "subsonic StartPlayback must be consumed"
    );
    {
      let guard = app.lock().await;
      let s = guard.subsonic_playback.as_ref().expect("session published");
      assert_eq!(s.index, 0);
      assert!(!s.player.is_paused(), "should be playing");
    }

    // Advance — downloads + plays the next track, moving the index.
    assert!(route_subsonic_event(&app, &IoEvent::NextTrack).await);
    {
      let guard = app.lock().await;
      let s = guard
        .subsonic_playback
        .as_ref()
        .expect("session still active");
      assert_eq!(s.index, 1, "Next should advance the queue index");
    }

    // A foreign (Spotify) start tears the session down and falls through.
    assert!(
      !route_subsonic_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None)
      )
      .await,
      "a non-subsonic start must fall through to the network"
    );
    assert!(
      app.lock().await.subsonic_playback.is_none(),
      "a foreign start must tear down the subsonic session (device handoff)"
    );
  }

  /// Regression for the search->play path: search, then play a result **without**
  /// browsing it into the track table first, so the metadata can only come from
  /// the search results. Before the `snapshot_tracks` fix this published no
  /// session ("No Subsonic tracks to play").
  #[tokio::test]
  #[ignore = "hits the live demo server AND requires an audio output device"]
  async fn live_dispatch_search_to_play() {
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user_config.behavior.subsonic_url = Some("https://demo.navidrome.org".to_string());
    app.user_config.behavior.subsonic_username = Some("demo".to_string());
    app.user_config.behavior.subsonic_password = Some("demo".to_string());
    let app = Arc::new(Mutex::new(app));

    assert!(
      route_subsonic_event(&app, &IoEvent::GetSubsonicSearchResults("love".to_string())).await
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
    assert!(!uris.is_empty(), "search should return tracks");
    // Track table is empty (never browsed) — metadata must resolve from search.
    assert!(app.lock().await.track_table.tracks.is_empty());

    assert!(route_subsonic_event(&app, &IoEvent::StartPlayback(None, Some(uris), Some(0))).await);
    let guard = app.lock().await;
    let s = guard
      .subsonic_playback
      .as_ref()
      .expect("search->play must publish a session");
    assert!(
      s.current().is_some_and(|t| !t.name.is_empty()),
      "metadata must resolve from the search results"
    );
  }
}
