use super::common_key_events;
use crate::core::app::{
  ActiveBlock, App, DialogContext, PendingPlaylistTrackRemoval, PendingTrackSelection,
  RecommendationsContext, RouteId, TrackTable, TrackTableContext,
};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
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
            if let Some(playlist_id) = active_playlist_id_static(app) {
              if let Some(playlist_tracks) = &app.playlist_tracks {
                // Check if there are more tracks to fetch
                if app.playlist_offset + app.large_search_limit < playlist_tracks.total {
                  app.playlist_offset += app.large_search_limit;
                  app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
                  // Set pending selection to move to first track when new page loads
                  app.pending_track_table_selection = Some(PendingTrackSelection::First);
                  return;
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
                app.pending_track_table_selection = Some(PendingTrackSelection::First);
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
              if let Some(playlist_id) = active_playlist_id_static(app) {
                app.playlist_offset = app.playlist_offset.saturating_sub(app.large_search_limit);
                app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
                // Set pending selection to move to last track when previous page loads
                app.pending_track_table_selection = Some(PendingTrackSelection::Last);
                return;
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
              // Set pending selection to move to last track when previous page loads
              app.pending_track_table_selection = Some(PendingTrackSelection::Last);
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
            if let Some(playlist_id) = active_playlist_id_static(app) {
              if let Some(playlist_tracks) = &app.playlist_tracks {
                if app.playlist_offset + app.large_search_limit < playlist_tracks.total {
                  app.playlist_offset += app.large_search_limit;
                  app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
                }
              }
            }
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
            if let Some(playlist_id) = active_playlist_id_static(app) {
              if app.playlist_offset >= app.large_search_limit {
                app.playlist_offset -= app.large_search_limit;
              }
              app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
            }
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
    Key::Char('w') => open_add_to_playlist_dialog(app),
    Key::Char('x') => open_remove_from_playlist_dialog(app),
    Key::Char('s') => handle_save_track_event(app),
    Key::Char('S') => play_random_song(app),
    k if k == app.user_config.keys.jump_to_end => jump_to_end(app),
    k if k == app.user_config.keys.jump_to_start => jump_to_start(app),
    //recommended song radio
    Key::Char('r') => {
      handle_recommended_tracks(app);
    }
    _ if key == app.user_config.keys.add_item_to_queue => on_queue(app),
    // Open sort menu
    Key::Char(',') => {
      super::sort_menu::open_sort_menu(app, crate::core::sort::SortContext::PlaylistTracks);
    }
    _ => {}
  }
}

fn open_add_to_playlist_dialog(app: &mut App) {
  let track = match app.track_table.tracks.get(app.track_table.selected_index) {
    Some(track) => track,
    None => return,
  };

  let track_id = track.id.clone().map(|id| id.into_static());
  let track_name = track.name.clone();
  app.begin_add_track_to_playlist_flow(track_id, track_name);
}

fn open_remove_from_playlist_dialog(app: &mut App) {
  let playlist_context = match active_playlist_target_for_track_table_context(app) {
    Some(context) => context,
    None => {
      app.set_status_message(
        "Remove only works in selected playlist views".to_string(),
        4,
      );
      return;
    }
  };

  let track = match app.track_table.tracks.get(app.track_table.selected_index) {
    Some(track) => track,
    None => return,
  };

  let track_id = match track.id.clone() {
    Some(id) => id.into_static(),
    None => {
      app.set_status_message("Track cannot be edited in playlist".to_string(), 4);
      return;
    }
  };
  let track_name = track.name.clone();

  let position = match app
    .playlist_track_positions
    .as_ref()
    .and_then(|positions| positions.get(app.track_table.selected_index))
    .copied()
  {
    Some(position) => position,
    None => {
      app.set_status_message("Cannot resolve track position for removal".to_string(), 4);
      return;
    }
  };

  app.clear_dialog_state();
  app.pending_playlist_track_removal = Some(PendingPlaylistTrackRemoval {
    playlist_id: playlist_context.0,
    playlist_name: playlist_context.1,
    track_id,
    track_name,
    position,
  });
  app.push_navigation_stack(
    RouteId::Dialog,
    ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm),
  );
}

fn play_random_song(app: &mut App) {
  if let Some(context) = &app.track_table.context {
    match context {
      TrackTableContext::MyPlaylists => {
        let context_id = active_playlist_context_id(app);
        let track_json = active_playlist_total_tracks(app);

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
        // Play random track from currently displayed discover playlist, but keep the full list
        // so next/previous can continue within the mix.
        let mut playable_ids: Vec<PlayableId<'static>> = Vec::new();
        for track in &app.track_table.tracks {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
            playable_ids.push(playable_id);
          }
        }
        if !playable_ids.is_empty() {
          let rand_idx = thread_rng().gen_range(0..playable_ids.len());
          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(playable_ids),
            Some(rand_idx),
          ));
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
        if let (Some(total_tracks), Some(playlist_id)) = (
          active_playlist_total_tracks(app),
          active_playlist_id_static(app),
        ) {
          if app.large_search_limit < total_tracks {
            app.playlist_offset = total_tracks - (total_tracks % app.large_search_limit);
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

          let context_id = match &app.active_playlist_index {
            Some(active_playlist_index) => app
              .all_playlists
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
        let playable_ids: Vec<PlayableId<'static>> = tracks
          .iter()
          .filter_map(|track| track_playable_id(track.id.clone()))
          .collect();
        if !playable_ids.is_empty() {
          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(playable_ids),
            Some(*selected_index),
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
        // Play the selected track, but include the full discover list so playback can continue.
        let mut playable_ids: Vec<PlayableId<'static>> = Vec::new();
        let mut selected_offset: Option<usize> = None;

        for (idx, track) in tracks.iter().enumerate() {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
            if idx == *selected_index {
              selected_offset = Some(playable_ids.len());
            }
            playable_ids.push(playable_id);
          }
        }

        if !playable_ids.is_empty() {
          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(playable_ids),
            Some(selected_offset.unwrap_or(0)),
          ));
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
        if let Some(track) = tracks.get(*selected_index) {
          if let Some(playable_id) = track_playable_id(track.id.clone()) {
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
        if let Some(playlist_id) = active_playlist_id_static(app) {
          app.playlist_offset = 0;
          app.dispatch(IoEvent::GetPlaylistItems(playlist_id, app.playlist_offset));
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

fn active_playlist_id_static(app: &App) -> Option<PlaylistId<'static>> {
  app
    .active_playlist_index
    .and_then(|idx| app.all_playlists.get(idx))
    .map(|playlist| playlist.id.clone().into_static())
}

fn active_playlist_target_for_track_table_context(
  app: &App,
) -> Option<(PlaylistId<'static>, String)> {
  match app.track_table.context {
    Some(TrackTableContext::MyPlaylists) => app
      .active_playlist_index
      .and_then(|idx| app.all_playlists.get(idx))
      .map(|playlist| (playlist.id.clone().into_static(), playlist.name.clone())),
    Some(TrackTableContext::PlaylistSearch) => app
      .search_results
      .selected_playlists_index
      .and_then(|idx| {
        app
          .search_results
          .playlists
          .as_ref()
          .and_then(|playlists| playlists.items.get(idx))
      })
      .map(|playlist| (playlist.id.clone().into_static(), playlist.name.clone())),
    _ => None,
  }
}

fn active_playlist_context_id(app: &App) -> Option<PlayContextId<'static>> {
  app
    .active_playlist_index
    .and_then(|idx| app.all_playlists.get(idx))
    .map(|playlist| playlist_context_id_from_ref(&playlist.id))
}

fn active_playlist_total_tracks(app: &App) -> Option<u32> {
  app
    .active_playlist_index
    .and_then(|idx| app.all_playlists.get(idx))
    .map(|playlist| playlist.tracks.total)
}

fn playlist_context_id_from_ref(id: &PlaylistId<'_>) -> PlayContextId<'static> {
  PlayContextId::Playlist(id.clone().into_static())
}

fn track_playable_id(id: Option<TrackId<'_>>) -> Option<PlayableId<'static>> {
  id.map(|track_id| PlayableId::Track(track_id.into_static()))
}
