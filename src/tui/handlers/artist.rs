use super::common_key_events;
use crate::core::app::{ActiveBlock, App, ArtistBlock, RecommendationsContext};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::{
  idtypes::{AlbumId, ArtistId, TrackId},
  PlayableId,
};

fn handle_down_press_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let next_index = common_key_events::on_down_press_handler(
          &artist.top_tracks,
          Some(artist.selected_top_track_index),
        );
        artist.selected_top_track_index = next_index;
      }
      ArtistBlock::Albums => {
        let next_index = common_key_events::on_down_press_handler(
          &artist.albums.items,
          Some(artist.selected_album_index),
        );
        artist.selected_album_index = next_index;
      }
      ArtistBlock::RelatedArtists => {
        let next_index = common_key_events::on_down_press_handler(
          &artist.related_artists,
          Some(artist.selected_related_artist_index),
        );
        artist.selected_related_artist_index = next_index;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_down_press_on_hovered_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_hovered_block {
      ArtistBlock::TopTracks => {
        artist.artist_hovered_block = ArtistBlock::Albums;
      }
      ArtistBlock::Albums => {
        artist.artist_hovered_block = ArtistBlock::RelatedArtists;
      }
      ArtistBlock::RelatedArtists => {
        artist.artist_hovered_block = ArtistBlock::TopTracks;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_up_press_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let next_index = common_key_events::on_up_press_handler(
          &artist.top_tracks,
          Some(artist.selected_top_track_index),
        );
        artist.selected_top_track_index = next_index;
      }
      ArtistBlock::Albums => {
        let next_index = common_key_events::on_up_press_handler(
          &artist.albums.items,
          Some(artist.selected_album_index),
        );
        artist.selected_album_index = next_index;
      }
      ArtistBlock::RelatedArtists => {
        let next_index = common_key_events::on_up_press_handler(
          &artist.related_artists,
          Some(artist.selected_related_artist_index),
        );
        artist.selected_related_artist_index = next_index;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_up_press_on_hovered_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_hovered_block {
      ArtistBlock::TopTracks => {
        artist.artist_hovered_block = ArtistBlock::RelatedArtists;
      }
      ArtistBlock::Albums => {
        artist.artist_hovered_block = ArtistBlock::TopTracks;
      }
      ArtistBlock::RelatedArtists => {
        artist.artist_hovered_block = ArtistBlock::Albums;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_high_press_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let next_index = common_key_events::on_high_press_handler();
        artist.selected_top_track_index = next_index;
      }
      ArtistBlock::Albums => {
        let next_index = common_key_events::on_high_press_handler();
        artist.selected_album_index = next_index;
      }
      ArtistBlock::RelatedArtists => {
        let next_index = common_key_events::on_high_press_handler();
        artist.selected_related_artist_index = next_index;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_middle_press_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let next_index = common_key_events::on_middle_press_handler(&artist.top_tracks);
        artist.selected_top_track_index = next_index;
      }
      ArtistBlock::Albums => {
        let next_index = common_key_events::on_middle_press_handler(&artist.albums.items);
        artist.selected_album_index = next_index;
      }
      ArtistBlock::RelatedArtists => {
        let next_index = common_key_events::on_middle_press_handler(&artist.related_artists);
        artist.selected_related_artist_index = next_index;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_low_press_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let next_index = common_key_events::on_low_press_handler(&artist.top_tracks);
        artist.selected_top_track_index = next_index;
      }
      ArtistBlock::Albums => {
        let next_index = common_key_events::on_low_press_handler(&artist.albums.items);
        artist.selected_album_index = next_index;
      }
      ArtistBlock::RelatedArtists => {
        let next_index = common_key_events::on_low_press_handler(&artist.related_artists);
        artist.selected_related_artist_index = next_index;
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_recommend_event_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist.clone() {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let selected_index = artist.selected_top_track_index;
        if let Some(track) = artist.top_tracks.get(selected_index) {
          // TrackInfo.id is Option<String> — wrap it in a Vec<String> if present.
          let track_id_list: Option<Vec<String>> = track.id.as_ref().map(|id| vec![id.clone()]);
          app.recommendations_context = Some(RecommendationsContext::Song);
          app.recommendations_seed = track.name.clone();
          // We cannot pass a FullTrack here (track_table is owned by another slice).
          // Pass None so recommendations still work without a prepended seed track.
          app.get_recommendations_for_seed(None, track_id_list, None);
        }
      }
      ArtistBlock::RelatedArtists => {
        let selected_index = artist.selected_related_artist_index;
        let related = &artist.related_artists[selected_index];
        // ArtistInfo.id is Option<String>; only dispatch if an id is present to
        // avoid a seed-less recommendation call that returns garbage results.
        if let Some(id_str) = &related.id {
          let artist_id_list: Option<Vec<String>> = Some(vec![id_str.clone()]);
          let artist_name = related.name.clone();

          app.recommendations_context = Some(RecommendationsContext::Artist);
          app.recommendations_seed = artist_name;
          app.get_recommendations_for_seed(artist_id_list, None, None);
        }
      }
      _ => {}
    }
  }
}

fn handle_enter_event_on_selected_block(app: &mut App) {
  if let Some(artist) = &mut app.artist.clone() {
    match artist.artist_selected_block {
      ArtistBlock::TopTracks => {
        let selected_index = artist.selected_top_track_index;
        // TrackInfo.id is Option<String>; re-parse to TrackId to build PlayableId.
        let top_tracks: Vec<PlayableId<'static>> = artist
          .top_tracks
          .iter()
          .filter_map(|track| {
            track
              .id
              .as_ref()
              .and_then(|id| TrackId::from_id(id.as_str()).ok())
              .map(|tid| PlayableId::Track(tid.into_static()))
          })
          .collect();
        app.dispatch(IoEvent::StartPlayback(
          None,
          Some(top_tracks),
          Some(selected_index),
        ));
      }
      ArtistBlock::Albums => {
        if let Some(selected_album) = artist
          .albums
          .items
          .get(artist.selected_album_index)
          .cloned()
        {
          // AlbumInfo.id is Option<String>; re-parse to AlbumId to dispatch GetAlbum.
          // GetAlbum fetches a FullAlbum and sets AlbumTableContext::Full — do NOT
          // set track_table.context here, as GetAlbum does not use the track table.
          if let Some(id_str) = &selected_album.id {
            if let Ok(album_id) = AlbumId::from_id(id_str.as_str()) {
              app.dispatch(IoEvent::GetAlbum(album_id.into_static()));
            }
          }
        }
      }
      ArtistBlock::RelatedArtists => {
        let selected_index = artist.selected_related_artist_index;
        let related = &artist.related_artists[selected_index];
        let artist_name = related.name.clone();
        // ArtistInfo.id is Option<String>; re-parse to ArtistId to navigate.
        if let Some(id_str) = &related.id {
          if let Ok(artist_id) = ArtistId::from_id(id_str.as_str()) {
            app.get_artist(artist_id.into_static(), artist_name);
          }
        }
      }
      ArtistBlock::Empty => {}
    }
  }
}

fn handle_enter_event_on_hovered_block(app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match artist.artist_hovered_block {
      ArtistBlock::TopTracks => artist.artist_selected_block = ArtistBlock::TopTracks,
      ArtistBlock::Albums => artist.artist_selected_block = ArtistBlock::Albums,
      ArtistBlock::RelatedArtists => artist.artist_selected_block = ArtistBlock::RelatedArtists,
      ArtistBlock::Empty => {}
    }
  }
}

pub fn handler(key: Key, app: &mut App) {
  if let Some(artist) = &mut app.artist {
    match key {
      Key::Esc => {
        artist.artist_selected_block = ArtistBlock::Empty;
      }
      k if common_key_events::down_event(k, &app.user_config.keys) => {
        if artist.artist_selected_block != ArtistBlock::Empty {
          handle_down_press_on_selected_block(app);
        } else {
          handle_down_press_on_hovered_block(app);
        }
      }
      k if common_key_events::up_event(k, &app.user_config.keys) => {
        if artist.artist_selected_block != ArtistBlock::Empty {
          handle_up_press_on_selected_block(app);
        } else {
          handle_up_press_on_hovered_block(app);
        }
      }
      k if common_key_events::left_event(k, &app.user_config.keys) => {
        artist.artist_selected_block = ArtistBlock::Empty;
        match artist.artist_hovered_block {
          ArtistBlock::TopTracks => common_key_events::handle_left_event(app),
          ArtistBlock::Albums => {
            artist.artist_hovered_block = ArtistBlock::TopTracks;
          }
          ArtistBlock::RelatedArtists => {
            artist.artist_hovered_block = ArtistBlock::Albums;
          }
          ArtistBlock::Empty => {}
        }
      }
      k if common_key_events::right_event(k, &app.user_config.keys) => {
        artist.artist_selected_block = ArtistBlock::Empty;
        handle_down_press_on_hovered_block(app);
      }
      k if common_key_events::high_event(k)
        && artist.artist_selected_block != ArtistBlock::Empty =>
      {
        handle_high_press_on_selected_block(app);
      }
      k if common_key_events::middle_event(k)
        && artist.artist_selected_block != ArtistBlock::Empty =>
      {
        handle_middle_press_on_selected_block(app);
      }
      k if common_key_events::low_event(k)
        && artist.artist_selected_block != ArtistBlock::Empty =>
      {
        handle_low_press_on_selected_block(app);
      }
      Key::Enter => {
        if artist.artist_selected_block != ArtistBlock::Empty {
          handle_enter_event_on_selected_block(app);
        } else {
          handle_enter_event_on_hovered_block(app);
        }
      }
      Key::Char('r') if artist.artist_selected_block != ArtistBlock::Empty => {
        handle_recommend_event_on_selected_block(app);
      }
      Key::Char('w') => match artist.artist_selected_block {
        ArtistBlock::TopTracks => open_add_to_playlist_for_selected_top_track(app),
        ArtistBlock::Albums => app.current_user_saved_album_add(ActiveBlock::ArtistBlock),
        ArtistBlock::RelatedArtists => app.user_follow_artists(ActiveBlock::ArtistBlock),
        _ => (),
      },
      Key::Char('D') => match artist.artist_selected_block {
        ArtistBlock::Albums => app.current_user_saved_album_delete(ActiveBlock::ArtistBlock),
        ArtistBlock::RelatedArtists => app.user_unfollow_artists(ActiveBlock::ArtistBlock),
        _ => (),
      },
      _ if key == app.user_config.keys.add_item_to_queue => {
        if let Some(artist) = &app.artist {
          if let ArtistBlock::TopTracks = artist.artist_selected_block {
            if let Some(track) = artist.top_tracks.get(artist.selected_top_track_index) {
              // TrackInfo.id is Option<String>; re-parse to TrackId for the IoEvent.
              if let Some(id_str) = &track.id {
                if let Ok(track_id) = TrackId::from_id(id_str.as_str()) {
                  app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
                    track_id.into_static(),
                  )));
                }
              }
            };
          }
        }
      }
      _ => {}
    };
  }
}

fn open_add_to_playlist_for_selected_top_track(app: &mut App) {
  let Some(artist) = &app.artist else {
    return;
  };
  let Some(track) = artist.top_tracks.get(artist.selected_top_track_index) else {
    return;
  };

  // TrackInfo.id is Option<String>; re-parse to TrackId for the flow.
  let track_id = track
    .id
    .as_ref()
    .and_then(|id| TrackId::from_id(id.as_str()).ok())
    .map(|tid| tid.into_static());
  app.begin_add_track_to_playlist_flow(track_id, track.name.clone());
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;

  #[test]
  fn on_esc() {
    let mut app = App::default();

    handler(Key::Esc, &mut app);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
  }
}
