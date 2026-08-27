//! Segment download: fetch, parse, and decrypt one segment at a time. The
//! progressive stream (`progressive`) drives it and hands each decrypted
//! segment to `stream-download`.

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use reqwest::Client;

use super::cmaf::{self, InitSegment};

const SEGMENT_PLACEHOLDER: &str = "$SEGMENT$";

/// Fetch and parse segment 0.
pub async fn fetch_init(http: &Client, url_template: &str) -> Result<InitSegment> {
  if !url_template.contains(SEGMENT_PLACEHOLDER) {
    return Err(anyhow!("stream URL template has no {SEGMENT_PLACEHOLDER}"));
  }
  let bytes = fetch_segment(http, url_template, 0).await?;
  cmaf::parse_init(&bytes).context("segment 0 parse")
}

/// Fetch audio segment `index` (1 or later) and return its decrypted frames.
pub(super) async fn fetch_audio_segment(
  http: &Client,
  url_template: &str,
  index: u32,
  content_key: &[u8; 16],
) -> Result<Bytes> {
  let mut bytes = fetch_segment(http, url_template, index).await?;
  let table = cmaf::parse_segment(&bytes).with_context(|| format!("segment {index} parse"))?;
  let range = cmaf::decrypt_frames(&mut bytes, &table, content_key)
    .with_context(|| format!("segment {index} decrypt"))?;
  Ok(Bytes::from(bytes).slice(range))
}

fn segment_url(template: &str, index: u32) -> String {
  template.replace(SEGMENT_PLACEHOLDER, &index.to_string())
}

async fn fetch_segment(http: &Client, template: &str, index: u32) -> Result<Vec<u8>> {
  let response = http
    .get(segment_url(template, index))
    .send()
    .await
    .with_context(|| format!("segment {index} request"))?;
  let status = response.status();
  if !status.is_success() {
    return Err(anyhow!("segment {index} returned HTTP {status}"));
  }
  Ok(
    response
      .bytes()
      .await
      .with_context(|| format!("segment {index} body"))?
      .to_vec(),
  )
}

/// A fake segment server and a two-segment track, shared with the
/// progressive-delivery tests.
#[cfg(test)]
pub(super) mod test_support {
  use super::super::cmaf::fixtures;
  use super::SEGMENT_PLACEHOLDER;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};

  pub const KEY: [u8; 16] = [0x5a; 16];

  /// Serve `segments[i]` at `/seg/i`; returns the URL template.
  pub async fn serve(segments: Vec<Vec<u8>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
      loop {
        let Ok((mut stream, _)) = listener.accept().await else {
          break;
        };
        let mut buf = vec![0u8; 2048];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request.split_whitespace().nth(1).unwrap_or("");
        let index: usize = path
          .rsplit('/')
          .next()
          .and_then(|s| s.parse().ok())
          .unwrap_or(usize::MAX);
        let (status, body) = match segments.get(index) {
          Some(b) => ("200 OK", b.clone()),
          None => ("404 Not Found", Vec::new()),
        };
        let head = format!(
          "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
          body.len()
        );
        let _ = stream.write_all(head.as_bytes()).await;
        let _ = stream.write_all(&body).await;
        let _ = stream.shutdown().await;
      }
    });
    format!("http://127.0.0.1:{port}/seg/{SEGMENT_PLACEHOLDER}")
  }

  /// The segments of a track (init first) and the expected rebuilt file.
  pub fn fixture_track() -> (Vec<Vec<u8>>, Vec<u8>) {
    let (seg1, plain1) = fixtures::audio_segment(
      &[
        ((0u8..40).collect(), 1, [1, 2, 3, 4, 5, 6, 7, 8]),
        ((100u8..130).collect(), 0, [0; 8]),
      ],
      &KEY,
    );
    let (seg2, plain2) = fixtures::audio_segment(&[((200u8..220).collect(), 1, [9; 8])], &KEY);
    let init = fixtures::init_segment(&fixtures::flac_header(), &[70, 20]);
    let mut expected = fixtures::flac_header();
    expected[4] |= 0x80;
    expected.extend(plain1);
    expected.extend(plain2);
    (vec![init, seg1, seg2], expected)
  }
}

#[cfg(test)]
mod tests {
  use super::test_support::*;
  use super::*;

  #[tokio::test]
  async fn template_without_placeholder_is_rejected() {
    assert!(fetch_init(&Client::new(), "http://127.0.0.1:1/x")
      .await
      .is_err());
  }

  #[test]
  fn segment_url_substitutes_the_index() {
    assert_eq!(
      segment_url("https://h/t/$SEGMENT$.m4s?x=1", 12),
      "https://h/t/12.m4s?x=1"
    );
  }
}
