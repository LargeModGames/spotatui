use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::App;
use crate::tui::event::Key;
use crate::tui::layout::{
  content_width_without_now_playing, now_playing_hidden_reason, NowPlayingHidden,
};
use crate::tui::ui::player::PlaybarControl;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::away_from_playbar_event(k, &app.user_config) => {
      common_key_events::leave_playbar(app);
    }
    Key::Enter => toggle_now_playing_panel(app),
    k => {
      handle_action_key(k, app);
    }
  };
}

pub(crate) fn handle_action_key(key: Key, app: &mut App) -> bool {
  match key {
    k if k == app.user_config.keys.like_track => {
      handle_control(PlaybarControl::Like, app);
      true
    }
    Key::Char('w') => {
      app.apply(Action::OpenAddPlayingTrackDialog);
      true
    }
    _ => false,
  }
}

/// Open or close the now playing panel. Opening it on a terminal too narrow to
/// fit it says so instead of doing nothing visible.
fn toggle_now_playing_panel(app: &mut App) {
  app.view.now_playing_panel = !app.view.now_playing_panel;
  if !app.view.now_playing_panel {
    return;
  }
  app.refresh_up_next();
  let too_narrow = content_width_without_now_playing(app).is_some_and(|width| {
    now_playing_hidden_reason(app, width) == Some(NowPlayingHidden::TooNarrow)
  });
  if too_narrow {
    app.set_status_message(app.strings().screens.now_playing_too_narrow, 4);
  }
}

pub(crate) fn handle_control(control: PlaybarControl, app: &mut App) {
  match control {
    PlaybarControl::Prev => app.apply(Action::PreviousTrack),
    PlaybarControl::PlayPause => app.apply(Action::TogglePlayback),
    PlaybarControl::Next => app.apply(Action::NextTrack),
    PlaybarControl::Shuffle => app.apply(Action::ToggleShuffle),
    PlaybarControl::Repeat => app.apply(Action::CycleRepeat),
    PlaybarControl::Like => app.apply(Action::ToggleSaveCurrentItem),
    PlaybarControl::VolumeDown => app.apply(Action::VolumeDown),
    PlaybarControl::VolumeUp => app.apply(Action::VolumeUp),
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Up, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::MyPlaylists);
  }

  fn app_on_playbar(width: u16, height: u16) -> App {
    let mut app = App::default();
    app.view.size = crate::core::geometry::Viewport { width, height };
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));
    app
  }

  #[test]
  fn enter_on_the_playbar_opens_and_closes_the_now_playing_panel() {
    let mut app = app_on_playbar(160, 50);

    handler(Key::Enter, &mut app);
    assert!(app.view.now_playing_panel);
    assert!(crate::tui::layout::compute_main_layout(&app)
      .unwrap()
      .now_playing
      .is_some());
    assert_eq!(app.status_message(), None);

    handler(Key::Enter, &mut app);
    assert!(!app.view.now_playing_panel);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::PlayBar);
  }

  #[test]
  fn opening_the_now_playing_panel_on_a_narrow_terminal_says_why_it_does_not_show() {
    let mut app = app_on_playbar(100, 40);

    handler(Key::Enter, &mut app);

    assert!(
      app.view.now_playing_panel,
      "it shows once the terminal is wider"
    );
    assert_eq!(
      app.status_message(),
      Some(app.strings().screens.now_playing_too_narrow)
    );
  }

  #[test]
  fn on_add_current_track_without_playback_sets_status_message() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Char('w'), &mut app);

    assert_eq!(app.status_message(), Some("No track currently playing"));
  }

  #[test]
  fn an_origin_from_another_route_is_ignored_when_leaving_the_playbar() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Home), Some(ActiveBlock::Home));
    common_key_events::remember_playbar_origin(&mut app);
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
    common_key_events::remember_playbar_origin(&mut app);
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));
    assert_eq!(
      app.pop_navigation_stack().map(|route| route.id),
      Some(RouteId::AlbumTracks)
    );

    handler(Key::Up, &mut app);

    let route = app.get_current_route();
    assert_eq!(route.id, RouteId::Home);
    assert_eq!(route.active_block, ActiveBlock::Empty);
    assert_eq!(route.hovered_block, ActiveBlock::MyPlaylists);
  }
}
