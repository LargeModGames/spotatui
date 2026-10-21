//! Phosphor Scope: a green CRT oscilloscope. A dotted graticule, the live
//! waveform, and its last few sweeps fading behind it like P31 phosphor.

use super::{lerp, Cells, Dots, Frame, State};

const BG: (u8, u8, u8) = (6, 14, 8);
const GRID: (u8, u8, u8) = (20, 60, 30);
const TEXT: (u8, u8, u8) = (90, 200, 110);
const TRACE: (u8, u8, u8) = (120, 255, 140);
/// Older sweeps kept, and how bright each is (newest last).
const TRAILS: [f32; 4] = [0.18, 0.32, 0.55, 1.0];
/// Seconds of audio the trace spans (the analyzer's wave window).
const WAVE_SECONDS: f32 = 0.05;

pub fn draw(cells: &mut Cells, frame: &Frame, state: &mut State) {
  let (w, h) = (cells.width(), cells.height());
  cells.fill(0, 0, w, h, BG);
  cells.put(
    2,
    0,
    "CH1  2V/div   TIME 5ms/div   TRIG ⟋ AUTO",
    TEXT,
    None,
    false,
  );
  cells.put(w - 18, 0, "PHOSPHOR  P31", (60, 140, 80), None, false);

  // A new sweep only when the analyzer sends a new wave: a redraw with the
  // same audio (mouse movement, a resize) would otherwise fill every trail
  // with one wave and the fade would vanish.
  let wave = frame.audio.wave;
  if state.scope_trails.back().map(Vec::as_slice) != Some(wave) {
    state.scope_trails.push_back(wave.to_vec());
    while state.scope_trails.len() > TRAILS.len() {
      state.scope_trails.pop_front();
    }
  }

  let (x0, y0, sw, sh) = (4, 2, w - 8, h - 5);
  let mut dots = Dots::new(sw, sh);
  let (cw, ch) = (dots.width as f32, dots.height as f32);
  // Graticule: 10 x 8 divisions, dotted, with ticks along the center line.
  for i in 0..=10 {
    let x = i as f32 * (cw - 1.0) / 10.0;
    let mut y = 0.0;
    while y < ch {
      dots.set(x, y, GRID);
      y += 3.0;
    }
  }
  for j in 0..=8 {
    let y = j as f32 * (ch - 1.0) / 8.0;
    let mut x = 0.0;
    while x < cw {
      dots.set(x, y, GRID);
      x += 3.0;
    }
  }
  for k in 0..=50 {
    let x = k as f32 * (cw - 1.0) / 50.0;
    dots.set(x, ch / 2.0 - 1.0, GRID);
    dots.set(x, ch / 2.0 + 1.0, GRID);
  }
  // Sweeps, oldest and faintest first. Screen rows grow downward, so a
  // positive sample is drawn above the center line.
  let first = TRAILS.len() - state.scope_trails.len();
  for (sweep, brightness) in state.scope_trails.iter().zip(&TRAILS[first..]) {
    let color = lerp(BG, TRACE, *brightness);
    let points = super::resample(sweep, dots.width.max(2));
    for x in 0..dots.width.saturating_sub(1) {
      dots.line(
        x as f32,
        ch / 2.0 - points[x] * ch * 0.42,
        x as f32 + 1.0,
        ch / 2.0 - points[x + 1] * ch * 0.42,
        color,
      );
    }
  }
  cells.dots(&dots, x0, y0);

  let bezel = (60, 110, 70);
  cells.put(x0 - 1, y0 - 1, "╭", bezel, None, false);
  cells.put(x0 + sw, y0 - 1, "╮", bezel, None, false);
  cells.put(x0 - 1, y0 + sh, "╰", bezel, None, false);
  cells.put(x0 + sw, y0 + sh, "╯", bezel, None, false);

  // Readout: peak-to-peak and a zero-crossing estimate of the frequency.
  let (lo, hi) = wave
    .iter()
    .fold((0.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
  let crossings = wave
    .windows(2)
    .filter(|p| (p[0] < 0.0) != (p[1] < 0.0))
    .count();
  let freq = crossings as f32 / 2.0 / WAVE_SECONDS;
  cells.put(
    4,
    h - 2,
    &format!("Vpp {:.2} V    f {:.0} Hz", (hi - lo) * 2.0, freq),
    TEXT,
    None,
    false,
  );
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use ratatui::{buffer::Buffer, layout::Rect, style::Color};

  use super::super::tests::sample_bands;
  use super::super::{Audio, Cells, Frame, State};
  use super::*;
  use crate::tui::ui::head_unit::HeadUnitView;

  fn draw_wave(buf: &mut Buffer, state: &mut State, wave: &[f32]) {
    let bands = sample_bands(24);
    let view = HeadUnitView::default();
    let frame = Frame {
      audio: Audio {
        bands: &bands,
        wave,
      },
      view: &view,
      now: Instant::now(),
    };
    let area = buf.area;
    draw(&mut Cells { buf, area }, &frame, state);
  }

  #[test]
  fn a_redraw_with_the_same_wave_adds_no_sweep() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
    let mut state = State::default();
    let first = vec![0.3; 480];
    for _ in 0..6 {
      draw_wave(&mut buf, &mut state, &first);
    }
    assert_eq!(state.scope_trails.len(), 1, "six redraws of one wave");
    draw_wave(&mut buf, &mut state, &[-0.3; 480]);
    assert_eq!(state.scope_trails.len(), 2, "a new wave is a new sweep");
  }

  #[test]
  fn a_positive_wave_is_drawn_above_the_center_line() {
    let (w, h) = (80u16, 20u16);
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    draw_wave(&mut buf, &mut State::default(), &[0.8; 480]);
    // The screen spans rows 2..2 + (h - 5); its center line is halfway.
    let center = 2.0 + f32::from(h - 5) / 2.0;
    let trace = Color::Rgb(TRACE.0, TRACE.1, TRACE.2);
    let rows: Vec<u16> = (0..h)
      .filter(|&y| (0..w).any(|x| buf[(x, y)].fg == trace))
      .collect();
    assert!(!rows.is_empty(), "the trace is drawn");
    assert!(
      rows.iter().all(|&y| f32::from(y) + 1.0 <= center),
      "trace rows {rows:?} should sit above the center at {center}"
    );
  }
}
