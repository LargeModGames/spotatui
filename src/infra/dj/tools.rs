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
use crate::core::app::App;
use crate::infra::network::IoEvent;
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
  /// Whether executing it needs the live Spotify client (and therefore the
  /// serial IoEvent lane) rather than just the `App` lock.
  pub needs_network: bool,
  pub schema: fn() -> Value,
}

/// Every tool, in a fixed order.
pub const TOOLS: &[ToolSpec] = &[
  ToolSpec {
    name: "get_listening_history",
    title: "Listening history",
    description: "Summarise what the user has been listening to: top artists, top tracks, top albums, and recent plays. Call this first when choosing music for them. Returns aggregate names only — no identifiers or timestamps.",
    read_only: true,
    needs_network: false,
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
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "get_queue",
    title: "Queue",
    description: "The tracks currently queued up, in play order.",
    read_only: true,
    needs_network: false,
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "search_tracks",
    title: "Search tracks",
    description: "Search the catalogue for tracks and return their playable URIs. Use this to check that something exists before queueing it by name. Each result is marked `owned` when the user already has it (Liked Songs, or a playlist they own or collaborate on) — prefer results marked new when they asked for something they have not heard.",
    read_only: true,
    needs_network: true,
    schema: || {
      json!({
        "type": "object",
        "properties": {
          "query": {"type": "string", "description": "Free-text search, e.g. 'radiohead weird fishes'."},
          "limit": {"type": "integer", "minimum": 1, "maximum": 20, "description": "Maximum results. Defaults to 10."}
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
    schema: || json!({"type": "object", "additionalProperties": false}),
  },
  ToolSpec {
    name: "set_dj_vibe",
    title: "Set DJ vibe",
    description: "Set the standing direction the built-in auto-queue DJ follows when it refills the queue on its own, e.g. 'mellow instrumental for focusing'. Pass null to clear it. This does not start playback or queue anything. The built-in DJ is optional and may be switched off or not built in at all, so read the result: it says whether anything will act on the vibe. Either way it is stored and returned by get_listening_history, so you can honour it yourself when you queue.",
    read_only: false,
    needs_network: false,
    schema: || {
      json!({
        "type": "object",
        "properties": {"vibe": {"type": ["string", "null"], "description": "The direction to follow, or null to clear."}},
        "additionalProperties": false
      })
    },
  },
];

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
        None | Some(Value::Null) => 10,
        Some(value) => value
          .as_u64()
          .filter(|n| (1..=20).contains(n))
          .ok_or_else(|| invalid("limit must be an integer between 1 and 20"))?
          as usize,
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
           subsonic:…, or youtube:…); album, playlist and https links are not \
           playable here",
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
    other => Err(ToolCallError::UnknownTool(other.to_string())),
  }
}

impl DjToolCall {
  pub fn needs_network(&self) -> bool {
    matches!(
      self,
      Self::SearchTracks { .. } | Self::QueueTracks { .. } | Self::PlayNow { .. }
    )
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
    }
  }
}

/// Execute the calls that only need the `App` lock (plus, for history, a
/// blocking file read).
///
/// Returns `None` for calls that need the Spotify client; those go through the
/// serial IoEvent lane instead — see `crate::infra::network::dj`.
pub async fn execute_app_only(app: &Arc<Mutex<App>>, call: &DjToolCall) -> Option<ToolOutcome> {
  match call {
    DjToolCall::GetListeningHistory { period } => Some(history_outcome(app, *period).await),
    DjToolCall::GetNowPlaying => Some(now_playing_outcome(app).await),
    DjToolCall::GetQueue => Some(queue_outcome(app).await),
    DjToolCall::SkipTrack => {
      let mut app = app.lock().await;
      app.dispatch(IoEvent::NextTrack);
      Some(ToolOutcome::ok("Skipped to the next track"))
    }
    DjToolCall::SetDjVibe { vibe } => {
      let mut app = app.lock().await;
      // A new standing direction invalidates any refill already in flight for
      // the old one.
      app.dj.bump_generation();
      app.dj.vibe = vibe.clone();
      Some(ToolOutcome::ok(match vibe.as_deref() {
        Some(vibe) => format!("DJ vibe set to: {vibe}. {}", vibe_effect(&app)),
        None => "DJ vibe cleared".to_string(),
      }))
    }
    // These need the real Spotify client, which only the serial lane has.
    // `play_now` is among them because it confirms the track exists before
    // interrupting what is playing: dispatching blind used to report success
    // for a URI that stopped playback instead of starting it.
    DjToolCall::SearchTracks { .. }
    | DjToolCall::QueueTracks { .. }
    | DjToolCall::PlayNow { .. } => None,
  }
}

/// What, if anything, will actually act on a vibe that was just set.
///
/// Worth a sentence rather than a bare "vibe set". The vibe is a standing
/// direction for the *in-TUI* auto-queue DJ, and in a build without one nothing
/// consumes it on its own. An agent told only "DJ vibe set to: mellow"
/// reasonably concludes the music is about to change, and would be wrong.
///
/// It is never *ignored*, which is why this points at the fallback: the vibe is
/// stored on `App` and comes back out through `get_listening_history`, so an
/// agent running its own top-up loop can read it and honour it itself.
fn vibe_effect(app: &App) -> &'static str {
  // `App::dj` exists under `dj-core`, so `auto_queue` is readable here — but it
  // is permanently false in a build with no in-TUI DJ, and saying "auto-queue is
  // off" would imply it could be turned on. Once `ai-dj` exists this grows the
  // arm that reports the live toggle.
  let _ = app;
  "This build has no built-in auto-queue DJ, so nothing will act on it automatically. It is \
   stored and returned by get_listening_history, so you can follow it yourself when you queue."
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
    assert_eq!(names.last(), Some(&"set_dj_vibe"));
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
    assert!(parse_call("search_tracks", &json!({"query": "a", "limit": 99})).is_err());
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

    for uri in ["spotify:track:abc", "file:/music/a.flac", "youtube:abc"] {
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
    assert!(
      outcome.text.contains("no built-in auto-queue DJ"),
      "with no in-TUI DJ there is nothing to switch on, and saying \"off\" would imply there is: {}",
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
  fn needs_network_matches_the_spec_table() {
    // The lane split depends on these agreeing; a mismatch would send a
    // Spotify-dependent call to a Network built with no client.
    for tool in TOOLS {
      let args = match tool.name {
        "search_tracks" => json!({"query": "x"}),
        "queue_tracks" => json!({"tracks": [{"uri": "spotify:track:x"}]}),
        "play_now" => json!({"uri": "spotify:track:x"}),
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
}
