//! Track lyrics from Spotify's color-lyrics endpoint.

use super::router::{retry_read, upstream_error};
use super::session;
use librespot_core::error::ErrorKind;
use librespot_core::SpotifyId;
use serde::Deserialize;

/// Lyrics lines as `(start ms, text)`; the start is `0` on every line when not synced.
#[derive(Debug, PartialEq)]
pub struct SpotifyLyrics {
  pub lines: Vec<(u128, String)>,
  pub synced: bool,
}

#[derive(Deserialize)]
struct Response {
  lyrics: Body,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
  sync_type: String,
  #[serde(default)]
  lines: Vec<Line>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
  start_time_ms: String,
  words: String,
}

/// The lyrics of one track, or `None` when Spotify has none for it.
pub async fn track_lyrics(base62: &str) -> anyhow::Result<Option<SpotifyLyrics>> {
  let id = SpotifyId::from_base62(base62)?;
  let session = session::current()
    .await
    .map_err(|reason| anyhow::anyhow!(reason.message()))?;
  let spclient = session.session().spclient();
  match retry_read(|| spclient.get_lyrics(&id)).await {
    Ok(bytes) => Ok(parse_lyrics(&bytes)),
    Err(error) if error.kind == ErrorKind::NotFound => Ok(None),
    Err(error) => Err(upstream_error(error)),
  }
}

fn parse_lyrics(bytes: &[u8]) -> Option<SpotifyLyrics> {
  let body = serde_json::from_slice::<Response>(bytes).ok()?.lyrics;
  let synced = body.sync_type == "LINE_SYNCED";
  let lines: Vec<(u128, String)> = body
    .lines
    .into_iter()
    .filter_map(|line| {
      let start = if synced {
        line.start_time_ms.parse().ok()?
      } else {
        0
      };
      Some((start, line.words))
    })
    .collect();
  (!lines.is_empty()).then_some(SpotifyLyrics { lines, synced })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn body(sync_type: &str, lines: &str) -> Vec<u8> {
    format!(r#"{{"lyrics":{{"syncType":"{sync_type}","lines":[{lines}],"provider":"MusixMatch"}},"colors":{{}}}}"#)
      .into_bytes()
  }

  #[test]
  fn line_synced_lyrics_keep_their_start_times() {
    let json = body(
      "LINE_SYNCED",
      r#"{"startTimeMs":"960","words":"One more time","syllables":[],"endTimeMs":"0"},
         {"startTimeMs":"4210","words":"♪","syllables":[],"endTimeMs":"0"}"#,
    );
    assert_eq!(
      parse_lyrics(&json),
      Some(SpotifyLyrics {
        lines: vec![(960, "One more time".into()), (4210, "♪".into())],
        synced: true,
      })
    );
  }

  #[test]
  fn unsynced_lyrics_are_marked_unsynced() {
    let json = body(
      "UNSYNCED",
      r#"{"startTimeMs":"0","words":"first"},{"startTimeMs":"0","words":"second"}"#,
    );
    let lyrics = parse_lyrics(&json).unwrap();
    assert!(!lyrics.synced);
    assert_eq!(
      lyrics.lines,
      vec![(0, "first".into()), (0, "second".into())]
    );
  }

  #[test]
  fn empty_lines_mean_no_lyrics() {
    assert_eq!(parse_lyrics(&body("LINE_SYNCED", "")), None);
    assert_eq!(parse_lyrics(b"not json"), None);
  }

  #[test]
  fn a_malformed_start_time_drops_only_that_line() {
    let json = body(
      "LINE_SYNCED",
      r#"{"startTimeMs":"x","words":"bad"},{"startTimeMs":"1500","words":"good"}"#,
    );
    assert_eq!(
      parse_lyrics(&json).unwrap().lines,
      vec![(1500, "good".into())]
    );
  }
}
