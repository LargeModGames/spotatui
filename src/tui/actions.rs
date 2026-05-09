use crate::core::app::{
  ActiveBlock, AlbumTableContext, App, ArtistBlock, DialogContext, PendingPlaylistTrackRemoval,
  PendingTrackSelection, RecommendationsContext, RouteId, SettingsCategory, TrackTableContext,
  LIBRARY_OPTIONS,
};
use crate::gui::{GuiIndexedBlock, GuiSortContextId};
use crate::infra::network::sync::{ControlMode, PlaybackAction};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
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

pub fn open_help(app: &mut App) {
  app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
}

pub fn open_devices(app: &mut App) {
  app.dispatch(IoEvent::GetDevices);
  app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
}

pub fn open_cover_art(app: &mut App) {
  app.push_navigation_stack(RouteId::CoverArtView, ActiveBlock::CoverArtView);
}

pub fn open_audio_analysis(app: &mut App) {
  app.get_audio_analysis();
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

pub fn open_sort_menu(app: &mut App, context: GuiSortContextId) {
  crate::tui::handlers::sort_menu::open_sort_menu(app, sort_context_from_gui(context));
}

pub fn close_sort_menu(app: &mut App) {
  crate::tui::handlers::sort_menu::handler(Key::Esc, app);
}

pub fn select_sort_option(app: &mut App, index: usize) {
  let available_fields = match app.sort_context {
    Some(context) => context.available_fields(),
    None => return,
  };

  if available_fields.is_empty() {
    return;
  }

  app.sort_menu_selected = index.min(available_fields.len().saturating_sub(1));
}

pub fn apply_sort_option(app: &mut App, index: Option<usize>) {
  if let Some(index) = index {
    select_sort_option(app, index);
  }
  crate::tui::handlers::sort_menu::handler(Key::Enter, app);
}

pub fn select_queue_item(app: &mut App, index: usize) {
  let len = app.queue.as_ref().map_or(0, |queue| {
    let now = usize::from(queue.currently_playing.is_some());
    now + queue.queue.len()
  });

  if len == 0 {
    app.queue_selected_index = 0;
    return;
  }

  app.queue_selected_index = index.min(len.saturating_sub(1));
}

pub fn select_dialog_index(app: &mut App, index: usize) {
  match app.get_current_route().active_block {
    ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker) => {
      let count = app.editable_playlists().len();
      if count == 0 {
        app.playlist_picker_selected_index = 0;
      } else {
        app.playlist_picker_selected_index = index.min(count.saturating_sub(1));
      }
    }
    ActiveBlock::Dialog(_) => {
      app.confirm = index == 0;
    }
    _ => {}
  }
}

pub fn set_dialog_confirm(app: &mut App, confirm: bool) {
  if !matches!(
    app.get_current_route().active_block,
    ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
  ) {
    app.confirm = confirm;
  }
}

pub fn confirm_dialog(app: &mut App) {
  crate::tui::handlers::dialog::handler(Key::Enter, app);
}

pub fn cancel_dialog(app: &mut App) {
  crate::tui::handlers::dialog::handler(Key::Char('q'), app);
}

pub fn dismiss_announcement(app: &mut App) {
  crate::tui::handlers::announcement_prompt::handler(Key::Enter, app);
}

pub fn select_settings_category(app: &mut App, index: usize) {
  let categories = SettingsCategory::all();
  if categories.is_empty() {
    return;
  }

  app.settings_category = SettingsCategory::from_index(index.min(categories.len() - 1));
  app.settings_selected_index = 0;
  app.load_settings_for_category();
}

pub fn select_settings_item(app: &mut App, index: usize) {
  if app.settings_items.is_empty() {
    app.settings_selected_index = 0;
    return;
  }

  app.settings_selected_index = index.min(app.settings_items.len().saturating_sub(1));
}

pub fn activate_setting(app: &mut App) {
  crate::tui::handlers::settings::handler(Key::Enter, app);
}

pub fn update_settings_edit_buffer(app: &mut App, value: String) {
  if app.settings_edit_mode {
    app.settings_edit_buffer = value;
  }
}

pub fn commit_settings_edit(app: &mut App) {
  crate::tui::handlers::settings::handler(Key::Enter, app);
}

pub fn cancel_settings_edit(app: &mut App) {
  crate::tui::handlers::settings::handler(Key::Esc, app);
}

pub fn save_settings(app: &mut App) {
  crate::tui::handlers::settings::handler(app.effective_save_settings_key(), app);
}

pub fn resolve_settings_unsaved_prompt(app: &mut App, save: bool) {
  if !app.settings_unsaved_prompt_visible {
    return;
  }

  app.settings_unsaved_prompt_save_selected = save;
  crate::tui::handlers::settings::handler(Key::Enter, app);
}

pub fn cycle_visualizer_style(app: &mut App) {
  crate::tui::handlers::analysis::handler(Key::Char('V'), app);
}

pub fn open_indexed_item(app: &mut App, block: GuiIndexedBlock, index: usize) {
  match block {
    GuiIndexedBlock::TrackTable => {
      select_track_table_row(app, index);
      play_selected_track(app);
    }
    GuiIndexedBlock::SearchTracks => {
      if let Some(index) = select_search_track(app, index) {
        play_search_track(app, index);
      }
    }
    GuiIndexedBlock::SearchAlbums => {
      open_search_album(app, index);
    }
    GuiIndexedBlock::SearchArtists => {
      open_search_artist(app, index);
    }
    GuiIndexedBlock::SearchPlaylists => {
      open_search_playlist(app, index);
    }
    GuiIndexedBlock::SearchShows => {
      open_search_show(app, index);
    }
    GuiIndexedBlock::SavedAlbums => {
      open_saved_album(app, index);
    }
    GuiIndexedBlock::SavedArtists => {
      open_saved_artist(app, index);
    }
    GuiIndexedBlock::SavedPodcasts => {
      open_saved_podcast(app, index);
    }
    GuiIndexedBlock::AlbumTracks => {
      play_album_track(app, index);
    }
    GuiIndexedBlock::ArtistTopTracks => {
      play_artist_top_track(app, index);
    }
    GuiIndexedBlock::ArtistAlbums => {
      open_artist_album(app, index);
    }
    GuiIndexedBlock::ArtistRelatedArtists => {
      open_related_artist(app, index);
    }
    GuiIndexedBlock::PodcastEpisodes => {
      play_podcast_episode(app, index);
    }
    GuiIndexedBlock::RecentlyPlayed => {
      play_recently_played_track(app, index);
    }
    GuiIndexedBlock::DiscoverTopTracks => {
      play_discover_track(app, index, true);
    }
    GuiIndexedBlock::DiscoverArtistsMix => {
      play_discover_track(app, index, false);
    }
    GuiIndexedBlock::Queue => {
      select_queue_item(app, index);
    }
  }
}

pub fn play_indexed_item(app: &mut App, block: GuiIndexedBlock, index: usize) {
  open_indexed_item(app, block, index);
}

pub fn queue_indexed_item(app: &mut App, block: GuiIndexedBlock, index: usize) {
  match block {
    GuiIndexedBlock::TrackTable => {
      select_track_table_row(app, index);
      queue_selected_track(app);
    }
    GuiIndexedBlock::SearchTracks => queue_search_track(app, index),
    GuiIndexedBlock::AlbumTracks => queue_album_track(app, index),
    GuiIndexedBlock::ArtistTopTracks => queue_artist_top_track(app, index),
    GuiIndexedBlock::PodcastEpisodes => queue_podcast_episode(app, index),
    GuiIndexedBlock::RecentlyPlayed => queue_recently_played_track(app, index),
    GuiIndexedBlock::DiscoverTopTracks => queue_discover_track(app, index, true),
    GuiIndexedBlock::DiscoverArtistsMix => queue_discover_track(app, index, false),
    _ => {}
  }
}

pub fn toggle_save_indexed_item(app: &mut App, block: GuiIndexedBlock, index: usize) {
  match block {
    GuiIndexedBlock::TrackTable => {
      select_track_table_row(app, index);
      toggle_save_selected_track(app);
    }
    GuiIndexedBlock::SearchTracks => toggle_save_search_track(app, index),
    GuiIndexedBlock::SearchAlbums => toggle_save_search_album(app, index),
    GuiIndexedBlock::SearchArtists => toggle_follow_search_artist(app, index),
    GuiIndexedBlock::SearchShows => toggle_save_search_show(app, index),
    GuiIndexedBlock::SavedAlbums => toggle_save_saved_album(app, index),
    GuiIndexedBlock::SavedArtists => toggle_follow_saved_artist(app, index),
    GuiIndexedBlock::SavedPodcasts => toggle_save_saved_podcast(app, index),
    GuiIndexedBlock::AlbumTracks => toggle_save_album_track(app, index),
    GuiIndexedBlock::ArtistTopTracks => toggle_save_artist_top_track(app, index),
    GuiIndexedBlock::ArtistAlbums => toggle_save_artist_album(app, index),
    GuiIndexedBlock::ArtistRelatedArtists => toggle_follow_related_artist(app, index),
    GuiIndexedBlock::PodcastEpisodes => toggle_save_current_show(app),
    GuiIndexedBlock::RecentlyPlayed => toggle_save_recently_played_track(app, index),
    GuiIndexedBlock::DiscoverTopTracks => toggle_save_discover_track(app, index, true),
    GuiIndexedBlock::DiscoverArtistsMix => toggle_save_discover_track(app, index, false),
    _ => {}
  }
}

pub fn add_indexed_item_to_playlist(app: &mut App, block: GuiIndexedBlock, index: usize) {
  match block {
    GuiIndexedBlock::TrackTable => {
      select_track_table_row(app, index);
      open_add_selected_track_to_playlist(app);
    }
    GuiIndexedBlock::SearchTracks => add_search_track_to_playlist(app, index),
    GuiIndexedBlock::AlbumTracks => add_album_track_to_playlist(app, index),
    GuiIndexedBlock::ArtistTopTracks => add_artist_top_track_to_playlist(app, index),
    GuiIndexedBlock::RecentlyPlayed => add_recently_played_track_to_playlist(app, index),
    GuiIndexedBlock::DiscoverTopTracks => add_discover_track_to_playlist(app, index, true),
    GuiIndexedBlock::DiscoverArtistsMix => add_discover_track_to_playlist(app, index, false),
    _ => {}
  }
}

pub fn recommend_indexed_item(app: &mut App, block: GuiIndexedBlock, index: usize) {
  match block {
    GuiIndexedBlock::TrackTable => {
      select_track_table_row(app, index);
      recommendations_for_selected_track(app);
    }
    GuiIndexedBlock::SearchTracks => recommend_search_track(app, index),
    GuiIndexedBlock::SearchArtists => recommend_search_artist(app, index),
    GuiIndexedBlock::ArtistTopTracks => recommend_artist_top_track(app, index),
    GuiIndexedBlock::ArtistRelatedArtists => recommend_related_artist(app, index),
    GuiIndexedBlock::RecentlyPlayed => recommend_recently_played_track(app, index),
    GuiIndexedBlock::DiscoverTopTracks => recommend_discover_track(app, index, true),
    GuiIndexedBlock::DiscoverArtistsMix => recommend_discover_track(app, index, false),
    GuiIndexedBlock::SavedArtists => recommend_saved_artist(app, index),
    _ => {}
  }
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

fn select_search_track(app: &mut App, index: usize) -> Option<usize> {
  let tracks = app.search_results.tracks.as_ref()?;
  let selected = index.min(tracks.items.len().saturating_sub(1));
  app.search_results.selected_block = crate::core::app::SearchResultBlock::SongSearch;
  app.search_results.hovered_block = crate::core::app::SearchResultBlock::SongSearch;
  app.search_results.selected_tracks_index = Some(selected);
  Some(selected)
}

fn play_search_track(app: &mut App, index: usize) {
  let tracks = match &app.search_results.tracks {
    Some(tracks) => tracks,
    None => return,
  };
  let selected = index.min(tracks.items.len().saturating_sub(1));
  app.search_results.selected_block = crate::core::app::SearchResultBlock::SongSearch;
  app.search_results.selected_tracks_index = Some(selected);
  let playable_ids: Vec<PlayableId<'static>> = tracks
    .items
    .iter()
    .filter_map(|track| track_playable_id(track.id.clone()))
    .collect();
  if !playable_ids.is_empty() {
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(playable_ids),
      Some(selected),
    ));
  }
}

fn queue_search_track(app: &mut App, index: usize) {
  let Some(tracks) = &app.search_results.tracks else {
    return;
  };
  let Some(track) = tracks.items.get(index) else {
    return;
  };
  if let Some(playable_id) = track_playable_id(track.id.clone()) {
    app.dispatch(IoEvent::AddItemToQueue(playable_id));
  }
}

fn toggle_save_search_track(app: &mut App, index: usize) {
  let Some(tracks) = &app.search_results.tracks else {
    return;
  };
  let Some(track) = tracks.items.get(index) else {
    return;
  };
  if let Some(playable_id) = track_playable_id(track.id.clone()) {
    app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
  }
}

fn add_search_track_to_playlist(app: &mut App, index: usize) {
  let Some(tracks) = &app.search_results.tracks else {
    return;
  };
  let Some(track) = tracks.items.get(index) else {
    return;
  };
  app.begin_add_track_to_playlist_flow(
    track.id.clone().map(|id| id.into_static()),
    track.name.clone(),
  );
}

fn recommend_search_track(app: &mut App, index: usize) {
  let Some(tracks) = &app.search_results.tracks else {
    return;
  };
  let Some(track) = tracks.items.get(index) else {
    return;
  };
  let Some(track_id) = track.id.as_ref() else {
    return;
  };
  app.recommendations_context = Some(RecommendationsContext::Song);
  app.recommendations_seed = track.name.clone();
  app.get_recommendations_for_track_id(track_id.id().to_string());
}

fn open_search_album(app: &mut App, index: usize) {
  let Some(albums) = &app.search_results.albums else {
    return;
  };
  let Some(album) = albums.items.get(index).cloned() else {
    return;
  };
  app.search_results.selected_block = crate::core::app::SearchResultBlock::AlbumSearch;
  app.search_results.hovered_block = crate::core::app::SearchResultBlock::AlbumSearch;
  app.search_results.selected_album_index = Some(index);
  app.track_table.context = Some(TrackTableContext::AlbumSearch);
  app.dispatch(IoEvent::GetAlbumTracks(Box::new(album)));
}

fn toggle_save_search_album(app: &mut App, index: usize) {
  let Some(albums) = &app.search_results.albums else {
    return;
  };
  let Some(album) = albums.items.get(index) else {
    return;
  };
  let Some(album_id) = album.id.clone() else {
    return;
  };
  let saved = app.saved_album_ids_set.contains(album_id.id());
  if saved {
    app.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id.into_static()));
  } else {
    app.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id.into_static()));
  }
}

fn open_search_artist(app: &mut App, index: usize) {
  let Some(artists) = &app.search_results.artists else {
    return;
  };
  let Some(artist) = artists.items.get(index) else {
    return;
  };
  app.search_results.selected_block = crate::core::app::SearchResultBlock::ArtistSearch;
  app.search_results.hovered_block = crate::core::app::SearchResultBlock::ArtistSearch;
  app.search_results.selected_artists_index = Some(index);
  app.get_artist(artist.id.as_ref().into_static(), artist.name.clone());
}

fn recommend_search_artist(app: &mut App, index: usize) {
  let Some(artists) = &app.search_results.artists else {
    return;
  };
  let Some(artist) = artists.items.get(index) else {
    return;
  };
  app.recommendations_context = Some(RecommendationsContext::Artist);
  app.recommendations_seed = artist.name.clone();
  app.get_recommendations_for_seed(Some(vec![artist.id.id().to_string()]), None, None);
}

fn toggle_follow_search_artist(app: &mut App, index: usize) {
  let Some(artists) = &app.search_results.artists else {
    return;
  };
  let Some(artist) = artists.items.get(index) else {
    return;
  };
  if app.followed_artist_ids_set.contains(artist.id.id()) {
    app.dispatch(IoEvent::UserUnfollowArtists(vec![artist
      .id
      .clone()
      .into_static()]));
  } else {
    app.dispatch(IoEvent::UserFollowArtists(vec![artist
      .id
      .clone()
      .into_static()]));
  }
}

fn open_search_playlist(app: &mut App, index: usize) {
  let Some(playlists) = &app.search_results.playlists else {
    return;
  };
  let Some(playlist) = playlists.items.get(index) else {
    return;
  };
  app.search_results.selected_block = crate::core::app::SearchResultBlock::PlaylistSearch;
  app.search_results.hovered_block = crate::core::app::SearchResultBlock::PlaylistSearch;
  app.search_results.selected_playlists_index = Some(index);
  let playlist_id = playlist.id.id().to_string();
  select_playlist_by_id(app, &playlist_id);
}

fn open_search_show(app: &mut App, index: usize) {
  let Some(shows) = &app.search_results.shows else {
    return;
  };
  let Some(show) = shows.items.get(index).cloned() else {
    return;
  };
  app.search_results.selected_block = crate::core::app::SearchResultBlock::ShowSearch;
  app.search_results.hovered_block = crate::core::app::SearchResultBlock::ShowSearch;
  app.search_results.selected_shows_index = Some(index);
  app.dispatch(IoEvent::GetShowEpisodes(Box::new(show)));
}

fn toggle_save_search_show(app: &mut App, index: usize) {
  let Some(shows) = &app.search_results.shows else {
    return;
  };
  let Some(show) = shows.items.get(index) else {
    return;
  };
  if app.saved_show_ids_set.contains(show.id.id()) {
    app.dispatch(IoEvent::CurrentUserSavedShowDelete(
      show.id.clone().into_static(),
    ));
  } else {
    app.dispatch(IoEvent::CurrentUserSavedShowAdd(
      show.id.clone().into_static(),
    ));
  }
}

fn open_saved_album(app: &mut App, index: usize) {
  let Some(albums) = app.library.saved_albums.get_results(None) else {
    return;
  };
  let Some(selected_album) = albums.items.get(index) else {
    return;
  };
  app.album_list_index = index.min(albums.items.len().saturating_sub(1));
  app.selected_album_full = Some(crate::core::app::SelectedFullAlbum {
    album: selected_album.album.clone(),
    selected_index: 0,
  });
  app.album_table_context = AlbumTableContext::Full;
  app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
}

fn toggle_save_saved_album(app: &mut App, index: usize) {
  let Some(albums) = app.library.saved_albums.get_results(None) else {
    return;
  };
  let Some(selected_album) = albums.items.get(index) else {
    return;
  };
  let album_id = selected_album.album.id.clone().into_static();
  if app.saved_album_ids_set.contains(album_id.id()) {
    app.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id));
  } else {
    app.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id));
  }
}

fn open_saved_artist(app: &mut App, index: usize) {
  let Some(artists) = app.library.saved_artists.get_results(None) else {
    return;
  };
  let Some(artist) = artists.items.get(index) else {
    return;
  };
  app.artists_list_index = index.min(artists.items.len().saturating_sub(1));
  app.get_artist(artist.id.as_ref().into_static(), artist.name.clone());
}

fn recommend_saved_artist(app: &mut App, index: usize) {
  let Some(artists) = app.library.saved_artists.get_results(None) else {
    return;
  };
  let Some(artist) = artists.items.get(index) else {
    return;
  };
  app.recommendations_context = Some(RecommendationsContext::Artist);
  app.recommendations_seed = artist.name.clone();
  app.get_recommendations_for_seed(Some(vec![artist.id.id().to_string()]), None, None);
}

fn toggle_follow_saved_artist(app: &mut App, index: usize) {
  let Some(artists) = app.library.saved_artists.get_results(None) else {
    return;
  };
  let Some(artist) = artists.items.get(index) else {
    return;
  };
  if app.followed_artist_ids_set.contains(artist.id.id()) {
    app.dispatch(IoEvent::UserUnfollowArtists(vec![artist
      .id
      .clone()
      .into_static()]));
  } else {
    app.dispatch(IoEvent::UserFollowArtists(vec![artist
      .id
      .clone()
      .into_static()]));
  }
}

fn open_saved_podcast(app: &mut App, index: usize) {
  let Some(shows) = app.library.saved_shows.get_results(None) else {
    return;
  };
  let Some(show) = shows.items.get(index).cloned() else {
    return;
  };
  app.shows_list_index = index.min(shows.items.len().saturating_sub(1));
  app.dispatch(IoEvent::GetShowEpisodes(Box::new(show.show)));
}

fn toggle_save_saved_podcast(app: &mut App, index: usize) {
  let Some(shows) = app.library.saved_shows.get_results(None) else {
    return;
  };
  let Some(show) = shows.items.get(index) else {
    return;
  };
  let show_id = show.show.id.clone().into_static();
  if app.saved_show_ids_set.contains(show_id.id()) {
    app.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
  } else {
    app.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id));
  }
}

fn set_album_track_selection(app: &mut App, index: usize) -> bool {
  match app.album_table_context {
    AlbumTableContext::Full => {
      let Some(selected_album) = &app.selected_album_full else {
        return false;
      };
      let selected = index.min(selected_album.album.tracks.items.len().saturating_sub(1));
      app.saved_album_tracks_index = selected;
      true
    }
    AlbumTableContext::Simplified => {
      let Some(selected_album) = &mut app.selected_album_simplified else {
        return false;
      };
      let selected = index.min(selected_album.tracks.items.len().saturating_sub(1));
      selected_album.selected_index = selected;
      true
    }
  }
}

fn play_album_track(app: &mut App, index: usize) {
  if !set_album_track_selection(app, index) {
    return;
  }
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = app.selected_album_full.clone() {
        app.dispatch(IoEvent::StartPlayback(
          Some(PlayContextId::Album(selected_album.album.id.into_static())),
          None,
          Some(app.saved_album_tracks_index),
        ));
      }
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album) = app.selected_album_simplified.clone() {
        app.dispatch(IoEvent::StartPlayback(
          selected_album
            .album
            .id
            .map(|id| PlayContextId::Album(id.into_static())),
          None,
          Some(selected_album.selected_index),
        ));
      }
    }
  }
}

fn queue_album_track(app: &mut App, index: usize) {
  if !set_album_track_selection(app, index) {
    return;
  }
  let playable_id = match app.album_table_context {
    AlbumTableContext::Full => app
      .selected_album_full
      .as_ref()
      .and_then(|album| album.album.tracks.items.get(app.saved_album_tracks_index))
      .and_then(|track| track_playable_id(track.id.clone())),
    AlbumTableContext::Simplified => app
      .selected_album_simplified
      .as_ref()
      .and_then(|album| album.tracks.items.get(album.selected_index))
      .and_then(|track| track_playable_id(track.id.clone())),
  };
  if let Some(playable_id) = playable_id {
    app.dispatch(IoEvent::AddItemToQueue(playable_id));
  }
}

fn toggle_save_album_track(app: &mut App, index: usize) {
  if !set_album_track_selection(app, index) {
    return;
  }
  let playable_id = match app.album_table_context {
    AlbumTableContext::Full => app
      .selected_album_full
      .as_ref()
      .and_then(|album| album.album.tracks.items.get(app.saved_album_tracks_index))
      .and_then(|track| track_playable_id(track.id.clone())),
    AlbumTableContext::Simplified => app
      .selected_album_simplified
      .as_ref()
      .and_then(|album| album.tracks.items.get(album.selected_index))
      .and_then(|track| track_playable_id(track.id.clone())),
  };
  if let Some(playable_id) = playable_id {
    app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
  }
}

fn add_album_track_to_playlist(app: &mut App, index: usize) {
  if !set_album_track_selection(app, index) {
    return;
  }
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(track) = app
        .selected_album_full
        .as_ref()
        .and_then(|album| album.album.tracks.items.get(app.saved_album_tracks_index))
      {
        app.begin_add_track_to_playlist_flow(
          track.id.clone().map(|id| id.into_static()),
          track.name.clone(),
        );
      }
    }
    AlbumTableContext::Simplified => {
      if let Some(track) = app
        .selected_album_simplified
        .as_ref()
        .and_then(|album| album.tracks.items.get(album.selected_index))
      {
        app.begin_add_track_to_playlist_flow(
          track.id.clone().map(|id| id.into_static()),
          track.name.clone(),
        );
      }
    }
  }
}

fn play_artist_top_track(app: &mut App, index: usize) {
  let Some(artist) = &app.artist else {
    return;
  };
  if artist.top_tracks.is_empty() {
    return;
  }
  let selected = index.min(artist.top_tracks.len().saturating_sub(1));
  let playable_ids: Vec<PlayableId<'static>> = artist
    .top_tracks
    .iter()
    .filter_map(|track| track_playable_id(track.id.clone()))
    .collect();
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::TopTracks;
    artist.selected_top_track_index = selected;
  }
  if !playable_ids.is_empty() {
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(playable_ids),
      Some(selected),
    ));
  }
}

fn queue_artist_top_track(app: &mut App, index: usize) {
  let (selected, playable_id) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.top_tracks.len().saturating_sub(1));
    let playable_id = artist
      .top_tracks
      .get(selected)
      .and_then(|track| track_playable_id(track.id.clone()));
    (selected, playable_id)
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::TopTracks;
    artist.selected_top_track_index = selected;
  }
  if let Some(playable_id) = playable_id {
    app.dispatch(IoEvent::AddItemToQueue(playable_id));
  }
}

fn toggle_save_artist_top_track(app: &mut App, index: usize) {
  let (selected, playable_id) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.top_tracks.len().saturating_sub(1));
    let playable_id = artist
      .top_tracks
      .get(selected)
      .and_then(|track| track_playable_id(track.id.clone()));
    (selected, playable_id)
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::TopTracks;
    artist.selected_top_track_index = selected;
  }
  if let Some(playable_id) = playable_id {
    app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
  }
}

fn add_artist_top_track_to_playlist(app: &mut App, index: usize) {
  let (selected, track_id, track_name) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.top_tracks.len().saturating_sub(1));
    let Some(track) = artist.top_tracks.get(selected) else {
      return;
    };
    (
      selected,
      track.id.clone().map(|id| id.into_static()),
      track.name.clone(),
    )
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::TopTracks;
    artist.selected_top_track_index = selected;
  }
  app.begin_add_track_to_playlist_flow(track_id, track_name);
}

fn recommend_artist_top_track(app: &mut App, index: usize) {
  let (selected, seed, track_id_list, first_track) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.top_tracks.len().saturating_sub(1));
    let Some(track) = artist.top_tracks.get(selected) else {
      return;
    };
    (
      selected,
      track.name.clone(),
      track.id.as_ref().map(|id| vec![id.id().to_string()]),
      Some(track.clone()),
    )
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::TopTracks;
    artist.selected_top_track_index = selected;
  }
  app.recommendations_context = Some(RecommendationsContext::Song);
  app.recommendations_seed = seed;
  app.get_recommendations_for_seed(None, track_id_list, first_track);
}

fn open_artist_album(app: &mut App, index: usize) {
  let (selected, album) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.albums.items.len().saturating_sub(1));
    (selected, artist.albums.items.get(selected).cloned())
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::Albums;
    artist.selected_album_index = selected;
  }
  if let Some(album) = album {
    app.track_table.context = Some(TrackTableContext::AlbumSearch);
    app.dispatch(IoEvent::GetAlbumTracks(Box::new(album)));
  }
}

fn toggle_save_artist_album(app: &mut App, index: usize) {
  let (selected, album_id) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.albums.items.len().saturating_sub(1));
    (
      selected,
      artist
        .albums
        .items
        .get(selected)
        .and_then(|album| album.id.clone()),
    )
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::Albums;
    artist.selected_album_index = selected;
  }
  let Some(album_id) = album_id else {
    return;
  };
  if app.saved_album_ids_set.contains(album_id.id()) {
    app.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id.into_static()));
  } else {
    app.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id.into_static()));
  }
}

fn open_related_artist(app: &mut App, index: usize) {
  let (selected, artist_id, artist_name) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.related_artists.len().saturating_sub(1));
    let Some(selected_artist) = artist.related_artists.get(selected) else {
      return;
    };
    (
      selected,
      selected_artist.id.as_ref().into_static(),
      selected_artist.name.clone(),
    )
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::RelatedArtists;
    artist.selected_related_artist_index = selected;
  }
  app.get_artist(artist_id, artist_name);
}

fn recommend_related_artist(app: &mut App, index: usize) {
  let (selected, artist_name, artist_id) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.related_artists.len().saturating_sub(1));
    let Some(selected_artist) = artist.related_artists.get(selected) else {
      return;
    };
    (
      selected,
      selected_artist.name.clone(),
      selected_artist.id.id().to_string(),
    )
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::RelatedArtists;
    artist.selected_related_artist_index = selected;
  }
  app.recommendations_context = Some(RecommendationsContext::Artist);
  app.recommendations_seed = artist_name;
  app.get_recommendations_for_seed(Some(vec![artist_id]), None, None);
}

fn toggle_follow_related_artist(app: &mut App, index: usize) {
  let (selected, artist_id) = {
    let Some(artist) = &app.artist else {
      return;
    };
    let selected = index.min(artist.related_artists.len().saturating_sub(1));
    let Some(selected_artist) = artist.related_artists.get(selected) else {
      return;
    };
    (selected, selected_artist.id.as_ref().into_static())
  };
  if let Some(artist) = &mut app.artist {
    artist.artist_selected_block = ArtistBlock::RelatedArtists;
    artist.selected_related_artist_index = selected;
  }
  if app.followed_artist_ids_set.contains(artist_id.id()) {
    app.dispatch(IoEvent::UserUnfollowArtists(vec![artist_id]));
  } else {
    app.dispatch(IoEvent::UserFollowArtists(vec![artist_id]));
  }
}

fn play_podcast_episode(app: &mut App, index: usize) {
  let Some(episodes) = app.library.show_episodes.get_results(None) else {
    return;
  };
  let selected = index.min(episodes.items.len().saturating_sub(1));
  app.episode_list_index = selected;
  let episode_ids: Vec<PlayableId<'static>> = episodes
    .items
    .iter()
    .map(|episode| PlayableId::Episode(episode.id.clone().into_static()))
    .collect();
  if !episode_ids.is_empty() {
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(episode_ids),
      Some(selected),
    ));
  }
}

fn queue_podcast_episode(app: &mut App, index: usize) {
  let Some(episodes) = app.library.show_episodes.get_results(None) else {
    return;
  };
  let selected = index.min(episodes.items.len().saturating_sub(1));
  app.episode_list_index = selected;
  if let Some(episode) = episodes.items.get(selected) {
    app.dispatch(IoEvent::AddItemToQueue(PlayableId::Episode(
      episode.id.clone().into_static(),
    )));
  }
}

fn toggle_save_current_show(app: &mut App) {
  match app.episode_table_context {
    crate::core::app::EpisodeTableContext::Full => {
      if let Some(show) = app
        .selected_show_full
        .as_ref()
        .map(|show| show.show.id.clone())
      {
        if app.saved_show_ids_set.contains(show.id()) {
          app.dispatch(IoEvent::CurrentUserSavedShowDelete(show.into_static()));
        } else {
          app.dispatch(IoEvent::CurrentUserSavedShowAdd(show.into_static()));
        }
      }
    }
    crate::core::app::EpisodeTableContext::Simplified => {
      if let Some(show) = app
        .selected_show_simplified
        .as_ref()
        .map(|show| show.show.id.clone())
      {
        if app.saved_show_ids_set.contains(show.id()) {
          app.dispatch(IoEvent::CurrentUserSavedShowDelete(show.into_static()));
        } else {
          app.dispatch(IoEvent::CurrentUserSavedShowAdd(show.into_static()));
        }
      }
    }
  }
}

fn play_recently_played_track(app: &mut App, index: usize) {
  let Some(recently_played) = &app.recently_played.result else {
    return;
  };
  let selected = index.min(recently_played.items.len().saturating_sub(1));
  app.recently_played.index = selected;
  let playable_ids: Vec<PlayableId<'static>> = recently_played
    .items
    .iter()
    .filter_map(|item| track_playable_id(item.track.id.clone()))
    .collect();
  if !playable_ids.is_empty() {
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(playable_ids),
      Some(selected),
    ));
  }
}

fn queue_recently_played_track(app: &mut App, index: usize) {
  let Some(recently_played) = &app.recently_played.result else {
    return;
  };
  let selected = index.min(recently_played.items.len().saturating_sub(1));
  app.recently_played.index = selected;
  if let Some(playable_id) = recently_played
    .items
    .get(selected)
    .and_then(|item| track_playable_id(item.track.id.clone()))
  {
    app.dispatch(IoEvent::AddItemToQueue(playable_id));
  }
}

fn toggle_save_recently_played_track(app: &mut App, index: usize) {
  let Some(recently_played) = &app.recently_played.result else {
    return;
  };
  let selected = index.min(recently_played.items.len().saturating_sub(1));
  app.recently_played.index = selected;
  if let Some(playable_id) = recently_played
    .items
    .get(selected)
    .and_then(|item| track_playable_id(item.track.id.clone()))
  {
    app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
  }
}

fn add_recently_played_track_to_playlist(app: &mut App, index: usize) {
  let Some(recently_played) = &app.recently_played.result else {
    return;
  };
  let selected = index.min(recently_played.items.len().saturating_sub(1));
  app.recently_played.index = selected;
  let Some(track) = recently_played.items.get(selected) else {
    return;
  };
  app.begin_add_track_to_playlist_flow(
    track.track.id.clone().map(|id| id.into_static()),
    track.track.name.clone(),
  );
}

fn recommend_recently_played_track(app: &mut App, index: usize) {
  let Some(recently_played) = &app.recently_played.result else {
    return;
  };
  let selected = index.min(recently_played.items.len().saturating_sub(1));
  app.recently_played.index = selected;
  let Some(track) = recently_played.items.get(selected) else {
    return;
  };
  let Some(track_id) = track.track.id.as_ref() else {
    return;
  };
  app.recommendations_context = Some(RecommendationsContext::Song);
  app.recommendations_seed = track.track.name.clone();
  app.get_recommendations_for_track_id(track_id.id().to_string());
}

fn play_discover_track(app: &mut App, index: usize, top_tracks: bool) {
  let tracks = if top_tracks {
    &app.discover_top_tracks
  } else {
    &app.discover_artists_mix
  };
  if tracks.is_empty() {
    return;
  }
  let selected = index.min(tracks.len().saturating_sub(1));
  app.discover_selected_index = selected;
  let playable_ids: Vec<PlayableId<'static>> = tracks
    .iter()
    .filter_map(|track| track_playable_id(track.id.clone()))
    .collect();
  if !playable_ids.is_empty() {
    app.dispatch(IoEvent::StartPlayback(
      None,
      Some(playable_ids),
      Some(selected),
    ));
  }
}

fn queue_discover_track(app: &mut App, index: usize, top_tracks: bool) {
  let tracks = if top_tracks {
    &app.discover_top_tracks
  } else {
    &app.discover_artists_mix
  };
  if tracks.is_empty() {
    return;
  }
  let selected = index.min(tracks.len().saturating_sub(1));
  app.discover_selected_index = selected;
  if let Some(playable_id) = tracks
    .get(selected)
    .and_then(|track| track_playable_id(track.id.clone()))
  {
    app.dispatch(IoEvent::AddItemToQueue(playable_id));
  }
}

fn toggle_save_discover_track(app: &mut App, index: usize, top_tracks: bool) {
  let tracks = if top_tracks {
    &app.discover_top_tracks
  } else {
    &app.discover_artists_mix
  };
  if tracks.is_empty() {
    return;
  }
  let selected = index.min(tracks.len().saturating_sub(1));
  app.discover_selected_index = selected;
  if let Some(playable_id) = tracks
    .get(selected)
    .and_then(|track| track_playable_id(track.id.clone()))
  {
    app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
  }
}

fn add_discover_track_to_playlist(app: &mut App, index: usize, top_tracks: bool) {
  let tracks = if top_tracks {
    &app.discover_top_tracks
  } else {
    &app.discover_artists_mix
  };
  if tracks.is_empty() {
    return;
  }
  let selected = index.min(tracks.len().saturating_sub(1));
  app.discover_selected_index = selected;
  if let Some(track) = tracks.get(selected) {
    app.begin_add_track_to_playlist_flow(
      track.id.clone().map(|id| id.into_static()),
      track.name.clone(),
    );
  }
}

fn recommend_discover_track(app: &mut App, index: usize, top_tracks: bool) {
  let tracks = if top_tracks {
    &app.discover_top_tracks
  } else {
    &app.discover_artists_mix
  };
  if tracks.is_empty() {
    return;
  }
  let selected = index.min(tracks.len().saturating_sub(1));
  app.discover_selected_index = selected;
  let Some(track) = tracks.get(selected) else {
    return;
  };
  app.recommendations_context = Some(RecommendationsContext::Song);
  app.recommendations_seed = track.name.clone();
  app.get_recommendations_for_seed(
    None,
    track.id.as_ref().map(|id| vec![id.id().to_string()]),
    Some(track.clone()),
  );
}

fn sort_context_from_gui(context: GuiSortContextId) -> crate::core::sort::SortContext {
  match context {
    GuiSortContextId::PlaylistTracks => crate::core::sort::SortContext::PlaylistTracks,
    GuiSortContextId::SavedAlbums => crate::core::sort::SortContext::SavedAlbums,
    GuiSortContextId::SavedArtists => crate::core::sort::SortContext::SavedArtists,
    GuiSortContextId::RecentlyPlayed => crate::core::sort::SortContext::RecentlyPlayed,
  }
}
