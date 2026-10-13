use crate::core::app::{ActiveBlock, App};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
  Frame,
};

use super::util::get_color;
use crate::tui::theme::EmphasisExt;

pub fn draw_discover(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Discover,
    current_route.hovered_block == ActiveBlock::Discover,
  );
  let theme = app.user_config.theme;

  let [list_area, info_area] = layout_chunk.layout(&Layout::vertical([
    Constraint::Min(8),
    Constraint::Length(6),
  ]));

  // Shelf titles are rows too, so the selected playlist sits below them.
  let mut items: Vec<ListItem> = Vec::new();
  let mut selected_row = None;
  let mut playlist_index = 0;
  for shelf in app.discover_shelves() {
    items.push(ListItem::new(Line::from(Span::styled(
      shelf.title.clone(),
      Style::default()
        .fg(theme.hint.into())
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ))));
    for playlist in &shelf.playlists {
      if playlist_index == app.view.discover_selected_index {
        selected_row = Some(items.len());
      }
      items.push(ListItem::new(Line::from(Span::styled(
        format!("  {}", playlist.name),
        Style::default().fg(theme.text.into()),
      ))));
      playlist_index += 1;
    }
  }
  if items.is_empty() {
    let message = if app.discover_loading() {
      "Loading Made For You…"
    } else {
      "Nothing here yet."
    };
    items.push(ListItem::new(Span::styled(
      message,
      Style::default().fg(theme.inactive.into()),
    )));
  }

  let title = if app.discover_loading() {
    "Discover [Loading...]"
  } else {
    "Discover"
  };
  let mut state = ListState::default();
  state.select(selected_row);
  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, get_color(highlight_state, theme)))
        .border_style(get_color(highlight_state, theme)),
    )
    .highlight_style(
      get_color(highlight_state, theme)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(Line::from("▶ ").style(get_color(highlight_state, theme)));
  f.render_stateful_widget(list, list_area, &mut state);

  let key = |text: &'static str| Span::styled(text, Style::default().fg(theme.hint.into()));
  let label = |text: &'static str| Span::styled(text, Style::default().fg(theme.text.into()));
  let mut info_lines = vec![Line::from(vec![
    key("Enter "),
    label("Open"),
    key("   e "),
    label("Play"),
    key("   w "),
    label("Save to library"),
  ])];
  if let Some(description) = app
    .discover_playlist(app.view.discover_selected_index)
    .and_then(|playlist| playlist.description.as_deref())
  {
    info_lines.push(Line::from(""));
    info_lines.push(Line::from(Span::styled(
      description.to_string(),
      Style::default().fg(theme.inactive.into()),
    )));
  }
  let info = Paragraph::new(info_lines)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
          "Made For You",
          Style::default().fg(theme.inactive.into()),
        ))
        .border_style(Style::default().fg(theme.inactive.into())),
    )
    .wrap(Wrap { trim: true });
  f.render_widget(info, info_area);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::RouteId;
  use crate::core::test_helpers::discover_shelves;
  use ratatui::{backend::TestBackend, Terminal};

  fn rendered(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(50, 16)).unwrap();
    terminal.draw(|f| draw_discover(f, app, f.area())).unwrap();
    terminal
      .backend()
      .buffer()
      .content()
      .chunks(50)
      .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
      .collect::<Vec<_>>()
      .join("\n")
  }

  #[test]
  fn shelves_draw_as_headers_with_the_selected_description_below() {
    let mut app = App::default();
    app.push_navigation_stack(RouteId::Discover, ActiveBlock::Discover);
    app.end_discover_fetch(Some(discover_shelves()));
    app.view.discover_selected_index = 2;

    let screen = rendered(&app);

    assert!(screen.contains("Daily"));
    assert!(screen.contains("New"));
    assert!(screen.contains("▶   Release Radar"));
    assert!(screen.contains("Release Radar for you"));
  }
}
