//! Turning names into playable URIs.
//!
//! A model hands back `(title, artist)` pairs, some of which do not exist. This
//! module searches the catalogue for each one and **drops anything it cannot
//! confidently match** — hallucinated tracks are the primary failure mode once
//! the model is the recommender, and queueing a fuzzy near-miss is worse than
//! queueing nothing. Search always returns *something*, so a title match is
//! required rather than trusting the top hit.
//!
//! Spotify's own `/recommendations` is not used anywhere here: it has been
//! restricted to apps that held extended quota before 2024-11-27, and spotatui
//! users register their own client ID, so it returns 403 for them. The model is
//! the recommender; this module is only the lookup.

use super::brief::{dedupe_key, normalize};
use super::DjSuggestion;
use crate::core::plugin_api::TrackInfo;
use crate::infra::network::search::TrackSearchResponse;
use crate::infra::network::Network;
use rspotify::model::track::FullTrack;
use std::collections::HashSet;

/// How many candidates to ask the catalogue for per suggestion. Enough to get
/// past a remaster or a live version at the top of the list, small enough to
/// keep the response cheap.
const CANDIDATES: usize = 5;

/// What came back from a resolve pass.
///
/// Not `Eq`: `TrackInfo` is only `PartialEq` (its shape is pinned by the plugin
/// contract).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolveReport {
  /// Playable tracks, in the order the suggestions arrived.
  pub resolved: Vec<TrackInfo>,
  /// `"Title — Artist"` for suggestions with no confident catalogue match.
  /// Reported back to whichever model asked, so it can correct itself on its
  /// next step rather than naming the same nonexistent track again.
  pub unresolved: Vec<String>,
  /// Suggestions dropped because they were already queued, playing, or recently
  /// played — see [`SkipSets::session`], which spans all three.
  pub duplicates: Vec<String>,
  /// Suggestions dropped because the listener already has them, with the
  /// avoid-library filter on.
  ///
  /// A separate bucket from [`Self::duplicates`] because the two need different
  /// words in front of the user and different words to the model: "already queued"
  /// is wrong for a track they own, and reporting these as *not in the catalogue*
  /// would tell the model something false.
  pub in_library: Vec<String>,
}

/// The two reasons a suggestion can be rejected before it is even searched for.
///
/// Kept apart rather than merged into one set so [`ResolveReport`] can say which
/// happened. Both hold [`dedupe_key`] values.
pub struct SkipSets<'a> {
  /// Already queued, playing, or recently played. Always applied.
  pub session: &'a HashSet<String>,
  /// In the listener's own playlists. Empty unless the caller asked to avoid the
  /// library: in-TUI that is the <kbd>Ctrl</kbd>+<kbd>O</kbd> toggle, over MCP it is
  /// `queue_tracks(exclude_owned: true)`. Left empty for an ordinary MCP queue, where
  /// the agent named specific tracks and dropping one behind its back would be wrong.
  pub library: &'a HashSet<String>,
}

impl ResolveReport {
  /// One-line summary for a status message or an MCP tool result.
  pub fn summary(&self) -> String {
    let mut parts = vec![format!("queued {}", self.resolved.len())];
    if !self.unresolved.is_empty() {
      parts.push(format!("{} not found", self.unresolved.len()));
    }
    if !self.in_library.is_empty() {
      parts.push(format!("{} already yours", self.in_library.len()));
    }
    if !self.duplicates.is_empty() {
      parts.push(format!("{} already queued", self.duplicates.len()));
    }
    parts.join(", ")
  }
}

/// Resolve suggestions to tracks, skipping duplicates and stopping at `cap`.
///
/// `skips` holds [`dedupe_key`] values for everything to reject unheard, split by
/// reason. `cap` bounds the batch — on an external Connect device each queued
/// Spotify track becomes its own Web API call
/// (`App::add_track_to_native_queue`), so an unbounded batch is an unbounded
/// number of round trips.
///
/// The library gate here is the *cheap* one: it matches on the name the model
/// gave, before any search is paid for. The exact gate runs on the resolved track
/// ID afterwards, in the caller — see `Network::reject_owned_tracks`.
pub async fn resolve_suggestions(
  net: &Network,
  suggestions: &[DjSuggestion],
  skips: &SkipSets<'_>,
  cap: usize,
  #[allow(unused_variables)] youtube_fallback: Option<&YouTubeResolver>,
) -> ResolveReport {
  let mut report = ResolveReport::default();
  // Guards against a model repeating itself inside one reply, which it does.
  let mut claimed: HashSet<String> = HashSet::new();

  for suggestion in suggestions {
    if report.resolved.len() >= cap {
      break;
    }
    let label = suggestion.label();
    let key = dedupe_key(&suggestion.title, &suggestion.artist);
    if skips.library.contains(&key) {
      report.in_library.push(label);
      continue;
    }
    if skips.session.contains(&key) || !claimed.insert(key) {
      report.duplicates.push(label);
      continue;
    }

    match resolve_one(net, suggestion, youtube_fallback).await {
      Some(track) => report.resolved.push(track),
      None => {
        // Deliberately debug-level and never a toast: a model that invents a
        // track is routine, and surfacing it would be constant noise.
        log::debug!("DJ: no catalogue match for {label}");
        report.unresolved.push(label);
      }
    }
  }

  report
}

async fn resolve_one(
  net: &Network,
  suggestion: &DjSuggestion,
  youtube_fallback: Option<&YouTubeResolver>,
) -> Option<TrackInfo> {
  if let Some(track) = resolve_spotify(net, &suggestion.title, &suggestion.artist).await {
    return Some(track);
  }
  #[cfg(feature = "youtube")]
  if let Some(resolver) = youtube_fallback {
    if let Some(track) = resolver
      .resolve(&suggestion.title, &suggestion.artist)
      .await
    {
      return Some(track);
    }
  }
  let _ = youtube_fallback;
  None
}

/// Search Spotify for one track and return the best confident match.
///
/// Unlike `SearchNetwork::get_search_results` and `search_tracks_for_playlist`,
/// which are fire-and-forget and write into `App`, this **returns** its result —
/// the DJ needs an answer, not a UI side effect.
pub async fn resolve_spotify(net: &Network, title: &str, artist: &str) -> Option<TrackInfo> {
  // Spotify field filters keep the fuzzy matching honest; a bare concatenated
  // query happily returns a different artist's song with a similar name.
  let query = vec![
    (
      "q",
      format!("track:\"{}\" artist:\"{}\"", escape(title), escape(artist)),
    ),
    ("type", "track".to_string()),
    ("limit", CANDIDATES.to_string()),
    ("offset", "0".to_string()),
  ];

  let response = match net
    .spotify_get_typed::<TrackSearchResponse>("search", &query)
    .await
  {
    Ok(response) => response,
    Err(e) => {
      // A resolve miss must never surface as the modal error page; the caller
      // reports an aggregate count instead.
      log::debug!("DJ: Spotify search failed for {title} — {artist}: {e}");
      return None;
    }
  };

  best_spotify_match(&response.tracks.items, title, artist)
}

/// Strip the characters that would break out of a quoted Spotify field filter.
fn escape(value: &str) -> String {
  value.replace(['"', '\\'], " ")
}

fn best_spotify_match(candidates: &[FullTrack], title: &str, artist: &str) -> Option<TrackInfo> {
  candidates
    .iter()
    .filter(|track| track.id.is_some())
    .filter_map(|track| {
      let names = track
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
      let score = match_score(&track.name, &names, title, artist)?;
      Some((score, track))
    })
    // `max_by_key` keeps the *last* maximum; candidates are in relevance order,
    // so prefer the earliest by comparing on a descending index tiebreak.
    .enumerate()
    .max_by_key(|(index, (score, _))| (*score, usize::MAX - index))
    .map(|(_, (_, track))| TrackInfo::from(track))
}

/// Score a candidate against what was asked for, or `None` to reject it.
///
/// Rejecting is the important half. Search never returns nothing, so without a
/// floor every invented track resolves to whatever was closest.
fn match_score(
  candidate_title: &str,
  candidate_artist: &str,
  want_title: &str,
  want_artist: &str,
) -> Option<u8> {
  let (ct, ca) = (normalize(candidate_title), normalize(candidate_artist));
  let (wt, wa) = (normalize(want_title), normalize(want_artist));
  if ct.is_empty() || wt.is_empty() {
    return None;
  }

  let title_score = if ct == wt {
    2
  } else if ct.contains(&wt) || wt.contains(&ct) {
    // Covers "Weird Fishes" vs "Weird Fishes / Arpeggi" and
    // "Nude" vs "Nude - Remastered".
    1
  } else {
    return None;
  };

  // An artist mismatch on an otherwise-matching title is usually a cover, a
  // karaoke version, or a different band entirely — all wrong.
  let artist_score = if ca == wa {
    2
  } else if !wa.is_empty() && (ca.contains(&wa) || wa.contains(&ca)) {
    1
  } else if wa.is_empty() {
    0
  } else {
    return None;
  };

  Some(title_score * 2 + artist_score)
}

/// YouTube fallback for tracks Spotify does not have (or cannot license).
///
/// Holds a built source rather than building one per lookup, because
/// `YouTubeSource::search` shells out to `yt-dlp` and the config read behind
/// `build_source` needs the `App` lock.
pub struct YouTubeResolver {
  #[cfg(feature = "youtube")]
  source: crate::infra::youtube::YouTubeSource,
}

impl YouTubeResolver {
  #[cfg(feature = "youtube")]
  pub async fn new(app: &std::sync::Arc<tokio::sync::Mutex<crate::core::app::App>>) -> Self {
    Self {
      source: crate::infra::youtube::dispatch::build_source(app).await,
    }
  }

  #[cfg(feature = "youtube")]
  async fn resolve(&self, title: &str, artist: &str) -> Option<TrackInfo> {
    use crate::core::source::Searcher;
    let query = format!("{artist} {title}");
    let results = match self.source.search(&query).await {
      Ok(results) => results,
      Err(e) => {
        log::debug!("DJ: YouTube search failed for {query}: {e}");
        return None;
      }
    };
    // A YouTube title is free text ("Radiohead - Weird Fishes [HQ]"), so the
    // strict scoring used for Spotify would reject nearly everything. Require
    // the normalised title to appear somewhere in the video title instead.
    let wanted = normalize(title);
    results.tracks.into_iter().find(|track| {
      let haystack = normalize(&format!("{} {}", track.name, track.artists.join(" ")));
      !wanted.is_empty() && haystack.contains(&wanted)
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// `TrackInfo` has no `Default` (the plugin contract pins its shape), so
  /// build a minimal placeholder for the report-shape tests.
  fn stub_track() -> TrackInfo {
    TrackInfo {
      uri: Some("spotify:track:stub".to_string()),
      name: "Stub".to_string(),
      artists: vec!["Stub".to_string()],
      album: String::new(),
      duration_ms: 200_000,
      id: Some("stub".to_string()),
      album_id: None,
      artist_refs: Vec::new(),
      is_playable: true,
      is_local: false,
      track_number: 1,
      explicit: false,
      image_url: None,
    }
  }

  #[test]
  fn exact_title_and_artist_scores_highest() {
    let exact = match_score("Weird Fishes", "Radiohead", "Weird Fishes", "Radiohead");
    let partial = match_score(
      "Weird Fishes / Arpeggi",
      "Radiohead",
      "Weird Fishes",
      "Radiohead",
    );
    assert!(exact > partial);
    assert!(partial.is_some());
  }

  #[test]
  fn punctuation_and_case_do_not_matter() {
    assert!(match_score("NUDE!", "radiohead", "Nude", "Radiohead").is_some());
  }

  #[test]
  fn remaster_suffix_still_matches() {
    assert!(match_score("Nude - Remastered 2017", "Radiohead", "Nude", "Radiohead").is_some());
  }

  #[test]
  fn wrong_artist_is_rejected_even_with_matching_title() {
    // The cover-version trap: right title, wrong band.
    assert_eq!(
      match_score("Nude", "Karaoke Kings", "Nude", "Radiohead"),
      None
    );
  }

  #[test]
  fn unrelated_title_is_rejected() {
    // The hallucination trap: search would have returned this happily.
    assert_eq!(
      match_score("Purple Moonlight", "Radiohead", "Weird Fishes", "Radiohead"),
      None
    );
  }

  #[test]
  fn collaborator_differences_still_match() {
    assert!(match_score(
      "Tearing Me Up",
      "Bob Moses",
      "Tearing Me Up",
      "Bob Moses, RAC"
    )
    .is_some());
  }

  #[test]
  fn the_summary_distinguishes_owned_from_already_queued() {
    let owned = ResolveReport {
      in_library: vec!["a".into()],
      ..ResolveReport::default()
    };
    let summary = owned.summary();
    assert!(summary.contains("1 already yours"));
    assert!(
      !summary.contains("already queued"),
      "a track they own was never queued; saying so explains nothing"
    );
  }

  #[test]
  fn summary_counts_what_is_still_in_the_report() {
    // `summary()` reads `resolved`, so a caller that takes the vector first gets
    // "queued 0" no matter how many landed. Guards the ordering in `dj_queue`.
    let mut report = ResolveReport {
      resolved: vec![stub_track()],
      ..ResolveReport::default()
    };
    assert!(report.summary().contains("queued 1"));
    let _ = std::mem::take(&mut report.resolved);
    assert!(report.summary().contains("queued 0"));
  }

  #[test]
  fn summary_mentions_every_bucket() {
    let report = ResolveReport {
      resolved: vec![stub_track()],
      unresolved: vec!["ghost".into()],
      duplicates: vec!["dupe".into()],
      in_library: vec!["owned".into()],
    };
    let summary = report.summary();
    assert!(summary.contains("queued 1"));
    assert!(summary.contains("1 not found"));
    assert!(summary.contains("1 already queued"));
    assert!(summary.contains("1 already yours"));
  }
}
