use crate::core::pagination::{CursorPaged, Paged};
use crate::core::plugin_api::{
  ArtistInfo, EpisodeInfo, PlayableInfo, PlaylistInfo, SavedAlbumInfo, ShowInfo, TrackInfo,
};
use crate::core::sort::{SortContext, SortField, SortOrder, SortState};
use crate::core::source::Source;
use crate::core::user_config::{color_to_string, normalize_tick_rate_milliseconds, UserConfig};
use crate::infra::network::sync::{PartySession, PartyStatus};
use crate::infra::network::IoEvent;
#[cfg(any(feature = "streaming", feature = "audio-decode"))]
use crate::infra::queue::QueueNowPlaying;
use crate::tui::event::Key;
use anyhow::anyhow;
use ratatui::layout::Size;
use rspotify::{
  model::enums::Country,
  model::{
    context::CurrentPlaybackContext, device::DevicePayload, idtypes::PlaylistId, track::FullTrack,
    PlayableItem,
  },
  prelude::*, // Adds Id trait for .id() method
};
use std::cell::Cell;
use std::sync::mpsc::Sender;
#[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
use std::sync::Arc;
use std::{
  cmp::{max, min},
  collections::HashSet,
  time::{Duration, Instant, SystemTime},
};
use unicode_width::UnicodeWidthStr;

use arboard::Clipboard;
#[cfg(feature = "streaming")]
use chrono::Utc;
use log::info;
#[cfg(feature = "streaming")]
use rspotify::model::{
  context::Actions,
  device::Device,
  enums::{CurrentlyPlayingType, RepeatState},
  DeviceType,
};

/// Sidebar library entries. The "Local Files" entry only appears when the
/// `local-files` feature is built in (otherwise there is nothing to browse).
#[cfg(feature = "local-files")]
pub const LIBRARY_OPTIONS: [&str; 8] = [
  "Discover",
  "Recently Played",
  "Friends",
  "Liked Songs",
  "Albums",
  "Artists",
  "Podcasts",
  "Local Files",
];
#[cfg(not(feature = "local-files"))]
pub const LIBRARY_OPTIONS: [&str; 7] = [
  "Discover",
  "Recently Played",
  "Friends",
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

#[cfg(feature = "streaming")]
const FRESH_NATIVE_ACTIVITY_WINDOW: Duration = Duration::from_secs(5);

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

// Offset-keyed page caches are always kept sorted by `Paged::offset`, but the cache
// can be sparse, so visible-page identity is derived from the offset, never raw
// cache adjacency. There is no `DeserializeOwned` bound because `Paged` carries
// already-mapped domain items.
impl<T> ScrollableResultPages<Paged<T>> {
  pub fn page_index_for_offset(&self, offset: u32) -> Option<usize> {
    self
      .pages
      .binary_search_by_key(&offset, |page| page.offset)
      .ok()
  }

  pub fn upsert_page_by_offset(&mut self, new_page: Paged<T>) -> usize {
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

/// Minimal source-agnostic snapshot of the signed-in user, used for playlist
/// ownership checks and market/country resolution. Holds only string fields so
/// no `rspotify::model` type leaks into [`App`] state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UserInfo {
  pub id: String,
  pub display_name: Option<String>,
  /// ISO 3166-1 alpha-2 country code (e.g. `"US"`), when known.
  pub country: Option<String>,
}

#[derive(Default)]
pub struct SpotifyResultAndSelectedIndex<T> {
  pub index: usize,
  pub result: T,
}

#[derive(Clone)]
pub struct Library {
  pub selected_index: usize,
  pub saved_tracks: ScrollableResultPages<Paged<TrackInfo>>,
  pub saved_albums: ScrollableResultPages<Paged<SavedAlbumInfo>>,
  pub saved_shows: ScrollableResultPages<Paged<ShowInfo>>,
  pub saved_artists: ScrollableResultPages<CursorPaged<ArtistInfo>>,
  pub show_episodes: ScrollableResultPages<Paged<EpisodeInfo>>,
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
  /// Confirm deleting a local YouTube playlist (sidebar `D` under the
  /// YouTube source).
  YouTubePlaylistWindow,
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
  MiniPlayer,
  Dialog(DialogContext),

  AnnouncementPrompt,
  ExitPrompt,
  Settings,
  SortMenu,
  Queue,
  Party,
  CreatePlaylistForm,
  Friends,
  LocalBrowser,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputContext {
  #[default]
  GlobalSearch,
  PlaylistTrackSearch,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RouteId {
  Analysis,
  AlbumTracks,
  AlbumList,
  Artist,
  LyricsView,
  CoverArtView,
  MiniPlayer,
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
  Friends,
  LocalBrowser,
}

impl RouteId {
  /// Routes that can be shown at startup with no extra context (no album/artist
  /// id, search query, etc.). These are the only routes `startup_route` may
  /// select.
  pub const STARTUP_OPTIONS: &'static [RouteId] = &[
    RouteId::Home,
    RouteId::RecentlyPlayed,
    RouteId::Podcasts,
    RouteId::Discover,
    RouteId::Artists,
    RouteId::AlbumList,
  ];

  /// Parse a `startup_route` config token. Unknown / non-context-free strings
  /// return `None` (the caller logs a warning and falls back to Home).
  pub fn from_config_str(s: &str) -> Option<RouteId> {
    match s.trim().to_ascii_lowercase().as_str() {
      "home" => Some(RouteId::Home),
      "recently_played" | "recent" => Some(RouteId::RecentlyPlayed),
      "podcasts" => Some(RouteId::Podcasts),
      "discover" => Some(RouteId::Discover),
      "artists" | "library" => Some(RouteId::Artists),
      "album_list" | "albums" => Some(RouteId::AlbumList),
      _ => None,
    }
  }

  /// The config-file token for this route (inverse of `from_config_str`).
  pub fn to_config_str(&self) -> &'static str {
    match self {
      RouteId::Home => "home",
      RouteId::RecentlyPlayed => "recently_played",
      RouteId::Podcasts => "podcasts",
      RouteId::Discover => "discover",
      RouteId::Artists => "artists",
      RouteId::AlbumList => "album_list",
      _ => "home",
    }
  }
}

// ── Friends feature ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct FriendEntry {
  pub id: String,
  pub name: String,
  pub is_online: bool,
  pub now_playing: Option<FriendNowPlaying>,
  /// Total listening time in milliseconds (from spotatui.com)
  #[allow(dead_code)]
  pub listening_ms: u64,
  /// Total number of listens tracked on spotatui.com
  #[allow(dead_code)]
  pub total_listens: u64,
}

#[derive(Clone, Debug)]
pub struct FriendNowPlaying {
  pub title: String,
  pub artists: String,
}

/// A user returned from the username/code search.
#[derive(Clone, Debug)]
pub struct FriendSearchResult {
  pub id: String,
  pub name: String,
  pub is_following: bool,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FriendFilter {
  #[default]
  All,
  Online,
}

/// Which tab is active in the "Add Friend" dialog.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum FriendAddMode {
  #[default]
  Code,
  Search,
}

// ─────────────────────────────────────────────────────────────────────────────

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
  LocalPlaylist,
  SubsonicPlaylist,
  YouTubePlaylist,
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

/// Which panel of the combined Source & Device picker (the `d` screen) has
/// keyboard focus.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SourceFocus {
  Source,
  #[default]
  Devices,
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
  pub albums: Option<crate::core::pagination::Paged<crate::core::plugin_api::AlbumInfo>>,
  pub artists: Option<crate::core::pagination::Paged<crate::core::plugin_api::ArtistInfo>>,
  pub playlists: Option<crate::core::pagination::Paged<crate::core::plugin_api::PlaylistInfo>>,
  pub tracks: Option<crate::core::pagination::Paged<crate::core::plugin_api::TrackInfo>>,
  pub shows: Option<crate::core::pagination::Paged<crate::core::plugin_api::ShowInfo>>,
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
  pub tracks: Vec<TrackInfo>,
  pub selected_index: usize,
  pub context: Option<TrackTableContext>,
}

fn sort_playlist_track_matches(matches: &mut [(FullTrack, usize)], sort_state: SortState) {
  if sort_state.field == SortField::Default {
    return;
  }

  matches.sort_by(|(track_a, position_a), (track_b, position_b)| {
    let order = match sort_state.field {
      SortField::Name => track_a.name.cmp(&track_b.name),
      SortField::Duration => track_a.duration.cmp(&track_b.duration),
      SortField::Artist => {
        let empty_string = String::new();
        let artist_a = track_a
          .artists
          .first()
          .map(|artist| &artist.name)
          .unwrap_or(&empty_string);
        let artist_b = track_b
          .artists
          .first()
          .map(|artist| &artist.name)
          .unwrap_or(&empty_string);
        artist_a.cmp(artist_b)
      }
      SortField::Album => track_a.album.name.cmp(&track_b.album.name),
      SortField::DateAdded => position_a.cmp(position_b),
      SortField::Default => std::cmp::Ordering::Equal,
    };

    if sort_state.order == SortOrder::Descending {
      order.reverse()
    } else {
      order
    }
  });
}

#[derive(Clone)]
pub struct PendingPlaylistTrackAdd {
  /// Track id/URI passed through to `AddTrackToPlaylist`.
  pub track_id: String,
  pub track_name: String,
}

#[derive(Clone)]
pub struct PendingPlaylistTrackRemoval {
  /// Playlist id/URI passed through to `RemoveTrackFromPlaylistAtPosition`.
  pub playlist_id: String,
  pub playlist_name: String,
  /// Track id/URI passed through to `RemoveTrackFromPlaylistAtPosition`.
  pub track_id: String,
  pub track_name: String,
  pub position: usize,
}

#[derive(Clone)]
pub struct SelectedShow {
  pub show: ShowInfo,
}

#[derive(Clone)]
pub struct SelectedFullShow {
  pub show: ShowInfo,
}

#[derive(Clone)]
pub struct SelectedAlbum {
  pub album: crate::core::plugin_api::AlbumInfo,
  pub tracks: crate::core::pagination::Paged<crate::core::plugin_api::TrackInfo>,
  pub selected_index: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct SelectedFullAlbum {
  pub album: crate::core::plugin_api::AlbumInfo,
  pub selected_index: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Artist {
  pub artist_id: String,
  pub artist_name: String,
  pub albums: crate::core::pagination::Paged<crate::core::plugin_api::AlbumInfo>,
  pub related_artists: Vec<crate::core::plugin_api::ArtistInfo>,
  pub top_tracks: Vec<crate::core::plugin_api::TrackInfo>,
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

/// Status of the currently-playing track's cover art, mirroring [`LyricsStatus`].
/// Drives the placeholder message shown when art can't be displayed, so a
/// missing image reads as an explicit state rather than silently showing
/// nothing (or, worse, the previous track's art).
#[cfg(feature = "cover-art")]
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CoverArtStatus {
  /// Nothing is playing / no art has been requested yet.
  #[default]
  NotStarted,
  /// A fetch/decode for the current track is in flight.
  Loading,
  /// Art for the current track is loaded and rendering.
  Loaded,
  /// The current source has no cover art to show (e.g. internet radio, or a
  /// local file with no embedded picture).
  Unavailable,
  /// A fetch/decode was attempted for the current track but failed.
  Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativePlaybackOrigin {
  Context,
  #[default]
  RawList,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeTrackKind {
  #[default]
  Track,
  Episode,
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
  pub kind: NativeTrackKind,
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
  Icons,
  Keybindings,
  Theme,
}

impl SettingsCategory {
  pub fn all() -> &'static [SettingsCategory] {
    &[
      SettingsCategory::Behavior,
      SettingsCategory::Icons,
      SettingsCategory::Keybindings,
      SettingsCategory::Theme,
    ]
  }

  pub fn name(&self) -> &'static str {
    match self {
      SettingsCategory::Behavior => "Behavior",
      SettingsCategory::Icons => "Icons",
      SettingsCategory::Keybindings => "Keybindings",
      SettingsCategory::Theme => "Theme",
    }
  }

  pub fn index(&self) -> usize {
    match self {
      SettingsCategory::Behavior => 0,
      SettingsCategory::Icons => 1,
      SettingsCategory::Keybindings => 2,
      SettingsCategory::Theme => 3,
    }
  }

  pub fn from_index(index: usize) -> Self {
    match index {
      0 => SettingsCategory::Behavior,
      1 => SettingsCategory::Icons,
      2 => SettingsCategory::Keybindings,
      3 => SettingsCategory::Theme,
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

const STARTUP_ROUTE_SETTING_OPTIONS: &[&str] = &[
  "home",
  "recently_played",
  "podcasts",
  "discover",
  "artists",
  "album_list",
];
const PLAYLIST_TRACK_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "date_added",
  "date_added:desc",
  "artist",
  "artist:desc",
  "album",
  "album:desc",
  "duration",
  "duration:desc",
];
const SAVED_ALBUM_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "date_added",
  "date_added:desc",
  "artist",
  "artist:desc",
];
const SAVED_ARTIST_SORT_SETTING_OPTIONS: &[&str] = &["default", "name", "name:desc"];
const RECENTLY_PLAYED_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "artist",
  "artist:desc",
  "album",
  "album:desc",
];
const SIDEBAR_POSITION_SETTING_OPTIONS: &[&str] = &["left", "right", "hidden"];
const PLAYBAR_POSITION_SETTING_OPTIONS: &[&str] = &["bottom", "top"];

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

/// Domain-level representation of the playback queue.
/// Replaces `rspotify::model::CurrentUserQueue` in `App` state.
#[derive(Debug, Clone)]
pub struct QueueState {
  pub currently_playing: Option<PlayableInfo>,
  pub queue: Vec<PlayableInfo>,
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
  pub artists: Vec<crate::core::plugin_api::ArtistInfo>,
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
  pub queue: Option<QueueState>,
  pub queue_selected_index: usize,
  /// The native cross-source playback queue (FIFO). Unlike [`Self::queue`]
  /// (a read-only mirror of Spotify's Web-API queue), this is owned by the app
  /// and holds tracks from any source.
  pub native_queue: Vec<TrackInfo>,
  /// How to resume the underlying per-source context after the native queue
  /// drains. Populated when a track is queued over an active context.
  pub queue_suspended: Option<crate::core::queue::SuspendedContext>,
  /// What the native queue's playback slot is currently playing, if anything.
  /// Overlays the per-source `*_playback` contexts without mutating them (those
  /// are the context to resume). Gated to builds that can actually play a queued
  /// track — every read goes through the unconditional
  /// [`Self::queue_owns_playback`] accessor.
  #[cfg(any(feature = "streaming", feature = "audio-decode"))]
  pub queue_now: Option<crate::infra::queue::QueueNowPlaying>,
  /// Bounded retry guard for the native-Spotify queue slot. When a queued
  /// Spotify track is playing via a direct `player.load` (no Spirc context) and
  /// Spirc self-advances to a different track, the player-event handler reissues
  /// the queued track and increments this. Reset to 0 each time a new Spotify
  /// queue slot is published; capped so a genuinely-gone track can't loop.
  #[cfg(feature = "streaming")]
  pub spotify_queue_guard_reloads: u8,
  #[cfg(feature = "cover-art")]
  pub cover_art: crate::tui::cover_art::CoverArt,
  /// Status of the current track's cover art, driving the placeholder message.
  #[cfg(feature = "cover-art")]
  pub cover_art_status: CoverArtStatus,
  // Inputs:
  // input is the string for input;
  // input_idx is the index of the cursor in terms of character;
  // input_cursor_position is the sum of the width of characters preceding the cursor.
  // Reason for this complication is due to non-ASCII characters, they may
  // take more than 1 bytes to store and more than 1 character width to display.
  pub input: Vec<char>,
  pub input_idx: usize,
  pub input_cursor_position: u16,
  pub input_context: InputContext,
  /// Horizontal scroll offset for the input box, computed during rendering.
  pub input_scroll_offset: Cell<u16>,
  pub liked_song_ids_set: HashSet<String>,
  pub followed_artist_ids_set: HashSet<String>,
  pub saved_album_ids_set: HashSet<String>,
  pub saved_show_ids_set: HashSet<String>,
  pub library: Library,
  pub playlist_offset: u32,
  // Each item carries its absolute playlist position (`page.offset + raw slot
  // index`) alongside the playable. The position is computed in the mapping
  // layer before unparseable/local slots are dropped, so removal-by-position and
  // play-from-here offsets stay correct (see `playlist_items_page`).
  pub playlist_tracks: Option<Paged<(u32, PlayableInfo)>>,
  pub playlist_track_pages: ScrollableResultPages<Paged<(u32, PlayableInfo)>>,
  pub playlist_track_table_id: Option<PlaylistId<'static>>,
  pub active_playlist_track_filter: Option<String>,
  pub pending_playlist_track_search: Option<String>,
  pub playlists: Option<Paged<PlaylistInfo>>,
  pub recently_played: SpotifyResultAndSelectedIndex<
    Option<crate::core::pagination::CursorPaged<crate::core::plugin_api::TrackInfo>>,
  >,
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
  pub user: Option<UserInfo>,
  pub album_list_index: usize,
  pub artists_list_index: usize,
  /// Folders (one per subdirectory of the configured music dir) shown by the
  /// Local Files browser, and the cursor within that list.
  pub local_playlists: Vec<PlaylistInfo>,
  pub local_playlists_index: usize,
  /// The user's Subsonic server playlists shown by the Subsonic browser, and the
  /// cursor within that list. Populated by `GetSubsonicPlaylists` dispatch.
  pub subsonic_playlists: Vec<PlaylistInfo>,
  pub subsonic_playlists_index: usize,
  /// The user's configured internet-radio stations (as playable rows, uri
  /// `radio:<url>`) shown by the sidebar when the Radio source is active, and
  /// the cursor within that list. Populated by `GetRadioStations` dispatch.
  /// Unconditional (domain type) because the sidebar match arms key on the
  /// unconditional `Source::Radio` variant even in the slim build.
  pub radio_stations: Vec<TrackInfo>,
  pub radio_stations_index: usize,
  /// The user's local YouTube playlists (from `youtube_playlists.yml`), shown
  /// by the sidebar when the YouTube source is active. Unconditional for the
  /// same slim-build reason as [`radio_stations`](Self::radio_stations).
  pub youtube_playlists: Vec<PlaylistInfo>,
  /// The `youtube:playlist:` URI currently open in the shared track table, so
  /// the remove-track flow knows which playlist to edit.
  pub youtube_open_playlist: Option<String>,
  /// The source the UI is currently scoped to (sidebar, search, capability
  /// gating). Browse-scope only — never changes playback routing.
  pub active_source: Source,
  /// Cursor within the Source panel of the `d` picker (index into [`Source::ALL`]).
  pub source_list_index: usize,
  /// Which panel of the `d` picker currently has focus.
  pub source_device_focus: SourceFocus,
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
  /// Expiry of the current Spotify access token, or `None` when there is no
  /// Spotify session (launched against a free source). The token-refresh poll
  /// in the runner skips refreshing while this is `None`.
  pub spotify_token_expiry: Option<SystemTime>,
  /// Whether a Spotify session is available (token loaded at startup or added
  /// via in-TUI login). Gates the Spotify-only startup dispatches so a
  /// free-source launch doesn't spam "connect Spotify" messages.
  pub spotify_connected: bool,
  pub auth_refresh_in_progress: bool,
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
  /// A `file://` URI to start playing once the UI is up (set from `--play-file`).
  /// Consumed and cleared on first render.
  pub pending_play_file: Option<String>,
  /// Native playback state - updated by player events, used when streaming is active
  /// This is more reliable than current_playback_context.is_playing during native streaming
  pub native_is_playing: Option<bool>,
  /// Tracks whether the current native playback was started from a Spotify context
  /// or from a raw URI-list/native-only route.
  pub native_playback_origin: Option<NativePlaybackOrigin>,
  /// Prevent idle/sleep during playback
  pub keepawake: Option<keepawake::KeepAwake>,
  /// Timestamp of the last native device activation
  #[allow(dead_code)]
  pub last_device_activation: Option<Instant>,
  /// Whether a native device activation is still in progress
  #[allow(dead_code)]
  pub native_activation_pending: bool,
  /// Selected index in the Discover view
  pub discover_selected_index: usize,
  /// Top tracks from the user for Discover feature
  pub discover_top_tracks: Vec<TrackInfo>,
  /// Top Artists Mix tracks for Discover feature
  pub discover_artists_mix: Vec<TrackInfo>,
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
  pub recently_played_sort: SortState,
  /// Animation frame counter for the "Liked" heart flash effect (0-10)
  pub liked_song_animation_frame: Option<u8>,
  /// Global animation tick counter, incremented every tick.
  pub animation_tick: u64,
  /// Last time the listening party host broadcast playback state.
  pub last_party_sync_at: Instant,
  /// Ephemeral status message shown in the playbar
  pub status_message: Option<String>,
  /// When to clear the status message
  pub status_message_expires_at: Option<Instant>,
  /// True when the current status message is an error (blocks normal message overwrites)
  pub status_message_is_error: bool,
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
  pub all_playlists: Vec<PlaylistInfo>,
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
  pub saved_tracks_prefetch_in_flight: HashSet<u32>,
  /// Incremented every time the playlist track table is reloaded to guard stale prefetch tasks
  pub playlist_tracks_prefetch_generation: u64,
  pub playlist_tracks_prefetch_in_flight: HashSet<u32>,
  /// Tracks whether a ChangeVolume request is on its way to Spotify.
  /// When true, we hold off on sending another one — rapid key presses
  /// just update `pending_volume` and the latest value wins.
  pub is_volume_change_in_flight: bool,
  /// Reference to the native streaming player for direct control (bypasses event channel)
  #[cfg(feature = "streaming")]
  pub streaming_player: Option<Arc<crate::infra::player::StreamingPlayer>>,
  /// The active local-file playback session (multi-source Phase 3), or `None`
  /// when Spotify owns playback. Decoupled from Spotify/librespot state: the
  /// local playbar reads progress and pause state live from the player here, so
  /// librespot events and polls never desync it. `Some` exactly while a local
  /// file is playing; dropping it releases the audio output device.
  #[cfg(feature = "local-files")]
  pub local_playback: Option<crate::infra::local::LocalPlaybackState>,
  /// The active Subsonic playback session (multi-source Phase 4), or `None` when
  /// another backend owns playback. Same decoupling contract as
  /// [`local_playback`](Self::local_playback): the playbar reads progress/pause
  /// live from the player here, never touching Spotify/librespot fields.
  #[cfg(feature = "subsonic")]
  pub subsonic_playback: Option<crate::infra::subsonic::SubsonicPlaybackState>,
  /// The active internet-radio playback session (multi-source Phase 5), or
  /// `None` when another backend owns playback. Same decoupling contract as
  /// [`local_playback`](Self::local_playback); unlike it there is no queue —
  /// a station is one infinite stream.
  #[cfg(feature = "internet-radio")]
  pub radio_playback: Option<crate::infra::radio::RadioPlaybackState>,
  /// The active YouTube playback session (multi-source, yt-dlp backed), or
  /// `None` when another backend owns playback. Same decoupling contract as
  /// [`subsonic_playback`](Self::subsonic_playback).
  #[cfg(feature = "youtube")]
  pub youtube_playback: Option<crate::infra::youtube::YouTubePlaybackState>,
  /// Sender used to recover native streaming when a stale/disconnected player is detected.
  #[cfg(feature = "streaming")]
  pub streaming_recovery_tx:
    Option<tokio::sync::mpsc::UnboundedSender<crate::infra::player::StreamingRecoveryRequest>>,
  /// Reference to MPRIS manager for emitting Seeked signals after native seeks
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub mpris_manager: Option<Arc<crate::infra::mpris::MprisManager>>,

  // Friends screen state
  /// All friends fetched from spotatui.com (follows list)
  pub friends: Vec<FriendEntry>,
  /// Whether friends are currently loading from the API
  pub friends_loading: bool,
  /// Own friend code fetched from spotatui.com
  pub friend_code: Option<String>,
  /// Cursor position in the friends list
  pub friend_selected_index: usize,
  /// Active filter (All / Online)
  pub friend_filter: FriendFilter,
  /// Inline search / filter input on the Friends screen
  pub friend_search_input: Vec<char>,
  /// Whether the "Add Friend" overlay dialog is open
  pub friend_add_dialog_visible: bool,
  /// Which tab is active inside the add-friend dialog
  pub friend_add_mode: FriendAddMode,
  /// Input buffer for the "add by friend code" text field
  pub friend_add_input: Vec<char>,
  /// Input buffer for the "search by username" text field in the add dialog
  pub friend_user_search_input: Vec<char>,
  /// Results from searching users by name
  pub friend_user_search_results: Vec<FriendSearchResult>,
  /// Selected row in the user-search results list
  pub friend_user_search_selected: usize,
  /// Timestamp of the last time friends were refreshed (for periodic polling)
  pub last_friends_refresh_at: Instant,

  // Create Playlist form state
  pub create_playlist_name: Vec<char>,
  pub create_playlist_name_idx: usize,
  pub create_playlist_name_cursor: u16,
  pub create_playlist_stage: CreatePlaylistStage,
  pub create_playlist_tracks: Vec<TrackInfo>,
  pub create_playlist_search_results: Vec<TrackInfo>,
  pub create_playlist_search_input: Vec<char>,
  pub create_playlist_search_idx: usize,
  pub create_playlist_search_cursor: u16,
  pub create_playlist_selected_result: usize,
  pub create_playlist_focus: CreatePlaylistFocus,
  /// Commands queued by keybindings for the scripting engine to run.
  pub pending_plugin_commands: Vec<String>,
  /// Per-plugin playbar segments, keyed by plugin name (BTreeMap for deterministic order).
  pub plugin_playbar_segments: std::collections::BTreeMap<String, String>,
  /// Currently displayed plugin popup, if any.
  pub plugin_popup: Option<crate::core::plugin_api::PluginPopup>,
  /// Scroll offset for the plugin popup.
  pub plugin_popup_scroll: u16,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PendingTrackSelection {
  Index(usize),
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
      local_playlists: Vec::new(),
      local_playlists_index: 0,
      subsonic_playlists: Vec::new(),
      subsonic_playlists_index: 0,
      radio_stations: Vec::new(),
      radio_stations_index: 0,
      youtube_playlists: Vec::new(),
      youtube_open_playlist: None,
      active_source: Source::default(),
      source_list_index: 0,
      source_device_focus: SourceFocus::default(),
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
      small_search_limit: 4,
      api_error: String::new(),
      current_playback_context: None,
      last_track_id: None,
      pending_stop_after_track: false,
      devices: None,
      queue: None,
      queue_selected_index: 0,
      native_queue: Vec::new(),
      queue_suspended: None,
      #[cfg(any(feature = "streaming", feature = "audio-decode"))]
      queue_now: None,
      #[cfg(feature = "streaming")]
      spotify_queue_guard_reloads: 0,
      input: vec![],
      input_idx: 0,
      input_cursor_position: 0,
      input_context: InputContext::GlobalSearch,
      input_scroll_offset: Cell::new(0),
      playlist_offset: 0,
      playlist_tracks: None,
      playlist_track_pages: ScrollableResultPages::new(),
      playlist_track_table_id: None,
      active_playlist_track_filter: None,
      pending_playlist_track_search: None,
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
      spotify_token_expiry: None,
      spotify_connected: false,
      auth_refresh_in_progress: false,
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
      pending_play_file: None,
      native_is_playing: None,
      native_playback_origin: None,
      keepawake: None,
      last_device_activation: None,
      native_activation_pending: false,
      // Sort menu defaults
      sort_menu_visible: false,
      sort_menu_selected: 0,
      sort_context: None,
      playlist_sort: SortState::new(),
      album_sort: SortState::new(),
      artist_sort: SortState::new(),
      recently_played_sort: SortState::new(),
      liked_song_animation_frame: None,
      animation_tick: 0,
      last_party_sync_at: Instant::now(),
      status_message: None,
      status_message_expires_at: None,
      status_message_is_error: false,
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
      saved_tracks_prefetch_in_flight: HashSet::new(),
      playlist_tracks_prefetch_generation: 0,
      playlist_tracks_prefetch_in_flight: HashSet::new(),
      is_volume_change_in_flight: false,
      pending_volume: None,
      last_dispatched_volume: None,
      #[cfg(feature = "streaming")]
      streaming_player: None,
      #[cfg(feature = "local-files")]
      local_playback: None,
      #[cfg(feature = "subsonic")]
      subsonic_playback: None,
      #[cfg(feature = "internet-radio")]
      radio_playback: None,
      #[cfg(feature = "youtube")]
      youtube_playback: None,
      #[cfg(feature = "streaming")]
      streaming_recovery_tx: None,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: None,
      #[cfg(feature = "cover-art")]
      cover_art: crate::tui::cover_art::CoverArt::new(),
      #[cfg(feature = "cover-art")]
      cover_art_status: CoverArtStatus::default(),
      friends: Vec::new(),
      friends_loading: false,
      friend_code: None,
      friend_selected_index: 0,
      friend_filter: FriendFilter::All,
      friend_search_input: Vec::new(),
      friend_add_dialog_visible: false,
      friend_add_mode: FriendAddMode::Code,
      friend_add_input: Vec::new(),
      friend_user_search_input: Vec::new(),
      friend_user_search_results: Vec::new(),
      friend_user_search_selected: 0,
      last_friends_refresh_at: Instant::now(),
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
      pending_plugin_commands: Vec::new(),
      plugin_playbar_segments: std::collections::BTreeMap::new(),
      plugin_popup: None,
      plugin_popup_scroll: 0,
    }
  }
}

impl App {
  pub fn new(
    io_tx: Sender<IoEvent>,
    user_config: UserConfig,
    spotify_token_expiry: Option<SystemTime>,
  ) -> App {
    // Read the persisted active source before moving user_config into the struct,
    // so the restored value overrides the Source::default() set by App::default().
    let active_source = user_config.behavior.active_source;
    // Resolve configurable per-context default sort states. Config validation
    // already rejected invalid specs at load time, so parse failure here is a
    // defensive fallback to the built-in default sort.
    let parse_sort = |spec: &str, ctx: SortContext| -> SortState {
      SortState::parse(spec, ctx).unwrap_or_default()
    };
    let playlist_sort = parse_sort(
      &user_config.behavior.default_sort_playlist_tracks,
      SortContext::PlaylistTracks,
    );
    let album_sort = parse_sort(
      &user_config.behavior.default_sort_saved_albums,
      SortContext::SavedAlbums,
    );
    let artist_sort = parse_sort(
      &user_config.behavior.default_sort_saved_artists,
      SortContext::SavedArtists,
    );
    let recently_played_sort = parse_sort(
      &user_config.behavior.default_sort_recently_played,
      SortContext::RecentlyPlayed,
    );
    // Resolve the configurable startup route. Unknown / non-context-free values
    // degrade to Home + warn (precedent: StartupBehavior::from_name).
    let startup_route_id = match RouteId::from_config_str(&user_config.behavior.startup_route) {
      Some(id) => id,
      None => {
        log::warn!(
          "[config] startup_route '{}' is not a valid context-free route (valid: {}); using Home",
          user_config.behavior.startup_route,
          RouteId::STARTUP_OPTIONS
            .iter()
            .map(|r| r.to_config_str())
            .collect::<Vec<_>>()
            .join(", ")
        );
        RouteId::Home
      }
    };
    let startup_route = Route {
      id: startup_route_id,
      active_block: ActiveBlock::Empty,
      hovered_block: ActiveBlock::Library,
    };
    App {
      io_tx: Some(io_tx),
      user_config,
      // A token expiry means a Spotify session loaded at startup; a free-source
      // launch with no cached token passes `None`. In-TUI login flips both fields.
      spotify_connected: spotify_token_expiry.is_some(),
      spotify_token_expiry,
      active_source,
      navigation_stack: vec![startup_route],
      playlist_sort,
      album_sort,
      artist_sort,
      recently_played_sort,
      ..App::default()
    }
  }

  /// Sort the recently-played track list in place per `recently_played_sort`.
  /// `Default` keeps the API's play order (a re-fetch restores it).
  pub fn sort_recently_played_items(&mut self) {
    let sort_state = self.recently_played_sort;
    if sort_state.field == SortField::Default {
      return;
    }
    if let Some(page) = self.recently_played.result.as_mut() {
      page.items.sort_by(|a, b| {
        let order = match sort_state.field {
          SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
          SortField::Artist => {
            let artist_a = a.artists.first().map(|s| s.to_lowercase());
            let artist_b = b.artists.first().map(|s| s.to_lowercase());
            artist_a.cmp(&artist_b)
          }
          SortField::Album => a.album.to_lowercase().cmp(&b.album.to_lowercase()),
          _ => std::cmp::Ordering::Equal,
        };
        if sort_state.order == SortOrder::Descending {
          order.reverse()
        } else {
          order
        }
      });
    }
  }

  // Send a network event to the network thread
  /// Clone the IoEvent sender so a spawned task (e.g. the in-TUI Spotify login
  /// callback server) can dispatch events back into the pump without holding the
  /// `App` lock. `None` before the sender is wired up or after teardown.
  pub fn io_tx_clone(&self) -> Option<Sender<IoEvent>> {
    self.io_tx.clone()
  }

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

  /// Snapshot the currently-playing non-Spotify session for persistence, or
  /// `None` when Spotify (or nothing) owns playback. Reads the live position and
  /// pause state straight from whichever source's player is active, mirroring
  /// the source-ownership order the runner tick uses. Spotify playback is not
  /// persisted here — its resume is handled by `startup_behavior` device logic.
  /// The resume point (`resume_index`, `resume_position_ms`) recorded when the
  /// active decoded context was suspended under the native queue, or `None` when
  /// no decoded context is suspended. Only one context is ever active, so this
  /// unambiguously describes it.
  #[cfg(any(feature = "youtube", feature = "subsonic", feature = "local-files"))]
  fn suspended_resume(&self) -> Option<(Option<usize>, u64)> {
    match self.queue_suspended.as_ref()? {
      #[cfg(feature = "local-files")]
      crate::core::queue::SuspendedContext::Local {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[cfg(feature = "subsonic")]
      crate::core::queue::SuspendedContext::Subsonic {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[cfg(feature = "youtube")]
      crate::core::queue::SuspendedContext::YouTube {
        resume_index,
        resume_position_ms,
      } => Some((*resume_index, *resume_position_ms)),
      #[allow(unreachable_patterns)]
      _ => None,
    }
  }

  pub fn current_persisted_playback(
    &self,
  ) -> Option<crate::core::persisted_playback::PersistedPlayback> {
    #[cfg(any(
      feature = "youtube",
      feature = "subsonic",
      feature = "local-files",
      feature = "internet-radio"
    ))]
    use crate::core::persisted_playback::PersistedPlayback;
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_ref() {
      // While suspended under the queue, the context's player is playing a
      // *queued* track, so read the resume point from `queue_suspended` instead
      // of the (repurposed) live player. A `None` resume_index means the context
      // was exhausted — don't persist it (the queue itself still persists).
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::YouTube {
            tracks: s.tracks.clone(),
            index,
            position_ms,
            paused: false,
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::YouTube {
            tracks: s.tracks.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
          });
        }
      }
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_ref() {
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::Subsonic {
            tracks: s.tracks.clone(),
            index,
            position_ms,
            paused: false,
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::Subsonic {
            tracks: s.tracks.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
          });
        }
      }
    }
    #[cfg(feature = "local-files")]
    if let Some(s) = self.local_playback.as_ref() {
      match self.suspended_resume() {
        Some((Some(index), position_ms)) => {
          return Some(PersistedPlayback::Local {
            queue: s.queue.clone(),
            index,
            position_ms,
            paused: false,
          });
        }
        Some((None, _)) => {}
        None => {
          return Some(PersistedPlayback::Local {
            queue: s.queue.clone(),
            index: s.index,
            position_ms: s.player.position().as_millis() as u64,
            paused: s.player.is_paused(),
          });
        }
      }
    }
    #[cfg(feature = "internet-radio")]
    if let Some(s) = self.radio_playback.as_ref() {
      return Some(PersistedPlayback::Radio {
        station: s.station.clone(),
        paused: s.player.is_paused(),
      });
    }
    None
  }

  /// Snapshot the full session to persist: the active non-Spotify playback (if
  /// any) plus the native queue. Returns `None` only when there is nothing to
  /// save (no active source *and* an empty queue), so the caller clears the
  /// session file — preserving the existing Some→None clear semantics.
  pub fn current_persisted_session(
    &self,
  ) -> Option<crate::core::persisted_playback::PersistedSession> {
    let playback = self.current_persisted_playback();
    let queue_now_track = self.queue_now_track();
    if playback.is_none() && self.native_queue.is_empty() && queue_now_track.is_none() {
      return None;
    }
    // A track playing through the queue slot has been popped off `native_queue`;
    // prepend it so a mid-queue quit resumes it (it re-enters the queue on the
    // next launch).
    let mut queue = self.native_queue.clone();
    if let Some(track) = queue_now_track {
      queue.insert(0, track.clone());
    }
    Some(crate::core::persisted_playback::PersistedSession { playback, queue })
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

  pub fn clear_friend_add_dialog_state(&mut self) {
    self.friend_add_dialog_visible = false;
    self.friend_add_mode = FriendAddMode::Code;
    self.friend_add_input.clear();
    self.friend_user_search_input.clear();
    self.friend_user_search_results.clear();
    self.friend_user_search_selected = 0;
  }

  pub fn open_friend_add_dialog(&mut self) {
    self.clear_friend_add_dialog_state();
    self.friend_add_dialog_visible = true;
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
    // A live error message blocks normal messages from overwriting it.
    if self.status_message_is_error {
      if let (Some(_), Some(expires_at)) = (&self.status_message, self.status_message_expires_at) {
        if Instant::now() < expires_at {
          return;
        }
      }
    }
    self.status_message = Some(message.into());
    let ttl = self.scaled_status_ttl(ttl_secs);
    self.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl));
    self.status_message_is_error = false;
  }

  /// Queue a plugin command name to be executed by the scripting engine.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn queue_plugin_command(&mut self, name: String) {
    self.pending_plugin_commands.push(name);
  }

  /// Set an error status message. Errors always replace whatever is currently shown
  /// (including a previous error) and are styled distinctly in the UI.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn set_error_status_message(&mut self, message: impl Into<String>, ttl_secs: u64) {
    self.status_message = Some(message.into());
    let ttl = self.scaled_status_ttl(ttl_secs);
    self.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl));
    self.status_message_is_error = true;
  }

  /// Scale a status-message TTL by `status_message_ttl_percent` (default 100
  /// == 1.0×). Applied here at the single sink so the ~66 call sites keep
  /// their relative per-severity TTLs.
  fn scaled_status_ttl(&self, ttl_secs: u64) -> u64 {
    let pct = self.user_config.behavior.status_message_ttl_percent as u64;
    // round to nearest, never zero.
    ((ttl_secs * pct + 50) / 100).max(1)
  }

  #[cfg(feature = "streaming")]
  pub fn request_native_streaming_recovery_if_disconnected(
    &mut self,
    reselect_device: bool,
  ) -> bool {
    let Some(player) = self.streaming_player.as_ref() else {
      return false;
    };

    if player.is_connected() {
      return false;
    }

    // Stop the old spirc before dropping our reference so the dead session
    // doesn't linger as a ghost Connect device (#297).
    player.shutdown();
    self.streaming_player = None;
    self.is_streaming_active = false;
    self.native_activation_pending = false;
    self.native_device_id = None;
    self.native_is_playing = Some(false);
    self.native_track_info = None;
    self.native_playback_origin = None;
    self.song_progress_ms = 0;
    self.last_track_id = None;
    self.last_device_activation = None;
    self.seek_ms = None;
    if reselect_device {
      self.current_playback_context = None;
    }

    self.set_status_message("Native streaming disconnected; attempting recovery.", 8);
    if let Some(tx) = &self.streaming_recovery_tx {
      let _ = tx.send(crate::infra::player::StreamingRecoveryRequest { reselect_device });
    }
    self.dispatch(IoEvent::GetCurrentPlayback);
    true
  }

  pub fn playlist_is_editable(&self, playlist: &PlaylistInfo) -> bool {
    let Some(user) = &self.user else {
      return false;
    };

    playlist.owner_id.as_deref() == Some(user.id.as_str()) || playlist.collaborative
  }

  pub fn editable_playlists(&self) -> Vec<&PlaylistInfo> {
    self
      .all_playlists
      .iter()
      .filter(|playlist| self.playlist_is_editable(playlist))
      .collect()
  }

  /// The destination playlists offered by the add-track picker dialog for the
  /// active source: local YouTube playlists under YouTube, the user's editable
  /// Spotify playlists otherwise.
  pub fn playlist_picker_items(&self) -> Vec<&PlaylistInfo> {
    if self.active_source == Source::YouTube {
      self.youtube_playlists.iter().collect()
    } else {
      self.editable_playlists()
    }
  }

  pub fn begin_add_track_to_playlist_flow(&mut self, track_id: Option<String>, track_name: String) {
    let Some(track_id) = track_id else {
      self.set_status_message("Track cannot be added to playlist".to_string(), 4);
      return;
    };

    // Under the YouTube source the destinations are the *local* playlists
    // (youtube_playlists.yml), not the Spotify ones — no user/playlist
    // fetches apply. The picker's Enter routes by source too.
    if self.active_source == Source::YouTube {
      if self.youtube_playlists.is_empty() {
        // Kick a (re)load in case the file changed on disk; if it is
        // genuinely empty the user needs to create a playlist first.
        self.dispatch(IoEvent::GetYouTubePlaylists);
        self.set_status_message(
          "No YouTube playlists yet — create one from the sidebar".to_string(),
          4,
        );
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
      return;
    }

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
    self.get_playlist_display_items().len()
  }

  /// Get a visible item by display index in the current folder.
  pub fn get_playlist_display_item_at(&self, display_index: usize) -> Option<&PlaylistFolderItem> {
    self
      .get_playlist_display_items()
      .into_iter()
      .nth(display_index)
  }

  /// Get visible playlist items in the current folder (used by UI rendering).
  ///
  /// Single source of truth for the visible order: rendering and index-based
  /// selection (keyboard + mouse) both go through it, so they can never
  /// disagree. When `group_folders_first` is set, folders are hoisted to the
  /// top via a stable sort, preserving each group's relative order.
  pub fn get_playlist_display_items(&self) -> Vec<&PlaylistFolderItem> {
    let mut items: Vec<&PlaylistFolderItem> = self
      .playlist_folder_items
      .iter()
      .filter(|item| self.is_playlist_item_visible_in_current_folder(item))
      .collect();
    if self.user_config.behavior.group_folders_first {
      items.sort_by_key(|item| !matches!(item, PlaylistFolderItem::Folder(_)));
    }
    items
  }

  /// Get the playlist for a PlaylistFolderItem::Playlist variant
  #[allow(dead_code)]
  pub fn get_playlist_for_item(&self, item: &PlaylistFolderItem) -> Option<&PlaylistInfo> {
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
      return self.all_playlists.get(*index).and_then(|p| p.id.clone());
    }

    self
      .playlists
      .as_ref()
      .and_then(|playlists| playlists.items.get(selected_index))
      .and_then(|playlist| playlist.id.clone())
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
    // No Spotify session (free-source launch): the poll would hit the auth gate
    // and re-flash a "connect Spotify" status message every interval. Free
    // sources drive their own playback state, so skip the Spotify poll entirely.
    if !self.spotify_connected {
      return;
    }

    // Poll interval depends on playback mode:
    // - Native streaming: configurable (default 5s; real-time events provide
    //   updates between polls).
    // - External players (spotifyd, etc.): 1 second (no events, need faster
    //   polling for smooth playbar) — stays hardcoded, not a preference.
    let poll_interval_ms: u128 = if self.is_streaming_active {
      self.user_config.behavior.playback_poll_seconds as u128 * 1000
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

  pub fn update_on_tick(&mut self, elapsed: Duration) {
    // Increment global animation tick (wraps after ~9.4 quintillion ticks, effectively never)
    self.animation_tick = self.animation_tick.wrapping_add(1);

    // Periodic party sync: host broadcasts state about every 2 seconds.
    // Keep this before early-return paths so sync still happens during native-streaming fast paths.
    if self.party_status == PartyStatus::Hosting
      && self.last_party_sync_at.elapsed() >= Duration::from_secs(2)
    {
      self.last_party_sync_at = Instant::now();
      self.dispatch(IoEvent::SyncPlayback);
    }

    // Periodic friends refresh: re-fetch when the Friends screen is active, every 30 seconds.
    if self.get_current_route().id == RouteId::Friends
      && self.last_friends_refresh_at.elapsed() >= Duration::from_secs(30)
      && !self.friends_loading
      && self.user_config.behavior.sync_token.is_some()
    {
      self.last_friends_refresh_at = Instant::now();
      self.dispatch(IoEvent::GetFriends);
    }

    if let Some(expires_at) = self.status_message_expires_at {
      if Instant::now() >= expires_at {
        self.status_message = None;
        self.status_message_expires_at = None;
        self.status_message_is_error = false;
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
    let playing_now = self.user_config.behavior.keepawake_enabled
      && self
        .native_is_playing
        .or_else(|| self.current_playback_context.as_ref().map(|c| c.is_playing))
        .unwrap_or(false);
    match (playing_now, self.keepawake.is_some()) {
      (true, false) => {
        self.keepawake = keepawake::Builder::default()
          .idle(true)
          .sleep(true)
          .display(true)
          .reason("Playing music")
          .app_name("spotatui")
          .create()
          .ok();
      }
      (false, true) => self.keepawake = None,
      _ => {}
    }

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
        let elapsed_ms = elapsed.as_millis();
        let duration_ms = match item {
          PlayableItem::Track(track) => track.duration.num_milliseconds() as u128,
          PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u128,
          _ => return,
        };

        self.song_progress_ms = (self.song_progress_ms + elapsed_ms).min(duration_ms);
      }
      // When paused, keep song_progress_ms unchanged
    }
  }

  pub fn seek_forwards(&mut self) {
    info!(
      "seeking forwards by {} ms",
      self.user_config.behavior.seek_milliseconds
    );
    // A seekable decoded source (local/subsonic/youtube) owns the session: seek
    // relative to *its* live position, never from the stale/foreign Spotify
    // progress. Radio returns None here, so its seek keys are correct no-ops.
    // The source player clamps to the track duration internally, so no upper
    // clamp is needed (and we must not read the stale Spotify context duration).
    if let Some(pos) = self.active_source_position_ms() {
      let new_progress = (pos as u32).saturating_add(self.user_config.behavior.seek_milliseconds);
      self.song_progress_ms = new_progress as u128;
      self.seek_ms = None;
      self.dispatch(IoEvent::Seek(new_progress));
      return;
    }
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
    // A seekable decoded source (local/subsonic/youtube) owns the session: seek
    // relative to *its* live position, never from the stale/foreign Spotify
    // progress. Radio returns None here, so its seek keys are correct no-ops.
    if let Some(pos) = self.active_source_position_ms() {
      let new_progress = (pos as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
      self.song_progress_ms = new_progress as u128;
      self.seek_ms = None;
      self.dispatch(IoEvent::Seek(new_progress));
      return;
    }
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

  /// Seek to an absolute position within the current track (e.g. from clicking or
  /// dragging on the playbar progress line). The target is clamped to the track
  /// duration. Mirrors the dispatch logic of [`Self::seek_forwards`].
  pub fn seek_to(&mut self, position_ms: u32) {
    // A seekable decoded source (local/subsonic/youtube) owns the session: seek
    // it to the absolute target directly (the source player clamps to the track
    // duration internally). Radio returns None here, so its playbar drags are
    // correct no-ops. Never read the stale Spotify context duration for a source.
    if self.active_source_position_ms().is_some() {
      self.song_progress_ms = position_ms as u128;
      self.seek_ms = None;
      self.dispatch(IoEvent::Seek(position_ms));
      return;
    }
    if let Some(CurrentPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayableItem::Track(track) => track.duration.num_milliseconds() as u32,
        PlayableItem::Episode(episode) => episode.duration.num_milliseconds() as u32,
        _ => return,
      };

      let new_progress = position_ms.min(duration_ms);
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
    first_track: Option<TrackInfo>,
  ) {
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetRecommendationsForSeed(
      seed_artists,
      seed_tracks,
      Box::new(first_track),
      user_country,
    ));
  }

  pub fn get_recommendations_for_track_id(&mut self, id: String) {
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetRecommendationsForTrackId(id, user_country));
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
      // No Spotify device volume (e.g. a decoded source is playing, or the slim
      // build has no context): fall back to the configured volume, not 0, so the
      // playbar and volume-up/down base math stay correct for every source.
      .unwrap_or(self.user_config.behavior.volume_percent as u32)
  }

  /// Set volume to an absolute percentage (0-100). Routes through the same
  /// native-streaming fast path and API coalescing logic as the keyboard
  /// volume keys, so Lua actions behave identically to keypresses.
  #[cfg_attr(not(feature = "scripting"), allow(dead_code))]
  pub fn set_volume_percent(&mut self, volume: u8) {
    let next_volume = volume.min(100);
    let current_volume = self.desired_volume() as u8;

    if next_volume != current_volume {
      info!("setting volume to {}", next_volume);
      // A decoded source owns the sink: route the volume change to its
      // dispatcher (which sets the rodio sink's gain), never to the paused
      // librespot. The dispatcher converts the u8 percentage to a float.
      if self.active_decoded_source() {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
        self.user_config.behavior.volume_percent = next_volume;
        let _ = self.user_config.save_config();
        self.pending_volume = Some(next_volume);
        return;
      }
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

          // Notify MPRIS clients of the change (VolumeChanged is never emitted by
          // librespot for local mixer changes, so this is the only way the
          // Volume D-Bus property stays in sync)
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_volume(next_volume);
          }
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
      // A decoded source owns the sink: route the volume change to its
      // dispatcher (which sets the rodio sink's gain), never to the paused
      // librespot. The dispatcher converts the u8 percentage to a float.
      if self.active_decoded_source() {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
        self.user_config.behavior.volume_percent = next_volume;
        let _ = self.user_config.save_config();
        self.pending_volume = Some(next_volume);
        return;
      }
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

          // Notify MPRIS clients of the change (VolumeChanged is never emitted by
          // librespot for local mixer changes, so this is the only way the
          // Volume D-Bus property stays in sync)
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_volume(next_volume);
          }
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

      // A decoded source owns the sink: route the volume change to its
      // dispatcher (which sets the rodio sink's gain), never to the paused
      // librespot. The dispatcher converts the u8 percentage to a float.
      if self.active_decoded_source() {
        self.dispatch(IoEvent::ChangeVolume(next_volume_u8));
        self.user_config.behavior.volume_percent = next_volume_u8;
        let _ = self.user_config.save_config();
        self.pending_volume = Some(next_volume_u8);
        return;
      }
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

          // Notify MPRIS clients of the change (VolumeChanged is never emitted by
          // librespot for local mixer changes, so this is the only way the
          // Volume D-Bus property stays in sync)
          #[cfg(all(feature = "mpris", target_os = "linux"))]
          if let Some(ref mpris) = self.mpris_manager {
            mpris.set_volume(next_volume_u8);
          }
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

  #[cfg(feature = "streaming")]
  pub fn mark_native_streaming_device_available(
    &mut self,
    device_id: String,
    device_name: String,
    volume_percent: u8,
  ) {
    self.native_device_id = Some(device_id.clone());
    self.is_streaming_active = true;
    self.native_activation_pending = false;
    self.native_is_playing = Some(false);

    if self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.item.as_ref())
      .is_some()
    {
      return;
    }

    self.current_playback_context = Some(CurrentPlaybackContext {
      device: Device {
        id: Some(device_id),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: device_name,
        _type: DeviceType::Computer,
        volume_percent: Some(u32::from(volume_percent)),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: self.user_config.behavior.shuffle_enabled,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: false,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Unknown,
      actions: Actions::default(),
    });
  }

  #[cfg(feature = "streaming")]
  pub fn has_fresh_native_activity(&self) -> bool {
    self.native_track_info.is_some()
      || self.native_is_playing == Some(true)
      || self
        .last_device_activation
        .is_some_and(|instant| instant.elapsed() < FRESH_NATIVE_ACTIVITY_WINDOW)
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

    // Fallback: strict name match (case-insensitive), but only while we have
    // fresh native activity or a recent explicit activation. After a recovery,
    // Spotify can keep returning the old "spotatui" device while the new native
    // player is connected but stopped/not active.
    if let Some(native_name) = native_device_name.as_ref() {
      let current_device_name = ctx.device.name.to_lowercase();
      if current_device_name == native_name.as_str() && self.has_fresh_native_activity() {
        return true;
      }
    }

    // No match - not the active device
    false
  }

  /// Whether Spotify playback is happening on an *external* Connect device
  /// (i.e. a Spotify context exists and it is not our own native streaming
  /// device). When true, `z` on a Spotify track keeps today's Web-API
  /// `AddItemToQueue` behavior instead of routing to the native queue. Under a
  /// build without native streaming, any Spotify context is external by
  /// definition.
  pub fn spotify_external_device_active(&self) -> bool {
    #[cfg(feature = "streaming")]
    {
      self.current_playback_context.is_some() && !self.is_native_streaming_active_for_playback()
    }
    #[cfg(not(feature = "streaming"))]
    {
      self.current_playback_context.is_some()
    }
  }

  /// Add a track to the native cross-source queue.
  ///
  /// Rejects tracks with no URI and radio streams (a live stream is not a finite
  /// track). Spotify tracks on an external Connect device keep today's Web-API
  /// queue behavior (there is no native sink to play them through); everything
  /// else is pushed onto [`Self::native_queue`].
  pub fn add_track_to_native_queue(&mut self, track: TrackInfo) {
    let Some(uri) = track.uri.clone() else {
      self.set_status_message("Cannot queue: track has no URI", 3);
      return;
    };
    if uri.starts_with("radio:") {
      self.set_status_message("Radio stations can't be queued", 3);
      return;
    }
    // A Spotify track controlled on an external device has no native sink to
    // play through, so fall back to the Spotify Web-API queue.
    if matches!(
      crate::core::queue::queue_item_source(&uri),
      crate::core::queue::QueueItemSource::Spotify
    ) && self.spotify_external_device_active()
    {
      self.dispatch(IoEvent::AddItemToQueue(uri));
      return;
    }
    let name = track.name.clone();
    self.native_queue.push(track);
    self.set_status_message(format!("Queued: {name}"), 3);
    // Keep the Spotify mirror queue ([`Self::queue`]) current while a native
    // Spotify context is playing: it is the snapshot source for the resume
    // target when this newly-queued item later suspends the context.
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() && !self.queue_owns_playback() {
      self.dispatch(IoEvent::GetQueue);
    }
  }

  /// Whether the native queue's playback slot currently owns the output (either a
  /// decoded queued track or a native-streamed Spotify one). The single gated
  /// entry point for every queue-ownership check; reduces to `false` in a slim
  /// build where the slot cannot exist.
  pub fn queue_owns_playback(&self) -> bool {
    #[cfg(any(feature = "streaming", feature = "audio-decode"))]
    {
      self.queue_now.is_some()
    }
    #[cfg(not(any(feature = "streaming", feature = "audio-decode")))]
    {
      false
    }
  }

  /// Whether the queue slot is playing a Spotify track via native streaming.
  /// While true, librespot owns the sink and any still-`Some` decoded
  /// `*_playback` struct is a *suspended* context that must stay invisible to
  /// the decoded-ownership predicates (`active_decoded_source`,
  /// `active_decoded_player`, transport routing) — otherwise a space-bar toggle
  /// or media key would resume the suspended player on top of librespot.
  pub(crate) fn queue_now_is_spotify(&self) -> bool {
    #[cfg(feature = "streaming")]
    {
      matches!(
        self.queue_now.as_ref(),
        Some(QueueNowPlaying::Spotify { .. })
      )
    }
    #[cfg(not(feature = "streaming"))]
    {
      false
    }
  }

  /// The queue slot's player when it is playing a *decoded* queued track (local /
  /// Subsonic / YouTube). `None` for a Spotify slot or an empty slot.
  #[cfg(feature = "audio-decode")]
  pub fn queue_now_decoded_player(&self) -> Option<&Arc<crate::infra::audio::LocalPlayer>> {
    match self.queue_now.as_ref()? {
      QueueNowPlaying::Decoded(d) => Some(&d.player),
      #[cfg(feature = "streaming")]
      QueueNowPlaying::Spotify { .. } => None,
    }
  }

  /// Take the queue slot, returning its player when it was a decoded track (so
  /// the caller can stop it). Clears [`Self::queue_now`] either way.
  #[cfg(feature = "audio-decode")]
  pub fn take_queue_now_decoded_player(&mut self) -> Option<Arc<crate::infra::audio::LocalPlayer>> {
    match self.queue_now.take() {
      Some(QueueNowPlaying::Decoded(d)) => Some(d.player),
      _ => None,
    }
  }

  /// The track currently playing through the queue slot, if any. Used by
  /// persistence to prepend it back onto the saved queue so a mid-queue quit
  /// doesn't lose the in-flight track.
  fn queue_now_track(&self) -> Option<&TrackInfo> {
    #[cfg(any(feature = "streaming", feature = "audio-decode"))]
    {
      match self.queue_now.as_ref()? {
        #[cfg(feature = "audio-decode")]
        QueueNowPlaying::Decoded(d) => Some(&d.track),
        #[cfg(feature = "streaming")]
        QueueNowPlaying::Spotify { track } => Some(track),
      }
    }
    #[cfg(not(any(feature = "streaming", feature = "audio-decode")))]
    {
      None
    }
  }

  /// A `"{name} - {artists}"` label for the track playing through the queue slot,
  /// for the Queue screen's "Now playing" row. `None` when the slot is empty.
  pub fn queue_now_display(&self) -> Option<String> {
    let track = self.queue_now_track()?;
    Some(format!("{} - {}", track.name, track.artists.join(", ")))
  }

  /// Suspend the active decoded context with **skip** semantics (resume at the
  /// context's *next* track, position 0) and latch its `advancing` guard so the
  /// runner tick leaves it alone. Radio is torn down (a live stream can't share
  /// the sink) and its station stashed for reconnect. A no-op when no decoded
  /// context is active. Called before handing the sink to the native queue.
  pub(crate) fn suspend_active_decoded_context_for_skip(&mut self) {
    #[cfg(feature = "local-files")]
    if let Some(local) = self.local_playback.as_mut() {
      let resume_index = crate::infra::local::next_index(local.index, local.queue.len());
      local.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Local {
        resume_index,
        resume_position_ms: 0,
      });
      return;
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_mut() {
      let resume_index = crate::infra::subsonic::next_index(s.index, s.tracks.len());
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Subsonic {
        resume_index,
        resume_position_ms: 0,
      });
      return;
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_mut() {
      let resume_index = crate::infra::youtube::next_index(s.index, s.tracks.len());
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::YouTube {
        resume_index,
        resume_position_ms: 0,
      });
      return;
    }
    #[cfg(feature = "internet-radio")]
    if let Some(radio) = self.radio_playback.take() {
      radio.player.stop();
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Radio {
        station: radio.station,
      });
    }
  }

  /// Suspend the active decoded context with **mid-track** semantics (resume the
  /// *same* track at its live position) — the Enter-jump path. Radio has no
  /// seekable position, so it is stashed for reconnect like the skip path.
  pub(crate) fn suspend_active_decoded_context_mid_track(&mut self) {
    #[cfg(feature = "local-files")]
    if let Some(local) = self.local_playback.as_mut() {
      let position_ms = local.player.position().as_millis() as u64;
      let index = local.index;
      local.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Local {
        resume_index: Some(index),
        resume_position_ms: position_ms,
      });
      return;
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = self.subsonic_playback.as_mut() {
      let position_ms = s.player.position().as_millis() as u64;
      let index = s.index;
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Subsonic {
        resume_index: Some(index),
        resume_position_ms: position_ms,
      });
      return;
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = self.youtube_playback.as_mut() {
      let position_ms = s.player.position().as_millis() as u64;
      let index = s.index;
      s.advancing = true;
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::YouTube {
        resume_index: Some(index),
        resume_position_ms: position_ms,
      });
      return;
    }
    #[cfg(feature = "internet-radio")]
    if let Some(radio) = self.radio_playback.take() {
      radio.player.stop();
      self.queue_suspended = Some(crate::core::queue::SuspendedContext::Radio {
        station: radio.station,
      });
    }
  }

  /// Snapshot how to resume the underlying native-Spotify context once the
  /// native queue drains, and record it in [`Self::queue_suspended`]. Skip
  /// semantics: `resume_track_uri` is the head of the Spotify mirror queue
  /// ([`Self::queue`]) — i.e. the *next* track Spirc would have played — so the
  /// context resumes at its next track, matching Spotify's own queue behavior.
  /// Either field is `None` when the corresponding state is unknown; the resume
  /// handler degrades gracefully (context-only, or track-only, or "finished").
  #[cfg(feature = "streaming")]
  pub(crate) fn suspend_native_spotify_context_for_queue(&mut self) {
    let context_uri = self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.context.as_ref())
      .map(|c| c.uri.clone());
    let resume_track_uri = self
      .queue
      .as_ref()
      .and_then(|q| q.queue.first())
      .and_then(|item| match item {
        crate::core::plugin_api::PlayableInfo::Track(t) => t.uri.clone(),
        crate::core::plugin_api::PlayableInfo::Episode(e) => e.uri.clone(),
      });
    self.queue_suspended = Some(crate::core::queue::SuspendedContext::Spotify {
      context_uri,
      resume_track_uri,
    });
  }

  /// Suspend the native-Spotify context with **mid-track** semantics (the
  /// Enter-jump path): resume at the track that was playing when the user
  /// jumped, not the context's next one. Position is not preserved — the
  /// Spotify resume path restarts the track. Pauses the streaming player so the
  /// queued track doesn't play over it. A no-op unless native streaming is the
  /// active playback device.
  #[cfg(feature = "streaming")]
  pub(crate) fn suspend_native_spotify_context_mid_track(&mut self) {
    if !self.is_native_streaming_active_for_playback() {
      return;
    }
    let context_uri = self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.context.as_ref())
      .map(|c| c.uri.clone());
    // Resume target: the *current* item. Fall back to the mirror queue's head
    // (the context's next track) when the current item is unknown.
    let resume_track_uri = self
      .current_playback_context
      .as_ref()
      .and_then(|ctx| ctx.item.as_ref())
      .and_then(|item| match item {
        PlayableItem::Track(t) => t.id.as_ref().map(|id| id.uri()),
        PlayableItem::Episode(e) => Some(e.id.uri()),
        _ => None,
      })
      .or_else(|| {
        self
          .queue
          .as_ref()
          .and_then(|q| q.queue.first())
          .and_then(|item| match item {
            crate::core::plugin_api::PlayableInfo::Track(t) => t.uri.clone(),
            crate::core::plugin_api::PlayableInfo::Episode(e) => e.uri.clone(),
          })
      });
    self.queue_suspended = Some(crate::core::queue::SuspendedContext::Spotify {
      context_uri,
      resume_track_uri,
    });
    if let Some(player) = self.streaming_player.as_ref() {
      player.pause();
    }
  }

  /// Handle a native-streaming `EndOfTrack` while the native queue is in play.
  ///
  /// Returns `true` when the queue took over (an `AdvanceNativeQueue` was
  /// dispatched, so the caller must **not** fall back to
  /// `EnsurePlaybackContinues`), `false` to let the normal continue-playback path
  /// run. Two cases:
  ///
  /// - **A queued Spotify track just ended** (`queue_now_is_spotify`): clear the
  ///   slot *now* — before the advance is processed — so the Spirc self-advance
  ///   guard can't see the stale slot on the next `TrackChanged` and reissue the
  ///   finished track over the next item's download window. Pause librespot
  ///   (Spirc may already be loading its own next track) and advance the queue.
  /// - **A stray librespot `EndOfTrack` while a decoded queued track owns the
  ///   sink** (`queue_owns_playback` without a Spotify slot): consume it without
  ///   touching the queue — advancing would skip the audible decoded track, and
  ///   `EnsurePlaybackContinues` would resume Spotify over it.
  /// - **A context track ended with items waiting** (queue idle, non-empty):
  ///   snapshot the Spotify context for resume, `pause()` the streaming player to
  ///   preempt Spirc's own auto-advance, then advance the queue.
  #[cfg(feature = "streaming")]
  pub(crate) fn handle_native_spotify_track_end(&mut self) -> bool {
    if self.queue_now_is_spotify() {
      self.queue_now = None;
      self.spotify_queue_guard_reloads = 0;
      if let Some(player) = self.streaming_player.as_ref() {
        player.pause();
      }
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return true;
    }
    if self.queue_owns_playback() {
      return true;
    }
    if !self.native_queue.is_empty() {
      self.suspend_native_spotify_context_for_queue();
      // Preempt Spirc: after a direct `player.load`, Spirc may try to advance to
      // the next context track on its own. Pausing first stops that before the
      // queue slot takes the sink.
      if let Some(player) = self.streaming_player.as_ref() {
        player.pause();
      }
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return true;
    }
    false
  }

  /// Spirc self-advance guard for the native-Spotify queue slot.
  ///
  /// A queued Spotify track plays via a direct `player.load` (no Spirc context),
  /// so Spirc may try to advance to the next context track on its own when the
  /// queued track ends. Given the base62 id of the track librespot just switched
  /// to, this returns `Some(uri)` to reissue the queued track (Spirc fought
  /// back), or `None` to leave playback alone. Bounded by
  /// [`Self::spotify_queue_guard_reloads`] so a genuinely-gone track can't wedge
  /// a reload loop; the budget resets whenever the queued track is confirmed
  /// playing. See Risk #1 in the plan — the mitigation is pending a live
  /// experiment and cannot be verified without a real Spotify session.
  #[cfg(feature = "streaming")]
  pub(crate) fn spotify_queue_guard_reload_uri(
    &mut self,
    playing_base62_id: &str,
  ) -> Option<String> {
    let queued_uri = match self.queue_now.as_ref()? {
      #[cfg(feature = "audio-decode")]
      QueueNowPlaying::Decoded(_) => return None,
      QueueNowPlaying::Spotify { track } => track.uri.clone(),
    }?;
    let queued_id = queued_uri.rsplit(':').next().unwrap_or(queued_uri.as_str());
    if queued_id == playing_base62_id {
      // The queued track is (re)confirmed playing: clear the retry budget.
      self.spotify_queue_guard_reloads = 0;
      return None;
    }
    const MAX_RELOADS: u8 = 2;
    if self.spotify_queue_guard_reloads >= MAX_RELOADS {
      return None;
    }
    self.spotify_queue_guard_reloads += 1;
    Some(queued_uri)
  }

  /// Whether any decoded-audio source (local file, Subsonic, internet radio, or
  /// YouTube) currently owns the playback session.
  ///
  /// Starting a non-Spotify source only *pauses* librespot; it never clears
  /// `is_streaming_active` / `current_playback_context`, so
  /// [`is_native_streaming_active_for_playback`](Self::is_native_streaming_active_for_playback)
  /// stays true while a decoded source owns the rodio sink. The direct-control
  /// transport methods (next/prev/volume) use this guard to route to the active
  /// source via `IoEvent` dispatch instead of driving the paused librespot.
  ///
  /// Radio is included: routing Next/volume to radio's dispatcher (which no-ops
  /// or handles it) is still correct — we must never drive librespot while a
  /// source is playing. In a build with all source features off this reduces to
  /// `false`.
  fn active_decoded_source(&self) -> bool {
    // The native queue slot playing a decoded track owns the sink even when no
    // per-source `*_playback` context is set (e.g. queueing from an idle app).
    #[cfg(feature = "audio-decode")]
    if self.queue_now_decoded_player().is_some() {
      return true;
    }
    // A queued Spotify track owns the sink via librespot; any remaining
    // `*_playback` below is a suspended context, not the active source.
    if self.queue_now_is_spotify() {
      return false;
    }
    #[cfg(feature = "local-files")]
    if self.local_playback.is_some() {
      return true;
    }
    #[cfg(feature = "subsonic")]
    if self.subsonic_playback.is_some() {
      return true;
    }
    #[cfg(feature = "internet-radio")]
    if self.radio_playback.is_some() {
      return true;
    }
    #[cfg(feature = "youtube")]
    if self.youtube_playback.is_some() {
      return true;
    }
    false
  }

  /// The player of whichever decoded source (local file, Subsonic, internet
  /// radio, or YouTube) currently owns the session, or `None` when Spotify (or
  /// nothing) owns it. All four sources decode through the same `LocalPlayer`
  /// sink, so a single accessor covers transport/seek routing for every one.
  /// Ordering mirrors [`Self::active_decoded_source`].
  #[cfg(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  ))]
  // Consumed only by the OS media integrations (MPRIS / macOS / Windows), so
  // builds with decoded sources but none of those integrations leave it unused.
  #[cfg_attr(
    not(any(
      all(feature = "mpris", target_os = "linux"),
      all(feature = "macos-media", target_os = "macos"),
      all(feature = "windows-media", target_os = "windows")
    )),
    allow(dead_code)
  )]
  pub fn active_decoded_player(&self) -> Option<&std::sync::Arc<crate::infra::audio::LocalPlayer>> {
    #[cfg(feature = "audio-decode")]
    if let Some(p) = self.queue_now_decoded_player() {
      return Some(p);
    }
    // A queued Spotify track owns the sink via librespot; any remaining
    // `*_playback` below is a suspended context, not the active source.
    if self.queue_now_is_spotify() {
      return None;
    }
    #[cfg(feature = "local-files")]
    if let Some(s) = &self.local_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "subsonic")]
    if let Some(s) = &self.subsonic_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "internet-radio")]
    if let Some(s) = &self.radio_playback {
      return Some(&s.player);
    }
    #[cfg(feature = "youtube")]
    if let Some(s) = &self.youtube_playback {
      return Some(&s.player);
    }
    None
  }

  /// The current playback position, in milliseconds, of the active *seekable*
  /// decoded source (local file, Subsonic, or YouTube).
  ///
  /// Read live from the source player's sink. Internet radio is intentionally
  /// **excluded** — a live stream is not seekable — so radio returns `None` here
  /// and seek keys become correct no-ops for radio. In a build with all seekable
  /// source features off this reduces to `None`.
  fn active_source_position_ms(&self) -> Option<u128> {
    #[cfg(feature = "audio-decode")]
    if let Some(p) = self.queue_now_decoded_player() {
      return Some(p.position().as_millis());
    }
    // A queued Spotify track owns the sink; librespot events drive progress and
    // any remaining `*_playback` below is a suspended context.
    if self.queue_now_is_spotify() {
      return None;
    }
    #[cfg(feature = "local-files")]
    if let Some(local) = &self.local_playback {
      return Some(local.player.position().as_millis());
    }
    #[cfg(feature = "subsonic")]
    if let Some(subsonic) = &self.subsonic_playback {
      return Some(subsonic.player.position().as_millis());
    }
    #[cfg(feature = "youtube")]
    if let Some(youtube) = &self.youtube_playback {
      return Some(youtube.player.position().as_millis());
    }
    None
  }

  pub fn toggle_playback(&mut self) {
    // The native queue slot owns the sink: toggle its player directly (covers the
    // idle-app case where no per-source context is set).
    #[cfg(feature = "audio-decode")]
    if let Some(player) = self.queue_now_decoded_player() {
      if player.is_paused() {
        player.resume();
      } else {
        player.pause();
      }
      return;
    }

    // A queued Spotify track owns the sink via librespot: skip the per-source
    // arms (any still-`Some` `*_playback` is a suspended context whose paused
    // player must not be resumed on top of librespot) and fall through to the
    // native-streaming control below.
    if !self.queue_now_is_spotify() {
      // Local-file playback owns the session: toggle the local sink directly. The
      // playbar reads pause state live from the player, so nothing else to update.
      #[cfg(feature = "local-files")]
      if let Some(local) = &self.local_playback {
        if local.player.is_paused() {
          local.player.resume();
        } else {
          local.player.pause();
        }
        return;
      }

      // Subsonic playback owns the session the same way: toggle its sink directly.
      #[cfg(feature = "subsonic")]
      if let Some(subsonic) = &self.subsonic_playback {
        if subsonic.player.is_paused() {
          subsonic.player.resume();
        } else {
          subsonic.player.pause();
        }
        return;
      }

      // YouTube playback owns the session the same way: toggle its sink directly.
      #[cfg(feature = "youtube")]
      if let Some(youtube) = &self.youtube_playback {
        if youtube.player.is_paused() {
          youtube.player.resume();
        } else {
          youtube.player.pause();
        }
        return;
      }

      // Internet-radio playback owns the session the same way: toggle its sink
      // directly. Without this branch radio falls through to the streaming path,
      // which only ever emits a bare resume — so Play/Pause could resume radio but
      // never pause it.
      #[cfg(feature = "internet-radio")]
      if let Some(radio) = &self.radio_playback {
        if radio.player.is_paused() {
          radio.player.resume();
        } else {
          radio.player.pause();
        }
        return;
      }
    }

    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      if self
        .current_playback_context
        .as_ref()
        .and_then(|ctx| ctx.item.as_ref())
        .is_none()
      {
        self.dispatch(IoEvent::StartPlayback(None, None, None));
        return;
      }

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
    // The native queue owns playback: a forward-only queue has no "previous",
    // so restart the current queued track. The queue router intercepts the
    // dispatched event for both decoded and Spotify queue slots.
    if self.queue_owns_playback() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::PreviousTrack);
      return;
    }
    // A decoded source owns the session: route to its dispatcher, never to the
    // paused librespot. Preserve the ">= 3s restarts current, else previous"
    // semantics (radio no-ops both Seek and PreviousTrack).
    if self.active_decoded_source() {
      if self.song_progress_ms >= 3_000 {
        self.dispatch(IoEvent::Seek(0));
      } else {
        self.dispatch(IoEvent::PreviousTrack);
      }
      self.song_progress_ms = 0;
      return;
    }
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
    // The native queue owns playback: restart the current queued track (the
    // queue router intercepts the event for both slot kinds).
    if self.queue_owns_playback() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::ForcePreviousTrack);
      return;
    }
    // A decoded source owns the session: route to its dispatcher, never to the
    // paused librespot. The source handles or no-ops ForcePreviousTrack.
    if self.active_decoded_source() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::ForcePreviousTrack);
      return;
    }
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
    // The native queue owns playback: skip to the next queued item (or resume
    // the suspended context when the queue drains).
    if self.queue_owns_playback() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return;
    }
    // A decoded context is playing with items waiting in the queue: suspend it
    // (skip semantics — resume at the context's next track) and start the queue.
    if self.active_decoded_source() && !self.native_queue.is_empty() {
      self.suspend_active_decoded_context_for_skip();
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::AdvanceNativeQueue);
      return;
    }
    // A decoded source (local/subsonic/radio/youtube) owns the session: route to
    // its dispatcher, never to the paused librespot. The source handles or
    // no-ops NextTrack (radio has no queue).
    if self.active_decoded_source() {
      self.song_progress_ms = 0;
      self.dispatch(IoEvent::NextTrack);
      return;
    }
    // Use native streaming player for instant control (bypasses event channel latency)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback() {
      // A native-Spotify context is playing with items waiting in the queue:
      // suspend it (skip semantics) and hand the sink to the queue instead of
      // Spirc-advancing the context. (`queue_owns_playback` is already handled
      // above, so here the context, not a queued track, is playing.)
      if !self.native_queue.is_empty() {
        self.suspend_native_spotify_context_for_queue();
        if let Some(player) = self.streaming_player.as_ref() {
          player.pause();
        }
        self.song_progress_ms = 0;
        self.dispatch(IoEvent::AdvanceNativeQueue);
        return;
      }
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

  pub fn set_saved_tracks_to_table_continuous(&mut self) {
    let mut tracks = Vec::new();
    let mut expected_offset = 0;
    let mut seen_offsets = HashSet::new();
    let mut active_index = 0;

    for (page_index, page) in self.library.saved_tracks.pages.iter().enumerate() {
      if page.offset != expected_offset || !seen_offsets.insert(page.offset) {
        break;
      }

      tracks.extend(page.items.iter().cloned());
      expected_offset = expected_offset.saturating_add(page.limit);
      active_index = page_index;

      if page.next.is_none() {
        break;
      }
    }

    self.library.saved_tracks.index = active_index;
    self.replace_track_table_tracks(tracks);
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  pub fn set_playlist_tracks_to_table_continuous(&mut self) {
    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut track_ids: Vec<String> = Vec::new();
    let mut positions: Vec<usize> = Vec::new();
    let mut expected_offset = 0;
    let mut seen_offsets = HashSet::new();
    let mut active_index = 0;
    let mut active_page = None;

    for (page_index, page) in self.playlist_track_pages.pages.iter().enumerate() {
      if page.offset != expected_offset || !seen_offsets.insert(page.offset) {
        break;
      }

      for (position, item) in page.items.iter() {
        if let PlayableInfo::Track(track) = item {
          if let Some(id) = track.id.as_ref() {
            track_ids.push(id.clone());
          }
          tracks.push(track.clone());
          positions.push(*position as usize);
        }
      }

      expected_offset = expected_offset.saturating_add(page.limit);
      active_index = page_index;
      active_page = Some(page.clone());

      if page.next.is_none() {
        break;
      }
    }

    self.playlist_track_pages.index = active_index;
    self.playlist_tracks = active_page;
    self.playlist_offset = 0;
    self.replace_track_table_tracks(tracks);
    self.playlist_track_positions = Some(positions);
    self.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
  }

  pub fn reset_saved_tracks_view(&mut self) {
    self.saved_tracks_prefetch_generation = self.saved_tracks_prefetch_generation.wrapping_add(1);
    self.saved_tracks_prefetch_in_flight.clear();
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
    self.playlist_tracks_prefetch_in_flight.clear();
    self.playlist_track_table_id = Some(playlist_id);
    self.active_playlist_track_filter = None;
    self.pending_playlist_track_search = None;
    self.playlist_track_pages.clear();
    self.playlist_tracks = None;
    self.playlist_offset = 0;
    self.pending_track_table_selection = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks.clear();
    self.track_table.context = Some(context);
    self.playlist_track_positions = None;
  }

  pub fn replace_track_table_tracks(&mut self, tracks: Vec<TrackInfo>) {
    self.playlist_track_positions = None;

    let track_count = tracks.len();
    if track_count > 0 {
      if let Some(pending) = self.pending_track_table_selection.take() {
        self.track_table.selected_index = match pending {
          PendingTrackSelection::Index(index) => index.min(track_count.saturating_sub(1)),
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

  pub fn is_playlist_track_filter_active(&self) -> bool {
    self.active_playlist_track_filter.is_some()
  }

  pub fn clear_playlist_track_filter(&mut self) {
    self.active_playlist_track_filter = None;
    self.pending_playlist_track_search = None;
    self.input_context = InputContext::GlobalSearch;
    if self.playlist_track_pages.pages.is_empty() {
      self.track_table.tracks.clear();
      self.track_table.selected_index = 0;
      self.playlist_track_positions = None;
      return;
    }
    self.set_playlist_tracks_to_table_continuous();
  }

  pub fn apply_playlist_track_search_results(
    &mut self,
    playlist_id: &PlaylistId<'_>,
    query: String,
    mut matches: Vec<(FullTrack, usize)>,
  ) -> bool {
    if !self.is_playlist_track_table_active_for(playlist_id) {
      return false;
    }

    sort_playlist_track_matches(&mut matches, self.playlist_sort);

    let track_ids = matches
      .iter()
      .filter_map(|(track, _)| track.id.as_ref().map(|id| id.id().to_string()))
      .collect();
    let tracks: Vec<TrackInfo> = matches
      .iter()
      .map(|(track, _)| TrackInfo::from(track))
      .collect();
    let positions: Vec<usize> = matches.into_iter().map(|(_, position)| position).collect();

    self.active_playlist_track_filter = Some(query);
    self.pending_playlist_track_search = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks = tracks;
    self.playlist_track_positions = Some(positions);
    self.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
    true
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
      .or_else(|| {
        self
          .playlist_track_pages
          .pages
          .first()
          .map(|page| page.total)
      })
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

  pub fn next_missing_saved_tracks_offset_continuous(&self) -> Option<u32> {
    let saved_tracks_page = self
      .library
      .saved_tracks
      .get_results(Some(self.library.saved_tracks.index))?;
    saved_tracks_page.next.as_ref()?;
    Some(saved_tracks_page.offset + saved_tracks_page.limit)
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

  pub fn next_missing_playlist_tracks_offset_continuous(&self) -> Option<u32> {
    let playlist_tracks_page = self
      .playlist_track_pages
      .get_results(Some(self.playlist_track_pages.index))?;
    playlist_tracks_page.next.as_ref()?;
    Some(playlist_tracks_page.offset + playlist_tracks_page.limit)
  }

  pub fn current_playlist_has_more_tracks(&self) -> bool {
    if self.is_playlist_track_filter_active() {
      return false;
    }

    self
      .playlist_tracks
      .as_ref()
      .is_some_and(|playlist_tracks| playlist_tracks.next.is_some())
  }

  pub fn current_saved_tracks_has_more_tracks(&self) -> bool {
    self
      .library
      .saved_tracks
      .get_results(Some(self.library.saved_tracks.index))
      .is_some_and(|saved_tracks| saved_tracks.next.is_some())
  }

  pub fn selected_playlist_track_position(&self) -> Option<usize> {
    self
      .playlist_track_positions
      .as_ref()
      .and_then(|positions| positions.get(self.track_table.selected_index))
      .copied()
  }

  pub fn set_saved_artists_to_table(&mut self, saved_artists_page: &CursorPaged<ArtistInfo>) {
    self.artists = saved_artists_page.items.clone();
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
            if let Some(after) = last_artist.id.as_deref() {
              self.dispatch(IoEvent::GetFollowedArtists(Some(after.to_string())));
            }
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
    if !self.current_saved_tracks_has_more_tracks() {
      return;
    }

    if let Some(next_offset) = self.next_missing_saved_tracks_offset_continuous() {
      if self
        .library
        .saved_tracks
        .page_index_for_offset(next_offset)
        .is_some()
      {
        self.set_saved_tracks_to_table_continuous();
      } else if !self.saved_tracks_prefetch_in_flight.contains(&next_offset) {
        self.saved_tracks_prefetch_in_flight.insert(next_offset);
        self.dispatch(IoEvent::GetCurrentSavedTracks(Some(next_offset)));
      }
    }
  }

  pub fn get_playlist_tracks_next(&mut self) {
    if self.is_playlist_track_filter_active() {
      return;
    }

    let Some(playlist_id) = self.current_playlist_track_table_id() else {
      return;
    };
    if !self.current_playlist_has_more_tracks() {
      return;
    }

    if let Some(next_offset) = self.next_missing_playlist_tracks_offset_continuous() {
      if self
        .playlist_track_pages
        .page_index_for_offset(next_offset)
        .is_some()
      {
        self.set_playlist_tracks_to_table_continuous();
      } else if !self
        .playlist_tracks_prefetch_in_flight
        .contains(&next_offset)
      {
        self.playlist_tracks_prefetch_in_flight.insert(next_offset);
        self.dispatch(IoEvent::GetPlaylistItems(
          playlist_id.id().to_string(),
          next_offset,
        ));
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

    let tracks = tracks.iter().map(TrackInfo::from).collect();
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
            if let Some(ref id_str) = selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id_str.clone()));
            }
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(albums) = self.library.saved_albums.get_results(None) {
          if let Some(selected_album) = albums.items.get(self.album_list_index) {
            if let Some(id) = selected_album.album.id.as_deref() {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id.to_string()));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          if let Some(selected_album) = artist.albums.items.get(artist.selected_album_index) {
            if let Some(id_str) = &selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id_str.clone()));
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
            if let Some(ref id_str) = selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(id_str.clone()));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          if let Some(selected_album) = artist.albums.items.get(artist.selected_album_index) {
            if let Some(id_str) = &selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(id_str.clone()));
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
          self.dispatch(IoEvent::GetCurrentShowEpisodes(show_id, offset));
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
            let selected_artist = &artists.items[selected_index];
            if let Some(ref id_str) = selected_artist.id {
              self.dispatch(IoEvent::UserUnfollowArtists(vec![id_str.clone()]));
            }
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(artists) = self.library.saved_artists.get_results(None) {
          if let Some(id) = artists
            .items
            .get(self.artists_list_index)
            .and_then(|selected_artist| selected_artist.id.as_deref())
          {
            self.dispatch(IoEvent::UserUnfollowArtists(vec![id.to_string()]));
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          let selected_artis = &artist.related_artists[artist.selected_related_artist_index];
          if let Some(id_str) = &selected_artis.id {
            self.dispatch(IoEvent::UserUnfollowArtists(vec![id_str.clone()]));
          }
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
            let selected_artist = &artists.items[selected_index];
            if let Some(ref id_str) = selected_artist.id {
              self.dispatch(IoEvent::UserFollowArtists(vec![id_str.clone()]));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          let selected_artis = &artist.related_artists[artist.selected_related_artist_index];
          if let Some(id_str) = &selected_artis.id {
            self.dispatch(IoEvent::UserFollowArtists(vec![id_str.clone()]));
          }
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
      let selected_playlist = &playlists.items[selected_index];
      let selected_public = selected_playlist.public;
      if let Some(ref playlist_id_str) = selected_playlist.id {
        // owner_id carries the Spotify user id (populated in PlaylistInfo::from_simplified).
        // The network handler ignores this param (_playlist_owner_id), so a fallback
        // string is harmless — but we use the real id when available.
        let owner_id = selected_playlist
          .owner_id
          .clone()
          .unwrap_or_else(|| "unknown".to_string());
        self.dispatch(IoEvent::UserFollowPlaylist(
          owner_id,
          playlist_id_str.clone(),
          selected_public,
        ));
      }
    }
  }

  pub fn user_unfollow_playlist(&mut self) {
    info!("unfollowing playlist");
    if let (Some(selected_index), Some(user)) = (self.selected_playlist_index, &self.user) {
      if let Some(PlaylistFolderItem::Playlist { index, .. }) =
        self.get_playlist_display_item_at(selected_index)
      {
        // Pass the stored string ids straight through to the IoEvent.
        let ids = self.all_playlists.get(*index).and_then(|playlist| {
          let selected_id = playlist.id.clone()?;
          Some((user.id.clone(), selected_id))
        });
        if let Some((user_id, selected_id)) = ids {
          self.dispatch(IoEvent::UserUnfollowPlaylist(user_id, selected_id));
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
      // `user.id` is the domain string id (UserInfo) and `selected_playlist.id`
      // is an Option<String> (PlaylistInfo); both pass straight to the IoEvent.
      if let Some(ref id_str) = selected_playlist.id {
        self.dispatch(IoEvent::UserUnfollowPlaylist(
          user.id.clone(),
          id_str.clone(),
        ));
      }
    }
  }

  pub fn user_follow_show(&mut self, block: ActiveBlock) {
    info!("following show");
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows {
          if let Some(selected_index) = self.search_results.selected_shows_index {
            if let Some(show) = shows.items.get(selected_index) {
              if let Some(ref id_str) = show.id {
                self.dispatch(IoEvent::CurrentUserSavedShowAdd(id_str.clone()));
              }
            }
          }
        }
      }
      ActiveBlock::EpisodeTable => {
        if let Some(show_id) = self.selected_episode_show_id() {
          self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id));
        }
      }
      _ => (),
    }
  }

  /// Resolve the currently selected show's id/URI (from the episode-table
  /// context). Returns `None` if the stored domain show has no id.
  fn selected_episode_show_id(&self) -> Option<String> {
    match self.episode_table_context {
      EpisodeTableContext::Full => self
        .selected_show_full
        .as_ref()
        .and_then(|s| s.show.id.clone()),
      EpisodeTableContext::Simplified => self
        .selected_show_simplified
        .as_ref()
        .and_then(|s| s.show.id.clone()),
    }
  }

  pub fn user_unfollow_show(&mut self, block: ActiveBlock) {
    info!("unfollowing show");
    match block {
      ActiveBlock::Podcasts => {
        if let Some(id) = self
          .library
          .saved_shows
          .get_results(None)
          .and_then(|shows| shows.items.get(self.shows_list_index))
          .and_then(|selected_show| selected_show.id.as_deref())
        {
          self.dispatch(IoEvent::CurrentUserSavedShowDelete(id.to_string()));
        }
      }
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows {
          if let Some(selected_index) = self.search_results.selected_shows_index {
            if let Some(ref id_str) = shows.items[selected_index].id {
              self.dispatch(IoEvent::CurrentUserSavedShowDelete(id_str.clone()));
            }
          }
        }
      }
      ActiveBlock::EpisodeTable => {
        if let Some(show_id) = self.selected_episode_show_id() {
          self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
        }
      }
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
            use crate::infra::mpris::LoopStatusEvent;
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

  pub fn get_artist(&mut self, artist_id: String, input_artist_name: String) {
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetArtist(
      artist_id,
      input_artist_name,
      user_country,
    ));
  }

  pub fn get_user_country(&self) -> Option<Country> {
    // `country` is stored as its ISO 3166-1 alpha-2 string (the multi-source
    // domain holds no rspotify types); re-derive the rspotify `Country` here at
    // the boundary, the same way IDs are re-parsed when dispatching IoEvents.
    let code = self
      .user
      .as_ref()
      .and_then(|user| user.country.as_deref())?;
    serde_json::from_value(serde_json::Value::String(code.to_string())).ok()
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
          id: "behavior.animation_tick_rate_milliseconds".to_string(),
          name: "Animation Tick Rate (ms)".to_string(),
          description: "Refresh rate for animation-heavy views".to_string(),
          value: SettingValue::Number(
            self.user_config.behavior.animation_tick_rate_milliseconds as i64,
          ),
        },
        SettingItem {
          id: "behavior.status_message_ttl_percent".to_string(),
          name: "Status TTL Percent".to_string(),
          description: "Scale status message duration from 10% to 1000%".to_string(),
          value: SettingValue::Number(self.user_config.behavior.status_message_ttl_percent as i64),
        },
        SettingItem {
          id: "behavior.playback_poll_seconds".to_string(),
          name: "Playback Poll Seconds".to_string(),
          description: "Seconds between regular playback refreshes".to_string(),
          value: SettingValue::Number(self.user_config.behavior.playback_poll_seconds as i64),
        },
        SettingItem {
          id: "behavior.table_scroll_padding".to_string(),
          name: "Table Scroll Padding".to_string(),
          description: "Rows reserved while scrolling tables".to_string(),
          value: SettingValue::Number(self.user_config.behavior.table_scroll_padding as i64),
        },
        SettingItem {
          id: "behavior.like_animation_frames".to_string(),
          name: "Like Animation Frames".to_string(),
          description: "Frames used by the playbar like animation".to_string(),
          value: SettingValue::Number(self.user_config.behavior.like_animation_frames as i64),
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
          id: "behavior.group_folders_first".to_string(),
          name: "Playlist Folders First".to_string(),
          description: "List folders at the top of the Playlists tab".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.group_folders_first),
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
          id: "behavior.keepawake_enabled".to_string(),
          name: "Keep System Awake".to_string(),
          description: "Prevent the system from sleeping while music is playing".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.keepawake_enabled),
        },
        SettingItem {
          id: "behavior.enable_media_keys".to_string(),
          name: "Media Key Controls".to_string(),
          description:
            "Let OS media keys, headphone buttons, and remote controls (playerctl, Now Playing) control playback"
              .to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_media_keys),
        },
        SettingItem {
          id: "behavior.startup_behavior".to_string(),
          name: "Startup Behavior".to_string(),
          description: "Playback state when spotatui starts. Continue resumes your last session (including a saved non-Spotify track) exactly as it was; Play always starts; Pause always pauses.".to_string(),
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
          id: "behavior.startup_route".to_string(),
          name: "Startup Route".to_string(),
          description: "Screen shown when spotatui starts".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.startup_route.clone(),
            STARTUP_ROUTE_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_playlist_tracks".to_string(),
          name: "Playlist Track Sort".to_string(),
          description: "Default sort for playlist track tables".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_playlist_tracks.clone(),
            PLAYLIST_TRACK_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_saved_albums".to_string(),
          name: "Saved Album Sort".to_string(),
          description: "Default sort for saved albums".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_saved_albums.clone(),
            SAVED_ALBUM_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_saved_artists".to_string(),
          name: "Saved Artist Sort".to_string(),
          description: "Default sort for saved artists".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_saved_artists.clone(),
            SAVED_ARTIST_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_recently_played".to_string(),
          name: "Recently Played Sort".to_string(),
          description: "Default sort for recently played tracks".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_recently_played.clone(),
            RECENTLY_PLAYED_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.sidebar_position".to_string(),
          name: "Sidebar Position".to_string(),
          description: "Place the sidebar left, right, or hide it".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.sidebar_position.clone(),
            SIDEBAR_POSITION_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.playbar_position".to_string(),
          name: "Playbar Position".to_string(),
          description: "Place the playbar at the bottom or top".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.playbar_position.clone(),
            PLAYBAR_POSITION_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.small_terminal_width".to_string(),
          name: "Small Terminal Width".to_string(),
          description: "Width below which compact layout is used".to_string(),
          value: SettingValue::Number(self.user_config.behavior.small_terminal_width as i64),
        },
        SettingItem {
          id: "behavior.small_terminal_height".to_string(),
          name: "Small Terminal Height".to_string(),
          description: "Height below which compact margins are used".to_string(),
          value: SettingValue::Number(self.user_config.behavior.small_terminal_height as i64),
        },
        SettingItem {
          id: "behavior.enable_announcements".to_string(),
          name: "Remote Announcements".to_string(),
          description: "Show one-time announcements from remote JSON feed".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_announcements),
        },
        #[cfg(feature = "self-update")]
        SettingItem {
          id: "behavior.disable_auto_update".to_string(),
          name: "Disable Auto-Update".to_string(),
          description: "Skip the automatic update check on startup. Use the 'spotatui update' command to update manually.".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.disable_auto_update),
        },
        #[cfg(feature = "self-update")]
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
          id: "behavior.sync_token".to_string(),
          name: "Sync Token".to_string(),
          description: "API token from spotatui.com to sync listening history".to_string(),
          value: SettingValue::String(
            self
              .user_config
              .behavior
              .sync_token
              .clone()
              .unwrap_or_default(),
          ),
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
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "behavior.playbar_cover_art_size_percent".to_string(),
          name: "Cover Art Size".to_string(),
          description: "Playbar cover art size as a percentage (25-200)".to_string(),
          value: SettingValue::Number(
            self.user_config.behavior.playbar_cover_art_size_percent as i64,
          ),
        },
      ],
      SettingsCategory::Icons => vec![
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
        SettingItem {
          id: "behavior.gauge_filled_icon".to_string(),
          name: "Gauge Filled Icon".to_string(),
          description: "Single-cell icon for filled gauge segments".to_string(),
          value: SettingValue::String(self.user_config.behavior.gauge_filled_icon.clone()),
        },
        SettingItem {
          id: "behavior.gauge_unfilled_icon".to_string(),
          name: "Gauge Empty Icon".to_string(),
          description: "Single-cell icon for empty gauge segments".to_string(),
          value: SettingValue::String(self.user_config.behavior.gauge_unfilled_icon.clone()),
        },
        SettingItem {
          id: "behavior.active_source_icon".to_string(),
          name: "Active Source Icon".to_string(),
          description: "Icon for the active playback source".to_string(),
          value: SettingValue::String(self.user_config.behavior.active_source_icon.clone()),
        },
        SettingItem {
          id: "behavior.episode_played_icon".to_string(),
          name: "Episode Played Icon".to_string(),
          description: "Single-cell icon for fully played episodes".to_string(),
          value: SettingValue::String(self.user_config.behavior.episode_played_icon.clone()),
        },
        SettingItem {
          id: "behavior.sort_ascending_icon".to_string(),
          name: "Sort Ascending Icon".to_string(),
          description: "Single-cell icon for ascending sort".to_string(),
          value: SettingValue::String(self.user_config.behavior.sort_ascending_icon.clone()),
        },
        SettingItem {
          id: "behavior.sort_descending_icon".to_string(),
          name: "Sort Descending Icon".to_string(),
          description: "Single-cell icon for descending sort".to_string(),
          value: SettingValue::String(self.user_config.behavior.sort_descending_icon.clone()),
        },
        SettingItem {
          id: "behavior.list_highlight_icon".to_string(),
          name: "List Highlight Icon".to_string(),
          description: "Icon shown next to highlighted list rows".to_string(),
          value: SettingValue::String(self.user_config.behavior.list_highlight_icon.clone()),
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
          id: "keys.move_up".to_string(),
          name: "Move Up".to_string(),
          description: "Move selection up".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_up)),
        },
        SettingItem {
          id: "keys.move_down".to_string(),
          name: "Move Down".to_string(),
          description: "Move selection down".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_down)),
        },
        SettingItem {
          id: "keys.move_left".to_string(),
          name: "Move Left".to_string(),
          description: "Move selection left".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_left)),
        },
        SettingItem {
          id: "keys.move_right".to_string(),
          name: "Move Right".to_string(),
          description: "Move selection right".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_right)),
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
          id: "keys.remove_from_queue".to_string(),
          name: "Remove from Queue".to_string(),
          description: "Remove the selected track from the queue".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.remove_from_queue)),
        },
        SettingItem {
          id: "keys.like_track".to_string(),
          name: "Like Track".to_string(),
          description: "Toggle saved state for the currently playing track or episode"
            .to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.like_track)),
        },
        SettingItem {
          id: "keys.generate_recap".to_string(),
          name: "Generate Listening Recap".to_string(),
          description: "Generate and open the 30-day listening recap HTML card".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.generate_recap)),
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
        SettingItem {
          id: "keys.miniplayer_view".to_string(),
          name: "Miniplayer View".to_string(),
          description: "Toggle full-screen playbar view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.miniplayer_view)),
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

    let mut settings_error: Option<String> = None;
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
            self.user_config.behavior.tick_rate_milliseconds = normalize_tick_rate_milliseconds(*v);
          }
        }
        "behavior.animation_tick_rate_milliseconds" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.animation_tick_rate_milliseconds =
              normalize_tick_rate_milliseconds(*v);
          }
        }
        "behavior.status_message_ttl_percent" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.status_message_ttl_percent = (*v).clamp(10, 1000) as u16;
          }
        }
        "behavior.playback_poll_seconds" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.playback_poll_seconds = (*v).max(1) as u64;
          }
        }
        "behavior.table_scroll_padding" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.table_scroll_padding = (*v).max(0) as u16;
          }
        }
        "behavior.like_animation_frames" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.like_animation_frames = (*v).max(1) as u8;
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
        "behavior.group_folders_first" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.group_folders_first = *v;
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
        "behavior.startup_route" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.startup_route = v.clone();
          }
        }
        "behavior.default_sort_playlist_tracks" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.default_sort_playlist_tracks = v.clone();
          }
        }
        "behavior.default_sort_saved_albums" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.default_sort_saved_albums = v.clone();
          }
        }
        "behavior.default_sort_saved_artists" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.default_sort_saved_artists = v.clone();
          }
        }
        "behavior.default_sort_recently_played" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.default_sort_recently_played = v.clone();
          }
        }
        "behavior.sidebar_position" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.sidebar_position = v.clone();
          }
        }
        "behavior.playbar_position" => {
          if let SettingValue::Cycle(v, _) = &setting.value {
            self.user_config.behavior.playbar_position = v.clone();
          }
        }
        "behavior.small_terminal_width" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.small_terminal_width = (*v).max(1) as u16;
          }
        }
        "behavior.small_terminal_height" => {
          if let SettingValue::Number(v) = &setting.value {
            self.user_config.behavior.small_terminal_height = (*v).max(1) as u16;
          }
        }
        "behavior.keepawake_enabled" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.keepawake_enabled = *v;
          }
        }
        "behavior.enable_media_keys" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.enable_media_keys = *v;
          }
        }
        "behavior.enable_announcements" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.enable_announcements = *v;
          }
        }
        #[cfg(feature = "self-update")]
        "behavior.disable_auto_update" => {
          if let SettingValue::Bool(v) = &setting.value {
            self.user_config.behavior.disable_auto_update = *v;
          }
        }
        #[cfg(feature = "self-update")]
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
        "behavior.sync_token" => {
          if let SettingValue::String(v) = &setting.value {
            let trimmed = v.trim();
            self.user_config.behavior.sync_token = if trimmed.is_empty() {
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
        "behavior.gauge_filled_icon"
        | "behavior.gauge_unfilled_icon"
        | "behavior.episode_played_icon"
        | "behavior.sort_ascending_icon"
        | "behavior.sort_descending_icon" => {
          if let SettingValue::String(v) = &setting.value {
            if UnicodeWidthStr::width(v.as_str()) == 1 {
              match setting.id.as_str() {
                "behavior.gauge_filled_icon" => {
                  self.user_config.behavior.gauge_filled_icon = v.clone()
                }
                "behavior.gauge_unfilled_icon" => {
                  self.user_config.behavior.gauge_unfilled_icon = v.clone()
                }
                "behavior.episode_played_icon" => {
                  self.user_config.behavior.episode_played_icon = v.clone()
                }
                "behavior.sort_ascending_icon" => {
                  self.user_config.behavior.sort_ascending_icon = v.clone()
                }
                "behavior.sort_descending_icon" => {
                  self.user_config.behavior.sort_descending_icon = v.clone()
                }
                _ => {}
              }
            } else {
              settings_error = Some(format!(
                "{} must be exactly one terminal cell wide",
                setting.name
              ));
            }
          }
        }
        "behavior.active_source_icon" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.active_source_icon = v.clone();
          }
        }
        "behavior.list_highlight_icon" => {
          if let SettingValue::String(v) = &setting.value {
            self.user_config.behavior.list_highlight_icon = v.clone();
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
        #[cfg(feature = "cover-art")]
        "behavior.playbar_cover_art_size_percent" => {
          if let SettingValue::Number(v) = setting.value {
            self.user_config.behavior.playbar_cover_art_size_percent =
              crate::core::user_config::normalize_playbar_cover_art_size_percent(v);
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
        "keys.move_up" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.move_up = key;
            }
          }
        }
        "keys.move_down" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.move_down = key;
            }
          }
        }
        "keys.move_left" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.move_left = key;
            }
          }
        }
        "keys.move_right" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.move_right = key;
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
        "keys.remove_from_queue" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.remove_from_queue = key;
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
        "keys.generate_recap" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.generate_recap = key;
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
        "keys.miniplayer_view" => {
          if let SettingValue::Key(v) = &setting.value {
            if let Ok(key) = crate::core::user_config::parse_key_public(v.clone()) {
              self.user_config.keys.miniplayer_view = key;
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
    if let Some(message) = settings_error {
      self.set_status_message(message, 4);
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
  use crate::core::test_helpers::{playlist_info, user_info};
  use chrono::{Duration as ChronoDuration, Utc};
  use rspotify::model::{
    artist::SimplifiedArtist,
    idtypes::{PlaylistId, TrackId},
    page::Page,
    track::SavedTrack,
    SimplifiedAlbum,
  };
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

  fn queue_track(uri: Option<&str>, name: &str) -> TrackInfo {
    TrackInfo {
      uri: uri.map(|u| u.to_string()),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1000,
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

  #[test]
  fn add_track_to_native_queue_pushes_normal_track() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.add_track_to_native_queue(queue_track(Some("subsonic:track:1"), "Song"));
    assert_eq!(app.native_queue.len(), 1);
    assert_eq!(app.native_queue[0].name, "Song");
    // No Web-API dispatch for a non-Spotify item.
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn default_sort_recently_played_seeds_state_and_sorts_items() {
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.default_sort_recently_played = "name:desc".to_string();
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    assert_eq!(app.recently_played_sort.field, SortField::Name);
    assert_eq!(app.recently_played_sort.order, SortOrder::Descending);

    app.recently_played.result = Some(crate::core::pagination::CursorPaged {
      items: vec![
        queue_track(None, "Alpha"),
        queue_track(None, "Charlie"),
        queue_track(None, "Bravo"),
      ],
      limit: 3,
      next: None,
      cursor_after: None,
      total: None,
    });

    app.sort_recently_played_items();

    let names: Vec<_> = app
      .recently_played
      .result
      .as_ref()
      .unwrap()
      .items
      .iter()
      .map(|t| t.name.as_str())
      .collect();
    assert_eq!(names, vec!["Charlie", "Bravo", "Alpha"]);
  }

  #[test]
  fn add_track_to_native_queue_rejects_missing_uri() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.add_track_to_native_queue(queue_track(None, "No URI"));
    assert!(app.native_queue.is_empty());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_rejects_radio() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.add_track_to_native_queue(queue_track(Some("radio:https://example.com/live"), "Live"));
    assert!(app.native_queue.is_empty());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_spotify_no_context_pushes_natively() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // No playback context => not external => queue natively (Spotify tracks play
    // via native streaming).
    app.add_track_to_native_queue(queue_track(Some("spotify:track:abc"), "Spotify Song"));
    assert_eq!(app.native_queue.len(), 1);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn add_track_to_native_queue_spotify_external_device_dispatches_web_api() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // A Spotify context with no native streaming device reads as external.
    app.current_playback_context = Some(make_external_context());
    app.add_track_to_native_queue(queue_track(Some("spotify:track:abc"), "Spotify Song"));
    // Routed to the Web-API queue; nothing pushed to the native queue.
    assert!(app.native_queue.is_empty());
    match rx.recv().unwrap() {
      IoEvent::AddItemToQueue(uri) => assert_eq!(uri, "spotify:track:abc"),
      _ => panic!("expected AddItemToQueue dispatch"),
    }
  }

  #[allow(deprecated)]
  fn make_external_context() -> CurrentPlaybackContext {
    use rspotify::model::{
      context::Actions, CurrentlyPlayingType, Device, DeviceType, RepeatState,
    };
    CurrentPlaybackContext {
      device: Device {
        id: Some("external".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Phone".to_string(),
        _type: DeviceType::Smartphone,
        volume_percent: Some(50),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: true,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  #[cfg(feature = "streaming")]
  #[allow(deprecated)]
  fn context_playing(context_uri: &str) -> CurrentPlaybackContext {
    use rspotify::model::{context::Context, Type};
    let mut ctx = make_external_context();
    ctx.context = Some(Context {
      uri: context_uri.to_string(),
      href: String::new(),
      external_urls: HashMap::new(),
      _type: Type::Playlist,
    });
    ctx
  }

  /// The suspension snapshot records the context's uri and, as the resume target,
  /// the head of the Spotify mirror queue (the *next* track Spirc would play).
  #[cfg(feature = "streaming")]
  #[test]
  fn suspend_native_spotify_context_snapshots_context_and_next_track() {
    use crate::core::plugin_api::PlayableInfo;
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(context_playing("spotify:playlist:ctx123"));
    app.queue = Some(QueueState {
      currently_playing: Some(PlayableInfo::Track(queue_track(
        Some("spotify:track:current"),
        "Current",
      ))),
      queue: vec![
        PlayableInfo::Track(queue_track(Some("spotify:track:next1"), "Next One")),
        PlayableInfo::Track(queue_track(Some("spotify:track:next2"), "Next Two")),
      ],
    });

    app.suspend_native_spotify_context_for_queue();

    match app.queue_suspended {
      Some(SuspendedContext::Spotify {
        context_uri,
        resume_track_uri,
      }) => {
        assert_eq!(context_uri.as_deref(), Some("spotify:playlist:ctx123"));
        assert_eq!(resume_track_uri.as_deref(), Some("spotify:track:next1"));
      }
      other => panic!("expected a Spotify suspension, got {other:?}"),
    }
  }

  /// With no mirror queue or context, the snapshot degrades to all-None (the
  /// resume handler then finishes the queue rather than panicking).
  #[cfg(feature = "streaming")]
  #[test]
  fn suspend_native_spotify_context_degrades_to_none_without_state() {
    use crate::core::queue::SuspendedContext;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    app.suspend_native_spotify_context_for_queue();

    match app.queue_suspended {
      Some(SuspendedContext::Spotify {
        context_uri,
        resume_track_uri,
      }) => {
        assert!(context_uri.is_none());
        assert!(resume_track_uri.is_none());
      }
      other => panic!("expected a Spotify suspension, got {other:?}"),
    }
  }

  /// When the native queue slot owns playback, `next_track` advances the queue
  /// instead of driving the streaming player's own `next`.
  #[cfg(feature = "streaming")]
  #[test]
  fn next_track_advances_native_queue_when_queue_owns_playback() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.next_track();

    // The first dispatched event is the queue advance, not a Spotify NextTrack.
    assert!(
      matches!(rx.recv().unwrap(), IoEvent::AdvanceNativeQueue),
      "expected AdvanceNativeQueue to be dispatched first"
    );
  }

  /// When the queued Spotify track ends, the slot must be cleared *before* the
  /// advance is dispatched — a stale slot lets the Spirc self-advance guard
  /// reissue the finished track over the next item's download window (heard as
  /// "the Spotify song keeps playing while the YouTube track downloads").
  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_slot_end_clears_slot_before_advancing() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });
    app.spotify_queue_guard_reloads = 1;

    assert!(app.handle_native_spotify_track_end());

    assert!(!app.queue_owns_playback(), "the ended slot is cleared");
    assert_eq!(app.spotify_queue_guard_reloads, 0);
    assert!(
      app
        .spotify_queue_guard_reload_uri("some-other-track-id")
        .is_none(),
      "an empty slot must never reissue the finished track"
    );
    assert!(matches!(rx.recv().unwrap(), IoEvent::AdvanceNativeQueue));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_queue_slot_shadows_decoded_activity_checks() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    assert!(!app.active_decoded_source());
    assert!(app.active_source_position_ms().is_none());
  }

  #[cfg(all(feature = "streaming", feature = "audio-decode"))]
  #[test]
  fn spotify_queue_slot_shadows_decoded_player_lookup() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    assert!(app.active_decoded_player().is_none());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn toggle_playback_with_spotify_queue_slot_does_not_panic() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.toggle_playback();

    assert!(app.queue_now_is_spotify());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn previous_track_restarts_native_queue_when_queue_owns_playback() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.previous_track();

    assert!(
      matches!(rx.recv().unwrap(), IoEvent::PreviousTrack),
      "expected PreviousTrack to be dispatched for the queue router"
    );
  }

  /// The Spirc self-advance guard reissues the queued track only when Spirc has
  /// switched away from it, and only within its bounded retry budget.
  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_queue_guard_reissues_only_on_mismatch_and_within_budget() {
    use crate::infra::queue::QueueNowPlaying;
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.queue_now = Some(QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    // Same track (base62 id): no reissue, budget stays clear.
    assert_eq!(app.spotify_queue_guard_reload_uri("queued"), None);
    assert_eq!(app.spotify_queue_guard_reloads, 0);

    // Spirc switched away: reissue our track, up to the cap, then stop.
    assert_eq!(
      app.spotify_queue_guard_reload_uri("other").as_deref(),
      Some("spotify:track:queued")
    );
    assert_eq!(
      app.spotify_queue_guard_reload_uri("other").as_deref(),
      Some("spotify:track:queued")
    );
    assert_eq!(app.spotify_queue_guard_reload_uri("other"), None);
    assert_eq!(app.spotify_queue_guard_reloads, 2);

    // The queued track being confirmed playing again resets the budget.
    assert_eq!(app.spotify_queue_guard_reload_uri("queued"), None);
    assert_eq!(app.spotify_queue_guard_reloads, 0);
  }

  #[test]
  fn group_folders_first_hoists_folders_stably_and_only_when_enabled() {
    fn folder(name: &str) -> PlaylistFolderItem {
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: name.to_string(),
        current_id: 0,
        target_id: 1,
      })
    }
    fn playlist(index: usize) -> PlaylistFolderItem {
      PlaylistFolderItem::Playlist {
        index,
        current_id: 0,
      }
    }
    // Interleaved: playlist, folder A, playlist, folder B (all at root level).
    let mut app = App {
      playlist_folder_items: vec![playlist(0), folder("A"), playlist(1), folder("B")],
      ..Default::default()
    };

    // Off (default): order is untouched.
    app.user_config.behavior.group_folders_first = false;
    let names: Vec<&str> = app
      .get_playlist_display_items()
      .iter()
      .map(|i| match i {
        PlaylistFolderItem::Folder(f) => f.name.as_str(),
        PlaylistFolderItem::Playlist { .. } => "P",
      })
      .collect();
    assert_eq!(names, vec!["P", "A", "P", "B"]);

    // On: folders float to the top; both groups keep their relative order.
    app.user_config.behavior.group_folders_first = true;
    let names: Vec<&str> = app
      .get_playlist_display_items()
      .iter()
      .map(|i| match i {
        PlaylistFolderItem::Folder(f) => f.name.as_str(),
        PlaylistFolderItem::Playlist { .. } => "P",
      })
      .collect();
    assert_eq!(names, vec!["A", "B", "P", "P"]);
    // Selection index resolves against the same reordered list.
    assert!(matches!(
      app.get_playlist_display_item_at(0),
      Some(PlaylistFolderItem::Folder(_))
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn fresh_native_activity_is_true_when_native_metadata_exists() {
    let mut app = App {
      native_track_info: Some(NativeTrackInfo::default()),
      ..Default::default()
    };

    assert!(app.has_fresh_native_activity());

    app.native_track_info = None;
    assert!(!app.has_fresh_native_activity());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn fresh_native_activity_is_true_when_native_is_playing() {
    let app = App {
      native_is_playing: Some(true),
      ..Default::default()
    };

    assert!(app.has_fresh_native_activity());
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn fresh_native_activity_uses_recent_activation_window() {
    let mut app = App {
      last_device_activation: Some(Instant::now()),
      ..Default::default()
    };

    assert!(app.has_fresh_native_activity());

    app.last_device_activation = Some(Instant::now() - Duration::from_secs(6));

    assert!(!app.has_fresh_native_activity());
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

  fn saved_tracks_domain_page(
    offset: u32,
    total: u32,
    ids: &[&str],
    has_next: bool,
  ) -> Paged<TrackInfo> {
    crate::infra::network::mapping::map_page(
      &saved_tracks_page(offset, total, ids, has_next),
      |st| TrackInfo::from(&st.track),
    )
  }

  fn empty_playlist_page(
    offset: u32,
    total: u32,
    limit: u32,
    has_next: bool,
  ) -> Paged<(u32, PlayableInfo)> {
    Paged {
      items: vec![],
      limit,
      next: has_next.then(|| "https://example.com/playlists/test/items?next".to_string()),
      offset,
      previous: None,
      total,
    }
  }

  fn playlist_page(
    offset: u32,
    total: u32,
    ids: &[&str],
    has_next: bool,
  ) -> Paged<(u32, PlayableInfo)> {
    Paged {
      items: ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
          let position = offset + index as u32;
          let track = PlayableInfo::Track(TrackInfo::from(&full_track(
            id,
            &format!("Track {offset}-{index}"),
          )));
          (position, track)
        })
        .collect(),
      limit: ids.len() as u32,
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
    pages.add_pages(saved_tracks_domain_page(
      0,
      4,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    ));

    let inserted_index = pages.upsert_page_by_offset(saved_tracks_domain_page(
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
    pages.add_pages(saved_tracks_domain_page(
      0,
      2,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    ));

    let replaced_index = pages.upsert_page_by_offset(saved_tracks_domain_page(
      0,
      2,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    ));

    assert_eq!(replaced_index, 0);
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(
      pages.pages[0].items[0].id.as_deref().unwrap(),
      "0000000000000000000003"
    );
  }

  #[test]
  fn upsert_page_by_offset_keeps_active_page_when_inserting_before_it() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_domain_page(
      0,
      6,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    ));
    pages.add_pages(saved_tracks_domain_page(
      4,
      6,
      &["0000000000000000000005", "0000000000000000000006"],
      false,
    ));
    pages.index = 1;

    let inserted_index = pages.upsert_page_by_offset(saved_tracks_domain_page(
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
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.saved_tracks_prefetch_generation = 7;
    let saved_tracks_domain_page = crate::infra::network::mapping::map_page(
      &saved_tracks_page(
        0,
        2,
        &["0000000000000000000001", "0000000000000000000002"],
        false,
      ),
      |st| TrackInfo::from(&st.track),
    );
    app.library.saved_tracks.add_pages(saved_tracks_domain_page);
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
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
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
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
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
        assert_eq!(id, playlist_id.id());
        assert_eq!(offset, 20);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn playlist_next_uses_cached_adjacent_page_before_fetching() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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

    assert_eq!(app.playlist_offset, 0);
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
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn playlist_continuous_table_stops_at_sparse_cache_gap() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = playlist_page(
      0,
      6,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let sparse_page = playlist_page(
      4,
      6,
      &["0000000000000000000005", "0000000000000000000006"],
      false,
    );

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.playlist_track_pages.upsert_page_by_offset(first_page);
    app.playlist_track_pages.upsert_page_by_offset(sparse_page);

    app.set_playlist_tracks_to_table_continuous();

    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1]));
    match rx.recv().unwrap() {
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        assert_eq!(track_ids.len(), 2);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn playlist_next_cached_page_applies_pending_continuous_index() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = playlist_page(
      0,
      4,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let second_page = playlist_page(
      2,
      4,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    );

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app
      .playlist_track_pages
      .upsert_page_by_offset(first_page.clone());
    app.playlist_track_pages.upsert_page_by_offset(second_page);
    app.playlist_tracks = Some(first_page);
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];
    app.track_table.selected_index = 1;
    app.pending_track_table_selection = Some(PendingTrackSelection::Index(2));

    app.get_playlist_tracks_next();

    assert_eq!(app.track_table.tracks.len(), 4);
    assert_eq!(app.track_table.selected_index, 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1, 2, 3]));
  }

  #[test]
  fn playlist_search_results_preserve_source_positions_and_handle_no_matches() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app.pending_playlist_track_search = Some("track".to_string());

    assert!(app.apply_playlist_track_search_results(
      &playlist_id,
      "track".to_string(),
      vec![
        (full_track("0000000000000000000002", "Second"), 8),
        (full_track("0000000000000000000004", "Fourth"), 11),
      ],
    ));

    assert_eq!(app.active_playlist_track_filter, Some("track".to_string()));
    assert!(app.pending_playlist_track_search.is_none());
    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![8, 11]));
    match rx.recv().unwrap() {
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        assert_eq!(track_ids.len(), 2);
      }
      _ => panic!("unexpected event"),
    }

    assert!(app.apply_playlist_track_search_results(&playlist_id, "none".to_string(), vec![]));
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.playlist_track_positions, Some(vec![]));
  }

  #[test]
  fn clearing_playlist_search_restores_cached_continuous_view() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let page = playlist_page(
      0,
      2,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.playlist_track_pages.upsert_page_by_offset(page);
    app.active_playlist_track_filter = Some("second".to_string());
    app.track_table.tracks = vec![TrackInfo::from(&full_track(
      "0000000000000000000002",
      "Second",
    ))];
    app.playlist_track_positions = Some(vec![1]);

    app.clear_playlist_track_filter();

    assert!(app.active_playlist_track_filter.is_none());
    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1]));
  }

  #[test]
  fn apply_sorted_playlist_tracks_if_current_requires_matching_playlist_identity_and_context() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let sidebar_playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");
    let active_playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let original_track = full_track("0000000000000000000001", "Original");

    app.track_table.tracks = vec![TrackInfo::from(&original_track)];
    app.track_table.context = Some(TrackTableContext::PlaylistSearch);
    app.playlist_track_table_id = Some(active_playlist_id.clone());

    assert!(!app.apply_sorted_playlist_tracks_if_current(
      &sidebar_playlist_id,
      vec![full_track("0000000000000000000002", "Wrong Playlist")],
    ));
    assert_eq!(
      app.track_table.tracks[0].id.as_deref(),
      original_track.id.as_ref().map(|id| id.id())
    );

    app.track_table.context = Some(TrackTableContext::SavedTracks);
    assert!(!app.apply_sorted_playlist_tracks_if_current(
      &active_playlist_id,
      vec![full_track("0000000000000000000003", "Wrong Context")],
    ));
    assert_eq!(
      app.track_table.tracks[0].id.as_deref(),
      original_track.id.as_ref().map(|id| id.id())
    );
  }

  #[test]
  fn editable_playlists_include_owned_and_collaborative_only() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.all_playlists = vec![
      playlist_info("37i9dQZF1DXcBWIGoYBM5M", "Owned", "spotatui-owner", false),
      playlist_info(
        "37i9dQZF1DX4WYpdgoIcn6",
        "Collaborative",
        "friend-owner",
        true,
      ),
      playlist_info("37i9dQZF1DWZqd5JICZI0u", "Followed", "friend-owner", false),
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
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.playlists = Some(Paged {
      total: 1,
      ..Default::default()
    });
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DWZqd5JICZI0u",
      "Followed",
      "friend-owner",
      false,
    )];

    app.begin_add_track_to_playlist_flow(
      Some("0000000000000000000001".to_string()),
      "Track".to_string(),
    );

    assert_eq!(
      app.status_message.as_deref(),
      Some("No editable playlists available")
    );
    assert!(app.pending_playlist_track_add.is_none());
  }

  // --- status message priority tests ---

  fn make_app_simple() -> App {
    let (tx, _rx) = channel();
    App::new(tx, UserConfig::new(), Some(SystemTime::now()))
  }

  // Regression for transport-4: with no Spotify device volume and no pending
  // volume (the state while a decoded source plays, and the whole slim build),
  // `desired_volume` must fall back to the configured volume, not 0. The old
  // `.unwrap_or(0)` made volume-down a dead no-op and the first volume-up snap
  // to the increment. This is the only hardware-free guard for that fix — a
  // source-active transport test needs a real audio device (see report).
  #[test]
  fn desired_volume_falls_back_to_config_when_no_context() {
    let mut app = make_app_simple();
    app.current_playback_context = None;
    app.pending_volume = None;
    app.user_config.behavior.volume_percent = 42;

    assert_eq!(
      app.desired_volume(),
      42,
      "with no device volume and no pending volume, base volume must come from config, not 0"
    );
  }

  #[test]
  fn normal_message_does_not_overwrite_live_error() {
    let mut app = make_app_simple();
    app.set_error_status_message("plugin error", 6);
    assert!(app.status_message_is_error);

    app.set_status_message("now playing", 4);

    assert_eq!(app.status_message.as_deref(), Some("plugin error"));
    assert!(app.status_message_is_error);
  }

  #[test]
  fn error_overwrites_normal_message() {
    let mut app = make_app_simple();
    app.set_status_message("now playing", 4);
    assert!(!app.status_message_is_error);

    app.set_error_status_message("plugin error", 6);

    assert_eq!(app.status_message.as_deref(), Some("plugin error"));
    assert!(app.status_message_is_error);
  }

  #[test]
  fn error_overwrites_previous_error() {
    let mut app = make_app_simple();
    app.set_error_status_message("first error", 6);
    app.set_error_status_message("second error", 6);

    assert_eq!(app.status_message.as_deref(), Some("second error"));
    assert!(app.status_message_is_error);
  }

  #[test]
  fn normal_message_accepted_after_error_expires() {
    let mut app = make_app_simple();
    app.set_error_status_message("plugin error", 6);

    // Simulate expiry by backdating the timestamp.
    app.status_message_expires_at = Some(Instant::now() - Duration::from_secs(1));

    app.set_status_message("now playing", 4);

    assert_eq!(app.status_message.as_deref(), Some("now playing"));
    assert!(!app.status_message_is_error);
  }

  #[test]
  fn current_route_playlist_track_table_requires_track_table_route() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    assert!(app.is_playlist_track_table_active_for(&playlist_id));
    assert!(!app.is_current_route_playlist_track_table_for(&playlist_id));

    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    assert!(app.is_current_route_playlist_track_table_for(&playlist_id));
  }

  #[test]
  fn poll_current_playback_skips_when_spotify_disconnected() {
    let (tx, rx) = channel();
    // No Spotify session (free-source launch): spotify_connected == false.
    let mut app = App::new(tx, UserConfig::new(), None);
    assert!(!app.spotify_connected);
    // Force the poll interval to have elapsed so only the connection gate matters.
    app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(10);

    app.poll_current_playback();

    // Nothing dispatched: no per-tick "connect Spotify" auth-spam for free sources.
    assert!(rx.try_recv().is_err());
    assert!(!app.is_fetching_current_playback);
  }

  #[test]
  fn poll_current_playback_dispatches_when_spotify_connected() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    assert!(app.spotify_connected);
    app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(10);

    app.poll_current_playback();

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetCurrentPlayback)));
    assert!(app.is_fetching_current_playback);
  }
}
