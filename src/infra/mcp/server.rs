//! The MCP request handler and the newline-delimited JSON-RPC loop.
//!
//! Generic over the byte stream and over how tools actually run, because the
//! same code serves two very different situations:
//!
//! * inside the running TUI, over a loopback TCP connection, with a real
//!   [`ToolExecutor`] that can touch `App` and Spotify;
//! * inside `spotatui mcp` when no TUI is running, over stdio, with an executor
//!   that only reports that the player is not up.
//!
//! The spec explicitly allows the second case: the stdio framing — one
//! newline-delimited JSON-RPC message per line over a reliable bidirectional
//! byte stream — "works unchanged over Unix domain sockets, TCP connections, or
//! any similar channel".

use super::executor::ToolExecutor;
use super::protocol::{self as proto, Era, MetaCheck};
use crate::infra::dj::tools::{self, ToolCallError};
use serde_json::{json, Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// Per-connection state. The protocol is stateless by design, so this holds only
/// what is genuinely per-connection: the era latch, plus identity captured for
/// diagnostics (never for behaviour — the spec is explicit that `clientInfo` is
/// self-reported and must not drive decisions).
#[derive(Default)]
pub struct Session {
  pub era: Era,
  /// Protocol version the client last declared, for logs.
  pub negotiated_version: Option<String>,
  /// `name/version` the client self-reported, for logs.
  pub client: Option<String>,
}

/// Run the protocol loop until the input stream ends.
///
/// Exits on EOF, which the spec names as the primary graceful-shutdown signal
/// for stdio and the only portable one.
pub async fn serve<R, W, E>(input: R, mut output: W, executor: &E) -> std::io::Result<()>
where
  R: tokio::io::AsyncRead + Unpin,
  W: AsyncWrite + Unpin,
  E: ToolExecutor,
{
  let mut lines = BufReader::new(input).lines();
  let mut session = Session::default();

  while let Some(line) = lines.next_line().await? {
    if line.trim().is_empty() {
      continue;
    }
    if let Some(response) = handle_line(&mut session, &line, executor).await {
      // One message per line, and never an embedded newline: `to_string` on a
      // `Value` is compact, so this holds by construction.
      output.write_all(response.to_string().as_bytes()).await?;
      output.write_all(b"\n").await?;
      output.flush().await?;
    }
  }
  Ok(())
}

/// Handle one line, returning the response to write, or `None` for
/// notifications (which must never be answered).
pub async fn handle_line<E: ToolExecutor>(
  session: &mut Session,
  line: &str,
  executor: &E,
) -> Option<Value> {
  let message: Value = match serde_json::from_str(line) {
    Ok(message) => message,
    Err(e) => {
      return Some(proto::error_response(
        None,
        proto::PARSE_ERROR,
        format!("Parse error: {e}"),
      ))
    }
  };

  let id = message.get("id").filter(|id| !id.is_null()).cloned();
  let Some(method) = message.get("method").and_then(Value::as_str) else {
    return Some(proto::error_response(
      id.as_ref(),
      proto::INVALID_REQUEST,
      "Request must include a method",
    ));
  };
  let params = message.get("params");

  // Notifications carry no id and must not be answered.
  if id.is_none() {
    handle_notification(session, method);
    return None;
  }

  Some(handle_request(session, method, params, id.as_ref(), executor).await)
}

fn handle_notification(_session: &mut Session, method: &str) {
  match method {
    // Legacy handshake completion. Nothing to do — the era was already latched
    // by `initialize`.
    "notifications/initialized" | "initialized" => {}
    // Every tool call here completes in one step, so there is nothing to abort;
    // acknowledged so it is not mistaken for an unimplemented method.
    "notifications/cancelled" => {}
    other => log::debug!("MCP: ignoring notification {other}"),
  }
}

async fn handle_request<E: ToolExecutor>(
  session: &mut Session,
  method: &str,
  params: Option<&Value>,
  id: Option<&Value>,
  executor: &E,
) -> Value {
  match method {
    // Modern discovery. Also the backward-compatibility probe a dual-era client
    // sends first, so answering it settles the era as modern.
    "server/discover" => {
      if let Some(response) = settle_modern(session, params, id) {
        return response;
      }
      proto::result_response(id, &session.era, proto::discover_body())
    }

    // Legacy handshake. Its presence *is* the signal to serve legacy semantics
    // for the rest of this connection.
    "initialize" => {
      let requested = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str);
      let negotiated = proto::negotiate_legacy(requested);
      session.era = Era::Legacy(negotiated.clone());
      session.negotiated_version = Some(negotiated.clone());
      // Legacy clients put identity at the top level rather than in `_meta`.
      session.client = params
        .and_then(|params| params.get("clientInfo"))
        .and_then(implementation_label);
      log::debug!(
        "MCP: legacy handshake, serving protocol {negotiated} to {}",
        describe(session)
      );
      // Built before the era is consulted for shaping, so pass Legacy directly.
      proto::result_response(id, &session.era, proto::initialize_body(&negotiated))
    }

    "tools/list" => {
      if let Some(response) = require_era(session, params, id) {
        return response;
      }
      let mut body = Map::new();
      body.insert("tools".to_string(), json!(tool_descriptors()));
      if !session.era.is_legacy() {
        // `CacheableResult` is required on list results in the modern revision.
        proto::add_cache_hints(&mut body);
      }
      proto::result_response(id, &session.era, body)
    }

    "tools/call" => {
      if let Some(response) = require_era(session, params, id) {
        return response;
      }
      // A missing `name` is a malformed request, not a call to a tool called
      // "". Defaulting it to empty reported `METHOD_NOT_FOUND: Unknown tool: `,
      // which tells the client nothing about what to fix.
      let Some(name) = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
      else {
        return proto::error_response(id, proto::INVALID_PARAMS, "tools/call requires params.name");
      };
      let args = params
        .and_then(|params| params.get("arguments"))
        .cloned()
        .unwrap_or(Value::Null);

      // Per the spec, a bad request shape is a *protocol* error while a tool
      // that ran and failed is a result with `isError: true`. Keeping these
      // apart matters: clients feed execution errors back to the model for
      // self-correction, but not protocol errors.
      let call = match tools::parse_call(name, &args) {
        Ok(call) => call,
        Err(ToolCallError::UnknownTool(name)) => {
          return proto::error_response(
            id,
            proto::METHOD_NOT_FOUND,
            format!("Unknown tool: {name}"),
          )
        }
        Err(e @ ToolCallError::InvalidArguments(_)) => {
          return proto::error_response(id, proto::INVALID_PARAMS, e.to_string())
        }
      };

      let outcome = executor.call(call).await;
      proto::result_response(
        id,
        &session.era,
        proto::tool_result_body(&outcome.text, outcome.structured, outcome.is_error),
      )
    }

    // Not implemented, deliberately: `prompts` and `resources` are not
    // advertised in our capabilities, and `ping` / `logging/setLevel` were
    // removed from the protocol in this revision.
    other => proto::error_response(
      id,
      proto::METHOD_NOT_FOUND,
      format!("Method not found: {other}"),
    ),
  }
}

/// Validate a modern request and latch [`Era::Modern`], or return the error.
fn settle_modern(
  session: &mut Session,
  params: Option<&Value>,
  id: Option<&Value>,
) -> Option<Value> {
  match proto::check_meta(params) {
    MetaCheck::Ok { version } => {
      if session.era != Era::Modern || session.negotiated_version.as_deref() != Some(&version) {
        log::debug!("MCP: serving protocol {version} to {}", describe(session));
      }
      session.era = Era::Modern;
      session.negotiated_version = Some(version);
      note_client(session, params);
      None
    }
    MetaCheck::UnsupportedVersion(requested) => {
      Some(proto::unsupported_version_response(id, &requested))
    }
    MetaCheck::MissingCapabilities => Some(proto::missing_capabilities_response(id)),
    MetaCheck::Malformed(detail) => Some(proto::error_response(id, proto::INVALID_PARAMS, detail)),
  }
}

/// Decide how to serve a substantive request, settling the era if needed.
///
/// A legacy connection has already handshaken, so its requests carry no `_meta`
/// and must not be rejected for that. A connection that has not handshaken must
/// present modern `_meta` — which is also what gives a legacy client that
/// skipped `initialize` an actionable error naming the versions we speak.
fn require_era(session: &mut Session, params: Option<&Value>, id: Option<&Value>) -> Option<Value> {
  if session.era.is_legacy() {
    return None;
  }
  if session.era == Era::Unknown && !proto::looks_modern(params) {
    return Some(proto::error_response_with_data(
      id,
      proto::INVALID_PARAMS,
      "Requests must carry params._meta['io.modelcontextprotocol/protocolVersion'] and \
       ['io.modelcontextprotocol/clientCapabilities'], or open with an initialize handshake",
      json!({ "supported": proto::SUPPORTED_VERSIONS }),
    ));
  }
  settle_modern(session, params, id)
}

/// Record the client's self-reported identity from a modern request's `_meta`.
fn note_client(session: &mut Session, params: Option<&Value>) {
  if session.client.is_some() {
    return;
  }
  session.client = proto::meta(params)
    .and_then(|meta| meta.get(proto::META_CLIENT_INFO))
    .and_then(implementation_label);
}

/// `"name/version"` from an MCP `Implementation` object.
fn implementation_label(value: &Value) -> Option<String> {
  let name = value.get("name").and_then(Value::as_str)?;
  let version = value.get("version").and_then(Value::as_str).unwrap_or("?");
  Some(format!("{name}/{version}"))
}

fn describe(session: &Session) -> &str {
  session
    .client
    .as_deref()
    .unwrap_or("an unidentified client")
}

/// The tool list, in the fixed order [`tools::TOOLS`] declares.
fn tool_descriptors() -> Vec<Value> {
  tools::TOOLS
    .iter()
    .map(|tool| {
      json!({
        "name": tool.name,
        "title": tool.title,
        "description": tool.description,
        "inputSchema": (tool.schema)(),
        "annotations": {
          "title": tool.title,
          "readOnlyHint": tool.read_only,
          // Nothing here destroys data; the worst case is a queue the user has
          // to clear. Skipping and playing are not idempotent, though.
          "destructiveHint": false,
          "idempotentHint": tool.read_only,
          "openWorldHint": true,
        },
      })
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::dj::tools::{DjToolCall, ToolOutcome};

  /// Records what it was asked to run and echoes a canned answer.
  struct FakeExecutor;

  impl ToolExecutor for FakeExecutor {
    async fn call(&self, call: DjToolCall) -> ToolOutcome {
      ToolOutcome::ok(format!("ran {}", call.tool_name()))
    }
  }

  fn modern_meta() -> Value {
    json!({
      proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
      proto::META_CLIENT_CAPABILITIES: {},
      proto::META_CLIENT_INFO: {"name": "test", "version": "1"},
    })
  }

  async fn exchange(session: &mut Session, message: Value) -> Value {
    handle_line(session, &message.to_string(), &FakeExecutor)
      .await
      .expect("expected a response")
  }

  #[tokio::test]
  async fn modern_discover_then_tools_list() {
    let mut session = Session::default();
    let discover = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover",
             "params": {"_meta": modern_meta()}}),
    )
    .await;
    assert_eq!(discover["result"]["resultType"], json!("complete"));
    assert_eq!(session.era, Era::Modern);
    assert!(discover["result"]["supportedVersions"]
      .as_array()
      .unwrap()
      .contains(&json!(proto::PROTOCOL_MODERN)));

    let listed = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list",
             "params": {"_meta": modern_meta()}}),
    )
    .await;
    let result = &listed["result"];
    assert_eq!(result["resultType"], json!("complete"));
    // CacheableResult fields are mandatory on list results now.
    assert_eq!(result["ttlMs"], json!(proto::LIST_TTL_MS));
    assert_eq!(result["cacheScope"], json!(proto::CACHE_SCOPE));
    assert_eq!(
      result["_meta"][proto::META_SERVER_INFO]["name"],
      json!("spotatui")
    );
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), crate::infra::dj::tools::TOOLS.len());
    assert_eq!(tools[0]["name"], json!("get_listening_history"));
  }

  #[tokio::test]
  async fn legacy_initialize_latches_and_shapes_results() {
    let mut session = Session::default();
    let init = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {"name": "legacy", "version": "1"}
      }}),
    )
    .await;
    assert_eq!(init["result"]["protocolVersion"], json!("2025-06-18"));
    assert!(init["result"].get("resultType").is_none());
    assert_eq!(session.era, Era::Legacy("2025-06-18".into()));

    // A legacy client's later requests carry no `_meta` at all, and must still
    // be served rather than rejected for the missing fields.
    let listed = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert!(listed["result"]["tools"].is_array());
    assert!(listed["result"].get("resultType").is_none());
    assert!(listed["result"].get("ttlMs").is_none());
  }

  #[tokio::test]
  async fn era_latch_survives_across_requests_on_one_connection() {
    let mut session = Session::default();
    exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2025-11-25"}}),
    )
    .await;
    for id in 2..5 {
      let response = exchange(
        &mut session,
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"}),
      )
      .await;
      assert!(response["result"].get("resultType").is_none(), "id {id}");
    }
    assert_eq!(session.era, Era::Legacy("2025-11-25".into()));
  }

  #[tokio::test]
  async fn unknown_legacy_version_still_negotiates() {
    // A legacy client has no fall-forward path, so failing the handshake would
    // leave it permanently broken.
    let mut session = Session::default();
    let init = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "1999-01-01"}}),
    )
    .await;
    assert_eq!(
      init["result"]["protocolVersion"],
      json!(proto::PROTOCOL_LEGACY_FALLBACK)
    );
  }

  #[tokio::test]
  async fn modern_request_without_meta_is_rejected_with_invalid_params() {
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    )
    .await;
    assert_eq!(response["error"]["code"], json!(proto::INVALID_PARAMS));
    // Name the versions we speak, so a confused client can recover.
    assert!(response["error"]["data"]["supported"].is_array());
  }

  #[tokio::test]
  async fn unsupported_version_is_reported_with_32022() {
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": {
        proto::META_PROTOCOL_VERSION: "1900-01-01",
        proto::META_CLIENT_CAPABILITIES: {},
      }}}),
    )
    .await;
    assert_eq!(response["error"]["code"], json!(-32022));
  }

  #[tokio::test]
  async fn missing_client_capabilities_is_reported_with_32021() {
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": {
        proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
      }}}),
    )
    .await;
    assert_eq!(response["error"]["code"], json!(-32021));
  }

  #[tokio::test]
  async fn tool_call_runs_and_reports() {
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "_meta": modern_meta(),
        "name": "get_now_playing",
        "arguments": {}
      }}),
    )
    .await;
    assert_eq!(response["result"]["isError"], json!(false));
    assert_eq!(
      response["result"]["content"][0]["text"],
      json!("ran get_now_playing")
    );
  }

  #[tokio::test]
  async fn unknown_tool_is_a_protocol_error_not_an_is_error_result() {
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "_meta": modern_meta(),
        "name": "make_coffee",
        "arguments": {}
      }}),
    )
    .await;
    assert_eq!(response["error"]["code"], json!(proto::METHOD_NOT_FOUND));
    assert!(response.get("result").is_none());
  }

  #[tokio::test]
  async fn a_call_with_no_tool_name_is_invalid_params_not_an_unknown_tool() {
    // Defaulting the missing name to "" reported `Unknown tool: ` under
    // METHOD_NOT_FOUND, which names nothing and points the client at the wrong
    // problem: the request shape is what is wrong, not the tool table.
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "_meta": modern_meta(),
        "arguments": {}
      }}),
    )
    .await;
    assert_eq!(response["error"]["code"], json!(proto::INVALID_PARAMS));
    assert!(response["error"]["message"]
      .as_str()
      .unwrap()
      .contains("params.name"));
  }

  #[tokio::test]
  async fn bad_tool_arguments_are_invalid_params() {
    let mut session = Session::default();
    let response = exchange(
      &mut session,
      json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
        "_meta": modern_meta(),
        "name": "search_tracks",
        "arguments": {}
      }}),
    )
    .await;
    assert_eq!(response["error"]["code"], json!(proto::INVALID_PARAMS));
  }

  #[tokio::test]
  async fn notifications_are_never_answered() {
    let mut session = Session::default();
    for method in ["notifications/initialized", "notifications/cancelled"] {
      let message = json!({"jsonrpc": "2.0", "method": method});
      assert!(
        handle_line(&mut session, &message.to_string(), &FakeExecutor)
          .await
          .is_none(),
        "{method} must not get a response"
      );
    }
  }

  #[tokio::test]
  async fn malformed_json_is_a_parse_error() {
    let mut session = Session::default();
    let response = handle_line(&mut session, "{not json", &FakeExecutor)
      .await
      .unwrap();
    assert_eq!(response["error"]["code"], json!(proto::PARSE_ERROR));
    assert_eq!(response["id"], Value::Null);
  }

  #[tokio::test]
  async fn method_is_required() {
    let mut session = Session::default();
    let response = handle_line(&mut session, &json!({"id": 1}).to_string(), &FakeExecutor)
      .await
      .unwrap();
    assert_eq!(response["error"]["code"], json!(proto::INVALID_REQUEST));
  }

  #[tokio::test]
  async fn removed_methods_report_method_not_found() {
    // `ping` and `logging/setLevel` were removed in 2026-07-28; we advertise
    // neither prompts nor resources.
    let mut session = Session::default();
    session.era = Era::Modern;
    for method in ["ping", "logging/setLevel", "prompts/list", "resources/list"] {
      let response = exchange(
        &mut session,
        json!({"jsonrpc": "2.0", "id": 9, "method": method, "params": {"_meta": modern_meta()}}),
      )
      .await;
      assert_eq!(
        response["error"]["code"],
        json!(proto::METHOD_NOT_FOUND),
        "{method}"
      );
    }
  }

  #[tokio::test]
  async fn serve_pumps_a_whole_conversation_over_a_stream() {
    let input = format!(
      "{}\n{}\n",
      json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": modern_meta()}}),
      json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"_meta": modern_meta()}}),
    );
    let mut output = Vec::new();
    serve(input.as_bytes(), &mut output, &FakeExecutor)
      .await
      .unwrap();

    let text = String::from_utf8(output).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2, "one response line per request");
    for line in &lines {
      // Never an embedded newline, and always valid JSON on its own line.
      let parsed: Value = serde_json::from_str(line).unwrap();
      assert_eq!(parsed["jsonrpc"], json!("2.0"));
    }
  }

  #[test]
  fn descriptors_expose_read_only_hints_that_match_the_tool_table() {
    for descriptor in tool_descriptors() {
      let name = descriptor["name"].as_str().unwrap();
      let spec = crate::infra::dj::tools::spec(name).unwrap();
      assert_eq!(
        descriptor["annotations"]["readOnlyHint"],
        json!(spec.read_only),
        "{name}"
      );
      assert!(
        descriptor["inputSchema"]["type"] == json!("object"),
        "{name}"
      );
    }
  }
}
