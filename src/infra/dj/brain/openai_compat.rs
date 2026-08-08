//! The OpenAI-compatible brain: one adapter, many providers.
//!
//! `POST {base_url}/chat/completions` is spoken by OpenAI, OpenRouter, Ollama,
//! LM Studio, vLLM, and llama.cpp, so the **local model** case is just a
//! `base_url` pointing at `http://localhost:11434/v1` with no API key at all.
//!
//! Structured output is requested via `response_format`, but degraded gracefully:
//! plenty of local servers ignore or reject it, and the reply parser is tolerant
//! enough to read a fenced JSON block out of prose anyway.

use super::{parse_step, step_schema, system_prompt, user_prompt, DjRequest, DjStep};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
/// Ollama's OpenAI-compatible endpoint — the most common local-model setup.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434/v1";
const MAX_TOKENS: u32 = 2048;

/// Which token-limit field this server takes.
///
/// `max_tokens` is what every OpenAI-compatible server has always accepted;
/// OpenAI's newer models require `max_completion_tokens` and reject the old name.
/// Sending the new one everywhere would break the local servers this backend
/// exists for, so it is a downgrade the server asks for, not a default.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenLimit {
  MaxTokens,
  MaxCompletionTokens,
}

impl TokenLimit {
  fn field(self) -> &'static str {
    match self {
      Self::MaxTokens => "max_tokens",
      Self::MaxCompletionTokens => "max_completion_tokens",
    }
  }
}

/// A non-success HTTP reply, kept as a type so the retry can read the status.
///
/// The retry used to search the *rendered* error for "400" or "response_format",
/// which both the base URL (`http://localhost:11400/v1`) and the model's own
/// words could contain — so unrelated failures bought a second, full-price model
/// call. Only this error means "the server refused something we sent".
#[derive(Debug)]
struct RejectedRequest {
  status: reqwest::StatusCode,
  message: String,
}

impl RejectedRequest {
  /// The two statuses an OpenAI-compatible server uses for a field it will not
  /// take. A 401, 404, 429, or 5xx is a real failure and retrying is pure waste.
  fn rejected_a_field(&self) -> bool {
    self.status == reqwest::StatusCode::BAD_REQUEST
      || self.status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
  }

  /// Whether the *server's own* message names this field. Deliberately not the
  /// rendered error chain, which is what made the old guard misfire.
  fn blames(&self, field: &str) -> bool {
    self.message.contains(field)
  }
}

impl std::fmt::Display for RejectedRequest {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}: {}", self.status, self.message)
  }
}

impl std::error::Error for RejectedRequest {}

pub struct OpenAiCompatBrain {
  /// Optional: a local server usually needs none.
  api_key: Option<String>,
  model: String,
  base_url: String,
  http: reqwest::Client,
}

impl OpenAiCompatBrain {
  pub fn new(api_key: Option<String>, model: Option<String>, base_url: Option<String>) -> Self {
    Self {
      api_key: api_key.filter(|key| !key.trim().is_empty()),
      model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
      base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
      http: super::shared_client(),
    }
  }

  /// One turn, downgrading the request only for fields the server actually
  /// refused.
  ///
  /// Both downgrades are one-way and each fires at most once, so a turn costs at
  /// most three requests and cannot loop.
  pub async fn step(&self, request: &DjRequest) -> Result<DjStep> {
    let mut structured = true;
    let mut tokens = TokenLimit::MaxTokens;
    loop {
      let error = match self.attempt(request, structured, tokens).await {
        Ok(reply) => return Ok(reply),
        Err(error) => error,
      };

      // Only the server refusing a field we sent is worth paying for a second
      // model call. A transport error, a 401, or a reply we could not parse
      // would fail exactly the same way twice.
      let Some(rejected) = error.downcast_ref::<RejectedRequest>() else {
        return Err(error);
      };
      if !rejected.rejected_a_field() {
        return Err(error);
      }

      if tokens == TokenLimit::MaxTokens && rejected.blames("max_tokens") {
        // OpenAI's newer models take `max_completion_tokens` and refuse the old
        // name outright, so dropping `response_format` here would fix nothing.
        log::debug!("DJ: server rejected max_tokens, retrying with max_completion_tokens");
        tokens = TokenLimit::MaxCompletionTokens;
      } else if structured {
        // Plenty of local servers reject `response_format`. The prompt alone plus
        // the tolerant parser is enough, so this is not a hard failure.
        log::debug!("DJ: server rejected response_format, retrying without it: {error}");
        structured = false;
      } else {
        return Err(error);
      }
    }
  }

  async fn attempt(
    &self,
    request: &DjRequest,
    structured: bool,
    tokens: TokenLimit,
  ) -> Result<DjStep> {
    let mut body = json!({
      "model": self.model,
      "messages": [
        { "role": "system", "content": system_prompt() },
        { "role": "user", "content": user_prompt(request) },
      ],
    });
    body[tokens.field()] = json!(MAX_TOKENS);
    if structured {
      body["response_format"] = json!({
        "type": "json_schema",
        "json_schema": { "name": "dj_reply", "schema": step_schema() }
      });
    }

    let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
    let mut post = self
      .http
      .post(&url)
      .header("content-type", "application/json");
    if let Some(key) = &self.api_key {
      post = post.header("authorization", format!("Bearer {key}"));
    }

    let response = post
      .json(&body)
      .send()
      .await
      .map_err(|e| anyhow!("could not reach {url}: {e}"))?;

    let status = response.status();
    let payload: Value = response
      .json()
      .await
      .map_err(|e| anyhow!("{url} returned a body we could not read ({status}): {e}"))?;

    if !status.is_success() {
      let message = payload
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("no error message");
      return Err(
        RejectedRequest {
          status,
          message: message.to_string(),
        }
        .into(),
      );
    }

    let text = payload
      .get("choices")
      .and_then(Value::as_array)
      .and_then(|choices| choices.first())
      .and_then(|choice| choice.get("message"))
      .and_then(|message| message.get("content"))
      .and_then(Value::as_str)
      .ok_or_else(|| anyhow!("{url} returned no message content"))?;

    parse_step(text)
  }
}

/// Hand-written to keep the API key out of logs and panic messages — see the
/// equivalent note on `AnthropicBrain`.
impl std::fmt::Debug for OpenAiCompatBrain {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OpenAiCompatBrain")
      .field("model", &self.model)
      .field("base_url", &self.base_url)
      .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
  use tokio::net::TcpListener;

  fn request() -> DjRequest {
    DjRequest {
      want: 2,
      ..DjRequest::default()
    }
  }

  /// Serve `responses` in order, collecting each request body.
  async fn serve(
    responses: Vec<(&'static str, Value)>,
  ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
      let mut bodies = Vec::new();
      for (status, body) in responses {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (read_half, mut write_half) = stream.split();
        let mut reader = BufReader::new(read_half);
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
          reader.read_exact(&mut request_body).await.unwrap();
        }
        bodies.push(String::from_utf8_lossy(&request_body).to_string());

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
      }
      bodies
    });
    (base_url, handle)
  }

  fn completion(text: &str) -> Value {
    json!({"choices": [{"message": {"role": "assistant", "content": text}}]})
  }

  #[tokio::test]
  async fn parses_a_successful_completion() {
    let (base_url, server) = serve(vec![(
      "200 OK",
      completion(r#"{"say":"Local pick.","tracks":[{"title":"Nude","artist":"Radiohead"}]}"#),
    )])
    .await;
    let brain = OpenAiCompatBrain::new(None, None, Some(base_url));
    let reply = brain.step(&request()).await.unwrap();
    assert_eq!(reply.say.as_deref(), Some("Local pick."));
    server.await.unwrap();
  }

  #[tokio::test]
  async fn a_local_server_needs_no_api_key_header() {
    let (base_url, server) =
      serve(vec![("200 OK", completion(r#"{"say":"ok","tracks":[]}"#))]).await;
    let brain = OpenAiCompatBrain::new(None, None, Some(base_url));
    let _ = brain.step(&request()).await;
    // The request went out and was accepted without any credential.
    assert_eq!(server.await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn a_blank_api_key_is_treated_as_absent() {
    let brain = OpenAiCompatBrain::new(Some("   ".into()), None, None);
    assert!(brain.api_key.is_none());
  }

  #[tokio::test]
  async fn retries_without_response_format_when_the_server_rejects_it() {
    // Many local servers 400 on `response_format`; the prompt alone plus the
    // tolerant parser is enough, so this must not be a hard failure.
    let (base_url, server) = serve(vec![
      (
        "400 Bad Request",
        json!({"error": {"message": "unknown field response_format"}}),
      ),
      (
        "200 OK",
        completion(r#"{"say":"second try","tracks":[{"title":"A","artist":"B"}]}"#),
      ),
    ])
    .await;
    let brain = OpenAiCompatBrain::new(None, None, Some(base_url));
    let reply = brain.step(&request()).await.unwrap();
    assert_eq!(reply.say.as_deref(), Some("second try"));

    let bodies = server.await.unwrap();
    assert_eq!(bodies.len(), 2, "should have retried exactly once");
    let first: Value = serde_json::from_str(&bodies[0]).unwrap();
    let second: Value = serde_json::from_str(&bodies[1]).unwrap();
    assert!(first.get("response_format").is_some());
    assert!(second.get("response_format").is_none());
  }

  #[tokio::test]
  async fn a_rejected_max_tokens_is_retried_with_max_completion_tokens() {
    // OpenAI's newer models refuse `max_tokens` outright. Dropping
    // `response_format` instead would fail the same way a second time.
    let (base_url, server) = serve(vec![
      (
        "400 Bad Request",
        json!({"error": {"message": "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."}}),
      ),
      (
        "200 OK",
        completion(r#"{"say":"newer model","tool_calls":[]}"#),
      ),
    ])
    .await;
    let brain = OpenAiCompatBrain::new(None, None, Some(base_url));
    let reply = brain.step(&request()).await.unwrap();
    assert_eq!(reply.say.as_deref(), Some("newer model"));

    let bodies = server.await.unwrap();
    assert_eq!(bodies.len(), 2, "should have retried exactly once");
    let second: Value = serde_json::from_str(&bodies[1]).unwrap();
    assert!(second.get("max_completion_tokens").is_some());
    assert!(second.get("max_tokens").is_none());
    assert!(
      second.get("response_format").is_some(),
      "the token field was the complaint, so the schema request survives"
    );
  }

  #[tokio::test]
  async fn a_reply_that_merely_mentions_400_is_not_retried() {
    // The retry reads the HTTP status, not the rendered error. This reply parses
    // to an error whose text contains both old triggers, and it must still cost
    // exactly one model call.
    let (base_url, server) = serve(vec![(
      "200 OK",
      completion("sorry, I hit error 400 on response_format"),
    )])
    .await;
    let brain = OpenAiCompatBrain::new(None, None, Some(base_url));
    assert!(brain.step(&request()).await.is_err());
    assert_eq!(
      server.await.unwrap().len(),
      1,
      "an unparseable reply is not a rejected field"
    );
  }

  #[tokio::test]
  async fn an_unrelated_error_is_not_retried() {
    let (base_url, server) = serve(vec![(
      "401 Unauthorized",
      json!({"error": {"message": "bad token"}}),
    )])
    .await;
    let brain = OpenAiCompatBrain::new(Some("nope".into()), None, Some(base_url));
    let err = brain.step(&request()).await.unwrap_err().to_string();
    assert!(err.contains("bad token"), "{err}");
    assert_eq!(server.await.unwrap().len(), 1, "must not retry a 401");
  }

  #[test]
  fn defaults_point_at_ollama() {
    let brain = OpenAiCompatBrain::new(None, None, None);
    assert_eq!(brain.base_url, DEFAULT_BASE_URL);
    assert!(brain.base_url.contains("11434"));
  }
}
