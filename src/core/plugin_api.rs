//! Plugin-facing domain facade.
//!
//! Serde-serializable snapshots with string IDs/URIs only. rspotify types must never leak
//! through this boundary — that is the compatibility contract for the future scripting API
//! and multi-source refactor. All conversions from rspotify types happen in the mapping
//! functions below; callers receive only the plain structs defined here.

// Nothing in the main binary calls this API yet; Phase 1 will wire it up.
#![allow(dead_code)]

use crate::core::app::App;
use crate::infra::media_metadata::current_playback_snapshot;
use rspotify::model::RepeatState;
use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 5;

/// A popup dialog produced by a plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginPopup {
  pub title: String,
  pub lines: Vec<PopupLine>,
}

/// A single line in a [`PluginPopup`].
#[derive(Debug, Clone, PartialEq)]
pub struct PopupLine {
  pub text: String,
  pub fg: Option<ratatui::style::Color>,
  pub bold: bool,
  pub italic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
  pub uri: Option<String>,
  pub name: String,
  /// Display artist names. Mirrors `artist_refs[*].name`; retained for the
  /// `api_version = 4` scripting contract (plugins read `track.artists`).
  pub artists: Vec<String>,
  /// Display album name. Retained for the scripting contract.
  pub album: String,
  pub duration_ms: u64,
  // --- Fields below are additive (post-Phase-0). They only ever ADD keys to the
  // serialized snapshot, so the `api_version = 4` plugin contract is preserved. ---
  /// Spotify base62 track id (`None` for local/unknown tracks).
  #[serde(default)]
  pub id: Option<String>,
  /// Spotify base62 id of the track's album, when known.
  #[serde(default)]
  pub album_id: Option<String>,
  /// Structured, navigable artist references (id + name). Populated when the
  /// source provides per-artist data; empty when only a combined display string
  /// is available (e.g. native-playback snapshots).
  #[serde(default)]
  pub artist_refs: Vec<ArtistRef>,
  #[serde(default = "default_true")]
  pub is_playable: bool,
  #[serde(default)]
  pub is_local: bool,
  #[serde(default)]
  pub track_number: u32,
  #[serde(default)]
  pub explicit: bool,
  /// A directly-fetchable cover-art image URL for this track, when the source
  /// can provide one (Subsonic getCoverArt, YouTube thumbnail). `None` for
  /// sources without per-track art. Additive: only adds a key to the serialized
  /// snapshot, preserving the api_version = 4 plugin contract.
  #[serde(default)]
  pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackState {
  pub track: Option<TrackInfo>,
  pub is_playing: bool,
  pub progress_ms: u64,
  pub shuffle: bool,
  /// One of `"off"`, `"track"`, or `"context"`.
  pub repeat: String,
  pub volume_percent: Option<u8>,
  pub device: Option<DeviceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
  pub id: Option<String>,
  pub name: String,
  /// Lowercased DeviceType name, e.g. `"computer"`, `"smartphone"`, `"speaker"`.
  pub kind: String,
  pub is_active: bool,
  pub volume_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistInfo {
  pub uri: String,
  pub name: String,
  /// Owner display name (falls back to the owner id when no display name is set).
  pub owner: String,
  pub track_count: u32,
  // --- Additive fields (post-Phase-0); see TrackInfo note above. ---
  /// Spotify base62 playlist id, when known.
  #[serde(default)]
  pub id: Option<String>,
  /// Owner's base62 user id, when known. Used for ownership checks (the display
  /// `owner` is not stable for comparison).
  #[serde(default)]
  pub owner_id: Option<String>,
  #[serde(default)]
  pub collaborative: bool,
  #[serde(default)]
  pub public: Option<bool>,
  #[serde(default)]
  pub image_url: Option<String>,
}

/// A navigable reference to an artist: optional Spotify id plus display name.
///
/// Reused by [`TrackInfo`], [`AlbumInfo`], and [`ArtistInfo`]. `id` is `None`
/// for local/unknown sources or when the API omits it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ArtistRef {
  pub id: Option<String>,
  pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ArtistInfo {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  #[serde(default)]
  pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AlbumInfo {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  #[serde(default)]
  pub artists: Vec<ArtistRef>,
  /// One of `"album"`, `"single"`, `"compilation"` (lowercased), when known.
  #[serde(default)]
  pub album_type: Option<String>,
  #[serde(default)]
  pub release_date: Option<String>,
  #[serde(default)]
  pub total_tracks: Option<u32>,
  #[serde(default)]
  pub image_url: Option<String>,
  /// Populated when mapped from a full album; empty for simplified albums.
  #[serde(default)]
  pub tracks: Vec<TrackInfo>,
}

/// An album as it appears in the user's saved-albums library: the album itself
/// plus the saved-relationship metadata (`added_at`). `added_at` is a property
/// of the *save*, not the album, so it lives here rather than on [`AlbumInfo`]
/// (which is also reused for search results and discographies). Carried so the
/// "Date Added" sort on the saved-albums screen keeps working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SavedAlbumInfo {
  pub album: AlbumInfo,
  /// RFC 3339 UTC timestamp; sorts lexicographically == chronologically.
  #[serde(default)]
  pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ShowInfo {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  #[serde(default)]
  pub description: String,
  /// Show publisher (the "by …" attribution rendered in the podcast list).
  #[serde(default)]
  pub publisher: String,
  #[serde(default)]
  pub image_url: Option<String>,
}

/// Where playback of an episode was last left off. Mirrors rspotify's
/// `ResumePoint`; carried so the episode list can render the "fully played"
/// marker and resume position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumePointInfo {
  pub fully_played: bool,
  pub resume_position_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeInfo {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  pub duration_ms: u64,
  /// Parent show name. Populated from a full episode (e.g. a queue item);
  /// empty for simplified episodes that are already shown within their show's
  /// context (the show-episodes list).
  #[serde(default)]
  pub show_name: String,
  #[serde(default)]
  pub description: String,
  #[serde(default)]
  pub release_date: String,
  #[serde(default = "default_true")]
  pub is_playable: bool,
  /// Resume/played state, when the source provides it (e.g. the show-episodes
  /// list). Drives the "fully played" marker and resume-position display.
  #[serde(default)]
  pub resume_point: Option<ResumePointInfo>,
  #[serde(default)]
  pub image_url: Option<String>,
}

/// A playable item in a queue or playlist: either a music track or a podcast
/// episode. Maps from rspotify's `PlayableItem` (the `Unknown` variant maps to
/// `None`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlayableInfo {
  Track(TrackInfo),
  Episode(EpisodeInfo),
}

/// Aggregated, source-agnostic search results. Sources without a given
/// capability simply leave the corresponding vector empty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SearchResults {
  #[serde(default)]
  pub tracks: Vec<TrackInfo>,
  #[serde(default)]
  pub albums: Vec<AlbumInfo>,
  #[serde(default)]
  pub artists: Vec<ArtistInfo>,
  #[serde(default)]
  pub playlists: Vec<PlaylistInfo>,
  #[serde(default)]
  pub shows: Vec<ShowInfo>,
}

/// Retained content of a plugin custom screen. Screens are retained-mode by
/// design: draw runs with `&App` only (the engine is unreachable there), so
/// plugins publish content via effects and the renderer just reads it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PluginScreenContent {
  pub title: String,
  pub widgets: Vec<PluginWidget>,
}

/// A widget in a [`PluginScreenContent`]. Fixed-height widgets take their
/// requested rows; the remaining vertical space is split evenly between the
/// rest.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginWidget {
  Paragraph {
    lines: Vec<PopupLine>,
    height: Option<u16>,
  },
  List {
    title: Option<String>,
    items: Vec<PopupLine>,
    /// 0-based index of the highlighted item.
    selected: Option<usize>,
    height: Option<u16>,
  },
  Gauge {
    /// 0.0..=1.0 (clamped at the API layer).
    ratio: f64,
    label: Option<String>,
  },
}

/// A queue item as exposed to plugins. [`PlayableInfo`]'s externally-tagged
/// serde shape (`{ Track = {...} }`) is hostile to Lua, so queue items are
/// flattened into an explicit `kind` plus at most one populated payload field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueItemSnapshot {
  /// `"track"` or `"episode"`.
  pub kind: String,
  #[serde(default)]
  pub track: Option<TrackInfo>,
  #[serde(default)]
  pub episode: Option<EpisodeInfo>,
}

impl QueueItemSnapshot {
  pub fn from_playable(item: &PlayableInfo) -> Self {
    match item {
      PlayableInfo::Track(t) => QueueItemSnapshot {
        kind: "track".to_string(),
        track: Some(t.clone()),
        episode: None,
      },
      PlayableInfo::Episode(e) => QueueItemSnapshot {
        kind: "episode".to_string(),
        track: None,
        episode: Some(e.clone()),
      },
    }
  }
}

/// The playback queue as exposed to plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QueueSnapshot {
  #[serde(default)]
  pub currently_playing: Option<QueueItemSnapshot>,
  #[serde(default)]
  pub items: Vec<QueueItemSnapshot>,
}

/// A single timestamped lyrics line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricsLineSnapshot {
  pub time_ms: u64,
  pub text: String,
}

/// Lyrics for the current track as exposed to plugins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LyricsSnapshot {
  /// One of `"not_started"`, `"loading"`, `"found"`, `"not_found"`.
  pub status: String,
  #[serde(default)]
  pub lines: Vec<LyricsLineSnapshot>,
}

/// Safe, plugin-visible behavior settings. Secrets and service credentials
/// (sync_token, relay_server_url, subsonic credentials, discord client id,
/// announcement feeds) are deliberately excluded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BehaviorSnapshot {
  pub seek_milliseconds: u32,
  pub volume_increment: u8,
  pub tick_rate_milliseconds: u64,
  pub animation_tick_rate_milliseconds: u64,
  pub enable_text_emphasis: bool,
  pub show_loading_indicator: bool,
  pub enforce_wide_search_bar: bool,
  pub liked_icon: String,
  pub shuffle_icon: String,
  pub repeat_track_icon: String,
  pub repeat_context_icon: String,
  pub playing_icon: String,
  pub paused_icon: String,
  pub set_window_title: bool,
  pub shuffle_enabled: bool,
  pub stop_after_current_track: bool,
  pub sidebar_width_percent: u8,
  pub playbar_height_rows: u16,
  pub library_height_percent: u8,
  /// Lowercased active source name, e.g. `"spotify"`, `"local"`.
  pub active_source: String,
}

/// User configuration as exposed to plugins: theme colors as config-file
/// strings plus safe behavior scalars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ConfigSnapshot {
  pub theme: std::collections::BTreeMap<String, String>,
  pub behavior: BehaviorSnapshot,
}

/// Build a [`ConfigSnapshot`] from the live user config. Theme colors use the
/// same string forms `parse_theme_item` accepts (named or `"r, g, b"`).
pub fn config_snapshot(config: &crate::core::user_config::UserConfig) -> ConfigSnapshot {
  use crate::core::user_config::color_to_string;
  let t = &config.theme;
  let theme: std::collections::BTreeMap<String, String> = [
    ("active", t.active),
    ("analysis_bar", t.analysis_bar),
    ("analysis_bar_text", t.analysis_bar_text),
    ("banner", t.banner),
    ("background", t.background),
    ("error_border", t.error_border),
    ("error_text", t.error_text),
    ("header", t.header),
    ("highlighted_lyrics", t.highlighted_lyrics),
    ("hint", t.hint),
    ("hovered", t.hovered),
    ("inactive", t.inactive),
    ("playbar_background", t.playbar_background),
    ("playbar_progress", t.playbar_progress),
    ("playbar_progress_text", t.playbar_progress_text),
    ("playbar_text", t.playbar_text),
    ("selected", t.selected),
    ("text", t.text),
  ]
  .into_iter()
  .map(|(name, color)| (name.to_string(), color_to_string(color)))
  .collect();

  let b = &config.behavior;
  ConfigSnapshot {
    theme,
    behavior: BehaviorSnapshot {
      seek_milliseconds: b.seek_milliseconds,
      volume_increment: b.volume_increment,
      tick_rate_milliseconds: b.tick_rate_milliseconds,
      animation_tick_rate_milliseconds: b.animation_tick_rate_milliseconds,
      enable_text_emphasis: b.enable_text_emphasis,
      show_loading_indicator: b.show_loading_indicator,
      enforce_wide_search_bar: b.enforce_wide_search_bar,
      liked_icon: b.liked_icon.clone(),
      shuffle_icon: b.shuffle_icon.clone(),
      repeat_track_icon: b.repeat_track_icon.clone(),
      repeat_context_icon: b.repeat_context_icon.clone(),
      playing_icon: b.playing_icon.clone(),
      paused_icon: b.paused_icon.clone(),
      set_window_title: b.set_window_title,
      shuffle_enabled: b.shuffle_enabled,
      stop_after_current_track: b.stop_after_current_track,
      sidebar_width_percent: b.sidebar_width_percent,
      playbar_height_rows: b.playbar_height_rows,
      library_height_percent: b.library_height_percent,
      active_source: format!("{:?}", b.active_source).to_lowercase(),
    },
  }
}

/// Default for serde `is_playable` fields: a track/episode is assumed playable
/// unless the API explicitly says otherwise.
fn default_true() -> bool {
  true
}

// ---------------------------------------------------------------------------
// Mapping helpers
// ---------------------------------------------------------------------------

impl PlaybackState {
  pub fn repeat_from(state: RepeatState) -> String {
    match state {
      RepeatState::Off => "off".to_string(),
      RepeatState::Track => "track".to_string(),
      RepeatState::Context => "context".to_string(),
    }
  }
}

impl DeviceInfo {
  pub fn from_rspotify(device: &rspotify::model::Device) -> Self {
    DeviceInfo {
      id: device.id.clone(),
      name: device.name.clone(),
      kind: format!("{:?}", device._type).to_lowercase(),
      is_active: device.is_active,
      volume_percent: device.volume_percent.map(|v| v.min(100) as u8),
    }
  }
}

impl PlaylistInfo {
  pub fn from_simplified(p: &rspotify::model::SimplifiedPlaylist) -> Self {
    use rspotify::prelude::Id;
    let owner = p
      .owner
      .display_name
      .clone()
      .unwrap_or_else(|| p.owner.id.id().to_string());
    PlaylistInfo {
      uri: p.id.uri(),
      name: p.name.clone(),
      owner,
      track_count: p.items.total,
      id: Some(p.id.id().to_string()),
      owner_id: Some(p.owner.id.id().to_string()),
      collaborative: p.collaborative,
      public: p.public,
      image_url: p.images.first().map(|img| img.url.clone()),
    }
  }
}

/// Build a [`PlaybackState`] from the current [`App`] state.
///
/// Returns `None` only when there is no playback context at all (both
/// `current_playback_snapshot` and `app.current_playback_context` are absent).
pub fn playback_state(app: &App) -> Option<PlaybackState> {
  let snapshot = current_playback_snapshot(app);
  let context = app.current_playback_context.as_ref();

  if snapshot.is_none() && context.is_none() {
    return None;
  }

  let track = snapshot.as_ref().map(|s| TrackInfo {
    uri: s.item_uri.clone(),
    name: s.metadata.title.clone(),
    artists: s.metadata.artists.clone(),
    album: s.metadata.album.clone(),
    duration_ms: s.metadata.duration_ms as u64,
    id: s.item_id.clone(),
    album_id: None,
    // The native-playback snapshot carries a single combined artist display
    // string (see `media_metadata`), not structured per-artist data, so there
    // are no navigable refs to populate here.
    artist_refs: Vec::new(),
    is_playable: true,
    is_local: false,
    track_number: 0,
    explicit: false,
    image_url: None,
  });

  let (is_playing, shuffle, repeat, device) = if let Some(s) = &snapshot {
    let repeat_str = s
      .repeat
      .map(PlaybackState::repeat_from)
      .unwrap_or_else(|| "off".to_string());
    let device = context.map(|ctx| DeviceInfo::from_rspotify(&ctx.device));
    (s.is_playing, s.shuffle, repeat_str, device)
  } else {
    // snapshot is None but context is Some — build from context only
    let ctx = context.unwrap();
    let repeat_str = PlaybackState::repeat_from(ctx.repeat_state);
    let device = Some(DeviceInfo::from_rspotify(&ctx.device));
    (ctx.is_playing, ctx.shuffle_state, repeat_str, device)
  };

  let volume_percent = device.as_ref().and_then(|d| d.volume_percent);

  let progress_ms = snapshot.as_ref().map(|s| s.progress_ms as u64).unwrap_or(0);

  Some(PlaybackState {
    track,
    is_playing,
    progress_ms,
    shuffle,
    repeat,
    volume_percent,
    device,
  })
}

/// Return a list of available devices from [`App`]'s cached device payload.
pub fn device_list(app: &App) -> Vec<DeviceInfo> {
  app
    .devices
    .as_ref()
    .map(|payload| {
      payload
        .devices
        .iter()
        .map(DeviceInfo::from_rspotify)
        .collect()
    })
    .unwrap_or_default()
}

/// Stable plugin-facing name of a route. Exhaustive on purpose (no `_` arm):
/// adding a `RouteId` without naming it here is a compile error, keeping the
/// scripting contract in sync.
pub fn route_name(route: &crate::core::app::Route) -> String {
  use crate::core::app::RouteId;
  match &route.id {
    RouteId::Analysis => "analysis",
    RouteId::AlbumTracks => "album_tracks",
    RouteId::AlbumList => "album_list",
    RouteId::Artist => "artist",
    RouteId::LyricsView => "lyrics",
    RouteId::CoverArtView => "cover_art",
    RouteId::MiniPlayer => "miniplayer",
    RouteId::Error => "error",
    RouteId::Home => "home",
    RouteId::RecentlyPlayed => "recently_played",
    RouteId::Search => "search",
    RouteId::SelectedDevice => "devices",
    RouteId::TrackTable => "track_table",
    RouteId::Discover => "discover",
    RouteId::Artists => "artists",
    RouteId::Podcasts => "podcasts",
    RouteId::PodcastEpisodes => "podcast_episodes",
    RouteId::Recommendations => "recommendations",
    RouteId::Dialog => "dialog",
    RouteId::AnnouncementPrompt => "announcement",
    RouteId::RecapPrompt => "recap_prompt",
    RouteId::ExitPrompt => "exit_prompt",
    RouteId::Settings => "settings",
    RouteId::HelpMenu => "help",
    RouteId::Queue => "queue",
    RouteId::Party => "party",
    RouteId::CreatePlaylist => "create_playlist",
    RouteId::Friends => "friends",
    RouteId::LocalBrowser => "local_browser",
    RouteId::Stats => "stats",
    RouteId::PluginScreen(name) => return format!("plugin:{name}"),
  }
  .to_string()
}

// ---------------------------------------------------------------------------
// Snapshot serializers for plugin data reads
// ---------------------------------------------------------------------------

/// The user's playlists (full list, folder structure flattened away).
pub fn playlists_snapshot(app: &App) -> Vec<PlaylistInfo> {
  app.all_playlists.clone()
}

/// Saved ("liked") tracks fetched so far, in library order.
pub fn saved_tracks_snapshot(app: &App) -> Vec<TrackInfo> {
  app
    .library
    .saved_tracks
    .pages
    .iter()
    .flat_map(|page| page.items.iter().cloned())
    .collect()
}

/// Saved albums fetched so far, in library order.
pub fn saved_albums_snapshot(app: &App) -> Vec<SavedAlbumInfo> {
  app
    .library
    .saved_albums
    .pages
    .iter()
    .flat_map(|page| page.items.iter().cloned())
    .collect()
}

/// Saved shows fetched so far, in library order.
pub fn saved_shows_snapshot(app: &App) -> Vec<ShowInfo> {
  app
    .library
    .saved_shows
    .pages
    .iter()
    .flat_map(|page| page.items.iter().cloned())
    .collect()
}

/// Recently played tracks (most recent first, as returned by the API).
pub fn recently_played_snapshot(app: &App) -> Vec<TrackInfo> {
  app
    .recently_played
    .result
    .as_ref()
    .map(|page| page.items.clone())
    .unwrap_or_default()
}

/// The Spotify playback queue. An unavailable queue (no active device) maps to
/// an empty snapshot rather than an error.
pub fn queue_snapshot(app: &App) -> QueueSnapshot {
  let Some(queue) = app.queue.as_ref() else {
    return QueueSnapshot::default();
  };
  QueueSnapshot {
    currently_playing: queue
      .currently_playing
      .as_ref()
      .map(QueueItemSnapshot::from_playable),
    items: queue
      .queue
      .iter()
      .map(QueueItemSnapshot::from_playable)
      .collect(),
  }
}

/// The current search results (first page of each category).
pub fn search_results_snapshot(app: &App) -> SearchResults {
  SearchResults {
    tracks: app
      .search_results
      .tracks
      .as_ref()
      .map(|p| p.items.clone())
      .unwrap_or_default(),
    albums: app
      .search_results
      .albums
      .as_ref()
      .map(|p| p.items.clone())
      .unwrap_or_default(),
    artists: app
      .search_results
      .artists
      .as_ref()
      .map(|p| p.items.clone())
      .unwrap_or_default(),
    playlists: app
      .search_results
      .playlists
      .as_ref()
      .map(|p| p.items.clone())
      .unwrap_or_default(),
    shows: app
      .search_results
      .shows
      .as_ref()
      .map(|p| p.items.clone())
      .unwrap_or_default(),
  }
}

/// Lyrics for the current track, with the fetch status spelled out.
pub fn lyrics_snapshot(app: &App) -> LyricsSnapshot {
  use crate::core::app::LyricsStatus;
  let status = match app.lyrics_status {
    LyricsStatus::NotStarted => "not_started",
    LyricsStatus::Loading => "loading",
    LyricsStatus::Found => "found",
    LyricsStatus::NotFound => "not_found",
  };
  LyricsSnapshot {
    status: status.to_string(),
    lines: app
      .lyrics
      .as_ref()
      .map(|lines| {
        lines
          .iter()
          .map(|(time_ms, text)| LyricsLineSnapshot {
            time_ms: *time_ms as u64,
            text: text.clone(),
          })
          .collect()
      })
      .unwrap_or_default(),
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    CurrentlyPlayingType, Device, DeviceType, RepeatState,
  };
  use std::{sync::mpsc::channel, time::SystemTime};

  fn make_app() -> App {
    let (tx, _rx) = channel();
    App::new(
      tx,
      crate::core::user_config::UserConfig::new(),
      Some(SystemTime::now()),
    )
  }

  #[allow(deprecated)]
  fn make_device(volume: u32) -> Device {
    Device {
      id: Some("dev-abc".to_string()),
      is_active: true,
      is_private_session: false,
      is_restricted: false,
      name: "Living Room TV".to_string(),
      _type: DeviceType::Tv,
      volume_percent: Some(volume),
    }
  }

  #[allow(deprecated)]
  fn make_playback_context_no_item(
    is_playing: bool,
    shuffle: bool,
    repeat: RepeatState,
    device: Device,
  ) -> CurrentPlaybackContext {
    CurrentPlaybackContext {
      device,
      repeat_state: repeat,
      shuffle_state: shuffle,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Unknown,
      actions: Actions::default(),
    }
  }

  // --- DeviceInfo::from_rspotify ---

  #[test]
  fn device_info_maps_all_fields_and_lowercases_kind() {
    let d = make_device(75);
    let info = DeviceInfo::from_rspotify(&d);
    assert_eq!(info.id.as_deref(), Some("dev-abc"));
    assert_eq!(info.name, "Living Room TV");
    assert_eq!(info.kind, "tv");
    assert!(info.is_active);
    assert_eq!(info.volume_percent, Some(75));
  }

  #[test]
  fn device_info_computer_kind() {
    #[allow(deprecated)]
    let d = Device {
      id: None,
      is_active: false,
      is_private_session: false,
      is_restricted: false,
      name: "Laptop".to_string(),
      _type: DeviceType::Computer,
      volume_percent: Some(50),
    };
    let info = DeviceInfo::from_rspotify(&d);
    assert_eq!(info.kind, "computer");
    assert_eq!(info.volume_percent, Some(50));
    assert!(info.id.is_none());
    assert!(!info.is_active);
  }

  #[test]
  fn device_info_volume_clamped_to_u8() {
    // volume_percent is u32; values > 255 should not cause panic (min(100) ensures <= 100).
    #[allow(deprecated)]
    let d = Device {
      id: None,
      is_active: false,
      is_private_session: false,
      is_restricted: false,
      name: "X".to_string(),
      _type: DeviceType::Smartphone,
      volume_percent: Some(100),
    };
    let info = DeviceInfo::from_rspotify(&d);
    assert_eq!(info.volume_percent, Some(100));
  }

  // --- repeat_from ---

  #[test]
  fn repeat_off_maps_to_string() {
    assert_eq!(PlaybackState::repeat_from(RepeatState::Off), "off");
  }

  #[test]
  fn repeat_track_maps_to_string() {
    assert_eq!(PlaybackState::repeat_from(RepeatState::Track), "track");
  }

  #[test]
  fn repeat_context_maps_to_string() {
    assert_eq!(PlaybackState::repeat_from(RepeatState::Context), "context");
  }

  // --- playback_state ---

  #[test]
  fn playback_state_returns_none_on_default_app() {
    let app = make_app();
    assert!(playback_state(&app).is_none());
  }

  #[test]
  fn playback_state_with_context_no_item_returns_some_with_track_none() {
    let mut app = make_app();
    let device = make_device(60);
    app.current_playback_context = Some(make_playback_context_no_item(
      true,
      true,
      RepeatState::Context,
      device,
    ));

    let state = playback_state(&app).expect("should be Some");
    assert!(state.track.is_none());
    assert!(state.is_playing);
    assert!(state.shuffle);
    assert_eq!(state.repeat, "context");
    assert_eq!(state.volume_percent, Some(60));
    let dev = state.device.as_ref().expect("device should be present");
    assert_eq!(dev.id.as_deref(), Some("dev-abc"));
    assert_eq!(dev.name, "Living Room TV");
    assert_eq!(dev.kind, "tv");
  }

  // --- PlaylistInfo::from_simplified ---

  #[test]
  fn playlist_info_maps_all_fields() {
    let playlist = crate::core::test_helpers::simplified_playlist(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Today's Top Hits",
      "spotify",
      false,
    );
    let info = PlaylistInfo::from_simplified(&playlist);
    assert_eq!(info.uri, "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M");
    assert_eq!(info.name, "Today's Top Hits");
    // test_helpers::simplified_playlist sets owner display_name = owner_id
    assert_eq!(info.owner, "spotify");
    assert_eq!(info.owner_id.as_deref(), Some("spotify"));
    assert_eq!(info.track_count, 5);
  }

  #[test]
  fn playlist_info_falls_back_to_owner_id_when_no_display_name() {
    use rspotify::model::{
      idtypes::{PlaylistId, UserId},
      playlist::PlaylistTracksRef,
      user::PublicUser,
    };
    use std::collections::HashMap;

    #[allow(deprecated)]
    let playlist = rspotify::model::SimplifiedPlaylist {
      collaborative: false,
      external_urls: HashMap::new(),
      href: "https://api.spotify.com/v1/playlists/abc".to_string(),
      id: PlaylistId::from_id("37i9dQZF1DXcBWIGoYBM5M")
        .unwrap()
        .into_static(),
      images: Vec::new(),
      name: "Chill Vibes".to_string(),
      owner: PublicUser {
        display_name: None,
        external_urls: HashMap::new(),
        followers: None,
        href: "https://api.spotify.com/v1/users/spotifyuser".to_string(),
        id: UserId::from_id("spotifyuser").unwrap().into_static(),
        images: Vec::new(),
      },
      public: None,
      snapshot_id: "snap".to_string(),
      tracks: PlaylistTracksRef {
        href: "https://api.spotify.com/v1/playlists/abc/tracks".to_string(),
        total: 10,
      },
      items: PlaylistTracksRef {
        href: "https://api.spotify.com/v1/playlists/abc/tracks".to_string(),
        total: 10,
      },
    };
    let info = PlaylistInfo::from_simplified(&playlist);
    assert_eq!(info.owner, "spotifyuser");
    assert_eq!(info.owner_id.as_deref(), Some("spotifyuser"));
    assert_eq!(info.track_count, 10);
  }
}
