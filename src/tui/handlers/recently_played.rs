use super::common_key_events;
use crate::core::app::App;
use crate::core::app::RecommendationsContext;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::idtypes::PlayableId;
use rspotify::prelude::Id;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k) => common_key_events::handle_left_event(app),
    k if common_key_events::down_event(k) => {
      if let Some(recently_played_result) = &app.recently_played.result {
        let next_index = common_key_events::on_down_press_handler(
          &recently_played_result.items,
          Some(app.recently_played.index),
        );
        app.recently_played.index = next_index;
      }
    }
    k if common_key_events::up_event(k) => {
      if let Some(recently_played_result) = &app.recently_played.result {
        let next_index = common_key_events::on_up_press_handler(
          &recently_played_result.items,
          Some(app.recently_played.index),
        );
        app.recently_played.index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if let Some(_recently_played_result) = &app.recently_played.result {
        let next_index = common_key_events::on_high_press_handler();
        app.recently_played.index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if let Some(recently_played_result) = &app.recently_played.result {
        let next_index = common_key_events::on_middle_press_handler(&recently_played_result.items);
        app.recently_played.index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if let Some(recently_played_result) = &app.recently_played.result {
        let next_index = common_key_events::on_low_press_handler(&recently_played_result.items);
        app.recently_played.index = next_index;
      }
    }
    Key::Char('s') => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        if let Some(selected_track) = recently_played_result.items.get(app.recently_played.index) {
          if let Some(track_id) = &selected_track.track.id {
            app.dispatch(IoEvent::ToggleSaveTrack(PlayableId::Track(
              track_id.clone().into_static(),
            )));
          };
        };
      };
    }
    Key::Char('w') => open_add_to_playlist_for_selected_recent_track(app),
    Key::Enter => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        let track_uris: Vec<PlayableId<'static>> = recently_played_result
          .items
          .iter()
          .filter_map(|item| {
            item
              .track
              .id
              .as_ref()
              .map(|track_id| PlayableId::Track(track_id.clone().into_static()))
          })
          .collect();

        app.dispatch(IoEvent::StartPlayback(
          None,
          Some(track_uris),
          Some(app.recently_played.index),
        ));
      };
    }
    Key::Char('r') => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        if let Some(selected_track) = recently_played_result.items.get(app.recently_played.index) {
          if let Some(track_id) = &selected_track.track.id {
            app.recommendations_context = Some(RecommendationsContext::Song);
            app.recommendations_seed = selected_track.track.name.clone();
            app.get_recommendations_for_track_id(track_id.id().to_string());
          };
        };
      };
    }
    _ if key == app.user_config.keys.add_item_to_queue => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        if let Some(selected_track) = recently_played_result.items.get(app.recently_played.index) {
          if let Some(track_id) = &selected_track.track.id {
            app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
              track_id.clone().into_static(),
            )));
          };
        };
      };
    }
    _ => {}
  };
}

fn open_add_to_playlist_for_selected_recent_track(app: &mut App) {
  let Some(recently_played_result) = &app.recently_played.result else {
    return;
  };
  let Some(selected_track) = recently_played_result.items.get(app.recently_played.index) else {
    return;
  };

  let track_id = selected_track.track.id.clone().map(|id| id.into_static());
  app.begin_add_track_to_playlist_flow(track_id, selected_track.track.name.clone());
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(
      Some(ActiveBlock::AlbumTracks),
      Some(ActiveBlock::AlbumTracks),
    );

    handler(Key::Left, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::Library);
  }

  #[test]
  fn on_esc() {
    let mut app = App::default();

    handler(Key::Esc, &mut app);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
  }
}
