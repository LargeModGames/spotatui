use super::common_key_events;
use crate::core::app::{
  ActiveBlock, App, DialogContext, RecommendationsContext, RouteId, SearchResultBlock,
  TrackTableContext,
};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::{
  idtypes::{AlbumId, ArtistId, PlaylistId, ShowId, TrackId},
  show::SimplifiedShow,
  PlayableId,
};

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
          if let Some(ref id_str) = track.id {
            if let Ok(track_id) = TrackId::from_id(id_str.as_str()) {
              app.dispatch(IoEvent::AddItemToQueue(PlayableId::Track(
                track_id.into_static(),
              )));
            }
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
        app.search_results.selected_album_index,
        &app.search_results.albums,
      ) {
        if let Some(album) = albums_result.items.get(index) {
          if let Some(ref id_str) = album.id {
            if let Ok(album_id) = AlbumId::from_id(id_str.as_str()) {
              app.track_table.context = Some(TrackTableContext::AlbumSearch);
              app.dispatch(IoEvent::GetAlbum(album_id.into_static()));
            }
          }
        };
      }
    }
    SearchResultBlock::SongSearch => {
      let index = app.search_results.selected_tracks_index;
      let track_ids: Option<Vec<PlayableId<'static>>> =
        app.search_results.tracks.as_ref().map(|paged| {
          paged
            .items
            .iter()
            .filter_map(|track| {
              track.id.as_ref().and_then(|id_str| {
                TrackId::from_id(id_str.as_str())
                  .ok()
                  .map(|id| PlayableId::Track(id.into_static()))
              })
            })
            .collect()
        });
      app.dispatch(IoEvent::StartPlayback(None, track_ids, index));
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(index) = app.search_results.selected_artists_index {
        if let Some(result) = &app.search_results.artists {
          if let Some(artist) = result.items.get(index) {
            if let Some(ref id_str) = artist.id {
              if let Ok(artist_id) = ArtistId::from_id(id_str.as_str()) {
                app.get_artist(artist_id.into_static(), artist.name.clone());
              }
            }
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
          if let Some(ref id_str) = playlist.id {
            if let Ok(playlist_id) = PlaylistId::from_id(id_str.as_str()) {
              // Go to playlist tracks table
              let playlist_id = playlist_id.into_static();
              app
                .reset_playlist_tracks_view(playlist_id.clone(), TrackTableContext::PlaylistSearch);
              app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
            }
          }
        };
      }
    }
    SearchResultBlock::ShowSearch => {
      if let (Some(index), Some(shows_result)) = (
        app.search_results.selected_shows_index,
        &app.search_results.shows,
      ) {
        if let Some(show) = shows_result.items.get(index) {
          if let Some(ref id_str) = show.id {
            if let Ok(show_id) = ShowId::from_id(id_str.as_str()) {
              // Reconstruct a minimal SimplifiedShow from the domain id.
              // GetShowEpisodes uses show.id to fetch episodes and sets
              // EpisodeTableContext::Simplified, which the episode table reads
              // from app.library.show_episodes (populated by add_pages).
              // Using GetShow would set EpisodeTableContext::Full but NOT populate
              // show_episodes, resulting in a blank episode list.
              #[allow(deprecated)]
              let minimal_show = SimplifiedShow {
                id: show_id.into_static(),
                name: show.name.clone(),
                description: show.description.clone(),
                explicit: false,
                external_urls: std::collections::HashMap::new(),
                href: String::new(),
                images: Vec::new(),
                is_externally_hosted: None,
                languages: Vec::new(),
                media_type: String::new(),
                copyrights: Vec::new(),
                available_markets: Vec::new(),
                publisher: String::new(),
              };
              app.dispatch(IoEvent::GetShowEpisodes(Box::new(minimal_show)));
            }
          }
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
      if let Some(index) = app.search_results.selected_tracks_index {
        if let Some(track) = app
          .search_results
          .tracks
          .as_ref()
          .and_then(|paged| paged.items.get(index))
          .cloned()
        {
          let track_id_list: Option<Vec<String>> = track.id.as_ref().map(|id| vec![id.clone()]);

          app.recommendations_context = Some(RecommendationsContext::Song);
          app.recommendations_seed = track.name.clone();
          app.get_recommendations_for_seed(None, track_id_list, Some(track));
        };
      };
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(index) = app.search_results.selected_artists_index {
        if let Some(artist) = app
          .search_results
          .artists
          .as_ref()
          .and_then(|paged| paged.items.get(index))
        {
          let artist_id_list: Option<Vec<String>> = artist.id.as_ref().map(|id| vec![id.clone()]);
          app.recommendations_context = Some(RecommendationsContext::Artist);
          app.recommendations_seed = artist.name.clone();
          app.get_recommendations_for_seed(artist_id_list, None, None);
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
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if app.search_results.selected_block != SearchResultBlock::Empty {
        handle_down_press_on_selected_block(app);
      } else {
        handle_down_press_on_hovered_block(app);
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if app.search_results.selected_block != SearchResultBlock::Empty {
        handle_up_press_on_selected_block(app);
      } else {
        handle_up_press_on_hovered_block(app);
      }
    }
    k if common_key_events::left_event(k, &app.user_config.keys) => {
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
    k if common_key_events::right_event(k, &app.user_config.keys) => {
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

  let track_id = track
    .id
    .as_ref()
    .and_then(|id_str| TrackId::from_id(id_str.as_str()).ok())
    .map(|id| id.into_static());
  app.begin_add_track_to_playlist_flow(track_id, track.name.clone());
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    app::{ActiveBlock, RouteId},
    pagination::Paged,
    plugin_api::TrackInfo,
    test_helpers::{full_track, playlist_info, user_info},
    user_config::UserConfig,
  };
  use std::{sync::mpsc::channel, time::SystemTime};

  #[test]
  fn pressing_w_on_search_song_opens_add_to_playlist_picker() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user = Some(user_info("spotatui-owner"));
    app.playlists = Some(Paged {
      total: 1,
      ..Default::default()
    });
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Owned Playlist",
      "spotatui-owner",
      false,
    )];
    app.search_results.tracks = Some(Paged {
      items: vec![TrackInfo::from(&full_track(
        "0000000000000000000001",
        "Search Track",
      ))],
      offset: 0,
      limit: 1,
      total: 1,
      next: None,
      previous: None,
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
