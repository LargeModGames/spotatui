use crate::core::app::{App, RouteId};
use crate::core::plugin_api::{
  PluginCoverArtFit, PluginLength, PluginScreenContent, PluginWidget, PluginWidgetKind, PopupLine,
};
use crate::tui::theme::ThemeExt;
use ratatui::{
  layout::{Alignment, Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
  Frame,
};

/// Rows a `gauge` takes when it carries no explicit height. Sized for the
/// bordered block plus one row of bar.
const GAUGE_ROWS: u16 = 3;

/// The axis along which a container splits its area.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
  Vertical,
  Horizontal,
}

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
    .border_style(Style::default().fg(app.user_config.theme.active.into()))
    .title(Span::styled(
      title,
      Style::default()
        .fg(app.user_config.theme.header.into())
        .add_modifier(Modifier::BOLD),
    ));
  let inner = outer.inner(area);
  f.render_widget(outer, area);

  let Some(content) = content else {
    // Registered (or mistyped) screen with no content published yet.
    let placeholder = Paragraph::new(Line::from(Span::styled(
      format!("plugin screen '{name}' has no content yet"),
      Style::default().fg(app.user_config.theme.hint.into()),
    )));
    f.render_widget(placeholder, inner);
    return;
  };

  draw_widgets(f, app, content, inner);
}

/// The constraint a widget contributes to its parent's split.
///
/// Only the size hint matching the parent's axis is consulted; the other is
/// ignored. Without one, the widget shares the remaining space evenly - except
/// a gauge stacked vertically, which keeps its fixed row count.
fn constraint_for(widget: &PluginWidget, axis: Axis) -> Constraint {
  let length = match axis {
    Axis::Vertical => widget.height,
    Axis::Horizontal => widget.width,
  };
  match length {
    Some(PluginLength::Cells(cells)) => Constraint::Length(cells),
    Some(PluginLength::Percent(percent)) => Constraint::Percentage(percent),
    None if axis == Axis::Vertical && matches!(widget.kind, PluginWidgetKind::Gauge { .. }) => {
      Constraint::Length(GAUGE_ROWS)
    }
    None => Constraint::Fill(1),
  }
}

/// Flatten a widget tree into drawable leaves paired with their rects.
///
/// `Row`/`Column` are pure layout: they contribute a constraint to their own
/// parent, then split their share among their children along their own axis.
/// Kept free of `Frame` so the nesting and percentage math is unit-testable.
fn layout_tree<'a>(
  widgets: &'a [PluginWidget],
  area: Rect,
  axis: Axis,
  out: &mut Vec<(&'a PluginWidget, Rect)>,
) {
  if widgets.is_empty() {
    return;
  }

  let constraints: Vec<Constraint> = widgets.iter().map(|w| constraint_for(w, axis)).collect();
  let chunks = match axis {
    Axis::Vertical => Layout::vertical(constraints).split(area),
    Axis::Horizontal => Layout::horizontal(constraints).split(area),
  };

  for (widget, chunk) in widgets.iter().zip(chunks.iter()) {
    match &widget.kind {
      PluginWidgetKind::Row { children } => layout_tree(children, *chunk, Axis::Horizontal, out),
      PluginWidgetKind::Column { children } => layout_tree(children, *chunk, Axis::Vertical, out),
      _ => out.push((widget, *chunk)),
    }
  }
}

fn draw_widgets(f: &mut Frame<'_>, app: &App, content: &PluginScreenContent, area: Rect) {
  let mut leaves = Vec::new();
  layout_tree(&content.widgets, area, Axis::Vertical, &mut leaves);

  for (widget, chunk) in leaves {
    match &widget.kind {
      PluginWidgetKind::Paragraph { lines, scroll } => {
        let text: Vec<Line> = lines.iter().map(styled_line).collect();
        let offset = if *scroll {
          app.view.plugin_screen_scroll
        } else {
          0
        };
        let paragraph = Paragraph::new(text).scroll((offset, 0));
        f.render_widget(paragraph, chunk);
      }
      PluginWidgetKind::List {
        title,
        items,
        selected,
      } => {
        let list_items: Vec<ListItem> = items
          .iter()
          .map(|pl| ListItem::new(styled_line(pl)))
          .collect();
        let mut block = Block::default()
          .borders(Borders::ALL)
          .border_style(Style::default().fg(app.user_config.theme.inactive.into()));
        if let Some(title) = title {
          block = block.title(Span::styled(
            title.clone(),
            Style::default().fg(app.user_config.theme.header.into()),
          ));
        }
        let list = List::new(list_items).block(block).highlight_style(
          Style::default()
            .fg(app.user_config.theme.selected.into())
            .add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default();
        state.select(selected.filter(|s| *s < items.len()));
        f.render_stateful_widget(list, chunk, &mut state);
      }
      PluginWidgetKind::Gauge { ratio, label } => {
        let gauge = Gauge::default()
          .block(Block::default().borders(Borders::ALL))
          .gauge_style(Style::default().fg(app.user_config.theme.playbar_progress.into()))
          .ratio(ratio.clamp(0.0, 1.0))
          .label(Span::styled(
            label.clone().unwrap_or_default(),
            Style::default().fg(app.user_config.theme.playbar_progress_text.into()),
          ));
        f.render_widget(gauge, chunk);
      }
      PluginWidgetKind::CoverArt { fit } => draw_cover_art(f, app, chunk, *fit),
      // Containers are flattened away by `layout_tree`.
      PluginWidgetKind::Row { .. } | PluginWidgetKind::Column { .. } => {}
    }
  }
}

/// Draw the current track's cover art into a plugin screen slot.
///
/// The image is measured then centered, because `ratatui-image` renders
/// top-left within the area it is given.
#[cfg(feature = "cover-art")]
fn draw_cover_art(f: &mut Frame<'_>, app: &App, area: Rect, fit: PluginCoverArtFit) {
  use crate::tui::layout::center_rect_within;
  use ratatui::widgets::Clear;

  if !app.cover_art.available() {
    draw_cover_art_message(
      f,
      app,
      area,
      crate::tui::cover_art::status_message(app.cover_art.status),
    );
    return;
  }

  let fitted = crate::tui::cover_art::plugin_size_for(area, fit, &app.cover_art).unwrap_or(area);
  let target = center_rect_within(area, fitted);

  // Clear the whole slot, not just `target`: a previous track's image may have
  // been larger, and only cells that enter the frame diff repaint over leftover
  // terminal graphics. `Clear` resets them to the terminal default, so restore
  // the theme background - unlike the fullscreen view, this sits inside a
  // themed block and would otherwise leave a hole around the image.
  f.render_widget(Clear, area);
  f.render_widget(
    Block::default().style(app.user_config.theme.base_style()),
    area,
  );
  crate::tui::cover_art::render_plugin(f, target, fit, &app.cover_art);
}

#[cfg(not(feature = "cover-art"))]
fn draw_cover_art(f: &mut Frame<'_>, app: &App, area: Rect, _fit: PluginCoverArtFit) {
  // The widget still parses without the feature so a plugin degrades to a
  // labelled gap rather than failing at `set_screen`.
  draw_cover_art_message(f, app, area, "Built without cover art");
}

fn draw_cover_art_message(f: &mut Frame<'_>, app: &App, area: Rect, message: &str) {
  if area.height == 0 {
    return;
  }
  let paragraph = Paragraph::new(message)
    .style(Style::default().fg(app.user_config.theme.inactive.into()))
    .alignment(Alignment::Center);
  f.render_widget(
    paragraph,
    Rect {
      x: area.x,
      y: area.y + area.height / 2,
      width: area.width,
      height: 1,
    },
  );
}

fn styled_line(pl: &PopupLine) -> Line<'static> {
  let mut style = Style::default();
  if let Some(fg) = pl.fg {
    style = style.fg(fg.into());
  }
  if pl.bold {
    style = style.add_modifier(Modifier::BOLD);
  }
  if pl.italic {
    style = style.add_modifier(Modifier::ITALIC);
  }
  Line::from(Span::styled(pl.text.clone(), style))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn paragraph() -> PluginWidgetKind {
    PluginWidgetKind::Paragraph {
      lines: Vec::new(),
      scroll: true,
    }
  }

  fn gauge() -> PluginWidgetKind {
    PluginWidgetKind::Gauge {
      ratio: 0.5,
      label: None,
    }
  }

  fn sized(
    kind: PluginWidgetKind,
    width: Option<PluginLength>,
    height: Option<PluginLength>,
  ) -> PluginWidget {
    PluginWidget {
      kind,
      width,
      height,
    }
  }

  fn rects(widgets: &[PluginWidget], area: Rect) -> Vec<Rect> {
    let mut out = Vec::new();
    layout_tree(widgets, area, Axis::Vertical, &mut out);
    out.into_iter().map(|(_, rect)| rect).collect()
  }

  #[test]
  fn top_level_stacks_vertically_and_splits_fill_evenly() {
    let widgets = vec![
      PluginWidget::new(paragraph()),
      PluginWidget::new(paragraph()),
    ];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 40, 20)),
      vec![Rect::new(0, 0, 40, 10), Rect::new(0, 10, 40, 10)]
    );
  }

  #[test]
  fn explicit_height_takes_its_rows_before_fill() {
    let widgets = vec![
      sized(paragraph(), None, Some(PluginLength::Cells(3))),
      PluginWidget::new(paragraph()),
    ];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 40, 20)),
      vec![Rect::new(0, 0, 40, 3), Rect::new(0, 3, 40, 17)]
    );
  }

  #[test]
  fn row_splits_horizontally_by_width_percent() {
    let widgets = vec![PluginWidget::new(PluginWidgetKind::Row {
      children: vec![
        sized(paragraph(), Some(PluginLength::Percent(40)), None),
        sized(paragraph(), Some(PluginLength::Percent(60)), None),
      ],
    })];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 100, 20)),
      vec![Rect::new(0, 0, 40, 20), Rect::new(40, 0, 60, 20)]
    );
  }

  #[test]
  fn row_nested_in_column_uses_each_containers_axis() {
    // The row is sized vertically by its own `height`; its children are then
    // sized horizontally by their `width`.
    let widgets = vec![
      sized(
        PluginWidgetKind::Row {
          children: vec![
            sized(paragraph(), Some(PluginLength::Cells(30)), None),
            PluginWidget::new(paragraph()),
          ],
        },
        None,
        Some(PluginLength::Cells(8)),
      ),
      PluginWidget::new(paragraph()),
    ];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 100, 20)),
      vec![
        Rect::new(0, 0, 30, 8),
        Rect::new(30, 0, 70, 8),
        Rect::new(0, 8, 100, 12),
      ]
    );
  }

  #[test]
  fn gauge_defaults_to_fixed_rows_but_honors_an_explicit_height() {
    let widgets = vec![
      PluginWidget::new(gauge()),
      sized(gauge(), None, Some(PluginLength::Cells(6))),
      PluginWidget::new(paragraph()),
    ];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 40, 20)),
      vec![
        Rect::new(0, 0, 40, GAUGE_ROWS),
        Rect::new(0, GAUGE_ROWS, 40, 6),
        Rect::new(0, GAUGE_ROWS + 6, 40, 11),
      ]
    );
  }

  #[test]
  fn gauge_in_a_row_fills_the_height_and_shares_the_width() {
    // The fixed row count only applies on the vertical axis.
    let widgets = vec![PluginWidget::new(PluginWidgetKind::Row {
      children: vec![PluginWidget::new(gauge()), PluginWidget::new(gauge())],
    })];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 40, 20)),
      vec![Rect::new(0, 0, 20, 20), Rect::new(20, 0, 20, 20)]
    );
  }

  fn render(content: PluginScreenContent, width: u16, height: u16) -> ratatui::buffer::Buffer {
    render_scrolled(content, width, height, 0)
  }

  fn render_scrolled(
    content: PluginScreenContent,
    width: u16,
    height: u16,
    scroll: u16,
  ) -> ratatui::buffer::Buffer {
    render_with(content, width, height, |app| {
      app.view.plugin_screen_scroll = scroll;
    })
    .1
  }

  fn render_with(
    content: PluginScreenContent,
    width: u16,
    height: u16,
    setup: impl FnOnce(&mut App),
  ) -> (App, ratatui::buffer::Buffer) {
    use crate::core::app::ActiveBlock;
    use ratatui::{backend::TestBackend, Terminal};

    let mut app = App::default();
    app.push_navigation_stack(
      RouteId::PluginScreen("demo".to_string()),
      ActiveBlock::PluginScreen,
    );
    app.plugin_screens.insert("demo".to_string(), content);
    setup(&mut app);

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw_plugin_screen(f, &app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (app, buffer)
  }

  fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    cols(buffer, y, 0..buffer.area().width)
  }

  /// Text in a column range. Ranges are taken over cells, not bytes: the border
  /// glyphs are multi-byte, so slicing the joined string would not line up.
  fn cols(buffer: &ratatui::buffer::Buffer, y: u16, range: std::ops::Range<u16>) -> String {
    range
      .filter_map(|x| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  fn text(line: &str) -> PopupLine {
    PopupLine {
      text: line.to_string(),
      fg: None,
      bold: false,
      italic: false,
    }
  }

  /// A `cover_art` beside a paragraph must draw the text in the right-hand
  /// column, and label the image slot rather than leaving it blank when no
  /// image is loaded (which is also the whole story in a build without the
  /// `cover-art` feature).
  #[test]
  fn cover_art_row_draws_the_sibling_paragraph_and_labels_the_image_slot() {
    let content = PluginScreenContent {
      title: "Demo".to_string(),
      widgets: vec![PluginWidget::new(PluginWidgetKind::Row {
        children: vec![
          sized(
            PluginWidgetKind::CoverArt {
              fit: PluginCoverArtFit::Contain,
            },
            Some(PluginLength::Percent(50)),
            None,
          ),
          sized(
            PluginWidgetKind::Paragraph {
              lines: vec![text("lyric line")],
              scroll: true,
            },
            Some(PluginLength::Percent(50)),
            None,
          ),
        ],
      })],
    };

    // Wide enough that the longest slot message fits in half the inner area.
    let buffer = render(content, 100, 12);
    // Inner area is x=1..99, y=1..11; the row fills it, so the two 50% columns
    // are cells 1..50 and 50..99.
    let (left, right) = (1..50, 50..99);

    let paragraph_row = cols(&buffer, 1, right.clone());
    assert!(
      paragraph_row.contains("lyric line"),
      "paragraph not in the right-hand column: {paragraph_row}"
    );
    let left_of_paragraph = cols(&buffer, 1, left.clone());
    assert!(
      !left_of_paragraph.contains("lyric line"),
      "paragraph leaked into the image column: {left_of_paragraph}"
    );

    // The message differs per build: with the feature on this is the
    // "no image loaded yet" status, with it off it is the degraded-build hint.
    #[cfg(feature = "cover-art")]
    let expected = "No cover art available";
    #[cfg(not(feature = "cover-art"))]
    let expected = "Built without cover art";

    let label_row = cols(&buffer, 1 + 10 / 2, left);
    assert!(
      label_row.contains(expected),
      "expected {expected:?} in the left-hand column: {label_row}"
    );
  }

  /// The image slot `Clear`s its cells to strip terminal graphics artifacts,
  /// which resets them to the terminal default - inside a themed block that
  /// would punch a visible hole. Nothing in the inner area may be left reset.
  #[cfg(feature = "cover-art")]
  #[test]
  fn cover_art_image_slot_restores_the_theme_background() {
    crate::tui::cover_art::init_test_renderer();
    let content = PluginScreenContent {
      title: "Demo".to_string(),
      widgets: vec![PluginWidget::new(PluginWidgetKind::CoverArt {
        fit: PluginCoverArtFit::Contain,
      })],
    };

    let background = crate::core::theme::Color::Rgb(10, 20, 30);
    let (_app, buffer) = render_with(content, 40, 20, |app| {
      app.user_config.theme.background = background;
      // 64x64 so the fitted image covers several cells whatever the detected
      // font size is. `store_decoded` is synchronous; no network involved.
      app
        .cover_art
        .store_decoded("test".to_string(), image::DynamicImage::new_rgb8(64, 64));
    });
    for y in 1..19 {
      for x in 1..39 {
        let cell = buffer.cell((x, y)).expect("cell in bounds");
        assert_ne!(
          cell.bg,
          ratatui::style::Color::Reset,
          "cell ({x}, {y}) left at the terminal default background"
        );
      }
    }
  }

  /// A `scroll = false` paragraph stays put while the shared PageUp/PageDown
  /// offset moves the rest - the case that matters once a header sits beside
  /// scrolling content in a `row`.
  #[test]
  fn scroll_offset_skips_paragraphs_that_opted_out() {
    let content = || PluginScreenContent {
      title: "Demo".to_string(),
      widgets: vec![
        sized(
          PluginWidgetKind::Paragraph {
            lines: vec![text("pinned header"), text("header line two")],
            scroll: false,
          },
          None,
          Some(PluginLength::Cells(4)),
        ),
        PluginWidget::new(PluginWidgetKind::Paragraph {
          lines: vec![text("body first"), text("body second")],
          scroll: true,
        }),
      ],
    };

    let unscrolled = render(content(), 40, 12);
    assert!(row_text(&unscrolled, 1).contains("pinned header"));
    assert!(row_text(&unscrolled, 5).contains("body first"));

    let scrolled = render_scrolled(content(), 40, 12, 1);
    assert!(
      row_text(&scrolled, 1).contains("pinned header"),
      "opted-out paragraph scrolled: {}",
      row_text(&scrolled, 1)
    );
    assert!(
      row_text(&scrolled, 5).contains("body second"),
      "scrolling paragraph did not move: {}",
      row_text(&scrolled, 5)
    );
  }

  #[test]
  fn empty_container_contributes_no_leaves() {
    let widgets = vec![
      PluginWidget::new(PluginWidgetKind::Row {
        children: Vec::new(),
      }),
      PluginWidget::new(paragraph()),
    ];
    assert_eq!(
      rects(&widgets, Rect::new(0, 0, 40, 20)),
      vec![Rect::new(0, 10, 40, 10)]
    );
  }
}
