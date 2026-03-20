use std::{
  io::prelude::*,
  net::{TcpListener, TcpStream},
};

pub fn redirect_uri_web_server(port: u16) -> Result<String, ()> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", port));

  match listener {
    Ok(listener) => {
      for stream in listener.incoming() {
        match stream {
          Ok(stream) => {
            if let Some(url) = handle_connection(stream) {
              return Ok(url);
            }
          }
          Err(e) => {
            println!("Error: {}", e);
          }
        };
      }
    }
    Err(e) => {
      println!("Error: {}", e);
    }
  }

  Err(())
}

fn handle_connection(mut stream: TcpStream) -> Option<String> {
  // The request will be quite large (> 512) so just assign plenty just in case
  let mut buffer = [0; 1000];
  let _ = stream.read(&mut buffer).unwrap();

  // convert buffer into string and 'parse' the URL
  match String::from_utf8(buffer.to_vec()) {
    Ok(request) => {
      let split: Vec<&str> = request.split_whitespace().collect();

      if split.len() > 1 {
        // Extract the path from the HTTP request (e.g., "/callback?code=...&state=...")
        let path = split[1];

        // Only accept requests that contain a `code` query parameter — the OAuth callback.
        // Ignore browser noise like /favicon.ico, pre-flight requests, etc.
        if !path.contains("code=") {
          send_error_response("Not a callback request".to_string(), stream);
          return None;
        }

        // Parse the host header to build the full URL
        let host = request
          .lines()
          .find(|line| line.to_lowercase().starts_with("host:"))
          .and_then(|line| line.split(':').nth(1))
          .map(|h| h.trim())
          .unwrap_or("127.0.0.1:8888");

        // Construct the full URL
        let full_url = format!("http://{}{}", host, path);

        respond_with_success(stream);
        return Some(full_url);
      }

      // Malformed HTTP is normal browser pre-flight — send 400 silently,
      // the loop will continue waiting for the real OAuth callback.
      send_error_response("Malformed request".to_string(), stream);
    }
    Err(e) => {
      let msg = format!("Invalid UTF-8 sequence: {}", e);
      println!("Error: {}", msg);
      send_error_response(msg, stream);
    }
  };

  None
}

fn respond_with_success(mut stream: TcpStream) {
  let contents = include_str!("redirect_uri.html");

  let response = format!(
    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    contents.len(),
    contents
  );

  stream.write_all(response.as_bytes()).unwrap();
  stream.flush().unwrap();
  // Give the browser time to receive the response before closing
  std::thread::sleep(std::time::Duration::from_millis(100));
}

fn send_error_response(error_message: String, mut stream: TcpStream) {
  let body = format!("400 - Bad Request - {}", error_message);
  let response = format!(
    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    body.len(),
    body
  );

  let _ = stream.write_all(response.as_bytes());
  let _ = stream.flush();
  std::thread::sleep(std::time::Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
  use super::*;

  fn send_to_handle_connection(request: &[u8]) -> Option<String> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let request = request.to_vec();
    let writer_thread = std::thread::spawn(move || {
      let mut client = TcpStream::connect(addr).unwrap();
      client.write_all(&request).unwrap();
      // Read and discard the response so handle_connection's write doesn't block
      let mut buf = Vec::new();
      let _ = client.read_to_end(&mut buf);
    });

    let (server_side, _) = listener.accept().unwrap();
    let result = handle_connection(server_side);
    writer_thread.join().unwrap();
    result
  }

  #[test]
  fn valid_callback_returns_url_with_code() {
    let request = b"GET /login?code=abc&state=xyz HTTP/1.1\r\nHost: 127.0.0.1:8989\r\n\r\n";
    let url = send_to_handle_connection(request);
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
  fn whitespace_only_request_returns_none_without_printing() {
    // Whitespace-only payload: split_whitespace() returns empty vec (len 0 ≤ 1) → None silently
    let result = send_to_handle_connection(b" \r\n\r\n");
    assert!(result.is_none());
  }

  #[test]
  fn preflight_single_token_returns_none() {
    // A single token (no path) also triggers the malformed branch → None, no panic
    let result = send_to_handle_connection(b"GET");
    assert!(result.is_none());
  }

  #[test]
  fn favicon_request_returns_none() {
    let request = b"GET /favicon.ico HTTP/1.1\r\nHost: 127.0.0.1:8989\r\n\r\n";
    let result = send_to_handle_connection(request);
    assert!(result.is_none());
  }

  #[test]
  fn root_request_returns_none() {
    let request = b"GET / HTTP/1.1\r\nHost: 127.0.0.1:8989\r\n\r\n";
    let result = send_to_handle_connection(request);
    assert!(result.is_none());
  }
}
