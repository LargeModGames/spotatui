use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::{ActiveBlock, App};
use crate::tui::event::Key;
use crate::tui::ui::player::PlaybarControl;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::MyPlaylists));
    }
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

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Up, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::MyPlaylists);
  }

  #[test]
  fn on_add_current_track_without_playback_sets_status_message() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Char('w'), &mut app);

    assert_eq!(
      app.status_message.as_deref(),
      Some("No track currently playing")
    );
  }
}
