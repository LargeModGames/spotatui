//! The loopback page server: serves the embedded frontend and upgrades the page's
//! socket once the Host, Origin and launch-code checks pass.

use crate::gui::bridge::{self, Link};
use crate::infra::loopback::{generate_token, tokens_match};
use anyhow::{Context, Result};
use std::io::Cursor;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;

const MAX_HEAD_BYTES: usize = 16 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCH_CODE_TTL: Duration = Duration::from_secs(60);
const CODE_PROTOCOL: &str = "spotatui.code.";
const TOKEN_PROTOCOL: &str = "spotatui.token.";
const PROTOCOL_HEADER: &str = "Sec-WebSocket-Protocol";
const TEXT: &str = "text/plain; charset=utf-8";
const MISSING_FRONTEND: &[u8] = b"This spotatui-gui was built without its frontend. Run `npm ci` and `npm run build` in gui/, then rebuild spotatui-gui (touch build.rs if cargo reuses the old build).\n";

type Assets = &'static [(&'static str, &'static [u8])];

/// Every file in `gui/dist`, embedded by build.rs.
pub(crate) static ASSETS: Assets = include!(concat!(env!("OUT_DIR"), "/gui_assets.rs"));

struct Server {
  port: u16,
  access: Arc<Access>,
  assets: Assets,
  link: Link,
}

/// The launch code the browser URL carries, and the session token it is traded for.
pub(crate) struct Access {
  code: Mutex<Option<(String, Instant)>>,
  token: String,
}

enum Admission {
  Fresh,
  Resumed,
}

impl Access {
  /// Replaces any earlier launch code with a fresh one, live for a minute from `now`.
  pub(crate) fn issue(&self, now: Instant) -> Result<String> {
    let code = generate_token()?;
    *self.code.lock().unwrap_or_else(PoisonError::into_inner) = Some((code.clone(), now));
    Ok(code)
  }

  /// Admits the live launch code once, or the session token any number of times.
  fn admit(&self, offered: &str, now: Instant) -> Option<Admission> {
    if let Some(token) = offered.strip_prefix(TOKEN_PROTOCOL) {
      return tokens_match(token, &self.token).then_some(Admission::Resumed);
    }
    let code = offered.strip_prefix(CODE_PROTOCOL)?;
    self
      .code
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .take_if(|(expected, issued)| {
        now.saturating_duration_since(*issued) < LAUNCH_CODE_TTL && tokens_match(code, expected)
      })?;
    Some(Admission::Fresh)
  }
}

struct Head {
  method: String,
  path: String,
  host: Option<String>,
  origin: Option<String>,
  fetch_site: Option<String>,
  upgrade: bool,
}

impl Head {
  fn new(request: &httparse::Request<'_, '_>) -> Self {
    let header = |name: &str| {
      request
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| String::from_utf8_lossy(header.value).into_owned())
    };
    Head {
      method: request.method.unwrap_or_default().to_string(),
      path: request.path.unwrap_or_default().to_string(),
      host: header("Host"),
      origin: header("Origin"),
      fetch_site: header("Sec-Fetch-Site"),
      upgrade: header("Upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket")),
    }
  }
}

/// Serves 127.0.0.1:0 until exit; the caller issues the launch code when it opens the browser.
pub(crate) async fn spawn_listener(assets: Assets, link: Link) -> Result<(u16, Arc<Access>)> {
  let listener = TcpListener::bind(("127.0.0.1", 0))
    .await
    .context("could not bind the GUI server on 127.0.0.1")?;
  let port = listener.local_addr()?.port();
  let access = Arc::new(Access {
    code: Mutex::new(None),
    token: generate_token()?,
  });
  let server = Arc::new(Server {
    port,
    access: Arc::clone(&access),
    assets,
    link,
  });
  tokio::spawn(async move {
    loop {
      let (stream, peer) = match listener.accept().await {
        Ok(accepted) => accepted,
        Err(e) => {
          log::warn!("GUI: accept failed: {e}");
          tokio::time::sleep(Duration::from_millis(100)).await;
          continue;
        }
      };
      if !peer.ip().is_loopback() {
        log::warn!("GUI: refusing non-loopback connection from {peer}");
        continue;
      }
      let server = Arc::clone(&server);
      tokio::spawn(async move {
        if let Err(e) = serve_connection(stream, &server).await {
          log::debug!("GUI: connection ended: {e}");
        }
      });
    }
  });
  Ok((port, access))
}

async fn serve_connection(mut stream: TcpStream, server: &Server) -> Result<()> {
  let (head, raw) = tokio::time::timeout(HEAD_TIMEOUT, read_head(&mut stream))
    .await
    .context("request head timed out")??;
  let port = server.port;
  if !is_allowed(&head, port) {
    return respond(&mut stream, port, "403 Forbidden", TEXT, b"forbidden\n").await;
  }
  if head.method != "GET" {
    return respond(
      &mut stream,
      port,
      "405 Method Not Allowed",
      TEXT,
      b"method not allowed\n",
    )
    .await;
  }
  if head.upgrade {
    return open_socket(stream, raw, server).await;
  }
  let path = head.path.split('?').next().unwrap_or_default();
  let name = if path == "/" {
    "index.html"
  } else {
    path.trim_start_matches('/')
  };
  match server.assets.iter().find(|(asset, _)| *asset == name) {
    Some((_, body)) => respond(&mut stream, port, "200 OK", content_type(name), body).await,
    None if name == "index.html" => {
      respond(
        &mut stream,
        port,
        "503 Service Unavailable",
        TEXT,
        MISSING_FRONTEND,
      )
      .await
    }
    None => respond(&mut stream, port, "404 Not Found", TEXT, b"not found\n").await,
  }
}

/// Reads one request head, keeping every byte read so an upgrade can replay it.
async fn read_head(stream: &mut TcpStream) -> Result<(Head, Vec<u8>)> {
  let mut buf = vec![0; MAX_HEAD_BYTES];
  let mut len = 0;
  loop {
    if len == buf.len() {
      anyhow::bail!("request head too long");
    }
    let n = stream.read(&mut buf[len..]).await?;
    if n == 0 {
      anyhow::bail!("client closed before the request head ended");
    }
    len += n;
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    if request.parse(&buf[..len])?.is_complete() {
      let head = Head::new(&request);
      buf.truncate(len);
      return Ok((head, buf));
    }
  }
}

/// DNS rebinding and cross-site checks: an exact Host, and an Origin equal to it
/// (absent only on a same-origin or top-level page GET).
fn is_allowed(head: &Head, port: u16) -> bool {
  let Some(host) = head.host.as_deref() else {
    return false;
  };
  if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
    return false;
  }
  match head.origin.as_deref() {
    Some(origin) => origin == format!("http://{host}"),
    None => {
      !head.upgrade
        && matches!(
          head.fetch_site.as_deref(),
          None | Some("none" | "same-origin")
        )
    }
  }
}

// The callback's error type is tungstenite's `ErrorResponse`, large by its API.
#[allow(clippy::result_large_err)]
async fn open_socket(stream: TcpStream, raw: Vec<u8>, server: &Server) -> Result<()> {
  let (read_half, write_half) = stream.into_split();
  let replay = tokio::io::join(Cursor::new(raw).chain(read_half), write_half);
  let mut admission = None;
  let socket =
    tokio_tungstenite::accept_hdr_async(replay, |request: &Request, mut response: Response| {
      let offered = request.headers().get(PROTOCOL_HEADER).cloned();
      admission = offered
        .as_ref()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| server.access.admit(value, Instant::now()));
      match offered {
        Some(value) if admission.is_some() => {
          response.headers_mut().insert(PROTOCOL_HEADER, value);
          Ok(response)
        }
        _ => {
          let mut refusal = ErrorResponse::new(Some("forbidden".to_string()));
          *refusal.status_mut() = StatusCode::FORBIDDEN;
          Err(refusal)
        }
      }
    })
    .await?;
  let token = matches!(admission, Some(Admission::Fresh)).then(|| server.access.token.clone());
  bridge::run_socket(socket, server.link.clone(), token).await;
  Ok(())
}

async fn respond(
  stream: &mut TcpStream,
  port: u16,
  status: &str,
  content_type: &str,
  body: &[u8],
) -> Result<()> {
  let head = format!(
    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; img-src 'self' https:; connect-src ws://127.0.0.1:{port} ws://localhost:{port}; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
    body.len()
  );
  stream.write_all(head.as_bytes()).await?;
  stream.write_all(body).await?;
  stream.shutdown().await?;
  Ok(())
}

fn content_type(name: &str) -> &'static str {
  match name.rsplit_once('.').map(|(_, extension)| extension) {
    Some("html") => "text/html; charset=utf-8",
    Some("js") => "text/javascript",
    Some("css") => "text/css",
    Some("svg") => "image/svg+xml",
    _ => "application/octet-stream",
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::user_config::UserConfig;
  use crate::gui::protocol::ClientMessage;
  use futures::{SinkExt, StreamExt};
  use std::time::SystemTime;
  use tokio::sync::mpsc::UnboundedReceiver;
  use tokio_tungstenite::tungstenite::client::IntoClientRequest;
  use tokio_tungstenite::tungstenite::Message as WsMessage;
  use tokio_tungstenite::WebSocketStream;

  const PAGE: Assets = &[("index.html", b"<!doctype html>" as &[u8])];

  fn test_link() -> (Link, UnboundedReceiver<ClientMessage>) {
    let (tx, _rx) = std::sync::mpsc::channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let (link, _publisher, inbox) = bridge::channel(Arc::new(tokio::sync::Mutex::new(app)));
    (link, inbox)
  }

  async fn start(assets: Assets) -> (u16, String, UnboundedReceiver<ClientMessage>) {
    let (link, inbox) = test_link();
    let (port, access) = spawn_listener(assets, link).await.unwrap();
    let code = access.issue(Instant::now()).unwrap();
    (port, code, inbox)
  }

  async fn get(port: u16, head: String) -> String {
    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    client.write_all(head.as_bytes()).await.unwrap();
    let mut reply = Vec::new();
    client.read_to_end(&mut reply).await.unwrap();
    String::from_utf8_lossy(&reply).into_owned()
  }

  async fn connect(
    port: u16,
    origin: &str,
    protocol: Option<&str>,
  ) -> Result<WebSocketStream<TcpStream>> {
    let mut request = format!("ws://127.0.0.1:{port}/ws").into_client_request()?;
    request.headers_mut().insert("Origin", origin.parse()?);
    if let Some(protocol) = protocol {
      request
        .headers_mut()
        .insert(PROTOCOL_HEADER, protocol.parse()?);
    }
    let stream = TcpStream::connect(("127.0.0.1", port)).await?;
    Ok(tokio_tungstenite::client_async(request, stream).await?.0)
  }

  async fn next_json(socket: &mut WebSocketStream<TcpStream>) -> serde_json::Value {
    let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
      .await
      .expect("a frame within 5 s")
      .unwrap()
      .unwrap();
    serde_json::from_str(frame.to_text().unwrap()).unwrap()
  }

  fn head(host: &str, origin: Option<&str>, fetch_site: Option<&str>, upgrade: bool) -> Head {
    Head {
      method: "GET".to_string(),
      path: "/".to_string(),
      host: Some(host.to_string()),
      origin: origin.map(str::to_string),
      fetch_site: fetch_site.map(str::to_string),
      upgrade,
    }
  }

  #[tokio::test]
  async fn a_foreign_host_is_refused() {
    let (port, _code, _inbox) = start(PAGE).await;

    let reply = get(
      port,
      format!("GET / HTTP/1.1\r\nHost: attacker.example:{port}\r\n\r\n"),
    )
    .await;

    assert!(reply.starts_with("HTTP/1.1 403"), "{reply}");
  }

  #[test]
  fn cross_site_requests_fail_the_origin_check() {
    assert!(is_allowed(
      &head("127.0.0.1:9", None, Some("none"), false),
      9
    ));
    assert!(is_allowed(
      &head("localhost:9", None, Some("same-origin"), false),
      9
    ));
    assert!(is_allowed(
      &head("127.0.0.1:9", Some("http://127.0.0.1:9"), None, true),
      9
    ));
    assert!(!is_allowed(
      &head("127.0.0.1:9", None, Some("cross-site"), false),
      9
    ));
    assert!(!is_allowed(
      &head("127.0.0.1:9", None, Some("same-site"), false),
      9
    ));
    assert!(!is_allowed(
      &head("127.0.0.1:9", Some("http://localhost:9"), None, false),
      9
    ));
    assert!(!is_allowed(&head("127.0.0.1:9", None, None, true), 9));
  }

  #[tokio::test]
  async fn a_foreign_origin_cannot_open_the_socket() {
    let (port, code, _inbox) = start(PAGE).await;
    let protocol = format!("{CODE_PROTOCOL}{code}");

    assert!(connect(port, "http://attacker.example", Some(&protocol))
      .await
      .is_err());
    assert!(
      connect(port, &format!("http://127.0.0.1:{port}"), Some(&protocol))
        .await
        .is_ok()
    );
  }

  #[tokio::test]
  async fn a_socket_without_a_valid_token_is_refused() {
    let (port, _code, _inbox) = start(PAGE).await;
    let origin = format!("http://127.0.0.1:{port}");

    assert!(connect(port, &origin, None).await.is_err());
    let wrong = format!("{TOKEN_PROTOCOL}{}", "0".repeat(32));
    assert!(connect(port, &origin, Some(&wrong)).await.is_err());
  }

  #[tokio::test]
  async fn the_launch_code_opens_one_socket_and_never_a_second() {
    let (port, code, _inbox) = start(PAGE).await;
    let origin = format!("http://127.0.0.1:{port}");
    let protocol = format!("{CODE_PROTOCOL}{code}");

    let mut first = connect(port, &origin, Some(&protocol)).await.unwrap();
    let hello = next_json(&mut first).await;
    let token = hello["payload"]["token"].as_str().unwrap().to_string();

    assert!(connect(port, &origin, Some(&protocol)).await.is_err());
    let resumed = format!("{TOKEN_PROTOCOL}{token}");
    let mut second = connect(port, &origin, Some(&resumed)).await.unwrap();
    assert!(next_json(&mut second).await["payload"]["token"].is_null());
  }

  #[test]
  fn an_expired_launch_code_is_refused() {
    let t0 = Instant::now();
    let access = Access {
      code: Mutex::new(Some(("c0de".to_string(), t0))),
      token: "t".to_string(),
    };

    assert!(access
      .admit("spotatui.code.c0de", t0 + LAUNCH_CODE_TTL)
      .is_none());
    assert!(access.admit("spotatui.code.c0de", t0).is_some());
    let code = access.issue(t0 + LAUNCH_CODE_TTL).unwrap();
    assert!(access
      .admit(&format!("{CODE_PROTOCOL}{code}"), t0 + LAUNCH_CODE_TTL)
      .is_some());
  }

  #[tokio::test]
  async fn the_page_is_served_with_the_security_headers() {
    let (port, _code, _inbox) = start(PAGE).await;
    let request = |method: &str, path: &str| {
      format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nSec-Fetch-Site: none\r\n\r\n")
    };

    let page = get(port, request("GET", "/")).await;
    assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
    assert!(page.contains("Cache-Control: no-store"));
    assert!(page.contains("X-Content-Type-Options: nosniff"));
    assert!(page.contains(&format!(
      "connect-src ws://127.0.0.1:{port} ws://localhost:{port}"
    )));
    assert!(!page.contains("Access-Control-"));
    assert!(page.ends_with("<!doctype html>"));
    assert!(get(port, request("GET", "/missing.js"))
      .await
      .starts_with("HTTP/1.1 404"));
    assert!(get(port, request("POST", "/"))
      .await
      .starts_with("HTTP/1.1 405"));
  }

  #[tokio::test]
  async fn a_build_without_the_frontend_answers_503() {
    let (port, _code, _inbox) = start(&[]).await;

    let reply = get(
      port,
      format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
    )
    .await;

    assert!(reply.starts_with("HTTP/1.1 503"), "{reply}");
    assert!(reply.contains("npm run build"));
  }

  #[tokio::test]
  async fn a_request_head_over_the_cap_is_dropped() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (link, _inbox) = test_link();
    let server = Server {
      port,
      access: Arc::new(Access {
        code: Mutex::new(None),
        token: "t".to_string(),
      }),
      assets: PAGE,
      link,
    };
    let client = tokio::spawn(async move {
      let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
      let _ = stream.write_all(&vec![b'a'; MAX_HEAD_BYTES + 1]).await;
      stream
    });
    let (stream, _) = listener.accept().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(5), serve_connection(stream, &server))
      .await
      .unwrap();

    assert!(result.unwrap_err().to_string().contains("too long"));
    drop(client);
  }

  #[tokio::test]
  async fn a_launch_code_connect_gets_hello_then_every_channel() {
    let (port, code, mut inbox) = start(PAGE).await;
    let mut socket = connect(
      port,
      &format!("http://127.0.0.1:{port}"),
      Some(&format!("{CODE_PROTOCOL}{code}")),
    )
    .await
    .unwrap();

    let mut values = Vec::new();
    for _ in 0..7 {
      values.push(next_json(&mut socket).await);
    }
    let kinds: Vec<_> = values.iter().map(|value| value["kind"].clone()).collect();
    assert_eq!(
      kinds,
      ["hello", "route", "status", "theme", "playback", "devices", "queue"]
    );
    assert!(values[0]["payload"]["token"].is_string());
    assert!(values[0]["payload"]["revisions"]["playback"].is_u64());

    socket
      .send(WsMessage::text(
        r#"{"type":"action","action":"TogglePlayback"}"#,
      ))
      .await
      .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(5), inbox.recv())
      .await
      .unwrap();
    assert!(matches!(received, Some(ClientMessage::Action { .. })));
  }
}
