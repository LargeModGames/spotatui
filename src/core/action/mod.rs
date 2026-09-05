//! The shared action vocabulary: one enum of frontend-neutral state changes,
//! applied by [`App::apply`](crate::core::app::App).
//!
//! Every producer that is not the network layer's own result handling routes
//! its `App` mutations through here: the Lua scripting engine drains its
//! queued actions into `apply`, the mutating DJ/MCP tools call `apply`
//! directly, and the terminal handlers adopt it screen by screen as the
//! conversion sub-PRs land. Each arm delegates to the same ownership-aware
//! `App` method the equivalent keybinding uses, so the
//! `queue_owns_playback()` / `active_decoded_source()` /
//! native-streaming predicate order is evaluated in one place and the
//! native fast paths and throttling/coalescing are honoured automatically.
//!
//! Design rules (enforced by review plus the gates in `src/gates.rs`):
//!
//! - No rspotify type appears in [`Action`]; payloads are strings, scalars,
//!   and the `core::plugin_api` snapshot types.
//! - No raw `IoEvent` payload either: a producer that can smuggle an
//!   arbitrary event is outside the vocabulary. Arms may dispatch events,
//!   but the variant set is the contract.
//! - Playback starts go through `App::start_playback_uris` /
//!   `App::start_playback_context`; no arm builds a `StartPlayback` event
//!   by hand.
//! - Matches in this module are exhaustive: the deny below refuses a
//!   catch-all arm, and the `wildcard_arms_in_action_tree` gate keeps the
//!   count at zero, tests included.
//! - Address by identity (URIs, ids, names), never by list ordinal.
//!
//! The serde derives are the wire shape for future frontend codegen; they
//! are deliberately in place before any second frontend consumes them.

// The compiler half of the catch-all gate: a new variant must be placed.
#![deny(
  clippy::wildcard_enum_match_arm,
  clippy::match_wildcard_for_single_variants
)]

use serde::{Deserialize, Serialize};

use crate::core::app::DiscoverTimeRange;
use crate::core::plugin_api::{PluginPopup, PluginScreenContent, ShowInfo, TrackInfo};
use crate::core::sort::{SortContext, SortField};
use crate::core::source::Source;
use crate::core::theme::{Color, ThemeField};

mod apply;
#[cfg(test)]
mod tests;

/// A frontend-neutral state change, applied by `App::apply`.
///
/// Variant payloads are fully resolved except where a value only `App` knows
/// is needed ([`Action::Search`] resolves the user country and
/// [`Action::UnfollowPlaylist`] the user id at apply time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
  /// Start playback if it is not already playing (intent, not a toggle).
  Play,
  /// Pause playback if it is playing (intent, not a toggle).
  Pause,
  /// Toggle play/pause, matching the toggle-playback key (and MPRIS
  /// PlayPause) exactly.
  TogglePlayback,
  NextTrack,
  PreviousTrack,
  /// Always go to the previous track (or restart the current queue slot),
  /// skipping the ">= 3s restarts the current track" rule of
  /// [`Action::PreviousTrack`].
  ForcePreviousTrack,
  /// Seek to an absolute position in the current track, in milliseconds.
  SeekTo(u32),
  /// Seek forwards by the user's configured seek step.
  SeekForward,
  /// Seek backwards by the user's configured seek step.
  SeekBackward,
  /// Set the volume to an absolute percentage (0-100).
  SetVolume(u8),
  /// Raise the volume by the user's configured increment.
  VolumeUp,
  /// Lower the volume by the user's configured increment.
  VolumeDown,
  /// Ensure shuffle matches the given state (intent, not a toggle).
  SetShuffle(bool),
  /// Toggle shuffle, matching the shuffle key exactly (source-gated: decoded
  /// queues reorder in place, radio and the native queue slot no-op).
  ToggleShuffle,
  /// Cycle repeat off, then context, then track, matching the repeat key.
  CycleRepeat,
  /// Set repeat to an absolute mode. Mirrors the historic Lua `set_repeat`:
  /// this is the Web API path only; an ownership-aware absolute setter does
  /// not exist yet.
  SetRepeat(RepeatSetting),
  /// Play an explicit list of playable URIs, optionally from an offset into
  /// that list.
  PlayUris {
    uris: Vec<String>,
    offset: Option<usize>,
  },
  /// Play a Spotify context URI (album/playlist/artist/show), optionally
  /// from a 0-based track offset.
  PlayContext {
    uri: String,
    offset: Option<usize>,
  },
  /// Play one track URI as the first track inside a Spotify context URI:
  /// the playlist Enter-start that keeps the selected track first even with
  /// shuffle on (the network layer deliberately does not trim the uri list
  /// when a context is present).
  PlayTrackInContext {
    context: String,
    track: String,
  },
  /// Transfer playback to a Connect device. `persist` records the device as
  /// the user's saved preference; producers other than the interactive
  /// device picker pass `false`.
  TransferPlayback {
    device_id: String,
    persist: bool,
  },
  /// Add one playable URI (track or episode) to the queue.
  AddToQueue(String),
  /// Add one track to the native cross-source queue (the queue key). Distinct
  /// from [`Action::AddToQueue`], the Spotify Web API queue.
  QueueTrack(TrackInfo),
  /// Jump to one native-queue item, addressed by URI with the 0-based position
  /// as the tie-breaker for duplicates.
  PlayQueueItem {
    uri: String,
    position: usize,
  },
  /// Addressed like [`Action::PlayQueueItem`].
  RemoveFromQueue {
    uri: String,
    position: usize,
  },
  /// Move a native-queue item to an absolute index; addressed like
  /// [`Action::PlayQueueItem`], out of range is a no-op.
  MoveQueueItem {
    uri: String,
    from: usize,
    to: usize,
  },
  /// Run a search against the Spotify catalog; the user country is resolved
  /// at apply time. Deliberately source-blind: existing producers (Lua
  /// `spotatui.search`) contracted the Web API search, so browsing scope
  /// does not reroute it. The terminal search box uses
  /// [`Action::SearchActiveSource`] instead.
  Search(String),
  /// Run a search against the active browse source's catalog; falls back to
  /// the Spotify catalog when the active source has no own search (the
  /// Local-files scope behaves this way too).
  SearchActiveSource(String),
  /// Search one playlist's tracks; apply records the pending search and
  /// announces it, exactly like the playlist filter input does today.
  SearchPlaylistTracks {
    playlist_id: String,
    query: String,
  },
  CreatePlaylist {
    name: String,
    track_uris: Vec<String>,
  },
  /// Create a playlist in the local YouTube playlists file; a no-op without
  /// the `youtube` feature. [`Action::CreatePlaylist`] stays Spotify-only.
  CreateYouTubePlaylist(String),
  /// Search for tracks to add to the playlist being composed; results land
  /// in the create form's candidate list, not the search screen.
  SearchTracksForPlaylist(String),
  /// `playlist` is a URI: `youtube:playlist:` edits the local YouTube file,
  /// anything else is the Spotify add (bare id or `spotify:playlist:` URI).
  AddTrackToPlaylist {
    playlist: String,
    track: String,
  },
  /// Routed like [`Action::AddTrackToPlaylist`]; `position` is only read by
  /// the Spotify removal.
  RemoveTrackFromPlaylist {
    playlist: String,
    track: String,
    /// 0-based position of the track within the playlist.
    position: usize,
  },
  FollowPlaylist(String),
  /// Unfollow a playlist; the current user id is resolved at apply time.
  UnfollowPlaylist(String),
  /// Delete a local `youtube:playlist:` playlist; Spotify playlists leave
  /// through [`Action::UnfollowPlaylist`].
  DeletePlaylist(String),
  /// Save or unsave a track by bare base62 id or `spotify:track:` URI; the
  /// network layer accepts both.
  ToggleSaveTrack(String),
  /// Save or unsave whatever is playing now, resolved through the
  /// playback-ownership order at apply time (the native queue slot's own
  /// track before the cached Spotify context).
  ToggleSaveCurrentItem,
  SaveAlbum(String),
  UnsaveAlbum(String),
  SaveShow(String),
  UnsaveShow(String),
  FollowArtist(String),
  UnfollowArtist(String),
  /// The spotatui.com social graph (friend code / user id), not Spotify.
  AddFriendByCode(String),
  AddFriendById(String),
  UnfollowFriend(String),
  /// A query under the server's two-byte minimum clears the stale results
  /// instead of asking. The network layer only accepts a result while the
  /// live search buffer still equals the query.
  SearchFriendUsers(String),
  /// Persist an internet-radio station and mirror it into the sidebar.
  FavoriteRadioStation(TrackInfo),
  /// Remove a saved station by `radio:` URI.
  RemoveRadioStation(String),
  /// message, ttl_secs
  Notify(String, u64),
  /// Error message, ttl_secs. Always shown; blocks normal message
  /// overwrites until it expires.
  NotifyError(String, u64),
  /// Navigate to a top-level surface; apply mirrors the matching keybinding
  /// exactly.
  Navigate(NavTarget),
  /// Pop the navigation stack (same as the back key).
  Back,
  /// Fetch the next page of a paginated list surface - the shared "hit the
  /// end of the list" consequence that GUI infinite scroll also fires.
  /// Self-guarding: a no-op when no next page exists. Page-flipped surfaces
  /// (saved shows, show episodes) move the visible page when it is cached.
  LoadMore(ListTarget),
  /// Sort a list surface by a field, like the sort menu: the field already
  /// in effect flips the direction instead.
  Sort {
    context: SortContext,
    field: SortField,
  },
  /// Flip the recorded sort direction without re-sorting loaded rows.
  ToggleSortOrder(SortContext),
  /// Open a resource page (album/artist/show/playlist) by id. Never starts
  /// playback; Open and Play stay disjoint.
  Open(OpenTarget),
  /// Open a show's episode list from a snapshot the producer holds
  /// ([`OpenTarget::Show`] only knows an id). Top-level because `ShowInfo`
  /// has no `Eq`.
  OpenShowEpisodes(ShowInfo),
  /// Open a library sidebar section and fetch its data, mirroring the
  /// library row's Enter consequence exactly.
  OpenLibrary(LibraryTarget),
  /// Show a Discover row's cached mix, or fetch it.
  OpenDiscover(DiscoverTarget),
  /// Switch the active browse source (and persist the choice), then fetch
  /// the sidebar data that source needs. Browse scope only: never
  /// interrupts playback.
  SelectSource(Source),
  /// Open the add-to-playlist picker for the track table's current
  /// selection; resolved from the selection at apply time.
  OpenAddTrackDialog,
  /// The picker for one named track; `track_id` is `None` when the item
  /// cannot be added (a local file, an episode) and apply reports that.
  OpenAddTrackDialogFor {
    track_id: Option<String>,
    track_name: String,
  },
  /// The picker for the item playing now, resolved through the ownership
  /// order at apply time.
  OpenAddPlayingTrackDialog,
  /// Stage a remove-track-from-playlist confirmation for the track table's
  /// current selection; resolved from the selection at apply time.
  OpenRemoveTrackDialog,
  /// Open the album page of the item that is playing now; resolved through
  /// the ownership order at apply time (episodes open their show).
  JumpToAlbum,
  /// Open the album list of the first artist of the track that is playing
  /// now; resolved through the ownership order at apply time.
  JumpToArtist,
  /// Open the context (album/artist/playlist) that playback runs in;
  /// resolved through the ownership order at apply time.
  JumpToContext,
  /// Copy a share URL for the item playing now to the clipboard; a silent
  /// no-op without playback or a clipboard.
  CopyUrl(CopyTarget),
  /// Generate a listening recap. The period is resolved at apply time: the
  /// selected period on the Stats screen when that screen is current, 30
  /// days anywhere else.
  GenerateRecap,
  /// Step the Stats period through its ring and reload; relative because the
  /// period type is not part of the wire shape.
  CycleStatsPeriod {
    forward: bool,
  },
  /// Seed the track-radio recommendations flow from one track. The full
  /// snapshot rides along because the results table prepends it as the
  /// context row.
  RecommendFromTrack(TrackInfo),
  /// `name` is the seed label the results header shows.
  RecommendFromArtist {
    id: String,
    name: String,
  },
  /// Like [`Action::RecommendFromTrack`] but prepends no context row.
  RecommendFromTrackId {
    id: String,
    name: String,
  },
  /// Always starts in host-only control; [`Action::TogglePartyControlMode`]
  /// is the only mode change.
  StartParty,
  JoinParty {
    code: String,
    name: String,
  },
  LeaveParty,
  /// A no-op without a session.
  TogglePartyControlMode,
  /// Set or clear a playbar segment for a plugin (keyed by plugin name).
  SetPlaybarSegment {
    plugin: String,
    text: Option<String>,
  },
  /// Show a plugin popup dialog.
  ShowPopup(PluginPopup),
  /// Dismiss the plugin popup, if one is shown.
  ClosePopup,
  /// Apply theme color overrides at runtime.
  SetTheme(Vec<(ThemeField, Color)>),
  /// Commit the Settings screen's staged rows and write `config.yml`; the
  /// outcome says whether the write succeeded.
  SaveSettings,
  /// Cycle the visualizer style and persist it.
  CycleVisualizerStyle,
  /// Answer the community-playlist pin prompt: keep the pin, or hide it and
  /// persist that. Either way the prompt never asks again.
  AnswerCommunityPinPrompt {
    keep: bool,
  },
  /// Open the monthly recap in the browser and close its popup.
  OpenRecap,
  /// Close the recap popup without opening the recap.
  DismissRecapPrompt,
  /// Close the recap popup and switch the monthly prompt off in `config.yml`.
  DisableRecapPrompt,
  /// Publish (retained) content for a registered plugin screen.
  SetScreenContent {
    name: String,
    content: PluginScreenContent,
  },
  /// Navigate to a registered plugin screen.
  ShowScreen(String),
  /// Pop the named plugin screen if it is the current route.
  CloseScreen(String),
  /// Queue a batch of DJ-chosen tracks; the outcome reports how many were
  /// accepted. A no-op outside `dj-core` builds.
  #[cfg_attr(not(feature = "dj-core"), allow(dead_code))]
  QueueTracks(Vec<TrackInfo>),
  /// Set (or clear, with `None`) the DJ's standing vibe. Bumps the DJ
  /// generation exactly once; the in-TUI agent's adopt-one-bump rule
  /// depends on that exact count. A no-op outside `dj-core` builds.
  #[cfg_attr(not(feature = "dj-core"), allow(dead_code))]
  SetDjVibe(Option<String>),
  /// The DJ screen's own keys; no-ops outside `ai-dj` builds. `AskDj` refuses
  /// a turn already in flight and bumps the DJ generation exactly once.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  AskDj(String),
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  DjVibeShift,
  /// A toggle, not a setter: the body bumps the generation and may dispatch
  /// on every call.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  ToggleDjAutoQueue,
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  ToggleDjFreshOnly,
  /// Opens the DJ screen first when it is not current; does not consult the
  /// configured flag.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  OpenDjSetup,
  /// Close the DJ brain picker without changing the brain, and persist the
  /// "answered" marker so it does not reappear.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  DismissDjSetup,
  /// Apply the finished DJ brain picker, close it, and persist the choice.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  CommitDjSetup,
}

/// What applying an [`Action`] produced, beyond the state change itself.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionOutcome {
  /// The action was applied (or routed); nothing further to report.
  Applied,
  /// [`Action::QueueTracks`]: how many offered tracks entered the queue.
  #[cfg_attr(not(feature = "dj-core"), allow(dead_code))]
  Queued { accepted: usize },
  /// [`Action::SaveSettings`]: whether `config.yml` was written.
  SettingsSaved { saved: bool },
}

/// An absolute repeat mode, mirroring Spotify's three states without the
/// rspotify type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatSetting {
  Off,
  Track,
  Context,
}

/// Surfaces reachable through [`Action::Navigate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTarget {
  Home,
  Queue,
  Settings,
  Devices,
  Help,
  Lyrics,
  RecentlyPlayed,
  Party,
  Analysis,
  MiniPlayer,
}

impl NavTarget {
  /// Every target, in the order the Lua API has always advertised them.
  pub const ALL: [NavTarget; 10] = [
    NavTarget::Home,
    NavTarget::Queue,
    NavTarget::Settings,
    NavTarget::Devices,
    NavTarget::Help,
    NavTarget::Lyrics,
    NavTarget::RecentlyPlayed,
    NavTarget::Party,
    NavTarget::Analysis,
    NavTarget::MiniPlayer,
  ];

  /// The name used by `spotatui.navigate(name)`.
  pub fn name(self) -> &'static str {
    match self {
      NavTarget::Home => "home",
      NavTarget::Queue => "queue",
      NavTarget::Settings => "settings",
      NavTarget::Devices => "devices",
      NavTarget::Help => "help",
      NavTarget::Lyrics => "lyrics",
      NavTarget::RecentlyPlayed => "recently_played",
      NavTarget::Party => "party",
      NavTarget::Analysis => "analysis",
      NavTarget::MiniPlayer => "miniplayer",
    }
  }

  /// Look up a target by name; `None` for unknown names.
  pub fn from_name(name: &str) -> Option<NavTarget> {
    NavTarget::ALL.into_iter().find(|t| t.name() == name)
  }
}

/// A paginated list surface [`Action::LoadMore`] can advance. Grows
/// additively as later screens adopt continuous pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListTarget {
  /// The open playlist's track table (MyPlaylists / PlaylistSearch).
  PlaylistTracks,
  /// The liked-songs (saved tracks) table.
  SavedTracks,
  /// The saved-podcasts list (page-flipped, not continuous).
  SavedShows,
  /// The open show's episode list; the show is resolved at apply time.
  ShowEpisodes,
}

/// A resource deep-link [`Action::Open`] can address, by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenTarget {
  /// Open an album page. `from_search` pins the track table to the
  /// album-search context first, matching the search-results Enter; every
  /// other producer passes `false` and leaves the table untouched.
  Album { id: String, from_search: bool },
  /// Open a saved album from the saved-albums cache; a truncated tracklist is
  /// refetched in full.
  SavedAlbum(String),
  /// Open an artist page. The display name rides along because
  /// `App::get_artist` seeds the header with it before data arrives.
  Artist { id: String, name: String },
  /// Open a playlist's track table. `from_search` selects the table context
  /// the producer uses (search results vs the user's playlists), matching
  /// what each opening path has always passed.
  Playlist { id: String, from_search: bool },
  /// A decoded source's playlist or folder, routed by URI scheme (`file:`,
  /// `subsonic:`, `youtube:playlist:`, `qobuz:`); an unknown scheme is a no-op.
  SourcePlaylist(String),
  /// Scope the playlist sidebar to a rootlist folder id (session-local, not
  /// to be persisted).
  PlaylistFolder(usize),
  /// Open a podcast show's episode list.
  Show(String),
  /// Open the album containing this track.
  TrackAlbum(String),
}

/// Library sidebar sections reachable through [`Action::OpenLibrary`].
/// Deliberately NOT advertised through the Lua `navigate()` API (unlike
/// [`NavTarget`]): adding rows there would expand the published plugin
/// surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LibraryTarget {
  #[default]
  Discover,
  RecentlyPlayed,
  Friends,
  Stats,
  LikedSongs,
  Albums,
  Artists,
  Podcasts,
  /// Row only present under `local-files`; a no-op without it.
  LocalFiles,
  /// Row only present under `ai-dj`; a no-op without it.
  AiDj,
}

impl LibraryTarget {
  /// Every target, in the order of `library_row_requirements()`.
  #[cfg(test)]
  pub const ALL: [LibraryTarget; 10] = [
    LibraryTarget::Discover,
    LibraryTarget::RecentlyPlayed,
    LibraryTarget::Friends,
    LibraryTarget::Stats,
    LibraryTarget::LikedSongs,
    LibraryTarget::Albums,
    LibraryTarget::Artists,
    LibraryTarget::Podcasts,
    LibraryTarget::LocalFiles,
    LibraryTarget::AiDj,
  ];

  /// The sidebar label for this target.
  pub fn name(self) -> &'static str {
    match self {
      LibraryTarget::Discover => "Discover",
      LibraryTarget::RecentlyPlayed => "Recently Played",
      LibraryTarget::Friends => "Friends",
      LibraryTarget::Stats => "Stats",
      LibraryTarget::LikedSongs => "Liked Songs",
      LibraryTarget::Albums => "Albums",
      LibraryTarget::Artists => "Artists",
      LibraryTarget::Podcasts => "Podcasts",
      LibraryTarget::LocalFiles => "Local Files",
      LibraryTarget::AiDj => "AI DJ",
    }
  }
}

/// Which Discover row [`Action::OpenDiscover`] activates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoverTarget {
  ArtistsMix,
  TopTracks(DiscoverTimeRange),
}

/// What [`Action::CopyUrl`] copies; both address the item playing now (an
/// episode gives its episode / show URL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyTarget {
  CurrentSong,
  CurrentAlbum,
}
