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
use rspotify::prelude::Id;
use std::collections::HashSet;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k) => common_key_events::handle_left_event(app),
    k if common_key_events::down_event(k) => {
      let current_index = app.track_table.selected_index;
      let tracks_len = app.track_table.tracks.len();

      if tracks_len == 0 {
        return;
      }

      // Check if we're at the last track and there are more tracks to load
      if current_index == tracks_len - 1 {
        match &app.track_table.context {
          Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
            let has_next_page = app
              .current_playlist_track_page()
              .is_some_and(|playlist_tracks| {
                playlist_tracks.offset + playlist_tracks.limit < playlist_tracks.total
              });
            if has_next_page {
              app.pending_track_table_selection = Some(PendingTrackSelection::First);
              app.get_playlist_tracks_next();
              return;
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
                app.pending_track_table_selection = Some(PendingTrackSelection::First);
                app.get_current_user_saved_tracks_next();
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

      if app.track_table.tracks.is_empty() {
        return;
      }

      // Check if we're at the first track and there are previous tracks to load
      if current_index == 0 {
        match &app.track_table.context {
          Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
            if app
              .current_playlist_track_page()
              .is_some_and(|playlist_tracks| playlist_tracks.offset > 0)
            {
              app.pending_track_table_selection = Some(PendingTrackSelection::Last);
              app.get_playlist_tracks_previous();
              return;
            }
            return;
          }
          Some(TrackTableContext::DiscoverPlaylist) => {
            // Discover playlists don't support pagination
          }
          Some(TrackTableContext::SavedTracks) => {
            // Check if there are previous saved tracks to load
            if app.library.saved_tracks.index > 0 {
              // Set pending selection to move to last track when previous page loads
              app.pending_track_table_selection = Some(PendingTrackSelection::Last);
              app.get_current_user_saved_tracks_previous();
              return;
            }
            return;
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
          TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
            if app
              .current_playlist_track_page()
              .is_some_and(|playlist_tracks| {
                playlist_tracks.offset + playlist_tracks.limit < playlist_tracks.total
              })
            {
              app.get_playlist_tracks_next();
            }
          }
          TrackTableContext::RecommendedTracks => {}
          TrackTableContext::SavedTracks => {
            app.get_current_user_saved_tracks_next();
          }
          TrackTableContext::AlbumSearch => {}
          TrackTableContext::DiscoverPlaylist => {}
        }
      };
    }
    // Scroll up
    k if k == app.user_config.keys.previous_page => {
      if let Some(context) = &app.track_table.context {
        match context {
          TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
            app.get_playlist_tracks_previous();
          }
          TrackTableContext::RecommendedTracks => {}
          TrackTableContext::SavedTracks => {
            app.get_current_user_saved_tracks_previous();
          }
          TrackTableContext::AlbumSearch => {}
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
  let playlist_context = match current_playlist_target_for_track_table_context(app) {
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
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
        let context_id = current_playlist_context_id(app);
        let track_json = current_playlist_total_tracks(app);

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
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
        if let (Some(total_tracks), Some(playlist_id)) = (
          current_playlist_total_tracks(app),
          current_playlist_id_static(app),
        ) {
          if total_tracks > 0 {
            let last_page_offset =
              ((total_tracks - 1) / app.large_search_limit) * app.large_search_limit;
            if let Some(cached_index) = app
              .playlist_track_pages
              .page_index_for_offset(last_page_offset)
            {
              app.show_playlist_tracks_page_at_index(cached_index);
              return;
            }

            app.dispatch(IoEvent::GetPlaylistItems(playlist_id, last_page_offset));
          }
        }
      }
      TrackTableContext::RecommendedTracks => {}
      TrackTableContext::SavedTracks => {}
      TrackTableContext::AlbumSearch => {}
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
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
        if let Some(track) = tracks.get(*selected_index) {
          // Get the track ID to play
          let track_playable_id = track_playable_id(track.id.clone());
          let context_id = current_playlist_context_id(app);

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
      TrackTableContext::SavedTracks => {
        if let Some((all_playable_ids, absolute_offset)) = saved_tracks_playback_request(app) {
          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(all_playable_ids),
            Some(absolute_offset),
          ));
        }
      }
      TrackTableContext::AlbumSearch => {}
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
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
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
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
        if let Some(cached_index) = app.playlist_track_pages.page_index_for_offset(0) {
          app.show_playlist_tracks_page_at_index(cached_index);
        } else if let Some(playlist_id) = current_playlist_id_static(app) {
          app.dispatch(IoEvent::GetPlaylistItems(playlist_id, 0));
        }
      }
      TrackTableContext::RecommendedTracks => {}
      TrackTableContext::SavedTracks => {}
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::DiscoverPlaylist => {}
    }
  }
}

fn current_playlist_id_static(app: &App) -> Option<PlaylistId<'static>> {
  app.current_playlist_track_table_id()
}

fn current_playlist_target_for_track_table_context(
  app: &App,
) -> Option<(PlaylistId<'static>, String)> {
  let playlist_id = current_playlist_id_static(app)?;
  let playlist_name = playlist_name_for_id(app, &playlist_id)?;
  Some((playlist_id, playlist_name))
}

fn playlist_name_for_id(app: &App, playlist_id: &PlaylistId<'_>) -> Option<String> {
  app
    .all_playlists
    .iter()
    .find(|playlist| playlist.id.id() == playlist_id.id())
    .map(|playlist| playlist.name.clone())
    .or_else(|| {
      app
        .search_results
        .playlists
        .as_ref()
        .and_then(|playlists| {
          playlists
            .items
            .iter()
            .find(|playlist| playlist.id.id() == playlist_id.id())
        })
        .map(|playlist| playlist.name.clone())
    })
}

fn current_playlist_context_id(app: &App) -> Option<PlayContextId<'static>> {
  current_playlist_id_static(app).map(|playlist_id| playlist_context_id_from_ref(&playlist_id))
}

fn current_playlist_total_tracks(app: &App) -> Option<u32> {
  app.current_playlist_track_total()
}

fn playlist_context_id_from_ref(id: &PlaylistId<'_>) -> PlayContextId<'static> {
  PlayContextId::Playlist(id.clone().into_static())
}

fn track_playable_id(id: Option<TrackId<'_>>) -> Option<PlayableId<'static>> {
  id.map(|track_id| PlayableId::Track(track_id.into_static()))
}

fn saved_tracks_playback_request(app: &App) -> Option<(Vec<PlayableId<'static>>, usize)> {
  let current_page = app.library.saved_tracks.get_results(None)?;
  let selected_row_offset = current_page.offset as usize + app.track_table.selected_index;
  let estimated_tracks = app
    .library
    .saved_tracks
    .pages
    .iter()
    .map(|page| page.items.len())
    .sum();
  let mut playable_ids = Vec::with_capacity(estimated_tracks);
  let mut selected_playable_offset = None;
  let mut seen_offsets = HashSet::new();

  for page in &app.library.saved_tracks.pages {
    if !seen_offsets.insert(page.offset) {
      continue;
    }

    for (item_index, item) in page.items.iter().enumerate() {
      if let Some(playable_id) = track_playable_id(item.track.id.clone()) {
        if page.offset as usize + item_index == selected_row_offset {
          selected_playable_offset = Some(playable_ids.len());
        }
        playable_ids.push(playable_id);
      }
    }
  }

  selected_playable_offset.map(|offset| (playable_ids, offset))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::full_track;
  use crate::core::user_config::UserConfig;
  use chrono::Utc;
  use rspotify::model::{page::Page, track::SavedTrack};
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn saved_track(id: &str, name: &str) -> SavedTrack {
    SavedTrack {
      added_at: Utc::now(),
      track: full_track(id, name),
    }
  }

  fn saved_tracks_page(offset: u32, ids: &[&str], has_next: bool) -> Page<SavedTrack> {
    Page {
      href: "https://example.com/me/tracks".to_string(),
      items: ids
        .iter()
        .enumerate()
        .map(|(index, id)| saved_track(id, &format!("Track {offset}-{index}")))
        .collect(),
      limit: ids.len() as u32,
      next: has_next.then(|| "https://example.com/me/tracks?next".to_string()),
      offset,
      previous: None,
      total: 4,
    }
  }

  fn app_with_saved_tracks() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.track_table.context = Some(TrackTableContext::SavedTracks);
    (app, rx)
  }

  #[test]
  fn saved_tracks_playback_request_uses_page_zero_selection() {
    let (mut app, _rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &[
        "0000000000000000000001",
        "0000000000000000000002",
        "0000000000000000000003",
      ],
      false,
    );
    app.track_table.selected_index = 1;
    app.track_table.tracks = page.items.iter().map(|item| item.track.clone()).collect();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    let (uris, offset) = saved_tracks_playback_request(&app).unwrap();

    assert_eq!(offset, 1);
    assert_eq!(uris.len(), 3);
    assert_eq!(uris[offset].uri(), "spotify:track:0000000000000000000002");
  }

  #[test]
  fn saved_tracks_playback_request_uses_absolute_offset_on_later_pages() {
    let (mut app, _rx) = app_with_saved_tracks();
    let first_page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let second_page = saved_tracks_page(
      2,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    );
    app.library.saved_tracks.upsert_page_by_offset(first_page);
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(second_page.clone());
    app.library.saved_tracks.index = 1;
    app.track_table.selected_index = 1;
    app.track_table.tracks = second_page
      .items
      .iter()
      .map(|item| item.track.clone())
      .collect();

    let (uris, offset) = saved_tracks_playback_request(&app).unwrap();

    assert_eq!(offset, 3);
    assert_eq!(uris.len(), 4);
    assert_eq!(uris[offset].uri(), "spotify:track:0000000000000000000004");
  }

  #[test]
  fn enter_dispatches_saved_tracks_playback_for_selected_song() {
    let (mut app, rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );
    app.track_table.selected_index = 1;
    app.track_table.tracks = page.items.iter().map(|item| item.track.clone()).collect();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    handler(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::StartPlayback(None, Some(uris), Some(offset)) => {
        assert_eq!(offset, 1);
        assert_eq!(uris[offset].uri(), "spotify:track:0000000000000000000002");
      }
      other => panic!("unexpected event: {:?}", event_name(&other)),
    }
  }

  #[test]
  fn empty_track_table_down_event_does_not_panic() {
    let (mut app, _rx) = app_with_saved_tracks();
    app.track_table.tracks.clear();
    app.track_table.selected_index = 0;

    handler(Key::Down, &mut app);

    assert_eq!(app.track_table.selected_index, 0);
  }

  #[test]
  fn up_on_first_saved_tracks_row_without_previous_page_does_not_wrap() {
    let (mut app, _rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    app.track_table.selected_index = 0;
    app.track_table.tracks = page.items.iter().map(|item| item.track.clone()).collect();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    handler(Key::Up, &mut app);

    assert_eq!(app.track_table.selected_index, 0);
  }

  #[test]
  fn up_on_first_playlist_row_without_previous_page_does_not_wrap() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.track_table.tracks = vec![
      full_track("0000000000000000000001", "Track 1"),
      full_track("0000000000000000000002", "Track 2"),
    ];
    app.track_table.selected_index = 0;
    app.playlist_offset = 0;

    handler(Key::Up, &mut app);

    assert_eq!(app.track_table.selected_index, 0);
  }

  #[test]
  fn saved_tracks_playback_request_ignores_duplicate_page_offsets() {
    let (mut app, _rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );
    app.library.saved_tracks.add_pages(page.clone());
    app.library.saved_tracks.add_pages(page);
    app.library.saved_tracks.index = 0;
    app.track_table.selected_index = 1;
    app.track_table.tracks = app.library.saved_tracks.pages[0]
      .items
      .iter()
      .map(|item| item.track.clone())
      .collect();

    let (uris, offset) = saved_tracks_playback_request(&app).unwrap();

    assert_eq!(offset, 1);
    assert_eq!(uris.len(), 2);
    assert_eq!(uris[offset].uri(), "spotify:track:0000000000000000000002");
  }

  fn event_name(event: &IoEvent) -> &'static str {
    match event {
      IoEvent::StartPlayback(_, _, _) => "StartPlayback",
      _ => "other",
    }
  }
}
