//! Rack EQ: an 80s silver rack-unit spectrum analyzer. Left and right LED
//! ladders per band (green, amber, red), a peak LED that holds and falls,
//! screws, a model plate and knobs.

use super::{lerp, update_peaks, Cells, Frame, State};

/// Bands per channel.
pub const BANDS: usize = 10;
/// The analyzer's range (`infra/audio/analyzer.rs::build_cava`), for labels.
const FREQ_START: f32 = 50.0;
const FREQ_END: f32 = 10_000.0;

const PLATE: (u8, u8, u8) = (168, 170, 172);
const PLATE_EDGE: (u8, u8, u8) = (140, 142, 145);
const SCREW: (u8, u8, u8) = (90, 90, 92);
const WINDOW: (u8, u8, u8) = (24, 24, 26);
const LED_GREEN: (u8, u8, u8) = (40, 230, 70);
const LED_AMBER: (u8, u8, u8) = (250, 190, 30);
const LED_RED: (u8, u8, u8) = (250, 50, 40);
const LABEL: (u8, u8, u8) = (150, 150, 150);

/// The label for band `i`: its center frequency on the analyzer's log scale,
/// rounded to the nearest standard third-octave frequency.
pub fn band_label(i: usize) -> &'static str {
  const ISO: [(f32, &str); 23] = [
    (63.0, "63"),
    (80.0, "80"),
    (100.0, "100"),
    (125.0, "125"),
    (160.0, "160"),
    (200.0, "200"),
    (250.0, "250"),
    (315.0, "315"),
    (400.0, "400"),
    (500.0, "500"),
    (630.0, "630"),
    (800.0, "800"),
    (1000.0, "1k"),
    (1250.0, "1.2k"),
    (1600.0, "1.6k"),
    (2000.0, "2k"),
    (2500.0, "2.5k"),
    (3150.0, "3.1k"),
    (4000.0, "4k"),
    (5000.0, "5k"),
    (6300.0, "6.3k"),
    (8000.0, "8k"),
    (10000.0, "10k"),
  ];
  let center = FREQ_START * (FREQ_END / FREQ_START).powf((i as f32 + 0.5) / BANDS as f32);
  ISO
    .iter()
    .min_by(|a, b| {
      (a.0.ln() - center.ln())
        .abs()
        .total_cmp(&(b.0.ln() - center.ln()).abs())
    })
    .map_or("", |(_, label)| label)
}

/// Columns from one band to the next: each channel's ten 3-column ladders must
/// end before the other channel's label in the middle of the window, so the
/// spacing narrows from 5 to 3 on smaller terminals.
fn band_step(window_width: i32) -> i32 {
  // Left ladders start 3 in, and the last one is 3 wide; the right label sits
  // at the window's middle.
  ((window_width / 2 - 7) / (BANDS as i32 - 1)).clamp(3, 5)
}

pub fn draw(cells: &mut Cells, frame: &Frame, state: &mut State, dt: f32) {
  let (w, h) = (cells.width(), cells.height());
  cells.fill(0, 0, w, h, PLATE);
  cells.fill(0, 0, w, 1, PLATE_EDGE);
  cells.fill(0, h - 1, w, 1, PLATE_EDGE);
  for (x, y) in [(2, 0), (w - 3, 0), (2, h - 1), (w - 3, h - 1)] {
    cells.put(x, y, "◉", SCREW, Some(PLATE_EDGE), false);
  }
  cells.put(
    5,
    2,
    "GRAPHIC EQUALIZER  ·  SPECTRUM ANALYZER",
    (30, 30, 34),
    None,
    true,
  );
  cells.put(w - 26, 2, "MODEL SA-1200", (60, 60, 64), None, false);

  let (wx, wy, ww, wh) = (4, 4, w - 8, h - 10);
  cells.fill(wx, wy, ww, wh, WINDOW);

  let (left, right) = frame.audio.channels();
  let (left, right) = (
    super::resample(&left, BANDS),
    super::resample(&right, BANDS),
  );
  let mut both = left.clone();
  both.extend(&right);
  update_peaks(&mut state.rack_peaks, &both, frame.now, dt, 600, 0.6);

  let segments = (wh - 6).max(3);
  let step = band_step(ww);
  // Ladders are 3 wide, 2 on the tightest spacing, so a gap always shows.
  let bar = (step - 1).min(3);
  let (lit_glyph, peak_glyph) = ("▆".repeat(bar as usize), "▂".repeat(bar as usize));
  for (ch, (x0, label, values)) in [(wx + 3, "L", &left), (wx + ww / 2 + 2, "R", &right)]
    .into_iter()
    .enumerate()
  {
    cells.put(x0 - 2, wy + 1, label, LABEL, Some(WINDOW), true);
    // The frequency labels are up to 4 wide: on a tight spacing, skip any
    // that would touch the one before.
    let mut label_end = i32::MIN;
    for (b, &v) in values.iter().enumerate() {
      let x = x0 + b as i32 * step;
      let lit = (v.clamp(0.0, 1.0) * segments as f32).round() as i32;
      for s in 0..segments {
        let y = wy + wh - 3 - s;
        let f = s as f32 / segments as f32;
        let led = if f < 0.64 {
          LED_GREEN
        } else if f < 0.86 {
          LED_AMBER
        } else {
          LED_RED
        };
        let color = if s < lit {
          led
        } else {
          lerp(WINDOW, led, 0.18)
        };
        cells.put(x, y, &lit_glyph, color, Some(WINDOW), false);
      }
      let peak = state.rack_peaks.get(ch * BANDS + b).map_or(v, |p| p.value);
      let peak_seg =
        ((peak.clamp(0.0, 1.0) * segments as f32).round() as i32 + 1).min(segments - 1);
      if peak_seg >= lit {
        cells.put(
          x,
          wy + wh - 3 - peak_seg,
          &peak_glyph,
          (255, 255, 255),
          Some(WINDOW),
          false,
        );
      }
      let text = band_label(b);
      let text_x = x + (bar - text.len() as i32) / 2;
      if text_x > label_end {
        cells.put(text_x, wy + wh - 2, text, LABEL, Some(WINDOW), false);
        label_end = text_x + text.len() as i32;
      }
    }
  }

  for (i, name) in ["INPUT", "RANGE", "DECAY", "PEAK HOLD"]
    .into_iter()
    .enumerate()
  {
    let x = 8 + i as i32 * 26;
    cells.put(x, h - 4, "( ● )", (40, 40, 44), None, false);
    cells.put(x - 1, h - 3, name, (50, 50, 54), None, false);
  }
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use ratatui::{buffer::Buffer, layout::Rect};

  use super::super::tests::{sample_bands, sample_wave, screen};
  use super::super::{Audio, Cells, Frame, State};
  use super::*;
  use crate::tui::ui::head_unit::HeadUnitView;

  #[test]
  fn the_left_ladders_end_before_the_right_channel_from_80_columns_up() {
    for w in [80, 90, 100, 108, 116, 160] {
      let ww = w - 8;
      let step = band_step(ww);
      let left_end = 3 + (BANDS as i32 - 1) * step + 3;
      assert!(
        left_end <= ww / 2,
        "{w} columns: ladders run to {left_end}, the R label is at {}",
        ww / 2
      );
    }
    assert_eq!(band_step(200), 5);
  }

  fn label_row(w: u16) -> String {
    let bands = sample_bands(24);
    let wave = sample_wave();
    let view = HeadUnitView::default();
    let area = Rect::new(0, 0, w, 20);
    let mut buf = Buffer::empty(area);
    let frame = Frame {
      audio: Audio {
        bands: &bands,
        wave: &wave,
      },
      view: &view,
      now: Instant::now(),
    };
    let mut cells = Cells {
      buf: &mut buf,
      area,
    };
    draw(&mut cells, &frame, &mut State::default(), 0.0);
    // The window is rows 4..14 at this height; the labels sit on its
    // second-last row.
    screen(&buf).lines().nth(12).unwrap().to_string()
  }

  #[test]
  fn band_labels_stay_apart_and_match_in_both_channels() {
    let all: Vec<&str> = (0..BANDS).map(band_label).collect();
    for w in [80u16, 96, 116] {
      let row = label_row(w);
      let shown: Vec<&str> = row.split_whitespace().collect();
      let (left, right) = shown.split_at(shown.len() / 2);
      assert_eq!(left, right, "{w} columns: {row:?}");
      // Every word is a whole label, in order, starting with the lowest.
      assert_eq!(left.first(), Some(&all[0]), "{w} columns: {row:?}");
      let mut rest = all.iter();
      for word in left {
        assert!(
          rest.any(|l| l == word),
          "{w} columns: {word:?} is not a band label in order: {row:?}"
        );
      }
    }
    // Wide enough for the 5-column spacing, all ten show.
    assert_eq!(label_row(116).split_whitespace().count(), 2 * BANDS);
  }
}
