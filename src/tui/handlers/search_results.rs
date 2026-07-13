use super::common_key_events;
use crate::core::app::{
  ActiveBlock, App, DialogContext, RecommendationsContext, RouteId, SearchResultBlock,
  TrackTableContext,
};
use crate::core::plugin_api::TrackInfo;
use crate::core::source::Source;
use crate::core::user_config::RadioStationAddOutcome;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::idtypes::PlaylistId;

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
  if let SearchResultBlock::SongSearch = app.search_results.selected_block {
    let track = app.search_results.selected_tracks_index.and_then(|index| {
      app
        .search_results
        .tracks
        .as_ref()
        .and_then(|tracks| tracks.items.get(index).cloned())
    });
    if let Some(track) = track {
      app.add_track_to_native_queue(track);
    }
  }
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
            app.track_table.context = Some(TrackTableContext::AlbumSearch);
            app.dispatch(IoEvent::GetAlbum(id_str.clone()));
          }
        };
      }
    }
    SearchResultBlock::SongSearch => {
      let index = app.search_results.selected_tracks_index;
      let track_ids: Option<Vec<String>> = app.search_results.tracks.as_ref().map(|paged| {
        paged
          .items
          .iter()
          .filter_map(|track| track.uri.clone())
          .collect()
      });
      app.dispatch(IoEvent::StartPlayback(None, track_ids, index));
    }
    SearchResultBlock::ArtistSearch => {
      if let Some(index) = app.search_results.selected_artists_index {
        if let Some(result) = &app.search_results.artists {
          if let Some(artist) = result.items.get(index) {
            if let Some(ref id_str) = artist.id {
              app.get_artist(id_str.clone(), artist.name.clone());
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
              // Go to playlist tracks table: navigates immediately with the
              // cleared table as the loading state (see open_playlist_tracks).
              app
                .open_playlist_tracks(playlist_id.into_static(), TrackTableContext::PlaylistSearch);
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
          // GetShowEpisodes populates app.library.show_episodes (GetShow sets
          // EpisodeTableContext::Full but does NOT populate it, leaving a blank
          // episode list). `show` is already a domain ShowInfo.
          app.dispatch(IoEvent::GetShowEpisodes(Box::new(show.clone())));
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

fn selected_radio_station(app: &App) -> Option<TrackInfo> {
  let index = app.search_results.selected_tracks_index?;
  app
    .search_results
    .tracks
    .as_ref()?
    .items
    .get(index)
    .cloned()
}

/// Move `station` into the sidebar list under the favorited `name`/`url`,
/// deduped by `radio:` URI. Takes ownership so the station is not cloned again.
fn add_station_to_sidebar(app: &mut App, mut station: TrackInfo, name: String, url: &str) {
  let uri = format!("{}{url}", super::RADIO_URI_PREFIX);
  if app
    .radio_stations
    .iter()
    .any(|existing| existing.uri.as_deref() == Some(uri.as_str()))
  {
    return;
  }

  station.name = name;
  station.uri = Some(uri);
  app.radio_stations.push(station);
  if app.selected_playlist_index.is_none() {
    app.selected_playlist_index = Some(0);
  }
}

fn favorite_radio_station(app: &mut App, station: TrackInfo) {
  let Some(url) = station.uri.as_deref().and_then(super::radio_stream_url) else {
    app.set_status_message("Radio station has no stream URL".to_string(), 4);
    return;
  };

  let trimmed = station.name.trim();
  let name = if trimmed.is_empty() { url } else { trimmed }.to_string();
  let url = url.to_string();

  let message = match app.user_config.add_radio_station(&name, &url) {
    Ok(RadioStationAddOutcome::Added) => format!("Favorited radio station: {name}"),
    Ok(RadioStationAddOutcome::AlreadyExists) => {
      format!("Radio station already favorited: {name}")
    }
    Err(error) => {
      app.set_error_status_message(format!("Could not favorite radio station: {error}"), 6);
      return;
    }
  };

  add_station_to_sidebar(app, station, name, &url);
  app.set_status_message(message, 4);
}

pub(super) fn favorite_selected_radio_station(app: &mut App) {
  let Some(station) = selected_radio_station(app) else {
    app.set_status_message("No radio station selected".to_string(), 4);
    return;
  };
  favorite_radio_station(app, station);
}

#[cfg(feature = "internet-radio")]
pub(super) fn favorite_current_radio_station(app: &mut App) -> bool {
  let Some(station) = app
    .radio_playback
    .as_ref()
    .map(|session| session.station.clone())
  else {
    return false;
  };
  favorite_radio_station(app, station);
  true
}

#[cfg(not(feature = "internet-radio"))]
pub(super) fn favorite_current_radio_station(_app: &mut App) -> bool {
  false
}

/// Key handling for the internet-radio results view: a single full-area
/// Stations panel (see `draw_radio_station_results`), backed by the
/// `SongSearch` block. Navigation is pinned to that one block so focus can
/// never wander into the four Spotify-only blocks that aren't drawn, and
/// Enter plays the highlighted station directly (no select-the-block first —
/// there is only one block). Spotify-only actions (`w`/`D`/`r`/queue) are
/// inert here.
fn handle_radio_key(key: Key, app: &mut App) {
  // Whatever mouse hovering or stale state did, the only visible block is
  // the station list.
  app.search_results.hovered_block = SearchResultBlock::SongSearch;
  match key {
    Key::Esc => {
      app.search_results.selected_block = SearchResultBlock::Empty;
    }
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      app.search_results.selected_block = SearchResultBlock::Empty;
      common_key_events::handle_left_event(app);
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      handle_down_press_on_selected_block(app);
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      handle_up_press_on_selected_block(app);
    }
    k if common_key_events::high_event(k) => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      handle_high_press_on_selected_block(app);
    }
    k if common_key_events::middle_event(k) => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      handle_middle_press_on_selected_block(app);
    }
    k if common_key_events::low_event(k) => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      handle_low_press_on_selected_block(app);
    }
    k if k == app.user_config.keys.like_track => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      favorite_selected_radio_station(app);
    }
    Key::Enter => {
      app.search_results.selected_block = SearchResultBlock::SongSearch;
      handle_enter_event_on_selected_block(app);
    }
    _ => {}
  }
}

pub fn handler(key: Key, app: &mut App) {
  if app.active_source == Source::Radio {
    handle_radio_key(key, app);
    return;
  }
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
          if let Some(selected_playlist) = playlists.items.get(selected_index) {
            let selected_playlist = selected_playlist.name.clone();
            app.dialog = Some(selected_playlist);
            app.confirm = false;

            app.push_navigation_stack(
              RouteId::Dialog,
              ActiveBlock::Dialog(DialogContext::PlaylistSearch),
            );
          }
        }
      }
      SearchResultBlock::ShowSearch => app.user_unfollow_show(ActiveBlock::SearchResultBlock),
      SearchResultBlock::Empty => {}
    },
    Key::Char('r') => handle_recommended_tracks(app),
    _ if key == app.user_config.keys.add_item_to_queue => handle_add_item_to_queue(app),
    Key::Char('s') => handle_save_track_event(app),
    _ => {}
  }
}

fn handle_save_track_event(app: &mut App) {
  if let SearchResultBlock::SongSearch = app.search_results.selected_block {
    let uri = app.search_results.selected_tracks_index.and_then(|index| {
      app
        .search_results
        .tracks
        .as_ref()
        .and_then(|tracks| tracks.items.get(index))
        .and_then(|track| track.uri.clone())
    });
    if let Some(uri) = uri {
      app.dispatch(IoEvent::ToggleSaveTrack(uri));
    }
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

  app.begin_add_track_to_playlist_flow(track.id.clone(), track.name.clone());
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

  fn station(uri: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(uri.to_string()),
      name: name.to_string(),
      artists: vec!["ambient".to_string()],
      album: "US \u{2022} MP3 \u{2022} 128 kbps".to_string(),
      duration_ms: 0,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  /// Radio results are a single panel: navigation must stay pinned to the
  /// SongSearch block (the others aren't drawn) and Enter must start the
  /// highlighted station.
  #[test]
  fn radio_results_pin_navigation_and_enter_plays_station() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.active_source = Source::Radio;
    app.search_results.tracks = Some(Paged {
      items: vec![
        station("radio:https://a.example/one", "One FM"),
        station("radio:https://b.example/two", "Two FM"),
      ],
      total: 2,
      ..Default::default()
    });
    app.search_results.selected_tracks_index = Some(0);
    app.search_results.hovered_block = SearchResultBlock::SongSearch;
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    // Down/right-style keys must never hover/select another block.
    handler(Key::Down, &mut app);
    assert_eq!(
      app.search_results.hovered_block,
      SearchResultBlock::SongSearch
    );
    assert_eq!(
      app.search_results.selected_block,
      SearchResultBlock::SongSearch
    );
    assert_eq!(app.search_results.selected_tracks_index, Some(1));

    // Enter plays the highlighted station via the shared StartPlayback path.
    handler(Key::Enter, &mut app);
    match rx.try_recv().unwrap() {
      IoEvent::StartPlayback(None, Some(uris), Some(1)) => {
        assert_eq!(uris[1], "radio:https://b.example/two");
      }
      _ => panic!("expected a StartPlayback of the station uris"),
    }
  }

  #[test]
  fn radio_results_favorite_key_saves_selected_station() {
    use crate::core::user_config::UserConfigPaths;

    let dir = tempfile::tempdir().unwrap();
    let mut user_config = UserConfig::new();
    user_config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    let (tx, _rx) = channel();
    let mut app = App::new(tx, user_config, Some(SystemTime::now()));
    app.active_source = Source::Radio;
    app.search_results.tracks = Some(Paged {
      items: vec![station(
        "radio:https://ice1.somafm.com/groovesalad-128-mp3",
        "Groove Salad",
      )],
      total: 1,
      ..Default::default()
    });
    app.search_results.selected_tracks_index = Some(0);
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    let favorite_key = app.user_config.keys.like_track;
    handler(favorite_key, &mut app);

    assert_eq!(app.user_config.behavior.radio_stations.len(), 1);
    assert_eq!(
      app.user_config.behavior.radio_stations[0].url,
      "https://ice1.somafm.com/groovesalad-128-mp3"
    );
    assert_eq!(app.radio_stations.len(), 1);
    assert_eq!(
      app.status_message.as_deref(),
      Some("Favorited radio station: Groove Salad")
    );
  }

  #[test]
  fn pressing_w_on_search_song_opens_add_to_playlist_picker() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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

  /// Issue #348 regression: `s` on a highlighted search-result track must
  /// toggle its saved/liked state, matching the track-table binding shown in
  /// the help menu ("Save track in list or table").
  #[test]
  fn pressing_s_on_search_song_toggles_saved_track() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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

    handler(Key::Char('s'), &mut app);

    match rx.try_recv().unwrap() {
      IoEvent::ToggleSaveTrack(uri) => {
        assert_eq!(uri, "spotify:track:0000000000000000000001");
      }
      _ => panic!("expected a ToggleSaveTrack for the selected search track"),
    }
  }

  /// panic-1 regression: a stale `selected_playlists_index` left over from a
  /// longer search page must not panic when a shorter page has since replaced
  /// it (the root-cause clamp lives in `infra/network/search.rs`; this test
  /// guards the handler-side defense in depth: `.get()` instead of `[..]`).
  #[test]
  fn pressing_shift_d_with_stale_index_past_shorter_playlist_page_does_not_panic() {
    let (_tx, _rx) = channel();
    let mut app = App::new(_tx, UserConfig::new(), Some(SystemTime::now()));
    app.search_results.playlists = Some(Paged {
      items: vec![playlist_info(
        "37i9dQZF1DXcBWIGoYBM5M",
        "Only Playlist",
        "spotatui-owner",
        false,
      )],
      offset: 0,
      limit: 1,
      total: 1,
      next: None,
      previous: None,
    });
    // Stale index from a previous, longer page — out of range for the page above.
    app.search_results.selected_playlists_index = Some(20);
    app.search_results.selected_block = SearchResultBlock::PlaylistSearch;
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    // Must not panic.
    handler(Key::Char('D'), &mut app);

    // Out-of-range index: the dialog is not opened (no-op), matching sibling
    // `.get()`-guarded handlers elsewhere in this file.
    assert!(app.dialog.is_none());
  }
}
