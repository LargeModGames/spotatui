//! Retro visualizer styles: Analog VU, Winamp, Phosphor Scope, Outrun, Rack EQ
//! and Cassette Deck. Each was designed as a mockup at real terminal
//! resolution first and is a port of that mockup, sized from the area it's
//! given.
//!
//! The shared pieces live here: a cell writer that clips to the area, a
//! Braille dot canvas (2x4 dots per cell, one color per cell: the most common
//! among its dots), color math, the audio views every style reads, and the
//! little bit of animation state (needle ballistics, falling peaks, phosphor
//! trails, the grid's scroll) kept between frames in a thread-local, since
//! draw functions only see `&App`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;

use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
};

use crate::tui::ui::head_unit::HeadUnitView;

pub mod cassette;
pub mod outrun;
pub mod rack_eq;
pub mod scope;
pub mod vu;
pub mod winamp;

pub type Rgb = (u8, u8, u8);

pub fn rgb(c: Rgb) -> Color {
  Color::Rgb(c.0, c.1, c.2)
}

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
  let t = t.clamp(0.0, 1.0);
  let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
  (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Linear resample onto `target` points.
pub fn resample(values: &[f32], target: usize) -> Vec<f32> {
  if target == 0 {
    return Vec::new();
  }
  if values.is_empty() {
    return vec![0.0; target];
  }
  if values.len() == 1 || target == 1 {
    return vec![values[0]; target];
  }
  let scale = (values.len() - 1) as f32 / (target - 1) as f32;
  (0..target)
    .map(|i| {
      let pos = i as f32 * scale;
      let idx = pos.floor() as usize;
      let frac = pos - idx as f32;
      match values.get(idx + 1) {
        Some(next) => values[idx] * (1.0 - frac) + next * frac,
        None => values[idx.min(values.len() - 1)],
      }
    })
    .collect()
}

/// What the analyzer gives a style: cava's mirrored stereo bands and the
/// newest stretch of the waveform.
#[derive(Debug, Clone, Copy)]
pub struct Audio<'a> {
  /// Left channel reversed on the left half, right channel forward on the
  /// right half (cava's display order).
  pub bands: &'a [f32],
  /// Mono mix, -1 to 1, oldest first.
  pub wave: &'a [f32],
}

impl Audio<'_> {
  /// Each channel's bands, bass to treble.
  pub fn channels(&self) -> (Vec<f32>, Vec<f32>) {
    let half = self.bands.len() / 2;
    if half == 0 {
      return (self.bands.to_vec(), self.bands.to_vec());
    }
    let left = (0..half).map(|i| self.bands[half - 1 - i]).collect();
    let right = self.bands[half..half * 2].to_vec();
    (left, right)
  }

  /// One bass-to-treble row, the louder channel per band.
  pub fn mono(&self) -> Vec<f32> {
    let (l, r) = self.channels();
    l.iter().zip(&r).map(|(a, b)| a.max(*b)).collect()
  }

  /// Each channel's loudness, 0-1: the RMS of its bands, scaled so music
  /// sits in the upper half.
  pub fn levels(&self) -> (f32, f32) {
    let rms = |v: &[f32]| {
      if v.is_empty() {
        0.0
      } else {
        ((v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt() * 1.4).clamp(0.0, 1.0)
      }
    };
    let (l, r) = self.channels();
    (rms(&l), rms(&r))
  }
}

/// Writes cells relative to an area, clipping anything outside it.
pub struct Cells<'a> {
  pub buf: &'a mut Buffer,
  pub area: Rect,
}

impl Cells<'_> {
  pub fn width(&self) -> i32 {
    i32::from(self.area.width)
  }

  pub fn height(&self) -> i32 {
    i32::from(self.area.height)
  }

  fn at(&mut self, x: i32, y: i32) -> Option<&mut ratatui::buffer::Cell> {
    if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
      return None;
    }
    let (x, y) = (self.area.x + x as u16, self.area.y + y as u16);
    Some(&mut self.buf[(x, y)])
  }

  /// Text from `(x, y)`, one character per cell; `bg: None` keeps the
  /// background already there.
  pub fn put(&mut self, x: i32, y: i32, text: &str, fg: Rgb, bg: Option<Rgb>, bold: bool) {
    for (i, ch) in text.chars().enumerate() {
      if let Some(cell) = self.at(x + i as i32, y) {
        cell.set_char(ch).set_fg(rgb(fg));
        if let Some(bg) = bg {
          cell.set_bg(rgb(bg));
        }
        let style = if bold {
          Style::default().add_modifier(Modifier::BOLD)
        } else {
          Style::default().remove_modifier(Modifier::BOLD)
        };
        cell.set_style(style);
      }
    }
  }

  pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, bg: Rgb) {
    for yy in y..y + h {
      for xx in x..x + w {
        if let Some(cell) = self.at(xx, yy) {
          cell.set_char(' ').set_fg(rgb(bg)).set_bg(rgb(bg));
        }
      }
    }
  }

  /// Background only, keeping what's drawn in the cell.
  pub fn tint(&mut self, x: i32, y: i32, bg: Rgb) {
    if let Some(cell) = self.at(x, y) {
      cell.set_bg(rgb(bg));
    }
  }

  /// Draw a dot canvas with its top-left cell at `(x0, y0)`; cells with no
  /// dots are left as they are.
  pub fn dots(&mut self, canvas: &Dots, x0: i32, y0: i32) {
    const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    for cy in 0..canvas.height / 4 {
      for cx in 0..canvas.width / 2 {
        let mut bits = 0;
        let mut counts: Vec<(Rgb, u8)> = Vec::with_capacity(2);
        for (dx, column) in BITS.iter().enumerate() {
          for (dy, bit) in column.iter().enumerate() {
            if let Some(color) = canvas.get(cx * 2 + dx, cy * 4 + dy) {
              bits |= bit;
              match counts.iter_mut().find(|(c, _)| *c == color) {
                Some((_, n)) => *n += 1,
                None => counts.push((color, 1)),
              }
            }
          }
        }
        // Most dots wins; ties go to the brighter color.
        let Some((color, _)) = counts
          .iter()
          .max_by_key(|(c, n)| (*n, u32::from(c.0) + u32::from(c.1) + u32::from(c.2)))
        else {
          continue;
        };
        let glyph = char::from_u32(0x2800 + bits).unwrap_or(' ');
        if let Some(cell) = self.at(x0 + cx as i32, y0 + cy as i32) {
          cell.set_char(glyph).set_fg(rgb(*color));
        }
      }
    }
  }
}

/// A Braille dot canvas: `cols * 2` dots across, `rows * 4` down.
pub struct Dots {
  pub width: usize,
  pub height: usize,
  dots: Vec<Option<Rgb>>,
}

impl Dots {
  pub fn new(cols: i32, rows: i32) -> Self {
    let width = cols.max(0) as usize * 2;
    let height = rows.max(0) as usize * 4;
    Dots {
      width,
      height,
      dots: vec![None; width * height],
    }
  }

  pub fn get(&self, x: usize, y: usize) -> Option<Rgb> {
    self.dots[y * self.width + x]
  }

  pub fn set(&mut self, x: f32, y: f32, color: Rgb) {
    let (x, y) = (x.round(), y.round());
    if x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height {
      self.dots[y as usize * self.width + x as usize] = Some(color);
    }
  }

  pub fn clear(&mut self, x: f32, y: f32) {
    let (x, y) = (x.round(), y.round());
    if x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height {
      self.dots[y as usize * self.width + x as usize] = None;
    }
  }

  pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.0) as i32;
    for i in 0..=steps {
      let t = i as f32 / steps as f32;
      self.set(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, color);
    }
  }

  pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb) {
    let mut y = y0.round();
    while y < y1.round() {
      let mut x = x0.round();
      while x < x1.round() {
        self.set(x, y, color);
        x += 1.0;
      }
      y += 1.0;
    }
  }
}

/// A falling peak marker: holds, then drops at a steady rate.
#[derive(Debug, Clone, Copy)]
pub struct Peak {
  pub value: f32,
  held_at: Instant,
}

/// Update `peaks` toward `values`: a new high resets the hold, otherwise after
/// `hold_ms` the marker falls `fall_per_sec` of full scale per second.
pub fn update_peaks(
  peaks: &mut Vec<Peak>,
  values: &[f32],
  now: Instant,
  dt: f32,
  hold_ms: u128,
  fall_per_sec: f32,
) {
  if peaks.len() != values.len() {
    *peaks = values
      .iter()
      .map(|&value| Peak {
        value,
        held_at: now,
      })
      .collect();
  }
  for (peak, &v) in peaks.iter_mut().zip(values) {
    if v >= peak.value {
      *peak = Peak {
        value: v,
        held_at: now,
      };
    } else if now.saturating_duration_since(peak.held_at).as_millis() > hold_ms {
      peak.value = (peak.value - fall_per_sec * dt).max(v);
    }
  }
}

/// Animation state between frames, one set per style.
#[derive(Debug, Default)]
pub struct State {
  last: Option<Instant>,
  pub vu_needles: [f32; 2],
  pub winamp_peaks: Vec<Peak>,
  pub rack_peaks: Vec<Peak>,
  pub scope_trails: VecDeque<Vec<f32>>,
  pub outrun_phase: f32,
  /// Each reel's own angle, so the two turn at their own speeds without a
  /// jump when one wraps.
  pub reel_angles: [f32; 2],
}

impl State {
  /// Seconds since the last frame (0 on the first, capped so a stall doesn't
  /// jump the animation).
  pub fn tick(&mut self, now: Instant) -> f32 {
    let dt = self
      .last
      .map(|t| now.saturating_duration_since(t).as_secs_f32())
      .unwrap_or(0.0)
      .min(0.25);
    self.last = Some(now);
    dt
  }
}

thread_local! {
  pub static STATE: RefCell<State> = RefCell::new(State::default());
}

/// `m:ss` (or `h:mm:ss`) of a position.
pub fn clock(ms: u128) -> String {
  let s = ms / 1000;
  let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
  if h > 0 {
    format!("{h}:{m:02}:{s:02}")
  } else {
    format!("{m}:{s:02}")
  }
}

/// Everything a style draws from.
pub struct Frame<'a> {
  pub audio: Audio<'a>,
  pub view: &'a HeadUnitView,
  pub now: Instant,
}

/// Which retro style to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retro {
  AnalogVu,
  Winamp,
  PhosphorScope,
  Outrun,
  RackEq,
  Cassette,
}

impl Retro {
  /// Bars the analyzer should produce for this style (both channels).
  pub fn bar_count(self) -> usize {
    match self {
      Retro::AnalogVu | Retro::Cassette => 24,
      Retro::Winamp => winamp::BARS * 2,
      Retro::PhosphorScope => 16,
      Retro::Outrun => outrun::RIDGE_POINTS * 2,
      Retro::RackEq => rack_eq::BANDS * 2,
    }
  }
}

pub fn render(style: Retro, buf: &mut Buffer, area: Rect, frame: &Frame<'_>) {
  if area.width < 20 || area.height < 8 {
    return;
  }
  STATE.with(|state| {
    let mut state = state.borrow_mut();
    let dt = state.tick(frame.now);
    let mut cells = Cells { buf, area };
    match style {
      Retro::AnalogVu => vu::draw(&mut cells, frame, &mut state, dt),
      Retro::Winamp => winamp::draw(&mut cells, frame, &mut state, dt),
      Retro::PhosphorScope => scope::draw(&mut cells, frame, &mut state),
      Retro::Outrun => outrun::draw(&mut cells, frame, &mut state, dt),
      Retro::RackEq => rack_eq::draw(&mut cells, frame, &mut state, dt),
      Retro::Cassette => cassette::draw(&mut cells, frame, &mut state, dt),
    }
  });
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  /// A plausible music frame: strong lows, a mid bump, a little wave.
  pub fn sample_bands(per_channel: usize) -> Vec<f32> {
    let row: Vec<f32> = (0..per_channel)
      .map(|i| {
        let x = i as f32 / per_channel.max(2) as f32;
        (0.95 * (-((x - 0.08).powi(2)) / 0.02).exp()
          + 0.6 * (-((x - 0.42).powi(2)) / 0.02).exp()
          + 0.3 * (1.0 - x))
          .clamp(0.04, 1.0)
      })
      .collect();
    let mut bands: Vec<f32> = row.iter().rev().copied().collect();
    bands.extend(row);
    bands
  }

  pub fn sample_wave() -> Vec<f32> {
    (0..480)
      .map(|i| 0.55 * (i as f32 * 0.11).sin() + 0.2 * (i as f32 * 0.47).sin())
      .collect()
  }

  pub fn screen(buf: &Buffer) -> String {
    let area = buf.area;
    (area.y..area.bottom())
      .map(|y| {
        (area.x..area.right())
          .map(|x| buf[(x, y)].symbol().to_string())
          .collect::<String>()
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  #[test]
  fn channels_unmirror_cavas_stereo_layout() {
    let audio = Audio {
      bands: &[0.3, 0.2, 0.1, 0.4, 0.5, 0.6],
      wave: &[],
    };
    let (l, r) = audio.channels();
    assert_eq!(l, vec![0.1, 0.2, 0.3]);
    assert_eq!(r, vec![0.4, 0.5, 0.6]);
    assert_eq!(audio.mono(), vec![0.4, 0.5, 0.6]);
  }

  #[test]
  fn peaks_hold_then_fall() {
    let t0 = Instant::now();
    let mut peaks = Vec::new();
    update_peaks(&mut peaks, &[1.0], t0, 0.0, 500, 1.0);
    update_peaks(
      &mut peaks,
      &[0.0],
      t0 + std::time::Duration::from_millis(100),
      0.1,
      500,
      1.0,
    );
    assert_eq!(peaks[0].value, 1.0);
    update_peaks(
      &mut peaks,
      &[0.0],
      t0 + std::time::Duration::from_millis(900),
      0.2,
      500,
      1.0,
    );
    assert!(peaks[0].value < 1.0);
  }

  #[test]
  fn the_dot_canvas_packs_braille_with_the_majority_color() {
    let mut dots = Dots::new(1, 1);
    dots.set(0.0, 0.0, (9, 9, 9));
    dots.set(1.0, 3.0, (9, 9, 9));
    dots.set(0.0, 1.0, (1, 1, 1));
    let area = Rect::new(0, 0, 1, 1);
    let mut buf = Buffer::empty(area);
    Cells {
      buf: &mut buf,
      area,
    }
    .dots(&dots, 0, 0);
    assert_eq!(buf[(0, 0)].symbol(), "\u{2883}");
    assert_eq!(buf[(0, 0)].fg, Color::Rgb(9, 9, 9));
  }

  #[test]
  fn every_style_draws_at_normal_and_tiny_sizes_without_panicking() {
    let bands = sample_bands(24);
    let wave = sample_wave();
    let view = HeadUnitView {
      title: Some("Get Your Brits Out".into()),
      progress_ms: Some(203_000),
      duration_ms: Some(400_000),
      is_playing: true,
      shuffle: true,
      ..HeadUnitView::default()
    };
    for style in [
      Retro::AnalogVu,
      Retro::Winamp,
      Retro::PhosphorScope,
      Retro::Outrun,
      Retro::RackEq,
      Retro::Cassette,
    ] {
      for (w, h) in [
        (116u16, 30u16),
        (80, 20),
        (40, 10),
        (20, 8),
        (19, 7),
        (300, 80),
      ] {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let frame = Frame {
          audio: Audio {
            bands: &bands,
            wave: &wave,
          },
          view: &view,
          now: Instant::now(),
        };
        render(style, &mut buf, area, &frame);
        if (w, h) == (116, 30) {
          let text = screen(&buf);
          assert!(
            text.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)) || style == Retro::RackEq,
            "{style:?} draws its dot art:\n{text}"
          );
        }
      }
    }
  }
}
