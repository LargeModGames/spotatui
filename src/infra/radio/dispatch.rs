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
//! device at a time: starting radio pauses or parks librespot **and** tears down any
//! local/Subsonic session; the reciprocal teardowns live in those sources'
//! start paths.
//!
//! ## Live-stream semantics
//!
//! A station is infinite, so several transport events are **consumed as
//! no-ops** while radio owns the session — `Seek` (nothing to seek within),
//! `Repeat`, and `NextTrack`/`PreviousTrack` (no queue). Consuming them is load-bearing:
//! falling through would hand them to the Spotify dispatch, which would try to
//! act on a Spotify session that isn't playing.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::{
  config_station_to_track_info, is_radio_uri, open_radio_stream, stream_url_from_uri,
  RadioPlaybackState, RadioSource,
};
use crate::core::app::App;
use crate::core::plugin_api::TrackInfo;
use crate::core::source::{Searcher, Source};
use crate::core::state::merged_radio_stations;
use crate::infra::audio::{LocalPlayer, PreparedStream};
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
        p.set_volume(*volume);
        let mut app = app.lock().await;
        app.runtime_state.volume_percent = *volume;
        app.schedule_state_save(crate::core::state::PersistedRuntimeState::volume_percent(
          *volume,
        ));
        true
      }
      None => false,
    },
    // Meaningless on a live stream — consume so they never reach Spotify.
    IoEvent::Seek(_) => player(app).await.is_some(),
    IoEvent::Repeat(_) => player(app).await.is_some(),
    IoEvent::NextTrack | IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack
      if player(app).await.is_some() =>
    {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .radio_no_skip
          .to_string(),
      )
      .await;
      true
    }
    _ => false,
  }
}

// ---------------------------------------------------------------------------
// Browse + search
// ---------------------------------------------------------------------------

/// Load configured and saved stations into `app.radio_stations()` (the sidebar's
/// Stations panel). No network.
async fn load_radio_stations(app: &Arc<Mutex<App>>) {
  let mut guard = app.lock().await;
  let stations: Vec<TrackInfo> = merged_radio_stations(
    &guard.user_config.behavior.radio_stations,
    &guard.runtime_state.radio_stations,
  )
  .into_iter()
  .map(|station| config_station_to_track_info(&station.name, &station.url))
  .collect();
  let empty = stations.is_empty();
  *guard.radio_stations_mut() = stations;
  if empty {
    {
      let strings = guard.strings();
      guard.set_status_message(strings.status.infra.radio_empty.to_string(), 6)
    };
  }
}

/// Search the radio-browser.info directory and populate `app.search_results`.
///
/// Like the Subsonic search, only the songs block is populated — each row is a
/// station; Enter on one dispatches its `radio:` URI back through this module.
async fn run_radio_search(app: &Arc<Mutex<App>>, query: &str) {
  let Some(source_rev) = app.lock().await.begin_search(Source::Radio) else {
    return;
  };
  let source = RadioSource::new();
  match source.search(query).await {
    Ok(results) => {
      app
        .lock()
        .await
        .show_source_search_tracks_if_current(source_rev, query, results.tracks);
    }
    Err(e) => {
      if app.lock().await.source_revision() != source_rev {
        return;
      }
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .radio_search_failed(&e.to_string()),
      )
      .await
    }
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
    .radio_stations()
    .iter()
    .find(matches)
    .or_else(|| app.track_table.tracks.iter().find(matches))
    .or_else(|| {
      app
        .search_results()
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
async fn release_other_backends(app: &Arc<Mutex<App>>) {
  // Take the sink from native Spotify so no rebuild resumes it under this
  // source.
  #[cfg(feature = "streaming")]
  app.lock().await.release_native_for_decoded();
  // The other decoded sources never see this radio: start (the pump's
  // short-circuit), so their sessions are torn down here.
  let players = app.lock().await.take_decoded_sessions_except(Source::Radio);
  for player in players {
    player.stop_detached();
  }
}

/// Reuse the live radio player, or open a fresh output device for one. A
/// freshly opened player is **not** published to `App` here — the caller
/// publishes the session only on a successful first play.
async fn acquire_player(
  app: &Arc<Mutex<App>>,
  previous: Option<Arc<LocalPlayer>>,
  open_player: impl FnOnce() -> anyhow::Result<LocalPlayer> + Send + 'static,
) -> Option<Arc<LocalPlayer>> {
  if let Some(player) = previous {
    return Some(player);
  }
  match tokio::task::spawn_blocking(open_player).await {
    Ok(Ok(p)) => Some(Arc::new(p)),
    Ok(Err(e)) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .radio_audio_unavailable(&e.to_string()),
      )
      .await;
      None
    }
    Err(e) => {
      set_error(
        app,
        crate::infra::app_strings(app)
          .await
          .status
          .infra
          .audio_init_failed(&e.to_string()),
      )
      .await;
      None
    }
  }
}

struct PreparedStation {
  audio: PreparedStream,
  now_playing: Arc<std::sync::Mutex<Option<String>>>,
}

enum TuneInError {
  InvalidUri,
  Open(anyhow::Error),
  Prepare(anyhow::Error),
}

impl TuneInError {
  fn message(&self, strings: &crate::core::i18n::status_infra::InfraStatusStrings) -> String {
    let (error, fallback) = match self {
      Self::InvalidUri => return strings.radio_uri_invalid.to_string(),
      Self::Open(error) => (error, strings.radio_open_failed(&error.to_string())),
      Self::Prepare(error) if error.is::<tokio::task::JoinError>() => {
        (error, strings.radio_task_failed(&error.to_string()))
      }
      Self::Prepare(error) => (error, strings.radio_play_failed(&error.to_string())),
    };
    error
      .downcast_ref::<super::stream::RadioOpenError>()
      .map_or(fallback, |reason| reason.message(strings).to_string())
  }
}

pub(crate) struct RadioTuneIn {
  station: TrackInfo,
  paused: bool,
  /// For a startup restore, [`App::playback_requests`] when it began. Any
  /// newer playback intent recorded since then wins over the restore.
  requests: Option<u64>,
  result: Result<PreparedStation, TuneInError>,
}

async fn prepare_station(station: &mut TrackInfo) -> Result<PreparedStation, TuneInError> {
  let url = station
    .uri
    .as_deref()
    .and_then(|uri| stream_url_from_uri(uri).ok())
    .ok_or(TuneInError::InvalidUri)?;
  let opened = open_radio_stream(url).await.map_err(TuneInError::Open)?;
  if let Some(name) = opened.station_name.as_ref().filter(|_| station.name == url) {
    station.name = name.clone();
  }
  let now_playing = Arc::clone(&opened.now_playing);
  let audio = opened.prepare().await.map_err(TuneInError::Prepare)?;
  Ok(PreparedStation { audio, now_playing })
}

/// This may run off the pump for startup restore. It never sees the App or sink.
/// `requests` is the restore's [`App::playback_requests`] snapshot, `None` for
/// a tune-in the user asked for.
pub(crate) async fn prepare_radio(
  mut station: TrackInfo,
  paused: bool,
  requests: Option<u64>,
) -> RadioTuneIn {
  let result = prepare_station(&mut station).await;
  RadioTuneIn {
    station,
    paused,
    requests,
    result,
  }
}

/// Only the serial pump may take the sink and publish a prepared station.
pub(crate) async fn finish_radio(app: &Arc<Mutex<App>>, tune_in: RadioTuneIn) {
  finish_radio_with_player(app, tune_in, LocalPlayer::new).await;
}

async fn finish_radio_with_player(
  app: &Arc<Mutex<App>>,
  tune_in: RadioTuneIn,
  open_player: impl FnOnce() -> anyhow::Result<LocalPlayer> + Send + 'static,
) {
  let RadioTuneIn {
    station,
    paused,
    requests,
    result,
  } = tune_in;
  let (PreparedStation { audio, now_playing }, previous) = {
    let mut guard = app.lock().await;
    // Check under the App lock so a newer request or play/pause action wins
    // even when the pump took the restore result first.
    if requests.is_some_and(|requests| requests != guard.playback_requests()) {
      return;
    }
    let prepared = match result {
      Ok(prepared) => prepared,
      Err(error) => {
        let message = error.message(&guard.strings().status.infra);
        guard.set_status_message(message, 6);
        return;
      }
    };
    guard.claim_decoded_sink(Source::Radio);
    // Keep the old station published, so its session stays saved, while the
    // tick ignores its briefly empty sink.
    let previous = guard
      .radio_playback
      .as_ref()
      .map(|session| Arc::clone(&session.player));
    guard.set_radio_retuning(previous.is_some());
    (prepared, previous)
  };
  release_other_backends(app).await;
  // A failed start keeps the claim, so a bare resume cannot wake librespot.
  let Some(player) = acquire_player(app, previous, open_player).await else {
    return;
  };

  player.set_volume(app.lock().await.runtime_state.volume_percent);
  let decode_player = Arc::clone(&player);
  let stage = tokio::task::spawn_blocking(move || {
    if paused {
      decode_player.stage_prepared(audio)
    } else {
      decode_player.play_prepared(audio)
    }
  })
  .await;
  let mut guard = app.lock().await;
  let strings = guard.strings();
  let error = match stage {
    Ok(Ok(())) => None,
    Ok(Err(error)) => Some(strings.status.infra.radio_play_failed(&error.to_string())),
    Err(error) => Some(strings.status.infra.radio_task_failed(&error.to_string())),
  };
  guard.set_radio_retuning(false);
  if let Some(error) = error {
    radio_stage_failed(&mut guard, error);
    return;
  }

  let message = strings.status.infra.radio_playing(&station.name);
  let directory_id = station.id.clone();
  guard.radio_playback = Some(RadioPlaybackState {
    player,
    station,
    now_playing,
  });
  guard.set_status_message(message, 4);
  drop(guard);

  if let Some(id) = directory_id {
    tokio::spawn(async move { RadioSource::new().click(&id).await });
  }
}

/// Drop an old station that already went silent and report why the new one
/// did not start. The claim stays: a failed start still owns the sink.
fn radio_stage_failed(app: &mut App, error: String) {
  if app
    .radio_playback
    .as_ref()
    .is_some_and(|old| old.player.is_finished())
  {
    app.radio_playback = None;
  }
  app.set_status_message(error, 6);
}

/// Begin playing the station at `uri`, taking over the session.
async fn start_radio(app: &Arc<Mutex<App>>, uri: &str) {
  if stream_url_from_uri(uri).is_err() {
    let mut guard = app.lock().await;
    let message = guard.strings().status.infra.radio_uri_invalid.to_string();
    guard.set_status_message(message, 6);
    return;
  }
  let station = snapshot_station(&*app.lock().await, uri);
  finish_radio(app, prepare_radio(station, false, None).await).await;
}

/// End the radio session, releasing the output device. Stopping the sink drops
/// the decoder, which drops the stream reader, which cancels the background
/// download task (`cancel_on_drop`).
pub async fn teardown_radio(app: &Arc<Mutex<App>>) {
  // Take under the lock, stop off it: a sink clear waits for the audio thread.
  let session = app.lock().await.radio_playback.take();
  if let Some(s) = session {
    Arc::clone(&s.player).stop_detached_holding(s);
  }
}

async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::pagination::Paged;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn test_app() -> App {
    let (tx, _rx) = channel();
    App::new(tx, UserConfig::new(), Some(SystemTime::now()))
  }

  #[tokio::test]
  async fn a_failed_tune_in_does_not_take_the_audio_sink() {
    let app = Arc::new(Mutex::new(test_app()));
    let start = IoEvent::StartPlayback(Some("radio:not a URL".to_owned()), None, None);
    assert!(route_radio_event(&app, &start).await);
    let app = app.lock().await;
    assert!(app.status_message().is_some());
    assert!(!app.decoded_sink_claimed());
    assert!(app.radio_playback.is_none());
  }

  #[test]
  fn a_station_that_fails_to_stage_keeps_the_radio_claim() {
    let mut app = test_app();
    app.claim_decoded_sink(Source::Radio);
    radio_stage_failed(&mut app, "no audio".to_owned());
    assert!(app.decoded_sink_claimed());
    assert!(app.status_message().is_some());
  }

  #[tokio::test]
  async fn a_failed_player_open_keeps_the_radio_claim() {
    let app = Arc::new(Mutex::new(test_app()));

    finish_radio_with_player(&app, prepared_tune_in(None), unavailable_player).await;

    let app = app.lock().await;
    assert!(app.decoded_sink_claimed());
    assert!(app.radio_playback.is_none());
    let message = app
      .strings()
      .status
      .infra
      .radio_audio_unavailable("output unavailable");
    assert_eq!(app.status_message(), Some(message.as_str()));
  }

  #[tokio::test]
  async fn playback_actions_without_spotify_discard_a_ready_radio_restore() {
    use crate::core::action::Action;

    for action in [Action::TogglePlayback, Action::Play, Action::Pause] {
      let (tx, rx) = channel();
      let mut app = App::new(tx, UserConfig::new(), None);
      let restore = prepared_tune_in(Some(app.playback_requests()));
      let label = format!("{action:?}");
      app.apply(action);
      let status = app.status_message().map(str::to_owned);
      assert!(rx.try_recv().is_err());
      let app = Arc::new(Mutex::new(app));

      finish_radio_with_player(&app, restore, unavailable_player).await;

      let app = app.lock().await;
      assert!(!app.decoded_sink_claimed(), "{label}");
      assert_eq!(app.status_message(), status.as_deref(), "{label}");
    }
  }

  fn prepared_tune_in(requests: Option<u64>) -> RadioTuneIn {
    RadioTuneIn {
      station: station_row("radio:https://example.com/live", "Saved FM"),
      paused: false,
      requests,
      result: Ok(PreparedStation {
        audio: silent_wav(),
        now_playing: Arc::default(),
      }),
    }
  }

  fn unavailable_player() -> anyhow::Result<LocalPlayer> {
    Err(anyhow::anyhow!("output unavailable"))
  }

  fn failed_restore(requests: u64) -> RadioTuneIn {
    RadioTuneIn {
      station: station_row("radio:http://127.0.0.1:1/live", "Saved FM"),
      paused: false,
      requests: Some(requests),
      result: Err(TuneInError::Open(anyhow::anyhow!("connection refused"))),
    }
  }

  #[tokio::test]
  async fn a_ready_restore_applies_when_nothing_was_requested_since() {
    let app = Arc::new(Mutex::new(test_app()));
    let requests = app.lock().await.playback_requests();
    finish_radio(&app, failed_restore(requests)).await;
    assert!(app.lock().await.status_message().is_some());
  }

  /// The pump can take a finished restore before a request that was already
  /// dispatched. The request still wins, Next included.
  #[tokio::test]
  async fn a_request_dispatched_before_a_ready_restore_discards_it() {
    for request in [
      IoEvent::NextTrack,
      IoEvent::StartPlayback(Some("radio:https://example.com/other".into()), None, None),
    ] {
      let app = Arc::new(Mutex::new(test_app()));
      let restore = failed_restore(app.lock().await.playback_requests());
      app.lock().await.dispatch(request);
      finish_radio(&app, restore).await;
      let app = app.lock().await;
      assert!(app.status_message().is_none());
      assert!(!app.decoded_sink_claimed());
    }
  }

  /// A silent one-second WAV, decodable without a network.
  fn silent_wav() -> PreparedStream {
    let samples = 44_100u32;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + samples * 2).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(44_100u32.to_le_bytes());
    wav.extend((44_100u32 * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((samples * 2).to_le_bytes());
    wav.resize(wav.len() + samples as usize * 2, 0);
    LocalPlayer::prepare_stream(std::io::Cursor::new(wav), Some("audio/wav"), None).unwrap()
  }

  /// A switch reuses the old station's player, whose sink is empty for a
  /// moment. A tick in that window must still see a saved session, or it
  /// clears `last_session.yml` behind the new station's save.
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "requires an audio output device; plays nothing (paused, silent)"]
  async fn a_station_switch_keeps_the_old_session_saved_until_the_new_one_plays() {
    let app = Arc::new(Mutex::new(test_app()));
    let player = Arc::new(LocalPlayer::new().expect("open default output device"));
    app.lock().await.radio_playback = Some(RadioPlaybackState {
      player,
      station: station_row("radio:https://example.com/old", "Old FM"),
      now_playing: Arc::default(),
    });
    let tune_in = RadioTuneIn {
      station: station_row("radio:https://example.com/new", "New FM"),
      paused: true,
      requests: None,
      result: Ok(PreparedStation {
        audio: silent_wav(),
        now_playing: Arc::default(),
      }),
    };

    // The App mutex is fair: the switch queues behind this guard, and this
    // test's next lock queues behind the switch's first one, so it lands
    // right after the switch claimed the sink.
    let guard = app.lock().await;
    let switch = tokio::spawn({
      let app = Arc::clone(&app);
      async move { finish_radio(&app, tune_in).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    drop(guard);
    {
      let app = app.lock().await;
      assert!(app.decoded_sink_claimed(), "the switch has started");
      assert!(app.has_persistable_session());
      assert!(app.radio_retuning());
    }
    switch.await.unwrap();
    let app = app.lock().await;
    assert!(!app.radio_retuning());
    let session = app.radio_playback.as_ref().expect("new station published");
    assert_eq!(session.station.name, "New FM");
  }

  fn station_row(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      generated_name: None,
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
      release_date: None,
    }
  }

  #[test]
  fn snapshot_prefers_sidebar_station_list() {
    let mut app = test_app();
    let uri = "radio:https://ice1.somafm.com/groovesalad-128-mp3";
    *app.radio_stations_mut() = vec![station_row(uri, "Groove Salad")];
    app.track_table.tracks = vec![station_row(uri, "Wrong Name")];
    assert_eq!(snapshot_station(&app, uri).name, "Groove Salad");
  }

  #[test]
  fn snapshot_falls_back_to_search_results() {
    let mut app = test_app();
    let uri = "radio:https://example.com/stream";
    app.set_search_results(crate::core::app::SearchResult {
      tracks: Some(Paged {
        items: vec![station_row(uri, "Searched FM")],
        total: 1,
        ..Default::default()
      }),
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
    assert!(
      !route_radio_event(
        &app,
        &IoEvent::Repeat(rspotify::model::enums::RepeatState::Off)
      )
      .await
    );
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

  /// Loading the station list is consumed and merges config + saved stations.
  #[tokio::test]
  async fn get_radio_stations_merges_configured_and_saved_lists() {
    let app = Arc::new(Mutex::new(test_app()));
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    assert!(app.lock().await.radio_stations().is_empty());

    {
      let mut guard = app.lock().await;
      guard.user_config.behavior.radio_stations = vec![crate::core::state::RadioStationConfig {
        name: "Configured Groove".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      }];
      guard.runtime_state.radio_stations = vec![
        crate::core::state::RadioStationConfig {
          name: "Groove Salad".to_string(),
          url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
        },
        crate::core::state::RadioStationConfig {
          name: "Secret Agent".to_string(),
          url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
        },
      ];
    }
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    let guard = app.lock().await;
    assert_eq!(guard.radio_stations().len(), 2);
    assert_eq!(guard.radio_stations()[0].name, "Configured Groove");
    assert_eq!(
      guard.radio_stations()[0].uri.as_deref(),
      Some("radio:https://ice1.somafm.com/groovesalad-128-mp3")
    );
    assert_eq!(guard.radio_stations()[1].name, "Secret Agent");
  }

  /// The station shape that froze the app: a live stream whose codec rodio's
  /// symphonia does not register (MPEG-2 ADTS AAC — its ADTS reader only claims
  /// the MPEG-4 `ff f1` marker, not `ff f9`). The probe scans for a marker that
  /// will never arrive, and this runs on the serial pump, so an unbounded one
  /// takes searching and playback controls down with it. What must happen
  /// instead: give up, say so, publish no session, and leave the app usable.
  #[tokio::test(flavor = "multi_thread")]
  #[ignore = "hits a live radio station AND requires an audio output device"]
  async fn an_unrecognisable_live_stream_gives_up_instead_of_hanging() {
    let app = Arc::new(Mutex::new(test_app()));
    {
      let mut guard = app.lock().await;
      guard.runtime_state.radio_stations = vec![crate::core::state::RadioStationConfig {
        name: "Radio Bruno (MPEG-2 ADTS AAC)".to_string(),
        url: "https://stream3.xdevel.com/audio6s975355-281/stream/icecast.audio".to_string(),
      }];
    }
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    let uri = app.lock().await.radio_stations()[0].uri.clone().unwrap();

    let started = std::time::Instant::now();
    assert!(route_radio_event(&app, &IoEvent::StartPlayback(Some(uri), None, None)).await);
    let elapsed = started.elapsed();

    assert!(
      elapsed < super::super::stream::PROBE_TIMEOUT + std::time::Duration::from_secs(15),
      "tune-in must be bounded, took {elapsed:?}"
    );
    let guard = app.lock().await;
    assert!(
      guard.radio_playback.is_none(),
      "nothing decodable, so no session may be published"
    );
    assert!(
      guard.api_error().contains("radio stream") || guard.status_message().is_some(),
      "the user has to be told why nothing played"
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
      guard.runtime_state.radio_stations = vec![crate::core::state::RadioStationConfig {
        name: "Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      }];
    }
    assert!(route_radio_event(&app, &IoEvent::GetRadioStations).await);
    let uri = app.lock().await.radio_stations()[0].uri.clone().unwrap();

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
    assert!(
      route_radio_event(
        &app,
        &IoEvent::Repeat(rspotify::model::enums::RepeatState::Off)
      )
      .await
    );
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
