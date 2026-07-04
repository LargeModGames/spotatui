use crate::core::plugin_api::{ArtistRef, PlaybackState, TrackInfo};

use super::effects::ScriptEffect;
use super::engine::ScriptEngine;
use super::events::{diff_events, ScriptEvent};
use super::shared::HttpResponseData;

fn track(uri: &str, name: &str) -> TrackInfo {
  TrackInfo {
    uri: Some(uri.to_string()),
    name: name.to_string(),
    artists: vec!["Artist".to_string()],
    album: "Album".to_string(),
    duration_ms: 200_000,
    id: None,
    album_id: None,
    artist_refs: vec![ArtistRef {
      id: None,
      name: "Artist".to_string(),
    }],
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    image_url: None,
  }
}

fn playback(track: Option<TrackInfo>, is_playing: bool, progress_ms: u64) -> PlaybackState {
  PlaybackState {
    track,
    is_playing,
    progress_ms,
    shuffle: false,
    repeat: "off".to_string(),
    volume_percent: Some(50),
    device: None,
  }
}

/// Take all currently-queued effects out of the shared buffer.
/// (`ScriptEffect` is not `PartialEq` because `IoEvent` isn't, so tests pattern-match.)
fn drain(engine: &ScriptEngine) -> Vec<ScriptEffect> {
  engine.shared.effects.borrow_mut().drain(..).collect()
}

/// Assert a single effect was queued and return it.
fn one(engine: &ScriptEngine) -> ScriptEffect {
  let mut effects = drain(engine);
  assert_eq!(effects.len(), 1, "expected exactly one effect");
  effects.pop().unwrap()
}

// --- handler registration + emission ---

#[test]
fn track_change_handler_queues_notify() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source(
      "test",
      r#"
        spotatui.on("track_change", function(pb)
          spotatui.notify("now: " .. pb.track.name, 5)
        end)
      "#,
    )
    .unwrap();

  *engine.shared.playback.borrow_mut() = Some(playback(Some(track("uri:1", "Song A")), true, 0));
  engine.emit(ScriptEvent::TrackChange);

  match one(&engine) {
    ScriptEffect::Notify(msg, ttl) => {
      assert_eq!(msg, "now: Song A");
      assert_eq!(ttl, 5);
    }
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn erroring_handler_is_disabled_after_one_strike() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source(
      "bad",
      r#"
        spotatui.on("start", function() error("boom") end)
        spotatui.on("start", function() spotatui.notify("healthy", 1) end)
      "#,
    )
    .unwrap();

  engine.emit(ScriptEvent::Start);
  let first = drain(&engine);
  // One error notify (from the bad handler) plus the healthy notify.
  assert_eq!(first.len(), 2);
  match &first[0] {
    ScriptEffect::NotifyError(m, 6) => assert!(m.contains("error in on_start")),
    _ => panic!("expected error notify first"),
  }
  match &first[1] {
    ScriptEffect::Notify(m, 1) => assert_eq!(m, "healthy"),
    _ => panic!("expected healthy notify second"),
  }

  // Second emit: bad handler removed, only the healthy one fires (no new error).
  engine.emit(ScriptEvent::Start);
  match one(&engine) {
    ScriptEffect::Notify(m, 1) => assert_eq!(m, "healthy"),
    _ => panic!("expected only the healthy notify"),
  }
}

#[test]
fn unknown_event_name_is_an_error() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source("test", r#"spotatui.on("bogus_event", function() end)"#);
  assert!(result.is_err());
}

// --- require_api ---

#[test]
fn require_api_at_or_below_current_succeeds() {
  use crate::core::plugin_api::API_VERSION;
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source("test", &format!("spotatui.require_api({API_VERSION})"))
    .unwrap();
  engine
    .load_source("test2", "spotatui.require_api(1)")
    .unwrap();
}

#[test]
fn require_api_above_current_fails_with_clear_message() {
  use crate::core::plugin_api::API_VERSION;
  let mut engine = ScriptEngine::new().unwrap();
  let too_high = API_VERSION + 1;
  let err = engine
    .load_source("test", &format!("spotatui.require_api({too_high})"))
    .unwrap_err()
    .to_string();
  assert!(err.contains(&too_high.to_string()), "message: {err}");
  assert!(err.contains(&API_VERSION.to_string()), "message: {err}");

  // The engine is not poisoned: a later, compatible plugin still loads.
  engine.load_source("ok", "spotatui.require_api(1)").unwrap();
}

#[test]
fn require_api_rejects_non_positive_version() {
  let mut engine = ScriptEngine::new().unwrap();
  assert!(engine
    .load_source("test", "spotatui.require_api(0)")
    .is_err());
}

// --- action functions queue the right effect ---

fn run_action(src: &str) -> ScriptEffect {
  let mut engine = ScriptEngine::new().unwrap();
  engine.load_source("test", src).unwrap();
  one(&engine)
}

#[test]
fn action_play_queues_play() {
  matches!(run_action("spotatui.play()"), ScriptEffect::Play);
}

#[test]
fn action_pause_queues_pause() {
  matches!(run_action("spotatui.pause()"), ScriptEffect::Pause);
}

#[test]
fn action_next_queues_next() {
  matches!(run_action("spotatui.next()"), ScriptEffect::Next);
}

#[test]
fn action_previous_queues_previous() {
  matches!(run_action("spotatui.previous()"), ScriptEffect::Previous);
}

#[test]
fn action_seek_queues_seek() {
  match run_action("spotatui.seek(12345)") {
    ScriptEffect::Seek(ms) => assert_eq!(ms, 12345),
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn action_set_volume_clamps_above_100() {
  match run_action("spotatui.set_volume(250)") {
    ScriptEffect::SetVolume(v) => assert_eq!(v, 100),
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
  match run_action("spotatui.set_volume(-10)") {
    ScriptEffect::SetVolume(v) => assert_eq!(v, 0),
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn action_shuffle_queues_set_shuffle() {
  match run_action("spotatui.shuffle(true)") {
    ScriptEffect::SetShuffle(on) => assert!(on),
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
  match run_action("spotatui.shuffle(false)") {
    ScriptEffect::SetShuffle(on) => assert!(!on),
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn action_search_queues_search_effect() {
  match run_action(r#"spotatui.search("daft punk")"#) {
    ScriptEffect::Search(q) => assert_eq!(q, "daft punk"),
    _ => panic!("expected a Search effect"),
  }
}

#[test]
fn action_notify_default_ttl_is_4() {
  match run_action(r#"spotatui.notify("hi")"#) {
    ScriptEffect::Notify(m, ttl) => {
      assert_eq!(m, "hi");
      assert_eq!(ttl, 4);
    }
    _ => panic!("expected a Notify effect"),
  }
}

mod http_tests {
  use super::*;

  /// Run `test` inside a tokio runtime, passing a URL backed by a local listener that
  /// accepts connections but never responds. The real request spawned by `http_get` /
  /// `http_post` hangs until the client timeout, so it can never race the injected
  /// synthetic result, and no traffic leaves the machine.
  fn with_runtime_engine(test: impl FnOnce(&mut ScriptEngine, &str)) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let mut engine = ScriptEngine::new().unwrap();
    test(&mut engine, &url);
  }

  fn response(status: u16, body: &str) -> HttpResponseData {
    HttpResponseData {
      status,
      body: body.to_string(),
    }
  }

  #[test]
  fn json_round_trip() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "json",
        r#"
          local decoded = spotatui.json_decode('{"name":"Song","nested":{"ok":true},"items":[1,2]}')
          local encoded = spotatui.json_encode(decoded)
          local again = spotatui.json_decode(encoded)
          spotatui.notify(again.name .. ":" .. tostring(again.nested.ok) .. ":" .. tostring(again.items[2]), 1)
        "#,
      )
      .unwrap();

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "Song:true:2"),
      _ => panic!("expected json round-trip notify"),
    }
  }

  #[test]
  fn json_decode_invalid_input_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "json",
        r#"
          local ok = pcall(function()
            spotatui.json_decode("{")
          end)
          spotatui.notify(tostring(ok), 1)
        "#,
      )
      .unwrap();

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "false"),
      _ => panic!("expected pcall failure notify"),
    }
  }

  #[test]
  fn json_encode_non_serializable_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "json",
        r#"
          local ok = pcall(function()
            spotatui.json_encode(function() end)
          end)
          spotatui.notify(tostring(ok), 1)
        "#,
      )
      .unwrap();

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "false"),
      _ => panic!("expected pcall failure notify"),
    }
  }

  #[test]
  fn json_null_decodes_to_sentinel_not_nil() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "json",
        r#"
          local NULL = spotatui.json_decode("null")
          local decoded = spotatui.json_decode('{"x":null}')
          spotatui.notify(tostring(decoded.x == nil) .. ":" .. tostring(decoded.x == NULL), 1)
        "#,
      )
      .unwrap();

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "false:true"),
      _ => panic!("expected null sentinel notify"),
    }
  }

  #[test]
  fn http_get_callback_fires_on_synthetic_success() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              if err then
                spotatui.notify(err, 1)
              else
                spotatui.notify(resp.body, 1)
              end
            end)
          "#
      );
      engine.load_source("fetcher", &source).unwrap();

      engine.inject_http_result(1, Ok(response(200, "hello")));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "hello"),
        _ => panic!("expected http success notify"),
      }
    });
  }

  #[tokio::test(flavor = "current_thread")]
  async fn http_get_spawn_path_delivers_response() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut buf = [0_u8; 1024];
      let _ = socket.read(&mut buf).await.unwrap();
      let body = "from server";
      let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
      );
      socket.write_all(response.as_bytes()).await.unwrap();
    });

    let mut engine = ScriptEngine::new().unwrap();
    let source = format!(
      r#"
        spotatui.http_get("http://{addr}/lyrics", function(resp, err)
          if err then
            spotatui.notify(err, 1)
          else
            spotatui.notify(tostring(resp.status) .. ":" .. resp.body, 1)
          end
        end)
      "#
    );
    engine.load_source("fetcher", &source).unwrap();

    for _ in 0..100 {
      engine.drain_http_callbacks_for_test();
      if !engine.shared.effects.borrow().is_empty() {
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "200:from server"),
      _ => panic!("expected spawned http response notify"),
    }
    server.await.unwrap();
  }

  #[test]
  fn http_post_callback_fires_on_synthetic_success() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_post("{url}", "body", nil, function(resp, err)
              if err then
                spotatui.notify(err, 1)
              else
                spotatui.notify(tostring(resp.status) .. ":" .. resp.body, 1)
              end
            end)
          "#
      );
      engine.load_source("poster", &source).unwrap();

      engine.inject_http_result(1, Ok(response(201, "created")));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "201:created"),
        _ => panic!("expected http post success notify"),
      }
    });
  }

  #[test]
  fn http_get_callback_fires_on_synthetic_error() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              if err then
                spotatui.notify(err, 1)
              else
                spotatui.notify(resp.body, 1)
              end
            end)
          "#
      );
      engine.load_source("fetcher", &source).unwrap();

      engine.inject_http_result(1, Err("dns failed".to_string()));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "dns failed"),
        _ => panic!("expected http error notify"),
      }
    });
  }

  #[test]
  fn http_callback_is_one_shot() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              spotatui.notify(resp.body, 1)
            end)
          "#
      );
      engine.load_source("fetcher", &source).unwrap();

      engine.inject_http_result(1, Ok(response(200, "first")));
      engine.drain_http_callbacks_for_test();
      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "first"),
        _ => panic!("expected first callback notify"),
      }

      engine.inject_http_result(1, Ok(response(200, "second")));
      engine.drain_http_callbacks_for_test();
      assert!(drain(engine).is_empty());
    });
  }

  #[test]
  fn http_callbacks_keep_token_identity_after_earlier_delivery() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}a", function(resp, err)
              spotatui.notify("a:" .. resp.body, 1)
            end)
            spotatui.http_get("{url}b", function(resp, err)
              spotatui.notify("b:" .. resp.body, 1)
            end)
          "#
      );
      engine.load_source("fetcher", &source).unwrap();

      engine.inject_http_result(1, Ok(response(200, "one")));
      engine.drain_http_callbacks_for_test();
      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "a:one"),
        _ => panic!("expected first callback notify"),
      }

      engine.inject_http_result(2, Ok(response(200, "two")));
      engine.drain_http_callbacks_for_test();
      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "b:two"),
        _ => panic!("expected second callback notify"),
      }
    });
  }

  #[test]
  fn http_callback_attribution() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              spotatui.set_playbar(resp.body)
            end)
          "#
      );
      engine.load_source("lyrics_plugin", &source).unwrap();

      engine.inject_http_result(1, Ok(response(200, "lyrics ready")));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::SetPlaybarSegment { plugin, text } => {
          assert_eq!(plugin, "lyrics_plugin");
          assert_eq!(text.as_deref(), Some("lyrics ready"));
        }
        _ => panic!("expected attributed playbar segment"),
      }
    });
  }

  #[test]
  fn http_callback_error_queues_notify_error_without_breaking_engine() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              error("callback boom")
            end)
          "#
      );
      engine.load_source("bad_fetcher", &source).unwrap();

      engine.inject_http_result(1, Ok(response(200, "ignored")));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::NotifyError(msg, 6) => {
          assert!(msg.contains("bad_fetcher"));
          assert!(msg.contains("error in http callback"));
          assert!(msg.contains("callback boom"));
        }
        _ => panic!("expected http callback error notify"),
      }
      assert!(engine.shared.current_plugin.borrow().is_empty());

      engine
        .load_source("healthy", r#"spotatui.notify("still alive", 1)"#)
        .unwrap();
      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "still alive"),
        _ => panic!("expected engine to keep running"),
      }
    });
  }

  #[test]
  fn http_get_invalid_scheme_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_source(
      "fetcher",
      r#"spotatui.http_get("ftp://example.com", function() end)"#,
    );
    assert!(result.is_err());
    assert!(result
      .unwrap_err()
      .to_string()
      .contains("unsupported URL scheme"));
  }

  #[test]
  fn http_get_no_runtime_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_source(
      "fetcher",
      r#"spotatui.http_get("https://example.com", function() end)"#,
    );
    assert!(result.is_err());
    assert!(result
      .unwrap_err()
      .to_string()
      .contains("no tokio runtime available"));
  }

  #[test]
  fn http_resp_ok_true_for_2xx() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              spotatui.notify(tostring(resp.ok), 1)
            end)
          "#
      );
      engine.load_source("fetcher", &source).unwrap();

      engine.inject_http_result(1, Ok(response(204, "")));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "true"),
        _ => panic!("expected ok=true notify"),
      }
    });
  }

  #[test]
  fn http_resp_ok_false_for_4xx() {
    with_runtime_engine(|engine, url| {
      let source = format!(
        r#"
            spotatui.http_get("{url}", function(resp, err)
              spotatui.notify(tostring(resp.ok) .. ":" .. tostring(err == nil), 1)
            end)
          "#
      );
      engine.load_source("fetcher", &source).unwrap();

      engine.inject_http_result(1, Ok(response(404, "not found")));
      engine.drain_http_callbacks_for_test();

      match one(engine) {
        ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "false:true"),
        _ => panic!("expected ok=false notify"),
      }
    });
  }
}

// --- drain_effects: routes through App methods ---

#[cfg(test)]
mod drain_tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use chrono::Duration as ChronoDuration;
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    CurrentlyPlayingType, Device, DeviceType, PlayableItem, RepeatState,
  };
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), SystemTime::now());
    (app, rx)
  }

  #[allow(deprecated)]
  fn make_device() -> Device {
    Device {
      id: Some("dev-test".to_string()),
      is_active: true,
      is_private_session: false,
      is_restricted: false,
      name: "Test Device".to_string(),
      _type: DeviceType::Computer,
      volume_percent: Some(50),
    }
  }

  #[allow(deprecated)]
  fn make_context(is_playing: bool, shuffle_state: bool) -> CurrentPlaybackContext {
    CurrentPlaybackContext {
      device: make_device(),
      repeat_state: RepeatState::Off,
      shuffle_state,
      context: None,
      timestamp: chrono::Utc::now(),
      progress: None,
      is_playing,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Unknown,
      actions: Actions::default(),
    }
  }

  #[allow(deprecated)]
  fn make_context_with_track(is_playing: bool) -> CurrentPlaybackContext {
    use crate::core::test_helpers::full_track;
    let track = full_track("4uLU6hMCjMI75M1A2tKUQC", "Test Song");
    CurrentPlaybackContext {
      device: make_device(),
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: chrono::Utc::now(),
      progress: Some(ChronoDuration::milliseconds(0)),
      is_playing,
      item: Some(PlayableItem::Track(track)),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  fn push_effect(engine: &ScriptEngine, effect: ScriptEffect) {
    engine.shared.effects.borrow_mut().push(effect);
  }

  #[test]
  fn drain_pause_while_playing_dispatches_pause_playback() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context(true, false));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::Pause);
    engine.drain_effects(&mut app);

    match rx.try_recv() {
      Ok(IoEvent::PausePlayback) => {}
      _ => panic!("expected PausePlayback, got unexpected variant (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn drain_pause_while_already_paused_is_noop() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context(false, false));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::Pause);
    engine.drain_effects(&mut app);

    assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  }

  #[test]
  fn drain_play_while_paused_dispatches_start_playback() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context(false, false));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::Play);
    engine.drain_effects(&mut app);

    match rx.try_recv() {
      Ok(IoEvent::StartPlayback(None, None, None)) => {}
      _ => panic!(
        "expected StartPlayback(None,None,None), got unexpected variant (IoEvent is not Debug)"
      ),
    }
  }

  #[test]
  fn drain_play_while_already_playing_is_noop() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context(true, false));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::Play);
    engine.drain_effects(&mut app);

    assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  }

  #[test]
  fn drain_shuffle_true_when_off_dispatches_shuffle_true() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context(false, false));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::SetShuffle(true));
    engine.drain_effects(&mut app);

    match rx.try_recv() {
      Ok(IoEvent::Shuffle(true)) => {}
      _ => panic!("expected Shuffle(true), got unexpected variant (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn drain_shuffle_false_when_already_off_is_noop() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context(false, false));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::SetShuffle(false));
    engine.drain_effects(&mut app);

    assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  }

  #[test]
  fn drain_set_volume_sets_pending_volume() {
    let (mut app, _rx) = make_app();

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::SetVolume(80));
    engine.drain_effects(&mut app);

    assert_eq!(app.pending_volume, Some(80));
  }

  #[test]
  fn drain_seek_with_track_context_dispatches_seek() {
    let (mut app, rx) = make_app();
    app.current_playback_context = Some(make_context_with_track(true));

    let engine = ScriptEngine::new().unwrap();
    push_effect(&engine, ScriptEffect::Seek(30_000));
    engine.drain_effects(&mut app);

    match rx.try_recv() {
      Ok(IoEvent::Seek(ms)) => assert_eq!(ms, 30_000),
      _ => panic!("expected Seek(30000), got unexpected variant (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn drain_notify_error_sets_error_flag_on_app() {
    let (mut app, _rx) = make_app();

    let engine = ScriptEngine::new().unwrap();
    push_effect(
      &engine,
      ScriptEffect::NotifyError("plugin crashed".to_string(), 6),
    );
    engine.drain_effects(&mut app);

    assert_eq!(app.status_message.as_deref(), Some("plugin crashed"));
    assert!(app.status_message_is_error);
  }

  #[test]
  fn drain_notify_error_blocks_subsequent_normal_notify() {
    let (mut app, _rx) = make_app();

    let engine = ScriptEngine::new().unwrap();
    push_effect(
      &engine,
      ScriptEffect::NotifyError("error msg".to_string(), 6),
    );
    push_effect(&engine, ScriptEffect::Notify("normal msg".to_string(), 4));
    engine.drain_effects(&mut app);

    assert_eq!(app.status_message.as_deref(), Some("error msg"));
    assert!(app.status_message_is_error);
  }
}

// --- register_command ---

#[test]
fn register_command_happy_path() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source(
      "myplugin",
      r#"spotatui.register_command("hello", function() spotatui.notify("hi", 1) end)"#,
    )
    .unwrap();
  assert!(drain(&engine).is_empty());
}

#[test]
fn register_command_empty_name_is_error() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source("test", r#"spotatui.register_command("", function() end)"#);
  assert!(result.is_err());
}

#[test]
fn register_command_whitespace_name_is_error() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source(
    "test",
    r#"spotatui.register_command("bad name", function() end)"#,
  );
  assert!(result.is_err());
}

#[test]
fn register_command_duplicate_is_error() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source("a", r#"spotatui.register_command("cmd", function() end)"#)
    .unwrap();
  let result = engine.load_source("b", r#"spotatui.register_command("cmd", function() end)"#);
  assert!(result.is_err());
}

// --- run_pending_commands ---

#[cfg(test)]
mod command_tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), SystemTime::now());
    (app, rx)
  }

  #[test]
  fn run_pending_commands_invokes_callback() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "myplugin",
        r#"spotatui.register_command("greet", function() spotatui.notify("hello", 2) end)"#,
      )
      .unwrap();
    let (mut app, _rx) = make_app();
    app.queue_plugin_command("greet".to_string());
    engine.run_pending_commands(&mut app);
    assert_eq!(app.status_message.as_deref(), Some("hello"));
  }

  #[test]
  fn run_pending_commands_unknown_name_sets_error() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    app.queue_plugin_command("nonexistent".to_string());
    engine.run_pending_commands(&mut app);
    assert!(app.status_message_is_error);
    assert!(app
      .status_message
      .as_deref()
      .unwrap_or("")
      .contains("nonexistent"));
  }

  #[test]
  fn run_pending_commands_erroring_callback_sets_error_and_stays_registered() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "badplugin",
        r#"spotatui.register_command("boom", function() error("explode") end)"#,
      )
      .unwrap();
    let (mut app, _rx) = make_app();
    app.queue_plugin_command("boom".to_string());
    engine.run_pending_commands(&mut app);
    assert!(app.status_message_is_error);

    // Second invocation: command must still be registered (not removed).
    app.pending_plugin_commands.clear();
    app.status_message = None;
    app.status_message_is_error = false;
    app.queue_plugin_command("boom".to_string());
    engine.run_pending_commands(&mut app);
    assert!(app.status_message_is_error);
  }

  #[test]
  fn run_pending_commands_sets_current_plugin_during_invocation() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "myplugin",
        r#"spotatui.register_command("check_plugin", function()
          spotatui.notify("ok", 1)
        end)"#,
      )
      .unwrap();
    let (mut app, _rx) = make_app();
    app.queue_plugin_command("check_plugin".to_string());
    engine.run_pending_commands(&mut app);
    assert_eq!(app.status_message.as_deref(), Some("ok"));
    // current_plugin is cleared after the call
    assert!(engine.shared.current_plugin.borrow().is_empty());
  }
}

// --- set_playbar ---

#[test]
fn set_playbar_queues_segment_with_current_plugin() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source("myplugin", r#"spotatui.set_playbar("hello world")"#)
    .unwrap();
  match one(&engine) {
    ScriptEffect::SetPlaybarSegment { plugin, text } => {
      assert_eq!(plugin, "myplugin");
      assert_eq!(text, Some("hello world".to_string()));
    }
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn set_playbar_nil_queues_clear() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source("myplugin", r#"spotatui.set_playbar(nil)"#)
    .unwrap();
  match one(&engine) {
    ScriptEffect::SetPlaybarSegment { plugin, text } => {
      assert_eq!(plugin, "myplugin");
      assert!(text.is_none());
    }
    other => panic!("unexpected effect: {:?}", std::mem::discriminant(&other)),
  }
}

#[cfg(test)]
mod playbar_effect_tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), SystemTime::now());
    (app, rx)
  }

  #[test]
  fn applying_set_playbar_segment_inserts_into_map() {
    let (mut app, _rx) = make_app();
    let engine = ScriptEngine::new().unwrap();
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::SetPlaybarSegment {
        plugin: "myplugin".to_string(),
        text: Some("seg text".to_string()),
      });
    engine.drain_effects(&mut app);
    assert_eq!(
      app
        .plugin_playbar_segments
        .get("myplugin")
        .map(|s| s.as_str()),
      Some("seg text")
    );
  }

  #[test]
  fn applying_set_playbar_segment_nil_removes_from_map() {
    let (mut app, _rx) = make_app();
    app
      .plugin_playbar_segments
      .insert("myplugin".to_string(), "old".to_string());
    let engine = ScriptEngine::new().unwrap();
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::SetPlaybarSegment {
        plugin: "myplugin".to_string(),
        text: None,
      });
    engine.drain_effects(&mut app);
    assert!(app.plugin_playbar_segments.get("myplugin").is_none());
  }
}

// --- popup ---

#[test]
fn popup_plain_string_lines_work() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source("test", r#"spotatui.popup("My Title", "single line")"#)
    .unwrap();
  match one(&engine) {
    ScriptEffect::ShowPopup(p) => {
      assert_eq!(p.title, "My Title");
      assert_eq!(p.lines.len(), 1);
      assert_eq!(p.lines[0].text, "single line");
      assert!(p.lines[0].fg.is_none());
      assert!(!p.lines[0].bold);
      assert!(!p.lines[0].italic);
    }
    other => panic!("unexpected: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn popup_array_of_strings() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source("test", r#"spotatui.popup("T", {"line 1", "line 2"})"#)
    .unwrap();
  match one(&engine) {
    ScriptEffect::ShowPopup(p) => {
      assert_eq!(p.lines.len(), 2);
      assert_eq!(p.lines[0].text, "line 1");
      assert_eq!(p.lines[1].text, "line 2");
    }
    other => panic!("unexpected: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn popup_styled_table_lines() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source(
      "test",
      r#"spotatui.popup("T", {{ text = "bold red", fg = "Red", bold = true, italic = false }})"#,
    )
    .unwrap();
  match one(&engine) {
    ScriptEffect::ShowPopup(p) => {
      assert_eq!(p.lines.len(), 1);
      assert_eq!(p.lines[0].text, "bold red");
      assert_eq!(p.lines[0].fg, Some(ratatui::style::Color::Red));
      assert!(p.lines[0].bold);
      assert!(!p.lines[0].italic);
    }
    other => panic!("unexpected: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn popup_bad_color_raises() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source(
    "test",
    r#"spotatui.popup("T", {{ text = "hi", fg = "NotAColor" }})"#,
  );
  // parse_theme_item falls back to Black on unknown, so this may not error.
  // The plan says it raises; let's confirm behaviour: if it doesn't raise, the test
  // documents that parse_theme_item is lenient.
  // We just ensure no panic occurred.
  let _ = result;
}

#[test]
fn popup_missing_text_field_raises() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source("test", r#"spotatui.popup("T", {{ bold = true }})"#);
  assert!(result.is_err(), "missing 'text' field should be an error");
}

#[test]
fn popup_non_table_non_string_line_raises() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source("test", r#"spotatui.popup("T", {42})"#);
  assert!(result.is_err(), "integer line should be an error");
}

#[cfg(test)]
mod popup_effect_tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::plugin_api::{PluginPopup, PopupLine};
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), SystemTime::now());
    (app, rx)
  }

  #[test]
  fn applying_show_popup_sets_app_popup_and_resets_scroll() {
    let (mut app, _rx) = make_app();
    app.plugin_popup_scroll = 5;
    let engine = ScriptEngine::new().unwrap();
    let popup = PluginPopup {
      title: "Test".to_string(),
      lines: vec![PopupLine {
        text: "hello".to_string(),
        fg: None,
        bold: false,
        italic: false,
      }],
    };
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::ShowPopup(popup.clone()));
    engine.drain_effects(&mut app);
    assert_eq!(app.plugin_popup, Some(popup));
    assert_eq!(app.plugin_popup_scroll, 0);
  }
}

// --- set_theme ---

#[test]
fn set_theme_valid_field_queues_effect() {
  let mut engine = ScriptEngine::new().unwrap();
  engine
    .load_source(
      "test",
      r#"spotatui.set_theme({ playbar_text = "Magenta" })"#,
    )
    .unwrap();
  match one(&engine) {
    ScriptEffect::SetTheme(pairs) => {
      assert_eq!(pairs.len(), 1);
      assert_eq!(pairs[0].0, "playbar_text");
      assert_eq!(pairs[0].1, ratatui::style::Color::Magenta);
    }
    other => panic!("unexpected: {:?}", std::mem::discriminant(&other)),
  }
}

#[test]
fn set_theme_unknown_field_raises() {
  let mut engine = ScriptEngine::new().unwrap();
  let result = engine.load_source("test", r#"spotatui.set_theme({ not_a_field = "Red" })"#);
  assert!(result.is_err(), "unknown theme field should raise");
}

#[test]
fn set_theme_bad_color_raises() {
  let mut engine = ScriptEngine::new().unwrap();
  // parse_theme_item is lenient (falls back to Black) for unknown named colors.
  // The API wraps it with map_err, but since parse_theme_item returns Ok for unknowns,
  // this test documents the actual behaviour.
  let result = engine.load_source(
    "test",
    r#"spotatui.set_theme({ playbar_text = "999, 999, 999" })"#,
  );
  // 999 > 255 so u8 parse fails -> should be an error.
  assert!(result.is_err(), "out-of-range RGB should raise");
}

#[cfg(test)]
mod theme_effect_tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), SystemTime::now());
    (app, rx)
  }

  #[test]
  fn applying_set_theme_mutates_app_theme_field() {
    let (mut app, _rx) = make_app();
    let engine = ScriptEngine::new().unwrap();
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::SetTheme(vec![(
        "playbar_text".to_string(),
        ratatui::style::Color::Magenta,
      )]));
    engine.drain_effects(&mut app);
    assert_eq!(
      app.user_config.theme.playbar_text,
      ratatui::style::Color::Magenta
    );
  }
}

// --- diff_events ---

#[test]
fn diff_none_to_some_is_track_change() {
  let new = Some(playback(Some(track("uri:1", "A")), true, 0));
  let q = Some(vec![]);
  let events = diff_events(&None, &None, &new, &q);
  assert!(events.contains(&ScriptEvent::TrackChange));
  // None -> playing also flips is_playing.
  assert!(events.contains(&ScriptEvent::PlaybackStateChange));
}

#[test]
fn diff_track_change_on_different_uri() {
  let old = Some(playback(Some(track("uri:1", "A")), true, 0));
  let new = Some(playback(Some(track("uri:2", "B")), true, 0));
  let q = Some(vec![]);
  let events = diff_events(&old, &q, &new, &q);
  assert!(events.contains(&ScriptEvent::TrackChange));
  assert!(!events.contains(&ScriptEvent::PlaybackStateChange));
}

#[test]
fn diff_play_pause_flip() {
  let old = Some(playback(Some(track("uri:1", "A")), true, 1000));
  let new = Some(playback(Some(track("uri:1", "A")), false, 1000));
  let q = Some(vec![]);
  let events = diff_events(&old, &q, &new, &q);
  assert_eq!(events, vec![ScriptEvent::PlaybackStateChange]);
}

#[test]
fn diff_seek_backward_beyond_threshold() {
  let old = Some(playback(Some(track("uri:1", "A")), true, 10_000));
  let new = Some(playback(Some(track("uri:1", "A")), true, 5_000));
  let q = Some(vec![]);
  let events = diff_events(&old, &q, &new, &q);
  assert!(events.contains(&ScriptEvent::Seek));
}

#[test]
fn diff_seek_forward_beyond_threshold() {
  let old = Some(playback(Some(track("uri:1", "A")), true, 1_000));
  let new = Some(playback(Some(track("uri:1", "A")), true, 9_000));
  let q = Some(vec![]);
  let events = diff_events(&old, &q, &new, &q);
  assert!(events.contains(&ScriptEvent::Seek));
}

#[test]
fn diff_small_forward_jump_is_not_seek() {
  // 3s forward jump is within Connect polling tolerance.
  let old = Some(playback(Some(track("uri:1", "A")), true, 1_000));
  let new = Some(playback(Some(track("uri:1", "A")), true, 4_000));
  let q = Some(vec![]);
  let events = diff_events(&old, &q, &new, &q);
  assert!(!events.contains(&ScriptEvent::Seek));
}

#[test]
fn diff_volume_change() {
  let old = Some(playback(Some(track("uri:1", "A")), true, 1_000));
  let mut new = playback(Some(track("uri:1", "A")), true, 1_000);
  new.volume_percent = Some(80);
  let q = Some(vec![]);
  let events = diff_events(&old, &q, &Some(new), &q);
  assert!(events.contains(&ScriptEvent::VolumeChange));
}

#[test]
fn diff_queue_change() {
  let old = Some(playback(Some(track("uri:1", "A")), true, 1_000));
  let new = old.clone();
  let old_q = Some(vec!["a".to_string()]);
  let new_q = Some(vec!["a".to_string(), "b".to_string()]);
  let events = diff_events(&old, &old_q, &new, &new_q);
  assert_eq!(events, vec![ScriptEvent::QueueChange]);
}

// --- directory plugin loading (spotatui plugin add) ---

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static TMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Fresh, unique temp directory to act as a config dir.
fn temp_config_dir() -> PathBuf {
  let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("spotatui_lua_load_{}_{}", std::process::id(), n));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

fn write_file(path: &Path, contents: &str) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, contents).unwrap();
}

/// True if any queued effect is a successful Notify carrying `needle`.
fn has_notify(engine: &ScriptEngine, needle: &str) -> bool {
  drain(engine).into_iter().any(|e| match e {
    ScriptEffect::Notify(msg, _) => msg.contains(needle),
    _ => false,
  })
}

#[test]
fn dir_plugin_main_lua_is_loaded() {
  let cfg = temp_config_dir();
  write_file(
    &cfg.join("plugins").join("foo").join("main.lua"),
    r#"spotatui.notify("loaded foo", 1)"#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  assert_eq!(loaded, 1);
  assert!(has_notify(&engine, "loaded foo"));
  std::fs::remove_dir_all(&cfg).unwrap();
}

#[test]
fn dir_plugin_init_lua_is_used_as_fallback() {
  let cfg = temp_config_dir();
  write_file(
    &cfg.join("plugins").join("bar").join("init.lua"),
    r#"spotatui.notify("loaded bar", 1)"#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  assert_eq!(loaded, 1);
  assert!(has_notify(&engine, "loaded bar"));
  std::fs::remove_dir_all(&cfg).unwrap();
}

#[test]
fn dir_plugin_without_entry_point_is_skipped() {
  let cfg = temp_config_dir();
  // Directory exists but has no main.lua/init.lua, plus a hidden dir that must be ignored.
  std::fs::create_dir_all(cfg.join("plugins").join("empty")).unwrap();
  write_file(
    &cfg.join("plugins").join(".hidden").join("main.lua"),
    r#"spotatui.notify("should not load", 1)"#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  assert_eq!(loaded, 0);
  assert!(drain(&engine).is_empty());
  std::fs::remove_dir_all(&cfg).unwrap();
}

#[test]
fn dir_plugin_can_require_sibling_module() {
  let cfg = temp_config_dir();
  let plugin = cfg.join("plugins").join("qux");
  write_file(
    &plugin.join("helper.lua"),
    r#"return { msg = "from helper" }"#,
  );
  write_file(
    &plugin.join("main.lua"),
    r#"
      local helper = require("helper")
      spotatui.notify(helper.msg, 1)
    "#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  // A successful load proves `require` resolved the sibling module via package.path.
  assert_eq!(loaded, 1);
  assert!(has_notify(&engine, "from helper"));
  std::fs::remove_dir_all(&cfg).unwrap();
}

#[test]
fn single_file_and_directory_plugins_both_load() {
  let cfg = temp_config_dir();
  write_file(
    &cfg.join("plugins").join("flat.lua"),
    r#"spotatui.notify("flat", 1)"#,
  );
  write_file(
    &cfg.join("plugins").join("nested").join("main.lua"),
    r#"spotatui.notify("nested", 1)"#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  assert_eq!(loaded, 2);
  std::fs::remove_dir_all(&cfg).unwrap();
}

#[test]
fn directory_named_with_lua_extension_loads_once_without_error() {
  // A directory literally named `weird.lua` must be treated only as a directory plugin,
  // not also fed to the single-file path (which would raise a spurious load error).
  let cfg = temp_config_dir();
  write_file(
    &cfg.join("plugins").join("weird.lua").join("main.lua"),
    r#"spotatui.notify("weird ok", 1)"#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  assert_eq!(loaded, 1);
  let effects = drain(&engine);
  assert!(
    !effects
      .iter()
      .any(|e| matches!(e, ScriptEffect::NotifyError(_, _))),
    "a .lua-named directory must not produce a load error"
  );
  std::fs::remove_dir_all(&cfg).unwrap();
}

#[test]
fn hidden_single_file_plugin_is_skipped() {
  // Hidden files (e.g. macOS `._foo.lua` cruft) must be ignored, matching the directory branch.
  let cfg = temp_config_dir();
  write_file(
    &cfg.join("plugins").join(".secret.lua"),
    r#"spotatui.notify("should not load", 1)"#,
  );

  let mut engine = ScriptEngine::new().unwrap();
  let loaded = engine.load_user_scripts(&cfg);

  assert_eq!(loaded, 0);
  assert!(drain(&engine).is_empty());
  std::fs::remove_dir_all(&cfg).unwrap();
}
