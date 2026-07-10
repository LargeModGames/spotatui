use crate::core::app::{App, RouteId};
use crate::core::plugin_api::{PluginScreenContent, PluginWidget, PopupLine};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
  Frame,
};

/// Draw the plugin custom screen named by the current route. Retained-mode:
/// this only reads `app.plugin_screens`; plugins update content via effects.
pub fn draw_plugin_screen(f: &mut Frame<'_>, app: &App) {
  let name = match &app.get_current_route().id {
    RouteId::PluginScreen(name) => name.clone(),
    _ => return,
  };

  let area = f.area();
  let content = app.plugin_screens.get(&name);
  let title = content
    .map(|c| c.title.clone())
    .filter(|t| !t.is_empty())
    .unwrap_or_else(|| name.clone());

  let outer = Block::default()
    .borders(Borders::ALL)
    .style(app.user_config.theme.base_style())
    .border_style(Style::default().fg(app.user_config.theme.active))
    .title(Span::styled(
      title,
      Style::default()
        .fg(app.user_config.theme.header)
        .add_modifier(Modifier::BOLD),
    ));
  let inner = outer.inner(area);
  f.render_widget(outer, area);

  let Some(content) = content else {
    // Registered (or mistyped) screen with no content published yet.
    let placeholder = Paragraph::new(Line::from(Span::styled(
      format!("plugin screen '{name}' has no content yet"),
      Style::default().fg(app.user_config.theme.hint),
    )));
    f.render_widget(placeholder, inner);
    return;
  };

  draw_widgets(f, app, content, inner);
}

fn draw_widgets(f: &mut Frame<'_>, app: &App, content: &PluginScreenContent, area: Rect) {
  if content.widgets.is_empty() {
    return;
  }

  // Fixed-height widgets take their rows; the rest split the remainder evenly.
  let constraints: Vec<Constraint> = content
    .widgets
    .iter()
    .map(|w| match w {
      PluginWidget::Paragraph {
        height: Some(h), ..
      } => Constraint::Length(*h),
      PluginWidget::List {
        height: Some(h), ..
      } => Constraint::Length(*h),
      PluginWidget::Gauge { .. } => Constraint::Length(3),
      _ => Constraint::Fill(1),
    })
    .collect();
  let chunks = Layout::vertical(constraints).split(area);

  for (widget, chunk) in content.widgets.iter().zip(chunks.iter()) {
    match widget {
      PluginWidget::Paragraph { lines, .. } => {
        let text: Vec<Line> = lines.iter().map(styled_line).collect();
        let paragraph = Paragraph::new(text).scroll((app.plugin_screen_scroll, 0));
        f.render_widget(paragraph, *chunk);
      }
      PluginWidget::List {
        title,
        items,
        selected,
        ..
      } => {
        let list_items: Vec<ListItem> = items
          .iter()
          .map(|pl| ListItem::new(styled_line(pl)))
          .collect();
        let mut block = Block::default()
          .borders(Borders::ALL)
          .border_style(Style::default().fg(app.user_config.theme.inactive));
        if let Some(title) = title {
          block = block.title(Span::styled(
            title.clone(),
            Style::default().fg(app.user_config.theme.header),
          ));
        }
        let list = List::new(list_items).block(block).highlight_style(
          Style::default()
            .fg(app.user_config.theme.selected)
            .add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default();
        state.select(selected.filter(|s| *s < items.len()));
        f.render_stateful_widget(list, *chunk, &mut state);
      }
      PluginWidget::Gauge { ratio, label } => {
        let gauge = Gauge::default()
          .block(Block::default().borders(Borders::ALL))
          .gauge_style(Style::default().fg(app.user_config.theme.playbar_progress))
          .ratio(ratio.clamp(0.0, 1.0))
          .label(Span::styled(
            label.clone().unwrap_or_default(),
            Style::default().fg(app.user_config.theme.playbar_progress_text),
          ));
        f.render_widget(gauge, *chunk);
      }
    }
  }
}

fn styled_line(pl: &PopupLine) -> Line<'static> {
  let mut style = Style::default();
  if let Some(fg) = pl.fg {
    style = style.fg(fg);
  }
  if pl.bold {
    style = style.add_modifier(Modifier::BOLD);
  }
  if pl.italic {
    style = style.add_modifier(Modifier::ITALIC);
  }
  Line::from(Span::styled(pl.text.clone(), style))
}
