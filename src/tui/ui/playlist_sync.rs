use crate::core::app::{ActiveBlock, App};
use crate::core::playlist_sync::{Link, UnmatchReason};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::Style,
  text::{Line, Span},
  widgets::{Block, Borders, Paragraph, Wrap},
  Frame,
};

use super::util::{draw_selectable_list, get_color, hint_span};

pub fn draw_playlist_sync(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::PlaylistSync,
    current_route.hovered_block == ActiveBlock::PlaylistSync,
  );
  let theme = app.user_config.theme;

  let [links_area, detail_area, help_area] = layout_chunk.layout(&Layout::vertical([
    Constraint::Min(4),
    Constraint::Length(10),
    Constraint::Length(1),
  ]));

  let links = app.playlist_sync_links();
  let mut title = "Playlist sync".to_string();
  if app.playlist_sync_in_flight() {
    title.push_str(" (syncing...)");
  }

  if links.is_empty() {
    let empty = Paragraph::new(
      "No playlist links. Highlight a playlist in the sidebar and press m to mirror it.",
    )
    .wrap(Wrap { trim: true })
    .style(Style::default().fg(theme.hint.into()))
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, get_color(highlight_state, theme)))
        .border_style(get_color(highlight_state, theme)),
    );
    f.render_widget(empty, links_area);
  } else {
    let rows: Vec<String> = links.iter().map(link_row).collect();
    draw_selectable_list(
      f,
      app,
      links_area,
      &title,
      &rows,
      highlight_state,
      Some(app.view.playlist_sync_selected_link.min(rows.len() - 1)),
    );
  }

  draw_detail(f, app, detail_area);
  draw_help_bar(f, app, help_area);
}

/// One link's row: the master, its source, and the sources it mirrors onto.
fn link_row(link: &Link) -> String {
  let mut mirrors = link
    .mirrors
    .iter()
    .map(|mirror| mirror.endpoint.source.label())
    .collect::<Vec<&str>>()
    .join(", ");
  if mirrors.is_empty() {
    mirrors = "no mirrors".to_string();
  }
  format!(
    "{} [{}] -> {} ({} unmatched)",
    link.master.name,
    link.master.source.label(),
    mirrors,
    link.unmatched_count()
  )
}

/// What the unmatched line says about a track the last run could not place.
fn reason_text(reason: &UnmatchReason) -> String {
  match reason {
    UnmatchReason::NoCandidate => "no match".to_string(),
    UnmatchReason::NotSyncable => "cannot sync".to_string(),
    UnmatchReason::SearchFailed(text) => format!("search failed: {text}"),
  }
}

fn draw_detail(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;
  let selected = app.selected_playlist_sync_link();
  let title = selected
    .zip(app.playlist_sync_last_report())
    .and_then(|(link, report)| {
      report
        .links
        .iter()
        .find(|entry| entry.id == link.id)
        .map(|entry| format!("Last run: {}", entry.line()))
    })
    .unwrap_or_else(|| "Details".to_string());

  let mut lines: Vec<Line> = Vec::new();
  if let Some(link) = selected {
    for mirror in &link.mirrors {
      lines.push(Line::from(Span::styled(
        format!(
          "{}: {} matched, {} unmatched, last run {}",
          mirror.endpoint.source.label(),
          mirror.matches.len(),
          mirror.unmatched.len(),
          mirror.last_run.as_deref().unwrap_or("never")
        ),
        Style::default().fg(theme.text.into()),
      )));
      for entry in &mirror.unmatched {
        lines.push(Line::from(Span::styled(
          format!(
            "  {} - {}: {}",
            entry.title,
            entry.artist,
            reason_text(&entry.reason)
          ),
          Style::default().fg(theme.hint.into()),
        )));
      }
    }
  }

  let detail = Paragraph::new(lines).block(
    Block::default()
      .borders(Borders::ALL)
      .title(Span::styled(
        title,
        Style::default().fg(theme.inactive.into()),
      ))
      .border_style(Style::default().fg(theme.inactive.into())),
  );
  f.render_widget(detail, area);
}

fn draw_help_bar(f: &mut Frame<'_>, app: &App, area: Rect) {
  let theme = app.user_config.theme;
  let label = |text: &'static str| Span::styled(text, Style::default().fg(theme.inactive.into()));

  let line = Line::from(vec![
    hint_span("s", theme),
    label(" Sync now  "),
    hint_span("D", theme),
    label(" Remove link  "),
    hint_span("↑/↓", theme),
    label(" Select  "),
    hint_span("←", theme),
    label(" Back"),
  ]);

  f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::RouteId;
  use crate::core::playlist_sync::{Endpoint, Mirror, Unmatched};
  use crate::core::source::Source;
  use ratatui::{backend::TestBackend, Terminal};
  use std::collections::BTreeMap;

  fn rendered(app: &App, area: Rect) -> String {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw_playlist_sync(f, app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  fn endpoint(source: Source, name: &str) -> Endpoint {
    Endpoint {
      source,
      playlist_uri: format!("{}:playlist:9", source.to_config_str()),
      name: name.to_string(),
    }
  }

  #[test]
  fn the_sync_screen_lists_links_and_their_mirrors() {
    let mut app = App::default_connected();
    let mut matches = BTreeMap::new();
    matches.insert("master-1".to_string(), "mirror-1".to_string());
    app.set_playlist_sync_links(vec![Link {
      id: "aaaaaaaaaaaa".to_string(),
      master: endpoint(Source::Spotify, "Road Trip"),
      mirrors: vec![Mirror {
        endpoint: endpoint(Source::Qobuz, "Road Trip"),
        matches,
        unmatched: vec![Unmatched {
          master_key: "master-2".to_string(),
          title: "Rare Take".to_string(),
          artist: "Nobody".to_string(),
          reason: UnmatchReason::NoCandidate,
        }],
        last_run: Some("2026-09-17T10:00:00Z".to_string()),
      }],
    }]);
    app.push_navigation_stack(RouteId::PlaylistSync, ActiveBlock::PlaylistSync);

    let content = rendered(&app, Rect::new(0, 0, 100, 20));

    assert!(
      content.contains("Road Trip [Spotify] -> Qobuz"),
      "link row missing: {content}"
    );
    assert!(
      content.contains("Qobuz: 1 matched, 1 unmatched, last run 2026-09-17T10:00:00Z"),
      "mirror detail missing: {content}"
    );
    assert!(
      content.contains("Rare Take - Nobody: no match"),
      "unmatched detail missing: {content}"
    );
  }

  #[test]
  fn an_empty_sync_screen_explains_the_m_key() {
    let mut app = App::default_connected();
    app.push_navigation_stack(RouteId::PlaylistSync, ActiveBlock::PlaylistSync);

    let content = rendered(&app, Rect::new(0, 0, 100, 20));

    assert!(
      content.contains("press m to mirror it"),
      "empty hint missing: {content}"
    );
    assert!(content.contains("Sync now"), "help bar missing: {content}");
  }
}
