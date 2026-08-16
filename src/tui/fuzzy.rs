//! Subsequence fuzzy matching for the Settings row filter.
//!
//! Query characters have to appear in the haystack in order but need not be
//! adjacent, so `skms` still finds `Seek Milliseconds`. Whitespace in the query
//! is ignored entirely, which lets one query land on both the human name and
//! the config id of the same setting: `vol inc` matches `Volume Increment` and
//! `behavior.volume_increment` alike.
//!
//! Case handling follows the same smart-case rule as the Help filter: an
//! all-lowercase query matches case-insensitively, any uppercase character in
//! the query makes the whole match case-sensitive.

/// Every matched character is worth this much before bonuses.
const SCORE_MATCH: i32 = 8;
/// A character that starts a word: string start, after a separator, or the
/// upper half of a camelCase hump.
const BONUS_BOUNDARY: i32 = 12;
/// A character matched directly after the previous one. Deliberately larger
/// than [`BONUS_BOUNDARY`]: a run that starts on a word boundary collects both,
/// which is what keeps `rep` on `Repeat` instead of `Recently Played Sort`.
const BONUS_CONSECUTIVE: i32 = 20;
/// Charged per haystack character skipped before the first match (capped), so
/// `text` ranks `Text Emphasis` above `Playbar Progress Text`.
const MAX_LEADING_PENALTY: i32 = 12;

/// One successful match: how well the query fits, and where it landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyMatch {
  /// Higher is a better fit. Only comparable between matches of the same query.
  pub score: i32,
  /// Ascending, non-overlapping byte ranges into the haystack, adjacent
  /// characters merged into one range.
  pub ranges: Vec<(usize, usize)>,
}

/// Best-scoring subsequence match of `query` in `haystack`, or `None` when the
/// characters do not appear in order. An all-whitespace query matches
/// everything with a zero score and no ranges.
pub fn fuzzy_match(haystack: &str, query: &str) -> Option<FuzzyMatch> {
  let case_sensitive = query.chars().any(char::is_uppercase);
  let needle: Vec<char> = query
    .chars()
    .filter(|c| !c.is_whitespace())
    .map(|c| fold_case(c, case_sensitive))
    .collect();
  if needle.is_empty() {
    return Some(FuzzyMatch {
      score: 0,
      ranges: Vec::new(),
    });
  }

  let hay: Vec<(usize, char)> = haystack.char_indices().collect();
  let folded: Vec<char> = hay
    .iter()
    .map(|(_, c)| fold_case(*c, case_sensitive))
    .collect();

  // Try every place the first query character can land and match the rest
  // greedily from there, keeping the best placement. A single left-to-right
  // scan would take the scattered `a`, `b`, `c` of "a b c abc" instead of the
  // tight run at the end, which shows up directly as badly placed highlights.
  let mut best: Option<FuzzyMatch> = None;
  for start in 0..hay.len() {
    if folded[start] != needle[0] {
      continue;
    }
    let mut positions = vec![start];
    let mut cursor = start + 1;
    for needle_char in &needle[1..] {
      let Some(offset) = folded[cursor..].iter().position(|c| c == needle_char) else {
        break;
      };
      positions.push(cursor + offset);
      cursor += offset + 1;
    }
    if positions.len() < needle.len() {
      // Every later start has fewer characters left to match with.
      break;
    }

    let mut score = -(start as i32).min(MAX_LEADING_PENALTY);
    for (i, &j) in positions.iter().enumerate() {
      score += SCORE_MATCH;
      if is_boundary(&hay, j) {
        score += BONUS_BOUNDARY;
      }
      if i > 0 && positions[i - 1] + 1 == j {
        score += BONUS_CONSECUTIVE;
      }
    }
    // Earliest placement wins ties, so equal matches highlight deterministically.
    if best.as_ref().is_none_or(|b| score > b.score) {
      let mut ranges: Vec<(usize, usize)> = Vec::new();
      for &j in &positions {
        let (byte, character) = hay[j];
        let end = byte + character.len_utf8();
        match ranges.last_mut() {
          Some(last) if last.1 == byte => last.1 = end,
          _ => ranges.push((byte, end)),
        }
      }
      best = Some(FuzzyMatch { score, ranges });
    }
  }
  best
}

fn fold_case(character: char, case_sensitive: bool) -> char {
  if case_sensitive {
    character
  } else {
    character.to_lowercase().next().unwrap_or(character)
  }
}

fn is_boundary(hay: &[(usize, char)], index: usize) -> bool {
  if index == 0 {
    return true;
  }
  let previous = hay[index - 1].1;
  let current = hay[index].1;
  !previous.is_alphanumeric() || (previous.is_lowercase() && current.is_uppercase())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn score(haystack: &str, query: &str) -> i32 {
    fuzzy_match(haystack, query)
      .unwrap_or_else(|| panic!("expected {query:?} to match {haystack:?}"))
      .score
  }

  fn matched(haystack: &str, query: &str) -> String {
    fuzzy_match(haystack, query)
      .unwrap_or_else(|| panic!("expected {query:?} to match {haystack:?}"))
      .ranges
      .iter()
      .map(|(start, end)| &haystack[*start..*end])
      .collect()
  }

  #[test]
  fn characters_may_be_skipped_but_not_reordered_and_whitespace_is_ignored() {
    assert!(fuzzy_match("Seek Milliseconds", "skms").is_some());
    assert!(fuzzy_match("Seek Milliseconds", "smks").is_none());
    assert!(fuzzy_match("ms", "milliseconds").is_none());
    assert!(fuzzy_match("Volume Increment", "vol inc").is_some());
    assert!(fuzzy_match("behavior.volume_increment", "vol inc").is_some());
    let blank = fuzzy_match("Seek Duration (ms)", "   ").expect("blank query matches");
    assert_eq!(blank.score, 0);
    assert!(blank.ranges.is_empty());
  }

  #[test]
  fn smart_case_only_applies_when_the_query_has_uppercase() {
    assert!(fuzzy_match("Volume Increment", "volume").is_some());
    assert!(fuzzy_match("Volume Increment", "Volume").is_some());
    assert!(fuzzy_match("volume increment", "Volume").is_none());
  }

  #[test]
  fn tight_early_word_start_matches_outrank_scattered_late_buried_ones() {
    assert!(score("Repeat", "rep") > score("Recently Played Sort", "rep"));
    assert!(score("Seek Forwards", "sf") > score("Class of", "sf"));
    assert!(score("Text Emphasis", "text") > score("Playbar Progress Text", "text"));
  }

  #[test]
  fn ranges_are_merged_byte_offsets_of_the_best_placement() {
    assert_eq!(
      fuzzy_match("Volume Increment", "volinc")
        .expect("match")
        .ranges,
      vec![(0, 3), (7, 10)]
    );
    // A greedy scan would take the leading `a`, `b`, `c` and highlight three
    // scattered characters; the tight run at the end is the better match.
    assert_eq!(matched("a b c abc", "abc"), "abc");
    assert_eq!(matched("Émphasis ícon", "éí"), "Éí");
  }
}
