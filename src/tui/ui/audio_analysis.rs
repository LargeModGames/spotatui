use super::util;
use crate::core::app::App;
use crate::core::user_config::VisualizerStyle;
use ratatui::{
  buffer::Buffer,
  layout::{Constraint, Layout, Rect},
  style::Style,
  text::{Line, Span},
  widgets::{Block, Borders, Paragraph, Widget},
  Frame,
};

use tui_bar_graph::{BarGraph, BarStyle, ColorMode};
use tui_equalizer::{Band, Equalizer};

pub fn draw(f: &mut Frame<'_>, app: &App) {
  let margin = util::get_main_layout_margin(app);

  let [info_area, visualizer_area] = f
    .area()
    .layout(&Layout::vertical([Constraint::Length(3), Constraint::Min(10)]).margin(margin));

  let white = Style::default().fg(app.user_config.theme.text);
  let gray = Style::default().fg(app.user_config.theme.inactive);
  let tick_rate = app.user_config.behavior.tick_rate_milliseconds;
  let visualizer_style = app.user_config.behavior.visualizer_style;

  let info_block = Block::default()
    .title(Span::styled(
      format!("Audio Visualization ({})", visualizer_style.name()),
      Style::default().fg(app.user_config.theme.inactive),
    ))
    .borders(Borders::ALL)
    .border_style(Style::default().fg(app.user_config.theme.inactive));

  let bar_chart_title = &format!("Spectrum | {} FPS | Press q to exit", 1000 / tick_rate);

  let bar_chart_block = Block::default()
    .borders(Borders::ALL)
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

    let peak_text = format!("Peak: {:.0}%", spectrum.peak * 100.0);
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

    // Render the appropriate visualizer based on user setting
    match visualizer_style {
      VisualizerStyle::Equalizer => {
        f.render_widget(bar_chart_block, visualizer_area);
        render_equalizer(f, &spectrum.bands, inner_area);
      }
      VisualizerStyle::BarGraph => {
        f.render_widget(bar_chart_block, visualizer_area);
        render_bar_graph(f, &spectrum.bands, inner_area);
      }
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

/// Render equalizer-style visualization using tui-equalizer
/// https://github.com/joshka/tui-equalizer
///
/// The tui-equalizer widget renders each band at 2 chars wide, left-aligned.
/// We pass the 12 frequency bands directly for a clean look.
fn render_equalizer(f: &mut Frame<'_>, bands: &[f32], area: Rect) {
  if bands.is_empty() || area.width == 0 || area.height == 0 {
    return;
  }

  // tui-equalizer renders best (and fastest) with a small, fixed number of bands.
  // We deliberately keep this to the raw analyzer band count (12).
  let eq_bands: Vec<Band> = bands
    .iter()
    .map(|&v| {
      // Visually boost quieter signals so the equalizer "reaches" higher.
      const EQ_GAMMA: f64 = 0.65; // < 1.0 boosts lows
      const EQ_GAIN: f64 = 1.35; // overall gain
      let value = (v.clamp(0.0, 1.0) as f64).powf(EQ_GAMMA) * EQ_GAIN;
      Band::from(value.clamp(0.0, 1.0))
    })
    .collect();

  let equalizer = Equalizer {
    bands: eq_bands,
    brightness: 1.0,
  };

  // Cap height to keep rendering fast on very tall terminals.
  const MAX_EQ_HEIGHT: u16 = 24;
  let render_height = area.height.clamp(1, MAX_EQ_HEIGHT);
  let base_width = (bands.len() as u16) * 2;

  // Render into a small off-screen buffer, then "stretch" each band horizontally by repeating it.
  // This fills the available width without generating additional (interpolated) bands.
  let tmp_area = Rect::new(0, 0, base_width, render_height);
  let mut tmp = Buffer::empty(tmp_area);
  equalizer.render(tmp_area, &mut tmp);

  // tui-equalizer draws only the left cell of each 2-cell band. Duplicate it into the right cell
  // so we don't alternate colored/default cells (a major perf hit on Windows terminals).
  for band_index in 0..(bands.len() as u16) {
    let left_x = band_index * 2;
    let right_x = left_x + 1;
    for y in 0..render_height {
      let left_cell = tmp[(left_x, y)].clone();
      tmp[(right_x, y)] = left_cell;
    }
  }

  let target_width = area.width & !1;
  if target_width < base_width {
    // Too narrow to fit all bands; just render what we can centered.
    let render_width = target_width.max(2);
    let render_x = area.x + area.width.saturating_sub(render_width) / 2;
    let render_area = Rect {
      x: render_x,
      y: area.y + area.height.saturating_sub(render_height),
      width: render_width,
      height: render_height,
    };
    let buf = f.buffer_mut();
    for y in 0..render_height {
      for x in 0..render_width {
        buf[(render_area.x + x, render_area.y + y)] = tmp[(x, y)].clone();
      }
    }
    return;
  }

  // Distribute width evenly across bands in 2-cell "pairs" so each band stays aligned.
  let pairs_total = (target_width / 2) as usize;
  let band_count = bands.len();
  let pairs_per_band = pairs_total / band_count;
  if pairs_per_band == 0 {
    return;
  }
  let extra_pairs = pairs_total % band_count;

  let render_area = Rect {
    x: area.x,
    y: area.y + area.height.saturating_sub(render_height),
    width: target_width,
    height: render_height,
  };

  let buf = f.buffer_mut();
  let mut x_cursor: u16 = 0;
  for band_index in 0..band_count {
    let band_pairs = pairs_per_band + usize::from(band_index < extra_pairs);
    let band_width = (band_pairs as u16) * 2;
    let src_x = (band_index as u16) * 2;

    for y in 0..render_height {
      let cell = tmp[(src_x, y)].clone();
      for dx in 0..band_width {
        buf[(render_area.x + x_cursor + dx, render_area.y + y)] = cell.clone();
      }
    }

    x_cursor += band_width;
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

  // In Braille mode the widget has 2x horizontal resolution, so feed 2 values per cell.
  let target_width = (area.width as usize) * 2;
  let data = interpolate_bands(bands, target_width);

  let bar_graph = BarGraph::new(data)
    .with_gradient(colorgrad::preset::turbo())
    .with_bar_style(BarStyle::Braille) // Braille for high-res, Solid for blocks
    .with_color_mode(ColorMode::VerticalGradient)
    .with_max(1.0);

  f.render_widget(bar_graph, area);
}

/// Interpolate band values to fill the target width for smoother display
fn interpolate_bands(bands: &[f32], target_width: usize) -> Vec<f64> {
  if bands.is_empty() {
    return vec![0.0; target_width];
  }
  if bands.len() == 1 {
    return vec![bands[0] as f64; target_width];
  }

  let mut result = Vec::with_capacity(target_width);
  let scale = (bands.len() - 1) as f64 / (target_width - 1).max(1) as f64;

  for i in 0..target_width {
    let pos = i as f64 * scale;
    let idx = pos.floor() as usize;
    let frac = pos - idx as f64;

    let value = if idx + 1 < bands.len() {
      bands[idx] as f64 * (1.0 - frac) + bands[idx + 1] as f64 * frac
    } else {
      bands[idx.min(bands.len() - 1)] as f64
    };

    result.push(value);
  }

  result
}
