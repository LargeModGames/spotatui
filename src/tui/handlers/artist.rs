use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::{App, Artist, ArtistBlock};
use crate::core::plugin_api::TrackInfo;
use crate::tui::event::Key;

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

/// The top track under the cursor.
fn selected_top_track(artist: &Artist) -> Option<TrackInfo> {
  artist
    .top_tracks
    .get(artist.selected_top_track_index)
    .cloned()
}

fn top_track_playback_request(artist: &Artist) -> (Vec<String>, Option<usize>) {
  common_key_events::uri_playback_request(
    artist.top_tracks.iter().map(|track| track.uri.clone()),
    artist.selected_top_track_index,
  )
}

/// The album id under the artist page's album cursor.
fn selected_album_id(artist: &Artist) -> Option<String> {
  artist
    .albums
    .items
    .get(artist.selected_album_index)?
    .id
    .clone()
}

/// `None` when the row is missing or has no id: the related-artists list is
/// often empty while the cursor still reports 0.
fn selected_related_artist(artist: &Artist) -> Option<(String, String)> {
  let related = artist
    .related_artists
    .get(artist.selected_related_artist_index)?;
  Some((related.id.clone()?, related.name.clone()))
}

fn handle_recommend_event_on_selected_block(app: &mut App) {
  let Some(artist) = &app.artist else {
    return;
  };
  match artist.artist_selected_block {
    ArtistBlock::TopTracks => {
      // `track` is already a domain TrackInfo (Artist.top_tracks was
      // migrated), so seed recommendations with it directly.
      if let Some(track) = selected_top_track(artist) {
        app.apply(Action::RecommendFromTrack(track));
      }
    }
    ArtistBlock::RelatedArtists => {
      // ArtistInfo.id is Option<String>; only dispatch if an id is present to
      // avoid a seed-less recommendation call that returns garbage results.
      if let Some((id, name)) = selected_related_artist(artist) {
        app.apply(Action::RecommendFromArtist { id, name });
      }
    }
    _ => {}
  }
}

fn handle_enter_event_on_selected_block(app: &mut App) {
  let Some(artist) = &app.artist else {
    return;
  };
  match artist.artist_selected_block {
    ArtistBlock::TopTracks => {
      let (uris, offset) = top_track_playback_request(artist);
      app.apply(Action::PlayUris { uris, offset });
    }
    ArtistBlock::Albums => {
      // GetAlbum fetches a FullAlbum and sets AlbumTableContext::Full — do NOT
      // set track_table.context here, as GetAlbum does not use the track table.
      if let Some(id) = selected_album_id(artist) {
        app.apply(Action::Open(OpenTarget::Album {
          id,
          from_search: false,
        }));
      }
    }
    ArtistBlock::RelatedArtists => {
      if let Some((id, name)) = selected_related_artist(artist) {
        app.apply(Action::Open(OpenTarget::Artist { id, name }));
      }
    }
    ArtistBlock::Empty => {}
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
        ArtistBlock::TopTracks => {
          if let Some(track) = app.artist.as_ref().and_then(selected_top_track) {
            app.apply(Action::OpenAddTrackDialogFor {
              track_id: track.id,
              track_name: track.name,
            });
          }
        }
        ArtistBlock::Albums => {
          if let Some(id) = artist_page_album_id(app) {
            app.apply(Action::SaveAlbum(id));
          }
        }
        ArtistBlock::RelatedArtists => {
          if let Some((id, _)) = artist_page_related_artist(app) {
            app.apply(Action::FollowArtist(id));
          }
        }
        _ => (),
      },
      Key::Char('D') => match artist.artist_selected_block {
        ArtistBlock::Albums => {
          if let Some(id) = artist_page_album_id(app) {
            app.apply(Action::UnsaveAlbum(id));
          }
        }
        ArtistBlock::RelatedArtists => {
          if let Some((id, _)) = artist_page_related_artist(app) {
            app.apply(Action::UnfollowArtist(id));
          }
        }
        _ => (),
      },
      _ if key == app.user_config.keys.add_item_to_queue => {
        let track = app
          .artist
          .as_ref()
          .filter(|artist| artist.artist_selected_block == ArtistBlock::TopTracks)
          .and_then(selected_top_track);
        if let Some(track) = track {
          app.apply(Action::QueueTrack(track));
        }
      }
      _ => {}
    };
  }
}

/// Re-read from the whole `App`: the key match's borrow of `app.artist` has
/// ended by the time an arm body runs.
fn artist_page_album_id(app: &App) -> Option<String> {
  app.artist.as_ref().and_then(selected_album_id)
}

fn artist_page_related_artist(app: &App) -> Option<(String, String)> {
  app.artist.as_ref().and_then(selected_related_artist)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::ArtistInfo;

  fn artist_page(related_artists: Vec<ArtistInfo>) -> Artist {
    Artist {
      artist_id: "artist1".to_string(),
      artist_name: "First".to_string(),
      albums: Paged::default(),
      related_artists,
      top_tracks: vec![],
      selected_album_index: 0,
      selected_related_artist_index: 0,
      selected_top_track_index: 0,
      artist_hovered_block: ArtistBlock::TopTracks,
      artist_selected_block: ArtistBlock::RelatedArtists,
    }
  }

  #[test]
  fn on_esc() {
    let mut app = App::default();

    handler(Key::Esc, &mut app);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
  }

  #[test]
  fn related_artist_lookup_on_an_empty_list_resolves_nothing() {
    // An empty related-artists list with the cursor at 0 used to index out of bounds.
    assert_eq!(selected_related_artist(&artist_page(vec![])), None);
  }

  #[test]
  fn related_artist_lookup_returns_the_row_identity() {
    let page = artist_page(vec![ArtistInfo {
      id: Some("artist2".to_string()),
      uri: Some("spotify:artist:artist2".to_string()),
      name: "Second".to_string(),
      image_url: None,
    }]);

    assert_eq!(
      selected_related_artist(&page),
      Some(("artist2".to_string(), "Second".to_string()))
    );
  }

  #[test]
  fn related_artist_lookup_without_an_id_resolves_nothing() {
    let page = artist_page(vec![ArtistInfo {
      id: None,
      uri: None,
      name: "Unknown".to_string(),
      image_url: None,
    }]);

    assert_eq!(selected_related_artist(&page), None);
  }
}
