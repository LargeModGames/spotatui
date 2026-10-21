//! The Head Unit visualizer: the spectrum stage of an early-2000s Alpine head
//! unit's VFD (the MDA-W892J), drawn in Braille dots at 2x4 per cell, the same
//! fine dots as the Bar Graph style, so its angles can be drawn true.
//!
//! The stage, as on the MDA-W892J's faceplate:
//! - a floor in real perspective: twelve strips lying on it, one per band,
//!   all converging toward one vanishing point up and off to the left, so
//!   every strip leans and the right ones lean most. Each is lit from the
//!   front as far back as its band's level reaches: green, then lavender,
//!   then blue, the rest glowing faintly. Strips keep a fixed width along
//!   their length and take one color per terminal cell, so edges and color
//!   bands stay clean.
//! - a back wall of six panels, one over each pair of strips, biggest on the
//!   left and shrinking to the right;
//! - a right wall of three panels whose bottoms drop away toward the viewer.
//!
//! Around it, in the theme's colors: a level meter, frequency marks, mode
//! chips and a segment-digit clock with its unlit "88:88" ghost behind it.
//!
//! The stage's own colors are the faceplate's, sampled from the photo (see
//! `vfd_palette`); the chrome around it follows the theme. Named theme colors
//! are approximated with their xterm RGB values.
//!
//! The level meter's easing needs memory across frames, but draw functions
//! only see `&App`; the renderer keeps that presentation-only state in a
//! thread-local.

use std::cell::RefCell;
use std::time::Instant;

use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::retro::{lerp, resample, rgb, Cells, Dots, Rgb};
use crate::core::user_config::Theme;

/// Strips on the floor, as on the faceplate. Even, since each back-wall panel
/// stands over a pair.
pub const BANDS: usize = 12;
/// Level meter on the left: two columns wide plus a one-column gap.
const METER_WIDTH: u16 = 2;

/// The analyzer's frequency range (`infra/audio/analyzer.rs::build_cava`),
/// used to place the frequency marks under the strips.
const FREQ_START: f32 = 50.0;
const FREQ_END: f32 = 10_000.0;
const FREQ_MARKS: [(f32, &str); 6] = [
  (63.0, "63"),
  (160.0, "160"),
  (400.0, "400"),
  (1_000.0, "1k"),
  (2_500.0, "2.5k"),
  (6_300.0, "6.3k"),
];

/// Level meter segment: the lower three quarters of a cell, so stacked cells
/// leave a sliver of gap between segments.
const SEGMENT: &str = "▆";

/// What the readout strip shows besides the stage. Built by the caller from
/// the playback snapshot so this module stays a pure function of plain values.
#[derive(Debug, Default, Clone)]
pub struct HeadUnitView {
  pub title: Option<String>,
  pub progress_ms: Option<u128>,
  pub duration_ms: Option<u32>,
  pub is_playing: bool,
  pub shuffle: bool,
  pub repeat: bool,
  pub live: bool,
}

/// Bands per channel the analyzer should produce. The analyzer mirrors the
/// two channels, so the runner asks for twice this.
pub fn band_count() -> usize {
  BANDS
}

/// Fold cava's mirrored stereo layout (left channel reversed on the left half,
/// right channel forward on the right) into one bass-to-treble row, taking the
/// louder channel per band so either side lights the strip.
pub fn fold_stereo(bands: &[f32]) -> Vec<f32> {
  let half = bands.len() / 2;
  if half == 0 {
    return bands.to_vec();
  }
  (0..half)
    .map(|i| bands[half - 1 - i].max(bands[half + i]))
    .collect()
}

/// The draw interval the meter's easing was tuned at: the visualizer's 16 ms
/// animation tick.
const FRAME_SECONDS: f32 = 1.0 / 60.0;

#[derive(Debug, Default)]
struct HeadUnitState {
  /// The latest row, one value per strip.
  live: Vec<f32>,
  level: f32,
  last: Option<Instant>,
}

impl HeadUnitState {
  fn update(&mut self, bands: &[f32], now: Instant) {
    self.live = resample(bands, BANDS);
    // RMS of the row, eased so the meter swings like a needle, not a strobe.
    let rms = if self.live.is_empty() {
      0.0
    } else {
      (self.live.iter().map(|v| v * v).sum::<f32>() / self.live.len() as f32).sqrt()
    };
    let target = (rms * 1.4).clamp(0.0, 1.0);
    // The easing runs on time, not on draws, so extra redraws (mouse
    // movement, a resize) don't speed the needle up. The first frame counts
    // as one tick; a stall is capped so the needle doesn't jump.
    let dt = self
      .last
      .map_or(FRAME_SECONDS, |t| {
        now.saturating_duration_since(t).as_secs_f32()
      })
      .min(0.25);
    self.last = Some(now);
    let per_frame: f32 = if target > self.level { 0.6 } else { 0.15 };
    let ease = 1.0 - (1.0 - per_frame).powf(dt / FRAME_SECONDS);
    self.level += (target - self.level) * ease;
  }
}

thread_local! {
  static STATE: RefCell<HeadUnitState> = RefCell::new(HeadUnitState::default());
}

/// RGB for any ratatui color, xterm values for the named ones.
fn to_rgb(color: Color, fallback: Rgb) -> Rgb {
  match color {
    Color::Rgb(r, g, b) => (r, g, b),
    Color::Black => (0, 0, 0),
    Color::Red => (205, 0, 0),
    Color::Green => (0, 205, 0),
    Color::Yellow => (205, 205, 0),
    Color::Blue => (0, 0, 238),
    Color::Magenta => (205, 0, 205),
    Color::Cyan => (0, 205, 205),
    Color::Gray => (229, 229, 229),
    Color::DarkGray => (127, 127, 127),
    Color::LightRed => (255, 0, 0),
    Color::LightGreen => (0, 255, 0),
    Color::LightYellow => (255, 255, 0),
    Color::LightBlue => (92, 92, 255),
    Color::LightMagenta => (255, 0, 255),
    Color::LightCyan => (0, 255, 255),
    Color::White => (255, 255, 255),
    Color::Indexed(_) | Color::Reset => fallback,
  }
}

/// The VFD's colors, all derived from the theme.
#[derive(Debug, Clone, Copy)]
struct Vfd {
  low: Rgb,
  mid: Rgb,
  high: Rgb,
  hot: Rgb,
  cap: Rgb,
  bg: Rgb,
  label: Rgb,
}

impl Vfd {
  fn from_theme(theme: &Theme) -> Self {
    let bg = to_rgb(theme.background.into(), (16, 16, 16));
    let fg = to_rgb(theme.text.into(), (220, 220, 220));
    Vfd {
      low: to_rgb(theme.analysis_bar.into(), (0, 200, 200)),
      mid: to_rgb(theme.hovered.into(), (180, 0, 180)),
      high: to_rgb(theme.hint.into(), (200, 200, 0)),
      hot: to_rgb(theme.error_text.into(), (255, 100, 100)),
      cap: fg,
      bg,
      label: to_rgb(theme.inactive.into(), (128, 128, 128)),
    }
  }

  /// Bar color at height fraction `t` (0 = floor, 1 = top of the column).
  fn gradient(&self, t: f32) -> Rgb {
    if t < 0.5 {
      lerp(self.low, self.mid, t * 2.0)
    } else {
      lerp(self.mid, self.high, (t - 0.5) * 2.0)
    }
  }

  /// `color` faded `amount` of the way into the background.
  fn dim(&self, color: Rgb, amount: f32) -> Rgb {
    lerp(color, self.bg, amount)
  }

  /// An unlit segment: the faint glass-behind-the-phosphor look.
  fn ghost(&self, color: Rgb) -> Rgb {
    self.dim(color, 0.9)
  }
}

fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, symbol: &str, fg: Rgb) {
  if x >= area.x && x < area.right() && y >= area.y && y < area.bottom() {
    buf[(x, y)].set_symbol(symbol).set_fg(rgb(fg));
  }
}

fn put_str(buf: &mut Buffer, area: Rect, x: u16, y: u16, text: &str, style: Style) {
  if y < area.y || y >= area.bottom() || x >= area.right() {
    return;
  }
  let width = usize::from(area.right() - x);
  buf.set_stringn(x, y, text, width, style);
}

/// Render the Head Unit visualizer into `area` (the block's inner rect).
/// `bands` is the analyzer's mirrored stereo output.
pub fn render(
  buf: &mut Buffer,
  area: Rect,
  bands: &[f32],
  theme: &Theme,
  view: &HeadUnitView,
  now: Instant,
) {
  if area.width < 12 || area.height < 4 {
    return;
  }
  let row = fold_stereo(bands);
  if row.is_empty() {
    return;
  }
  STATE.with(|state| {
    let mut state = state.borrow_mut();
    state.update(&row, now);
    draw(buf, area, theme, view, &state);
  });
}

fn draw(buf: &mut Buffer, area: Rect, theme: &Theme, view: &HeadUnitView, state: &HeadUnitState) {
  let vfd = Vfd::from_theme(theme);

  // Chrome comes and goes with the room available, the stage last to go.
  let show_header = area.height >= 12;
  let show_readout = area.height >= 15 && area.width >= 44;
  let show_labels = area.height >= 8;
  let show_meter = area.width >= 40;

  let mut top = area.y;
  let mut bottom = area.bottom();
  if show_header {
    draw_header(buf, Rect::new(area.x, top, area.width, 1), view, &vfd);
    top += 2;
  }
  if show_readout {
    bottom -= 3;
    draw_readout(
      buf,
      Rect::new(area.x, bottom, area.width, 3),
      view,
      state.level,
      &vfd,
    );
    bottom -= 1;
  }
  let labels_y = if show_labels {
    bottom -= 1;
    Some(bottom)
  } else {
    None
  };

  let mut main = Rect::new(area.x, top, area.width, bottom.saturating_sub(top));
  if main.height < 2 {
    return;
  }
  if show_meter {
    draw_meter(
      buf,
      Rect::new(main.x, main.y, METER_WIDTH, main.height),
      state.level,
      &vfd,
    );
    main.x += METER_WIDTH + 1;
    main.width -= METER_WIDTH + 1;
  }

  let mut canvas = Dots::new(i32::from(main.width), i32::from(main.height));
  let stage = Stage::new(&canvas);
  stage.draw(&mut canvas, &state.live);
  let mut cells = Cells { buf, area: main };
  let (w, h) = (cells.width(), cells.height());
  cells.fill(0, 0, w, h, vfd_palette::GLASS);
  cells.dots(&canvas, 0, 0);

  if let Some(y) = labels_y {
    stage.draw_labels(buf, Rect::new(area.x, y, area.width, 1), main.x, &vfd);
  }
}

type Point = (f32, f32);

/// The faceplate's own VFD colors, sampled from the MDA-W892J photo. The
/// stage is meant to look like that display, so these don't follow the theme.
mod vfd_palette {
  use super::Rgb;
  /// The black glass behind the display.
  pub const GLASS: Rgb = (6, 8, 12);
  /// Strip bands, front to back.
  pub const GREEN: Rgb = (58, 196, 104);
  pub const LAVENDER: Rgb = (186, 156, 232);
  pub const BLUE: Rgb = (52, 104, 226);
  /// Wall panels: deep blue at the top, sky blue at the bottom.
  pub const WALL_TOP: Rgb = (18, 70, 220);
  pub const WALL_BOTTOM: Rgb = (70, 146, 232);
}

/// The floor's corners as fractions of the stage, in the order front-left,
/// front-right, back-right, back-left. Mapped through a perspective
/// transform, so strips converge as in a real view.
const FLOOR: [Point; 4] = [(0.10, 0.995), (0.95, 0.80), (0.80, 0.40), (0.04, 0.40)];
/// Share of each band's lane its strip fills; the rest is dark floor.
const STRIP_FILL: f32 = 0.55;
/// Where a strip's colors change, as a share of its length from the front,
/// and how brightly its unlit part glows.
const GREEN_UNTIL: f32 = 0.46;
const LAVENDER_UNTIL: f32 = 0.70;
const UNLIT: f32 = 0.20;
/// Back wall: the leftmost panel's top, the rightmost's, and the level
/// bottom they all share.
const BACK_TOP_LEFT: f32 = 0.02;
const BACK_TOP_RIGHT: f32 = 0.11;
const BACK_BOTTOM: f32 = 0.31;
/// Right wall: from `x` to `x`, tops level, bottoms dropping along a slant
/// toward the viewer; equal-width panels.
const SIDE_LEFT: f32 = 0.835;
const SIDE_RIGHT: f32 = 0.995;
const SIDE_TOP: f32 = 0.03;
const SIDE_BOTTOM: (f32, f32) = (0.34, 0.74);
const SIDE_PANELS: usize = 3;
const SIDE_FILL: f32 = 0.78;

/// A perspective transform from the unit square onto a quad (Heckbert's
/// square-to-quad mapping): `(u, v)` with u across and v from the quad's first
/// edge to its far edge.
#[derive(Debug, Clone, Copy)]
struct Projection {
  m: [[f32; 3]; 3],
}

impl Projection {
  /// Corners for (0,0), (1,0), (1,1), (0,1).
  fn new([p0, p1, p2, p3]: [Point; 4]) -> Self {
    let (dx1, dx2, dx3) = (p1.0 - p2.0, p3.0 - p2.0, p0.0 - p1.0 + p2.0 - p3.0);
    let (dy1, dy2, dy3) = (p1.1 - p2.1, p3.1 - p2.1, p0.1 - p1.1 + p2.1 - p3.1);
    let (g, h) = if dx3.abs() < 1e-9 && dy3.abs() < 1e-9 {
      (0.0, 0.0)
    } else {
      let det = dx1 * dy2 - dx2 * dy1;
      ((dx3 * dy2 - dx2 * dy3) / det, (dx1 * dy3 - dx3 * dy1) / det)
    };
    Projection {
      m: [
        [p1.0 - p0.0 + g * p1.0, p3.0 - p0.0 + h * p3.0, p0.0],
        [p1.1 - p0.1 + g * p1.1, p3.1 - p0.1 + h * p3.1, p0.1],
        [g, h, 1.0],
      ],
    }
  }

  fn apply(&self, u: f32, v: f32) -> Point {
    let m = &self.m;
    let w = m[2][0] * u + m[2][1] * v + m[2][2];
    (
      (m[0][0] * u + m[0][1] * v + m[0][2]) / w,
      (m[1][0] * u + m[1][1] * v + m[1][2]) / w,
    )
  }

  /// Screen point back to `(u, v)`.
  fn invert(&self, (x, y): Point) -> Point {
    let m = &self.m;
    // Adjugate of the 3x3 matrix; the scale cancels in the division.
    let a = [
      [
        m[1][1] * m[2][2] - m[1][2] * m[2][1],
        m[0][2] * m[2][1] - m[0][1] * m[2][2],
        m[0][1] * m[1][2] - m[0][2] * m[1][1],
      ],
      [
        m[1][2] * m[2][0] - m[1][0] * m[2][2],
        m[0][0] * m[2][2] - m[0][2] * m[2][0],
        m[0][2] * m[1][0] - m[0][0] * m[1][2],
      ],
      [
        m[1][0] * m[2][1] - m[1][1] * m[2][0],
        m[0][1] * m[2][0] - m[0][0] * m[2][1],
        m[0][0] * m[1][1] - m[0][1] * m[1][0],
      ],
    ];
    let w = a[2][0] * x + a[2][1] * y + a[2][2];
    (
      (a[0][0] * x + a[0][1] * y + a[0][2]) / w,
      (a[1][0] * x + a[1][1] * y + a[1][2]) / w,
    )
  }
}

/// The stage, in canvas dots.
#[derive(Debug, Clone, Copy)]
struct Stage {
  floor: Projection,
  width: f32,
  height: f32,
}

impl Stage {
  fn new(canvas: &Dots) -> Self {
    let (w, h) = (
      (canvas.width as f32 - 1.0).max(1.0),
      (canvas.height as f32 - 1.0).max(1.0),
    );
    Stage {
      floor: Projection::new(FLOOR.map(|(x, y)| (x * w, y * h))),
      width: w,
      height: h,
    }
  }

  /// Strip `band`'s lane across the floor, `(u0, u1)`.
  fn lane(band: usize) -> (f32, f32) {
    let pad = (1.0 - STRIP_FILL) / 2.0;
    let n = BANDS as f32;
    ((band as f32 + pad) / n, (band as f32 + 1.0 - pad) / n)
  }

  /// Whether a dot lies on the floor (gaps between strips included), so the
  /// walls never draw into it.
  fn on_floor(&self, x: usize, y: usize) -> bool {
    let (u, v) = self.floor.invert((x as f32 + 0.5, y as f32 + 0.5));
    (-0.02..1.02).contains(&u) && (-0.05..1.03).contains(&v)
  }

  fn strip_color(v: f32, level: f32) -> Rgb {
    use vfd_palette::{BLUE, GLASS, GREEN, LAVENDER};
    let band = if v < GREEN_UNTIL {
      GREEN
    } else if v < LAVENDER_UNTIL {
      LAVENDER
    } else {
      BLUE
    };
    if v <= level {
      band
    } else {
      lerp(GLASS, band, UNLIT)
    }
  }

  fn draw(&self, canvas: &mut Dots, live: &[f32]) {
    for (band, &level) in live.iter().enumerate() {
      self.draw_strip(canvas, band, level.clamp(0.0, 1.0));
    }
    self.draw_back_wall(canvas);
    self.draw_side_wall(canvas);
  }

  /// One strip: a fixed width in dots (its width at mid-depth), centered on
  /// its perspective center line from the front edge to the far edge, with
  /// flat ends. Both edges step together, so they stay clean, and it still
  /// leans toward the vanishing point. Its color is decided once per terminal
  /// cell row, at the center line, so each cell is one color.
  fn draw_strip(&self, canvas: &mut Dots, band: usize, level: f32) {
    let (u0, u1) = Self::lane(band);
    let uc = (u0 + u1) / 2.0;
    let (front, back) = (self.floor.apply(uc, 0.0), self.floor.apply(uc, 1.0));
    let width = (self.floor.apply(u1, 0.5).0 - self.floor.apply(u0, 0.5).0)
      .round()
      .max(2.0) as i32;
    let (y_front, y_back) = (front.1.floor() as i32, back.1.ceil() as i32);
    let depth = (front.1 - back.1).max(1e-3);
    for y in y_back.max(0)..=y_front.min(canvas.height as i32 - 1) {
      let t = (front.1 - y as f32) / depth;
      let cx = front.0 + (back.0 - front.0) * t;
      let x0 = (cx - width as f32 / 2.0).round() as i32;
      // The color of this dot row's terminal cell row, read at its middle.
      let mid = ((y / 4) * 4 + 2).clamp(y_back, y_front);
      let v = (front.1 - mid as f32) / depth;
      let color = Self::strip_color(v, level);
      for x in x0.max(0)..(x0 + width).min(canvas.width as i32) {
        canvas.set_dot(x as usize, y as usize, color);
      }
    }
  }

  /// The back wall: a panel over each pair of strips, lined up with the
  /// pair's far ends; clean rectangles, biggest on the left, shrinking to the
  /// right; deep blue at the top to sky blue at the bottom.
  fn draw_back_wall(&self, canvas: &mut Dots) {
    let pairs = BANDS / 2;
    let bottom = (BACK_BOTTOM * self.height).round() as usize;
    for k in 0..pairs {
      let (ua, _) = Self::lane(2 * k);
      let (_, ub) = Self::lane(2 * k + 1);
      let (xa, xb) = (self.floor.apply(ua, 1.0).0, self.floor.apply(ub, 1.0).0);
      let t = k as f32 / (pairs - 1).max(1) as f32;
      let top =
        ((BACK_TOP_LEFT + (BACK_TOP_RIGHT - BACK_TOP_LEFT) * t) * self.height).round() as usize;
      for y in top..=bottom.min(canvas.height - 1) {
        let shade = (bottom - y) as f32 / (bottom - top).max(1) as f32;
        let color = lerp(vfd_palette::WALL_BOTTOM, vfd_palette::WALL_TOP, shade);
        for x in xa.round().max(0.0) as usize..(xb.round().max(0.0) as usize).min(canvas.width) {
          if canvas.get(x, y).is_none() && !self.on_floor(x, y) {
            canvas.set_dot(x, y, color);
          }
        }
      }
    }
  }

  /// The right wall: equal-width panels, tops level, bottoms dropping along
  /// a slant toward the viewer.
  fn draw_side_wall(&self, canvas: &mut Dots) {
    for y in 0..canvas.height {
      for x in 0..canvas.width {
        let fx = (x as f32 + 0.5) / self.width;
        let fy = (y as f32 + 0.5) / self.height;
        if !(SIDE_LEFT..SIDE_RIGHT).contains(&fx) || canvas.get(x, y).is_some() {
          continue;
        }
        let t = (fx - SIDE_LEFT) / (SIDE_RIGHT - SIDE_LEFT);
        let bottom = SIDE_BOTTOM.0 + (SIDE_BOTTOM.1 - SIDE_BOTTOM.0) * t;
        if !(SIDE_TOP..=bottom).contains(&fy) {
          continue;
        }
        let panel = ((t * SIDE_PANELS as f32) as usize).min(SIDE_PANELS - 1);
        let within = t * SIDE_PANELS as f32 - panel as f32;
        if (within - 0.5).abs() >= SIDE_FILL / 2.0 || self.on_floor(x, y) {
          continue;
        }
        let shade = (bottom - fy) / (bottom - SIDE_TOP).max(1e-3);
        canvas.set_dot(
          x,
          y,
          lerp(vfd_palette::WALL_BOTTOM, vfd_palette::WALL_TOP, shade),
        );
      }
    }
  }

  /// Frequency marks under the strips' front ends, on the analyzer's log scale.
  fn draw_labels(&self, buf: &mut Buffer, line: Rect, origin_x: u16, vfd: &Vfd) {
    let span = (FREQ_END / FREQ_START).ln();
    let style = Style::default().fg(rgb(vfd.label));
    let mut free_from = line.x;
    for (freq, text) in FREQ_MARKS {
      let pos = (freq / FREQ_START).ln() / span;
      let band = (pos * (BANDS - 1) as f32).round() as usize;
      let (px, _) = self.floor.apply((band as f32 + 0.5) / BANDS as f32, 0.0);
      let center = origin_x + (px / 2.0) as u16;
      let x = center.saturating_sub(text.len() as u16 / 2).max(line.x);
      if x < free_from || x + text.len() as u16 > line.right() {
        continue;
      }
      put_str(buf, line, x, line.y, text, style);
      free_from = x + text.len() as u16 + 1;
    }
  }
}

fn chip(buf: &mut Buffer, area: Rect, x: u16, text: &str, lit: bool, vfd: &Vfd) -> u16 {
  let label = format!(" {text} ");
  let style = if lit {
    Style::default()
      .fg(rgb(vfd.bg))
      .bg(rgb(vfd.high))
      .add_modifier(Modifier::BOLD)
  } else {
    Style::default().fg(rgb(vfd.ghost(vfd.high)))
  };
  put_str(buf, area, x, area.y, &label, style);
  label.len() as u16
}

/// The longest start of `text` that fits in `cells` terminal cells.
fn fit_width(text: &str, cells: usize) -> String {
  let mut used = 0;
  text
    .chars()
    .take_while(|c| {
      used += c.width().unwrap_or(0);
      used <= cells
    })
    .collect()
}

fn draw_header(buf: &mut Buffer, area: Rect, view: &HeadUnitView, vfd: &Vfd) {
  let dim = Style::default().fg(rgb(vfd.label));
  let left = "DSP HEAD UNIT";
  put_str(buf, area, area.x, area.y, left, dim);

  let badge_width = 5;
  let right = area.right().saturating_sub(badge_width);
  chip(buf, area, right, "DHE", true, vfd);

  // The track title, wide-spaced like the faceplate's lettering.
  if let Some(title) = view.title.as_deref().filter(|t| !t.is_empty()) {
    let room = usize::from(
      right
        .saturating_sub(area.x + left.len() as u16 + 2)
        .saturating_sub(2),
    );
    let spaced: String = title
      .to_uppercase()
      .chars()
      .flat_map(|c| [c, ' '])
      .collect::<String>()
      .trim_end()
      .to_string();
    // Measured in cells: a CJK character takes two.
    let upper = title.to_uppercase();
    let text = if spaced.width() <= room {
      spaced
    } else {
      fit_width(&upper, room)
    };
    let width = text.width() as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let x = x.max(area.x + left.len() as u16 + 2);
    // Stop short of the chip even if centering pushed the title right.
    let clip = Rect {
      width: right.saturating_sub(1).saturating_sub(area.x),
      ..area
    };
    put_str(
      buf,
      clip,
      x,
      area.y,
      &text,
      Style::default()
        .fg(rgb(vfd.mid))
        .add_modifier(Modifier::BOLD),
    );
  }
}

/// Vertical level meter, segments from the floor, the top fifth in the
/// theme's error color like a clip light.
fn draw_meter(buf: &mut Buffer, area: Rect, level: f32, vfd: &Vfd) {
  let height = area.height;
  let lit = (level.clamp(0.0, 1.0) * f32::from(height)).round() as u16;
  for seg in 0..height {
    let t = f32::from(seg) / f32::from(height.max(2) - 1);
    let base = if t >= 0.8 {
      vfd.hot
    } else {
      vfd.gradient(t * 0.5)
    };
    let color = if seg < lit { base } else { vfd.ghost(base) };
    for dx in 0..area.width {
      put(
        buf,
        area,
        area.x + dx,
        area.bottom() - 1 - seg,
        SEGMENT,
        color,
      );
    }
  }
}

/// Seven-segment digits three cells tall, drawn with heavy box glyphs.
const DIGITS: [[&str; 3]; 10] = [
  ["┏━┓", "┃ ┃", "┗━┛"],
  ["  ╻", "  ┃", "  ╹"],
  ["╺━┓", "┏━┛", "┗━╸"],
  ["╺━┓", " ━┫", "╺━┛"],
  ["╻ ╻", "┗━┫", "  ╹"],
  ["┏━╸", "┗━┓", "╺━┛"],
  ["┏━╸", "┣━┓", "┗━┛"],
  ["╺━┓", "  ┃", "  ╹"],
  ["┏━┓", "┣━┫", "┗━┛"],
  ["┏━┓", "┗━┫", "╺━┛"],
];
const COLON: [&str; 3] = ["▪", " ", "▪"];

/// `mm:ss` (or `hh:mm:ss` past an hour) of the playback position.
pub fn clock_text(progress_ms: u128) -> String {
  let secs = progress_ms / 1000;
  let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
  if h > 0 {
    format!("{h}:{m:02}:{s:02}")
  } else {
    format!("{m:02}:{s:02}")
  }
}

/// Draw `text` (digits and colons) in segment digits; unlit cells show the
/// "8" ghost. Returns the width used.
fn draw_digits(buf: &mut Buffer, area: Rect, x: u16, text: &str, lit: Rgb, vfd: &Vfd) -> u16 {
  let ghost = vfd.ghost(lit);
  let mut cx = x;
  for ch in text.chars() {
    let (glyph, width): (&[&str; 3], u16) = match ch {
      ':' => (&COLON, 1),
      d if d.is_ascii_digit() => (&DIGITS[d as usize - '0' as usize], 3),
      _ => continue,
    };
    for (row, line) in glyph.iter().enumerate() {
      let ghost_line = if width == 3 {
        DIGITS[8][row]
      } else {
        COLON[row]
      };
      for (i, (on, off)) in line.chars().zip(ghost_line.chars()).enumerate() {
        let y = area.y + row as u16;
        let x = cx + i as u16;
        if on != ' ' {
          put(buf, area, x, y, &on.to_string(), lit);
        } else if off != ' ' {
          put(buf, area, x, y, &off.to_string(), ghost);
        }
      }
    }
    cx += width + 1;
  }
  cx - x
}

fn draw_readout(buf: &mut Buffer, area: Rect, view: &HeadUnitView, level: f32, vfd: &Vfd) {
  let clock = view.progress_ms.map(clock_text);
  let shown = clock.clone().unwrap_or_else(|| "--:--".to_string());
  // When nothing is playing, "88:88" with every segment in the unlit ghost
  // color, as on a stopped unit.
  let lit = if clock.is_some() {
    vfd.cap
  } else {
    vfd.ghost(vfd.cap)
  };
  let digits = shown.replace('-', "8");
  let mut x = area.x + METER_WIDTH + 1;
  x += draw_digits(buf, area, x, &digits, lit, vfd) + 1;

  let small = Style::default().fg(rgb(vfd.label));
  let state = if view.live {
    "LIVE"
  } else if view.is_playing {
    "PLAY"
  } else if clock.is_some() {
    "PAUSE"
  } else {
    "STOP"
  };
  put_str(
    buf,
    area,
    x,
    area.y,
    "ST",
    Style::default().fg(rgb(vfd.mid)),
  );
  put_str(buf, area, x, area.y + 2, state, small);

  // Mode chips at the right, lit like the faceplate's buttons.
  let chips = [
    ("SHUF", view.shuffle),
    ("RPT", view.repeat),
    ("CLIP", level > 0.92),
  ];
  let width: u16 = chips.iter().map(|(t, _)| t.len() as u16 + 3).sum();
  let mut cx = area.right().saturating_sub(width);
  let chip_row = Rect::new(area.x, area.y + 1, area.width, 1);
  if cx > x + 6 {
    for (text, on) in chips {
      cx += chip(buf, chip_row, cx, text, on, vfd) + 1;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::ThemePreset;

  fn screen(buf: &Buffer) -> String {
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
  fn fold_stereo_reads_bass_to_treble_and_keeps_the_louder_channel() {
    // Left half reversed (treble outer), right half forward.
    let mirrored = [0.1, 0.2, 0.9, 0.3, 0.4, 0.05];
    assert_eq!(fold_stereo(&mirrored), vec![0.9, 0.4, 0.1]);
  }

  #[test]
  fn clock_text_switches_to_hours_past_an_hour() {
    assert_eq!(clock_text(83_000), "01:23");
    assert_eq!(clock_text(3_723_000), "1:02:03");
  }

  #[test]
  fn the_projection_maps_the_floor_corners_and_back() {
    let floor = Projection::new(FLOOR);
    for ((u, v), corner) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
      .into_iter()
      .zip(FLOOR)
    {
      let (x, y) = floor.apply(u, v);
      assert!((x - corner.0).abs() < 1e-4 && (y - corner.1).abs() < 1e-4);
      let (bu, bv) = floor.invert((x, y));
      assert!((bu - u).abs() < 1e-3 && (bv - v).abs() < 1e-3);
    }
  }

  #[test]
  fn every_strip_leans_toward_the_vanishing_point_and_more_to_the_right() {
    let floor = Projection::new(FLOOR);
    let lean = |band: usize| {
      let u = (band as f32 + 0.5) / BANDS as f32;
      floor.apply(u, 0.0).0 - floor.apply(u, 1.0).0
    };
    // Even the leftmost strip leans: its front end sits right of its back.
    assert!(lean(0) > 0.0);
    for band in 1..BANDS {
      assert!(lean(band) > lean(band - 1), "strip {band}");
    }
  }

  #[test]
  fn back_panels_shrink_to_the_right_and_stand_over_pairs_of_strips() {
    let canvas = Dots::new(100, 22);
    let stage = Stage::new(&canvas);
    let width = |k: usize| {
      let (ua, _) = Stage::lane(2 * k);
      let (_, ub) = Stage::lane(2 * k + 1);
      stage.floor.apply(ub, 1.0).0 - stage.floor.apply(ua, 1.0).0
    };
    for k in 1..BANDS / 2 {
      assert!(width(k) < width(k - 1), "panel {k}");
    }
    assert!(BACK_TOP_RIGHT > BACK_TOP_LEFT, "and shorter to the right");
  }

  #[test]
  fn walls_never_draw_on_the_floor() {
    let mut canvas = Dots::new(100, 22);
    let stage = Stage::new(&canvas);
    stage.draw(&mut canvas, &[0.5; BANDS]);
    let mut walls = Dots::new(100, 22);
    stage.draw_back_wall(&mut walls);
    stage.draw_side_wall(&mut walls);
    for y in 0..walls.height {
      for x in 0..walls.width {
        if walls.get(x, y).is_some() {
          assert!(!stage.on_floor(x, y), "wall dot on the floor at {x},{y}");
        }
      }
    }
  }

  #[test]
  fn each_strip_row_keeps_one_width_and_each_cell_one_color() {
    let mut canvas = Dots::new(100, 22);
    let stage = Stage::new(&canvas);
    stage.draw_strip(&mut canvas, 0, 0.6);
    let rows: Vec<(usize, Vec<usize>)> = (0..canvas.height)
      .map(|y| {
        let xs: Vec<usize> = (0..canvas.width)
          .filter(|&x| canvas.get(x, y).is_some())
          .collect();
        (y, xs)
      })
      .filter(|(_, xs)| !xs.is_empty())
      .collect();
    let width = rows[0].1.len();
    for (y, xs) in &rows {
      assert_eq!(xs.len(), width, "row {y} is a different width");
      assert_eq!(xs.last().unwrap() - xs[0] + 1, width, "row {y} has a hole");
    }
    for cy in 0..canvas.height / 4 {
      let colors: std::collections::HashSet<Rgb> = (0..canvas.width)
        .flat_map(|x| (0..4).filter_map(move |dy| Some((x, cy * 4 + dy))))
        .filter_map(|(x, y)| canvas.get(x, y))
        .collect();
      assert!(colors.len() <= 1, "cell row {cy} mixes colors");
    }
  }

  #[test]
  fn strips_run_green_lavender_blue_and_dim_past_the_level() {
    assert_eq!(Stage::strip_color(0.1, 1.0), vfd_palette::GREEN);
    assert_eq!(Stage::strip_color(0.6, 1.0), vfd_palette::LAVENDER);
    assert_eq!(Stage::strip_color(0.9, 1.0), vfd_palette::BLUE);
    assert_ne!(Stage::strip_color(0.9, 0.5), vfd_palette::BLUE);
  }

  #[test]
  fn a_cjk_title_fits_the_header_by_width_and_leaves_the_chip_alone() {
    let theme = ThemePreset::Gruvbox.to_theme();
    let vfd = Vfd::from_theme(&theme);
    for (width, title) in [
      (100u16, "東京事変 群青日和"),
      (60, "東京事変 群青日和 東京事変 群青日和 東京事変 群青日和"),
      (60, "Around the World"),
    ] {
      let area = Rect::new(0, 0, width, 1);
      let mut buf = Buffer::empty(area);
      let view = HeadUnitView {
        title: Some(title.into()),
        ..HeadUnitView::default()
      };
      draw_header(&mut buf, area, &view, &vfd);
      let row = screen(&buf);
      assert!(
        row.ends_with(" DHE "),
        "{title:?}: the chip is intact: {row:?}"
      );
      assert!(row.starts_with("DSP HEAD UNIT"), "{title:?}: {row:?}");
      // A wide glyph covers the next cell, so that cell must be blank.
      for x in 0..width - 1 {
        if buf[(x, 0)].symbol().width() == 2 {
          assert_eq!(
            buf[(x + 1, 0)].symbol(),
            " ",
            "{title:?}: covered glyph: {row:?}"
          );
        }
      }
      // The title starts with its first character, so it was trimmed at the
      // end if at all.
      let first = title.chars().next().unwrap().to_uppercase().next().unwrap();
      assert!(row.contains(first), "{title:?}: {row:?}");
    }
  }

  #[test]
  fn fit_width_counts_cells_not_characters() {
    assert_eq!(fit_width("東京事変", 5), "東京");
    assert_eq!(fit_width("東京事変", 8), "東京事変");
    assert_eq!(fit_width("ABC", 2), "AB");
  }

  #[test]
  fn the_meter_eases_on_time_not_on_redraws() {
    let t0 = Instant::now();
    let loud = [0.9; BANDS];
    // One frame's worth of time, drawn once or ten times, moves it the same.
    let mut once = HeadUnitState::default();
    once.update(&loud, t0);
    once.update(&loud, t0 + std::time::Duration::from_millis(16));
    let mut many = HeadUnitState::default();
    many.update(&loud, t0);
    for _ in 0..10 {
      many.update(&loud, t0 + std::time::Duration::from_millis(16));
    }
    assert!(
      (once.level - many.level).abs() < 1e-6,
      "{} vs {}",
      once.level,
      many.level
    );
    // At the 60 fps it was tuned at, a rising step still covers 60%.
    let mut tuned = HeadUnitState::default();
    tuned.update(&loud, t0);
    let target = (0.9f32 * 1.4).clamp(0.0, 1.0);
    assert!((tuned.level - 0.6 * target).abs() < 1e-4, "{}", tuned.level);
  }

  #[test]
  fn the_idle_clock_shows_its_ghost_segments() {
    let theme = ThemePreset::Gruvbox.to_theme();
    let vfd = Vfd::from_theme(&theme);
    let area = Rect::new(0, 0, 60, 3);
    let mut buf = Buffer::empty(area);
    draw_readout(&mut buf, area, &HeadUnitView::default(), 0.0, &vfd);
    let ghost = rgb(vfd.ghost(vfd.cap));
    // The clock's digits start past the meter column.
    let x0 = METER_WIDTH + 1;
    let segments: Vec<Color> = (0..3)
      .flat_map(|y| (x0..x0 + 16).map(move |x| (x, y)))
      .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
      .map(|(x, y)| buf[(x, y)].fg)
      .collect();
    assert!(!segments.is_empty(), "the idle clock draws its segments");
    assert!(
      segments.iter().all(|&c| c == ghost && c != rgb(vfd.bg)),
      "idle segments use the ghost color, not the background: {segments:?}"
    );
  }

  #[test]
  fn renders_the_faceplate_at_a_normal_size() {
    let area = Rect::new(0, 0, 116, 30);
    let mut buf = Buffer::empty(area);
    let theme = ThemePreset::Gruvbox.to_theme();
    let view = HeadUnitView {
      title: Some("Around the World".into()),
      progress_ms: Some(83_000),
      is_playing: true,
      shuffle: true,
      ..HeadUnitView::default()
    };
    render(&mut buf, area, &[0.6; 24], &theme, &view, Instant::now());
    let text = screen(&buf);
    assert!(
      text.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c)),
      "the stage is drawn in Braille dots:\n{text}"
    );
    for expected in [
      "DSP HEAD UNIT",
      "DHE",
      "A R O U N D",
      "SHUF",
      "PLAY",
      "1k",
      SEGMENT,
    ] {
      assert!(text.contains(expected), "missing {expected:?}:\n{text}");
    }
  }

  #[test]
  fn tiny_areas_draw_nothing_and_do_not_panic() {
    let theme = ThemePreset::Default.to_theme();
    for (w, h) in [(0u16, 0u16), (11, 30), (200, 3), (12, 4), (13, 5), (20, 8)] {
      let area = Rect::new(0, 0, w, h);
      let mut buf = Buffer::empty(area);
      render(
        &mut buf,
        area,
        &[0.5; 24],
        &theme,
        &HeadUnitView::default(),
        Instant::now(),
      );
    }
  }
}
