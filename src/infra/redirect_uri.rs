use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Acceptance logic for the callback server: given a raw HTTP request, return
/// the full callback URL when it carries an OAuth `code=` query parameter,
/// else `None` (browser noise like /favicon.ico, pre-flight requests, or
/// malformed input).
fn extract_callback_url(request: &str) -> Option<String> {
  let split: Vec<&str> = request.split_whitespace().collect();
  if split.len() <= 1 {
    return None;
  }
  // The path is the second whitespace-separated token, e.g. "/callback?code=...".
  let path = split[1];
  if !path.contains("code=") {
    return None;
  }

  let host = request
    .lines()
    .find(|line| line.to_lowercase().starts_with("host:"))
    .and_then(|line| line.split(':').nth(1))
    .map(|h| h.trim())
    .unwrap_or("127.0.0.1:8888");

  Some(format!("http://{}{}", host, path))
}

/// OAuth callback server, used by both the pre-TUI startup login and the
/// in-TUI login flow: it never blocks the caller's thread (the old blocking
/// variant parked a tokio worker in a std accept() loop with no timeout and
/// could hang the startup login entirely, #364). Returns the callback URL, or
/// `Err(())` on bind/accept failure. Callers that need an overall timeout
/// (e.g. the in-TUI flow) apply it via `tokio::time::timeout` so an abandoned
/// login doesn't leak the listener.
pub async fn redirect_uri_web_server_async(port: u16) -> Result<String, ()> {
  let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
    .await
    .map_err(|e| log::warn!("[login] failed to bind callback server on port {port}: {e}"))?;
  run_accept_loop(listener).await
}

/// Accept-loop extracted so tests can inject a pre-bound listener (port 0) and
/// avoid races caused by hard-coding a port that might already be in use.
async fn run_accept_loop(listener: tokio::net::TcpListener) -> Result<String, ()> {
  const MAX_CONSECUTIVE_ACCEPT_ERRORS: u8 = 20;
  let mut consecutive_accept_errors = 0u8;
  loop {
    let mut stream = match listener.accept().await {
      Ok((stream, _)) => {
        consecutive_accept_errors = 0;
        stream
      }
      Err(e) => {
        consecutive_accept_errors = consecutive_accept_errors.saturating_add(1);
        log::warn!("[login] callback accept error: {e}");
        if consecutive_accept_errors >= MAX_CONSECUTIVE_ACCEPT_ERRORS {
          return Err(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        continue;
      }
    };

    let mut buffer = vec![0u8; 1000];
    let n = match stream.read(&mut buffer).await {
      Ok(0) | Err(_) => continue,
      Ok(n) => n,
    };
    let request = String::from_utf8_lossy(&buffer[..n]);

    if let Some(url) = extract_callback_url(&request) {
      let _ = write_async_response(&mut stream, "200 OK", include_str!("redirect_uri.html")).await;
      return Ok(url);
    }

    // Browser noise (favicon, pre-flight): 400 and keep waiting for the callback.
    let _ = write_async_response(&mut stream, "400 Bad Request", "400 - Bad Request").await;
  }
}

async fn write_async_response(
  stream: &mut tokio::net::TcpStream,
  status: &str,
  body: &str,
) -> std::io::Result<()> {
  let response = format!(
    "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    status,
    body.len(),
    body
  );
  stream.write_all(response.as_bytes()).await?;
  stream.flush().await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn valid_callback_returns_url_with_code() {
    let request = "GET /login?code=abc&state=xyz HTTP/1.1\r\nHost: 127.0.0.1:8989\r\n\r\n";
    let url = extract_callback_url(request);
    assert!(url.is_some());
    let url = url.unwrap();
    assert!(
      url.contains("code=abc"),
      "url should contain code=abc, got: {}",
      url
    );
    assert!(
      url.contains("state=xyz"),
      "url should contain state=xyz, got: {}",
      url
    );
  }

  #[test]
  fn whitespace_only_request_returns_none() {
    // Whitespace-only payload: split_whitespace() returns empty vec (len 0 ≤ 1) → None silently
    assert!(extract_callback_url(" \r\n\r\n").is_none());
  }

  #[test]
  fn preflight_single_token_returns_none() {
    // A single token (no path) also triggers the malformed branch → None, no panic
    assert!(extract_callback_url("GET").is_none());
  }

  #[test]
  fn favicon_request_returns_none() {
    let request = "GET /favicon.ico HTTP/1.1\r\nHost: 127.0.0.1:8989\r\n\r\n";
    assert!(extract_callback_url(request).is_none());
  }

  #[test]
  fn root_request_returns_none() {
    let request = "GET / HTTP/1.1\r\nHost: 127.0.0.1:8989\r\n\r\n";
    assert!(extract_callback_url(request).is_none());
  }

  // --- async server tests --------------------------------------------------

  #[tokio::test]
  async fn async_server_returns_callback_url() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let client = tokio::spawn(async move {
      use tokio::io::{AsyncReadExt, AsyncWriteExt};
      let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
      let req =
        format!("GET /callback?code=testcode123 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
      stream.write_all(req.as_bytes()).await.unwrap();
      let mut buf = vec![0u8; 4096];
      let _ = stream.read(&mut buf).await;
    });

    let result = run_accept_loop(listener).await;
    client.await.unwrap();

    let url = result.expect("server should return Ok(url)");
    assert!(url.contains("code=testcode123"), "unexpected url: {url}");
  }

  #[tokio::test]
  async fn async_server_skips_noise_then_returns_on_real_callback() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let client = tokio::spawn(async move {
      use tokio::io::{AsyncReadExt, AsyncWriteExt};

      // First request: browser noise (favicon) — should get 400 and be ignored.
      let mut noise = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
      noise
        .write_all(
          format!("GET /favicon.ico HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap();
      let mut buf = vec![0u8; 4096];
      let _ = noise.read(&mut buf).await;
      drop(noise);

      // Second request: real OAuth callback.
      let mut real = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
      real
        .write_all(
          format!("GET /callback?code=realcode456 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
            .as_bytes(),
        )
        .await
        .unwrap();
      let _ = real.read(&mut vec![0u8; 4096]).await;
    });

    let result = run_accept_loop(listener).await;
    client.await.unwrap();

    let url = result.expect("server should return Ok(url)");
    assert!(url.contains("code=realcode456"), "unexpected url: {url}");
  }
}
