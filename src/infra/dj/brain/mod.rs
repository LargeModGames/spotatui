//! The in-TUI DJ's "brain": where the DJ's decisions come from.
//!
//! Three backends behind one contract:
//!
//! * [`agent_cli`] — a coding agent the user already has installed and
//!   authenticated (`claude`, `codex`, `copilot`, …). **No API key**, since it
//!   rides their existing subscription.
//! * [`anthropic`] — the Anthropic Messages API with a key.
//! * [`openai_compat`] — anything speaking `/chat/completions`, which covers
//!   OpenAI, OpenRouter, and local models via Ollama or LM Studio.
//!
//! ## One step at a time
//!
//! A brain does not answer a whole turn; it answers one **step** — some words for
//! the listener, some tool calls, or both. [`super::agent`] runs the tools and
//! calls back with the results, so the DJ can search before it commits, look at
//! the queue, or simply ask a question and stop. A step with nothing to call is
//! how a plain conversational reply happens.
//!
//! The tools are [`super::tools::TOOLS`] verbatim — the same table the MCP server
//! publishes — so a tool added there is one the in-TUI DJ can use with no change
//! here.
//!
//! **All three backends speak the same JSON step protocol** rather than each
//! adopting its provider's native tool-calling. `agent_cli` is a one-shot
//! subprocess and has no native form to adopt, and keeping one protocol means one
//! prompt and one parser for all three. The two HTTP backends still get the shape
//! enforced for them: [`step_schema`] is what they hand to structured output.

pub mod agent_cli;
pub mod anthropic;
pub mod openai_compat;

use super::brief::TasteBrief;
use super::tools::TOOLS;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

/// Shared HTTP client for the DJ's API backends.
///
/// A blanket timeout is mandatory, not optional — the same reasoning as
/// `network::requests::shared_http_client`: a hung request on the serial IoEvent
/// pump would freeze every other event behind it. The window is generous because
/// a reasoning model can legitimately take a while.
pub fn shared_client() -> reqwest::Client {
  static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
  CLIENT
    .get_or_init(|| {
      reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .unwrap_or_default()
    })
    .clone()
}

/// What the DJ is asked for, at one step of a turn.
///
/// `Default` exists for the test fixtures: the struct gains a field per prompt
/// feature, and the backend tests' near-identical literals would each need
/// updating for every one.
#[derive(Default)]
pub struct DjRequest {
  pub brief: TasteBrief,
  /// The conversation so far, oldest first, as `(speaker, text)`.
  pub history: Vec<(String, String)>,
  /// Tools already called *this turn*, with what they returned. Empty on the
  /// first step; this is how the loop feeds results back.
  pub scratch: Vec<ToolExchange>,
  /// How many tracks to queue, when this turn queues any.
  pub want: usize,
  /// Whether the listener wants only tracks they do not already have. A hard
  /// filter enforces this after the fact; saying it in the prompt is what stops
  /// the model burning the whole batch on their favourites first.
  pub avoid_library: bool,
  /// Whether this turn must end in music rather than words.
  ///
  /// Set for an auto-queue refill and a vibe shift, where nobody is watching the
  /// screen: a clarifying question there is indistinguishable from a hang.
  pub must_act: bool,
}

/// One completed tool call, as the next step sees it.
#[derive(Clone)]
pub struct ToolExchange {
  pub name: String,
  pub arguments: Value,
  /// What the tool returned, verbatim. Errors included — an error the model can
  /// read is how it recovers without a repair round.
  pub result: String,
}

/// A tool the model wants run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolInvocation {
  pub name: String,
  pub arguments: Value,
}

/// One step of a turn: something to say, something to run, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DjStep {
  /// A sentence for the transcript, if the model offered one.
  pub say: Option<String>,
  /// Empty means the turn is over — which is exactly what a conversational reply
  /// looks like.
  pub calls: Vec<ToolInvocation>,
}

/// The backends, dispatched by `match`.
///
/// Deliberately an enum rather than a trait: all three are known at compile time,
/// and `async fn` in a trait object would need the `async-trait` crate — a new
/// dependency for no benefit.
#[derive(Debug)]
pub enum DjBrain {
  AgentCli(agent_cli::AgentCliBrain),
  Anthropic(anthropic::AnthropicBrain),
  OpenAiCompat(openai_compat::OpenAiCompatBrain),
}

impl DjBrain {
  pub async fn step(&self, request: &DjRequest) -> Result<DjStep> {
    match self {
      Self::AgentCli(brain) => brain.step(request).await,
      Self::Anthropic(brain) => brain.step(request).await,
      Self::OpenAiCompat(brain) => brain.step(request).await,
    }
  }

  /// Short label for status messages.
  pub fn label(&self) -> &'static str {
    match self {
      Self::AgentCli(_) => "agent CLI",
      Self::Anthropic(_) => "Anthropic",
      Self::OpenAiCompat(_) => "OpenAI-compatible",
    }
  }
}

/// The system prompt every backend uses.
///
/// The tool list is rendered from [`TOOLS`], so the DJ inherits whatever the MCP
/// server publishes.
pub fn system_prompt() -> String {
  format!(
    "You are the DJ for a terminal music player, talking to the listener in a chat \
     window. You can hold a conversation and you can act on the player.\n\n\
     Work one step at a time. Each step you reply with ONLY a JSON object, no prose \
     outside it:\n{}\n\n\
     `say` is what the listener reads: a sentence or two, plain text.\n\
     `tool_calls` is what you want run. Anything you call is executed and the \
     results come straight back to you, so you can look something up, then decide.\n\n\
     Rules:\n\
     - Reply with words and NO tool calls when you are done, or when you would \
       rather ask the listener something than guess. Asking is normal; do not queue \
       music just to have done something.\n\
     - Do not narrate a tool call you have not made. Call it, read the result, then \
       tell the listener what actually happened.\n\
     - When you queue, give real, existing tracks with their exact released titles \
       and the primary artist. queue_tracks reports what it could not find, so read \
       its result instead of assuming.\n\
     - Do not repeat anything listed as recently played, queued, or now playing.\n\
     - Vary the artists; do not queue several tracks by the same one unless asked.\n\n\
     Tools:\n{}",
    step_shape_hint(),
    tool_catalogue()
  )
}

fn step_shape_hint() -> String {
  json!({
    "say": "what the listener reads, or empty",
    "tool_calls": [{"name": "a tool name", "arguments": {}}]
  })
  .to_string()
}

/// The tool table as prompt text: name, what it does, and its argument schema.
fn tool_catalogue() -> String {
  TOOLS
    .iter()
    .map(|tool| {
      format!(
        "- {}: {}\n  arguments: {}",
        tool.name,
        tool.description,
        (tool.schema)()
      )
    })
    .collect::<Vec<_>>()
    .join("\n")
}

/// The JSON Schema backends use to constrain a step, where they support it.
///
/// `arguments` is deliberately an unconstrained object: the per-tool schemas vary,
/// and no single schema can describe them all. [`super::tools::parse_call`]
/// validates the arguments properly once the tool is known.
pub fn step_schema() -> Value {
  json!({
    "type": "object",
    "properties": {
      "say": {"type": "string"},
      "tool_calls": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "name": {"type": "string", "enum": TOOLS.iter().map(|tool| tool.name).collect::<Vec<_>>()},
            "arguments": {"type": "object"}
          },
          "required": ["name", "arguments"]
        }
      }
    },
    "required": ["say", "tool_calls"],
    "additionalProperties": false
  })
}

/// The user-turn text: the taste brief, the conversation, and the ask.
pub fn user_prompt(request: &DjRequest) -> String {
  let mut prompt = request.brief.to_prompt_block();

  if !request.history.is_empty() {
    prompt.push_str("\nConversation so far:\n");
    for (speaker, text) in &request.history {
      prompt.push_str(&format!("{speaker}: {text}\n"));
    }
  }

  if !request.scratch.is_empty() {
    prompt.push_str("\nTools you have already run this turn:\n");
    for exchange in &request.scratch {
      prompt.push_str(&format!(
        "{}({}) returned:\n{}\n\n",
        exchange.name, exchange.arguments, exchange.result
      ));
    }
  }

  if request.avoid_library {
    prompt.push_str(
      "\nThe listener only wants tracks they do NOT already own. Anything already \
       in their Liked Songs or their own playlists will be rejected, so avoid the \
       obvious well-known picks for this taste and reach for adjacent artists, \
       deeper album cuts, and newer releases instead.\n",
    );
  }

  prompt.push_str(&if request.must_act {
    // No listener is watching a refill, so a question here reads as a hang.
    format!(
      "\nQueue {} track(s) now with queue_tracks, naming them by title and artist — \
       it resolves names itself, so do not spend a step searching first. Do not ask \
       any questions; nobody is watching. Reply with only the JSON object.",
      request.want
    )
  } else {
    format!(
      "\nTake the next step now. Queue about {} track(s) if this is the point at \
       which you queue. Reply with only the JSON object.",
      request.want
    )
  });
  prompt
}

/// Pull a [`DjStep`] out of whatever a backend produced.
///
/// Deliberately tolerant. A structured-output request gives back bare JSON, but
/// an agent CLI wraps its answer in prose and often a ```json fence — and each
/// CLI does it differently. Scanning for the JSON is uniform across all of them
/// and survives version bumps in any of them.
pub fn parse_step(raw: &str) -> Result<DjStep> {
  let candidate = extract_json_object(raw)
    .ok_or_else(|| anyhow!("no JSON object found in the reply: {}", truncate(raw, 200)))?;
  let value: Value = serde_json::from_str(&candidate).map_err(|e| {
    anyhow!(
      "reply was not valid JSON ({e}): {}",
      truncate(&candidate, 200)
    )
  })?;

  let say = value
    .get("say")
    .and_then(Value::as_str)
    .map(str::trim)
    .filter(|say| !say.is_empty())
    .map(str::to_string);

  let mut calls: Vec<ToolInvocation> = value
    .get("tool_calls")
    .and_then(Value::as_array)
    .map(|calls| {
      calls
        .iter()
        .filter_map(|call| {
          let name = call.get("name").and_then(Value::as_str)?.trim();
          if name.is_empty() {
            return None;
          }
          Some(ToolInvocation {
            name: name.to_string(),
            // A tool taking no arguments is often written without the key at all.
            arguments: call.get("arguments").cloned().unwrap_or_else(|| json!({})),
          })
        })
        .collect()
    })
    .unwrap_or_default();

  // A bare `tracks` array is not the protocol, but small local models fall back to
  // it constantly. Reading it as the queue call it plainly means costs ten lines
  // and is the difference between a weak model working and doing nothing.
  if calls.is_empty() {
    if let Some(tracks) = value.get("tracks").and_then(Value::as_array) {
      if !tracks.is_empty() {
        calls.push(ToolInvocation {
          name: "queue_tracks".to_string(),
          arguments: json!({ "tracks": tracks }),
        });
      }
    }
  }

  if calls.is_empty() && say.is_none() {
    return Err(anyhow!(
      "reply contained neither a message nor any tool calls"
    ));
  }
  Ok(DjStep { say, calls })
}

/// Find the JSON object in a blob of text.
///
/// Prefers the **last** fenced ```json block, then the last balanced `{…}` run:
/// a model that reasons out loud before answering tends to put the real answer
/// last, and an agent CLI may echo the requested shape from the prompt before
/// producing its own.
fn extract_json_object(raw: &str) -> Option<String> {
  if let Some(fenced) = last_fenced_block(raw) {
    if let Some(object) = last_balanced_object(&fenced) {
      return Some(object);
    }
  }
  last_balanced_object(raw)
}

fn last_fenced_block(raw: &str) -> Option<String> {
  let mut blocks = Vec::new();
  let mut rest = raw;
  while let Some(start) = rest.find("```") {
    let after = &rest[start + 3..];
    // Skip an optional language tag on the fence line.
    let body_start = after.find('\n').map(|i| i + 1).unwrap_or(after.len());
    let body = &after[body_start..];
    match body.find("```") {
      Some(end) => {
        blocks.push(body[..end].to_string());
        rest = &body[end + 3..];
      }
      // Unterminated fence: take the remainder, since a truncated reply is
      // still worth trying to read.
      None => {
        blocks.push(body.to_string());
        break;
      }
    }
  }
  blocks.pop()
}

/// The last top-level balanced `{…}` in `raw`, ignoring braces inside strings.
fn last_balanced_object(raw: &str) -> Option<String> {
  let bytes = raw.as_bytes();
  let mut best: Option<(usize, usize)> = None;
  let mut start: Option<usize> = None;
  let mut depth = 0usize;
  let mut in_string = false;
  let mut escaped = false;

  for (index, &byte) in bytes.iter().enumerate() {
    if in_string {
      if escaped {
        escaped = false;
      } else if byte == b'\\' {
        escaped = true;
      } else if byte == b'"' {
        in_string = false;
      }
      continue;
    }
    match byte {
      b'"' => in_string = true,
      b'{' => {
        if depth == 0 {
          start = Some(index);
        }
        depth += 1;
      }
      b'}' => {
        depth = depth.saturating_sub(1);
        if depth == 0 {
          if let Some(open) = start.take() {
            best = Some((open, index + 1));
          }
        }
      }
      _ => {}
    }
  }
  best.map(|(open, close)| raw[open..close].to_string())
}

fn truncate(value: &str, max: usize) -> String {
  let trimmed = value.trim();
  if trimmed.chars().count() <= max {
    return trimmed.to_string();
  }
  trimmed.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_a_bare_json_step() {
    let step = parse_step(
      r#"{"say":"Here you go.","tool_calls":[{"name":"queue_tracks","arguments":{"tracks":[{"title":"Nude","artist":"Radiohead"}]}}]}"#,
    )
    .unwrap();
    assert_eq!(step.say.as_deref(), Some("Here you go."));
    assert_eq!(step.calls.len(), 1);
    assert_eq!(step.calls[0].name, "queue_tracks");
  }

  #[test]
  fn a_step_with_words_and_no_calls_is_a_conversational_reply() {
    // The whole point of the loop's terminal case: the DJ is allowed to just talk.
    let step =
      parse_step(r#"{"say":"Instrumental, or are vocals fine?","tool_calls":[]}"#).unwrap();
    assert_eq!(
      step.say.as_deref(),
      Some("Instrumental, or are vocals fine?")
    );
    assert!(step.calls.is_empty());
  }

  #[test]
  fn parses_a_fenced_step_wrapped_in_prose() {
    // What an agent CLI actually emits.
    let raw = "Sure! Here's a mellow set for you.\n\n```json\n{\"say\":\"Mellow.\",\"tool_calls\":[{\"name\":\"get_queue\",\"arguments\":{}}]}\n```\n\nLet me know if you want more.";
    let step = parse_step(raw).unwrap();
    assert_eq!(step.say.as_deref(), Some("Mellow."));
    assert_eq!(step.calls[0].name, "get_queue");
  }

  #[test]
  fn prefers_the_last_fenced_block_when_the_prompt_shape_is_echoed() {
    // Regression guard: a CLI that repeats the requested shape before answering
    // must not have the example parsed as the answer.
    let raw = concat!(
      "I'll use this shape:\n```json\n{\"say\":\"what the listener reads\",\"tool_calls\":[]}\n```\n",
      "Here is the actual set:\n```json\n{\"say\":\"Real answer.\",\"tool_calls\":[{\"name\":\"skip_track\",\"arguments\":{}}]}\n```"
    );
    let step = parse_step(raw).unwrap();
    assert_eq!(step.say.as_deref(), Some("Real answer."));
    assert_eq!(step.calls.len(), 1);
  }

  #[test]
  fn handles_an_unlabelled_fence() {
    let raw =
      "```\n{\"say\":\"ok\",\"tool_calls\":[{\"name\":\"get_queue\",\"arguments\":{}}]}\n```";
    assert_eq!(parse_step(raw).unwrap().calls.len(), 1);
  }

  #[test]
  fn a_call_with_no_arguments_key_is_still_a_call() {
    // Models routinely omit `arguments` for a no-argument tool.
    let step = parse_step(r#"{"say":"","tool_calls":[{"name":"get_now_playing"}]}"#).unwrap();
    assert_eq!(step.calls[0].name, "get_now_playing");
    assert_eq!(step.calls[0].arguments, json!({}));
  }

  #[test]
  fn a_bare_tracks_array_is_read_as_the_queue_call_it_means() {
    // Small local models fall back to this shape constantly; reading it is the
    // difference between one of them working and doing nothing at all.
    let step =
      parse_step(r#"{"say":"ok","tracks":[{"title":"Nude","artist":"Radiohead"}]}"#).unwrap();
    assert_eq!(step.calls.len(), 1);
    assert_eq!(step.calls[0].name, "queue_tracks");
    assert_eq!(step.calls[0].arguments["tracks"][0]["title"], json!("Nude"));

    // But an explicit tool call always wins over it.
    let both = parse_step(
      r#"{"say":"ok","tracks":[{"title":"A","artist":"B"}],"tool_calls":[{"name":"skip_track","arguments":{}}]}"#,
    )
    .unwrap();
    assert_eq!(both.calls.len(), 1);
    assert_eq!(both.calls[0].name, "skip_track");
  }

  #[test]
  fn braces_inside_strings_do_not_confuse_the_scanner() {
    let raw = r#"{"say":"a } brace { inside","tool_calls":[{"name":"get_queue","arguments":{}}]}"#;
    let step = parse_step(raw).unwrap();
    assert_eq!(step.say.as_deref(), Some("a } brace { inside"));
    assert_eq!(step.calls.len(), 1);
  }

  #[test]
  fn escaped_quotes_inside_strings_are_handled() {
    let raw = r#"{"say":"they said \"hi\"","tool_calls":[]}"#;
    assert_eq!(
      parse_step(raw).unwrap().say.as_deref(),
      Some("they said \"hi\"")
    );
  }

  #[test]
  fn a_call_missing_a_name_is_dropped_not_fatal() {
    let raw = r#"{"say":"ok","tool_calls":[{"arguments":{}},{"name":"get_queue","arguments":{}}]}"#;
    let step = parse_step(raw).unwrap();
    assert_eq!(step.calls.len(), 1);
    assert_eq!(step.calls[0].name, "get_queue");
  }

  #[test]
  fn a_reply_with_neither_message_nor_calls_is_an_error() {
    assert!(parse_step(r#"{"tool_calls":[]}"#).is_err());
  }

  #[test]
  fn non_json_output_is_an_error_naming_what_was_seen() {
    let err = parse_step("I'm afraid I can't do that.")
      .unwrap_err()
      .to_string();
    assert!(err.contains("no JSON object"));
    assert!(err.contains("can't do that"));
  }

  #[test]
  fn truncated_json_reports_a_parse_failure() {
    let err = parse_step(r#"{"say":"oops","tool_calls":[{"name":"#)
      .unwrap_err()
      .to_string();
    assert!(
      err.contains("no JSON object") || err.contains("not valid JSON"),
      "{err}"
    );
  }

  #[test]
  fn the_prompt_offers_every_tool_the_mcp_server_publishes() {
    // Iterated rather than listed, so a tool added to the table fails this until
    // the in-TUI DJ can see it too. That inheritance is the point of the design.
    let prompt = system_prompt();
    for tool in TOOLS {
      assert!(prompt.contains(tool.name), "{} is not offered", tool.name);
    }
    let names = step_schema()["properties"]["tool_calls"]["items"]["properties"]["name"]["enum"]
      .as_array()
      .unwrap()
      .clone();
    assert_eq!(names.len(), TOOLS.len(), "the schema must offer them too");
  }

  #[test]
  fn a_tool_result_reaches_the_next_step_verbatim() {
    let request = DjRequest {
      want: 5,
      scratch: vec![ToolExchange {
        name: "queue_tracks".into(),
        arguments: json!({"tracks": []}),
        result: "Not found (skipped): Ghost Track — Nobody".into(),
      }],
      ..DjRequest::default()
    };
    let prompt = user_prompt(&request);
    // How the model learns to stop naming a track that does not exist. It replaces
    // the old repair round, which had to be told the same thing out of band.
    assert!(prompt.contains("Ghost Track — Nobody"));
    assert!(prompt.contains("queue_tracks"));
  }

  #[test]
  fn a_must_act_turn_is_told_to_queue_without_asking() {
    // Nobody is watching an auto-queue refill, so a question there is a hang.
    let request = DjRequest {
      want: 6,
      must_act: true,
      ..DjRequest::default()
    };
    let prompt = user_prompt(&request);
    assert!(prompt.contains("Queue 6 track(s) now"));
    assert!(prompt.contains("Do not ask any questions"));
    // And it must not spend one of its two steps on a search it does not need.
    assert!(prompt.contains("do not spend a step searching"));

    let conversational = DjRequest {
      want: 6,
      ..DjRequest::default()
    };
    assert!(!user_prompt(&conversational).contains("Do not ask any questions"));
  }

  #[test]
  fn conversation_history_is_included_in_order() {
    let request = DjRequest {
      history: vec![
        ("Listener".into(), "something chill".into()),
        ("DJ".into(), "here you go".into()),
      ],
      want: 3,
      ..DjRequest::default()
    };
    let prompt = user_prompt(&request);
    let chill = prompt.find("something chill").unwrap();
    let here = prompt.find("here you go").unwrap();
    assert!(chill < here, "history must stay in order");
  }

  #[test]
  fn the_last_listener_line_is_the_live_request() {
    // Load-bearing: there is no separate instruction slot, so what the listener
    // typed reaches the model *only* as the final history line — which is why the
    // handler pushes it to the transcript and does not also pass it alongside.
    let request = DjRequest {
      history: vec![
        ("Listener".into(), "something chill".into()),
        ("DJ".into(), "here you go".into()),
        ("Listener".into(), "now something faster".into()),
      ],
      want: 3,
      ..DjRequest::default()
    };
    let prompt = user_prompt(&request);
    assert_eq!(
      prompt.matches("now something faster").count(),
      1,
      "the ask must appear once, not once per delivery route"
    );
    let ask = prompt.find("now something faster").unwrap();
    assert!(ask > prompt.find("Conversation so far:").unwrap());
    assert!(
      ask < prompt.find("Take the next step now").unwrap(),
      "the newest line has to sit immediately before the ask to read as the request"
    );
  }

  #[test]
  fn the_avoid_library_prompt_asks_for_the_unfamiliar() {
    let request = DjRequest {
      want: 6,
      avoid_library: true,
      ..DjRequest::default()
    };
    let prompt = user_prompt(&request);
    assert!(prompt.contains("do NOT already own"));
    // Without this steer the model reaches for the listener's favourites first and
    // the filter eats the batch.
    assert!(prompt.contains("newer releases"));

    let off = DjRequest {
      want: 6,
      ..DjRequest::default()
    };
    assert!(!user_prompt(&off).contains("already own"));
  }

  #[test]
  fn real_agent_cli_output_parses_noise_and_all() {
    // Captured verbatim from `copilot --no-color -p "<the real prompt>"`. It
    // narrates its own shell activity on *stdout* before the answer, which is
    // exactly why the parser scans for the last balanced object rather than
    // trusting the whole stream to be JSON.
    let raw = "\u{2717} Write dummy\n  \u{2514} noop\n\n\u{25cf} noop (shell)\n  \u{2502}                echo not used\n  \u{2514} 2 lines\u{2026}\n\n{\"say\":\"\",\"tool_calls\":               [{\"name\":\"get_now_playing\",\"arguments\":{}},{\"name\":\"get_queue\",               \"arguments\":{}}]}";
    let step = parse_step(raw).unwrap();
    assert_eq!(step.calls.len(), 2);
    assert_eq!(step.calls[0].name, "get_now_playing");
    assert_eq!(step.calls[1].name, "get_queue");
    // An empty `say` is a real answer from a real CLI, and must not become a
    // blank transcript line.
    assert_eq!(step.say, None);
  }

  #[test]
  fn schema_matches_the_shape_the_prompt_describes() {
    let schema = step_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("say")));
    assert!(required.contains(&json!("tool_calls")));
    let call_required = schema["properties"]["tool_calls"]["items"]["required"]
      .as_array()
      .unwrap();
    assert!(call_required.contains(&json!("name")));
    assert!(call_required.contains(&json!("arguments")));
    // The prompt's example must be parseable by our own parser.
    assert!(parse_step(&step_shape_hint()).is_ok());
  }
}
