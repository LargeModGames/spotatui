use std::borrow::Cow;
use std::collections::VecDeque;

use anyhow::Result;
use futures::TryStreamExt;
use stream_download::http::reqwest::Url;
use tokio::time::{timeout_at, Instant};

use super::stream::{HttpRadioStream, RadioChunk, RadioOpenError};

const MAX_PLAYLIST_BYTES: usize = 64 * 1024;
const SNIFF_BYTES: usize = 1024;

pub(super) enum ResponseBody {
  Audio(VecDeque<RadioChunk>),
  Playlist(Url),
}

pub(super) async fn inspect(
  stream: &mut HttpRadioStream,
  content_type: Option<&str>,
  deadline: Instant,
  followed_playlist: bool,
) -> Result<ResponseBody> {
  let content_type = content_type.unwrap_or_default().to_ascii_lowercase();
  let playlist_type = matches!(
    content_type.as_str(),
    "audio/x-scpls"
      | "application/x-scpls"
      | "audio/mpegurl"
      | "audio/x-mpegurl"
      | "application/mpegurl"
      | "application/x-mpegurl"
      | "application/vnd.apple.mpegurl"
  );
  if !playlist_type && (content_type.starts_with("audio/") || content_type == "application/ogg") {
    return Ok(ResponseBody::Audio(VecDeque::new()));
  }

  let mut prefix = VecDeque::new();
  if !playlist_type {
    let sniff = async {
      let mut sample = Vec::new();
      while sample.len() < SNIFF_BYTES {
        let Some(bytes) = stream.try_next().await? else {
          break;
        };
        sample.extend_from_slice(&bytes[..bytes.len().min(SNIFF_BYTES - sample.len())]);
        prefix.push_back(bytes);
        if let Some(playlist) = sniff_playlist(&sample) {
          return Ok(playlist);
        }
      }
      Ok::<_, anyhow::Error>(false)
    };
    if !timeout_at(deadline, sniff)
      .await
      .map_err(|_| RadioOpenError::ProbeTimeout)??
    {
      return Ok(ResponseBody::Audio(prefix));
    }
  }

  let read = async {
    let mut body = Vec::new();
    loop {
      let bytes = match prefix.pop_front() {
        Some(bytes) => bytes,
        None => match stream
          .try_next()
          .await
          .map_err(|_| RadioOpenError::Playlist)?
        {
          Some(bytes) => bytes,
          None => break,
        },
      };
      if bytes.len() > MAX_PLAYLIST_BYTES - body.len() {
        return Err(RadioOpenError::Playlist);
      }
      body.extend_from_slice(&bytes);
    }
    resolve(&body, followed_playlist)
  };
  let url = timeout_at(deadline, read)
    .await
    .map_err(|_| RadioOpenError::Playlist)??;
  Ok(ResponseBody::Playlist(url))
}

fn text(bytes: &[u8], complete: bool) -> Option<Cow<'_, str>> {
  let (bytes, order) = match bytes {
    [0xff, 0xfe, bytes @ ..] => (bytes, u16::from_le_bytes as fn([u8; 2]) -> u16),
    [0xfe, 0xff, bytes @ ..] => (bytes, u16::from_be_bytes as fn([u8; 2]) -> u16),
    _ => {
      return Some(String::from_utf8_lossy(
        bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes),
      ))
    }
  };
  let (pairs, last) = bytes.as_chunks::<2>();
  let units = pairs.iter().copied().map(order);
  let decoded = char::decode_utf16(units);
  if complete {
    if !last.is_empty() {
      return None;
    }
    Some(Cow::Owned(decoded.collect::<Result<String, _>>().ok()?))
  } else {
    Some(Cow::Owned(
      decoded
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect(),
    ))
  }
}

fn sniff_playlist(bytes: &[u8]) -> Option<bool> {
  if [b"\xef\xbb\xbf".as_slice(), b"\xff\xfe", b"\xfe\xff"]
    .iter()
    .any(|bom| bom.starts_with(bytes))
  {
    return None;
  }
  let decoded = text(bytes, false).unwrap_or_default();
  let start = decoded.trim_start().to_ascii_lowercase();
  let starters = ["#", "[playlist]", "file", "https://", "http://"];
  if starters.iter().any(|starter| start.starts_with(starter)) {
    Some(true)
  } else if starters.iter().any(|starter| starter.starts_with(&start)) {
    None
  } else {
    Some(false)
  }
}

fn resolve(bytes: &[u8], followed_playlist: bool) -> Result<Url, RadioOpenError> {
  let decoded = text(bytes, true).ok_or(RadioOpenError::Playlist)?;
  let lines = decoded.split(['\r', '\n']).map(str::trim);
  if lines.clone().any(|line| line.starts_with("#EXT-X-")) {
    return Err(RadioOpenError::Hls);
  }
  if followed_playlist {
    return Err(RadioOpenError::Playlist);
  }
  let first_pls = lines
    .clone()
    .filter_map(file_entry)
    .min_by_key(|(index, _)| *index);
  first_pls
    .map(|(_, url)| url)
    .or_else(|| {
      lines
        .filter(|line| !line.starts_with('#'))
        .find_map(http_url)
    })
    .ok_or(RadioOpenError::Playlist)
}

fn file_entry(line: &str) -> Option<(u64, Url)> {
  let (key, url) = line.split_once('=')?;
  let key = key.trim();
  let suffix = key.get(4..)?;
  if !key[..4].eq_ignore_ascii_case("file") || !suffix.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  Some((suffix.parse().ok()?, http_url(url.trim())?))
}

fn http_url(line: &str) -> Option<Url> {
  let url = Url::parse(line).ok()?;
  matches!(url.scheme(), "http" | "https").then_some(url)
}
