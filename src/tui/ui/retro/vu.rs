//! Analog VU: two backlit amber 1970s hi-fi meters, left and right. The
//! needles swing with each channel's level, eased like a real meter's
//! ballistics; the PEAK lamp lights while either needle is past 0 VU.

use std::f32::consts::PI;

use super::{lerp, Cells, Dots, Frame, State};

const CASE: (u8, u8, u8) = (14, 10, 6);
const FACE: (u8, u8, u8) = (232, 200, 130);
const FACE_EDGE: (u8, u8, u8) = (190, 150, 80);
const INK: (u8, u8, u8) = (40, 28, 18);
const NEEDLE: (u8, u8, u8) = (20, 14, 10);
const RED: (u8, u8, u8) = (186, 40, 30);
const LABEL: (u8, u8, u8) = (150, 120, 80);
const TITLE: (u8, u8, u8) = (210, 170, 100);

/// Where 0 VU sits along the scale, and the scale's marks.
const ZERO: f32 = 0.72;
const MARKS: [(f32, &str); 8] = [
  (0.0, "-20"),
  (0.22, "-10"),
  (0.36, "-7"),
  (0.48, "-5"),
  (0.60, "-3"),
  (0.72, "0"),
  (0.86, "+2"),
  (1.0, "+3"),
];
/// The needle sweep, degrees from the right-hand horizontal.
const SWEEP: (f32, f32) = (125.0, 55.0);
/// How quickly a needle follows its level: rises a bit faster than it falls,
/// like the damped movement of a real meter.
const RISE_PER_SEC: f32 = 9.0;
const FALL_PER_SEC: f32 = 5.0;

pub fn draw(cells: &mut Cells, frame: &Frame, state: &mut State, dt: f32) {
  let (w, h) = (cells.width(), cells.height());
  cells.fill(0, 0, w, h, CASE);

  let (left, right) = frame.audio.levels();
  for (needle, target) in state.vu_needles.iter_mut().zip([left, right]) {
    let rate = if target > *needle {
      RISE_PER_SEC
    } else {
      FALL_PER_SEC
    };
    *needle += (target - *needle) * (rate * dt).min(1.0);
  }
  let over = state.vu_needles.iter().any(|&n| n > ZERO);

  cells.put(2, 0, "STEREO VU", TITLE, None, true);
  cells.put(w - 22, 0, "PEAK ", LABEL, None, false);
  cells.put(
    w - 17,
    0,
    "●",
    if over { RED } else { lerp(CASE, RED, 0.3) },
    None,
    false,
  );
  cells.put(w - 15, 0, "POWER ", LABEL, None, false);
  cells.put(w - 9, 0, "●", (240, 170, 60), None, false);

  let face_w = (w - 9) / 2;
  let face_h = h - 6;
  for (i, label) in ["L", "R"].into_iter().enumerate() {
    let x0 = 3 + i as i32 * (face_w + 4);
    face(cells, x0, 2, face_w, face_h, state.vu_needles[i], label);
  }
  cells.put(
    2,
    h - 2,
    &format!(
      "L {:>+3.0} VU    R {:>+3.0} VU",
      vu_db(state.vu_needles[0]),
      vu_db(state.vu_needles[1])
    ),
    (120, 95, 60),
    None,
    false,
  );
}

/// The dB a needle position reads, between the scale's marks.
pub fn vu_db(f: f32) -> f32 {
  const DB: [f32; 8] = [-20.0, -10.0, -7.0, -5.0, -3.0, 0.0, 2.0, 3.0];
  let f = f.clamp(0.0, 1.0);
  for i in 0..MARKS.len() - 1 {
    let (a, b) = (MARKS[i].0, MARKS[i + 1].0);
    if f <= b {
      let t = (f - a) / (b - a);
      return DB[i] + (DB[i + 1] - DB[i]) * t;
    }
  }
  3.0
}

fn face(cells: &mut Cells, x0: i32, y0: i32, w: i32, h: i32, level: f32, label: &str) {
  if w < 12 || h < 6 {
    return;
  }
  cells.fill(x0, y0, w, h, FACE);
  cells.fill(x0, y0, w, 1, FACE_EDGE);
  cells.fill(x0, y0 + h - 1, w, 1, FACE_EDGE);

  let mut dots = Dots::new(w, h);
  let (cw, ch) = (dots.width as f32, dots.height as f32);
  let (a0, a1) = (SWEEP.0 * PI / 180.0, SWEEP.1 * PI / 180.0);
  // A flat arc like a real VU meter: the pivot well below the face, the
  // sweep filling 80% of its width, its crown 40% of the way down.
  let r = 0.40 * cw / a1.cos();
  let (px, py) = (cw / 2.0, ch * 0.40 + r);
  let at = |a: f32, radius: f32| (px + radius * a.cos(), py - radius * a.sin());

  for i in 0..900 {
    let f = i as f32 / 899.0;
    let a = a0 + (a1 - a0) * f;
    let (x, y) = at(a, r);
    dots.set(x, y, INK);
    if f >= ZERO {
      for dr in [2.0, 3.0, 4.0] {
        let (x, y) = at(a, r + dr);
        dots.set(x, y, RED);
      }
    }
  }
  for (f, text) in MARKS {
    let a = a0 + (a1 - a0) * f;
    let color = if f < ZERO { INK } else { RED };
    let (xa, ya) = at(a, r + 1.0);
    let (xb, yb) = at(a, r + 5.0);
    dots.line(xa, ya, xb, yb, color);
    let (lx, ly) = at(a, r + 13.0);
    let col = x0 + (lx / 2.0).round() as i32 - text.len() as i32 / 2;
    let row = y0 + (ly / 4.0).round() as i32;
    cells.put(col, row, text, color, None, true);
  }
  let a = a0 + (a1 - a0) * level.clamp(0.0, 1.0);
  let (tx, ty) = at(a, r + 6.0);
  dots.line(px, py, tx, ty, NEEDLE);
  cells.dots(&dots, x0, y0);

  cells.put(x0 + w / 2 - 1, y0 + h - 9, "VU", INK, None, true);
  cells.put(x0 + 3, y0 + h - 3, label, INK, None, true);
  cells.put(
    x0 + w - 12,
    y0 + h - 3,
    "dB · 1kHz",
    (110, 80, 50),
    None,
    false,
  );
}
