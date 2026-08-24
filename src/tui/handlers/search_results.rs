use super::common_key_events;
use crate::core::action::{Action, OpenTarget};
use crate::core::app::{ActiveBlock, App, DialogContext, RouteId, SearchResultBlock};
use crate::core::plugin_api::{ShowInfo, TrackInfo};
use crate::core::source::Source;
use crate::tui::event::Key;

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
      app.apply(Action::QueueTrack(track));
    }
  }
}

/// The highlighted album's id; `.get()` guards a stale index.
fn selected_search_album_id(app: &App) -> Option<String> {
  let index = app.search_results.selected_album_index?;
  app
    .search_results
    .albums
    .as_ref()?
    .items
    .get(index)?
    .id
    .clone()
}

/// The highlighted artist's (id, name).
fn selected_search_artist_identity(app: &App) -> Option<(String, String)> {
  let index = app.search_results.selected_artists_index?;
  let artist = app.search_results.artists.as_ref()?.items.get(index)?;
  Some((artist.id.clone()?, artist.name.clone()))
}

/// The highlighted track row; `None` without a selection.
fn selected_search_track_row(app: &App) -> Option<&TrackInfo> {
  let index = app.search_results.selected_tracks_index?;
  app.search_results.tracks.as_ref()?.items.get(index)
}

/// The highlighted show row; `None` without a selection.
fn selected_search_show_row(app: &App) -> Option<&ShowInfo> {
  let index = app.search_results.selected_shows_index?;
  app.search_results.shows.as_ref()?.items.get(index)
}

/// The highlighted show's snapshot, which the episode-list open carries whole.
fn selected_search_show(app: &App) -> Option<ShowInfo> {
  selected_search_show_row(app).cloned()
}

/// The highlighted show's Spotify id, for the save/unsave keys.
fn selected_search_show_id(app: &App) -> Option<String> {
  selected_search_show_row(app)?.id.clone()
}

fn handle_enter_event_on_selected_block(app: &mut App) {
  match &app.search_results.selected_block {
    SearchResultBlock::AlbumSearch => {
      if let Some(id) = selected_search_album_id(app) {
        app.apply(Action::Open(OpenTarget::Album {
          id,
          from_search: true,
        }));
      }
    }
    SearchResultBlock::SongSearch => {
      let Some(paged) = app.search_results.tracks.as_ref() else {
        return;
      };
      // No selection: an out-of-range index yields `offset: None`.
      let selected = app
        .search_results
        .selected_tracks_index
        .unwrap_or(paged.items.len());
      let (uris, offset) = common_key_events::uri_playback_request(
        paged.items.iter().map(|track| track.uri.clone()),
        selected,
      );
      app.apply(Action::PlayUris { uris, offset });
    }
    SearchResultBlock::ArtistSearch => {
      if let Some((id, name)) = selected_search_artist_identity(app) {
        app.apply(Action::Open(OpenTarget::Artist { id, name }));
      }
    }
    SearchResultBlock::PlaylistSearch => {
      // Go to playlist tracks table: navigates immediately with the cleared
      // table as the loading state (see open_playlist_tracks).
      if let Some(id) = app.selected_search_result_playlist_id() {
        app.apply(Action::Open(OpenTarget::Playlist {
          id,
          from_search: true,
        }));
      }
    }
    SearchResultBlock::ShowSearch => {
      // OpenShowEpisodes populates app.library.show_episodes (opening the show
      // by id sets EpisodeTableContext::Full but does NOT populate it, leaving
      // a blank episode list). `show` is already a domain ShowInfo.
      if let Some(show) = selected_search_show(app) {
        app.apply(Action::OpenShowEpisodes(show));
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
          app.apply(Action::RecommendFromTrack(track));
        };
      };
    }
    SearchResultBlock::ArtistSearch => {
      if let Some((id, name)) = selected_search_artist_identity(app) {
        app.apply(Action::RecommendFromArtist { id, name });
      }
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

pub(super) fn favorite_selected_radio_station(app: &mut App) {
  let Some(station) = selected_radio_station(app) else {
    app.set_status_message("No radio station selected".to_string(), 4);
    return;
  };
  app.apply(Action::FavoriteRadioStation(station));
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
  app.apply(Action::FavoriteRadioStation(station));
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
      _ => handle_enter_event_on_selected_block(app),
    },
    Key::Char('w') => match app.search_results.selected_block {
      SearchResultBlock::AlbumSearch => {
        if let Some(id) = selected_search_album_id(app) {
          app.apply(Action::SaveAlbum(id));
        }
      }
      SearchResultBlock::SongSearch => {
        let track =
          selected_search_track_row(app).map(|track| (track.id.clone(), track.name.clone()));
        if let Some((track_id, track_name)) = track {
          app.apply(Action::OpenAddTrackDialogFor {
            track_id,
            track_name,
          });
        }
      }
      SearchResultBlock::ArtistSearch => {
        if let Some((id, _name)) = selected_search_artist_identity(app) {
          app.apply(Action::FollowArtist(id));
        }
      }
      SearchResultBlock::PlaylistSearch => {
        if let Some(id) = app.selected_search_result_playlist_id() {
          app.apply(Action::FollowPlaylist(id));
        }
      }
      SearchResultBlock::ShowSearch => {
        if let Some(id) = selected_search_show_id(app) {
          app.apply(Action::SaveShow(id));
        }
      }
      SearchResultBlock::Empty => {}
    },
    Key::Char('D') => match app.search_results.selected_block {
      SearchResultBlock::AlbumSearch => {
        if let Some(id) = selected_search_album_id(app) {
          app.apply(Action::UnsaveAlbum(id));
        }
      }
      SearchResultBlock::SongSearch => {}
      SearchResultBlock::ArtistSearch => {
        if let Some((id, _name)) = selected_search_artist_identity(app) {
          app.apply(Action::UnfollowArtist(id));
        }
      }
      SearchResultBlock::PlaylistSearch => {
        if let (Some(playlists), Some(selected_index)) = (
          &app.search_results.playlists,
          app.search_results.selected_playlists_index,
        ) {
          if let Some(selected_playlist) = playlists.items.get(selected_index) {
            let selected_playlist = selected_playlist.name.clone();
            app.view.dialog = Some(selected_playlist);
            app.view.confirm = false;

            app.push_navigation_stack(
              RouteId::Dialog,
              ActiveBlock::Dialog(DialogContext::PlaylistSearch),
            );
          }
        }
      }
      SearchResultBlock::ShowSearch => {
        if let Some(id) = selected_search_show_id(app) {
          app.apply(Action::UnsaveShow(id));
        }
      }
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
      app.apply(Action::ToggleSaveTrack(uri));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    app::{ActiveBlock, RouteId, TrackTableContext},
    pagination::Paged,
    plugin_api::TrackInfo,
    test_helpers::{full_track, playlist_info, user_info},
    user_config::UserConfig,
  };
  use crate::infra::network::IoEvent;
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
    app.state_path = Some(dir.path().join("state.yml"));
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

    assert_eq!(app.runtime_state.radio_stations.len(), 1);
    assert_eq!(
      app.runtime_state.radio_stations[0].url,
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
    assert!(app.view.dialog.is_none());
  }

  #[test]
  fn enter_on_song_search_without_results_starts_nothing() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    handler(Key::Enter, &mut app);
    handler(Key::Enter, &mut app);

    assert_eq!(
      app.search_results.selected_block,
      SearchResultBlock::SongSearch
    );
    assert!(rx.try_recv().is_err(), "no results means nothing starts");
  }

  #[test]
  fn enter_on_playlist_search_opens_the_track_table_in_the_search_context() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // No public setter for search results: seeded directly.
    app.search_results.playlists = Some(Paged {
      items: vec![playlist_info(
        "37i9dQZF1DXcBWIGoYBM5M",
        "Search Playlist",
        "spotatui-owner",
        false,
      )],
      offset: 0,
      limit: 1,
      total: 1,
      next: None,
      previous: None,
    });
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    handler(Key::Right, &mut app);
    handler(Key::Down, &mut app);
    handler(Key::Enter, &mut app);
    handler(Key::Enter, &mut app);

    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::PlaylistSearch)
    );
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    assert_eq!(
      app.pending_playlist_open.as_deref(),
      Some("37i9dQZF1DXcBWIGoYBM5M")
    );
  }
}
