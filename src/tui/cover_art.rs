//! Terminal rendering for the decoded cover art held in
//! `core::art::CoverArtStore`. This is the only place the store's image meets
//! `ratatui-image`: a process-wide renderer owns the `Picker` (probed once in
//! `start_ui`, after `ratatui::init()`, so `App` construction never touches
//! stdout) and caches one `StatefulProtocol` per drawing surface, keyed by the
//! store's key — a protocol is rebuilt when the art changes, never per frame.

use crate::core::art::{CoverArtStatus, CoverArtStore};
use crate::core::plugin_api::PluginCoverArtFit;
use log::{info, warn};
use ratatui::{
  layout::{Rect, Size},
  Frame,
};
use ratatui_image::{
  picker::{Picker, ProtocolType},
  protocol::StatefulProtocol,
  Resize, StatefulImage,
};
use std::sync::{Mutex, MutexGuard, OnceLock};

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
    let mut lock = surface.lock().unwrap();
    let stale = lock
      .as_ref()
      .is_some_and(|surface| store.key() != Some(surface.key.as_str()));
    if stale {
      *lock = None;
    }
  }
}

/// Render into the playbar's cover slot.
pub fn render(f: &mut Frame, area: Rect, store: &CoverArtStore) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(&renderer.playbar, f, area, Resize::Fit(None), store);
  }
}

/// Measure the playbar image's fitted size, so the layout can size its slot.
pub fn size_for(area: Rect, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.playbar, area, Resize::Fit(None), store)
}

/// Render into the fullscreen cover art view.
pub fn render_fullscreen(f: &mut Frame, area: Rect, store: &CoverArtStore) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(&renderer.fullscreen, f, area, Resize::Fit(None), store);
  }
}

/// Measure the fullscreen image's fitted size, so the caller can center it.
pub fn fullscreen_size_for(area: Rect, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.fullscreen, area, Resize::Fit(None), store)
}

/// Render into a plugin screen's `cover_art` widget slot.
pub fn render_plugin(f: &mut Frame, area: Rect, fit: PluginCoverArtFit, store: &CoverArtStore) {
  if let Some(renderer) = RENDERER.get() {
    renderer.render_surface(&renderer.plugin, f, area, fit.resize(), store);
  }
}

/// Measure the plugin slot's fitted size, so the caller can center it.
pub fn plugin_size_for(area: Rect, fit: PluginCoverArtFit, store: &CoverArtStore) -> Option<Rect> {
  let renderer = RENDERER.get()?;
  renderer.surface_size_for(&renderer.plugin, area, fit.resize(), store)
}

struct CoverArtRenderer {
  picker: Picker,
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
}

impl CoverArtRenderer {
  fn new(picker: Picker) -> Self {
    Self {
      picker,
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

  /// Reconcile a surface's cached protocol with the store: rebuild it when the
  /// store holds art under a different key, drop it when the store is empty.
  /// This is the per-key cache that keeps a protocol from being rebuilt for a
  /// frame that renders the same art.
  fn ensure<'a>(
    &self,
    surface: &'a Mutex<Option<Surface>>,
    store: &CoverArtStore,
  ) -> MutexGuard<'a, Option<Surface>> {
    let mut lock = surface.lock().unwrap();
    match store.key() {
      None => *lock = None,
      Some(key) => {
        let cached = lock.as_ref().is_some_and(|surface| surface.key == key);
        if !cached {
          if let Some(image) = store.image() {
            *lock = Some(Surface {
              key: key.to_string(),
              protocol: self.picker.new_resize_protocol(image.clone()),
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
    resize: Resize,
    store: &CoverArtStore,
  ) {
    let mut lock = self.ensure(surface, store);
    if let Some(surface) = lock.as_mut() {
      f.render_stateful_widget(
        StatefulImage::new().resize(resize),
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
