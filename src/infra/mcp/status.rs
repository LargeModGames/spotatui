//! `spotatui mcp status`: a safe, machine-checkable probe of the MCP setup.
//!
//! This exists because of a trap. `spotatui mcp` **is the server** — it reads
//! JSON-RPC from stdin and blocks forever. An AI agent told to "test the MCP
//! server" will run it, hang, and eventually be killed, having learned nothing.
//! `status` gives the agent something it can actually run: it checks each step of
//! the setup, prints one line per check, and sets a **non-zero exit code** when
//! the setup is not ready.
//!
//! Used by `docs/mcp-setup.md`, which is written for an agent to follow.

use super::control::{self, Handshake};
use super::protocol as proto;
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// One line of the report.
struct Check {
  ok: bool,
  label: &'static str,
  detail: String,
}

impl Check {
  fn pass(label: &'static str, detail: impl Into<String>) -> Self {
    Self {
      ok: true,
      label,
      detail: detail.into(),
    }
  }
  fn fail(label: &'static str, detail: impl Into<String>) -> Self {
    Self {
      ok: false,
      label,
      detail: detail.into(),
    }
  }
}

/// Run every check and render the report. Returns the rendered report (the
/// caller prints it - this is library code, the CLI owns stdout) and the
/// process exit code: `0` when the MCP setup is fully working, `1` otherwise.
pub async fn run(as_json: bool) -> (String, i32) {
  let mut checks = Vec::new();

  // 1. Is the feature even compiled in? Reaching this code proves it is, which is
  //    worth stating explicitly so an agent can distinguish "wrong binary" from
  //    "not configured".
  checks.push(Check::pass(
    "binary",
    format!(
      "spotatui {} with the mcp-server feature",
      proto::SERVER_VERSION
    ),
  ));

  // 2. Has a running TUI published a control file?
  let handshake = control::read_handshake();
  match &handshake {
    Ok(handshake) => checks.push(Check::pass(
      "control-file",
      format!(
        "found (port {}, pid {}) at {}",
        handshake.port,
        handshake.pid,
        control::handshake_path()
          .map(|path| path.display().to_string())
          .unwrap_or_else(|_| "<unknown>".into())
      ),
    )),
    Err(e) => checks.push(Check::fail(
      "control-file",
      format!(
        "{e}. Start spotatui and set `behavior.mcp_enabled: true` in {}, then restart it.",
        // The resolved path, not a hard-coded `~/.config`: with an absolute
        // XDG_CONFIG_HOME the agent would otherwise be sent to edit a file
        // spotatui never reads, and this check would keep failing.
        control::handshake_path()
          .ok()
          .and_then(|path| path.parent().map(|dir| dir.join("config.yml")))
          .map(|path| path.display().to_string())
          .unwrap_or_else(|| "the spotatui config directory".into())
      ),
    )),
  }

  // 3. Can we complete the handshake and get a protocol answer back? This is the
  //    check that actually proves end-to-end reachability.
  if let Ok(handshake) = &handshake {
    match probe(handshake).await {
      Ok(version) => checks.push(Check::pass(
        "connection",
        format!("handshake accepted; server speaks protocol {version}"),
      )),
      Err(e) => checks.push(Check::fail(
        "connection",
        format!("{e}. The control file may be stale — restart spotatui."),
      )),
    }
  }

  let ready = checks.iter().all(|check| check.ok);

  let report = if as_json {
    render_json(&checks, ready)
  } else {
    render_text(&checks, ready)
  };

  (report, i32::from(!ready))
}

fn render_json(checks: &[Check], ready: bool) -> String {
  let payload = json!({
    "ready": ready,
    "checks": checks.iter().map(|check| json!({
      "name": check.label,
      "ok": check.ok,
      "detail": check.detail,
    })).collect::<Vec<_>>(),
    "register_command": "claude mcp add spotatui -- spotatui mcp",
  });
  serde_json::to_string_pretty(&payload).unwrap_or_default()
}

fn render_text(checks: &[Check], ready: bool) -> String {
  let mut lines: Vec<String> = checks
    .iter()
    .map(|check| {
      format!(
        "{} {:<14} {}",
        if check.ok { "ok  " } else { "FAIL" },
        check.label,
        check.detail
      )
    })
    .collect();
  lines.push(String::new());
  if ready {
    lines.push("MCP is ready. Register it with your client, e.g.:".into());
    lines.push("  claude mcp add spotatui -- spotatui mcp".into());
  } else {
    lines.push("MCP is not ready yet — see the failing check(s) above.".into());
    lines.push("Full instructions: docs/mcp-setup.md".into());
  }
  lines.join("\n")
}

/// Connect, authenticate, and ask the server which protocol it speaks.
async fn probe(handshake: &Handshake) -> Result<String, String> {
  let connect = TcpStream::connect(("127.0.0.1", handshake.port));
  let stream = tokio::time::timeout(PROBE_TIMEOUT, connect)
    .await
    .map_err(|_| "connection timed out".to_string())?
    .map_err(|e| format!("could not connect on port {}: {e}", handshake.port))?;

  let (read_half, mut write_half) = stream.into_split();
  let mut lines = BufReader::new(read_half).lines();

  let hello = json!({ "token": handshake.token });
  write_half
    .write_all(format!("{hello}\n").as_bytes())
    .await
    .map_err(|e| format!("could not send the handshake: {e}"))?;

  // A modern `server/discover`, which every version of this server answers.
  let request = json!({
    "jsonrpc": "2.0",
    "id": "status-probe",
    "method": "server/discover",
    "params": { "_meta": {
      proto::META_PROTOCOL_VERSION: proto::PROTOCOL_MODERN,
      proto::META_CLIENT_CAPABILITIES: {},
      proto::META_CLIENT_INFO: { "name": "spotatui-mcp-status", "version": proto::SERVER_VERSION },
    }}
  });
  write_half
    .write_all(format!("{request}\n").as_bytes())
    .await
    .map_err(|e| format!("could not send the probe: {e}"))?;

  let line = tokio::time::timeout(PROBE_TIMEOUT, lines.next_line())
    .await
    .map_err(|_| "the server did not answer in time".to_string())?
    .map_err(|e| format!("could not read the answer: {e}"))?
    .ok_or_else(|| "the server closed the connection (bad token?)".to_string())?;

  let response: serde_json::Value =
    serde_json::from_str(&line).map_err(|e| format!("unreadable answer: {e}"))?;
  if let Some(error) = response.get("error") {
    return Err(format!("server returned an error: {error}"));
  }
  response
    .get("result")
    .and_then(|result| result.get("supportedVersions"))
    .and_then(|versions| versions.as_array())
    .and_then(|versions| versions.first())
    .and_then(|version| version.as_str())
    .map(str::to_string)
    .ok_or_else(|| "answer did not include supportedVersions".to_string())
}

/// The probe, for the control-channel round-trip test in the sibling module.
#[cfg(test)]
pub(super) async fn probe_for_test(handshake: &Handshake) -> Result<String, String> {
  probe(handshake).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::net::TcpListener;

  #[test]
  fn text_report_ends_with_the_register_hint_when_ready() {
    let checks = vec![Check::pass(
      "binary",
      "spotatui with the mcp-server feature",
    )];
    let report = render_text(&checks, true);
    assert!(report.starts_with("ok   binary"), "{report}");
    assert!(
      report.ends_with(
        "\n\nMCP is ready. Register it with your client, e.g.:\n  claude mcp add spotatui -- spotatui mcp"
      ),
      "{report}"
    );
  }

  #[test]
  fn text_report_points_at_the_failing_check_when_not_ready() {
    let checks = vec![
      Check::pass("binary", "x"),
      Check::fail("control-file", "missing"),
    ];
    let report = render_text(&checks, false);
    assert!(report.contains("FAIL control-file"), "{report}");
    assert!(
      report.ends_with("Full instructions: docs/mcp-setup.md"),
      "{report}"
    );
  }

  #[test]
  fn json_report_carries_ready_and_every_check() {
    let checks = vec![Check::fail("control-file", "missing")];
    let report = render_json(&checks, false);
    let value: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(value["ready"], false);
    assert_eq!(value["checks"][0]["name"], "control-file");
    assert_eq!(value["checks"][0]["ok"], false);
    assert_eq!(value["checks"][0]["detail"], "missing");
  }

  #[tokio::test]
  async fn probe_reports_the_protocol_when_the_server_answers() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let (read_half, mut write_half) = stream.into_split();
      let mut lines = BufReader::new(read_half).lines();
      // Handshake, then the discover request.
      let _token = lines.next_line().await.unwrap();
      let _request = lines.next_line().await.unwrap();
      let response = json!({
        "jsonrpc": "2.0", "id": "status-probe",
        "result": {"resultType": "complete", "supportedVersions": [proto::PROTOCOL_MODERN]}
      });
      write_half
        .write_all(format!("{response}\n").as_bytes())
        .await
        .unwrap();
    });

    let handshake = Handshake {
      port,
      token: "t".into(),
      pid: 1,
    };
    assert_eq!(probe(&handshake).await.unwrap(), proto::PROTOCOL_MODERN);
  }

  #[tokio::test]
  async fn probe_reports_a_closed_connection_as_a_bad_token() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      // Accept and hang up, which is what a token mismatch looks like.
      drop(stream);
    });

    let handshake = Handshake {
      port,
      token: "wrong".into(),
      pid: 1,
    };
    let err = probe(&handshake).await.unwrap_err();
    assert!(err.contains("closed") || err.contains("could not"), "{err}");
  }

  #[tokio::test]
  async fn probe_fails_fast_when_nothing_is_listening() {
    // Bind then drop, so the port is almost certainly free.
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let handshake = Handshake {
      port,
      token: "t".into(),
      pid: 1,
    };
    let err = probe(&handshake).await.unwrap_err();
    assert!(err.contains("could not connect"), "{err}");
  }

  #[tokio::test]
  async fn probe_surfaces_a_protocol_error_rather_than_claiming_success() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let (read_half, mut write_half) = stream.into_split();
      let mut lines = BufReader::new(read_half).lines();
      let _ = lines.next_line().await;
      let _ = lines.next_line().await;
      let response = json!({
        "jsonrpc": "2.0", "id": "status-probe",
        "error": {"code": -32022, "message": "Unsupported protocol version"}
      });
      write_half
        .write_all(format!("{response}\n").as_bytes())
        .await
        .unwrap();
    });

    let handshake = Handshake {
      port,
      token: "t".into(),
      pid: 1,
    };
    let err = probe(&handshake).await.unwrap_err();
    assert!(err.contains("Unsupported protocol version"), "{err}");
  }
}
