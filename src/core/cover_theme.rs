//! Adaptive theming from album art: extract the dominant colors of the
//! decoded cover image, derive a UI theme from them, and fade the live theme
//! between targets on track change.
//!
//! Everything here is pure pixel/color math on data the cover-art pipeline
//! already decodes, so it behaves identically on every platform and terminal.

use crate::core::theme::Color;
use std::time::Duration;

use crate::core::app::ease_in_out_cubic;
use crate::core::user_config::Theme;

/// How long a theme fade takes. Long enough to read as a transition, short
/// enough that the theme settles before the track's intro is over.
const THEME_FADE_SECS: f32 = 0.8;

/// Cover art is downsampled so no side exceeds this before analysis; one pass
/// over at most 64x64 pixels keeps extraction well under a millisecond.
const SAMPLE_DIM: u32 = 64;

/// Minimum share of sampled pixels (1/50 = 2%) a color region needs before it
/// can be picked as the secondary accent, so single stray pixels never drive
/// the theme.
const SECONDARY_MIN_SHARE: u32 = 50;

/// Accent colors extracted from one cover image, before any legibility
/// adjustment (that happens in [`derive_theme`], which knows the base theme's
/// background).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlbumPalette {
  pub primary: (u8, u8, u8),
  pub secondary: (u8, u8, u8),
}

/// Whether an album-derived theme is applied. `base` is the user's own theme,
/// kept while album accents are on screen (or fading out) so it can be
/// restored, and so settings changes know what the user's real colors are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CoverThemeState {
  #[default]
  Inactive,
  Active {
    base: Theme,
  },
  /// Fading back to `base`; flips to `Inactive` when the fade completes.
  Restoring {
    base: Theme,
  },
}

/// An in-flight fade of the live theme. Driven by real elapsed time rather
/// than tick count, so its speed is independent of the configured tick rates.
#[derive(Clone, Copy, Debug)]
pub struct ThemeTransition {
  from: Theme,
  to: Theme,
  progress: f32,
}

impl ThemeTransition {
  pub fn new(from: Theme, to: Theme) -> Self {
    Self {
      from,
      to,
      progress: 0.0,
    }
  }

  pub fn advance(&mut self, elapsed: Duration) {
    self.progress = (self.progress + elapsed.as_secs_f32() / THEME_FADE_SECS).min(1.0);
  }

  pub fn current(&self) -> Theme {
    let eased = ease_in_out_cubic(f64::from(self.progress)) as f32;
    lerp_theme(&self.from, &self.to, eased)
  }

  pub fn is_complete(&self) -> bool {
    self.progress >= 1.0
  }

  pub fn target(&self) -> Theme {
    self.to
  }
}

#[derive(Clone, Default)]
struct Bin {
  count: u32,
  r: u64,
  g: u64,
  b: u64,
}

struct Candidate {
  rgb: (u8, u8, u8),
  hsv: (f32, f32, f32),
  count: u32,
  score: f32,
}

/// Extract the two accent colors that best represent `img`.
///
/// Pixels are pooled into 4-bits-per-channel buckets and each bucket is scored
/// by `count * (0.15 + saturation) * mid-brightness preference`: a colorful
/// region beats a larger near-black or near-white one (borders, text, vinyl
/// grooves), which is what makes the result feel like "the album's color"
/// rather than its literal average. Returns `None` when the image has no
/// opaque pixels.
pub fn extract_palette(img: &image::DynamicImage) -> Option<AlbumPalette> {
  let thumb = img.thumbnail(SAMPLE_DIM, SAMPLE_DIM).into_rgba8();

  // 4 bits per channel: keys are a dense 0..4096 index, so a flat array beats
  // hashing per pixel.
  let mut bins = vec![Bin::default(); 4096];
  let mut total: u32 = 0;
  for px in thumb.pixels() {
    if px[3] < 128 {
      continue;
    }
    let (r, g, b) = (px[0], px[1], px[2]);
    let key = (usize::from(r >> 4) << 8) | (usize::from(g >> 4) << 4) | usize::from(b >> 4);
    let bin = &mut bins[key];
    bin.count += 1;
    bin.r += u64::from(r);
    bin.g += u64::from(g);
    bin.b += u64::from(b);
    total += 1;
  }
  if total == 0 {
    return None;
  }

  let mut candidates: Vec<Candidate> = bins
    .iter()
    .filter(|bin| bin.count > 0)
    .map(|bin| {
      let n = u64::from(bin.count);
      let rgb = ((bin.r / n) as u8, (bin.g / n) as u8, (bin.b / n) as u8);
      let hsv = rgb_to_hsv(rgb);
      let brightness_weight = (1.0 - (hsv.2 - 0.62).abs()).max(0.15);
      let score = bin.count as f32 * (0.15 + hsv.1) * brightness_weight;
      Candidate {
        rgb,
        hsv,
        count: bin.count,
        score,
      }
    })
    .collect();
  // Tie-break on the color value so equal scores (common in synthetic images)
  // resolve deterministically under the unstable sort.
  candidates.sort_unstable_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.rgb.cmp(&b.rgb)));

  let primary = &candidates[0];

  // Secondary: a hue clearly different from the primary, falling back to a
  // brightness contrast (covers monochrome art), falling back to the primary
  // itself ([`derive_theme`] keeps the on-screen accents distinct).
  let eligible: Vec<&Candidate> = candidates
    .iter()
    .skip(1)
    .filter(|c| c.count * SECONDARY_MIN_SHARE >= total)
    .collect();
  let secondary_rgb = eligible
    .iter()
    .find(|c| c.hsv.1 >= 0.15 && hue_distance(c.hsv.0, primary.hsv.0) >= 60.0)
    .or_else(|| {
      eligible
        .iter()
        .find(|c| (c.hsv.2 - primary.hsv.2).abs() >= 0.2)
    })
    .map(|c| c.rgb)
    .unwrap_or(primary.rgb);

  Some(AlbumPalette {
    primary: primary.rgb,
    secondary: secondary_rgb,
  })
}

/// Build the theme shown while `palette`'s album is playing: `base` with only
/// the accent fields replaced. Text, background, borders and error colors stay
/// the user's own, so legibility never depends on what the art happens to be.
pub fn derive_theme(base: &Theme, palette: &AlbumPalette) -> Theme {
  let dark_bg = background_is_dark(base);
  let primary = accent_color(palette.primary, dark_bg);
  let mut secondary = accent_color(palette.secondary, dark_bg);
  // Monochrome art repeats the primary, and the brightness clamp can land two
  // distinct accents on the same color; keep hovered visually distinct from
  // selected either way.
  if secondary == primary {
    secondary = contrast_variant(primary, dark_bg);
  }
  Theme {
    active: primary,
    banner: primary,
    hovered: secondary,
    selected: primary,
    playbar_progress: primary,
    highlighted_lyrics: primary,
    analysis_bar: primary,
    ..*base
  }
}

fn background_is_dark(base: &Theme) -> bool {
  match base.background {
    Color::Rgb(r, g, b) => luma((r, g, b)) < 0.5,
    // Reset / named backgrounds follow the terminal palette, which we can't
    // read; assume dark, the overwhelmingly common case for TUIs.
    _ => true,
  }
}

/// Clamp an extracted color's brightness so it stays readable against the
/// base background: lifted on dark backgrounds, dimmed on light ones.
fn accent_color(rgb: (u8, u8, u8), dark_bg: bool) -> Color {
  let (h, s, v) = rgb_to_hsv(rgb);
  let v = if dark_bg { v.max(0.55) } else { v.min(0.55) };
  let (r, g, b) = hsv_to_rgb((h, s, v));
  Color::Rgb(r, g, b)
}

/// A brightness variant of an already-clamped accent that stays on the legal
/// side of the clamp, so it cannot collapse back into the original.
fn contrast_variant(color: Color, dark_bg: bool) -> Color {
  let Color::Rgb(r, g, b) = color else {
    return color;
  };
  let (h, s, v) = rgb_to_hsv((r, g, b));
  let v = if dark_bg {
    // Legal range [0.55, 1.0].
    if v > 0.775 {
      v - 0.225
    } else {
      v + 0.225
    }
  } else {
    // Legal range [0.0, 0.55].
    if v < 0.325 {
      v + 0.225
    } else {
      v - 0.225
    }
  };
  let (r, g, b) = hsv_to_rgb((h, s, v.clamp(0.0, 1.0)));
  Color::Rgb(r, g, b)
}

fn luma((r, g, b): (u8, u8, u8)) -> f32 {
  (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}

fn hue_distance(a: f32, b: f32) -> f32 {
  let d = (a - b).abs() % 360.0;
  d.min(360.0 - d)
}

/// Thin adapters over `csscolorparser` (already linked via `colorgrad`), so
/// this module carries no color-space math of its own. Same conventions:
/// h in [0..360], s/v in [0..1].
fn rgb_to_hsv((r, g, b): (u8, u8, u8)) -> (f32, f32, f32) {
  let [h, s, v, _] = colorgrad::Color::from_rgba8(r, g, b, 255).to_hsva();
  (h, s, v)
}

fn hsv_to_rgb((h, s, v): (f32, f32, f32)) -> (u8, u8, u8) {
  let [r, g, b, _] = colorgrad::Color::from_hsva(h, s, v, 1.0).to_rgba8();
  (r, g, b)
}

/// Interpolate between two colors. Only RGB pairs can blend; when either end
/// is a named ANSI color or `Reset` (whose actual RGB the terminal owns), the
/// color snaps at the halfway point instead.
fn lerp_color(from: Color, to: Color, t: f32) -> Color {
  match (from, to) {
    (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
      let [r, g, b, _] = colorgrad::Color::from_rgba8(r1, g1, b1, 255)
        .interpolate_rgb(&colorgrad::Color::from_rgba8(r2, g2, b2, 255), t)
        .to_rgba8();
      Color::Rgb(r, g, b)
    }
    _ => {
      if t < 0.5 {
        from
      } else {
        to
      }
    }
  }
}

fn lerp_theme(from: &Theme, to: &Theme, t: f32) -> Theme {
  Theme {
    analysis_bar: lerp_color(from.analysis_bar, to.analysis_bar, t),
    analysis_bar_text: lerp_color(from.analysis_bar_text, to.analysis_bar_text, t),
    active: lerp_color(from.active, to.active, t),
    banner: lerp_color(from.banner, to.banner, t),
    error_border: lerp_color(from.error_border, to.error_border, t),
    error_text: lerp_color(from.error_text, to.error_text, t),
    hint: lerp_color(from.hint, to.hint, t),
    hovered: lerp_color(from.hovered, to.hovered, t),
    inactive: lerp_color(from.inactive, to.inactive, t),
    playbar_background: lerp_color(from.playbar_background, to.playbar_background, t),
    playbar_progress: lerp_color(from.playbar_progress, to.playbar_progress, t),
    playbar_progress_text: lerp_color(from.playbar_progress_text, to.playbar_progress_text, t),
    playbar_text: lerp_color(from.playbar_text, to.playbar_text, t),
    selected: lerp_color(from.selected, to.selected, t),
    text: lerp_color(from.text, to.text, t),
    background: lerp_color(from.background, to.background, t),
    header: lerp_color(from.header, to.header, t),
    highlighted_lyrics: lerp_color(from.highlighted_lyrics, to.highlighted_lyrics, t),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use image::{DynamicImage, Rgba, RgbaImage};

  fn solid(r: u8, g: u8, b: u8) -> DynamicImage {
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(32, 32, Rgba([r, g, b, 255])))
  }

  #[test]
  fn solid_image_yields_that_color_as_primary() {
    let palette = extract_palette(&solid(200, 30, 40)).unwrap();
    assert_eq!(palette.primary, (200, 30, 40));
  }

  #[test]
  fn two_tone_image_yields_hue_distinct_secondary() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(32, 32, |x, _| {
      if x < 16 {
        Rgba([200, 30, 40, 255])
      } else {
        Rgba([30, 60, 220, 255])
      }
    }));
    let palette = extract_palette(&img).unwrap();
    let (pr, _, pb) = palette.primary;
    let (sr, _, sb) = palette.secondary;
    // One accent red-dominant, the other blue-dominant, in either order.
    assert!(
      (pr > pb && sb > sr) || (pb > pr && sr > sb),
      "expected one red and one blue accent, got {palette:?}"
    );
  }

  #[test]
  fn grayscale_image_still_yields_a_palette() {
    let palette = extract_palette(&solid(128, 128, 128)).unwrap();
    assert_eq!(palette.primary, (128, 128, 128));
    // With no second color to offer, the palette repeats the primary;
    // derive_theme keeps the on-screen accents distinct.
    assert_eq!(palette.secondary, palette.primary);
    let derived = derive_theme(&Theme::default(), &palette);
    assert_ne!(derived.hovered, derived.selected);
  }

  #[test]
  fn fully_transparent_image_yields_none() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 16, Rgba([10, 10, 10, 0])));
    assert!(extract_palette(&img).is_none());
  }

  #[test]
  fn colorful_region_beats_larger_dark_region() {
    // 75% near-black border, 25% saturated orange: the orange should win.
    let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(32, 32, |x, _| {
      if x < 8 {
        Rgba([240, 140, 20, 255])
      } else {
        Rgba([12, 12, 12, 255])
      }
    }));
    let palette = extract_palette(&img).unwrap();
    assert_eq!(palette.primary, (240, 140, 20));
  }

  #[test]
  fn derive_replaces_accents_and_keeps_base_text_and_background() {
    let base = Theme::default();
    let palette = AlbumPalette {
      primary: (200, 30, 40),
      secondary: (30, 60, 220),
    };
    let derived = derive_theme(&base, &palette);
    assert_ne!(derived.active, base.active);
    assert_eq!(derived.text, base.text);
    assert_eq!(derived.background, base.background);
    assert_eq!(derived.error_border, base.error_border);
    assert_eq!(derived.inactive, base.inactive);
  }

  #[test]
  fn derive_lifts_dark_accents_on_dark_background() {
    let base = Theme::default(); // background: Reset -> assumed dark
    let palette = AlbumPalette {
      primary: (10, 10, 60), // very dark navy
      secondary: (10, 10, 60),
    };
    let derived = derive_theme(&base, &palette);
    match derived.active {
      Color::Rgb(r, g, b) => {
        let v = f32::from(r.max(g).max(b)) / 255.0;
        assert!(v >= 0.54, "accent stayed too dark: {r},{g},{b}");
      }
      other => panic!("expected RGB accent, got {other:?}"),
    }
  }

  #[test]
  fn derive_dims_bright_accents_on_light_background() {
    let base = Theme {
      background: Color::Rgb(250, 250, 250),
      ..Theme::default()
    };
    let palette = AlbumPalette {
      primary: (255, 240, 100), // bright yellow
      secondary: (255, 240, 100),
    };
    let derived = derive_theme(&base, &palette);
    match derived.active {
      Color::Rgb(r, g, b) => {
        let v = f32::from(r.max(g).max(b)) / 255.0;
        assert!(v <= 0.56, "accent stayed too bright: {r},{g},{b}");
      }
      other => panic!("expected RGB accent, got {other:?}"),
    }
  }

  #[test]
  fn derived_accents_stay_distinct_after_clamping() {
    let base = Theme::default();
    // Monochrome palette: both accents clamp to the same brightness floor.
    let palette = AlbumPalette {
      primary: (128, 128, 128),
      secondary: (52, 52, 52),
    };
    let derived = derive_theme(&base, &palette);
    assert_ne!(derived.hovered, derived.selected);
  }

  #[test]
  fn lerp_color_blends_rgb_and_snaps_named_at_half() {
    let from = Color::Rgb(0, 0, 0);
    let to = Color::Rgb(255, 255, 255);
    assert_eq!(lerp_color(from, to, 0.0), from);
    assert_eq!(lerp_color(from, to, 1.0), to);
    match lerp_color(from, to, 0.5) {
      Color::Rgb(r, g, b) => {
        assert!((127..=128).contains(&r));
        assert_eq!(r, g);
        assert_eq!(g, b);
      }
      other => panic!("expected RGB midpoint, got {other:?}"),
    }
    assert_eq!(lerp_color(Color::Cyan, to, 0.4), Color::Cyan);
    assert_eq!(lerp_color(Color::Cyan, to, 0.6), to);
  }

  #[test]
  fn transition_advances_with_elapsed_time_and_completes() {
    let from = Theme::default();
    let to = derive_theme(
      &from,
      &AlbumPalette {
        primary: (200, 30, 40),
        secondary: (30, 60, 220),
      },
    );
    let mut transition = ThemeTransition::new(from, to);
    assert_eq!(transition.current(), from);

    transition.advance(Duration::from_millis(400));
    assert!(!transition.is_complete());
    let mid = transition.current();
    assert_ne!(mid, from);
    assert_ne!(mid, to);

    transition.advance(Duration::from_millis(500));
    assert!(transition.is_complete());
    assert_eq!(transition.current(), to);
  }
}
