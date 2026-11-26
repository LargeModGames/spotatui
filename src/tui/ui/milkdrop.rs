//! The MilkDrop visualizer style: tidewarp's built-in presets, drawn with half-blocks.
//!
//! The engine lives in a thread-local, like the retro styles' animation state, because
//! draw functions only see `&App`. The runner feeds it the capture's new samples.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Instant;

use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use tidewarp::{presets, presets::BuiltinPreset, Config, Engine, PresetId, Transition};

use crate::core::paths;
use crate::infra::audio::RecentAudio;
use crate::tui::event::Key;

/// Seconds each preset plays before the next one blends in.
const SECONDS_PER_PRESET: f64 = 30.0;
const BLEND_SECONDS: f64 = 5.0;

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
  size: (u16, u16),
}

/// The `.milk` files under the user's preset folder, sorted by path. The folder is
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
      } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("milk"))
      {
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
    let engine = Engine::with_gpu(config.clone()).unwrap_or_else(|_| Engine::new(config));
    let mut entries: Vec<Entry> = presets::ALL
      .iter()
      .map(|p| Entry {
        name: p.name.to_string(),
        source: Source::Builtin(p),
      })
      .collect();
    // Tests never read the user's real preset folder.
    if !cfg!(test) {
      entries.extend(user_presets());
    }
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
      if self.engine.switch_to(id, transition).is_err() {
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

/// Renders the engine at the area's size, two pixels per cell, and writes the cells.
pub fn render(buf: &mut Buffer, area: Rect) {
  if area.width == 0 || area.height == 0 {
    return;
  }
  with_state(|state| {
    let (cols, rows) = (area.width, area.height);
    let (width, height) = (usize::from(cols), usize::from(rows) * 2);
    if state.size != (cols, rows) {
      state.engine.resize(u32::from(cols), u32::from(rows) * 2);
      state.rgba.resize(width * height * 4, 0);
      state.size = (cols, rows);
    }
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

#[cfg(test)]
mod tests {
  use super::*;

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
