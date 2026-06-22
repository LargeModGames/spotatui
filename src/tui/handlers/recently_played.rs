use super::common_key_events;
use crate::core::app::App;
use crate::core::app::RecommendationsContext;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::idtypes::PlayableId;
use rspotify::model::TrackId;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(recently_played_result) = &app.recently_played.result {
        let next_index = common_key_events::on_down_press_handler(
          &recently_played_result.items,
          Some(app.recently_played.index),
        );
        app.recently_played.index = next_index;
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
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
          if let Some(ref id_str) = selected_track.id {
            if let Ok(track_id) = TrackId::from_id(id_str.as_str()) {
              app.dispatch(IoEvent::ToggleSaveTrack(PlayableId::Track(
                track_id.into_static(),
              )));
            }
          };
        };
      };
    }
    Key::Char('w') => open_add_to_playlist_for_selected_recent_track(app),
    Key::Enter => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        let selected = app.recently_played.index;
        // Build uri list while tracking the remapped offset for the selected track.
        // Tracks without a valid id (e.g. local files) are omitted from the uri list,
        // so the offset into the resulting vec must be recomputed, not taken verbatim
        // from `app.recently_played.index`.
        let mut remapped_offset: Option<usize> = None;
        let mut uri_index = 0usize;
        let track_uris: Vec<PlayableId<'static>> = recently_played_result
          .items
          .iter()
          .enumerate()
          .filter_map(|(orig_index, item)| {
            let playable = item
              .id
              .as_ref()
              .and_then(|id_str| TrackId::from_id(id_str.as_str()).ok())
              .map(|track_id| PlayableId::Track(track_id.into_static()));
            if playable.is_some() {
              if orig_index == selected {
                remapped_offset = Some(uri_index);
              }
              uri_index += 1;
            }
            playable
          })
          .collect();

        app.dispatch(IoEvent::StartPlayback(
          None,
          Some(track_uris),
          remapped_offset,
        ));
      };
    }
    Key::Char('r') => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        if let Some(selected_track) = recently_played_result.items.get(app.recently_played.index) {
          if let Some(ref id_str) = selected_track.id {
            app.recommendations_context = Some(RecommendationsContext::Song);
            app.recommendations_seed = selected_track.name.clone();
            app.get_recommendations_for_track_id(id_str.clone());
          };
        };
      };
    }
    _ if key == app.user_config.keys.add_item_to_queue => {
      if let Some(recently_played_result) = &app.recently_played.result.clone() {
        if let Some(selected_track) = recently_played_result.items.get(app.recently_played.index) {
          if let Some(ref id_str) = selected_track.id {
            if let Ok(track_id) = TrackId::from_id(id_str.as_str()) {
              app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
                track_id.into_static(),
              )));
            }
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

  let track_id = selected_track
    .id
    .as_ref()
    .and_then(|id_str| TrackId::from_id(id_str.as_str()).ok())
    .map(|id| id.into_static());
  app.begin_add_track_to_playlist_flow(track_id, selected_track.name.clone());
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
