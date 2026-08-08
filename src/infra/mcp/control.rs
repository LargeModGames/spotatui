//! The control channel: how `spotatui mcp` reaches the running TUI.
//!
//! MCP stdio servers are spawned by the *client*, but the music is playing in an
//! already-running TUI process. So the TUI listens on a loopback socket and
//! `spotatui mcp` relays between the agent's stdio and that socket. The spec
//! sanctions reusing the stdio framing over "Unix domain sockets, TCP
//! connections, or any similar channel", so the hop carries ordinary MCP
//! messages and the relay stays a line pump.
//!
//! ## Trust boundary
//!
//! The listener binds `127.0.0.1` only and requires a shared secret as the first
//! line of every connection. Both the port and the secret live in
//! `~/.config/spotatui/mcp.json`, written `0600` inside a directory that is
//! already `0700` on unix and carries an auto-written `.gitignore` — the same
//! protection the Spotify token cache relies on. A local process able to read
//! that file is already inside the trust boundary.
//!
//! It is **opt-in**: nothing listens unless `behavior.mcp_enabled` is set.
//! Opening a control socket that can drive playback and read listening history
//! is a security-posture change, so it should be a deliberate act.

use super::executor::AppExecutor;
use super::server;
use crate::core::app::App;
use crate::infra::network::IoEvent;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

const HANDSHAKE_FILE: &str = "mcp.json";
/// Overrides the handshake file location. Test-only: the real path is derived from
/// the config directory, and a test must not write into the user's own.
const HANDSHAKE_PATH_ENV: &str = "SPOTATUI_MCP_HANDSHAKE_PATH";
/// A connecting relay has to present its token immediately; without a deadline a
/// silent connection would hold a task open forever.
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the TUI publishes so a relay can find and authenticate to it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Handshake {
  pub port: u16,
  pub token: String,
  /// PID of the TUI that wrote this, so a relay can tell a live socket from a
  /// stale file left by a process that was killed.
  pub pid: u32,
}

pub fn handshake_path() -> Result<PathBuf> {
  if let Ok(path) = std::env::var(HANDSHAKE_PATH_ENV) {
    if !path.trim().is_empty() {
      return Ok(PathBuf::from(path));
    }
  }
  let dir = crate::core::user_config::default_app_config_dir()
    .context("could not determine the spotatui config directory")?;
  Ok(dir.join(HANDSHAKE_FILE))
}

/// Read the published handshake, if a TUI has written one.
pub fn read_handshake() -> Result<Handshake> {
  let path = handshake_path()?;
  let raw = std::fs::read_to_string(&path)
    .with_context(|| format!("no MCP control file at {}", path.display()))?;
  serde_json::from_str(&raw)
    .with_context(|| format!("malformed MCP control file at {}", path.display()))
}

fn write_handshake(handshake: &Handshake) -> Result<()> {
  let path = handshake_path()?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let body = serde_json::to_string_pretty(handshake)?;
  // Owner-only, and owner-only *before* the token is in it: creating at the
  // umask default and chmod-ing afterwards leaves a window where any local user
  // can read a capability to drive the player.
  #[cfg(unix)]
  {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(true)
      .mode(0o600)
      .open(&path)?;
    // `mode` only applies when the file is created; an older run may have left a
    // world-readable one behind.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(body.as_bytes())?;
  }
  #[cfg(not(unix))]
  std::fs::write(&path, body)?;
  Ok(())
}

/// Remove the published handshake. Called on shutdown so a relay does not chase
/// a socket that is gone.
pub fn clear_handshake() {
  if let Ok(path) = handshake_path() {
    let _ = std::fs::remove_file(path);
  }
}

fn generate_token() -> String {
  // 128 bits from the OS CSPRNG, hex-encoded.
  let high = rand::random::<u64>();
  let low = rand::random::<u64>();
  format!("{high:016x}{low:016x}")
}

/// Bind the control listener and serve connections until the process exits.
///
/// Returns the bound port so the caller can report it. Each accepted connection
/// gets its own task and its own [`server::Session`], so the per-connection era
/// latch cannot leak between two different agents talking to us at once.
pub async fn spawn_listener(app: Arc<Mutex<App>>, io_tx: Sender<IoEvent>) -> Result<u16> {
  let listener = TcpListener::bind(("127.0.0.1", 0))
    .await
    .context("could not bind the MCP control socket on 127.0.0.1")?;
  let port = listener.local_addr()?.port();
  let token = generate_token();

  write_handshake(&Handshake {
    port,
    token: token.clone(),
    pid: std::process::id(),
  })
  .context("could not publish the MCP control file")?;

  tokio::spawn(async move {
    loop {
      let (stream, peer) = match listener.accept().await {
        Ok(accepted) => accepted,
        Err(e) => {
          log::warn!("MCP: accept failed: {e}");
          continue;
        }
      };
      // Defence in depth: the bind is already loopback-only, but a bound socket
      // is worth re-checking rather than assuming.
      if !peer.ip().is_loopback() {
        log::warn!("MCP: refusing non-loopback connection from {peer}");
        continue;
      }
      let app = Arc::clone(&app);
      let io_tx = io_tx.clone();
      let token = token.clone();
      tokio::spawn(async move {
        if let Err(e) = serve_connection(stream, app, io_tx, token, HANDSHAKE_TIMEOUT).await {
          log::debug!("MCP: connection ended: {e}");
        }
      });
    }
  });

  Ok(port)
}

/// Authenticate one connection, then hand it to the protocol loop.
///
/// `handshake_timeout` is a parameter rather than a constant so the deadline is
/// testable without manipulating the clock.
async fn serve_connection(
  stream: TcpStream,
  app: Arc<Mutex<App>>,
  io_tx: Sender<IoEvent>,
  expected_token: String,
  handshake_timeout: std::time::Duration,
) -> Result<()> {
  let (read_half, mut write_half) = stream.into_split();
  let mut reader = BufReader::new(read_half);

  // The token line comes before any MCP traffic. This is our own transport, so a
  // non-MCP handshake line is fine here; the stdio channel the relay presents to
  // the agent stays pure MCP.
  let mut first_line = String::new();
  let read = tokio::time::timeout(handshake_timeout, reader.read_line(&mut first_line)).await;
  match read {
    Ok(Ok(0)) => anyhow::bail!("client closed before handshake"),
    Ok(Ok(_)) => {}
    Ok(Err(e)) => return Err(e.into()),
    Err(_) => anyhow::bail!("handshake timed out"),
  }

  let presented = serde_json::from_str::<serde_json::Value>(&first_line)
    .ok()
    .and_then(|value| {
      value
        .get("token")
        .and_then(|token| token.as_str())
        .map(str::to_string)
    });

  let authorised = presented.as_deref().is_some_and(|token| {
    // Length-independent comparison is overkill for a loopback socket, but
    // constant-time-ish beats an early-exit compare and costs nothing.
    token.len() == expected_token.len()
      && token
        .bytes()
        .zip(expected_token.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
  });

  if !authorised {
    let _ = write_half
      .write_all(b"{\"error\":\"unauthorized\"}\n")
      .await;
    anyhow::bail!("bad MCP control token");
  }

  let executor = AppExecutor::new(app, io_tx);
  server::serve(reader, write_half, &executor).await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn generated_tokens_are_long_and_distinct() {
    let a = generate_token();
    let b = generate_token();
    assert_eq!(a.len(), 32, "128 bits, hex-encoded");
    assert_ne!(a, b);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
  }

  #[test]
  fn handshake_round_trips_through_json() {
    let handshake = Handshake {
      port: 51234,
      token: "deadbeef".into(),
      pid: 42,
    };
    let encoded = serde_json::to_string(&handshake).unwrap();
    let decoded: Handshake = serde_json::from_str(&encoded).unwrap();
    assert_eq!(handshake, decoded);
  }

  #[tokio::test]
  async fn a_bad_token_closes_the_connection() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (io_tx, _io_rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::default()));

    let server = tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      serve_connection(
        stream,
        app,
        io_tx,
        "correct-token".to_string(),
        HANDSHAKE_TIMEOUT,
      )
      .await
    });

    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    client
      .write_all(b"{\"token\":\"wrong-token\"}\n")
      .await
      .unwrap();

    // The server rejects and hangs up rather than serving any tools.
    let result = server.await.unwrap();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("token"));
  }

  #[tokio::test]
  async fn a_good_token_opens_the_protocol() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    // Keep the receiver alive so app-only calls are not reported as shut down.
    let (io_tx, _io_rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::default()));

    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let _ = serve_connection(
        stream,
        app,
        io_tx,
        "correct-token".to_string(),
        HANDSHAKE_TIMEOUT,
      )
      .await;
    });

    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).lines();

    write_half
      .write_all(b"{\"token\":\"correct-token\"}\n")
      .await
      .unwrap();
    let request = serde_json::json!({
      "jsonrpc": "2.0", "id": 1, "method": "server/discover",
      "params": {"_meta": {
        super::super::protocol::META_PROTOCOL_VERSION: super::super::protocol::PROTOCOL_MODERN,
        super::super::protocol::META_CLIENT_CAPABILITIES: {},
      }}
    });
    write_half
      .write_all(format!("{request}\n").as_bytes())
      .await
      .unwrap();

    let line = reader.next_line().await.unwrap().expect("a response");
    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
      response["result"]["resultType"],
      serde_json::json!("complete")
    );
  }

  /// The full startup seam: `spawn_listener` binds, publishes a handshake, and
  /// the `status` probe reaches it — the path the runtime takes at boot, which no
  /// other test exercises end to end.
  #[tokio::test]
  async fn spawn_listener_publishes_a_handshake_the_status_probe_can_use() {
    let dir = std::env::temp_dir().join(format!("spotatui-mcp-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mcp.json");
    // Serialised against the other env-var tests by being the only one to use
    // this variable, and cleaned up below.
    unsafe { std::env::set_var(HANDSHAKE_PATH_ENV, &path) };

    let (io_tx, _io_rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::default()));
    let port = spawn_listener(app, io_tx)
      .await
      .expect("listener should bind");

    let published = read_handshake().expect("a handshake should be published");
    assert_eq!(published.port, port);
    assert_eq!(published.pid, std::process::id());
    assert_eq!(published.token.len(), 32);

    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
      assert_eq!(mode, 0o600, "the token file must be owner-only");
    }

    // The probe the setup docs tell an agent to run.
    let version = super::super::status::probe_for_test(&published)
      .await
      .expect("the probe should reach the listener");
    assert_eq!(version, super::super::protocol::PROTOCOL_MODERN);

    clear_handshake();
    assert!(read_handshake().is_err(), "shutdown should unpublish it");
    unsafe { std::env::remove_var(HANDSHAKE_PATH_ENV) };
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[tokio::test]
  async fn a_silent_client_is_dropped_rather_than_held_open() {
    // Regression guard: without the handshake deadline a connection that never
    // speaks would pin a task for the life of the process.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (io_tx, _io_rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::default()));

    let server = tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      serve_connection(
        stream,
        app,
        io_tx,
        "t".to_string(),
        std::time::Duration::from_millis(50),
      )
      .await
    });

    // Connected and held open, but never writes a handshake line.
    let _client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), server)
      .await
      .expect("the deadline must fire rather than hanging")
      .unwrap();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("timed out"));
  }
}
