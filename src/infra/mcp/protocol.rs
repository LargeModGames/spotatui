//! MCP wire protocol: versions, error codes, `_meta` keys, and result shaping.
//!
//! Hand-rolled rather than taken from an SDK, for two reasons. The
//! `2026-07-28` revision is new enough that no Rust SDK implements it yet, and
//! spotatui specifically needs to be **dual-era** — see [`Era`] — which is a
//! server-side concern the SDKs do not expose. The surface is small enough that
//! newline-delimited JSON-RPC over `serde_json` is the cheaper option, and it
//! keeps this feature free of new dependencies.
//!
//! Reference: <https://modelcontextprotocol.io/specification/2026-07-28>

use serde_json::{json, Map, Value};

/// The revision this server prefers. Stateless, per-request metadata, no
/// handshake.
pub const PROTOCOL_MODERN: &str = "2026-07-28";

/// Every revision this server will serve, newest first.
///
/// The legacy entries are not decoration. Per the spec's compatibility matrix a
/// *legacy client against a modern-only server fails outright*, and the coding
/// agents this feature exists to serve (Claude Code, Codex, Gemini CLI) shipped
/// against these revisions. Dropping them would make the MCP server useless in
/// exactly the place it is meant to be used.
pub const SUPPORTED_VERSIONS: &[&str] = &[
  PROTOCOL_MODERN,
  "2025-11-25",
  "2025-06-18",
  "2025-03-26",
  "2024-11-05",
];

/// Version reported to a legacy client that did not name one we recognise.
pub const PROTOCOL_LEGACY_FALLBACK: &str = "2025-06-18";

pub const SERVER_NAME: &str = "spotatui";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Freshness hint on cacheable list results. The tool set is a compile-time
/// constant, so it is safe to let clients hold it for a long while.
pub const LIST_TTL_MS: u64 = 3_600_000;
/// `private`, not `public`: these results describe one user's player and
/// listening history, so a shared intermediary must not cache them.
pub const CACHE_SCOPE: &str = "private";

// `_meta` keys reserved by the specification.
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

// JSON-RPC standard codes.
pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
// MCP-allocated codes. The `-32020..=-32099` band is reserved for the spec, and
// these were renumbered out of `-3200x` in this revision — emitting the old
// values would now be a protocol violation.
pub const MISSING_REQUIRED_CLIENT_CAPABILITY: i64 = -32021;
pub const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// Which protocol generation a single connection is speaking.
///
/// The spec makes this a property of how the client *opens*: a request carrying
/// modern per-request `_meta` is served statelessly, an `initialize` request
/// selects legacy semantics scoped to the process. It is latched per connection
/// rather than per process because the TUI serves several agents at once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Era {
  /// Nothing has arrived yet that settles the question.
  #[default]
  Unknown,
  Modern,
  /// Legacy, carrying the version negotiated at `initialize`.
  Legacy(String),
}

impl Era {
  pub fn is_legacy(&self) -> bool {
    matches!(self, Self::Legacy(_))
  }
}

/// Server capabilities.
///
/// `tools` only, and deliberately without `listChanged`: the tool set is a
/// compile-time constant, so there is nothing to notify about and no
/// `subscriptions/listen` stream to implement.
pub fn capabilities() -> Value {
  json!({ "tools": {} })
}

pub fn server_info() -> Value {
  json!({ "name": SERVER_NAME, "version": SERVER_VERSION })
}

/// Guidance handed to the model alongside the tool list.
pub fn instructions() -> &'static str {
  "Controls the user's spotatui music player. To act as their DJ: call \
   get_listening_history first to learn their taste, then search_tracks to \
   confirm a track exists, then queue_tracks. queue_tracks reports anything it \
   could not find, so read the result rather than assuming every track was \
   queued. Prefer queue_tracks over play_now unless the user asked to hear \
   something immediately. search_tracks marks each result as owned when the user \
   already has it (Liked Songs, or a playlist they own or collaborate on): when \
   they asked for something new, pick the ones marked new, or pass exclude_owned \
   to queue_tracks to have it skip the rest for you."
}

/// A JSON-RPC id. Kept as a `Value` because the spec allows string or number and
/// requires echoing it back unchanged.
pub type Id = Value;

pub fn error_response(id: Option<&Id>, code: i64, message: impl Into<String>) -> Value {
  json!({
    "jsonrpc": "2.0",
    "id": id.cloned().unwrap_or(Value::Null),
    "error": { "code": code, "message": message.into() }
  })
}

pub fn error_response_with_data(
  id: Option<&Id>,
  code: i64,
  message: impl Into<String>,
  data: Value,
) -> Value {
  json!({
    "jsonrpc": "2.0",
    "id": id.cloned().unwrap_or(Value::Null),
    "error": { "code": code, "message": message.into(), "data": data }
  })
}

/// The `UnsupportedProtocolVersionError` a modern server owes a client whose
/// requested version it cannot serve.
pub fn unsupported_version_response(id: Option<&Id>, requested: &str) -> Value {
  error_response_with_data(
    id,
    UNSUPPORTED_PROTOCOL_VERSION,
    "Unsupported protocol version",
    json!({ "supported": SUPPORTED_VERSIONS, "requested": requested }),
  )
}

/// Wrap a result body for the connection's era.
///
/// Modern results carry `resultType` and a `_meta.serverInfo`; legacy results
/// must not, since those fields did not exist in those revisions.
pub fn result_response(id: Option<&Id>, era: &Era, mut body: Map<String, Value>) -> Value {
  if !era.is_legacy() {
    body.insert("resultType".to_string(), json!("complete"));
    // `or_insert_with` hands back whatever is already there, so a future caller
    // that put a non-object under `_meta` would panic a connection task here.
    // Overwrite it instead: the field is ours to shape.
    let entry = body.entry("_meta".to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
      *entry = json!({});
    }
    if let Some(meta) = entry.as_object_mut() {
      meta.insert(META_SERVER_INFO.to_string(), server_info());
    }
  }
  json!({
    "jsonrpc": "2.0",
    "id": id.cloned().unwrap_or(Value::Null),
    "result": Value::Object(body)
  })
}

/// Add the `CacheableResult` fields required on list-shaped results.
pub fn add_cache_hints(body: &mut Map<String, Value>) {
  body.insert("ttlMs".to_string(), json!(LIST_TTL_MS));
  body.insert("cacheScope".to_string(), json!(CACHE_SCOPE));
}

/// The `DiscoverResult` body for `server/discover`.
pub fn discover_body() -> Map<String, Value> {
  let mut body = Map::new();
  body.insert("supportedVersions".to_string(), json!(SUPPORTED_VERSIONS));
  body.insert("capabilities".to_string(), capabilities());
  body.insert("instructions".to_string(), json!(instructions()));
  add_cache_hints(&mut body);
  body
}

/// The legacy `initialize` result body.
pub fn initialize_body(negotiated: &str) -> Map<String, Value> {
  let mut body = Map::new();
  body.insert("protocolVersion".to_string(), json!(negotiated));
  body.insert("capabilities".to_string(), capabilities());
  body.insert("serverInfo".to_string(), server_info());
  body.insert("instructions".to_string(), json!(instructions()));
  body
}

/// Pick the version to negotiate with a legacy client.
///
/// Legacy clients have no fall-forward path, so echoing back a version they
/// asked for is the only outcome that leaves them working; when we do not
/// recognise it at all, name one we do rather than failing the handshake.
pub fn negotiate_legacy(requested: Option<&str>) -> String {
  match requested {
    Some(version) if SUPPORTED_VERSIONS.contains(&version) => version.to_string(),
    _ => PROTOCOL_LEGACY_FALLBACK.to_string(),
  }
}

/// Extract `params._meta`.
pub fn meta(params: Option<&Value>) -> Option<&Map<String, Value>> {
  params?.get("_meta")?.as_object()
}

/// Whether a request carries the per-request metadata that identifies a modern
/// client. Used to settle [`Era`] on the first substantive request.
pub fn looks_modern(params: Option<&Value>) -> bool {
  meta(params)
    .and_then(|meta| meta.get(META_PROTOCOL_VERSION))
    .and_then(Value::as_str)
    .is_some()
}

/// Validation outcome for a modern request's `_meta`.
pub enum MetaCheck {
  Ok {
    version: String,
  },
  /// A required field is absent, so the request is malformed.
  Malformed(&'static str),
  UnsupportedVersion(String),
  MissingCapabilities,
}

/// Check the `_meta` fields the spec marks required on every modern request.
pub fn check_meta(params: Option<&Value>) -> MetaCheck {
  let Some(meta) = meta(params) else {
    return MetaCheck::Malformed(
      "requests must carry params._meta with io.modelcontextprotocol/protocolVersion and \
       io.modelcontextprotocol/clientCapabilities",
    );
  };
  let Some(version) = meta.get(META_PROTOCOL_VERSION).and_then(Value::as_str) else {
    return MetaCheck::Malformed(
      "params._meta['io.modelcontextprotocol/protocolVersion'] is required",
    );
  };
  if !SUPPORTED_VERSIONS.contains(&version) {
    return MetaCheck::UnsupportedVersion(version.to_string());
  }
  // Required, and load-bearing rather than ceremonial: the spec forbids relying
  // on a capability the client has not declared.
  if meta.get(META_CLIENT_CAPABILITIES).is_none() {
    return MetaCheck::MissingCapabilities;
  }
  MetaCheck::Ok {
    version: version.to_string(),
  }
}

/// The `MissingRequiredClientCapabilityError` response.
pub fn missing_capabilities_response(id: Option<&Id>) -> Value {
  error_response_with_data(
    id,
    MISSING_REQUIRED_CLIENT_CAPABILITY,
    "Missing required client capabilities",
    json!({ "requiredCapabilities": [META_CLIENT_CAPABILITIES] }),
  )
}

/// A `tools/call` result. `content` is always text; `structuredContent` mirrors
/// it as JSON when the tool has a machine-readable form.
pub fn tool_result_body(
  text: &str,
  structured: Option<Value>,
  is_error: bool,
) -> Map<String, Value> {
  let mut body = Map::new();
  body.insert(
    "content".to_string(),
    json!([{ "type": "text", "text": text }]),
  );
  body.insert("isError".to_string(), json!(is_error));
  if let Some(structured) = structured {
    body.insert("structuredContent".to_string(), structured);
  }
  body
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn modern_results_carry_result_type_and_server_info() {
    let response = result_response(Some(&json!(1)), &Era::Modern, Map::new());
    let result = &response["result"];
    assert_eq!(result["resultType"], json!("complete"));
    assert_eq!(
      result["_meta"][META_SERVER_INFO]["name"],
      json!(SERVER_NAME)
    );
  }

  #[test]
  fn legacy_results_omit_modern_only_fields() {
    // `resultType` did not exist in the legacy revisions; emitting it to a
    // legacy client is at best noise and at worst a validation failure.
    let response = result_response(
      Some(&json!(1)),
      &Era::Legacy("2025-06-18".into()),
      Map::new(),
    );
    let result = &response["result"];
    assert!(result.get("resultType").is_none());
    assert!(result.get("_meta").is_none());
  }

  #[test]
  fn discover_advertises_every_supported_version() {
    let body = discover_body();
    let versions = body["supportedVersions"].as_array().unwrap();
    assert_eq!(versions[0], json!(PROTOCOL_MODERN));
    assert!(versions.len() > 1, "legacy versions must be advertised too");
    // server/discover is cacheable, so it owes the CacheableResult fields.
    assert_eq!(body["ttlMs"], json!(LIST_TTL_MS));
    assert_eq!(body["cacheScope"], json!(CACHE_SCOPE));
  }

  #[test]
  fn capabilities_omit_list_changed() {
    // Claiming listChanged would oblige us to serve subscriptions/listen.
    let caps = capabilities();
    assert!(caps["tools"].as_object().unwrap().is_empty());
  }

  #[test]
  fn meta_check_accepts_a_well_formed_modern_request() {
    let params = json!({"_meta": {
      META_PROTOCOL_VERSION: PROTOCOL_MODERN,
      META_CLIENT_CAPABILITIES: {},
    }});
    assert!(matches!(
      check_meta(Some(&params)),
      MetaCheck::Ok { version } if version == PROTOCOL_MODERN
    ));
  }

  #[test]
  fn meta_check_rejects_missing_fields() {
    assert!(matches!(check_meta(None), MetaCheck::Malformed(_)));
    assert!(matches!(
      check_meta(Some(&json!({}))),
      MetaCheck::Malformed(_)
    ));
    let no_caps = json!({"_meta": {META_PROTOCOL_VERSION: PROTOCOL_MODERN}});
    assert!(matches!(
      check_meta(Some(&no_caps)),
      MetaCheck::MissingCapabilities
    ));
  }

  #[test]
  fn meta_check_reports_unsupported_versions() {
    let params = json!({"_meta": {
      META_PROTOCOL_VERSION: "1900-01-01",
      META_CLIENT_CAPABILITIES: {},
    }});
    assert!(matches!(
      check_meta(Some(&params)),
      MetaCheck::UnsupportedVersion(v) if v == "1900-01-01"
    ));
  }

  #[test]
  fn unsupported_version_error_uses_the_renumbered_code() {
    let response = unsupported_version_response(Some(&json!(1)), "1900-01-01");
    assert_eq!(response["error"]["code"], json!(-32022));
    assert_eq!(response["error"]["data"]["requested"], json!("1900-01-01"));
    assert!(response["error"]["data"]["supported"]
      .as_array()
      .unwrap()
      .contains(&json!(PROTOCOL_MODERN)));
  }

  #[test]
  fn legacy_negotiation_echoes_known_versions() {
    assert_eq!(negotiate_legacy(Some("2025-06-18")), "2025-06-18");
    assert_eq!(negotiate_legacy(Some("2025-11-25")), "2025-11-25");
    // Unknown or absent: name something we do speak rather than fail, since a
    // legacy client cannot fall forward.
    assert_eq!(
      negotiate_legacy(Some("1999-01-01")),
      PROTOCOL_LEGACY_FALLBACK
    );
    assert_eq!(negotiate_legacy(None), PROTOCOL_LEGACY_FALLBACK);
  }

  #[test]
  fn looks_modern_only_when_protocol_version_present() {
    assert!(!looks_modern(None));
    assert!(!looks_modern(Some(&json!({}))));
    assert!(!looks_modern(Some(&json!({"_meta": {}}))));
    assert!(looks_modern(Some(
      &json!({"_meta": {META_PROTOCOL_VERSION: PROTOCOL_MODERN}})
    )));
  }

  #[test]
  fn tool_result_marks_errors_explicitly() {
    let ok = tool_result_body("done", None, false);
    assert_eq!(ok["isError"], json!(false));
    assert_eq!(ok["content"][0]["type"], json!("text"));

    let failed = tool_result_body("nope", None, true);
    assert_eq!(failed["isError"], json!(true));
  }
}
