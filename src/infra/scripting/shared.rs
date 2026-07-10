use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::core::app::PluginDataKind;
use crate::core::plugin_api::{
  ConfigSnapshot, DeviceInfo, PlaybackState, PlaylistInfo, QueueSnapshot, SearchResults,
};

use super::effects::ScriptEffect;

/// Registry key for the table mapping event name -> array of `{ plugin, callback }`.
pub(super) const HANDLERS_KEY: &str = "spotatui.handlers";

/// Registry key for the table mapping command name -> `{ plugin, callback }`.
pub(super) const COMMANDS_KEY: &str = "spotatui.commands";

/// Registry key for the table mapping HTTP token -> `{ plugin, callback }`.
pub(super) const HTTP_CALLBACKS_KEY: &str = "spotatui.http_callbacks";

/// Registry key for the table mapping data-request token -> `{ plugin, callback }`.
pub(super) const DATA_CALLBACKS_KEY: &str = "spotatui.data_callbacks";

/// Registry key for the table mapping timer token -> `{ plugin, callback }`.
pub(super) const TIMER_CALLBACKS_KEY: &str = "spotatui.timer_callbacks";

/// Registry key for the table mapping screen name ->
/// `{ plugin, title, on_key, on_open?, on_close? }`.
pub(super) const SCREENS_KEY: &str = "spotatui.screens";

pub(super) type HttpResult = (u64, Result<HttpResponseData, String>);

pub(super) struct HttpResponseData {
  pub(super) status: u16,
  pub(super) body: String,
}

/// A timer armed by `spotatui.set_timeout` / `set_interval`, waiting for the
/// engine's next timer pass. Queued (rather than inserted directly into the
/// engine's active list) so arming a timer from inside a firing callback can't
/// invalidate the iteration.
pub(super) struct NewTimer {
  pub(super) token: u64,
  pub(super) delay: std::time::Duration,
  /// `Some` for `set_interval`: the reschedule period.
  pub(super) interval: Option<std::time::Duration>,
}

/// A queued `spotatui.get_*` call, drained by the engine's intake pass while
/// it holds `&mut App` (so the generation capture + dispatch is atomic).
pub(super) struct DataRequest {
  pub(super) token: u64,
  pub(super) kind: PluginDataKind,
  /// Search query for `PluginDataKind::Search` requests.
  pub(super) arg: Option<String>,
}

/// Stable identifier for a plugin, derived from its load name: strip a
/// trailing `.lua`, then map anything outside `[A-Za-z0-9_-]` to `_`.
/// Used for storage namespaces and anywhere a filename-safe id is needed.
pub(crate) fn plugin_id(name: &str) -> String {
  let base = name.strip_suffix(".lua").unwrap_or(name);
  base
    .chars()
    .map(|c| {
      if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
        c
      } else {
        '_'
      }
    })
    .collect()
}

/// State shared between the engine and the Lua closures via `Rc`.
///
/// `mlua` is built without the `send` feature, so `Rc`/`RefCell` are fine here: everything
/// runs on the single UI task.
pub(crate) struct ScriptShared {
  /// Playback snapshot, refreshed by the runner before callbacks run.
  pub(crate) playback: RefCell<Option<PlaybackState>>,
  pub(super) devices: RefCell<Vec<DeviceInfo>>,
  pub(crate) effects: RefCell<Vec<ScriptEffect>>,
  /// Plugin name currently being loaded, so `spotatui.on` can tag its callbacks.
  pub(super) current_plugin: RefCell<String>,
  /// Single token counter shared by HTTP requests, data requests and timers.
  pub(super) next_token: Cell<u64>,
  /// Intake queue for `spotatui.get_*` data requests.
  pub(super) data_requests: RefCell<Vec<DataRequest>>,
  /// Intake queue for newly-armed timers.
  pub(super) new_timers: RefCell<Vec<NewTimer>>,
  /// Intake queue for `spotatui.cancel_timer` tokens.
  pub(super) cancelled_timers: RefCell<Vec<u64>>,
  // Caches backing the synchronous reads (`spotatui.playlists()` etc.).
  // Refreshed by the engine only when the matching generation advanced.
  pub(super) playlists_cache: RefCell<Vec<PlaylistInfo>>,
  pub(super) queue_cache: RefCell<QueueSnapshot>,
  pub(super) search_results_cache: RefCell<SearchResults>,
  /// Backs `spotatui.config()`; refreshed by the engine each tick.
  pub(super) config_cache: RefCell<ConfigSnapshot>,
  /// Current route name, refreshed alongside the engine's route diffing;
  /// backs the synchronous `spotatui.current_route()` read.
  pub(super) current_route: RefCell<String>,
  /// The app config directory, set by `load_user_scripts`. Plugin storage
  /// lives under `<config_dir>/plugin-data/<plugin_id>.json`.
  pub(super) config_dir: RefCell<Option<PathBuf>>,
  /// Lazily-loaded storage namespaces: plugin_id -> flat JSON object.
  pub(super) storage: RefCell<BTreeMap<String, serde_json::Map<String, serde_json::Value>>>,
  /// Namespaces with unflushed writes.
  pub(super) storage_dirty: RefCell<BTreeSet<String>>,
}

impl ScriptShared {
  pub(super) fn new() -> Self {
    ScriptShared {
      playback: RefCell::new(None),
      devices: RefCell::new(Vec::new()),
      effects: RefCell::new(Vec::new()),
      current_plugin: RefCell::new(String::new()),
      next_token: Cell::new(0),
      data_requests: RefCell::new(Vec::new()),
      new_timers: RefCell::new(Vec::new()),
      cancelled_timers: RefCell::new(Vec::new()),
      playlists_cache: RefCell::new(Vec::new()),
      queue_cache: RefCell::new(QueueSnapshot::default()),
      search_results_cache: RefCell::new(SearchResults::default()),
      config_cache: RefCell::new(ConfigSnapshot::default()),
      current_route: RefCell::new(String::new()),
      config_dir: RefCell::new(None),
      storage: RefCell::new(BTreeMap::new()),
      storage_dirty: RefCell::new(BTreeSet::new()),
    }
  }

  /// Path of a plugin's storage file, when a config dir is known.
  pub(super) fn storage_path(&self, namespace: &str) -> Option<PathBuf> {
    self
      .config_dir
      .borrow()
      .as_ref()
      .map(|dir| dir.join("plugin-data").join(format!("{namespace}.json")))
  }
}
