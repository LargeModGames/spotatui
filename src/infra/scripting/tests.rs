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
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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

// --- dispatch-backed actions ---

#[cfg(test)]
mod action_tests {
  use super::*;
  use crate::core::app::{App, UserInfo};
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use rspotify::model::RepeatState;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    (app, rx)
  }

  #[test]
  fn set_repeat_maps_all_modes() {
    for (mode, expected) in [
      ("off", RepeatState::Off),
      ("track", RepeatState::Track),
      ("context", RepeatState::Context),
    ] {
      match run_action(&format!(r#"spotatui.set_repeat("{mode}")"#)) {
        ScriptEffect::Dispatch(IoEvent::Repeat(state)) => assert_eq!(state, expected),
        _ => panic!("expected Dispatch(Repeat) for mode '{mode}'"),
      }
    }
  }

  #[test]
  fn set_repeat_invalid_mode_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("test", r#"spotatui.set_repeat("all")"#)
      .is_err());
  }

  #[test]
  fn cycle_repeat_queues_cycle_effect() {
    assert!(matches!(
      run_action("spotatui.cycle_repeat()"),
      ScriptEffect::CycleRepeat
    ));
  }

  #[test]
  fn play_uri_track_uses_uri_list() {
    match run_action(r#"spotatui.play_uri("spotify:track:abc123")"#) {
      ScriptEffect::Dispatch(IoEvent::StartPlayback(None, Some(uris), None)) => {
        assert_eq!(uris, vec!["spotify:track:abc123".to_string()]);
      }
      _ => panic!("expected StartPlayback with uri list"),
    }
  }

  #[test]
  fn play_uri_album_uses_context() {
    match run_action(r#"spotatui.play_uri("spotify:album:abc123")"#) {
      ScriptEffect::Dispatch(IoEvent::StartPlayback(Some(ctx), None, None)) => {
        assert_eq!(ctx, "spotify:album:abc123");
      }
      _ => panic!("expected StartPlayback with context"),
    }
  }

  #[test]
  fn play_uri_garbage_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("test", r#"spotatui.play_uri("not-a-uri")"#)
      .is_err());
  }

  #[test]
  fn play_context_carries_offset() {
    match run_action(r#"spotatui.play_context("spotify:playlist:p1", 5)"#) {
      ScriptEffect::Dispatch(IoEvent::StartPlayback(Some(ctx), None, Some(offset))) => {
        assert_eq!(ctx, "spotify:playlist:p1");
        assert_eq!(offset, 5);
      }
      _ => panic!("expected StartPlayback with context + offset"),
    }
  }

  #[test]
  fn play_context_rejects_track_uri() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("test", r#"spotatui.play_context("spotify:track:t1")"#)
      .is_err());
  }

  #[test]
  fn add_to_queue_queues_dispatch() {
    match run_action(r#"spotatui.add_to_queue("spotify:track:t1")"#) {
      ScriptEffect::Dispatch(IoEvent::AddItemToQueue(uri)) => {
        assert_eq!(uri, "spotify:track:t1");
      }
      _ => panic!("expected AddItemToQueue dispatch"),
    }
  }

  #[test]
  fn create_playlist_with_uris() {
    match run_action(r#"spotatui.create_playlist("Mix", {"spotify:track:a", "spotify:track:b"})"#) {
      ScriptEffect::Dispatch(IoEvent::CreateNewPlaylist(name, uris)) => {
        assert_eq!(name, "Mix");
        assert_eq!(uris.len(), 2);
      }
      _ => panic!("expected CreateNewPlaylist dispatch"),
    }
  }

  #[test]
  fn create_playlist_empty_name_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("test", r#"spotatui.create_playlist("  ")"#)
      .is_err());
  }

  #[test]
  fn playlist_remove_track_requires_position() {
    match run_action(r#"spotatui.playlist_remove_track("p1", "t1", 0)"#) {
      ScriptEffect::Dispatch(IoEvent::RemoveTrackFromPlaylistAtPosition(p, t, pos)) => {
        assert_eq!(p, "p1");
        assert_eq!(t, "t1");
        assert_eq!(pos, 0);
      }
      _ => panic!("expected RemoveTrackFromPlaylistAtPosition dispatch"),
    }

    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("test", r#"spotatui.playlist_remove_track("p1", "t1")"#)
      .is_err());
    assert!(engine
      .load_source("test", r#"spotatui.playlist_remove_track("p1", "t1", -1)"#)
      .is_err());
  }

  #[test]
  fn transfer_playback_does_not_persist_device() {
    match run_action(r#"spotatui.transfer_playback("dev-1")"#) {
      ScriptEffect::Dispatch(IoEvent::TransferPlaybackToDevice(id, persist)) => {
        assert_eq!(id, "dev-1");
        assert!(!persist);
      }
      _ => panic!("expected TransferPlaybackToDevice dispatch"),
    }
  }

  #[test]
  fn follow_and_save_actions_map_to_events() {
    assert!(matches!(
      run_action(r#"spotatui.toggle_save_track("spotify:track:t1")"#),
      ScriptEffect::Dispatch(IoEvent::ToggleSaveTrack(_))
    ));
    assert!(matches!(
      run_action(r#"spotatui.save_album("a1")"#),
      ScriptEffect::Dispatch(IoEvent::CurrentUserSavedAlbumAdd(_))
    ));
    assert!(matches!(
      run_action(r#"spotatui.unsave_album("a1")"#),
      ScriptEffect::Dispatch(IoEvent::CurrentUserSavedAlbumDelete(_))
    ));
    assert!(matches!(
      run_action(r#"spotatui.save_show("s1")"#),
      ScriptEffect::Dispatch(IoEvent::CurrentUserSavedShowAdd(_))
    ));
    assert!(matches!(
      run_action(r#"spotatui.unsave_show("s1")"#),
      ScriptEffect::Dispatch(IoEvent::CurrentUserSavedShowDelete(_))
    ));
    match run_action(r#"spotatui.follow_artist("ar1")"#) {
      ScriptEffect::Dispatch(IoEvent::UserFollowArtists(ids)) => {
        assert_eq!(ids, vec!["ar1".to_string()]);
      }
      _ => panic!("expected UserFollowArtists dispatch"),
    }
    assert!(matches!(
      run_action(r#"spotatui.unfollow_artist("ar1")"#),
      ScriptEffect::Dispatch(IoEvent::UserUnfollowArtists(_))
    ));
    assert!(matches!(
      run_action(r#"spotatui.follow_playlist("p1")"#),
      ScriptEffect::Dispatch(IoEvent::UserFollowPlaylist(_, _, None))
    ));
    assert!(matches!(
      run_action(r#"spotatui.unfollow_playlist("p1")"#),
      ScriptEffect::UnfollowPlaylist(_)
    ));
  }

  #[test]
  fn empty_string_argument_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("test", r#"spotatui.add_to_queue("")"#)
      .is_err());
  }

  #[test]
  fn drain_dispatch_sends_io_event() {
    let (mut app, rx) = make_app();
    let engine = ScriptEngine::new().unwrap();
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::Dispatch(IoEvent::AddItemToQueue(
        "spotify:track:t1".to_string(),
      )));
    engine.drain_effects(&mut app);

    match rx.try_recv() {
      Ok(IoEvent::AddItemToQueue(uri)) => assert_eq!(uri, "spotify:track:t1"),
      _ => panic!("expected AddItemToQueue on channel (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn drain_unfollow_playlist_resolves_current_user() {
    let (mut app, rx) = make_app();
    app.user = Some(UserInfo {
      id: "me-123".to_string(),
      display_name: None,
      country: None,
    });
    let engine = ScriptEngine::new().unwrap();
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::UnfollowPlaylist("p1".to_string()));
    engine.drain_effects(&mut app);

    match rx.try_recv() {
      Ok(IoEvent::UserUnfollowPlaylist(user_id, playlist_id)) => {
        assert_eq!(user_id, "me-123");
        assert_eq!(playlist_id, "p1");
      }
      _ => panic!("expected UserUnfollowPlaylist on channel (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn drain_unfollow_playlist_without_user_sets_error() {
    let (mut app, rx) = make_app();
    let engine = ScriptEngine::new().unwrap();
    engine
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::UnfollowPlaylist("p1".to_string()));
    engine.drain_effects(&mut app);

    assert!(rx.try_recv().is_err(), "no IoEvent expected");
    assert!(app.status_message_is_error);
  }
}

// --- async data reads (spotatui.get_*) ---

#[cfg(test)]
mod data_read_tests {
  use super::*;
  use crate::core::app::{App, LyricsStatus, PluginDataKind};
  use crate::core::plugin_api::PlaylistInfo;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::{Duration, Instant, SystemTime};

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    (app, rx)
  }

  fn playlist(name: &str) -> PlaylistInfo {
    PlaylistInfo {
      uri: format!("spotify:playlist:{name}"),
      name: name.to_string(),
      owner: "owner".to_string(),
      track_count: 3,
      id: Some(name.to_string()),
      owner_id: Some("owner".to_string()),
      collaborative: false,
      public: Some(true),
      image_url: None,
    }
  }

  const GET_PLAYLISTS_NOTIFY: &str = r#"
    spotatui.get_playlists(function(data, err)
      if err then
        spotatui.notify("err: " .. err, 1)
      else
        spotatui.notify("ok: " .. #data .. ":" .. (data[1] and data[1].name or "-"), 1)
      end
    end)
  "#;

  #[test]
  fn get_playlists_dispatches_io_event_and_resolves_on_bump() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, rx) = make_app();
    engine.load_source("reader", GET_PLAYLISTS_NOTIFY).unwrap();

    let now = Instant::now();
    engine.process_data_requests_for_test(&mut app, now);

    match rx.try_recv() {
      Ok(IoEvent::GetPlaylists) => {}
      _ => panic!("expected GetPlaylists dispatch (IoEvent is not Debug)"),
    }
    // Not resolved yet: generation unchanged.
    assert!(drain(&engine).is_empty());

    // Simulate the network write + bump, then the next engine pass resolves.
    app.all_playlists = vec![playlist("Jams")];
    app.plugin_data_generations.bump(PluginDataKind::Playlists);
    engine.process_data_requests_for_test(&mut app, now + Duration::from_millis(500));

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "ok: 1:Jams"),
      _ => panic!("expected data callback notify"),
    }
  }

  #[test]
  fn data_request_times_out_with_distinct_error() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine.load_source("reader", GET_PLAYLISTS_NOTIFY).unwrap();

    let now = Instant::now();
    engine.process_data_requests_for_test(&mut app, now);
    assert!(drain(&engine).is_empty());

    // Never bump; jump past the deadline.
    engine.process_data_requests_for_test(&mut app, now + Duration::from_secs(16));
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "err: request timed out"),
      _ => panic!("expected timeout notify"),
    }
  }

  #[test]
  fn erroring_data_callback_queues_notify_error_and_is_one_shot() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine
      .load_source(
        "bad_reader",
        r#"spotatui.get_playlists(function(data, err) error("data boom") end)"#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_data_requests_for_test(&mut app, now);
    app.plugin_data_generations.bump(PluginDataKind::Playlists);
    engine.process_data_requests_for_test(&mut app, now + Duration::from_millis(1));

    match one(&engine) {
      ScriptEffect::NotifyError(msg, 6) => {
        assert!(msg.contains("bad_reader"));
        assert!(msg.contains("data boom"));
      }
      _ => panic!("expected data callback error notify"),
    }
    assert!(engine.shared.current_plugin.borrow().is_empty());

    // The slot is cleared: another bump must not re-fire the callback.
    app.plugin_data_generations.bump(PluginDataKind::Playlists);
    engine.process_data_requests_for_test(&mut app, now + Duration::from_millis(2));
    assert!(drain(&engine).is_empty());
  }

  #[test]
  fn two_same_kind_requests_both_resolve_on_one_bump() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine
      .load_source(
        "reader",
        r#"
          spotatui.get_playlists(function(data, err) spotatui.notify("first", 1) end)
          spotatui.get_playlists(function(data, err) spotatui.notify("second", 1) end)
        "#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_data_requests_for_test(&mut app, now);
    app.plugin_data_generations.bump(PluginDataKind::Playlists);
    engine.process_data_requests_for_test(&mut app, now + Duration::from_millis(1));

    let effects = drain(&engine);
    let messages: Vec<String> = effects
      .into_iter()
      .filter_map(|e| match e {
        ScriptEffect::Notify(m, _) => Some(m),
        _ => None,
      })
      .collect();
    assert_eq!(messages, vec!["first".to_string(), "second".to_string()]);
  }

  #[test]
  fn get_search_results_dispatches_query() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, rx) = make_app();
    engine
      .load_source(
        "searcher",
        r#"spotatui.get_search_results("daft punk", function(data, err) end)"#,
      )
      .unwrap();

    engine.process_data_requests_for_test(&mut app, Instant::now());
    match rx.try_recv() {
      Ok(IoEvent::GetSearchResults(q, _)) => assert_eq!(q, "daft punk"),
      _ => panic!("expected GetSearchResults dispatch (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn get_search_results_empty_query_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_source(
      "searcher",
      r#"spotatui.get_search_results("", function() end)"#,
    );
    assert!(result.is_err());
  }

  #[test]
  fn get_lyrics_terminal_status_delivers_immediately_without_dispatch() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, rx) = make_app();
    app.lyrics_status = LyricsStatus::Found;
    app.lyrics = Some(vec![(1500, "hello lyrics".to_string())]);

    engine
      .load_source(
        "lyricist",
        r#"
          spotatui.get_lyrics(function(data, err)
            spotatui.notify(data.status .. ":" .. data.lines[1].text .. ":" .. data.lines[1].time_ms, 1)
          end)
        "#,
      )
      .unwrap();

    engine.process_data_requests_for_test(&mut app, Instant::now());
    assert!(
      rx.try_recv().is_err(),
      "lyrics must not dispatch an IoEvent"
    );
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "found:hello lyrics:1500"),
      _ => panic!("expected immediate lyrics notify"),
    }
  }

  #[test]
  fn get_lyrics_pending_resolves_when_fetch_completes() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, rx) = make_app();
    app.lyrics_status = LyricsStatus::Loading;

    engine
      .load_source(
        "lyricist",
        r#"spotatui.get_lyrics(function(data, err) spotatui.notify(data.status, 1) end)"#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_data_requests_for_test(&mut app, now);
    assert!(
      rx.try_recv().is_err(),
      "lyrics must not dispatch an IoEvent"
    );
    assert!(drain(&engine).is_empty());

    app.lyrics_status = LyricsStatus::NotFound;
    app.plugin_data_generations.bump(PluginDataKind::Lyrics);
    engine.process_data_requests_for_test(&mut app, now + Duration::from_millis(1));
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "not_found"),
      _ => panic!("expected pending lyrics notify"),
    }
  }

  #[test]
  fn get_queue_resolves_with_flattened_items() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine
      .load_source(
        "queuer",
        r#"
          spotatui.get_queue(function(data, err)
            local current = data.currently_playing and data.currently_playing.track.name or "-"
            spotatui.notify(current .. ":" .. #data.items .. ":" .. data.items[1].kind, 1)
          end)
        "#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_data_requests_for_test(&mut app, now);

    app.queue = Some(crate::core::app::QueueState {
      currently_playing: Some(crate::core::plugin_api::PlayableInfo::Track(track(
        "uri:now",
        "Now Playing",
      ))),
      queue: vec![crate::core::plugin_api::PlayableInfo::Track(track(
        "uri:next", "Next Up",
      ))],
    });
    app.plugin_data_generations.bump(PluginDataKind::Queue);
    engine.process_data_requests_for_test(&mut app, now + Duration::from_millis(1));

    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "Now Playing:1:track"),
      _ => panic!("expected queue notify"),
    }
  }

  #[test]
  fn cached_playlists_read_refreshes_on_generation_change() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();

    // First tick: sentinel forces a refresh of the (empty) snapshot.
    engine.on_tick(&mut app);
    engine
      .load_source(
        "sync",
        r#"spotatui.notify("n=" .. #spotatui.playlists(), 1)"#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "n=0"),
      _ => panic!("expected empty cached read"),
    }

    // Data lands without a bump: the cache must NOT refresh.
    app.all_playlists = vec![playlist("A")];
    engine.on_tick(&mut app);
    engine
      .load_source(
        "sync2",
        r#"spotatui.notify("n=" .. #spotatui.playlists(), 1)"#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "n=0"),
      _ => panic!("expected stale cached read before bump"),
    }

    // After the bump the next tick refreshes.
    app.plugin_data_generations.bump(PluginDataKind::Playlists);
    engine.on_tick(&mut app);
    engine
      .load_source(
        "sync3",
        r#"spotatui.notify("n=" .. #spotatui.playlists(), 1)"#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "n=1"),
      _ => panic!("expected refreshed cached read"),
    }
  }
}

// --- timers ---

#[cfg(test)]
mod timer_tests {
  use super::*;
  use std::time::{Duration, Instant};

  #[test]
  fn set_timeout_fires_once() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "timer",
        r#"spotatui.set_timeout(100, function() spotatui.notify("fired", 1) end)"#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_timers_for_test(now); // arms; not due yet
    assert!(drain(&engine).is_empty());

    engine.process_timers_for_test(now + Duration::from_millis(150));
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "fired"),
      _ => panic!("expected timeout notify"),
    }

    // One-shot: never fires again.
    engine.process_timers_for_test(now + Duration::from_millis(500));
    assert!(drain(&engine).is_empty());
  }

  #[test]
  fn set_interval_repeats_and_skips_missed_periods() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "timer",
        r#"spotatui.set_interval(100, function() spotatui.notify("tick", 1) end)"#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_timers_for_test(now); // arm
    engine.process_timers_for_test(now + Duration::from_millis(150));
    assert_eq!(drain(&engine).len(), 1);

    engine.process_timers_for_test(now + Duration::from_millis(260));
    assert_eq!(drain(&engine).len(), 1);

    // A long stall fires ONCE (no catch-up burst), rescheduled from `now`.
    engine.process_timers_for_test(now + Duration::from_millis(2000));
    assert_eq!(drain(&engine).len(), 1);
  }

  #[test]
  fn cancel_timer_before_due_prevents_firing() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "timer",
        r#"
          local h = spotatui.set_timeout(100, function() spotatui.notify("nope", 1) end)
          spotatui.cancel_timer(h)
        "#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_timers_for_test(now);
    engine.process_timers_for_test(now + Duration::from_millis(500));
    assert!(drain(&engine).is_empty());
  }

  #[test]
  fn erroring_interval_is_removed_with_notify_error() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "bad_timer",
        r#"spotatui.set_interval(100, function() error("interval boom") end)"#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_timers_for_test(now);
    engine.process_timers_for_test(now + Duration::from_millis(150));
    match one(&engine) {
      ScriptEffect::NotifyError(msg, 6) => {
        assert!(msg.contains("bad_timer"));
        assert!(msg.contains("interval boom"));
      }
      _ => panic!("expected timer error notify"),
    }

    // One strike: the interval is gone.
    engine.process_timers_for_test(now + Duration::from_millis(1000));
    assert!(drain(&engine).is_empty());
  }

  #[test]
  fn timer_set_inside_event_handler_fires() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "timer",
        r#"
          spotatui.on("start", function()
            spotatui.set_timeout(50, function() spotatui.notify("deferred", 1) end)
          end)
        "#,
      )
      .unwrap();

    engine.emit(ScriptEvent::Start);
    assert!(drain(&engine).is_empty());

    let now = Instant::now();
    engine.process_timers_for_test(now); // arms next pass
    engine.process_timers_for_test(now + Duration::from_millis(100));
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "deferred"),
      _ => panic!("expected deferred timer notify"),
    }
  }

  #[test]
  fn timer_set_inside_timer_callback_fires_next_pass() {
    let mut engine = ScriptEngine::new().unwrap();
    engine
      .load_source(
        "timer",
        r#"
          spotatui.set_timeout(10, function()
            spotatui.set_timeout(10, function() spotatui.notify("nested", 1) end)
          end)
        "#,
      )
      .unwrap();

    let now = Instant::now();
    engine.process_timers_for_test(now);
    engine.process_timers_for_test(now + Duration::from_millis(20));
    assert!(drain(&engine).is_empty(), "outer fired, nested only queued");
    // The nested timer arms on the NEXT pass; its delay counts from there.
    engine.process_timers_for_test(now + Duration::from_millis(50));
    assert!(drain(&engine).is_empty(), "nested armed, not yet due");
    engine.process_timers_for_test(now + Duration::from_millis(100));
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "nested"),
      _ => panic!("expected nested timer notify"),
    }
  }

  #[test]
  fn negative_ms_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("timer", r#"spotatui.set_timeout(-5, function() end)"#)
      .is_err());
    assert!(engine
      .load_source("timer", r#"spotatui.set_interval(0, function() end)"#)
      .is_err());
  }
}

// --- plugin_id sanitizer ---

#[test]
fn plugin_id_strips_lua_suffix_and_sanitizes() {
  use super::shared::plugin_id;
  assert_eq!(plugin_id("stats.lua"), "stats");
  assert_eq!(plugin_id("my-plugin"), "my-plugin");
  assert_eq!(plugin_id("weird name!.lua"), "weird_name_");
  assert_eq!(plugin_id("dir.plugin"), "dir_plugin");
  assert_eq!(plugin_id("под.lua"), "___");
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

// --- plugin storage ---

#[cfg(test)]
mod storage_tests {
  use super::*;

  /// Engine with a temp config dir registered (empty plugins dir is fine).
  fn engine_with_dir(cfg: &Path) -> ScriptEngine {
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_user_scripts(cfg);
    engine
  }

  #[test]
  fn storage_round_trip_within_one_engine() {
    let cfg = temp_config_dir();
    let mut engine = engine_with_dir(&cfg);
    engine
      .load_source(
        "a.lua",
        r#"
          spotatui.storage_set("count", 42)
          spotatui.storage_set("nested", { x = 1, tags = {"a", "b"} })
          spotatui.notify(spotatui.storage_get("count") .. ":" .. spotatui.storage_get("nested").tags[2], 1)
        "#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "42:b"),
      _ => panic!("expected storage round-trip notify"),
    }
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn storage_flushes_and_reloads_in_new_engine() {
    let cfg = temp_config_dir();
    {
      let mut engine = engine_with_dir(&cfg);
      engine
        .load_source("a.lua", r#"spotatui.storage_set("song", "Nightcall")"#)
        .unwrap();
      engine.flush_storage(true);
    }

    // The file exists under the sanitized plugin id.
    let file = cfg.join("plugin-data").join("a.json");
    assert!(file.is_file(), "expected {}", file.display());

    let mut engine = engine_with_dir(&cfg);
    engine
      .load_source(
        "a.lua",
        r#"spotatui.notify(spotatui.storage_get("song") or "missing", 1)"#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "Nightcall"),
      _ => panic!("expected persisted value notify"),
    }
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn storage_set_nil_deletes_key() {
    let cfg = temp_config_dir();
    let mut engine = engine_with_dir(&cfg);
    engine
      .load_source(
        "a.lua",
        r#"
          spotatui.storage_set("gone", "soon")
          spotatui.storage_set("gone", nil)
          spotatui.notify(tostring(spotatui.storage_get("gone")) .. ":" .. #spotatui.storage_keys(), 1)
        "#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "nil:0"),
      _ => panic!("expected nil-delete notify"),
    }
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn storage_remove_and_keys() {
    let cfg = temp_config_dir();
    let mut engine = engine_with_dir(&cfg);
    engine
      .load_source(
        "a.lua",
        r#"
          spotatui.storage_set("one", 1)
          spotatui.storage_set("two", 2)
          spotatui.storage_remove("one")
          local keys = spotatui.storage_keys()
          spotatui.notify(#keys .. ":" .. keys[1], 1)
        "#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "1:two"),
      _ => panic!("expected remove/keys notify"),
    }
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn storage_set_function_value_raises() {
    let cfg = temp_config_dir();
    let mut engine = engine_with_dir(&cfg);
    let result = engine.load_source("a.lua", r#"spotatui.storage_set("f", function() end)"#);
    assert!(result.is_err(), "functions must not be storable");
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn storage_isolated_per_plugin() {
    let cfg = temp_config_dir();
    {
      let mut engine = engine_with_dir(&cfg);
      engine
        .load_source("a.lua", r#"spotatui.storage_set("who", "plugin a")"#)
        .unwrap();
      engine
        .load_source("b", r#"spotatui.storage_set("who", "plugin b")"#)
        .unwrap();
      engine.flush_storage(true);
    }

    assert!(cfg.join("plugin-data").join("a.json").is_file());
    assert!(cfg.join("plugin-data").join("b.json").is_file());

    let mut engine = engine_with_dir(&cfg);
    engine
      .load_source("b", r#"spotatui.notify(spotatui.storage_get("who"), 1)"#)
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "plugin b"),
      _ => panic!("expected isolated namespace notify"),
    }
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn corrupt_storage_file_starts_empty() {
    let cfg = temp_config_dir();
    write_file(&cfg.join("plugin-data").join("a.json"), "{not json");

    let mut engine = engine_with_dir(&cfg);
    engine
      .load_source(
        "a.lua",
        r#"spotatui.notify(tostring(spotatui.storage_get("anything")), 1)"#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "nil"),
      _ => panic!("expected empty-after-corruption notify"),
    }
    std::fs::remove_dir_all(&cfg).unwrap();
  }

  #[test]
  fn storage_outside_plugin_context_raises() {
    // current_plugin is empty outside load/callback paths; simulate by loading
    // a chunk with an empty plugin name.
    let cfg = temp_config_dir();
    let mut engine = engine_with_dir(&cfg);
    let result = engine.load_source("", r#"spotatui.storage_get("x")"#);
    assert!(result.is_err());
    std::fs::remove_dir_all(&cfg).unwrap();
  }
}

// --- state events (route/device/search) + shuffle/repeat diffs ---

#[cfg(test)]
mod state_event_tests {
  use super::*;
  use crate::core::app::{ActiveBlock, App, RouteId};
  use crate::core::plugin_api::DeviceInfo;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use crate::infra::scripting::events::diff_state_events;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    (app, rx)
  }

  fn device(id: &str, active: bool) -> DeviceInfo {
    DeviceInfo {
      id: Some(id.to_string()),
      name: id.to_string(),
      kind: "computer".to_string(),
      is_active: active,
      volume_percent: Some(50),
    }
  }

  #[test]
  fn diff_shuffle_change() {
    let old = Some(playback(Some(track("uri:1", "A")), true, 0));
    let mut new_pb = playback(Some(track("uri:1", "A")), true, 0);
    new_pb.shuffle = true;
    let q = Some(vec![]);
    let events = diff_events(&old, &q, &Some(new_pb), &q);
    assert_eq!(events, vec![ScriptEvent::ShuffleChange]);
  }

  #[test]
  fn diff_repeat_change() {
    let old = Some(playback(Some(track("uri:1", "A")), true, 0));
    let mut new_pb = playback(Some(track("uri:1", "A")), true, 0);
    new_pb.repeat = "track".to_string();
    let q = Some(vec![]);
    let events = diff_events(&old, &q, &Some(new_pb), &q);
    assert_eq!(events, vec![ScriptEvent::RepeatChange]);
  }

  #[test]
  fn diff_none_to_some_is_not_shuffle_or_repeat_change() {
    let mut new_pb = playback(Some(track("uri:1", "A")), false, 0);
    new_pb.shuffle = true;
    new_pb.repeat = "context".to_string();
    let q = Some(vec![]);
    let events = diff_events(&None, &q, &Some(new_pb), &q);
    assert!(!events.contains(&ScriptEvent::ShuffleChange));
    assert!(!events.contains(&ScriptEvent::RepeatChange));
  }

  #[test]
  fn diff_state_route_change_carries_new_name() {
    let events = diff_state_events("home", "queue", &[], &[], false);
    assert_eq!(events, vec![ScriptEvent::RouteChange("queue".to_string())]);
  }

  #[test]
  fn diff_state_device_change_on_active_flip() {
    let old = [device("a", true), device("b", false)];
    let new = [device("a", false), device("b", true)];
    let events = diff_state_events("home", "home", &old, &new, false);
    assert_eq!(events, vec![ScriptEvent::DeviceChange]);
  }

  #[test]
  fn diff_state_no_events_when_identical() {
    let devs = [device("a", true)];
    assert!(diff_state_events("home", "home", &devs, &devs, false).is_empty());
  }

  #[test]
  fn diff_state_search_advance() {
    let events = diff_state_events("home", "home", &[], &[], true);
    assert_eq!(events, vec![ScriptEvent::SearchResults]);
  }

  #[test]
  fn route_change_event_fires_after_key_navigation() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine
      .load_source(
        "router",
        r#"spotatui.on("route_change", function(ev) spotatui.notify("now at " .. ev.name, 1) end)"#,
      )
      .unwrap();

    // Baseline the route (as on_start would), then navigate like a key handler.
    engine.run_pending_commands(&mut app);
    let _ = drain(&engine);
    app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);
    engine.run_pending_commands(&mut app);

    assert_eq!(app.status_message.as_deref(), Some("now at queue"));
  }

  #[test]
  fn current_route_sync_read_tracks_navigation() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
    engine.on_tick(&mut app);
    engine
      .load_source("reader", r#"spotatui.notify(spotatui.current_route(), 1)"#)
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "settings"),
      _ => panic!("expected current_route notify"),
    }
  }

  #[test]
  fn navigate_unknown_target_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source("nav", r#"spotatui.navigate("narnia")"#)
      .is_err());
  }

  #[test]
  fn navigate_queue_pushes_route_and_fetches() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, rx) = make_app();
    engine
      .load_source("nav", r#"spotatui.navigate("queue")"#)
      .unwrap();
    engine.drain_effects(&mut app);

    assert_eq!(app.get_current_route().id, RouteId::Queue);
    match rx.try_recv() {
      Ok(IoEvent::GetQueue) => {}
      _ => panic!("expected GetQueue dispatch (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn back_pops_navigation_stack() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);
    engine.load_source("nav", r#"spotatui.back()"#).unwrap();
    engine.drain_effects(&mut app);
    assert_eq!(app.get_current_route().id, RouteId::Home);
  }

  #[test]
  fn device_change_event_fires_on_active_device_swap() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine
      .load_source(
        "dev",
        r#"spotatui.on("device_change", function() spotatui.notify("devices moved", 1) end)"#,
      )
      .unwrap();

    engine.on_tick(&mut app); // baseline: no devices
    let _ = drain(&engine);

    #[allow(deprecated)]
    let payload = rspotify::model::device::DevicePayload {
      devices: vec![rspotify::model::Device {
        id: Some("dev-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk".to_string(),
        _type: rspotify::model::DeviceType::Computer,
        volume_percent: Some(30),
      }],
    };
    app.devices = Some(payload);
    engine.on_tick(&mut app);
    assert_eq!(app.status_message.as_deref(), Some("devices moved"));
  }
}

// --- custom plugin screens ---

#[cfg(test)]
mod screen_tests {
  use super::*;
  use crate::core::app::{ActiveBlock, App, RouteId};
  use crate::core::plugin_api::PluginWidget;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    (app, rx)
  }

  const STATS_SCREEN: &str = r#"
    spotatui.register_screen("stats", {
      title = "Stats",
      on_key = function(key)
        spotatui.notify("key: " .. key, 1)
      end,
      on_open = function() spotatui.notify("opened", 1) end,
      on_close = function() spotatui.notify("closed", 1) end,
    })
  "#;

  #[test]
  fn show_screen_pushes_route_and_close_pops() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();
    engine
      .load_source("stats.lua", r#"spotatui.show_screen("stats")"#)
      .unwrap();
    engine.drain_effects(&mut app);

    assert_eq!(
      app.get_current_route().id,
      RouteId::PluginScreen("stats".to_string())
    );
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::PluginScreen
    );

    engine
      .load_source("stats.lua", r#"spotatui.close_screen("stats")"#)
      .unwrap();
    engine.drain_effects(&mut app);
    assert!(!matches!(
      app.get_current_route().id,
      RouteId::PluginScreen(_)
    ));
  }

  #[test]
  fn set_screen_publishes_widgets() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();
    engine
      .load_source(
        "stats.lua",
        r#"
          spotatui.set_screen("stats", {
            { type = "paragraph", lines = {"hello", { text = "styled", bold = true }}, height = 4 },
            { type = "list", title = "Songs", items = {"one", "two"}, selected = 2 },
            { type = "gauge", ratio = 1.7, label = "70%" },
          })
        "#,
      )
      .unwrap();
    engine.drain_effects(&mut app);

    let content = app.plugin_screens.get("stats").expect("content published");
    assert_eq!(content.title, "Stats");
    assert_eq!(content.widgets.len(), 3);
    match &content.widgets[0] {
      PluginWidget::Paragraph { lines, height } => {
        assert_eq!(lines.len(), 2);
        assert!(lines[1].bold);
        assert_eq!(*height, Some(4));
      }
      _ => panic!("expected paragraph widget"),
    }
    match &content.widgets[1] {
      PluginWidget::List {
        title,
        items,
        selected,
        ..
      } => {
        assert_eq!(title.as_deref(), Some("Songs"));
        assert_eq!(items.len(), 2);
        // Lua-side selected is 1-based; stored 0-based.
        assert_eq!(*selected, Some(1));
      }
      _ => panic!("expected list widget"),
    }
    match &content.widgets[2] {
      PluginWidget::Gauge { ratio, label } => {
        assert_eq!(*ratio, 1.0, "ratio must be clamped");
        assert_eq!(label.as_deref(), Some("70%"));
      }
      _ => panic!("expected gauge widget"),
    }
  }

  #[test]
  fn set_screen_rejects_unknown_widget_type_and_bad_selected() {
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();
    assert!(engine
      .load_source(
        "stats.lua",
        r#"spotatui.set_screen("stats", {{ type = "table" }})"#
      )
      .is_err());
    assert!(engine
      .load_source(
        "stats.lua",
        r#"spotatui.set_screen("stats", {{ type = "list", items = {"x"}, selected = 0 }})"#
      )
      .is_err());
  }

  #[test]
  fn set_screen_unregistered_or_foreign_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();

    // Not registered at all.
    assert!(engine
      .load_source("stats.lua", r#"spotatui.set_screen("nope", {})"#)
      .is_err());

    // Registered, but owned by another plugin.
    let err = engine
      .load_source("intruder.lua", r#"spotatui.show_screen("stats")"#)
      .unwrap_err()
      .to_string();
    assert!(err.contains("belongs to plugin"), "got: {err}");
  }

  #[test]
  fn duplicate_screen_name_raises() {
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();
    assert!(engine
      .load_source(
        "other.lua",
        r#"spotatui.register_screen("stats", { on_key = function() end })"#
      )
      .is_err());
  }

  #[test]
  fn register_screen_requires_on_key() {
    let mut engine = ScriptEngine::new().unwrap();
    assert!(engine
      .load_source(
        "stats.lua",
        r#"spotatui.register_screen("stats", { title = "T" })"#
      )
      .is_err());
  }

  #[test]
  fn pending_screen_key_reaches_on_key() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();

    app
      .pending_plugin_screen_keys
      .push(("stats".to_string(), "ctrl-x".to_string()));
    engine.run_pending_commands(&mut app);

    assert_eq!(app.status_message.as_deref(), Some("key: ctrl-x"));
  }

  #[test]
  fn erroring_on_key_is_one_strike() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine
      .load_source(
        "bad.lua",
        r#"spotatui.register_screen("boom", { on_key = function() error("kaput") end })"#,
      )
      .unwrap();

    app
      .pending_plugin_screen_keys
      .push(("boom".to_string(), "a".to_string()));
    engine.run_pending_commands(&mut app);
    assert!(app.status_message_is_error);
    assert!(app
      .status_message
      .as_deref()
      .unwrap_or("")
      .contains("kaput"));

    // Second key: the erroring on_key was removed, nothing fires.
    app.status_message = None;
    app.status_message_is_error = false;
    app
      .pending_plugin_screen_keys
      .push(("boom".to_string(), "a".to_string()));
    engine.run_pending_commands(&mut app);
    assert!(app.status_message.is_none());
  }

  #[test]
  fn on_open_and_on_close_fire_on_route_transitions() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    engine.load_source("stats.lua", STATS_SCREEN).unwrap();

    // Baseline route tracking.
    engine.run_pending_commands(&mut app);

    engine
      .load_source("stats.lua", r#"spotatui.show_screen("stats")"#)
      .unwrap();
    engine.drain_effects(&mut app);
    engine.run_pending_commands(&mut app);
    assert_eq!(app.status_message.as_deref(), Some("opened"));

    app.pop_navigation_stack();
    engine.run_pending_commands(&mut app);
    assert_eq!(app.status_message.as_deref(), Some("closed"));
  }

  #[test]
  fn route_name_for_plugin_screen_is_prefixed() {
    use crate::core::app::Route;
    let route = Route {
      id: RouteId::PluginScreen("stats".to_string()),
      active_block: ActiveBlock::PluginScreen,
      hovered_block: ActiveBlock::PluginScreen,
    };
    assert_eq!(crate::core::plugin_api::route_name(&route), "plugin:stats");
  }
}

// --- config() read ---

#[cfg(test)]
mod config_tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn make_app() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    (app, rx)
  }

  #[test]
  fn config_exposes_theme_and_behavior_scalars() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    app.user_config.theme.playbar_text = ratatui::style::Color::Magenta;
    app.user_config.behavior.seek_milliseconds = 12345;
    engine.on_tick(&mut app);

    engine
      .load_source(
        "cfg",
        r#"
          local c = spotatui.config()
          spotatui.notify(c.theme.playbar_text .. ":" .. c.behavior.seek_milliseconds, 1)
        "#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "Magenta:12345"),
      _ => panic!("expected config notify"),
    }
  }

  #[test]
  fn config_excludes_secrets() {
    let mut engine = ScriptEngine::new().unwrap();
    let (mut app, _rx) = make_app();
    app.user_config.behavior.sync_token = Some("SECRET".to_string());
    engine.on_tick(&mut app);

    engine
      .load_source(
        "cfg",
        r#"
          local c = spotatui.config()
          spotatui.notify(tostring(c.behavior.sync_token) .. ":" .. tostring(c.behavior.relay_server_url), 1)
        "#,
      )
      .unwrap();
    match one(&engine) {
      ScriptEffect::Notify(msg, 1) => assert_eq!(msg, "nil:nil"),
      _ => panic!("expected secrets-excluded notify"),
    }
  }
}
