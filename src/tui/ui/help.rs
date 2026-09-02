use crate::core::app::App;

/// Return Help rows matching the active filter.
/// Whitespace-separated terms are ANDed and matching is case-insensitive unless
/// a term contains an uppercase ASCII character.
pub fn get_filtered_help_docs(app: &App) -> Vec<Vec<String>> {
  filter_help_docs(crate::tui::keymap::help_rows(app), &app.view.help_filter)
}

fn filter_help_docs(rows: Vec<Vec<String>>, filter: &str) -> Vec<Vec<String>> {
  let terms = filter.split_whitespace().collect::<Vec<_>>();
  if terms.is_empty() {
    return rows;
  }

  rows
    .into_iter()
    .filter(|row| {
      terms
        .iter()
        .all(|term| row.iter().any(|field| help_field_contains(field, term)))
    })
    .collect()
}

fn help_field_contains(field: &str, term: &str) -> bool {
  if term.chars().any(|c| c.is_ascii_uppercase()) {
    field.contains(term)
  } else {
    field.to_lowercase().contains(&term.to_lowercase())
  }
}

/// All byte offsets where `needle` starts in `haystack`, including overlapping
/// occurrences, which `str::match_indices` skips (it resumes after each match).
/// After each hit the search resumes one char later, not one match later.
fn overlapping_match_starts(haystack: &str, needle: &str) -> Vec<usize> {
  let mut starts = Vec::new();
  let mut from = 0;
  while let Some(pos) = haystack[from..].find(needle) {
    let start = from + pos;
    starts.push(start);
    from = start + haystack[start..].chars().next().map_or(1, char::len_utf8);
  }
  starts
}

/// Byte ranges within a rendered help line where filter terms match, sorted and
/// merged, so the Help table can highlight why each row survived the filter.
/// Case sensitivity follows the same smart-case rule as [`help_field_contains`].
pub fn help_match_ranges(line: &str, filter: &str) -> Vec<(usize, usize)> {
  let mut ranges: Vec<(usize, usize)> = Vec::new();
  // Lowercasing can change byte lengths for non-ASCII text, so offsets into the
  // lowercased haystack do not index `line` directly. Record, for every byte of
  // the lowercased haystack, the byte range of the original char it came from,
  // and map matches back through that table. Built lazily on the first
  // case-insensitive term.
  let mut haystack = String::new();
  let mut byte_map: Vec<(usize, usize)> = Vec::new();
  for term in filter.split_whitespace() {
    if term.chars().any(|c| c.is_ascii_uppercase()) {
      for start in overlapping_match_starts(line, term) {
        ranges.push((start, start + term.len()));
      }
    } else {
      if haystack.is_empty() {
        for (orig_start, c) in line.char_indices() {
          let orig_end = orig_start + c.len_utf8();
          for lowered in c.to_lowercase() {
            for _ in 0..lowered.len_utf8() {
              byte_map.push((orig_start, orig_end));
            }
            haystack.push(lowered);
          }
        }
      }
      let needle = term.to_lowercase();
      for start in overlapping_match_starts(&haystack, &needle) {
        // A match may start or end inside the multi-char lowering of a single
        // original char; widen it to whole original chars.
        ranges.push((byte_map[start].0, byte_map[start + needle.len() - 1].1));
      }
    }
  }
  ranges.sort_unstable();
  let mut merged: Vec<(usize, usize)> = Vec::new();
  for (start, end) in ranges {
    match merged.last_mut() {
      Some(last) if start <= last.1 => last.1 = last.1.max(end),
      _ => merged.push((start, end)),
    }
  }
  merged
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rows() -> Vec<Vec<String>> {
    vec![
      vec![
        "Increase volume by 10%".to_string(),
        "+".to_string(),
        "General".to_string(),
      ],
      vec![
        "Search tracks in current playlist".to_string(),
        "<Ctrl+f>".to_string(),
        "Track table (playlist views)".to_string(),
      ],
      vec![
        "Open settings".to_string(),
        "<Alt+,>".to_string(),
        "General".to_string(),
      ],
    ]
  }

  #[test]
  fn help_filter_matches_terms_across_columns() {
    let filtered = filter_help_docs(rows(), "ctrl playlist");

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0][0], "Search tracks in current playlist");
  }

  #[test]
  fn help_filter_uses_smart_case() {
    assert_eq!(filter_help_docs(rows(), "VOLUME").len(), 0);
    assert_eq!(filter_help_docs(rows(), "volume").len(), 1);
    assert_eq!(filter_help_docs(rows(), "Increase").len(), 1);
  }

  #[test]
  fn empty_help_filter_preserves_all_rows_and_order() {
    let rows = rows();

    assert_eq!(filter_help_docs(rows.clone(), "   "), rows);
  }

  #[test]
  fn help_match_ranges_finds_every_occurrence_of_every_term() {
    let ranges = help_match_ranges("Increase volume by 10%  +  General", "volume general");

    assert_eq!(ranges, vec![(9, 15), (27, 34)]);
  }

  #[test]
  fn help_match_ranges_uses_smart_case() {
    assert!(help_match_ranges("Increase volume", "VOLUME").is_empty());
    assert_eq!(
      help_match_ranges("Increase volume", "Increase"),
      vec![(0, 8)]
    );
  }

  #[test]
  fn help_match_ranges_merges_overlapping_matches() {
    assert_eq!(help_match_ranges("aaaa", "aa aaa"), vec![(0, 4)]);
  }

  #[test]
  fn help_match_ranges_finds_overlapping_occurrences_of_one_term() {
    // `str::match_indices` alone would stop at (0, 2), leaving the last 'a'
    // unhighlighted.
    assert_eq!(help_match_ranges("aaa", "aa"), vec![(0, 3)]);
    assert_eq!(help_match_ranges("AAA", "AA"), vec![(0, 3)]);
  }

  #[test]
  fn help_match_ranges_maps_offsets_when_lowercasing_changes_byte_lengths() {
    // 'İ' (2 bytes) lowercases to "i\u{307}" (3 bytes), shifting every
    // lowercased offset after it by one.
    assert_eq!(help_match_ranges("İ volume", "volume"), vec![(3, 9)]);

    // 'ẞ' (3 bytes) lowercases to 'ß' (2 bytes), so with a leading 'İ' the
    // total byte length is unchanged while offsets in between are still
    // shifted — a length check alone cannot catch this.
    let line = "İ volume ẞ";
    assert_eq!(line.len(), line.to_lowercase().len());
    let ranges = help_match_ranges(line, "volume");
    assert_eq!(ranges, vec![(3, 9)]);
    assert_eq!(&line[3..9], "volume");
  }

  #[test]
  fn help_match_ranges_widens_matches_to_original_char_boundaries() {
    // "i\u{307}" matches inside the lowering of 'İ'; the highlight covers the
    // whole original char rather than slicing mid-char.
    assert_eq!(help_match_ranges("İx", "i\u{307}"), vec![(0, 2)]);
  }
}
