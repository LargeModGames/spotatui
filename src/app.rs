use super::user_config::UserConfig;
use crate::cli::UpdateInfo;
use crate::network::IoEvent;
use crate::sort::{SortContext, SortState};
use anyhow::anyhow;
use ratatui::layout::Size;
use rspotify::{
  model::enums::Country,
  model::{
    album::{FullAlbum, SavedAlbum, SimplifiedAlbum},
    artist::FullArtist,
    context::CurrentPlaybackContext,
    device::DevicePayload,
    idtypes::{ArtistId, ShowId, TrackId},
    page::{CursorBasedPage, Page},
    playing::PlayHistory,
    playlist::{PlaylistItem, SimplifiedPlaylist},
    show::{FullShow, Show, SimplifiedEpisode, SimplifiedShow},
    track::{FullTrack, SavedTrack, SimplifiedTrack},
    user::PrivateUser,
    PlayableItem,
  },
  prelude::*, // Adds Id trait for .id() method
};
use std::sync::mpsc::Sender;
use std::{
  cmp::{max, min},
  collections::HashSet,
  time::{Instant, SystemTime},
};

use arboard::Clipboard;

pub const LIBRARY_OPTIONS: [&str; 6] = [
  "Discover",
  "Recently Played",
  "Liked Songs",
  "Albums",
  "Artists",
  "Podcasts",
];

const DEFAULT_ROUTE: Route = Route {
  id: RouteId::Home,
  active_block: ActiveBlock::Empty,
  hovered_block: ActiveBlock::Library,
};

#[derive(Clone)]
pub struct ScrollableResultPages<T> {
  pub index: usize,
  pub pages: Vec<T>,
}

impl<T> ScrollableResultPages<T> {
  pub fn new() -> ScrollableResultPages<T> {
    ScrollableResultPages {
      index: 0,
      pages: vec![],
    }
  }

  pub fn get_results(&self, at_index: Option<usize>) -> Option<&T> {
    self.pages.get(at_index.unwrap_or(self.index))
  }

  pub fn get_mut_results(&mut self, at_index: Option<usize>) -> Option<&mut T> {
    self.pages.get_mut(at_index.unwrap_or(self.index))
  }

  pub fn add_pages(&mut self, new_pages: T) {
    self.pages.push(new_pages);
    // Whenever a new page is added, set the active index to the end of the vector
    self.index = self.pages.len() - 1;
  }
}

#[derive(Default)]
pub struct SpotifyResultAndSelectedIndex<T> {
  pub index: usize,
  pub result: T,
}

#[derive(Clone)]
pub struct Library {
  pub selected_index: usize,
  pub saved_tracks: ScrollableResultPages<Page<SavedTrack>>,
  pub saved_albums: ScrollableResultPages<Page<SavedAlbum>>,
  pub saved_shows: ScrollableResultPages<Page<Show>>,
  pub saved_artists: ScrollableResultPages<CursorBasedPage<FullArtist>>,
  pub show_episodes: ScrollableResultPages<Page<SimplifiedEpisode>>,
}

#[derive(PartialEq, Debug)]
pub enum SearchResultBlock {
  AlbumSearch,
  SongSearch,
  ArtistSearch,
  PlaylistSearch,
  ShowSearch,
  Empty,
}

#[derive(PartialEq, Debug, Clone)]
pub enum ArtistBlock {
  TopTracks,
  Albums,
  RelatedArtists,
  Empty,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DialogContext {
  PlaylistWindow,
  PlaylistSearch,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActiveBlock {
  Analysis,
  PlayBar,
  AlbumTracks,
  AlbumList,
  ArtistBlock,
  Empty,
  Error,
  HelpMenu,
  Home,
  Input,
  Library,
  MyPlaylists,
  Podcasts,
  EpisodeTable,
  RecentlyPlayed,
  SearchResultBlock,
  SelectDevice,
  TrackTable,
  Discover,
  Artists,
  BasicView,
  Dialog(DialogContext),
  UpdatePrompt,
  Settings,
  SortMenu,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RouteId {
  Analysis,
  AlbumTracks,
  AlbumList,
  Artist,
  BasicView,
  Error,
  Home,
  RecentlyPlayed,
  Search,
  SelectedDevice,
  TrackTable,
  Discover,
  Artists,
  Podcasts,
  PodcastEpisodes,
  Recommendations,
  Dialog,
  UpdatePrompt,
  Settings,
}

#[derive(Debug)]
pub struct Route {
  pub id: RouteId,
  pub active_block: ActiveBlock,
  pub hovered_block: ActiveBlock,
}

// Is it possible to compose enums?
#[derive(PartialEq, Debug)]
pub enum TrackTableContext {
  MyPlaylists,
  AlbumSearch,
  PlaylistSearch,
  SavedTracks,
  RecommendedTracks,
  DiscoverPlaylist,
}

// Is it possible to compose enums?
#[derive(Clone, PartialEq, Debug, Copy)]
pub enum AlbumTableContext {
  Simplified,
  Full,
}

#[derive(Clone, PartialEq, Debug, Copy)]
pub enum EpisodeTableContext {
  Simplified,
  Full,
}

/// Time range for Top Tracks/Artists in Discover feature
#[derive(Clone, PartialEq, Debug, Copy, Default)]
pub enum DiscoverTimeRange {
  /// Last 4 weeks
  Short,
  /// Last 6 months (default)
  #[default]
  Medium,
  /// All time
  Long,
}

impl DiscoverTimeRange {
  pub fn label(&self) -> &'static str {
    match self {
      DiscoverTimeRange::Short => "4 weeks",
      DiscoverTimeRange::Medium => "6 months",
      DiscoverTimeRange::Long => "All time",
    }
  }

  pub fn next(&self) -> Self {
    match self {
      DiscoverTimeRange::Short => DiscoverTimeRange::Medium,
      DiscoverTimeRange::Medium => DiscoverTimeRange::Long,
      DiscoverTimeRange::Long => DiscoverTimeRange::Short,
    }
  }

  pub fn prev(&self) -> Self {
    match self {
      DiscoverTimeRange::Short => DiscoverTimeRange::Long,
      DiscoverTimeRange::Medium => DiscoverTimeRange::Short,
      DiscoverTimeRange::Long => DiscoverTimeRange::Medium,
    }
  }
}

#[derive(Clone, PartialEq, Debug)]
pub enum RecommendationsContext {
  Artist,
  Song,
}

pub struct SearchResult {
  pub albums: Option<Page<SimplifiedAlbum>>,
  pub artists: Option<Page<FullArtist>>,
  pub playlists: Option<Page<SimplifiedPlaylist>>,
  pub tracks: Option<Page<FullTrack>>,
  pub shows: Option<Page<SimplifiedShow>>,
  pub selected_album_index: Option<usize>,
  pub selected_artists_index: Option<usize>,
  pub selected_playlists_index: Option<usize>,
  pub selected_tracks_index: Option<usize>,
  pub selected_shows_index: Option<usize>,
  pub hovered_block: SearchResultBlock,
  pub selected_block: SearchResultBlock,
}

#[derive(Default)]
pub struct TrackTable {
  pub tracks: Vec<FullTrack>,
  pub selected_index: usize,
  pub context: Option<TrackTableContext>,
}

#[derive(Clone)]
pub struct SelectedShow {
  pub show: SimplifiedShow,
}

#[derive(Clone)]
pub struct SelectedFullShow {
  pub show: FullShow,
}

#[derive(Clone)]
pub struct SelectedAlbum {
  pub album: SimplifiedAlbum,
  pub tracks: Page<SimplifiedTrack>,
  pub selected_index: usize,
}

#[derive(Clone)]
pub struct SelectedFullAlbum {
  pub album: FullAlbum,
  pub selected_index: usize,
}

#[derive(Clone)]
pub struct Artist {
  pub artist_name: String,
  pub albums: Page<SimplifiedAlbum>,
  pub related_artists: Vec<FullArtist>,
  pub top_tracks: Vec<FullTrack>,
  pub selected_album_index: usize,
  pub selected_related_artist_index: usize,
  pub selected_top_track_index: usize,
  pub artist_hovered_block: ArtistBlock,
  pub artist_selected_block: ArtistBlock,
}

/// Spectrum data for local audio visualization
#[derive(Clone, Default)]
pub struct SpectrumData {
  pub bands: [f32; 12],
  pub peak: f32,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub enum LyricsStatus {
  #[default]
  NotStarted,
  Loading,
  Found,
  NotFound,
}

/// Immediate track info from native player for instant UI updates
/// Used to display track info immediately when skipping, before API responds
#[derive(Clone, Debug, Default)]
pub struct NativeTrackInfo {
  pub name: String,
  pub artists: Vec<String>,
  #[allow(dead_code)]
  pub album: String, // Reserved for future use (e.g., displaying album in playbar)
  pub duration_ms: u32,
}

/// Settings screen category tabs
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SettingsCategory {
  #[default]
  Behavior,
  Keybindings,
  Theme,
}

impl SettingsCategory {
  pub fn all() -> &'static [SettingsCategory] {
    &[
      SettingsCategory::Behavior,
      SettingsCategory::Keybindings,
      SettingsCategory::Theme,
    ]
  }

  pub fn name(&self) -> &'static str {
    match self {
      SettingsCategory::Behavior => "Behavior",
      SettingsCategory::Keybindings => "Keybindings",
      SettingsCategory::Theme => "Theme",
    }
  }

  pub fn index(&self) -> usize {
    match self {
      SettingsCategory::Behavior => 0,
      SettingsCategory::Keybindings => 1,
      SettingsCategory::Theme => 2,
    }
  }

  pub fn from_index(index: usize) -> Self {
    match index {
      0 => SettingsCategory::Behavior,
      1 => SettingsCategory::Keybindings,
      2 => SettingsCategory::Theme,
      _ => SettingsCategory::Behavior,
    }
  }
}

/// Represents a setting's value type
#[derive(Clone, PartialEq, Debug)]
pub enum SettingValue {
  Bool(bool),
  Number(i64),
  String(String),
  Color(String),  // Stored as "R,G,B" or color name
  Key(String),    // Key representation like "ctrl-s" or "a"
  Preset(String), // Theme preset name - cycles through available presets
}

impl SettingValue {
  #[allow(dead_code)]
  pub fn display(&self) -> String {
    match self {
      SettingValue::Bool(v) => if *v { "On" } else { "Off" }.to_string(),
      SettingValue::Number(v) => v.to_string(),
      SettingValue::String(v) => v.clone(),
      SettingValue::Color(v) => v.clone(),
      SettingValue::Key(v) => v.clone(),
      SettingValue::Preset(v) => v.clone(),
    }
  }
}

/// Represents a single configurable setting
#[derive(Clone, Debug)]
pub struct SettingItem {
  pub id: String,   // e.g., "behavior.seek_milliseconds"
  pub name: String, // e.g., "Seek Duration"
  #[allow(dead_code)]
  pub description: String, // e.g., "Milliseconds to skip when seeking" (for future tooltip)
  pub value: SettingValue,
}

pub struct App {
  pub instant_since_last_current_playback_poll: Instant,
  navigation_stack: Vec<Route>,
  pub spectrum_data: Option<SpectrumData>,
  pub audio_capture_active: bool,
  pub home_scroll: u16,
  pub user_config: UserConfig,
  pub artists: Vec<FullArtist>,
  pub artist: Option<Artist>,
  pub album_table_context: AlbumTableContext,
  pub saved_album_tracks_index: usize,
  pub api_error: String,
  pub current_playback_context: Option<CurrentPlaybackContext>,
  pub last_track_id: Option<String>,
  pub devices: Option<DevicePayload>,
  // Inputs:
  // input is the string for input;
  // input_idx is the index of the cursor in terms of character;
  // input_cursor_position is the sum of the width of characters preceding the cursor.
  // Reason for this complication is due to non-ASCII characters, they may
  // take more than 1 bytes to store and more than 1 character width to display.
  pub input: Vec<char>,
  pub input_idx: usize,
  pub input_cursor_position: u16,
  pub liked_song_ids_set: HashSet<String>,
  pub followed_artist_ids_set: HashSet<String>,
  pub saved_album_ids_set: HashSet<String>,
  pub saved_show_ids_set: HashSet<String>,
  pub large_search_limit: u32,
  pub library: Library,
  pub playlist_offset: u32,
  pub playlist_tracks: Option<Page<PlaylistItem>>,
  pub playlists: Option<Page<SimplifiedPlaylist>>,
  pub recently_played: SpotifyResultAndSelectedIndex<Option<CursorBasedPage<PlayHistory>>>,
  pub recommended_tracks: Vec<FullTrack>,
  pub recommendations_seed: String,
  pub recommendations_context: Option<RecommendationsContext>,
  pub search_results: SearchResult,
  pub selected_album_simplified: Option<SelectedAlbum>,
  pub selected_album_full: Option<SelectedFullAlbum>,
  pub selected_device_index: Option<usize>,
  pub selected_playlist_index: Option<usize>,
  pub active_playlist_index: Option<usize>,
  pub size: Size,
  #[allow(dead_code)]
  pub small_search_limit: u32,
  pub song_progress_ms: u128,
  pub seek_ms: Option<u128>,
  pub track_table: TrackTable,
  pub episode_table_context: EpisodeTableContext,
  pub selected_show_simplified: Option<SelectedShow>,
  pub selected_show_full: Option<SelectedFullShow>,
  pub user: Option<PrivateUser>,
  pub album_list_index: usize,
  pub artists_list_index: usize,
  pub clipboard: Option<Clipboard>,
  pub shows_list_index: usize,
  pub episode_list_index: usize,
  pub help_docs_size: u32,
  pub help_menu_page: u32,
  pub help_menu_max_lines: u32,
  pub help_menu_offset: u32,
  pub is_loading: bool,
  io_tx: Option<Sender<IoEvent>>,
  pub is_fetching_current_playback: bool,
  pub spotify_token_expiry: SystemTime,
  pub dialog: Option<String>,
  pub confirm: bool,
  pub update_available: Option<UpdateInfo>,
  pub update_prompt_acknowledged: bool,
  pub lyrics: Option<Vec<(u128, String)>>,
  pub lyrics_status: LyricsStatus,
  pub global_song_count: Option<u64>,
  pub global_song_count_failed: bool,
  // Settings screen state
  pub settings_category: SettingsCategory,
  pub settings_items: Vec<SettingItem>,
  pub settings_selected_index: usize,
  pub settings_edit_mode: bool,
  pub settings_edit_buffer: String,
  /// Immediate track info from native player for instant UI updates
  pub native_track_info: Option<NativeTrackInfo>,
  /// Whether native streaming is active (disables API-based progress calculation)
  pub is_streaming_active: bool,
  /// Device id for the native streaming device when known
  pub native_device_id: Option<String>,
  /// Native playback state - updated by player events, used when streaming is active
  /// This is more reliable than current_playback_context.is_playing during native streaming
  pub native_is_playing: Option<bool>,
  /// Selected index in the Discover view
  pub discover_selected_index: usize,
  /// Top tracks from the user for Discover feature
  pub discover_top_tracks: Vec<FullTrack>,
  /// Top Artists Mix tracks for Discover feature
  pub discover_artists_mix: Vec<FullTrack>,
  /// Time range for Top Tracks
  pub discover_time_range: DiscoverTimeRange,
  /// Whether we're currently loading discover data
  pub discover_loading: bool,
  // Sort menu state
  /// Whether the sort menu popup is visible
  pub sort_menu_visible: bool,
  /// Currently selected sort option in the menu
  pub sort_menu_selected: usize,
  /// Current sort context (what we're sorting)
  pub sort_context: Option<SortContext>,
  /// Current sort state per context
  pub playlist_sort: SortState,
  pub album_sort: SortState,
  pub artist_sort: SortState,
  /// Animation frame counter for the "Liked" heart flash effect (0-10)
  pub liked_song_animation_frame: Option<u8>,
}

impl Default for App {
  fn default() -> Self {
    App {
      spectrum_data: None,
      audio_capture_active: false,
      album_table_context: AlbumTableContext::Full,
      album_list_index: 0,
      discover_selected_index: 0,
      discover_top_tracks: vec![],
      discover_artists_mix: vec![],
      discover_time_range: DiscoverTimeRange::default(),
      discover_loading: false,
      artists_list_index: 0,
      shows_list_index: 0,
      episode_list_index: 0,
      artists: vec![],
      artist: None,
      user_config: UserConfig::new(),
      saved_album_tracks_index: 0,
      recently_played: Default::default(),
      size: Size::default(),
      selected_album_simplified: None,
      selected_album_full: None,
      home_scroll: 0,
      library: Library {
        saved_tracks: ScrollableResultPages::new(),
        saved_albums: ScrollableResultPages::new(),
        saved_shows: ScrollableResultPages::new(),
        saved_artists: ScrollableResultPages::new(),
        show_episodes: ScrollableResultPages::new(),
        selected_index: 0,
      },
      liked_song_ids_set: HashSet::new(),
      followed_artist_ids_set: HashSet::new(),
      saved_album_ids_set: HashSet::new(),
      saved_show_ids_set: HashSet::new(),
      navigation_stack: vec![DEFAULT_ROUTE],
      large_search_limit: 20,
      small_search_limit: 4,
      api_error: String::new(),
      current_playback_context: None,
      last_track_id: None,
      devices: None,
      input: vec![],
      input_idx: 0,
      input_cursor_position: 0,
      playlist_offset: 0,
      playlist_tracks: None,
      playlists: None,
      recommended_tracks: vec![],
      recommendations_context: None,
      recommendations_seed: "".to_string(),
      search_results: SearchResult {
        hovered_block: SearchResultBlock::SongSearch,
        selected_block: SearchResultBlock::Empty,
        albums: None,
        artists: None,
        playlists: None,
        shows: None,
        selected_album_index: None,
        selected_artists_index: None,
        selected_playlists_index: None,
        selected_tracks_index: None,
        selected_shows_index: None,
        tracks: None,
      },
      song_progress_ms: 0,
      seek_ms: None,
      selected_device_index: None,
      selected_playlist_index: None,
      active_playlist_index: None,
      track_table: Default::default(),
      episode_table_context: EpisodeTableContext::Full,
      selected_show_simplified: None,
      selected_show_full: None,
      user: None,
      instant_since_last_current_playback_poll: Instant::now(),
      clipboard: Clipboard::new().ok(),
      help_docs_size: 0,
      help_menu_page: 0,
      help_menu_max_lines: 0,
      help_menu_offset: 0,
      is_loading: false,
      io_tx: None,
      is_fetching_current_playback: false,
      spotify_token_expiry: SystemTime::now(),
      dialog: None,
      confirm: false,
      update_available: None,
      update_prompt_acknowledged: false,
      lyrics: None,
      lyrics_status: LyricsStatus::default(),
      global_song_count: None,
      global_song_count_failed: false,
      // Settings defaults
      settings_category: SettingsCategory::default(),
      settings_items: Vec::new(),
      settings_selected_index: 0,
      settings_edit_mode: false,
      settings_edit_buffer: String::new(),
      native_track_info: None,
      is_streaming_active: false,
      native_device_id: None,
      native_is_playing: None,
      // Sort menu defaults
      sort_menu_visible: false,
      sort_menu_selected: 0,
      sort_context: None,
      playlist_sort: SortState::new(),
      album_sort: SortState::new(),
      artist_sort: SortState::new(),
      liked_song_animation_frame: None,
    }
  }
}

impl App {
  pub fn new(
    io_tx: Sender<IoEvent>,
    user_config: UserConfig,
    spotify_token_expiry: SystemTime,
  ) -> App {
    App {
      io_tx: Some(io_tx),
      user_config,
      spotify_token_expiry,
      ..App::default()
    }
  }

  // Send a network event to the network thread
  pub fn dispatch(&mut self, action: IoEvent) {
    // `is_loading` will be set to false again after the async action has finished in network.rs
    self.is_loading = true;
    if let Some(io_tx) = &self.io_tx {
      if let Err(e) = io_tx.send(action) {
        self.is_loading = false;
        println!("Error from dispatch {}", e);
        // TODO: handle error
      };
    }
  }

  // Close the IO channel to allow the network thread to exit gracefully
  pub fn close_io_channel(&mut self) {
    self.io_tx = None;
  }

  fn apply_seek(&mut self, seek_ms: u32) {
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
      };

      let event = if seek_ms < duration_ms {
        IoEvent::Seek(seek_ms)
      } else {
        IoEvent::NextTrack
      };

      self.dispatch(event);
    }
  }

  fn poll_current_playback(&mut self) {
    // Poll interval depends on playback mode:
    // - Native streaming: 5 seconds (real-time events provide updates between polls)
    // - External players (spotifyd, etc.): 1 second (no events, need faster polling for smooth playbar)
    let poll_interval_ms = if self.is_streaming_active {
      5_000
    } else {
      1_000
    };

    let elapsed = self
      .instant_since_last_current_playback_poll
      .elapsed()
      .as_millis();

    if !self.is_fetching_current_playback && elapsed >= poll_interval_ms {
      self.is_fetching_current_playback = true;
      // Trigger the seek if the user has set a new position
      match self.seek_ms {
        Some(seek_ms) => self.apply_seek(seek_ms as u32),
        None => self.dispatch(IoEvent::GetCurrentPlayback),
      }
    }
  }

  pub fn update_on_tick(&mut self) {
    if let Some(frame) = self.liked_song_animation_frame {
      if frame > 0 {
        self.liked_song_animation_frame = Some(frame - 1);
      } else {
        self.liked_song_animation_frame = None;
      }
    }

    self.poll_current_playback();

    if let Some(CurrentPlaybackContext {
      item: Some(item),
      progress,
      is_playing,
      ..
    }) = &self.current_playback_context
    {
      // When native streaming is active, skip API-based progress calculation
      // The native player's PositionChanged events update song_progress_ms directly
      if self.is_streaming_active {
        let ms_since_poll = self
          .instant_since_last_current_playback_poll
          .elapsed()
          .as_millis();
        if ms_since_poll < 2000 {
          return; // Recent native update - don't overwrite
        }
        // No recent native update - fall through to API-based calculation as fallback
      }

      let ms_since_poll = self
        .instant_since_last_current_playback_poll
        .elapsed()
        .as_millis();

      // Resync from fresh API data (within 300ms of poll) to correct drift
      if ms_since_poll < 300 {
        self.song_progress_ms = progress
          .as_ref()
          .map(|p| p.num_milliseconds() as u128)
          .unwrap_or(0);
      } else if *is_playing {
        // Smooth incremental updates between API polls
        let tick_rate_ms = self.user_config.behavior.tick_rate_milliseconds as u128;
        let duration_ms = match item {
          PlayableItem::Track(track) => track.duration.num_milliseconds() as u128,
          PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u128,
        };

        self.song_progress_ms = (self.song_progress_ms + tick_rate_ms).min(duration_ms);
      }
      // When paused, keep song_progress_ms unchanged
    }
  }

  pub fn seek_forwards(&mut self) {
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
      };

      let old_progress = match self.seek_ms {
        Some(seek_ms) => seek_ms,
        None => self.song_progress_ms,
      };

      let new_progress = min(
        old_progress as u32 + self.user_config.behavior.seek_milliseconds,
        duration_ms,
      );

      self.seek_ms = Some(new_progress as u128);
      // Dispatch the seek immediately instead of waiting for the poll interval
      self.apply_seek(new_progress);
    }
  }

  pub fn seek_backwards(&mut self) {
    let old_progress = match self.seek_ms {
      Some(seek_ms) => seek_ms,
      None => self.song_progress_ms,
    };
    let new_progress =
      (old_progress as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
    self.seek_ms = Some(new_progress as u128);
    // Dispatch the seek immediately instead of waiting for the poll interval
    self.dispatch(IoEvent::Seek(new_progress));
  }

  pub fn get_recommendations_for_seed(
    &mut self,
    seed_artists: Option<Vec<String>>,
    seed_tracks: Option<Vec<String>>,
    first_track: Option<FullTrack>,
  ) {
    let user_country = self.get_user_country();
    let seed_artist_ids = seed_artists.and_then(|ids| {
      ids
        .into_iter()
        .map(|id| ArtistId::from_id(id).ok())
        .collect()
    });
    let seed_track_ids = seed_tracks.and_then(|ids| {
      ids
        .into_iter()
        .map(|id| TrackId::from_id(id).ok())
        .collect()
    });
    self.dispatch(IoEvent::GetRecommendationsForSeed(
      seed_artist_ids,
      seed_track_ids,
      Box::new(first_track),
      user_country,
    ));
  }

  pub fn get_recommendations_for_track_id(&mut self, id: String) {
    let user_country = self.get_user_country();
    if let Ok(track_id) = TrackId::from_id(id) {
      self.dispatch(IoEvent::GetRecommendationsForTrackId(
        track_id,
        user_country,
      ));
    }
  }

  pub fn increase_volume(&mut self) {
    if let Some(context) = self.current_playback_context.clone() {
      let current_volume = context.device.volume_percent.unwrap_or(0) as u8;
      let next_volume = min(
        current_volume + self.user_config.behavior.volume_increment,
        100,
      );

      if next_volume != current_volume {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
      }
    }
  }

  pub fn decrease_volume(&mut self) {
    if let Some(context) = self.current_playback_context.clone() {
      let current_volume = context.device.volume_percent.unwrap_or(0) as i8;
      let next_volume = max(
        current_volume - self.user_config.behavior.volume_increment as i8,
        0,
      );

      if next_volume != current_volume {
        self.dispatch(IoEvent::ChangeVolume(next_volume as u8));
      }
    }
  }

  pub fn handle_error(&mut self, e: anyhow::Error) {
    self.push_navigation_stack(RouteId::Error, ActiveBlock::Error);
    self.api_error = e.to_string();
  }

  pub fn toggle_playback(&mut self) {
    let is_playing = if self.is_streaming_active {
      self
        .native_is_playing
        .or_else(|| self.current_playback_context.as_ref().map(|c| c.is_playing))
        .unwrap_or(false)
    } else {
      self
        .current_playback_context
        .as_ref()
        .map(|c| c.is_playing)
        .unwrap_or(false)
    };

    if is_playing {
      self.dispatch(IoEvent::PausePlayback);
    } else {
      // When no offset or uris are passed, spotify will resume current playback
      self.dispatch(IoEvent::StartPlayback(None, None, None));
    }
  }

  pub fn previous_track(&mut self) {
    if self.song_progress_ms >= 3_000 {
      self.dispatch(IoEvent::Seek(0));
    } else {
      self.dispatch(IoEvent::PreviousTrack);
    }
  }

  // The navigation_stack actually only controls the large block to the right of `library` and
  // `playlists`
  pub fn push_navigation_stack(&mut self, next_route_id: RouteId, next_active_block: ActiveBlock) {
    if !self
      .navigation_stack
      .last()
      .map(|last_route| last_route.id == next_route_id)
      .unwrap_or(false)
    {
      self.navigation_stack.push(Route {
        id: next_route_id,
        active_block: next_active_block,
        hovered_block: next_active_block,
      });
    }
  }

  pub fn pop_navigation_stack(&mut self) -> Option<Route> {
    if self.navigation_stack.len() == 1 {
      None
    } else {
      self.navigation_stack.pop()
    }
  }

  pub fn get_current_route(&self) -> &Route {
    // if for some reason there is no route return the default
    self.navigation_stack.last().unwrap_or(&DEFAULT_ROUTE)
  }

  fn get_current_route_mut(&mut self) -> &mut Route {
    self.navigation_stack.last_mut().unwrap()
  }

  pub fn set_current_route_state(
    &mut self,
    active_block: Option<ActiveBlock>,
    hovered_block: Option<ActiveBlock>,
  ) {
    let current_route = self.get_current_route_mut();
    if let Some(active_block) = active_block {
      current_route.active_block = active_block;
    }
    if let Some(hovered_block) = hovered_block {
      current_route.hovered_block = hovered_block;
    }
  }

  pub fn copy_song_url(&mut self) {
    let clipboard = match &mut self.clipboard {
      Some(ctx) => ctx,
      None => return,
    };

    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      match item {
        PlayableItem::Track(track) => {
          let track_id = track.id.as_ref().map(|id| id.id().to_string());

          match track_id {
            Some(id) if !id.is_empty() => {
              if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/track/{}", id)) {
                self.handle_error(anyhow!("failed to set clipboard content: {}", e));
              }
            }
            _ => {
              self.handle_error(anyhow!("Track has no ID"));
            }
          }
        }
        PlayableItem::Episode(episode) => {
          let episode_id = episode.id.id().to_string();
          if let Err(e) =
            clipboard.set_text(format!("https://open.spotify.com/episode/{}", episode_id))
          {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
      }
    }
  }

  pub fn copy_album_url(&mut self) {
    let clipboard = match &mut self.clipboard {
      Some(ctx) => ctx,
      None => return,
    };

    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      match item {
        PlayableItem::Track(track) => {
          let album_id = track.album.id.as_ref().map(|id| id.id().to_string());

          match album_id {
            Some(id) if !id.is_empty() => {
              if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/album/{}", id)) {
                self.handle_error(anyhow!("failed to set clipboard content: {}", e));
              }
            }
            _ => {
              self.handle_error(anyhow!("Album has no ID"));
            }
          }
        }
        PlayableItem::Episode(episode) => {
          let show_id = episode.show.id.id().to_string();
          if let Err(e) = clipboard.set_text(format!("https://open.spotify.com/show/{}", show_id)) {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
      }
    }
  }

  pub fn set_saved_tracks_to_table(&mut self, saved_track_page: &Page<SavedTrack>) {
    self.dispatch(IoEvent::SetTracksToTable(
      saved_track_page
        .items
        .clone()
        .into_iter()
        .map(|item| item.track)
        .collect::<Vec<FullTrack>>(),
    ));
  }

  pub fn set_saved_artists_to_table(&mut self, saved_artists_page: &CursorBasedPage<FullArtist>) {
    self.dispatch(IoEvent::SetArtistsToTable(
      saved_artists_page
        .items
        .clone()
        .into_iter()
        .collect::<Vec<FullArtist>>(),
    ))
  }

  pub fn get_current_user_saved_artists_next(&mut self) {
    match self
      .library
      .saved_artists
      .get_results(Some(self.library.saved_artists.index + 1))
      .cloned()
    {
      Some(saved_artists) => {
        self.set_saved_artists_to_table(&saved_artists);
        self.library.saved_artists.index += 1
      }
      None => {
        if let Some(saved_artists) = &self.library.saved_artists.clone().get_results(None) {
          if let Some(last_artist) = saved_artists.items.last() {
            self.dispatch(IoEvent::GetFollowedArtists(Some(
              last_artist.id.clone().into_static(),
            )));
          }
        }
      }
    }
  }

  pub fn get_current_user_saved_artists_previous(&mut self) {
    if self.library.saved_artists.index > 0 {
      self.library.saved_artists.index -= 1;
    }

    if let Some(saved_artists) = &self.library.saved_artists.get_results(None).cloned() {
      self.set_saved_artists_to_table(saved_artists);
    }
  }

  pub fn get_current_user_saved_tracks_next(&mut self) {
    // Before fetching the next tracks, check if we have already fetched them
    match self
      .library
      .saved_tracks
      .get_results(Some(self.library.saved_tracks.index + 1))
      .cloned()
    {
      Some(saved_tracks) => {
        self.set_saved_tracks_to_table(&saved_tracks);
        self.library.saved_tracks.index += 1
      }
      None => {
        if let Some(saved_tracks) = &self.library.saved_tracks.get_results(None) {
          let offset = Some(saved_tracks.offset + saved_tracks.limit);
          self.dispatch(IoEvent::GetCurrentSavedTracks(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_tracks_previous(&mut self) {
    if self.library.saved_tracks.index > 0 {
      self.library.saved_tracks.index -= 1;
    }

    if let Some(saved_tracks) = &self.library.saved_tracks.get_results(None).cloned() {
      self.set_saved_tracks_to_table(saved_tracks);
    }
  }

  pub fn shuffle(&mut self) {
    if let Some(context) = &self.current_playback_context.clone() {
      self.dispatch(IoEvent::Shuffle(!context.shuffle_state));
    };
  }

  pub fn get_current_user_saved_albums_next(&mut self) {
    match self
      .library
      .saved_albums
      .get_results(Some(self.library.saved_albums.index + 1))
      .cloned()
    {
      Some(_) => self.library.saved_albums.index += 1,
      None => {
        if let Some(saved_albums) = &self.library.saved_albums.get_results(None) {
          let offset = Some(saved_albums.offset + saved_albums.limit);
          self.dispatch(IoEvent::GetCurrentUserSavedAlbums(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_albums_previous(&mut self) {
    if self.library.saved_albums.index > 0 {
      self.library.saved_albums.index -= 1;
    }
  }

  pub fn current_user_saved_album_delete(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(albums) = &self.search_results.albums {
          if let Some(selected_index) = self.search_results.selected_album_index {
            let selected_album = &albums.items[selected_index];
            if let Some(album_id) = selected_album.id.clone() {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id.into_static()));
            }
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(albums) = self.library.saved_albums.get_results(None) {
          if let Some(selected_album) = albums.items.get(self.album_list_index) {
            let album_id = selected_album.album.id.clone();
            self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id.into_static()));
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          if let Some(selected_album) = artist.albums.items.get(artist.selected_album_index) {
            if let Some(album_id) = selected_album.id.clone() {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id.into_static()));
            }
          }
        }
      }
      _ => (),
    }
  }

  pub fn current_user_saved_album_add(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(albums) = &self.search_results.albums {
          if let Some(selected_index) = self.search_results.selected_album_index {
            let selected_album = &albums.items[selected_index];
            if let Some(album_id) = selected_album.id.clone() {
              self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id.into_static()));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          if let Some(selected_album) = artist.albums.items.get(artist.selected_album_index) {
            if let Some(album_id) = selected_album.id.clone() {
              self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id.into_static()));
            }
          }
        }
      }
      _ => (),
    }
  }

  pub fn get_current_user_saved_shows_next(&mut self) {
    match self
      .library
      .saved_shows
      .get_results(Some(self.library.saved_shows.index + 1))
      .cloned()
    {
      Some(_) => self.library.saved_shows.index += 1,
      None => {
        if let Some(saved_shows) = &self.library.saved_shows.get_results(None) {
          let offset = Some(saved_shows.offset + saved_shows.limit);
          self.dispatch(IoEvent::GetCurrentUserSavedShows(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_shows_previous(&mut self) {
    if self.library.saved_shows.index > 0 {
      self.library.saved_shows.index -= 1;
    }
  }

  pub fn get_episode_table_next(&mut self, show_id: String) {
    match self
      .library
      .show_episodes
      .get_results(Some(self.library.show_episodes.index + 1))
      .cloned()
    {
      Some(_) => self.library.show_episodes.index += 1,
      None => {
        if let Some(show_episodes) = &self.library.show_episodes.get_results(None) {
          let offset = Some(show_episodes.offset + show_episodes.limit);
          if let Ok(show_id) = ShowId::from_id(show_id) {
            self.dispatch(IoEvent::GetCurrentShowEpisodes(show_id, offset));
          }
        }
      }
    }
  }

  pub fn get_episode_table_previous(&mut self) {
    if self.library.show_episodes.index > 0 {
      self.library.show_episodes.index -= 1;
    }
  }

  pub fn user_unfollow_artists(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(artists) = &self.search_results.artists {
          if let Some(selected_index) = self.search_results.selected_artists_index {
            let selected_artist: &FullArtist = &artists.items[selected_index];
            self.dispatch(IoEvent::UserUnfollowArtists(vec![selected_artist
              .id
              .clone()
              .into_static()]));
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(artists) = self.library.saved_artists.get_results(None) {
          if let Some(selected_artist) = artists.items.get(self.artists_list_index) {
            self.dispatch(IoEvent::UserUnfollowArtists(vec![selected_artist
              .id
              .clone()
              .into_static()]));
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          let selected_artis = &artist.related_artists[artist.selected_related_artist_index];
          self.dispatch(IoEvent::UserUnfollowArtists(vec![selected_artis
            .id
            .clone()
            .into_static()]));
        }
      }
      _ => (),
    };
  }

  pub fn user_follow_artists(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(artists) = &self.search_results.artists {
          if let Some(selected_index) = self.search_results.selected_artists_index {
            let selected_artist: &FullArtist = &artists.items[selected_index];
            self.dispatch(IoEvent::UserFollowArtists(vec![selected_artist
              .id
              .clone()
              .into_static()]));
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          let selected_artis = &artist.related_artists[artist.selected_related_artist_index];
          self.dispatch(IoEvent::UserFollowArtists(vec![selected_artis
            .id
            .clone()
            .into_static()]));
        }
      }
      _ => (),
    }
  }

  pub fn user_follow_playlist(&mut self) {
    if let SearchResult {
      playlists: Some(ref playlists),
      selected_playlists_index: Some(selected_index),
      ..
    } = self.search_results
    {
      let selected_playlist: &SimplifiedPlaylist = &playlists.items[selected_index];
      let selected_id = selected_playlist.id.clone();
      let selected_public = selected_playlist.public;
      let selected_owner_id = selected_playlist.owner.id.clone();
      self.dispatch(IoEvent::UserFollowPlaylist(
        selected_owner_id.into_static(),
        selected_id.into_static(),
        selected_public,
      ));
    }
  }

  pub fn user_unfollow_playlist(&mut self) {
    if let (Some(playlists), Some(selected_index), Some(user)) =
      (&self.playlists, self.selected_playlist_index, &self.user)
    {
      let selected_playlist = &playlists.items[selected_index];
      let selected_id = selected_playlist.id.clone();
      let user_id = user.id.clone();
      self.dispatch(IoEvent::UserUnfollowPlaylist(
        user_id.into_static(),
        selected_id.into_static(),
      ));
    }
  }

  pub fn user_unfollow_playlist_search_result(&mut self) {
    if let (Some(playlists), Some(selected_index), Some(user)) = (
      &self.search_results.playlists,
      self.search_results.selected_playlists_index,
      &self.user,
    ) {
      let selected_playlist = &playlists.items[selected_index];
      let selected_id = selected_playlist.id.clone();
      let user_id = user.id.clone();
      self.dispatch(IoEvent::UserUnfollowPlaylist(
        user_id.into_static(),
        selected_id.into_static(),
      ));
    }
  }

  pub fn user_follow_show(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows {
          if let Some(selected_index) = self.search_results.selected_shows_index {
            if let Some(show_id) = shows.items.get(selected_index).map(|item| item.id.clone()) {
              self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id.into_static()));
            }
          }
        }
      }
      ActiveBlock::EpisodeTable => match self.episode_table_context {
        EpisodeTableContext::Full => {
          if let Some(selected_episode) = self.selected_show_full.clone() {
            let show_id = selected_episode.show.id;
            self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id.into_static()));
          }
        }
        EpisodeTableContext::Simplified => {
          if let Some(selected_episode) = self.selected_show_simplified.clone() {
            let show_id = selected_episode.show.id;
            self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id.into_static()));
          }
        }
      },
      _ => (),
    }
  }

  pub fn user_unfollow_show(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::Podcasts => {
        if let Some(shows) = self.library.saved_shows.get_results(None) {
          if let Some(selected_show) = shows.items.get(self.shows_list_index) {
            let show_id = selected_show.show.id.clone();
            self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id.into_static()));
          }
        }
      }
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows {
          if let Some(selected_index) = self.search_results.selected_shows_index {
            let show_id = shows.items[selected_index].id.clone();
            self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id.into_static()));
          }
        }
      }
      ActiveBlock::EpisodeTable => match self.episode_table_context {
        EpisodeTableContext::Full => {
          if let Some(selected_episode) = self.selected_show_full.clone() {
            let show_id = selected_episode.show.id;
            self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id.into_static()));
          }
        }
        EpisodeTableContext::Simplified => {
          if let Some(selected_episode) = self.selected_show_simplified.clone() {
            let show_id = selected_episode.show.id;
            self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id.into_static()));
          }
        }
      },
      _ => (),
    }
  }

  /// Toggle the audio analysis visualization view
  /// This now uses local FFT analysis instead of the deprecated Spotify API
  pub fn get_audio_analysis(&mut self) {
    if self.get_current_route().id != RouteId::Analysis {
      // Enter visualization mode
      self.push_navigation_stack(RouteId::Analysis, ActiveBlock::Analysis);
    }
    // Spectrum data will be updated by the audio capture system on each tick
  }

  pub fn repeat(&mut self) {
    if let Some(context) = &self.current_playback_context.clone() {
      self.dispatch(IoEvent::Repeat(context.repeat_state));
    }
  }

  pub fn get_artist(&mut self, artist_id: ArtistId<'static>, input_artist_name: String) {
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetArtist(
      artist_id,
      input_artist_name,
      user_country,
    ));
  }

  pub fn get_user_country(&self) -> Option<Country> {
    self.user.as_ref().and_then(|user| user.country)
  }

  pub fn calculate_help_menu_offset(&mut self) {
    let old_offset = self.help_menu_offset;

    if self.help_menu_max_lines < self.help_docs_size {
      self.help_menu_offset = self.help_menu_page * self.help_menu_max_lines;
    }
    if self.help_menu_offset > self.help_docs_size {
      self.help_menu_offset = old_offset;
      self.help_menu_page -= 1;
    }
  }

  /// Load settings for the current category into settings_items
  pub fn load_settings_for_category(&mut self) {
    use crate::event::Key;

    // Helper to convert Key to displayable string
    fn key_to_string(key: &Key) -> String {
      match key {
        Key::Char(c) => c.to_string(),
        Key::Ctrl(c) => format!("ctrl-{}", c),
        Key::Alt(c) => format!("alt-{}", c),
        Key::Enter => "enter".to_string(),
        Key::Esc => "esc".to_string(),
        Key::Backspace => "backspace".to_string(),
        Key::Delete => "del".to_string(),
        Key::Left => "left".to_string(),
        Key::Right => "right".to_string(),
        Key::Up => "up".to_string(),
        Key::Down => "down".to_string(),
        Key::PageUp => "pageup".to_string(),
        Key::PageDown => "pagedown".to_string(),
        _ => "unknown".to_string(),
      }
    }

    self.settings_items = match self.settings_category {
      SettingsCategory::Behavior => vec![
        SettingItem {
          id: "behavior.seek_milliseconds".to_string(),
          name: "Seek Duration (ms)".to_string(),
          description: "Milliseconds to skip when seeking".to_string(),
          value: SettingValue::Number(self.user_config.behavior.seek_milliseconds as i64),
        },
        SettingItem {
          id: "behavior.volume_increment".to_string(),
          name: "Volume Increment".to_string(),
          description: "Volume change per keypress (0-100)".to_string(),
          value: SettingValue::Number(self.user_config.behavior.volume_increment as i64),
        },
        SettingItem {
          id: "behavior.tick_rate_milliseconds".to_string(),
          name: "Tick Rate (ms)".to_string(),
          description: "UI refresh rate in milliseconds".to_string(),
          value: SettingValue::Number(self.user_config.behavior.tick_rate_milliseconds as i64),
        },
        SettingItem {
          id: "behavior.enable_text_emphasis".to_string(),
          name: "Text Emphasis".to_string(),
          description: "Enable bold/italic text styling".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_text_emphasis),
        },
        SettingItem {
          id: "behavior.show_loading_indicator".to_string(),
          name: "Loading Indicator".to_string(),
          description: "Show loading status in UI".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.show_loading_indicator),
        },
        SettingItem {
          id: "behavior.enforce_wide_search_bar".to_string(),
          name: "Wide Search Bar".to_string(),
          description: "Force search bar to take full width".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enforce_wide_search_bar),
        },
        SettingItem {
          id: "behavior.set_window_title".to_string(),
          name: "Set Window Title".to_string(),
          description: "Update terminal window title with track info".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.set_window_title),
        },
        SettingItem {
          id: "behavior.liked_icon".to_string(),
          name: "Liked Icon".to_string(),
          description: "Icon for liked songs".to_string(),
          value: SettingValue::String(self.user_config.behavior.liked_icon.clone()),
        },
        SettingItem {
          id: "behavior.shuffle_icon".to_string(),
          name: "Shuffle Icon".to_string(),
          description: "Icon for shuffle mode".to_string(),
          value: SettingValue::String(self.user_config.behavior.shuffle_icon.clone()),
        },
        SettingItem {
          id: "behavior.playing_icon".to_string(),
          name: "Playing Icon".to_string(),
          description: "Icon for playing state".to_string(),
          value: SettingValue::String(self.user_config.behavior.playing_icon.clone()),
        },
        SettingItem {
          id: "behavior.paused_icon".to_string(),
          name: "Paused Icon".to_string(),
          description: "Icon for paused state".to_string(),
          value: SettingValue::String(self.user_config.behavior.paused_icon.clone()),
        },
      ],
      SettingsCategory::Keybindings => vec![
        SettingItem {
          id: "keys.back".to_string(),
          name: "Back".to_string(),
          description: "Go back / quit".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.back)),
        },
        SettingItem {
          id: "keys.next_page".to_string(),
          name: "Next Page".to_string(),
          description: "Navigate to next page".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.next_page)),
        },
        SettingItem {
          id: "keys.previous_page".to_string(),
          name: "Previous Page".to_string(),
          description: "Navigate to previous page".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.previous_page)),
        },
        SettingItem {
          id: "keys.toggle_playback".to_string(),
          name: "Toggle Playback".to_string(),
          description: "Play/pause".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.toggle_playback)),
        },
        SettingItem {
          id: "keys.seek_backwards".to_string(),
          name: "Seek Backwards".to_string(),
          description: "Seek backwards in track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.seek_backwards)),
        },
        SettingItem {
          id: "keys.seek_forwards".to_string(),
          name: "Seek Forwards".to_string(),
          description: "Seek forwards in track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.seek_forwards)),
        },
        SettingItem {
          id: "keys.next_track".to_string(),
          name: "Next Track".to_string(),
          description: "Skip to next track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.next_track)),
        },
        SettingItem {
          id: "keys.previous_track".to_string(),
          name: "Previous Track".to_string(),
          description: "Go to previous track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.previous_track)),
        },
        SettingItem {
          id: "keys.shuffle".to_string(),
          name: "Shuffle".to_string(),
          description: "Toggle shuffle mode".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.shuffle)),
        },
        SettingItem {
          id: "keys.repeat".to_string(),
          name: "Repeat".to_string(),
          description: "Cycle repeat mode".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.repeat)),
        },
        SettingItem {
          id: "keys.search".to_string(),
          name: "Search".to_string(),
          description: "Open search".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.search)),
        },
        SettingItem {
          id: "keys.help".to_string(),
          name: "Help".to_string(),
          description: "Show help menu".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.help)),
        },
      ],
      SettingsCategory::Theme => {
        fn color_to_string(color: ratatui::style::Color) -> String {
          match color {
            ratatui::style::Color::Rgb(r, g, b) => format!("{},{},{}", r, g, b),
            ratatui::style::Color::Reset => "Reset".to_string(),
            ratatui::style::Color::Black => "Black".to_string(),
            ratatui::style::Color::Red => "Red".to_string(),
            ratatui::style::Color::Green => "Green".to_string(),
            ratatui::style::Color::Yellow => "Yellow".to_string(),
            ratatui::style::Color::Blue => "Blue".to_string(),
            ratatui::style::Color::Magenta => "Magenta".to_string(),
            ratatui::style::Color::Cyan => "Cyan".to_string(),
            ratatui::style::Color::Gray => "Gray".to_string(),
            ratatui::style::Color::DarkGray => "DarkGray".to_string(),
            ratatui::style::Color::LightRed => "LightRed".to_string(),
            ratatui::style::Color::LightGreen => "LightGreen".to_string(),
            ratatui::style::Color::LightYellow => "LightYellow".to_string(),
            ratatui::style::Color::LightBlue => "LightBlue".to_string(),
            ratatui::style::Color::LightMagenta => "LightMagenta".to_string(),
            ratatui::style::Color::LightCyan => "LightCyan".to_string(),
            ratatui::style::Color::White => "White".to_string(),
            _ => "Unknown".to_string(),
          }
        }

        vec![
          SettingItem {
            id: "theme.preset".to_string(),
            name: "Theme Preset".to_string(),
            description: "Choose a preset theme or customize below".to_string(),
            value: SettingValue::Preset("Default (Cyan)".to_string()), // Default preset
          },
          SettingItem {
            id: "theme.active".to_string(),
            name: "Active Color".to_string(),
            description: "Color for active elements".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.active)),
          },
          SettingItem {
            id: "theme.banner".to_string(),
            name: "Banner Color".to_string(),
            description: "Color for banner text".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.banner)),
          },
          SettingItem {
            id: "theme.hint".to_string(),
            name: "Hint Color".to_string(),
            description: "Color for hints".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.hint)),
          },
          SettingItem {
            id: "theme.hovered".to_string(),
            name: "Hovered Color".to_string(),
            description: "Color for hovered elements".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.hovered)),
          },
          SettingItem {
            id: "theme.selected".to_string(),
            name: "Selected Color".to_string(),
            description: "Color for selected items".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.selected)),
          },
          SettingItem {
            id: "theme.inactive".to_string(),
            name: "Inactive Color".to_string(),
            description: "Color for inactive elements".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.inactive)),
          },
          SettingItem {
            id: "theme.text".to_string(),
            name: "Text Color".to_string(),
            description: "Default text color".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.text)),
          },
          SettingItem {
            id: "theme.error_text".to_string(),
            name: "Error Text Color".to_string(),
            description: "Color for error messages".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.error_text)),
          },
          SettingItem {
            id: "theme.playbar_background".to_string(),
            name: "Playbar Background".to_string(),
            description: "Background color for playbar".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.playbar_background)),
          },
          SettingItem {
            id: "theme.playbar_progress".to_string(),
            name: "Playbar Progress".to_string(),
            description: "Color for playbar progress".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.playbar_progress)),
          },
          SettingItem {
            id: "theme.highlighted_lyrics".to_string(),
            name: "Lyrics Highlight".to_string(),
            description: "Color for current lyrics line".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.highlighted_lyrics)),
          },
        ]
      }
    };
    self.settings_selected_index = 0;
  }

  /// Apply changes from settings_items back to user_config
  pub fn apply_settings_changes(&mut self) {
    for setting in &self.settings_items {
      match setting.id.as_str() {
        // Behavior settings
        "behavior.seek_milliseconds" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.seek_milliseconds = *v as u32;
          }
        }
        "behavior.volume_increment" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.volume_increment = (*v).clamp(0, 100) as u8;
          }
        }
        "behavior.tick_rate_milliseconds" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.tick_rate_milliseconds = (*v).max(1) as u64;
          }
        }
        "behavior.enable_text_emphasis" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.enable_text_emphasis = *v;
          }
        }
        "behavior.show_loading_indicator" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.show_loading_indicator = *v;
          }
        }
        "behavior.enforce_wide_search_bar" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.enforce_wide_search_bar = *v;
          }
        }
        "behavior.set_window_title" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.set_window_title = *v;
          }
        }
        "behavior.liked_icon" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.liked_icon = v.clone();
          }
        }
        "behavior.shuffle_icon" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.shuffle_icon = v.clone();
          }
        }
        "behavior.playing_icon" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.playing_icon = v.clone();
          }
        }
        "behavior.paused_icon" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.paused_icon = v.clone();
          }
        }
        // Theme preset - applies all colors at once
        "theme.preset" => {
          if let SettingValue::Preset(preset_name) = &setting.value {
            use crate::user_config::ThemePreset;
            let preset = ThemePreset::from_name(preset_name);
            if preset != ThemePreset::Custom {
              // Apply the preset's theme colors
              self.user_config.theme = preset.to_theme();
            }
          }
        }
        // Note: Individual color changes and keybindings require more complex parsing
        // and may need restart to take full effect
        _ => {}
      }
    }
  }
}
