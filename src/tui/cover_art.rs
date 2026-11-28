//! Terminal cover-art rendering and per-surface dither caches.

use crate::core::art::CoverArtStore;
use crate::core::plugin_api::PluginCoverArtFit;
use crate::core::theme::{resolve, Color, Palette};
use crate::core::user_config::UserConfig;
use log::{info, warn};
use ratatui::{
  layout::{Rect, Size},
  Frame,
};
use ratatui_image::{
  picker::{cap_parser::QueryStdioOptions, Capability, Picker, ProtocolType},
  protocol::StatefulProtocol,
  FontSize, Resize, StatefulImage,
};
use std::cell::Cell;
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

mod dither;
use dither::{Algorithm, Mask};

const DEFAULT_FADE: Duration = Duration::from_millis(40);
const MAX_FADE: Duration = Duration::from_millis(40);
const FADE_LEVELS: u8 = u8::MAX;
const HOLD_OPACITY: f32 = 0.125;

fn fade_level(opacity: f32) -> u8 {
  (opacity.clamp(0.0, 1.0) * f32::from(FADE_LEVELS)).round() as u8
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
    let mut options = QueryStdioOptions::default();
    let iterm2 = is_iterm2(
      std::env::var("TERM_PROGRAM").ok().as_deref(),
      std::env::var("LC_TERMINAL").ok().as_deref(),
      tmux_client_termtype().as_deref(),
    );
    if iterm2 {
      options.blacklist_protocols = vec![ProtocolType::Kitty, ProtocolType::Sixel];
    }
    let (picker, measured_cell) = match Picker::from_query_stdio_with_options(options) {
      Ok(mut picker) => {
        let measured = picker
          .capabilities()
          .iter()
          .any(|cap| matches!(cap, Capability::CellSize(Some(_))));
        let device_cell = (iterm2 && !picker.tmux_detected())
          .then(iterm2_device_cell_size)
          .flatten();
        if let Some(cell) = device_cell {
          #[allow(deprecated)]
          let sized = Picker::from_fontsize(cell);
          picker = sized;
          picker.set_protocol_type(ProtocolType::Iterm2);
        } else if iterm2 && measured {
          picker.set_protocol_type(ProtocolType::Iterm2);
        }
        (picker, measured || device_cell.is_some())
      }
      Err(err) => {
        warn!("cover art renderer fallback to halfblocks: {err}");
        (Picker::halfblocks(), false)
      }
    };

    let cell = picker.font_size();
    info!(
      "cover art renderer detected a {:?} backend, cell {}x{} px",
      picker.protocol_type(),
      cell.width,
      cell.height
    );
    let renderer = CoverArtRenderer::new(picker);
    if measured_cell {
      renderer.with_measured_cell_ratio()
    } else {
      renderer
    }
  });
}

/// iTerm2 reports cells in points but draws one inline image pixel per device pixel.
fn iterm2_device_cell_size() -> Option<FontSize> {
  use std::io::{Read, Write};
  let mut stdout = std::io::stdout();
  stdout
    .write_all(b"\x1b]1337;ReportCellSize\x07\x1b[5n")
    .ok()?;
  stdout.flush().ok()?;
  let (sender, receiver) = mpsc::channel();
  std::thread::spawn(move || {
    let mut reply = Vec::new();
    let mut chunk = [0_u8; 64];
    while let Ok(read @ 1..) = std::io::stdin().read(&mut chunk) {
      reply.extend_from_slice(&chunk[..read]);
      let done = reply.windows(4).any(|window| window == b"\x1b[0n");
      // A failed send means the wait timed out: stop before more keys are read.
      if sender.send(done.then(|| reply.clone())).is_err() || done {
        break;
      }
    }
  });
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
  let reply = loop {
    let left = deadline.checked_duration_since(std::time::Instant::now())?;
    if let Some(reply) = receiver.recv_timeout(left).ok()? {
      break reply;
    }
  };
  parse_report_cell_size(&String::from_utf8_lossy(&reply))
}

/// The device-pixel cell from `ReportCellSize=height;width[;scale]`, whose sizes are in points.
fn parse_report_cell_size(reply: &str) -> Option<FontSize> {
  let fields = reply.split("ReportCellSize=").nth(1)?;
  let fields = fields.split(['\x07', '\x1b']).next()?;
  let mut values = fields.split(';').map(|value| value.parse::<f64>().ok());
  let height = values.next()??;
  let width = values.next()??;
  let scale = values.next().flatten().unwrap_or(1.0);
  let pixels = |points: f64| {
    let pixels = (points * scale).round();
    (1.0..=f64::from(u16::MAX))
      .contains(&pixels)
      .then_some(pixels as u16)
  };
  Some(FontSize::new(pixels(width)?, pixels(height)?))
}

/// iTerm2 answers the Kitty query but cannot draw Kitty placeholders, so skip Kitty and Sixel.
fn is_iterm2(
  term_program: Option<&str>,
  lc_terminal: Option<&str>,
  tmux_client: Option<&str>,
) -> bool {
  if let Some(client) = tmux_client {
    return client.contains("iTerm");
  }
  [term_program, lc_terminal]
    .into_iter()
    .flatten()
    .any(|name| name.contains("iTerm"))
}

/// The attached tmux client's terminal, which stays current when `LC_TERMINAL` goes stale.
fn tmux_client_termtype() -> Option<String> {
  std::env::var_os("TMUX")?;
  let output = std::process::Command::new("tmux")
    .args(["display-message", "-p", "#{client_termtype}"])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .output()
    .ok()?;
  let termtype = String::from_utf8_lossy(&output.stdout).trim().to_string();
  (!termtype.is_empty()).then_some(termtype)
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

/// Height-to-width ratio of one terminal cell, from the cell size measured at
/// startup. `None` for the halfblocks fallback or when the terminal did not
/// report its cell size; callers then fall back to their own estimate.
pub fn cell_ratio() -> Option<f32> {
  RENDERER.get().and_then(|renderer| renderer.cell_ratio)
}

pub fn fade_active() -> bool {
  RENDERER.get().is_some_and(CoverArtRenderer::fade_active)
}

/// Drop cached protocols that no longer match the store, so cleared art
/// releases its memory even while no cover surface is being drawn. Called once
/// per frame by the runner; rebuilds happen lazily at the next render.
pub fn sync(
  playing: &CoverArtStore,
  browse_preview: &CoverArtStore,
  now: Instant,
  fade_browse_preview: bool,
) {
  let Some(renderer) = RENDERER.get() else {
    return;
  };
  for (surface, store, allow_fade) in [
    (&renderer.playbar, playing, false),
    (&renderer.pane_playbar, playing, false),
    (&renderer.fullscreen, playing, false),
    (&renderer.now_playing, playing, false),
    (&renderer.plugin, playing, false),
    (
      &renderer.browse_preview,
      browse_preview,
      fade_browse_preview,
    ),
  ] {
    renderer.sync_surface(surface, store, now, allow_fade);
  }
}

thread_local! {
  static IN_PANE_PLAYBAR: Cell<bool> = const { Cell::new(false) };
}

/// Run `draw` with the playbar art on the Mini player pane's own surface.
pub fn in_pane_playbar<R>(draw: impl FnOnce() -> R) -> R {
  IN_PANE_PLAYBAR.with(|flag| flag.set(true));
  let result = draw();
  IN_PANE_PLAYBAR.with(|flag| flag.set(false));
  result
}

/// Render into the playbar's cover slot.
pub fn render(f: &mut Frame, area: Rect, store: &CoverArtStore, config: &UserConfig) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(
      renderer.playbar_surface(),
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
  renderer.surface_size_for(renderer.playbar_surface(), area, Resize::Fit(None), store)
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

/// Render into the now playing panel's cover slot.
pub fn render_now_playing(f: &mut Frame, area: Rect, store: &CoverArtStore, config: &UserConfig) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(
      &renderer.now_playing,
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

/// Measure the now playing panel image's fitted size, so the caller can center it.
pub fn now_playing_size_for(area: Rect, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.now_playing, area, Resize::Fit(None), store)
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

fn browse_preview_resize() -> Resize {
  Resize::Scale(Some(image::imageops::FilterType::Lanczos3))
}

fn browse_preview_render_style(config: &UserConfig) -> RenderStyle {
  RenderStyle {
    resize: browse_preview_resize(),
    background: config.theme.background,
    fill: true,
  }
}

pub fn render_browse_preview(
  f: &mut Frame,
  area: Rect,
  store: &CoverArtStore,
  config: &UserConfig,
) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(
      &renderer.browse_preview,
      f,
      area,
      store,
      config,
      browse_preview_render_style(config),
    );
  }
}

pub fn browse_preview_size_for(area: Rect, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(
    &renderer.browse_preview,
    area,
    browse_preview_resize(),
    store,
  )
}

struct CoverArtRenderer {
  picker: Picker,
  terminal_background: Option<[u8; 3]>,
  /// Measured cell height / width; see [`cell_ratio`].
  cell_ratio: Option<f32>,
  /// Playbar protocol state.
  playbar: Mutex<Option<Surface>>,
  /// The Mini player pane's playbar, drawn in the same frame as the main playbar.
  pane_playbar: Mutex<Option<Surface>>,
  /// Separate protocol state for the fullscreen cover art view, avoiding
  /// conflicts when the same image is rendered in both the playbar and
  /// fullscreen in one frame.
  fullscreen: Mutex<Option<Surface>>,
  /// The now playing panel beside the main content, drawn in the same frame
  /// as the playbar and possibly a cover art pane, each at its own size.
  now_playing: Mutex<Option<Surface>>,
  /// Separate protocol state for a plugin screen's `cover_art` widget, for the
  /// same reason: a plugin can size its image differently from the playbar, and
  /// a shared protocol would re-encode on every switch between them.
  plugin: Mutex<Option<Surface>>,
  browse_preview: Mutex<Option<Surface>>,
}

/// A surface's cached protocol together with the store key it was built from.
struct Surface {
  key: String,
  protocol: StatefulProtocol,
  source: Arc<image::DynamicImage>,
  fade: Option<FadeState>,
  snap_generation: u64,
  dither: Option<DitherSurface>,
}

struct FadeState {
  target: String,
  started: Instant,
  duration: Duration,
  step: u8,
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

fn faded_image(
  source: &image::DynamicImage,
  opacity: f32,
  background: [u8; 3],
) -> image::DynamicImage {
  let mut rgba = source.to_rgba8();
  for pixel in rgba.pixels_mut() {
    let alpha = f32::from(pixel[3]) / 255.0 * opacity;
    for channel in 0..3 {
      let value =
        f32::from(background[channel]) * (1.0 - alpha) + f32::from(pixel[channel]) * alpha;
      pixel[channel] = value.round().clamp(0.0, 255.0) as u8;
    }
    pixel[3] = 255;
  }
  image::DynamicImage::ImageRgba8(rgba)
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

struct RenderMoment {
  style: RenderStyle,
  now: Instant,
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
      cell_ratio: None,
      playbar: Mutex::new(None),
      pane_playbar: Mutex::new(None),
      fullscreen: Mutex::new(None),
      now_playing: Mutex::new(None),
      plugin: Mutex::new(None),
      browse_preview: Mutex::new(None),
    }
  }

  /// Record the picker's cell size as measured. Only image protocols draw
  /// whole pixels per cell, so halfblocks keeps `None`.
  fn with_measured_cell_ratio(mut self) -> Self {
    let font = self.picker.font_size();
    if self.full_image_support() && font.width > 0 && font.height > 0 {
      self.cell_ratio = Some(f32::from(font.height) / f32::from(font.width));
    }
    self
  }

  fn playbar_surface(&self) -> &Mutex<Option<Surface>> {
    if IN_PANE_PLAYBAR.with(Cell::get) {
      &self.pane_playbar
    } else {
      &self.playbar
    }
  }

  fn full_image_support(&self) -> bool {
    match self.picker.protocol_type() {
      ProtocolType::Kitty | ProtocolType::Iterm2 | ProtocolType::Sixel => true,
      ProtocolType::Halfblocks => false,
    }
  }

  fn fade_active(&self) -> bool {
    lock_surface(&self.browse_preview)
      .as_ref()
      .and_then(|surface| surface.fade.as_ref())
      .is_some_and(|fade| {
        fade.step > fade_level(HOLD_OPACITY) && fade.started.elapsed() < fade.duration
      })
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

  fn sync_surface(
    &self,
    surface: &Mutex<Option<Surface>>,
    store: &CoverArtStore,
    now: Instant,
    allow_fade: bool,
  ) {
    let mut lock = lock_surface(surface);
    let snap_generation = store.snap_generation();
    if lock
      .as_ref()
      .is_some_and(|surface| surface.snap_generation != snap_generation)
    {
      let clear = {
        let current = lock.as_mut().expect("surface");
        current.fade = None;
        let clear = self.install_current(current, store);
        current.snap_generation = snap_generation;
        clear
      };
      if clear {
        *lock = None;
        return;
      }
    }
    if !allow_fade && lock.as_ref().is_some_and(|surface| surface.fade.is_some()) {
      let clear = {
        let current = lock.as_mut().expect("surface");
        current.fade = None;
        self.install_current(current, store)
      };
      if clear {
        *lock = None;
        return;
      }
    }
    let transition = if allow_fade { store.transition() } else { None }
      .or_else(|| store.key().map(|key| (key, now)));
    if let (Some((key, started)), Some(current)) = (transition, lock.as_mut()) {
      let needs_fade = match current.fade.as_ref() {
        Some(fade) => fade.target != key,
        None => current.key != key,
      };
      if needs_fade {
        if allow_fade {
          self.begin_fade(current, store, key, started);
        } else {
          let clear = self.install_current(current, store);
          if clear {
            *lock = None;
            return;
          }
        }
      }
    }
    let target_ready = lock.as_ref().is_some_and(|surface| {
      surface
        .fade
        .as_ref()
        .is_some_and(|fade| fade.target.is_empty() || store.key() == Some(fade.target.as_str()))
    });
    if target_ready {
      let clear = {
        let current = lock.as_mut().expect("surface");
        current.fade = None;
        self.install_current(current, store)
      };
      if clear {
        *lock = None;
        return;
      }
    }
    let stale = lock
      .as_ref()
      .is_some_and(|surface| surface.fade.is_none() && store.key() != Some(surface.key.as_str()));
    if stale {
      *lock = None;
    }
  }

  fn begin_fade(&self, surface: &mut Surface, store: &CoverArtStore, key: &str, started: Instant) {
    let duration = store
      .average_load_duration()
      .unwrap_or(DEFAULT_FADE)
      .min(MAX_FADE)
      .max(Duration::from_millis(1));
    surface.fade = Some(FadeState {
      target: key.to_string(),
      started,
      duration,
      step: FADE_LEVELS,
    });
    surface.dither = None;
  }

  fn ensure<'a>(
    &self,
    surface: &'a Mutex<Option<Surface>>,
    store: &CoverArtStore,
  ) -> MutexGuard<'a, Option<Surface>> {
    self.ensure_at(surface, store, Instant::now(), false)
  }

  fn ensure_at<'a>(
    &self,
    surface: &'a Mutex<Option<Surface>>,
    store: &CoverArtStore,
    now: Instant,
    allow_fade: bool,
  ) -> MutexGuard<'a, Option<Surface>> {
    let mut lock = lock_surface(surface);
    match store.key() {
      None => {
        if !lock.as_ref().is_some_and(|surface| surface.fade.is_some()) {
          *lock = None;
        }
      }
      Some(key) => {
        let cached = lock
          .as_ref()
          .is_some_and(|surface| surface.fade.is_some() || surface.key == key);
        if !cached {
          if let Some(current) = lock.as_mut() {
            if allow_fade {
              self.begin_fade(current, store, key, now);
            } else {
              let clear = self.install_current(current, store);
              if clear {
                *lock = None;
                return lock;
              }
            }
          } else if let Some(image) = store.image_shared() {
            *lock = Some(Surface {
              key: key.to_string(),
              protocol: self.picker.new_resize_protocol((*image).clone()),
              source: image,
              fade: None,
              snap_generation: store.snap_generation(),
              dither: None,
            });
          }
        } else if let Some(current) = lock.as_mut() {
          if current.fade.is_none() && current.key != key {
            if allow_fade {
              self.begin_fade(current, store, key, now);
            } else {
              let clear = self.install_current(current, store);
              if clear {
                *lock = None;
                return lock;
              }
            }
          }
        }
      }
    }
    lock
  }

  fn background_rgb(&self, color: Color) -> [u8; 3] {
    let palette = Palette {
      reset: self.terminal_background.unwrap_or([0, 0, 0]),
      ..Palette::default()
    };
    resolve(color, &palette)
  }

  fn install_current(&self, surface: &mut Surface, store: &CoverArtStore) -> bool {
    let Some(key) = store.key() else {
      return true;
    };
    if let Some(image) = store.image_shared() {
      surface.key = key.to_string();
      surface.source = Arc::clone(&image);
      surface.protocol = self.picker.new_resize_protocol((*image).clone());
      surface.dither = None;
      false
    } else {
      true
    }
  }

  fn step_fade(
    &self,
    surface: &mut Surface,
    store: &CoverArtStore,
    now: Instant,
    background: [u8; 3],
  ) -> bool {
    let (target_ready, started, duration, step) = {
      let fade = surface.fade.as_ref().expect("fade state");
      (
        fade.target.is_empty() || store.key() == Some(fade.target.as_str()),
        fade.started,
        fade.duration,
        fade.step,
      )
    };
    let elapsed = now.saturating_duration_since(started);
    if target_ready {
      surface.fade = None;
      return self.install_current(surface, store);
    }
    let hold_level = fade_level(HOLD_OPACITY);
    if elapsed >= duration {
      if step != hold_level {
        let opacity = f32::from(hold_level) / f32::from(FADE_LEVELS);
        let image = faded_image(&surface.source, opacity, background);
        surface.protocol = self.picker.new_resize_protocol(image);
        surface.fade.as_mut().expect("fade state").step = hold_level;
      }
      return false;
    }
    let progress = (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0);
    let next_step = fade_level(1.0 - progress).max(hold_level);
    if next_step != step {
      let opacity = f32::from(next_step) / f32::from(FADE_LEVELS);
      let image = faded_image(&surface.source, opacity, background);
      surface.protocol = self.picker.new_resize_protocol(image);
      surface.fade.as_mut().expect("fade state").step = next_step;
    }
    false
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
    self.render_surface_at(
      surface,
      f,
      area,
      store,
      config,
      RenderMoment {
        style,
        now: Instant::now(),
      },
    );
  }

  fn render_surface_at(
    &self,
    surface: &Mutex<Option<Surface>>,
    f: &mut Frame,
    area: Rect,
    store: &CoverArtStore,
    config: &UserConfig,
    moment: RenderMoment,
  ) {
    let RenderMoment { style, now } = moment;
    let allow_fade =
      std::ptr::eq(surface, &self.browse_preview) && config.behavior.fade_song_selection_cover_art;
    let mut lock = self.ensure_at(surface, store, now, allow_fade);
    if !allow_fade && lock.as_ref().is_some_and(|surface| surface.fade.is_some()) {
      let clear = {
        let current = lock.as_mut().expect("surface");
        current.fade = None;
        self.install_current(current, store)
      };
      if clear {
        *lock = None;
        return;
      }
    }
    if lock.as_ref().is_some_and(|surface| surface.fade.is_some()) {
      let background = self.background_rgb(style.background);
      let clear = {
        let current = lock.as_mut().expect("surface");
        self.step_fade(current, store, now, background)
      };
      if clear {
        *lock = None;
        return;
      }
      let current = lock.as_mut().expect("surface");
      f.render_stateful_widget(
        StatefulImage::new().resize(style.resize),
        area,
        &mut current.protocol,
      );
      return;
    }
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

  fn is_reddish(color: ratatui::style::Color) -> bool {
    matches!(color, ratatui::style::Color::Rgb(r, g, b) if r > 150 && g < 50 && b < 50)
  }

  #[test]
  fn iterm2_is_found_by_term_program_or_by_lc_terminal_inside_tmux() {
    assert!(is_iterm2(Some("iTerm.app"), None, None));
    assert!(is_iterm2(Some("tmux"), Some("iTerm2"), None));
    assert!(is_iterm2(
      Some("tmux"),
      Some("iTerm2"),
      Some("iTerm2 3.5.10")
    ));
  }

  #[test]
  fn other_terminals_are_not_taken_for_iterm2() {
    assert!(!is_iterm2(Some("ghostty"), None, None));
    assert!(!is_iterm2(Some("tmux"), None, None));
    assert!(!is_iterm2(None, None, None));
  }

  fn cell(reply: &str) -> Option<(u16, u16)> {
    parse_report_cell_size(reply).map(|size| (size.width, size.height))
  }

  #[test]
  fn iterm2_cell_points_are_scaled_to_device_pixels() {
    let reply = "\x1b]1337;ReportCellSize=17.0;8.0;2.0\x1b\\\x1b[0n";
    assert_eq!(cell(reply), Some((16, 34)));
  }

  #[test]
  fn an_iterm2_reply_without_a_scale_keeps_the_points() {
    let reply = "\x1b]1337;ReportCellSize=17.5;8.25\x07\x1b[0n";
    assert_eq!(cell(reply), Some((8, 18)));
  }

  #[test]
  fn a_terminal_without_report_cell_size_gives_no_cell() {
    assert_eq!(cell("\x1b[0n"), None);
    assert_eq!(cell("\x1b]1337;ReportCellSize=0;0;2\x07"), None);
  }

  #[test]
  fn a_stale_iterm_env_yields_to_the_attached_tmux_client() {
    assert!(!is_iterm2(
      Some("tmux"),
      Some("iTerm2"),
      Some("kitty(0.36.4)")
    ));
  }

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
                width,
                now: Instant| {
      terminal
        .draw(|frame| {
          renderer.render_surface_at(
            &renderer.playbar,
            frame,
            Rect::new(0, 0, width, 8),
            store,
            config,
            RenderMoment {
              style: RenderStyle {
                resize: Resize::Fit(None),
                background: config.theme.playbar_background,
                fill: false,
              },
              now,
            },
          );
          renderer.render_surface_at(
            &renderer.fullscreen,
            frame,
            Rect::new(18, 0, 16, 8),
            store,
            config,
            RenderMoment {
              style: RenderStyle {
                resize: Resize::Fit(None),
                background: config.theme.background,
                fill: false,
              },
              now,
            },
          );
          renderer.render_surface_at(
            &renderer.plugin,
            frame,
            Rect::new(36, 0, 16, 8),
            store,
            config,
            RenderMoment {
              style: RenderStyle {
                resize: Resize::Scale(None),
                background: config.theme.background,
                fill: true,
              },
              now,
            },
          );
        })
        .unwrap();
    };
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      16,
      Instant::now(),
    );
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
      draw(
        &renderer,
        &mut terminal,
        &store,
        &config,
        16,
        Instant::now(),
      );
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
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      16,
      Instant::now(),
    );
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
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      17,
      Instant::now(),
    );
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(
        dither.requested_key.as_ref().unwrap_or(&dither.key).width,
        34
      );
    }
    config.behavior.cover_art_dither_algorithm = "atkinson".into();
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      17,
      Instant::now(),
    );
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
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      17,
      Instant::now(),
    );
    {
      let guard = lock_surface(&renderer.playbar);
      let dither = guard.as_ref().unwrap().dither.as_ref().unwrap();
      assert_eq!(
        dither.requested_key.as_ref().unwrap_or(&dither.key).scale,
        3
      );
    }
    store.store_decoded("second".into(), DynamicImage::new_rgb8(16, 16));
    let fade_start = Instant::now();
    draw(&renderer, &mut terminal, &store, &config, 17, fade_start);
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      17,
      fade_start + DEFAULT_FADE + Duration::from_secs(1),
    );
    draw(
      &renderer,
      &mut terminal,
      &store,
      &config,
      17,
      Instant::now(),
    );
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

  #[test]
  fn changing_art_fades_out_for_the_average_load_time_then_pops_in_the_new_image() {
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "old".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([200, 0, 0]))),
    );
    store.note_fetch_started();
    std::thread::sleep(Duration::from_millis(30));
    store.note_fetch_finished(true);
    let fade_duration = store.average_load_duration().unwrap();
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let config = UserConfig::new();
    let style = RenderStyle {
      resize: Resize::Fit(None),
      background: Color::Rgb(0, 0, 0),
      fill: false,
    };
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let start = Instant::now();
    let draw = |terminal: &mut Terminal<TestBackend>, store: &CoverArtStore, now: Instant| {
      terminal
        .draw(|frame| {
          renderer.render_surface_at(
            &renderer.browse_preview,
            frame,
            Rect::new(0, 0, 8, 4),
            store,
            &config,
            RenderMoment {
              style: RenderStyle {
                resize: style.resize.clone(),
                background: style.background,
                fill: style.fill,
              },
              now,
            },
          )
        })
        .unwrap();
    };

    draw(&mut terminal, &store, start);
    assert!(!renderer.fade_active());
    store.begin_fetch("new");
    let (_, requested) = store.transition().unwrap();
    renderer.sync_surface(&renderer.browse_preview, &store, requested, true);
    draw(&mut terminal, &store, requested + Duration::from_millis(1));
    assert_eq!(
      lock_surface(&renderer.browse_preview)
        .as_ref()
        .unwrap()
        .fade
        .as_ref()
        .unwrap()
        .duration,
      fade_duration
    );
    assert!(terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .any(|cell| is_reddish(cell.fg) || is_reddish(cell.bg)));

    store.store_decoded(
      "new".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([0, 0, 200]))),
    );
    renderer.sync_surface(
      &renderer.browse_preview,
      &store,
      requested + Duration::from_millis(1),
      true,
    );
    draw(&mut terminal, &store, requested + Duration::from_millis(1));
    assert!(!renderer.fade_active());
    assert!(terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .any(|cell| cell.fg == ratatui::style::Color::Rgb(0, 0, 200)
        || cell.bg == ratatui::style::Color::Rgb(0, 0, 200)));
  }

  #[test]
  fn fade_uses_fine_grained_opacity_levels() {
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "old".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([200, 0, 0]))),
    );
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let config = UserConfig::new();
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let start = Instant::now();
    let draw = |terminal: &mut Terminal<TestBackend>, store: &CoverArtStore, now: Instant| {
      terminal
        .draw(|frame| {
          renderer.render_surface_at(
            &renderer.browse_preview,
            frame,
            Rect::new(0, 0, 8, 4),
            store,
            &config,
            RenderMoment {
              style: RenderStyle {
                resize: Resize::Fit(None),
                background: Color::Rgb(0, 0, 0),
                fill: false,
              },
              now,
            },
          )
        })
        .unwrap();
    };

    draw(&mut terminal, &store, start);
    store.begin_fetch("new");
    let (_, requested) = store.transition().unwrap();
    renderer.sync_surface(&renderer.browse_preview, &store, requested, true);

    let mut levels = Vec::new();
    for index in 0u32..16 {
      let at = requested + DEFAULT_FADE * index / 16;
      draw(&mut terminal, &store, at);
      let level = lock_surface(&renderer.browse_preview)
        .as_ref()
        .and_then(|surface| surface.fade.as_ref())
        .expect("fade still running")
        .step;
      levels.push(level);
    }

    assert!(levels.windows(2).all(|pair| pair[0] >= pair[1]));
    assert!(
      levels
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        >= 14
    );
  }

  #[test]
  fn fade_starts_when_the_fetch_starts_not_when_the_image_arrives() {
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "old".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([200, 0, 0]))),
    );
    store.note_fetch_started();
    std::thread::sleep(Duration::from_millis(30));
    store.note_fetch_finished(true);
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let config = UserConfig::new();
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let start = Instant::now();
    let draw = |terminal: &mut Terminal<TestBackend>, store: &CoverArtStore, now: Instant| {
      terminal
        .draw(|frame| {
          renderer.render_surface_at(
            &renderer.browse_preview,
            frame,
            Rect::new(0, 0, 8, 4),
            store,
            &config,
            RenderMoment {
              style: RenderStyle {
                resize: Resize::Fit(None),
                background: Color::Rgb(0, 0, 0),
                fill: false,
              },
              now,
            },
          )
        })
        .unwrap();
    };

    draw(&mut terminal, &store, start);
    store.begin_fetch("new");
    let (_, requested) = store.transition().unwrap();
    renderer.sync_surface(&renderer.browse_preview, &store, requested, true);
    assert!(renderer.fade_active());
    assert_eq!(store.key(), Some("old"));
    draw(&mut terminal, &store, requested + Duration::from_millis(1));
    assert!(terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .any(|cell| is_reddish(cell.fg) || is_reddish(cell.bg)));

    store.store_decoded(
      "new".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([0, 0, 200]))),
    );
    renderer.sync_surface(
      &renderer.browse_preview,
      &store,
      requested + Duration::from_millis(2),
      true,
    );
    draw(&mut terminal, &store, requested + Duration::from_millis(2));
    assert!(!renderer.fade_active());
    assert!(terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .any(|cell| cell.fg == ratatui::style::Color::Rgb(0, 0, 200)
        || cell.bg == ratatui::style::Color::Rgb(0, 0, 200)));
  }

  #[test]
  fn fade_holds_while_the_new_image_is_still_loading() {
    let mut store = CoverArtStore::default();
    store.store_decoded(
      "old".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([200, 0, 0]))),
    );
    store.note_fetch_started();
    std::thread::sleep(Duration::from_millis(30));
    store.note_fetch_finished(true);
    let fade_duration = store.average_load_duration().unwrap();
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let config = UserConfig::new();
    let mut terminal = Terminal::new(TestBackend::new(8, 4)).unwrap();
    let now = Instant::now();
    let draw = |terminal: &mut Terminal<TestBackend>, store: &CoverArtStore, now: Instant| {
      terminal
        .draw(|frame| {
          renderer.render_surface_at(
            &renderer.browse_preview,
            frame,
            Rect::new(0, 0, 8, 4),
            store,
            &config,
            RenderMoment {
              style: RenderStyle {
                resize: Resize::Fit(None),
                background: Color::Rgb(0, 0, 0),
                fill: false,
              },
              now,
            },
          )
        })
        .unwrap();
    };

    draw(&mut terminal, &store, now);
    store.begin_fetch("new");
    let (_, requested) = store.transition().unwrap();
    renderer.sync_surface(&renderer.browse_preview, &store, requested, true);
    renderer.sync_surface(
      &renderer.browse_preview,
      &store,
      requested + fade_duration,
      true,
    );
    draw(
      &mut terminal,
      &store,
      requested + fade_duration + Duration::from_millis(1),
    );
    assert!(!renderer.fade_active(), "the hold frame does not animate");
    let held = terminal.backend().buffer().clone();
    let next = requested + fade_duration + Duration::from_millis(2);
    store.begin_transition_at("third", next);
    renderer.sync_surface(&renderer.browse_preview, &store, next, true);
    draw(&mut terminal, &store, next + fade_duration);
    assert_eq!(
      terminal.backend().buffer(),
      &held,
      "another transition must hold at the same brightness as the first"
    );
    assert!(!renderer.fade_active());
    assert!(!terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .any(|cell| is_reddish(cell.fg) || is_reddish(cell.bg)));

    store.store_decoded(
      "third".into(),
      DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, Rgb([0, 0, 200]))),
    );
    renderer.sync_surface(&renderer.browse_preview, &store, next + fade_duration, true);
    draw(&mut terminal, &store, next + fade_duration);
    assert!(!renderer.fade_active());
    assert!(terminal
      .backend()
      .buffer()
      .content()
      .iter()
      .any(|cell| cell.fg == ratatui::style::Color::Rgb(0, 0, 200)
        || cell.bg == ratatui::style::Color::Rgb(0, 0, 200)));
  }

  #[test]
  fn browse_preview_scales_small_art_to_fill_the_slot() {
    let mut store = CoverArtStore::default();
    store.store_decoded("small".into(), DynamicImage::new_rgb8(8, 8));
    let renderer = CoverArtRenderer::new(Picker::halfblocks());
    let area = Rect::new(0, 0, 20, 10);

    let fitted = renderer
      .surface_size_for(
        &renderer.browse_preview,
        area,
        browse_preview_resize(),
        &store,
      )
      .unwrap();

    assert_eq!(fitted.width, area.width);
    assert_eq!(fitted.height, area.height);
  }

  #[test]
  fn browse_preview_render_style_fills_the_dither_mask() {
    assert!(browse_preview_render_style(&UserConfig::new()).fill);
  }
}
