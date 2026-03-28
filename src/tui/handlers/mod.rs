mod album_list;
mod album_tracks;
mod analysis;
mod announcement_prompt;
mod artist;
mod artists;
mod common_key_events;
mod create_playlist;
#[cfg(feature = "cover-art")]
mod cover_art_view;
mod dialog;
mod discover;
mod empty;
mod episode_table;
mod error_screen;
mod help_menu;
mod home;
mod input;
mod library;
mod lyrics_view;
mod mouse;
mod party;
mod playbar;
mod playlist;
mod podcasts;
mod queue_menu;
mod recently_played;
pub mod resize;
mod search_results;
mod select_device;
mod settings;
mod sort_menu;
mod track_table;

use crate::core::app::{ActiveBlock, App, ArtistBlock, RouteId, SearchResultBlock};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use rspotify::model::idtypes::PlaylistId;
use rspotify::model::{context::CurrentPlaybackContext, PlayableItem};

pub use input::handler as input_handler;
pub use mouse::handler as mouse_handler;

#[cfg(target_os = "macos")]
fn key_matches_open_settings_binding(key: Key, binding: Key) -> bool {
  key == binding
    || (binding == Key::Alt(',') && key == Key::Char('≤'))
    || (binding == Key::Ctrl(',')
      && (key == Key::Ctrl('l')
        || key == Key::Ctrl('L')
        || key == Key::Ctrl('4')
        || key == Key::Ctrl('<')))
}

#[cfg(not(target_os = "macos"))]
fn key_matches_open_settings_binding(key: Key, binding: Key) -> bool {
  key == binding
}

fn open_settings(app: &mut App) {
  app.load_settings_for_category();
  app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
}

pub fn handle_app(key: Key, app: &mut App) {
  if app.get_current_route().active_block == ActiveBlock::Settings
    && (app.settings_unsaved_prompt_visible || app.settings_edit_mode)
  {
    settings::handler(key, app);
    return;
  }

  // When Party popup is open, all keys go to the party handler first (so 'c' and 'l' aren't stolen by global bindings).
  if app.get_current_route().active_block == ActiveBlock::Party {
    handle_block_events(key, app);
    return;
  }

  // When Create Playlist form is open, all keys go directly to the form handler
  // (so typed characters aren't stolen by global bindings like 'd', space, etc.)
  if app.get_current_route().active_block == ActiveBlock::CreatePlaylistForm {
    handle_block_events(key, app);
    return;
  }

  if app.maybe_activate_open_settings_fallback(key) {
    open_settings(app);
    if app.pending_keybinding_persist.is_some() {
      app.push_navigation_stack(
        RouteId::Dialog,
        ActiveBlock::Dialog(crate::core::app::DialogContext::PersistKeybindingFallback),
      );
    }
    return;
  }

  let effective_open_settings = app.effective_open_settings_key();
  if key_matches_open_settings_binding(key, app.user_config.keys.open_settings)
    || key_matches_open_settings_binding(key, effective_open_settings)
  {
    open_settings(app);
    return;
  }

  // First handle any global event and then move to block event
  match key {
    Key::Esc => {
      if app.get_current_route().active_block == ActiveBlock::Settings {
        settings::handler(key, app);
      } else {
        handle_escape(app);
      }
    }
    _ if key == app.user_config.keys.jump_to_album => {
      handle_jump_to_album(app);
    }
    _ if key == app.user_config.keys.jump_to_artist_album => {
      handle_jump_to_artist_album(app);
    }
    _ if key == app.user_config.keys.jump_to_context => {
      handle_jump_to_context(app);
    }
    _ if key == app.user_config.keys.manage_devices => {
      app.dispatch(IoEvent::GetDevices);
    }
    _ if key == app.user_config.keys.decrease_volume => {
      app.decrease_volume();
    }
    _ if key == app.user_config.keys.increase_volume => {
      app.increase_volume();
    }
    // Press space to toggle playback
    _ if key == app.user_config.keys.toggle_playback => {
      app.toggle_playback();
    }
    _ if key == app.user_config.keys.seek_backwards => {
      app.seek_backwards();
    }
    _ if key == app.user_config.keys.seek_forwards => {
      app.seek_forwards();
    }
    _ if key == app.user_config.keys.next_track => {
      app.next_track();
    }
    _ if key == app.user_config.keys.previous_track => {
      app.previous_track();
    }
    _ if key == app.user_config.keys.force_previous_track => {
      app.force_previous_track();
    }
    _ if key == app.user_config.keys.help => {
      app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
    }
    _ if key == app.user_config.keys.show_queue => {
      app.dispatch(IoEvent::GetQueue);
      app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);
    }

    _ if key == app.user_config.keys.shuffle => {
      app.shuffle();
    }
    _ if key == app.user_config.keys.repeat => {
      app.repeat();
    }
    _ if key == app.user_config.keys.search => {
      app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));
    }
    _ if key == app.user_config.keys.copy_song_url => {
      app.copy_song_url();
    }
    _ if key == app.user_config.keys.copy_album_url => {
      app.copy_album_url();
    }
    _ if key == app.user_config.keys.audio_analysis => {
      app.get_audio_analysis();
    }
    _ if key == app.user_config.keys.lyrics_view => {
      app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);
    }
    #[cfg(feature = "cover-art")]
    _ if key == app.user_config.keys.cover_art_view => {
      app.push_navigation_stack(RouteId::CoverArtView, ActiveBlock::CoverArtView);
    }
    _ if key == app.user_config.keys.listening_party => {
      app.push_navigation_stack(RouteId::Party, ActiveBlock::Party);
    }
    // Resize sidebar: { decreases, } increases width
    Key::Char('{') => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        resize::decrease_sidebar_width(app);
      }
    }
    Key::Char('}') => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        resize::increase_sidebar_width(app);
      }
    }
    // Resize playbar or library/playlist split depending on hovered pane:
    // ( decreases height, ) increases height
    Key::Char('(') => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        match app.get_current_route().hovered_block {
          ActiveBlock::Library | ActiveBlock::MyPlaylists => resize::decrease_library_height(app),
          _ => resize::decrease_playbar_height(app),
        }
      }
    }
    Key::Char(')') => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        match app.get_current_route().hovered_block {
          ActiveBlock::Library | ActiveBlock::MyPlaylists => resize::increase_library_height(app),
          _ => resize::increase_playbar_height(app),
        }
      }
    }
    // Reset all pane sizes to defaults
    Key::Char('|') => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        resize::reset_layout(app);
      }
    }
    Key::Char('W') => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        playbar::add_currently_playing_track_to_playlist(app);
      }
    }
    _ => handle_block_events(key, app),
  }
}

fn is_input_mode(app: &App) -> bool {
  matches!(
    app.get_current_route().active_block,
    ActiveBlock::Input
      | ActiveBlock::Dialog(_)
      | ActiveBlock::AnnouncementPrompt
      | ActiveBlock::ExitPrompt
      | ActiveBlock::CreatePlaylistForm
  )
}

// Handle event for the current active block
fn handle_block_events(key: Key, app: &mut App) {
  let current_route = app.get_current_route();
  match current_route.active_block {
    ActiveBlock::Analysis => {
      analysis::handler(key, app);
    }
    ActiveBlock::ArtistBlock => {
      artist::handler(key, app);
    }
    ActiveBlock::Input => {
      input::handler(key, app);
    }
    ActiveBlock::MyPlaylists => {
      playlist::handler(key, app);
    }
    ActiveBlock::TrackTable => {
      track_table::handler(key, app);
    }
    ActiveBlock::EpisodeTable => {
      episode_table::handler(key, app);
    }
    ActiveBlock::HelpMenu => {
      help_menu::handler(key, app);
    }
    ActiveBlock::Error => {
      error_screen::handler(key, app);
    }
    ActiveBlock::SelectDevice => {
      select_device::handler(key, app);
    }
    ActiveBlock::SearchResultBlock => {
      search_results::handler(key, app);
    }
    ActiveBlock::Home => {
      home::handler(key, app);
    }
    ActiveBlock::AlbumList => {
      album_list::handler(key, app);
    }
    ActiveBlock::AlbumTracks => {
      album_tracks::handler(key, app);
    }
    ActiveBlock::Library => {
      library::handler(key, app);
    }
    ActiveBlock::Empty => {
      empty::handler(key, app);
    }
    ActiveBlock::RecentlyPlayed => {
      recently_played::handler(key, app);
    }
    ActiveBlock::Artists => {
      artists::handler(key, app);
    }
    ActiveBlock::Discover => {
      discover::handler(key, app);
    }
    ActiveBlock::Podcasts => {
      podcasts::handler(key, app);
    }
    ActiveBlock::PlayBar => {
      playbar::handler(key, app);
    }
    ActiveBlock::LyricsView => {
      lyrics_view::handler(key, app);
    }
    ActiveBlock::CoverArtView => {
      #[cfg(feature = "cover-art")]
      cover_art_view::handler(key, app);
    }
    ActiveBlock::Dialog(_) => {
      dialog::handler(key, app);
    }

    ActiveBlock::AnnouncementPrompt => {
      announcement_prompt::handler(key, app);
    }
    ActiveBlock::ExitPrompt => {}
    ActiveBlock::Settings => {
      settings::handler(key, app);
    }
    ActiveBlock::SortMenu => {
      sort_menu::handler(key, app);
    }
    ActiveBlock::Queue => {
      queue_menu::handler(key, app);
    }
    ActiveBlock::Party => {
      party::handler(key, app);
    }
    ActiveBlock::CreatePlaylistForm => {
      create_playlist::handler(key, app);
    }
  }
}

fn handle_escape(app: &mut App) {
  match app.get_current_route().active_block {
    ActiveBlock::SearchResultBlock => {
      app.search_results.selected_block = SearchResultBlock::Empty;
    }
    ActiveBlock::ArtistBlock => {
      if let Some(artist) = &mut app.artist {
        artist.artist_selected_block = ArtistBlock::Empty;
      }
    }
    ActiveBlock::Error => {
      app.pop_navigation_stack();
    }
    ActiveBlock::Dialog(dialog_context) => {
      if dialog_context == crate::core::app::DialogContext::PersistKeybindingFallback {
        app.set_status_message("Using Alt+, for this session only", 4);
      }
      app.pop_navigation_stack();
      app.clear_dialog_state();
    }
    ActiveBlock::HelpMenu => {
      app.pop_navigation_stack();
    }
    ActiveBlock::Queue => {
      app.pop_navigation_stack();
    }
    ActiveBlock::Party => {
      app.pop_navigation_stack();
    }
    ActiveBlock::LyricsView | ActiveBlock::CoverArtView => {
      app.pop_navigation_stack();
    }
    // These are global views that have no active/inactive distinction so do nothing
    ActiveBlock::SelectDevice | ActiveBlock::Analysis => {}

    // Announcement prompt must be dismissed with Enter/Esc, not global escape
    ActiveBlock::AnnouncementPrompt => {}
    ActiveBlock::ExitPrompt => {}
    // Sort menu closes on escape
    ActiveBlock::SortMenu => {
      app.sort_menu_visible = false;
      app.sort_context = None;
      app.set_current_route_state(Some(ActiveBlock::Empty), None);
    }
    ActiveBlock::CreatePlaylistForm => {
      create_playlist::handler(Key::Esc, app);
    }
    _ => {
      app.set_current_route_state(Some(ActiveBlock::Empty), None);
    }
  }
}

fn handle_jump_to_context(app: &mut App) {
  if let Some(current_playback_context) = &app.current_playback_context {
    if let Some(play_context) = current_playback_context.context.clone() {
      match play_context._type {
        rspotify::model::enums::Type::Album => handle_jump_to_album(app),
        rspotify::model::enums::Type::Artist => handle_jump_to_artist_album(app),
        rspotify::model::enums::Type::Playlist => {
          if let Ok(playlist_id) = PlaylistId::from_uri(&play_context.uri) {
            app.dispatch(IoEvent::GetPlaylistItems(playlist_id.into_static(), 0));
          }
        }
        _ => {}
      }
    }
  }
}

fn handle_jump_to_album(app: &mut App) {
  if let Some(CurrentPlaybackContext {
    item: Some(item), ..
  }) = app.current_playback_context.to_owned()
  {
    match item {
      PlayableItem::Track(track) => {
        app.dispatch(IoEvent::GetAlbumTracks(Box::new(track.album)));
      }
      PlayableItem::Episode(episode) => {
        app.dispatch(IoEvent::GetShowEpisodes(Box::new(episode.show)));
      }
      _ => {}
    };
  }
}

// NOTE: this only finds the first artist of the song and jumps to their albums
fn handle_jump_to_artist_album(app: &mut App) {
  if let Some(CurrentPlaybackContext {
    item: Some(item), ..
  }) = app.current_playback_context.to_owned()
  {
    match item {
      PlayableItem::Track(track) => {
        if let Some(artist) = track.artists.first() {
          if let Some(artist_id) = &artist.id {
            app.get_artist(artist_id.as_ref().into_static(), artist.name.clone());
          }
        }
      }
      PlayableItem::Episode(_episode) => {
        // Do nothing for episode (yet!)
      }
      _ => {}
    }
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  #[cfg(target_os = "macos")]
  use crate::core::app::TrackTableContext;

  #[test]
  fn global_shift_w_adds_current_track_from_anywhere() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(Key::Char('W'), &mut app);

    assert_eq!(
      app.status_message.as_deref(),
      Some("No track currently playing")
    );
  }

  #[test]
  fn global_shift_w_is_not_intercepted_in_input_mode() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));

    handle_app(Key::Char('W'), &mut app);

    assert_eq!(app.input, vec!['W']);
    assert!(app.status_message.is_none());
  }

  #[test]
  fn force_previous_track_dispatches_from_anywhere() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    // Default force_previous_track is Key::Char('P')
    handle_app(Key::Char('P'), &mut app);

    // force_previous_track dispatches through App which requires no io_tx in tests,
    // so just confirm the route didn't change (it shouldn't navigate anywhere)
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Empty);
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn plain_comma_fallback_opens_settings_and_prompts_to_persist() {
    let mut app = App::default();
    app.user_config.keys.open_settings = Key::Ctrl(',');
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(Key::Char(','), &mut app);

    assert_eq!(
      app.keybinding_runtime.effective_open_settings,
      Some(Key::Alt(','))
    );
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(crate::core::app::DialogContext::PersistKeybindingFallback)
    );
    assert!(app.status_message.is_some());
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn plain_comma_does_not_override_track_table_sort_menu() {
    let mut app = App::default();
    app.user_config.keys.open_settings = Key::Ctrl(',');
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);

    handle_app(Key::Char(','), &mut app);

    assert_eq!(app.get_current_route().active_block, ActiveBlock::SortMenu);
  }
}
