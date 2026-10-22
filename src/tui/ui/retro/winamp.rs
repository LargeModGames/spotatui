//! Winamp: the classic 2000s player look. Nineteen scanlined bars, green
//! through yellow to red by height, grey peak caps that hold and fall, an
//! oscilloscope strip of the live waveform, and the transport row.

use super::{clock, update_peaks, Cells, Dots, Frame, State};

/// Bars across, as in the classic skin.
pub const BARS: usize = 19;

const BLACK: (u8, u8, u8) = (0, 0, 0);
const TITLE_BAR: (u8, u8, u8) = (32, 34, 52);
const LCD: (u8, u8, u8) = (0, 230, 0);
const GREEN: (u8, u8, u8) = (0, 200, 0);
const YELLOW: (u8, u8, u8) = (210, 210, 0);
const RED: (u8, u8, u8) = (230, 40, 20);
const CAP: (u8, u8, u8) = (170, 170, 190);
const DIM: (u8, u8, u8) = (120, 120, 140);

pub fn draw(cells: &mut Cells, frame: &Frame, state: &mut State, dt: f32) {
  let (w, h) = (cells.width(), cells.height());
  cells.fill(0, 0, w, h, BLACK);
  let view = frame.view;

  // Title bar: the player's name and the scrolling-marquee line.
  cells.fill(0, 0, w, 1, TITLE_BAR);
  cells.put(1, 0, "WINAMP", (220, 200, 120), Some(TITLE_BAR), true);
  let title = view.title.as_deref().unwrap_or("spotatui");
  let length = view
    .duration_ms
    .map(|d| format!(" ({})", clock(u128::from(d))))
    .unwrap_or_default();
  cells.put(
    9,
    0,
    &format!("·  1. {title}{length}"),
    LCD,
    Some(TITLE_BAR),
    false,
  );
  cells.put(w - 10, 0, "▬ ▭ ✕", (150, 150, 170), Some(TITLE_BAR), false);

  let time = view.progress_ms.map(clock).unwrap_or_else(|| "0:00".into());
  cells.put(2, 2, &format!("{time:>5}"), LCD, None, true);

  // Bars, with peaks that hold, then fall.
  let bars = super::resample(&frame.audio.mono(), BARS);
  update_peaks(&mut state.winamp_peaks, &bars, frame.now, dt, 300, 0.7);
  let bar_rows = (h - 10).max(2);
  let mut dots = Dots::new(w - 4, bar_rows);
  let pitch = (dots.width as f32 - 8.0) / BARS as f32;
  let bar_w = (pitch * 0.73).max(1.0);
  let top_room = 3.0;
  for (i, &v) in bars.iter().enumerate() {
    let x = 4.0 + i as f32 * pitch;
    let span = dots.height as f32 - top_room;
    let top = dots.height as f32 - 1.0 - v.clamp(0.0, 1.0) * span;
    let mut y = top.round();
    while y < dots.height as f32 {
      // Scanlines: every other dot row dark, like the skin's bars.
      if (y as usize).is_multiple_of(2) {
        let f = 1.0 - y / dots.height as f32;
        let color = if f < 0.45 {
          GREEN
        } else if f < 0.75 {
          YELLOW
        } else {
          RED
        };
        dots.rect(x, y, x + bar_w, y + 1.0, color);
      }
      y += 1.0;
    }
    let peak = state.winamp_peaks.get(i).map_or(v, |p| p.value);
    let cap_y = (dots.height as f32 - 1.0 - peak.clamp(0.0, 1.0) * span - 2.0).max(0.0);
    dots.rect(x, cap_y, x + bar_w, cap_y + 1.0, CAP);
  }
  cells.dots(&dots, 2, 4);

  // Oscilloscope strip.
  let osc_y = h - 6;
  cells.put(2, osc_y + 1, "OSC", (90, 90, 110), None, false);
  let mut osc = Dots::new(w - 8, 4);
  let wave = super::resample(frame.audio.wave, osc.width.max(2));
  let mid = osc.height as f32 / 2.0;
  for x in 0..osc.width.saturating_sub(1) {
    let (a, b) = (wave[x], wave[x + 1]);
    // Rows grow downward: positive samples go above the middle.
    osc.line(
      x as f32,
      mid - a * mid * 0.9,
      x as f32 + 1.0,
      mid - b * mid * 0.9,
      LCD,
    );
  }
  cells.dots(&osc, 6, osc_y);

  // Transport row: play, shuffle and repeat lit when on.
  let on = |lit: bool| if lit { LCD } else { DIM };
  let y = h - 1;
  cells.put(2, y, "|◀  ", DIM, None, false);
  cells.put(6, y, "▶", on(view.is_playing), None, false);
  cells.put(
    9,
    y,
    "‖",
    on(!view.is_playing && view.progress_ms.is_some()),
    None,
    false,
  );
  cells.put(12, y, "■  ▶|   [EQ] [PL]   ", DIM, None, false);
  cells.put(33, y, "shuffle", on(view.shuffle), None, false);
  cells.put(42, y, "repeat", on(view.repeat), None, false);
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use ratatui::{buffer::Buffer, layout::Rect, style::Color};

  use super::super::tests::sample_bands;
  use super::super::{Audio, Cells, Frame, State};
  use super::*;
  use crate::tui::ui::head_unit::HeadUnitView;

  #[test]
  fn the_scope_strip_draws_a_positive_wave_above_its_middle() {
    let (w, h) = (80u16, 20u16);
    let area = Rect::new(0, 0, w, h);
    let mut buf = Buffer::empty(area);
    let bands = sample_bands(24);
    let view = HeadUnitView::default();
    let frame = Frame {
      audio: Audio {
        bands: &bands,
        wave: &[0.8; 480],
      },
      view: &view,
      now: Instant::now(),
    };
    draw(
      &mut Cells {
        buf: &mut buf,
        area,
      },
      &frame,
      &mut State::default(),
      0.0,
    );
    // The strip is four rows from `h - 6`; its middle is between the second
    // and third.
    let osc_y = h - 6;
    let lcd = Color::Rgb(LCD.0, LCD.1, LCD.2);
    let rows: Vec<u16> = (osc_y..osc_y + 4)
      .filter(|&y| (6..w).any(|x| buf[(x, y)].fg == lcd))
      .collect();
    assert!(!rows.is_empty(), "the trace is drawn");
    assert!(
      rows.iter().all(|&y| y < osc_y + 2),
      "trace rows {rows:?} should be in the strip's top half (from {osc_y})"
    );
  }
}
