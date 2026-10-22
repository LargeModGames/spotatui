use crate::core::app::{section_at, AnalysisState, App, PITCH_NAMES};
use crate::core::user_config::{normalize_tick_rate_milliseconds, VisualizerStyle};
use crate::tui::layout::main_layout_margin;
use crate::tui::ui::{head_unit, retro};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  style::{Color, Style},
  symbols::bar,
  text::{Line, Span},
  widgets::{Block, Borders, Paragraph},
  Frame,
};
use std::time::Instant;

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
/// the rect from `app.view.size` through here rather than re-deriving the margin
/// and border inset itself.
///
/// This and `visualizer_bar_count` are only called behind the audio-viz
/// feature gates; slim builds still compile (and test) them.
#[allow(dead_code)]
pub fn visualizer_inner_width(app: &App) -> u16 {
  let root = Rect::new(0, 0, app.view.size.width, app.view.size.height);
  let [_, visualizer_area] = analysis_areas(root, main_layout_margin(app));
  visualizer_block().inner(visualizer_area).width
}

pub fn draw(f: &mut Frame<'_>, app: &App) {
  let margin = main_layout_margin(app);

  let [info_area, visualizer_area] = analysis_areas(f.area(), margin);

  let white = Style::default().fg(app.user_config.theme.text.into());
  let gray = Style::default().fg(app.user_config.theme.inactive.into());
  let tick_rate = app.user_config.behavior.animation_tick_rate_milliseconds;
  let tick_rate = normalize_tick_rate_milliseconds(tick_rate as i64);
  let visualizer_style = app.user_config.behavior.visualizer_style;
  let pitch_view = app.pitch_view_shows();
  let (style_name, chart_name) = if pitch_view {
    (
      app.strings().screens.spotify_pitch,
      app.strings().screens.pitches,
    )
  } else {
    (
      match visualizer_style {
        VisualizerStyle::Cava => "Cava",
        VisualizerStyle::BarGraph => app.strings().screens.bar_graph,
        VisualizerStyle::HeadUnit => app.strings().screens.head_unit,
        VisualizerStyle::AnalogVu => app.strings().screens.analog_vu,
        VisualizerStyle::Winamp => app.strings().screens.winamp,
        VisualizerStyle::PhosphorScope => app.strings().screens.phosphor_scope,
        VisualizerStyle::Outrun => app.strings().screens.outrun,
        VisualizerStyle::RackEq => app.strings().screens.rack_eq,
        VisualizerStyle::Cassette => app.strings().screens.cassette_deck,
      },
      app.strings().screens.spectrum,
    )
  };

  let info_block = Block::default()
    .title(Span::styled(
      app.strings().screens.visualization_title(style_name),
      Style::default().fg(app.user_config.theme.inactive.into()),
    ))
    .borders(Borders::ALL)
    .border_style(Style::default().fg(app.user_config.theme.inactive.into()));

  let bar_chart_title = &app.strings().screens.chart_title(
    chart_name,
    &(1000 / tick_rate).to_string(),
    &app.user_config.keys.back.to_string(),
  );

  let bar_chart_block = visualizer_block()
    .style(white)
    .title(Span::styled(bar_chart_title, gray))
    .border_style(gray);

  if pitch_view {
    draw_pitch(
      f,
      app,
      info_block,
      bar_chart_block,
      [info_area, visualizer_area],
    );
    return;
  }

  // Check if we have spectrum data from local audio capture
  if let Some(ref spectrum) = app.spectrum_data {
    // Info panel with status
    // Use ASCII-safe symbols instead of emojis for Windows compatibility
    let status_text = if app.audio_capture_active {
      app.strings().screens.capturing_audio
    } else {
      app.strings().screens.capture_paused
    };

    let peak = spectrum.bands.iter().copied().fold(0.0f32, f32::max);
    let peak_text = app.strings().screens.peak(&format!("{:.0}", peak * 100.0));

    let texts = vec![Line::from(vec![
      Span::styled(
        status_text,
        Style::default().fg(app.user_config.theme.text.into()),
      ),
      Span::raw("  "),
      Span::styled(
        peak_text,
        Style::default().fg(app.user_config.theme.inactive.into()),
      ),
      Span::raw("  |  "),
      Span::styled(
        app.strings().screens.visualizer_style_hint,
        Style::default().fg(app.user_config.theme.hint.into()),
      ),
    ])];

    let p = Paragraph::new(texts)
      .block(info_block)
      .style(Style::default().fg(app.user_config.theme.text.into()));
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
        app.user_config.theme.analysis_bar.into(),
      ),
      VisualizerStyle::HeadUnit => head_unit::render(
        f.buffer_mut(),
        inner_area,
        &spectrum.bands,
        &app.user_config.theme,
        &head_unit_view(app),
        Instant::now(),
      ),
      style => {
        if let Some(retro_style) = retro_style(style) {
          let view = head_unit_view(app);
          let frame = retro::Frame {
            audio: retro::Audio {
              bands: &spectrum.bands,
              wave: &spectrum.wave,
            },
            view: &view,
            now: Instant::now(),
          };
          retro::render(retro_style, f.buffer_mut(), inner_area, &frame);
        }
      }
    }
  } else {
    // No audio capture available
    let no_capture_text = vec![
      Line::from(app.strings().screens.no_capture),
      Line::from(""),
      #[cfg(target_os = "linux")]
      Line::from(app.strings().screens.capture_hint_linux),
      #[cfg(target_os = "windows")]
      Line::from(app.strings().screens.capture_hint_windows),
      #[cfg(target_os = "macos")]
      Line::from(app.strings().screens.capture_hint_macos),
      #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
      Line::from(app.strings().screens.capture_hint_other),
    ];

    let p = Paragraph::new(no_capture_text)
      .block(info_block)
      .style(Style::default().fg(app.user_config.theme.text.into()));
    f.render_widget(p, info_area);

    // Empty bar chart
    let empty_p = Paragraph::new(app.strings().screens.audio_waiting)
      .block(bar_chart_block)
      .style(Style::default().fg(app.user_config.theme.text.into()));
    f.render_widget(empty_p, visualizer_area);
  }
}

/// The retro style a visualizer style draws, if it is one.
fn retro_style(style: VisualizerStyle) -> Option<retro::Retro> {
  match style {
    VisualizerStyle::AnalogVu => Some(retro::Retro::AnalogVu),
    VisualizerStyle::Winamp => Some(retro::Retro::Winamp),
    VisualizerStyle::PhosphorScope => Some(retro::Retro::PhosphorScope),
    VisualizerStyle::Outrun => Some(retro::Retro::Outrun),
    VisualizerStyle::RackEq => Some(retro::Retro::RackEq),
    VisualizerStyle::Cassette => Some(retro::Retro::Cassette),
    VisualizerStyle::Cava | VisualizerStyle::BarGraph | VisualizerStyle::HeadUnit => None,
  }
}

/// The Head Unit readout, from the source-agnostic playback snapshot so it
/// reads the same for every source.
fn head_unit_view(app: &App) -> head_unit::HeadUnitView {
  use rspotify::model::RepeatState;
  match crate::infra::media_metadata::current_playback_snapshot(app) {
    Some(snapshot) => head_unit::HeadUnitView {
      title: Some(snapshot.metadata.title),
      progress_ms: Some(snapshot.progress_ms),
      duration_ms: Some(snapshot.metadata.duration_ms).filter(|&d| d > 0),
      is_playing: snapshot.is_playing,
      shuffle: snapshot.shuffle,
      repeat: snapshot.repeat.is_some_and(|r| r != RepeatState::Off),
      live: snapshot.is_live,
    },
    None => head_unit::HeadUnitView::default(),
  }
}

/// The Spotify pitch view: tempo, key and time signature of the current section,
/// and the 12 pitch bars of the current segment.
fn draw_pitch(
  f: &mut Frame<'_>,
  app: &App,
  info_block: Block<'_>,
  chart_block: Block<'_>,
  [info_area, chart_area]: [Rect; 2],
) {
  let theme = &app.user_config.theme;
  let text = Style::default().fg(theme.text.into());
  let hint = Span::styled(
    app.strings().screens.visualizer_style_hint,
    Style::default().fg(theme.hint.into()),
  );
  let AnalysisState::Ready(_, analysis) = app.audio_analysis() else {
    let message = match app.audio_analysis() {
      AnalysisState::Unavailable(_) => app.strings().screens.pitch_unavailable,
      AnalysisState::Failed(..) => app.strings().screens.pitch_failed,
      _ => app.strings().screens.pitch_loading,
    };
    f.render_widget(
      Paragraph::new(Line::from(hint)).block(info_block),
      info_area,
    );
    f.render_widget(
      Paragraph::new(message).block(chart_block).style(text),
      chart_area,
    );
    return;
  };

  let position = app.analysis_position_ms(Instant::now()).unwrap_or(0);
  let mut info = Vec::new();
  if let Some(section) = section_at(analysis, position) {
    info.push(Span::styled(
      format!(
        "{:.0} BPM ({:.0}%)  {} ({:.0}%)  {}/4 ({:.0}%)",
        section.tempo,
        section.tempo_confidence * 100.0,
        localized_key_label(app, section.key, section.minor),
        section.key_confidence * 100.0,
        section.time_signature,
        section.time_signature_confidence * 100.0
      ),
      text,
    ));
    info.push(Span::raw("  |  "));
  }
  info.push(hint);
  f.render_widget(
    Paragraph::new(Line::from(info)).block(info_block),
    info_area,
  );

  let inner = chart_block.inner(chart_area);
  f.render_widget(chart_block, chart_area);
  let [bars_area, labels_area] = inner.layout(&Layout::vertical([
    Constraint::Min(1),
    Constraint::Length(1),
  ]));
  let count = visualizer_bar_count(VisualizerStyle::Cava, bars_area.width);
  let bands: Vec<f32> = interpolate_bands(&app.pitch_levels(), count)
    .into_iter()
    .map(|value| value as f32)
    .collect();
  render_cava(f, &bands, bars_area, theme.analysis_bar.into());
  f.render_widget(
    Paragraph::new(pitch_labels(labels_area.width, count)).style(text),
    labels_area,
  );
}

/// The note names under the pitch bars, each under the bar that carries its pitch class.
fn localized_key_label(app: &App, key: Option<u8>, minor: Option<bool>) -> String {
  let Some(name) = key.and_then(|key| PITCH_NAMES.get(usize::from(key))) else {
    return "?".to_string();
  };
  match minor {
    Some(true) => app.strings().screens.minor_key(name),
    Some(false) => app.strings().screens.major_key(name),
    None => name.to_string(),
  }
}

fn pitch_labels(width: u16, bars: usize) -> String {
  let mut line = vec![' '; usize::from(width)];
  if bars < 2 {
    return line.into_iter().collect();
  }
  let (bar_width, spacing) = cava_bar_layout(width, bars as u16);
  let step = bar_width + spacing;
  let x_offset = width.saturating_sub(bars as u16 * step - spacing) / 2;
  for (pitch, name) in PITCH_NAMES.iter().enumerate() {
    let bar = (pitch * (bars - 1) + 5) / 11;
    let x = usize::from(x_offset + bar as u16 * step + bar_width / 2);
    for (offset, letter) in name.chars().enumerate() {
      if let Some(cell) = line.get_mut(x + offset) {
        *cell = letter;
      }
    }
  }
  line.into_iter().collect()
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
    // One bar per band and channel: the renderer folds the mirrored halves.
    VisualizerStyle::HeadUnit => head_unit::band_count() * 2,
    style => retro_style(style).map_or(2, retro::Retro::bar_count),
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

  #[test]
  fn spectrum_title_names_the_configured_back_key() {
    use crate::tui::event::Key;
    use ratatui::{backend::TestBackend, Terminal};

    let mut app = App::default();
    app.user_config.keys.back = Key::Char('x');

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| draw(f, &app)).unwrap();
    let buffer = terminal.backend().buffer();
    let content: String = (0..30)
      .flat_map(|y| (0..120).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect();

    assert!(
      content.contains("Press x to exit"),
      "spectrum title should name the rebound back key: {content}"
    );
    assert!(
      !content.contains("Press q to exit"),
      "spectrum title must not still show the default key: {content}"
    );
  }

  fn screen_text(app: &App) -> String {
    use ratatui::{backend::TestBackend, Terminal};

    let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
    terminal.draw(|f| draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..30)
      .flat_map(|y| (0..140).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  #[test]
  fn pitch_view_names_tempo_key_and_time_signature() {
    use crate::core::app::{AnalysisFetch, AnalysisSection, AnalysisSegment, TrackAnalysis};

    let mut app = App::default();
    app.get_audio_analysis();
    app.sync_audio_analysis(Some("track1"));
    app.set_audio_analysis(
      "track1",
      AnalysisFetch::Found(TrackAnalysis {
        sections: vec![AnalysisSection {
          start_ms: 0,
          tempo: 123.0,
          tempo_confidence: 0.82,
          key: Some(6),
          key_confidence: 0.64,
          minor: Some(true),
          time_signature: 4,
          time_signature_confidence: 1.0,
        }],
        segments: vec![AnalysisSegment {
          start_ms: 0,
          loudness_start: 0.0,
          loudness_max: 0.0,
          loudness_max_ms: 0,
          pitches: [1.0; 12],
        }],
      }),
    );

    let content = screen_text(&app);

    for expected in [
      "Audio Visualization (Spotify Pitch)",
      "Pitches |",
      "123 BPM (82%)",
      "F# minor (64%)",
      "4/4 (100%)",
      "C#",
    ] {
      assert!(
        content.contains(expected),
        "missing {expected:?}: {content}"
      );
    }
  }

  #[test]
  fn pitch_labels_run_from_c_under_the_first_bar_to_b_under_the_last() {
    let line = pitch_labels(120, visualizer_bar_count(VisualizerStyle::Cava, 120));

    assert_eq!(line.chars().count(), 120);
    assert!(line.trim_start().starts_with("C "), "{line:?}");
    assert!(line.trim_end().ends_with('B'), "{line:?}");
    let names: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(names, PITCH_NAMES);
  }

  #[test]
  fn pitch_view_says_when_spotify_has_no_analysis() {
    let mut app = App::default();
    app.get_audio_analysis();
    app.sync_audio_analysis(Some("track1"));
    app.set_audio_analysis("track1", crate::core::app::AnalysisFetch::Missing);

    let content = screen_text(&app);

    assert!(content.contains("Spotify has no pitch analysis for this track"));
    assert!(!content.contains("No audio capture available"));
  }
}
