use crate::core::app::{App, CreatePlaylistFocus, CreatePlaylistStage};
use crate::tui::theme::{EmphasisExt, ThemeExt};
use ratatui::{
  layout::{Constraint, Direction, Layout, Rect},
  style::{Modifier, Style},
  text::Span,
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
  Frame,
};

fn centered_rect(bounds: Rect, width_pct: u16, height_pct: u16) -> Rect {
  let width = (bounds.width * width_pct / 100).max(1);
  let height = (bounds.height * height_pct / 100).max(1);
  let x = bounds.x + bounds.width.saturating_sub(width) / 2;
  let y = bounds.y + bounds.height.saturating_sub(height) / 3;
  Rect::new(x, y, width, height)
}

// Columns to scroll a bordered single-line input so the cursor stays inside it.
fn input_scroll(cursor: u16, box_width: u16) -> u16 {
  let inner_width = box_width.saturating_sub(2);
  if inner_width > 0 && cursor >= inner_width {
    cursor - inner_width + 1
  } else {
    0
  }
}

pub fn draw_create_playlist_form(f: &mut Frame<'_>, app: &App) {
  let area = centered_rect(f.area(), 80, 80);
  f.render_widget(Clear, area);

  match app.view.create_playlist_stage {
    CreatePlaylistStage::Name => draw_name_stage(f, app, area),
    CreatePlaylistStage::AddTracks => draw_add_tracks_stage(f, app, area),
  }
}

fn draw_name_stage(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = &app.user_config.theme;

  let block = Block::default()
    .title(Span::styled(
      app.strings().popups.create_playlist_title,
      Style::default()
        .fg(theme.header.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))
    .borders(Borders::ALL)
    .style(theme.base_style())
    .border_style(Style::default().fg(theme.active.into()));
  f.render_widget(block, area);

  let inner = Layout::default()
    .direction(Direction::Vertical)
    .margin(2)
    .constraints([
      Constraint::Length(1),
      Constraint::Length(3),
      Constraint::Length(1),
    ])
    .split(area);

  let label = Paragraph::new(app.strings().popups.playlist_name_label).style(theme.base_style());
  f.render_widget(label, inner[0]);

  let name_text: String = app.view.create_playlist_name.iter().collect();
  let offset = input_scroll(app.view.create_playlist_name_cursor, inner[1].width);
  let input = Paragraph::new(name_text)
    .style(theme.base_style())
    .scroll((0, offset))
    .block(
      Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.active.into())),
    );
  f.render_widget(input, inner[1]);
  f.set_cursor_position((
    inner[1].x + 1 + app.view.create_playlist_name_cursor - offset,
    inner[1].y + 1,
  ));

  let hint = Paragraph::new(app.strings().popups.create_continue_hint)
    .style(Style::default().fg(theme.inactive.into()));
  f.render_widget(hint, inner[2]);
}

fn draw_add_tracks_stage(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = &app.user_config.theme;
  let name: String = app.view.create_playlist_name.iter().collect();
  let title = app.strings().popups.add_tracks_title(&name);

  let block = Block::default()
    .title(Span::styled(
      title,
      Style::default()
        .fg(theme.header.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))
    .borders(Borders::ALL)
    .style(theme.base_style())
    .border_style(Style::default().fg(theme.active.into()));
  f.render_widget(block, area);

  let inner = Layout::default()
    .direction(Direction::Vertical)
    .margin(1)
    .constraints([Constraint::Length(3), Constraint::Min(5)])
    .split(area);

  // Search input
  let search_text: String = app.view.create_playlist_search_input.iter().collect();
  let search_border_style = if app.view.create_playlist_focus == CreatePlaylistFocus::SearchInput {
    Style::default().fg(theme.active.into())
  } else {
    Style::default().fg(theme.inactive.into())
  };
  let offset = input_scroll(app.view.create_playlist_search_cursor, inner[0].width);
  let search_input = Paragraph::new(search_text)
    .style(theme.base_style())
    .scroll((0, offset))
    .block(
      Block::default()
        .title(Span::styled(
          app.strings().popups.create_search_title,
          Style::default().fg(theme.header.into()),
        ))
        .borders(Borders::ALL)
        .border_style(search_border_style),
    );
  f.render_widget(search_input, inner[0]);
  if app.view.create_playlist_focus == CreatePlaylistFocus::SearchInput {
    f.set_cursor_position((
      inner[0].x + 1 + app.view.create_playlist_search_cursor - offset,
      inner[0].y + 1,
    ));
  }

  // Two-panel area: results + added tracks
  let panels = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
    .split(inner[1]);

  // Left: search results
  let results_border_style = if app.view.create_playlist_focus == CreatePlaylistFocus::SearchResults
  {
    Style::default().fg(theme.active.into())
  } else {
    Style::default().fg(theme.inactive.into())
  };
  let result_items: Vec<ListItem> = app
    .create_playlist_search_results
    .iter()
    .map(|t| {
      let artist = t
        .artists
        .first()
        .map(|a| a.as_str())
        .unwrap_or(app.strings().popups.unknown_artist);
      ListItem::new(format!("{} — {}", t.name, artist)).style(theme.base_style())
    })
    .collect();

  let mut results_state = ListState::default();
  if app.view.create_playlist_focus == CreatePlaylistFocus::SearchResults
    && !app.create_playlist_search_results.is_empty()
  {
    results_state.select(Some(app.view.create_playlist_selected_result));
  }

  let results_list = List::new(result_items)
    .block(
      Block::default()
        .title(Span::styled(
          app.strings().popups.create_results_title,
          Style::default().fg(theme.header.into()),
        ))
        .borders(Borders::ALL)
        .border_style(results_border_style),
    )
    .highlight_style(
      Style::default()
        .fg(theme.selected.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .style(theme.base_style());
  f.render_stateful_widget(results_list, panels[0], &mut results_state);

  // Right: added tracks
  let added_border_style = if app.view.create_playlist_focus == CreatePlaylistFocus::AddedTracks {
    Style::default().fg(theme.active.into())
  } else {
    Style::default().fg(theme.inactive.into())
  };

  let added_items: Vec<ListItem> = app
    .create_playlist_tracks
    .iter()
    .map(|t| {
      let artist = t
        .artists
        .first()
        .map(|a| a.as_str())
        .unwrap_or(app.strings().popups.unknown_artist);
      ListItem::new(format!("{} — {}", t.name, artist)).style(theme.base_style())
    })
    .collect();

  let mut added_state = ListState::default();
  if app.view.create_playlist_focus == CreatePlaylistFocus::AddedTracks
    && !app.create_playlist_tracks.is_empty()
  {
    added_state.select(Some(app.view.create_playlist_selected_result));
  }

  let added_tracks_title = app
    .strings()
    .popups
    .added_tracks_title(app.create_playlist_tracks.len());
  let added_list = List::new(added_items)
    .block(
      Block::default()
        .title(Span::styled(
          added_tracks_title,
          Style::default().fg(theme.header.into()),
        ))
        .borders(Borders::ALL)
        .border_style(added_border_style),
    )
    .highlight_style(
      Style::default()
        .fg(theme.selected.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .style(theme.base_style());
  f.render_stateful_widget(added_list, panels[1], &mut added_state);
}

#[cfg(test)]
mod tests {
  use super::*;
  use ratatui::{backend::TestBackend, Terminal};

  #[test]
  fn traditional_chinese_create_form_keeps_the_playlist_name_literal() {
    let mut app = App::default();
    app.user_config.behavior.language = crate::core::i18n::Language::ZhHant;
    app.refresh_language();
    app.view.create_playlist_name = "{name}".chars().collect();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
      .draw(|f| draw_create_playlist_form(f, &app))
      .unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..30)
      .flat_map(|y| (0..100).map(move |x| (x, y)))
      .filter_map(|pos| buffer.cell(pos))
      .map(|cell| cell.symbol())
      .collect::<String>()
      .replace(' ', "");
    assert!(content.contains("建立播放清單"), "{content}");
    assert!(content.contains("播放清單名稱"), "{content}");
    assert!(content.contains("{name}"), "{content}");
    assert!(!content.contains("Playlistname"), "{content}");
  }

  #[test]
  fn chinese_add_tracks_title_keeps_the_playlist_name_literal() {
    for (language, expected) in [
      (crate::core::i18n::Language::ZhHans, "向“{name}”添加曲目"),
      (crate::core::i18n::Language::ZhHant, "向「{name}」加入曲目"),
    ] {
      let mut app = App::default();
      app.user_config.behavior.language = language;
      app.refresh_language();
      let title = app.strings().popups.add_tracks_title("{name}");
      assert!(title.starts_with(expected), "{title}");
    }
  }

  #[test]
  fn input_scroll_is_zero_while_the_cursor_fits() {
    assert_eq!(input_scroll(5, 20), 0);
    // A zero-width box must not underflow or scroll
    assert_eq!(input_scroll(5, 0), 0);
  }

  #[test]
  fn input_scroll_keeps_the_cursor_inside_the_box() {
    // box 20 wide -> 18 usable columns (2 are the borders)
    assert_eq!(input_scroll(18, 20), 1);
    assert_eq!(input_scroll(40, 20), 23);
  }

  #[test]
  fn long_playlist_name_keeps_the_cursor_inside_the_input_box() {
    let mut app = App::default();
    app.view.create_playlist_stage = CreatePlaylistStage::Name;
    // 200 chars is far wider than the box; cursor is a display-width count
    app.view.create_playlist_name = "a".repeat(200).chars().collect();
    app.view.create_playlist_name_cursor = 200;

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
      .draw(|f| draw_create_playlist_form(f, &app))
      .unwrap();
    let cursor = terminal.get_cursor_position().unwrap();

    // 100x30 screen: popup is 80 wide at x=10; margin 2 puts the input box at
    // x=12, width 76, so its right border is at x=87.
    assert!(cursor.x < 87, "cursor escaped the box: x={}", cursor.x);
    assert!(
      cursor.x > 12,
      "cursor is on the left border: x={}",
      cursor.x
    );
  }

  #[test]
  fn long_search_query_keeps_the_cursor_inside_the_search_box() {
    let mut app = App::default();
    app.view.create_playlist_stage = CreatePlaylistStage::AddTracks;
    app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    app.view.create_playlist_search_input = "a".repeat(200).chars().collect();
    app.view.create_playlist_search_cursor = 200;

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
      .draw(|f| draw_create_playlist_form(f, &app))
      .unwrap();
    let cursor = terminal.get_cursor_position().unwrap();

    // Margin is 1 here: box at x=11, width 78, right border at x=88.
    assert!(cursor.x < 88, "cursor escaped the box: x={}", cursor.x);
    assert!(
      cursor.x > 11,
      "cursor is on the left border: x={}",
      cursor.x
    );
  }
}
