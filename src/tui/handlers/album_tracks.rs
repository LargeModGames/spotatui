use super::common_key_events;
use crate::core::app::{AlbumTableContext, App, RecommendationsContext};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::{
  model::{PlayContextId, PlayableId},
  prelude::*,
};

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => match app.album_table_context {
      AlbumTableContext::Full => {
        if let Some(selected_album) = &app.selected_album_full {
          let next_index = common_key_events::on_down_press_handler(
            &selected_album.album.tracks.items,
            Some(app.saved_album_tracks_index),
          );
          app.saved_album_tracks_index = next_index;
        };
      }
      AlbumTableContext::Simplified => {
        if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
          let next_index = common_key_events::on_down_press_handler(
            &selected_album_simplified.tracks.items,
            Some(selected_album_simplified.selected_index),
          );
          selected_album_simplified.selected_index = next_index;
        }
      }
    },
    k if common_key_events::up_event(k, &app.user_config.keys) => match app.album_table_context {
      AlbumTableContext::Full => {
        if let Some(selected_album) = &app.selected_album_full {
          let next_index = common_key_events::on_up_press_handler(
            &selected_album.album.tracks.items,
            Some(app.saved_album_tracks_index),
          );
          app.saved_album_tracks_index = next_index;
        };
      }
      AlbumTableContext::Simplified => {
        if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
          let next_index = common_key_events::on_up_press_handler(
            &selected_album_simplified.tracks.items,
            Some(selected_album_simplified.selected_index),
          );
          selected_album_simplified.selected_index = next_index;
        }
      }
    },
    k if common_key_events::high_event(k) => handle_high_event(app),
    k if common_key_events::middle_event(k) => handle_middle_event(app),
    k if common_key_events::low_event(k) => handle_low_event(app),
    Key::Char('s') => handle_save_event(app),
    Key::Char('w') => handle_save_album_event(app),
    Key::Enter => match app.album_table_context {
      AlbumTableContext::Full => {
        if let Some(selected_album) = app.selected_album_full.clone() {
          let context_id = Some(PlayContextId::Album(selected_album.album.id.into_static()));
          app.dispatch(IoEvent::StartPlayback(
            context_id,
            None,
            Some(app.saved_album_tracks_index),
          ));
        };
      }
      AlbumTableContext::Simplified => {
        if let Some(selected_album_simplified) = &app.selected_album_simplified.clone() {
          let context_id = selected_album_simplified
            .album
            .id
            .clone()
            .map(|id| PlayContextId::Album(id.into_static()));
          app.dispatch(IoEvent::StartPlayback(
            context_id,
            None,
            Some(selected_album_simplified.selected_index),
          ));
        };
      }
    },
    //recommended playlist based on selected track
    Key::Char('r') => {
      handle_recommended_tracks(app);
    }
    _ if key == app.user_config.keys.add_item_to_queue => match app.album_table_context {
      AlbumTableContext::Full => {
        if let Some(selected_album) = app.selected_album_full.clone() {
          if let Some(track) = selected_album
            .album
            .tracks
            .items
            .get(app.saved_album_tracks_index)
          {
            if let Some(track_id) = &track.id {
              app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
                track_id.clone().into_static(),
              )));
            }
          }
        };
      }
      AlbumTableContext::Simplified => {
        if let Some(selected_album_simplified) = &app.selected_album_simplified.clone() {
          if let Some(track) = selected_album_simplified
            .tracks
            .items
            .get(selected_album_simplified.selected_index)
          {
            if let Some(track_id) = &track.id {
              app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
                track_id.clone().into_static(),
              )));
            }
          }
        };
      }
    },
    _ => {}
  };
}

fn handle_high_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      let next_index = common_key_events::on_high_press_handler();
      app.saved_album_tracks_index = next_index;
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
        let next_index = common_key_events::on_high_press_handler();
        selected_album_simplified.selected_index = next_index;
      }
    }
  }
}

fn handle_middle_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = &app.selected_album_full {
        let next_index =
          common_key_events::on_middle_press_handler(&selected_album.album.tracks.items);
        app.saved_album_tracks_index = next_index;
      };
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
        let next_index =
          common_key_events::on_middle_press_handler(&selected_album_simplified.tracks.items);
        selected_album_simplified.selected_index = next_index;
      }
    }
  }
}

fn handle_low_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = &app.selected_album_full {
        let next_index =
          common_key_events::on_low_press_handler(&selected_album.album.tracks.items);
        app.saved_album_tracks_index = next_index;
      };
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
        let next_index =
          common_key_events::on_low_press_handler(&selected_album_simplified.tracks.items);
        selected_album_simplified.selected_index = next_index;
      }
    }
  }
}

fn handle_recommended_tracks(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(albums) = &app.library.clone().saved_albums.get_results(None) {
        if let Some(selected_album) = albums.items.get(app.album_list_index) {
          if let Some(track) = &selected_album
            .album
            .tracks
            .items
            .get(app.saved_album_tracks_index)
          {
            if let Some(id) = &track.id {
              app.recommendations_context = Some(RecommendationsContext::Song);
              app.recommendations_seed = track.name.clone();
              app.get_recommendations_for_track_id(id.id().to_string());
            }
          }
        }
      }
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &app.selected_album_simplified.clone() {
        if let Some(track) = &selected_album_simplified
          .tracks
          .items
          .get(selected_album_simplified.selected_index)
        {
          if let Some(id) = &track.id {
            app.recommendations_context = Some(RecommendationsContext::Song);
            app.recommendations_seed = track.name.clone();
            app.get_recommendations_for_track_id(id.id().to_string());
          }
        }
      };
    }
  }
}

fn handle_save_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = app.selected_album_full.clone() {
        if let Some(selected_track) = selected_album
          .album
          .tracks
          .items
          .get(app.saved_album_tracks_index)
        {
          if let Some(track_id) = &selected_track.id {
            app.dispatch(IoEvent::ToggleSaveTrack(PlayableId::Track(
              track_id.clone().into_static(),
            )));
          };
        };
      };
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = app.selected_album_simplified.clone() {
        if let Some(selected_track) = selected_album_simplified
          .tracks
          .items
          .get(selected_album_simplified.selected_index)
        {
          if let Some(track_id) = &selected_track.id {
            app.dispatch(IoEvent::ToggleSaveTrack(PlayableId::Track(
              track_id.clone().into_static(),
            )));
          };
        };
      };
    }
  }
}

fn handle_save_album_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = app.selected_album_full.clone() {
        let album_id = selected_album.album.id.clone();
        app.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id.into_static()));
      };
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = app.selected_album_simplified.clone() {
        if let Some(album_id) = selected_album_simplified.album.id {
          app.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id.into_static()));
        };
      };
    }
  }
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
