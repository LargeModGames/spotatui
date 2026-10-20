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
  for (ch, (x0, label, values)) in [(wx + 3, "L", &left), (wx + ww / 2 + 2, "R", &right)]
    .into_iter()
    .enumerate()
  {
    cells.put(x0 - 2, wy + 1, label, LABEL, Some(WINDOW), true);
    for (b, &v) in values.iter().enumerate() {
      let x = x0 + b as i32 * 5;
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
        cells.put(x, y, "▆▆▆", color, Some(WINDOW), false);
      }
      let peak = state.rack_peaks.get(ch * BANDS + b).map_or(v, |p| p.value);
      let peak_seg =
        ((peak.clamp(0.0, 1.0) * segments as f32).round() as i32 + 1).min(segments - 1);
      if peak_seg >= lit {
        cells.put(
          x,
          wy + wh - 3 - peak_seg,
          "▂▂▂",
          (255, 255, 255),
          Some(WINDOW),
          false,
        );
      }
      let text = band_label(b);
      cells.put(
        x + (3 - text.len() as i32) / 2,
        wy + wh - 2,
        text,
        LABEL,
        Some(WINDOW),
        false,
      );
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
