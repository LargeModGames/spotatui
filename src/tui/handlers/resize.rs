use crate::core::app::App;

const SIDEBAR_STEP: u8 = 5;
const PLAYBAR_STEP: u16 = 1;
const LIBRARY_STEP: u8 = 5;

/// Decrease sidebar width by SIDEBAR_STEP percent (minimum 0%).
pub fn decrease_sidebar_width(app: &mut App) {
  let percent = app
    .runtime_state
    .sidebar_width_percent
    .saturating_sub(SIDEBAR_STEP);
  app.set_sidebar_width_percent(percent);
}

/// Increase sidebar width by SIDEBAR_STEP percent (maximum 100%).
pub fn increase_sidebar_width(app: &mut App) {
  let percent = app
    .runtime_state
    .sidebar_width_percent
    .saturating_add(SIDEBAR_STEP);
  app.set_sidebar_width_percent(percent);
}

/// Decrease playbar height by PLAYBAR_STEP rows (minimum 0 = hidden).
pub fn decrease_playbar_height(app: &mut App) {
  let rows = app
    .runtime_state
    .playbar_height_rows
    .saturating_sub(PLAYBAR_STEP);
  app.set_playbar_height_rows(rows);
}

/// Increase playbar height by PLAYBAR_STEP rows (capped at MAX_PLAYBAR_ROWS).
pub fn increase_playbar_height(app: &mut App) {
  let rows = app
    .runtime_state
    .playbar_height_rows
    .saturating_add(PLAYBAR_STEP);
  app.set_playbar_height_rows(rows);
}

/// Decrease the library section height within the sidebar (minimum 0% = hidden).
pub fn decrease_library_height(app: &mut App) {
  let percent = app
    .runtime_state
    .library_height_percent
    .saturating_sub(LIBRARY_STEP);
  app.set_library_height_percent(percent);
}

/// Increase the library section height within the sidebar (maximum 100%).
pub fn increase_library_height(app: &mut App) {
  let percent = app
    .runtime_state
    .library_height_percent
    .saturating_add(LIBRARY_STEP);
  app.set_library_height_percent(percent);
}

/// Reset all pane sizes to configured defaults, or runtime defaults.
pub fn reset_layout(app: &mut App) {
  app.reset_layout();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::limits::MAX_PLAYBAR_ROWS;
  use crate::core::state::RuntimeState;

  #[test]
  fn decrease_sidebar_reduces_width_by_step() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 20;
    decrease_sidebar_width(&mut app);
    assert_eq!(app.runtime_state.sidebar_width_percent, 15);
  }

  #[test]
  fn decrease_sidebar_clamps_at_zero() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 3;
    decrease_sidebar_width(&mut app);
    assert_eq!(app.runtime_state.sidebar_width_percent, 0);
  }

  #[test]
  fn increase_sidebar_increases_width_by_step() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 20;
    increase_sidebar_width(&mut app);
    assert_eq!(app.runtime_state.sidebar_width_percent, 25);
  }

  #[test]
  fn increase_sidebar_clamps_at_100() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 98;
    increase_sidebar_width(&mut app);
    assert_eq!(app.runtime_state.sidebar_width_percent, 100);
  }

  #[test]
  fn sidebar_can_be_fully_hidden() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 5;
    decrease_sidebar_width(&mut app);
    assert_eq!(app.runtime_state.sidebar_width_percent, 0);
  }

  #[test]
  fn decrease_playbar_reduces_height_by_step() {
    let mut app = App::default();
    app.runtime_state.playbar_height_rows = 6;
    decrease_playbar_height(&mut app);
    assert_eq!(app.runtime_state.playbar_height_rows, 5);
  }

  #[test]
  fn decrease_playbar_clamps_at_zero() {
    let mut app = App::default();
    app.runtime_state.playbar_height_rows = 0;
    decrease_playbar_height(&mut app);
    assert_eq!(app.runtime_state.playbar_height_rows, 0);
  }

  #[test]
  fn increase_playbar_increases_height_by_step() {
    let mut app = App::default();
    app.runtime_state.playbar_height_rows = 6;
    increase_playbar_height(&mut app);
    assert_eq!(app.runtime_state.playbar_height_rows, 7);
  }

  #[test]
  fn increase_playbar_clamps_at_max() {
    let mut app = App::default();
    app.runtime_state.playbar_height_rows = MAX_PLAYBAR_ROWS;
    increase_playbar_height(&mut app);
    assert_eq!(app.runtime_state.playbar_height_rows, MAX_PLAYBAR_ROWS);
  }

  #[test]
  fn playbar_can_be_hidden() {
    let mut app = App::default();
    app.runtime_state.playbar_height_rows = 1;
    decrease_playbar_height(&mut app);
    assert_eq!(app.runtime_state.playbar_height_rows, 0);
  }

  #[test]
  fn decrease_library_reduces_height_by_step() {
    let mut app = App::default();
    app.runtime_state.library_height_percent = 30;
    decrease_library_height(&mut app);
    assert_eq!(app.runtime_state.library_height_percent, 25);
  }

  #[test]
  fn increase_library_increases_height_by_step() {
    let mut app = App::default();
    app.runtime_state.library_height_percent = 30;
    increase_library_height(&mut app);
    assert_eq!(app.runtime_state.library_height_percent, 35);
  }

  #[test]
  fn library_can_be_fully_hidden() {
    let mut app = App::default();
    app.runtime_state.library_height_percent = 3;
    decrease_library_height(&mut app);
    assert_eq!(app.runtime_state.library_height_percent, 0);
  }

  #[test]
  fn reset_layout_restores_runtime_defaults_without_configured_defaults() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 50;
    app.runtime_state.playbar_height_rows = 0;
    app.runtime_state.library_height_percent = 80;
    reset_layout(&mut app);
    let defaults = RuntimeState::default();
    assert_eq!(
      app.runtime_state.sidebar_width_percent,
      defaults.sidebar_width_percent
    );
    assert_eq!(
      app.runtime_state.playbar_height_rows,
      defaults.playbar_height_rows
    );
    assert_eq!(
      app.runtime_state.library_height_percent,
      defaults.library_height_percent
    );
  }

  #[test]
  fn reset_layout_prefers_configured_defaults() {
    let mut app = App::default();
    app.runtime_state.sidebar_width_percent = 50;
    app.runtime_state.playbar_height_rows = 0;
    app.runtime_state.library_height_percent = 80;
    app.user_config.behavior.sidebar_width_percent = Some(35);
    app.user_config.behavior.playbar_height_rows = Some(9);
    app.user_config.behavior.library_height_percent = Some(45);
    reset_layout(&mut app);
    assert_eq!(app.runtime_state.sidebar_width_percent, 35);
    assert_eq!(app.runtime_state.playbar_height_rows, 9);
    assert_eq!(app.runtime_state.library_height_percent, 45);
  }

  #[test]
  fn reset_layout_clamps_configured_playbar_height_to_max() {
    let mut app = App::default();
    app.runtime_state.playbar_height_rows = 0;
    app.user_config.behavior.playbar_height_rows = Some(MAX_PLAYBAR_ROWS + 1);
    reset_layout(&mut app);
    assert_eq!(app.runtime_state.playbar_height_rows, MAX_PLAYBAR_ROWS);
  }
}
