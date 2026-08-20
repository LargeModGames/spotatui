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
//! - Matches in this module are exhaustive. The
//!   `wildcard_arms_in_action_tree` gate keeps the catch-all arm count at
//!   zero, tests included.
//! - Address by identity (URIs, ids, names), never by list ordinal.
//!
//! The serde derives are the wire shape for future frontend codegen; they
//! are deliberately in place before any second frontend consumes them.

use serde::{Deserialize, Serialize};

use crate::core::plugin_api::{PluginPopup, PluginScreenContent, TrackInfo};
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
  NextTrack,
  PreviousTrack,
  /// Seek to an absolute position in the current track, in milliseconds.
  SeekTo(u32),
  /// Set the volume to an absolute percentage (0-100).
  SetVolume(u8),
  /// Ensure shuffle matches the given state (intent, not a toggle).
  SetShuffle(bool),
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
  /// Transfer playback to a Connect device. `persist` records the device as
  /// the user's saved preference; producers other than the interactive
  /// device picker pass `false`.
  TransferPlayback {
    device_id: String,
    persist: bool,
  },
  /// Add one playable URI (track or episode) to the queue.
  AddToQueue(String),
  /// Run a search; the user country is resolved at apply time.
  Search(String),
  CreatePlaylist {
    name: String,
    track_uris: Vec<String>,
  },
  AddTrackToPlaylist {
    playlist: String,
    track: String,
  },
  RemoveTrackFromPlaylist {
    playlist: String,
    track: String,
    /// 0-based position of the track within the playlist.
    position: usize,
  },
  FollowPlaylist(String),
  /// Unfollow a playlist; the current user id is resolved at apply time.
  UnfollowPlaylist(String),
  ToggleSaveTrack(String),
  SaveAlbum(String),
  UnsaveAlbum(String),
  SaveShow(String),
  UnsaveShow(String),
  FollowArtist(String),
  UnfollowArtist(String),
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
  /// Set or clear a playbar segment for a plugin (keyed by plugin name).
  SetPlaybarSegment {
    plugin: String,
    text: Option<String>,
  },
  /// Show a plugin popup dialog.
  ShowPopup(PluginPopup),
  /// Apply theme color overrides at runtime.
  SetTheme(Vec<(ThemeField, Color)>),
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
}

/// What applying an [`Action`] produced, beyond the state change itself.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionOutcome {
  /// The action was applied (or routed); nothing further to report.
  Applied,
  /// [`Action::QueueTracks`]: how many offered tracks entered the queue.
  #[cfg_attr(not(feature = "dj-core"), allow(dead_code))]
  Queued { accepted: usize },
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
