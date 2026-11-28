//! The MilkDrop visualizer style: tidewarp's built-in presets, drawn as real pixels where
//! the terminal has the Kitty or Sixel protocol, and as half-blocks elsewhere.
//!
//! The engine lives in a thread-local, like the retro styles' animation state, because
//! draw functions only see `&App`. The runner feeds it the capture's new samples, and
//! writes the encoded image with [`flush`] after ratatui drew the frame.

use std::cell::RefCell;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossterm::{cursor::MoveTo, queue, terminal};
use ratatui::{
  buffer::{Buffer, CellDiffOption},
  layout::Rect,
  style::Color,
};
use tidewarp::{
  presets, presets::BuiltinPreset, Config, Engine, PresetId, TextureFolder, Transition,
};
use tidewarp_term::{fit, kitty, sixel, Protocol};

use crate::core::paths;
use crate::infra::audio::RecentAudio;
use crate::tui::event::Key;

/// Seconds each preset plays before the next one blends in.
const SECONDS_PER_PRESET: f64 = 30.0;
const BLEND_SECONDS: f64 = 5.0;
/// The id of the MilkDrop Kitty image, apart from the cover art's images.
const KITTY_ID: u32 = 0x6d69_6c6b;

/// Where a preset's text comes from.
enum Source {
  Builtin(&'static BuiltinPreset),
  File(PathBuf),
}

struct Entry {
  name: String,
  source: Source,
}

impl Entry {
  fn text(&self) -> Option<String> {
    match &self.source {
      Source::Builtin(preset) => Some(preset.text.to_string()),
      Source::File(path) => std::fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
    }
  }
}

struct State {
  engine: Engine,
  entries: Vec<Entry>,
  current: usize,
  /// The preset that is playing; it is unloaded when the next one starts.
  loaded: Option<PresetId>,
  started: Instant,
  switched_at: f64,
  written: u64,
  rgba: Vec<u8>,
  /// The engine size the last render asked for.
  size: (u32, u32),
  /// The encoded image of the last render and the area it covers, for [`flush`].
  image: Vec<u8>,
  pending: Option<(Rect, Protocol)>,
  sixel: sixel::Encoder,
  /// A Kitty image is on the screen; it stays above the text until it is deleted.
  kitty_shown: bool,
  /// The images in the preset folder that presets sample by name.
  textures: TextureFolder,
}

/// A `.milk` preset or a `.milk2` double preset.
fn is_preset_file(path: &Path) -> bool {
  path
    .extension()
    .is_some_and(|e| e.eq_ignore_ascii_case("milk") || e.eq_ignore_ascii_case("milk2"))
}

/// The preset files under the user's preset folder, sorted by path. The folder is
/// created when it is missing, so the user can find it.
fn user_presets() -> Vec<Entry> {
  let Some(dir) = paths::milkdrop_presets_dir() else {
    return Vec::new();
  };
  let _ = std::fs::create_dir_all(&dir);
  let mut files = Vec::new();
  let mut pending = vec![dir];
  while let Some(dir) = pending.pop() {
    let Ok(entries) = std::fs::read_dir(&dir) else {
      continue;
    };
    for entry in entries.flatten() {
      let path = entry.path();
      if path.is_dir() {
        pending.push(path);
      } else if is_preset_file(&path) {
        files.push(path);
      }
    }
  }
  files.sort();
  files
    .into_iter()
    .map(|path| Entry {
      name: path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
      source: Source::File(path),
    })
    .collect()
}

impl State {
  fn new() -> State {
    let config = Config {
      width: 1,
      height: 1,
      ..Config::default()
    };
    // Shader presets need the GPU; without one the CPU engine plays the others.
    let mut engine = Engine::with_gpu(config.clone()).unwrap_or_else(|_| Engine::new(config));
    let mut entries: Vec<Entry> = presets::ALL
      .iter()
      .map(|p| Entry {
        name: p.name.to_string(),
        source: Source::Builtin(p),
      })
      .collect();
    let mut textures = TextureFolder::default();
    // Tests never read the user's real preset folder.
    if !cfg!(test) {
      entries.extend(user_presets());
      if let Some(dir) = paths::milkdrop_presets_dir() {
        textures = TextureFolder::scan([dir]);
      }
    }
    engine.set_texture_names(textures.names());
    let mut state = State {
      engine,
      entries,
      current: 0,
      loaded: None,
      started: Instant::now(),
      switched_at: 0.0,
      written: 0,
      rgba: Vec::new(),
      size: (0, 0),
      image: Vec::new(),
      pending: None,
      sixel: sixel::Encoder::default(),
      kitty_shown: false,
      textures,
    };
    state.show(0, 1, Transition::Cut);
    state
  }

  /// Starts entry `start`, or the first entry after it in steps of `delta` that loads.
  fn show(&mut self, start: usize, delta: isize, transition: Transition) {
    let count = self.entries.len();
    for tries in 0..count {
      // Both values are small, so the casts cannot overflow.
      let index = (start as isize + delta * tries as isize).rem_euclid(count as isize) as usize;
      let Some(text) = self.entries[index].text() else {
        continue;
      };
      let Ok(id) = self.engine.load_preset(&text) else {
        continue;
      };
      let added = self.textures.add_needed(&mut self.engine, id);
      let switched = self.engine.switch_to(id, transition);
      // The running preset keeps its textures; the engine's copies are freed.
      for name in &added {
        self.engine.remove_texture(name);
      }
      if switched.is_err() {
        let _ = self.engine.unload_preset(id);
        continue;
      }
      if let Some(old) = self.loaded.replace(id) {
        let _ = self.engine.unload_preset(old);
      }
      self.current = index;
      return;
    }
  }

  /// Blends `delta` presets forward (or back, when negative) and restarts the timer.
  fn step(&mut self, delta: isize) {
    let count = self.entries.len();
    if count == 0 {
      return;
    }
    let start = (self.current as isize + delta).rem_euclid(count as isize) as usize;
    self.show(start, delta.signum(), Transition::Blend(BLEND_SECONDS));
    self.switched_at = self.started.elapsed().as_secs_f64();
  }
}

thread_local! {
  // Leaked on purpose: dropping the GPU device during thread teardown would read
  // thread-locals that are already gone. The OS frees it when the process ends.
  static STATE: RefCell<Option<&'static mut State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
  STATE.with(|cell| {
    let mut slot = cell.borrow_mut();
    f(slot.get_or_insert_with(|| Box::leak(Box::new(State::new()))))
  })
}

/// Blends to the next (`1`) or previous (`-1`) preset.
fn step_preset(delta: isize) {
  with_state(|state| state.step(delta));
}

/// `]` blends to the next preset and `[` to the previous one, while MilkDrop is `shown`.
pub fn handle_key(key: Key, shown: bool) {
  if !shown {
    return;
  }
  match key {
    Key::Char(']') => step_preset(1),
    Key::Char('[') => step_preset(-1),
    _ => {}
  }
}

/// The name of the preset that is playing or blending in.
pub fn current_name() -> String {
  with_state(|state| {
    state
      .entries
      .get(state.current)
      .map_or_else(String::new, |e| e.name.clone())
  })
}

/// Feeds the samples the capture took in since the last call.
pub fn feed(audio: &RecentAudio) {
  with_state(|state| {
    let new = audio.written.saturating_sub(state.written);
    state.written = audio.written;
    let count = usize::try_from(new).map_or(audio.samples.len(), |n| n.min(audio.samples.len()));
    let fresh = &audio.samples[audio.samples.len() - count..];
    state.engine.push_pcm(fresh, 1, audio.sample_rate);
  });
}

/// The image protocol of the terminal, from the cover art's probe.
fn frame_protocol() -> Option<(Protocol, (u32, u32))> {
  #[cfg(feature = "cover-art")]
  {
    crate::tui::cover_art::frame_protocol()
  }
  #[cfg(not(feature = "cover-art"))]
  {
    None
  }
}

/// Renders the engine for the area: as an image that [`flush`] writes after the frame
/// when the terminal has Kitty or Sixel, otherwise as half-block cells, two pixels each.
pub fn render(buf: &mut Buffer, area: Rect) {
  render_with(buf, area, frame_protocol());
}

fn render_with(buf: &mut Buffer, area: Rect, protocol: Option<(Protocol, (u32, u32))>) {
  if area.width == 0 || area.height == 0 {
    return;
  }
  with_state(|state| {
    let (cols, rows) = (area.width, area.height);
    let fitted = protocol.map(|(protocol, cell)| fit(protocol, (cols, rows), cell));
    let size = fitted.map_or((u32::from(cols), u32::from(rows) * 2), |s| {
      (s.width, s.height)
    });
    if state.size != size {
      state.engine.resize(size.0, size.1);
      state.size = size;
    }
    // The engine clamps each side to the GPU limit, so its own size is the frame's.
    let (width, height) = {
      let config = state.engine.config();
      (config.width as usize, config.height as usize)
    };
    state.rgba.resize(width * height * 4, 0);
    let now = state.started.elapsed().as_secs_f64();
    if now - state.switched_at > SECONDS_PER_PRESET {
      state.step(1);
    }
    if state
      .engine
      .render_into(now, &mut state.rgba, width * 4)
      .is_err()
    {
      return;
    }
    if let (Some((protocol, cell)), Some(fitted)) = (protocol, fitted) {
      let rendered = (width as u32, height as u32);
      state.image.clear();
      // A Kitty image fills the area. A Sixel image is drawn pixel for pixel and can
      // end inside the last row or column, whose cells ratatui must still clear.
      let covered = match protocol {
        Protocol::Kitty => {
          kitty::encode(
            &state.rgba,
            rendered,
            (cols, rows),
            KITTY_ID,
            &mut state.image,
          );
          area
        }
        Protocol::Sixel => {
          state
            .sixel
            .encode(&state.rgba, rendered, fitted.scale, 256, &mut state.image);
          let whole = |pixels: u32, cell: u32| (pixels * fitted.scale / cell.max(1)) as u16;
          Rect {
            width: whole(rendered.0, cell.0).min(cols),
            height: whole(rendered.1 / 6 * 6, cell.1).min(rows),
            ..area
          }
        }
      };
      // Ratatui leaves skipped cells alone, so the image is not drawn over.
      for position in covered.positions() {
        if let Some(cell) = buf.cell_mut(position) {
          cell.set_diff_option(CellDiffOption::Skip);
        }
      }
      state.pending = Some((covered, protocol));
      return;
    }
    for row in 0..rows {
      for col in 0..cols {
        let pixel = |y: usize| {
          let at = (y * width + usize::from(col)) * 4;
          Color::Rgb(state.rgba[at], state.rgba[at + 1], state.rgba[at + 2])
        };
        let top = pixel(usize::from(row) * 2);
        let bottom = pixel(usize::from(row) * 2 + 1);
        if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
          cell.set_symbol("\u{2580}").set_fg(top).set_bg(bottom);
        }
      }
    }
  });
}

/// Writes the image the last render encoded at its area, after ratatui drew `buf`. An
/// overlay that took cells of the area wins: the image waits, and a Kitty image, which
/// stays above the text, is deleted. When MilkDrop is not shown, it deletes the Kitty
/// image too. Without an engine, nothing happens.
pub fn flush(buf: &Buffer, out: &mut impl Write) -> io::Result<()> {
  STATE.with(|cell| {
    let mut slot = cell.borrow_mut();
    let Some(state) = slot.as_mut() else {
      return Ok(());
    };
    let pending = state.pending.take();
    let visible =
      pending.filter(|(area, _)| area.positions().all(|position| skipped(buf, position)));
    let mut bytes = Vec::new();
    match visible {
      Some((area, protocol)) => {
        queue!(
          out,
          terminal::BeginSynchronizedUpdate,
          MoveTo(area.x, area.y)
        )?;
        out.write_all(&state.image)?;
        queue!(out, terminal::EndSynchronizedUpdate)?;
        state.kitty_shown = protocol == Protocol::Kitty;
      }
      None if state.kitty_shown => {
        kitty::delete(KITTY_ID, &mut bytes);
        out.write_all(&bytes)?;
        state.kitty_shown = false;
      }
      None => return Ok(()),
    }
    out.flush()
  })
}

fn skipped(buf: &Buffer, position: ratatui::layout::Position) -> bool {
  buf
    .cell(position)
    .is_some_and(|cell| cell.diff_option == CellDiffOption::Skip)
}

/// Deletes the Kitty image on exit.
pub fn clear(out: &mut impl Write) {
  STATE.with(|cell| {
    if let Some(state) = cell.borrow_mut().as_mut().filter(|s| s.kitty_shown) {
      let mut bytes = Vec::new();
      kitty::delete(KITTY_ID, &mut bytes);
      let _ = out.write_all(&bytes);
      let _ = out.flush();
      state.kitty_shown = false;
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn flushed(buf: &Buffer) -> Vec<u8> {
    let mut out = Vec::new();
    flush(buf, &mut out).unwrap();
    out
  }

  #[test]
  fn a_sixel_frame_is_written_at_its_area_after_the_cells_are_skipped() {
    let area = Rect::new(2, 1, 8, 3);
    let mut buf = Buffer::empty(Rect::new(0, 0, 12, 5));
    render_with(&mut buf, area, Some((Protocol::Sixel, (10, 20))));
    assert!(area.positions().all(|p| skipped(&buf, p)));
    assert!(!skipped(&buf, (0, 0).into()));
    let out = String::from_utf8_lossy(&flushed(&buf)).into_owned();
    // Synchronized output, the cursor at column 3 row 2, then the Sixel image.
    assert!(out.starts_with("\x1b[?2026h\x1b[2;3H\x1bPq"), "{out:?}");
    assert!(out.ends_with("\x1b[?2026l"), "{out:?}");
    // Nothing is pending until the next render.
    assert!(flushed(&buf).is_empty());
  }

  #[test]
  fn a_sixel_row_the_image_covers_only_in_part_is_not_skipped() {
    // Two rows of 23 pixels are 46 pixels; the image is 42, whole bands of six.
    let area = Rect::new(0, 0, 8, 2);
    let mut buf = Buffer::empty(area);
    render_with(&mut buf, area, Some((Protocol::Sixel, (10, 23))));
    assert!((0..8).all(|x| skipped(&buf, (x, 0).into())));
    assert!((0..8).all(|x| !skipped(&buf, (x, 1).into())));
    assert!(!flushed(&buf).is_empty());
  }

  #[test]
  fn an_overlay_over_the_area_holds_the_image_and_deletes_a_kitty_image() {
    let area = Rect::new(0, 0, 6, 2);
    let mut buf = Buffer::empty(area);
    render_with(&mut buf, area, Some((Protocol::Kitty, (10, 20))));
    assert!(String::from_utf8_lossy(&flushed(&buf)).contains("\x1b_Ga=T"));
    render_with(&mut buf, area, Some((Protocol::Kitty, (10, 20))));
    // A popup draws one cell of the area.
    buf[(1, 1)]
      .set_diff_option(CellDiffOption::None)
      .set_symbol("x");
    let out = String::from_utf8_lossy(&flushed(&buf)).into_owned();
    assert_eq!(out, format!("\x1b_Ga=d,d=I,i={KITTY_ID},q=2\x1b\\"));
  }

  #[test]
  fn without_a_protocol_the_cells_are_half_blocks() {
    let area = Rect::new(0, 0, 4, 2);
    let mut buf = Buffer::empty(area);
    render_with(&mut buf, area, None);
    assert!(area
      .positions()
      .all(|p| !skipped(&buf, p) && buf[p].symbol() == "\u{2580}"));
  }

  #[test]
  fn brackets_change_the_preset_only_while_milkdrop_is_shown() {
    let first = current_name();
    handle_key(Key::Char(']'), false);
    assert_eq!(current_name(), first);
    handle_key(Key::Char(']'), true);
    assert_ne!(current_name(), first);
    handle_key(Key::Char('['), true);
    assert_eq!(current_name(), first);
  }

  #[test]
  fn milk_and_milk2_files_are_presets() {
    assert!(is_preset_file(Path::new("a/Geiss.milk")));
    assert!(is_preset_file(Path::new("a/Double.MILK2")));
    assert!(!is_preset_file(Path::new("a/texture.jpg")));
    assert!(!is_preset_file(Path::new("a/milk")));
  }

  #[test]
  fn stepping_moves_through_the_presets_and_wraps() {
    let first = current_name();
    step_preset(1);
    assert_ne!(current_name(), first);
    step_preset(-1);
    assert_eq!(current_name(), first);
    step_preset(-1);
    assert_eq!(
      current_name(),
      presets::ALL.last().unwrap().name.to_string()
    );
  }
}
