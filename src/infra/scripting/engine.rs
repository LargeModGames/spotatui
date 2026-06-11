use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, Value};
#[cfg(test)]
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::core::app::App;
use crate::core::plugin_api;

use super::api::install_api;
use super::effects::{apply_effects, ScriptEffect};
use super::events::{diff_events, queue_uris, ScriptEvent};
use super::shared::{
  HttpResponseData, HttpResult, ScriptShared, COMMANDS_KEY, HANDLERS_KEY, HTTP_CALLBACKS_KEY,
};

pub struct ScriptEngine {
  pub(super) lua: Lua,
  pub(crate) shared: Rc<ScriptShared>,
  /// Previous playback snapshot, for diffing on tick.
  last_playback: Option<crate::core::plugin_api::PlaybackState>,
  /// Previous queue item uris, for diffing on tick.
  last_queue: Option<Vec<String>>,
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

    install_api(&lua, &shared, http_tx.clone(), http_client, rt_handle)?;

    Ok(ScriptEngine {
      lua,
      shared,
      last_playback: None,
      last_queue: None,
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
    self.emit(ScriptEvent::Start);
    self.drain_http_callbacks();
    self.drain_effects(app);
  }

  /// On tick: if there are no handlers at all, return cheaply. Otherwise refresh caches,
  /// diff against the previous snapshot, emit each derived event, then drain.
  pub fn on_tick(&mut self, app: &mut App) {
    self.drain_http_callbacks();
    if !self.has_any_handlers() {
      self.drain_effects(app);
      return;
    }

    let new_playback = plugin_api::playback_state(app);
    let new_queue = Some(queue_uris(app));

    let events = diff_events(
      &self.last_playback,
      &self.last_queue,
      &new_playback,
      &new_queue,
    );

    *self.shared.playback.borrow_mut() = new_playback.clone();
    *self.shared.devices.borrow_mut() = plugin_api::device_list(app);

    self.last_playback = new_playback;
    self.last_queue = new_queue;

    for ev in events {
      self.emit(ev);
    }
    self.drain_http_callbacks();
    self.drain_effects(app);
  }

  /// Emit Quit, drain effects.
  pub fn on_quit(&mut self, app: &mut App) {
    self.refresh_caches(app);
    self.emit(ScriptEvent::Quit);
    self.drain_effects(app);
  }

  fn refresh_caches(&mut self, app: &App) {
    let pb = plugin_api::playback_state(app);
    *self.shared.playback.borrow_mut() = pb.clone();
    *self.shared.devices.borrow_mut() = plugin_api::device_list(app);
    self.last_playback = pb;
    self.last_queue = Some(queue_uris(app));
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

    let arg = if event.passes_playback_arg() {
      self.playback_value()
    } else {
      Value::Nil
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
  pub fn run_pending_commands(&mut self, app: &mut App) {
    if app.pending_plugin_commands.is_empty() {
      self.drain_http_callbacks();
      self.drain_effects(app);
      return;
    }
    self.refresh_caches(app);
    let names: Vec<String> = app.pending_plugin_commands.drain(..).collect();
    let commands: mlua::Table = match self.lua.named_registry_value(COMMANDS_KEY) {
      Ok(t) => t,
      Err(_) => {
        self.drain_http_callbacks();
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
    self.drain_effects(app);
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

/// First line of an error string (Lua tracebacks are multi-line).
fn first_line(s: &str) -> String {
  s.lines().next().unwrap_or(s).trim().to_string()
}
