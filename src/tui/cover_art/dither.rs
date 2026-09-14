//! Cover-art dithering at output resolution.

use image::{imageops::FilterType, DynamicImage, GrayImage, Rgb, RgbImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Algorithm {
  Bayer8x8,
  Stucki,
  Atkinson,
}

impl Algorithm {
  pub(super) fn from_name(name: &str) -> Self {
    match name {
      "bayer8x8" => Self::Bayer8x8,
      "atkinson" => Self::Atkinson,
      _ => Self::Stucki,
    }
  }
}

pub(super) struct Mask {
  pub(super) width: u32,
  pub(super) height: u32,
  /// 0 is a shadow, 1 is a highlight.
  pub(super) pixels: Vec<u8>,
}

/// Fit dimensions without resampling the source image.
pub(super) fn fit_dimensions(source: &DynamicImage, width: u32, height: u32) -> (u32, u32) {
  let ratio = (f64::from(width) / f64::from(source.width()))
    .min(f64::from(height) / f64::from(source.height()));
  (
    (f64::from(source.width()) * ratio).round().max(1.0) as u32,
    (f64::from(source.height()) * ratio).round().max(1.0) as u32,
  )
}

const BAYER8: [[u8; 8]; 8] = [
  [0, 48, 12, 60, 3, 51, 15, 63],
  [32, 16, 44, 28, 35, 19, 47, 31],
  [8, 56, 4, 52, 11, 59, 7, 55],
  [40, 24, 36, 20, 43, 27, 39, 23],
  [2, 50, 14, 62, 1, 49, 13, 61],
  [34, 18, 46, 30, 33, 17, 45, 29],
  [10, 58, 6, 54, 9, 57, 5, 53],
  [42, 26, 38, 22, 41, 25, 37, 21],
];

pub(super) fn make_mask(
  source: &DynamicImage,
  width: u32,
  height: u32,
  scale: u8,
  algorithm: Algorithm,
) -> Mask {
  let scale = u32::from(scale.max(1));
  let width = width.max(1);
  let height = height.max(1);
  let grid_width = width.div_ceil(scale);
  let grid_height = height.div_ceil(scale);
  let gray: GrayImage = source
    .resize_exact(grid_width, grid_height, FilterType::Lanczos3)
    .to_luma8();
  let mut tones: Vec<f32> = gray.pixels().map(|pixel| f32::from(pixel[0])).collect();
  // Normalize contrast without letting extreme pixels set the range.
  let mut sorted = tones.clone();
  sorted.sort_unstable_by(f32::total_cmp);
  let low = sorted[sorted.len() * 5 / 100];
  let high = sorted[sorted.len() * 95 / 100];
  if high - low >= 8.0 {
    for tone in &mut tones {
      *tone = ((*tone - low) * 224.0 / (high - low) + 16.0).clamp(0.0, 255.0);
    }
  }
  let shortest_side = grid_width.min(grid_height) as f32;
  // Keep ordered-dither noise low on small grids.
  let bayer_variation = 24.0 + ((shortest_side - 16.0) / 80.0).clamp(0.0, 1.0) * 104.0;
  let diffusion_strength = (shortest_side / 64.0).clamp(0.25, 1.0);
  let mut grid = vec![0_u8; tones.len()];

  for y in 0..grid_height as usize {
    for x in 0..grid_width as usize {
      let index = y * grid_width as usize + x;
      let threshold = match algorithm {
        Algorithm::Bayer8x8 => {
          128.0 + (f32::from(BAYER8[y % 8][x % 8]) + 0.5 - 32.0) * bayer_variation / 32.0
        }
        Algorithm::Stucki | Algorithm::Atkinson => 128.0,
      };
      let white = tones[index] >= threshold;
      grid[index] = u8::from(white);
      let error = tones[index] - if white { 255.0 } else { 0.0 };
      let taps: &[(i32, i32, f32)] = match algorithm {
        Algorithm::Bayer8x8 => &[],
        Algorithm::Stucki => &[
          (1, 0, 8.0 / 42.0),
          (2, 0, 4.0 / 42.0),
          (-2, 1, 2.0 / 42.0),
          (-1, 1, 4.0 / 42.0),
          (0, 1, 8.0 / 42.0),
          (1, 1, 4.0 / 42.0),
          (2, 1, 2.0 / 42.0),
          (-2, 2, 1.0 / 42.0),
          (-1, 2, 2.0 / 42.0),
          (0, 2, 4.0 / 42.0),
          (1, 2, 2.0 / 42.0),
          (2, 2, 1.0 / 42.0),
        ],
        Algorithm::Atkinson => &[
          (1, 0, 1.0 / 8.0),
          (2, 0, 1.0 / 8.0),
          (-1, 1, 1.0 / 8.0),
          (0, 1, 1.0 / 8.0),
          (1, 1, 1.0 / 8.0),
          (0, 2, 1.0 / 8.0),
        ],
      };
      for &(dx, dy, weight) in taps {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx >= 0 && ny >= 0 && nx < grid_width as i32 && ny < grid_height as i32 {
          let next = ny as usize * grid_width as usize + nx as usize;
          tones[next] += error * weight * diffusion_strength;
        }
      }
    }
  }

  let mut pixels = vec![0; (width * height) as usize];
  for y in 0..height {
    for x in 0..width {
      pixels[(y * width + x) as usize] = grid[((y / scale) * grid_width + x / scale) as usize];
    }
  }
  Mask {
    width,
    height,
    pixels,
  }
}

pub(super) fn luminance(rgb: [u8; 3]) -> f32 {
  0.2126 * f32::from(rgb[0]) + 0.7152 * f32::from(rgb[1]) + 0.0722 * f32::from(rgb[2])
}

pub(super) fn colorize(mask: &Mask, accent: [u8; 3], background: [u8; 3]) -> DynamicImage {
  let accent_is_dark = luminance(accent) < luminance(background);
  let image = RgbImage::from_fn(mask.width, mask.height, |x, y| {
    let highlight = mask.pixels[(y * mask.width + x) as usize] != 0;
    Rgb(if highlight != accent_is_dark {
      accent
    } else {
      background
    })
  });
  DynamicImage::ImageRgb8(image)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn fitted_dimensions_match_image_resize_without_resampling_source() {
    for (source_width, source_height, target_width, target_height) in [
      (600, 600, 80, 80),
      (600, 300, 80, 80),
      (300, 600, 80, 80),
      (31, 47, 80, 80),
    ] {
      let source = DynamicImage::new_rgb8(source_width, source_height);
      let expected = source.resize(target_width, target_height, FilterType::Lanczos3);
      assert_eq!(
        fit_dimensions(&source, target_width, target_height),
        (expected.width(), expected.height())
      );
    }
  }

  #[test]
  fn algorithms_are_deterministic_and_two_tone() {
    assert_eq!(Algorithm::from_name("stucki"), Algorithm::Stucki);
    assert_eq!(Algorithm::from_name("bayer8x8"), Algorithm::Bayer8x8);
    assert_eq!(Algorithm::from_name("unknown"), Algorithm::Stucki);
    let source = DynamicImage::ImageRgb8(RgbImage::from_fn(17, 13, |x, y| {
      Rgb([(x * 15) as u8, (y * 19) as u8, 127])
    }));
    for algorithm in [Algorithm::Bayer8x8, Algorithm::Stucki, Algorithm::Atkinson] {
      let first = make_mask(&source, 17, 13, 1, algorithm);
      let second = make_mask(&source, 17, 13, 1, algorithm);
      assert_eq!(first.pixels, second.pixels);
      let image = colorize(&first, [10, 20, 30], [240, 240, 240]).to_rgb8();
      assert!(image
        .pixels()
        .all(|p| p.0 == [10, 20, 30] || p.0 == [240, 240, 240]));
    }
  }

  #[test]
  fn tiny_low_contrast_art_keeps_its_large_shapes() {
    let source = DynamicImage::ImageRgb8(RgbImage::from_fn(8, 16, |x, _| {
      let tone = if x < 4 { 96 } else { 160 };
      Rgb([tone, tone, tone])
    }));
    for algorithm in [Algorithm::Bayer8x8, Algorithm::Stucki, Algorithm::Atkinson] {
      let mask = make_mask(&source, 8, 16, 1, algorithm);
      let mut shadow_highlights = 0;
      let mut subject_highlights = 0;
      for y in 0..16 {
        for x in 0..8 {
          if mask.pixels[(y * 8 + x) as usize] != 0 {
            if x < 4 {
              shadow_highlights += 1;
            } else {
              subject_highlights += 1;
            }
          }
        }
      }
      assert!(
        shadow_highlights <= 12,
        "{algorithm:?} lost the shadow: {shadow_highlights}"
      );
      assert!(
        subject_highlights >= 52,
        "{algorithm:?} lost the subject: {subject_highlights}"
      );
    }
  }

  #[test]
  fn scale_creates_larger_constant_blocks() {
    let source = DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 8, Rgb([128, 128, 128])));
    let mask = make_mask(&source, 12, 12, 3, Algorithm::Bayer8x8);
    for y in 0..12 {
      for x in 0..12 {
        assert_eq!(
          mask.pixels[(y * 12 + x) as usize],
          mask.pixels[((y / 3 * 3) * 12 + x / 3 * 3) as usize]
        );
      }
    }
  }

  #[test]
  fn light_background_assigns_dark_accent_to_shadows() {
    let mask = Mask {
      width: 2,
      height: 1,
      pixels: vec![0, 1],
    };
    let light = colorize(&mask, [20, 20, 20], [245, 245, 245]).to_rgb8();
    assert_eq!(light.get_pixel(0, 0).0, [20, 20, 20]);
    let dark = colorize(&mask, [240, 240, 240], [10, 10, 10]).to_rgb8();
    assert_eq!(dark.get_pixel(1, 0).0, [240, 240, 240]);
  }
}
