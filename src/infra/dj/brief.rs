//! The taste brief: a compact, **aggregate** summary of what the user listens
//! to, assembled from the local listening history.
//!
//! This is the only thing about the user that ever reaches a model, so it is
//! deliberately built from aggregates rather than raw records: names of tracks,
//! artists and albums, and nothing else. No timestamps, no Spotify IDs, no
//! identity, no play counts tied to a clock. That is a privacy property, not a
//! token optimisation — see `docs/ai-dj.md`.

use crate::infra::history::{
  aggregate_top_albums, aggregate_top_artists, aggregate_top_tracks, filter_listens_for_period,
  ListenRecord, RecapPeriod,
};

/// How many entries of each kind to carry. Enough for a model to infer taste,
/// small enough to stay cheap in a prompt (and in a local model's context).
const TOP_ARTISTS: usize = 15;
const TOP_TRACKS: usize = 20;
const TOP_ALBUMS: usize = 10;
/// Recent plays exist so the model can be told *not* to repeat them, and so the
/// enqueue path can dedupe against them.
const RECENT: usize = 25;

/// An aggregate portrait of the user's listening, plus the current steer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TasteBrief {
  pub period_label: &'static str,
  pub total_plays: usize,
  pub top_artists: Vec<String>,
  /// `"Title — Artist"` display strings, as produced by the Stats aggregation.
  pub top_tracks: Vec<String>,
  pub top_albums: Vec<String>,
  /// Newest first, `"Title — Artist"`.
  pub recent: Vec<String>,
  pub now_playing: Option<String>,
  /// The user's steer ("something chill for focusing"), when they gave one.
  pub vibe: Option<String>,
  /// Normalised dedupe keys for [`Self::recent`]. Never rendered into a prompt —
  /// this is for the enqueue path, so the DJ does not re-queue what just played.
  pub recent_keys: Vec<String>,
}

pub fn period_label(period: RecapPeriod) -> &'static str {
  match period {
    RecapPeriod::SevenDays => "the last 7 days",
    RecapPeriod::ThirtyDays => "the last 30 days",
    RecapPeriod::Month => "this month",
    RecapPeriod::Year => "this year",
    RecapPeriod::All => "all time",
  }
}

/// Normalise a title/artist pair into a dedupe key.
///
/// Lowercased, punctuation-insensitive and whitespace-collapsed, so
/// `"Weird Fishes / Arpeggi"` and `"weird fishes/arpeggi"` collide — a model
/// rarely reproduces a title byte-for-byte, and near-misses are exactly the
/// duplicates worth catching.
pub fn dedupe_key(title: &str, artist: &str) -> String {
  // The first artist only: a model may list more or fewer collaborators than
  // the catalogue does, and requiring the full list to match defeats the point.
  let primary = artist.split(',').next().unwrap_or(artist);
  format!("{}\u{1}{}", normalize(title), normalize(primary))
}

/// Lowercase, drop non-alphanumerics, collapse the resulting gaps to single
/// spaces. Shared by [`dedupe_key`] and the resolver's match scoring so the two
/// agree on what "the same track" means.
pub fn normalize(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  let mut pending_space = false;
  for ch in value.chars() {
    if ch.is_alphanumeric() {
      if pending_space && !out.is_empty() {
        out.push(' ');
      }
      pending_space = false;
      out.extend(ch.to_lowercase());
    } else {
      pending_space = true;
    }
  }
  out
}

/// Build the brief from an already-loaded set of records.
///
/// Callers must not read the listens file on an async task: [`load_listens`] is
/// blocking and `listens.jsonl` is unbounded append-only, so a long-time user's
/// file is large. Load it in `spawn_blocking`, then call this.
///
/// [`load_listens`]: crate::infra::history::load_listens
pub fn build_brief(listens: &[ListenRecord], period: RecapPeriod) -> TasteBrief {
  let filtered = filter_listens_for_period(listens, period);

  let mut recent = Vec::new();
  let mut recent_keys = Vec::new();
  let mut seen = std::collections::HashSet::new();
  // `filtered` is in file order (oldest first); walk backwards for newest first.
  for record in filtered.iter().rev() {
    let artist = record.artists.join(", ");
    let key = dedupe_key(&record.title, &artist);
    if !seen.insert(key.clone()) {
      continue;
    }
    recent.push(format!("{} — {}", record.title, artist));
    recent_keys.push(key);
    if recent.len() >= RECENT {
      break;
    }
  }

  TasteBrief {
    period_label: period_label(period),
    total_plays: filtered.len(),
    top_artists: rank_displays(aggregate_top_artists(&filtered, TOP_ARTISTS)),
    top_tracks: rank_displays(aggregate_top_tracks(&filtered, TOP_TRACKS)),
    top_albums: rank_displays(aggregate_top_albums(&filtered, TOP_ALBUMS)),
    recent,
    now_playing: None,
    vibe: None,
    recent_keys,
  }
}

fn rank_displays(entries: Vec<crate::infra::history::RankedEntry>) -> Vec<String> {
  entries.into_iter().map(|entry| entry.display).collect()
}

impl TasteBrief {
  /// Render the brief as the plain-text block handed to a model.
  ///
  /// Deliberately prose-and-lists rather than JSON: every backend consumes this,
  /// including small local models that handle a bulleted list better than a
  /// nested object.
  pub fn to_prompt_block(&self) -> String {
    let mut out = String::new();
    out.push_str(&format!(
      "Listening history ({}, {} qualifying plays):\n",
      self.period_label, self.total_plays
    ));
    push_list(&mut out, "Top artists", &self.top_artists);
    push_list(&mut out, "Top tracks", &self.top_tracks);
    push_list(&mut out, "Top albums", &self.top_albums);
    push_list(
      &mut out,
      "Recently played (do not repeat these)",
      &self.recent,
    );
    if let Some(now) = &self.now_playing {
      out.push_str(&format!("\nCurrently playing: {now}\n"));
    }
    if let Some(vibe) = &self.vibe {
      out.push_str(&format!("\nThe listener asked for: {vibe}\n"));
    }
    out
  }

  /// Whether there is enough history to say anything useful about taste.
  ///
  /// Consumed by the MCP `get_listening_history` tool, which tells the agent to
  /// ask the user rather than infer from nothing.
  pub fn is_sparse(&self) -> bool {
    self.total_plays < 5
  }
}

fn push_list(out: &mut String, label: &str, values: &[String]) {
  if values.is_empty() {
    return;
  }
  out.push_str(&format!("\n{label}:\n"));
  for value in values {
    out.push_str(&format!("- {value}\n"));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::history::{HistoryItemKind, HistoryPlaybackSource};
  use chrono::{Duration, Utc};

  fn record(title: &str, artist: &str, uri: Option<&str>, duration_ms: u32) -> ListenRecord {
    let now = Utc::now();
    let listened_ms = u64::from(duration_ms);
    ListenRecord {
      started_at: now - Duration::minutes(5),
      ended_at: now,
      listened_ms,
      duration_ms,
      // Mirror the collector: `qualified` is precomputed on write.
      qualified: duration_ms > 30_000 && listened_ms >= u64::from(duration_ms / 2).min(240_000),
      title: title.to_string(),
      artists: vec![artist.to_string()],
      album: format!("{title} EP"),
      item_kind: HistoryItemKind::Track,
      item_id: None,
      item_uri: uri.map(str::to_string),
      context_uri: None,
      // Every non-Spotify source is written as `ExternalDevice` by
      // `source_snapshot`, so this field cannot distinguish them. The brief
      // must not depend on it; `item_uri`'s scheme is the real discriminator.
      source: HistoryPlaybackSource::ExternalDevice,
    }
  }

  #[test]
  fn brief_aggregates_names_only() {
    let listens = vec![
      record(
        "Weird Fishes",
        "Radiohead",
        Some("spotify:track:a"),
        300_000,
      ),
      record("Nude", "Radiohead", Some("spotify:track:b"), 300_000),
      record(
        "Teardrop",
        "Massive Attack",
        Some("spotify:track:c"),
        300_000,
      ),
    ];
    let brief = build_brief(&listens, RecapPeriod::All);

    assert_eq!(brief.total_plays, 3);
    assert!(brief.top_artists.contains(&"Radiohead".to_string()));
    assert_eq!(brief.recent.len(), 3);
    // Newest first: the last record in file order leads.
    assert!(brief.recent[0].starts_with("Teardrop"));

    let prompt = brief.to_prompt_block();
    assert!(prompt.contains("Radiohead"));
    // No identifiers of any kind leak into the prompt.
    assert!(!prompt.contains("spotify:track"));
  }

  #[test]
  fn non_spotify_listens_are_included_and_identified_by_uri_scheme() {
    // Regression guard for the mislabelling at media_metadata.rs:344 — a YouTube
    // play is written with `source: ExternalDevice`, so anything that wants to
    // tell sources apart must read the URI scheme instead.
    let listens = vec![record(
      "Some Mix",
      "Uploader",
      Some("youtube:video:xyz"),
      600_000,
    )];
    let brief = build_brief(&listens, RecapPeriod::All);

    assert_eq!(brief.total_plays, 1);
    assert!(brief.recent[0].starts_with("Some Mix"));
    assert!(listens[0]
      .item_uri
      .as_deref()
      .is_some_and(|uri| uri.starts_with("youtube:")));
  }

  #[test]
  fn radio_never_qualifies_so_it_cannot_reach_the_brief() {
    // The radio branch passes `duration_ms = 0` (media_metadata.rs:282), so
    // `qualifies_listen` rejects it and `filter_listens_for_period` drops it.
    // This is correct, not a bug to be "fixed".
    let listens = vec![record(
      "SomaFM Groove Salad",
      "SomaFM",
      Some("radio:soma"),
      0,
    )];
    assert!(!listens[0].qualified);

    let brief = build_brief(&listens, RecapPeriod::All);
    assert_eq!(brief.total_plays, 0);
    assert!(brief.recent.is_empty());
    assert!(brief.is_sparse());
  }

  #[test]
  fn recent_is_deduped_by_normalised_key() {
    let listens = vec![
      record("Weird Fishes", "Radiohead", None, 300_000),
      record("weird  fishes!", "radiohead", None, 300_000),
    ];
    let brief = build_brief(&listens, RecapPeriod::All);
    assert_eq!(brief.recent.len(), 1, "near-miss titles must collide");
    assert_eq!(brief.recent_keys.len(), 1);
  }

  #[test]
  fn dedupe_key_ignores_case_punctuation_and_extra_artists() {
    assert_eq!(
      dedupe_key("Weird Fishes / Arpeggi", "Radiohead"),
      dedupe_key("weird fishes arpeggi", "Radiohead, Thom Yorke")
    );
    assert_ne!(
      dedupe_key("Nude", "Radiohead"),
      dedupe_key("Nude", "Prince")
    );
  }
}
