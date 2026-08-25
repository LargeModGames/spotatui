use crate::core::action::Action;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Char('s') => {
      app.apply(Action::ToggleSaveCurrentItem);
    }
    k if k == app.user_config.keys.back => {
      app.pop_navigation_stack();
    }
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::full_track;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    CurrentlyPlayingType, Device, DeviceType, PlayableItem, RepeatState,
  };
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn playing_track_context() -> CurrentPlaybackContext {
    CurrentPlaybackContext {
      device: Device {
        id: Some("dev-test".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Test Device".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(50),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: chrono::Utc::now(),
      progress: Some(chrono::Duration::milliseconds(0)),
      is_playing: true,
      item: Some(PlayableItem::Track(full_track(
        "4uLU6hMCjMI75M1A2tKUQC",
        "Test Song",
      ))),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  #[test]
  fn s_toggles_the_playing_track_in_the_library() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(playing_track_context());

    handler(Key::Char('s'), &mut app);

    match rx.try_recv() {
      Ok(IoEvent::ToggleSaveTrack(uri)) => {
        assert_eq!(uri, "spotify:track:4uLU6hMCjMI75M1A2tKUQC")
      }
      _ => panic!("expected ToggleSaveTrack for the playing track"),
    }
  }
}
