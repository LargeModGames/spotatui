//! Internet-radio browse/search/playback routing.
//!
//! [`route_radio_event`] is called from the runtime IoEvent pump *after* the
//! local-files and Subsonic dispatches and *before* `handle_network_event`.
//! When an event targets the radio source (a station-list/search request, or a
//! `radio:` playback URI) it is handled here and consumed; otherwise it falls
//! through to the normal Spotify dispatch.
//!
//! ## Decoupling & device ownership
//!
//! Radio playback owns a single piece of state, [`App::radio_playback`], and
//! never writes Spotify/librespot fields. Only one backend holds the audio
//! device at a time: starting radio pauses librespot **and** tears down any
//! local/Subsonic session; the reciprocal teardowns live in those sources'
//! start paths.
//!
//! ## Live-stream semantics
//!
//! A station is infinite, so several transport events are **consumed as
//! no-ops** while radio owns the session — `Seek` (nothing to seek within) and
//! `NextTrack`/`PreviousTrack` (no queue). Consuming them is load-bearing:
//! falling through would hand them to the Spotify dispatch, which would try to
//! act on a Spotify session that isn't playing.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::{
  config_station_to_track_info, is_radio_uri, open_radio_stream, stream_url_from_uri,
  uri_for_stream_url, RadioPlaybackState, RadioSource,
};
use crate::core::app::{App, SearchResultBlock};
use crate::core::pagination::Paged;
use crate::core::plugin_api::TrackInfo;
use crate::core::source::Searcher;
use crate::infra::audio::LocalPlayer;
use crate::infra::network::IoEvent;

/// Intercept events that target the radio source.
///
/// Returns `true` if the event was handled (and must **not** be forwarded to
/// the Spotify network), `false` to let the normal dispatch run.
pub async fn route_radio_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    IoEvent::GetRadioStations => {
      load_radio_stations(app).await;
      true
    }
    IoEvent::GetRadioSearchResults(query) => {
      run_radio_search(app, query).await;
      true
    }
    // A station play. Radio "queues" collapse to the one station at the offset:
    // stations are infinite, so queueing the rest is meaningless.
    IoEvent::StartPlayback(None, Some(uris), offset)
      if uris.first().is_some_and(|u| is_radio_uri(u)) =>
    {
      let index = offset.unwrap_or(0).min(uris.len().saturating_sub(1));
      start_radio(app, &uris[index]).await;
      true
    }
    IoEvent::StartPlayback(Some(uri), _, _) if is_radio_uri(uri) => {
      start_radio(app, uri).await;
      true
    }
    // Bare "resume current" — ours only while radio owns the session.
    IoEvent::StartPlayback(None, None, None) => match player(app).await {
      Some(p) => {
        p.resume();
        true
      }
      None => false,
    },
    // Any other start is a local/Subsonic/Spotify play: relinquish the device,
    // then let the normal dispatch run.
    IoEvent::StartPlayback(..) => {
      teardown_radio(app).await;
      false
    }
    IoEvent::PausePlayback => match player(app).await {
      Some(p) => {
        p.pause();
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
    // Meaningless on a live stream — consume so they never reach Spotify.
    IoEvent::Seek(_) => player(app).await.is_some(),
    IoEvent::NextTrack | IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack
      if player(app).await.is_some() =>
    {
      set_error(app, "Live radio: no track skipping".to_string()).await;
      true
    }
    _ => false,
  }
}

// ---------------------------------------------------------------------------
// Browse + search
// ---------------------------------------------------------------------------

/// Load the config-file station list into `app.radio_stations` (the sidebar's
/// Stations panel). No network — the list is user-configured.
async fn load_radio_stations(app: &Arc<Mutex<App>>) {
  let mut guard = app.lock().await;
  let stations: Vec<TrackInfo> = guard
    .user_config
    .behavior
    .radio_stations
    .iter()
    .filter(|s| !s.name.trim().is_empty() && !s.url.trim().is_empty())
    .map(|s| config_station_to_track_info(&s.name, &s.url))
    .collect();
  let empty = stations.is_empty();
  guard.radio_stations = stations;
  guard.radio_stations_index = 0;
  if empty {
    guard.set_status_message(
      "No radio stations configured (behavior.radio_stations); search to find some".to_string(),
      6,
    );
  }
}

/// Search the radio-browser.info directory and populate `app.search_results`.
///
/// Like the Subsonic search, only the songs block is populated — each row is a
/// station; Enter on one dispatches its `radio:` URI back through this module.
async fn run_radio_search(app: &Arc<Mutex<App>>, query: &str) {
  let source = RadioSource::new();
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
    Err(e) => set_error(app, format!("Radio search failed: {e}")).await,
  }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

/// The live radio player, if a radio session is active.
async fn player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app
    .lock()
    .await
    .radio_playback
    .as_ref()
    .map(|s| Arc::clone(&s.player))
}

/// Find the station row for `uri` in the browse views a play request can
/// originate from: the sidebar station list, the shared track table, and the
/// search results. Falls back to a synthetic row built from the URL itself, so
/// a `radio:` URI with no backing row still plays (it just has no nice name).
fn snapshot_station(app: &App, uri: &str) -> TrackInfo {
  let matches = |t: &&TrackInfo| t.uri.as_deref() == Some(uri);
  app
    .radio_stations
    .iter()
    .find(matches)
    .or_else(|| app.track_table.tracks.iter().find(matches))
    .or_else(|| {
      app
        .search_results
        .tracks
        .as_ref()
        .and_then(|p| p.items.iter().find(matches))
    })
    .cloned()
    .unwrap_or_else(|| {
      let url = stream_url_from_uri(uri).unwrap_or(uri);
      config_station_to_track_info(url, url)
    })
}

/// Release the other backends so only radio holds the output device.
async fn release_other_backends(_app: &Arc<Mutex<App>>) {
  // Pause native Spotify so librespot releases the device.
  #[cfg(feature = "streaming")]
  {
    let streaming = _app.lock().await.streaming_player.clone();
    if let Some(player) = streaming {
      player.pause();
    }
  }
  // Tear down any local-file session (dropping it releases its device handle).
  #[cfg(feature = "local-files")]
  {
    let local = _app.lock().await.local_playback.take();
    if let Some(local) = local {
      local.player.stop();
    }
  }
  // Tear down any Subsonic session.
  #[cfg(feature = "subsonic")]
  {
    let subsonic = _app.lock().await.subsonic_playback.take();
    if let Some(subsonic) = subsonic {
      subsonic.player.stop();
    }
  }
  // Tear down any YouTube session (the pump's short-circuit means the YouTube
  // dispatch never sees this radio: start).
  #[cfg(feature = "youtube")]
  {
    let youtube = _app.lock().await.youtube_playback.take();
    if let Some(youtube) = youtube {
      youtube.player.stop();
    }
  }
}

/// Reuse the live radio player, or open a fresh output device for one. A
/// freshly opened player is **not** published to `App` here — the caller
/// publishes the session only on a successful first play.
async fn acquire_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(p) = player(app).await {
    return Some(p);
  }
  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_error(app, format!("No audio output for radio playback: {e}")).await;
      None
    }
    Err(e) => {
      set_error(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

/// Begin playing the station at `uri`, taking over the session.
async fn start_radio(app: &Arc<Mutex<App>>, uri: &str) {
  let url = match stream_url_from_uri(uri) {
    Ok(u) => u.to_string(),
    Err(_) => {
      set_error(app, "Invalid radio URI".to_string()).await;
      return;
    }
  };
  let mut station = {
    let guard = app.lock().await;
    snapshot_station(&guard, uri)
  };

  // Only one backend owns the device at a time.
  release_other_backends(app).await;

  let Some(player) = acquire_player(app).await else {
    return;
  };

  // Connect + prefetch off the lock; this can take seconds on a slow network.
  let opened = match open_radio_stream(&url).await {
    Ok(o) => o,
    Err(e) => {
      set_error(app, format!("Cannot open radio stream: {e}")).await;
      return;
    }
  };

  // Count the click for directory-sourced stations (usage-policy nicety).
  // Fire-and-forget: it must never delay or fail playback.
  if let Some(uuid) = station.id.clone() {
    tokio::spawn(async move { RadioSource::new().click(&uuid).await });
  }

  // A synthetic row (raw URL as name) gets upgraded to the station's
  // self-reported icy-name.
  if station.name == url {
    if let Some(icy_name) = &opened.station_name {
      station.name = icy_name.clone();
      station.uri = Some(uri_for_stream_url(&url));
    }
  }

  let now_playing = Arc::clone(&opened.now_playing);
  let (reader, mime) = (opened.reader, opened.content_type);
  let decode_player = Arc::clone(&player);
  let result =
    tokio::task::spawn_blocking(move || decode_player.play_stream(reader, mime.as_deref())).await;

  match result {
    Ok(Ok(())) => {
      let volume = app.lock().await.user_config.behavior.volume_percent;
      player.set_volume(volume as f32 / 100.0);

      let display = station.name.clone();
      let mut guard = app.lock().await;
      // Publish the session exactly once, now that the stream is decoding.
      guard.radio_playback = Some(RadioPlaybackState {
        player,
        station,
        now_playing,
      });
      guard.set_status_message(format!("\u{266a} {display} (live)"), 4);
    }
    Ok(Err(e)) => set_error(app, format!("Cannot play radio stream: {e}")).await,
    Err(e) => set_error(app, format!("Radio playback task failed: {e}")).await,
  }
}

/// End the radio session, releasing the output device. Stopping the sink drops
/// the decoder, which drops the stream reader, which cancels the background
/// download task (`cancel_on_drop`).
pub async fn teardown_radio(app: &Arc<Mutex<App>>) {
  if let Some(s) = app.lock().await.radio_playback.take() {
    s.player.stop();
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
    App::new(tx, UserConfig::new(), SystemTime::now())
  }

  fn station_row(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(uri.to_string()),
      name: name.to_string(),
      artists: vec![],
      album: String::new(),
      duration_ms: 0,
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
  fn snapshot_prefers_sidebar_station_list() {
    let mut app = test_app();
    let uri = "radio:https://ice1.somafm.com/groovesalad-128-mp3";
    app.radio_stations = vec![station_row(uri, "Groove Salad")];
    app.track_table.tracks = vec![station_row(uri, "Wrong Name")];
    assert_eq!(snapshot_station(&app, uri).name, "Groove Salad");
  }

  #[test]
  fn snapshot_falls_back_to_search_results() {
    let mut app = test_app();
    let uri = "radio:https://example.com/stream";
    app.search_results.tracks = Some(Paged {
      items: vec![station_row(uri, "Searched FM")],
      total: 1,
      ..Default::default()
    });
    assert_eq!(snapshot_station(&app, uri).name, "Searched FM");
  }

  #[test]
  fn snapshot_synthesizes_a_row_for_unknown_uris() {
    let app = test_app();
    let station = snapshot_station(&app, "radio:https://example.com/live");
    assert_eq!(station.name, "https://example.com/live");
    assert_eq!(
      station.uri.as_deref(),
      Some("radio:https://example.com/live")
    );
    assert_eq!(station.duration_ms, 0);
  }

  /// Browse/search/transport events that are not radio's must fall through.
  #[tokio::test]
  async fn foreign_events_fall_through_without_a_session() {
    let app = Arc::new(Mutex::new(test_app()));
    // No radio session: transport events are not ours.
    assert!(!route_radio_event(&app, &IoEvent::PausePlayback).await);
    assert!(!route_radio_event(&app, &IoEvent::NextTrack).await);
    assert!(!route_radio_event(&app, &IoEvent::Seek(1000)).await);
    assert!(!route_radio_event(&app, &IoEvent::StartPlayback(None, None, None)).await);
    // A Spotify start falls through (and there is nothing to tear down).
    assert!(
      !route_radio_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None)
      )
      .await
    );
  }

  /// Loading the (empty) config station list is consumed and surfaces a hint.
  #[tokio::test]
  async fn get_radio_stations_loads_config_list() {
    let app = Arc::new(Mutex::new(test_app()));
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    assert!(app.lock().await.radio_stations.is_empty());

    // Now with two configured stations, one blank (filtered out).
    {
      let mut guard = app.lock().await;
      guard.user_config.behavior.radio_stations = vec![
        crate::core::user_config::RadioStationConfig {
          name: "Groove Salad".to_string(),
          url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
        },
        crate::core::user_config::RadioStationConfig {
          name: "  ".to_string(),
          url: "https://x.example/s".to_string(),
        },
      ];
    }
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    let guard = app.lock().await;
    assert_eq!(guard.radio_stations.len(), 1);
    assert_eq!(guard.radio_stations[0].name, "Groove Salad");
    assert_eq!(
      guard.radio_stations[0].uri.as_deref(),
      Some("radio:https://ice1.somafm.com/groovesalad-128-mp3")
    );
  }

  /// End-to-end dispatch test: drive `route_radio_event` exactly as the runtime
  /// pump does — load stations, start one, exercise the live-stream no-op
  /// transport semantics, and tear down on a foreign start. Ignored (needs
  /// network **and** an audio output device); run:
  /// `cargo test --features internet-radio -- --ignored live_dispatch`
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "hits the live SomaFM stream AND requires an audio output device"]
  async fn live_dispatch_play_and_teardown() {
    let app = Arc::new(Mutex::new(test_app()));
    {
      let mut guard = app.lock().await;
      guard.user_config.behavior.radio_stations =
        vec![crate::core::user_config::RadioStationConfig {
          name: "Groove Salad".to_string(),
          url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
        }];
    }
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    let uri = app.lock().await.radio_stations[0].uri.clone().unwrap();

    // Start the station.
    assert!(
      route_radio_event(&app, &IoEvent::StartPlayback(Some(uri), None, None)).await,
      "radio StartPlayback must be consumed"
    );
    {
      let guard = app.lock().await;
      let s = guard.radio_playback.as_ref().expect("session published");
      assert_eq!(s.station.name, "Groove Salad");
      assert!(!s.player.is_paused(), "should be playing");
    }

    // Live-stream semantics: Seek and Next/Prev are consumed as no-ops.
    assert!(route_radio_event(&app, &IoEvent::Seek(30_000)).await);
    assert!(route_radio_event(&app, &IoEvent::NextTrack).await);
    assert!(
      app.lock().await.radio_playback.is_some(),
      "no-op transport must not tear the session down"
    );

    // Pause/resume are ours while the session lives.
    assert!(route_radio_event(&app, &IoEvent::PausePlayback).await);
    assert!(app
      .lock()
      .await
      .radio_playback
      .as_ref()
      .is_some_and(|s| s.player.is_paused()));
    assert!(route_radio_event(&app, &IoEvent::StartPlayback(None, None, None)).await);

    // A foreign (Spotify) start tears the session down and falls through.
    assert!(
      !route_radio_event(
        &app,
        &IoEvent::StartPlayback(Some("spotify:track:x".to_string()), None, None)
      )
      .await,
      "a non-radio start must fall through to the network"
    );
    assert!(
      app.lock().await.radio_playback.is_none(),
      "a foreign start must tear down the radio session (device handoff)"
    );
  }
}
