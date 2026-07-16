use crate::core::app::{active_lyric_index, App, LyricsStatus};
use crate::core::layout::fullscreen_view_layout;
use ratatui::{
  layout::{Alignment, Rect},
  style::{Modifier, Style},
  widgets::{Block, Borders, Paragraph},
  Frame,
};

use super::player::draw_playbar;

pub fn draw_lyrics_view(f: &mut Frame<'_>, app: &App) {
  let (content_area, playbar_area) = fullscreen_view_layout(&app.user_config.behavior, f.area());

  draw_lyrics(f, app, content_area);
  if let Some(playbar_area) = playbar_area {
    draw_playbar(f, app, playbar_area);
  }
}

fn draw_lyrics(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = &app.user_config.theme;

  let mut notes: Vec<String> = Vec::new();
  if app.lyrics_status == LyricsStatus::Found {
    if !app.lyrics_synced {
      notes.push("timing estimated".to_string());
    }
    let offset_ms = app.lyrics_view.timing_offset_ms;
    if offset_ms != 0 {
      notes.push(format!("offset {:+.1}s", offset_ms as f64 / 1000.0));
    }
  }
  let title = if notes.is_empty() {
    " Lyrics ".to_string()
  } else {
    format!(" Lyrics ({}) ", notes.join(", "))
  };
  let block = Block::default()
    .borders(Borders::ALL)
    .title(title)
    .style(Style::default().fg(theme.inactive));
  f.render_widget(block.clone(), area);

  let inner_area = block.inner(area);
  if inner_area.width == 0 || inner_area.height == 0 {
    return;
  }

  if app.lyrics_status != LyricsStatus::Found {
    draw_state_message(f, app, inner_area);
    return;
  }

  let Some(lyrics) = &app.lyrics else {
    return;
  };
  if lyrics.is_empty() {
    return;
  }

  // Reserve the bottom row for the browsing hint while in manual mode.
  let manual = app.lyrics_view.manual_index.is_some();
  let mut lyric_area = inner_area;
  if manual && lyric_area.height > 1 {
    lyric_area.height -= 1;
  }

  let active_idx = active_lyric_index(lyrics, app.lyric_progress_ms());
  let focus_idx = app
    .lyrics_view
    .manual_index
    .unwrap_or(active_idx)
    .min(lyrics.len() - 1);

  // The focused line sits at the vertical center; `scroll_pos` is the eased
  // fractional line index, so the whole column glides between lines instead
  // of snapping (line spacing stays intact because every line shares the same
  // rounding offset).
  let target_row = i64::from(lyric_area.y) + i64::from(lyric_area.height / 2);
  let scroll_offset = app.lyrics_view.scroll_pos;

  for (line_idx, (_, text)) in lyrics.iter().enumerate() {
    let y = target_row + (line_idx as f64 - scroll_offset).round() as i64;
    if y < i64::from(lyric_area.y) || y >= i64::from(lyric_area.y) + i64::from(lyric_area.height) {
      continue;
    }

    let style = if line_idx == active_idx {
      Style::default()
        .fg(theme.highlighted_lyrics)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD))
    } else if manual && line_idx == focus_idx {
      Style::default().fg(theme.hovered)
    } else {
      Style::default().fg(theme.inactive)
    };

    f.render_widget(
      Paragraph::new(text.clone())
        .style(style)
        .alignment(Alignment::Center),
      Rect {
        x: lyric_area.x,
        y: y as u16,
        width: lyric_area.width,
        height: 1,
      },
    );
  }

  if manual {
    f.render_widget(
      Paragraph::new("browsing · ↑/↓ scroll · Esc follow")
        .style(Style::default().fg(theme.hint))
        .alignment(Alignment::Center),
      Rect {
        y: inner_area.y + inner_area.height - 1,
        height: 1,
        ..inner_area
      },
    );
  }
}

/// Centered two-line message for the non-Found lyric states.
fn draw_state_message(f: &mut Frame<'_>, app: &App, inner_area: Rect) {
  let (primary, secondary) = match app.lyrics_status {
    LyricsStatus::Loading => ("Fetching lyrics…", "from LRCLIB"),
    LyricsStatus::NotFound => ("No lyrics for this track", "lyrics provided by LRCLIB"),
    LyricsStatus::NotStarted => ("Nothing playing", "start a track to see lyrics"),
    LyricsStatus::Found => return,
  };

  let theme = &app.user_config.theme;
  let primary_y = inner_area.y + inner_area.height.saturating_sub(1) / 2;
  f.render_widget(
    Paragraph::new(primary)
      .style(Style::default().fg(theme.text))
      .alignment(Alignment::Center),
    Rect {
      y: primary_y,
      height: 1,
      ..inner_area
    },
  );
  let secondary_y = primary_y + 1;
  if secondary_y < inner_area.y + inner_area.height {
    f.render_widget(
      Paragraph::new(secondary)
        .style(Style::default().fg(theme.inactive))
        .alignment(Alignment::Center),
      Rect {
        y: secondary_y,
        height: 1,
        ..inner_area
      },
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

  fn app_with_lyrics() -> App {
    let mut app = App::default();
    app.lyrics = Some(vec![
      (0, "first line".to_string()),
      (10_000, "second line".to_string()),
      (20_000, "third line".to_string()),
    ]);
    app.lyrics_status = LyricsStatus::Found;
    app.lyrics_synced = true;
    app.song_progress_ms = 11_000;
    app.lyrics_view.scroll_pos = 1.0;
    app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);
    app
  }

  fn render(app: &App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| draw_lyrics_view(f, app)).unwrap();
    terminal.backend().buffer().clone()
  }

  fn buffer_text(buffer: &Buffer) -> String {
    (0..buffer.area().height)
      .map(|y| {
        (0..buffer.area().width)
          .filter_map(|x| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
          .collect::<String>()
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  fn row_of(buffer: &Buffer, needle: &str) -> Option<u16> {
    (0..buffer.area().height).find(|&y| {
      (0..buffer.area().width)
        .filter_map(|x| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
        .collect::<String>()
        .contains(needle)
    })
  }

  #[test]
  fn renders_all_lines_with_active_centered() {
    let app = app_with_lyrics();
    let buffer = render(&app);
    let text = buffer_text(&buffer);
    assert!(text.contains(" Lyrics "), "missing block title:\n{text}");
    assert!(text.contains("first line"), "missing prior line:\n{text}");
    assert!(text.contains("second line"), "missing active line:\n{text}");
    assert!(text.contains("third line"), "missing next line:\n{text}");

    // scroll_pos = 1.0 puts the active line at the vertical center of the
    // lyric area.
    let active_row = row_of(&buffer, "second line").expect("active line row");
    let expected = 1 + (24 - app.user_config.behavior.playbar_height_rows - 2) / 2;
    assert_eq!(active_row, expected);
  }

  #[test]
  fn fractional_scroll_shifts_the_whole_column() {
    let mut app = app_with_lyrics();
    let before = row_of(&render(&app), "second line").expect("active line row");
    app.lyrics_view.scroll_pos = 1.6;
    let buffer = render(&app);
    let after = row_of(&buffer, "second line").expect("active line row");
    assert_eq!(after, before - 1, "column should glide up mid-animation");
    // Line spacing stays intact while gliding.
    let third = row_of(&buffer, "third line").expect("next line row");
    assert_eq!(third, after + 1);
  }

  #[test]
  fn title_notes_estimated_timing_for_plain_lyrics() {
    let mut app = app_with_lyrics();
    app.lyrics_synced = false;
    let text = buffer_text(&render(&app));
    assert!(
      text.contains("Lyrics (timing estimated)"),
      "missing timing hint:\n{text}"
    );
  }

  #[test]
  fn title_shows_timing_offset_and_it_shifts_the_active_line() {
    let mut app = app_with_lyrics();
    // progress 11s + 9.5s nudge = 20.5s, past the third line's timestamp.
    app.lyrics_view.timing_offset_ms = 9_500;
    let buffer = render(&app);
    let text = buffer_text(&buffer);
    assert!(
      text.contains("Lyrics (offset +9.5s)"),
      "missing offset note:\n{text}"
    );
    let active_style = buffer
      .cell((
        buffer.area().width / 2,
        row_of(&buffer, "third line").expect("third line row"),
      ))
      .unwrap()
      .style();
    assert_eq!(
      active_style.fg,
      Some(app.user_config.theme.highlighted_lyrics),
      "nudged line should be the highlighted one"
    );
  }

  #[test]
  fn renders_browsing_hint_in_manual_mode() {
    let mut app = app_with_lyrics();
    app.lyrics_view.manual_index = Some(2);
    let text = buffer_text(&render(&app));
    assert!(text.contains("browsing"), "missing browsing hint:\n{text}");
    assert!(text.contains("third line"), "missing browsed line:\n{text}");
  }

  #[test]
  fn renders_not_found_state() {
    let mut app = app_with_lyrics();
    app.lyrics = None;
    app.lyrics_status = LyricsStatus::NotFound;
    let text = buffer_text(&render(&app));
    assert!(
      text.contains("No lyrics for this track"),
      "missing not-found message:\n{text}"
    );
  }
}
