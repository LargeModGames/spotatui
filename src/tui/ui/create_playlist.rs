use crate::core::app::{App, CreatePlaylistFocus, CreatePlaylistStage};
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

pub fn draw_create_playlist_form(f: &mut Frame<'_>, app: &App) {
  let area = centered_rect(f.area(), 80, 80);
  f.render_widget(Clear, area);

  match app.create_playlist_stage {
    CreatePlaylistStage::Name => draw_name_stage(f, app, area),
    CreatePlaylistStage::AddTracks => draw_add_tracks_stage(f, app, area),
  }
}

fn draw_name_stage(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = &app.user_config.theme;

  let block = Block::default()
    .title(Span::styled(
      "Create Playlist (Esc to cancel)",
      Style::default()
        .fg(theme.header)
        .add_modifier(Modifier::BOLD),
    ))
    .borders(Borders::ALL)
    .style(theme.base_style())
    .border_style(Style::default().fg(theme.active));
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

  let label = Paragraph::new("Playlist name:")
    .style(theme.base_style());
  f.render_widget(label, inner[0]);

  let name_text: String = app.create_playlist_name.iter().collect();
  let input = Paragraph::new(name_text)
    .style(theme.base_style())
    .block(
      Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.active)),
    );
  f.render_widget(input, inner[1]);

  let hint = Paragraph::new("Press Enter to continue, Esc to cancel")
    .style(Style::default().fg(theme.inactive));
  f.render_widget(hint, inner[2]);
}

fn draw_add_tracks_stage(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = &app.user_config.theme;
  let name: String = app.create_playlist_name.iter().collect();
  let title = format!("Add Tracks to \"{}\" (Enter=create, Tab=switch panel, Esc=cancel)", name);

  let block = Block::default()
    .title(Span::styled(
      title,
      Style::default()
        .fg(theme.header)
        .add_modifier(Modifier::BOLD),
    ))
    .borders(Borders::ALL)
    .style(theme.base_style())
    .border_style(Style::default().fg(theme.active));
  f.render_widget(block, area);

  let inner = Layout::default()
    .direction(Direction::Vertical)
    .margin(1)
    .constraints([
      Constraint::Length(3),
      Constraint::Min(5),
    ])
    .split(area);

  // Search input
  let search_text: String = app.create_playlist_search_input.iter().collect();
  let search_border_style = if app.create_playlist_focus == CreatePlaylistFocus::SearchInput {
    Style::default().fg(theme.active)
  } else {
    Style::default().fg(theme.inactive)
  };
  let search_input = Paragraph::new(search_text)
    .style(theme.base_style())
    .block(
      Block::default()
        .title(Span::styled(
          "Search (Enter to search)",
          Style::default().fg(theme.header),
        ))
        .borders(Borders::ALL)
        .border_style(search_border_style),
    );
  f.render_widget(search_input, inner[0]);

  // Two-panel area: results + added tracks
  let panels = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
    .split(inner[1]);

  // Left: search results
  let results_border_style = if app.create_playlist_focus == CreatePlaylistFocus::SearchResults {
    Style::default().fg(theme.active)
  } else {
    Style::default().fg(theme.inactive)
  };
  let result_items: Vec<ListItem> = app
    .create_playlist_search_results
    .iter()
    .map(|t| {
      let artist = t
        .artists
        .first()
        .map(|a| a.name.as_str())
        .unwrap_or("Unknown");
      ListItem::new(format!("{} — {}", t.name, artist))
        .style(theme.base_style())
    })
    .collect();

  let mut results_state = ListState::default();
  if app.create_playlist_focus == CreatePlaylistFocus::SearchResults
    && !app.create_playlist_search_results.is_empty()
  {
    results_state.select(Some(app.create_playlist_selected_result));
  }

  let results_list = List::new(result_items)
    .block(
      Block::default()
        .title(Span::styled(
          "Results (Enter to add)",
          Style::default().fg(theme.header),
        ))
        .borders(Borders::ALL)
        .border_style(results_border_style),
    )
    .highlight_style(
      Style::default()
        .fg(theme.selected)
        .add_modifier(Modifier::BOLD),
    )
    .style(theme.base_style());
  f.render_stateful_widget(results_list, panels[0], &mut results_state);

  // Right: added tracks
  let added_border_style = if app.create_playlist_focus == CreatePlaylistFocus::AddedTracks {
    Style::default().fg(theme.active)
  } else {
    Style::default().fg(theme.inactive)
  };

  let added_items: Vec<ListItem> = app
    .create_playlist_tracks
    .iter()
    .map(|t| {
      let artist = t
        .artists
        .first()
        .map(|a| a.name.as_str())
        .unwrap_or("Unknown");
      ListItem::new(format!("{} — {}", t.name, artist))
        .style(theme.base_style())
    })
    .collect();

  let mut added_state = ListState::default();
  if app.create_playlist_focus == CreatePlaylistFocus::AddedTracks
    && !app.create_playlist_tracks.is_empty()
  {
    added_state.select(Some(app.create_playlist_selected_result));
  }

  let added_tracks_title = format!(
    "Added ({}) — d=remove, Enter=create",
    app.create_playlist_tracks.len()
  );
  let added_list = List::new(added_items)
    .block(
      Block::default()
        .title(Span::styled(
          added_tracks_title,
          Style::default().fg(theme.header),
        ))
        .borders(Borders::ALL)
        .border_style(added_border_style),
    )
    .highlight_style(
      Style::default()
        .fg(theme.selected)
        .add_modifier(Modifier::BOLD),
    )
    .style(theme.base_style());
  f.render_stateful_widget(added_list, panels[1], &mut added_state);
}
