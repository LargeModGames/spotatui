use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::core::app::PluginDataKind;
use crate::core::plugin_api::{self, PluginPopup, PopupLine};
use crate::core::user_config::parse_theme_item;
use crate::infra::network::IoEvent;

use super::effects::ScriptEffect;
use super::events::VALID_EVENT_NAMES;
use super::shared::{
  DataRequest, HttpResponseData, HttpResult, NewTimer, ScriptShared, COMMANDS_KEY,
  DATA_CALLBACKS_KEY, HANDLERS_KEY, HTTP_CALLBACKS_KEY, SCREENS_KEY, TIMER_CALLBACKS_KEY,
};

/// Build the `spotatui` global table and its functions.
pub(super) fn install_api(
  lua: &Lua,
  shared: &Rc<ScriptShared>,
  http_tx: UnboundedSender<HttpResult>,
  http_client: reqwest::Client,
  rt_handle: Option<tokio::runtime::Handle>,
) -> mlua::Result<()> {
  let tbl = lua.create_table()?;

  tbl.set("api_version", plugin_api::API_VERSION)?;

  // spotatui.require_api(n): assert this build is new enough for the plugin.
  {
    let require_api = lua.create_function(move |_, n: i64| {
      if n < 1 {
        return Err(mlua::Error::RuntimeError(format!(
          "spotatui.require_api: version must be a positive integer, got {n}"
        )));
      }
      let n = n as u32;
      if n > plugin_api::API_VERSION {
        return Err(mlua::Error::RuntimeError(format!(
          "requires spotatui scripting API v{n} (this build provides v{}); update spotatui to use this plugin",
          plugin_api::API_VERSION
        )));
      }
      Ok(())
    })?;
    tbl.set("require_api", require_api)?;
  }

  // spotatui.on(event, fn)
  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let on = lua.create_function(move |_, (event, callback): (String, mlua::Function)| {
      if !VALID_EVENT_NAMES.contains(&event.as_str()) {
        return Err(mlua::Error::RuntimeError(format!(
          "spotatui.on: unknown event '{event}'; valid events: {}",
          VALID_EVENT_NAMES.join(", ")
        )));
      }
      let handlers: mlua::Table = lua_inner.named_registry_value(HANDLERS_KEY)?;
      let list: mlua::Table = match handlers.get::<Option<mlua::Table>>(event.clone())? {
        Some(t) => t,
        None => {
          let t = lua_inner.create_table()?;
          handlers.set(event.clone(), t.clone())?;
          t
        }
      };
      let entry = lua_inner.create_table()?;
      entry.set("plugin", shared.current_plugin.borrow().clone())?;
      entry.set("callback", callback)?;
      list.push(entry)?;
      Ok(())
    })?;
    tbl.set("on", on)?;
  }

  // Reads: spotatui.playback() / current_track() / devices()
  {
    let shared_pb = shared.clone();
    let playback = lua.create_function(move |lua, ()| {
      let pb = shared_pb.playback.borrow().clone();
      match pb {
        Some(state) => lua.to_value(&state),
        None => Ok(Value::Nil),
      }
    })?;
    tbl.set("playback", playback)?;

    let shared_ct = shared.clone();
    let current_track = lua.create_function(move |lua, ()| {
      let pb = shared_ct.playback.borrow().clone();
      match pb.and_then(|s| s.track) {
        Some(track) => lua.to_value(&track),
        None => Ok(Value::Nil),
      }
    })?;
    tbl.set("current_track", current_track)?;

    let shared_dev = shared.clone();
    let devices = lua.create_function(move |lua, ()| {
      let devices = shared_dev.devices.borrow().clone();
      lua.to_value(&devices)
    })?;
    tbl.set("devices", devices)?;
  }

  // Actions: queue effects.
  install_action(lua, &tbl, shared, "play", || ScriptEffect::Play)?;
  install_action(lua, &tbl, shared, "pause", || ScriptEffect::Pause)?;
  install_action(lua, &tbl, shared, "next", || ScriptEffect::Next)?;
  install_action(lua, &tbl, shared, "previous", || ScriptEffect::Previous)?;

  {
    let shared = shared.clone();
    let seek = lua.create_function(move |_, ms: u32| {
      shared.effects.borrow_mut().push(ScriptEffect::Seek(ms));
      Ok(())
    })?;
    tbl.set("seek", seek)?;
  }

  {
    let shared = shared.clone();
    let set_volume = lua.create_function(move |_, pct: i64| {
      let clamped = pct.clamp(0, 100) as u8;
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::SetVolume(clamped));
      Ok(())
    })?;
    tbl.set("set_volume", set_volume)?;
  }

  {
    let shared = shared.clone();
    let shuffle = lua.create_function(move |_, on: bool| {
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::SetShuffle(on));
      Ok(())
    })?;
    tbl.set("shuffle", shuffle)?;
  }

  {
    let shared = shared.clone();
    let search = lua.create_function(move |_, query: String| {
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::Search(query));
      Ok(())
    })?;
    tbl.set("search", search)?;
  }

  {
    let shared = shared.clone();
    let notify = lua.create_function(move |_, (msg, ttl): (String, Option<u64>)| {
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::Notify(msg, ttl.unwrap_or(4)));
      Ok(())
    })?;
    tbl.set("notify", notify)?;
  }

  {
    let log = lua.create_function(move |_, msg: String| {
      log::info!("[lua] {msg}");
      Ok(())
    })?;
    tbl.set("log", log)?;
  }

  {
    let json_decode = lua.create_function(move |lua, json: String| {
      let value: serde_json::Value = serde_json::from_str(&json).map_err(mlua::Error::external)?;
      lua.to_value(&value)
    })?;
    tbl.set("json_decode", json_decode)?;

    let json_encode = lua.create_function(move |lua, value: Value| {
      let value: serde_json::Value = lua.from_value(value)?;
      serde_json::to_string(&value).map_err(mlua::Error::external)
    })?;
    tbl.set("json_encode", json_encode)?;
  }

  // Async HTTP: request tasks send results back to the engine, which owns the Lua state.
  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let tx = http_tx.clone();
    let client = http_client.clone();
    let handle = rt_handle.clone();
    let http_get = lua.create_function(move |_, (url, callback): (String, mlua::Function)| {
      validate_http_url("spotatui.http_get", &url)?;
      let handle = handle.clone().ok_or_else(|| {
        mlua::Error::RuntimeError("spotatui.http_get: no tokio runtime available".to_string())
      })?;
      let token = register_http_callback(&lua_inner, &shared, callback)?;
      let client = client.clone();
      let tx = tx.clone();
      handle.spawn(async move {
        let result = run_http_get(client, url).await;
        let _ = tx.send((token, result));
      });
      Ok(())
    })?;
    tbl.set("http_get", http_get)?;
  }

  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let tx = http_tx.clone();
    let client = http_client.clone();
    let handle = rt_handle.clone();
    let http_post = lua.create_function(
      move |_,
            (url, body, headers, callback): (
        String,
        String,
        Option<mlua::Table>,
        mlua::Function,
      )| {
        validate_http_url("spotatui.http_post", &url)?;
        let handle = handle.clone().ok_or_else(|| {
          mlua::Error::RuntimeError("spotatui.http_post: no tokio runtime available".to_string())
        })?;
        let headers = collect_headers(headers)?;
        let token = register_http_callback(&lua_inner, &shared, callback)?;
        let client = client.clone();
        let tx = tx.clone();
        handle.spawn(async move {
          let result = run_http_post(client, url, body, headers).await;
          let _ = tx.send((token, result));
        });
        Ok(())
      },
    )?;
    tbl.set("http_post", http_post)?;
  }

  // spotatui.register_command(name, fn)
  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let register_command =
      lua.create_function(move |_, (name, callback): (String, mlua::Function)| {
        if name.is_empty() || name.contains(char::is_whitespace) {
          return Err(mlua::Error::RuntimeError(
            "spotatui.register_command: name must be a non-empty string without whitespace"
              .to_string(),
          ));
        }
        let commands: mlua::Table = lua_inner.named_registry_value(COMMANDS_KEY)?;
        if commands.get::<Option<mlua::Table>>(name.clone())?.is_some() {
          return Err(mlua::Error::RuntimeError(format!(
            "spotatui.register_command: command '{name}' is already registered"
          )));
        }
        let entry = lua_inner.create_table()?;
        entry.set("plugin", shared.current_plugin.borrow().clone())?;
        entry.set("callback", callback)?;
        commands.set(name, entry)?;
        Ok(())
      })?;
    tbl.set("register_command", register_command)?;
  }

  // spotatui.set_playbar(text_or_nil)
  {
    let shared = shared.clone();
    let set_playbar = lua.create_function(move |_, text: Option<String>| {
      let plugin = shared.current_plugin.borrow().clone();
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::SetPlaybarSegment { plugin, text });
      Ok(())
    })?;
    tbl.set("set_playbar", set_playbar)?;
  }

  // spotatui.popup(title, lines)
  {
    let shared = shared.clone();
    let popup = lua.create_function(move |_, (title, lines_val): (String, mlua::Value)| {
      let lines = parse_popup_lines(lines_val)?;
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::ShowPopup(PluginPopup { title, lines }));
      Ok(())
    })?;
    tbl.set("popup", popup)?;
  }

  // spotatui.set_theme(tbl)
  {
    let shared = shared.clone();
    let set_theme = lua.create_function(move |_, tbl: mlua::Table| {
      let mut pairs: Vec<(String, ratatui::style::Color)> = Vec::new();
      for pair in tbl.pairs::<String, String>() {
        let (field, color_str) = pair?;
        // Validate field name
        const VALID_FIELDS: &[&str] = &[
          "active",
          "banner",
          "error_border",
          "error_text",
          "hint",
          "hovered",
          "inactive",
          "playbar_background",
          "playbar_progress",
          "playbar_progress_text",
          "playbar_text",
          "selected",
          "text",
          "background",
          "header",
          "highlighted_lyrics",
          "analysis_bar",
          "analysis_bar_text",
        ];
        if !VALID_FIELDS.contains(&field.as_str()) {
          return Err(mlua::Error::RuntimeError(format!(
            "spotatui.set_theme: unknown theme field '{field}'"
          )));
        }
        let color = parse_theme_item(&color_str).map_err(|e| {
          mlua::Error::RuntimeError(format!(
            "spotatui.set_theme: invalid color for field '{field}': {e}"
          ))
        })?;
        pairs.push((field, color));
      }
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::SetTheme(pairs));
      Ok(())
    })?;
    tbl.set("set_theme", set_theme)?;
  }

  // Async data reads: spotatui.get_X(cb) with cb(data, err), like http.
  install_data_read(
    lua,
    &tbl,
    shared,
    "get_playlists",
    PluginDataKind::Playlists,
  )?;
  install_data_read(lua, &tbl, shared, "get_queue", PluginDataKind::Queue)?;
  install_data_read(
    lua,
    &tbl,
    shared,
    "get_saved_tracks",
    PluginDataKind::SavedTracks,
  )?;
  install_data_read(
    lua,
    &tbl,
    shared,
    "get_saved_albums",
    PluginDataKind::SavedAlbums,
  )?;
  install_data_read(
    lua,
    &tbl,
    shared,
    "get_saved_shows",
    PluginDataKind::SavedShows,
  )?;
  install_data_read(
    lua,
    &tbl,
    shared,
    "get_recently_played",
    PluginDataKind::RecentlyPlayed,
  )?;
  install_data_read(lua, &tbl, shared, "get_devices", PluginDataKind::Devices)?;
  install_data_read(lua, &tbl, shared, "get_lyrics", PluginDataKind::Lyrics)?;

  // spotatui.get_search_results(query, cb): the only data read with an argument.
  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let get_search_results =
      lua.create_function(move |_, (query, callback): (String, mlua::Function)| {
        if query.is_empty() {
          return Err(mlua::Error::RuntimeError(
            "spotatui.get_search_results: query must be a non-empty string".to_string(),
          ));
        }
        let token = register_callback(&lua_inner, &shared, DATA_CALLBACKS_KEY, callback)?;
        shared.data_requests.borrow_mut().push(DataRequest {
          token,
          kind: PluginDataKind::Search,
          arg: Some(query),
        });
        Ok(())
      })?;
    tbl.set("get_search_results", get_search_results)?;
  }

  // Cached synchronous reads, refreshed by the engine when the matching data
  // generation advances.
  {
    let shared_pl = shared.clone();
    let playlists =
      lua.create_function(move |lua, ()| lua.to_value(&*shared_pl.playlists_cache.borrow()))?;
    tbl.set("playlists", playlists)?;

    let shared_q = shared.clone();
    let queue =
      lua.create_function(move |lua, ()| lua.to_value(&*shared_q.queue_cache.borrow()))?;
    tbl.set("queue", queue)?;

    let shared_sr = shared.clone();
    let search_results = lua
      .create_function(move |lua, ()| lua.to_value(&*shared_sr.search_results_cache.borrow()))?;
    tbl.set("search_results", search_results)?;
  }

  // Transport / library actions routed through whitelisted IoEvents.
  {
    let shared = shared.clone();
    let set_repeat = lua.create_function(move |_, mode: String| {
      let state = match mode.as_str() {
        "off" => rspotify::model::RepeatState::Off,
        "track" => rspotify::model::RepeatState::Track,
        "context" => rspotify::model::RepeatState::Context,
        other => {
          return Err(mlua::Error::RuntimeError(format!(
            "spotatui.set_repeat: expected \"off\", \"track\" or \"context\", got '{other}'"
          )));
        }
      };
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::Dispatch(IoEvent::Repeat(state)));
      Ok(())
    })?;
    tbl.set("set_repeat", set_repeat)?;
  }

  install_action(lua, &tbl, shared, "cycle_repeat", || {
    ScriptEffect::CycleRepeat
  })?;

  install_string_action(lua, &tbl, shared, "transfer_playback", |device_id| {
    // `false`: a plugin-initiated transfer must not overwrite the user's saved
    // device preference (only the interactive device picker persists).
    ScriptEffect::Dispatch(IoEvent::TransferPlaybackToDevice(device_id, false))
  })?;

  // spotatui.play_uri(uri): tracks/episodes play as a uri list, containers as context.
  {
    let shared = shared.clone();
    let play_uri = lua.create_function(move |_, uri: String| {
      let effect = if uri.starts_with("spotify:track:") || uri.starts_with("spotify:episode:") {
        ScriptEffect::Dispatch(IoEvent::StartPlayback(None, Some(vec![uri]), None))
      } else if is_context_uri(&uri) {
        ScriptEffect::Dispatch(IoEvent::StartPlayback(Some(uri), None, None))
      } else {
        return Err(mlua::Error::RuntimeError(format!(
          "spotatui.play_uri: expected a spotify:track/episode/album/playlist/artist/show uri, got '{uri}'"
        )));
      };
      shared.effects.borrow_mut().push(effect);
      Ok(())
    })?;
    tbl.set("play_uri", play_uri)?;
  }

  // spotatui.play_context(uri, offset?): play a container, optionally from an
  // 0-based track offset.
  {
    let shared = shared.clone();
    let play_context = lua.create_function(move |_, (uri, offset): (String, Option<usize>)| {
      if !is_context_uri(&uri) {
        return Err(mlua::Error::RuntimeError(format!(
          "spotatui.play_context: expected a spotify:album/playlist/artist/show uri, got '{uri}'"
        )));
      }
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::Dispatch(IoEvent::StartPlayback(
          Some(uri),
          None,
          offset,
        )));
      Ok(())
    })?;
    tbl.set("play_context", play_context)?;
  }

  install_string_action(lua, &tbl, shared, "add_to_queue", |uri| {
    ScriptEffect::Dispatch(IoEvent::AddItemToQueue(uri))
  })?;

  // spotatui.create_playlist(name, uris?)
  {
    let shared = shared.clone();
    let create_playlist =
      lua.create_function(move |_, (name, uris): (String, Option<mlua::Table>)| {
        if name.trim().is_empty() {
          return Err(mlua::Error::RuntimeError(
            "spotatui.create_playlist: name must be a non-empty string".to_string(),
          ));
        }
        let mut track_ids = Vec::new();
        if let Some(uris) = uris {
          for uri in uris.sequence_values::<String>() {
            track_ids.push(uri?);
          }
        }
        shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::Dispatch(IoEvent::CreateNewPlaylist(
            name, track_ids,
          )));
        Ok(())
      })?;
    tbl.set("create_playlist", create_playlist)?;
  }

  // spotatui.playlist_add_track(playlist, track)
  {
    let shared = shared.clone();
    let playlist_add_track =
      lua.create_function(move |_, (playlist, track): (String, String)| {
        require_nonempty("spotatui.playlist_add_track", "playlist", &playlist)?;
        require_nonempty("spotatui.playlist_add_track", "track", &track)?;
        shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::Dispatch(IoEvent::AddTrackToPlaylist(
            playlist, track,
          )));
        Ok(())
      })?;
    tbl.set("playlist_add_track", playlist_add_track)?;
  }

  // spotatui.playlist_remove_track(playlist, track, position) -- 0-based, required.
  {
    let shared = shared.clone();
    let playlist_remove_track =
      lua.create_function(move |_, (playlist, track, position): (String, String, i64)| {
        require_nonempty("spotatui.playlist_remove_track", "playlist", &playlist)?;
        require_nonempty("spotatui.playlist_remove_track", "track", &track)?;
        let position = usize::try_from(position).map_err(|_| {
          mlua::Error::RuntimeError(format!(
            "spotatui.playlist_remove_track: position must be a non-negative (0-based) integer, got {position}"
          ))
        })?;
        shared.effects.borrow_mut().push(ScriptEffect::Dispatch(
          IoEvent::RemoveTrackFromPlaylistAtPosition(playlist, track, position),
        ));
        Ok(())
      })?;
    tbl.set("playlist_remove_track", playlist_remove_track)?;
  }

  install_string_action(lua, &tbl, shared, "follow_playlist", |playlist| {
    // The network handler ignores the owner-id parameter; "unknown" mirrors the
    // fallback the built-in follow flow uses.
    ScriptEffect::Dispatch(IoEvent::UserFollowPlaylist(
      "unknown".to_string(),
      playlist,
      None,
    ))
  })?;
  install_string_action(
    lua,
    &tbl,
    shared,
    "unfollow_playlist",
    ScriptEffect::UnfollowPlaylist,
  )?;
  install_string_action(lua, &tbl, shared, "toggle_save_track", |uri| {
    ScriptEffect::Dispatch(IoEvent::ToggleSaveTrack(uri))
  })?;
  install_string_action(lua, &tbl, shared, "save_album", |id| {
    ScriptEffect::Dispatch(IoEvent::CurrentUserSavedAlbumAdd(id))
  })?;
  install_string_action(lua, &tbl, shared, "unsave_album", |id| {
    ScriptEffect::Dispatch(IoEvent::CurrentUserSavedAlbumDelete(id))
  })?;
  install_string_action(lua, &tbl, shared, "save_show", |id| {
    ScriptEffect::Dispatch(IoEvent::CurrentUserSavedShowAdd(id))
  })?;
  install_string_action(lua, &tbl, shared, "unsave_show", |id| {
    ScriptEffect::Dispatch(IoEvent::CurrentUserSavedShowDelete(id))
  })?;
  install_string_action(lua, &tbl, shared, "follow_artist", |id| {
    ScriptEffect::Dispatch(IoEvent::UserFollowArtists(vec![id]))
  })?;
  install_string_action(lua, &tbl, shared, "unfollow_artist", |id| {
    ScriptEffect::Dispatch(IoEvent::UserUnfollowArtists(vec![id]))
  })?;

  // Timers. Fired from the engine's tick pass, so the effective resolution is
  // the UI tick rate (behavior.tick_rate_milliseconds).
  install_timer(lua, &tbl, shared, "set_timeout", false)?;
  install_timer(lua, &tbl, shared, "set_interval", true)?;

  {
    let shared = shared.clone();
    let cancel_timer = lua.create_function(move |_, handle: i64| {
      if let Ok(token) = u64::try_from(handle) {
        shared.cancelled_timers.borrow_mut().push(token);
      }
      Ok(())
    })?;
    tbl.set("cancel_timer", cancel_timer)?;
  }

  // spotatui.config(): theme colors + safe behavior scalars (cached sync read).
  {
    let shared = shared.clone();
    let config =
      lua.create_function(move |lua, ()| lua.to_value(&*shared.config_cache.borrow()))?;
    tbl.set("config", config)?;
  }

  // Navigation.
  {
    let shared = shared.clone();
    let navigate = lua.create_function(move |_, target: String| {
      if !super::effects::NAV_TARGETS.contains(&target.as_str()) {
        return Err(mlua::Error::RuntimeError(format!(
          "spotatui.navigate: unknown target '{target}'; valid targets: {}",
          super::effects::NAV_TARGETS.join(", ")
        )));
      }
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::Navigate(target));
      Ok(())
    })?;
    tbl.set("navigate", navigate)?;
  }

  install_action(lua, &tbl, shared, "back", || ScriptEffect::Back)?;

  {
    let shared = shared.clone();
    let current_route =
      lua.create_function(move |_, ()| Ok(shared.current_route.borrow().clone()))?;
    tbl.set("current_route", current_route)?;
  }

  // Custom plugin screens (retained-mode).
  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let register_screen = lua.create_function(move |_, (name, spec): (String, mlua::Table)| {
      if name.is_empty() || name.contains(char::is_whitespace) {
        return Err(mlua::Error::RuntimeError(
          "spotatui.register_screen: name must be a non-empty string without whitespace"
            .to_string(),
        ));
      }
      let screens: mlua::Table = lua_inner.named_registry_value(SCREENS_KEY)?;
      if screens.get::<Option<mlua::Table>>(name.clone())?.is_some() {
        return Err(mlua::Error::RuntimeError(format!(
          "spotatui.register_screen: screen '{name}' is already registered"
        )));
      }
      let on_key: mlua::Function =
        spec
          .get::<Option<mlua::Function>>("on_key")?
          .ok_or_else(|| {
            mlua::Error::RuntimeError(
              "spotatui.register_screen: spec must have an 'on_key' function".to_string(),
            )
          })?;
      let entry = lua_inner.create_table()?;
      entry.set("plugin", shared.current_plugin.borrow().clone())?;
      entry.set(
        "title",
        spec.get::<Option<String>>("title")?.unwrap_or_default(),
      )?;
      entry.set("on_key", on_key)?;
      if let Some(on_open) = spec.get::<Option<mlua::Function>>("on_open")? {
        entry.set("on_open", on_open)?;
      }
      if let Some(on_close) = spec.get::<Option<mlua::Function>>("on_close")? {
        entry.set("on_close", on_close)?;
      }
      screens.set(name, entry)?;
      Ok(())
    })?;
    tbl.set("register_screen", register_screen)?;
  }

  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let set_screen = lua.create_function(move |_, (name, widgets): (String, mlua::Table)| {
      let title = verify_screen_owner(&lua_inner, &shared, "spotatui.set_screen", &name)?;
      let widgets = parse_screen_widgets(widgets)?;
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::SetScreenContent {
          name,
          content: plugin_api::PluginScreenContent { title, widgets },
        });
      Ok(())
    })?;
    tbl.set("set_screen", set_screen)?;
  }

  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let show_screen = lua.create_function(move |_, name: String| {
      verify_screen_owner(&lua_inner, &shared, "spotatui.show_screen", &name)?;
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::ShowScreen(name));
      Ok(())
    })?;
    tbl.set("show_screen", show_screen)?;
  }

  {
    let lua_inner = lua.clone();
    let shared = shared.clone();
    let close_screen = lua.create_function(move |_, name: String| {
      verify_screen_owner(&lua_inner, &shared, "spotatui.close_screen", &name)?;
      shared
        .effects
        .borrow_mut()
        .push(ScriptEffect::CloseScreen(name));
      Ok(())
    })?;
    tbl.set("close_screen", close_screen)?;
  }

  // Plugin-scoped persistent storage: a flat JSON object per plugin under
  // <config_dir>/plugin-data/<plugin_id>.json. Lazily loaded on first touch,
  // flushed by the engine (throttled on tick, forced on quit).
  {
    let shared_get = shared.clone();
    let storage_get = lua.create_function(move |lua, key: String| {
      let namespace = storage_namespace(&shared_get)?;
      ensure_storage_loaded(&shared_get, &namespace);
      let storage = shared_get.storage.borrow();
      match storage.get(&namespace).and_then(|ns| ns.get(&key)) {
        Some(value) => lua.to_value(value),
        None => Ok(Value::Nil),
      }
    })?;
    tbl.set("storage_get", storage_get)?;

    let shared_set = shared.clone();
    let storage_set = lua.create_function(move |lua, (key, value): (String, Value)| {
      let namespace = storage_namespace(&shared_set)?;
      ensure_storage_loaded(&shared_set, &namespace);
      if value.is_nil() {
        let mut storage = shared_set.storage.borrow_mut();
        if let Some(ns) = storage.get_mut(&namespace) {
          ns.remove(&key);
        }
      } else {
        // serde round-trip rejects functions/userdata with a clear error.
        let json: serde_json::Value = lua.from_value(value).map_err(|e| {
          mlua::Error::RuntimeError(format!(
            "spotatui.storage_set: value must be JSON-serializable: {e}"
          ))
        })?;
        let mut storage = shared_set.storage.borrow_mut();
        storage
          .entry(namespace.clone())
          .or_default()
          .insert(key, json);
      }
      shared_set.storage_dirty.borrow_mut().insert(namespace);
      Ok(())
    })?;
    tbl.set("storage_set", storage_set)?;

    let shared_remove = shared.clone();
    let storage_remove = lua.create_function(move |_, key: String| {
      let namespace = storage_namespace(&shared_remove)?;
      ensure_storage_loaded(&shared_remove, &namespace);
      let removed = shared_remove
        .storage
        .borrow_mut()
        .get_mut(&namespace)
        .and_then(|ns| ns.remove(&key))
        .is_some();
      if removed {
        shared_remove.storage_dirty.borrow_mut().insert(namespace);
      }
      Ok(())
    })?;
    tbl.set("storage_remove", storage_remove)?;

    let shared_keys = shared.clone();
    let storage_keys = lua.create_function(move |_, ()| {
      let namespace = storage_namespace(&shared_keys)?;
      ensure_storage_loaded(&shared_keys, &namespace);
      let storage = shared_keys.storage.borrow();
      let keys: Vec<String> = storage
        .get(&namespace)
        .map(|ns| ns.keys().cloned().collect())
        .unwrap_or_default();
      Ok(keys)
    })?;
    tbl.set("storage_keys", storage_keys)?;
  }

  lua.globals().set("spotatui", tbl)?;
  Ok(())
}

/// The storage namespace for the plugin currently on the call stack.
fn storage_namespace(shared: &Rc<ScriptShared>) -> mlua::Result<String> {
  let namespace = super::shared::plugin_id(&shared.current_plugin.borrow());
  if namespace.is_empty() {
    return Err(mlua::Error::RuntimeError(
      "spotatui.storage_*: no plugin context (storage is only available from plugin code)"
        .to_string(),
    ));
  }
  Ok(namespace)
}

/// Load a namespace from disk on first touch. Missing or corrupt files map to
/// an empty namespace (corruption is logged, never fatal).
fn ensure_storage_loaded(shared: &Rc<ScriptShared>, namespace: &str) {
  if shared.storage.borrow().contains_key(namespace) {
    return;
  }
  let loaded = shared
    .storage_path(namespace)
    .and_then(|path| match std::fs::read_to_string(&path) {
      Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(serde_json::Value::Object(map)) => Some(map),
        Ok(_) => {
          log::warn!(
            "[lua] plugin storage {} is not a JSON object; starting empty",
            path.display()
          );
          None
        }
        Err(e) => {
          log::warn!(
            "[lua] plugin storage {} is corrupt ({e}); starting empty",
            path.display()
          );
          None
        }
      },
      Err(_) => None, // missing file: first run
    })
    .unwrap_or_default();
  shared
    .storage
    .borrow_mut()
    .insert(namespace.to_string(), loaded);
}

/// Install `spotatui.set_timeout(ms, fn)` / `set_interval(ms, fn)`, returning
/// an opaque handle for `cancel_timer`.
fn install_timer(
  lua: &Lua,
  tbl: &mlua::Table,
  shared: &Rc<ScriptShared>,
  name: &'static str,
  repeating: bool,
) -> mlua::Result<()> {
  let lua_inner = lua.clone();
  let shared = shared.clone();
  let f = lua.create_function(move |_, (ms, callback): (i64, mlua::Function)| {
    if ms < 0 {
      return Err(mlua::Error::RuntimeError(format!(
        "spotatui.{name}: milliseconds must be non-negative, got {ms}"
      )));
    }
    if repeating && ms == 0 {
      return Err(mlua::Error::RuntimeError(format!(
        "spotatui.{name}: interval must be at least 1 ms"
      )));
    }
    let token = register_callback(&lua_inner, &shared, TIMER_CALLBACKS_KEY, callback)?;
    let duration = std::time::Duration::from_millis(ms as u64);
    shared.new_timers.borrow_mut().push(NewTimer {
      token,
      delay: duration,
      interval: repeating.then_some(duration),
    });
    // register_callback guarantees the token fits in i64.
    Ok(token as i64)
  })?;
  tbl.set(name, f)?;
  Ok(())
}

/// True for Spotify container URIs playable as a context.
fn is_context_uri(uri: &str) -> bool {
  uri.starts_with("spotify:album:")
    || uri.starts_with("spotify:playlist:")
    || uri.starts_with("spotify:artist:")
    || uri.starts_with("spotify:show:")
}

fn require_nonempty(function_name: &str, arg_name: &str, value: &str) -> mlua::Result<()> {
  if value.trim().is_empty() {
    return Err(mlua::Error::RuntimeError(format!(
      "{function_name}: {arg_name} must be a non-empty string"
    )));
  }
  Ok(())
}

/// Install an action taking a single non-empty string argument.
fn install_string_action(
  lua: &Lua,
  tbl: &mlua::Table,
  shared: &Rc<ScriptShared>,
  name: &'static str,
  make: fn(String) -> ScriptEffect,
) -> mlua::Result<()> {
  let shared = shared.clone();
  let f = lua.create_function(move |_, value: String| {
    require_nonempty(&format!("spotatui.{name}"), "argument", &value)?;
    shared.effects.borrow_mut().push(make(value));
    Ok(())
  })?;
  tbl.set(name, f)?;
  Ok(())
}

/// Install an argument-less async data read: `spotatui.<name>(cb)` registers the
/// callback and queues a [`DataRequest`] for the engine's next intake pass.
fn install_data_read(
  lua: &Lua,
  tbl: &mlua::Table,
  shared: &Rc<ScriptShared>,
  name: &'static str,
  kind: PluginDataKind,
) -> mlua::Result<()> {
  let lua_inner = lua.clone();
  let shared = shared.clone();
  let f = lua.create_function(move |_, callback: mlua::Function| {
    let token = register_callback(&lua_inner, &shared, DATA_CALLBACKS_KEY, callback)?;
    shared.data_requests.borrow_mut().push(DataRequest {
      token,
      kind,
      arg: None,
    });
    Ok(())
  })?;
  tbl.set(name, f)?;
  Ok(())
}

fn validate_http_url(function_name: &str, url: &str) -> mlua::Result<()> {
  let parsed = reqwest::Url::parse(url)
    .map_err(|e| mlua::Error::RuntimeError(format!("{function_name}: invalid URL '{url}': {e}")))?;
  match parsed.scheme() {
    "http" | "https" => Ok(()),
    scheme => Err(mlua::Error::RuntimeError(format!(
      "{function_name}: unsupported URL scheme '{scheme}'"
    ))),
  }
}

/// Store a `{ plugin, callback }` entry under a fresh token in the given
/// registry table. Shared by HTTP requests, data requests and timers.
pub(super) fn register_callback(
  lua: &Lua,
  shared: &Rc<ScriptShared>,
  registry_key: &str,
  callback: mlua::Function,
) -> mlua::Result<u64> {
  let token = shared
    .next_token
    .get()
    .checked_add(1)
    .ok_or_else(|| mlua::Error::RuntimeError("spotatui: token overflow".to_string()))?;
  let key = i64::try_from(token)
    .map_err(|_| mlua::Error::RuntimeError("spotatui: token overflow".to_string()))?;
  shared.next_token.set(token);

  let callbacks: mlua::Table = lua.named_registry_value(registry_key)?;
  let entry = lua.create_table()?;
  entry.set("plugin", shared.current_plugin.borrow().clone())?;
  entry.set("callback", callback)?;
  callbacks.raw_set(key, entry)?;
  Ok(token)
}

fn register_http_callback(
  lua: &Lua,
  shared: &Rc<ScriptShared>,
  callback: mlua::Function,
) -> mlua::Result<u64> {
  register_callback(lua, shared, HTTP_CALLBACKS_KEY, callback)
}

fn collect_headers(headers: Option<mlua::Table>) -> mlua::Result<Vec<(String, String)>> {
  let Some(headers) = headers else {
    return Ok(Vec::new());
  };
  let mut out = Vec::new();
  for pair in headers.pairs::<String, String>() {
    out.push(pair?);
  }
  Ok(out)
}

async fn run_http_get(client: reqwest::Client, url: String) -> Result<HttpResponseData, String> {
  let response = client.get(url).send().await.map_err(|e| e.to_string())?;
  response_data(response).await
}

async fn run_http_post(
  client: reqwest::Client,
  url: String,
  body: String,
  headers: Vec<(String, String)>,
) -> Result<HttpResponseData, String> {
  let mut request = client.post(url).body(body);
  for (key, value) in headers {
    request = request.header(key, value);
  }
  let response = request.send().await.map_err(|e| e.to_string())?;
  response_data(response).await
}

async fn response_data(response: reqwest::Response) -> Result<HttpResponseData, String> {
  let status = response.status().as_u16();
  let bytes = response.bytes().await.map_err(|e| e.to_string())?;
  let body = String::from_utf8_lossy(&bytes).into_owned();
  Ok(HttpResponseData { status, body })
}

/// Check the screen exists and is owned by the calling plugin; returns the
/// screen's registered title.
fn verify_screen_owner(
  lua: &Lua,
  shared: &Rc<ScriptShared>,
  fn_name: &str,
  name: &str,
) -> mlua::Result<String> {
  let screens: mlua::Table = lua.named_registry_value(SCREENS_KEY)?;
  let entry: mlua::Table = screens
    .get::<Option<mlua::Table>>(name.to_string())?
    .ok_or_else(|| {
      mlua::Error::RuntimeError(format!(
        "{fn_name}: no screen named '{name}' is registered (call register_screen first)"
      ))
    })?;
  let owner: String = entry.get("plugin").unwrap_or_default();
  let caller = shared.current_plugin.borrow().clone();
  if owner != caller {
    return Err(mlua::Error::RuntimeError(format!(
      "{fn_name}: screen '{name}' belongs to plugin '{owner}'"
    )));
  }
  entry
    .get::<Option<String>>("title")
    .map(Option::unwrap_or_default)
}

/// Parse the widget array for `spotatui.set_screen`. Each item is a table with
/// a `type` of "paragraph", "list" or "gauge".
fn parse_screen_widgets(widgets: mlua::Table) -> mlua::Result<Vec<plugin_api::PluginWidget>> {
  const FN: &str = "spotatui.set_screen";
  let mut out = Vec::new();
  for item in widgets.sequence_values::<mlua::Table>() {
    let item = item?;
    let kind: String = item.get::<Option<String>>("type")?.ok_or_else(|| {
      mlua::Error::RuntimeError(format!("{FN}: each widget must have a 'type' field"))
    })?;
    match kind.as_str() {
      "paragraph" => {
        let lines: mlua::Value = item.get("lines")?;
        let lines = parse_styled_lines(FN, lines)?;
        let height: Option<u16> = item.get("height")?;
        out.push(plugin_api::PluginWidget::Paragraph { lines, height });
      }
      "list" => {
        let items_val: mlua::Value = item.get("items")?;
        let items = parse_styled_lines(FN, items_val)?;
        let title: Option<String> = item.get("title")?;
        let height: Option<u16> = item.get("height")?;
        // Lua-side `selected` is 1-based (like Lua arrays); stored 0-based.
        let selected: Option<i64> = item.get("selected")?;
        let selected = match selected {
          Some(s) if s >= 1 => Some((s - 1) as usize),
          Some(s) => {
            return Err(mlua::Error::RuntimeError(format!(
              "{FN}: list 'selected' is 1-based and must be >= 1, got {s}"
            )));
          }
          None => None,
        };
        out.push(plugin_api::PluginWidget::List {
          title,
          items,
          selected,
          height,
        });
      }
      "gauge" => {
        let ratio: f64 = item.get::<Option<f64>>("ratio")?.unwrap_or(0.0);
        let label: Option<String> = item.get("label")?;
        out.push(plugin_api::PluginWidget::Gauge {
          ratio: ratio.clamp(0.0, 1.0),
          label,
        });
      }
      other => {
        return Err(mlua::Error::RuntimeError(format!(
          "{FN}: unknown widget type '{other}' (expected paragraph, list or gauge)"
        )));
      }
    }
  }
  Ok(out)
}

/// Parse the `lines` argument for `spotatui.popup`.
fn parse_popup_lines(val: mlua::Value) -> mlua::Result<Vec<PopupLine>> {
  parse_styled_lines("spotatui.popup", val)
}

/// Parse styled lines shared by popups and screen widgets.
///
/// Accepts: a single string, or an array whose items are each a string or a table
/// `{ text, fg?, bold?, italic? }`.
fn parse_styled_lines(fn_name: &str, val: mlua::Value) -> mlua::Result<Vec<PopupLine>> {
  match val {
    mlua::Value::String(s) => Ok(vec![PopupLine {
      text: s.to_str()?.to_string(),
      fg: None,
      bold: false,
      italic: false,
    }]),
    mlua::Value::Table(tbl) => {
      let mut lines = Vec::new();
      for item in tbl.sequence_values::<mlua::Value>() {
        let item = item?;
        match item {
          mlua::Value::String(s) => lines.push(PopupLine {
            text: s.to_str()?.to_string(),
            fg: None,
            bold: false,
            italic: false,
          }),
          mlua::Value::Table(t) => {
            let text: Option<String> = t.get("text")?;
            let text = text.ok_or_else(|| {
              mlua::Error::RuntimeError(format!(
                "{fn_name}: each line table must have a 'text' field"
              ))
            })?;
            let fg_str: Option<String> = t.get("fg")?;
            let fg = fg_str
              .map(|s| {
                parse_theme_item(&s).map_err(|e| {
                  mlua::Error::RuntimeError(format!("{fn_name}: invalid color '{}': {}", s, e))
                })
              })
              .transpose()?;
            let bold: bool = t.get::<Option<bool>>("bold")?.unwrap_or(false);
            let italic: bool = t.get::<Option<bool>>("italic")?.unwrap_or(false);
            lines.push(PopupLine {
              text,
              fg,
              bold,
              italic,
            });
          }
          other => {
            return Err(mlua::Error::RuntimeError(format!(
              "{fn_name}: each line must be a string or table, got {}",
              other.type_name()
            )));
          }
        }
      }
      Ok(lines)
    }
    other => Err(mlua::Error::RuntimeError(format!(
      "{fn_name}: lines must be a string or array, got {}",
      other.type_name()
    ))),
  }
}

/// Install a no-argument action that pushes a fixed effect.
pub(super) fn install_action(
  lua: &Lua,
  tbl: &mlua::Table,
  shared: &Rc<ScriptShared>,
  name: &str,
  make: fn() -> ScriptEffect,
) -> mlua::Result<()> {
  let shared = shared.clone();
  let f = lua.create_function(move |_, ()| {
    shared.effects.borrow_mut().push(make());
    Ok(())
  })?;
  tbl.set(name, f)?;
  Ok(())
}
