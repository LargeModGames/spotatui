//! The pure playlist-sync engine: the link model, the per-mirror diff and the
//! track matcher. `plan` and `pick_candidate` take scalars and return plain
//! data, so every rule is unit tested without a network or a clock. The master
//! wins: a matched track deleted on the mirror comes back, and a track the sync
//! never added is never touched. `store` is the only part that touches the disk.

pub mod store;

use crate::core::source::Source;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Duration window for a title match, widened by the sources that report whole seconds.
const DURATION_TOLERANCE_MS: u64 = 2_000;

/// One track as a source reports it, reduced to what matching needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncTrack {
  /// The source-native track id, which is what a match is stored by.
  pub key: String,
  pub isrc: Option<String>,
  pub title: String,
  /// The first credited artist only.
  pub artist: String,
  pub duration_ms: Option<u64>,
}

/// One master playlist and the mirrors it is synced onto.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
  /// A locally generated id, stable across playlist renames.
  pub id: String,
  pub master: Endpoint,
  #[serde(default)]
  pub mirrors: Vec<Mirror>,
}

/// One playlist on one source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
  pub source: Source,
  pub playlist_uri: String,
  /// The playlist name as it read when the link was made.
  pub name: String,
}

/// One mirror of a link, with the match cache a re-run reuses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mirror {
  pub endpoint: Endpoint,
  /// Master track key to mirror track key, ordered so the saved file is stable.
  #[serde(default)]
  pub matches: BTreeMap<String, String>,
  #[serde(default)]
  pub unmatched: Vec<Unmatched>,
  /// RFC 3339 stamp of the last finished run.
  #[serde(default)]
  pub last_run: Option<String>,
}

/// A master track the last run could not place on a mirror.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unmatched {
  pub master_key: String,
  pub title: String,
  pub artist: String,
  pub reason: UnmatchReason,
}

/// Why a master track has no mirror track.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnmatchReason {
  /// The mirror source returned nothing that is the same recording.
  NoCandidate,
  /// The track cannot be mirrored at all, such as a local file or an episode.
  NotSyncable,
  /// The search itself failed; the message is what the source said.
  SearchFailed(String),
}

/// What one mirror needs in order to catch up with its master.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncPlan {
  /// Master tracks with no match entry: search the mirror for each.
  pub to_resolve: Vec<SyncTrack>,
  /// Matched pairs whose mirror track is absent, one per mirror key: (master key, mirror key).
  pub to_add: Vec<(String, String)>,
  /// Mirror keys to delete, deduped and on the mirror; the caller drops any it matched this run.
  pub to_remove: Vec<String>,
  /// Master keys to forget, whether or not a mirror track is removed with them.
  pub stale: Vec<String>,
}

impl Link {
  /// Unmatched tracks across every mirror, for the sync screen's link row.
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub fn unmatched_count(&self) -> usize {
    self
      .mirrors
      .iter()
      .map(|mirror| mirror.unmatched.len())
      .sum()
  }

  /// Whether `filter` names this link: its id exactly, or its master name, ignoring case.
  pub fn matches_filter(&self, filter: &str) -> bool {
    let filter = filter.trim();
    self.id == filter || self.master.name.trim().eq_ignore_ascii_case(filter)
  }
}

impl Mirror {
  /// An empty mirror at `endpoint`.
  pub fn new(endpoint: Endpoint) -> Self {
    Mirror {
      endpoint,
      matches: BTreeMap::new(),
      unmatched: Vec::new(),
      last_run: None,
    }
  }

  /// Remember that `master_key` resolved to `mirror_key` on this mirror.
  pub fn record_match(&mut self, master_key: &str, mirror_key: &str) {
    self
      .matches
      .insert(master_key.to_string(), mirror_key.to_string());
  }

  /// Forget the match entries a [`SyncPlan`] marked stale.
  pub fn drop_stale(&mut self, stale: &[String]) {
    for master_key in stale {
      self.matches.remove(master_key);
    }
  }

  /// Close a run: replace the unmatched list and stamp `at` as the last run.
  pub fn finish_run(&mut self, unmatched: Vec<Unmatched>, at: String) {
    self.unmatched = unmatched;
    self.last_run = Some(at);
  }
}

impl Unmatched {
  /// Record `track` as unresolved on a mirror.
  pub fn new(track: &SyncTrack, reason: UnmatchReason) -> Self {
    Unmatched {
      master_key: track.key.clone(),
      title: track.title.clone(),
      artist: track.artist.clone(),
      reason,
    }
  }
}

/// Diff one mirror against its master: what to search, add, remove and forget.
pub fn plan(
  master: &[SyncTrack],
  mirror_keys: &[String],
  matches: &BTreeMap<String, String>,
) -> SyncPlan {
  let present: BTreeSet<&str> = mirror_keys.iter().map(String::as_str).collect();
  let mut planned: BTreeSet<&str> = BTreeSet::new();
  let mut added: BTreeSet<&str> = BTreeSet::new();
  let mut out = SyncPlan::default();

  for track in master {
    if !planned.insert(track.key.as_str()) {
      continue;
    }
    let Some(mirror_key) = matches.get(&track.key) else {
      out.to_resolve.push(track.clone());
      continue;
    };
    if !present.contains(mirror_key.as_str()) && added.insert(mirror_key.as_str()) {
      out.to_add.push((track.key.clone(), mirror_key.clone()));
    }
  }

  let wanted: BTreeSet<&str> = matches
    .iter()
    .filter(|(master_key, _)| planned.contains(master_key.as_str()))
    .map(|(_, mirror_key)| mirror_key.as_str())
    .collect();
  let mut dropped: BTreeSet<&str> = BTreeSet::new();
  for (master_key, mirror_key) in matches {
    if planned.contains(master_key.as_str()) {
      continue;
    }
    out.stale.push(master_key.clone());
    if wanted.contains(mirror_key.as_str())
      || !present.contains(mirror_key.as_str())
      || !dropped.insert(mirror_key.as_str())
    {
      continue;
    }
    out.to_remove.push(mirror_key.clone());
  }

  out
}

/// Index of the candidate that is the same recording as `target`, if any.
pub fn pick_candidate(target: &SyncTrack, candidates: &[SyncTrack]) -> Option<usize> {
  let wanted_isrc = normalized_isrc_of(target);
  let isrc_hit = wanted_isrc.as_deref().and_then(|isrc| {
    candidates
      .iter()
      .position(|candidate| normalized_isrc_of(candidate).as_deref() == Some(isrc))
  });
  if isrc_hit.is_some() {
    return isrc_hit;
  }

  let title = normalize_text(&target.title);
  if title.is_empty() {
    return None;
  }
  let stripped = normalize_text(&strip_title_suffix(&target.title));
  let artist = normalize_text(&target.artist);
  candidates.iter().position(|candidate| {
    let listed = normalize_text(&candidate.title);
    (listed == title || (!stripped.is_empty() && listed == stripped))
      && normalize_text(&candidate.artist) == artist
      && durations_match(target.duration_ms, candidate.duration_ms)
  })
}

/// The title without its trailing `(feat. X)`, `[Remastered]` or ` - Radio Edit`
/// suffixes, which another catalog often drops; the duration check keeps a
/// different edition apart.
pub fn strip_title_suffix(title: &str) -> String {
  let mut out = title.trim();
  loop {
    if out.ends_with([')', ']']) {
      if let Some(open) = out.rfind(['(', '[']).filter(|open| *open > 0) {
        out = out[..open].trim_end();
        continue;
      }
    }
    if let Some(dash) = out.rfind(" - ").filter(|dash| *dash > 0) {
      out = out[..dash].trim_end();
      continue;
    }
    return out.to_string();
  }
}

/// Uppercase an ISRC and drop its separators, so `gb-aaa-00-00001` is canonical.
pub fn normalize_isrc(value: &str) -> String {
  value
    .chars()
    .filter(|ch| ch.is_ascii_alphanumeric())
    .map(|ch| ch.to_ascii_uppercase())
    .collect()
}

/// Lowercase, every non-alphanumeric run collapsed to one space, already trimmed.
pub fn normalize_text(value: &str) -> String {
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

/// A track's ISRC in canonical form, with a blank one read as absent.
fn normalized_isrc_of(track: &SyncTrack) -> Option<String> {
  track
    .isrc
    .as_deref()
    .map(normalize_isrc)
    .filter(|isrc| !isrc.is_empty())
}

/// Whether two durations are one recording's; an absent one never vetoes.
fn durations_match(left: Option<u64>, right: Option<u64>) -> bool {
  match (left, right) {
    (Some(left), Some(right)) => left.abs_diff(right) <= DURATION_TOLERANCE_MS,
    _ => true,
  }
}

/// What one run did, per link and in total.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
  pub links: Vec<LinkReport>,
  /// A failure that stopped the whole run, such as a rate limit.
  pub error: Option<String>,
  pub dry_run: bool,
}

/// What one link did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkReport {
  #[cfg_attr(not(feature = "tui"), allow(dead_code))]
  pub id: String,
  /// The master playlist name, which is what the user types after `--link`.
  pub name: String,
  pub added: usize,
  pub removed: usize,
  pub unmatched: usize,
  pub outcome: LinkOutcome,
}

/// How far a link or one of its mirrors got.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LinkOutcome {
  #[default]
  Ran,
  /// Nothing was attempted and nothing is wrong, such as a source not logged in.
  Skipped(String),
  /// Something the source did answer went wrong; the text is what it said.
  Failed(String),
}

/// What one mirror did, folded into its link's report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MirrorRun {
  pub added: usize,
  pub removed: usize,
  pub unmatched: usize,
  pub outcome: LinkOutcome,
}

impl SyncReport {
  /// Tracks added across every link.
  pub fn added(&self) -> usize {
    self.links.iter().map(|link| link.added).sum()
  }

  /// Tracks removed across every link.
  pub fn removed(&self) -> usize {
    self.links.iter().map(|link| link.removed).sum()
  }

  /// Master tracks left unplaced across every link.
  pub fn unmatched(&self) -> usize {
    self.links.iter().map(|link| link.unmatched).sum()
  }

  /// Whether the run itself failed or any link did; the CLI's exit signal.
  pub fn failed(&self) -> bool {
    self.error.is_some()
      || self
        .links
        .iter()
        .any(|link| matches!(link.outcome, LinkOutcome::Failed(_)))
  }

  /// The one-line summary a status bar shows.
  pub fn summary(&self) -> String {
    if let Some(error) = &self.error {
      return format!("Playlist sync failed: {error}");
    }
    if self.links.is_empty() {
      return "Playlist sync: no links".to_string();
    }
    let dry = if self.dry_run { " (dry run)" } else { "" };
    format!(
      "Playlist sync{dry}: {} added, {} removed, {} unmatched",
      self.added(),
      self.removed(),
      self.unmatched()
    )
  }

  /// The summary plus one line per link, for the CLI.
  pub fn printable(&self) -> String {
    let mut out = self.summary();
    for link in &self.links {
      out.push('\n');
      out.push_str(&link.line());
    }
    out
  }
}

impl LinkReport {
  /// An untouched report for one link.
  pub fn new(id: &str, name: &str) -> Self {
    LinkReport {
      id: id.to_string(),
      name: name.to_string(),
      ..LinkReport::default()
    }
  }

  /// Fold one mirror in: counts add up, a failure outranks a skip.
  pub fn absorb(&mut self, run: MirrorRun) {
    self.added += run.added;
    self.removed += run.removed;
    self.unmatched += run.unmatched;
    self.note(run.outcome);
  }

  /// Record an outcome for the link itself, keeping the worse of the two.
  pub fn note(&mut self, outcome: LinkOutcome) {
    if outcome.rank() > self.outcome.rank() {
      self.outcome = outcome;
    }
  }

  /// This link's line in the CLI report.
  pub fn line(&self) -> String {
    match &self.outcome {
      LinkOutcome::Ran => format!(
        "{}: {} added, {} removed, {} unmatched",
        self.name, self.added, self.removed, self.unmatched
      ),
      LinkOutcome::Skipped(why) => format!("{}: skipped ({why})", self.name),
      LinkOutcome::Failed(why) => format!("{}: failed ({why})", self.name),
    }
  }
}

impl LinkOutcome {
  /// How bad this outcome is; a fold keeps the highest and the first text of that rank.
  fn rank(&self) -> u8 {
    match self {
      LinkOutcome::Ran => 0,
      LinkOutcome::Skipped(_) => 1,
      LinkOutcome::Failed(_) => 2,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn track(key: &str, title: &str, artist: &str, duration_ms: u64) -> SyncTrack {
    SyncTrack {
      key: key.to_string(),
      isrc: None,
      title: title.to_string(),
      artist: artist.to_string(),
      duration_ms: Some(duration_ms),
    }
  }

  fn with_isrc(mut track: SyncTrack, isrc: &str) -> SyncTrack {
    track.isrc = Some(isrc.to_string());
    track
  }

  fn without_duration(mut track: SyncTrack) -> SyncTrack {
    track.duration_ms = None;
    track
  }

  fn keys(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
  }

  fn cache(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
      .iter()
      .map(|(master, mirror)| (master.to_string(), mirror.to_string()))
      .collect()
  }

  fn mirror_of(source: Source, uri: &str) -> Mirror {
    Mirror {
      endpoint: Endpoint {
        source,
        playlist_uri: uri.to_string(),
        name: "Mirror".to_string(),
      },
      matches: BTreeMap::new(),
      unmatched: Vec::new(),
      last_run: None,
    }
  }

  #[test]
  fn a_matched_track_present_on_the_mirror_is_a_no_op() {
    let master = vec![track("m1", "Creep", "Radiohead", 238_000)];
    let out = plan(&master, &keys(&["x1"]), &cache(&[("m1", "x1")]));
    assert_eq!(out, SyncPlan::default());
  }

  #[test]
  fn a_matched_track_missing_from_the_mirror_is_added_again() {
    let master = vec![track("m1", "Creep", "Radiohead", 238_000)];
    let out = plan(&master, &keys(&[]), &cache(&[("m1", "x1")]));
    assert_eq!(out.to_add, vec![("m1".to_string(), "x1".to_string())]);
    assert!(out.to_resolve.is_empty());
    assert!(out.to_remove.is_empty());
    assert!(out.stale.is_empty());
  }

  #[test]
  fn a_master_track_with_no_match_entry_goes_to_resolve() {
    let master = vec![track("m1", "Creep", "Radiohead", 238_000)];
    let out = plan(&master, &keys(&["x9"]), &cache(&[]));
    assert_eq!(out.to_resolve, master);
    assert!(out.to_add.is_empty());
    assert!(out.to_remove.is_empty());
    assert!(out.stale.is_empty());
  }

  #[test]
  fn a_dropped_master_track_is_removed_from_the_mirror_and_forgotten() {
    let master = vec![track("m1", "Creep", "Radiohead", 238_000)];
    let out = plan(
      &master,
      &keys(&["x1", "x2", "x9"]),
      &cache(&[("m1", "x1"), ("m2", "x2"), ("m3", "x3")]),
    );
    assert_eq!(out.stale, keys(&["m2", "m3"]));
    assert_eq!(out.to_remove, keys(&["x2"]));
    assert!(out.to_add.is_empty());
    assert!(out.to_resolve.is_empty());
  }

  #[test]
  fn a_duplicate_master_key_is_planned_once() {
    let first = track("m1", "Creep", "Radiohead", 238_000);
    let second = track("m2", "Idioteque", "Radiohead", 290_000);
    let master = vec![first.clone(), first.clone(), second.clone()];

    let out = plan(&master, &keys(&[]), &cache(&[]));
    assert_eq!(out.to_resolve, vec![first, second]);

    let out = plan(&master, &keys(&[]), &cache(&[("m1", "x1"), ("m2", "x2")]));
    assert_eq!(
      out.to_add,
      vec![
        ("m1".to_string(), "x1".to_string()),
        ("m2".to_string(), "x2".to_string()),
      ]
    );
  }

  #[test]
  fn to_add_keeps_master_order() {
    let master = vec![
      track("m3", "Third", "A", 100_000),
      track("m1", "First", "A", 100_000),
      track("m2", "Second", "A", 100_000),
    ];
    let out = plan(
      &master,
      &keys(&[]),
      &cache(&[("m1", "x1"), ("m2", "x2"), ("m3", "x3")]),
    );
    assert_eq!(
      out.to_add,
      vec![
        ("m3".to_string(), "x3".to_string()),
        ("m1".to_string(), "x1".to_string()),
        ("m2".to_string(), "x2".to_string()),
      ]
    );
  }

  #[test]
  fn a_mirror_key_two_master_tracks_share_is_added_once() {
    let master = vec![
      track("m1", "Creep", "Radiohead", 238_000),
      track("m2", "Creep", "Radiohead", 238_000),
    ];
    let out = plan(&master, &keys(&[]), &cache(&[("m1", "x1"), ("m2", "x1")]));
    assert_eq!(out.to_add, vec![("m1".to_string(), "x1".to_string())]);
    assert!(out.to_remove.is_empty());
    assert!(out.stale.is_empty());
  }

  #[test]
  fn a_mirror_track_two_master_tracks_share_survives_one_removal() {
    let master = vec![track("m2", "Creep", "Radiohead", 238_000)];
    let out = plan(
      &master,
      &keys(&["x1"]),
      &cache(&[("m1", "x1"), ("m2", "x1")]),
    );
    assert_eq!(out.stale, keys(&["m1"]));
    assert!(out.to_remove.is_empty());
    assert!(out.to_add.is_empty());

    let out = plan(&[], &keys(&["x1"]), &cache(&[("m1", "x1"), ("m2", "x1")]));
    assert_eq!(out.stale, keys(&["m1", "m2"]));
    assert_eq!(out.to_remove, keys(&["x1"]));
  }

  #[test]
  fn a_mirror_key_listed_twice_is_removed_once() {
    let out = plan(&[], &keys(&["x1", "x1"]), &cache(&[("m1", "x1")]));
    assert_eq!(out.stale, keys(&["m1"]));
    assert_eq!(out.to_remove, keys(&["x1"]));

    let master = vec![track("m1", "Creep", "Radiohead", 238_000)];
    let out = plan(&master, &keys(&["x1", "x1"]), &cache(&[("m1", "x1")]));
    assert_eq!(out, SyncPlan::default());
  }

  #[test]
  fn an_equal_isrc_wins_over_an_earlier_title_match() {
    let target = with_isrc(track("m1", "Creep", "Radiohead", 238_000), "GBAAA0000001");
    let candidates = vec![
      track("x1", "Creep", "Radiohead", 238_000),
      with_isrc(
        track("x2", "Creep (Acoustic)", "Radiohead", 240_000),
        "GBAAA0000001",
      ),
    ];
    assert_eq!(pick_candidate(&target, &candidates), Some(1));
  }

  #[test]
  fn the_first_of_two_equal_isrc_candidates_wins() {
    let target = with_isrc(track("m1", "Creep", "Radiohead", 238_000), "GBAAA0000001");
    let candidates = vec![
      track("x0", "Creep", "Radiohead", 238_000),
      with_isrc(
        track("x1", "Creep (Single)", "Radiohead", 238_000),
        "GBAAA0000001",
      ),
      with_isrc(track("x2", "Creep", "Radiohead", 238_000), "GBAAA0000001"),
    ];
    assert_eq!(pick_candidate(&target, &candidates), Some(1));
  }

  #[test]
  fn an_isrc_with_dashes_spaces_and_lowercase_matches_its_canonical_form() {
    assert_eq!(normalize_isrc(" gb-aaa-00-00001 "), "GBAAA0000001");
    let target = with_isrc(
      track("m1", "Creep", "Radiohead", 238_000),
      "gb-aaa 00-00001",
    );
    let candidates = vec![with_isrc(
      track("x1", "Other", "Nobody", 10_000),
      "GBAAA0000001",
    )];
    assert_eq!(pick_candidate(&target, &candidates), Some(0));
  }

  #[test]
  fn an_empty_isrc_is_treated_as_absent() {
    let target = with_isrc(track("m1", "Creep", "Radiohead", 238_000), "  ");

    let candidates = vec![with_isrc(
      track("x1", "Karma Police", "Radiohead", 260_000),
      "",
    )];
    assert_eq!(pick_candidate(&target, &candidates), None);

    let candidates = vec![with_isrc(track("x1", "Creep", "Radiohead", 238_000), "")];
    assert_eq!(pick_candidate(&target, &candidates), Some(0));
  }

  #[test]
  fn a_differing_isrc_still_allows_a_title_and_artist_match() {
    let target = with_isrc(track("m1", "Creep", "Radiohead", 238_000), "GBAAA0000001");
    let candidates = vec![with_isrc(
      track("x1", "Creep", "Radiohead", 238_000),
      "USZZZ9999999",
    )];
    assert_eq!(pick_candidate(&target, &candidates), Some(0));
  }

  #[test]
  fn a_title_and_artist_match_inside_the_duration_window_is_a_hit() {
    let target = track("m1", "Creep", "Radiohead", 238_000);
    let candidates = vec![
      track("x1", "Creep", "Coldplay", 238_000),
      track("x2", "Creep", "Radiohead", 239_900),
    ];
    assert_eq!(pick_candidate(&target, &candidates), Some(1));
    assert_eq!(pick_candidate(&target, &[]), None);
  }

  #[test]
  fn a_live_version_with_a_longer_duration_does_not_match() {
    let target = track("m1", "Creep", "Radiohead", 238_000);
    assert_eq!(
      pick_candidate(&target, &[track("x1", "Creep", "Radiohead", 258_000)]),
      None
    );

    let mut edge = track("x2", "Creep", "Radiohead", 240_000);
    assert_eq!(pick_candidate(&target, &[edge.clone()]), Some(0));
    edge.duration_ms = Some(240_001);
    assert_eq!(pick_candidate(&target, &[edge]), None);
  }

  #[test]
  fn a_missing_duration_on_either_side_does_not_block_a_match() {
    let target = track("m1", "Creep", "Radiohead", 238_000);
    let candidate = without_duration(track("x1", "Creep", "Radiohead", 0));
    assert_eq!(pick_candidate(&target, &[candidate]), Some(0));

    let blind = without_duration(track("m1", "Creep", "Radiohead", 0));
    let live = track("x1", "Creep", "Radiohead", 258_000);
    assert_eq!(pick_candidate(&blind, &[live]), Some(0));
  }

  #[test]
  fn punctuation_casing_and_unicode_do_not_block_a_match() {
    let target = track("m1", "Don\u{2019}t Stop Me Now!", "Queen", 210_000);
    let candidates = vec![track("x1", "  don't stop me NOW  ", "queen", 210_500)];
    assert_eq!(pick_candidate(&target, &candidates), Some(0));

    let target = track("m2", "J\u{f3}ga", "Bj\u{f6}rk", 305_000);
    let candidates = vec![track("x2", "J\u{d3}GA", "BJ\u{d6}RK", 305_000)];
    assert_eq!(pick_candidate(&target, &candidates), Some(0));
  }

  #[test]
  fn a_feat_suffix_only_the_master_carries_is_forgiven_within_the_duration_window() {
    let target = track("m1", "Nice For What (feat. Big Freedia)", "Drake", 210_000);
    let candidates = vec![track("x1", "Nice For What", "Drake", 210_000)];
    assert_eq!(pick_candidate(&target, &candidates), Some(0));

    let other_edition = vec![track("x1", "Nice For What", "Drake", 240_000)];
    assert_eq!(pick_candidate(&target, &other_edition), None);

    assert_eq!(strip_title_suffix("No Sleep (feat. Bonn)"), "No Sleep");
    assert_eq!(strip_title_suffix("Tsunami - Radio Edit"), "Tsunami");
    assert_eq!(
      strip_title_suffix("Sooraj Dooba Hain (From \"Roy\") [Remastered]"),
      "Sooraj Dooba Hain"
    );
    assert_eq!(strip_title_suffix("(Untitled)"), "(Untitled)");
  }

  #[test]
  fn a_blank_title_never_matches() {
    let target = track("m1", "  ", "", 238_000);
    let candidates = vec![track("x1", "", "", 238_000)];
    assert_eq!(pick_candidate(&target, &candidates), None);
  }

  #[test]
  fn normalize_text_lowercases_and_collapses_every_gap() {
    assert_eq!(
      normalize_text("  ***Weird Fishes / Arpeggi***  "),
      "weird fishes arpeggi"
    );
    assert_eq!(normalize_text("!!!"), "");
  }

  #[test]
  fn record_match_and_drop_stale_maintain_the_match_cache() {
    let mut mirror = mirror_of(Source::Qobuz, "qobuz:playlist:1");
    mirror.record_match("m1", "x1");
    mirror.record_match("m2", "x2");
    mirror.record_match("m1", "x9");
    assert_eq!(mirror.matches, cache(&[("m1", "x9"), ("m2", "x2")]));

    mirror.drop_stale(&keys(&["m2", "absent"]));
    assert_eq!(mirror.matches, cache(&[("m1", "x9")]));
  }

  #[test]
  fn finish_run_replaces_the_unmatched_list_and_stamps_the_run() {
    let mut mirror = mirror_of(Source::Subsonic, "subsonic:playlist:7");
    let missing = track("m1", "Creep", "Radiohead", 238_000);

    mirror.finish_run(
      vec![Unmatched::new(&missing, UnmatchReason::NoCandidate)],
      "2026-09-16T10:00:00Z".to_string(),
    );
    assert_eq!(mirror.unmatched.len(), 1);
    assert_eq!(mirror.unmatched[0].master_key, "m1");
    assert_eq!(mirror.unmatched[0].title, "Creep");
    assert_eq!(mirror.unmatched[0].reason, UnmatchReason::NoCandidate);

    mirror.finish_run(Vec::new(), "2026-09-17T10:00:00Z".to_string());
    assert!(mirror.unmatched.is_empty());
    assert_eq!(mirror.last_run.as_deref(), Some("2026-09-17T10:00:00Z"));
  }

  #[test]
  fn unmatched_count_sums_every_mirror() {
    let missing = track("m1", "Creep", "Radiohead", 238_000);

    let mut first = mirror_of(Source::Qobuz, "qobuz:playlist:1");
    first.finish_run(
      vec![Unmatched::new(&missing, UnmatchReason::NotSyncable)],
      "2026-09-16T10:00:00Z".to_string(),
    );

    let mut second = mirror_of(Source::Subsonic, "subsonic:playlist:7");
    second.finish_run(
      vec![
        Unmatched::new(&missing, UnmatchReason::NoCandidate),
        Unmatched::new(&missing, UnmatchReason::SearchFailed("429".to_string())),
      ],
      "2026-09-16T10:00:00Z".to_string(),
    );

    let link = Link {
      id: "abc".to_string(),
      master: Endpoint {
        source: Source::Spotify,
        playlist_uri: "spotify:playlist:1".to_string(),
        name: "Mine".to_string(),
      },
      mirrors: vec![first, second],
    };
    assert_eq!(link.unmatched_count(), 3);
  }

  #[test]
  fn a_link_report_folds_its_mirrors_and_a_failure_outranks_a_skip() {
    let mut report = LinkReport::new("abc123", "Road Trip");
    assert_eq!(report.id, "abc123");
    assert_eq!(report.outcome, LinkOutcome::Ran);

    report.absorb(MirrorRun {
      added: 2,
      removed: 1,
      unmatched: 3,
      outcome: LinkOutcome::Ran,
    });
    report.absorb(MirrorRun {
      added: 1,
      removed: 0,
      unmatched: 1,
      outcome: LinkOutcome::Skipped("Qobuz is not logged in".to_string()),
    });
    assert_eq!(report.added, 3);
    assert_eq!(report.removed, 1);
    assert_eq!(report.unmatched, 4);
    assert_eq!(
      report.outcome,
      LinkOutcome::Skipped("Qobuz is not logged in".to_string())
    );

    report.note(LinkOutcome::Skipped("a later skip".to_string()));
    assert_eq!(
      report.outcome,
      LinkOutcome::Skipped("Qobuz is not logged in".to_string())
    );

    report.note(LinkOutcome::Failed("429 rate limited".to_string()));
    report.note(LinkOutcome::Skipped("too late".to_string()));
    assert_eq!(
      report.outcome,
      LinkOutcome::Failed("429 rate limited".to_string())
    );
  }

  #[test]
  fn a_report_line_names_the_counts_a_skip_or_a_failure() {
    let mut ran = LinkReport::new("abc123", "Road Trip");
    ran.absorb(MirrorRun {
      added: 3,
      removed: 1,
      unmatched: 2,
      outcome: LinkOutcome::Ran,
    });
    assert_eq!(ran.line(), "Road Trip: 3 added, 1 removed, 2 unmatched");

    let mut skipped = LinkReport::new("def456", "Focus");
    skipped.note(LinkOutcome::Skipped("Spotify is not connected".to_string()));
    assert_eq!(skipped.line(), "Focus: skipped (Spotify is not connected)");

    let mut failed = LinkReport::new("ghi789", "Gym");
    failed.note(LinkOutcome::Failed("429 rate limited".to_string()));
    assert_eq!(failed.line(), "Gym: failed (429 rate limited)");
  }

  #[test]
  fn the_summary_totals_every_link_and_a_run_error_wins() {
    let empty = SyncReport::default();
    assert_eq!(empty.summary(), "Playlist sync: no links");
    assert!(!empty.failed());

    let mut first = LinkReport::new("abc123", "Road Trip");
    first.absorb(MirrorRun {
      added: 3,
      removed: 1,
      unmatched: 2,
      outcome: LinkOutcome::Ran,
    });
    let mut second = LinkReport::new("def456", "Focus");
    second.note(LinkOutcome::Skipped("Qobuz is not logged in".to_string()));

    let mut report = SyncReport {
      links: vec![first, second],
      error: None,
      dry_run: false,
    };
    assert_eq!(report.added(), 3);
    assert_eq!(report.removed(), 1);
    assert_eq!(report.unmatched(), 2);
    assert_eq!(
      report.summary(),
      "Playlist sync: 3 added, 1 removed, 2 unmatched"
    );
    assert!(!report.failed());
    assert_eq!(
      report.printable(),
      [
        "Playlist sync: 3 added, 1 removed, 2 unmatched",
        "Road Trip: 3 added, 1 removed, 2 unmatched",
        "Focus: skipped (Qobuz is not logged in)",
      ]
      .join("\n")
    );

    report.dry_run = true;
    assert_eq!(
      report.summary(),
      "Playlist sync (dry run): 3 added, 1 removed, 2 unmatched"
    );

    report.links[1].note(LinkOutcome::Failed("the write was refused".to_string()));
    assert!(report.failed());

    report.error = Some("429 rate limited".to_string());
    assert_eq!(report.summary(), "Playlist sync failed: 429 rate limited");
    assert!(report.failed());
  }

  #[test]
  fn a_link_filter_matches_the_id_exactly_and_the_name_case_insensitively() {
    let link = Link {
      id: "abc123".to_string(),
      master: Endpoint {
        source: Source::Spotify,
        playlist_uri: "spotify:playlist:1".to_string(),
        name: " Road Trip ".to_string(),
      },
      mirrors: Vec::new(),
    };

    assert!(link.matches_filter("abc123"));
    assert!(link.matches_filter("  abc123  "));
    assert!(link.matches_filter("road trip"));
    assert!(link.matches_filter(" ROAD TRIP "));
    assert!(!link.matches_filter("ABC123"));
    assert!(!link.matches_filter("Road"));
    assert!(!link.matches_filter(""));
  }
}
