use crate::core::app::{
  ActiveBlock, App, DialogContext, PendingPlaylistTrackRemoval, PendingTrackSelection,
  RecommendationsContext, RouteId, TrackTableContext, LIBRARY_OPTIONS,
};
use crate::infra::network::sync::{ControlMode, PlaybackAction};
use crate::infra::network::IoEvent;
use rand::{thread_rng, Rng};
use rspotify::model::idtypes::{PlayContextId, PlayableId, PlaylistId, TrackId};
use rspotify::prelude::Id;
use std::collections::HashSet;

pub fn open_home(app: &mut App) {
  app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
}

pub fn open_search(app: &mut App, query: Option<String>) {
  if let Some(query) = query {
    app.input = query.chars().collect();
    app.input_idx = app.input.len();
    app.input_cursor_position = app.input.len() as u16;
    if !query.trim().is_empty() {
      app.dispatch(IoEvent::GetSearchResults(query, app.get_user_country()));
    }
  }
  app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);
}

pub fn open_library_item(app: &mut App, index: usize) {
  app.library.selected_index = index.min(LIBRARY_OPTIONS.len().saturating_sub(1));
  match app.library.selected_index {
    0 => app.push_navigation_stack(RouteId::Discover, ActiveBlock::Discover),
    1 => {
      app.dispatch(IoEvent::GetRecentlyPlayed);
      app.push_navigation_stack(RouteId::RecentlyPlayed, ActiveBlock::RecentlyPlayed);
    }
    2 => open_saved_tracks(app),
    3 => {
      app.dispatch(IoEvent::GetCurrentUserSavedAlbums(None));
      app.push_navigation_stack(RouteId::AlbumList, ActiveBlock::AlbumList);
    }
    4 => {
      app.dispatch(IoEvent::GetFollowedArtists(None));
      app.push_navigation_stack(RouteId::Artists, ActiveBlock::Artists);
    }
    5 => {
      app.dispatch(IoEvent::GetCurrentUserSavedShows(None));
      app.push_navigation_stack(RouteId::Podcasts, ActiveBlock::Podcasts);
    }
    _ => {}
  }
}

pub fn open_saved_tracks(app: &mut App) {
  app.reset_saved_tracks_view();
  app.dispatch(IoEvent::GetCurrentSavedTracks(None));
  app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
}

pub fn open_queue(app: &mut App) {
  app.dispatch(IoEvent::GetQueue);
  app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);
}

pub fn open_settings(app: &mut App) {
  app.load_settings_for_category();
  app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
}

pub fn open_party(app: &mut App) {
  app.push_navigation_stack(RouteId::Party, ActiveBlock::Party);
}

pub fn open_lyrics(app: &mut App) {
  app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);
}

pub fn open_create_playlist(app: &mut App) {
  app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
}

pub fn select_playlist_by_id(app: &mut App, playlist_id: &str) {
  let Some(playlist) = app
    .all_playlists
    .iter()
    .find(|playlist| playlist.id.id() == playlist_id)
    .cloned()
    .or_else(|| {
      app.playlists.as_ref().and_then(|page| {
        page
          .items
          .iter()
          .find(|playlist| playlist.id.id() == playlist_id)
          .cloned()
      })
    })
  else {
    app.dispatch(IoEvent::GetPlaylists);
    app.set_status_message("Playlists loading, try again".to_string(), 4);
    return;
  };

  app.active_playlist_index = app
    .all_playlists
    .iter()
    .position(|item| item.id.id() == playlist.id.id());
  app.reset_playlist_tracks_view(
    playlist.id.clone().into_static(),
    TrackTableContext::MyPlaylists,
  );
  app.dispatch(IoEvent::GetPlaylistItems(
    playlist.id.clone().into_static(),
    0,
  ));
  app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
}

pub fn select_track_table_row(app: &mut App, index: usize) {
  if app.track_table.tracks.is_empty() {
    app.track_table.selected_index = 0;
    return;
  }
  app.track_table.selected_index = index.min(app.track_table.tracks.len().saturating_sub(1));
}

pub fn track_table_next_page(app: &mut App) {
  match app.track_table.context {
    Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
      app.get_playlist_tracks_next();
    }
    Some(TrackTableContext::SavedTracks) => app.get_current_user_saved_tracks_next(),
    _ => {}
  }
}

pub fn track_table_previous_page(app: &mut App) {
  match app.track_table.context {
    Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
      app.get_playlist_tracks_previous();
    }
    Some(TrackTableContext::SavedTracks) => app.get_current_user_saved_tracks_previous(),
    _ => {}
  }
}

pub fn move_track_selection(app: &mut App, delta: i32) {
  let len = app.track_table.tracks.len();
  if len == 0 {
    app.track_table.selected_index = 0;
    return;
  }

  if delta > 0 && app.track_table.selected_index == len - 1 {
    match app.track_table.context {
      Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
        if app
          .current_playlist_track_page()
          .is_some_and(|page| page.next.is_some())
        {
          app.pending_track_table_selection = Some(PendingTrackSelection::First);
          app.get_playlist_tracks_next();
          return;
        }
      }
      Some(TrackTableContext::SavedTracks) => {
        if app
          .library
          .saved_tracks
          .get_results(None)
          .is_some_and(|page| page.offset + page.limit < page.total)
        {
          app.pending_track_table_selection = Some(PendingTrackSelection::First);
          app.get_current_user_saved_tracks_next();
          return;
        }
      }
      _ => {}
    }
  }

  if delta < 0 && app.track_table.selected_index == 0 {
    match app.track_table.context {
      Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
        if app
          .current_playlist_track_page()
          .is_some_and(|page| page.offset > 0)
        {
          app.pending_track_table_selection = Some(PendingTrackSelection::Last);
          app.get_playlist_tracks_previous();
        }
        return;
      }
      Some(TrackTableContext::SavedTracks) => {
        if app.library.saved_tracks.index > 0 {
          app.pending_track_table_selection = Some(PendingTrackSelection::Last);
          app.get_current_user_saved_tracks_previous();
        }
        return;
      }
      _ => {}
    }
  }

  let next = (app.track_table.selected_index as i32 + delta).clamp(0, len.saturating_sub(1) as i32);
  app.track_table.selected_index = next as usize;
}

pub fn play_selected_track(app: &mut App) {
  match app.track_table.context {
    Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
      if let Some(track) = app.track_table.tracks.get(app.track_table.selected_index) {
        let playable_id = track_playable_id(track.id.clone());
        let context_id = current_playlist_context_id(app);
        if let Some(playable_id) = playable_id {
          app.dispatch(IoEvent::StartPlayback(
            context_id,
            Some(vec![playable_id]),
            Some(0),
          ));
        } else {
          app.dispatch(IoEvent::StartPlayback(
            context_id,
            None,
            Some(app.track_table.selected_index + app.playlist_offset as usize),
          ));
        }
      }
    }
    Some(TrackTableContext::SavedTracks) => {
      if let Some((playable_ids, offset)) = saved_tracks_playback_request(app) {
        app.dispatch(IoEvent::StartPlayback(
          None,
          Some(playable_ids),
          Some(offset),
        ));
      }
    }
    Some(TrackTableContext::RecommendedTracks) | Some(TrackTableContext::DiscoverPlaylist) => {
      play_visible_track_list(app);
    }
    _ => {}
  }
}

pub fn queue_selected_track(app: &mut App) {
  let playable_id = match app.track_table.context {
    Some(TrackTableContext::SavedTracks) => app
      .library
      .saved_tracks
      .get_results(None)
      .and_then(|page| page.items.get(app.track_table.selected_index))
      .and_then(|saved_track| track_playable_id(saved_track.track.id.clone())),
    _ => app
      .track_table
      .tracks
      .get(app.track_table.selected_index)
      .and_then(|track| track_playable_id(track.id.clone())),
  };

  if let Some(playable_id) = playable_id {
    app.dispatch(IoEvent::AddItemToQueue(playable_id));
  }
}

pub fn toggle_save_selected_track(app: &mut App) {
  if let Some(playable_id) = app
    .track_table
    .tracks
    .get(app.track_table.selected_index)
    .and_then(|track| track_playable_id(track.id.clone()))
  {
    app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
  }
}

pub fn open_add_selected_track_to_playlist(app: &mut App) {
  let Some(track) = app.track_table.tracks.get(app.track_table.selected_index) else {
    return;
  };

  app.begin_add_track_to_playlist_flow(
    track.id.clone().map(|id| id.into_static()),
    track.name.clone(),
  );
}

pub fn open_remove_selected_track_from_playlist(app: &mut App) {
  let Some((playlist_id, playlist_name)) = current_playlist_target_for_track_table_context(app)
  else {
    app.set_status_message(
      "Remove only works in selected playlist views".to_string(),
      4,
    );
    return;
  };

  let Some(track) = app.track_table.tracks.get(app.track_table.selected_index) else {
    return;
  };
  let track_name = track.name.clone();
  let Some(track_id) = track.id.clone().map(|id| id.into_static()) else {
    app.set_status_message("Track cannot be edited in playlist".to_string(), 4);
    return;
  };
  let Some(position) = app
    .playlist_track_positions
    .as_ref()
    .and_then(|positions| positions.get(app.track_table.selected_index))
    .copied()
  else {
    app.set_status_message("Cannot resolve track position for removal".to_string(), 4);
    return;
  };

  app.clear_dialog_state();
  app.pending_playlist_track_removal = Some(PendingPlaylistTrackRemoval {
    playlist_id,
    playlist_name,
    track_id,
    track_name,
    position,
  });
  app.push_navigation_stack(
    RouteId::Dialog,
    ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm),
  );
}

pub fn play_random_track(app: &mut App) {
  match app.track_table.context {
    Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
      if let (Some(context_id), Some(total_tracks)) = (
        current_playlist_context_id(app),
        app.current_playlist_track_total(),
      ) {
        if total_tracks > 0 {
          app.dispatch(IoEvent::StartPlayback(
            Some(context_id),
            None,
            Some(thread_rng().gen_range(0..total_tracks as usize)),
          ));
        }
      }
    }
    Some(TrackTableContext::SavedTracks) => {
      if let Some(page) = app.library.saved_tracks.get_results(None) {
        let playable_ids: Vec<PlayableId<'static>> = page
          .items
          .iter()
          .filter_map(|item| track_playable_id(item.track.id.clone()))
          .collect();
        if !playable_ids.is_empty() {
          let index = thread_rng().gen_range(0..playable_ids.len());
          app.dispatch(IoEvent::StartPlayback(
            None,
            Some(playable_ids),
            Some(index),
          ));
        }
      }
    }
    Some(TrackTableContext::DiscoverPlaylist) => play_visible_track_list_random(app),
    _ => {}
  }
}

pub fn recommendations_for_selected_track(app: &mut App) {
  if let Some(track) = app.track_table.tracks.get(app.track_table.selected_index) {
    let first_track = track.clone();
    let track_id_list = track.id.as_ref().map(|id| vec![id.to_string()]);
    app.recommendations_context = Some(RecommendationsContext::Song);
    app.recommendations_seed = first_track.name.clone();
    app.get_recommendations_for_seed(None, track_id_list, Some(first_track));
  }
}

pub fn start_party(app: &mut App, control_mode: ControlMode) {
  app.dispatch(IoEvent::StartParty(control_mode));
}

pub fn join_party(app: &mut App, code: String, name: String) {
  app.dispatch(IoEvent::JoinParty { code, name });
}

pub fn leave_party(app: &mut App) {
  app.dispatch(IoEvent::LeaveParty);
}

pub fn party_playback_command(app: &mut App, action: PlaybackAction) {
  app.dispatch(IoEvent::PartyPlaybackCommand(action));
}

fn play_visible_track_list(app: &mut App) {
  let mut playable_ids = Vec::new();
  let mut selected_offset = None;

  for (idx, track) in app.track_table.tracks.iter().enumerate() {
    if let Some(playable_id) = track_playable_id(track.id.clone()) {
      if idx == app.track_table.selected_index {
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

fn play_visible_track_list_random(app: &mut App) {
  let playable_ids: Vec<PlayableId<'static>> = app
    .track_table
    .tracks
    .iter()
    .filter_map(|track| track_playable_id(track.id.clone()))
    .collect();

  if !playable_ids.is_empty() {
    let index = thread_rng().gen_range(0..playable_ids.len());
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(playable_ids),
      Some(index),
    ));
  }
}

fn current_playlist_target_for_track_table_context(
  app: &App,
) -> Option<(PlaylistId<'static>, String)> {
  let playlist_id = app.current_playlist_track_table_id()?;
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
  app
    .current_playlist_track_table_id()
    .map(PlayContextId::Playlist)
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
