use crate::core::sort::{SortContext, SortState};
use crate::core::user_config::{color_to_string, UserConfig};
use crate::infra::network::sync::{PartySession, PartyStatus};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use anyhow::anyhow;
use ratatui::layout::Size;
use rspotify::{
  model::enums::Country,
  model::{
    album::{FullAlbum, SavedAlbum, SimplifiedAlbum},
    artist::FullArtist,
    context::{CurrentPlaybackContext, CurrentUserQueue},
    device::DevicePayload,
    idtypes::{ArtistId, PlaylistId, ShowId, TrackId},
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
use serde::de::DeserializeOwned;
use std::cell::Cell;
use std::sync::mpsc::Sender;
#[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
use std::sync::Arc;
use std::{
  cmp::{max, min},
  collections::HashSet,
  time::{Duration, Instant, SystemTime},
};

use arboard::Clipboard;
use log::info;

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

/// How long to ignore position updates after a seek (ms)
/// This prevents the UI from jumping back to old positions while the seek completes
pub const SEEK_POSITION_IGNORE_MS: u128 = 500;

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

  pub fn clear(&mut self) {
    self.index = 0;
    self.pages.clear();
  }

  pub fn add_pages(&mut self, new_pages: T) {
    self.pages.push(new_pages);
    // Whenever a new page is added, set the active index to the end of the vector
    self.index = self.pages.len() - 1;
  }
}

// Offset-keyed page caches are always kept sorted by page.offset, but the cache can be sparse.
// Visible-page identity must be derived from page.offset, not raw cache index adjacency.
impl<T: DeserializeOwned> ScrollableResultPages<Page<T>> {
  pub fn page_index_for_offset(&self, offset: u32) -> Option<usize> {
    self
      .pages
      .binary_search_by_key(&offset, |page| page.offset)
      .ok()
  }

  pub fn upsert_page_by_offset(&mut self, new_page: Page<T>) -> usize {
    let active_page_offset = self.pages.get(self.index).map(|page| page.offset);
    let new_page_offset = new_page.offset;

    match self
      .pages
      .binary_search_by_key(&new_page.offset, |page| page.offset)
    {
      Ok(index) => {
        self.pages[index] = new_page;
      }
      Err(index) => {
        self.pages.insert(index, new_page);
      }
    };

    if let Some(active_page_offset) = active_page_offset {
      if let Some(active_page_index) = self.page_index_for_offset(active_page_offset) {
        self.index = active_page_index;
      }
    } else if !self.pages.is_empty() {
      self.index = 0;
    }

    self
      .page_index_for_offset(new_page_offset)
      .expect("upserted page offset must exist in cache")
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
  AddTrackToPlaylistPicker,
  RemoveTrackFromPlaylistConfirm,
  PersistKeybindingFallback,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CapabilityState {
  #[default]
  Unknown,
  Yes,
  No,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerminalInputCapabilities {
  pub keyboard_enhancement_supported: bool,
  pub keyboard_enhancement_enabled: bool,
  pub ctrl_punct_reliable: CapabilityState,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyFallbackReason {
  CtrlCommaNotReported,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeybindingRuntimeState {
  pub effective_open_settings: Option<Key>,
  pub fallback_reason: Option<KeyFallbackReason>,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  pub fallback_notice_shown: bool,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  pub persist_prompt_shown: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PendingKeybindingPersist {
  pub open_settings_key: Key,
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
  LyricsView,
  CoverArtView,
  Dialog(DialogContext),

  AnnouncementPrompt,
  ExitPrompt,
  Settings,
  SortMenu,
  Queue,
  Party,
  CreatePlaylistForm,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RouteId {
  Analysis,
  AlbumTracks,
  AlbumList,
  Artist,
  LyricsView,
  CoverArtView,
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

  AnnouncementPrompt,
  ExitPrompt,
  Settings,
  HelpMenu,
  Queue,
  Party,
  CreatePlaylist,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnnouncementLevel {
  Info,
  Warning,
  Critical,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Announcement {
  pub id: String,
  pub title: String,
  pub body: String,
  pub level: AnnouncementLevel,
  pub url: Option<String>,
  pub received_at: Instant,
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
pub struct PendingPlaylistTrackAdd {
  pub track_id: TrackId<'static>,
  pub track_name: String,
}

#[derive(Clone)]
pub struct PendingPlaylistTrackRemoval {
  pub playlist_id: PlaylistId<'static>,
  pub playlist_name: String,
  pub track_id: TrackId<'static>,
  pub track_name: String,
  pub position: usize,
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
#[allow(dead_code)]
pub struct SelectedFullAlbum {
  pub album: FullAlbum,
  pub selected_index: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Artist {
  pub artist_id: String,
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
  pub artists_display: String,
  #[allow(dead_code)]
  pub album: String, // Reserved for future use (e.g., displaying album in playbar)
  pub duration_ms: u32,
}

/// A node in the playlist folder hierarchy from Spotify's rootlist
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PlaylistFolderNodeType {
  Folder,
  Playlist,
}

/// A node in the playlist folder hierarchy from Spotify's rootlist
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PlaylistFolderNode {
  pub name: Option<String>,
  pub node_type: PlaylistFolderNodeType,
  pub uri: String,
  pub children: Vec<PlaylistFolderNode>,
}

/// A folder entry for navigation in the playlist panel
#[derive(Clone, Debug)]
pub struct PlaylistFolder {
  pub name: String,
  /// Folder ID this item is visible in (which folder "page" it appears on)
  pub current_id: usize,
  /// Folder ID this item navigates to when selected
  pub target_id: usize,
}

/// A flattened item for display in the playlist panel
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum PlaylistFolderItem {
  Folder(PlaylistFolder),
  Playlist {
    /// Index into app.all_playlists
    index: usize,
    /// Folder ID this playlist is visible in
    current_id: usize,
  },
}

/// Which stage of the "Create Playlist" form we are on
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CreatePlaylistStage {
  #[default]
  Name,
  AddTracks,
}

/// Which panel inside the AddTracks stage has focus
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CreatePlaylistFocus {
  #[default]
  SearchInput,
  SearchResults,
  AddedTracks,
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
  /// A value cycling through a fixed list of options: (current, all options).
  Cycle(String, &'static [&'static str]),
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
      SettingValue::Cycle(v, _) => v.clone(),
    }
  }
}

/// Represents a single configurable setting
#[derive(Clone, Debug, PartialEq)]
pub struct SettingItem {
  pub id: String,   // e.g., "behavior.seek_milliseconds"
  pub name: String, // e.g., "Seek Duration"
  #[allow(dead_code)]
  pub description: String, // e.g., "Milliseconds to skip when seeking" (for future tooltip)
  pub value: SettingValue,
}

pub struct App {
  /// What the user actually wants the volume to be. We keep this around until
  /// Spotify's API comes back with the same value — otherwise a slow poll
  /// response can flash the old volume back on screen.
  pub pending_volume: Option<u8>,
  /// The last value we actually sent to the API. Lets us skip redundant
  /// dispatches while we're just waiting for confirmation.
  pub last_dispatched_volume: Option<u8>,
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
  /// Set to true when a track ends naturally and stop_after_current_track is enabled.
  /// The next Playing event will see this flag and immediately pause.
  #[allow(dead_code)]
  pub pending_stop_after_track: bool,
  pub devices: Option<DevicePayload>,
  pub queue: Option<CurrentUserQueue>,
  pub queue_selected_index: usize,
  #[cfg(feature = "cover-art")]
  pub cover_art: crate::tui::cover_art::CoverArt,
  // Inputs:
  // input is the string for input;
  // input_idx is the index of the cursor in terms of character;
  // input_cursor_position is the sum of the width of characters preceding the cursor.
  // Reason for this complication is due to non-ASCII characters, they may
  // take more than 1 bytes to store and more than 1 character width to display.
  pub input: Vec<char>,
  pub input_idx: usize,
  pub input_cursor_position: u16,
  /// Horizontal scroll offset for the input box, computed during rendering.
  pub input_scroll_offset: Cell<u16>,
  pub liked_song_ids_set: HashSet<String>,
  pub followed_artist_ids_set: HashSet<String>,
  pub saved_album_ids_set: HashSet<String>,
  pub saved_show_ids_set: HashSet<String>,
  pub large_search_limit: u32,
  pub library: Library,
  pub playlist_offset: u32,
  pub playlist_tracks: Option<Page<PlaylistItem>>,
  pub playlist_track_pages: ScrollableResultPages<Page<PlaylistItem>>,
  pub playlist_track_table_id: Option<PlaylistId<'static>>,
  pub playlists: Option<Page<SimplifiedPlaylist>>,
  pub recently_played: SpotifyResultAndSelectedIndex<Option<CursorBasedPage<PlayHistory>>>,
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
  /// Last time a native seek was actually sent to the player (for throttling)
  #[cfg(feature = "streaming")]
  pub last_native_seek: Option<Instant>,
  /// Pending seek position to send to player (throttled to avoid overwhelming librespot)
  #[cfg(feature = "streaming")]
  pub pending_native_seek: Option<u32>,
  /// Last time an API seek was sent (for throttling external device control)
  pub last_api_seek: Option<Instant>,
  /// Pending seek position for API (throttled to avoid overwhelming Spotify API)
  pub pending_api_seek: Option<u32>,
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
  pub pending_keybinding_persist: Option<PendingKeybindingPersist>,
  pub terminal_input_caps: TerminalInputCapabilities,
  pub keybinding_runtime: KeybindingRuntimeState,

  pub active_announcement: Option<Announcement>,
  pub pending_announcements: Vec<Announcement>,
  pub lyrics: Option<Vec<(u128, String)>>,
  pub lyrics_status: LyricsStatus,
  pub global_song_count: Option<u64>,
  pub global_song_count_failed: bool,
  // Settings screen state
  pub settings_category: SettingsCategory,
  pub settings_items: Vec<SettingItem>,
  pub settings_saved_items: Vec<SettingItem>,
  pub settings_selected_index: usize,
  pub settings_edit_mode: bool,
  pub settings_edit_buffer: String,
  pub settings_unsaved_prompt_visible: bool,
  pub settings_unsaved_prompt_save_selected: bool,
  /// Immediate track info from native player for instant UI updates
  pub native_track_info: Option<NativeTrackInfo>,
  /// Whether native streaming is active (disables API-based progress calculation)
  pub is_streaming_active: bool,
  /// Device id for the native streaming device when known
  #[allow(dead_code)]
  pub native_device_id: Option<String>,
  /// Native playback state - updated by player events, used when streaming is active
  /// This is more reliable than current_playback_context.is_playing during native streaming
  pub native_is_playing: Option<bool>,
  /// Timestamp of the last native device activation
  #[allow(dead_code)]
  pub last_device_activation: Option<Instant>,
  /// Whether a native device activation is still in progress
  #[allow(dead_code)]
  pub native_activation_pending: bool,
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
  /// Global animation tick counter, incremented every tick (~62 FPS)
  pub animation_tick: u64,
  /// Ephemeral status message shown in the playbar
  pub status_message: Option<String>,
  /// When to clear the status message
  pub status_message_expires_at: Option<Instant>,
  /// Listening party status
  pub party_status: PartyStatus,
  /// Active listening party session data
  pub party_session: Option<PartySession>,
  /// Input buffer for the party join code
  pub party_input: Vec<char>,
  /// Cursor position in party code input
  pub party_input_idx: usize,
  /// Input buffer for the required party guest name
  pub party_join_name: Vec<char>,
  /// Pending track table selection to apply when new page loads
  pub pending_track_table_selection: Option<PendingTrackSelection>,
  /// Maps visible track table rows to source playlist item positions.
  /// Used to remove a single selected playlist occurrence safely.
  pub playlist_track_positions: Option<Vec<usize>>,
  /// Selected playlist index in the add-to-playlist picker dialog
  pub playlist_picker_selected_index: usize,
  /// Pending track to add in add-to-playlist dialog flow
  pub pending_playlist_track_add: Option<PendingPlaylistTrackAdd>,
  /// Pending track removal info in remove-from-playlist confirmation flow
  pub pending_playlist_track_removal: Option<PendingPlaylistTrackRemoval>,
  /// Full flat list of all user playlists (all pages combined)
  pub all_playlists: Vec<SimplifiedPlaylist>,
  /// Folder tree from rootlist (None if not fetched or streaming disabled)
  pub _playlist_folder_nodes: Option<Vec<PlaylistFolderNode>>,
  /// Flattened folder+playlist items for display navigation
  pub playlist_folder_items: Vec<PlaylistFolderItem>,
  /// Current folder ID being viewed (0 = root)
  pub current_playlist_folder_id: usize,
  /// Incremented every time playlists are refreshed to guard stale background tasks
  pub _playlist_refresh_generation: u64,
  /// Incremented every time the saved tracks view is reloaded to guard stale prefetch tasks
  pub saved_tracks_prefetch_generation: u64,
  /// Incremented every time the playlist track table is reloaded to guard stale prefetch tasks
  pub playlist_tracks_prefetch_generation: u64,
  /// Tracks whether a ChangeVolume request is on its way to Spotify.
  /// When true, we hold off on sending another one — rapid key presses
  /// just update `pending_volume` and the latest value wins.
  pub is_volume_change_in_flight: bool,
  /// Reference to the native streaming player for direct control (bypasses event channel)
  #[cfg(feature = "streaming")]
  pub streaming_player: Option<Arc<crate::player::StreamingPlayer>>,
  /// Reference to MPRIS manager for emitting Seeked signals after native seeks
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub mpris_manager: Option<Arc<crate::mpris::MprisManager>>,

  // Create Playlist form state
  pub create_playlist_name: Vec<char>,
  pub create_playlist_name_idx: usize,
  pub create_playlist_name_cursor: u16,
  pub create_playlist_stage: CreatePlaylistStage,
  pub create_playlist_tracks: Vec<FullTrack>,
  pub create_playlist_search_results: Vec<FullTrack>,
  pub create_playlist_search_input: Vec<char>,
  pub create_playlist_search_idx: usize,
  pub create_playlist_search_cursor: u16,
  pub create_playlist_selected_result: usize,
  pub create_playlist_focus: CreatePlaylistFocus,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PendingTrackSelection {
  First,
  Last,
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
      pending_stop_after_track: false,
      devices: None,
      queue: None,
      queue_selected_index: 0,
      input: vec![],
      input_idx: 0,
      input_cursor_position: 0,
      input_scroll_offset: Cell::new(0),
      playlist_offset: 0,
      playlist_tracks: None,
      playlist_track_pages: ScrollableResultPages::new(),
      playlist_track_table_id: None,
      playlists: None,
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
      #[cfg(feature = "streaming")]
      last_native_seek: None,
      #[cfg(feature = "streaming")]
      pending_native_seek: None,
      last_api_seek: None,
      pending_api_seek: None,
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
      pending_keybinding_persist: None,
      terminal_input_caps: TerminalInputCapabilities::default(),
      keybinding_runtime: KeybindingRuntimeState::default(),

      active_announcement: None,
      pending_announcements: Vec::new(),
      lyrics: None,
      lyrics_status: LyricsStatus::default(),
      global_song_count: None,
      global_song_count_failed: false,
      // Settings defaults
      settings_category: SettingsCategory::default(),
      settings_items: Vec::new(),
      settings_saved_items: Vec::new(),
      settings_selected_index: 0,
      settings_edit_mode: false,
      settings_edit_buffer: String::new(),
      settings_unsaved_prompt_visible: false,
      settings_unsaved_prompt_save_selected: true,
      native_track_info: None,
      is_streaming_active: false,
      native_device_id: None,
      native_is_playing: None,
      last_device_activation: None,
      native_activation_pending: false,
      // Sort menu defaults
      sort_menu_visible: false,
      sort_menu_selected: 0,
      sort_context: None,
      playlist_sort: SortState::new(),
      album_sort: SortState::new(),
      artist_sort: SortState::new(),
      liked_song_animation_frame: None,
      animation_tick: 0,
      status_message: None,
      status_message_expires_at: None,
      party_status: PartyStatus::default(),
      party_session: None,
      party_input: Vec::new(),
      party_input_idx: 0,
      party_join_name: Vec::new(),
      pending_track_table_selection: None,
      playlist_track_positions: None,
      playlist_picker_selected_index: 0,
      pending_playlist_track_add: None,
      pending_playlist_track_removal: None,
      all_playlists: Vec::new(),
      _playlist_folder_nodes: None,
      playlist_folder_items: Vec::new(),
      current_playlist_folder_id: 0,
      _playlist_refresh_generation: 0,
      saved_tracks_prefetch_generation: 0,
      playlist_tracks_prefetch_generation: 0,
      is_volume_change_in_flight: false,
      pending_volume: None,
      last_dispatched_volume: None,
      #[cfg(feature = "streaming")]
      streaming_player: None,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: None,
      #[cfg(feature = "cover-art")]
      cover_art: crate::tui::cover_art::CoverArt::new(),
      create_playlist_name: Vec::new(),
      create_playlist_name_idx: 0,
      create_playlist_name_cursor: 0,
      create_playlist_stage: CreatePlaylistStage::Name,
      create_playlist_tracks: Vec::new(),
      create_playlist_search_results: Vec::new(),
      create_playlist_search_input: Vec::new(),
      create_playlist_search_idx: 0,
      create_playlist_search_cursor: 0,
      create_playlist_selected_result: 0,
      create_playlist_focus: CreatePlaylistFocus::SearchInput,
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

  #[allow(dead_code)]
  pub fn enqueue_announcements(&mut self, announcements: Vec<Announcement>) {
    if announcements.is_empty() {
      return;
    }

    let mut existing_ids: HashSet<String> = self
      .pending_announcements
      .iter()
      .map(|announcement| announcement.id.clone())
      .collect();

    if let Some(active) = &self.active_announcement {
      existing_ids.insert(active.id.clone());
    }

    let mut incoming = announcements
      .into_iter()
      .filter(|announcement| existing_ids.insert(announcement.id.clone()))
      .collect::<Vec<Announcement>>();

    if self.active_announcement.is_none() {
      if let Some(first) = incoming.first().cloned() {
        self.active_announcement = Some(first);
        incoming.remove(0);
      }
    }

    self.pending_announcements.extend(incoming);
  }

  pub fn dismiss_active_announcement(&mut self) -> Option<String> {
    let dismissed_id = self
      .active_announcement
      .take()
      .map(|announcement| announcement.id);

    if let Some(next_announcement) = self.pending_announcements.first().cloned() {
      self.active_announcement = Some(next_announcement);
      self.pending_announcements.remove(0);
    }

    dismissed_id
  }

  // Close the IO channel to allow the network thread to exit gracefully
  pub fn close_io_channel(&mut self) {
    self.io_tx = None;
  }

  pub fn clear_playlist_track_dialog_state(&mut self) {
    self.pending_playlist_track_add = None;
    self.pending_playlist_track_removal = None;
    self.playlist_picker_selected_index = 0;
  }

  pub fn clear_dialog_state(&mut self) {
    self.dialog = None;
    self.confirm = false;
    self.pending_keybinding_persist = None;
    self.clear_playlist_track_dialog_state();
  }

  pub fn effective_open_settings_key(&self) -> Key {
    self
      .keybinding_runtime
      .effective_open_settings
      .unwrap_or(self.user_config.keys.open_settings)
  }

  pub fn effective_save_settings_key(&self) -> Key {
    self.user_config.keys.save_settings
  }

  #[cfg(target_os = "macos")]
  fn allow_plain_comma_open_settings_fallback(&self) -> bool {
    !matches!(
      self.get_current_route().active_block,
      ActiveBlock::Input
        | ActiveBlock::TrackTable
        | ActiveBlock::AlbumList
        | ActiveBlock::Artists
        | ActiveBlock::SortMenu
        | ActiveBlock::Settings
        | ActiveBlock::Dialog(_)
    )
  }

  #[cfg(target_os = "macos")]
  pub fn maybe_activate_open_settings_fallback(&mut self, key: Key) -> bool {
    if self.user_config.keys.open_settings != Key::Ctrl(',') {
      return false;
    }

    if key == Key::Ctrl(',') {
      self.terminal_input_caps.ctrl_punct_reliable = CapabilityState::Yes;
      self.keybinding_runtime.effective_open_settings = None;
      self.keybinding_runtime.fallback_reason = None;
      return false;
    }

    if key == Key::Char(',') && self.allow_plain_comma_open_settings_fallback() {
      self.terminal_input_caps.ctrl_punct_reliable = CapabilityState::No;
      self.keybinding_runtime.effective_open_settings = Some(Key::Alt(','));
      self.keybinding_runtime.fallback_reason = Some(KeyFallbackReason::CtrlCommaNotReported);

      if !self.keybinding_runtime.fallback_notice_shown {
        self.set_status_message(
          "Ctrl+, not detected in this terminal; using Alt+, for this session",
          5,
        );
        self.keybinding_runtime.fallback_notice_shown = true;
      }

      if !self.keybinding_runtime.persist_prompt_shown {
        self.keybinding_runtime.persist_prompt_shown = true;
        self.pending_keybinding_persist = Some(PendingKeybindingPersist {
          open_settings_key: Key::Alt(','),
        });
        self.confirm = false;
      }

      return true;
    }

    false
  }

  #[cfg(not(target_os = "macos"))]
  pub fn maybe_activate_open_settings_fallback(&mut self, _key: Key) -> bool {
    false
  }

  pub fn persist_open_settings_fallback(&mut self) {
    let Some(persist) = self.pending_keybinding_persist else {
      return;
    };

    self.user_config.keys.open_settings = persist.open_settings_key;
    if let Err(e) = self.user_config.save_config() {
      self.handle_error(anyhow!("Failed to save keybinding fallback: {}", e));
      return;
    }

    self.keybinding_runtime.effective_open_settings = None;
    self.keybinding_runtime.fallback_reason = None;
    self.set_status_message(
      format!(
        "Saved open settings shortcut as {}",
        persist.open_settings_key
      ),
      4,
    );
  }

  pub fn set_status_message(&mut self, message: impl Into<String>, ttl_secs: u64) {
    self.status_message = Some(message.into());
    self.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl_secs));
  }

  pub fn playlist_is_editable(&self, playlist: &SimplifiedPlaylist) -> bool {
    let Some(user) = &self.user else {
      return false;
    };

    playlist.owner.id.id() == user.id.id() || playlist.collaborative
  }

  pub fn editable_playlists(&self) -> Vec<&SimplifiedPlaylist> {
    self
      .all_playlists
      .iter()
      .filter(|playlist| self.playlist_is_editable(playlist))
      .collect()
  }

  pub fn begin_add_track_to_playlist_flow(
    &mut self,
    track_id: Option<TrackId<'static>>,
    track_name: String,
  ) {
    let Some(track_id) = track_id else {
      self.set_status_message("Track cannot be added to playlist".to_string(), 4);
      return;
    };

    let mut requested_data = false;
    if self.user.is_none() {
      self.dispatch(IoEvent::GetUser);
      requested_data = true;
    }
    if self.playlists.is_none() {
      self.dispatch(IoEvent::GetPlaylists);
      requested_data = true;
    }
    if requested_data {
      self.set_status_message("Playlist destinations loading, try again".to_string(), 4);
      return;
    }

    if self.editable_playlists().is_empty() {
      self.set_status_message("No editable playlists available".to_string(), 4);
      return;
    }

    self.clear_dialog_state();
    self.pending_playlist_track_add = Some(PendingPlaylistTrackAdd {
      track_id,
      track_name,
    });
    self.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker),
    );
  }

  pub fn is_playlist_item_visible_in_current_folder(&self, item: &PlaylistFolderItem) -> bool {
    match item {
      PlaylistFolderItem::Folder(f) => f.current_id == self.current_playlist_folder_id,
      PlaylistFolderItem::Playlist { current_id, .. } => {
        *current_id == self.current_playlist_folder_id
      }
    }
  }

  /// Get the number of items visible in the current folder level.
  pub fn get_playlist_display_count(&self) -> usize {
    self
      .playlist_folder_items
      .iter()
      .filter(|item| self.is_playlist_item_visible_in_current_folder(item))
      .count()
  }

  /// Get a visible item by display index in the current folder.
  pub fn get_playlist_display_item_at(&self, display_index: usize) -> Option<&PlaylistFolderItem> {
    self
      .playlist_folder_items
      .iter()
      .filter(|item| self.is_playlist_item_visible_in_current_folder(item))
      .nth(display_index)
  }

  /// Get visible playlist items in the current folder (used by UI rendering).
  pub fn get_playlist_display_items(&self) -> Vec<&PlaylistFolderItem> {
    self
      .playlist_folder_items
      .iter()
      .filter(|item| self.is_playlist_item_visible_in_current_folder(item))
      .collect()
  }

  /// Get the SimplifiedPlaylist for a PlaylistFolderItem::Playlist variant
  #[allow(dead_code)]
  pub fn get_playlist_for_item(&self, item: &PlaylistFolderItem) -> Option<&SimplifiedPlaylist> {
    match item {
      PlaylistFolderItem::Playlist { index, .. } => self.all_playlists.get(*index),
      PlaylistFolderItem::Folder(_) => None,
    }
  }

  /// Get the currently selected playlist id in the visible playlist list.
  #[allow(dead_code)]
  pub fn get_selected_playlist_id(&self) -> Option<String> {
    let selected_index = self.selected_playlist_index?;
    if let Some(PlaylistFolderItem::Playlist { index, .. }) =
      self.get_playlist_display_item_at(selected_index)
    {
      return self
        .all_playlists
        .get(*index)
        .map(|p| p.id.id().to_string());
    }

    self
      .playlists
      .as_ref()
      .and_then(|playlists| playlists.items.get(selected_index))
      .map(|playlist| playlist.id.id().to_string())
  }

  fn apply_seek(&mut self, seek_ms: u32) {
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
        _ => return,
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
    // Increment global animation tick (wraps after ~9.4 quintillion ticks, effectively never)
    self.animation_tick = self.animation_tick.wrapping_add(1);

    // Periodic party sync: host broadcasts state every ~2 seconds (~125 ticks at 16ms)
    // Keep this before early-return paths so sync still happens during native-streaming fast paths.
    if self.party_status == PartyStatus::Hosting && self.animation_tick.is_multiple_of(125) {
      self.dispatch(IoEvent::SyncPlayback);
    }

    if let Some(expires_at) = self.status_message_expires_at {
      if Instant::now() >= expires_at {
        self.status_message = None;
        self.status_message_expires_at = None;
      }
    }

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

      // Skip position updates if we recently seeked (let UI show our target position)
      let recently_seeked = self
        .last_api_seek
        .is_some_and(|t| t.elapsed().as_millis() < SEEK_POSITION_IGNORE_MS);

      if recently_seeked {
        return; // Don't overwrite our seek target
      }

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
          _ => return,
        };

        self.song_progress_ms = (self.song_progress_ms + tick_rate_ms).min(duration_ms);
      }
      // When paused, keep song_progress_ms unchanged
    }
  }

  pub fn seek_forwards(&mut self) {
    info!(
      "seeking forwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
        _ => return,
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

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() && self.streaming_player.is_some() {
        // Always update UI immediately
        self.song_progress_ms = new_progress as u128;
        self.seek_ms = None;

        // Throttle actual seeks to avoid overwhelming librespot (max ~20/sec)
        const SEEK_THROTTLE_MS: u128 = 50;
        let should_seek_now = self
          .last_native_seek
          .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

        if should_seek_now {
          self.execute_native_seek(new_progress);
        } else {
          // Queue the seek - will be flushed by tick loop or next seek
          self.pending_native_seek = Some(new_progress);
        }
        return;
      }

      // Fallback: API-based seek for external devices (with throttling)
      self.queue_api_seek(new_progress);
    }
  }

  pub fn seek_backwards(&mut self) {
    info!(
      "seeking backwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    let old_progress = match self.seek_ms {
      Some(seek_ms) => seek_ms,
      None => self.song_progress_ms,
    };
    let new_progress =
      (old_progress as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
    self.seek_ms = Some(new_progress as u128);

    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() && self.streaming_player.is_some() {
      // Always update UI immediately
      self.song_progress_ms = new_progress as u128;
      self.seek_ms = None;

      // Throttle actual seeks to avoid overwhelming librespot (max ~20/sec)
      const SEEK_THROTTLE_MS: u128 = 50;
      let should_seek_now = self
        .last_native_seek
        .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

      if should_seek_now {
        self.execute_native_seek(new_progress);
      } else {
        // Queue the seek - will be flushed by tick loop or next seek
        self.pending_native_seek = Some(new_progress);
      }
      return;
    }

    // Fallback: API-based seek for external devices (with throttling)
    self.queue_api_seek(new_progress);
  }

  /// Queue an API-based seek with throttling (for external device control)
  fn queue_api_seek(&mut self, position_ms: u32) {
    // Always update UI immediately
    self.song_progress_ms = position_ms as u128;
    self.seek_ms = None;

    // Start the ignore window immediately when the user requests a seek
    // This prevents position updates from overwriting our target while waiting
    let now = Instant::now();

    // Mark poll data as stale so resync won't happen after ignore window
    self.instant_since_last_current_playback_poll = now;

    // Throttle API calls (max ~5/sec to respect rate limits)
    const API_SEEK_THROTTLE_MS: u128 = 200;
    let should_seek_now = self
      .last_api_seek
      .is_none_or(|t| t.elapsed().as_millis() >= API_SEEK_THROTTLE_MS);

    // Update last_api_seek for BOTH the ignore window AND throttling
    // This ensures the ignore window starts immediately on any seek request
    self.last_api_seek = Some(now);

    if should_seek_now {
      self.execute_api_seek(position_ms);
    } else {
      // Queue the seek - will be flushed by tick loop
      self.pending_api_seek = Some(position_ms);
    }
  }

  /// Execute an API-based seek
  fn execute_api_seek(&mut self, position_ms: u32) {
    self.pending_api_seek = None;
    self.apply_seek(position_ms);
  }

  /// Flush any pending API seek (called from tick loop)
  pub fn flush_pending_api_seek(&mut self) {
    if let Some(position) = self.pending_api_seek {
      const API_SEEK_THROTTLE_MS: u128 = 200;
      let should_flush = self
        .last_api_seek
        .is_none_or(|t| t.elapsed().as_millis() >= API_SEEK_THROTTLE_MS);

      if should_flush {
        self.execute_api_seek(position);
      }
    }
  }

  /// Execute a native seek and update tracking state
  #[cfg(feature = "streaming")]
  fn execute_native_seek(&mut self, position_ms: u32) {
    if let Some(ref player) = self.streaming_player {
      player.seek(position_ms);
      self.last_native_seek = Some(Instant::now());
      self.pending_native_seek = None;

      // Notify MPRIS clients that position jumped
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      if let Some(ref mpris) = self.mpris_manager {
        mpris.emit_seeked(position_ms as u64);
      }
    }
  }

  /// Flush any pending native seek (called from tick loop)
  #[cfg(feature = "streaming")]
  pub fn flush_pending_native_seek(&mut self) {
    if let Some(position) = self.pending_native_seek {
      // Only flush if enough time has passed since last seek
      const SEEK_THROTTLE_MS: u128 = 50;
      let should_flush = self
        .last_native_seek
        .is_none_or(|t| t.elapsed().as_millis() >= SEEK_THROTTLE_MS);

      if should_flush {
        self.execute_native_seek(position);
      }
    }
  }

  /// Picks up pending volume changes from the tick loop and sends them to Spotify.
  ///
  /// Skips dispatching if the previous request is still in flight, or if we
  /// already sent this exact value and are just waiting for the API to confirm.
  ///
  /// We intentionally don't clear `pending_volume` here — it sticks around until
  /// `get_current_playback` sees the matching value come back from the API.
  pub fn flush_pending_volume(&mut self) {
    if self.is_volume_change_in_flight {
      return; // previous request still processing
    }
    if let Some(volume) = self.pending_volume {
      if self.last_dispatched_volume == Some(volume) {
        return; // already dispatched this value, waiting for API to confirm
      }
      self.is_volume_change_in_flight = true;
      self.last_dispatched_volume = Some(volume);
      self.dispatch(IoEvent::ChangeVolume(volume));
    }
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

  /// Returns the volume the UI should show and volume-up/down should use as a base.
  ///
  /// If the user just pressed a volume key, we show their input (not what the API
  /// says) because Spotify can be slow to reflect the change. Without this, you'd
  /// see the percentage jump back to the old value for a split second before
  /// correcting — especially noticeable when spamming volume up/down.
  pub fn desired_volume(&self) -> u32 {
    if let Some(pending) = self.pending_volume {
      return pending as u32;
    }
    self
      .current_playback_context
      .as_ref()
      .and_then(|c| c.device.volume_percent)
      .unwrap_or(0)
  }

  /// Bump volume up. Uses `desired_volume()` as the base so rapid presses
  /// don't accidentally calculate from a stale API value.
  pub fn increase_volume(&mut self) {
    let current_volume = self.desired_volume() as u8;
    let next_volume = min(
      current_volume + self.user_config.behavior.volume_increment,
      100,
    );

    if next_volume != current_volume {
      info!("increasing volume: {} -> {}", current_volume, next_volume);
      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.set_volume(next_volume);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.device.volume_percent = Some(next_volume.into());
          }
          self.user_config.behavior.volume_percent = next_volume;
          let _ = self.user_config.save_config();
          self.pending_volume = Some(next_volume);
          return;
        }
      }

      // Fallback to API-based volume control for external devices
      // Coalesce: only dispatch if no request is already in flight
      self.pending_volume = Some(next_volume);
      if !self.is_volume_change_in_flight {
        self.is_volume_change_in_flight = true;
        self.dispatch(IoEvent::ChangeVolume(next_volume));
      }
    }
  }

  /// Bump volume down. Uses `desired_volume()` as the base so rapid presses
  /// don't accidentally calculate from a stale API value.
  pub fn decrease_volume(&mut self) {
    let current_volume = self.desired_volume() as i8;
    let next_volume = max(
      current_volume - self.user_config.behavior.volume_increment as i8,
      0,
    );

    if next_volume != current_volume {
      let next_volume_u8 = next_volume as u8;
      info!(
        "decreasing volume: {} -> {}",
        current_volume, next_volume_u8
      );

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.set_volume(next_volume_u8);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.device.volume_percent = Some(next_volume_u8.into());
          }
          self.user_config.behavior.volume_percent = next_volume_u8;
          let _ = self.user_config.save_config();
          self.pending_volume = Some(next_volume_u8);
          return;
        }
      }

      // Fallback to API-based volume control for external devices
      // Coalesce: only dispatch if no request is already in flight
      self.pending_volume = Some(next_volume_u8);
      if !self.is_volume_change_in_flight {
        self.is_volume_change_in_flight = true;
        self.dispatch(IoEvent::ChangeVolume(next_volume_u8));
      }
    }
  }

  pub fn handle_error(&mut self, e: anyhow::Error) {
    info!("error occurred: {}", e);
    self.push_navigation_stack(RouteId::Error, ActiveBlock::Error);
    self.api_error = e.to_string();
  }

  /// Check if native streaming is the active playback device
  /// Returns true only if the player is connected AND it's the currently active device
  #[cfg(feature = "streaming")]
  fn is_native_streaming_active_for_playback(&self) -> bool {
    // Check if player exists and is connected
    let player_connected = self
      .streaming_player
      .as_ref()
      .is_some_and(|p| p.is_connected());

    if !player_connected {
      return false;
    }

    // Get native device name from player
    let native_device_name = self
      .streaming_player
      .as_ref()
      .map(|p| p.device_name().to_lowercase());

    // If no context yet (e.g., at startup), use the app state flag which is
    // set when the native streaming device is activated/selected.
    let Some(ref ctx) = self.current_playback_context else {
      return self.is_streaming_active;
    };

    // First, check if the current playback device matches the native streaming device ID
    if let (Some(current_id), Some(native_id)) =
      (ctx.device.id.as_ref(), self.native_device_id.as_ref())
    {
      if current_id == native_id {
        return true;
      }
    }

    // Fallback: strict name match (case-insensitive)
    if let Some(native_name) = native_device_name.as_ref() {
      let current_device_name = ctx.device.name.to_lowercase();
      if current_device_name == native_name.as_str() {
        return true;
      }
    }

    // No match - not the active device
    false
  }

  pub fn toggle_playback(&mut self) {
    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      if let Some(ref player) = self.streaming_player {
        let is_playing = self
          .native_is_playing
          .or_else(|| self.current_playback_context.as_ref().map(|c| c.is_playing))
          .unwrap_or(false);
        info!(
          "toggling playback: {}",
          if is_playing { "paused" } else { "playing" }
        );
        if is_playing {
          player.pause();
          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.is_playing = false;
          }
          self.native_is_playing = Some(false);
        } else {
          player.play();
          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.is_playing = true;
          }
          self.native_is_playing = Some(true);
        }
        return;
      }
    }

    // Fallback to API-based playback control for external devices
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
    info!("playing previous track or restarting current track");
    if self.song_progress_ms >= 3_000 {
      // If more than 3 seconds into the song, restart from beginning
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.seek(0);
          self.song_progress_ms = 0;
          self.seek_ms = None;
          return;
        }
      }

      // Fallback for external devices
      self.dispatch(IoEvent::Seek(0));
    } else {
      // If less than 3 seconds in, go to previous track
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          player.activate();
          player.prev();
          // Reset progress immediately for UI feedback
          self.song_progress_ms = 0;
          // librespot can occasionally land in a paused state after a skip.
          // Schedule a short delayed resume to avoid racing the track transition.
          let player = std::sync::Arc::clone(player);
          std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            player.activate();
            player.play();
          });
          return;
        }
      }

      // Fallback for external devices
      self.dispatch(IoEvent::PreviousTrack);
    }
  }

  pub fn force_previous_track(&mut self) {
    info!("force skipping to previous track");
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      if let Some(ref player) = self.streaming_player {
        player.activate();
        // First prev() restarts the current track (if past Spotify's ~3s threshold).
        // After a short delay the second prev() actually skips to the previous track,
        // since the position is now back at 0.
        player.prev();
        self.song_progress_ms = 0;
        let player = std::sync::Arc::clone(player);
        std::thread::spawn(move || {
          std::thread::sleep(std::time::Duration::from_millis(500));
          player.prev();
          std::thread::sleep(std::time::Duration::from_millis(300));
          player.activate();
          player.play();
        });
        return;
      }
    }

    self.song_progress_ms = 0;
    self.dispatch(IoEvent::ForcePreviousTrack);
  }

  pub fn next_track(&mut self) {
    info!("skipping to next track");
    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      if let Some(ref player) = self.streaming_player {
        player.activate();
        player.next();
        // Reset progress immediately for UI feedback
        self.song_progress_ms = 0;
        // librespot can occasionally land in a paused state after a skip.
        // Schedule a short delayed resume to avoid racing the track transition.
        let player = std::sync::Arc::clone(player);
        std::thread::spawn(move || {
          std::thread::sleep(std::time::Duration::from_millis(300));
          player.activate();
          player.play();
        });
        return;
      }
    }

    // Fallback for external devices
    self.dispatch(IoEvent::NextTrack);
  }

  // The navigation_stack actually only controls the large block to the right of `library` and
  // `playlists`
  pub fn push_navigation_stack(&mut self, next_route_id: RouteId, next_active_block: ActiveBlock) {
    info!("navigating to {:?}", next_route_id);
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
    info!("navigating back");
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
    info!("copying song url to clipboard");
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
        _ => {}
      }
    }
  }

  pub fn copy_album_url(&mut self) {
    info!("copying album url to clipboard");
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
        _ => {}
      }
    }
  }

  pub fn set_saved_tracks_to_table(&mut self, saved_track_page: &Page<SavedTrack>) {
    self.replace_track_table_tracks(
      saved_track_page
        .items
        .iter()
        .map(|item| item.track.clone())
        .collect::<Vec<FullTrack>>(),
    );
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  pub fn set_playlist_tracks_to_table(&mut self, playlist_track_page: &Page<PlaylistItem>) {
    let mut tracks: Vec<FullTrack> = Vec::new();
    let mut track_ids: Vec<TrackId<'static>> = Vec::new();
    let mut positions: Vec<usize> = Vec::new();

    for (idx, item) in playlist_track_page.items.iter().enumerate() {
      if let Some(PlayableItem::Track(full_track)) = item.item.as_ref() {
        tracks.push(full_track.clone());
        if let Some(track_id) = full_track.id.as_ref() {
          track_ids.push(track_id.clone().into_static());
        }
        positions.push(playlist_track_page.offset as usize + idx);
      }
    }

    self.replace_track_table_tracks(tracks);
    self.playlist_track_positions = Some(positions);
    self.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
  }

  pub fn reset_saved_tracks_view(&mut self) {
    self.saved_tracks_prefetch_generation = self.saved_tracks_prefetch_generation.wrapping_add(1);
    self.library.saved_tracks.clear();
    self.pending_track_table_selection = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks.clear();
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  pub fn reset_playlist_tracks_view(
    &mut self,
    playlist_id: PlaylistId<'static>,
    context: TrackTableContext,
  ) {
    self.playlist_tracks_prefetch_generation =
      self.playlist_tracks_prefetch_generation.wrapping_add(1);
    self.playlist_track_table_id = Some(playlist_id);
    self.playlist_track_pages.clear();
    self.playlist_tracks = None;
    self.playlist_offset = 0;
    self.pending_track_table_selection = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks.clear();
    self.track_table.context = Some(context);
    self.playlist_track_positions = None;
  }

  pub fn replace_track_table_tracks(&mut self, tracks: Vec<FullTrack>) {
    self.playlist_track_positions = None;

    let track_count = tracks.len();
    if track_count > 0 {
      if let Some(pending) = self.pending_track_table_selection.take() {
        self.track_table.selected_index = match pending {
          PendingTrackSelection::First => 0,
          PendingTrackSelection::Last => track_count.saturating_sub(1),
        };
      } else {
        let max_index = track_count.saturating_sub(1);
        if self.track_table.selected_index > max_index {
          self.track_table.selected_index = max_index;
        }
      }
    } else {
      self.track_table.selected_index = 0;
    }

    self.track_table.tracks = tracks;
  }

  pub fn is_playlist_track_table_context(&self) -> bool {
    matches!(
      self.track_table.context,
      Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch)
    )
  }

  pub fn current_playlist_track_table_id(&self) -> Option<PlaylistId<'static>> {
    self
      .is_playlist_track_table_context()
      .then_some(self.playlist_track_table_id.clone())
      .flatten()
  }

  pub fn current_playlist_track_total(&self) -> Option<u32> {
    self.current_playlist_track_table_id()?;
    self
      .playlist_tracks
      .as_ref()
      .map(|playlist_tracks| playlist_tracks.total)
  }

  pub fn current_playlist_track_page(&self) -> Option<&Page<PlaylistItem>> {
    self.current_playlist_track_table_id()?;
    self.playlist_tracks.as_ref()
  }

  pub fn is_playlist_track_table_active_for(&self, playlist_id: &PlaylistId<'_>) -> bool {
    self
      .current_playlist_track_table_id()
      .as_ref()
      .is_some_and(|current_playlist_id| current_playlist_id.id() == playlist_id.id())
  }

  pub fn is_current_route_playlist_track_table_for(&self, playlist_id: &PlaylistId<'_>) -> bool {
    self.get_current_route().id == RouteId::TrackTable
      && self.is_playlist_track_table_active_for(playlist_id)
  }

  pub fn show_saved_tracks_page_at_index(&mut self, page_index: usize) {
    let Some(saved_tracks_page) = self
      .library
      .saved_tracks
      .get_results(Some(page_index))
      .cloned()
    else {
      return;
    };

    self.library.saved_tracks.index = page_index;
    self.set_saved_tracks_to_table(&saved_tracks_page);
  }

  pub fn show_playlist_tracks_page_at_index(&mut self, page_index: usize) {
    let Some(playlist_tracks_page) = self
      .playlist_track_pages
      .get_results(Some(page_index))
      .cloned()
    else {
      return;
    };

    self.playlist_track_pages.index = page_index;
    self.playlist_offset = playlist_tracks_page.offset;
    self.playlist_tracks = Some(playlist_tracks_page.clone());
    self.set_playlist_tracks_to_table(&playlist_tracks_page);
  }

  pub fn show_playlist_tracks_page_at_offset(&mut self, offset: u32) -> Option<usize> {
    let page_index = self.playlist_track_pages.page_index_for_offset(offset)?;
    self.show_playlist_tracks_page_at_index(page_index);
    Some(page_index)
  }

  pub fn next_missing_saved_tracks_offset(&self, page_index: usize) -> Option<u32> {
    let saved_tracks_page = self.library.saved_tracks.get_results(Some(page_index))?;
    saved_tracks_page.next.as_ref()?;

    let next_offset = saved_tracks_page.offset + saved_tracks_page.limit;
    self
      .library
      .saved_tracks
      .page_index_for_offset(next_offset)
      .is_none()
      .then_some(next_offset)
  }

  pub fn next_missing_playlist_tracks_offset(&self, page_index: usize) -> Option<u32> {
    let playlist_tracks_page = self.playlist_track_pages.get_results(Some(page_index))?;
    playlist_tracks_page.next.as_ref()?;

    let next_offset = playlist_tracks_page.offset + playlist_tracks_page.limit;
    self
      .playlist_track_pages
      .page_index_for_offset(next_offset)
      .is_none()
      .then_some(next_offset)
  }

  pub fn dispatch_saved_tracks_prefetch(&mut self, offset: u32) {
    self.dispatch(IoEvent::PreFetchSavedTracksPage {
      offset,
      generation: self.saved_tracks_prefetch_generation,
    });
  }

  pub fn dispatch_playlist_tracks_prefetch(&mut self, offset: u32) {
    if let Some(playlist_id) = self.current_playlist_track_table_id() {
      self.dispatch(IoEvent::PreFetchPlaylistTracksPage {
        playlist_id,
        offset,
        generation: self.playlist_tracks_prefetch_generation,
      });
    }
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
    let next_index = self.library.saved_tracks.index + 1;
    match self.library.saved_tracks.get_results(Some(next_index)) {
      Some(_) => {
        self.show_saved_tracks_page_at_index(next_index);
        if let Some(offset) = self.next_missing_saved_tracks_offset(next_index) {
          self.dispatch_saved_tracks_prefetch(offset);
        }
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
    if self.library.saved_tracks.index == 0 {
      return;
    }

    let previous_index = self.library.saved_tracks.index - 1;
    self.show_saved_tracks_page_at_index(previous_index);
    if let Some(offset) = self.next_missing_saved_tracks_offset(previous_index) {
      self.dispatch_saved_tracks_prefetch(offset);
    }
  }

  pub fn get_playlist_tracks_next(&mut self) {
    let Some(playlist_tracks) = self.current_playlist_track_page().cloned() else {
      return;
    };
    let Some(playlist_id) = self.current_playlist_track_table_id() else {
      return;
    };
    let Some(next_offset) = playlist_tracks
      .next
      .as_ref()
      .map(|_| playlist_tracks.offset + playlist_tracks.limit)
    else {
      return;
    };

    match self.show_playlist_tracks_page_at_offset(next_offset) {
      Some(page_index) => {
        if let Some(offset) = self.next_missing_playlist_tracks_offset(page_index) {
          self.dispatch_playlist_tracks_prefetch(offset);
        }
      }
      None => {
        self.dispatch(IoEvent::GetPlaylistItems(playlist_id, next_offset));
      }
    }
  }

  pub fn get_playlist_tracks_previous(&mut self) {
    let Some(playlist_tracks) = self.current_playlist_track_page().cloned() else {
      return;
    };
    let Some(playlist_id) = self.current_playlist_track_table_id() else {
      return;
    };
    if playlist_tracks.offset == 0 {
      return;
    }

    let previous_offset = playlist_tracks.offset.saturating_sub(playlist_tracks.limit);
    match self.show_playlist_tracks_page_at_offset(previous_offset) {
      Some(page_index) => {
        if let Some(offset) = self.next_missing_playlist_tracks_offset(page_index) {
          self.dispatch_playlist_tracks_prefetch(offset);
        }
      }
      None => {
        self.dispatch(IoEvent::GetPlaylistItems(playlist_id, previous_offset));
      }
    }
  }

  pub fn apply_sorted_playlist_tracks_if_current(
    &mut self,
    playlist_id: &PlaylistId<'_>,
    tracks: Vec<FullTrack>,
  ) -> bool {
    if !self.is_playlist_track_table_active_for(playlist_id) {
      return false;
    }

    self.replace_track_table_tracks(tracks);
    self.track_table.selected_index = 0;
    true
  }

  pub fn shuffle(&mut self) {
    if let Some(context) = &self.current_playback_context.clone() {
      let new_shuffle_state = !context.shuffle_state;
      info!("toggling shuffle: {}", new_shuffle_state);

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          // Try to set shuffle on the native player
          let _ = player.set_shuffle(new_shuffle_state);

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.shuffle_state = new_shuffle_state;
          }
          self.user_config.behavior.shuffle_enabled = new_shuffle_state;
          let _ = self.user_config.save_config();

          // Notify MPRIS clients of the change
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_shuffle(new_shuffle_state);
          }
          return;
        }
      }

      // Fallback to API-based shuffle for external devices
      self.dispatch(IoEvent::Shuffle(new_shuffle_state));
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
    info!("removing album from saved albums");
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
    info!("adding album to saved albums");
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
    info!("unfollowing artist");
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
    info!("following artist");
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
    info!("following playlist");
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
    info!("unfollowing playlist");
    if let (Some(selected_index), Some(user)) = (self.selected_playlist_index, &self.user) {
      if let Some(PlaylistFolderItem::Playlist { index, .. }) =
        self.get_playlist_display_item_at(selected_index)
      {
        if let Some(playlist) = self.all_playlists.get(*index) {
          let selected_id = playlist.id.clone();
          let user_id = user.id.clone();
          self.dispatch(IoEvent::UserUnfollowPlaylist(
            user_id.into_static(),
            selected_id.into_static(),
          ));
        }
      }
    }
  }

  pub fn user_unfollow_playlist_search_result(&mut self) {
    info!("unfollowing playlist from search results");
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
    info!("following show");
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
    info!("unfollowing show");
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
    info!("entering audio analysis view");
    if self.get_current_route().id != RouteId::Analysis {
      // Enter visualization mode
      self.push_navigation_stack(RouteId::Analysis, ActiveBlock::Analysis);
    }
    // Spectrum data will be updated by the audio capture system on each tick
  }

  pub fn repeat(&mut self) {
    if let Some(context) = &self.current_playback_context.clone() {
      let current_repeat_state = context.repeat_state;
      info!("toggling repeat mode: {:?}", current_repeat_state);

      // Use native streaming player for instant control (bypasses event channel latency)
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active_for_playback() {
        if let Some(ref player) = self.streaming_player {
          use rspotify::model::enums::RepeatState;

          // Try to set repeat on the native player (pass current state, not next)
          let _ = player.set_repeat(current_repeat_state);

          // Calculate next state for UI update
          let next_repeat_state = match current_repeat_state {
            RepeatState::Off => RepeatState::Context,
            RepeatState::Context => RepeatState::Track,
            RepeatState::Track => RepeatState::Off,
          };

          // Update UI state immediately
          if let Some(ctx) = &mut self.current_playback_context {
            ctx.repeat_state = next_repeat_state;
          }

          // Notify MPRIS clients of the change
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            use crate::mpris::LoopStatusEvent;
            let loop_status = match next_repeat_state {
              RepeatState::Off => LoopStatusEvent::None,
              RepeatState::Context => LoopStatusEvent::Playlist,
              RepeatState::Track => LoopStatusEvent::Track,
            };
            mpris.set_loop_status(loop_status);
          }
          return;
        }
      }

      // Fallback to API-based repeat for external devices
      self.dispatch(IoEvent::Repeat(current_repeat_state));
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

  #[allow(deprecated)]
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
          id: "behavior.disable_mouse_inputs".to_string(),
          name: "Disable Mouse Inputs".to_string(),
          description: "Disable mouse inputs for keyboard-only navigation".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.disable_mouse_inputs),
        },
        SettingItem {
          id: "behavior.set_window_title".to_string(),
          name: "Set Window Title".to_string(),
          description: "Update terminal window title with track info".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.set_window_title),
        },
        SettingItem {
          id: "behavior.enable_discord_rpc".to_string(),
          name: "Discord Rich Presence".to_string(),
          description: "Show your current track in Discord".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_discord_rpc),
        },
        SettingItem {
          id: "behavior.stop_after_current_track".to_string(),
          name: "Stop After Current Track".to_string(),
          description: "Pause playback when the current track finishes".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.stop_after_current_track),
        },
        SettingItem {
          id: "behavior.startup_behavior".to_string(),
          name: "Startup Behavior".to_string(),
          description: "Playback state when spotatui starts: continue, play, or pause".to_string(),
          value: SettingValue::Cycle(
            self
              .user_config
              .behavior
              .startup_behavior
              .name()
              .to_string(),
            crate::core::user_config::StartupBehavior::options(),
          ),
        },
        SettingItem {
          id: "behavior.enable_announcements".to_string(),
          name: "Remote Announcements".to_string(),
          description: "Show one-time announcements from remote JSON feed".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_announcements),
        },
        SettingItem {
          id: "behavior.disable_auto_update".to_string(),
          name: "Disable Auto-Update".to_string(),
          description: "Skip the automatic update check on startup. Use the 'spotatui update' command to update manually.".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.disable_auto_update),
        },
        SettingItem {
          id: "behavior.auto_update_delay".to_string(),
          name: "Auto-Update Delay".to_string(),
          description: "How long to wait before installing an available update. Use '0' for immediate, or e.g. '10m', '2h', '7d'. Only applies when auto-update is enabled.".to_string(),
          value: SettingValue::String(self.user_config.behavior.auto_update_delay.clone()),
        },
        SettingItem {
          id: "behavior.announcement_feed_url".to_string(),
          name: "Announcements Feed URL".to_string(),
          description: "Remote JSON feed URL (HTTPS)".to_string(),
          value: SettingValue::String(
            self
              .user_config
              .behavior
              .announcement_feed_url
              .clone()
              .unwrap_or_default(),
          ),
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
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "behavior.draw_cover_art".to_string(),
          name: "Draw Cover Art".to_string(),
          description: "Enable rendering song/episode cover art".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.draw_cover_art),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "behavior.draw_cover_art_forced".to_string(),
          name: "Force Draw Cover Art".to_string(),
          description: "Force rendering of cover art despite terminal support".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.draw_cover_art_forced),
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
          id: "keys.force_previous_track".to_string(),
          name: "Force Previous Track".to_string(),
          description: "Always skip to the previous track (ignoring playback position)".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.force_previous_track)),
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
        SettingItem {
          id: "keys.open_settings".to_string(),
          name: "Open Settings".to_string(),
          description: "Open settings menu".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.open_settings)),
        },
        SettingItem {
          id: "keys.save_settings".to_string(),
          name: "Save Settings".to_string(),
          description: "Save settings to file".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.save_settings)),
        },
        SettingItem {
          id: "keys.jump_to_album".to_string(),
          name: "Jump to Album".to_string(),
          description: "Jump to currently playing album".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.jump_to_album)),
        },
        SettingItem {
          id: "keys.jump_to_artist_album".to_string(),
          name: "Jump to Artist".to_string(),
          description: "Jump to artist's albums".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.jump_to_artist_album)),
        },
        SettingItem {
          id: "keys.jump_to_context".to_string(),
          name: "Jump to Context".to_string(),
          description: "Jump to current playback context".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.jump_to_context)),
        },
        SettingItem {
          id: "keys.manage_devices".to_string(),
          name: "Manage Devices".to_string(),
          description: "Open device selection".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.manage_devices)),
        },
        SettingItem {
          id: "keys.decrease_volume".to_string(),
          name: "Decrease Volume".to_string(),
          description: "Decrease playback volume".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.decrease_volume)),
        },
        SettingItem {
          id: "keys.increase_volume".to_string(),
          name: "Increase Volume".to_string(),
          description: "Increase playback volume".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.increase_volume)),
        },
        SettingItem {
          id: "keys.add_item_to_queue".to_string(),
          name: "Add to Queue".to_string(),
          description: "Add selected item to queue".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.add_item_to_queue)),
        },
        SettingItem {
          id: "keys.show_queue".to_string(),
          name: "Show Queue".to_string(),
          description: "Show playback queue".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.show_queue)),
        },
        SettingItem {
          id: "keys.like_track".to_string(),
          name: "Like Track".to_string(),
          description: "Toggle saved state for the currently playing track or episode"
            .to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.like_track)),
        },
        SettingItem {
          id: "keys.copy_song_url".to_string(),
          name: "Copy Song URL".to_string(),
          description: "Copy current song URL to clipboard".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.copy_song_url)),
        },
        SettingItem {
          id: "keys.copy_album_url".to_string(),
          name: "Copy Album URL".to_string(),
          description: "Copy current album URL to clipboard".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.copy_album_url)),
        },
        SettingItem {
          id: "keys.audio_analysis".to_string(),
          name: "Audio Analysis".to_string(),
          description: "Open audio analysis view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.audio_analysis)),
        },
        SettingItem {
          id: "keys.lyrics_view".to_string(),
          name: "Lyrics View".to_string(),
          description: "Open lyrics view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.lyrics_view)),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "keys.cover_art_view".to_string(),
          name: "Cover Art View".to_string(),
          description: "Open full-screen cover art view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.cover_art_view)),
        },
      ],
      SettingsCategory::Theme => {
        vec![
          SettingItem {
            id: "theme.preset".to_string(),
            name: "Theme Preset".to_string(),
            description: "Choose a preset theme or customize below".to_string(),
            value: SettingValue::Preset(self.user_config.current_preset.name().to_string()),
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
            id: "theme.error_border".to_string(),
            name: "Error Border Color".to_string(),
            description: "Border color for error messages".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.error_border)),
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
            id: "theme.playbar_progress_text".to_string(),
            name: "Playbar Progress Text".to_string(),
            description: "Color for playbar progress text".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.playbar_progress_text)),
          },
          SettingItem {
            id: "theme.playbar_text".to_string(),
            name: "Playbar Text".to_string(),
            description: "Color for playbar text".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.playbar_text)),
          },
          SettingItem {
            id: "theme.highlighted_lyrics".to_string(),
            name: "Lyrics Highlight".to_string(),
            description: "Color for current lyrics line".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.highlighted_lyrics)),
          },
          SettingItem {
            id: "theme.background".to_string(),
            name: "Background".to_string(),
            description: "Color for the background".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.background)),
          },
          SettingItem {
            id: "theme.header".to_string(),
            name: "Header".to_string(),
            description: "Color for the header".to_string(),
            value: SettingValue::Color(color_to_string(self.user_config.theme.header)),
          },
        ]
      }
    };
    self.settings_selected_index = 0;
    self.settings_saved_items = self.settings_items.clone();
    self.settings_unsaved_prompt_visible = false;
    self.settings_unsaved_prompt_save_selected = true;
  }

  // Apply changes from settings_items back to user_config
  pub fn apply_settings_changes(&mut self) {
    use crate::core::user_config::{parse_theme_item, ThemePreset};

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
        "behavior.disable_mouse_inputs" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.disable_mouse_inputs = *v;
          }
        }
        "behavior.set_window_title" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.set_window_title = *v;
          }
        }
        "behavior.enable_discord_rpc" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.enable_discord_rpc = *v;
          }
        }
        "behavior.stop_after_current_track" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.stop_after_current_track = *v;
          }
        }
        "behavior.startup_behavior" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.startup_behavior =
              crate::core::user_config::StartupBehavior::from_name(v);
          }
        }
        "behavior.enable_announcements" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.enable_announcements = *v;
          }
        }
        "behavior.disable_auto_update" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.disable_auto_update = *v;
          }
        }
        "behavior.auto_update_delay" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.auto_update_delay = v.clone();
          }
        }
        "behavior.announcement_feed_url" => {
          if let SettingValue::String(v) = &setting.value {
            let trimmed = v.trim();
            self.user_config.behavior.announcement_feed_url = if trimmed.is_empty() {
              None
            } else {
              Some(trimmed.to_string())
            };
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
        #[cfg(feature = "cover-art")]
        "behavior.draw_cover_art" => {
          if let SettingValue::Bool(v) = setting.value {
            self.user_config.behavior.draw_cover_art = v;
          }
        }
        #[cfg(feature = "cover-art")]
        "behavior.draw_cover_art_forced" => {
          if let SettingValue::Bool(v) = setting.value {
            self.user_config.behavior.draw_cover_art_forced = v;
          }
        }
        // Keybindings
        "keys.back" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.back = key;
            }
          }
        }
        "keys.next_page" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.next_page = key;
            }
          }
        }
        "keys.previous_page" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.previous_page = key;
            }
          }
        }
        "keys.toggle_playback" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.toggle_playback = key;
            }
          }
        }
        "keys.seek_backwards" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.seek_backwards = key;
            }
          }
        }
        "keys.seek_forwards" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.seek_forwards = key;
            }
          }
        }
        "keys.next_track" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.next_track = key;
            }
          }
        }
        "keys.previous_track" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.previous_track = key;
            }
          }
        }
        "keys.force_previous_track" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.force_previous_track = key;
            }
          }
        }
        "keys.shuffle" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.shuffle = key;
            }
          }
        }
        "keys.repeat" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.repeat = key;
            }
          }
        }
        "keys.search" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.search = key;
            }
          }
        }
        "keys.help" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.help = key;
            }
          }
        }
        "keys.open_settings" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.open_settings = key;
            }
          }
        }
        "keys.save_settings" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.save_settings = key;
            }
          }
        }
        "keys.jump_to_album" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.jump_to_album = key;
            }
          }
        }
        "keys.jump_to_artist_album" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.jump_to_artist_album = key;
            }
          }
        }
        "keys.jump_to_context" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.jump_to_context = key;
            }
          }
        }
        "keys.manage_devices" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.manage_devices = key;
            }
          }
        }
        "keys.decrease_volume" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.decrease_volume = key;
            }
          }
        }
        "keys.increase_volume" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.increase_volume = key;
            }
          }
        }
        "keys.add_item_to_queue" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.add_item_to_queue = key;
            }
          }
        }
        "keys.show_queue" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.show_queue = key;
            }
          }
        }
        "keys.like_track" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.like_track = key;
            }
          }
        }
        "keys.copy_song_url" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.copy_song_url = key;
            }
          }
        }
        "keys.copy_album_url" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.copy_album_url = key;
            }
          }
        }
        "keys.audio_analysis" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.audio_analysis = key;
            }
          }
        }
        "keys.lyrics_view" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.lyrics_view = key;
            }
          }
        }
        #[cfg(feature = "cover-art")]
        "keys.cover_art_view" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.cover_art_view = key;
            }
          }
        }
        // Decides whether the per-color changes following will apply.
        // A named preset takes priority; the user's custom_theme is preserved
        // so they can return to it later by selecting Custom.
        "theme.preset" => {
          if let SettingValue::Preset(name) = &setting.value {
            let preset = ThemePreset::from_name(name);
            self.user_config.current_preset = preset;
            if preset != ThemePreset::Custom {
              self.user_config.theme = preset.to_theme();
            }
          }
        }
        // Individual theme color overrides only apply when on Custom; they
        // update both the active theme and the persisted custom_theme.
        "theme.active" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.active = c;
              self.user_config.custom_theme.active = c;
            }
          }
        }
        "theme.banner" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.banner = c;
              self.user_config.custom_theme.banner = c;
            }
          }
        }
        "theme.hint" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.hint = c;
              self.user_config.custom_theme.hint = c;
            }
          }
        }
        "theme.hovered" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.hovered = c;
              self.user_config.custom_theme.hovered = c;
            }
          }
        }
        "theme.selected" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.selected = c;
              self.user_config.custom_theme.selected = c;
            }
          }
        }
        "theme.inactive" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.inactive = c;
              self.user_config.custom_theme.inactive = c;
            }
          }
        }
        "theme.text" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.text = c;
              self.user_config.custom_theme.text = c;
            }
          }
        }
        "theme.error_text" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.error_text = c;
              self.user_config.custom_theme.error_text = c;
            }
          }
        }
        "theme.error_border" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.error_border = c;
              self.user_config.custom_theme.error_border = c;
            }
          }
        }
        "theme.playbar_background" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.playbar_background = c;
              self.user_config.custom_theme.playbar_background = c;
            }
          }
        }
        "theme.playbar_progress" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.playbar_progress = c;
              self.user_config.custom_theme.playbar_progress = c;
            }
          }
        }
        "theme.playbar_progress_text" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.playbar_progress_text = c;
              self.user_config.custom_theme.playbar_progress_text = c;
            }
          }
        }
        "theme.playbar_text" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.playbar_text = c;
              self.user_config.custom_theme.playbar_text = c;
            }
          }
        }
        "theme.highlighted_lyrics" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.highlighted_lyrics = c;
              self.user_config.custom_theme.highlighted_lyrics = c;
            }
          }
        }
        "theme.background" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.background = c;
              self.user_config.custom_theme.background = c;
            }
          }
        }
        "theme.header" if self.user_config.current_preset == ThemePreset::Custom => {
          if let SettingValue::Color(v) = &setting.value {
            if let Ok(c) = parse_theme_item(v) {
              self.user_config.theme.header = c;
              self.user_config.custom_theme.header = c;
            }
          }
        }
        _ => {}
      }
    }
  }

  /// Updates the colour RGB entries when switching through the presets in themes
  pub fn sync_theme_color_settings(&mut self, theme: &crate::core::user_config::Theme) {
    let mappings: [(&str, ratatui::style::Color); 16] = [
      ("theme.active", theme.active),
      ("theme.banner", theme.banner),
      ("theme.hint", theme.hint),
      ("theme.hovered", theme.hovered),
      ("theme.selected", theme.selected),
      ("theme.inactive", theme.inactive),
      ("theme.text", theme.text),
      ("theme.error_text", theme.error_text),
      ("theme.error_border", theme.error_border),
      ("theme.playbar_background", theme.playbar_background),
      ("theme.playbar_progress", theme.playbar_progress),
      ("theme.playbar_progress_text", theme.playbar_progress_text),
      ("theme.playbar_text", theme.playbar_text),
      ("theme.highlighted_lyrics", theme.highlighted_lyrics),
      ("theme.background", theme.background),
      ("theme.header", theme.header),
    ];
    for setting in &mut self.settings_items {
      if let Some((_, color)) = mappings.iter().find(|(id, _)| *id == setting.id) {
        setting.value = SettingValue::Color(color_to_string(*color));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::{private_user, simplified_playlist};
  use chrono::{Duration as ChronoDuration, Utc};
  use rspotify::model::{artist::SimplifiedArtist, idtypes::PlaylistId};
  use rspotify::prelude::Id;
  use std::collections::HashMap;
  use std::sync::mpsc::channel;

  #[allow(deprecated)]
  fn full_track(id: &str, name: &str) -> FullTrack {
    FullTrack {
      album: SimplifiedAlbum {
        name: format!("{name} Album"),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: ChronoDuration::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: Some(TrackId::from_id(id).unwrap().into_static()),
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: name.to_string(),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    }
  }

  fn saved_track(id: &str, name: &str) -> SavedTrack {
    SavedTrack {
      added_at: Utc::now(),
      track: full_track(id, name),
    }
  }

  fn saved_tracks_page(offset: u32, total: u32, ids: &[&str], has_next: bool) -> Page<SavedTrack> {
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
      total,
    }
  }

  fn empty_playlist_page(
    offset: u32,
    total: u32,
    limit: u32,
    has_next: bool,
  ) -> Page<PlaylistItem> {
    Page {
      href: "https://example.com/playlists/test/items".to_string(),
      items: vec![],
      limit,
      next: has_next.then(|| "https://example.com/playlists/test/items?next".to_string()),
      offset,
      previous: None,
      total,
    }
  }

  fn playlist_id(id: &str) -> PlaylistId<'static> {
    PlaylistId::from_id(id).unwrap().into_static()
  }

  #[test]
  fn upsert_page_by_offset_preserves_active_index() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_page(
      0,
      4,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    ));

    let inserted_index = pages.upsert_page_by_offset(saved_tracks_page(
      2,
      4,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    ));

    assert_eq!(inserted_index, 1);
    assert_eq!(pages.index, 0);
    assert_eq!(pages.pages.len(), 2);
  }

  #[test]
  fn upsert_page_by_offset_replaces_duplicate_page() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_page(
      0,
      2,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    ));

    let replaced_index = pages.upsert_page_by_offset(saved_tracks_page(
      0,
      2,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    ));

    assert_eq!(replaced_index, 0);
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(
      pages.pages[0].items[0].track.id.as_ref().unwrap().id(),
      "0000000000000000000003"
    );
  }

  #[test]
  fn upsert_page_by_offset_keeps_active_page_when_inserting_before_it() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_page(
      0,
      6,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    ));
    pages.add_pages(saved_tracks_page(
      4,
      6,
      &["0000000000000000000005", "0000000000000000000006"],
      false,
    ));
    pages.index = 1;

    let inserted_index = pages.upsert_page_by_offset(saved_tracks_page(
      2,
      6,
      &["0000000000000000000003", "0000000000000000000004"],
      true,
    ));

    assert_eq!(inserted_index, 1);
    assert_eq!(pages.index, 2);
    assert_eq!(pages.pages[pages.index].offset, 4);
  }

  #[test]
  fn reset_saved_tracks_view_clears_cached_pages_and_bumps_generation() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.saved_tracks_prefetch_generation = 7;
    app.library.saved_tracks.add_pages(saved_tracks_page(
      0,
      2,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    ));
    app.track_table.tracks = vec![
      full_track("0000000000000000000001", "Track 1"),
      full_track("0000000000000000000002", "Track 2"),
    ];
    app.track_table.selected_index = 1;

    app.reset_saved_tracks_view();

    assert_eq!(app.saved_tracks_prefetch_generation, 8);
    assert!(app.library.saved_tracks.pages.is_empty());
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.track_table.selected_index, 0);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
  }

  #[test]
  fn reset_playlist_tracks_view_clears_cached_pages_and_bumps_generation() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    let playlist_id = PlaylistId::from_id("37i9dQZF1DXcBWIGoYBM5M")
      .unwrap()
      .into_static();
    app.playlist_tracks_prefetch_generation = 4;
    app.playlist_track_table_id = Some(playlist_id.clone());
    app
      .playlist_track_pages
      .add_pages(empty_playlist_page(0, 40, 20, true));
    app.playlist_tracks = Some(empty_playlist_page(0, 40, 20, true));
    app.playlist_offset = 20;
    app.track_table.selected_index = 1;
    app.track_table.tracks = vec![
      full_track("0000000000000000000001", "Track 1"),
      full_track("0000000000000000000002", "Track 2"),
    ];

    app.reset_playlist_tracks_view(playlist_id.clone(), TrackTableContext::MyPlaylists);

    assert_eq!(app.playlist_tracks_prefetch_generation, 5);
    assert_eq!(app.playlist_track_table_id, Some(playlist_id));
    assert!(app.playlist_track_pages.pages.is_empty());
    assert!(app.playlist_tracks.is_none());
    assert_eq!(app.playlist_offset, 0);
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.track_table.selected_index, 0);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::MyPlaylists)
    );
  }

  #[test]
  fn playlist_next_requests_adjacent_offset_when_cache_is_sparse() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    let playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");
    let first_page = empty_playlist_page(0, 100, 20, true);
    let last_page = empty_playlist_page(80, 100, 20, false);

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(first_page.clone());
    app.playlist_track_pages.upsert_page_by_offset(last_page);
    app.playlist_tracks = Some(first_page);
    app.playlist_offset = 0;

    app.get_playlist_tracks_next();

    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(id, offset) => {
        assert_eq!(id.id(), playlist_id.id());
        assert_eq!(offset, 20);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn playlist_previous_requests_adjacent_offset_when_cache_is_sparse() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = empty_playlist_page(0, 100, 20, true);
    let last_page = empty_playlist_page(80, 100, 20, false);

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app.playlist_track_pages.upsert_page_by_offset(first_page);
    app
      .playlist_track_pages
      .upsert_page_by_offset(last_page.clone());
    app.playlist_tracks = Some(last_page);
    app.playlist_offset = 80;

    app.get_playlist_tracks_previous();

    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(id, offset) => {
        assert_eq!(id.id(), playlist_id.id());
        assert_eq!(offset, 60);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn playlist_next_uses_cached_adjacent_page_before_fetching() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = empty_playlist_page(0, 60, 20, true);
    let second_page = empty_playlist_page(20, 60, 20, true);

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(first_page.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(second_page.clone());
    app.playlist_tracks = Some(first_page);
    app.playlist_offset = 0;

    app.get_playlist_tracks_next();

    assert_eq!(app.playlist_offset, 20);
    assert_eq!(
      app.playlist_tracks.as_ref().map(|page| page.offset),
      Some(20)
    );
    match rx.recv().unwrap() {
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        assert!(track_ids.is_empty());
      }
      _ => panic!("unexpected event"),
    }
    match rx.recv().unwrap() {
      IoEvent::PreFetchPlaylistTracksPage {
        playlist_id: id,
        offset,
        ..
      } => {
        assert_eq!(id.id(), playlist_id.id());
        assert_eq!(offset, 40);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn apply_sorted_playlist_tracks_if_current_requires_matching_playlist_identity_and_context() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    let sidebar_playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");
    let active_playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let original_track = full_track("0000000000000000000001", "Original");

    app.track_table.tracks = vec![original_track.clone()];
    app.track_table.context = Some(TrackTableContext::PlaylistSearch);
    app.playlist_track_table_id = Some(active_playlist_id.clone());

    assert!(!app.apply_sorted_playlist_tracks_if_current(
      &sidebar_playlist_id,
      vec![full_track("0000000000000000000002", "Wrong Playlist")],
    ));
    assert_eq!(
      app.track_table.tracks[0].id.as_ref().unwrap().id(),
      original_track.id.as_ref().unwrap().id()
    );

    app.track_table.context = Some(TrackTableContext::SavedTracks);
    assert!(!app.apply_sorted_playlist_tracks_if_current(
      &active_playlist_id,
      vec![full_track("0000000000000000000003", "Wrong Context")],
    ));
    assert_eq!(
      app.track_table.tracks[0].id.as_ref().unwrap().id(),
      original_track.id.as_ref().unwrap().id()
    );
  }

  #[test]
  fn editable_playlists_include_owned_and_collaborative_only() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user = Some(private_user("spotatui-owner"));
    app.all_playlists = vec![
      simplified_playlist("37i9dQZF1DXcBWIGoYBM5M", "Owned", "spotatui-owner", false),
      simplified_playlist(
        "37i9dQZF1DX4WYpdgoIcn6",
        "Collaborative",
        "friend-owner",
        true,
      ),
      simplified_playlist("37i9dQZF1DWZqd5JICZI0u", "Followed", "friend-owner", false),
    ];

    let editable_names = app
      .editable_playlists()
      .into_iter()
      .map(|playlist| playlist.name.clone())
      .collect::<Vec<_>>();

    assert_eq!(editable_names, vec!["Owned", "Collaborative"]);
  }

  #[test]
  fn begin_add_track_to_playlist_flow_requires_editable_playlist() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user = Some(private_user("spotatui-owner"));
    app.playlists = Some(Page {
      href: "https://api.spotify.com/v1/me/playlists".to_string(),
      items: vec![],
      limit: 50,
      next: None,
      offset: 0,
      previous: None,
      total: 1,
    });
    app.all_playlists = vec![simplified_playlist(
      "37i9dQZF1DWZqd5JICZI0u",
      "Followed",
      "friend-owner",
      false,
    )];

    app.begin_add_track_to_playlist_flow(
      Some(
        TrackId::from_id("0000000000000000000001")
          .unwrap()
          .into_static(),
      ),
      "Track".to_string(),
    );

    assert_eq!(
      app.status_message.as_deref(),
      Some("No editable playlists available")
    );
    assert!(app.pending_playlist_track_add.is_none());
  }

  #[test]
  fn current_route_playlist_track_table_requires_track_table_route() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    let playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    assert!(app.is_playlist_track_table_active_for(&playlist_id));
    assert!(!app.is_current_route_playlist_track_table_for(&playlist_id));

    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    assert!(app.is_current_route_playlist_track_table_for(&playlist_id));
  }
}
