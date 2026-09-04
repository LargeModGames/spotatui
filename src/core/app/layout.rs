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
