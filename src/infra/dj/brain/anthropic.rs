//! The Anthropic Messages API brain.
//!
//! Raw HTTP rather than an SDK, because there is no official Anthropic SDK for
//! Rust — which is the documented approach for an unsupported language.
//!
//! Two model-behaviour constraints are baked in and should not be "fixed" later:
//!
//! * **No sampling parameters.** Current models reject `temperature` / `top_p` /
//!   `top_k` with a 400, so variety cannot come from a temperature knob. It comes
//!   from rotating the history window and from asking for more candidates than
//!   are queued.
//! * **`stop_reason == "refusal"` must be checked before reading `content`**, or
//!   a refused request panics on an empty array.
//!
//! Non-streaming on purpose: a DJ turn is one sentence plus a track list, well
//! inside the client timeout, and streaming would buy nothing but machinery.

use super::{parse_step, step_schema, system_prompt, user_prompt, DjRequest, DjStep};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "claude-haiku-4-5";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 2048;

pub struct AnthropicBrain {
  api_key: String,
  model: String,
  /// A field rather than a constant so the HTTP layer is testable against a local
  /// listener. `friends.rs` hardcodes its URLs and consequently has no tests;
  /// `requests.rs` threads a base URL and does.
  base_url: String,
  http: reqwest::Client,
}

impl AnthropicBrain {
  pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Result<Self> {
    if api_key.trim().is_empty() {
      return Err(anyhow!(
        "no API key for the Anthropic DJ backend. Set SPOTATUI_DJ_API_KEY, or \
         behavior.dj_api_key in config.yml"
      ));
    }
    Ok(Self {
      api_key,
      model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
      base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
      // Shared, and always with an explicit timeout — see `super::shared_client`.
      http: super::shared_client(),
    })
  }

  pub async fn step(&self, request: &DjRequest) -> Result<DjStep> {
    let body = json!({
      "model": self.model,
      "max_tokens": MAX_TOKENS,
      "system": system_prompt(),
      "messages": [{ "role": "user", "content": user_prompt(request) }],
      // Constrain the shape at the API level instead of asking nicely for JSON.
      "output_config": { "format": { "type": "json_schema", "schema": step_schema() } },
    });

    let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
    let response = self
      .http
      .post(&url)
      .header("x-api-key", &self.api_key)
      .header("anthropic-version", API_VERSION)
      .header("content-type", "application/json")
      .json(&body)
      .send()
      .await
      .map_err(|e| anyhow!("could not reach the Anthropic API: {e}"))?;

    let status = response.status();
    let payload: Value = response
      .json()
      .await
      .map_err(|e| anyhow!("Anthropic API returned a body we could not read ({status}): {e}"))?;

    if !status.is_success() {
      let message = payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("no error message");
      return Err(anyhow!("Anthropic API error {status}: {message}"));
    }

    // Checked before touching `content`: a refusal comes back as HTTP 200 with an
    // empty content array, so indexing first would panic.
    if payload.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
      let category = payload
        .get("stop_details")
        .and_then(|details| details.get("category"))
        .and_then(Value::as_str)
        .unwrap_or("unspecified");
      return Err(anyhow!(
        "the model declined this request (category: {category})"
      ));
    }

    let text = first_text_block(&payload)
      .ok_or_else(|| anyhow!("Anthropic API returned no text content"))?;
    parse_step(&text)
  }
}

/// Hand-written so the API key can never reach a log line or panic message.
///
/// A `#[derive(Debug)]` here would print the key verbatim anywhere the struct is
/// formatted, which is exactly the accident this avoids.
impl std::fmt::Debug for AnthropicBrain {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AnthropicBrain")
      .field("model", &self.model)
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

/// Concatenate the `text` blocks of a Messages API response.
///
/// A response may lead with `thinking` blocks, so filtering by type is required
/// rather than reading `content[0]`.
fn first_text_block(payload: &Value) -> Option<String> {
  let blocks = payload.get("content")?.as_array()?;
  let text = blocks
    .iter()
    .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
    .filter_map(|block| block.get("text").and_then(Value::as_str))
    .collect::<Vec<_>>()
    .join("\n");
  (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
  use tokio::net::TcpListener;

  fn request() -> DjRequest {
    DjRequest {
      want: 2,
      ..DjRequest::default()
    }
  }

  /// Minimal one-shot HTTP server, mirroring `network::requests`' test pattern
  /// (there is no mockito/wiremock in this repo). Returns the request body.
  async fn serve_once(status: &str, body: Value) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let status = status.to_string();
    let handle = tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let (read_half, mut write_half) = stream.split();
      let mut reader = BufReader::new(read_half);

      // Read headers, note the declared length, then read exactly that body.
      let mut content_length = 0usize;
      loop {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
          content_length = value.trim().parse().unwrap_or(0);
        }
        if line == "\r\n" || line.is_empty() {
          break;
        }
      }
      let mut request_body = vec![0u8; content_length];
      if content_length > 0 {
        tokio::io::AsyncReadExt::read_exact(&mut reader, &mut request_body)
          .await
          .unwrap();
      }

      let payload = body.to_string();
      write_half
        .write_all(
          format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
            payload.len()
          )
          .as_bytes(),
        )
        .await
        .unwrap();
      write_half.flush().await.unwrap();
      String::from_utf8_lossy(&request_body).to_string()
    });
    (base_url, handle)
  }

  fn brain(base_url: String) -> AnthropicBrain {
    AnthropicBrain::new("test-key".into(), None, Some(base_url)).unwrap()
  }

  #[test]
  fn a_blank_key_is_rejected_with_the_env_var_named() {
    let err = AnthropicBrain::new("   ".into(), None, None)
      .unwrap_err()
      .to_string();
    assert!(err.contains("SPOTATUI_DJ_API_KEY"));
  }

  #[tokio::test]
  async fn parses_a_successful_reply() {
    let (base_url, server) = serve_once(
      "200 OK",
      json!({
        "stop_reason": "end_turn",
        "content": [{"type": "text", "text": r#"{"say":"Mellow.","tool_calls":[{"name":"queue_tracks","arguments":{"tracks":[{"title":"Nude","artist":"Radiohead"}]}}]}"#}]
      }),
    )
    .await;
    let reply = brain(base_url).step(&request()).await.unwrap();
    assert_eq!(reply.say.as_deref(), Some("Mellow."));
    assert_eq!(reply.calls[0].name, "queue_tracks");
    server.await.unwrap();
  }

  #[tokio::test]
  async fn sends_no_sampling_parameters() {
    // Current Anthropic models reject temperature/top_p/top_k with a 400, so
    // this asserts we never send them.
    let (base_url, server) = serve_once(
      "200 OK",
      json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": r#"{"say":"ok","tracks":[]}"#}]}),
    )
    .await;
    let _ = brain(base_url).step(&request()).await;
    let sent = server.await.unwrap();
    let body: Value = serde_json::from_str(&sent).unwrap();
    assert!(body.get("temperature").is_none());
    assert!(body.get("top_p").is_none());
    assert!(body.get("top_k").is_none());
    // And it does constrain the output shape at the API level.
    assert_eq!(
      body["output_config"]["format"]["type"],
      json!("json_schema")
    );
    assert_eq!(body["model"], json!(DEFAULT_MODEL));
  }

  #[tokio::test]
  async fn a_refusal_is_reported_rather_than_panicking_on_empty_content() {
    // Regression guard: a refusal is HTTP 200 with `content: []`, so any code
    // that reads content[0] first would panic here.
    let (base_url, server) = serve_once(
      "200 OK",
      json!({"stop_reason": "refusal", "stop_details": {"category": "cyber"}, "content": []}),
    )
    .await;
    let err = brain(base_url)
      .step(&request())
      .await
      .unwrap_err()
      .to_string();
    assert!(err.contains("declined"), "{err}");
    assert!(err.contains("cyber"), "{err}");
    server.await.unwrap();
  }

  #[tokio::test]
  async fn an_api_error_surfaces_the_servers_message() {
    let (base_url, server) = serve_once(
      "401 Unauthorized",
      json!({"error": {"message": "invalid x-api-key"}}),
    )
    .await;
    let err = brain(base_url)
      .step(&request())
      .await
      .unwrap_err()
      .to_string();
    assert!(err.contains("invalid x-api-key"), "{err}");
    server.await.unwrap();
  }

  #[tokio::test]
  async fn thinking_blocks_before_the_text_are_skipped() {
    let (base_url, server) = serve_once(
      "200 OK",
      json!({
        "stop_reason": "end_turn",
        "content": [
          {"type": "thinking", "thinking": ""},
          {"type": "text", "text": r#"{"say":"after thinking","tracks":[{"title":"A","artist":"B"}]}"#}
        ]
      }),
    )
    .await;
    let reply = brain(base_url).step(&request()).await.unwrap();
    assert_eq!(reply.say.as_deref(), Some("after thinking"));
    server.await.unwrap();
  }

  #[tokio::test]
  async fn a_response_with_no_text_content_is_an_error() {
    let (base_url, server) = serve_once(
      "200 OK",
      json!({"stop_reason": "end_turn", "content": [{"type": "thinking", "thinking": ""}]}),
    )
    .await;
    let err = brain(base_url)
      .step(&request())
      .await
      .unwrap_err()
      .to_string();
    assert!(err.contains("no text content"), "{err}");
    server.await.unwrap();
  }
}
