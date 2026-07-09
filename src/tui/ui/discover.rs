use crate::core::app::{ActiveBlock, App};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
  Frame,
};

use super::util::get_color;

pub fn draw_discover(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Discover,
    current_route.hovered_block == ActiveBlock::Discover,
  );

  // Split into two sections: playlist list and info panel
  let [list_area, info_area] = layout_chunk.layout(&Layout::vertical([
    Constraint::Min(8),
    Constraint::Length(8),
  ]));

  // Build discover options with status indicators
  let artists_mix_status = if app.discover_loading && app.discover_selected_index == 0 {
    " [Loading...]".to_string()
  } else if !app.discover_artists_mix.is_empty() {
    format!(" [{} tracks]", app.discover_artists_mix.len())
  } else {
    String::new()
  };

  let top_tracks_status = if app.discover_loading && app.discover_selected_index == 1 {
    " [Loading...]".to_string()
  } else if !app.discover_top_tracks.is_empty() {
    format!(" [{} tracks]", app.discover_top_tracks.len())
  } else {
    String::new()
  };

  let mut state = ListState::default();
  state.select(Some(app.discover_selected_index));

  let list_items: Vec<ListItem> = vec![
    // Top Artists Mix
    {
      let is_selected = app.discover_selected_index == 0;
      let prefix = if is_selected { "▸ " } else { "  " };
      let text_style = if is_selected {
        Style::default().fg(app.user_config.theme.selected)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };
      ListItem::new(Line::from(vec![
        Span::styled(prefix, Style::default().fg(app.user_config.theme.selected)),
        Span::styled(
          format!("{} Top Artists Mix", app.user_config.padded_liked_icon()),
          text_style,
        ),
        Span::styled(
          artists_mix_status,
          Style::default().fg(app.user_config.theme.hint),
        ),
      ]))
    },
    // Top Tracks with time range
    {
      let is_selected = app.discover_selected_index == 1;
      let prefix = if is_selected { "▸ " } else { "  " };
      let text_style = if is_selected {
        Style::default().fg(app.user_config.theme.selected)
      } else {
        Style::default().fg(app.user_config.theme.text)
      };
      let time_range_label = format!(" ({})", app.discover_time_range.label());
      ListItem::new(Line::from(vec![
        Span::styled(prefix, Style::default().fg(app.user_config.theme.selected)),
        Span::styled(
          format!("{} Top Tracks", app.user_config.padded_liked_icon()),
          text_style,
        ),
        Span::styled(
          time_range_label,
          if is_selected {
            Style::default().fg(app.user_config.theme.active)
          } else {
            Style::default().fg(app.user_config.theme.inactive)
          },
        ),
        Span::styled(
          top_tracks_status,
          Style::default().fg(app.user_config.theme.hint),
        ),
      ]))
    },
  ];

  let list = List::new(list_items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
          "Discover",
          get_color(highlight_state, app.user_config.theme),
        ))
        .border_style(get_color(highlight_state, app.user_config.theme)),
    )
    .highlight_style(
      get_color(highlight_state, app.user_config.theme)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    )
    .highlight_symbol(Line::from("▶ ").style(get_color(highlight_state, app.user_config.theme)));

  f.render_stateful_widget(list, list_area, &mut state);

  // Info panel at bottom - context-sensitive help
  let info_lines = vec![
    Line::from(vec![
      Span::styled("↑/↓ ", Style::default().fg(app.user_config.theme.hint)),
      Span::styled("Navigate", Style::default().fg(app.user_config.theme.text)),
      Span::styled("   Enter ", Style::default().fg(app.user_config.theme.hint)),
      Span::styled("Select", Style::default().fg(app.user_config.theme.text)),
      if app.discover_selected_index == 1 {
        Span::styled("   [/] ", Style::default().fg(app.user_config.theme.hint))
      } else {
        Span::styled("", Style::default())
      },
      if app.discover_selected_index == 1 {
        Span::styled(
          "Time range",
          Style::default().fg(app.user_config.theme.text),
        )
      } else {
        Span::styled("", Style::default())
      },
    ]),
    Line::from(""),
    Line::from(vec![
      Span::styled(
        "Top Artists Mix: ",
        Style::default().fg(app.user_config.theme.text),
      ),
      Span::styled(
        "Shuffled tracks from your top 5 artists",
        Style::default().fg(app.user_config.theme.hint),
      ),
    ]),
    Line::from(vec![
      Span::styled(
        "Top Tracks: ",
        Style::default().fg(app.user_config.theme.text),
      ),
      Span::styled(
        "Your most-listened songs",
        Style::default().fg(app.user_config.theme.hint),
      ),
    ]),
    Line::from(""),
    Line::from(vec![
      Span::styled(
        format!("Time range: {} ", app.discover_time_range.label()),
        Style::default().fg(app.user_config.theme.text),
      ),
      Span::styled(
        "(4 weeks / 6 months / All time)",
        Style::default().fg(app.user_config.theme.inactive),
      ),
    ]),
  ];

  let info = Paragraph::new(info_lines)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
          "Help",
          Style::default().fg(app.user_config.theme.inactive),
        ))
        .border_style(Style::default().fg(app.user_config.theme.inactive)),
    )
    .wrap(Wrap { trim: true });

  f.render_widget(info, info_area);
}
