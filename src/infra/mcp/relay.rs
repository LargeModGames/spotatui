//! `spotatui mcp`: the stdio MCP server an agent spawns.
//!
//! Mostly a **line pump** — the real server lives in the TUI process, where
//! `App` is, and both hops speak the same newline-delimited JSON-RPC framing so
//! nothing has to be translated.
//!
//! Which end serves a given line is decided per line, not once at startup. The
//! agent spawns this process when its session opens, which is routinely before
//! the user has started spotatui, and the user may restart spotatui while the
//! session is still going. So each line connects if it can and is forwarded, and
//! falls back to answering here with an actionable `isError` when it cannot.
//! Deciding once meant the first of those orderings stranded every tool for the
//! rest of the session, behind an error telling the user to start the player
//! they had just started.

use super::control::{self, Handshake};
use super::executor::OfflineExecutor;
use super::protocol as proto;
use super::server;
use anyhow::Result;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// How long a forwarded request may go unanswered before the relay gives up on
/// the socket.
///
/// Deliberately generous rather than snappy: `queue_tracks(exclude_owned)`
/// crawls the whole playlist library inline before it answers, and the serial
/// lane it runs on is head-of-line blocking, so a short deadline would turn a
/// slow-but-working call into a false failure. It exists only to bound the
/// unbounded case — a stopped process or a half-open connection, where nothing
/// is ever coming and the agent's whole MCP session would otherwise hang.
const UPSTREAM_RESPONSE_TIMEOUT: Duration = Duration::from_secs(300);
/// The handshake replay is answered from memory the moment it lands, so it gets
/// its own, much shorter deadline: waiting five minutes to discover a socket is
/// dead would strand every line behind it.
const HANDSHAKE_REPLAY_TIMEOUT: Duration = Duration::from_secs(15);

/// Entry point for the `mcp` subcommand.
///
/// Never returns an error for "no player running" — that is a normal state and
/// is reported through the protocol instead.
pub async fn run() -> Result<()> {
  let mut relay = Relay::default();
  let mut from_agent = BufReader::new(tokio::io::stdin()).lines();
  let mut stdout = tokio::io::stdout();

  // Per the spec, exit when stdin reaches EOF: the primary graceful-shutdown
  // signal for stdio, and the only portable one.
  while let Some(line) = from_agent.next_line().await? {
    if line.trim().is_empty() {
      continue;
    }
    if let Some(response) = relay.handle(&line).await {
      stdout.write_all(response.as_bytes()).await?;
      stdout.write_all(b"\n").await?;
      stdout.flush().await?;
    }
  }
  Ok(())
}

/// The live half of the relay: a socket to the TUI, plus what it takes to
/// rebuild that socket.
struct Upstream {
  reader: tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
  writer: tokio::net::tcp::OwnedWriteHalf,
  /// Whether a line has come back from this socket that proves the token was
  /// accepted.
  ///
  /// The control channel answers a *good* token with silence and a bad one with
  /// one non-MCP line, so nothing distinguishes the two until the socket first
  /// speaks. Checked once rather than on every response: one well-formed line
  /// settles it for the life of the connection.
  token_checked: bool,
}

/// The control channel's answer to a bad token, which is one line followed by a
/// hang-up. It is not MCP traffic and must never reach the agent as the response
/// to its request; a JSON-RPC error response is distinguishable because its
/// `error` is an object, not a bare string.
fn control_rejection(line: &str) -> Option<String> {
  match serde_json::from_str::<serde_json::Value>(line)
    .ok()?
    .get("error")?
    .as_str()?
  {
    "unauthorized" => Some(
      "spotatui rejected the control token; the control file is stale, so restart spotatui"
        .to_string(),
    ),
    _ => None,
  }
}

/// Why a forward failed, which is what decides whether it may be sent again.
///
/// The distinction is the whole point: MCP tool calls are not idempotent, so
/// "retry on any error" turns one dropped socket into a batch queued twice.
enum ForwardError {
  /// The line never reached spotatui. Replaying it cannot duplicate anything.
  Unsent(anyhow::Error),
  /// spotatui had the line when the connection dropped. Whether it acted on it
  /// is unknowable from here, so it must not be replayed.
  Sent(anyhow::Error),
}

/// What to tell the agent about a request the relay could not complete.
///
/// A delivered-then-lost request has to say so: MCP tool calls are not
/// idempotent, and an agent that reads a bare "lost the connection" will retry
/// the batch that may already have been queued.
fn offline_reason(error: &ForwardError) -> String {
  match error {
    ForwardError::Unsent(e) => format!("lost the connection to spotatui ({e})"),
    ForwardError::Sent(e) => format!(
      "lost the connection to spotatui after the request was sent ({e}); it may or may not have \
       been applied, so check the current state before retrying"
    ),
  }
}

impl std::fmt::Display for ForwardError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ForwardError::Unsent(e) | ForwardError::Sent(e) => write!(f, "{e}"),
    }
  }
}

/// How the relay finds the running TUI. Injected rather than hardcoded so tests
/// can point at a fake listener without writing to the user's config directory
/// or mutating the process-wide handshake-path variable — which is shared, and
/// so would make the tests order-dependent on each other.
type Locate = Box<dyn Fn() -> Result<Handshake> + Send>;

/// Relays each line to the TUI, connecting on demand.
///
/// The connection is established per line rather than once at startup, because
/// the two processes have independent lifetimes and neither order is unusual:
/// an agent typically spawns this server when its session opens, which may be
/// long before the user starts spotatui — and the user may restart spotatui
/// halfway through a session. Deciding once meant either state stranded the
/// tools for the rest of the session with an error telling the user to do the
/// thing they had just done.
struct Relay {
  upstream: Option<Upstream>,
  locate: Locate,
  /// The client's `initialize` line, replayed on every new connection.
  ///
  /// A legacy client handshakes once and its later requests carry no `_meta`.
  /// The TUI latches the era per connection, so a reconnect that skipped the
  /// replay would reject those requests for missing the fields the handshake
  /// exists to make unnecessary.
  handshake: Option<String>,
  /// Why the last connection attempt failed, quoted to the agent verbatim.
  offline_reason: String,
  /// Only used while offline; upstream answers carry the TUI's own session.
  session: server::Session,
  /// A field rather than the constant so the deadline is testable without
  /// waiting it out or manipulating the clock.
  response_timeout: Duration,
}

impl Default for Relay {
  fn default() -> Self {
    Self {
      upstream: None,
      locate: Box::new(control::read_handshake),
      handshake: None,
      offline_reason: String::new(),
      session: server::Session::default(),
      response_timeout: UPSTREAM_RESPONSE_TIMEOUT,
    }
  }
}

impl Relay {
  /// Handle one line from the agent, returning the line to write back.
  async fn handle(&mut self, line: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(line).ok();
    // Mirrors the server's own rule for what it will answer, because guessing
    // wrong desynchronises the stream for good: a reply we never read is left in
    // the socket, and every later request then returns the previous one's
    // response, under the wrong id. Only a well-formed notification — an object
    // with a method and no id — goes unanswered. Everything else, including a
    // parse failure or a JSON value that is not an object at all, comes back as
    // an error response.
    let expects_response = match &parsed {
      Some(serde_json::Value::Object(message)) => {
        message.get("id").is_some_and(|id| !id.is_null())
          || message
            .get("method")
            .and_then(serde_json::Value::as_str)
            .is_none()
      }
      _ => true,
    };
    let is_initialize = parsed
      .as_ref()
      .and_then(|message| message.get("method"))
      .and_then(serde_json::Value::as_str)
      == Some("initialize");

    let response = self.dispatch(line, expects_response).await;

    // Remembered *after* dispatch, never before: on the connection this line is
    // forwarded over, the handshake is this very line, and replaying it as well
    // would hand the TUI two initializes and leave a reply nobody reads.
    if is_initialize {
      self.handshake = Some(line.to_string());
    }
    response
  }

  async fn dispatch(&mut self, line: &str, expects_response: bool) -> Option<String> {
    if self.ensure_upstream().await {
      match self.forward(line, expects_response).await {
        Ok(response) => return response,
        Err(ForwardError::Unsent(e)) => {
          // The TUI went away before it heard us. Rebuild once and retry before
          // falling back, so a restart costs one line rather than the session.
          // Safe to replay precisely because the line never landed.
          log::debug!("MCP relay: upstream failed before the send ({e}); reconnecting");
          self.upstream = None;
          if self.ensure_upstream().await {
            match self.forward(line, expects_response).await {
              Ok(response) => return response,
              // The retry is where the line can *become* delivered, so its own
              // variant decides the wording. Reporting a plain "lost the
              // connection" here is what would send the agent back to retry a
              // batch that had in fact been queued.
              Err(retry_error) => {
                self.upstream = None;
                self.offline_reason = offline_reason(&retry_error);
                return self.answer_offline(line).await;
              }
            }
          }
          // Left as `ensure_upstream` set it. The reconnect is the step that
          // failed, and its reason ("is spotatui running with
          // behavior.mcp_enabled set?") is the one the user can act on;
          // overwriting it with the earlier send error hands the agent the
          // staler half of the story.
          log::debug!("MCP relay: reconnect failed after {e}");
        }
        Err(sent @ ForwardError::Sent(_)) => {
          // Deliberately *not* retried. spotatui had the request when the socket
          // died, so it may have acted on it; re-forwarding `queue_tracks` would
          // queue the same batch twice. An honest "may or may not have applied"
          // is recoverable by an agent, a silent duplicate is not.
          log::debug!("MCP relay: upstream failed after the send ({sent}); not replaying");
          self.upstream = None;
          self.offline_reason = offline_reason(&sent);
        }
      }
    }

    self.answer_offline(line).await
  }

  /// Connect if not already connected, replaying the handshake on a new socket.
  async fn ensure_upstream(&mut self) -> bool {
    if self.upstream.is_some() {
      return true;
    }
    let stream = match connect(&self.locate).await {
      Ok(stream) => stream,
      Err(reason) => {
        self.offline_reason = reason;
        return false;
      }
    };
    let (read_half, writer) = stream.into_split();
    let mut upstream = Upstream {
      reader: BufReader::new(read_half).lines(),
      writer,
      token_checked: false,
    };
    if let Some(handshake) = self.handshake.clone() {
      // The reply is the TUI's answer to a handshake the agent already had
      // answered, so it is consumed here rather than forwarded.
      if let Err(e) = send_line(&mut upstream, &handshake).await {
        self.offline_reason = format!("could not replay the handshake to spotatui ({e})");
        return false;
      }
      // Deadlined like every other upstream read: a socket that accepts the
      // replay and never answers would otherwise block here forever, and this
      // runs before any of the agent's own lines get a chance.
      let replay =
        tokio::time::timeout(HANDSHAKE_REPLAY_TIMEOUT, upstream.reader.next_line()).await;
      match replay {
        Ok(Ok(Some(reply))) => {
          // The rejection line arrives exactly here on a stale control file, and
          // taking it for a handshake answer would mark a socket the server has
          // already hung up on as healthy.
          if let Some(reason) = control_rejection(&reply) {
            self.offline_reason = reason;
            return false;
          }
          upstream.token_checked = true;
        }
        Ok(_) => {
          self.offline_reason = "spotatui closed the connection during the handshake".to_string();
          return false;
        }
        Err(_) => {
          self.offline_reason =
            "spotatui did not answer the handshake in time; it may be stopped or wedged"
              .to_string();
          return false;
        }
      }
    }
    self.upstream = Some(upstream);
    true
  }

  /// Write one line upstream and read its answer.
  async fn forward(
    &mut self,
    line: &str,
    expects_response: bool,
  ) -> std::result::Result<Option<String>, ForwardError> {
    let response_timeout = self.response_timeout;
    let upstream = self
      .upstream
      .as_mut()
      .ok_or_else(|| ForwardError::Unsent(anyhow::anyhow!("not connected")))?;
    send_line(upstream, line)
      .await
      .map_err(ForwardError::Unsent)?;
    if !expects_response {
      return Ok(None);
    }
    // Past this point the request is in spotatui's hands, so every failure is
    // `Sent`: it may already have queued the tracks. That includes the deadline
    // expiring, which is why it maps to the same variant — and why the socket is
    // then dropped rather than reused, since a late answer arriving on it would
    // be read as the *next* request's response and desynchronise the stream for
    // the rest of the session.
    let read = tokio::time::timeout(response_timeout, upstream.reader.next_line()).await;
    match read {
      Ok(Ok(Some(response))) => {
        // With no handshake to replay, this is the connection's first line, so
        // it is where a rejected token surfaces. `Unsent`, not `Sent`: the
        // control layer refuses before the MCP server ever reads the request, so
        // nothing was applied and a replay cannot duplicate anything.
        if !upstream.token_checked {
          if let Some(reason) = control_rejection(&response) {
            return Err(ForwardError::Unsent(anyhow::anyhow!("{reason}")));
          }
          upstream.token_checked = true;
        }
        Ok(Some(response))
      }
      Ok(Ok(None)) => Err(ForwardError::Sent(anyhow::anyhow!(
        "spotatui closed the connection"
      ))),
      Ok(Err(e)) => Err(ForwardError::Sent(e.into())),
      Err(_) => Err(ForwardError::Sent(anyhow::anyhow!(
        "spotatui did not answer within {}s",
        response_timeout.as_secs()
      ))),
    }
  }

  /// Serve the protocol ourselves, reporting that the player is not reachable.
  async fn answer_offline(&mut self, line: &str) -> Option<String> {
    let executor = OfflineExecutor::new(self.offline_reason.clone());
    // A handshake forwarded upstream never reached this session, so its era is
    // still unknown while the client believes it has negotiated one. A legacy
    // client's later requests carry no `_meta`, and would be rejected here for
    // exactly the fields the handshake exists to make unnecessary — turning the
    // player going away into a protocol error rather than the actionable
    // "spotatui is not available" this path exists to give. Replay it first.
    if self.session.era == proto::Era::Unknown {
      if let Some(handshake) = self.handshake.clone() {
        let _ = server::handle_line(&mut self.session, &handshake, &executor).await;
      }
    }
    server::handle_line(&mut self.session, line, &executor)
      .await
      .map(|response| response.to_string())
  }
}

async fn send_line(upstream: &mut Upstream, line: &str) -> Result<()> {
  upstream.writer.write_all(line.as_bytes()).await?;
  upstream.writer.write_all(b"\n").await?;
  upstream.writer.flush().await?;
  Ok(())
}

/// Locate and authenticate to the running TUI.
///
/// The `Err` payload is a human-readable reason, surfaced verbatim to the agent.
async fn connect(locate: &Locate) -> std::result::Result<TcpStream, String> {
  let handshake: Handshake = locate().map_err(|e| {
    // The common cases are "not running" and "mcp_enabled is off"; both look
    // identical from here, so say so rather than guessing.
    format!("{e} (is spotatui running with behavior.mcp_enabled set?)")
  })?;

  let mut stream = TcpStream::connect(("127.0.0.1", handshake.port))
    .await
    .map_err(|e| {
      format!(
        "could not connect to spotatui on 127.0.0.1:{} ({e}); the control file may be stale",
        handshake.port
      )
    })?;

  let hello = serde_json::json!({ "token": handshake.token });
  stream
    .write_all(format!("{hello}\n").as_bytes())
    .await
    .map_err(|e| format!("could not send the control handshake: {e}"))?;

  Ok(stream)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::mcp::protocol as proto;
  use serde_json::json;
  use std::sync::{Arc, Mutex as StdMutex};
  use tokio::net::TcpListener;

  #[tokio::test]
  async fn offline_mode_still_serves_a_valid_protocol() {
    // The important property: an agent that spawns us with no player running
    // gets a working MCP server, not a crash.
    let executor = OfflineExecutor::new("no control file");
    let input = format!(
      "{}\n{}\n{}\n",
      json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": {
        proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
        proto::META_CLIENT_CAPABILITIES: {},
      }}}),
      json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"_meta": {
        proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
        proto::META_CLIENT_CAPABILITIES: {},
      }}}),
      json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "_meta": {
          proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
          proto::META_CLIENT_CAPABILITIES: {},
        },
        "name": "get_now_playing",
        "arguments": {}
      }}),
    );
    let mut output = Vec::new();
    server::serve(input.as_bytes(), &mut output, &executor)
      .await
      .unwrap();

    let text = String::from_utf8(output).unwrap();
    let responses: Vec<serde_json::Value> = text
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    assert_eq!(responses.len(), 3);
    // discover and tools/list work fine offline — only execution fails.
    assert!(responses[0]["result"]["supportedVersions"].is_array());
    assert!(responses[1]["result"]["tools"].is_array());
    assert_eq!(responses[2]["result"]["isError"], json!(true));
    assert!(responses[2]["result"]["content"][0]["text"]
      .as_str()
      .unwrap()
      .contains("not available"));
  }

  #[tokio::test]
  async fn offline_mode_also_works_for_a_legacy_client() {
    // Claude Code / Codex as shipped today open with `initialize`; offline mode
    // must not be modern-only or it will look broken to them.
    let executor = OfflineExecutor::new("no control file");
    let input = format!(
      "{}\n{}\n",
      json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2025-06-18", "capabilities": {}}}),
      json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    let mut output = Vec::new();
    server::serve(input.as_bytes(), &mut output, &executor)
      .await
      .unwrap();
    let responses: Vec<serde_json::Value> = String::from_utf8(output)
      .unwrap()
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    assert_eq!(
      responses[0]["result"]["protocolVersion"],
      json!("2025-06-18")
    );
    assert!(responses[1]["result"]["tools"].is_array());
  }

  /// A stand-in TUI: authenticates like the real listener, echoes every request
  /// back, and keeps accepting new connections so a reconnect can be observed.
  ///
  /// Lines land in the returned buffer, shared rather than returned by the task
  /// because the task never finishes on its own.
  fn fake_tui(listener: TcpListener, token: String) -> Arc<StdMutex<Vec<String>>> {
    let seen = Arc::new(StdMutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    tokio::spawn(async move {
      loop {
        let Ok((stream, _)) = listener.accept().await else {
          break;
        };
        let sink = Arc::clone(&sink);
        let token = token.clone();
        tokio::spawn(async move {
          let (read_half, mut write_half) = stream.into_split();
          let mut lines = BufReader::new(read_half).lines();
          let Ok(Some(first)) = lines.next_line().await else {
            return;
          };
          assert_eq!(
            serde_json::from_str::<serde_json::Value>(&first).unwrap()["token"],
            json!(token),
            "the relay must present the token before any MCP traffic"
          );
          while let Ok(Some(line)) = lines.next_line().await {
            let id = serde_json::from_str::<serde_json::Value>(&line)
              .ok()
              .and_then(|message| message.get("id").cloned())
              .unwrap_or(serde_json::Value::Null);
            sink.lock().unwrap().push(line);
            let response = json!({"jsonrpc": "2.0", "id": id, "result": {"echoed": true}});
            if write_half
              .write_all(format!("{response}\n").as_bytes())
              .await
              .is_err()
            {
              return;
            }
          }
        });
      }
    });
    seen
  }

  /// A stand-in TUI that takes the request and then dies without answering —
  /// the one failure the relay must not paper over by sending it again.
  ///
  /// Keeps accepting, so a replay onto a fresh connection would show up in the
  /// buffer rather than simply failing to connect.
  fn fake_tui_that_dies_after_reading(
    listener: TcpListener,
    token: String,
  ) -> Arc<StdMutex<Vec<String>>> {
    let seen = Arc::new(StdMutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    tokio::spawn(async move {
      loop {
        let Ok((stream, _)) = listener.accept().await else {
          break;
        };
        let sink = Arc::clone(&sink);
        let token = token.clone();
        tokio::spawn(async move {
          let (read_half, _write_half) = stream.into_split();
          let mut lines = BufReader::new(read_half).lines();
          let Ok(Some(first)) = lines.next_line().await else {
            return;
          };
          assert_eq!(
            serde_json::from_str::<serde_json::Value>(&first).unwrap()["token"],
            json!(token)
          );
          if let Ok(Some(line)) = lines.next_line().await {
            sink.lock().unwrap().push(line);
          }
          // Drop both halves without writing a response.
        });
      }
    });
    seen
  }

  /// A stand-in TUI that reads the request and then simply never answers, the
  /// way a stopped process or a half-open connection does. Holds the socket open
  /// rather than dropping it, so only a deadline can end the wait.
  fn fake_tui_that_never_answers(listener: TcpListener, token: String) {
    tokio::spawn(async move {
      loop {
        let Ok((stream, _)) = listener.accept().await else {
          break;
        };
        let token = token.clone();
        tokio::spawn(async move {
          let (read_half, _write_half) = stream.into_split();
          let mut lines = BufReader::new(read_half).lines();
          let Ok(Some(first)) = lines.next_line().await else {
            return;
          };
          assert_eq!(
            serde_json::from_str::<serde_json::Value>(&first).unwrap()["token"],
            json!(token)
          );
          // Read everything, answer nothing, and keep both halves alive.
          while let Ok(Some(_)) = lines.next_line().await {}
          std::future::pending::<()>().await;
        });
      }
    });
  }

  /// A stand-in TUI whose token never matches: it answers with the control
  /// channel's own rejection line and hangs up, exactly as `serve_connection`
  /// does.
  fn fake_tui_that_rejects_the_token(listener: TcpListener) {
    tokio::spawn(async move {
      loop {
        let Ok((stream, _)) = listener.accept().await else {
          break;
        };
        tokio::spawn(async move {
          let (read_half, mut write_half) = stream.into_split();
          let mut lines = BufReader::new(read_half).lines();
          if lines.next_line().await.is_err() {
            return;
          }
          let _ = write_half
            .write_all(b"{\"error\":\"unauthorized\"}\n")
            .await;
        });
      }
    });
  }

  /// A relay pointed at `port`, with no dependence on any shared path or env.
  fn relay_for(port: u16, token: &str) -> Relay {
    let token = token.to_string();
    Relay {
      locate: Box::new(move || {
        Ok(Handshake {
          port,
          token: token.clone(),
          pid: std::process::id(),
        })
      }),
      ..Relay::default()
    }
  }

  /// A relay that can never find a player.
  fn relay_with_no_player() -> Relay {
    Relay {
      locate: Box::new(|| anyhow::bail!("no MCP control file")),
      ..Relay::default()
    }
  }

  async fn wait_for_lines(seen: &Arc<StdMutex<Vec<String>>>, count: usize) -> Vec<String> {
    for _ in 0..200 {
      let lines = seen.lock().unwrap().clone();
      if lines.len() >= count {
        return lines;
      }
      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    seen.lock().unwrap().clone()
  }

  fn modern_call() -> String {
    json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
      "_meta": {
        proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
        proto::META_CLIENT_CAPABILITIES: {},
      },
      "name": "get_now_playing", "arguments": {}
    }})
    .to_string()
  }

  #[tokio::test]
  async fn a_relay_that_started_before_the_player_connects_once_it_appears() {
    // The regression this exists for: the relay used to decide pump-vs-offline
    // once at startup, so an agent that spawned it before the user opened
    // spotatui got "not available" for the rest of the session, from an error
    // telling the user to do the thing they had already done.
    let mut relay = relay_with_no_player();
    let offline: serde_json::Value =
      serde_json::from_str(&relay.handle(&modern_call()).await.unwrap()).unwrap();
    assert_eq!(offline["result"]["isError"], json!(true));

    // The player starts. The very next call must reach it, with no restart.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = fake_tui(listener, "tok".to_string());
    relay.locate = Box::new(move || {
      Ok(Handshake {
        port,
        token: "tok".to_string(),
        pid: 1,
      })
    });

    let online: serde_json::Value =
      serde_json::from_str(&relay.handle(&modern_call()).await.unwrap()).unwrap();
    assert_eq!(
      online["result"]["echoed"],
      json!(true),
      "the call must be served by the player, not answered offline"
    );
    assert_eq!(wait_for_lines(&seen, 1).await.len(), 1);
  }

  #[tokio::test]
  async fn the_handshake_is_replayed_after_a_reconnect_but_not_before() {
    // A legacy client handshakes once; its later requests carry no `_meta`, so
    // a rebuilt connection that skipped the replay would reject them. Replaying
    // on the *first* connection would instead send initialize twice.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = fake_tui(listener, "tok".to_string());
    let mut relay = relay_for(port, "tok");

    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                      "params": {"protocolVersion": "2025-06-18"}})
    .to_string();
    relay.handle(&init).await.unwrap();
    assert_eq!(
      wait_for_lines(&seen, 1).await.len(),
      1,
      "the first connection must carry initialize exactly once"
    );

    // Drop the socket, as a spotatui restart would.
    relay.upstream = None;
    let listed = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}).to_string();
    relay.handle(&listed).await.unwrap();

    let lines = wait_for_lines(&seen, 3).await;
    assert_eq!(lines.len(), 3, "got {lines:?}");
    assert!(lines[0].contains("initialize"));
    assert!(
      lines[1].contains("initialize"),
      "the handshake must be replayed before the forwarded call: {lines:?}"
    );
    assert!(lines[2].contains("tools/list"));
  }

  #[tokio::test]
  async fn a_request_lost_after_delivery_is_reported_rather_than_sent_twice() {
    // MCP tool calls are not idempotent: if spotatui took `queue_tracks` and then
    // died, replaying it would queue the batch a second time. The relay cannot
    // know whether it was applied, so it has to say exactly that.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = fake_tui_that_dies_after_reading(listener, "tok".to_string());
    let mut relay = relay_for(port, "tok");

    let response: serde_json::Value =
      serde_json::from_str(&relay.handle(&modern_call()).await.unwrap()).unwrap();

    // Waits for a *second* line, which is what gives a replay time to show up:
    // asking for one would return the instant the first arrived and pass either
    // way.
    let lines = wait_for_lines(&seen, 2).await;
    assert_eq!(
      lines.len(),
      1,
      "the request must not be forwarded a second time: {lines:?}"
    );
    let text = response.to_string();
    assert!(
      text.contains("may or may not have been applied"),
      "the agent has to be told it is unknown, not that nothing happened: {text}"
    );
  }

  #[tokio::test]
  async fn a_rejected_control_token_becomes_an_offline_error_not_a_response() {
    // The control channel answers a bad token with one non-MCP line and hangs
    // up. Forwarded as-is it becomes the agent's "response" to a request it is
    // still waiting on, under no id it recognises.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    fake_tui_that_rejects_the_token(listener);
    let mut relay = relay_for(port, "stale-token");

    let response: serde_json::Value =
      serde_json::from_str(&relay.handle(&modern_call()).await.unwrap()).unwrap();

    assert!(
      response["result"]["isError"] == json!(true),
      "the rejection must be reported, not relayed: {response}"
    );
    let text = response.to_string();
    assert!(
      text.contains("control token"),
      "the reason has to name the stale control file: {text}"
    );
    // Nothing reached the server, so the agent must not be warned off retrying.
    assert!(
      !text.contains("may or may not have been applied"),
      "a refused token applied nothing: {text}"
    );
  }

  #[tokio::test]
  async fn the_handshake_replay_does_not_accept_a_rejection_as_a_valid_reply() {
    // The same line arriving one read earlier. Taken for a handshake answer, it
    // marked a socket the server had already hung up on as healthy.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    fake_tui_that_rejects_the_token(listener);
    let mut relay = relay_for(port, "stale-token");
    relay.handshake = Some(
      json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
             "params": {"protocolVersion": "2025-06-18"}})
      .to_string(),
    );

    assert!(!relay.ensure_upstream().await, "the socket is not usable");
    assert!(
      relay.offline_reason.contains("control token"),
      "{}",
      relay.offline_reason
    );
  }

  #[tokio::test]
  async fn a_player_that_takes_the_request_and_never_answers_does_not_hang_the_session() {
    // `run` handles one agent line at a time, so an upstream read with no
    // deadline blocks every later tool call too — a stopped spotatui would take
    // the agent's whole MCP session down with it. The deadline has to end the
    // wait, and it has to be reported as `Sent`: the request did land, so the
    // agent must not be told nothing happened.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    fake_tui_that_never_answers(listener, "tok".to_string());
    let mut relay = Relay {
      response_timeout: std::time::Duration::from_millis(100),
      ..relay_for(port, "tok")
    };

    let response: serde_json::Value = serde_json::from_str(
      &tokio::time::timeout(
        std::time::Duration::from_secs(5),
        relay.handle(&modern_call()),
      )
      .await
      .expect("the deadline must fire rather than hanging")
      .unwrap(),
    )
    .unwrap();

    assert_eq!(response["result"]["isError"], json!(true));
    let text = response.to_string();
    assert!(
      text.contains("may or may not have been applied"),
      "a delivered-then-unanswered request has to say so: {text}"
    );
    assert!(
      relay.upstream.is_none(),
      "the socket must be dropped, or a late answer would be read as the next request's"
    );
  }

  #[tokio::test]
  async fn a_player_that_restarts_mid_session_costs_one_line_not_the_session() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = fake_tui(listener, "tok".to_string());
    let mut relay = relay_for(port, "tok");

    relay.handle(&modern_call()).await.unwrap();
    // Kill the socket underneath, exactly as a restart does.
    relay.upstream = None;
    let after: serde_json::Value =
      serde_json::from_str(&relay.handle(&modern_call()).await.unwrap()).unwrap();
    assert_eq!(
      after["result"]["echoed"],
      json!(true),
      "the call after a restart must be served, not answered offline"
    );
    assert_eq!(wait_for_lines(&seen, 2).await.len(), 2);
  }

  #[tokio::test]
  async fn a_line_the_server_will_answer_never_leaves_a_reply_unread() {
    // A JSON value that is not an object has no `id`, but the server still
    // answers it (there is no method, so: invalid request). Treating it as a
    // notification would leave that reply in the socket, and every later
    // request would then read the *previous* request's response — an off-by-one
    // that never recovers.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _seen = fake_tui(listener, "tok".to_string());
    let mut relay = relay_for(port, "tok");

    for line in ["[]", "\"a string\"", "5", "{not json", "{\"id\": 7}"] {
      relay.handle(line).await;
    }

    // The stream must still be aligned: this call's answer, under this call's id.
    let response: serde_json::Value =
      serde_json::from_str(&relay.handle(&modern_call()).await.unwrap()).unwrap();
    assert_eq!(
      response["id"],
      json!(1),
      "the response must be this request's, not a stale one: {response}"
    );
  }

  #[tokio::test]
  async fn a_legacy_client_gets_the_offline_error_not_a_protocol_error() {
    // The exact story the reconnect fix exists for: Claude Code handshakes
    // (forwarded upstream, so the local session never saw it), the player then
    // goes away. Its next request carries no `_meta`, and answering it from a
    // session that never saw the handshake would reject it for missing the very
    // fields the handshake exists to make unnecessary.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _seen = fake_tui(listener, "tok".to_string());
    let mut relay = relay_for(port, "tok");

    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                      "params": {"protocolVersion": "2025-06-18"}})
    .to_string();
    relay.handle(&init).await.unwrap();

    // The player goes away for good.
    relay.upstream = None;
    relay.locate = Box::new(|| anyhow::bail!("no MCP control file"));

    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                      "params": {"name": "get_now_playing", "arguments": {}}})
    .to_string();
    let response: serde_json::Value =
      serde_json::from_str(&relay.handle(&call).await.unwrap()).unwrap();
    assert!(
      response.get("error").is_none(),
      "a legacy client must not be told its request shape is wrong: {response}"
    );
    assert_eq!(response["result"]["isError"], json!(true));
    assert!(response["result"]["content"][0]["text"]
      .as_str()
      .unwrap()
      .contains("not available"));
  }

  #[tokio::test]
  async fn notifications_are_forwarded_without_waiting_for_a_reply() {
    // The old pump was bidirectional, so a notification cost nothing. A
    // request/response loop that waited on one would hang the whole session.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = fake_tui(listener, "tok".to_string());
    let mut relay = relay_for(port, "tok");

    let notification = json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string();
    let answered = tokio::time::timeout(
      std::time::Duration::from_secs(5),
      relay.handle(&notification),
    )
    .await
    .expect("must not block waiting for a response that never comes");
    assert!(answered.is_none(), "a notification must never be answered");
    assert_eq!(wait_for_lines(&seen, 1).await.len(), 1);
  }
}
