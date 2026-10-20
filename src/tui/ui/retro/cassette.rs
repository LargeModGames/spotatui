//! Cassette Deck: a tape window with the reels turning while it plays (the
//! tape moving from the left reel to the right as the track goes on), LED
//! level bars per channel and an orange tape counter.

use std::f32::consts::PI;

use super::{lerp, Cells, Dots, Frame, State};

const BODY: (u8, u8, u8) = (34, 30, 28);
const SHELL: (u8, u8, u8) = (210, 200, 180);
const LABEL_STRIP: (u8, u8, u8) = (240, 235, 220);
const WINDOW: (u8, u8, u8) = (30, 26, 24);
const TAPE: (u8, u8, u8) = (90, 60, 40);
const HUB: (u8, u8, u8) = (235, 230, 215);
const TEXT: (u8, u8, u8) = (200, 180, 140);
const MUTED: (u8, u8, u8) = (150, 130, 100);
/// Reel speed while playing, degrees per second.
const DEGREES_PER_SEC: f32 = 150.0;

pub fn draw(cells: &mut Cells, frame: &Frame, state: &mut State, dt: f32) {
  let (w, h) = (cells.width(), cells.height());
  let view = frame.view;
  cells.fill(0, 0, w, h, BODY);
  cells.put(
    3,
    1,
    "AUTO REVERSE  ·  DOLBY B NR  ·  CrO₂",
    TEXT,
    None,
    false,
  );
  cells.put(w - 18, 1, "TAPE COUNTER", MUTED, None, false);
  let count = view.progress_ms.map_or(0, |ms| (ms / 1000) % 10_000);
  let digits: String = format!("{count:04}")
    .chars()
    .flat_map(|c| [c, ' '])
    .collect();
  cells.put(
    w - 16,
    2,
    digits.trim_end(),
    (255, 120, 40),
    Some((10, 8, 6)),
    true,
  );

  // The cassette, kept above the meter rows (they start at `h - 8`).
  let ww = (w * 69 / 100).max(20);
  let wh = shell_height(h);
  let (wx, wy) = ((w - ww) / 2, 4);
  cells.fill(wx, wy, ww, wh, SHELL);
  cells.fill(wx + 2, wy + 1, ww - 4, 3, LABEL_STRIP);
  let title = view.title.as_deref().unwrap_or("").to_uppercase();
  cells.put(
    wx + 4,
    wy + 2,
    &format!("C-90   {title}   ·   SIDE A"),
    (60, 50, 40),
    Some(LABEL_STRIP),
    true,
  );
  let (rx0, ry0, rw, rh) = (wx + 10, wy + 5, ww - 20, (wh - 7).max(3));
  cells.fill(rx0, ry0, rw, rh, WINDOW);

  let progress = match (view.progress_ms, view.duration_ms) {
    (Some(p), Some(d)) if d > 0 => (p as f32 / d as f32).clamp(0.0, 1.0),
    _ => 0.3,
  };
  let mut dots = Dots::new(rw, rh);
  let (cw, ch) = (dots.width as f32, dots.height as f32);
  let max_r = (ch / 2.0 - 1.0).max(4.0);
  let hub_r = (max_r * 0.46).max(3.0);
  // Tape moves from the left (supply) reel to the right (take-up) reel.
  let packs = [
    hub_r + 1.0 + (max_r - hub_r - 1.0) * (1.0 - progress),
    hub_r + 1.0 + (max_r - hub_r - 1.0) * progress,
  ];
  if view.is_playing {
    advance_reels(&mut state.reel_angles, packs, max_r, dt);
  }
  for (k, (cx, pack)) in [(cw * 0.22, packs[0]), (cw * 0.78, packs[1])]
    .into_iter()
    .enumerate()
  {
    let cy = ch / 2.0;
    disc(&mut dots, cx, cy, pack, TAPE);
    disc_clear(&mut dots, cx, cy, hub_r + 0.5);
    ring(&mut dots, cx, cy, hub_r, HUB);
    // Six teeth inside the hub, turning with the reel.
    let spin = state.reel_angles[k] + k as f32 * 30.0;
    for t in 0..6 {
      let a = (t as f32 * 60.0 + spin) * PI / 180.0;
      dots.line(
        cx + hub_r * 0.55 * a.cos(),
        cy + hub_r * 0.55 * a.sin(),
        cx + hub_r * a.cos(),
        cy + hub_r * a.sin(),
        HUB,
      );
    }
  }
  dots.line(cw * 0.22, ch - 1.0, cw * 0.78, ch - 1.0, TAPE);
  cells.dots(&dots, rx0, ry0);

  // LED level bars.
  let (l, r) = frame.audio.levels();
  let leds = ((w - 16) / 2).clamp(8, 40);
  for (i, (label, level)) in [("L", l), ("R", r)].into_iter().enumerate() {
    let y = h - 8 + i as i32 * 2;
    cells.put(6, y, label, TEXT, None, true);
    for k in 0..leds {
      let f = k as f32 / leds as f32;
      let led = if f < 0.65 {
        (60, 220, 90)
      } else if f < 0.825 {
        (250, 190, 40)
      } else {
        (250, 60, 40)
      };
      let color = if f < level {
        led
      } else {
        lerp(BODY, led, 0.18)
      };
      cells.put(8 + k * 2, y, "▐█", color, None, false);
    }
  }
  let scale = [
    (0.0, "-30"),
    (0.25, "-20"),
    (0.5, "-10"),
    (0.65, "-5"),
    (0.825, "0"),
    (0.925, "+3"),
    (1.0, "+6"),
  ];
  for (f, text) in scale {
    let x = 8 + ((f * (leds - 1) as f32) as i32) * 2 - 1;
    cells.put(x, h - 4, text, MUTED, None, false);
  }
  let transport = if view.is_playing {
    "◀◀   ▶   ■   ▶▶   ▲"
  } else {
    "◀◀   ‖   ■   ▶▶   ▲"
  };
  cells.put(6, h - 2, transport, TEXT, None, true);
}

/// Rows the cassette shell takes: half the area, but ending above the meter
/// rows (from `h - 8`) and never under 6.
fn shell_height(h: i32) -> i32 {
  (h / 2).min(h - 13).max(6)
}

/// Turns each reel by its own speed (the smaller pack turns faster, as on a
/// real deck). Each angle wraps on its own, so the teeth never jump.
fn advance_reels(angles: &mut [f32; 2], packs: [f32; 2], max_r: f32, dt: f32) {
  for (angle, pack) in angles.iter_mut().zip(packs) {
    let rate = DEGREES_PER_SEC * (max_r / pack.max(1.0));
    *angle = (*angle + rate * dt) % 360.0;
  }
}

fn disc(dots: &mut Dots, cx: f32, cy: f32, r: f32, color: (u8, u8, u8)) {
  let mut y = (cy - r).floor();
  while y <= cy + r {
    let half = (r * r - (y - cy) * (y - cy)).max(0.0).sqrt();
    dots.rect(cx - half, y, cx + half + 1.0, y + 1.0, color);
    y += 1.0;
  }
}

fn disc_clear(dots: &mut Dots, cx: f32, cy: f32, r: f32) {
  let mut y = (cy - r).floor();
  while y <= cy + r {
    let half = (r * r - (y - cy) * (y - cy)).max(0.0).sqrt();
    let mut x = (cx - half).round();
    while x <= (cx + half).round() {
      dots.clear(x, y);
      x += 1.0;
    }
    y += 1.0;
  }
}

fn ring(dots: &mut Dots, cx: f32, cy: f32, r: f32, color: (u8, u8, u8)) {
  for i in 0..180 {
    let a = i as f32 * 2.0 * PI / 180.0;
    dots.set(cx + r * a.cos(), cy + r * a.sin(), color);
  }
}

#[cfg(test)]
mod tests {
  use std::time::Instant;

  use ratatui::{buffer::Buffer, layout::Rect, style::Color};

  use super::super::tests::{sample_bands, sample_wave};
  use super::super::{Audio, Cells, Frame, State};
  use super::*;
  use crate::tui::ui::head_unit::HeadUnitView;

  #[test]
  fn the_shell_stays_above_the_meters_in_short_areas() {
    let bands = sample_bands(24);
    let wave = sample_wave();
    let view = HeadUnitView {
      title: Some("Side A".into()),
      is_playing: true,
      ..HeadUnitView::default()
    };
    for h in [19u16, 20, 22, 24, 30] {
      let area = Rect::new(0, 0, 80, h);
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
      let shell = Color::Rgb(SHELL.0, SHELL.1, SHELL.2);
      for y in (h - 8)..h {
        for x in 0..80 {
          assert_ne!(
            buf[(x, y)].bg,
            shell,
            "the shell reaches meter row {y} at height {h}"
          );
        }
      }
    }
  }

  #[test]
  fn the_reels_turn_smoothly_across_the_wrap() {
    // Unequal packs, so a shared angle scaled per reel would jump on the wrap.
    let (packs, max_r, dt) = ([5.3, 9.1], 11.0, 1.0 / 60.0);
    let mut angles = [0.0f32; 2];
    for _ in 0..(60 * 10) {
      let before = angles;
      advance_reels(&mut angles, packs, max_r, dt);
      for k in 0..2 {
        // The six teeth repeat every 60 degrees: compare within that turn.
        let step = (angles[k] - before[k]).rem_euclid(60.0);
        let expected = DEGREES_PER_SEC * (max_r / packs[k]) * dt;
        assert!((step - expected).abs() < 1e-3, "reel {k} jumped by {step}");
      }
    }
  }
}
