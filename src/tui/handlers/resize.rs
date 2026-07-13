use crate::core::app::App;

const SIDEBAR_STEP: u8 = 5;
const MAX_PLAYBAR_ROWS: u16 = 50;
const PLAYBAR_STEP: u16 = 1;
const LIBRARY_STEP: u8 = 5;

// Default layout values (must match UserConfig::new() defaults)
const DEFAULT_SIDEBAR_WIDTH: u8 = 20;
const DEFAULT_PLAYBAR_HEIGHT: u16 = 6;
const DEFAULT_LIBRARY_HEIGHT: u8 = 30;

/// Decrease sidebar width by SIDEBAR_STEP percent (minimum 0%).
pub fn decrease_sidebar_width(app: &mut App) {
  app.user_config.behavior.sidebar_width_percent = app
    .user_config
    .behavior
    .sidebar_width_percent
    .saturating_sub(SIDEBAR_STEP);
  app.schedule_config_save();
}

/// Increase sidebar width by SIDEBAR_STEP percent (maximum 100%).
pub fn increase_sidebar_width(app: &mut App) {
  app.user_config.behavior.sidebar_width_percent = app
    .user_config
    .behavior
    .sidebar_width_percent
    .saturating_add(SIDEBAR_STEP)
    .min(100);
  app.schedule_config_save();
}

/// Decrease playbar height by PLAYBAR_STEP rows (minimum 0 = hidden).
pub fn decrease_playbar_height(app: &mut App) {
  app.user_config.behavior.playbar_height_rows = app
    .user_config
    .behavior
    .playbar_height_rows
    .saturating_sub(PLAYBAR_STEP);
  app.schedule_config_save();
}

/// Increase playbar height by PLAYBAR_STEP rows (capped at MAX_PLAYBAR_ROWS).
pub fn increase_playbar_height(app: &mut App) {
  app.user_config.behavior.playbar_height_rows = app
    .user_config
    .behavior
    .playbar_height_rows
    .saturating_add(PLAYBAR_STEP)
    .min(MAX_PLAYBAR_ROWS);
  app.schedule_config_save();
}

/// Decrease the library section height within the sidebar (minimum 0% = hidden).
pub fn decrease_library_height(app: &mut App) {
  app.user_config.behavior.library_height_percent = app
    .user_config
    .behavior
    .library_height_percent
    .saturating_sub(LIBRARY_STEP);
  app.schedule_config_save();
}

/// Increase the library section height within the sidebar (maximum 100%).
pub fn increase_library_height(app: &mut App) {
  app.user_config.behavior.library_height_percent = app
    .user_config
    .behavior
    .library_height_percent
    .saturating_add(LIBRARY_STEP)
    .min(100);
  app.schedule_config_save();
}

/// Reset all pane sizes to their defaults.
pub fn reset_layout(app: &mut App) {
  app.user_config.behavior.sidebar_width_percent = DEFAULT_SIDEBAR_WIDTH;
  app.user_config.behavior.playbar_height_rows = DEFAULT_PLAYBAR_HEIGHT;
  app.user_config.behavior.library_height_percent = DEFAULT_LIBRARY_HEIGHT;
  app.schedule_config_save();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decrease_sidebar_reduces_width_by_step() {
    let mut app = App::default();
    app.user_config.behavior.sidebar_width_percent = 20;
    decrease_sidebar_width(&mut app);
    assert_eq!(app.user_config.behavior.sidebar_width_percent, 15);
  }

  #[test]
  fn decrease_sidebar_clamps_at_zero() {
    let mut app = App::default();
    app.user_config.behavior.sidebar_width_percent = 3;
    decrease_sidebar_width(&mut app);
    assert_eq!(app.user_config.behavior.sidebar_width_percent, 0);
  }

  #[test]
  fn increase_sidebar_increases_width_by_step() {
    let mut app = App::default();
    app.user_config.behavior.sidebar_width_percent = 20;
    increase_sidebar_width(&mut app);
    assert_eq!(app.user_config.behavior.sidebar_width_percent, 25);
  }

  #[test]
  fn increase_sidebar_clamps_at_100() {
    let mut app = App::default();
    app.user_config.behavior.sidebar_width_percent = 98;
    increase_sidebar_width(&mut app);
    assert_eq!(app.user_config.behavior.sidebar_width_percent, 100);
  }

  #[test]
  fn sidebar_can_be_fully_hidden() {
    let mut app = App::default();
    app.user_config.behavior.sidebar_width_percent = 5;
    decrease_sidebar_width(&mut app);
    assert_eq!(app.user_config.behavior.sidebar_width_percent, 0);
  }

  #[test]
  fn decrease_playbar_reduces_height_by_step() {
    let mut app = App::default();
    app.user_config.behavior.playbar_height_rows = 6;
    decrease_playbar_height(&mut app);
    assert_eq!(app.user_config.behavior.playbar_height_rows, 5);
  }

  #[test]
  fn decrease_playbar_clamps_at_zero() {
    let mut app = App::default();
    app.user_config.behavior.playbar_height_rows = 0;
    decrease_playbar_height(&mut app);
    assert_eq!(app.user_config.behavior.playbar_height_rows, 0);
  }

  #[test]
  fn increase_playbar_increases_height_by_step() {
    let mut app = App::default();
    app.user_config.behavior.playbar_height_rows = 6;
    increase_playbar_height(&mut app);
    assert_eq!(app.user_config.behavior.playbar_height_rows, 7);
  }

  #[test]
  fn increase_playbar_clamps_at_max() {
    let mut app = App::default();
    app.user_config.behavior.playbar_height_rows = MAX_PLAYBAR_ROWS;
    increase_playbar_height(&mut app);
    assert_eq!(
      app.user_config.behavior.playbar_height_rows,
      MAX_PLAYBAR_ROWS
    );
  }

  #[test]
  fn playbar_can_be_hidden() {
    let mut app = App::default();
    app.user_config.behavior.playbar_height_rows = 1;
    decrease_playbar_height(&mut app);
    assert_eq!(app.user_config.behavior.playbar_height_rows, 0);
  }

  #[test]
  fn decrease_library_reduces_height_by_step() {
    let mut app = App::default();
    app.user_config.behavior.library_height_percent = 30;
    decrease_library_height(&mut app);
    assert_eq!(app.user_config.behavior.library_height_percent, 25);
  }

  #[test]
  fn increase_library_increases_height_by_step() {
    let mut app = App::default();
    app.user_config.behavior.library_height_percent = 30;
    increase_library_height(&mut app);
    assert_eq!(app.user_config.behavior.library_height_percent, 35);
  }

  #[test]
  fn library_can_be_fully_hidden() {
    let mut app = App::default();
    app.user_config.behavior.library_height_percent = 3;
    decrease_library_height(&mut app);
    assert_eq!(app.user_config.behavior.library_height_percent, 0);
  }

  #[test]
  fn reset_layout_restores_all_defaults() {
    let mut app = App::default();
    app.user_config.behavior.sidebar_width_percent = 50;
    app.user_config.behavior.playbar_height_rows = 0;
    app.user_config.behavior.library_height_percent = 80;
    reset_layout(&mut app);
    assert_eq!(
      app.user_config.behavior.sidebar_width_percent,
      DEFAULT_SIDEBAR_WIDTH
    );
    assert_eq!(
      app.user_config.behavior.playbar_height_rows,
      DEFAULT_PLAYBAR_HEIGHT
    );
    assert_eq!(
      app.user_config.behavior.library_height_percent,
      DEFAULT_LIBRARY_HEIGHT
    );
  }
}
