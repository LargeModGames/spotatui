use super::{
  super::app::{App, RecommendationsContext, TrackTable, TrackTableContext},
  common_key_events,
};
use crate::event::Key;
use crate::network::IoEvent;
use rand::{thread_rng, Rng};
use rspotify::model::{
  idtypes::{PlayContextId, PlaylistId, TrackId},
  PlayableId,
};

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k) => common_key_events::handle_left_event(app),
    k if common_key_events::down_event(k) => {
      let current_index = app.track_table.selected_index;
      let tracks_len = app.track_table.tracks.len();

      // Check if we're at the last track and there are more tracks to load
      if current_index == tracks_len - 1 {
        match &app.track_table.context {
          Some(TrackTableContext::MyPlaylists) => {
            if let (Some(playlists), Some(selected_playlist_index)) =
              (&app.playlists, &app.selected_playlist_index)
            {
              if let Some(selected_playlist) = playlists.items.get(*selected_playlist_index) {
                if let Some(playlist_tracks) = &app.playlist_tracks {
                  // Check if there are more tracks to fetch
                  if app.playlist_offset + app.large_search_limit < playlist_tracks.total {
                    app.playlist_offset += app.large_search_limit;
                    let playlist_id = playlist_id_static_from_ref(&selected_playlist.id);
                    app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
                    // Keep selection at the last track; it will move to first of new page when loaded
                    app.track_table.selected_index = 0;
                    return;
                  }
                }
              }
            }
          }
          Some(TrackTableContext::DiscoverPlaylist) => {
            // Discover playlists don't support pagination
          }
          Some(TrackTableContext::SavedTracks) => {
            // Check if there are more saved tracks to load
            if let Some(saved_tracks) = app.library.saved_tracks.get_results(None) {
              let current_offset = saved_tracks.offset;
              let limit = saved_tracks.limit;
              // If there are more tracks beyond current page
              if current_offset + limit < saved_tracks.total {
                app.get_current_user_saved_tracks_next();
                app.track_table.selected_index = 0;
                return;
              }
            }
          }
          _ => {}
        }
      }

      let next_index = common_key_events::on_down_press_handler(
        &app.track_table.tracks,
        Some(app.track_table.selected_index),
      );
      app.track_table.selected_index = next_index;
    }
    k if common_key_events::up_event(k) => {
      let current_index = app.track_table.selected_index;

      // Check if we're at the first track and there are previous tracks to load
      if current_index == 0 {
        match &app.track_table.context {
          Some(TrackTableContext::MyPlaylists) => {
            if app.playlist_offset > 0 {
              if let (Some(playlists), Some(selected_playlist_index)) =
                (&app.playlists, &app.selected_playlist_index)
              {
                if let Some(selected_playlist) = playlists.items.get(*selected_playlist_index) {
                  app.playlist_offset = app.playlist_offset.saturating_sub(app.large_search_limit);
                  let playlist_id = playlist_id_static_from_ref(&selected_playlist.id);
                  app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
                  // Set selection to last track of the loaded page
                  app.track_table.selected_index =
                    app.large_search_limit.saturating_sub(1) as usize;
                  return;
                }
              }
            }
          }
          Some(TrackTableContext::DiscoverPlaylist) => {
            // Discover playlists don't support pagination
          }
          Some(TrackTableContext::SavedTracks) => {
            // Check if there are previous saved tracks to load
            if app.library.saved_tracks.index > 0 {
              app.get_current_user_saved_tracks_previous();
              // Set selection to last track of the loaded page
              app.track_table.selected_index = app.large_search_limit.saturating_sub(1) as usize;
              return;
            }
          }
          _ => {}
        }
      }

      let next_index = common_key_events::on_up_press_handler(
        &app.track_table.tracks,
        Some(app.track_table.selected_index),
      );
      app.track_table.selected_index = next_index;
    }
    k if common_key_events::high_event(k) => {
      let next_index = common_key_events::on_high_press_handler();
      app.track_table.selected_index = next_index;
    }
    k if common_key_events::middle_event(k) => {
      let next_index = common_key_events::on_middle_press_handler(&app.track_table.tracks);
      app.track_table.selected_index = next_index;
    }
    k if common_key_events::low_event(k) => {
      let next_index = common_key_events::on_low_press_handler(&app.track_table.tracks);
      app.track_table.selected_index = next_index;
    }
    Key::Enter => {
      on_enter(app);
    }
    // Scroll down
    k if k == app.user_config.keys.next_page => {
      if let Some(context) = &app.track_table.context {
        match context {
          TrackTableContext::MyPlaylists => {
            if let (Some(playlists), Some(selected_playlist_index)) =
              (&app.playlists, &app.selected_playlist_index)
            {
              if let Some(selected_playlist) =
                playlists.items.get(selected_playlist_index.to_owned())
              {
                if let Some(playlist_tracks) = &app.playlist_tracks {
                  if app.playlist_offset + app.large_search_limit < playlist_tracks.total {
                    app.playlist_offset += app.large_search_limit;
                    let playlist_id = playlist_id_static_from_ref(&selected_playlist.id);
                    app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
                  }
                }
              }
            };
          }
          TrackTableContext::RecommendedTracks => {}
          TrackTableContext::SavedTracks => {
            app.get_current_user_saved_tracks_next();
          }
          TrackTableContext::AlbumSearch => {}
          TrackTableContext::PlaylistSearch => {}
          TrackTableContext::DiscoverPlaylist => {}
        }
      };
    }
    // Scroll up
    k if k == app.user_config.keys.previous_page => {
      if let Some(context) = &app.track_table.context {
        match context {
          TrackTableContext::MyPlaylists => {
            if let (Some(playlists), Some(selected_playlist_index)) =
              (&app.playlists, &app.selected_playlist_index)
            {
              if app.playlist_offset >= app.large_search_limit {
                app.playlist_offset -= app.large_search_limit;
              };
              if let Some(selected_playlist) =
                playlists.items.get(selected_playlist_index.to_owned())
              {
                let playlist_id = playlist_id_static_from_ref(&selected_playlist.id);
                app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
              }
            };
          }
          TrackTableContext::RecommendedTracks => {}
          TrackTableContext::SavedTracks => {
            app.get_current_user_saved_tracks_previous();
          }
          TrackTableContext::AlbumSearch => {}
          TrackTableContext::PlaylistSearch => {}
          TrackTableContext::DiscoverPlaylist => {}
        }
      };
    }
    Key::Char('s') => handle_save_track_event(app),
    Key::Char('S') => play_random_song(app),
    k if k == app.user_config.keys.jump_to_end => jump_to_end(app),
    k if k == app.user_config.keys.jump_to_start => jump_to_start(app),
    //recommended song radio
    Key::Char('r') => {
      handle_recommended_tracks(app);
    }
    _ if key == app.user_config.keys.add_item_to_queue => on_queue(app),
    _ => {}
  }
}

fn play_random_song(app: &mut App) {
  if let Some(context) = &app.track_table.context {
    match context {
      TrackTableContext::MyPlaylists => {
        let (context_id, track_json) = match (&app.selected_playlist_index, &app.playlists) {
          (Some(selected_playlist_index), Some(playlists)) => {
            if let Some(selected_playlist) = playlists.items.get(selected_playlist_index.to_owned())
            {
              (
                Some(playlist_context_id_from_ref(&selected_playlist.id)),
                Some(selected_playlist.tracks.total),
              )
            } else {
              (None, None)
            }
          }
          _ => (None, None),
        };

        if let Some(val) = track_json {
          app.dispatch(IoEvent::StartPlayback(
            context_id,
            None,
            Some(thread_rng().gen_range(0..val as usize)),
          ));
        }
      }
      TrackTableContext::RecommendedTracks => {}
      TrackTableContext::SavedTracks => {
        if let Some(saved_tracks) = &app.library.saved_tracks.get_results(None) {
          let playable_ids: Vec<PlayableId<'static>> = saved_tracks
            .items
            .iter()
            .filter_map(|item| track_playable_id(item.track.id.clone()))
            .collect();
          if !playable_ids.is_empty() {
            let rand_idx = thread_rng().gen_range(0..playable_ids.len());
            app.dispatch(IoEvent::StartPlayback(
              None,
              Some(playable_ids),
              Some(rand_idx),
            ))
          }
        }
      }
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::PlaylistSearch => {
        let (context_id, playlist_track_json) = match (
          &app.search_results.selected_playlists_index,
          &app.search_results.playlists,
        ) {
          (Some(selected_playlist_index), Some(playlist_result)) => {
            if let Some(selected_playlist) = playlist_result
              .items
              .get(selected_playlist_index.to_owned())
            {
              (
                Some(playlist_context_id_from_ref(&selected_playlist.id)),
                Some(selected_playlist.tracks.total),
              )
            } else {
              (None, None)
            }
          }
          _ => (None, None),
        };
        if let Some(val) = playlist_track_json {
          app.dispatch(IoEvent::StartPlayback(
            context_id,
            None,
            Some(thread_rng().gen_range(0..val as usize)),
          ))
        }
      }
      TrackTableContext::DiscoverPlaylist => {
        // Play random track from currently displayed discover playlist
        let num_tracks = app.track_table.tracks.len();
        if num_tracks > 0 {
          if let Some(track) = app
            .track_table
            .tracks
            .get(thread_rng().gen_range(0..num_tracks))
          {
            if let Some(playable_id) = track_playable_id(track.id.clone()) {
              app.dispatch(IoEvent::StartPlayback(
                None,
                Some(vec![playable_id]),
                Some(0),
              ));
            }
          }
        }
      }
    }
  };
}

fn handle_save_track_event(app: &mut App) {
  let (selected_index, tracks) = (&app.track_table.selected_index, &app.track_table.tracks);
  if let Some(track) = tracks.get(*selected_index) {
    if let Some(playable_id) = track_playable_id(track.id.clone()) {
      app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
    }
  };
}

fn handle_recommended_tracks(app: &mut App) {
  let (selected_index, tracks) = (&app.track_table.selected_index, &app.track_table.tracks);
  if let Some(track) = tracks.get(*selected_index) {
    let first_track = track.clone();
    let track_id_list = track.id.as_ref().map(|id| vec![id.to_string()]);

    app.recommendations_context = Some(RecommendationsContext::Song);
    app.recommendations_seed = first_track.name.clone();
    app.get_recommendations_for_seed(None, track_id_list, Some(first_track));
  };
}

fn jump_to_end(app: &mut App) {
  if let Some(context) = &app.track_table.context {
    match context {
      TrackTableContext::MyPlaylists => {
        if let (Some(playlists), Some(selected_playlist_index)) =
          (&app.playlists, &app.selected_playlist_index)
        {
          if let Some(selected_playlist) = playlists.items.get(selected_playlist_index.to_owned()) {
            let total_tracks = selected_playlist.tracks.total;

            if app.large_search_limit < total_tracks {
              app.playlist_offset = total_tracks - (total_tracks % app.large_search_limit);
              let playlist_id = playlist_id_static_from_ref(&selected_playlist.id);
              app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
            }
          }
        }
      }
      TrackTableContext::RecommendedTracks => {}
      TrackTableContext::SavedTracks => {}
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::PlaylistSearch => {}
      TrackTableContext::DiscoverPlaylist => {}
    }
  }
}

fn on_enter(app: &mut App) {
  let TrackTable {
    context,
    selected_index,
    tracks,
  } = &app.track_table;
  if let Some(context) = &context {
    match context {
      TrackTableContext::MyPlaylists => {
        if let Some(track) = tracks.get(*selected_index) {
          // Get the track ID to play
          let track_playable_id = track_playable_id(track.id.clone());

          let context_id = match (&app.active_playlist_index, &app.playlists) {
            (Some(active_playlist_index), Some(playlists)) => playlists
              .items
              .get(active_playlist_index.to_owned())
              .map(|selected_playlist| playlist_context_id_from_ref(&selected_playlist.id)),
            _ => None,
          };

          // If we have a track ID, play it directly within the context
          // This ensures the selected track plays first, even with shuffle on
          if let Some(playable_id) = track_playable_id {
            app.dispatch(IoEvent::StartPlayback(
              context_id,
              Some(vec![playable_id]),
              Some(0), // Play the first (and only) track in the URIs list
            ));
          } else {
            // Fallback to context playback with offset
            app.dispatch(IoEvent::StartPlayback(
              context_id,
              None,
              Some(app.track_table.selected_index + app.playlist_offset as usize),
            ));
          }
        };
      }
      TrackTableContext::RecommendedTracks => {
        let playable_ids: Vec<PlayableId<'static>> = app
          .recommended_tracks
          .iter()
          .filter_map(|track| track_playable_id(track.id.clone()))
          .collect();
        if !playable_ids.is_empty() {
          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(playable_ids),
            Some(app.track_table.selected_index),
          ));
        }
      }
      TrackTableContext::SavedTracks => {
        // Collect tracks from ALL loaded pages (not just current page)
        // This gives us a larger playback range as the user browses
        let mut all_playable_ids: Vec<PlayableId<'static>> = Vec::new();
        let current_page_index = app.library.saved_tracks.index;

        // Iterate through all loaded pages
        for (page_idx, page) in app.library.saved_tracks.pages.iter().enumerate() {
          for item in &page.items {
            if let Some(id) = track_playable_id(item.track.id.clone()) {
              all_playable_ids.push(id);
            }
          }
          // If this is the current page, calculate the absolute offset for the selected track
          if page_idx == current_page_index {
            // This is handled below by calculating from page sizes
          }
        }

        if !all_playable_ids.is_empty() {
          // Calculate absolute offset: (sum of previous page sizes) + selected index in current page
          let mut absolute_offset = 0;
          for page_idx in 0..current_page_index {
            if let Some(page) = app.library.saved_tracks.pages.get(page_idx) {
              absolute_offset += page.items.len();
            }
          }
          absolute_offset += app.track_table.selected_index;

          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(all_playable_ids),
            Some(absolute_offset),
          ));
        }
      }
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::PlaylistSearch => {
        let TrackTable {
          selected_index,
          tracks,
          ..
        } = &app.track_table;
        if let Some(_track) = tracks.get(*selected_index) {
          let context_id = match (
            &app.search_results.selected_playlists_index,
            &app.search_results.playlists,
          ) {
            (Some(selected_playlist_index), Some(playlist_result)) => playlist_result
              .items
              .get(selected_playlist_index.to_owned())
              .map(|selected_playlist| playlist_context_id_from_ref(&selected_playlist.id)),
            _ => None,
          };

          app.dispatch(IoEvent::StartPlayback(
            context_id,
            None,
            Some(app.track_table.selected_index),
          ));
        };
      }
      TrackTableContext::DiscoverPlaylist => {
        // Play selected track from discover playlist
        if let Some(track) = tracks.get(*selected_index) {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
            app.dispatch(IoEvent::StartPlayback(
              None,
              Some(vec![playable_id]),
              Some(0),
            ));
          }
        }
      }
    }
  };
}

fn on_queue(app: &mut App) {
  let TrackTable {
    context,
    selected_index,
    tracks,
  } = &app.track_table;
  if let Some(context) = &context {
    match context {
      TrackTableContext::MyPlaylists => {
        if let Some(track) = tracks.get(*selected_index) {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
            app.dispatch(IoEvent::AddItemToQueue(playable_id));
          }
        };
      }
      TrackTableContext::RecommendedTracks => {
        if let Some(full_track) = app.recommended_tracks.get(app.track_table.selected_index) {
          if let Some(playable_id) = track_playable_id(full_track.id.clone()) {
            app.dispatch(IoEvent::AddItemToQueue(playable_id));
          }
        }
      }
      TrackTableContext::SavedTracks => {
        if let Some(page) = app.library.saved_tracks.get_results(None) {
          if let Some(saved_track) = page.items.get(app.track_table.selected_index) {
            if let Some(playable_id) = track_playable_id(saved_track.track.id.clone()) {
              app.dispatch(IoEvent::AddItemToQueue(playable_id));
            }
          }
        }
      }
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::PlaylistSearch => {
        let TrackTable {
          selected_index,
          tracks,
          ..
        } = &app.track_table;
        if let Some(track) = tracks.get(*selected_index) {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
            app.dispatch(IoEvent::AddItemToQueue(playable_id));
          }
        };
      }
      TrackTableContext::DiscoverPlaylist => {
        if let Some(track) = tracks.get(*selected_index) {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
            app.dispatch(IoEvent::AddItemToQueue(playable_id));
          }
        }
      }
    }
  };
}

fn jump_to_start(app: &mut App) {
  if let Some(context) = &app.track_table.context {
    match context {
      TrackTableContext::MyPlaylists => {
        if let (Some(playlists), Some(selected_playlist_index)) =
          (&app.playlists, &app.selected_playlist_index)
        {
          if let Some(selected_playlist) = playlists.items.get(selected_playlist_index.to_owned()) {
            app.playlist_offset = 0;
            let playlist_id = playlist_id_static_from_ref(&selected_playlist.id);
            app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
          }
        }
      }
      TrackTableContext::RecommendedTracks => {}
      TrackTableContext::SavedTracks => {}
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::PlaylistSearch => {}
      TrackTableContext::DiscoverPlaylist => {}
    }
  }
}

fn playlist_id_static_from_ref(id: &PlaylistId<'_>) -> PlaylistId<'static> {
  id.clone().into_static()
}

fn playlist_context_id_from_ref(id: &PlaylistId<'_>) -> PlayContextId<'static> {
  PlayContextId::Playlist(id.clone().into_static())
}

fn track_playable_id(id: Option<TrackId<'_>>) -> Option<PlayableId<'static>> {
  id.map(|track_id| PlayableId::Track(track_id.into_static()))
}
