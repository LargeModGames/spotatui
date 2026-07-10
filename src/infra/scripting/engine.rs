use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, Value};
#[cfg(test)]
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::core::app::{App, LyricsStatus, PluginDataKind};
use crate::core::plugin_api;
use crate::infra::network::IoEvent;

use super::api::install_api;
use super::effects::{apply_effects, ScriptEffect};
use super::events::{diff_events, diff_state_events, queue_uris, ScriptEvent};
use super::shared::{
  DataRequest, HttpResponseData, HttpResult, ScriptShared, COMMANDS_KEY, DATA_CALLBACKS_KEY,
  HANDLERS_KEY, HTTP_CALLBACKS_KEY, SCREENS_KEY, TIMER_CALLBACKS_KEY,
};

/// How long a plugin data request may wait for its generation to advance
/// before its callback is failed with a timeout error.
const DATA_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Minimum interval between throttled plugin-storage flushes (mirrors the
/// runner's session-save cadence).
const STORAGE_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);

/// An in-flight plugin data request: dispatched, waiting for the domain's
/// generation counter to move past the value captured at intake.
struct PendingDataRequest {
  token: u64,
  kind: PluginDataKind,
  requested_gen: u64,
  deadline: std::time::Instant,
}

/// An armed plugin timer. `interval` is `Some` for `set_interval`.
struct ActiveTimer {
  token: u64,
  due: std::time::Instant,
  interval: Option<std::time::Duration>,
}

pub struct ScriptEngine {
  pub(super) lua: Lua,
  pub(crate) shared: Rc<ScriptShared>,
  /// Previous playback snapshot, for diffing on tick.
  last_playback: Option<crate::core::plugin_api::PlaybackState>,
  /// Previous queue item uris, for diffing on tick.
  last_queue: Option<Vec<String>>,
  /// Previous route name, for `route_change` diffing.
  last_route: String,
  /// Set when the Search generation advanced since the last tick diff
  /// (consumed by the `search_results` event).
  search_gen_advanced: bool,
  /// Data requests dispatched but not yet resolved.
  pending_data: Vec<PendingDataRequest>,
  /// Armed plugin timers, fired from the tick pass.
  timers: Vec<ActiveTimer>,
  /// Generations backing the cached sync reads at their last refresh.
  /// `u64::MAX` sentinels force the first refresh.
  last_cache_gens: [u64; PluginDataKind::COUNT],
  /// Last throttled storage flush.
  last_storage_flush: Option<std::time::Instant>,
  http_rx: UnboundedReceiver<HttpResult>,
  #[cfg(test)]
  http_tx: UnboundedSender<HttpResult>,
}

impl ScriptEngine {
  /// Build the Lua state and install the `spotatui` global table.
  pub fn new() -> mlua::Result<Self> {
    let lua = Lua::new();
    let shared = Rc::new(ScriptShared::new());
    let (http_tx, http_rx) = unbounded_channel();
    let http_client = reqwest::Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .user_agent(format!("spotatui/{}", env!("CARGO_PKG_VERSION")))
      .build()
      .map_err(mlua::Error::external)?;
    let rt_handle = tokio::runtime::Handle::try_current().ok();

    // Registry handler table: { event_name = { {plugin=, callback=}, ... } }.
    let handlers = lua.create_table()?;
    lua.set_named_registry_value(HANDLERS_KEY, handlers)?;

    // Registry commands table: { command_name = { plugin=, callback= } }.
    let commands = lua.create_table()?;
    lua.set_named_registry_value(COMMANDS_KEY, commands)?;

    // Registry HTTP callback table: { token = { plugin=, callback= } }.
    let http_callbacks = lua.create_table()?;
    lua.set_named_registry_value(HTTP_CALLBACKS_KEY, http_callbacks)?;

    // Registry data callback table: { token = { plugin=, callback= } }.
    let data_callbacks = lua.create_table()?;
    lua.set_named_registry_value(DATA_CALLBACKS_KEY, data_callbacks)?;

    // Registry timer callback table: { token = { plugin=, callback= } }.
    let timer_callbacks = lua.create_table()?;
    lua.set_named_registry_value(TIMER_CALLBACKS_KEY, timer_callbacks)?;

    // Registry screens table: { name = { plugin=, title=, on_key=, on_open?, on_close? } }.
    let screens = lua.create_table()?;
    lua.set_named_registry_value(SCREENS_KEY, screens)?;

    install_api(&lua, &shared, http_tx.clone(), http_client, rt_handle)?;

    Ok(ScriptEngine {
      lua,
      shared,
      last_playback: None,
      last_queue: None,
      last_route: String::new(),
      search_gen_advanced: false,
      pending_data: Vec::new(),
      timers: Vec::new(),
      last_cache_gens: [u64::MAX; PluginDataKind::COUNT],
      last_storage_flush: None,
      http_rx,
      #[cfg(test)]
      http_tx,
    })
  }

  /// Load `init.lua`, then single-file `plugins/*.lua`, then directory plugins
  /// `plugins/<name>/main.lua` (falling back to `init.lua`). Each group is sorted by filename.
  /// Missing files/dir are fine. A failing file logs an error and queues a Notify effect but
  /// never aborts the others. Returns the number of plugins loaded successfully.
  pub fn load_user_scripts(&mut self, config_dir: &Path) -> usize {
    *self.shared.config_dir.borrow_mut() = Some(config_dir.to_path_buf());
    let mut loaded = 0;

    let init_path = config_dir.join("init.lua");
    if init_path.is_file() && self.load_file(&init_path, "init.lua") {
      loaded += 1;
    }

    let plugins_dir = config_dir.join("plugins");
    if plugins_dir.is_dir() {
      let entries: Vec<_> = std::fs::read_dir(&plugins_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();

      // Single-file plugins: plugins/<name>.lua. Real files only (a directory named
      // `foo.lua` is handled by the directory branch below), and hidden files are skipped to
      // match the directory branch and avoid loading OS cruft like `._foo.lua`.
      let mut files: Vec<_> = entries
        .iter()
        .filter(|p| p.is_file())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("lua"))
        .filter(|p| {
          p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| !n.starts_with('.'))
        })
        .cloned()
        .collect();
      files.sort();
      for path in files {
        let name = path
          .file_name()
          .and_then(|n| n.to_str())
          .unwrap_or("plugin")
          .to_string();
        if self.load_file(&path, &name) {
          loaded += 1;
        }
      }

      // Directory plugins: plugins/<name>/main.lua (or init.lua). These are how git-installed
      // plugins (`spotatui plugin add`) ship. Hidden dirs (e.g. dotfiles) are ignored.
      let mut dirs: Vec<_> = entries
        .iter()
        .filter(|p| p.is_dir())
        .filter(|p| {
          p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| !n.starts_with('.'))
        })
        .cloned()
        .collect();
      dirs.sort();
      for dir in dirs {
        let name = dir
          .file_name()
          .and_then(|n| n.to_str())
          .unwrap_or("plugin")
          .to_string();
        let entry = ["main.lua", "init.lua"]
          .iter()
          .map(|f| dir.join(f))
          .find(|p| p.is_file());
        let Some(entry) = entry else {
          continue;
        };
        if let Err(e) = self.add_plugin_module_path(&dir) {
          log::warn!("[lua] failed to extend package.path for plugin '{name}': {e}");
        }
        if self.load_file(&entry, &name) {
          loaded += 1;
        }
      }
    }

    loaded
  }

  /// Prepend a directory plugin's own folder to Lua's `package.path` so it can `require` its
  /// sibling modules (`require("foo")` -> `<dir>/foo.lua` or `<dir>/foo/init.lua`). `package.path`
  /// and the module cache are shared across all plugins: if two plugins both `require("util")`,
  /// Lua caches the first-loaded plugin's `util.lua` under that name and silently hands the same
  /// module to the later plugin. Give helper modules distinctive (e.g. plugin-prefixed) names.
  fn add_plugin_module_path(&self, dir: &Path) -> mlua::Result<()> {
    let package: mlua::Table = self.lua.globals().get("package")?;
    let current: String = package.get("path").unwrap_or_default();
    let dir = dir.to_string_lossy();
    let sep = std::path::MAIN_SEPARATOR;
    package.set(
      "path",
      format!("{dir}{sep}?.lua;{dir}{sep}?{sep}init.lua;{current}"),
    )?;
    Ok(())
  }

  /// Read and load a single file. Returns true on success. On any failure logs and queues a
  /// Notify effect, returning false.
  fn load_file(&mut self, path: &Path, name: &str) -> bool {
    let source = match std::fs::read_to_string(path) {
      Ok(s) => s,
      Err(e) => {
        log::error!("[lua] failed to read {}: {}", path.display(), e);
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{name}' failed to load: {e}"),
            6,
          ));
        return false;
      }
    };
    match self.load_source(name, &source) {
      Ok(()) => true,
      Err(e) => {
        let fl = first_line(&e.to_string());
        log::error!("[lua] failed to load plugin '{name}': {e}");
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{name}' failed to load: {fl}"),
            6,
          ));
        false
      }
    }
  }

  /// Execute a Lua chunk under the given plugin name (used as the chunk name for tracebacks).
  /// Public for tests.
  pub fn load_source(&mut self, plugin_name: &str, source: &str) -> mlua::Result<()> {
    *self.shared.current_plugin.borrow_mut() = plugin_name.to_string();
    let result = self
      .lua
      .load(source)
      .set_name(plugin_name.to_string())
      .exec();
    self.shared.current_plugin.borrow_mut().clear();
    result
  }

  /// Refresh caches, emit Start, drain effects.
  pub fn on_start(&mut self, app: &mut App) {
    self.refresh_caches(app);
    self.refresh_data_caches(app);
    self.emit(ScriptEvent::Start);
    self.drain_http_callbacks();
    self.process_data_requests(app);
    self.drain_effects(app);
  }

  /// On tick: if there are no handlers at all, return cheaply. Otherwise refresh caches,
  /// diff against the previous snapshot, emit each derived event, then drain.
  pub fn on_tick(&mut self, app: &mut App) {
    self.drain_http_callbacks();
    self.process_data_requests(app);
    self.process_timers();
    self.flush_storage(false);
    self.refresh_data_caches(app);
    if !self.has_any_handlers() {
      // Still drain effects and track route transitions: screens and timers
      // work without any event handlers registered.
      self.drain_effects(app);
      self.sync_route_and_emit_change(app);
      return;
    }

    let new_playback = plugin_api::playback_state(app);
    let new_queue = Some(queue_uris(app));

    let mut events = diff_events(
      &self.last_playback,
      &self.last_queue,
      &new_playback,
      &new_queue,
    );

    // Non-playback state diffs: route, devices, search generation. The device
    // diff must read the old cache before it is overwritten below.
    let new_route = plugin_api::route_name(app.get_current_route());
    let new_devices = plugin_api::device_list(app);
    let search_gen_advanced = self.search_gen_advanced;
    self.search_gen_advanced = false;
    {
      let old_devices = self.shared.devices.borrow();
      events.extend(diff_state_events(
        &self.last_route,
        &new_route,
        &old_devices,
        &new_devices,
        search_gen_advanced,
      ));
    }

    *self.shared.playback.borrow_mut() = new_playback.clone();
    *self.shared.devices.borrow_mut() = new_devices;
    if self.last_route != new_route {
      let old_route = std::mem::replace(&mut self.last_route, new_route.clone());
      self.handle_screen_transition(&old_route, &new_route);
    }
    *self.shared.current_route.borrow_mut() = new_route;

    self.last_playback = new_playback;
    self.last_queue = new_queue;

    for ev in events {
      self.emit(ev);
    }
    self.drain_http_callbacks();
    self.process_data_requests(app);
    self.drain_effects(app);
    // Catch route changes caused by effects drained just above (e.g. a timer
    // callback calling show_screen).
    self.sync_route_and_emit_change(app);
  }

  /// Emit Quit, drain effects, force-flush plugin storage.
  pub fn on_quit(&mut self, app: &mut App) {
    self.refresh_caches(app);
    self.emit(ScriptEvent::Quit);
    self.drain_effects(app);
    self.flush_storage(true);
  }

  fn refresh_caches(&mut self, app: &App) {
    let pb = plugin_api::playback_state(app);
    *self.shared.playback.borrow_mut() = pb.clone();
    *self.shared.devices.borrow_mut() = plugin_api::device_list(app);
    self.last_playback = pb;
    self.last_queue = Some(queue_uris(app));
    let route = plugin_api::route_name(app.get_current_route());
    self.last_route.clone_from(&route);
    *self.shared.current_route.borrow_mut() = route;
  }

  /// Detect a route change outside the tick diff (plugin commands and the key
  /// handlers that ran just before them can navigate). Fires screen
  /// on_close/on_open, emits `route_change`, and drains any queued effects.
  fn sync_route_and_emit_change(&mut self, app: &mut App) {
    let new_route = plugin_api::route_name(app.get_current_route());
    if new_route != self.last_route {
      let old_route = std::mem::replace(&mut self.last_route, new_route.clone());
      *self.shared.current_route.borrow_mut() = new_route.clone();
      self.handle_screen_transition(&old_route, &new_route);
      self.emit(ScriptEvent::RouteChange(new_route));
      self.drain_effects(app);
    }
  }

  /// Fire a plugin screen's on_close/on_open across a route transition.
  fn handle_screen_transition(&mut self, old_route: &str, new_route: &str) {
    if let Some(name) = old_route.strip_prefix("plugin:") {
      self.call_screen_callback(&name.to_string(), "on_close", None);
    }
    if let Some(name) = new_route.strip_prefix("plugin:") {
      self.call_screen_callback(&name.to_string(), "on_open", None);
    }
  }

  /// Forward keys queued while a plugin screen was focused to its `on_key`.
  fn dispatch_screen_keys(&mut self, app: &mut App) {
    if app.pending_plugin_screen_keys.is_empty() {
      return;
    }
    let keys: Vec<(String, String)> = app.pending_plugin_screen_keys.drain(..).collect();
    for (screen, key) in keys {
      self.call_screen_callback(&screen, "on_key", Some(key));
    }
  }

  /// Invoke one of a screen's registered callbacks (`on_key` gets the key
  /// string as its argument). Errors/panics queue a NotifyError and remove the
  /// offending callback from the screen entry (one strike).
  fn call_screen_callback(&mut self, screen: &String, field: &'static str, key: Option<String>) {
    let screens: mlua::Table = match self.lua.named_registry_value(SCREENS_KEY) {
      Ok(t) => t,
      Err(_) => return,
    };
    let entry: mlua::Table = match screens.get::<Option<mlua::Table>>(screen.clone()) {
      Ok(Some(t)) => t,
      _ => return,
    };
    let plugin: String = entry.get("plugin").unwrap_or_default();
    let callback: mlua::Function = match entry.get::<Option<mlua::Function>>(field) {
      Ok(Some(f)) => f,
      _ => return, // on_open/on_close are optional
    };
    drop(screens);

    *self.shared.current_plugin.borrow_mut() = plugin.clone();
    let call_result = catch_unwind(AssertUnwindSafe(|| match key {
      Some(key) => callback.call::<()>(key),
      None => callback.call::<()>(()),
    }));
    self.shared.current_plugin.borrow_mut().clear();

    let err_msg = match call_result {
      Ok(Ok(())) => return,
      Ok(Err(e)) => first_line(&e.to_string()),
      Err(_) => "panic".to_string(),
    };
    log::error!("[lua] plugin '{plugin}': error in screen '{screen}' {field}: {err_msg}");
    self
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::NotifyError(
        format!("plugin '{plugin}': error in screen '{screen}' {field}: {err_msg}"),
        6,
      ));
    // One strike: remove the erroring callback so it can't fire again.
    let _ = entry.set(field, Value::Nil);
  }

  fn has_any_handlers(&self) -> bool {
    let handlers: mlua::Table = match self.lua.named_registry_value(HANDLERS_KEY) {
      Ok(t) => t,
      Err(_) => return false,
    };
    handlers
      .pairs::<String, mlua::Table>()
      .any(|p| p.map(|(_, list)| list.raw_len() > 0).unwrap_or(false))
  }

  /// Invoke every registered callback for `event`. Lua errors and caught panics disable the
  /// offending callback (one strike) and queue a Notify effect.
  pub(crate) fn emit(&mut self, event: ScriptEvent) {
    let handlers: mlua::Table = match self.lua.named_registry_value(HANDLERS_KEY) {
      Ok(t) => t,
      Err(_) => return,
    };
    let list: mlua::Table = match handlers.get(event.lua_name()) {
      Ok(t) => t,
      Err(_) => return,
    };

    let len = list.raw_len();
    if len == 0 {
      return;
    }

    let arg = match &event {
      ScriptEvent::RouteChange(name) => match self.lua.create_table() {
        Ok(t) => {
          let _ = t.set("name", name.clone());
          Value::Table(t)
        }
        Err(_) => Value::Nil,
      },
      _ if event.passes_playback_arg() => self.playback_value(),
      _ => Value::Nil,
    };

    // Indices to remove after the pass (descending so removal stays valid).
    let mut to_remove: Vec<usize> = Vec::new();

    for idx in 1..=len {
      let entry: mlua::Table = match list.get(idx) {
        Ok(t) => t,
        Err(_) => continue,
      };
      let plugin: String = entry.get("plugin").unwrap_or_default();
      let callback: mlua::Function = match entry.get("callback") {
        Ok(f) => f,
        Err(_) => continue,
      };

      let arg = arg.clone();
      *self.shared.current_plugin.borrow_mut() = plugin.clone();
      let call_result = catch_unwind(AssertUnwindSafe(|| callback.call::<()>(arg)));
      self.shared.current_plugin.borrow_mut().clear();

      let err_msg = match call_result {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(first_line(&e.to_string())),
        Err(_) => Some("panic".to_string()),
      };

      if let Some(msg) = err_msg {
        log::error!(
          "[lua] plugin '{plugin}': error in on_{}: {msg}",
          event.lua_name()
        );
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{plugin}': error in on_{}: {msg}", event.lua_name()),
            6,
          ));
        to_remove.push(idx);
      }
    }

    for idx in to_remove.into_iter().rev() {
      let _ = list.raw_remove(idx);
    }
  }

  /// Serialize the cached playback snapshot into a Lua value (table or nil).
  fn playback_value(&self) -> Value {
    let pb = self.shared.playback.borrow().clone();
    match pb {
      Some(state) => self.lua.to_value(&state).unwrap_or(Value::Nil),
      None => Value::Nil,
    }
  }

  /// Run any commands queued in `app.pending_plugin_commands`, then drain effects.
  /// Also the route-change fast path: the key handler that ran just before may
  /// have navigated.
  pub fn run_pending_commands(&mut self, app: &mut App) {
    self.sync_route_and_emit_change(app);
    self.dispatch_screen_keys(app);
    if app.pending_plugin_commands.is_empty() {
      self.drain_http_callbacks();
      self.process_data_requests(app);
      self.drain_effects(app);
      return;
    }
    self.refresh_caches(app);
    let names: Vec<String> = app.pending_plugin_commands.drain(..).collect();
    let commands: mlua::Table = match self.lua.named_registry_value(COMMANDS_KEY) {
      Ok(t) => t,
      Err(_) => {
        self.drain_http_callbacks();
        self.process_data_requests(app);
        self.drain_effects(app);
        return;
      }
    };
    for name in names {
      let entry: mlua::Table = match commands.get::<Option<mlua::Table>>(name.clone()) {
        Ok(Some(t)) => t,
        _ => {
          self
            .shared
            .effects
            .borrow_mut()
            .push(ScriptEffect::NotifyError(
              format!("no plugin command named '{name}'"),
              6,
            ));
          continue;
        }
      };
      let plugin: String = entry.get("plugin").unwrap_or_default();
      let callback: mlua::Function = match entry.get("callback") {
        Ok(f) => f,
        Err(_) => continue,
      };
      *self.shared.current_plugin.borrow_mut() = plugin.clone();
      let call_result = catch_unwind(AssertUnwindSafe(|| callback.call::<()>(())));
      self.shared.current_plugin.borrow_mut().clear();
      match call_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
          let msg = first_line(&e.to_string());
          log::error!("[lua] plugin '{plugin}': error in command '{name}': {msg}");
          self
            .shared
            .effects
            .borrow_mut()
            .push(ScriptEffect::NotifyError(
              format!("plugin '{plugin}': error in command '{name}': {msg}"),
              6,
            ));
        }
        Err(_) => {
          log::error!("[lua] plugin '{plugin}': panic in command '{name}'");
          self
            .shared
            .effects
            .borrow_mut()
            .push(ScriptEffect::NotifyError(
              format!("plugin '{plugin}': panic in command '{name}'"),
              6,
            ));
        }
      }
    }
    self.drain_http_callbacks();
    self.process_data_requests(app);
    self.drain_effects(app);
    self.sync_route_and_emit_change(app);
  }

  /// Intake queued `spotatui.get_*` requests (capture generation + dispatch the
  /// matching IoEvent) and resolve pending ones whose generation advanced or
  /// whose deadline passed. Intake is atomic on the UI thread while holding
  /// `&mut App`, so no update can be lost between capture and dispatch.
  fn process_data_requests(&mut self, app: &mut App) {
    self.process_data_requests_at(app, std::time::Instant::now());
  }

  fn process_data_requests_at(&mut self, app: &mut App, now: std::time::Instant) {
    // Intake. Drain first so the RefCell borrow is dropped before any Lua or
    // App call, and so requests queued by callbacks below land in the next pass.
    let requests: Vec<DataRequest> = self.shared.data_requests.borrow_mut().drain(..).collect();
    for req in requests {
      // Lyrics fetches are driven by the runner's track-change detector, not by
      // dispatch: deliver immediately when the current status is terminal,
      // otherwise wait for the in-flight fetch to bump the generation.
      if req.kind == PluginDataKind::Lyrics
        && matches!(
          app.lyrics_status,
          LyricsStatus::Found | LyricsStatus::NotFound
        )
      {
        let value = self.data_value(req.kind, app);
        self.deliver_data_result(req.token, value.map_err(|e| e.to_string()));
        continue;
      }

      let requested_gen = app.plugin_data_generations.get(req.kind);
      let event = match req.kind {
        PluginDataKind::Playlists => Some(IoEvent::GetPlaylists),
        PluginDataKind::Queue => Some(IoEvent::GetQueue),
        PluginDataKind::Search => Some(IoEvent::GetSearchResults(
          req.arg.clone().unwrap_or_default(),
          app.get_user_country(),
        )),
        PluginDataKind::SavedTracks => Some(IoEvent::GetCurrentSavedTracks(None)),
        PluginDataKind::SavedAlbums => Some(IoEvent::GetCurrentUserSavedAlbums(None)),
        PluginDataKind::SavedShows => Some(IoEvent::GetCurrentUserSavedShows(None)),
        PluginDataKind::RecentlyPlayed => Some(IoEvent::GetRecentlyPlayedSilent),
        PluginDataKind::Devices => Some(IoEvent::GetDevicesSilent),
        PluginDataKind::Lyrics => None,
      };
      if let Some(event) = event {
        app.dispatch(event);
      }
      self.pending_data.push(PendingDataRequest {
        token: req.token,
        kind: req.kind,
        requested_gen,
        deadline: now + DATA_REQUEST_TIMEOUT,
      });
    }

    // Resolution. Serialize snapshots from `&App` first, collect the outcomes,
    // then call into Lua with no engine-side borrows held.
    if self.pending_data.is_empty() {
      return;
    }
    let mut resolved: Vec<(u64, Result<Value, String>)> = Vec::new();
    let lua = self.lua.clone();
    self.pending_data.retain(|p| {
      if app.plugin_data_generations.get(p.kind) != p.requested_gen {
        let value = data_value_for(&lua, p.kind, app).map_err(|e| e.to_string());
        resolved.push((p.token, value));
        false
      } else if now >= p.deadline {
        resolved.push((p.token, Err("request timed out".to_string())));
        false
      } else {
        true
      }
    });
    for (token, result) in resolved {
      self.deliver_data_result(token, result);
    }
  }

  /// Serialize the snapshot for a data kind from current `App` state.
  fn data_value(&self, kind: PluginDataKind, app: &App) -> mlua::Result<Value> {
    data_value_for(&self.lua, kind, app)
  }

  /// Arm newly-set timers, apply cancellations, then fire everything due.
  /// Callbacks run with catch_unwind + one strike (an erroring interval is
  /// removed). Intervals reschedule to `now + interval`, skipping missed
  /// periods rather than firing catch-up bursts.
  fn process_timers(&mut self) {
    self.process_timers_at(std::time::Instant::now());
  }

  fn process_timers_at(&mut self, now: std::time::Instant) {
    // Arm first, then cancel: a timer set and cancelled in the same pass stays
    // cancelled.
    let new_timers: Vec<_> = self.shared.new_timers.borrow_mut().drain(..).collect();
    for t in new_timers {
      self.timers.push(ActiveTimer {
        token: t.token,
        due: now + t.delay,
        interval: t.interval,
      });
    }
    let cancelled: Vec<u64> = self
      .shared
      .cancelled_timers
      .borrow_mut()
      .drain(..)
      .collect();
    if !cancelled.is_empty() {
      self.timers.retain(|t| !cancelled.contains(&t.token));
      if let Ok(callbacks) = self
        .lua
        .named_registry_value::<mlua::Table>(TIMER_CALLBACKS_KEY)
      {
        for token in cancelled {
          if let Ok(key) = i64::try_from(token) {
            let _ = callbacks.raw_set(key, Value::Nil);
          }
        }
      }
    }

    // Collect due tokens with no borrows held, rescheduling/removing as we go.
    let mut due: Vec<(u64, bool)> = Vec::new(); // (token, repeating)
    self.timers.retain_mut(|t| {
      if now < t.due {
        return true;
      }
      match t.interval {
        Some(interval) => {
          due.push((t.token, true));
          t.due = now + interval;
          true
        }
        None => {
          due.push((t.token, false));
          false
        }
      }
    });

    for (token, repeating) in due {
      if !self.fire_timer(token, repeating) && repeating {
        // One strike: remove the erroring interval.
        self.timers.retain(|t| t.token != token);
      }
    }
  }

  /// Invoke a timer callback. Returns false when the callback errored or
  /// panicked (the registry entry is removed in that case, and always for
  /// one-shot timeouts).
  fn fire_timer(&mut self, token: u64, repeating: bool) -> bool {
    let callbacks: mlua::Table = match self.lua.named_registry_value(TIMER_CALLBACKS_KEY) {
      Ok(t) => t,
      Err(_) => return false,
    };
    let key = match i64::try_from(token) {
      Ok(key) => key,
      Err(_) => return false,
    };
    let entry: mlua::Table = match callbacks.raw_get::<Option<mlua::Table>>(key) {
      Ok(Some(t)) => t,
      _ => return false,
    };
    let plugin: String = entry.get("plugin").unwrap_or_default();
    let callback: mlua::Function = match entry.get("callback") {
      Ok(f) => f,
      Err(_) => {
        let _ = callbacks.raw_set(key, Value::Nil);
        return false;
      }
    };
    if !repeating {
      let _ = callbacks.raw_set(key, Value::Nil);
    }
    drop(entry);
    drop(callbacks);

    *self.shared.current_plugin.borrow_mut() = plugin.clone();
    let call_result = catch_unwind(AssertUnwindSafe(|| callback.call::<()>(())));
    self.shared.current_plugin.borrow_mut().clear();

    let err_msg = match call_result {
      Ok(Ok(())) => return true,
      Ok(Err(e)) => first_line(&e.to_string()),
      Err(_) => "panic".to_string(),
    };
    log::error!("[lua] plugin '{plugin}': error in timer callback: {err_msg}");
    self
      .shared
      .effects
      .borrow_mut()
      .push(ScriptEffect::NotifyError(
        format!("plugin '{plugin}': error in timer callback: {err_msg}"),
        6,
      ));
    if repeating {
      // Remove the interval's registry entry too (one strike).
      if let Ok(callbacks) = self
        .lua
        .named_registry_value::<mlua::Table>(TIMER_CALLBACKS_KEY)
      {
        if let Ok(key) = i64::try_from(token) {
          let _ = callbacks.raw_set(key, Value::Nil);
        }
      }
    }
    false
  }

  #[cfg(test)]
  pub(super) fn process_timers_for_test(&mut self, now: std::time::Instant) {
    self.process_timers_at(now);
  }

  /// Write dirty storage namespaces to disk. Throttled unless `force` (quit).
  /// Last writer wins across concurrent spotatui instances -- documented.
  pub(super) fn flush_storage(&mut self, force: bool) {
    if self.shared.storage_dirty.borrow().is_empty() {
      return;
    }
    let now = std::time::Instant::now();
    if !force {
      if let Some(last) = self.last_storage_flush {
        if now.duration_since(last) < STORAGE_FLUSH_INTERVAL {
          return;
        }
      }
    }
    self.last_storage_flush = Some(now);

    let dirty: Vec<String> = std::mem::take(&mut *self.shared.storage_dirty.borrow_mut())
      .into_iter()
      .collect();
    let storage = self.shared.storage.borrow();
    for namespace in dirty {
      let Some(path) = self.shared.storage_path(&namespace) else {
        log::warn!("[lua] plugin storage for '{namespace}' has no config dir; not persisted");
        continue;
      };
      let Some(map) = storage.get(&namespace) else {
        continue;
      };
      if let Err(e) = write_storage_file(&path, map) {
        log::error!(
          "[lua] failed to write plugin storage {}: {e}",
          path.display()
        );
      }
    }
  }

  /// Call a data callback with `(data, nil)` or `(nil, err)`, one-shot. Errors
  /// and panics queue a NotifyError effect; the entry is always removed.
  fn deliver_data_result(&mut self, token: u64, result: Result<Value, String>) {
    let callbacks: mlua::Table = match self.lua.named_registry_value(DATA_CALLBACKS_KEY) {
      Ok(t) => t,
      Err(_) => return,
    };
    let key = match i64::try_from(token) {
      Ok(key) => key,
      Err(_) => return,
    };
    let entry: mlua::Table = match callbacks.raw_get::<Option<mlua::Table>>(key) {
      Ok(Some(t)) => t,
      _ => return,
    };
    let plugin: String = entry.get("plugin").unwrap_or_default();
    let callback: mlua::Function = match entry.get("callback") {
      Ok(f) => f,
      Err(_) => {
        let _ = callbacks.raw_set(key, Value::Nil);
        return;
      }
    };
    let _ = callbacks.raw_set(key, Value::Nil);
    drop(entry);
    drop(callbacks);

    let args = match result {
      Ok(value) => (value, Value::Nil),
      Err(err) => match self.lua.create_string(&err) {
        Ok(s) => (Value::Nil, Value::String(s)),
        Err(_) => (Value::Nil, Value::Nil),
      },
    };

    *self.shared.current_plugin.borrow_mut() = plugin.clone();
    let call_result = catch_unwind(AssertUnwindSafe(|| callback.call::<()>(args)));
    self.shared.current_plugin.borrow_mut().clear();

    match call_result {
      Ok(Ok(())) => {}
      Ok(Err(e)) => {
        let msg = first_line(&e.to_string());
        log::error!("[lua] plugin '{plugin}': error in data callback: {msg}");
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{plugin}': error in data callback: {msg}"),
            6,
          ));
      }
      Err(_) => {
        log::error!("[lua] plugin '{plugin}': panic in data callback");
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{plugin}': panic in data callback"),
            6,
          ));
      }
    }
  }

  /// Refresh the caches backing the synchronous reads (`spotatui.playlists()`
  /// etc.) when their data generation advanced since the last refresh.
  fn refresh_data_caches(&mut self, app: &App) {
    let refresh = |slot: &mut u64, kind: PluginDataKind| -> bool {
      let current = app.plugin_data_generations.get(kind);
      if *slot == current {
        return false;
      }
      *slot = current;
      true
    };

    if refresh(
      &mut self.last_cache_gens[PluginDataKind::Playlists.index()],
      PluginDataKind::Playlists,
    ) {
      *self.shared.playlists_cache.borrow_mut() = plugin_api::playlists_snapshot(app);
    }
    if refresh(
      &mut self.last_cache_gens[PluginDataKind::Queue.index()],
      PluginDataKind::Queue,
    ) {
      *self.shared.queue_cache.borrow_mut() = plugin_api::queue_snapshot(app);
    }
    // Config has no generation counter (theme/settings can change from many
    // places); rebuilding the small snapshot each pass is cheap.
    *self.shared.config_cache.borrow_mut() = plugin_api::config_snapshot(&app.user_config);

    let search_was = self.last_cache_gens[PluginDataKind::Search.index()];
    if refresh(
      &mut self.last_cache_gens[PluginDataKind::Search.index()],
      PluginDataKind::Search,
    ) {
      *self.shared.search_results_cache.borrow_mut() = plugin_api::search_results_snapshot(app);
      // Feed the `search_results` event, but not off the startup sentinel.
      if search_was != u64::MAX {
        self.search_gen_advanced = true;
      }
    }
  }

  #[cfg(test)]
  pub(super) fn process_data_requests_for_test(&mut self, app: &mut App, now: std::time::Instant) {
    self.process_data_requests_at(app, now);
  }

  fn drain_http_callbacks(&mut self) {
    while let Ok((token, result)) = self.http_rx.try_recv() {
      self.deliver_http_result(token, result);
    }
  }

  fn deliver_http_result(&mut self, token: u64, result: Result<HttpResponseData, String>) {
    let callbacks: mlua::Table = match self.lua.named_registry_value(HTTP_CALLBACKS_KEY) {
      Ok(t) => t,
      Err(_) => return,
    };
    let key = match i64::try_from(token) {
      Ok(key) => key,
      Err(_) => return,
    };
    let entry: mlua::Table = match callbacks.raw_get::<Option<mlua::Table>>(key) {
      Ok(Some(t)) => t,
      _ => return,
    };
    let plugin: String = entry.get("plugin").unwrap_or_default();
    let callback: mlua::Function = match entry.get("callback") {
      Ok(f) => f,
      Err(_) => {
        let _ = callbacks.raw_set(key, Value::Nil);
        return;
      }
    };
    let _ = callbacks.raw_set(key, Value::Nil);
    drop(entry);
    drop(callbacks);

    let args = match self.http_callback_args(result) {
      Ok(args) => args,
      Err(e) => {
        let msg = first_line(&e.to_string());
        log::error!("[lua] plugin '{plugin}': error preparing http callback: {msg}");
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{plugin}': error preparing http callback: {msg}"),
            6,
          ));
        return;
      }
    };

    *self.shared.current_plugin.borrow_mut() = plugin.clone();
    let call_result = catch_unwind(AssertUnwindSafe(|| callback.call::<()>(args)));
    self.shared.current_plugin.borrow_mut().clear();

    match call_result {
      Ok(Ok(())) => {}
      Ok(Err(e)) => {
        let msg = first_line(&e.to_string());
        log::error!("[lua] plugin '{plugin}': error in http callback: {msg}");
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{plugin}': error in http callback: {msg}"),
            6,
          ));
      }
      Err(_) => {
        log::error!("[lua] plugin '{plugin}': panic in http callback");
        self
          .shared
          .effects
          .borrow_mut()
          .push(ScriptEffect::NotifyError(
            format!("plugin '{plugin}': panic in http callback"),
            6,
          ));
      }
    }
  }

  fn http_callback_args(
    &self,
    result: Result<HttpResponseData, String>,
  ) -> mlua::Result<(Value, Value)> {
    match result {
      Ok(data) => {
        let resp = self.lua.create_table()?;
        resp.set("status", data.status)?;
        resp.set("ok", (200..=299).contains(&data.status))?;
        resp.set("body", data.body)?;
        Ok((Value::Table(resp), Value::Nil))
      }
      Err(err) => Ok((Value::Nil, Value::String(self.lua.create_string(&err)?))),
    }
  }

  #[cfg(test)]
  pub(super) fn inject_http_result(&self, token: u64, result: Result<HttpResponseData, String>) {
    self
      .http_tx
      .send((token, result))
      .expect("test HTTP result receiver should be alive");
  }

  #[cfg(test)]
  pub(super) fn drain_http_callbacks_for_test(&mut self) {
    self.drain_http_callbacks();
  }

  /// Drain queued effects into the app while holding `&mut App`.
  pub(crate) fn drain_effects(&self, app: &mut App) {
    let effects: Vec<ScriptEffect> = self.shared.effects.borrow_mut().drain(..).collect();
    apply_effects(effects, app);
  }
}

/// Serialize the plugin-facing snapshot for a data kind from `App` state.
fn data_value_for(lua: &Lua, kind: PluginDataKind, app: &App) -> mlua::Result<Value> {
  match kind {
    PluginDataKind::Playlists => lua.to_value(&plugin_api::playlists_snapshot(app)),
    PluginDataKind::Queue => lua.to_value(&plugin_api::queue_snapshot(app)),
    PluginDataKind::Search => lua.to_value(&plugin_api::search_results_snapshot(app)),
    PluginDataKind::SavedTracks => lua.to_value(&plugin_api::saved_tracks_snapshot(app)),
    PluginDataKind::SavedAlbums => lua.to_value(&plugin_api::saved_albums_snapshot(app)),
    PluginDataKind::SavedShows => lua.to_value(&plugin_api::saved_shows_snapshot(app)),
    PluginDataKind::RecentlyPlayed => lua.to_value(&plugin_api::recently_played_snapshot(app)),
    PluginDataKind::Devices => lua.to_value(&plugin_api::device_list(app)),
    PluginDataKind::Lyrics => lua.to_value(&plugin_api::lyrics_snapshot(app)),
  }
}

/// Temp-file + rename write of one storage namespace.
fn write_storage_file(
  path: &Path,
  map: &serde_json::Map<String, serde_json::Value>,
) -> std::io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let json = serde_json::to_string_pretty(map).map_err(std::io::Error::other)?;
  let tmp = path.with_extension("json.tmp");
  std::fs::write(&tmp, json)?;
  std::fs::rename(&tmp, path)
}

/// First line of an error string (Lua tracebacks are multi-line).
fn first_line(s: &str) -> String {
  s.lines().next().unwrap_or(s).trim().to_string()
}
