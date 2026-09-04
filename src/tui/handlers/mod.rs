#[cfg(feature = "ai-dj")]
mod ai_dj;
mod album_list;
mod album_tracks;
mod analysis;
mod announcement_prompt;
mod artist;
mod artists;
mod common_key_events;
mod community_pin_prompt;
#[cfg(feature = "cover-art")]
mod cover_art_view;
mod create_playlist;
mod dialog;
mod discover;
mod empty;
mod episode_table;
mod error_screen;
mod filter_input;
mod friends;
mod help_menu;
mod home;
mod input;
mod library;
mod local_browser;
mod lyrics_view;
mod miniplayer;
mod mouse;
mod party;
mod playbar;
mod playlist;
mod plugin_screen;
mod podcasts;
mod queue_menu;
mod recap_prompt;
mod recently_played;
pub mod resize;
mod search_results;
mod select_device;
mod settings;
mod sort_menu;
mod stats;
mod track_table;

use crate::core::action::{Action, CopyTarget, NavTarget};
use crate::core::app::{
  ActiveBlock, App, ArtistBlock, InputContext, PlaybackOwner, RouteId, SearchResultBlock,
  NOTHING_PLAYING_STATUS,
};
use crate::core::requirement::{Availability, Capability, Requirement};
use crate::core::source::Source;
use crate::tui::event::Key;

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

fn should_route_friends_before_globals(key: Key, app: &App) -> bool {
  if app.get_current_route().active_block != ActiveBlock::Friends {
    return false;
  }

  app.view.friend_add_dialog_visible
    || !app.view.friend_search_input.is_empty()
    || matches!(
      key,
      Key::Char('a') | Key::Char('c') | Key::Char('u') | Key::Tab
    )
}

pub fn handle_app(key: Key, app: &mut App) {
  // Plugin popup is a modal: intercept all keys before anything else.
  if app.plugin_popup.is_some() {
    match key {
      Key::Esc | Key::Char('q') => {
        app.apply(Action::ClosePopup);
      }
      k if common_key_events::up_event(k, &app.user_config.keys) => {
        app.view.plugin_popup_scroll = app.view.plugin_popup_scroll.saturating_sub(1);
      }
      k if common_key_events::down_event(k, &app.user_config.keys) => {
        let max_scroll = app
          .plugin_popup
          .as_ref()
          .map(|p| p.lines.len().saturating_sub(1) as u16)
          .unwrap_or(0);
        app.view.plugin_popup_scroll = app
          .view
          .plugin_popup_scroll
          .saturating_add(1)
          .min(max_scroll);
      }
      _ => {} // swallow all other keys
    }
    return;
  }

  // Help filtering is an inline modal input. Give it priority over global
  // bindings so queries can contain keys such as d, n, p, q, or ?.
  if app.get_current_route().active_block == ActiveBlock::HelpMenu
    && (app.view.help_filter_editing || key == app.user_config.keys.search)
  {
    help_menu::handler(key, app);
    return;
  }

  // Settings has the same inline modal inputs: an open edit, the unsaved-changes
  // prompt, and the row filter. Each needs the raw key before global bindings
  // claim it, and the search binding is what opens the filter in the first place.
  if app.get_current_route().active_block == ActiveBlock::Settings
    && (app.view.settings_unsaved_prompt_visible
      || app.view.settings_edit_mode
      || app.view.settings_filter_editing
      || key == app.user_config.keys.search)
  {
    settings::handler(key, app);
    return;
  }

  // When Party popup is open, all keys go to the party handler first (so 'c' and 'l' aren't stolen by global bindings).
  if app.get_current_route().active_block == ActiveBlock::Party {
    handle_block_events(key, app);
    return;
  }

  // The DJ prompt is a typing surface, so it needs first refusal on every key —
  // otherwise global bindings eat any character that happens to be a shortcut
  // ('d' for devices, space for play/pause, and so on).
  #[cfg(feature = "ai-dj")]
  if app.get_current_route().active_block == ActiveBlock::AiDj {
    handle_block_events(key, app);
    return;
  }

  // When Create Playlist form is open, all keys go directly to the form handler
  // (so typed characters aren't stolen by global bindings like 'd', space, etc.)
  if app.get_current_route().active_block == ActiveBlock::CreatePlaylistForm {
    handle_block_events(key, app);
    return;
  }

  // Friends has a few local keys that conflict with globals, plus inline input modes
  // that need first chance to consume typed characters.
  if should_route_friends_before_globals(key, app) {
    handle_block_events(key, app);
    return;
  }

  if app.maybe_activate_open_settings_fallback(key) {
    app.open_settings_screen();
    if app.pending_keybinding_persist_key().is_some() {
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
    app.apply(Action::Navigate(NavTarget::Settings));
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
      app.apply(Action::JumpToAlbum);
    }
    _ if key == app.user_config.keys.jump_to_artist_album => {
      app.apply(Action::JumpToArtist);
    }
    _ if key == app.user_config.keys.jump_to_context => {
      app.apply(Action::JumpToContext);
    }
    // Reachable from anywhere, without a trip through the sidebar row.
    #[cfg(feature = "ai-dj")]
    _ if key == app.user_config.keys.dj_open => {
      ai_dj::open(app);
    }
    #[cfg(feature = "ai-dj")]
    _ if key == app.user_config.keys.dj_toggle_auto_queue => {
      app.apply(Action::ToggleDjAutoQueue);
    }
    #[cfg(feature = "ai-dj")]
    _ if key == app.user_config.keys.dj_vibe_shift => {
      app.apply(Action::DjVibeShift);
    }
    #[cfg(feature = "ai-dj")]
    _ if key == app.user_config.keys.dj_toggle_fresh_only => {
      app.apply(Action::ToggleDjFreshOnly);
    }
    #[cfg(feature = "ai-dj")]
    _ if key == app.user_config.keys.dj_pick_model => {
      app.apply(Action::OpenDjSetup);
    }
    _ if key == app.user_config.keys.manage_devices => {
      app.apply(Action::Navigate(NavTarget::Devices));
    }
    _ if key == app.user_config.keys.decrease_volume => {
      app.apply(Action::VolumeDown);
    }
    _ if key == app.user_config.keys.increase_volume => {
      app.apply(Action::VolumeUp);
    }
    // Press space to toggle playback
    _ if key == app.user_config.keys.toggle_playback => {
      app.apply(Action::TogglePlayback);
    }
    _ if key == app.user_config.keys.seek_backwards => {
      app.apply(Action::SeekBackward);
    }
    _ if key == app.user_config.keys.seek_forwards => {
      app.apply(Action::SeekForward);
    }
    _ if key == app.user_config.keys.next_track => {
      app.apply(Action::NextTrack);
    }
    _ if key == app.user_config.keys.previous_track => {
      app.apply(Action::PreviousTrack);
    }
    _ if key == app.user_config.keys.force_previous_track => {
      app.apply(Action::ForcePreviousTrack);
    }
    _ if key == app.user_config.keys.help => {
      help_menu::open(app);
    }
    _ if key == app.user_config.keys.show_queue => {
      app.apply(Action::Navigate(NavTarget::Queue));
    }

    _ if key == app.user_config.keys.shuffle => {
      app.apply(Action::ToggleShuffle);
    }
    _ if key == app.user_config.keys.repeat => {
      app.apply(Action::CycleRepeat);
    }
    Key::Ctrl('f')
      if app.get_current_route().active_block == ActiveBlock::TrackTable
        && app.is_playlist_track_table_context() =>
    {
      app.view.input.clear();
      app.view.input_idx = 0;
      app.view.input_cursor_position = 0;
      app.view.input_context = InputContext::PlaylistTrackSearch;
      app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));
    }
    _ if key == app.user_config.keys.search => {
      open_global_search(app);
    }
    _ if key == app.user_config.keys.copy_song_url => {
      copy_url(app, Action::CopyUrl(CopyTarget::CurrentSong));
    }
    _ if key == app.user_config.keys.copy_album_url => {
      copy_url(app, Action::CopyUrl(CopyTarget::CurrentAlbum));
    }
    _ if key == app.user_config.keys.audio_analysis => {
      app.apply(Action::Navigate(NavTarget::Analysis));
    }
    _ if key == app.user_config.keys.lyrics_view => {
      app.apply(Action::Navigate(NavTarget::Lyrics));
    }
    _ if key == app.user_config.keys.miniplayer_view => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        app.apply(Action::Navigate(NavTarget::MiniPlayer));
      }
    }
    #[cfg(feature = "cover-art")]
    _ if key == app.user_config.keys.cover_art_view => {
      app.push_navigation_stack(RouteId::CoverArtView, ActiveBlock::CoverArtView);
    }
    _ if key == app.user_config.keys.listening_party => {
      app.apply(Action::Navigate(NavTarget::Party));
    }
    _ if key == app.user_config.keys.like_track => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else if app.active_source == Source::Radio {
        if app.get_current_route().active_block == ActiveBlock::SearchResultBlock {
          handle_block_events(key, app);
        } else if !search_results::favorite_current_radio_station(app) {
          app.set_status_message("No radio station selected or playing".to_string(), 4);
        }
      } else if app.active_source.supports_like() {
        app.apply(Action::ToggleSaveCurrentItem);
      } else {
        app.set_status_message(
          format!("Like isn't available for {}", app.active_source.label()),
          4,
        );
      }
    }
    #[cfg(feature = "scripting")]
    _ if app.user_config.plugin_command_keys.contains_key(&key) => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else if let Some(name) = app.user_config.plugin_command_keys.get(&key).cloned() {
        app.queue_plugin_command(name);
      }
    }
    _ if key == app.user_config.keys.generate_recap => {
      if is_input_mode(app) {
        handle_block_events(key, app);
      } else {
        app.apply(Action::GenerateRecap);
      }
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
        app.apply(Action::OpenAddPlayingTrackDialog);
      }
    }
    _ => handle_block_events(key, app),
  }
}

/// Open the global search box. Never rewrite the error frame in place to
/// `{Error, Input}`: push dedupes on the top frame's id, so a surviving Error
/// frame makes every later error silently fail to render. Dismiss the error
/// and open search on the screen below.
pub(super) fn focus_global_search(app: &mut App) {
  if app.get_current_route().id == RouteId::Error {
    app.clear_api_error();
  }
  app.view.input_context = InputContext::GlobalSearch;
  app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));
}

/// Focuses the search input, or reports why the key does nothing now.
pub(super) fn open_global_search(app: &mut App) -> bool {
  match app.availability(Requirement::Capability(Capability::Search)) {
    Availability::Available => {
      focus_global_search(app);
      true
    }
    Availability::NeedsSpotify => {
      app.set_status_message("Search needs a Spotify session", 4);
      false
    }
    Availability::NotForSource(_) | Availability::OnlyForSource(_) => {
      app.set_status_message(
        format!("Search isn't available for {}", app.active_source.label()),
        4,
      );
      false
    }
  }
}

/// The copy keys read the Spotify playback, so Spotify must own it and have an item.
fn copy_url(app: &mut App, copy: Action) {
  match app.playback_owner() {
    PlaybackOwner::Spotify | PlaybackOwner::NativeSpotify => {
      let has_item = app
        .current_playback_context
        .as_ref()
        .is_some_and(|context| context.item.is_some());
      if has_item {
        app.apply(copy);
      } else {
        app.set_status_message(NOTHING_PLAYING_STATUS, 4);
      }
    }
    PlaybackOwner::None => app.set_status_message(NOTHING_PLAYING_STATUS, 4),
    PlaybackOwner::Queue | PlaybackOwner::Decoded => {
      app.set_status_message("Copy URL needs a Spotify track playing", 4)
    }
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
      | ActiveBlock::RecapPrompt
      | ActiveBlock::CommunityPinPrompt
  ) || {
    #[cfg(feature = "ai-dj")]
    {
      app.get_current_route().active_block == ActiveBlock::AiDj
    }
    #[cfg(not(feature = "ai-dj"))]
    {
      false
    }
  }
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
    ActiveBlock::LocalBrowser => {
      local_browser::handler(key, app);
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
    ActiveBlock::MiniPlayer => {
      miniplayer::handler(key, app);
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
    ActiveBlock::Friends => {
      friends::handler(key, app);
    }
    ActiveBlock::Stats => {
      stats::handler(key, app);
    }
    #[cfg(feature = "ai-dj")]
    ActiveBlock::AiDj => {
      ai_dj::handler(key, app);
    }
    ActiveBlock::RecapPrompt => {
      recap_prompt::handler(key, app);
    }
    ActiveBlock::CommunityPinPrompt => {
      community_pin_prompt::handler(key, app);
    }
    ActiveBlock::PluginScreen => {
      plugin_screen::handler(key, app);
    }
  }
}

fn handle_escape(app: &mut App) {
  match app.get_current_route().active_block {
    // Delegated rather than duplicated, the way `ActiveBlock::Friends` is below.
    // The DJ's Esc is now three-state (step back through the picker, clear a
    // half-typed prompt, leave the screen), so a second copy of the rule here would
    // go stale the moment either side changed.
    #[cfg(feature = "ai-dj")]
    ActiveBlock::AiDj => ai_dj::handler(Key::Esc, app),
    ActiveBlock::SearchResultBlock => {
      app.view.search_selected_block = SearchResultBlock::Empty;
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
      if app.view.help_filter_editing || !app.view.help_filter.is_empty() {
        help_menu::clear_filter(app);
      } else {
        app.pop_navigation_stack();
      }
    }
    ActiveBlock::Queue => {
      app.pop_navigation_stack();
    }
    ActiveBlock::Party => {
      app.pop_navigation_stack();
    }
    ActiveBlock::LyricsView => {
      // Esc first leaves lyrics browsing mode; a second Esc closes the view.
      if app.view.lyrics_view.manual_index.is_some() {
        app.view.lyrics_view.manual_index = None;
      } else {
        app.pop_navigation_stack();
      }
    }
    ActiveBlock::SelectDevice
    | ActiveBlock::CoverArtView
    | ActiveBlock::MiniPlayer
    | ActiveBlock::PluginScreen => {
      app.pop_navigation_stack();
    }
    // This is a global view that has no active/inactive distinction so do nothing
    ActiveBlock::Analysis => {}

    // Announcement prompt must be dismissed with Enter/Esc, not global escape
    ActiveBlock::AnnouncementPrompt => {}
    ActiveBlock::ExitPrompt => {}
    // Sort menu closes on escape
    ActiveBlock::SortMenu => {
      app.view.sort_menu_visible = false;
      app.view.sort_context = None;
      app.set_current_route_state(Some(ActiveBlock::Empty), None);
    }
    ActiveBlock::CreatePlaylistForm => {
      create_playlist::handler(Key::Esc, app);
    }
    ActiveBlock::Friends => {
      friends::handler(Key::Esc, app);
    }
    // "[ESC] Later" on the recap popup: dismiss without opening.
    ActiveBlock::RecapPrompt => {
      app.apply(Action::DismissRecapPrompt);
    }
    // Esc keeps the pin but still marks the prompt shown so it never re-nags.
    ActiveBlock::CommunityPinPrompt => {
      community_pin_prompt::handler(Key::Esc, app);
    }
    _ => {
      app.set_current_route_state(Some(ActiveBlock::Empty), None);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::TrackTableContext;
  use crate::core::test_helpers::full_track;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use chrono::Utc;
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    device::DevicePayload,
    enums::{DeviceType, RepeatState},
    idtypes::PlaylistId,
    CurrentlyPlayingType, Device, PlayableItem,
  };
  use rspotify::prelude::Id;
  use std::{
    sync::mpsc::{channel, TryRecvError},
    time::SystemTime,
  };

  fn friends_app() -> App {
    let mut app = App::default();
    app.push_navigation_stack(RouteId::Friends, ActiveBlock::Friends);
    app
  }

  #[test]
  fn search_key_on_the_error_page_dismisses_the_error_first() {
    let mut app = App::default_connected();
    app.handle_error(anyhow::anyhow!("boom"));
    assert_eq!(app.get_current_route().id, RouteId::Error);

    handle_app(app.user_config.keys.search, &mut app);

    let route = app.get_current_route();
    assert_ne!(route.id, RouteId::Error, "the error frame must not survive");
    assert_eq!(route.active_block, ActiveBlock::Input);
    assert!(
      app.api_error().is_empty(),
      "the dismissal drops the message with the frame"
    );

    // A later error must still render: the dismissal above is what keeps the
    // navigation stack free of a deduping `{Error, ...}` frame.
    handle_app(Key::Esc, &mut app);
    app.handle_error(anyhow::anyhow!("again"));
    assert_eq!(app.get_current_route().id, RouteId::Error);
  }

  #[test]
  fn global_shift_w_adds_current_track_from_anywhere() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(Key::Char('W'), &mut app);

    assert_eq!(app.status_message(), Some("No track currently playing"));
  }

  #[test]
  fn global_shift_w_is_not_intercepted_in_input_mode() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));

    handle_app(Key::Char('W'), &mut app);

    assert_eq!(app.view.input, vec!['W']);
    assert!(app.status_message().is_none());
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

  #[test]
  fn escape_exits_device_selector() {
    let mut app = App::default();
    app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);

    handle_app(Key::Esc, &mut app);

    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::SelectDevice
    );
  }

  #[test]
  fn enter_on_device_selector_dispatches_transfer_and_exits() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.devices = Some(DevicePayload {
      devices: vec![Device {
        id: Some("device-1".to_string()),
        is_active: false,
        is_private_session: false,
        is_restricted: false,
        name: "Desk Speaker".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(42),
      }],
    });
    app.view.selected_device_index = Some(0);
    app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);

    handle_app(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::TransferPlaybackToDevice(device_id, persist_device_id) => {
        assert_eq!(device_id, "device-1");
        assert!(persist_device_id);
      }
      _ => panic!("unexpected event"),
    }
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::SelectDevice
    );
    assert_eq!(
      app.status_message(),
      Some("Switching playback to Desk Speaker")
    );
  }

  #[test]
  fn global_shift_f_likes_current_track_from_anywhere() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let track = full_track("0000000000000000000001", "Track 1");
    let expected_track_id = track.id.clone().unwrap();

    app.current_playback_context = Some(CurrentPlaybackContext {
      device: Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk Speaker".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(42),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: false,
      item: Some(PlayableItem::Track(track)),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    });
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    // Default like_track is Key::Char('F')
    handle_app(Key::Char('F'), &mut app);

    match rx.recv().unwrap() {
      IoEvent::ToggleSaveTrack(track_id) => {
        assert_eq!(track_id, expected_track_id.uri());
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn global_shift_f_is_not_intercepted_in_input_mode() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));

    handle_app(Key::Char('F'), &mut app);

    // In input mode, 'F' should be added to the input buffer
    assert_eq!(app.view.input, vec!['F']);
  }

  #[test]
  fn friends_a_opens_add_dialog_before_global_album_jump() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let track = full_track("0000000000000000000001", "Track 1");
    app.current_playback_context = Some(CurrentPlaybackContext {
      device: Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk Speaker".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(42),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: false,
      item: Some(PlayableItem::Track(track)),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    });
    app.push_navigation_stack(RouteId::Friends, ActiveBlock::Friends);

    handle_app(Key::Char('a'), &mut app);

    assert!(app.view.friend_add_dialog_visible);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Friends);
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
  }

  #[test]
  fn friends_c_prefers_friend_code_copy_over_global_song_copy() {
    let mut app = friends_app();
    app.friend_code = Some("jay-1234".to_string());
    app.clipboard = None;

    handle_app(Key::Char('c'), &mut app);

    assert_eq!(app.status_message(), Some("Clipboard not available"));
    assert!(!app.view.friend_add_dialog_visible);
  }

  #[test]
  fn friends_search_buffer_keeps_globally_bound_characters_local() {
    let mut app = friends_app();
    app.view.friend_search_input = vec!['j'];

    handle_app(Key::Char('a'), &mut app);
    handle_app(Key::Char('c'), &mut app);

    assert_eq!(app.view.friend_search_input, vec!['j', 'a', 'c']);
    assert!(!app.view.friend_add_dialog_visible);
    assert!(app.status_message().is_none());
  }

  #[test]
  fn friends_without_local_state_still_allows_non_conflicting_globals() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::Friends, ActiveBlock::Friends);

    handle_app(app.user_config.keys.next_track, &mut app);

    match rx.recv().unwrap() {
      IoEvent::NextTrack => {}
      _ => panic!("unexpected event"),
    }
    assert!(app.view.friend_search_input.is_empty());
    assert!(!app.view.friend_add_dialog_visible);
  }

  #[test]
  fn friends_add_dialog_keeps_priority_for_conflicting_keys() {
    let mut app = friends_app();
    app.open_friend_add_dialog();

    handle_app(Key::Char('c'), &mut app);

    assert!(app.view.friend_add_dialog_visible);
    assert_eq!(app.view.friend_add_input, vec!['c']);
    assert!(app.status_message().is_none());
  }

  #[test]
  fn ctrl_f_in_playlist_track_table_opens_playlist_search_input() {
    let mut app = App::default();
    let playlist_id = PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static();
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);

    handle_app(Key::Ctrl('f'), &mut app);

    assert_eq!(app.view.input_context, InputContext::PlaylistTrackSearch);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Input);
  }

  #[test]
  fn search_key_in_playlist_track_table_opens_global_search_input() {
    let mut app = App::default_connected();
    let playlist_id = PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static();
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);

    handle_app(app.user_config.keys.search, &mut app);

    assert_eq!(app.view.input_context, InputContext::GlobalSearch);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Input);
  }

  #[test]
  fn search_key_outside_playlist_track_table_opens_global_search_input() {
    let mut app = App::default_connected();
    app.track_table.context = Some(TrackTableContext::SavedTracks);
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);

    handle_app(app.user_config.keys.search, &mut app);

    assert_eq!(app.view.input_context, InputContext::GlobalSearch);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::Input);
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
    assert!(app.status_message().is_some());
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

  // --- U5: source-gate tests ---

  #[test]
  fn like_track_shows_hint_when_local_source() {
    let mut app = App::default();
    app.active_source = Source::Local;
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    // Default like_track key is 'F' (Shift+F)
    handle_app(app.user_config.keys.like_track, &mut app);

    assert_eq!(
      app.status_message(),
      Some("Like isn't available for Local Files")
    );
  }

  #[test]
  fn like_track_does_not_show_hint_when_spotify_source() {
    // No io_tx here so dispatch is a no-op; we just verify no status is set.
    let mut app = App::default();
    app.active_source = Source::Spotify;
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(app.user_config.keys.like_track, &mut app);

    // No playback context, so toggle_like returns early with a different message
    // (or nothing). The important thing is it's NOT the Local-gate message.
    assert_ne!(
      app.status_message(),
      Some("Like isn't available for Local Files")
    );
  }

  #[test]
  fn global_like_key_favorites_radio_search_station() {
    use crate::core::pagination::Paged;
    use crate::core::plugin_api::TrackInfo;
    use crate::core::user_config::UserConfigPaths;

    let dir = tempfile::tempdir().unwrap();
    let mut app = App::default();
    app.active_source = Source::Radio;
    app.user_config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    app.state_path = Some(dir.path().join("state.yml"));
    app.search_results.tracks = Some(Paged {
      items: vec![TrackInfo {
        uri: Some("radio:https://example.com/stream".to_string()),
        name: "Example FM".to_string(),
        artists: vec![],
        album: String::new(),
        duration_ms: 0,
        id: None,
        album_id: None,
        artist_refs: vec![],
        is_playable: true,
        is_local: false,
        track_number: 0,
        explicit: false,
        image_url: None,
      }],
      total: 1,
      ..Default::default()
    });
    app.view.search_selected_tracks_index = Some(0);
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    let favorite_key = app.user_config.keys.like_track;
    handle_app(favorite_key, &mut app);

    assert_eq!(app.runtime_state.radio_stations.len(), 1);
    assert_eq!(
      app.runtime_state.radio_stations[0].url,
      "https://example.com/stream"
    );
    assert_eq!(
      app.status_message(),
      Some("Favorited radio station: Example FM")
    );
  }

  fn spotify_track_context() -> CurrentPlaybackContext {
    CurrentPlaybackContext {
      device: Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk Speaker".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(42),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: true,
      item: Some(PlayableItem::Track(full_track(
        "0000000000000000000001",
        "Track 1",
      ))),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  #[test]
  fn copy_url_keys_follow_the_playback_owner_not_the_browse_scope() {
    let mut app = App::default().under_source(Source::Local);
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(app.user_config.keys.copy_song_url, &mut app);
    assert_eq!(app.status_message(), Some("Nothing is playing"));

    app.set_status_message("cleared", 1);
    handle_app(app.user_config.keys.copy_album_url, &mut app);
    assert_eq!(app.status_message(), Some("Nothing is playing"));

    // An idle session is not a playing track.
    let mut app = App::default_connected().under_source(Source::Local);
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));
    handle_app(app.user_config.keys.copy_album_url, &mut app);
    assert_eq!(app.status_message(), Some("Nothing is playing"));

    // A Spotify track playing under the Local scope still copies; with no
    // clipboard in a test App the copy exits without a message.
    let mut app = App::default_connected()
      .under_source(Source::Local)
      .with_playback(spotify_track_context());
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));
    handle_app(app.user_config.keys.copy_album_url, &mut app);
    assert_eq!(app.status_message(), None);
  }

  #[test]
  fn the_search_key_without_a_session_says_so_instead_of_opening_the_input() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(app.user_config.keys.search, &mut app);

    assert_ne!(app.get_current_route().active_block, ActiveBlock::Input);
    assert_eq!(app.status_message(), Some("Search needs a Spotify session"));
  }

  /// The DJ's own tests all call `ai_dj::handler` directly, which is the branch
  /// taken once the DJ screen already has focus (`handle_app` returns early for
  /// `ActiveBlock::AiDj`). That leaves the `dj_pick_model` arm of the global match
  /// untested, and it is the arm that makes the key work from Home, Search, Library
  /// or anywhere else — which is the only reason a global binding exists rather
  /// than a DJ-screen-local one.
  #[cfg(feature = "ai-dj")]
  #[test]
  fn the_reopen_binding_opens_the_dj_with_the_picker_from_another_screen() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(app.user_config.keys.dj_pick_model, &mut app);

    assert_eq!(app.get_current_route().id, RouteId::AiDj);
    assert!(
      app.dj.setup.is_some(),
      "and it arrives with the picker already up, not on an empty prompt"
    );
  }

  #[test]
  fn copy_song_url_proceeds_when_spotify_source() {
    // No clipboard in default App, so copy_song_url exits early; but no Local gate message.
    let mut app = App::default();
    app.active_source = Source::Spotify;
    app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::Library));

    handle_app(app.user_config.keys.copy_song_url, &mut app);

    assert_ne!(
      app.status_message(),
      Some("Copy URL isn't available for Local Files")
    );
  }
}
