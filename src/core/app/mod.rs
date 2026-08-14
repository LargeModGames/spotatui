use crate::core::geometry::Viewport;
use crate::core::input::Key;
use crate::core::pagination::{CursorPaged, Paged};
use crate::core::plugin_api::{
  ArtistInfo, EpisodeInfo, PlayableInfo, PlaylistInfo, SavedAlbumInfo, ShowInfo, TrackInfo,
};
use crate::core::sort::{SortContext, SortField, SortOrder, SortState};
use crate::core::source::Source;
use crate::core::state::{
  PersistedRuntimeState, RadioStationAddOutcome, RadioStationConfig, RuntimeState,
};
use crate::core::user_config::{color_to_string, normalize_tick_rate_milliseconds, UserConfig};
use crate::infra::history::{RecapPeriod, StatsData, StreakSummary};
use crate::infra::network::sync::{PartySession, PartyStatus};
use crate::infra::network::IoEvent;
#[cfg(any(
  feature = "streaming",
  feature = "local-files",
  feature = "subsonic",
  feature = "youtube"
))]
use crate::infra::queue::QueueNowPlaying;
use anyhow::anyhow;
use rspotify::{
  model::enums::Country,
  model::{
    context::CurrentPlaybackContext, device::DevicePayload, idtypes::PlaylistId, track::FullTrack,
    PlayableItem,
  },
  prelude::*, // Adds Id trait for .id() method
};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
// Bare `Arc` here is only ever named by the streaming player, the MPRIS manager,
// and the decoded queue-slot accessors below (whose gate is the queueable
// sources, not `audio-decode` — a radio-only build has no queue slot).
#[cfg(any(
  feature = "streaming",
  feature = "local-files",
  feature = "subsonic",
  feature = "youtube",
  all(feature = "mpris", target_os = "linux")
))]
use std::sync::Arc;
use std::{
  cmp::{max, min},
  collections::HashSet,
  time::{Duration, Instant, SystemTime},
};
use unicode_width::UnicodeWidthStr;

use arboard::Clipboard;
#[cfg(any(test, feature = "streaming"))]
use chrono::Utc;
use log::info;
#[cfg(feature = "streaming")]
use rspotify::model::{
  context::Actions,
  device::Device,
  enums::{CurrentlyPlayingType, RepeatState},
  DeviceType,
};

use crate::infra::queue::RepeatMode;

#[cfg(test)]
use crate::core::test_helpers::{playlist_info, user_info};
#[cfg(test)]
use chrono::Duration as ChronoDuration;
#[cfg(test)]
use rspotify::model::{
  artist::SimplifiedArtist, idtypes::TrackId, page::Page, track::SavedTrack, SimplifiedAlbum,
};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::mpsc::channel;

mod album_theme;
mod construction;
mod discover;
mod dj;
mod friends;
mod help;
mod keybindings;
mod library;
mod lyrics;
mod models;
mod native_backend;
mod native_recovery;
mod native_shuffle;
mod persistence;
mod playback_routing;
mod playlist_folders;
mod playlist_pages;
mod playlists;
mod plugins;
mod queue;
mod queue_suspend;
mod route;
mod scrollable_pages;
mod seek;
mod settings_apply;
mod settings_schema;
mod shuffle_repeat;
mod status;
mod tick;
mod transport;
mod volume;

#[cfg(test)]
mod test_support;

pub use discover::*;
pub use friends::*;
pub use help::*;
pub use keybindings::*;
pub use library::*;
pub use lyrics::*;
pub use models::*;
pub use native_backend::*;
#[cfg(feature = "streaming")]
pub use native_recovery::*;
#[cfg(feature = "streaming")]
pub(crate) use native_shuffle::*;
pub use playlist_folders::*;
pub use playlists::*;
pub use plugins::*;
pub use queue::*;
pub use route::*;
pub use scrollable_pages::*;
pub use seek::*;
pub use settings_schema::*;
pub use status::*;

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
  pub spectrum_data: Option<crate::infra::audio::SpectrumData>,
  pub audio_capture_active: bool,
  pub home_scroll: u16,
  pub user_config: UserConfig,
  pub runtime_state: RuntimeState,
  pub state_path: Option<PathBuf>,
  pub artists: Vec<crate::core::plugin_api::ArtistInfo>,
  pub artist: Option<Artist>,
  pub album_table_context: AlbumTableContext,
  pub saved_album_tracks_index: usize,
  /// The live error recorded by [`App::handle_error`], or empty when there is
  /// none. The `RouteId::Error` frame raised alongside it is a presentation
  /// *hint*, not the state: a frontend may draw it full-screen, render this
  /// string as a toast, or ignore it. Dismiss via [`App::clear_api_error`].
  pub api_error: String,
  /// When the live `api_error` stops being current. Stamped by `handle_error`,
  /// consumed by `App::expire_api_error` on the tick. `None` means none is live.
  pub api_error_expires_at: Option<Instant>,
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
  /// track — i.e. native streaming or a source with a finite track list, which
  /// excludes internet radio — and every read goes through the unconditional
  /// [`Self::queue_owns_playback`] accessor.
  #[cfg(any(
    feature = "streaming",
    feature = "local-files",
    feature = "subsonic",
    feature = "youtube"
  ))]
  pub queue_now: Option<crate::infra::queue::QueueNowPlaying>,
  /// Bounded retry guard for the native-Spotify queue slot. When a queued
  /// Spotify track is playing via a direct `player.load` (no Spirc context) and
  /// Spirc self-advances to a different track, the player-event handler reissues
  /// the queued track and increments this. Reset to 0 each time a new Spotify
  /// queue slot is published; capped so a genuinely-gone track can't loop.
  #[cfg(feature = "streaming")]
  pub spotify_queue_guard_reloads: u8,
  /// Whether the published Spotify queue slot should be playing. Set on
  /// publish, flipped by the slot's pause/resume transport arms. Unlike
  /// `native_is_playing` this survives a backend teardown, so a recovery
  /// replay of the slot can honor a user's pause.
  #[cfg(feature = "streaming")]
  pub queue_slot_desired_playing: bool,
  /// Decoded cover art for the current track plus its load status. The TUI
  /// renderer (`tui::cover_art`) caches its terminal protocols on this
  /// store's key.
  #[cfg(feature = "art-decode")]
  pub cover_art: crate::core::art::CoverArtStore,
  /// Image key currently desired by the UI. Detached decode results for older
  /// keys are discarded.
  #[cfg(feature = "art-decode")]
  pub desired_cover_art_key: Option<String>,
  /// Accent colors extracted from the current cover art. Stored even while
  /// Adaptive Theme is off, so toggling it on recolors immediately without a
  /// refetch. Cleared together with the art itself.
  #[cfg(feature = "art-decode")]
  pub cover_art_palette: Option<crate::core::cover_theme::AlbumPalette>,
  /// Whether an album-derived theme is applied, and the user theme to restore.
  #[cfg(feature = "art-decode")]
  pub cover_theme_state: crate::core::cover_theme::CoverThemeState,
  /// In-flight fade of the live theme, advanced by `update_on_tick`.
  #[cfg(feature = "art-decode")]
  pub theme_transition: Option<crate::core::cover_theme::ThemeTransition>,
  /// AI DJ session state: transcript, auto-queue toggle, vibe, and the
  /// generation counter that invalidates in-flight background work.
  #[cfg(feature = "dj-core")]
  pub dj: crate::infra::dj::DjState,
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
  /// Liked-state lookups pending for the detached contains worker, as bare
  /// base62 track ids (deduped). The `CurrentUserSavedTracksContains` handler
  /// enqueues here instead of resolving on the serial IoEvent pump.
  pub liked_lookup_pending: HashSet<String>,
  /// Whether the single detached liked-lookup worker is currently draining
  /// [`Self::liked_lookup_pending`].
  pub liked_lookup_worker_running: bool,
  /// Bumped on every local like/unlike so a detached liked-state read that
  /// started before the mutation is re-read instead of clobbering it.
  pub liked_state_epoch: u64,
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
  pub size: Viewport,
  #[allow(dead_code)]
  pub small_search_limit: u32,
  pub song_progress_ms: u128,
  /// When `update_on_tick` last ran. Every frontend must drive the tick (see
  /// `core::driver`); `playback_position_ms` reports stale past 2s so one
  /// that stops ticking shows a loud error instead of a frozen playbar.
  pub last_tick_at: Instant,
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
  /// Last time a decoded-source seek was dispatched (for throttling drags)
  pub last_source_seek: Option<Instant>,
  /// Pending decoded-source seek position (throttled; drags coalesce to the last target)
  pub pending_source_seek: Option<u32>,
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
  /// Text filter applied to the rows in the Help menu.
  pub help_filter: String,
  /// Whether typed keys are currently editing [`Self::help_filter`].
  pub help_filter_editing: bool,
  /// Formatted Help rows for the current width/keys/filter; `None` until the
  /// Help menu is first prepared for rendering.
  pub help_menu_model: Option<HelpMenuModel>,
  pub is_loading: bool,
  io_tx: Option<Sender<IoEvent>>,
  pub is_fetching_current_playback: bool,
  /// Expiry of the current Spotify access token, or `None` when there is no
  /// Spotify session (launched against a free source). The token-refresh poll
  /// in the driver skips refreshing while this is `None`.
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
  /// Title/artist pair whose lyrics response is currently desired. Detached
  /// service responses must match this before mutating visible state.
  pub desired_lyrics_identity: Option<(String, String)>,
  /// Scroll/browse state for the lyrics view.
  pub lyrics_view: LyricsViewState,
  /// Whether the current `lyrics` carry real LRC timestamps rather than
  /// synthesized evenly-spaced ones derived from plain lyrics.
  pub lyrics_synced: bool,
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
  /// The app-owned native-Spotify shuffle session, when client-side shuffle
  /// owns the current native playback (see [`NativeSpotifyShuffleSession`]).
  #[cfg(feature = "streaming")]
  pub native_spotify_shuffle: Option<NativeSpotifyShuffleSession>,
  /// Monotonic generation for [`Self::native_spotify_shuffle`]; bumped on every
  /// session create/clear so stale background fetches are discarded.
  #[cfg(feature = "streaming")]
  pub native_shuffle_generation: u64,
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
  /// Period shown on the Stats screen
  pub stats_period: RecapPeriod,
  /// Whether we're currently loading stats data
  pub stats_loading: bool,
  /// Selected index in the Stats screen's Top Tracks list
  pub stats_selected_track: usize,
  /// Aggregated listening stats for the Stats screen
  pub stats_data: Option<StatsData>,
  /// Cached listening streak summary (Home strip + Stats screen)
  pub listening_streaks: Option<StreakSummary>,
  /// Pending monthly recap popup (path + listen count)
  pub recap_prompt: Option<RecapPromptState>,
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
  /// Folder ID the add-to-playlist picker dialog is viewing (0 = root)
  pub playlist_picker_folder_id: usize,
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
  /// Backing storage for the injected community-playlist pin so display methods
  /// can hand out a `&PlaylistFolderItem`. Never stored in
  /// `playlist_folder_items`.
  pub community_pin_item: PlaylistFolderItem,
  /// Current folder ID being viewed (0 = root)
  pub current_playlist_folder_id: usize,
  /// Incremented every time playlists are refreshed to guard stale background tasks
  pub playlist_refresh_generation: u64,
  /// Incremented every time the saved tracks view is reloaded to guard stale prefetch tasks
  pub saved_tracks_prefetch_generation: u64,
  pub saved_tracks_prefetch_in_flight: HashSet<u32>,
  /// Incremented every time the playlist track table is reloaded to guard stale prefetch tasks
  pub playlist_tracks_prefetch_generation: u64,
  pub playlist_tracks_prefetch_in_flight: HashSet<u32>,
  /// Playlist ids whose full track list is being fetched for sorting. Further
  /// sort changes reuse the active fetch and apply the newest sort at completion.
  pub playlist_sort_fetch_in_flight: HashSet<String>,
  /// Playlist id whose page-1 open fetch is in flight, so spamming Enter on a
  /// playlist doesn't queue duplicate full fetches. Cleared when the response
  /// (or its error) lands.
  pub pending_playlist_open: Option<String>,
  /// Tracks whether a ChangeVolume request is on its way to Spotify.
  /// When true, we hold off on sending another one — rapid key presses
  /// just update `pending_volume` and the latest value wins.
  pub is_volume_change_in_flight: bool,
  /// Deadline for a debounced state save scheduled by auto-repeating runtime
  /// state changes such as volume and shuffle.
  pub state_save_due: Option<Instant>,
  /// Runtime-state patch waiting for the next debounced save.
  pub pending_state_save_patch: PersistedRuntimeState,
  /// Whether the current run of state-save failures has already been reported.
  /// A failed flush re-arms its own deadline, so the tick retries twice a
  /// second; without this latch the report would re-fire at that rate and pin
  /// the status bar into error mode, where `set_status_message` refuses to
  /// overwrite it and every ordinary message is dropped for the rest of the run.
  pub state_save_error_reported: bool,
  /// Reference to the native streaming player for direct control (bypasses event channel)
  #[cfg(feature = "streaming")]
  pub streaming_player: Option<Arc<crate::infra::player::StreamingPlayer>>,
  /// Player-global repeat mode for the decoded (non-Spotify) sources
  /// (Local / Subsonic / YouTube). Decoded sources have no Spotify
  /// `current_playback_context`, so their repeat lives here instead of in
  /// `current_playback_context.repeat_state`. Consulted by the auto-advance /
  /// skip logic and rendered in the source playbar. Radio (an infinite stream)
  /// and the native queue ignore it.
  pub decoded_repeat: RepeatMode,
  /// Player-global shuffle flag for the decoded (non-Spotify) sources. When set,
  /// the owning source's own queue is reordered in place (see `ShuffleBackup`),
  /// keeping the current track playing.
  pub decoded_shuffle: bool,
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
  /// A StartPlayback request parked while no usable backend exists (native
  /// session recovering, or startup init still materializing one). Replayed
  /// once the backend is ready so the user's press isn't silently dropped.
  #[cfg(feature = "streaming")]
  pub pending_start_playback: Option<PendingStartPlayback>,
  /// True while a native backend may materialize soon (recovery in flight, or
  /// the deferred startup init still running). While set, a playback request
  /// that finds no active device parks itself for replay instead of routing
  /// to the full-screen error.
  #[cfg(feature = "streaming")]
  pub native_backend_pending: bool,
  /// Armed when a native load is issued; a Playing/TrackChanged event disarms
  /// it. If it fires, the session is a zombie (passes `is_connected` but
  /// silently drops Spirc commands) and recovery is forced.
  #[cfg(feature = "streaming")]
  pub native_load_watchdog: Option<Instant>,
  /// Durable native playback intent, independent of the current librespot
  /// Session/Spirc instance. Used to restore the exact track or in-flight
  /// transition after a transport-level recovery.
  #[cfg(feature = "streaming")]
  pub native_playback_recovery: Option<NativePlaybackRecoverySnapshot>,
  /// A restore command has been issued for this generation and is waiting for
  /// a matching Playing/Paused event from the replacement player.
  #[cfg(feature = "streaming")]
  pub native_restore_pending: Option<NativePlaybackRestoreAttempt>,
  #[cfg(feature = "streaming")]
  native_playback_generation: u64,
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
  /// Per-domain write counters driving async plugin data reads (see
  /// [`PluginDataKind`]). Ungated: the network layer bumps them in every build;
  /// only the scripting engine reads them.
  pub plugin_data_generations: PluginDataGenerations,
  /// Retained content of plugin-registered custom screens, keyed by screen
  /// name. Written by script effects; read by the draw loop.
  pub plugin_screens:
    std::collections::BTreeMap<String, crate::core::plugin_api::PluginScreenContent>,
  /// Keys pressed while a plugin screen was focused: `(screen_name, key_string)`,
  /// drained by the script engine after each key event.
  pub pending_plugin_screen_keys: Vec<(String, String)>,
  /// Vertical scroll for the focused plugin screen.
  pub plugin_screen_scroll: u16,
  /// Per-plugin playbar segments, keyed by plugin name (BTreeMap for deterministic order).
  pub plugin_playbar_segments: std::collections::BTreeMap<String, String>,
  /// Currently displayed plugin popup, if any.
  pub plugin_popup: Option<crate::core::plugin_api::PluginPopup>,
  /// Scroll offset for the plugin popup.
  pub plugin_popup_scroll: u16,
  /// Where this run's log file is being written, resolved once here so draw
  /// code can show it without doing the environment lookup every frame.
  pub log_path: String,
}

impl App {
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
        log::error!("Error from dispatch {}", e);
        // TODO: handle error
      };
    }
  }

  /// [`Self::dispatch`] without setting `is_loading`.
  ///
  /// For work with its own progress surface. A DJ brain call runs for up to
  /// `dj_agent_timeout_secs` (minutes), and `dispatch` would pin the global
  /// spinner until the service-lane task finishes — the exact UX bug
  /// `DjState::thinking` exists to avoid, and the reason the MCP executor sends
  /// straight down the channel instead of dispatching.
  #[cfg(feature = "ai-dj")]
  pub fn dispatch_without_spinner(&self, action: IoEvent) {
    if let Some(io_tx) = &self.io_tx {
      if let Err(e) = io_tx.send(action) {
        log::warn!("dispatch_without_spinner failed (shutting down?): {e}");
      }
    }
  }

  // Close the IO channel to allow the network thread to exit gracefully
  pub fn close_io_channel(&mut self) {
    self.io_tx = None;
  }
}
