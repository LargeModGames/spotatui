//! The DJ tool surface: one definition, two consumers.
//!
//! The MCP server (`mcp-server`) publishes these verbatim as MCP tools; the
//! in-TUI DJ (`ai-dj`) calls the same handlers directly. Keeping the list here
//! means an agent driving spotatui over MCP and the built-in DJ can never drift
//! apart in what they are able to do.
//!
//! Schemas are JSON Schema 2020-12, which is the MCP default dialect (a tool
//! taking no arguments uses `{"type":"object","additionalProperties":false}`, the
//! spec's recommended empty schema). The list order is fixed and deterministic:
//! MCP clients cache `tools/list`, and a stable order also improves prompt-cache
//! hit rates when the tools are rendered into model context.

use super::{brief, DjSuggestion, MAX_BATCH};
use crate::core::action::Action;
use crate::core::app::App;
use crate::core::plugin_api::PlaylistInfo;
use crate::infra::network::ids;
use crate::infra::network::search::SPOTIFY_SEARCH_LIMIT;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Static description of one tool.
pub struct ToolSpec {
  pub name: &'static str,
  /// MCP-only: the in-TUI DJ renders `name` and `description` alone.
  #[cfg_attr(not(feature = "mcp-server"), allow(dead_code))]
  pub title: &'static str,
  pub description: &'static str,
  /// Whether the tool only reads state. Surfaced to MCP clients so they can
  /// decide what to confirm with the user.
  pub read_only: bool,
  /// Whether a call of it can need the live Spotify client (and therefore the
  /// Spotify lane) rather than just the `App` lock. A call with no Spotify
  /// item never does: see `DjToolCall::needs_network`.
  pub needs_network: bool,
  /// Offered to the in-app DJ too. The library, playlist and device tools are
  /// MCP-only: an agent there acts for a user who asked it to.
  #[cfg_attr(not(feature = "ai-dj"), allow(dead_code))]
  pub dj: bool,
  /// Removes something the user keeps; MCP clients confirm it with the user.
  #[cfg_attr(not(feature = "mcp-server"), allow(dead_code))]
  pub destructive: bool,
  pub schema: fn() -> Value,
}

/// Most URIs one library or playlist write takes, so a call stays a few requests.
pub const WRITE_BATCH: usize = 20;

fn uris_schema(description: &str) -> Value {
  json!({
    "type": "array",
    "minItems": 1,
    "maxItems": WRITE_BATCH,
    "items": {"type": "string"},
    "description": description,
  })
}

/// Every tool, in a fixed order.
pub const TOOLS: &[ToolSpec] = &[
  ToolSpec {
    name: "get_listening_history",
    title: "Listening history",
    description: "Summarise what the user has been listening to: top artists, top tracks, top albums, and recent plays. Call this first when choosing music for them. Returns aggregate names only — no identifiers or timestamps.",
    read_only: true,
    needs_network: false,
    dj: true,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "period": {
            "type": "string",
            "enum": ["7d", "30d", "month", "year", "all"],
            "description": "Window to summarise. Defaults to 30d."
          }
        },
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "get_now_playing",
    title: "Now playing",
    description: "The currently playing or paused track, whether playback is active, and how many tracks are waiting in the queue.",
    read_only: true,
    needs_network: false,
    dj: true,
    destructive: false,
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "get_queue",
    title: "Queue",
    description: "The tracks currently queued up, in play order.",
    read_only: true,
    needs_network: false,
    dj: true,
    destructive: false,
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "search_tracks",
    title: "Search tracks",
    description: "Search the catalogue for tracks and return their playable URIs. Use this to check that something exists before queueing it by name. Each result is marked `owned` when the user already has it (Liked Songs, or a playlist they own or collaborate on) — prefer results marked new when they asked for something they have not heard.",
    read_only: true,
    needs_network: true,
    dj: true,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "query": {"type": "string", "description": "Free-text search, e.g. 'radiohead weird fishes'."},
          "limit": {"type": "integer", "minimum": 1, "maximum": SPOTIFY_SEARCH_LIMIT, "description": format!("Maximum results. Defaults to {SPOTIFY_SEARCH_LIMIT}.")}
        },
        "required": ["query"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "queue_tracks",
    title: "Queue tracks",
    description: "Add tracks to the play queue. Give either a `uri` from search_tracks, or a `title` plus `artist` to be looked up. Tracks that cannot be found are skipped and reported back, so check the result rather than assuming everything was queued.",
    read_only: false,
    needs_network: true,
    dj: true,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "tracks": {
            "type": "array",
            "minItems": 1,
            "maxItems": MAX_BATCH,
            "description": "Tracks to queue, in play order.",
            "items": {
              "type": "object",
              "properties": {
                "uri": {"type": "string", "description": "Exact URI from search_tracks. Preferred when known."},
                "title": {"type": "string"},
                "artist": {"type": "string"}
              },
              "additionalProperties": false
            }
          },
          "exclude_owned": {
            "type": "boolean",
            "description": "Skip tracks the user already has (Liked Songs, or a playlist they own or collaborate on) instead of queueing them; the skipped ones are reported back. Defaults to false. Set it when they asked for music they have not heard, not when they named specific tracks. Nothing is substituted for a skipped track, so fewer may be queued than requested."
          }
        },
        "required": ["tracks"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "play_now",
    title: "Play now",
    description: "Start playing a track immediately, interrupting whatever is playing. Use queue_tracks instead unless the user asked for something right now. The track has to exist: give a URI from search_tracks rather than one you assembled yourself.",
    read_only: false,
    needs_network: true,
    dj: true,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {"uri": {"type": "string", "description": "A track URI from search_tracks."}},
        "required": ["uri"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "skip_track",
    title: "Skip track",
    description: "Skip to the next track.",
    read_only: false,
    needs_network: false,
    dj: true,
    destructive: false,
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "set_dj_vibe",
    title: "Set DJ vibe",
    description: "Set the standing direction the built-in auto-queue DJ follows when it refills the queue on its own, e.g. 'mellow instrumental for focusing'. Pass null to clear it. This does not start playback or queue anything. The built-in DJ is optional and may be switched off or not built in at all, so read the result: it says whether anything will act on the vibe. Either way it is stored and returned by get_listening_history, so you can honour it yourself when you queue.",
    read_only: false,
    needs_network: false,
    dj: true,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {"vibe": {"type": ["string", "null"], "description": "The direction to follow, or null to clear."}},
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "list_playlists",
    title: "List playlists",
    description: "The user's Spotify playlists in sidebar order, with their URIs, track counts and whether the user owns them. Use a URI from here with add_to_playlist or remove_from_playlist; only a playlist the user owns or collaborates on can be edited.",
    read_only: true,
    needs_network: false,
    dj: false,
    destructive: false,
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "get_album",
    title: "Get album",
    description: "An album's artists, release year and track list, with a URI for each track.",
    read_only: true,
    needs_network: true,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {"album": {"type": "string", "description": "A spotify:album: URI or album id."}},
        "required": ["album"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "get_artist",
    title: "Get artist",
    description: "An artist's top tracks and albums, each with a URI.",
    read_only: true,
    needs_network: true,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {"artist": {"type": "string", "description": "A spotify:artist: URI or artist id."}},
        "required": ["artist"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "list_devices",
    title: "List devices",
    description: "The Spotify Connect devices of the user's account, which one is active, and their volume.",
    read_only: true,
    needs_network: true,
    dj: false,
    destructive: false,
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "transfer_playback",
    title: "Transfer playback",
    description: "Move Spotify playback to another Connect device, by the id or the name list_devices gives.",
    read_only: false,
    needs_network: true,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {"device": {"type": "string", "description": "A device id or name from list_devices."}},
        "required": ["device"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "create_playlist",
    title: "Create playlist",
    description: "Create a Spotify playlist, optionally with tracks. It appears in list_playlists once Spotify has made it.",
    read_only: false,
    needs_network: false,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "name": {"type": "string"},
          "uris": uris_schema("Track URIs from search_tracks to start the playlist with.")
        },
        "required": ["name"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "add_to_playlist",
    title: "Add to playlist",
    description: "Add tracks to the end of a playlist the user owns or collaborates on.",
    read_only: false,
    needs_network: false,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "playlist": {"type": "string", "description": "A playlist URI from list_playlists."},
          "uris": uris_schema("Track URIs from search_tracks, in order.")
        },
        "required": ["playlist", "uris"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "remove_from_playlist",
    title: "Remove from playlist",
    description: "Remove one track from a playlist the user owns. Give the track's URI and its 0-based position in the playlist; nothing is removed when the track at that position is a different one.",
    read_only: false,
    needs_network: false,
    dj: false,
    destructive: true,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "playlist": {"type": "string", "description": "A playlist URI from list_playlists."},
          "uri": {"type": "string", "description": "The URI of the track to remove."},
          "position": {"type": "integer", "minimum": 0, "description": "The track's 0-based position."}
        },
        "required": ["playlist", "uri", "position"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "like_tracks",
    title: "Like tracks",
    description: "Add tracks to the user's Liked Songs, or remove them with liked set to false. Tracks already in that state are left alone.",
    read_only: false,
    needs_network: true,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "uris": uris_schema("Track URIs from search_tracks."),
          "liked": {"type": "boolean", "description": "False removes them from Liked Songs. Defaults to true."}
        },
        "required": ["uris"],
        "additionalProperties": false
      })
    },
  },
  ToolSpec {
    name: "follow_artist",
    title: "Follow artist",
    description: "Follow an artist, or unfollow them with follow set to false.",
    read_only: false,
    needs_network: false,
    dj: false,
    destructive: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "artist": {"type": "string", "description": "A spotify:artist: URI or artist id."},
          "follow": {"type": "boolean", "description": "False unfollows. Defaults to true."}
        },
        "required": ["artist"],
        "additionalProperties": false
      })
    },
  },
];

/// The tools the in-app DJ may call.
#[cfg(any(test, feature = "ai-dj"))]
pub fn dj_tools() -> impl Iterator<Item = &'static ToolSpec> {
  TOOLS.iter().filter(|tool| tool.dj)
}

/// [`parse_call`] for the in-app DJ: an MCP-only tool is unknown to it.
#[cfg(any(test, feature = "ai-dj"))]
pub fn parse_dj_call(name: &str, args: &Value) -> Result<DjToolCall, ToolCallError> {
  if !spec(name).is_some_and(|tool| tool.dj) {
    return Err(ToolCallError::UnknownTool(name.to_string()));
  }
  parse_call(name, args)
}

pub fn spec(name: &str) -> Option<&'static ToolSpec> {
  TOOLS.iter().find(|tool| tool.name == name)
}

/// A validated tool invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DjToolCall {
  GetListeningHistory {
    period: crate::infra::history::RecapPeriod,
  },
  GetNowPlaying,
  GetQueue,
  SearchTracks {
    query: String,
    limit: usize,
  },
  QueueTracks {
    items: Vec<QueueItem>,
    /// Reject anything the listener already has rather than queueing it.
    ///
    /// Off by default, and deliberately so: an agent told to queue one specific
    /// track the listener owns should get that track, not an explanation. It is
    /// opt-in for the *recommendation* case, where "you already have this" is the
    /// answer the caller actually wants.
    exclude_owned: bool,
    /// Extra [`super::dedupe_key`] values to reject, on top of what is queued and
    /// playing.
    ///
    /// Not an argument any caller can pass: [`parse_call`] always leaves it empty,
    /// so an MCP agent told to queue a specific track always gets that track. The
    /// in-TUI DJ fills it with its recently-played window, which is a taste rule
    /// belonging to that front door alone — put in `App::dj_skip_keys` it would
    /// silently drop tracks an agent explicitly asked for.
    extra_skip_keys: Vec<String>,
  },
  PlayNow {
    uri: String,
  },
  SkipTrack,
  SetDjVibe {
    vibe: Option<String>,
  },
  ListPlaylists,
  /// A bare album id.
  GetAlbum {
    album: String,
  },
  /// A bare artist id.
  GetArtist {
    artist: String,
  },
  ListDevices,
  TransferPlayback {
    device: String,
  },
  CreatePlaylist {
    name: String,
    uris: Vec<String>,
  },
  /// `playlist` is a `spotify:playlist:` URI, `uris` are `spotify:track:` URIs.
  AddToPlaylist {
    playlist: String,
    uris: Vec<String>,
  },
  RemoveFromPlaylist {
    playlist: String,
    uri: String,
    position: usize,
  },
  LikeTracks {
    uris: Vec<String>,
    liked: bool,
  },
  /// A bare artist id.
  FollowArtist {
    artist: String,
    follow: bool,
  },
}

/// One entry of `queue_tracks`: either an exact URI or a name to resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueItem {
  Uri(String),
  Named(DjSuggestion),
}

/// Why a call could not be parsed.
///
/// Distinct from an execution failure: per MCP, a bad request shape is a
/// protocol error while a tool that ran and failed is a result with
/// `isError: true`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolCallError {
  UnknownTool(String),
  InvalidArguments(String),
}

impl std::fmt::Display for ToolCallError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::UnknownTool(name) => write!(f, "Unknown tool: {name}"),
      Self::InvalidArguments(detail) => write!(f, "Invalid arguments: {detail}"),
    }
  }
}

/// What a tool produced.
pub struct ToolOutcome {
  /// Human/model-readable text. Always populated.
  pub text: String,
  /// Machine-readable mirror, when the tool has one. MCP-only: the in-TUI DJ
  /// feeds `text` back to its model, which reads prose perfectly well.
  #[cfg_attr(not(feature = "mcp-server"), allow(dead_code))]
  pub structured: Option<Value>,
  /// True for an execution failure the model can act on and retry.
  pub is_error: bool,
}

impl ToolOutcome {
  pub fn ok(text: impl Into<String>) -> Self {
    Self {
      text: text.into(),
      structured: None,
      is_error: false,
    }
  }

  pub fn with_data(text: impl Into<String>, structured: Value) -> Self {
    Self {
      text: text.into(),
      structured: Some(structured),
      is_error: false,
    }
  }

  pub fn error(text: impl Into<String>) -> Self {
    Self {
      text: text.into(),
      structured: None,
      is_error: true,
    }
  }
}

fn period_from_str(value: &str) -> Option<crate::infra::history::RecapPeriod> {
  use crate::infra::history::RecapPeriod;
  match value {
    "7d" => Some(RecapPeriod::SevenDays),
    "30d" => Some(RecapPeriod::ThirtyDays),
    "month" => Some(RecapPeriod::Month),
    "year" => Some(RecapPeriod::Year),
    "all" => Some(RecapPeriod::All),
    _ => None,
  }
}

/// Validate `(name, arguments)` into a [`DjToolCall`].
///
/// Every input is checked here rather than in the handlers, because MCP requires
/// servers to validate tool inputs and because both front doors funnel through
/// this one function.
pub fn parse_call(name: &str, args: &Value) -> Result<DjToolCall, ToolCallError> {
  use crate::infra::history::RecapPeriod;
  let invalid = |detail: &str| ToolCallError::InvalidArguments(detail.to_string());
  // An absent `arguments` is equivalent to `{}` for the no-argument tools.
  let obj = match args {
    Value::Null => &Value::Null,
    Value::Object(_) => args,
    _ => return Err(invalid("arguments must be an object")),
  };
  let get = |key: &str| obj.get(key);

  match name {
    "get_listening_history" => {
      let period = match get("period") {
        None | Some(Value::Null) => RecapPeriod::ThirtyDays,
        Some(Value::String(value)) => period_from_str(value)
          .ok_or_else(|| invalid("period must be one of 7d, 30d, month, year, all"))?,
        Some(_) => return Err(invalid("period must be a string")),
      };
      Ok(DjToolCall::GetListeningHistory { period })
    }
    "get_now_playing" => Ok(DjToolCall::GetNowPlaying),
    "get_queue" => Ok(DjToolCall::GetQueue),
    "search_tracks" => {
      let query = get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("query is required and must be a string"))?
        .trim()
        .to_string();
      if query.is_empty() {
        return Err(invalid("query must not be empty"));
      }
      let limit = match get("limit") {
        None | Some(Value::Null) => SPOTIFY_SEARCH_LIMIT as usize,
        Some(value) => value
          .as_u64()
          .filter(|n| (1..=u64::from(SPOTIFY_SEARCH_LIMIT)).contains(n))
          .ok_or_else(|| {
            invalid(&format!(
              "limit must be an integer between 1 and {SPOTIFY_SEARCH_LIMIT}"
            ))
          })? as usize,
      };
      Ok(DjToolCall::SearchTracks { query, limit })
    }
    "queue_tracks" => {
      let entries = get("tracks")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("tracks is required and must be an array"))?;
      if entries.is_empty() {
        return Err(invalid("tracks must not be empty"));
      }
      if entries.len() > MAX_BATCH {
        return Err(ToolCallError::InvalidArguments(format!(
          "tracks must contain at most {MAX_BATCH} entries"
        )));
      }
      let mut items = Vec::with_capacity(entries.len());
      for entry in entries {
        let uri = entry.get("uri").and_then(Value::as_str);
        let title = entry.get("title").and_then(Value::as_str);
        let artist = entry.get("artist").and_then(Value::as_str);
        match (uri, title, artist) {
          (Some(uri), _, _) if !uri.trim().is_empty() => {
            items.push(QueueItem::Uri(uri.trim().to_string()))
          }
          // Both halves have to be real: a blank artist resolves to nothing, so
          // accepting it would turn an argument error into a silent "not found".
          (_, Some(title), Some(artist))
            if !title.trim().is_empty() && !artist.trim().is_empty() =>
          {
            items.push(QueueItem::Named(DjSuggestion {
              title: title.trim().to_string(),
              artist: artist.trim().to_string(),
              why: None,
            }))
          }
          _ => {
            return Err(invalid(
              "each track needs either a uri, or both title and artist",
            ))
          }
        }
      }
      let exclude_owned = match get("exclude_owned") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(invalid("exclude_owned must be a boolean")),
      };
      Ok(DjToolCall::QueueTracks {
        items,
        exclude_owned,
        // Never from the wire: see the field's own note.
        extra_skip_keys: Vec::new(),
      })
    }
    "play_now" => {
      let uri = get("uri")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|uri| !uri.is_empty())
        .ok_or_else(|| invalid("uri is required and must be a non-empty string"))?
        .to_string();
      // Rejected at parse time, unlike `queue_tracks`: this tool takes exactly
      // one URI, so there is no partial success to report and an invalid
      // argument is the whole request.
      if !crate::core::queue::is_playable_track_uri(&uri) {
        return Err(invalid(
          "uri must name a single playable track (spotify:track:…, file:…, \
           subsonic:…, youtube:…, or qobuz:track:…); album, playlist and https \
           links are not playable here",
        ));
      }
      Ok(DjToolCall::PlayNow { uri })
    }
    "skip_track" => Ok(DjToolCall::SkipTrack),
    "set_dj_vibe" => {
      let vibe = match get("vibe") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => {
          let trimmed = value.trim();
          if trimmed.is_empty() {
            None
          } else {
            Some(trimmed.to_string())
          }
        }
        Some(_) => return Err(invalid("vibe must be a string or null")),
      };
      Ok(DjToolCall::SetDjVibe { vibe })
    }
    "list_playlists" => Ok(DjToolCall::ListPlaylists),
    "list_devices" => Ok(DjToolCall::ListDevices),
    "get_album" => Ok(DjToolCall::GetAlbum {
      album: spotify_id(get("album"), "album", ids::album_id)?,
    }),
    "get_artist" => Ok(DjToolCall::GetArtist {
      artist: spotify_id(get("artist"), "artist", ids::artist_id)?,
    }),
    "follow_artist" => Ok(DjToolCall::FollowArtist {
      artist: spotify_id(get("artist"), "artist", ids::artist_id)?,
      follow: flag(get("follow"), "follow")?,
    }),
    "transfer_playback" => {
      let device = get("device")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|device| !device.is_empty())
        .ok_or_else(|| invalid("device is required and must be a non-empty string"))?;
      Ok(DjToolCall::TransferPlayback {
        device: device.to_string(),
      })
    }
    "create_playlist" => {
      let name = get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("name is required and must be a non-empty string"))?;
      let uris = match get("uris") {
        None | Some(Value::Null) => Vec::new(),
        uris => track_uris(uris)?,
      };
      Ok(DjToolCall::CreatePlaylist {
        name: name.to_string(),
        uris,
      })
    }
    "add_to_playlist" => Ok(DjToolCall::AddToPlaylist {
      playlist: playlist_uri(get("playlist"))?,
      uris: track_uris(get("uris"))?,
    }),
    "remove_from_playlist" => {
      let uri = track_uris(Some(&json!([get("uri").cloned().unwrap_or(Value::Null)])))
        .map_err(|_| invalid("uri must be a spotify:track: URI or track id"))?
        .remove(0);
      let position = get("position")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("position is required and must be a non-negative integer"))?;
      Ok(DjToolCall::RemoveFromPlaylist {
        playlist: playlist_uri(get("playlist"))?,
        uri,
        position: position as usize,
      })
    }
    "like_tracks" => Ok(DjToolCall::LikeTracks {
      uris: track_uris(get("uris"))?,
      liked: flag(get("liked"), "liked")?,
    }),
    other => Err(ToolCallError::UnknownTool(other.to_string())),
  }
}

/// A Spotify id from a URI or a bare id of `kind`.
fn spotify_id<T: rspotify::prelude::Id>(
  value: Option<&Value>,
  kind: &str,
  parse: fn(&str) -> Option<T>,
) -> Result<String, ToolCallError> {
  value
    .and_then(Value::as_str)
    .and_then(|value| parse(value.trim()))
    .map(|id| id.id().to_string())
    .ok_or_else(|| {
      ToolCallError::InvalidArguments(format!("{kind} must be a spotify:{kind}: URI or {kind} id"))
    })
}

fn playlist_uri(value: Option<&Value>) -> Result<String, ToolCallError> {
  spotify_id(value, "playlist", ids::playlist_id).map(|id| format!("spotify:playlist:{id}"))
}

/// 1 to [`WRITE_BATCH`] track URIs or ids, as `spotify:track:` URIs.
fn track_uris(value: Option<&Value>) -> Result<Vec<String>, ToolCallError> {
  let invalid = |detail: String| ToolCallError::InvalidArguments(detail);
  let entries = value
    .and_then(Value::as_array)
    .ok_or_else(|| invalid("uris is required and must be an array".to_string()))?;
  if entries.is_empty() || entries.len() > WRITE_BATCH {
    return Err(invalid(format!(
      "uris must hold between 1 and {WRITE_BATCH} entries"
    )));
  }
  entries
    .iter()
    .map(|entry| {
      entry
        .as_str()
        .and_then(|uri| ids::track_id(uri.trim()))
        .map(|id| format!("spotify:track:{}", rspotify::prelude::Id::id(&id)))
        .ok_or_else(|| invalid(format!("{entry} is not a spotify:track: URI or track id")))
    })
    .collect()
}

/// An optional boolean that defaults to true.
fn flag(value: Option<&Value>, name: &str) -> Result<bool, ToolCallError> {
  match value {
    None | Some(Value::Null) => Ok(true),
    Some(Value::Bool(value)) => Ok(*value),
    Some(_) => Err(ToolCallError::InvalidArguments(format!(
      "{name} must be a boolean"
    ))),
  }
}

impl DjToolCall {
  /// Whether this call reaches the Spotify catalogue: a search, or a queue or
  /// play with a Spotify URI or a name to look up. Other sources' URIs are
  /// queued or played without it.
  pub fn needs_network(&self) -> bool {
    match self {
      Self::SearchTracks { .. } => true,
      Self::QueueTracks { items, .. } => queue_needs_spotify(items),
      Self::PlayNow { uri } => uri.starts_with("spotify:track:"),
      Self::GetAlbum { .. }
      | Self::GetArtist { .. }
      | Self::ListDevices
      | Self::TransferPlayback { .. }
      | Self::LikeTracks { .. } => true,
      Self::GetListeningHistory { .. }
      | Self::GetNowPlaying
      | Self::GetQueue
      | Self::SkipTrack
      | Self::SetDjVibe { .. }
      | Self::ListPlaylists
      | Self::CreatePlaylist { .. }
      | Self::AddToPlaylist { .. }
      | Self::RemoveFromPlaylist { .. }
      | Self::FollowArtist { .. } => false,
    }
  }

  pub fn tool_name(&self) -> &'static str {
    match self {
      Self::GetListeningHistory { .. } => "get_listening_history",
      Self::GetNowPlaying => "get_now_playing",
      Self::GetQueue => "get_queue",
      Self::SearchTracks { .. } => "search_tracks",
      Self::QueueTracks { .. } => "queue_tracks",
      Self::PlayNow { .. } => "play_now",
      Self::SkipTrack => "skip_track",
      Self::SetDjVibe { .. } => "set_dj_vibe",
      Self::ListPlaylists => "list_playlists",
      Self::GetAlbum { .. } => "get_album",
      Self::GetArtist { .. } => "get_artist",
      Self::ListDevices => "list_devices",
      Self::TransferPlayback { .. } => "transfer_playback",
      Self::CreatePlaylist { .. } => "create_playlist",
      Self::AddToPlaylist { .. } => "add_to_playlist",
      Self::RemoveFromPlaylist { .. } => "remove_from_playlist",
      Self::LikeTracks { .. } => "like_tracks",
      Self::FollowArtist { .. } => "follow_artist",
    }
  }
}

/// Whether a queue batch holds a Spotify URI or a name to look up.
pub fn queue_needs_spotify(items: &[QueueItem]) -> bool {
  items.iter().any(|item| match item {
    QueueItem::Uri(uri) => uri.starts_with("spotify:track:"),
    QueueItem::Named(_) => true,
  })
}

/// Execute the calls that only need the `App` lock (plus, for history, a
/// blocking file read).
///
/// Returns `None` for calls that run the network handlers: on the Spotify lane
/// when they need the client, else in the executor with none — see
/// `crate::infra::network::dj`.
pub async fn execute_app_only(app: &Arc<Mutex<App>>, call: &DjToolCall) -> Option<ToolOutcome> {
  match call {
    DjToolCall::GetListeningHistory { period } => Some(history_outcome(app, *period).await),
    DjToolCall::GetNowPlaying => Some(now_playing_outcome(app).await),
    DjToolCall::GetQueue => Some(queue_outcome(app).await),
    DjToolCall::SkipTrack => {
      let mut app = app.lock().await;
      // Through the shared vocabulary, so `App::next_track`'s ownership routing
      // (queue slot, decoded sources, suspend-to-queue, native shuffle
      // bookkeeping) applies; the old raw `IoEvent::NextTrack` dispatch missed
      // the App-side branches.
      app.apply(Action::NextTrack);
      Some(ToolOutcome::ok("Skipped to the next track"))
    }
    DjToolCall::SetDjVibe { vibe } => {
      let mut app = app.lock().await;
      // The apply arm bumps the DJ generation exactly once (the adopt-one-bump
      // rule in the turn loop depends on that count).
      app.apply(Action::SetDjVibe(vibe.clone()));
      Some(ToolOutcome::ok(match vibe.as_deref() {
        Some(vibe) => format!("DJ vibe set to: {vibe}. {}", vibe_effect(&app)),
        None => "DJ vibe cleared".to_string(),
      }))
    }
    DjToolCall::ListPlaylists => Some(playlists_outcome(app).await),
    DjToolCall::CreatePlaylist { name, uris } => Some(
      spotify_write(app, Action::CreatePlaylist {
        name: name.clone(),
        track_uris: uris.clone(),
      })
      .await
      .unwrap_or_else(|| {
        ToolOutcome::ok(format!(
          "Creating the playlist \"{name}\" with {} track(s). It is in list_playlists once Spotify has made it.",
          uris.len()
        ))
      }),
    ),
    DjToolCall::AddToPlaylist { playlist, uris } => {
      let mut app = app.lock().await;
      if !app.spotify_connected {
        return Some(not_connected());
      }
      if let Some(refused) = uneditable_playlist(&app, playlist) {
        return Some(refused);
      }
      for uri in uris {
        app.apply(Action::AddTrackToPlaylist {
          playlist: playlist.clone(),
          track: uri.clone(),
        });
      }
      Some(ToolOutcome::ok(format!(
        "Adding {} track(s) to {playlist}",
        uris.len()
      )))
    }
    DjToolCall::RemoveFromPlaylist {
      playlist,
      uri,
      position,
    } => {
      if let Some(refused) = uneditable_playlist(&*app.lock().await, playlist) {
        return Some(refused);
      }
      Some(
        spotify_write(app, Action::RemoveTrackFromPlaylist {
          playlist: playlist.clone(),
          track: uri.clone(),
          position: *position,
        })
        .await
        .unwrap_or_else(|| {
          ToolOutcome::ok(format!(
            "Removing {uri} at position {position} from {playlist}, if that is the track there"
          ))
        }),
      )
    }
    DjToolCall::FollowArtist { artist, follow } => {
      let action = if *follow {
        Action::FollowArtist(artist.clone())
      } else {
        Action::UnfollowArtist(artist.clone())
      };
      Some(
        spotify_write(app, action)
          .await
          .unwrap_or_else(|| {
            ToolOutcome::ok(if *follow {
              format!("Following spotify:artist:{artist}")
            } else {
              format!("Unfollowing spotify:artist:{artist}")
            })
          }),
      )
    }
    // These run the network handlers: with the Spotify client on the Spotify
    // lane, or with none when the call has no Spotify item. `play_now` confirms
    // a Spotify track exists before interrupting what is playing: dispatching
    // blind used to report success for a URI that stopped playback instead.
    DjToolCall::SearchTracks { .. }
    | DjToolCall::QueueTracks { .. }
    | DjToolCall::PlayNow { .. }
    | DjToolCall::GetAlbum { .. }
    | DjToolCall::GetArtist { .. }
    | DjToolCall::ListDevices
    | DjToolCall::TransferPlayback { .. }
    | DjToolCall::LikeTracks { .. } => None,
  }
}

fn not_connected() -> ToolOutcome {
  ToolOutcome::error(
    "spotatui has no Spotify session. Log in from the spotatui UI, then try again.",
  )
}

/// Applies a Spotify write through the pump; `Some` error without a session.
async fn spotify_write(app: &Arc<Mutex<App>>, action: Action) -> Option<ToolOutcome> {
  let mut app = app.lock().await;
  if !app.spotify_connected {
    return Some(not_connected());
  }
  app.apply(action);
  None
}

fn owns_playlist(app: &App, playlist: &PlaylistInfo) -> bool {
  playlist.owner_id.is_some()
    && playlist.owner_id.as_deref() == app.user().as_ref().map(|user| user.id.as_str())
}

/// An error for a loaded playlist the user neither owns nor collaborates on.
fn uneditable_playlist(app: &App, uri: &str) -> Option<ToolOutcome> {
  let playlist = app
    .all_playlists()
    .iter()
    .find(|playlist| playlist.uri == uri)?;
  (!playlist.collaborative && !owns_playlist(app, playlist)).then(|| {
    ToolOutcome::error(format!(
      "\"{}\" is not a playlist the user owns or collaborates on, so it cannot be edited",
      playlist.name
    ))
  })
}

async fn playlists_outcome(app: &Arc<Mutex<App>>) -> ToolOutcome {
  let app = app.lock().await;
  let playlists: Vec<Value> = app
    .all_playlists()
    .iter()
    .filter(|playlist| playlist.uri.starts_with("spotify:playlist:"))
    .map(|playlist| {
      let owned = owns_playlist(&app, playlist);
      json!({
        "name": playlist.name,
        "uri": playlist.uri,
        "tracks": playlist.track_count,
        "owned": owned,
        "collaborative": playlist.collaborative,
      })
    })
    .collect();
  if playlists.is_empty() {
    return ToolOutcome::with_data(
      "No Spotify playlists are loaded",
      json!({ "playlists": [] }),
    );
  }
  let text = playlists
    .iter()
    .map(|playlist| {
      format!(
        "{} [{}] {} tracks{}",
        playlist["name"].as_str().unwrap_or_default(),
        playlist["uri"].as_str().unwrap_or_default(),
        playlist["tracks"],
        if playlist["owned"] == true {
          ", owned"
        } else {
          ""
        }
      )
    })
    .collect::<Vec<_>>()
    .join("\n");
  ToolOutcome::with_data(text, json!({ "playlists": playlists }))
}

/// What, if anything, will actually act on a vibe that was just set.
///
/// Worth a sentence rather than a bare "vibe set". The vibe is a standing
/// direction for the *in-TUI* auto-queue DJ, and there are two ordinary
/// situations where nothing consumes it on its own: a build without `ai-dj`
/// (`--features mcp-server` alone is a complete build, and the commonest one),
/// and a build with it where the listener has not switched auto-queue on. An
/// agent told only "DJ vibe set to: mellow" reasonably concludes the music is
/// about to change, and would be wrong both times.
///
/// It is never *ignored*, which is why this points at the fallback: the vibe is
/// stored on `App` and comes back out through `get_listening_history`, so an
/// agent running its own top-up loop can read it and honour it itself.
fn vibe_effect(app: &App) -> &'static str {
  #[cfg(feature = "ai-dj")]
  {
    if app.dj.auto_queue {
      "The built-in DJ will follow it from its next refill."
    } else {
      "The built-in DJ's auto-queue is off, so nothing will act on it until the user turns it on; \
       queue tracks yourself in the meantime. get_listening_history returns it either way."
    }
  }
  #[cfg(not(feature = "ai-dj"))]
  {
    // `App::dj` exists under `dj-core`, so `auto_queue` is readable here — but it
    // is permanently false in this build, and saying "auto-queue is off" would
    // imply it could be turned on.
    let _ = app;
    "This build has no built-in auto-queue DJ, so nothing will act on it automatically. It is \
     stored and returned by get_listening_history, so you can follow it yourself when you queue."
  }
}

async fn history_outcome(
  app: &Arc<Mutex<App>>,
  period: crate::infra::history::RecapPeriod,
) -> ToolOutcome {
  // `load_listens` is blocking and reads the whole file, which is unbounded
  // append-only — never call it directly on an async task.
  let loaded = tokio::task::spawn_blocking(crate::infra::history::load_listens).await;
  let listens = match loaded {
    Ok(Ok(listens)) => listens,
    Ok(Err(e)) => return ToolOutcome::error(format!("Could not read listening history: {e}")),
    Err(e) => return ToolOutcome::error(format!("Listening history task failed: {e}")),
  };

  let mut summary = brief::build_brief(&listens, period);
  {
    let app = app.lock().await;
    summary.now_playing = super::current_track_label(&app);
    summary.vibe = app.dj.vibe.clone();
  }

  if summary.is_sparse() {
    return ToolOutcome::ok(format!(
      "Very little listening history so far ({} qualifying plays in {}). Ask the user what they \
       feel like instead of inferring it.",
      summary.total_plays, summary.period_label
    ));
  }

  let structured = json!({
    "period": summary.period_label,
    "total_plays": summary.total_plays,
    "top_artists": summary.top_artists,
    "top_tracks": summary.top_tracks,
    "top_albums": summary.top_albums,
    "recent": summary.recent,
    "now_playing": summary.now_playing,
    "vibe": summary.vibe,
  });
  ToolOutcome::with_data(summary.to_prompt_block(), structured)
}

async fn now_playing_outcome(app: &Arc<Mutex<App>>) -> ToolOutcome {
  let app = app.lock().await;
  let queue_depth = app.native_queue.len();
  // One snapshot for both the label and the transport state: it is the
  // source-agnostic view (Spotify, local, Subsonic, YouTube, radio), so this
  // works whatever is actually producing sound.
  match crate::infra::media_metadata::current_playback_snapshot(&app) {
    Some(snapshot) => {
      let label = format!(
        "{} — {}",
        snapshot.metadata.title,
        snapshot.primary_artist()
      );
      let state = if snapshot.is_playing {
        "playing"
      } else {
        "paused"
      };
      ToolOutcome::with_data(
        format!("{state}: {label} ({queue_depth} track(s) queued)"),
        json!({
          "track": label,
          "is_playing": snapshot.is_playing,
          "is_live": snapshot.is_live,
          "queue_depth": queue_depth,
        }),
      )
    }
    // Same keys as the playing branch, `is_live` included: a payload whose shape
    // depends on playback state makes a client read a missing key as unknown
    // rather than as "no, this is not a live stream".
    None => ToolOutcome::with_data(
      format!("Nothing is playing ({queue_depth} track(s) queued)"),
      json!({
        "track": Value::Null,
        "is_playing": false,
        "is_live": false,
        "queue_depth": queue_depth,
      }),
    ),
  }
}

async fn queue_outcome(app: &Arc<Mutex<App>>) -> ToolOutcome {
  let app = app.lock().await;
  if app.native_queue.is_empty() {
    return ToolOutcome::with_data("The queue is empty", json!({"tracks": []}));
  }
  let labels = app
    .native_queue
    .iter()
    .map(|track| format!("{} — {}", track.name, track.artists.join(", ")))
    .collect::<Vec<_>>();
  let text = labels
    .iter()
    .enumerate()
    .map(|(index, label)| format!("{}. {}", index + 1, label))
    .collect::<Vec<_>>()
    .join("\n");
  ToolOutcome::with_data(text, json!({"tracks": labels}))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::history::RecapPeriod;

  #[test]
  fn every_tool_schema_is_a_valid_object_schema() {
    for tool in TOOLS {
      let schema = (tool.schema)();
      assert_eq!(
        schema.get("type").and_then(Value::as_str),
        Some("object"),
        "{} must have an object input schema",
        tool.name
      );
      // MCP requires a valid schema object, never null.
      assert!(schema.is_object(), "{} schema must be an object", tool.name);
    }
  }

  #[test]
  fn tool_names_satisfy_mcp_naming_rules() {
    for tool in TOOLS {
      assert!(
        (1..=128).contains(&tool.name.len()),
        "{} name length",
        tool.name
      );
      assert!(
        tool
          .name
          .chars()
          .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')),
        "{} has characters MCP disallows",
        tool.name
      );
    }
  }

  #[test]
  fn tool_names_are_unique_and_order_is_deterministic() {
    let names: Vec<_> = TOOLS.iter().map(|tool| tool.name).collect();
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(names.len(), unique.len(), "duplicate tool name");
    // Order is part of the contract: MCP clients cache tools/list.
    assert_eq!(names[0], "get_listening_history");
    assert_eq!(names.last(), Some(&"follow_artist"));
  }

  #[test]
  fn no_argument_tools_use_the_recommended_empty_schema() {
    for name in ["get_now_playing", "get_queue", "skip_track"] {
      let schema = (spec(name).unwrap().schema)();
      assert_eq!(
        schema.get("additionalProperties"),
        Some(&Value::Bool(false)),
        "{name} should reject unexpected arguments"
      );
      assert!(schema.get("properties").is_none(), "{name} takes no args");
    }
  }

  #[test]
  fn history_period_defaults_and_parses() {
    let call = parse_call("get_listening_history", &json!({})).unwrap();
    assert_eq!(
      call,
      DjToolCall::GetListeningHistory {
        period: RecapPeriod::ThirtyDays
      }
    );
    let call = parse_call("get_listening_history", &json!({"period": "7d"})).unwrap();
    assert_eq!(
      call,
      DjToolCall::GetListeningHistory {
        period: RecapPeriod::SevenDays
      }
    );
    assert!(matches!(
      parse_call("get_listening_history", &json!({"period": "fortnight"})),
      Err(ToolCallError::InvalidArguments(_))
    ));
  }

  #[test]
  fn null_arguments_are_accepted_for_no_argument_tools() {
    assert_eq!(
      parse_call("get_now_playing", &Value::Null).unwrap(),
      DjToolCall::GetNowPlaying
    );
  }

  #[test]
  fn unknown_tool_is_distinct_from_bad_arguments() {
    // MCP treats these differently: unknown tool and malformed request are
    // protocol errors, a tool that ran and failed is isError.
    assert!(matches!(
      parse_call("make_coffee", &json!({})),
      Err(ToolCallError::UnknownTool(_))
    ));
    assert!(matches!(
      parse_call("search_tracks", &json!({})),
      Err(ToolCallError::InvalidArguments(_))
    ));
  }

  #[test]
  fn search_validates_query_and_limit() {
    assert!(parse_call("search_tracks", &json!({"query": "   "})).is_err());
    assert!(parse_call("search_tracks", &json!({"query": "a", "limit": 0})).is_err());
    assert!(parse_call(
      "search_tracks",
      &json!({"query": "a", "limit": SPOTIFY_SEARCH_LIMIT + 1})
    )
    .is_err());
    let call = parse_call("search_tracks", &json!({"query": " nude ", "limit": 3})).unwrap();
    assert_eq!(
      call,
      DjToolCall::SearchTracks {
        query: "nude".into(),
        limit: 3
      }
    );
  }

  #[test]
  fn queue_accepts_uris_and_named_pairs() {
    let call = parse_call(
      "queue_tracks",
      &json!({"tracks": [
        {"uri": "spotify:track:abc"},
        {"title": "Nude", "artist": "Radiohead"}
      ]}),
    )
    .unwrap();
    let DjToolCall::QueueTracks {
      items,
      exclude_owned,
      extra_skip_keys,
    } = call
    else {
      panic!("expected QueueTracks");
    };
    assert_eq!(items[0], QueueItem::Uri("spotify:track:abc".into()));
    assert!(matches!(&items[1], QueueItem::Named(s) if s.title == "Nude"));
    assert!(
      !exclude_owned,
      "an agent told to queue a specific track must get it, even one the listener owns"
    );
    assert!(
      extra_skip_keys.is_empty(),
      "the wire can never fill this: it is the in-TUI DJ's own taste policy"
    );

    let call = parse_call(
      "queue_tracks",
      &json!({"tracks": [{"uri": "spotify:track:abc"}], "exclude_owned": true}),
    )
    .unwrap();
    assert!(matches!(
      call,
      DjToolCall::QueueTracks {
        exclude_owned: true,
        ..
      }
    ));
  }

  #[test]
  fn play_now_rejects_anything_that_is_not_a_single_track() {
    // Rejected at parse time rather than reported as a failed execution: the
    // tool takes exactly one URI, so a bad one is the whole request. It used to
    // be dispatched blind, which stopped playback while reporting success.
    for uri in [
      "not-a-uri",
      "spotify:album:1DFixLWuPkv3KT3TnV35m3",
      "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
      "https://open.spotify.com/track/7o2AeQZzfCERsRmOM86EcB",
    ] {
      let error =
        parse_call("play_now", &json!({"uri": uri})).expect_err(&format!("{uri} must not parse"));
      let ToolCallError::InvalidArguments(detail) = error else {
        panic!("{uri} should be an argument error, not an unknown tool");
      };
      // Names the shape that does work, so the model can fix it in one step.
      assert!(detail.contains("spotify:track:"), "{uri}: {detail}");
    }

    for uri in [
      "spotify:track:abc",
      "file:/music/a.flac",
      "youtube:abc",
      "qobuz:track:1",
    ] {
      assert!(
        parse_call("play_now", &json!({"uri": uri})).is_ok(),
        "{uri}"
      );
    }
  }

  #[test]
  fn play_now_needs_the_network_so_it_can_check_the_track_exists() {
    // The lane split is what makes the existence check possible at all; the
    // app-only lane has no Spotify client.
    let call = parse_call("play_now", &json!({"uri": "spotify:track:abc"})).unwrap();
    assert!(call.needs_network());
  }

  #[test]
  fn exclude_owned_must_be_a_boolean_if_present() {
    // A string "true" is the shape a model gets wrong; accepting it silently
    // would turn the guarantee on (or off) by accident.
    assert!(matches!(
      parse_call(
        "queue_tracks",
        &json!({"tracks": [{"uri": "spotify:track:a"}], "exclude_owned": "true"})
      ),
      Err(ToolCallError::InvalidArguments(_))
    ));
    // Explicit null is the same as absent.
    assert!(matches!(
      parse_call(
        "queue_tracks",
        &json!({"tracks": [{"uri": "spotify:track:a"}], "exclude_owned": null})
      ),
      Ok(DjToolCall::QueueTracks {
        exclude_owned: false,
        ..
      })
    ));
  }

  #[test]
  fn the_queue_schema_declares_exclude_owned_without_requiring_it() {
    let schema = (spec("queue_tracks").unwrap().schema)();
    let property = schema
      .get("properties")
      .and_then(|properties| properties.get("exclude_owned"))
      .expect("exclude_owned must be declared, or clients cannot pass it");
    assert_eq!(property.get("type"), Some(&json!("boolean")));
    // `additionalProperties: false` means an undeclared param is rejected
    // outright, so the schema and `parse_call` have to agree.
    assert_eq!(
      schema.get("additionalProperties"),
      Some(&Value::Bool(false))
    );
    let required = schema
      .get("required")
      .and_then(Value::as_array)
      .expect("tracks stays required");
    assert!(
      !required.contains(&json!("exclude_owned")),
      "the default has to be usable without passing anything"
    );
  }

  #[test]
  fn queue_rejects_incomplete_entries_and_oversized_batches() {
    assert!(parse_call("queue_tracks", &json!({"tracks": [{"title": "Nude"}]})).is_err());
    assert!(parse_call("queue_tracks", &json!({"tracks": []})).is_err());
    // A blank half is as incomplete as a missing one. Accepted, it would build a
    // suggestion nothing can resolve and be reported as "not in the catalogue",
    // which tells the model the track does not exist instead of that it sent a
    // bad argument.
    for entry in [
      json!({"title": "Nude", "artist": "   "}),
      json!({"title": "  ", "artist": "Radiohead"}),
    ] {
      assert!(
        matches!(
          parse_call("queue_tracks", &json!({"tracks": [entry.clone()]})),
          Err(ToolCallError::InvalidArguments(_))
        ),
        "{entry} should be an argument error"
      );
    }
    let too_many: Vec<_> = (0..MAX_BATCH + 1)
      .map(|i| json!({"uri": format!("spotify:track:{i}")}))
      .collect();
    assert!(parse_call("queue_tracks", &json!({"tracks": too_many})).is_err());
  }

  #[tokio::test]
  async fn now_playing_keeps_one_payload_shape_whether_or_not_anything_plays() {
    // A default `App` has no playback context and no native track, so the
    // snapshot is `None` — the branch that used to drop `is_live`.
    let app = Arc::new(Mutex::new(App::default()));
    let outcome = execute_app_only(&app, &DjToolCall::GetNowPlaying)
      .await
      .expect("get_now_playing is answered without the network");
    let data = outcome
      .structured
      .expect("now_playing returns structured data");

    assert_eq!(data.get("track"), Some(&Value::Null));
    assert_eq!(data.get("is_playing"), Some(&json!(false)));
    // The key the playing branch always sends. Absent, a client cannot tell "not
    // a live stream" from "this build never says".
    assert_eq!(data.get("is_live"), Some(&json!(false)));
    assert_eq!(data.get("queue_depth"), Some(&json!(0)));
  }

  #[tokio::test]
  async fn setting_a_vibe_says_whether_anything_will_act_on_it() {
    let app = Arc::new(Mutex::new(App::default()));
    let outcome = execute_app_only(
      &app,
      &DjToolCall::SetDjVibe {
        vibe: Some("mellow instrumental".into()),
      },
    )
    .await
    .expect("set_dj_vibe is answered without the network");

    assert!(!outcome.is_error);
    assert!(outcome.text.contains("mellow instrumental"));
    assert_eq!(
      app.lock().await.dj.vibe.as_deref(),
      Some("mellow instrumental")
    );

    // The whole point: a bare "vibe set" lets an agent conclude the music is
    // about to change. Nothing is refilling the queue in either build here, so
    // the result has to say so and point at the fallback.
    assert!(
      outcome.text.contains("get_listening_history"),
      "the agent needs to be told it can read the vibe back: {}",
      outcome.text
    );
    #[cfg(feature = "ai-dj")]
    assert!(
      outcome.text.contains("auto-queue is off"),
      "a default App has auto-queue off: {}",
      outcome.text
    );
    #[cfg(not(feature = "ai-dj"))]
    assert!(
      outcome.text.contains("no built-in auto-queue DJ"),
      "without ai-dj there is no DJ to switch on, and saying \"off\" would imply there is: {}",
      outcome.text
    );
  }

  #[tokio::test]
  async fn clearing_a_vibe_stays_terse() {
    // Nothing was set, so there is no expectation to correct.
    let app = Arc::new(Mutex::new(App::default()));
    let outcome = execute_app_only(&app, &DjToolCall::SetDjVibe { vibe: None })
      .await
      .unwrap();
    assert_eq!(outcome.text, "DJ vibe cleared");
    assert!(app.lock().await.dj.vibe.is_none());
  }

  #[test]
  fn vibe_treats_blank_as_cleared() {
    assert_eq!(
      parse_call("set_dj_vibe", &json!({"vibe": "  "})).unwrap(),
      DjToolCall::SetDjVibe { vibe: None }
    );
    assert_eq!(
      parse_call("set_dj_vibe", &json!({"vibe": " mellow "})).unwrap(),
      DjToolCall::SetDjVibe {
        vibe: Some("mellow".into())
      }
    );
  }

  #[test]
  fn a_call_with_no_spotify_item_needs_no_network() {
    let opaque_play = parse_call("play_now", &json!({"uri": "qobuz:track:1"})).unwrap();
    let opaque_queue = parse_call(
      "queue_tracks",
      &json!({"tracks": [{"uri": "file:/a.flac"}, {"uri": "youtube:abc"}], "exclude_owned": true}),
    )
    .unwrap();
    let named_queue = parse_call(
      "queue_tracks",
      &json!({"tracks": [{"uri": "file:/a.flac"}, {"title": "t", "artist": "a"}]}),
    )
    .unwrap();

    assert!(!opaque_play.needs_network());
    assert!(!opaque_queue.needs_network());
    assert!(named_queue.needs_network());
  }

  #[test]
  fn needs_network_matches_the_spec_table() {
    // The lane split depends on these agreeing; a mismatch would send a
    // Spotify-dependent call to a Network built with no client.
    for tool in TOOLS {
      let args = match tool.name {
        "search_tracks" => json!({"query": "x"}),
        "queue_tracks" => json!({"tracks": [{"uri": "spotify:track:x"}]}),
        "play_now" => json!({"uri": "spotify:track:x"}),
        "get_album" => json!({"album": ALBUM}),
        "get_artist" | "follow_artist" => json!({"artist": ARTIST}),
        "transfer_playback" => json!({"device": "Kitchen"}),
        "create_playlist" => json!({"name": "Road Trip"}),
        "add_to_playlist" => json!({"playlist": PLAYLIST, "uris": [TRACK]}),
        "remove_from_playlist" => json!({"playlist": PLAYLIST, "uri": TRACK, "position": 0}),
        "like_tracks" => json!({"uris": [TRACK]}),
        _ => json!({}),
      };
      let call = parse_call(tool.name, &args).unwrap();
      assert_eq!(
        call.needs_network(),
        tool.needs_network,
        "{} disagrees about needing the network",
        tool.name
      );
      assert_eq!(call.tool_name(), tool.name);
    }
  }

  const TRACK: &str = "4uLU6hMCjMI75M1A2tKUQC";
  const ALBUM: &str = "6dVIqQ8qmQ5GBnJ9shOYGE";
  const ARTIST: &str = "4tZwfgrHOc3mvqYlEYSvVi";
  const PLAYLIST: &str = "37i9dQZF1DXcBWIGoYBM5M";

  #[test]
  fn the_library_and_device_tools_are_hidden_from_the_dj() {
    let offered: Vec<&str> = dj_tools().map(|tool| tool.name).collect();
    assert_eq!(offered.len(), 8);
    assert!(!offered.contains(&"like_tracks"));
    assert_eq!(
      parse_dj_call("like_tracks", &json!({"uris": [TRACK]})),
      Err(ToolCallError::UnknownTool("like_tracks".to_string()))
    );
    assert_eq!(
      parse_dj_call("skip_track", &json!({})),
      Ok(DjToolCall::SkipTrack)
    );
  }

  #[test]
  fn only_a_playlist_removal_is_destructive() {
    let destructive: Vec<&str> = TOOLS
      .iter()
      .filter(|tool| tool.destructive)
      .map(|tool| tool.name)
      .collect();
    assert_eq!(destructive, vec!["remove_from_playlist"]);
    assert!(TOOLS
      .iter()
      .filter(|tool| tool.destructive)
      .all(|tool| !tool.read_only));
  }

  #[test]
  fn playlist_writes_take_ids_or_uris_and_send_uris() {
    let call = parse_call(
      "add_to_playlist",
      &json!({"playlist": PLAYLIST, "uris": [TRACK, format!("spotify:track:{TRACK}")]}),
    )
    .unwrap();
    let track_uri = format!("spotify:track:{TRACK}");
    assert_eq!(
      call,
      DjToolCall::AddToPlaylist {
        playlist: format!("spotify:playlist:{PLAYLIST}"),
        uris: vec![track_uri.clone(), track_uri],
      }
    );
  }

  #[test]
  fn a_write_takes_one_to_a_batch_of_track_uris() {
    let too_many: Vec<&str> = vec![TRACK; WRITE_BATCH + 1];
    assert!(parse_call("like_tracks", &json!({"uris": too_many})).is_err());
    assert!(parse_call("like_tracks", &json!({"uris": []})).is_err());
    assert!(parse_call(
      "like_tracks",
      &json!({"uris": [format!("spotify:album:{ALBUM}")]})
    )
    .is_err());
    assert!(parse_call("like_tracks", &json!({"uris": vec![TRACK; WRITE_BATCH]})).is_ok());
  }

  #[test]
  fn a_removal_needs_the_track_and_its_position() {
    let base = json!({"playlist": PLAYLIST, "uri": TRACK});
    assert!(parse_call("remove_from_playlist", &base).is_err());
    let mut negative = base.clone();
    negative["position"] = json!(-1);
    assert!(parse_call("remove_from_playlist", &negative).is_err());
    let mut wrong = base.clone();
    wrong["position"] = json!(3);
    wrong["uri"] = json!(format!("spotify:episode:{TRACK}"));
    assert!(parse_call("remove_from_playlist", &wrong).is_err());
    let mut good = base;
    good["position"] = json!(3);
    assert!(matches!(
      parse_call("remove_from_playlist", &good),
      Ok(DjToolCall::RemoveFromPlaylist { position: 3, .. })
    ));
  }

  #[test]
  fn like_and_follow_default_to_on() {
    assert_eq!(
      parse_call("like_tracks", &json!({"uris": [TRACK]})).unwrap(),
      DjToolCall::LikeTracks {
        uris: vec![format!("spotify:track:{TRACK}")],
        liked: true
      }
    );
    assert_eq!(
      parse_call(
        "follow_artist",
        &json!({"artist": format!("spotify:artist:{ARTIST}"), "follow": false})
      )
      .unwrap(),
      DjToolCall::FollowArtist {
        artist: ARTIST.to_string(),
        follow: false
      }
    );
    assert!(parse_call("follow_artist", &json!({"artist": ARTIST, "follow": "no"})).is_err());
  }

  #[test]
  fn a_lookup_needs_an_id_of_its_own_kind() {
    assert_eq!(
      parse_call(
        "get_album",
        &json!({"album": format!("spotify:album:{ALBUM}")})
      )
      .unwrap(),
      DjToolCall::GetAlbum {
        album: ALBUM.to_string()
      }
    );
    assert!(parse_call(
      "get_album",
      &json!({"album": format!("spotify:artist:{ARTIST}")})
    )
    .is_err());
    assert!(parse_call("transfer_playback", &json!({"device": "  "})).is_err());
    assert!(parse_call("create_playlist", &json!({"name": ""})).is_err());
  }

  fn app_with_channel(
    connected: bool,
  ) -> (
    Arc<Mutex<App>>,
    std::sync::mpsc::Receiver<crate::infra::network::IoEvent>,
  ) {
    let (tx, rx) = std::sync::mpsc::channel();
    let app = App::new(
      tx,
      crate::core::user_config::UserConfig::new(),
      connected.then(std::time::SystemTime::now),
    );
    (Arc::new(Mutex::new(app)), rx)
  }

  #[tokio::test]
  async fn a_write_without_a_spotify_session_is_refused() {
    let (app, rx) = app_with_channel(false);
    let call = parse_call("create_playlist", &json!({"name": "Road Trip"})).unwrap();
    let outcome = execute_app_only(&app, &call).await.unwrap();
    assert!(outcome.is_error);
    assert!(rx.try_recv().is_err(), "nothing reaches the pump");
  }

  #[tokio::test]
  async fn adding_to_a_playlist_sends_one_add_per_track() {
    use crate::infra::network::IoEvent;
    let (app, rx) = app_with_channel(true);
    let call = parse_call(
      "add_to_playlist",
      &json!({"playlist": PLAYLIST, "uris": [TRACK, TRACK]}),
    )
    .unwrap();
    let outcome = execute_app_only(&app, &call).await.unwrap();
    assert!(!outcome.is_error);
    for _ in 0..2 {
      assert!(matches!(rx.try_recv(), Ok(IoEvent::AddTrackToPlaylist(..))));
    }
  }

  #[tokio::test]
  async fn a_followed_playlist_of_someone_else_is_not_edited() {
    use crate::core::test_helpers::{playlist_info, user_info};
    let (app, rx) = app_with_channel(true);
    {
      let mut app = app.lock().await;
      *app.user_mut() = Some(user_info("me"));
      app.all_playlists_mut().extend([
        playlist_info("00000000000000000000p2", "Theirs", "other", false),
        playlist_info("00000000000000000000p3", "Shared", "other", true),
      ]);
    }
    for tool in ["add_to_playlist", "remove_from_playlist"] {
      let args = json!({"playlist": "spotify:playlist:00000000000000000000p2", "uris": [TRACK], "uri": TRACK, "position": 0});
      let outcome = execute_app_only(&app, &parse_call(tool, &args).unwrap())
        .await
        .unwrap();
      assert!(outcome.is_error, "{tool}");
    }
    assert!(rx.try_recv().is_err(), "nothing reaches the pump");

    let shared = json!({"playlist": "spotify:playlist:00000000000000000000p3", "uris": [TRACK]});
    let outcome = execute_app_only(&app, &parse_call("add_to_playlist", &shared).unwrap())
      .await
      .unwrap();
    assert!(!outcome.is_error);
  }

  #[tokio::test]
  async fn unfollowing_sends_the_unfollow() {
    use crate::infra::network::IoEvent;
    let (app, rx) = app_with_channel(true);
    let call = parse_call("follow_artist", &json!({"artist": ARTIST, "follow": false})).unwrap();
    execute_app_only(&app, &call).await.unwrap();
    assert!(matches!(rx.try_recv(), Ok(IoEvent::UserUnfollowArtists(_))));
  }

  #[tokio::test]
  async fn the_playlist_list_marks_what_the_user_owns() {
    use crate::core::test_helpers::{playlist_info, user_info};
    let (app, _rx) = app_with_channel(true);
    {
      let mut app = app.lock().await;
      *app.user_mut() = Some(user_info("me"));
      app.all_playlists_mut().extend([
        playlist_info("00000000000000000000p1", "Mine", "me", false),
        playlist_info("00000000000000000000p2", "Theirs", "other", false),
      ]);
    }
    let outcome = execute_app_only(&app, &DjToolCall::ListPlaylists)
      .await
      .unwrap();
    let playlists = outcome.structured.unwrap()["playlists"].clone();
    assert_eq!(playlists[0]["owned"], true);
    assert_eq!(playlists[1]["owned"], false);
    assert!(outcome.text.contains("Mine"));
  }
}
