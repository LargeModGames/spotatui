//! Segment download: fetch, decrypt, and write the track as segments arrive.
//!
//! Two phases so the caller can read the total size before the long part:
//! [`begin`] fetches segment 0 and writes the codec header, then
//! [`TrackDownload::finish`] streams every audio segment into the same file.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;

use super::cmaf::{self, InitSegment};

const SEGMENT_PLACEHOLDER: &str = "$SEGMENT$";

/// A track download after segment 0 was parsed and the header was written.
pub struct TrackDownload {
  http: Client,
  url_template: String,
  content_key: [u8; 16],
  init: InitSegment,
  file: File,
}

/// Fetch and parse segment 0 and write the codec header to `dest`.
pub async fn begin(
  http: &Client,
  url_template: &str,
  content_key: [u8; 16],
  dest: &Path,
) -> Result<TrackDownload> {
  if !url_template.contains(SEGMENT_PLACEHOLDER) {
    return Err(anyhow!("stream URL template has no {SEGMENT_PLACEHOLDER}"));
  }
  let bytes = fetch_segment(http, url_template, 0).await?;
  let init = cmaf::parse_init(&bytes).context("segment 0 parse")?;
  let mut file =
    File::create(dest).with_context(|| format!("creating stream file {}", dest.display()))?;
  file
    .write_all(&init.header)
    .with_context(|| format!("writing stream to {}", dest.display()))?;
  Ok(TrackDownload {
    http: http.clone(),
    url_template: url_template.to_string(),
    content_key,
    init,
    file,
  })
}

impl TrackDownload {
  /// The size of the finished file, from the init table.
  pub fn total_bytes(&self) -> u64 {
    self.init.total_bytes()
  }

  /// Fetch, decrypt and append every audio segment, then flush the file.
  pub async fn finish(mut self) -> Result<()> {
    let count = self.init.segment_lengths.len() as u32;
    for index in 1..=count {
      let mut bytes = fetch_segment(&self.http, &self.url_template, index).await?;
      let table = cmaf::parse_segment(&bytes).with_context(|| format!("segment {index} parse"))?;
      let range = cmaf::decrypt_frames(&mut bytes, &table, &self.content_key)
        .with_context(|| format!("segment {index} decrypt"))?;
      self
        .file
        .write_all(&bytes[range])
        .with_context(|| format!("segment {index} write"))?;
    }
    self.file.flush().context("flushing stream file")
  }
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

#[cfg(test)]
mod tests {
  use super::super::cmaf::fixtures;
  use super::*;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};

  const KEY: [u8; 16] = [0x5a; 16];

  /// Serve `segments[i]` at `/seg/i`; returns the URL template.
  async fn serve(segments: Vec<Vec<u8>>) -> String {
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

  fn fixture_track() -> (Vec<Vec<u8>>, Vec<u8>) {
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

  #[tokio::test]
  async fn download_rebuilds_header_and_decrypted_frames() {
    let (segments, expected) = fixture_track();
    let template = serve(segments).await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let download = begin(&Client::new(), &template, KEY, tmp.path())
      .await
      .unwrap();
    assert_eq!(download.total_bytes(), 42 + 70 + 20);
    download.finish().await.unwrap();
    assert_eq!(std::fs::read(tmp.path()).unwrap(), expected);
  }

  #[tokio::test]
  async fn missing_segment_fails_the_download() {
    let (mut segments, _) = fixture_track();
    segments.pop();
    let template = serve(segments).await;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let download = begin(&Client::new(), &template, KEY, tmp.path())
      .await
      .unwrap();
    let err = download.finish().await.unwrap_err().to_string();
    assert!(err.contains("segment 2"), "{err}");
  }

  #[tokio::test]
  async fn template_without_placeholder_is_rejected() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    assert!(
      begin(&Client::new(), "http://127.0.0.1:1/x", KEY, tmp.path())
        .await
        .is_err()
    );
  }

  #[test]
  fn segment_url_substitutes_the_index() {
    assert_eq!(
      segment_url("https://h/t/$SEGMENT$.m4s?x=1", 12),
      "https://h/t/12.m4s?x=1"
    );
  }
}
