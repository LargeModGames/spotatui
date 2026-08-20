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

  /// Show a plugin popup, resetting its scroll to the top.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub(crate) fn show_plugin_popup(&mut self, popup: crate::core::plugin_api::PluginPopup) {
    self.plugin_popup = Some(popup);
    self.view.plugin_popup_scroll = 0;
  }

  /// Navigate to a registered plugin screen, resetting its scroll. A no-op
  /// push when the screen is already the current route (the scroll still
  /// resets, matching the historic behavior).
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub(crate) fn open_plugin_screen(&mut self, name: String) {
    if self.get_current_route().id != RouteId::PluginScreen(name.clone()) {
      self.push_navigation_stack(RouteId::PluginScreen(name), ActiveBlock::PluginScreen);
    }
    self.view.plugin_screen_scroll = 0;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  // These live here rather than in `core/action/tests.rs` because they must
  // seed the scroll fields to a nonzero value first, and `core/app/` is the
  // one place the `view_writes_outside_tui` gate exempts.

  #[test]
  fn show_plugin_popup_resets_the_scroll() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.view.plugin_popup_scroll = 5;

    app.show_plugin_popup(crate::core::plugin_api::PluginPopup {
      title: "Hi".to_string(),
      lines: Vec::new(),
    });

    assert_eq!(
      app.plugin_popup.as_ref().map(|p| p.title.as_str()),
      Some("Hi")
    );
    assert_eq!(app.view.plugin_popup_scroll, 0);
  }

  #[test]
  fn open_plugin_screen_resets_the_scroll_even_when_already_current() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.view.plugin_screen_scroll = 4;
    app.open_plugin_screen("stats".to_string());
    assert_eq!(
      app.get_current_route().id,
      RouteId::PluginScreen("stats".to_string())
    );
    assert_eq!(app.view.plugin_screen_scroll, 0, "reset on the first open");

    // Re-opening the already-current screen must not stack a second frame,
    // but the scroll still resets (the historic behavior).
    app.view.plugin_screen_scroll = 4;
    app.open_plugin_screen("stats".to_string());
    assert_eq!(app.view.plugin_screen_scroll, 0, "reset on a re-open too");
    app.pop_navigation_stack();
    assert_eq!(
      app.get_current_route().id,
      RouteId::Home,
      "one pop suffices, so the re-open pushed no second frame"
    );
  }
}
