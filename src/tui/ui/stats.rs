use crate::core::app::{ActiveBlock, App};
use crate::infra::history::{format_duration, period_label, RankedEntry, RecapPeriod};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
  Frame,
};

use super::util::{get_color, hint_span};

pub fn draw_stats(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Stats,
    current_route.hovered_block == ActiveBlock::Stats,
  );
  let theme = app.user_config.theme;

  let [tabs_area, summary_area, lists_area, days_area, help_area] =
    layout_chunk.layout(&Layout::vertical([
      Constraint::Length(3),
      Constraint::Length(3),
      Constraint::Min(6),
      Constraint::Length(7),
      Constraint::Length(1),
    ]));

  // Period tabs
  let titles: Vec<Line> = RecapPeriod::ALL_PERIODS
    .iter()
    .map(|period| Line::from(period_label(*period)))
    .collect();
  let selected = RecapPeriod::ALL_PERIODS
    .iter()
    .position(|period| *period == app.stats_period)
    .unwrap_or(0);
  let tabs = Tabs::new(titles)
    .select(selected)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled("Stats", get_color(highlight_state, theme)))
        .border_style(get_color(highlight_state, theme)),
    )
    .style(Style::default().fg(theme.text))
    .highlight_style(
      Style::default()
        .fg(theme.active)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    );
  f.render_widget(tabs, tabs_area);

  // Summary strip: streak + totals
  let mut summary_spans: Vec<Span> = Vec::new();
  if let Some(streaks) = &app.listening_streaks {
    summary_spans.push(Span::styled(
      format!("{}-day streak", streaks.current_days),
      Style::default()
        .fg(theme.active)
        .add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)),
    ));
    summary_spans.push(Span::styled(
      format!(
        " (best {}) · {} today",
        streaks.longest_days,
        format_duration(streaks.today_ms)
      ),
      Style::default().fg(theme.text),
    ));
  }
  if app.stats_loading {
    summary_spans.push(Span::styled(
      "  [Loading...]",
      Style::default().fg(theme.hint),
    ));
  } else if let Some(stats) = &app.stats_data {
    summary_spans.push(Span::styled(
      format!(
        "  {} plays · {} listened",
        stats.total_plays,
        format_duration(stats.total_time_ms)
      ),
      Style::default().fg(theme.text),
    ));
  }
  if summary_spans.is_empty() {
    summary_spans.push(Span::styled(
      "No listening history recorded yet. Play something!",
      Style::default().fg(theme.hint),
    ));
  }
  let summary = Paragraph::new(Line::from(summary_spans)).block(
    Block::default()
      .borders(Borders::ALL)
      .title(Span::styled(
        period_label(app.stats_period),
        Style::default().fg(theme.inactive),
      ))
      .border_style(Style::default().fg(theme.inactive)),
  );
  f.render_widget(summary, summary_area);

  // Three ranked lists side by side
  let [tracks_area, artists_area, albums_area] = lists_area.layout(&Layout::horizontal([
    Constraint::Percentage(40),
    Constraint::Percentage(30),
    Constraint::Percentage(30),
  ]));

  let (top_tracks, top_artists, top_albums): (&[RankedEntry], &[RankedEntry], &[RankedEntry]) =
    match &app.stats_data {
      Some(stats) => (&stats.top_tracks, &stats.top_artists, &stats.top_albums),
      None => (&[], &[], &[]),
    };

  draw_ranked_panel(
    f,
    app,
    tracks_area,
    "Top Tracks",
    top_tracks,
    get_color(highlight_state, theme),
    Some(app.stats_selected_track),
  );
  draw_ranked_panel(
    f,
    app,
    artists_area,
    "Top Artists",
    top_artists,
    Style::default().fg(theme.inactive),
    None,
  );
  draw_ranked_panel(
    f,
    app,
    albums_area,
    "Top Albums",
    top_albums,
    Style::default().fg(theme.inactive),
    None,
  );

  // Last-10-days bars + hint line
  let days = app
    .stats_data
    .as_ref()
    .map(|stats| stats.days.as_slice())
    .unwrap_or(&[]);
  let max_value = days
    .iter()
    .map(|entry| entry.value)
    .max()
    .unwrap_or(1)
    .max(1);
  let mut day_lines: Vec<Line> = days
    .iter()
    .map(|entry| {
      let width = ((entry.value as f64 / max_value as f64) * 24.0).round() as usize;
      Line::from(vec![
        Span::styled(
          format!("{}  ", entry.display),
          Style::default().fg(theme.text),
        ),
        Span::styled("█".repeat(width.max(1)), Style::default().fg(theme.active)),
        Span::styled(
          format!(" {}", entry.detail),
          Style::default().fg(theme.hint),
        ),
      ])
    })
    .collect();
  if day_lines.is_empty() {
    day_lines.push(Line::from(Span::styled(
      "No plays in this period yet.",
      Style::default().fg(theme.hint),
    )));
  }

  let days_panel = Paragraph::new(day_lines).block(
    Block::default()
      .borders(Borders::ALL)
      .title(Span::styled(
        "Last 10 Days",
        Style::default().fg(theme.inactive),
      ))
      .border_style(Style::default().fg(theme.inactive)),
  );
  f.render_widget(days_panel, days_area);

  draw_help_bar(f, app, help_area);
}

fn draw_help_bar(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;
  let label = |text: &'static str| Span::styled(text, Style::default().fg(theme.inactive));

  let line = Line::from(vec![
    hint_span("↑/↓", theme),
    label(" Select track  "),
    hint_span("Enter", theme),
    label(" Play  "),
    hint_span("[ / ]", theme),
    label(" Change period  "),
    Span::styled(
      app.user_config.keys.generate_recap.to_string(),
      Style::default().fg(theme.hint).add_modifier(Modifier::BOLD),
    ),
    label(" Open share card  "),
    hint_span("←/Esc", theme),
    label(" Back"),
  ]);

  f.render_widget(Paragraph::new(line), area);
}

fn ranked_list_items<'a>(entries: &'a [RankedEntry], app: &App) -> Vec<ListItem<'a>> {
  let theme = app.user_config.theme;
  entries
    .iter()
    .enumerate()
    .map(|(i, entry)| {
      ListItem::new(Line::from(vec![
        Span::styled(format!("{:>2}. ", i + 1), Style::default().fg(theme.hint)),
        Span::styled(entry.display.as_str(), Style::default().fg(theme.text)),
        Span::styled(
          format!("  {}", entry.detail),
          Style::default().fg(theme.hint),
        ),
      ]))
    })
    .collect()
}

fn draw_ranked_panel(
  f: &mut Frame<'_>,
  app: &App,
  area: Rect,
  title: &str,
  entries: &[RankedEntry],
  style: Style,
  selected: Option<usize>,
) {
  let list = List::new(ranked_list_items(entries, app)).block(
    Block::default()
      .borders(Borders::ALL)
      .title(Span::styled(title, style))
      .border_style(style),
  );
  match selected {
    Some(index) => {
      let list = list
        .highlight_style(style.add_modifier(app.user_config.behavior.emphasis(Modifier::BOLD)))
        .highlight_symbol(Line::from("▶ ").style(style));
      let mut state = ListState::default();
      if !entries.is_empty() {
        state.select(Some(index.min(entries.len() - 1)));
      }
      f.render_stateful_widget(list, area, &mut state);
    }
    None => f.render_widget(list, area),
  }
}
