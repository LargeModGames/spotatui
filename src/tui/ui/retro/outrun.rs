//! Outrun: synthwave. A striped sunset sun whose cut-outs widen with the bass,
//! the spectrum as a neon mountain skyline in front of it, and a magenta
//! perspective grid scrolling toward the viewer.

use super::{lerp, Cells, Dots, Frame, State};

/// Points along the skyline.
pub const RIDGE_POINTS: usize = 48;

const SKY_TOP: (u8, u8, u8) = (22, 6, 40);
const SKY_BOTTOM: (u8, u8, u8) = (110, 24, 96);
const GROUND: (u8, u8, u8) = (10, 4, 22);
const MOUNTAIN: (u8, u8, u8) = (12, 4, 28);
const NEON: (u8, u8, u8) = (0, 240, 255);
const NEON_DIM: (u8, u8, u8) = (0, 150, 210);
const GRID: (u8, u8, u8) = (255, 40, 200);
const GRID_FAR: (u8, u8, u8) = (190, 30, 160);
const HORIZON: (u8, u8, u8) = (255, 90, 220);
/// Grid scroll speed: how many line spacings pass per second.
const SCROLL_PER_SEC: f32 = 0.55;

pub fn draw(cells: &mut Cells, frame: &Frame, state: &mut State, dt: f32) {
  let (w, h) = (cells.width(), cells.height());
  let horizon_row = (h * 17 + 15) / 30;
  for y in 0..h {
    let color = if y < horizon_row {
      lerp(SKY_TOP, SKY_BOTTOM, y as f32 / horizon_row as f32)
    } else {
      GROUND
    };
    cells.fill(0, y, w, 1, color);
  }
  if frame.view.is_playing {
    state.outrun_phase = (state.outrun_phase + SCROLL_PER_SEC * dt) % 1.0;
  }

  let mut dots = Dots::new(w, h);
  let (cw, ch) = (dots.width as f32, dots.height as f32);
  let hz = horizon_row as f32 * 4.0;

  let mono = frame.audio.mono();
  let bass = mono.iter().take(3).copied().fold(0.0f32, f32::max);

  // The sun: a disc with horizontal cut-outs in its lower half, thicker
  // toward the horizon, widening with the bass.
  let (sx, sy, sr) = (cw * 0.5, hz - 4.0, hz * 0.68);
  let mut y = (sy - sr).max(0.0);
  while y <= sy {
    let depth = (y - (sy - sr)) / sr;
    let mut cut = false;
    if depth > 0.45 {
      let gap = 1 + ((depth - 0.45) / 0.55 * 4.0 + bass * 2.0) as i32;
      cut = (sy as i32 - y as i32).rem_euclid(7) < gap;
    }
    if !cut {
      let half = (sr * sr - (y - sy) * (y - sy)).max(0.0).sqrt();
      let color = lerp((255, 236, 110), (255, 50, 150), depth);
      dots.rect(sx - half, y, sx + half, y + 1.0, color);
    }
    y += 1.0;
  }

  // The skyline: the spectrum as a ridge, everything below it erased so the
  // sun sets behind it, with a two-dot neon rim.
  let ridge = super::resample(&mono, RIDGE_POINTS);
  let ridge_height = hz * 0.44;
  let top_at = |x: f32| {
    let pos = x / (cw - 1.0).max(1.0) * (RIDGE_POINTS - 1) as f32;
    let i = (pos.floor() as usize).min(RIDGE_POINTS - 2);
    let f = pos - i as f32;
    let v = ridge[i] * (1.0 - f) + ridge[i + 1] * f;
    hz - 1.0 - v * ridge_height
  };
  for x in 0..dots.width {
    let top = top_at(x as f32);
    let mut y = top + 2.0;
    while y < hz {
      dots.clear(x as f32, y);
      y += 1.0;
    }
    dots.set(x as f32, top, NEON);
    dots.set(x as f32, top + 1.0, NEON_DIM);
  }

  // The grid: lines running to a vanishing point on the horizon (kept clear
  // of the horizon itself), the horizon line, and cross lines spaced like a
  // ground plane, scrolling toward the viewer.
  let vx = cw / 2.0;
  let (far, near) = (cw * 9.0 / 232.0, cw * 62.0 / 232.0);
  for i in -9..=9 {
    let (x_far, x_near) = (vx + i as f32 * far, vx + i as f32 * near);
    let start = hz + 7.0;
    let t = (start - hz) / (ch - 1.0 - hz);
    dots.line(x_far + (x_near - x_far) * t, start, x_near, ch - 1.0, GRID);
  }
  dots.rect(0.0, hz, cw, hz + 1.0, HORIZON);
  for k in 0..8 {
    let z = 1.25 * 1.65f32.powf(k as f32 - state.outrun_phase);
    if z < 1.0 {
      continue;
    }
    let y = hz + (ch - hz) / z;
    // Lines bunched against the horizon read as noise at dot resolution.
    if y - hz < 6.0 {
      break;
    }
    let color = if z < 3.0 { GRID } else { GRID_FAR };
    dots.rect(0.0, y, cw, y + 1.0, color);
  }
  cells.dots(&dots, 0, 0);

  // Mountains: their cells take a near-black background against the sky.
  for cx in 0..w {
    let top_row = (top_at(cx as f32 * 2.0) / 4.0) as i32 + 1;
    for row in top_row..horizon_row {
      cells.tint(cx, row, MOUNTAIN);
    }
  }

  cells.put(2, 1, "O U T R U N", NEON, None, true);
  let title = frame.view.title.as_deref().unwrap_or("").to_uppercase();
  let label = format!("♪ {title}");
  cells.put(
    w - label.chars().count() as i32 - 2,
    1,
    &label,
    (255, 120, 220),
    None,
    false,
  );
}
