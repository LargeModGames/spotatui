//! The JSON the server and the page exchange over the socket. A channel message
//! carries that channel's whole state; a page applies it when its `rev` is at
//! least the last one it applied for that channel. Hello's revisions are informational.

use crate::core::action::Action;
use crate::core::app::{App, DiscoverShelf, DisplayDomain, DisplayRevisions, SessionPlay};
use crate::core::first_run::compiled_in_sources;
use crate::core::playlist_sync::Unmatched;
use crate::core::plugin_api::{
  device_list, recently_played_snapshot, route_name, AlbumInfo, ArtistInfo, DeviceInfo,
  EpisodeInfo, PlaybackState, PlaylistInfo, QueueItemSnapshot, QueueSnapshot, ShowInfo, TrackInfo,
};
use crate::core::source::Source;
use crate::core::theme::{resolve, Color, Palette, Theme, ThemeField};
use crate::core::update::{UpdateAsk, UpdateStatus};
use crate::gui::onboarding::{OnboardingReply, OnboardingView};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
  Hello {
    payload: HelloPayload,
  },
  /// The first-launch questions; sent on connect and on every change until boot is done.
  Onboarding {
    payload: OnboardingView,
  },
  /// The playback position in ms, or `null` while the tick loop is stale.
  Tick {
    payload: Option<u64>,
  },
  Playback {
    rev: u64,
    payload: PlaybackPayload,
  },
  Queue {
    rev: u64,
    payload: Box<QueuePayload>,
  },
  /// The listening party this app hosts or follows.
  Party {
    rev: u64,
    payload: PartyPayload,
  },
  Status {
    rev: u64,
    payload: StatusPayload,
  },
  /// The browse scope; never the playing source.
  Source {
    rev: u64,
    payload: SourcePayload,
  },
  /// The sidebar lists of every source.
  Library {
    rev: u64,
    payload: Box<SourcePlaylists>,
  },
  /// Liked Songs, on its own channel: the page cache grows page by page.
  Liked {
    rev: u64,
    payload: Box<LikedSongs>,
  },
  /// Field name to RGB; `null` is `Reset`, the page's own default.
  Theme {
    rev: u64,
    payload: BTreeMap<String, Option<[u8; 3]>>,
  },
  Devices {
    rev: u64,
    payload: Vec<DeviceInfo>,
  },
  /// The last search's hits, from whichever source ran it.
  Search {
    rev: u64,
    payload: Box<SearchPayload>,
  },
  Route {
    rev: u64,
    payload: String,
  },
  /// The listening stats of the selected period, from local history.
  Stats {
    rev: u64,
    payload: Box<StatsPayload>,
  },
  /// The lyrics of the playing track, fetched on every track change.
  Lyrics {
    rev: u64,
    payload: Box<LyricsPayload>,
  },
  /// The album the app last fetched for its album page.
  Album {
    rev: u64,
    payload: Box<AlbumPayload>,
  },
  /// The plays finished since this process started, oldest first.
  Session {
    rev: u64,
    payload: Vec<SessionPlay>,
  },
  /// The shelves of Spotify's Made For You page.
  Discover {
    rev: u64,
    payload: Box<DiscoverPayload>,
  },
  /// The playlist-sync links and the run state.
  PlaylistSync {
    rev: u64,
    payload: Box<PlaylistSyncPayload>,
  },
  /// The rows of the shared track table: an opened playlist, folder or listing.
  TrackTable {
    rev: u64,
    payload: Box<TrackTablePayload>,
  },
  /// The artist the app last fetched for its artist page.
  Artist {
    rev: u64,
    payload: Box<ArtistPayload>,
  },
  /// Spotify's recently played tracks, most recent first.
  RecentlyPlayed {
    rev: u64,
    payload: Vec<TrackInfo>,
  },
  /// The liked Spotify track ids among the rows the page can show: the track
  /// table, the album, the artist's top tracks, recently played and Liked Songs.
  LikedMarks {
    rev: u64,
    payload: Vec<String>,
  },
  /// The settings rows the page offers, in display order.
  Settings {
    rev: u64,
    payload: Vec<SettingRow>,
  },
  /// The desktop shell's updater; `null` without one.
  Update {
    rev: u64,
    payload: Option<UpdateStatus>,
  },
  /// The podcast show the app last fetched for its show page.
  Show {
    rev: u64,
    payload: Box<ShowPayload>,
  },
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SettingRow {
  id: String,
  /// `playback`, `streaming`, `integrations`, `library`, `sources` or `updates`.
  group: &'static str,
  name: String,
  description: String,
  value: SettingRowValue,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SettingRowValue {
  Bool {
    value: bool,
  },
  Number {
    value: i64,
  },
  Text {
    value: String,
  },
  /// Set with a `Text` input that names one of `options`.
  Choice {
    value: String,
    options: Vec<String>,
  },
  /// A password, set with a `Text` input; the page never gets it back.
  Secret {
    set: bool,
  },
}

/// The Behavior rows the page offers: (id, group, a description for the desktop app).
/// The other rows only mean something in a terminal.
const GUI_SETTINGS: &[(&str, &str, Option<&str>)] = &[
  ("behavior.seek_milliseconds", "playback", None),
  ("behavior.volume_increment", "playback", None),
  ("behavior.stop_after_current_track", "playback", None),
  ("behavior.keepawake_enabled", "playback", None),
  ("behavior.enable_media_keys", "playback", None),
  ("behavior.enable_streaming", "streaming", None),
  ("behavior.streaming_device_name", "streaming", None),
  ("behavior.streaming_bitrate", "streaming", None),
  ("behavior.streaming_audio_cache", "streaming", None),
  ("behavior.streaming_autoplay", "streaming", None),
  #[cfg(feature = "discord-rpc")]
  ("behavior.enable_discord_rpc", "integrations", None),
  ("behavior.enable_global_song_count", "integrations", None),
  ("behavior.enable_lyrics", "integrations", None),
  ("behavior.default_sort_playlist_tracks", "library", None),
  ("behavior.default_sort_saved_albums", "library", None),
  ("behavior.default_sort_saved_artists", "library", None),
  ("behavior.subsonic_url", "sources", None),
  ("behavior.subsonic_username", "sources", None),
  ("behavior.subsonic_password", "sources", None),
  ("behavior.local_music_path", "sources", None),
  ("behavior.ytdlp_path", "sources", None),
  ("behavior.qobuz_quality", "sources", None),
  (
    "behavior.disable_auto_update",
    "updates",
    Some("Skip the checks at launch and every 6 hours; check, download and install updates from this page yourself"),
  ),
  (
    "behavior.auto_update_delay",
    "updates",
    Some("How long a release must be out before it downloads: '0' for at once, or for example '10m', '2h', '7d'"),
  ),
];

/// The page's rows a phone leaves out: keyboard steps, a desktop sleep lock, and the player it always runs.
const PHONE_HIDDEN_SETTINGS: &[&str] = &[
  "behavior.seek_milliseconds",
  "behavior.volume_increment",
  "behavior.keepawake_enabled",
  "behavior.enable_streaming",
];

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct HelloPayload {
  version: &'static str,
  /// Present only on the connect that traded a launch code.
  token: Option<String>,
  revisions: DisplayRevisions,
  /// A desktop shell can shrink its window to the mini player.
  window: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct PlaybackPayload {
  item: Option<NowPlaying>,
  volume: u32,
  device: Option<String>,
  liked: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct NowPlaying {
  title: String,
  artists: Vec<String>,
  album: String,
  image_url: Option<String>,
  duration_ms: u32,
  uri: Option<String>,
  is_playing: bool,
  is_live: bool,
  shuffle: bool,
  repeat: String,
  /// The Spotify play context (album, playlist or artist); `null` for a raw list, a queue slot or another source.
  context_uri: Option<String>,
  /// The play context's name for "Playing from": Spotify's own, else the library playlist, the album, Liked Songs or the context's artist; `null` when unknown or when `context_uri` is `null`.
  context_name: Option<String>,
  /// The delivered Qobuz stream quality, such as `FLAC 24/96`; `null` for every other source.
  quality: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct QueuePayload {
  spotify: QueueSnapshot,
  native: Vec<TrackInfo>,
  now: Option<TrackInfo>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct StatusPayload {
  message: Option<String>,
  is_error: bool,
  api_error: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SourcePayload {
  active: Source,
  /// The sources compiled into this build, in display order.
  compiled: Vec<Source>,
}

/// Liked Songs cached from the top with no gap; `LoadMore(SavedTracks)` fetches the next page.
#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct LikedSongs {
  tracks: Vec<TrackInfo>,
  /// The library's total as Spotify reports it; 0 before the first page.
  total: u32,
  has_more: bool,
  /// False without a Spotify session: the list cannot load.
  available: bool,
  /// False until the first page landed; an empty list before that is still loading.
  loaded: bool,
}

/// The track table rows; `uri` names the playlist they belong to, and is `null` while it loads.
#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct TrackTablePayload {
  uri: Option<String>,
  tracks: Vec<TrackInfo>,
  /// `LoadMore(PlaylistTracks)` fetches the next page.
  has_more: bool,
  /// Each row's position in its Spotify playlist, for `RemoveTrackFromPlaylist`; `null` for any other table.
  positions: Option<Vec<usize>>,
  /// The seed track id when the rows are that track's radio.
  radio: Option<String>,
}

/// A playlist folder of the Spotify rootlist, or a playlist inside one.
#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum LibraryNode {
  Folder {
    name: String,
    uri: String,
    children: Vec<LibraryNode>,
  },
  Playlist {
    uri: String,
  },
}

/// Every source's sidebar list; the page shows the one for the active source.
#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SourcePlaylists {
  spotify: Vec<PlaylistInfo>,
  local: Vec<PlaylistInfo>,
  subsonic: Vec<PlaylistInfo>,
  qobuz: Vec<PlaylistInfo>,
  youtube: Vec<PlaylistInfo>,
  /// The Music app's playlists, led by an "All songs" row; empty off macOS.
  apple_music: Vec<PlaylistInfo>,
  /// Stations are playable rows (`radio:<url>`), not playlists.
  radio: Vec<TrackInfo>,
  /// Spotify's saved albums cached so far; `OpenLibrary(Albums)` fetches them.
  saved_albums: Vec<AlbumInfo>,
  /// `LoadMore(SavedAlbums)` fetches the next page.
  saved_albums_has_more: bool,
  /// Spotify's followed artists cached so far; `OpenLibrary(Artists)` fetches them.
  followed_artists: Vec<ArtistInfo>,
  /// Spotify's saved podcast shows cached so far; `OpenLibrary(Podcasts)` fetches them.
  saved_shows: Vec<ShowInfo>,
  /// `LoadMore(SavedShows)` fetches the next page.
  saved_shows_has_more: bool,
  /// The ids of the saved shows among every show the app has seen.
  saved_show_ids: Vec<String>,
  /// The URIs of the Spotify playlists the user can add tracks to.
  editable: Vec<String>,
  /// The Spotify playlists in their folders; `null` without the rootlist, so the flat list stands.
  folders: Option<Vec<LibraryNode>>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SearchPayload {
  /// False until a search landed; empty lists before that mean "not searched".
  ran: bool,
  /// The query the results answer; the page ignores results for an older query.
  query: Option<String>,
  tracks: Vec<TrackInfo>,
  artists: Vec<ArtistInfo>,
  albums: Vec<AlbumInfo>,
  playlists: Vec<PlaylistInfo>,
  /// Podcast shows; only a Spotify search finds them.
  shows: Vec<ShowInfo>,
  /// The Spotify track ids among `tracks` that are in Liked Songs.
  liked: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct PlaylistSyncPayload {
  links: Vec<SyncLinkView>,
  /// A run owns the sync slot.
  running: bool,
  /// The last finished run's summary; it covers only the links that run touched.
  last_summary: Option<String>,
  last_failed: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SyncLinkView {
  id: String,
  source: Source,
  name: String,
  mirrors: Vec<SyncMirrorView>,
  /// This link's line in the last run, which names a skipped or failed mirror.
  last_line: Option<String>,
  last_failed: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SyncMirrorView {
  source: Source,
  /// Tracks placed on the mirror; the match cache itself stays on the server.
  matched: u32,
  unmatched: Vec<Unmatched>,
  /// RFC 3339; `null` for a mirror that never finished a run.
  last_run: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct DiscoverPayload {
  /// False without a Spotify session.
  available: bool,
  /// A fetch runs; the shelves already shown stay until it lands.
  loading: bool,
  /// `null` until the first fetch lands.
  shelves: Option<Vec<DiscoverShelf>>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct LyricsPayload {
  /// `not_started`, `loading`, `found` or `not_found`.
  status: &'static str,
  /// False when the line times are estimated from plain lyrics.
  synced: bool,
  lines: Vec<LyricLine>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct LyricLine {
  at_ms: u64,
  text: String,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct AlbumPayload {
  /// With its tracks; `null` before the first album fetch.
  album: Option<AlbumInfo>,
  /// The album is in the user's library, as the last check found.
  saved: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct ShowPayload {
  /// `null` before the first show fetch.
  page: Option<PodcastPage>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct PodcastPage {
  show: ShowInfo,
  /// The show is in the user's library, as the last check found.
  saved: bool,
  /// False until the first episode page landed; no episodes before that is still loading.
  loaded: bool,
  /// The episodes cached so far, newest first.
  episodes: Vec<EpisodeInfo>,
  /// The show's episode count as Spotify reports it.
  total: u32,
  /// `LoadMore(ShowEpisodes)` fetches the next page.
  has_more: bool,
}

/// The size of the desktop shell's window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
  Full,
  /// A small window that stays on top.
  Mini,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct ArtistPayload {
  /// `null` before the first artist fetch.
  page: Option<ArtistPage>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct ArtistPage {
  id: String,
  name: String,
  image_url: Option<String>,
  /// The user follows this artist, as the last follow check found.
  following: bool,
  top_tracks: Vec<TrackInfo>,
  albums: Vec<AlbumInfo>,
  related_artists: Vec<ArtistInfo>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct PartyPayload {
  phase: PartyPhase,
  /// Present while hosting or joined.
  room: Option<PartyRoom>,
  /// False without a Spotify session: a party cannot start or join.
  available: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub(crate) enum PartyPhase {
  Disconnected,
  Connecting,
  Hosting,
  Joined,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct PartyRoom {
  host: bool,
  /// Empty until the relay answers.
  code: String,
  /// Empty until a guest's join lands.
  host_name: String,
  guests: Vec<String>,
  /// Guests may control playback; the host sets it.
  shared_control: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct StatsPayload {
  /// `7d`, `30d`, `month`, `year` or `all`.
  period: &'static str,
  loading: bool,
  /// False until a load for this period landed.
  loaded: bool,
  /// Every period's qualified plays, in the ring order of the period keys.
  plays: Vec<PeriodPlays>,
  top_artists: Vec<StatsRow>,
  top_albums: Vec<StatsRow>,
  top_tracks: Vec<StatsRow>,
  /// The top five of the last 7 days, whatever the selected period.
  week_tracks: Vec<StatsRow>,
  movements: Vec<StatsMovement>,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct PeriodPlays {
  period: &'static str,
  plays: u32,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct StatsRow {
  title: String,
  /// Tracks only.
  artist: Option<String>,
  /// Tracks only.
  uri: Option<String>,
  listened_ms: u64,
  /// Artists only: 1-based, by listened time over all history.
  all_time_rank: Option<u32>,
  /// Artists only: the first listen falls inside the period.
  new: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct StatsMovement {
  name: String,
  /// `climb`, `fall` or `new`.
  kind: &'static str,
  from: Option<u32>,
  /// `None` for an artist the period never played.
  to: Option<u32>,
}

#[derive(Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClientMessage {
  Action {
    action: Box<Action>,
  },
  Onboarding {
    reply: OnboardingReply,
  },
  /// Only a desktop shell has an updater to ask.
  Update {
    ask: UpdateAsk,
  },
  /// Only a desktop shell has a window to resize.
  Window {
    mode: WindowMode,
  },
  Quit,
}

pub(crate) fn hello(
  revisions: DisplayRevisions,
  token: Option<String>,
  window: bool,
) -> ServerMessage {
  ServerMessage::Hello {
    payload: HelloPayload {
      version: env!("CARGO_PKG_VERSION"),
      token,
      revisions,
      window,
    },
  }
}

pub(crate) fn onboarding(view: OnboardingView) -> ServerMessage {
  ServerMessage::Onboarding { payload: view }
}

pub(crate) fn tick(app: &App) -> ServerMessage {
  ServerMessage::Tick {
    payload: app.playback_position_ms().map(|ms| ms as u64),
  }
}

/// Every channel whose revision moved since `sent`.
pub(crate) fn diff(sent: &DisplayRevisions, app: &App) -> Vec<ServerMessage> {
  let now = app.display_revisions();
  DisplayDomain::ALL
    .into_iter()
    .filter(|domain| now.get(*domain) != sent.get(*domain))
    .filter_map(|domain| channel_message(app, domain, now.get(domain)))
    .collect()
}

/// Every channel once.
pub(crate) fn resync(app: &App) -> Vec<ServerMessage> {
  let now = app.display_revisions();
  DisplayDomain::ALL
    .into_iter()
    .filter_map(|domain| channel_message(app, domain, now.get(domain)))
    .collect()
}

pub(crate) fn encode(message: &ServerMessage) -> String {
  serde_json::to_string(message).expect("a protocol message serializes")
}

fn channel_message(app: &App, domain: DisplayDomain, rev: u64) -> Option<ServerMessage> {
  Some(match domain {
    DisplayDomain::Route => ServerMessage::Route {
      rev,
      payload: route_name(app.get_current_route()),
    },
    DisplayDomain::Status => ServerMessage::Status {
      rev,
      payload: StatusPayload {
        message: app.status_message().map(str::to_string),
        is_error: app.status_message_is_error(),
        api_error: Some(app.api_error())
          .filter(|error| !error.is_empty())
          .map(str::to_string),
      },
    },
    DisplayDomain::Theme => ServerMessage::Theme {
      rev,
      payload: theme_colors(&app.user_config.theme),
    },
    DisplayDomain::Playback => ServerMessage::Playback {
      rev,
      payload: playback(app),
    },
    DisplayDomain::Devices => ServerMessage::Devices {
      rev,
      payload: device_list(app),
    },
    DisplayDomain::Queue => {
      let (spotify, native, now) = app.queue_view();
      let spotify = spotify
        .as_ref()
        .map(|queue| QueueSnapshot {
          currently_playing: queue
            .currently_playing
            .as_ref()
            .map(QueueItemSnapshot::from_playable),
          items: queue
            .queue
            .iter()
            .map(QueueItemSnapshot::from_playable)
            .collect(),
        })
        .unwrap_or_default();
      ServerMessage::Queue {
        rev,
        payload: Box::new(QueuePayload {
          spotify,
          native: native.clone(),
          now: now.clone(),
        }),
      }
    }
    DisplayDomain::Source => ServerMessage::Source {
      rev,
      payload: SourcePayload {
        active: app.active_source,
        compiled: compiled_in_sources(),
      },
    },
    DisplayDomain::Library => ServerMessage::Library {
      rev,
      payload: Box::new(playlists(app)),
    },
    DisplayDomain::LikedSongs => ServerMessage::Liked {
      rev,
      payload: Box::new(liked(app)),
    },
    DisplayDomain::Search => ServerMessage::Search {
      rev,
      payload: Box::new(search(app)),
    },
    DisplayDomain::Stats => ServerMessage::Stats {
      rev,
      payload: Box::new(stats(app)),
    },
    DisplayDomain::Party => ServerMessage::Party {
      rev,
      payload: party(app),
    },
    DisplayDomain::Lyrics => ServerMessage::Lyrics {
      rev,
      payload: Box::new(lyrics(app)),
    },
    DisplayDomain::Album => ServerMessage::Album {
      rev,
      payload: Box::new(album(app)),
    },
    DisplayDomain::Session => ServerMessage::Session {
      rev,
      payload: app.session_plays().to_vec(),
    },
    DisplayDomain::Discover => {
      let view = app.discover_view();
      ServerMessage::Discover {
        rev,
        payload: Box::new(DiscoverPayload {
          available: view.available,
          loading: view.loading,
          shelves: view.shelves.clone(),
        }),
      }
    }
    DisplayDomain::PlaylistSync => ServerMessage::PlaylistSync {
      rev,
      payload: Box::new(playlist_sync(app)),
    },
    DisplayDomain::TrackTable => {
      let view = app.track_table_view();
      ServerMessage::TrackTable {
        rev,
        payload: Box::new(TrackTablePayload {
          uri: view.uri.clone(),
          tracks: view.tracks.clone(),
          has_more: view.has_more,
          positions: view.positions.clone(),
          radio: view.radio.clone(),
        }),
      }
    }
    DisplayDomain::Artist => ServerMessage::Artist {
      rev,
      payload: Box::new(artist(app)),
    },
    DisplayDomain::RecentlyPlayed => ServerMessage::RecentlyPlayed {
      rev,
      payload: recently_played_snapshot(app),
    },
    DisplayDomain::LikedMarks => ServerMessage::LikedMarks {
      rev,
      payload: app.liked_marks_view().to_vec(),
    },
    DisplayDomain::Settings => ServerMessage::Settings {
      rev,
      payload: settings(app),
    },
    DisplayDomain::Update => ServerMessage::Update {
      rev,
      payload: app.update_status().cloned(),
    },
    DisplayDomain::Show => ServerMessage::Show {
      rev,
      payload: Box::new(show(app)),
    },
  })
}

fn show(app: &App) -> ShowPayload {
  use crate::core::app::EpisodeTableContext;
  let selected = match app.episode_table_context {
    EpisodeTableContext::Full => app.selected_show_full.as_ref().map(|full| &full.show),
    EpisodeTableContext::Simplified => app
      .selected_show_simplified
      .as_ref()
      .map(|simplified| &simplified.show),
  };
  let mut pages: Vec<_> = app.library().show_episodes.pages.iter().collect();
  pages.sort_by_key(|page| page.offset);
  ShowPayload {
    page: selected.map(|show| PodcastPage {
      show: show.clone(),
      saved: show
        .id
        .as_ref()
        .is_some_and(|id| app.saved_show_ids_set().contains(id)),
      loaded: !pages.is_empty(),
      episodes: pages
        .iter()
        .flat_map(|page| page.items.iter().cloned())
        .collect(),
      total: pages.last().map_or(0, |page| page.total),
      has_more: pages.last().is_some_and(|page| page.has_next()),
    }),
  }
}

fn settings(app: &App) -> Vec<SettingRow> {
  settings_for(app, cfg!(target_os = "android"))
}

fn settings_for(app: &App, phone: bool) -> Vec<SettingRow> {
  use crate::core::app::SettingValue;
  let items = app.behavior_setting_items();
  GUI_SETTINGS
    .iter()
    .filter(|(_, group, _)| *group != "updates" || app.update_status().is_some())
    .filter(|(id, _, _)| !(phone && PHONE_HIDDEN_SETTINGS.contains(id)))
    .filter_map(|(id, group, description)| {
      let item = items
        .iter()
        .find(|item| item.id == *id && app.setting_offered(item))?;
      let value = match &item.value {
        SettingValue::Bool(value) => SettingRowValue::Bool { value: *value },
        SettingValue::Number(value) => SettingRowValue::Number { value: *value },
        SettingValue::String(value) => SettingRowValue::Text {
          value: value.clone(),
        },
        SettingValue::Cycle(value, options) => SettingRowValue::Choice {
          value: value.clone(),
          options: options.iter().map(|option| option.to_string()).collect(),
        },
        SettingValue::Secret(value) => SettingRowValue::Secret {
          set: !value.is_empty(),
        },
        SettingValue::Color(_) | SettingValue::Key(_) | SettingValue::Preset(_) => return None,
      };
      Some(SettingRow {
        id: item.id.clone(),
        group,
        name: item.name.clone(),
        description: description.map_or_else(|| item.description.clone(), str::to_string),
        value,
      })
    })
    .collect()
}

fn artist(app: &App) -> ArtistPayload {
  ArtistPayload {
    page: app.artist.as_ref().map(|artist| ArtistPage {
      id: artist.artist_id.clone(),
      name: artist.artist_name.clone(),
      image_url: artist.image_url.clone(),
      following: app.followed_artist_ids_set().contains(&artist.artist_id),
      top_tracks: artist.top_tracks.clone(),
      albums: artist.albums.items.clone(),
      related_artists: artist.related_artists.clone(),
    }),
  }
}

fn playlist_sync(app: &App) -> PlaylistSyncPayload {
  use crate::core::playlist_sync::LinkOutcome;
  let report = app.playlist_sync_last_report();
  PlaylistSyncPayload {
    links: app
      .playlist_sync_links()
      .iter()
      .map(|link| {
        let last = report.and_then(|report| report.links.iter().find(|run| run.id == link.id));
        SyncLinkView {
          id: link.id.clone(),
          source: link.master.source,
          name: link.master.name.clone(),
          mirrors: link
            .mirrors
            .iter()
            .map(|mirror| SyncMirrorView {
              source: mirror.endpoint.source,
              matched: mirror.matches.len() as u32,
              unmatched: mirror.unmatched.clone(),
              last_run: mirror.last_run.clone(),
            })
            .collect(),
          last_line: last.map(|run| run.line()),
          last_failed: last.is_some_and(|run| matches!(run.outcome, LinkOutcome::Failed(_))),
        }
      })
      .collect(),
    running: app.playlist_sync_in_flight(),
    last_summary: report.map(|report| report.summary()),
    last_failed: report.is_some_and(|report| report.failed()),
  }
}

fn lyrics(app: &App) -> LyricsPayload {
  use crate::core::app::LyricsStatus;
  LyricsPayload {
    status: match app.lyrics_status() {
      LyricsStatus::NotStarted => "not_started",
      LyricsStatus::Loading => "loading",
      LyricsStatus::Found => "found",
      LyricsStatus::NotFound => "not_found",
    },
    synced: app.lyrics_synced(),
    lines: app
      .lyrics()
      .unwrap_or_default()
      .iter()
      .map(|(at, text)| LyricLine {
        at_ms: *at as u64,
        text: text.clone(),
      })
      .collect(),
  }
}

fn album(app: &App) -> AlbumPayload {
  use crate::core::app::AlbumTableContext;
  let album = match app.album_table_context {
    AlbumTableContext::Full => app
      .selected_album_full
      .as_ref()
      .map(|selected| selected.album.clone()),
    AlbumTableContext::Simplified => {
      app
        .selected_album_simplified
        .as_ref()
        .map(|selected| AlbumInfo {
          tracks: selected.tracks.items.clone(),
          total_tracks: Some(selected.tracks.total),
          ..selected.album.clone()
        })
    }
  };
  AlbumPayload {
    saved: album
      .as_ref()
      .and_then(|album| album.id.as_ref())
      .is_some_and(|id| app.saved_album_ids_set().contains(id)),
    album,
  }
}

fn party(app: &App) -> PartyPayload {
  use crate::infra::network::sync::{ControlMode, PartyRole, PartyStatus};
  PartyPayload {
    phase: match app.party_status() {
      PartyStatus::Disconnected => PartyPhase::Disconnected,
      PartyStatus::Connecting => PartyPhase::Connecting,
      PartyStatus::Hosting => PartyPhase::Hosting,
      PartyStatus::Joined => PartyPhase::Joined,
    },
    room: app.party_session().map(|session| PartyRoom {
      host: session.role == PartyRole::Host,
      code: session.code.clone(),
      host_name: session.host_name.clone(),
      guests: session.guests.clone(),
      shared_control: session.control_mode == ControlMode::SharedControl,
    }),
    available: app.spotify_connected,
  }
}

fn stats(app: &App) -> StatsPayload {
  use crate::infra::history::{MovementKind, RankedEntry};
  fn row(entry: &RankedEntry) -> StatsRow {
    let (title, artist) = match &entry.parts {
      Some((title, artist)) => (title.clone(), Some(artist.clone())),
      None => (entry.display.clone(), None),
    };
    StatsRow {
      title,
      artist,
      uri: entry.uri.clone(),
      listened_ms: entry.value,
      all_time_rank: None,
      new: false,
    }
  }
  let data = app.stats_data.as_ref();
  let rows = |pick: fn(&crate::infra::history::StatsData) -> &Vec<RankedEntry>| {
    data.map_or_else(Vec::new, |data| pick(data).iter().map(row).collect())
  };
  StatsPayload {
    period: app.stats_period.key(),
    loading: app.stats_loading(),
    loaded: data.is_some(),
    plays: data.map_or_else(Vec::new, |data| {
      data
        .period_plays
        .iter()
        .map(|(period, plays)| PeriodPlays {
          period: period.key(),
          plays: *plays as u32,
        })
        .collect()
    }),
    top_artists: data.map_or_else(Vec::new, |data| {
      data
        .top_artists
        .iter()
        .zip(&data.artist_ranks)
        .map(|(entry, rank)| StatsRow {
          all_time_rank: rank.all_time,
          new: rank.new,
          ..row(entry)
        })
        .collect()
    }),
    top_albums: rows(|data| &data.top_albums),
    top_tracks: rows(|data| &data.top_tracks),
    week_tracks: rows(|data| &data.week_tracks),
    movements: data.map_or_else(Vec::new, |data| {
      data
        .movements
        .iter()
        .map(|movement| StatsMovement {
          name: movement.name.clone(),
          kind: match movement.kind {
            MovementKind::Climb => "climb",
            MovementKind::Fall => "fall",
            MovementKind::New => "new",
          },
          from: movement.from,
          to: movement.to,
        })
        .collect()
    }),
  }
}

fn search(app: &App) -> SearchPayload {
  fn items<T: Clone>(page: &Option<crate::core::pagination::Paged<T>>) -> Vec<T> {
    page
      .as_ref()
      .map(|page| page.items.clone())
      .unwrap_or_default()
  }
  let results = app.search_results();
  SearchPayload {
    ran: results.tracks.is_some(),
    query: results.query.clone(),
    tracks: items(&results.tracks),
    artists: items(&results.artists),
    albums: items(&results.albums),
    playlists: items(&results.playlists),
    shows: items(&results.shows),
    liked: app.search_liked_ids(),
  }
}

fn liked(app: &App) -> LikedSongs {
  let (tracks, _, has_more) = app.saved_tracks_prefix();
  let pages = &app.library().saved_tracks.pages;
  LikedSongs {
    tracks,
    total: pages.first().map_or(0, |page| page.total),
    has_more,
    available: app.spotify_connected,
    loaded: !pages.is_empty(),
  }
}

fn playlists(app: &App) -> SourcePlaylists {
  SourcePlaylists {
    spotify: app.all_playlists().clone(),
    local: app.local_playlists().clone(),
    subsonic: app.subsonic_playlists().clone(),
    qobuz: app.qobuz_playlists().clone(),
    youtube: app.youtube_playlists().clone(),
    apple_music: app.apple_music_playlists().to_vec(),
    radio: app.radio_stations().clone(),
    saved_albums: saved_albums(app),
    saved_albums_has_more: last_page_has_next(&app.library().saved_albums.pages),
    followed_artists: app
      .library()
      .saved_artists
      .pages
      .iter()
      .flat_map(|page| page.items.iter().cloned())
      .collect(),
    saved_shows: saved_shows(app),
    saved_shows_has_more: last_page_has_next(&app.library().saved_shows.pages),
    saved_show_ids: app.saved_show_ids_set().iter().cloned().collect(),
    editable: app
      .editable_playlists()
      .into_iter()
      .map(|playlist| playlist.uri.clone())
      .collect(),
    folders: app.playlist_folder_nodes().as_ref().map(|nodes| {
      fn convert(node: &crate::core::app::PlaylistFolderNode) -> LibraryNode {
        match node.node_type {
          crate::core::app::PlaylistFolderNodeType::Folder => LibraryNode::Folder {
            name: node.name.clone().unwrap_or_default(),
            uri: node.uri.clone(),
            children: node.children.iter().map(convert).collect(),
          },
          crate::core::app::PlaylistFolderNodeType::Playlist => LibraryNode::Playlist {
            uri: node.uri.clone(),
          },
        }
      }
      nodes.iter().map(convert).collect()
    }),
  }
}

fn last_page_has_next<T>(pages: &[crate::core::pagination::Paged<T>]) -> bool {
  pages
    .iter()
    .max_by_key(|page| page.offset)
    .is_some_and(|page| page.has_next())
}

/// The saved shows still saved, in offset order: an unsave keeps the cached row.
fn saved_shows(app: &App) -> Vec<ShowInfo> {
  let mut pages: Vec<_> = app.library().saved_shows.pages.iter().collect();
  pages.sort_by_key(|page| page.offset);
  let saved = app.saved_show_ids_set();
  pages
    .into_iter()
    .flat_map(|page| page.items.iter())
    .filter(|show| show.id.as_ref().is_some_and(|id| saved.contains(id)))
    .cloned()
    .collect()
}

fn saved_albums(app: &App) -> Vec<AlbumInfo> {
  let mut pages: Vec<_> = app.library().saved_albums.pages.iter().collect();
  pages.sort_by_key(|page| page.offset);
  pages
    .into_iter()
    .flat_map(|page| page.items.iter().map(|saved| saved.album.clone()))
    .collect()
}

fn playback(app: &App) -> PlaybackPayload {
  let (snapshot, volume, device, liked, quality, context_name) = app.playback_view();
  PlaybackPayload {
    item: snapshot.as_ref().map(|snapshot| NowPlaying {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      image_url: snapshot.metadata.image_url.clone(),
      duration_ms: snapshot.metadata.duration_ms,
      uri: snapshot.item_uri.clone(),
      is_playing: snapshot.is_playing,
      is_live: snapshot.is_live,
      shuffle: snapshot.shuffle,
      repeat: snapshot
        .repeat
        .map(PlaybackState::repeat_from)
        .unwrap_or_else(|| "off".to_string()),
      // A queue slot plays over a suspended context it does not belong to.
      context_uri: if app.queue_owns_playback() {
        None
      } else {
        snapshot.context_uri.clone()
      },
      context_name: context_name.clone(),
      quality: quality.clone(),
    }),
    volume: *volume,
    device: device.clone(),
    liked: *liked,
  }
}

fn theme_colors(theme: &Theme) -> BTreeMap<String, Option<[u8; 3]>> {
  let palette = Palette::default();
  ThemeField::ALL
    .into_iter()
    .map(|field| {
      let color = theme.get(field);
      (
        field.name().to_string(),
        (color != Color::Reset).then(|| resolve(color, &palette)),
      )
    })
    .collect()
}

/// The first message of that kind, as JSON; panics when none was produced.
#[cfg(test)]
pub(crate) fn pushed(messages: &[ServerMessage], kind: &str) -> serde_json::Value {
  messages
    .iter()
    .map(|message| serde_json::to_value(message).unwrap())
    .find(|value| value["kind"] == kind)
    .unwrap_or_else(|| panic!("no {kind} message"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::action::{ListTarget, NavTarget};
  use crate::core::pagination::Paged;
  use crate::core::test_helpers::playlist_info;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::Receiver;
  use std::time::SystemTime;

  fn app() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = std::sync::mpsc::channel();
    (App::new(tx, UserConfig::new(), Some(SystemTime::now())), rx)
  }

  fn apply_from_page(app: &mut App, text: &str) -> Vec<ServerMessage> {
    let before = app.display_revisions();
    let ClientMessage::Action { action } = serde_json::from_str(text).unwrap() else {
      panic!("not an action");
    };
    app.apply(*action);
    diff(&before, app)
  }

  fn action_frame(action: Action) -> String {
    serde_json::json!({ "type": "action", "action": action }).to_string()
  }

  #[test]
  fn resync_sends_every_channel_once_in_domain_order() {
    let (app, _rx) = app();

    let kinds: Vec<_> = resync(&app)
      .iter()
      .map(|message| serde_json::to_value(message).unwrap()["kind"].clone())
      .collect();

    assert_eq!(
      kinds,
      [
        "route",
        "status",
        "source",
        "theme",
        "playback",
        "party",
        "devices",
        "search",
        "lyrics",
        "artist",
        "library",
        "liked",
        "queue",
        "stats",
        "album",
        "session",
        "discover",
        "playlist_sync",
        "track_table",
        "recently_played",
        "liked_marks",
        "settings",
        "update",
        "show"
      ]
    );
    let revisions = serde_json::to_value(app.display_revisions()).unwrap();
    assert_eq!(
      revisions.as_object().unwrap().len(),
      DisplayDomain::ALL.len()
    );
  }

  #[test]
  fn a_track_queued_from_the_page_pushes_the_queue_channel() {
    let (mut app, _rx) = app();
    let track = TrackInfo {
      uri: Some("subsonic:track:1".to_string()),
      name: "One".to_string(),
      artists: vec![],
      album: String::new(),
      duration_ms: 1000,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
      release_date: None,
    };

    let messages = apply_from_page(&mut app, &action_frame(Action::QueueTrack(track)));

    let queue = pushed(&messages, "queue");
    assert_eq!(queue["payload"]["native"][0]["name"], "One");
    assert!(queue["rev"].as_u64().unwrap() > 0);
  }

  #[test]
  fn a_queue_refresh_from_the_page_fetches_the_spotify_queue_without_a_route_push() {
    let (mut app, rx) = app();

    let messages = apply_from_page(&mut app, &action_frame(Action::RefreshQueue));

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQueue)));
    assert!(messages
      .iter()
      .all(|message| serde_json::to_value(message).unwrap()["kind"] != "route"));
  }

  #[test]
  fn a_device_refresh_from_the_page_fetches_devices_under_any_browse_source() {
    let (mut app, rx) = app();
    app.active_source = Source::Local;

    apply_from_page(&mut app, &action_frame(Action::RefreshDevices));

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetDevicesSilent)));
  }

  #[test]
  fn a_theme_set_from_the_page_pushes_rgb_and_leaves_reset_to_the_page() {
    let (mut app, _rx) = app();

    let messages = apply_from_page(
      &mut app,
      r#"{"type":"action","action":{"SetTheme":[["Active",{"Rgb":[1,2,3]}]]}}"#,
    );

    let theme = pushed(&messages, "theme");
    assert_eq!(theme["payload"]["active"], serde_json::json!([1, 2, 3]));
    assert!(theme["payload"]["text"].is_null());
  }

  #[test]
  fn a_navigation_from_the_page_pushes_the_route_name() {
    let (mut app, _rx) = app();

    let messages = apply_from_page(&mut app, &action_frame(Action::Navigate(NavTarget::Queue)));

    assert_eq!(pushed(&messages, "route")["payload"], "queue");
  }

  #[test]
  #[allow(deprecated)]
  fn a_cached_device_list_pushes_the_devices_channel() {
    use rspotify::model::device::{Device, DevicePayload};
    use rspotify::model::DeviceType;
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.set_devices(DevicePayload {
      devices: vec![Device {
        id: Some("device-1".to_string()),
        is_active: false,
        is_private_session: false,
        is_restricted: false,
        name: "Kitchen".to_string(),
        _type: DeviceType::Speaker,
        volume_percent: Some(40),
      }],
    });

    assert_eq!(
      pushed(&diff(&before, &app), "devices")["payload"][0]["name"],
      "Kitchen"
    );
  }

  fn liked_page(offset: u32, limit: u32, total: u32, has_next: bool) -> Paged<TrackInfo> {
    Paged {
      items: (offset..offset + limit)
        .map(|n| TrackInfo {
          uri: Some(format!("spotify:track:{n}")),
          name: format!("Track {n}"),
          artists: vec![],
          album: String::new(),
          duration_ms: 1000,
          id: None,
          album_id: None,
          artist_refs: vec![],
          is_playable: true,
          is_local: false,
          track_number: 0,
          explicit: false,
          image_url: None,
          release_date: None,
        })
        .collect(),
      offset,
      limit,
      total,
      next: has_next.then(|| "next".to_string()),
      previous: None,
    }
  }

  #[test]
  fn a_loaded_liked_songs_page_pushes_the_liked_channel_and_not_the_library() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();
    assert_eq!(pushed(&resync(&app), "liked")["payload"]["loaded"], false);

    app
      .saved_tracks_mut()
      .upsert_page_by_offset(liked_page(0, 1, 3, true));

    let messages = diff(&before, &app);
    let liked = &pushed(&messages, "liked")["payload"];
    assert_eq!(liked["tracks"][0]["name"], "Track 0");
    assert_eq!(liked["total"], 3);
    assert_eq!(liked["has_more"], true);
    assert_eq!(liked["loaded"], true);
    assert!(messages
      .iter()
      .all(|message| serde_json::to_value(message).unwrap()["kind"] != "library"));
  }

  #[test]
  fn load_more_from_the_page_fetches_the_next_liked_songs_page() {
    let (mut app, rx) = app();
    app
      .saved_tracks_mut()
      .upsert_page_by_offset(liked_page(0, 1, 3, true));

    apply_from_page(
      &mut app,
      &action_frame(Action::LoadMore(ListTarget::SavedTracks)),
    );

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetCurrentSavedTracks(Some(1)))
    ));
  }

  #[test]
  fn a_source_picked_on_the_page_pushes_the_scope_and_fetches_its_sidebar() {
    let (mut app, rx) = app();
    let dir = tempfile::tempdir().unwrap();
    app.state_path = Some(dir.path().join("state.yml"));

    let messages = apply_from_page(&mut app, &action_frame(Action::SelectSource(Source::Local)));

    assert_eq!(pushed(&messages, "source")["payload"]["active"], "Local");
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetLocalPlaylists)));
  }

  #[test]
  fn every_sources_playlists_ride_the_library_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    *app.subsonic_playlists_mut() = vec![playlist_info("p1", "Mix", "me", false)];

    let playlists = &pushed(&diff(&before, &app), "library")["payload"];
    assert_eq!(playlists["subsonic"][0]["name"], "Mix");
    assert_eq!(playlists["spotify"], serde_json::json!([]));
  }

  #[test]
  fn a_search_before_any_query_reports_it_has_not_run() {
    let (app, _rx) = app();

    let search = &pushed(&resync(&app), "search")["payload"];

    assert_eq!(search["ran"], false);
    assert_eq!(search["tracks"], serde_json::json!([]));
  }

  #[test]
  fn a_search_typed_on_the_page_dispatches_the_active_sources_search() {
    let (mut app, rx) = app();

    apply_from_page(
      &mut app,
      &action_frame(Action::SearchActiveSource("kygo".to_string())),
    );

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetSearchResults(query, _)) if query == "kygo"
    ));
  }

  #[test]
  fn a_liked_mark_landing_after_the_results_resends_the_search() {
    let (mut app, _rx) = app();
    let mut page = liked_page(0, 1, 1, false);
    page.items[0].id = Some("0".to_string());
    app.set_search_results(crate::core::app::SearchResult {
      tracks: Some(page),
      query: Some("kygo".to_string()),
      ..Default::default()
    });
    let landed = &pushed(&diff(&DisplayRevisions::default(), &app), "search")["payload"];
    assert_eq!(landed["ran"], true);
    assert_eq!(landed["query"], "kygo");
    assert_eq!(landed["tracks"][0]["name"], "Track 0");
    assert_eq!(landed["liked"], serde_json::json!([]));
    let before = app.display_revisions();

    app.liked_song_ids_set_mut().insert("0".to_string());
    app.note_display_changes();

    let search = &pushed(&diff(&before, &app), "search")["payload"];
    assert_eq!(search["liked"], serde_json::json!(["0"]));
    assert_eq!(search["query"], "kygo");
    let marked = app.display_revisions();

    app
      .liked_song_ids_set_mut()
      .insert("not-in-the-results".to_string());
    app.note_display_changes();

    assert_eq!(
      app.display_revisions().get(DisplayDomain::Search),
      marked.get(DisplayDomain::Search)
    );
  }

  fn stats_data() -> crate::infra::history::StatsData {
    use crate::infra::history::{ArtistRank, RankedEntry, RecapPeriod};
    let entry = |display: &str, parts: Option<(&str, &str)>| RankedEntry {
      display: display.to_string(),
      detail: String::new(),
      value: 60_000,
      uri: parts.map(|_| "spotify:track:1".to_string()),
      parts: parts.map(|(title, artist)| (title.to_string(), artist.to_string())),
    };
    crate::infra::history::StatsData {
      total_plays: 1,
      total_time_ms: 60_000,
      top_tracks: vec![entry("Freeze - Kygo", Some(("Freeze", "Kygo")))],
      top_artists: vec![entry("Kygo", None)],
      top_albums: vec![],
      days: vec![],
      period_plays: vec![(RecapPeriod::SevenDays, 1), (RecapPeriod::All, 9)],
      week_tracks: vec![],
      artist_ranks: vec![ArtistRank {
        all_time: Some(4),
        new: false,
      }],
      movements: vec![],
    }
  }

  #[test]
  fn opening_stats_from_the_page_pushes_the_stats_channel_loading() {
    let (mut app, rx) = app();

    let messages = apply_from_page(
      &mut app,
      &action_frame(Action::OpenLibrary(
        crate::core::action::LibraryTarget::Stats,
      )),
    );

    let stats = &pushed(&messages, "stats")["payload"];
    assert_eq!(stats["period"], "30d");
    assert_eq!(stats["loading"], true);
    assert_eq!(stats["loaded"], false);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::LoadListeningStats(_))));
  }

  #[test]
  fn landed_stats_push_rows_with_their_all_time_rank_and_the_track_apart() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.land_listening_stats(app.stats_period, stats_data());

    let stats = &pushed(&diff(&before, &app), "stats")["payload"];
    assert_eq!(stats["loaded"], true);
    assert_eq!(stats["top_artists"][0]["all_time_rank"], 4);
    assert_eq!(stats["top_tracks"][0]["title"], "Freeze");
    assert_eq!(stats["top_tracks"][0]["artist"], "Kygo");
    assert_eq!(stats["plays"][1]["period"], "all");
  }

  #[test]
  fn a_stats_result_for_a_period_no_longer_selected_is_dropped_without_a_push() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.land_listening_stats(crate::infra::history::RecapPeriod::Year, stats_data());

    assert!(app.stats_data.is_none());
    assert_eq!(
      app.display_revisions().get(DisplayDomain::Stats),
      before.get(DisplayDomain::Stats)
    );
  }

  #[test]
  fn a_stats_export_from_the_page_uses_the_selected_period_on_any_route() {
    let (mut app, rx) = app();
    app.stats_period = crate::infra::history::RecapPeriod::Year;

    apply_from_page(&mut app, &action_frame(Action::GenerateStatsRecap));

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GenerateRecap(
        crate::infra::history::RecapPeriod::Year
      ))
    ));
  }

  #[test]
  fn no_party_pushes_a_disconnected_party_that_says_spotify_is_missing() {
    let (mut app, _rx) = app();
    app.spotify_connected = false;

    let party = &pushed(&resync(&app), "party")["payload"];

    assert_eq!(party["phase"], "disconnected");
    assert!(party["room"].is_null());
    assert_eq!(party["available"], false);
  }

  #[test]
  fn a_hosted_party_pushes_its_code_guests_and_control_mode() {
    use crate::infra::network::sync::{ControlMode, PartyRole, PartySession, PartyStatus};
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.set_party_status(PartyStatus::Hosting);
    app.set_party_session(Some(PartySession {
      role: PartyRole::Host,
      code: "ABC123".to_string(),
      guests: vec!["Sam".to_string()],
      control_mode: ControlMode::HostOnly,
      host_name: "Host".to_string(),
    }));

    let party = &pushed(&diff(&before, &app), "party")["payload"];
    assert_eq!(party["phase"], "hosting");
    assert_eq!(party["room"]["code"], "ABC123");
    assert_eq!(party["room"]["guests"][0], "Sam");
    assert_eq!(party["room"]["shared_control"], false);

    let messages = apply_from_page(&mut app, &action_frame(Action::TogglePartyControlMode));
    assert_eq!(
      pushed(&messages, "party")["payload"]["room"]["shared_control"],
      true
    );
  }

  #[test]
  fn found_lyrics_push_the_lyrics_channel_with_millisecond_lines() {
    use crate::core::app::LyricsStatus;
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.set_lyrics(
      LyricsStatus::Found,
      Some(vec![(0, "a".to_string()), (5_000, "b".to_string())]),
      false,
    );

    let lyrics = &pushed(&diff(&before, &app), "lyrics")["payload"];
    assert_eq!(lyrics["status"], "found");
    assert_eq!(lyrics["synced"], false);
    assert_eq!(lyrics["lines"][1]["at_ms"], 5000);
  }

  #[test]
  fn the_album_channel_carries_the_saved_state_and_a_save_rerides_it() {
    use crate::core::app::{AlbumTableContext, SelectedAlbum};
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.selected_album_simplified = Some(SelectedAlbum {
      album: AlbumInfo {
        id: Some("21".to_string()),
        uri: Some("spotify:album:21".to_string()),
        name: "21".to_string(),
        ..AlbumInfo::default()
      },
      tracks: liked_page(0, 2, 11, true),
      selected_index: 0,
    });
    app.album_table_context = AlbumTableContext::Simplified;
    app.bump_display(DisplayDomain::Album);

    let payload = &pushed(&diff(&before, &app), "album")["payload"];
    assert_eq!(payload["saved"], false);

    let before = app.display_revisions();
    app.saved_album_ids_set_mut().insert("21".to_string());
    let payload = &pushed(&diff(&before, &app), "album")["payload"];
    assert_eq!(payload["saved"], true);
    assert_eq!(payload["album"]["name"], "21");
  }

  #[test]
  fn a_fetched_album_rides_the_album_channel_with_its_page_of_tracks() {
    use crate::core::app::{AlbumTableContext, SelectedAlbum};
    let (mut app, _rx) = app();
    assert!(pushed(&resync(&app), "album")["payload"]["album"].is_null());
    let before = app.display_revisions();

    app.selected_album_simplified = Some(SelectedAlbum {
      album: AlbumInfo {
        uri: Some("spotify:album:21".to_string()),
        name: "21".to_string(),
        ..AlbumInfo::default()
      },
      tracks: liked_page(0, 2, 11, true),
      selected_index: 0,
    });
    app.album_table_context = AlbumTableContext::Simplified;
    app.bump_display(DisplayDomain::Album);

    let album = &pushed(&diff(&before, &app), "album")["payload"]["album"];
    assert_eq!(album["name"], "21");
    assert_eq!(album["tracks"][1]["name"], "Track 1");
    assert_eq!(album["total_tracks"], 11);
  }

  #[test]
  fn fetched_followed_artists_ride_the_library_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app
      .library_mut()
      .saved_artists
      .add_pages(crate::core::pagination::CursorPaged {
        items: vec![ArtistInfo {
          name: "Avicii".to_string(),
          ..ArtistInfo::default()
        }],
        ..Default::default()
      });

    let library = &pushed(&diff(&before, &app), "library")["payload"];
    assert_eq!(library["followed_artists"][0]["name"], "Avicii");
  }

  #[test]
  fn a_fetched_artist_rides_the_artist_channel_with_its_follow_state() {
    use crate::core::app::{Artist, ArtistBlock};
    let (mut app, _rx) = app();
    assert!(pushed(&resync(&app), "artist")["payload"]["page"].is_null());
    let before = app.display_revisions();

    app.artist = Some(Artist {
      artist_id: "kygo".to_string(),
      artist_name: "Kygo".to_string(),
      image_url: Some("https://i.scdn.co/image/kygo".to_string()),
      albums: Paged {
        items: vec![AlbumInfo {
          name: "Cloud Nine".to_string(),
          ..AlbumInfo::default()
        }],
        ..Paged::default()
      },
      related_artists: vec![],
      top_tracks: liked_page(0, 2, 2, false).items,
      selected_album_index: 0,
      selected_related_artist_index: 0,
      selected_top_track_index: 0,
      artist_hovered_block: ArtistBlock::TopTracks,
      artist_selected_block: ArtistBlock::TopTracks,
    });
    app.bump_display(DisplayDomain::Artist);

    let payload = &pushed(&diff(&before, &app), "artist")["payload"];
    assert_eq!(payload["page"]["name"], "Kygo");
    assert_eq!(payload["page"]["image_url"], "https://i.scdn.co/image/kygo");
    assert_eq!(payload["page"]["top_tracks"][1]["name"], "Track 1");
    assert_eq!(payload["page"]["albums"][0]["name"], "Cloud Nine");
    assert_eq!(payload["page"]["following"], false);

    let before = app.display_revisions();
    app.followed_artist_ids_set_mut().insert("kygo".to_string());
    let page = &pushed(&diff(&before, &app), "artist")["payload"]["page"];
    assert_eq!(page["following"], true);
  }

  #[test]
  fn saved_albums_ride_the_library_channel_in_offset_order() {
    use crate::core::plugin_api::SavedAlbumInfo;
    let (mut app, _rx) = app();
    let before = app.display_revisions();
    let page = |offset: u32, name: &str| Paged {
      items: vec![SavedAlbumInfo {
        album: AlbumInfo {
          name: name.to_string(),
          ..AlbumInfo::default()
        },
        added_at: String::new(),
      }],
      offset,
      ..Paged::default()
    };

    app.store_saved_albums_page(page(1, "Second"));
    app.store_saved_albums_page(page(0, "First"));

    let albums = &pushed(&diff(&before, &app), "library")["payload"]["saved_albums"];
    assert_eq!(albums[0]["name"], "First");
    assert_eq!(albums[1]["name"], "Second");
  }

  #[test]
  fn opening_a_cached_saved_album_pushes_its_saved_state() {
    use crate::core::plugin_api::SavedAlbumInfo;
    let (mut app, rx) = app();
    let before = app.display_revisions();

    // A complete saved-albums page for a one-track album.
    app.store_saved_albums_page(Paged {
      items: vec![SavedAlbumInfo {
        album: AlbumInfo {
          id: Some("21".to_string()),
          name: "21".to_string(),
          tracks: liked_page(0, 1, 1, false).items,
          total_tracks: Some(1),
          ..AlbumInfo::default()
        },
        added_at: String::new(),
      }],
      offset: 0,
      limit: 1,
      total: 1,
      ..Paged::default()
    });

    // Opening the cached album dispatches no save check.
    app.open_saved_album("21".to_string());
    assert!(rx.try_recv().is_err());

    let payload = &pushed(&diff(&before, &app), "album")["payload"];
    assert_eq!(payload["album"]["id"], "21");
    assert_eq!(payload["saved"], true);
  }

  #[test]
  fn the_album_page_says_if_its_album_is_saved_and_follows_an_unsave() {
    use crate::core::app::{AlbumTableContext, SelectedAlbum};
    let (mut app, _rx) = app();
    app.selected_album_simplified = Some(SelectedAlbum {
      album: AlbumInfo {
        id: Some("21".to_string()),
        name: "21".to_string(),
        ..AlbumInfo::default()
      },
      tracks: liked_page(0, 1, 1, false),
      selected_index: 0,
    });
    app.album_table_context = AlbumTableContext::Simplified;
    app.saved_album_ids_set_mut().insert("21".to_string());
    let before = app.display_revisions();
    assert_eq!(pushed(&resync(&app), "album")["payload"]["saved"], true);

    app.saved_album_ids_set_mut().remove("21");

    assert_eq!(
      pushed(&diff(&before, &app), "album")["payload"]["saved"],
      false
    );
  }

  fn episode(n: u32) -> EpisodeInfo {
    EpisodeInfo {
      id: Some(format!("e{n}")),
      uri: Some(format!("spotify:episode:e{n}")),
      name: format!("Episode {n}"),
      duration_ms: 60_000,
      show_name: String::new(),
      description: String::new(),
      release_date: String::new(),
      is_playable: true,
      resume_point: None,
      image_url: None,
    }
  }

  #[test]
  fn a_fetched_show_rides_the_show_channel_with_its_episodes_and_saved_state() {
    use crate::core::app::{EpisodeTableContext, SelectedFullShow};
    let (mut app, _rx) = app();
    assert!(pushed(&resync(&app), "show")["payload"]["page"].is_null());
    let before = app.display_revisions();

    app.selected_show_full = Some(SelectedFullShow {
      show: ShowInfo {
        id: Some("pod".to_string()),
        name: "Pod".to_string(),
        ..ShowInfo::default()
      },
    });
    app.episode_table_context = EpisodeTableContext::Full;
    app.store_show_episodes_page(
      Paged {
        items: vec![episode(0), episode(1)],
        offset: 0,
        limit: 2,
        total: 5,
        next: Some("next".to_string()),
        ..Paged::default()
      },
      true,
    );

    let page = &pushed(&diff(&before, &app), "show")["payload"]["page"];
    assert_eq!(page["show"]["name"], "Pod");
    assert_eq!(page["episodes"][1]["name"], "Episode 1");
    assert_eq!(page["total"], 5);
    assert_eq!(page["has_more"], true);
    assert_eq!(page["saved"], false);

    let before = app.display_revisions();
    app.saved_show_ids_set_mut().insert("pod".to_string());
    let page = &pushed(&diff(&before, &app), "show")["payload"]["page"];
    assert_eq!(page["saved"], true);
  }

  #[test]
  fn a_first_episode_page_replaces_the_episodes_of_the_show_before() {
    let (mut app, _rx) = app();
    let page = |items| Paged {
      items,
      ..Paged::default()
    };
    app.store_show_episodes_page(page(vec![episode(0)]), true);

    app.store_show_episodes_page(page(vec![episode(7)]), true);

    let pages = &app.library().show_episodes.pages;
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].items[0].name, "Episode 7");
  }

  #[test]
  fn saved_shows_ride_the_library_channel_and_an_unsaved_show_leaves_it() {
    let (mut app, _rx) = app();
    let show = |id: &str| ShowInfo {
      id: Some(id.to_string()),
      name: id.to_string(),
      ..ShowInfo::default()
    };
    app.store_saved_shows_page(Paged {
      items: vec![show("a"), show("b")],
      ..Paged::default()
    });
    let library = &pushed(&resync(&app), "library")["payload"];
    assert_eq!(library["saved_shows"].as_array().unwrap().len(), 2);
    assert!(app.saved_show_ids_set().contains("a"));
    let before = app.display_revisions();

    app.saved_show_ids_set_mut().remove("a");

    let library = &pushed(&diff(&before, &app), "library")["payload"];
    assert_eq!(library["saved_shows"][0]["name"], "b");
    assert_eq!(library["saved_shows"].as_array().unwrap().len(), 1);
  }

  #[test]
  fn the_rootlist_folders_ride_the_library_channel_as_a_tree() {
    use crate::core::app::{PlaylistFolderNode, PlaylistFolderNodeType};
    let (mut app, _rx) = app();
    assert!(pushed(&resync(&app), "library")["payload"]["folders"].is_null());
    let before = app.display_revisions();

    *app.playlist_folder_nodes_mut() = Some(vec![PlaylistFolderNode {
      name: Some("Gym".to_string()),
      node_type: PlaylistFolderNodeType::Folder,
      uri: "spotify:user:me:folder:1".to_string(),
      children: vec![PlaylistFolderNode {
        name: None,
        node_type: PlaylistFolderNodeType::Playlist,
        uri: "spotify:playlist:p1".to_string(),
        children: vec![],
      }],
    }]);

    let folders = &pushed(&diff(&before, &app), "library")["payload"]["folders"];
    assert_eq!(folders[0]["kind"], "folder");
    assert_eq!(folders[0]["name"], "Gym");
    assert_eq!(folders[0]["children"][0]["kind"], "playlist");
    assert_eq!(folders[0]["children"][0]["uri"], "spotify:playlist:p1");
  }

  #[test]
  fn a_playlist_table_sends_its_row_positions_and_a_radio_table_its_seed() {
    use crate::core::app::TrackTableContext;
    use rspotify::model::idtypes::PlaylistId;
    let (mut app, _rx) = app();
    app.playlist_track_table_id = Some(PlaylistId::from_id("p1").unwrap().into_static());
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.track_table.tracks = liked_page(0, 2, 2, false).items;
    app.playlist_track_positions = Some(vec![4, 9]);
    app.note_display_changes();
    let table = &pushed(&resync(&app), "track_table")["payload"];
    assert_eq!(table["positions"], serde_json::json!([4, 9]));
    assert!(table["radio"].is_null());
    let before = app.display_revisions();

    app.track_table.context = Some(TrackTableContext::RecommendedTracks);
    app.set_radio_seed_id(Some("seed".to_string()));
    app.note_display_changes();

    let table = &pushed(&diff(&before, &app), "track_table")["payload"];
    assert!(table["positions"].is_null());
    assert_eq!(table["radio"], "seed");
  }

  #[test]
  fn hello_says_whether_a_shell_can_resize_its_window() {
    let (app, _rx) = app();
    let message = encode(&hello(app.display_revisions(), None, true));
    let hello: serde_json::Value = serde_json::from_str(&message).unwrap();
    assert_eq!(hello["payload"]["window"], true);
    let ClientMessage::Window { mode } =
      serde_json::from_str(r#"{"type":"window","mode":"mini"}"#).unwrap()
    else {
      panic!("not a window message");
    };
    assert_eq!(mode, WindowMode::Mini);
  }

  #[test]
  fn a_visible_row_turning_liked_pushes_the_liked_marks_channel() {
    let (mut app, _rx) = app();
    let mut rows = liked_page(0, 2, 2, false).items;
    rows[0].id = Some("seen".to_string());
    rows[1].id = Some("other".to_string());
    app.recently_played = Some(crate::core::pagination::CursorPaged {
      items: rows,
      ..Default::default()
    });
    app.liked_song_ids_set_mut().insert("unseen".to_string());
    app.note_display_changes();
    let before = app.display_revisions();

    app.liked_song_ids_set_mut().insert("seen".to_string());
    app.note_display_changes();

    let marks = &pushed(&diff(&before, &app), "liked_marks")["payload"];
    assert_eq!(marks, &serde_json::json!(["seen"]));
  }

  #[test]
  fn the_library_channel_names_the_playlists_the_user_can_edit() {
    use crate::core::app::UserInfo;
    let (mut app, _rx) = app();
    *app.user_mut() = Some(UserInfo {
      id: "me".to_string(),
      ..UserInfo::default()
    });
    let before = app.display_revisions();

    *app.all_playlists_mut() = vec![
      playlist_info("mine", "Mine", "me", false),
      playlist_info("shared", "Shared", "friend", true),
      playlist_info("theirs", "Theirs", "friend", false),
    ];

    let editable = &pushed(&diff(&before, &app), "library")["payload"]["editable"];
    assert_eq!(
      editable,
      &serde_json::json!(["spotify:playlist:mine", "spotify:playlist:shared"])
    );
  }

  #[test]
  fn the_settings_channel_offers_the_gui_rows_and_the_update_rows_only_with_an_updater() {
    let (mut app, _rx) = app();
    let ids = |app: &App| -> Vec<String> {
      settings(app)
        .iter()
        .map(|row| row.id.clone())
        .collect::<Vec<_>>()
    };

    let without = ids(&app);
    let before = app.display_revisions();
    app.set_update_status(UpdateStatus::Idle);

    assert_eq!(without[0], "behavior.seek_milliseconds");
    assert!(!without
      .iter()
      .any(|id| id.starts_with("behavior.tick_rate")));
    assert!(!without.contains(&"behavior.disable_auto_update".to_string()));
    let rows = &pushed(&diff(&before, &app), "settings")["payload"];
    assert!(rows
      .as_array()
      .unwrap()
      .iter()
      .any(|row| row["id"] == "behavior.disable_auto_update"
        && row["group"] == "updates"
        && row["value"]["kind"] == "bool"));
  }

  #[test]
  fn a_phone_settings_page_leaves_out_the_rows_a_phone_cannot_use() {
    let (app, _rx) = app();
    let ids = |phone| -> Vec<String> {
      settings_for(&app, phone)
        .iter()
        .map(|row| row.id.clone())
        .collect()
    };

    assert!(ids(false).contains(&"behavior.seek_milliseconds".to_string()));
    let phone = ids(true);
    assert!(!phone
      .iter()
      .any(|id| PHONE_HIDDEN_SETTINGS.contains(&id.as_str())));
    assert!(phone.contains(&"behavior.stop_after_current_track".to_string()));
  }

  #[test]
  fn an_update_status_rides_the_update_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.set_update_status(UpdateStatus::Ready {
      version: "0.45.0".to_string(),
    });

    let update = &pushed(&diff(&before, &app), "update")["payload"];
    assert_eq!(update["state"], "ready");
    assert_eq!(update["version"], "0.45.0");
  }

  #[test]
  fn recently_played_rides_its_own_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.recently_played = Some(crate::core::pagination::CursorPaged {
      items: liked_page(0, 1, 1, false).items,
      ..Default::default()
    });
    app.bump_display(DisplayDomain::RecentlyPlayed);

    let recent = &pushed(&diff(&before, &app), "recently_played")["payload"];
    assert_eq!(recent[0]["name"], "Track 0");
  }

  #[test]
  fn opening_the_playing_tracks_album_from_the_page_fetches_it_by_track_id() {
    let (mut app, rx) = app();

    apply_from_page(
      &mut app,
      &action_frame(Action::Open(crate::core::action::OpenTarget::TrackAlbum(
        "4uLU6hMCjMI75M1A2tKUQC".to_string(),
      ))),
    );

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetAlbumForTrack(_))));
  }

  #[test]
  fn a_finished_play_pushes_the_session_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.record_session_play(SessionPlay {
      started_at_ms: 1_000,
      ended_at_ms: 241_000,
      listened_ms: 240_000,
      duration_ms: 243_000,
      title: "Turning Tables".to_string(),
      artists: vec!["Adele".to_string()],
      album: "21".to_string(),
      uri: Some("file:///21/03.flac".to_string()),
      image_url: None,
    });

    let plays = &pushed(&diff(&before, &app), "session")["payload"];
    assert_eq!(plays[0]["title"], "Turning Tables");
    assert_eq!(plays[0]["started_at_ms"], 1000);
  }

  #[test]
  fn landed_made_for_you_shelves_push_the_discover_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    app.end_discover_fetch(Some(crate::core::test_helpers::discover_shelves()));
    app.note_display_changes();

    let discover = &pushed(&diff(&before, &app), "discover")["payload"];
    assert_eq!(discover["shelves"][1]["title"], "New");
    assert_eq!(
      discover["shelves"][1]["playlists"][0]["name"],
      "Release Radar"
    );
    assert_eq!(discover["loading"], false);
    assert_eq!(discover["available"], true);
  }

  #[test]
  fn an_opened_source_playlist_names_its_rows_only_after_they_land() {
    use crate::core::action::OpenTarget;
    use crate::core::app::TrackTableContext;
    let (mut app, rx) = app();
    let uri = "subsonic:playlist:7";
    app.set_source_track_table(
      "subsonic:playlist:1",
      liked_page(0, 2, 2, false).items,
      TrackTableContext::SubsonicPlaylist,
    );
    app.note_display_changes();

    let before = app.display_revisions();
    app.apply(Action::Open(OpenTarget::SourcePlaylist(uri.to_string())));
    let loading = &pushed(&diff(&before, &app), "track_table")["payload"];
    assert!(loading["uri"].is_null());
    assert_eq!(loading["tracks"], serde_json::json!([]));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetSubsonicTracks(u)) if u == uri));

    let before = app.display_revisions();
    app.set_source_track_table(
      uri,
      liked_page(0, 1, 1, false).items,
      TrackTableContext::SubsonicPlaylist,
    );
    app.note_display_changes();
    let landed = &pushed(&diff(&before, &app), "track_table")["payload"];
    assert_eq!(landed["uri"], uri);
    assert_eq!(landed["tracks"][0]["name"], "Track 0");
    assert_eq!(landed["has_more"], false);
  }

  #[test]
  fn a_spotify_playlist_names_its_rows_once_the_open_is_no_longer_pending() {
    use crate::core::action::OpenTarget;
    let (mut app, _rx) = app();
    let id = "37i9dQZF1DXcBWIGoYBM5M";

    app.apply(Action::Open(OpenTarget::Playlist {
      id: format!("spotify:playlist:{id}"),
      from_search: false,
    }));
    assert!(app.track_table_view().uri.is_none());

    app.pending_playlist_open = None;
    let before = app.display_revisions();
    app.note_display_changes();
    assert_eq!(
      pushed(&diff(&before, &app), "track_table")["payload"]["uri"],
      format!("spotify:playlist:{id}")
    );
  }

  #[test]
  fn opening_discover_from_the_page_fetches_made_for_you_again() {
    use crate::core::action::LibraryTarget;
    let (mut app, rx) = app();
    app.end_discover_fetch(Some(crate::core::test_helpers::discover_shelves()));

    apply_from_page(
      &mut app,
      &action_frame(Action::OpenLibrary(LibraryTarget::Discover)),
    );

    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetMadeForYou)));
  }

  #[test]
  fn loaded_sync_links_push_the_playlist_sync_channel_with_counts_not_the_match_cache() {
    use crate::core::playlist_sync::{Endpoint, Link, Mirror, UnmatchReason};
    let (mut app, _rx) = app();
    let before = app.display_revisions();
    let endpoint = |source, name: &str| Endpoint {
      source,
      playlist_uri: format!("{name}:1"),
      name: name.to_string(),
    };

    app.set_playlist_sync_links(vec![Link {
      id: "abc123".to_string(),
      master: endpoint(Source::Spotify, "Road Trip"),
      mirrors: vec![Mirror {
        endpoint: endpoint(Source::Qobuz, "Road Trip"),
        matches: [("a".to_string(), "b".to_string())].into_iter().collect(),
        unmatched: vec![Unmatched {
          master_key: "c".to_string(),
          title: "Levels".to_string(),
          artist: "Avicii".to_string(),
          reason: UnmatchReason::NoCandidate,
        }],
        last_run: None,
      }],
    }]);

    let sync = &pushed(&diff(&before, &app), "playlist_sync")["payload"];
    let mirror = &sync["links"][0]["mirrors"][0];
    assert_eq!(sync["links"][0]["name"], "Road Trip");
    assert_eq!(mirror["source"], "Qobuz");
    assert_eq!(mirror["matched"], 1);
    assert_eq!(mirror["unmatched"][0]["reason"], "NoCandidate");
    assert!(mirror.get("matches").is_none());
  }

  #[test]
  fn a_sync_run_in_flight_rides_the_playlist_sync_channel() {
    let (mut app, _rx) = app();
    let before = app.display_revisions();

    assert!(app.begin_playlist_sync());
    assert_eq!(
      pushed(&diff(&before, &app), "playlist_sync")["payload"]["running"],
      true
    );
    let begun = app.display_revisions();
    assert!(!app.begin_playlist_sync());

    assert_eq!(
      app.display_revisions().get(DisplayDomain::PlaylistSync),
      begun.get(DisplayDomain::PlaylistSync)
    );
  }

  #[test]
  fn a_tick_carries_the_position_and_no_revision() {
    let (mut app, _rx) = app();
    app.song_progress_ms = 1_234;

    assert_eq!(
      serde_json::to_value(tick(&app)).unwrap(),
      serde_json::json!({ "kind": "tick", "payload": 1234 })
    );
  }
  #[test]
  fn typescript_bindings_are_written_to_the_frontend_source_tree() {
    use crate::core::plugin_api::{EpisodeInfo, ResumePointInfo};
    use ts_rs::TS;
    fn written<T: TS + 'static>(dir: &std::path::Path, cfg: &ts_rs::Config) {
      T::export_all(cfg).unwrap();
      let file = std::fs::read_to_string(dir.join(format!("{}.ts", T::ident(cfg)))).unwrap();
      assert!(
        file.contains(&T::decl(cfg)),
        "{} shares its name with another type",
        T::ident(cfg)
      );
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("gui/src/bindings");
    if dir.exists() {
      std::fs::remove_dir_all(&dir).unwrap();
    }
    let cfg = ts_rs::Config::new()
      .with_large_int("number")
      .with_out_dir(dir.clone());
    written::<Action>(&dir, &cfg);
    written::<ServerMessage>(&dir, &cfg);
    written::<ClientMessage>(&dir, &cfg);
    written::<HelloPayload>(&dir, &cfg);
    written::<PlaybackPayload>(&dir, &cfg);
    written::<NowPlaying>(&dir, &cfg);
    written::<QueuePayload>(&dir, &cfg);
    written::<StatusPayload>(&dir, &cfg);
    written::<SourcePayload>(&dir, &cfg);
    written::<LikedSongs>(&dir, &cfg);
    written::<SourcePlaylists>(&dir, &cfg);
    written::<SearchPayload>(&dir, &cfg);
    written::<StatsPayload>(&dir, &cfg);
    written::<PartyPayload>(&dir, &cfg);
    written::<LyricsPayload>(&dir, &cfg);
    written::<AlbumPayload>(&dir, &cfg);
    written::<SessionPlay>(&dir, &cfg);
    written::<DiscoverPayload>(&dir, &cfg);
    written::<PlaylistSyncPayload>(&dir, &cfg);
    written::<TrackTablePayload>(&dir, &cfg);
    written::<ArtistPayload>(&dir, &cfg);
    written::<ArtistPage>(&dir, &cfg);
    written::<ArtistInfo>(&dir, &cfg);
    written::<AlbumInfo>(&dir, &cfg);
    written::<PlaylistInfo>(&dir, &cfg);
    written::<DisplayRevisions>(&dir, &cfg);
    written::<DeviceInfo>(&dir, &cfg);
    written::<QueueSnapshot>(&dir, &cfg);
    written::<QueueItemSnapshot>(&dir, &cfg);
    written::<EpisodeInfo>(&dir, &cfg);
    written::<ResumePointInfo>(&dir, &cfg);
    written::<OnboardingView>(&dir, &cfg);
    written::<crate::gui::onboarding::OnboardingQuestion>(&dir, &cfg);
    written::<crate::gui::onboarding::OnboardingAsk>(&dir, &cfg);
    written::<crate::gui::onboarding::SourceChoice>(&dir, &cfg);
    written::<OnboardingReply>(&dir, &cfg);
    written::<crate::gui::onboarding::OnboardingReplyAnswer>(&dir, &cfg);
    written::<LibraryNode>(&dir, &cfg);
    written::<ShowPayload>(&dir, &cfg);
    written::<PodcastPage>(&dir, &cfg);
    written::<ShowInfo>(&dir, &cfg);
    written::<WindowMode>(&dir, &cfg);
    for entry in std::fs::read_dir(&dir).unwrap() {
      let path = entry.unwrap().path();
      assert!(
        !std::fs::read_to_string(&path).unwrap().contains("bigint"),
        "{}",
        path.display()
      );
    }
  }
}
