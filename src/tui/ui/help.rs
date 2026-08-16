use crate::core::app::App;

/// Return Help rows matching the active filter.
/// Whitespace-separated terms are ANDed and matching is case-insensitive unless
/// a term contains an uppercase ASCII character.
pub fn get_filtered_help_docs(app: &App) -> Vec<Vec<String>> {
  filter_help_docs(get_help_docs(app), &app.help_filter)
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

pub fn get_help_docs(app: &App) -> Vec<Vec<String>> {
  let key_bindings = &app.user_config.keys;
  vec![
    vec![
      String::from("Scroll down to next result page"),
      key_bindings.next_page.to_string(),
      String::from("Pagination"),
    ],
    vec![
      String::from("Scroll up to previous result page"),
      key_bindings.previous_page.to_string(),
      String::from("Pagination"),
    ],
    vec![
      String::from("Jump to start of playlist"),
      key_bindings.jump_to_start.to_string(),
      String::from("Pagination"),
    ],
    vec![
      String::from("Jump to end of playlist"),
      key_bindings.jump_to_end.to_string(),
      String::from("Pagination"),
    ],
    vec![
      String::from("Jump to currently playing album"),
      key_bindings.jump_to_album.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Jump to currently playing artist's album list"),
      key_bindings.jump_to_artist_album.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Jump to current play context"),
      key_bindings.jump_to_context.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Increase volume by 10%"),
      key_bindings.increase_volume.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Decrease volume by 10%"),
      key_bindings.decrease_volume.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Skip to next track"),
      key_bindings.next_track.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Skip to previous track"),
      key_bindings.previous_track.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Force skip to previous track"),
      key_bindings.force_previous_track.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Seek backwards 5 seconds"),
      key_bindings.seek_backwards.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Seek forwards 5 seconds"),
      key_bindings.seek_forwards.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Toggle shuffle"),
      key_bindings.shuffle.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Copy url to currently playing song/episode"),
      key_bindings.copy_song_url.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Copy url to currently playing album/show"),
      key_bindings.copy_album_url.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Cycle repeat mode"),
      key_bindings.repeat.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Move selection left"),
      format!("{} | <Left Arrow Key> | <Ctrl+b>", key_bindings.move_left),
      String::from("General"),
    ],
    vec![
      String::from("Move selection down"),
      format!("{} | <Down Arrow Key> | <Ctrl+n>", key_bindings.move_down),
      String::from("General"),
    ],
    vec![
      String::from("Move selection up"),
      format!("{} | <Up Arrow Key> | <Ctrl+p>", key_bindings.move_up),
      String::from("General"),
    ],
    vec![
      String::from("Move selection right"),
      format!("{} | <Right Arrow Key> | <Ctrl+f>", key_bindings.move_right),
      String::from("General (Ctrl+f searches inside playlist track tables)"),
    ],
    vec![
      String::from("Move selection to top of list"),
      String::from("H"),
      String::from("General"),
    ],
    vec![
      String::from("Move selection to middle of list"),
      String::from("M"),
      String::from("General"),
    ],
    vec![
      String::from("Move selection to bottom of list"),
      String::from("L"),
      String::from("General"),
    ],
    vec![
      String::from("Enter input for search"),
      key_bindings.search.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Filter help rows"),
      key_bindings.search.to_string(),
      String::from("Help menu"),
    ],
    vec![
      String::from("Filter settings rows"),
      key_bindings.search.to_string(),
      String::from("Settings"),
    ],
    vec![
      String::from("Pause/Resume playback"),
      key_bindings.toggle_playback.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Enter active mode"),
      String::from("<Enter>"),
      String::from("General"),
    ],
    vec![
      String::from("Go to audio analysis screen"),
      key_bindings.audio_analysis.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Cycle visualizer style (in audio analysis)"),
      String::from("V"),
      String::from("General"),
    ],
    vec![
      String::from("Go to lyrics view"),
      key_bindings.lyrics_view.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Scroll lyrics (pauses auto-follow)"),
      format!(
        "{}/{} | <Up>/<Down> | <Ctrl+p>/<Ctrl+n>",
        key_bindings.move_up, key_bindings.move_down
      ),
      String::from("Lyrics view"),
    ],
    vec![
      String::from("Resume following the current lyric line"),
      String::from("f or <Esc>"),
      String::from("Lyrics view"),
    ],
    vec![
      String::from("Nudge lyric timing earlier/later"),
      format!(
        "{}/{} | <Right>/<Left> | <Ctrl+f>/<Ctrl+b>",
        key_bindings.move_right, key_bindings.move_left
      ),
      String::from("Lyrics view"),
    ],
    vec![
      String::from("Toggle miniplayer view"),
      key_bindings.miniplayer_view.to_string(),
      String::from("General"),
    ],
    #[cfg(feature = "cover-art")]
    vec![
      String::from("Go to cover art view"),
      key_bindings.cover_art_view.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Go back or exit when nowhere left to back to"),
      key_bindings.back.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Switch music source / select playback device"),
      key_bindings.manage_devices.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Open settings"),
      app.effective_open_settings_key().to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Save settings"),
      app.effective_save_settings_key().to_string(),
      String::from("Settings"),
    ],
    vec![
      String::from("Enter hover mode"),
      String::from("<Esc>"),
      String::from("Selected block"),
    ],
    vec![
      String::from("Save track in list or table"),
      String::from("s"),
      String::from("Selected block"),
    ],
    vec![
      String::from("Add selected track to playlist"),
      String::from("w"),
      String::from("Track table / search songs / artist top tracks / recently played"),
    ],
    vec![
      String::from("Add currently playing track to playlist"),
      String::from("w"),
      String::from("Playbar"),
    ],
    vec![
      String::from("Quick-add currently playing track to playlist"),
      String::from("W"),
      String::from("Global"),
    ],
    vec![
      String::from("Decrease sidebar width"),
      String::from("{"),
      String::from("Layout"),
    ],
    vec![
      String::from("Increase sidebar width"),
      String::from("}"),
      String::from("Layout"),
    ],
    vec![
      String::from("Decrease playbar or library height"),
      String::from("("),
      String::from("Layout"),
    ],
    vec![
      String::from("Increase playbar or library height"),
      String::from(")"),
      String::from("Layout"),
    ],
    vec![
      String::from("Reset layout to defaults"),
      String::from("|"),
      String::from("Layout"),
    ],
    vec![
      String::from("Remove selected track from current playlist"),
      String::from("x"),
      String::from("Track table (playlist views)"),
    ],
    vec![
      String::from("Search tracks in current playlist"),
      String::from("<Ctrl+f>"),
      String::from("Track table (playlist views)"),
    ],
    vec![
      String::from("Clear playlist track search filter"),
      key_bindings.back.to_string(),
      String::from("Track table (filtered playlist views)"),
    ],
    vec![
      String::from("Start playback or enter album/artist/playlist"),
      key_bindings.submit.to_string(),
      String::from("Selected block"),
    ],
    vec![
      String::from("Play recommendations for song/artist"),
      String::from("r"),
      String::from("Selected block"),
    ],
    vec![
      String::from("Play all tracks for artist"),
      String::from("e"),
      String::from("Library -> Artists"),
    ],
    vec![
      String::from("Search with input text"),
      String::from("<Enter>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Move cursor one space left"),
      String::from("<Left Arrow Key>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Move cursor one space right"),
      String::from("<Right Arrow Key>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Delete entire input"),
      String::from("<Ctrl+l>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Delete text from cursor to start of input"),
      String::from("<Ctrl+u>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Delete text from cursor to end of input"),
      String::from("<Ctrl+k>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Delete previous word"),
      String::from("<Ctrl+w>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Jump to start of input"),
      String::from("<Ctrl+a>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Jump to end of input"),
      String::from("<Ctrl+e>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Escape from the input back to hovered block"),
      String::from("<Esc>"),
      String::from("Search input"),
    ],
    vec![
      String::from("Delete saved album"),
      String::from("D"),
      String::from("Library -> Albums"),
    ],
    vec![
      String::from("Delete saved playlist"),
      String::from("D"),
      String::from("Playlist"),
    ],
    vec![
      String::from("Remove favorite radio station"),
      String::from("D"),
      String::from("Radio"),
    ],
    vec![
      String::from("Follow an artist/playlist"),
      String::from("w"),
      String::from("Search result"),
    ],
    vec![
      String::from("Save (like) album to library"),
      String::from("w"),
      String::from("Search result"),
    ],
    vec![
      String::from("Play random song in playlist"),
      String::from("S"),
      String::from("Selected Playlist"),
    ],
    vec![
      String::from("Toggle sort order of podcast episodes"),
      String::from("S"),
      String::from("Selected Show"),
    ],
    vec![
      String::from("Add track to queue"),
      key_bindings.add_item_to_queue.to_string(),
      String::from("Hovered over track"),
    ],
    vec![
      String::from("Show queue"),
      key_bindings.show_queue.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Remove selected track from queue"),
      key_bindings.remove_from_queue.to_string(),
      String::from("Queue"),
    ],
    vec![
      String::from("Move selected queue item down / up"),
      String::from("J / K"),
      String::from("Queue"),
    ],
    vec![
      String::from("Play selected queue item (skip ahead to it)"),
      String::from("<Enter>"),
      String::from("Queue"),
    ],
    vec![
      String::from("Toggle saved state for currently playing track/episode"),
      key_bindings.like_track.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Favorite highlighted/playing radio station"),
      key_bindings.like_track.to_string(),
      String::from("Radio"),
    ],
    vec![
      String::from("Generate listening recap card (selected period on Stats, 30 days elsewhere)"),
      key_bindings.generate_recap.to_string(),
      String::from("General"),
    ],
    vec![
      String::from("Open Stats screen"),
      String::from("Library sidebar"),
      String::from("Stats"),
    ],
    vec![
      String::from("Cycle stats period"),
      String::from("[ / ]"),
      String::from("Stats"),
    ],
    vec![
      String::from("Play selected top track"),
      String::from("<Enter>"),
      String::from("Stats"),
    ],
    vec![
      String::from("Open sort menu"),
      String::from(","),
      String::from("Track/Album/Artist list"),
    ],
    vec![
      String::from("Open Listening Party menu"),
      key_bindings.listening_party.to_string(),
      String::from("General"),
    ],
    // `docs/keybindings.md` promises `?` lists every binding, and until now none of
    // the DJ's five were here.
    #[cfg(feature = "ai-dj")]
    vec![
      String::from("Open the AI DJ screen"),
      key_bindings.dj_open.to_string(),
      String::from("AI DJ"),
    ],
    #[cfg(feature = "ai-dj")]
    vec![
      String::from("Toggle DJ auto-queue (keep the queue topped up)"),
      key_bindings.dj_toggle_auto_queue.to_string(),
      String::from("AI DJ"),
    ],
    #[cfg(feature = "ai-dj")]
    vec![
      String::from("Shift the vibe (drop the DJ's queued tracks and re-ask)"),
      key_bindings.dj_vibe_shift.to_string(),
      String::from("AI DJ"),
    ],
    #[cfg(feature = "ai-dj")]
    vec![
      String::from("Toggle \"only tracks I do not already have\""),
      key_bindings.dj_toggle_fresh_only.to_string(),
      String::from("AI DJ"),
    ],
    #[cfg(feature = "ai-dj")]
    vec![
      String::from("Choose which AI and model the DJ uses"),
      key_bindings.dj_pick_model.to_string(),
      String::from("AI DJ"),
    ],
  ]
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
