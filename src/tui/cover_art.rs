//! Terminal cover-art rendering and per-surface dither caches.

use crate::core::art::{CoverArtStatus, CoverArtStore};
use crate::core::plugin_api::PluginCoverArtFit;
use crate::core::theme::{resolve, Color, Palette};
use crate::core::user_config::UserConfig;
use log::{info, warn};
use ratatui::{
  layout::{Rect, Size},
  Frame,
};
use ratatui_image::{
  picker::{Capability, Picker, ProtocolType},
  protocol::StatefulProtocol,
  Resize, StatefulImage,
};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock, PoisonError};

mod dither;
use dither::{Algorithm, Mask};

/// The message shown in place of the image when none is loaded, so "no art"
/// always reads as a deliberate outcome rather than a blank pane. Shared by the
/// fullscreen view and the plugin-screen `cover_art` widget.
pub fn status_message(status: CoverArtStatus) -> &'static str {
  match status {
    CoverArtStatus::Loading => "Loading cover art...",
    CoverArtStatus::Unavailable => "No cover art for this source",
    CoverArtStatus::Failed => "Cover art unavailable",
    CoverArtStatus::Loaded | CoverArtStatus::NotStarted => "No cover art available",
  }
}

impl PluginCoverArtFit {
  /// The `ratatui-image` resize strategy for this fit mode. Kept private so
  /// `Resize` does not leak into the UI layer.
  fn resize(self) -> Resize {
    match self {
      PluginCoverArtFit::Contain => Resize::Fit(None),
      PluginCoverArtFit::Scale => Resize::Scale(None),
    }
  }
}

/// One renderer per process: the picker describes the one terminal this
/// process draws to. Until [`init_renderer`] runs, every render below is a
/// no-op and [`full_image_support`] reports false.
static RENDERER: OnceLock<CoverArtRenderer> = OnceLock::new();

/// Probe the terminal for its image protocol and install the process-wide
/// renderer. Called from `start_ui` after `ratatui::init()`: the probe is a
/// stdio round-trip and must not run during `App` construction.
pub fn init_renderer() {
  RENDERER.get_or_init(|| {
    let picker = Picker::from_query_stdio().unwrap_or_else(|err| {
      warn!("cover art renderer fallback to halfblocks: {err}");
      Picker::halfblocks()
    });

    info!(
      "cover art renderer detected a {:?} backend",
      picker.protocol_type()
    );
    CoverArtRenderer::new(picker)
  });
}

/// Install a halfblocks renderer without probing stdio, so `TestBackend`
/// tests can render actual image cells.
#[cfg(test)]
pub fn init_test_renderer() {
  RENDERER.get_or_init(|| CoverArtRenderer::new(Picker::halfblocks()));
}

/// Whether the detected terminal draws real pixels (Kitty/iTerm2/Sixel) as
/// opposed to the halfblocks character fallback.
pub fn full_image_support() -> bool {
  RENDERER
    .get()
    .is_some_and(CoverArtRenderer::full_image_support)
}

/// Drop cached protocols that no longer match the store, so cleared art
/// releases its memory even while no cover surface is being drawn. Called once
/// per frame by the runner; rebuilds happen lazily at the next render.
pub fn sync(store: &CoverArtStore) {
  let Some(renderer) = RENDERER.get() else {
    return;
  };
  for surface in [&renderer.playbar, &renderer.fullscreen, &renderer.plugin] {
    let mut lock = lock_surface(surface);
    let stale = lock
      .as_ref()
      .is_some_and(|surface| store.key() != Some(surface.key.as_str()));
    if stale {
      *lock = None;
    }
  }
}

/// Render into the playbar's cover slot.
pub fn render(f: &mut Frame, area: Rect, store: &CoverArtStore, config: &UserConfig) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(
      &renderer.playbar,
      f,
      area,
      store,
      config,
      RenderStyle {
        resize: Resize::Fit(None),
        background: config.theme.playbar_background,
        fill: false,
      },
    );
  }
}

/// Measure the playbar image's fitted size, so the layout can size its slot.
pub fn size_for(area: Rect, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.playbar, area, Resize::Fit(None), store)
}

/// Render into the fullscreen cover art view.
pub fn render_fullscreen(f: &mut Frame, area: Rect, store: &CoverArtStore, config: &UserConfig) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(
      &renderer.fullscreen,
      f,
      area,
      store,
      config,
      RenderStyle {
        resize: Resize::Fit(None),
        background: config.theme.background,
        fill: false,
      },
    );
  }
}

/// Measure the fullscreen image's fitted size, so the caller can center it.
pub fn fullscreen_size_for(area: Rect, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.fullscreen, area, Resize::Fit(None), store)
}

/// Render into a plugin screen's `cover_art` widget slot.
pub fn render_plugin(
  f: &mut Frame,
  area: Rect,
  fit: PluginCoverArtFit,
  store: &CoverArtStore,
  config: &UserConfig,
) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(
      &renderer.plugin,
      f,
      area,
      store,
      config,
      RenderStyle {
        resize: fit.resize(),
        background: config.theme.background,
        fill: fit == PluginCoverArtFit::Scale,
      },
    );
  }
}

/// Measure the plugin slot's fitted size, so the caller can center it.
pub fn plugin_size_for(area: Rect, fit: PluginCoverArtFit, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.plugin, area, fit.resize(), store)
}

struct CoverArtRenderer {
  picker: Picker,
  terminal_background: Option<[u8; 3]>,
  /// Playbar protocol state.
  playbar: Mutex<Option<Surface>>,
  /// Separate protocol state for the fullscreen cover art view, avoiding
  /// conflicts when the same image is rendered in both the playbar and
  /// fullscreen in one frame.
  fullscreen: Mutex<Option<Surface>>,
  /// Separate protocol state for a plugin screen's `cover_art` widget, for the
  /// same reason: a plugin can size its image differently from the playbar, and
  /// a shared protocol would re-encode on every switch between them.
  plugin: Mutex<Option<Surface>>,
}

/// A surface's cached protocol together with the store key it was built from.
struct Surface {
  key: String,
  protocol: StatefulProtocol,
  dither: Option<DitherSurface>,
}

#[derive(Clone, PartialEq, Eq)]
struct MaskKey {
  artwork: String,
  width: u32,
  height: u32,
  algorithm: String,
  scale: u8,
  fill: bool,
}

type ColorPair = ([u8; 3], [u8; 3]);

struct DitherSurface {
  key: MaskKey,
  pending: Option<mpsc::Receiver<PreparedMask>>,
  requested_key: Option<MaskKey>,
  failed: bool,
  mask: Option<Arc<Mask>>,
  pending_color: Option<(ColorPair, mpsc::Receiver<image::DynamicImage>)>,
  colors: Option<ColorPair>,
  protocol: Option<StatefulProtocol>,
}

struct PreparedMask {
  mask: Mask,
  initial_color: Option<(ColorPair, image::DynamicImage)>,
}

fn spawn_mask_worker(
  source: Arc<image::DynamicImage>,
  key: MaskKey,
  desired: ColorPair,
  graphics: bool,
) -> DitherSurface {
  let (sender, receiver) = mpsc::channel();
  let (width, height, scale, fill, algorithm) = (
    key.width,
    key.height,
    key.scale,
    key.fill,
    key.algorithm.clone(),
  );
  std::thread::spawn(move || {
    let (fitted_width, fitted_height) = if fill {
      (width, height)
    } else {
      dither::fit_dimensions(&source, width, height)
    };
    let mask = dither::make_mask(
      &source,
      fitted_width,
      fitted_height,
      scale,
      Algorithm::from_name(&algorithm),
    );
    let initial_color = graphics.then(|| (desired, dither::colorize(&mask, desired.0, desired.1)));
    let _ = sender.send(PreparedMask {
      mask,
      initial_color,
    });
  });
  DitherSurface {
    key,
    pending: Some(receiver),
    requested_key: None,
    failed: false,
    mask: None,
    pending_color: None,
    colors: None,
    protocol: None,
  }
}

struct RenderStyle {
  resize: Resize,
  background: Color,
  fill: bool,
}

/// Lock a surface's protocol cache, recovering a poisoned guard: the caches
/// are pure derived state (rebuilt from the store at the next render), so a
/// panic that unwound while a guard was held must not turn every later frame
/// into a poison panic.
fn lock_surface(surface: &Mutex<Option<Surface>>) -> MutexGuard<'_, Option<Surface>> {
  surface.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Render the mask with 2×4 Braille dots per terminal cell.
fn render_braille_mask(
  f: &mut Frame,
  area: Rect,
  mask: &Mask,
  accent: [u8; 3],
  background: [u8; 3],
) {
  let accent_is_dark = dither::luminance(accent) < dither::luminance(background);
  let rgb = |value: [u8; 3]| ratatui::style::Color::Rgb(value[0], value[1], value[2]);
  const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
  for y in 0..area.height {
    if u32::from(y) * 4 >= mask.height {
      break;
    }
    for x in 0..area.width.min(mask.width.div_ceil(2) as u16) {
      let mut accent_dots = 0_u8;
      for (dot_y, row) in DOTS.iter().enumerate() {
        for (dot_x, bit) in row.iter().enumerate() {
          let pixel_x = u32::from(x) * 2 + dot_x as u32;
          let pixel_y = u32::from(y) * 4 + dot_y as u32;
          if pixel_x < mask.width && pixel_y < mask.height {
            let highlight = mask.pixels[(pixel_y * mask.width + pixel_x) as usize] != 0;
            if highlight != accent_is_dark {
              accent_dots |= *bit;
            }
          }
        }
      }
      // Fill the cell with its majority color.
      let accent_majority = accent_dots.count_ones() > 4;
      let (foreground, cell_background, dots) = if accent_majority {
        (background, accent, !accent_dots)
      } else {
        (accent, background, accent_dots)
      };
      if let Some(cell) = f.buffer_mut().cell_mut((area.x + x, area.y + y)) {
        cell
          .set_char(char::from_u32(0x2800 + u32::from(dots)).unwrap_or(' '))
          .set_fg(rgb(foreground))
          .set_bg(rgb(cell_background));
      }
    }
  }
}

impl CoverArtRenderer {
  fn new(picker: Picker) -> Self {
    let terminal_background = picker.capabilities().iter().find_map(|capability| {
      if let Capability::Background(r, g, b) = capability {
        Some([*r, *g, *b])
      } else {
        None
      }
    });
    Self {
      picker,
      terminal_background,
      playbar: Mutex::new(None),
      fullscreen: Mutex::new(None),
      plugin: Mutex::new(None),
    }
  }

  fn full_image_support(&self) -> bool {
    match self.picker.protocol_type() {
      ProtocolType::Kitty | ProtocolType::Iterm2 | ProtocolType::Sixel => true,
      ProtocolType::Halfblocks => false,
    }
  }

  fn dither_grid(&self, area: Rect, scale: u8) -> (u32, u32, u8) {
    let font = self.picker.font_size();
    if self.full_image_support() {
      (
        u32::from(area.width) * u32::from(font.width),
        u32::from(area.height) * u32::from(font.height),
        scale,
      )
    } else {
      (u32::from(area.width) * 2, u32::from(area.height) * 4, scale)
    }
  }

  fn dither_colors(&self, config: &UserConfig, background: Color) -> ColorPair {
    let fallback_background = self.terminal_background.unwrap_or([0, 0, 0]);
    let mut palette = Palette {
      reset: fallback_background,
      ..Palette::default()
    };
    let background = resolve(background, &palette);
    palette.reset = [255, 255, 255];
    let accent = resolve(
      config
        .cover_art_dither_color
        .unwrap_or(config.theme.playbar_progress),
      &palette,
    );
    (accent, background)
  }

  /// Reconcile a surface's cached protocol with the store: rebuild it when the
  /// store holds art under a different key, drop it when the store is empty.
  /// This is the per-key cache that keeps a protocol from being rebuilt for a
  /// frame that renders the same art.
  fn ensure<'a>(
    &self,
    surface: &'a Mutex<Option<Surface>>,
    store: &CoverArtStore,
  ) -> MutexGuard<'a, Option<Surface>> {
    let mut lock = lock_surface(surface);
    match store.key() {
      None => *lock = None,
      Some(key) => {
        let cached = lock.as_ref().is_some_and(|surface| surface.key == key);
        if !cached {
          if let Some(image) = store.image() {
            *lock = Some(Surface {
              key: key.to_string(),
              protocol: self.picker.new_resize_protocol(image.clone()),
              dither: None,
            });
          }
        }
      }
    }
    lock
  }

  fn render_surface(
    &self,
    surface: &Mutex<Option<Surface>>,
    f: &mut Frame,
    area: Rect,
    store: &CoverArtStore,
    config: &UserConfig,
    style: RenderStyle,
  ) {
    let mut lock = self.ensure(surface, store);
    if let Some(surface) = lock.as_mut() {
      if config.behavior.cover_art_dither && area.width > 0 && area.height > 0 {
        let graphics = self.full_image_support();
        let desired = self.dither_colors(config, style.background);
        let scale = crate::core::user_config::normalize_cover_art_dither_pixel_scale(i64::from(
          config.behavior.cover_art_dither_pixel_scale,
        ));
        let (width, height, pixel_scale) = self.dither_grid(area, scale);
        let key = MaskKey {
          artwork: surface.key.clone(),
          width,
          height,
          algorithm: config.behavior.cover_art_dither_algorithm.clone(),
          scale: pixel_scale,
          fill: style.fill,
        };
        let pending_same_artwork = surface
          .dither
          .as_ref()
          .is_some_and(|d| d.pending.is_some() && d.key.artwork == key.artwork);
        if pending_same_artwork {
          surface.dither.as_mut().unwrap().requested_key = Some(key.clone());
        } else if surface.dither.as_ref().is_none_or(|d| d.key != key) {
          if let Some(source) = store.image_shared() {
            surface.dither = Some(spawn_mask_worker(source, key, desired, graphics));
          }
        }
        if let Some(dither) = surface.dither.as_mut() {
          if let Some(receiver) = dither.pending.as_ref() {
            match receiver.try_recv() {
              Ok(prepared) => {
                dither.mask = Some(Arc::new(prepared.mask));
                dither.pending = None;
                if let Some((colors, image)) = prepared.initial_color {
                  if colors == desired {
                    dither.protocol = Some(self.picker.new_resize_protocol(image));
                    dither.colors = Some(colors);
                  }
                }
              }
              Err(mpsc::TryRecvError::Disconnected) => {
                warn!("cover art dithering worker stopped before producing an image");
                dither.pending = None;
                dither.failed = true;
              }
              Err(mpsc::TryRecvError::Empty) => {}
            }
          }
          if dither.pending.is_none() {
            if let Some(requested_key) = dither.requested_key.take() {
              if requested_key != dither.key {
                if let Some(source) = store.image_shared() {
                  *dither = spawn_mask_worker(source, requested_key, desired, graphics);
                }
              }
            }
          }
          if dither.failed {
            f.render_stateful_widget(
              StatefulImage::new().resize(style.resize),
              area,
              &mut surface.protocol,
            );
            return;
          }
          if let Some(mask) = &dither.mask {
            if !graphics {
              dither.colors = Some(desired);
              render_braille_mask(f, area, mask, desired.0, desired.1);
              return;
            }
            if let Some((colors, receiver)) = dither.pending_color.as_ref() {
              if let Ok(image) = receiver.try_recv() {
                let completed = *colors;
                dither.pending_color = None;
                if completed == desired {
                  dither.protocol = Some(self.picker.new_resize_protocol(image));
                  dither.colors = Some(completed);
                }
              }
            }
            if dither.colors != Some(desired) && dither.pending_color.is_none() {
              let mask = Arc::clone(mask);
              let (sender, receiver) = mpsc::channel();
              std::thread::spawn(move || {
                let _ = sender.send(dither::colorize(&mask, desired.0, desired.1));
              });
              dither.pending_color = Some((desired, receiver));
            }
          }
          if let Some(protocol) = dither.protocol.as_mut() {
            f.render_stateful_widget(StatefulImage::new().resize(style.resize), area, protocol);
            return;
          }
        }
        f.render_widget(ratatui::widgets::Clear, area);
        f.render_widget(
          ratatui::widgets::Block::default()
            .style(ratatui::style::Style::default().bg(style.background.into())),
          area,
        );
        return;
      } else {
        surface.dither = None;
      }
      f.render_stateful_widget(
        StatefulImage::new().resize(style.resize),
        area,
        &mut surface.protocol,
      );
    }
  }

  fn surface_size_for(
    &self,
    surface: &Mutex<Option<Surface>>,
    area: Rect,
    resize: Resize,
    store: &CoverArtStore,
  ) -> Option<Rect> {
    let lock = self.ensure(surface, store);
    lock.as_ref().map(|surface| {
      let size = surface.protocol.size_for(
        resize,
        Size {
          width: area.width,
          height: area.height,
        },
      );
      Rect::new(0, 0, size.width, size.height)
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use image::{DynamicImage, Rgb, RgbImage};
  use ratatui::{backend::TestBackend, Terminal};

  #[test]
  fn dither_grid_matches_braille_and_graphics_pixels() {
    let area = Rect::new(0, 0, 8, 4);
    let halfblocks = CoverArtRenderer::new(Picker::halfblocks());
    assert_eq!(halfblocks.dither_grid(area, 1), (16, 16, 1));
    assert_eq!(halfblocks.dither_grid(area, 2), (16, 16, 2));
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    let graphics = CoverArtRenderer::new(picker);
    assert_eq!(graphics.dither_grid(area, 1), (80, 80, 1));
    assert_eq!(graphics.dither_grid(area, 2), (80, 80, 2));
    assert_eq!(graphics.dither_grid(area, 3), (80, 80, 3));
  }

  #[test]
  fn braille_fallback_uses_subcell_detail_and_solid_majority_color() {
    let mut pixels = vec![0; 16];
    pixels[0] = 1;
    for y in 0..4 {
      pixels[y * 4 + 2] = 1;
      pixels[y * 4 + 3] = 1;
    }
    let mask = Mask {
      width: 4,
      height: 4,
      pixels,
    };
    let mut terminal = Terminal::new(TestBackend::new(2, 1)).unwrap();
    terminal
      .draw(|frame| {
        render_braille_mask(
          frame,
          Rect::new(0, 0, 2, 1),
          &mask,
          [200, 205, 240],
          [20, 20, 30],
        )
      })
      .unwrap();
    let left = terminal.backend().buffer().cell((0, 0)).unwrap();
    let right = terminal.backend().buffer().cell((1, 0)).unwrap();
    assert_eq!(left.symbol(), "⠁");
    assert_eq!(left.fg, ratatui::style::Color::Rgb(200, 205, 240));
    assert_eq!(left.bg, ratatui::style::Color::Rgb(20, 20, 30));
    assert_eq!(right.symbol(), "⠀");
    assert_eq!(right.bg, ratatui::style::Color::Rgb(200, 205, 240));
  }

  #[test]
  fn graphics_recolor_finishes_off_frame_and_reuses_mask() {
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    let renderer = CoverArtRenderer::new(picker);
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "cover".into(),
      DynamicImage::ImageRgb8(RgbImage::from_fn(128, 128, |x, y| {
        Rgb([x as u8, y as u8, 64])
      })),
    );
    let mut config = UserConfig::new();
    config.behavior.cover_art_dither = true;
    let draw = |terminal: &mut Terminal<TestBackend>, config: &UserConfig| {
      terminal
        .draw(|frame| {
          renderer.render_surface(
            &renderer.playbar,
            frame,
            Rect::new(0, 0, 8, 4),
            &store,
            config,
            RenderStyle {
              resize: Resize::Fit(None),
              background: Color::Rgb(0, 0, 0),
              fill: false,
            },
          )
        })
        .unwrap();
    };
    for _ in 0..100 {
      draw(&mut terminal, &config);
      if lock_surface(&renderer.playbar)
        .as_ref()
        .unwrap()
        .dither
        .as_ref()
        .unwrap()
        .protocol
        .is_some()
      {
        break;
      }
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let guard = lock_surface(&renderer.playbar);
    let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
    assert!(dither.protocol.is_some());
    assert!(dither.pending_color.is_none());
    assert_eq!(
      (
        dither.mask.as_ref().unwrap().width,
        dither.mask.as_ref().unwrap().height
      ),
      (80, 80)
    );
    let mask = Arc::as_ptr(dither.mask.as_ref().unwrap());
    drop(guard);
    config.theme.playbar_progress = Color::Rgb(9, 8, 7);
    draw(&mut terminal, &config);
    let guard = lock_surface(&renderer.playbar);
    let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
    assert_eq!(Arc::as_ptr(dither.mask.as_ref().unwrap()), mask);
    assert!(dither.protocol.is_some());
    drop(guard);
    for _ in 0..100 {
      draw(&mut terminal, &config);
      if lock_surface(&renderer.playbar)
        .as_ref()
        .unwrap()
        .dither
        .as_ref()
        .unwrap()
        .colors
        == Some(([9, 8, 7], [0, 0, 0]))
      {
        break;
      }
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(lock_surface(&renderer.playbar)
      .as_ref()
      .unwrap()
      .dither
      .as_ref()
      .unwrap()
      .protocol
      .is_some());
  }

  #[test]
  fn pending_dither_clears_the_slot_instead_of_drawing_original_art() {
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "cover".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 64, Rgb([220, 40, 40]))),
    );
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let mut config = UserConfig::new();
    config.behavior.cover_art_dither = true;
    let (sender, receiver) = mpsc::channel();
    {
      let mut surface = renderer.ensure(&renderer.playbar, &store);
      surface.as_mut().unwrap().dither = Some(DitherSurface {
        key: MaskKey {
          artwork: "cover".into(),
          width: 16,
          height: 16,
          algorithm: "stucki".into(),
          scale: 1,
          fill: false,
        },
        pending: Some(receiver),
        requested_key: None,
        failed: false,
        mask: None,
        pending_color: None,
        colors: None,
        protocol: None,
      });
    }
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    terminal
      .draw(|frame| {
        renderer.render_surface(
          &renderer.playbar,
          frame,
          Rect::new(0, 0, 8, 4),
          &store,
          &config,
          RenderStyle {
            resize: Resize::Fit(None),
            background: Color::Rgb(2, 3, 4),
            fill: false,
          },
        );
      })
      .unwrap();
    assert!(terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .all(|cell| { cell.symbol() == " " && cell.bg == ratatui::style::Color::Rgb(2, 3, 4) }));
    drop(sender);
    terminal
      .draw(|frame| {
        renderer.render_surface(
          &renderer.playbar,
          frame,
          Rect::new(0, 0, 8, 4),
          &store,
          &config,
          RenderStyle {
            resize: Resize::Fit(None),
            background: Color::Rgb(2, 3, 4),
            fill: false,
          },
        );
      })
      .unwrap();
    assert!(
      lock_surface(&renderer.playbar)
        .as_ref()
        .unwrap()
        .dither
        .as_ref()
        .unwrap()
        .failed
    );
    assert!(terminal.backend().buffer().content().iter().any(|cell| {
      cell.fg == ratatui::style::Color::Rgb(220, 40, 40)
        || cell.bg == ratatui::style::Color::Rgb(220, 40, 40)
    }));
  }

  #[test]
  fn pending_mask_worker_uses_latest_request_for_same_artwork() {
    let mut store = CoverArtStore::default();
    store.store_decoded("cover".into(), DynamicImage::new_rgb8(64, 64));
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let mut config = UserConfig::new();
    config.behavior.cover_art_dither = true;
    let (sender, receiver) = mpsc::channel();
    {
      let mut surface = renderer.ensure(&renderer.playbar, &store);
      surface.as_mut().unwrap().dither = Some(DitherSurface {
        key: MaskKey {
          artwork: "cover".into(),
          width: 16,
          height: 16,
          algorithm: "stucki".into(),
          scale: 1,
          fill: false,
        },
        pending: Some(receiver),
        requested_key: None,
        failed: false,
        mask: None,
        pending_color: None,
        colors: None,
        protocol: None,
      });
    }
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let draw = |terminal: &mut Terminal<TestBackend>, config: &UserConfig| {
      terminal
        .draw(|frame| {
          renderer.render_surface(
            &renderer.playbar,
            frame,
            Rect::new(0, 0, 8, 4),
            &store,
            config,
            RenderStyle {
              resize: Resize::Fit(None),
              background: Color::Rgb(0, 0, 0),
              fill: false,
            },
          );
        })
        .unwrap();
    };

    config.behavior.cover_art_dither_pixel_scale = 2;
    draw(&mut terminal, &config);
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(dither.key.scale, 1);
      assert_eq!(dither.requested_key.as_ref().unwrap().scale, 2);
      assert!(dither.pending.is_some());
    }

    config.behavior.cover_art_dither_pixel_scale = 3;
    config.behavior.cover_art_dither_algorithm = "atkinson".into();
    draw(&mut terminal, &config);
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(dither.key.scale, 1);
      assert_eq!(dither.requested_key.as_ref().unwrap().scale, 3);
      assert_eq!(dither.requested_key.as_ref().unwrap().algorithm, "atkinson");
    }

    sender
      .send(PreparedMask {
        mask: Mask {
          width: 16,
          height: 16,
          pixels: vec![0; 256],
        },
        initial_color: None,
      })
      .unwrap();
    draw(&mut terminal, &config);
    let guard = lock_surface(&renderer.playbar);
    let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
    assert_eq!(dither.key.scale, 3);
    assert_eq!(dither.key.algorithm, "atkinson");
    assert!(dither.pending.is_some());
    assert!(dither.mask.is_none());
  }

  #[test]
  fn tiny_playbar_never_flashes_original_art_while_dithering() {
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "cover".into(),
      DynamicImage::ImageRgb8(RgbImage::from_fn(64, 64, |x, y| {
        if x < 32 && y < 32 {
          Rgb([200, 40, 40])
        } else {
          Rgb([30, 120, 160])
        }
      })),
    );
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let original_renderer = CoverArtRenderer::new(Picker::halfblocks());
    let mut enabled = UserConfig::new();
    enabled.behavior.cover_art_dither = true;
    let disabled = UserConfig::new();
    let mut tiny = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let mut original = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let style = || RenderStyle {
      resize: Resize::Fit(None),
      background: Color::Rgb(0, 0, 0),
      fill: false,
    };
    tiny
      .draw(|frame| {
        renderer.render_surface(
          &renderer.playbar,
          frame,
          Rect::new(0, 0, 8, 4),
          &store,
          &enabled,
          style(),
        )
      })
      .unwrap();
    original
      .draw(|frame| {
        original_renderer.render_surface(
          &original_renderer.playbar,
          frame,
          Rect::new(0, 0, 8, 4),
          &store,
          &disabled,
          style(),
        )
      })
      .unwrap();
    assert_ne!(tiny.backend().buffer(), original.backend().buffer());
    assert!(lock_surface(&renderer.playbar)
      .as_ref()
      .unwrap()
      .dither
      .is_some());
    for _ in 0..100 {
      tiny
        .draw(|frame| {
          renderer.render_surface(
            &renderer.playbar,
            frame,
            Rect::new(0, 0, 8, 4),
            &store,
            &enabled,
            style(),
          )
        })
        .unwrap();
      if lock_surface(&renderer.playbar)
        .as_ref()
        .unwrap()
        .dither
        .as_ref()
        .unwrap()
        .mask
        .is_some()
      {
        break;
      }
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let guard = lock_surface(&renderer.playbar);
    let mask = guard
      .as_ref()
      .unwrap()
      .dither
      .as_ref()
      .unwrap()
      .mask
      .as_ref()
      .unwrap();
    assert_eq!((mask.width, mask.height), (16, 16));
    drop(guard);
    assert_ne!(tiny.backend().buffer(), original.backend().buffer());
    assert!(lock_surface(&renderer.playbar)
      .as_ref()
      .unwrap()
      .dither
      .is_some());
  }

  #[test]
  fn all_surfaces_invalidate_masks_and_recolor_without_redithering() {
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let mut terminal = Terminal::new(TestBackend::new(54, 18)).unwrap();
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "first".into(),
      DynamicImage::ImageRgb8(RgbImage::from_fn(16, 16, |x, y| {
        Rgb([(x * 16) as u8, (y * 16) as u8, 120])
      })),
    );
    let mut config = UserConfig::new();
    config.behavior.cover_art_dither = true;
    config.theme.playbar_background = Color::Rgb(1, 2, 3);
    config.theme.background = Color::Rgb(4, 5, 6);
    let draw = |renderer: &CoverArtRenderer,
                terminal: &mut Terminal<TestBackend>,
                store: &CoverArtStore,
                config: &UserConfig,
                width| {
      terminal
        .draw(|frame| {
          renderer.render_surface(
            &renderer.playbar,
            frame,
            Rect::new(0, 0, width, 8),
            store,
            config,
            RenderStyle {
              resize: Resize::Fit(None),
              background: config.theme.playbar_background,
              fill: false,
            },
          );
          renderer.render_surface(
            &renderer.fullscreen,
            frame,
            Rect::new(18, 0, 16, 8),
            store,
            config,
            RenderStyle {
              resize: Resize::Fit(None),
              background: config.theme.background,
              fill: false,
            },
          );
          renderer.render_surface(
            &renderer.plugin,
            frame,
            Rect::new(36, 0, 16, 8),
            store,
            config,
            RenderStyle {
              resize: Resize::Scale(None),
              background: config.theme.background,
              fill: true,
            },
          );
        })
        .unwrap();
    };
    draw(&renderer, &mut terminal, &store, &config, 16);
    for surface in [&renderer.playbar, &renderer.fullscreen, &renderer.plugin] {
      assert_eq!(
        lock_surface(surface)
          .as_ref()
          .unwrap()
          .dither
          .as_ref()
          .unwrap()
          .key
          .artwork,
        "first"
      );
    }
    for _ in 0..100 {
      draw(&renderer, &mut terminal, &store, &config, 16);
      if [&renderer.playbar, &renderer.fullscreen, &renderer.plugin]
        .iter()
        .all(|surface| {
          lock_surface(surface)
            .as_ref()
            .unwrap()
            .dither
            .as_ref()
            .unwrap()
            .mask
            .is_some()
        })
      {
        break;
      }
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let before = Arc::as_ptr(
      lock_surface(&renderer.playbar)
        .as_ref()
        .unwrap()
        .dither
        .as_ref()
        .unwrap()
        .mask
        .as_ref()
        .unwrap(),
    );
    config.theme.playbar_progress = Color::Rgb(30, 40, 50);
    draw(&renderer, &mut terminal, &store, &config, 16);
    let guard = lock_surface(&renderer.playbar);
    let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
    assert_eq!(Arc::as_ptr(dither.mask.as_ref().unwrap()), before);
    assert_eq!(dither.colors, Some(([30, 40, 50], [1, 2, 3])));
    let rendered_height = dither.mask.as_ref().unwrap().height.div_ceil(4) as u16;
    let rendered_width = dither.mask.as_ref().unwrap().width.div_ceil(2) as u16;
    drop(guard);
    let accent = ratatui::style::Color::Rgb(30, 40, 50);
    let background = ratatui::style::Color::Rgb(1, 2, 3);
    let buffer = terminal.backend().buffer();
    assert!((0..rendered_height)
      .flat_map(|y| (0..rendered_width).map(move |x| (x, y)))
      .any(|position| {
        let cell = buffer.cell(position).unwrap();
        cell.fg == accent || cell.bg == accent
      }));
    for y in 0..rendered_height {
      for x in 0..rendered_width {
        let cell = buffer.cell((x, y)).unwrap();
        assert!(matches!(cell.fg, color if color == accent || color == background));
        assert!(matches!(cell.bg, color if color == accent || color == background));
      }
    }
    draw(&renderer, &mut terminal, &store, &config, 17);
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(
        dither.requested_key.as_ref().unwrap_or(&dither.key).width,
        34
      );
    }
    config.behavior.cover_art_dither_algorithm = "atkinson".into();
    draw(&renderer, &mut terminal, &store, &config, 17);
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(
        dither
          .requested_key
          .as_ref()
          .unwrap_or(&dither.key)
          .algorithm,
        "atkinson"
      );
    }
    config.behavior.cover_art_dither_pixel_scale = 4;
    draw(&renderer, &mut terminal, &store, &config, 17);
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(
        dither.requested_key.as_ref().unwrap_or(&dither.key).scale,
        3
      );
    }
    store.store_decoded("second".into(), DynamicImage::new_rgb8(16, 16));
    draw(&renderer, &mut terminal, &store, &config, 17);
    for surface in [&renderer.playbar, &renderer.fullscreen, &renderer.plugin] {
      assert_eq!(
        lock_surface(surface)
          .as_ref()
          .unwrap()
          .dither
          .as_ref()
          .unwrap()
          .key
          .artwork,
        "second"
      );
    }
  }
}
