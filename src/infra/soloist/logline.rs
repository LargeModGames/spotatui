//! Soloist's stdout: the facts spotatui reads from it, and redaction.
//!
//! A line looks like `09:50:11.277 I [/f:462   ] ready, device_name=..`, with an
//! optional `YYYY-MM-DD HH:MM:SS.mmm: ` prefix (spotify/soloist#18).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEvent {
  Ready { device_id: String },
  WsListening { port: u16 },
  WaitingForLogin,
  LoggedIn,
  LoginFailed,
  ExpiresIn { count: u32, unit: String },
  Expired,
  AnotherSession,
  Codec(String),
  SampleRate(u32),
  AverageBitrate(u32),
}

/// The message part of a line, after the time, level and source field.
pub fn message(line: &str) -> &str {
  match line.find("] ") {
    Some(end) if line[..end].contains('[') => &line[end + 2..],
    _ => line,
  }
}

fn number_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
  let start = text.find(marker)? + marker.len();
  let rest = text[start..].trim_start();
  let end = rest
    .find(|c: char| !c.is_ascii_digit())
    .unwrap_or(rest.len());
  (end > 0).then(|| &rest[..end])
}

pub fn parse_line(line: &str) -> Option<LogEvent> {
  let text = message(line);
  let trimmed = text.trim();
  if let Some(start) = text.find("device_id=") {
    if text.contains("ready, device_name=") {
      let id = text[start + "device_id=".len()..]
        .split_whitespace()
        .next()?;
      return Some(LogEvent::Ready {
        device_id: id.to_string(),
      });
    }
  }
  if let Some(start) = text.find("websocket server listening on ") {
    let address = text[start + "websocket server listening on ".len()..].trim();
    let port = address.rsplit(':').next()?.parse().ok()?;
    return Some(LogEvent::WsListening { port });
  }
  if text.contains("waiting for login") {
    return Some(LogEvent::WaitingForLogin);
  }
  if text.contains("logged in as ") {
    return Some(LogEvent::LoggedIn);
  }
  if text.contains("login failed") {
    return Some(LogEvent::LoginFailed);
  }
  if let Some(count) = number_after(text, "client expires in ") {
    let after = &text[text.find(count)? + count.len()..];
    return Some(LogEvent::ExpiresIn {
      count: count.parse().ok()?,
      unit: after
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string(),
    });
  }
  if text.contains("client expired") || text.contains("build expired") {
    return Some(LogEvent::Expired);
  }
  if text.contains("another session is running") {
    return Some(LogEvent::AnotherSession);
  }
  if let Some(codec) = trimmed.strip_prefix("codec: ") {
    return Some(LogEvent::Codec(codec.trim().to_ascii_lowercase()));
  }
  if trimmed.starts_with("sample rate: ") {
    return number_after(trimmed, "sample rate: ")?
      .parse()
      .ok()
      .map(LogEvent::SampleRate);
  }
  if let Some(bitrate) = number_after(text, "FileStreamer file average bitrate: ") {
    return bitrate.parse().ok().map(LogEvent::AverageBitrate);
  }
  None
}

/// Remove ANSI escape sequences (`ESC [ ... letter`).
pub fn strip_ansi(line: &str) -> String {
  let mut out = String::with_capacity(line.len());
  let mut chars = line.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '\u{1b}' {
      if chars.peek() == Some(&'[') {
        chars.next();
        for next in chars.by_ref() {
          if next.is_ascii_alphabetic() {
            break;
          }
        }
      }
      continue;
    }
    out.push(c);
  }
  out
}

fn mask_after(line: &mut String, marker: &str, stop: impl Fn(char) -> bool) {
  let mut from = 0;
  while let Some(found) = line[from..].find(marker) {
    let start = from + found + marker.len();
    let end = line[start..]
      .find(&stop)
      .map_or(line.len(), |end| start + end);
    from = if end > start {
      line.replace_range(start..end, "***");
      start + 3
    } else {
      start
    };
  }
}

/// The line without the API key, the account name and URL query strings.
pub fn redact(line: &str, api_key: &str) -> String {
  let mut out = if api_key.is_empty() {
    line.to_string()
  } else {
    line.replace(api_key, "***")
  };
  mask_after(&mut out, "session:", |c| c == ']' || c.is_whitespace());
  mask_after(&mut out, "logged in as ", char::is_whitespace);
  mask_after(&mut out, "Users\\", |c| c == '\\' || c.is_whitespace());
  mask_after(&mut out, "Users/", |c| c == '/' || c.is_whitespace());
  for scheme in ["http://", "https://"] {
    let mut from = 0;
    while let Some(found) = out[from..].find(scheme) {
      let start = from + found;
      let end = out[start..]
        .find(char::is_whitespace)
        .map_or(out.len(), |end| start + end);
      if let Some(query) = out[start..end].find('?') {
        out.replace_range(start + query..end, "?***");
      }
      from = (start + scheme.len()).min(out.len());
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_line_reads_the_ready_line_device_id() {
    let line = "09:50:11.277 I [/f:462                          ] ready, device_name=spotatui device_id=70623e1b-ddb7-411c-8fb7-06be482a580e";
    assert_eq!(
      parse_line(line),
      Some(LogEvent::Ready {
        device_id: "70623e1b-ddb7-411c-8fb7-06be482a580e".to_string()
      })
    );
  }

  #[test]
  fn parse_line_reads_the_websocket_port() {
    let line = "09:50:11.277 I [/f:1021                         ] api: websocket server listening on 127.0.0.1:54321";
    assert_eq!(
      parse_line(line),
      Some(LogEvent::WsListening { port: 54321 })
    );
  }

  #[test]
  fn parse_line_spots_waiting_for_login_with_the_dash() {
    let line = "09:43:32.275 I [/f:475                          ] waiting for login \u{2014} connect to \"spotatui\" from your Spotify app";
    assert_eq!(parse_line(line), Some(LogEvent::WaitingForLogin));
  }

  #[test]
  fn parse_line_strips_the_timestamp_prefix() {
    let line = "2026-10-25 09:50:11.537: 09:50:11.537 I [/f:415                          ] logged in as someone";
    assert_eq!(parse_line(line), Some(LogEvent::LoggedIn));
  }

  #[test]
  fn parse_line_spots_login_failed_and_another_session() {
    assert_eq!(
      parse_line("E [/f:1 ] login failed: make sure --api-key is valid"),
      Some(LogEvent::LoginFailed)
    );
    assert_eq!(
      parse_line("E [/f:1 ] another session is running for data directory"),
      Some(LogEvent::AnotherSession)
    );
  }

  #[test]
  fn parse_line_reads_expiry_count_and_unit() {
    let line = "09:50:11.221 I [/f:128                          ] client expires in 88 days";
    assert_eq!(
      parse_line(line),
      Some(LogEvent::ExpiresIn {
        count: 88,
        unit: "days".to_string()
      })
    );
  }

  #[test]
  fn parse_line_spots_both_expired_texts() {
    assert_eq!(
      parse_line(
        "E [/f:1 ] client expired, please update to a newer version. Exiting with exit code 10"
      ),
      Some(LogEvent::Expired)
    );
    assert_eq!(
      parse_line("Spotify Soloist build expired"),
      Some(LogEvent::Expired)
    );
  }

  #[test]
  fn parse_line_reads_codec_sample_rate_and_average_bitrate() {
    assert_eq!(
      parse_line("09:50:21.819 I [/f:347                          ]     codec: vorbis"),
      Some(LogEvent::Codec("vorbis".to_string()))
    );
    assert_eq!(
      parse_line("09:50:21.819 I [/f:349                          ]     sample rate: 44100 Hz"),
      Some(LogEvent::SampleRate(44100))
    );
    assert_eq!(
      parse_line("09:50:21.816 I [/f:134                          ] FileStreamer file average bitrate: 160000"),
      Some(LogEvent::AverageBitrate(160000))
    );
  }

  #[test]
  fn parse_line_ignores_unknown_lines() {
    assert_eq!(
      parse_line("09:50:11.468 I [/f:98                           ] DSP CPU features: SSE1"),
      None
    );
  }

  #[test]
  fn redact_removes_the_key_from_a_line() {
    assert_eq!(redact("args -k abc123 -n x", "abc123"), "args -k *** -n x");
  }

  #[test]
  fn redact_masks_the_account_name_and_url_queries() {
    assert_eq!(
      redact(
        "Session restored from store, session: [session:someone]",
        ""
      ),
      "Session restored from store, session: [session:***]"
    );
    assert_eq!(redact("logged in as someone", ""), "logged in as ***");
    assert_eq!(
      redact(r"open C:\soloist\settings\Users\someone\prefs", ""),
      r"open C:\soloist\settings\Users\***\prefs"
    );
    assert_eq!(
      redact("GET https://host/path?token=abc done", ""),
      "GET https://host/path?*** done"
    );
  }

  #[test]
  fn strip_ansi_removes_color_codes() {
    assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m text"), "red text");
  }
}
