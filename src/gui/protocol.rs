//! The JSON the server and the page exchange over the socket. A channel message
//! carries that channel's whole state; a page applies it when its `rev` is at
//! least the last one it applied for that channel. Hello's revisions are informational.

use crate::core::action::Action;
use crate::core::app::{App, DiscoverTimeRange, DisplayDomain, DisplayRevisions, SessionPlay};
use crate::core::first_run::compiled_in_sources;
use crate::core::playlist_sync::Unmatched;
use crate::core::plugin_api::{
  device_list, route_name, AlbumInfo, ArtistInfo, DeviceInfo, PlaybackState, PlaylistInfo,
  QueueItemSnapshot, QueueSnapshot, TrackInfo,
};
use crate::core::source::Source;
use crate::core::theme::{resolve, Color, Palette, Theme, ThemeField};
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
  /// Spotify's top tracks for one range and the top artists mix.
  Discover {
    rev: u64,
    payload: Box<DiscoverPayload>,
  },
  /// The playlist-sync links and the run state.
  PlaylistSync {
    rev: u64,
    payload: Box<PlaylistSyncPayload>,
  },
}

#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct HelloPayload {
  version: &'static str,
  /// Present only on the connect that traded a launch code.
  token: Option<String>,
  revisions: DisplayRevisions,
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

/// Every source's sidebar list; the page shows the one for the active source.
#[derive(Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub(crate) struct SourcePlaylists {
  spotify: Vec<PlaylistInfo>,
  local: Vec<PlaylistInfo>,
  subsonic: Vec<PlaylistInfo>,
  qobuz: Vec<PlaylistInfo>,
  youtube: Vec<PlaylistInfo>,
  /// Stations are playable rows (`radio:<url>`), not playlists.
  radio: Vec<TrackInfo>,
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
  /// A fetch runs; the page cannot tell which list it is for.
  loading: bool,
  /// The range `top_tracks` belongs to; `null` before the first landing.
  top_tracks_range: Option<DiscoverTimeRange>,
  top_tracks: Vec<TrackInfo>,
  artists_mix: Vec<TrackInfo>,
  /// False once this app key lost the artist top-tracks endpoint.
  artists_mix_available: bool,
  /// The ids among both lists that are in Liked Songs.
  liked_ids: Vec<String>,
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
  Action { action: Box<Action> },
  Onboarding { reply: OnboardingReply },
  Quit,
}

pub(crate) fn hello(revisions: DisplayRevisions, token: Option<String>) -> ServerMessage {
  ServerMessage::Hello {
    payload: HelloPayload {
      version: env!("CARGO_PKG_VERSION"),
      token,
      revisions,
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
          top_tracks_range: view.range,
          top_tracks: view.top_tracks.clone(),
          artists_mix: view.artists_mix.clone(),
          artists_mix_available: view.mix_available,
          liked_ids: view.liked_ids.clone(),
        }),
      }
    }
    DisplayDomain::PlaylistSync => ServerMessage::PlaylistSync {
      rev,
      payload: Box::new(playlist_sync(app)),
    },
    DisplayDomain::Artist => return None,
  })
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
  AlbumPayload {
    album: match app.album_table_context {
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
    },
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
  let tracks = items(&results.tracks);
  let liked = tracks
    .iter()
    .filter(|track| {
      track
        .uri
        .as_deref()
        .is_some_and(|uri| uri.starts_with("spotify:track:"))
    })
    .filter_map(|track| track.id.clone())
    .filter(|id| app.liked_song_ids_set().contains(id))
    .collect();
  SearchPayload {
    ran: results.tracks.is_some(),
    query: results.query.clone(),
    tracks,
    artists: items(&results.artists),
    albums: items(&results.albums),
    playlists: items(&results.playlists),
    liked,
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
    radio: app.radio_stations().clone(),
  }
}

fn playback(app: &App) -> PlaybackPayload {
  let (snapshot, volume, device, liked) = app.playback_view();
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
      context_uri: snapshot.context_uri.clone(),
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
        "library",
        "liked",
        "queue",
        "stats",
        "album",
        "session",
        "discover",
        "playlist_sync"
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

    let search = &pushed(&diff(&before, &app), "search")["payload"];
    assert_eq!(search["liked"], serde_json::json!(["0"]));
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
  fn landed_top_tracks_push_the_discover_channel_with_their_range_and_liked_marks() {
    use crate::core::app::DiscoverTimeRange;
    let (mut app, _rx) = app();
    let before = app.display_revisions();
    let mut page = liked_page(0, 2, 2, false);
    page.items[1].id = Some("liked".to_string());

    app.set_discover_top_tracks(DiscoverTimeRange::Long, page.items);
    app.liked_song_ids_set_mut().insert("liked".to_string());
    app.note_display_changes();

    let discover = &pushed(&diff(&before, &app), "discover")["payload"];
    assert_eq!(discover["top_tracks_range"], "Long");
    assert_eq!(discover["top_tracks"][1]["name"], "Track 1");
    assert_eq!(discover["liked_ids"], serde_json::json!(["liked"]));
    assert_eq!(discover["available"], true);
  }

  #[test]
  fn opening_another_range_from_the_page_fetches_it_instead_of_the_cache() {
    use crate::core::action::DiscoverTarget;
    use crate::core::app::DiscoverTimeRange;
    let (mut app, rx) = app();
    app.set_discover_top_tracks(DiscoverTimeRange::Medium, liked_page(0, 1, 1, false).items);

    apply_from_page(
      &mut app,
      &action_frame(Action::OpenDiscover(DiscoverTarget::TopTracks(
        DiscoverTimeRange::Short,
      ))),
    );

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetUserTopTracks(DiscoverTimeRange::Short))
    ));
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
