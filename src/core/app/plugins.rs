use super::*;

/// Data domains plugins can request through the scripting API. Each domain has
/// a generation counter in [`PluginDataGenerations`] that the network layer
/// bumps whenever it writes that domain to `App`, so the script engine can tell
/// "the data a plugin asked for has arrived" without a per-request completion
/// signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginDataKind {
  Playlists,
  Queue,
  Search,
  SavedTracks,
  SavedAlbums,
  SavedShows,
  RecentlyPlayed,
  Devices,
  Lyrics,
}

impl PluginDataKind {
  pub const COUNT: usize = 9;

  pub fn index(self) -> usize {
    self as usize
  }
}

/// Per-domain write counters for plugin data requests. See [`PluginDataKind`].
#[derive(Debug, Default)]
pub struct PluginDataGenerations {
  counters: [u64; PluginDataKind::COUNT],
}

impl PluginDataGenerations {
  pub fn bump(&mut self, kind: PluginDataKind) {
    let slot = &mut self.counters[kind.index()];
    *slot = slot.wrapping_add(1);
  }

  // Only the scripting engine reads generations; the network layer just bumps.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn get(&self, kind: PluginDataKind) -> u64 {
    self.counters[kind.index()]
  }
}

impl App {
  /// Queue a plugin command name to be executed by the scripting engine.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn queue_plugin_command(&mut self, name: String) {
    self.pending_plugin_commands.push(name);
  }
}
