//! Serde types for `yt-dlp --flat-playlist --dump-json` output.

use serde::Deserialize;

/// One video row from a `ytsearchN:` flat-playlist search. yt-dlp prints one
/// JSON object per line; flat extraction omits many fields, so everything is
/// defaulted — a sparse row must never fail the whole search.
#[derive(Debug, Deserialize)]
pub struct YtVideo {
  #[serde(default)]
  pub id: String,
  #[serde(default)]
  pub title: String,
  /// Channel name. Flat rows carry `channel` and/or `uploader`; prefer
  /// `channel`, fall back to `uploader`.
  #[serde(default)]
  pub channel: Option<String>,
  #[serde(default)]
  pub uploader: Option<String>,
  /// Duration in (possibly fractional) seconds; `None` for livestreams and
  /// some flat rows.
  #[serde(default)]
  pub duration: Option<f64>,
  #[serde(default)]
  pub view_count: Option<u64>,
  /// Best thumbnail URL. yt-dlp emits a top-level `thumbnail` string; `None`
  /// for sparse flat rows that omit it.
  #[serde(default)]
  pub thumbnail: Option<String>,
}
