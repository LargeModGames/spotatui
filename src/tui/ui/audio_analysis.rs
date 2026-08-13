use crate::core::app::App;
use crate::core::layout::main_layout_margin;
use crate::core::user_config::{normalize_tick_rate_milliseconds, VisualizerStyle};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Color, Style},
  symbols::bar,
  text::{Line, Span},
  widgets::{Block, Borders, Paragraph},
  Frame,
};

use tui_bar_graph::{BarGraph, BarStyle, ColorMode};

/// The analysis screen's two rows: the info strip and the visualizer block.
fn analysis_areas(root: Rect, margin: u16) -> [Rect; 2] {
  root.layout(&Layout::vertical([Constraint::Length(3), Constraint::Min(10)]).margin(margin))
}

/// The visualizer's surrounding block, borders only. `draw` styles and titles
/// it; `visualizer_inner_width` needs it only to measure the border inset.
fn visualizer_block<'a>() -> Block<'a> {
  Block::default().borders(Borders::ALL)
}

/// Inner width of the visualizer block at the current terminal size. The
/// runner picks the analyzer's bar count before a frame exists, so it derives
/// the rect from `app.size` through here rather than re-deriving the margin
/// and border inset itself.
///
/// This and `visualizer_bar_count` are only called behind the audio-viz
/// feature gates; slim builds still compile (and test) them.
#[allow(dead_code)]
pub fn visualizer_inner_width(app: &App) -> u16 {
  let root = Rect::new(0, 0, app.size.width, app.size.height);
  let [_, visualizer_area] = analysis_areas(root, main_layout_margin(app));
  visualizer_block().inner(visualizer_area).width
}

pub fn draw(f: &mut Frame<'_>, app: &App) {
  let margin = main_layout_margin(app);

  let [info_area, visualizer_area] = analysis_areas(f.area(), margin);

  let white = Style::default().fg(app.user_config.theme.text);
  let gray = Style::default().fg(app.user_config.theme.inactive);
  let tick_rate = app.user_config.behavior.animation_tick_rate_milliseconds;
  let tick_rate = normalize_tick_rate_milliseconds(tick_rate as i64);
  let visualizer_style = app.user_config.behavior.visualizer_style;

  let info_block = Block::default()
    .title(Span::styled(
      format!("Audio Visualization ({})", visualizer_style.name()),
      Style::default().fg(app.user_config.theme.inactive),
    ))
    .borders(Borders::ALL)
    .border_style(Style::default().fg(app.user_config.theme.inactive));

  let bar_chart_title = &format!("Spectrum | {} FPS | Press q to exit", 1000 / tick_rate);

  let bar_chart_block = visualizer_block()
    .style(white)
    .title(Span::styled(bar_chart_title, gray))
    .border_style(gray);

  // Check if we have spectrum data from local audio capture
  if let Some(ref spectrum) = app.spectrum_data {
    // Info panel with status
    // Use ASCII-safe symbols instead of emojis for Windows compatibility
    let status_text = if app.audio_capture_active {
      "[>] Capturing audio"
    } else {
      "[||] Paused"
    };

    let peak = spectrum.bands.iter().copied().fold(0.0f32, f32::max);
    let peak_text = format!("Peak: {:.0}%", peak * 100.0);
    let style_hint = "Press 'V' to cycle visualizer style";

    let texts = vec![Line::from(vec![
      Span::styled(status_text, Style::default().fg(app.user_config.theme.text)),
      Span::raw("  "),
      Span::styled(
        peak_text,
        Style::default().fg(app.user_config.theme.inactive),
      ),
      Span::raw("  |  "),
      Span::styled(style_hint, Style::default().fg(app.user_config.theme.hint)),
    ])];

    let p = Paragraph::new(texts)
      .block(info_block)
      .style(Style::default().fg(app.user_config.theme.text));
    f.render_widget(p, info_area);

    // Calculate inner area for visualizer (within the block borders)
    let inner_area = bar_chart_block.inner(visualizer_area);
    f.render_widget(bar_chart_block, visualizer_area);

    // Render the appropriate visualizer based on user setting
    match visualizer_style {
      VisualizerStyle::BarGraph => render_bar_graph(f, &spectrum.bands, inner_area),
      VisualizerStyle::Cava => render_cava(
        f,
        &spectrum.bands,
        inner_area,
        app.user_config.theme.analysis_bar,
      ),
    }
  } else {
    // No audio capture available
    let no_capture_text = vec![
      Line::from("No audio capture available"),
      Line::from(""),
      #[cfg(target_os = "linux")]
      Line::from("Hint: Ensure PipeWire or PulseAudio is running with a monitor device"),
      #[cfg(target_os = "windows")]
      Line::from("Hint: Audio loopback should work automatically on Windows"),
      #[cfg(target_os = "macos")]
      Line::from("Hint: macOS requires a virtual audio device like BlackHole"),
      #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
      Line::from("Hint: Audio capture may not be supported on this platform"),
    ];

    let p = Paragraph::new(no_capture_text)
      .block(info_block)
      .style(Style::default().fg(app.user_config.theme.text));
    f.render_widget(p, info_area);

    // Empty bar chart
    let empty_p = Paragraph::new("Waiting for audio input...")
      .block(bar_chart_block)
      .style(Style::default().fg(app.user_config.theme.text));
    f.render_widget(empty_p, visualizer_area);
  }
}

/// Render bar graph-style visualization using tui-bar-graph
/// https://github.com/joshka/tui-widgets/tree/main/tui-bar-graph
///
/// The tui-bar-graph widget fills the entire area with one bar per column.
fn render_bar_graph(f: &mut Frame<'_>, bands: &[f32], area: Rect) {
  if bands.is_empty() || area.width == 0 || area.height == 0 {
    return;
  }

  let target_width = (area.width as usize) * 2;
  let data = interpolate_bands(bands, target_width);

  let bar_graph = BarGraph::new(data)
    .with_gradient(colorgrad::preset::turbo())
    .with_bar_style(BarStyle::Braille) // Braille for high-res, Solid for blocks
    .with_color_mode(ColorMode::VerticalGradient)
    .with_max(1.0);

  f.render_widget(bar_graph, area);
}

/// Linearly interpolate band values onto `target_width` points. Bands normally
/// already arrive at Braille resolution (2 per cell, requested by the runner),
/// in which case this is an exact copy. They fall short when the analyzer caps
/// the count at what the sample rate supports (513 at 44.1/48 kHz, i.e.
/// terminals wider than ~257 columns) or when a style switch or resize lags a
/// frame; stretching keeps the right side from going blank, since the widget
/// draws left-aligned.
fn interpolate_bands(bands: &[f32], target_width: usize) -> Vec<f64> {
  if bands.is_empty() {
    return vec![0.0; target_width];
  }
  if bands.len() == 1 {
    return vec![f64::from(bands[0]); target_width];
  }

  let mut result = Vec::with_capacity(target_width);
  let scale = (bands.len() - 1) as f64 / (target_width - 1).max(1) as f64;

  for i in 0..target_width {
    let pos = i as f64 * scale;
    let idx = pos.floor() as usize;
    let frac = pos - idx as f64;

    let value = if idx + 1 < bands.len() {
      f64::from(bands[idx]) * (1.0 - frac) + f64::from(bands[idx + 1]) * frac
    } else {
      f64::from(bands[idx.min(bands.len() - 1)])
    };

    result.push(value);
  }

  result
}

/// Bar glyphs from empty to full block, indexed by filled eighths (0-8):
/// ratatui's own nine-level bar set, with a blank for the empty cell.
const EIGHTH_GLYPHS: [&str; 9] = [
  bar::NINE_LEVELS.empty,
  bar::NINE_LEVELS.one_eighth,
  bar::NINE_LEVELS.one_quarter,
  bar::NINE_LEVELS.three_eighths,
  bar::NINE_LEVELS.half,
  bar::NINE_LEVELS.five_eighths,
  bar::NINE_LEVELS.three_quarters,
  bar::NINE_LEVELS.seven_eighths,
  bar::NINE_LEVELS.full,
];

/// Total filled eighths for a bar value in a column `rows` cells tall, with
/// cava's idle floor of one eighth (the signature resting line). u32 math:
/// rows * 8 would overflow u16 on pathological (>8191-row) terminal heights.
fn bar_eighths(value: f32, rows: u16) -> u32 {
  let max_eighths = u32::from(rows) * 8;
  let total = (value.clamp(0.0, 1.0) * max_eighths as f32) as u32;
  total.clamp(1, max_eighths)
}

/// Eighths filled (0-8) in the cell `row` rows above the column's bottom.
fn cell_eighths(total_eighths: u32, row: u16) -> u32 {
  total_eighths.saturating_sub(u32::from(row) * 8).min(8)
}

/// Cap on the cava style's bar count: the classic cava look at a normal font.
/// Past this the renderer widens bars to fill the terminal, so dense fonts get
/// chunky columns instead of one pencil bar per 3 columns.
#[allow(dead_code)]
const MAX_CAVA_BARS: usize = 40;

/// cava's own bar geometry, 2 columns of bar plus a 1-column gap. It is the
/// floor for `cava_bar_layout`, and the divisor `visualizer_bar_count` uses to
/// pick a count that fits.
const CAVA_BAR_FOOTPRINT: u16 = 3;

/// Bars the analyzer should produce for the visualizer block's `inner_width`.
/// Each style has its own geometry; the analyzer clamps counts its sample rate
/// cannot support, and renderers draw from `bands.len()`, so a stale count
/// during a resize race only mis-sizes one frame.
#[allow(dead_code)]
pub fn visualizer_bar_count(style: VisualizerStyle, inner_width: u16) -> usize {
  let inner_width = usize::from(inner_width);
  match style {
    // Braille has 2x horizontal resolution: one real bar per half-cell.
    VisualizerStyle::BarGraph => (inner_width * 2).max(2),
    // Below ~120 columns this degrades to exactly cava's auto rule.
    VisualizerStyle::Cava => {
      ((inner_width + 1) / usize::from(CAVA_BAR_FOOTPRINT)).clamp(2, MAX_CAVA_BARS)
    }
  }
}

/// Bar width and spacing for the cava style. The bar count is capped at
/// `MAX_CAVA_BARS`, so bars widen with the terminal to fill the area at
/// roughly cava's 2:1 bar-to-gap ratio, with `CAVA_BAR_FOOTPRINT` as the floor.
fn cava_bar_layout(inner_width: u16, bars: u16) -> (u16, u16) {
  if bars == 0 {
    return (CAVA_BAR_FOOTPRINT - 1, 1);
  }
  // u32 internals, footprint capped at the width: keeps the math (and the
  // renderer's bar_width + spacing sums) off u16 overflow at any input.
  let floor = u32::from(CAVA_BAR_FOOTPRINT);
  let footprint = ((u32::from(inner_width) + 1) / u32::from(bars))
    .clamp(floor, u32::from(inner_width).max(floor));
  let spacing = ((footprint + 1) / floor).max(1);
  ((footprint - spacing) as u16, spacing as u16)
}

/// Render cava-style visualization: one independent bar per band drawn in
/// eighth blocks, widened to fill the width (see `cava_bar_layout`), centered.
fn render_cava(f: &mut Frame<'_>, bands: &[f32], area: Rect, color: Color) {
  if bands.is_empty() || area.width == 0 || area.height == 0 {
    return;
  }

  let (bar_width, spacing) = cava_bar_layout(area.width, bands.len() as u16);
  let step = bar_width + spacing;
  let drawn_width = (bands.len() as u16) * step - spacing;
  let x_offset = area.width.saturating_sub(drawn_width) / 2;
  let style = Style::default().fg(color);
  let buf = f.buffer_mut();

  for (i, &value) in bands.iter().enumerate() {
    let x0 = area.x + x_offset + (i as u16) * step;
    // Clip bars that no longer fit (the bar count lags one frame on resize).
    if x0 + bar_width > area.x + area.width {
      break;
    }

    let total_eighths = bar_eighths(value, area.height);
    for row in 0..area.height {
      let filled = cell_eighths(total_eighths, row);
      if filled == 0 {
        break;
      }
      let glyph = EIGHTH_GLYPHS[filled as usize];
      let y = area.y + area.height - 1 - row;
      for dx in 0..bar_width {
        buf[(x0 + dx, y)].set_symbol(glyph).set_style(style);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn bar_eighths_has_cavas_idle_floor() {
    assert_eq!(bar_eighths(0.0, 10), 1);
  }

  #[test]
  fn bar_eighths_fills_the_column_at_full_scale() {
    assert_eq!(bar_eighths(1.0, 10), 80);
  }

  #[test]
  fn bar_eighths_clamps_overshoot() {
    assert_eq!(bar_eighths(1.7, 10), 80);
  }

  #[test]
  fn cava_bar_layout_widens_bars_to_fill_dense_terminals() {
    // 216 columns, 40 bars: 5 columns per bar -> 3-wide bars, 2-wide gaps.
    assert_eq!(cava_bar_layout(216, 40), (3, 2));
    // 400 columns: 10 per bar -> 7-wide bars, 3-wide gaps (~2:1 holds).
    assert_eq!(cava_bar_layout(400, 40), (7, 3));
  }

  #[test]
  fn cava_bar_layout_floors_at_cavas_own_geometry() {
    // Exactly enough room for 40 bars at cava's default 2+1 geometry.
    assert_eq!(cava_bar_layout(119, 40), (2, 1));
    // Narrow terminals (fewer bars requested) keep the 2+1 default.
    assert_eq!(cava_bar_layout(80, 27), (2, 1));
    // Too narrow to fit: geometry floors at 2+1 and the renderer clips.
    assert_eq!(cava_bar_layout(5, 40), (2, 1));
  }

  #[test]
  fn cell_glyphs_build_bottom_up() {
    // 11 eighths in a 3-row column: full block, then three eighths, then air.
    let glyphs: Vec<&str> = (0..3)
      .map(|row| EIGHTH_GLYPHS[cell_eighths(11, row) as usize])
      .collect();
    assert_eq!(glyphs, vec!["█", "▃", " "]);
  }

  #[test]
  fn visualizer_bar_count_follows_each_styles_geometry() {
    // BarGraph gets two Braille bars per cell.
    assert_eq!(visualizer_bar_count(VisualizerStyle::BarGraph, 100), 200);

    // Cava: one bar per 3 columns (bar width 2 + spacing 1).
    assert_eq!(visualizer_bar_count(VisualizerStyle::Cava, 80), 27);
  }

  #[test]
  fn visualizer_bar_count_clamps_degenerate_widths() {
    // Zero-width terminals still request a drawable minimum.
    assert_eq!(visualizer_bar_count(VisualizerStyle::Cava, 0), 2);
    assert_eq!(visualizer_bar_count(VisualizerStyle::BarGraph, 0), 2);

    // The bar cap holds on wide terminals and dense fonts.
    assert_eq!(visualizer_bar_count(VisualizerStyle::Cava, 4000), 40);
    assert_eq!(visualizer_bar_count(VisualizerStyle::Cava, 216), 40);
  }

  #[test]
  fn requested_bar_count_and_drawn_geometry_agree() {
    // The two halves of the cava contract: the count the runner asks for must
    // fit the width the renderer lays it out in, at every width.
    for inner_width in [20u16, 40, 80, 119, 120, 216, 400] {
      let bars = visualizer_bar_count(VisualizerStyle::Cava, inner_width) as u16;
      let (bar_width, spacing) = cava_bar_layout(inner_width, bars);
      let drawn = bars * (bar_width + spacing) - spacing;
      assert!(
        drawn <= inner_width,
        "{bars} bars drawn as {drawn} columns overflow width {inner_width}"
      );
    }
  }
}
