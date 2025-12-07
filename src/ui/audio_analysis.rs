use super::util;
use crate::app::App;
use ratatui::{
  layout::{Constraint, Direction, Layout},
  style::Style,
  text::{Line, Span},
  widgets::{BarChart, Block, Borders, Paragraph},
  Frame,
};

/// Frequency band labels (low to high frequency)
const BAND_LABELS: [&str; 12] = [
  "Sub", "Bass", "Low", "LMid", "Mid", "UMid", "High", "HiMd", "Pres", "Bril", "Air", "Ultra",
];

pub fn draw(f: &mut Frame<'_>, app: &App) {
  let margin = util::get_main_layout_margin(app);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(3), Constraint::Min(10)].as_ref())
    .margin(margin)
    .split(f.size());

  let white = Style::default().fg(app.user_config.theme.text);
  let gray = Style::default().fg(app.user_config.theme.inactive);
  let tick_rate = app.user_config.behavior.tick_rate_milliseconds;

  let info_block = Block::default()
    .title(Span::styled(
      "Audio Visualization",
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

  let width = (chunks[1].width as f32 / (1 + BAND_LABELS.len()) as f32).max(3.0);

  // Check if we have spectrum data from local audio capture
  if let Some(ref spectrum) = app.spectrum_data {
    // Info panel with status
    let status_text = if app.audio_capture_active {
      "🎵 Capturing audio"
    } else {
      "⏸ Paused"
    };

    let peak_text = format!("Peak: {:.0}%", spectrum.peak * 100.0);

    let texts = vec![Line::from(vec![
      Span::styled(status_text, Style::default().fg(app.user_config.theme.text)),
      Span::raw("  "),
      Span::styled(
        peak_text,
        Style::default().fg(app.user_config.theme.inactive),
      ),
    ])];

    let p = Paragraph::new(texts)
      .block(info_block)
      .style(Style::default().fg(app.user_config.theme.text));
    f.render_widget(p, chunks[0]);

    // Create bar chart data from spectrum bands
    let data: Vec<(&str, u64)> = spectrum
      .bands
      .iter()
      .enumerate()
      .map(|(index, &value)| {
        let label = BAND_LABELS.get(index).unwrap_or(&"?");
        // Scale value to u64 for display (0.0-1.0 -> 0-1000)
        // Cap at 800 so bars never hit the top (max is 1000)
        let bar_value = ((value * 1000.0) as u64).min(800);
        (*label, bar_value)
      })
      .collect();

    let spectrum_bar = BarChart::default()
      .block(bar_chart_block)
      .data(&data)
      .bar_width(width as u16)
      .max(1000) // Fixed max so bars are relative to this
      .bar_style(Style::default().fg(app.user_config.theme.analysis_bar))
      .value_style(
        Style::default()
          .fg(app.user_config.theme.analysis_bar_text)
          .bg(app.user_config.theme.analysis_bar),
      );
    f.render_widget(spectrum_bar, chunks[1]);
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
    f.render_widget(p, chunks[0]);

    // Empty bar chart
    let empty_p = Paragraph::new("Waiting for audio input...")
      .block(bar_chart_block)
      .style(Style::default().fg(app.user_config.theme.text));
    f.render_widget(empty_p, chunks[1]);
  }
}
