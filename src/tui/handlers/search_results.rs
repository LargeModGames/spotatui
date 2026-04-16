use super::common_key_events;
use crate::core::app::{
  ActiveBlock, App, DialogContext, RecommendationsContext, RouteId, SearchResultBlock,
  TrackTableContext,
};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::{model::PlayableId, prelude::*};

fn handle_down_press_on_selected_block(app: &mut App) {
  // Start selecting within the selected block
  match app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let Some(result) = &app.search_results.albums {
        let next_index = common_key_events::on_down_press_handler(
          &result.items,
          app.search_results.selected_album_index,
        );
        app.search_results.selected_album_index = Some(next_index);
      }
    }
    SearchResultBlock::SongSearch => {
      if let Some(result) = &app.search_results.tracks {
        let next_index = common_key_events::on_down_press_handler(
          &result.items,
          app.search_results.selected_tracks_index,
        );
        app.search_results.selected_tracks_index = Some(next_index);
      }
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(result) = &app.search_results.artists {
        let next_index = common_key_events::on_down_press_handler(
          &result.items,
          app.search_results.selected_artists_index,
        );
        app.search_results.selected_artists_index = Some(next_index);
      }
    }
    SearchResultBlock::PlaylistSearch => {
      if let Some(result) = &app.search_results.playlists {
        let next_index = common_key_events::on_down_press_handler(
          &result.items,
          app.search_results.selected_playlists_index,
        );
        app.search_results.selected_playlists_index = Some(next_index);
      }
    }
    SearchResultBlock::ShowSearch => {
      if let Some(result) = &app.search_results.shows {
        let next_index = common_key_events::on_down_press_handler(
          &result.items,
          app.search_results.selected_shows_index,
        );
        app.search_results.selected_shows_index = Some(next_index);
      }
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_down_press_on_hovered_block(app: &mut App) {
  match app.search_results.hovered_block {
    SearchResultBlock::AlbumSearch => {
      app.search_results.hovered_block = SearchResultBlock::ShowSearch;
    }
    SearchResultBlock::SongSearch => {
      app.search_results.hovered_block = SearchResultBlock::AlbumSearch;
    }
    SearchResultBlock::ArtistSearch => {
      app.search_results.hovered_block = SearchResultBlock::PlaylistSearch;
    }
    SearchResultBlock::PlaylistSearch => {
      app.search_results.hovered_block = SearchResultBlock::ShowSearch;
    }
    SearchResultBlock::ShowSearch => {
      app.search_results.hovered_block = SearchResultBlock::SongSearch;
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_up_press_on_selected_block(app: &mut App) {
  // Start selecting within the selected block
  match app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let Some(result) = &app.search_results.albums {
        let next_index = common_key_events::on_up_press_handler(
          &result.items,
          app.search_results.selected_album_index,
        );
        app.search_results.selected_album_index = Some(next_index);
      }
    }
    SearchResultBlock::SongSearch => {
      if let Some(result) = &app.search_results.tracks {
        let next_index = common_key_events::on_up_press_handler(
          &result.items,
          app.search_results.selected_tracks_index,
        );
        app.search_results.selected_tracks_index = Some(next_index);
      }
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(result) = &app.search_results.artists {
        let next_index = common_key_events::on_up_press_handler(
          &result.items,
          app.search_results.selected_artists_index,
        );
        app.search_results.selected_artists_index = Some(next_index);
      }
    }
    SearchResultBlock::PlaylistSearch => {
      if let Some(result) = &app.search_results.playlists {
        let next_index = common_key_events::on_up_press_handler(
          &result.items,
          app.search_results.selected_playlists_index,
        );
        app.search_results.selected_playlists_index = Some(next_index);
      }
    }
    SearchResultBlock::ShowSearch => {
      if let Some(result) = &app.search_results.shows {
        let next_index = common_key_events::on_up_press_handler(
          &result.items,
          app.search_results.selected_shows_index,
        );
        app.search_results.selected_shows_index = Some(next_index);
      }
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_up_press_on_hovered_block(app: &mut App) {
  match app.search_results.hovered_block {
    SearchResultBlock::AlbumSearch => {
      app.search_results.hovered_block = SearchResultBlock::SongSearch;
    }
    SearchResultBlock::SongSearch => {
      app.search_results.hovered_block = SearchResultBlock::ShowSearch;
    }
    SearchResultBlock::ArtistSearch => {
      app.search_results.hovered_block = SearchResultBlock::ShowSearch;
    }
    SearchResultBlock::PlaylistSearch => {
      app.search_results.hovered_block = SearchResultBlock::ArtistSearch;
    }
    SearchResultBlock::ShowSearch => {
      app.search_results.hovered_block = SearchResultBlock::AlbumSearch;
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_high_press_on_selected_block(app: &mut App) {
  match app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let Some(_result) = &app.search_results.albums {
        let next_index = common_key_events::on_high_press_handler();
        app.search_results.selected_album_index = Some(next_index);
      }
    }
    SearchResultBlock::SongSearch => {
      if let Some(_result) = &app.search_results.tracks {
        let next_index = common_key_events::on_high_press_handler();
        app.search_results.selected_tracks_index = Some(next_index);
      }
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(_result) = &app.search_results.artists {
        let next_index = common_key_events::on_high_press_handler();
        app.search_results.selected_artists_index = Some(next_index);
      }
    }
    SearchResultBlock::PlaylistSearch => {
      if let Some(_result) = &app.search_results.playlists {
        let next_index = common_key_events::on_high_press_handler();
        app.search_results.selected_playlists_index = Some(next_index);
      }
    }
    SearchResultBlock::ShowSearch => {
      if let Some(_result) = &app.search_results.shows {
        let next_index = common_key_events::on_high_press_handler();
        app.search_results.selected_shows_index = Some(next_index);
      }
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_middle_press_on_selected_block(app: &mut App) {
  match app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let Some(result) = &app.search_results.albums {
        let next_index = common_key_events::on_middle_press_handler(&result.items);
        app.search_results.selected_album_index = Some(next_index);
      }
    }
    SearchResultBlock::SongSearch => {
      if let Some(result) = &app.search_results.tracks {
        let next_index = common_key_events::on_middle_press_handler(&result.items);
        app.search_results.selected_tracks_index = Some(next_index);
      }
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(result) = &app.search_results.artists {
        let next_index = common_key_events::on_middle_press_handler(&result.items);
        app.search_results.selected_artists_index = Some(next_index);
      }
    }
    SearchResultBlock::PlaylistSearch => {
      if let Some(result) = &app.search_results.playlists {
        let next_index = common_key_events::on_middle_press_handler(&result.items);
        app.search_results.selected_playlists_index = Some(next_index);
      }
    }
    SearchResultBlock::ShowSearch => {
      if let Some(result) = &app.search_results.shows {
        let next_index = common_key_events::on_middle_press_handler(&result.items);
        app.search_results.selected_shows_index = Some(next_index);
      }
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_low_press_on_selected_block(app: &mut App) {
  match app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let Some(result) = &app.search_results.albums {
        let next_index = common_key_events::on_low_press_handler(&result.items);
        app.search_results.selected_album_index = Some(next_index);
      }
    }
    SearchResultBlock::SongSearch => {
      if let Some(result) = &app.search_results.tracks {
        let next_index = common_key_events::on_low_press_handler(&result.items);
        app.search_results.selected_tracks_index = Some(next_index);
      }
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(result) = &app.search_results.artists {
        let next_index = common_key_events::on_low_press_handler(&result.items);
        app.search_results.selected_artists_index = Some(next_index);
      }
    }
    SearchResultBlock::PlaylistSearch => {
      if let Some(result) = &app.search_results.playlists {
        let next_index = common_key_events::on_low_press_handler(&result.items);
        app.search_results.selected_playlists_index = Some(next_index);
      }
    }
    SearchResultBlock::ShowSearch => {
      if let Some(result) = &app.search_results.shows {
        let next_index = common_key_events::on_low_press_handler(&result.items);
        app.search_results.selected_shows_index = Some(next_index);
      }
    }
    SearchResultBlock::Empty => {}
  }
}

fn handle_add_item_to_queue(app: &mut App) {
  match &app.search_results.selected_block {
    SearchResultBlock::SongSearch => {
      if let (Some(index), Some(tracks)) = (
        app.search_results.selected_tracks_index,
        &app.search_results.tracks,
      ) {
        if let Some(track) = tracks.items.get(index) {
          if let Some(track_id) = &track.id {
            app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
              track_id.clone().into_static(),
            )));
          }
        }
      }
    }
    SearchResultBlock::ArtistSearch => {}
    SearchResultBlock::PlaylistSearch => {}
    SearchResultBlock::AlbumSearch => {}
    SearchResultBlock::ShowSearch => {}
    SearchResultBlock::Empty => {}
  };
}

fn handle_enter_event_on_selected_block(app: &mut App) {
  match &app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let (Some(index), Some(albums_result)) = (
        &app.search_results.selected_album_index,
        &app.search_results.albums,
      ) {
        if let Some(album) = albums_result.items.get(index.to_owned()).cloned() {
          app.track_table.context = Some(TrackTableContext::AlbumSearch);
          app.dispatch(IoEvent::GetAlbumTracks(Box::new(album)));
        };
      }
    }
    SearchResultBlock::SongSearch => {
      let index = app.search_results.selected_tracks_index;
      let tracks = app.search_results.tracks.clone();
      let track_ids: Option<Vec<PlayableId<'static>>> = tracks.map(|tracks| {
        tracks
          .items
          .into_iter()
          .filter_map(|track| track.id.map(|id| PlayableId::Track(id.into_static())))
          .collect()
      });
      app.dispatch(IoEvent::StartPlayback(None, track_ids, index));
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(index) = &app.search_results.selected_artists_index {
        if let Some(result) = app.search_results.artists.clone() {
          if let Some(artist) = result.items.get(index.to_owned()) {
            app.get_artist(artist.id.as_ref().into_static(), artist.name.clone());
          };
        };
      };
    }
    SearchResultBlock::PlaylistSearch => {
      if let (Some(index), Some(playlists_result)) = (
        app.search_results.selected_playlists_index,
        &app.search_results.playlists,
      ) {
        if let Some(playlist) = playlists_result.items.get(index) {
          // Go to playlist tracks table
          let playlist_id = playlist.id.clone().into_static();
          app.reset_playlist_tracks_view(playlist_id.clone(), TrackTableContext::PlaylistSearch);
          app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
        };
      }
    }
    SearchResultBlock::ShowSearch => {
      if let (Some(index), Some(shows_result)) = (
        app.search_results.selected_shows_index,
        &app.search_results.shows,
      ) {
        if let Some(show) = shows_result.items.get(index).cloned() {
          // Go to show tracks table
          app.dispatch(IoEvent::GetShowEpisodes(Box::new(show)));
        };
      }
    }
    SearchResultBlock::Empty => {}
  };
}

fn handle_enter_event_on_hovered_block(app: &mut App) {
  match app.search_results.hovered_block {
    SearchResultBlock::AlbumSearch => {
      let next_index = app.search_results.selected_album_index.unwrap_or(0);

      app.search_results.selected_album_index = Some(next_index);
      app.search_results.selected_block = SearchResultBlock::AlbumSearch;
    }
    SearchResultBlock::SongSearch => {
      let next_index = app.search_results.selected_tracks_index.unwrap_or(0);

      app.search_results.selected_tracks_index = Some(next_index);
      app.search_results.selected_block = SearchResultBlock::SongSearch;
    }
    SearchResultBlock::ArtistSearch => {
      let next_index = app.search_results.selected_artists_index.unwrap_or(0);

      app.search_results.selected_artists_index = Some(next_index);
      app.search_results.selected_block = SearchResultBlock::ArtistSearch;
    }
    SearchResultBlock::PlaylistSearch => {
      let next_index = app.search_results.selected_playlists_index.unwrap_or(0);

      app.search_results.selected_playlists_index = Some(next_index);
      app.search_results.selected_block = SearchResultBlock::PlaylistSearch;
    }
    SearchResultBlock::ShowSearch => {
      let next_index = app.search_results.selected_shows_index.unwrap_or(0);

      app.search_results.selected_shows_index = Some(next_index);
      app.search_results.selected_block = SearchResultBlock::ShowSearch;
    }
    SearchResultBlock::Empty => {}
  };
}

fn handle_recommended_tracks(app: &mut App) {
  match app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {}
    SearchResultBlock::SongSearch => {
      if let Some(index) = &app.search_results.selected_tracks_index {
        if let Some(result) = app.search_results.tracks.clone() {
          if let Some(track) = result.items.get(index.to_owned()) {
            let track_id_list: Option<Vec<String>> =
              track.id.as_ref().map(|id| vec![id.id().to_string()]);

            app.recommendations_context = Some(RecommendationsContext::Song);
            app.recommendations_seed = track.name.clone();
            app.get_recommendations_for_seed(None, track_id_list, Some(track.clone()));
          };
        };
      };
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(index) = &app.search_results.selected_artists_index {
        if let Some(result) = app.search_results.artists.clone() {
          if let Some(artist) = result.items.get(index.to_owned()) {
            let artist_id_list: Option<Vec<String>> = Some(vec![artist.id.id().to_string()]);
            app.recommendations_context = Some(RecommendationsContext::Artist);
            app.recommendations_seed = artist.name.clone();
            app.get_recommendations_for_seed(artist_id_list, None, None);
          };
        };
      };
    }
    SearchResultBlock::PlaylistSearch => {}
    SearchResultBlock::ShowSearch => {}
    SearchResultBlock::Empty => {}
  }
}

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Esc => {
      app.search_results.selected_block = SearchResultBlock::Empty;
    }
    k if common_key_events::down_event(k) => {
      if app.search_results.selected_block != SearchResultBlock::Empty {
        handle_down_press_on_selected_block(app);
      } else {
        handle_down_press_on_hovered_block(app);
      }
    }
    k if common_key_events::up_event(k) => {
      if app.search_results.selected_block != SearchResultBlock::Empty {
        handle_up_press_on_selected_block(app);
      } else {
        handle_up_press_on_hovered_block(app);
      }
    }
    k if common_key_events::left_event(k) => {
      app.search_results.selected_block = SearchResultBlock::Empty;
      match app.search_results.hovered_block {
        SearchResultBlock::AlbumSearch => {
          common_key_events::handle_left_event(app);
        }
        SearchResultBlock::SongSearch => {
          common_key_events::handle_left_event(app);
        }
        SearchResultBlock::ArtistSearch => {
          app.search_results.hovered_block = SearchResultBlock::SongSearch;
        }
        SearchResultBlock::PlaylistSearch => {
          app.search_results.hovered_block = SearchResultBlock::AlbumSearch;
        }
        SearchResultBlock::ShowSearch => {
          common_key_events::handle_left_event(app);
        }
        SearchResultBlock::Empty => {}
      }
    }
    k if common_key_events::right_event(k) => {
      app.search_results.selected_block = SearchResultBlock::Empty;
      match app.search_results.hovered_block {
        SearchResultBlock::AlbumSearch => {
          app.search_results.hovered_block = SearchResultBlock::PlaylistSearch;
        }
        SearchResultBlock::SongSearch => {
          app.search_results.hovered_block = SearchResultBlock::ArtistSearch;
        }
        SearchResultBlock::ArtistSearch => {
          app.search_results.hovered_block = SearchResultBlock::SongSearch;
        }
        SearchResultBlock::PlaylistSearch => {
          app.search_results.hovered_block = SearchResultBlock::AlbumSearch;
        }
        SearchResultBlock::ShowSearch => {}
        SearchResultBlock::Empty => {}
      }
    }
    k if common_key_events::high_event(k)
      && app.search_results.selected_block != SearchResultBlock::Empty =>
    {
      handle_high_press_on_selected_block(app);
    }
    k if common_key_events::middle_event(k)
      && app.search_results.selected_block != SearchResultBlock::Empty =>
    {
      handle_middle_press_on_selected_block(app);
    }
    k if common_key_events::low_event(k)
      && app.search_results.selected_block != SearchResultBlock::Empty =>
    {
      handle_low_press_on_selected_block(app)
    }
    // Handle pressing enter when block is selected to start playing track
    Key::Enter => match app.search_results.selected_block {
      SearchResultBlock::Empty => handle_enter_event_on_hovered_block(app),
      SearchResultBlock::PlaylistSearch => {
        app.playlist_offset = 0;
        handle_enter_event_on_selected_block(app);
      }
      _ => handle_enter_event_on_selected_block(app),
    },
    Key::Char('w') => match app.search_results.selected_block {
      SearchResultBlock::AlbumSearch => {
        app.current_user_saved_album_add(ActiveBlock::SearchResultBlock)
      }
      SearchResultBlock::SongSearch => open_add_to_playlist_for_selected_search_track(app),
      SearchResultBlock::ArtistSearch => app.user_follow_artists(ActiveBlock::SearchResultBlock),
      SearchResultBlock::PlaylistSearch => {
        app.user_follow_playlist();
      }
      SearchResultBlock::ShowSearch => app.user_follow_show(ActiveBlock::SearchResultBlock),
      SearchResultBlock::Empty => {}
    },
    Key::Char('D') => match app.search_results.selected_block {
      SearchResultBlock::AlbumSearch => {
        app.current_user_saved_album_delete(ActiveBlock::SearchResultBlock)
      }
      SearchResultBlock::SongSearch => {}
      SearchResultBlock::ArtistSearch => app.user_unfollow_artists(ActiveBlock::SearchResultBlock),
      SearchResultBlock::PlaylistSearch => {
        if let (Some(playlists), Some(selected_index)) = (
          &app.search_results.playlists,
          app.search_results.selected_playlists_index,
        ) {
          let selected_playlist = &playlists.items[selected_index].name;
          app.dialog = Some(selected_playlist.clone());
          app.confirm = false;

          app.push_navigation_stack(
            RouteId::Dialog,
            ActiveBlock::Dialog(DialogContext::PlaylistSearch),
          );
        }
      }
      SearchResultBlock::ShowSearch => app.user_unfollow_show(ActiveBlock::SearchResultBlock),
      SearchResultBlock::Empty => {}
    },
    Key::Char('r') => handle_recommended_tracks(app),
    _ if key == app.user_config.keys.add_item_to_queue => handle_add_item_to_queue(app),
    // Add `s` to "see more" on each option
    _ => {}
  }
}

fn open_add_to_playlist_for_selected_search_track(app: &mut App) {
  let Some(tracks) = &app.search_results.tracks else {
    return;
  };
  let Some(selected_index) = app.search_results.selected_tracks_index else {
    return;
  };
  let Some(track) = tracks.items.get(selected_index) else {
    return;
  };

  let track_id = track.id.clone().map(|id| id.into_static());
  app.begin_add_track_to_playlist_flow(track_id, track.name.clone());
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    app::{ActiveBlock, RouteId},
    test_helpers::{private_user, simplified_playlist},
    user_config::UserConfig,
  };
  use chrono::Duration as ChronoDuration;
  use rspotify::model::{
    artist::SimplifiedArtist, idtypes::TrackId, page::Page, track::FullTrack, SimplifiedAlbum,
  };
  use std::{collections::HashMap, sync::mpsc::channel, time::SystemTime};

  #[allow(deprecated)]
  fn full_track(id: &str, name: &str) -> FullTrack {
    FullTrack {
      album: SimplifiedAlbum {
        name: "Album".to_string(),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: ChronoDuration::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: Some(TrackId::from_id(id).unwrap().into_static()),
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: name.to_string(),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    }
  }

  #[test]
  fn pressing_w_on_search_song_opens_add_to_playlist_picker() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user = Some(private_user("spotatui-owner"));
    app.playlists = Some(Page {
      href: "https://api.spotify.com/v1/me/playlists".to_string(),
      items: vec![],
      limit: 50,
      next: None,
      offset: 0,
      previous: None,
      total: 1,
    });
    app.all_playlists = vec![simplified_playlist(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Owned Playlist",
      "spotatui-owner",
      false,
    )];
    app.search_results.tracks = Some(Page {
      href: "https://api.spotify.com/v1/search".to_string(),
      items: vec![full_track("0000000000000000000001", "Search Track")],
      limit: 1,
      next: None,
      offset: 0,
      previous: None,
      total: 1,
    });
    app.search_results.selected_block = SearchResultBlock::SongSearch;
    app.search_results.selected_tracks_index = Some(0);
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    handler(Key::Char('w'), &mut app);

    assert_eq!(
      app
        .pending_playlist_track_add
        .as_ref()
        .map(|pending| pending.track_name.as_str()),
      Some("Search Track")
    );
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    );
  }
}
