use super::*;

impl App {
  /// Set the sidebar width, clamped to 100%, and schedule the state save.
  pub fn set_sidebar_width_percent(&mut self, percent: u8) {
    self.runtime_state.sidebar_width_percent = percent.min(100);
    self.schedule_state_save(PersistedRuntimeState::sidebar_width_percent(
      self.runtime_state.sidebar_width_percent,
    ));
  }

  /// Set the playbar height, clamped to `MAX_PLAYBAR_ROWS`, and schedule the save.
  pub fn set_playbar_height_rows(&mut self, rows: u16) {
    self.runtime_state.playbar_height_rows = rows.min(crate::core::limits::MAX_PLAYBAR_ROWS);
    self.schedule_state_save(PersistedRuntimeState::playbar_height_rows(
      self.runtime_state.playbar_height_rows,
    ));
  }

  /// Set the library split height, clamped to 100%, and schedule the save.
  pub fn set_library_height_percent(&mut self, percent: u8) {
    self.runtime_state.library_height_percent = percent.min(100);
    self.schedule_state_save(PersistedRuntimeState::library_height_percent(
      self.runtime_state.library_height_percent,
    ));
  }

  pub fn pane_layout(&self) -> &PaneNode {
    &self.runtime_state.pane_layout
  }

  pub fn pane_open(&self, pane: Pane) -> bool {
    self.runtime_state.pane_layout.contains(pane)
  }

  /// Move the focus to `Main`, or to the first pane while `Main` is hidden, when the focused pane is gone.
  pub fn keep_pane_focus_in_layout(&mut self) {
    let tree = &self.runtime_state.pane_layout;
    if !tree.contains(self.view.focused_pane) {
      self.view.focused_pane = if tree.contains(Pane::Main) {
        Pane::Main
      } else {
        tree.panes()[0]
      };
    }
  }

  /// Run `change` on the pane layout and save it when it reports a change.
  pub fn change_pane_layout(&mut self, change: impl FnOnce(&mut PaneNode) -> bool) -> bool {
    if !change(&mut self.runtime_state.pane_layout) {
      return false;
    }
    self.keep_pane_focus_in_layout();
    self.schedule_state_save(PersistedRuntimeState::pane_layout(
      &self.runtime_state.pane_layout,
    ));
    true
  }

  #[cfg_attr(not(feature = "gui"), allow(dead_code))]
  pub fn gui_pane_layout(&self) -> &GuiLayout {
    &self.runtime_state.gui_pane_layout
  }

  /// Store and save the GUI layout, or the default for `None`; false when nothing changed.
  #[cfg_attr(not(feature = "gui"), allow(dead_code))]
  pub fn set_gui_pane_layout(&mut self, layout: Option<GuiLayout>) -> bool {
    let layout = layout.map_or_else(GuiLayout::default, |layout| layout.sanitized());
    if layout == self.runtime_state.gui_pane_layout {
      return false;
    }
    self.runtime_state.gui_pane_layout = layout;
    self.bump_display(DisplayDomain::PaneLayout);
    self.schedule_state_save(PersistedRuntimeState::gui_pane_layout(
      &self.runtime_state.gui_pane_layout,
    ));
    true
  }

  /// Every pane back to its configured default, or the runtime default.
  pub fn reset_layout(&mut self) {
    let defaults = RuntimeState::default();
    let behavior = &self.user_config.behavior;
    let sidebar = behavior
      .sidebar_width_percent
      .unwrap_or(defaults.sidebar_width_percent);
    let playbar = behavior
      .playbar_height_rows
      .unwrap_or(defaults.playbar_height_rows);
    let library = behavior
      .library_height_percent
      .unwrap_or(defaults.library_height_percent);
    self.set_sidebar_width_percent(sidebar);
    self.set_playbar_height_rows(playbar);
    self.set_library_height_percent(library);
  }
}
