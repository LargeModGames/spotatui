use super::common_key_events;
use crate::core::app::{ActiveBlock, App};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use crate::tui::ui::player::PlaybarControl;
use rspotify::model::{context::CurrentPlaybackContext, PlayableId, PlayableItem};

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::up_event(k) => {
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
      add_currently_playing_track_to_playlist(app);
      true
    }
    _ => false,
  }
}

pub(crate) fn handle_control(control: PlaybarControl, app: &mut App) {
  match control {
    PlaybarControl::Prev => app.previous_track(),
    PlaybarControl::PlayPause => app.toggle_playback(),
    PlaybarControl::Next => app.next_track(),
    PlaybarControl::Shuffle => app.shuffle(),
    PlaybarControl::Repeat => app.repeat(),
    PlaybarControl::Like => toggle_like_currently_playing_item(app),
    PlaybarControl::VolumeDown => app.decrease_volume(),
    PlaybarControl::VolumeUp => app.increase_volume(),
  }
}

pub(crate) fn toggle_like_currently_playing_item(app: &mut App) {
  if let Some(CurrentPlaybackContext {
    item: Some(item), ..
  }) = app.current_playback_context.to_owned()
  {
    match item {
      PlayableItem::Track(track) => {
        if let Some(track_id) = track.id {
          app.dispatch(IoEvent::ToggleSaveTrack(PlayableId::Track(
            track_id.into_static(),
          )));
        }
      }
      PlayableItem::Episode(episode) => {
        app.dispatch(IoEvent::ToggleSaveTrack(PlayableId::Episode(
          episode.id.into_static(),
        )));
      }
      _ => {}
    };
  };
}

pub(crate) fn add_currently_playing_track_to_playlist(app: &mut App) {
  if let Some(CurrentPlaybackContext {
    item: Some(item), ..
  }) = app.current_playback_context.to_owned()
  {
    match item {
      PlayableItem::Track(track) => {
        let track_id = track.id.map(|id| id.into_static());
        app.begin_add_track_to_playlist_flow(track_id, track.name);
      }
      PlayableItem::Episode(_) => {
        app.set_status_message("Only tracks can be added to playlists".to_string(), 4);
      }
      _ => {}
    };
  } else {
    app.set_status_message("No track currently playing".to_string(), 4);
  }
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
