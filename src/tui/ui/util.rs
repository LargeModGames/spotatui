use crate::core::app::{ActiveBlock, App, ArtistBlock, SearchResultBlock};
use crate::core::user_config::Theme;
use ratatui::{
  layout::Rect,
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, BorderType, Borders, List, ListItem, ListState},
  Frame,
};
use rspotify::model::artist::SimplifiedArtist;
use std::time::Duration;

pub const SMALL_TERMINAL_WIDTH: u16 = 150;
pub const SMALL_TERMINAL_HEIGHT: u16 = 45;

pub fn get_search_results_highlight_state(
  app: &App,
  block_to_match: SearchResultBlock,
) -> (bool, bool) {
  let current_route = app.get_current_route();
  (
    app.search_results.selected_block == block_to_match,
    current_route.hovered_block == ActiveBlock::SearchResultBlock
      && app.search_results.hovered_block == block_to_match,
  )
}

pub fn get_artist_highlight_state(app: &App, block_to_match: ArtistBlock) -> (bool, bool) {
  let current_route = app.get_current_route();
  if let Some(artist) = &app.artist {
    let is_hovered = artist.artist_selected_block == block_to_match;
    let is_selected = current_route.hovered_block == ActiveBlock::ArtistBlock
      && artist.artist_hovered_block == block_to_match;
    (is_hovered, is_selected)
  } else {
    (false, false)
  }
}

pub fn get_color((is_active, is_hovered): (bool, bool), theme: Theme) -> Style {
  match (is_active, is_hovered) {
    (true, _) => Style::default().fg(theme.selected).bg(theme.background),
    (false, true) => Style::default().fg(theme.hovered).bg(theme.background),
    _ => Style::default().fg(theme.inactive).bg(theme.background),
  }
}

pub fn draw_selectable_list<S>(
  f: &mut Frame<'_>,
  app: &App,
  layout_chunk: Rect,
  title: &str,
  items: &[S],
  highlight_state: (bool, bool),
  selected_index: Option<usize>,
) where
  S: std::convert::AsRef<str>,
{
  let mut state = ListState::default();
  state.select(selected_index);

  let lst_items: Vec<ListItem> = items
    .iter()
    .map(|i| ListItem::new(Span::raw(i.as_ref())))
    .collect();

  let block = Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .title(Span::styled(
      title,
      get_color(highlight_state, app.user_config.theme),
    ))
    .border_style(get_color(highlight_state, app.user_config.theme));

  let list = List::new(lst_items)
    .block(block)
    .style(app.user_config.theme.base_style())
    .highlight_style(
      get_color(highlight_state, app.user_config.theme)
        .add_modifier(Modifier::BOLD | Modifier::REVERSED),
    )
    .highlight_symbol(Line::from("▶ ").style(get_color(highlight_state, app.user_config.theme)));
  f.render_stateful_widget(list, layout_chunk, &mut state);
}

pub fn create_artist_string(artists: &[SimplifiedArtist]) -> String {
  artists
    .iter()
    .map(|artist| artist.name.to_string())
    .collect::<Vec<String>>()
    .join(", ")
}

/// Join a domain artist list's display names with `", "` for rendering.
///
/// Source-agnostic counterpart to [`create_artist_string`] (which takes the
/// rspotify `SimplifiedArtist`). Use this once a screen's data is domain-typed
/// ([`crate::core::plugin_api::ArtistRef`], e.g. `AlbumInfo::artists`), so the
/// renderer needs no rspotify types. Note `TrackInfo::artists` is already
/// `Vec<String>` and can be `.join(", ")` directly.
// Wired up by the per-screen migration slices, not yet by the binary.
#[allow(dead_code)]
pub fn join_artist_names(artists: &[crate::core::plugin_api::ArtistRef]) -> String {
  artists
    .iter()
    .map(|artist| artist.name.as_str())
    .collect::<Vec<&str>>()
    .join(", ")
}

pub fn millis_to_minutes(millis: u128) -> String {
  let minutes = millis / 60000;
  let seconds = (millis % 60000) / 1000;
  let seconds_display = if seconds < 10 {
    format!("0{}", seconds)
  } else {
    format!("{}", seconds)
  };

  if seconds == 60 {
    format!("{}:00", minutes + 1)
  } else {
    format!("{}:{}", minutes, seconds_display)
  }
}

pub fn display_track_progress(progress: u128, track_duration: Duration) -> String {
  let duration = millis_to_minutes(track_duration.as_millis());
  let progress_display = millis_to_minutes(progress);
  let remaining = millis_to_minutes(track_duration.as_millis().saturating_sub(progress));

  format!("{}/{} (-{})", progress_display, duration, remaining,)
}

pub fn truncate_text(s: &str, max_chars: usize) -> String {
  if s.chars().count() <= max_chars {
    s.to_string()
  } else {
    let mut truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
  }
}

// `percentage` param needs to be between 0 and 1
pub fn get_percentage_width(width: u16, percentage: f32) -> u16 {
  let padding = 3;
  let width = width - padding;
  (f32::from(width) * percentage) as u16
}

// Ensure track progress percentage is between 0 and 100 inclusive
pub fn get_track_progress_percentage(song_progress_ms: u128, track_duration: Duration) -> u16 {
  let min_perc = 0_f64;
  let track_progress = std::cmp::min(song_progress_ms, track_duration.as_millis());
  let track_perc = (track_progress as f64 / track_duration.as_millis() as f64) * 100_f64;
  min_perc.max(track_perc) as u16
}

// Make better use of space on small terminals
pub fn get_main_layout_margin(app: &App) -> u16 {
  if app.size.height > SMALL_TERMINAL_HEIGHT {
    1
  } else {
    0
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn join_artist_names_test() {
    use crate::core::plugin_api::ArtistRef;
    let artists = vec![
      ArtistRef {
        id: None,
        name: "Daft Punk".to_string(),
      },
      ArtistRef {
        id: Some("abc".to_string()),
        name: "Pharrell".to_string(),
      },
    ];
    assert_eq!(join_artist_names(&artists), "Daft Punk, Pharrell");
    assert_eq!(join_artist_names(&[]), "");
  }

  #[test]
  fn millis_to_minutes_test() {
    assert_eq!(millis_to_minutes(0), "0:00");
    assert_eq!(millis_to_minutes(1000), "0:01");
    assert_eq!(millis_to_minutes(1500), "0:01");
    assert_eq!(millis_to_minutes(1900), "0:01");
    assert_eq!(millis_to_minutes(60 * 1000), "1:00");
    assert_eq!(millis_to_minutes(60 * 1500), "1:30");
  }

  #[test]
  fn display_track_progress_test() {
    let two_minutes = Duration::from_millis(2 * 60 * 1000);
    assert_eq!(display_track_progress(0, two_minutes), "0:00/2:00 (-2:00)");
    assert_eq!(
      display_track_progress(Duration::from_millis(60 * 1000).as_millis(), two_minutes),
      "1:00/2:00 (-1:00)"
    );
  }

  #[test]
  fn get_track_progress_percentage_test() {
    let track_length = Duration::from_millis(60 * 1000);
    assert_eq!(get_track_progress_percentage(0, track_length), 0);
    assert_eq!(
      get_track_progress_percentage((60 * 1000) / 2, track_length),
      50
    );

    // If progress is somehow higher than total duration, 100 should be max
    assert_eq!(
      get_track_progress_percentage(60 * 1000 * 2, track_length),
      100
    );
  }
}
