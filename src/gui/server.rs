//! The loopback page server: serves the embedded frontend and upgrades the page's
//! socket once the Host, Origin and launch-code checks pass.

use crate::gui::bridge::{self, CoverSource, Link};
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
/// A plain request, or the head and handshake of a socket, must finish within this.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCH_CODE_TTL: Duration = Duration::from_secs(60);
const CODE_PROTOCOL: &str = "spotatui.code.";
const TOKEN_PROTOCOL: &str = "spotatui.token.";
const PROTOCOL_HEADER: &str = "Sec-WebSocket-Protocol";
const TEXT: &str = "text/plain; charset=utf-8";
/// A cover fetch ends inside the request's own limit.
const COVER_FETCH_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_COVER_BYTES: usize = 16 * 1024 * 1024;

type Assets = &'static [(&'static str, &'static [u8])];

/// Every file in `gui/dist`, embedded by build.rs.
pub(crate) static ASSETS: Assets = include!(concat!(env!("OUT_DIR"), "/gui_assets.rs"));

struct Server {
  port: u16,
  access: Arc<Access>,
  assets: Assets,
  link: Link,
  covers: reqwest::Client,
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
    covers: reqwest::Client::builder()
      .timeout(COVER_FETCH_TIMEOUT)
      .build()
      .context("could not build the cover client")?,
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
      // Another local user can read the launch URL from the browser's command line.
      #[cfg(target_os = "linux")]
      if !same_user(peer.port(), port) {
        log::warn!("GUI: refusing a loopback connection from another user");
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
  let upgrade = tokio::time::timeout(REQUEST_TIMEOUT, answer(&mut stream, server))
    .await
    .context("request timed out")??;
  match upgrade {
    Some(raw) => open_socket(stream, raw, server).await,
    None => Ok(()),
  }
}

/// Answers a plain request; returns the head read so far when it asks for the socket.
async fn answer(stream: &mut TcpStream, server: &Server) -> Result<Option<Vec<u8>>> {
  let (head, raw) = read_head(stream).await?;
  let port = server.port;
  if !is_allowed(&head, port) {
    respond(stream, port, "403 Forbidden", TEXT, b"forbidden\n").await?;
    return Ok(None);
  }
  if head.method != "GET" {
    respond(
      stream,
      port,
      "405 Method Not Allowed",
      TEXT,
      b"method not allowed\n",
    )
    .await?;
    return Ok(None);
  }
  if head.upgrade {
    return Ok(Some(raw));
  }
  let path = head.path.split('?').next().unwrap_or_default();
  if path == "/cover" {
    cover(stream, server, &head.path).await?;
    return Ok(None);
  }
  let name = if path == "/" {
    "index.html"
  } else {
    path.trim_start_matches('/')
  };
  match server.assets.iter().find(|(asset, _)| *asset == name) {
    Some((_, body)) => respond(stream, port, "200 OK", content_type(name), body).await?,
    None if name == "index.html" => {
      respond(
        stream,
        port,
        "503 Service Unavailable",
        TEXT,
        server.link.strings().await.gui.missing_frontend.as_bytes(),
      )
      .await?
    }
    None => respond(stream, port, "404 Not Found", TEXT, b"not found\n").await?,
  }
  Ok(None)
}

/// The playing item's cover, so the page can read its colours; only with the session token.
async fn cover(stream: &mut TcpStream, server: &Server, target: &str) -> Result<()> {
  let port = server.port;
  let query = target
    .split_once('?')
    .map(|(_, query)| query)
    .unwrap_or_default();
  let (mut uri, mut token) = (None, None);
  for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
    match &*key {
      "uri" => uri = Some(value.into_owned()),
      "t" => token = Some(value.into_owned()),
      _ => {}
    }
  }
  if !token.is_some_and(|token| tokens_match(&token, &server.access.token)) {
    return respond(stream, port, "403 Forbidden", TEXT, b"forbidden\n").await;
  }
  let source = match uri {
    Some(uri) => server.link.cover_source(&uri).await,
    None => None,
  };
  let picture = match source {
    Some(source) => cover_bytes(&server.covers, source).await,
    None => None,
  };
  match picture
    .as_deref()
    .and_then(|bytes| Some((image_type(bytes)?, bytes)))
  {
    Some((kind, bytes)) => respond(stream, port, "200 OK", kind, bytes).await,
    None => respond(stream, port, "404 Not Found", TEXT, b"not found\n").await,
  }
}

async fn cover_bytes(client: &reqwest::Client, source: CoverSource) -> Option<Vec<u8>> {
  match source {
    CoverSource::Url(url) => {
      let mut response = client
        .get(&url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
      let mut body = Vec::new();
      while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > MAX_COVER_BYTES - body.len() {
          return None;
        }
        body.extend_from_slice(&chunk);
      }
      Some(body)
    }
    #[cfg(feature = "local-files")]
    CoverSource::File(path) => tokio::task::spawn_blocking(move || {
      crate::infra::local::embedded_cover_bytes(&path, MAX_COVER_BYTES)
    })
    .await
    .ok()?
    .ok(),
  }
}

/// The image type its first bytes name; any other body is refused.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
  match bytes {
    [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
    [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
    [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
    [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
    _ => None,
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

/// Whether a loopback peer runs as this process's user, from the kernel's socket table.
#[cfg(target_os = "linux")]
fn same_user(peer_port: u16, local_port: u16) -> bool {
  use std::os::unix::fs::MetadataExt;
  let own = std::fs::metadata("/proc/self").map(|meta| meta.uid());
  let table = std::fs::read_to_string("/proc/net/tcp");
  matches!((own, table), (Ok(own), Ok(table)) if peer_uid(&table, peer_port, local_port) == Some(own))
}

/// The uid of the socket in /proc/net/tcp from peer_port to local_port.
#[cfg(target_os = "linux")]
fn peer_uid(table: &str, peer_port: u16, local_port: u16) -> Option<u32> {
  let port = |address: &str| {
    address
      .rsplit_once(':')
      .and_then(|(_, port)| u16::from_str_radix(port, 16).ok())
  };
  table.lines().skip(1).find_map(|line| {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let matches = port(fields.get(1)?)? == peer_port && port(fields.get(2)?)? == local_port;
    matches.then(|| fields.get(7)?.parse().ok())?
  })
}

// The callback's error type is tungstenite's `ErrorResponse`, large by its API.
#[allow(clippy::result_large_err)]
async fn open_socket(stream: TcpStream, raw: Vec<u8>, server: &Server) -> Result<()> {
  let (read_half, write_half) = stream.into_split();
  let replay = tokio::io::join(Cursor::new(raw).chain(read_half), write_half);
  let mut admission = None;
  let handshake =
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
    });
  let socket = tokio::time::timeout(REQUEST_TIMEOUT, handshake)
    .await
    .context("handshake timed out")??;
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
    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; img-src 'self' https: http:; connect-src ws://127.0.0.1:{port} ws://localhost:{port}; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
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
    Some("woff2") => "font/woff2",
    Some("woff") => "font/woff",
    _ => "application/octet-stream",
  }
}

#[cfg(test)]
mod tests {

  #[tokio::test]
  async fn a_missing_frontend_uses_the_saved_language_before_boot() {
    use crate::core::i18n::Language;
    for language in [Language::En, Language::ZhHans, Language::ZhHant] {
      let (_boot, apps) = tokio::sync::watch::channel(None);
      let onboarding = Arc::new(crate::gui::onboarding::BrowserOnboarding::with_language(
        language,
      ));
      let (link, _publisher, _inbox) = bridge::channel(apps, onboarding, false);
      let (port, _access) = spawn_listener(&[], link).await.unwrap();
      let reply = get(
        port,
        format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
      )
      .await;
      assert!(reply.starts_with("HTTP/1.1 503"));
      assert!(reply.ends_with(language.strings().gui.missing_frontend));
      assert!(reply.contains("`npm ci`"));
    }
  }
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
    let (link, _publisher, inbox) = bridge::tests::booted(Arc::new(tokio::sync::Mutex::new(app)));
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

  const PLAYING_URI: &str = "spotify:track:4uLU6hMCjMI75M1A2tKUQC";
  const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnot really a png";

  /// A server whose app plays a track with art at `image_url`, and its session token.
  async fn start_playing(image_url: &str) -> (u16, String) {
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.current_playback_context = Some(crate::core::test_helpers::playing_with_art(
      "4uLU6hMCjMI75M1A2tKUQC",
      image_url,
    ));
    app.note_display_changes();
    let (link, _publisher, _inbox) = bridge::tests::booted(Arc::new(tokio::sync::Mutex::new(app)));
    let (port, access) = spawn_listener(PAGE, link).await.unwrap();
    (port, access.token.clone())
  }

  /// Serves one request with `content_type` and `body`, at the URL it returns.
  async fn upstream(content_type: &'static str, body: &'static [u8]) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
      let (mut stream, _) = listener.accept().await.unwrap();
      let _ = read_head(&mut stream).await;
      let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
      );
      stream.write_all(head.as_bytes()).await.unwrap();
      stream.write_all(body).await.unwrap();
    });
    format!("http://127.0.0.1:{port}/cover.png")
  }

  async fn get_cover(port: u16, query: &str) -> String {
    get(
      port,
      format!(
        "GET /cover?{query} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nSec-Fetch-Site: same-origin\r\n\r\n"
      ),
    )
    .await
  }

  #[tokio::test]
  async fn a_cover_needs_the_session_token() {
    let (port, _token) = start_playing("http://127.0.0.1:9/cover.png").await;
    let missing = get_cover(port, &format!("uri={PLAYING_URI}")).await;
    assert!(missing.starts_with("HTTP/1.1 403"), "{missing}");
    let wrong = get_cover(port, &format!("uri={PLAYING_URI}&t=guess")).await;
    assert!(wrong.starts_with("HTTP/1.1 403"), "{wrong}");
  }

  #[tokio::test]
  async fn a_cover_of_an_item_that_is_not_playing_is_not_found() {
    let (port, token) = start_playing("http://127.0.0.1:9/cover.png").await;
    let reply = get_cover(port, &format!("uri=spotify:track:other&t={token}")).await;
    assert!(reply.starts_with("HTTP/1.1 404"), "{reply}");
  }

  #[tokio::test]
  async fn the_playing_cover_passes_through_as_its_image_type() {
    let (port, token) = start_playing(&upstream("application/octet-stream", PNG).await).await;
    let reply = get_cover(port, &format!("uri={PLAYING_URI}&t={token}")).await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains("Content-Type: image/png\r\n"), "{reply}");
    assert!(reply.contains("Cache-Control: no-store\r\n"), "{reply}");
    assert!(reply.ends_with(&*String::from_utf8_lossy(PNG)), "{reply}");
  }

  #[tokio::test]
  async fn a_cover_that_is_not_an_image_is_not_served() {
    let (port, token) = start_playing(&upstream("image/png", b"<html>hi</html>").await).await;
    let reply = get_cover(port, &format!("uri={PLAYING_URI}&t={token}")).await;
    assert!(reply.starts_with("HTTP/1.1 404"), "{reply}");
  }

  #[tokio::test]
  async fn a_cross_site_cover_request_is_refused() {
    let (port, token) = start_playing("http://127.0.0.1:9/cover.png").await;
    let reply = get(
      port,
      format!(
        "GET /cover?uri={PLAYING_URI}&t={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nSec-Fetch-Site: cross-site\r\n\r\n"
      ),
    )
    .await;
    assert!(reply.starts_with("HTTP/1.1 403"), "{reply}");
  }

  #[test]
  fn only_jpeg_png_gif_and_webp_are_images() {
    assert_eq!(image_type(b"\xFF\xD8\xFF\xE0rest"), Some("image/jpeg"));
    assert_eq!(image_type(PNG), Some("image/png"));
    assert_eq!(image_type(b"GIF89a"), Some("image/gif"));
    assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    assert_eq!(image_type(b"<svg/>"), None);
    assert_eq!(image_type(b""), None);
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
    // DNS rebinding: a foreign name that resolves to 127.0.0.1, with its own origin.
    for upgrade in [false, true] {
      assert!(!is_allowed(
        &head(
          "attacker.example:9",
          Some("http://attacker.example:9"),
          None,
          upgrade
        ),
        9
      ));
    }
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

  #[test]
  fn bundled_fonts_are_served_with_a_font_type() {
    let font = "assets/hanken-grotesk-latin-400-normal-abc123";
    assert_eq!(content_type(&format!("{font}.woff2")), "font/woff2");
    assert_eq!(content_type(&format!("{font}.woff")), "font/woff");
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
      covers: reqwest::Client::new(),
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
    for _ in 0..28 {
      values.push(next_json(&mut socket).await);
    }
    let kinds: Vec<_> = values.iter().map(|value| value["kind"].clone()).collect();
    assert_eq!(
      kinds,
      [
        "hello",
        "onboarding",
        "route",
        "status",
        "source",
        "theme",
        "playback",
        "party",
        "devices",
        "search",
        "lyrics",
        "artist",
        "library",
        "liked",
        "queue",
        "stats",
        "album",
        "session",
        "discover",
        "playlist_sync",
        "track_table",
        "recently_played",
        "liked_marks",
        "settings",
        "update",
        "show",
        "announcement",
        "welcome"
      ]
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
  #[tokio::test]
  async fn a_request_that_never_finishes_its_head_times_out() {
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
      covers: reqwest::Client::new(),
    };
    let client = tokio::spawn(async move {
      let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
      stream.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
      stream
    });
    let (stream, _) = listener.accept().await.unwrap();

    let result = serve_connection(stream, &server).await;

    assert!(result.unwrap_err().to_string().contains("timed out"));
    drop(client);
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn the_socket_table_names_the_user_of_a_connection() {
    let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:D431 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 12345 1\n";

    assert_eq!(peer_uid(table, 0xD431, 0x1F90), Some(1000));
    assert_eq!(peer_uid(table, 0x1F90, 0xD431), None);
  }

  #[cfg(target_os = "linux")]
  #[tokio::test]
  async fn a_connection_from_this_user_is_recognized() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let (_stream, peer) = listener.accept().await.unwrap();

    assert!(same_user(peer.port(), port));
  }
  /// A server whose app has not booted yet, and a page connected to it.
  async fn before_boot() -> (
    WebSocketStream<TcpStream>,
    Arc<crate::gui::onboarding::BrowserOnboarding>,
    tokio::sync::watch::Sender<Option<Arc<tokio::sync::Mutex<App>>>>,
    u16,
    Arc<Access>,
  ) {
    let onboarding = Arc::new(crate::gui::onboarding::BrowserOnboarding::new());
    let (boot, apps) = tokio::sync::watch::channel(None);
    let (link, _publisher, _inbox) = bridge::channel(apps, Arc::clone(&onboarding), false);
    let (port, access) = spawn_listener(PAGE, link).await.unwrap();
    let mut page = open_page(port, &access).await;
    let first = next_json(&mut page).await;
    assert_eq!(first["kind"], "onboarding");
    assert!(first["payload"]["pending"].is_null());
    (page, onboarding, boot, port, access)
  }

  /// A page opened with a fresh launch code, past its hello.
  async fn open_page(port: u16, access: &Access) -> WebSocketStream<TcpStream> {
    let code = access.issue(Instant::now()).unwrap();
    let mut page = connect(
      port,
      &format!("http://127.0.0.1:{port}"),
      Some(&format!("{CODE_PROTOCOL}{code}")),
    )
    .await
    .unwrap();
    assert_eq!(next_json(&mut page).await["kind"], "hello");
    page
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn a_page_connected_before_boot_answers_a_prompt_over_the_socket() {
    let (mut first, onboarding, _boot, port, access) = before_boot().await;

    let asking = {
      let onboarding = Arc::clone(&onboarding);
      tokio::task::spawn_blocking(move || {
        use crate::core::onboarding::Onboarding;
        onboarding.prompt_line("Client ID: ")
      })
    };
    let question = next_json(&mut first).await;
    assert_eq!(
      question["payload"]["pending"]["ask"],
      serde_json::json!({ "kind": "Line", "prompt": "Client ID: ", "masked": false })
    );
    // A second page sees the open question and may answer it.
    let mut second = open_page(port, &access).await;
    let replayed = next_json(&mut second).await;
    assert_eq!(
      replayed["payload"]["pending"]["seq"],
      question["payload"]["pending"]["seq"]
    );
    let reply = serde_json::json!({
      "type": "onboarding",
      "reply": { "seq": question["payload"]["pending"]["seq"], "answer": { "kind": "Line", "text": "abc" } }
    });
    second
      .send(WsMessage::text(reply.to_string()))
      .await
      .unwrap();

    assert_eq!(asking.await.unwrap().unwrap(), "abc\n");
  }

  #[tokio::test]
  async fn publishing_the_app_resyncs_a_page_connected_before_boot() {
    let (mut page, _onboarding, boot, _port, _access) = before_boot().await;
    let (tx, _rx) = std::sync::mpsc::channel();
    let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));

    boot.send_replace(Some(Arc::new(tokio::sync::Mutex::new(app))));

    let mut kinds = Vec::new();
    for _ in 0..26 {
      kinds.push(next_json(&mut page).await["kind"].clone());
    }
    assert_eq!(
      kinds,
      [
        "route",
        "status",
        "source",
        "theme",
        "playback",
        "party",
        "devices",
        "search",
        "lyrics",
        "artist",
        "library",
        "liked",
        "queue",
        "stats",
        "album",
        "session",
        "discover",
        "playlist_sync",
        "track_table",
        "recently_played",
        "liked_marks",
        "settings",
        "update",
        "show",
        "announcement",
        "welcome"
      ]
    );
    assert!(
      tokio::time::timeout(Duration::from_millis(300), page.next())
        .await
        .is_err(),
      "a second resync arrived"
    );
  }
}
