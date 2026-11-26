//! The MilkDrop visualizer style: tidewarp's built-in presets, drawn with half-blocks.
//!
//! The engine lives in a thread-local, like the retro styles' animation state, because
//! draw functions only see `&App`. The runner feeds it the capture's new samples.

use std::cell::RefCell;
use std::time::Instant;

use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use tidewarp::{presets, ComfortMode, Config, Engine, PresetId, Transition};

use crate::core::user_config::VisualizerStyle;
use crate::infra::audio::RecentAudio;
use crate::tui::event::Key;

/// Seconds each preset plays before the next one blends in.
const SECONDS_PER_PRESET: f64 = 30.0;
const BLEND_SECONDS: f64 = 5.0;

struct State {
  engine: Engine,
  /// The loaded presets with their display names.
  presets: Vec<(PresetId, &'static str)>,
  current: usize,
  started: Instant,
  switched_at: f64,
  written: u64,
  rgba: Vec<u8>,
  size: (u16, u16),
}

impl State {
  fn new() -> State {
    let mut engine = Engine::new(Config {
      width: 1,
      height: 1,
      comfort: ComfortMode::Calm,
      ..Config::default()
    });
    let presets: Vec<(PresetId, &'static str)> = presets::ALL
      .iter()
      .filter_map(|p| engine.load_preset(p.text).ok().map(|id| (id, p.name)))
      .collect();
    if let Some((first, _)) = presets.first() {
      let _ = engine.switch_to(*first, Transition::Cut);
    }
    State {
      engine,
      presets,
      current: 0,
      started: Instant::now(),
      switched_at: 0.0,
      written: 0,
      rgba: Vec::new(),
      size: (0, 0),
    }
  }
}

thread_local! {
  static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
  STATE.with(|cell| f(cell.borrow_mut().get_or_insert_with(State::new)))
}

impl State {
  /// Blends `delta` presets forward (or back, when negative) and restarts the timer.
  fn step(&mut self, delta: isize) {
    let count = self.presets.len();
    if count == 0 {
      return;
    }
    // Both values are small, so the casts cannot overflow.
    self.current = (self.current as isize + delta).rem_euclid(count as isize) as usize;
    let _ = self.engine.switch_to(
      self.presets[self.current].0,
      Transition::Blend(BLEND_SECONDS),
    );
    self.switched_at = self.started.elapsed().as_secs_f64();
  }
}

/// Blends to the next (`1`) or previous (`-1`) preset.
fn step_preset(delta: isize) {
  with_state(|state| state.step(delta));
}

/// `]` blends to the next preset and `[` to the previous one, while `style` is MilkDrop.
pub fn handle_key(key: Key, style: VisualizerStyle) {
  if style != VisualizerStyle::MilkDrop {
    return;
  }
  match key {
    Key::Char(']') => step_preset(1),
    Key::Char('[') => step_preset(-1),
    _ => {}
  }
}

/// The name of the preset that is playing or blending in.
pub fn current_name() -> &'static str {
  with_state(|state| {
    state
      .presets
      .get(state.current)
      .map_or("", |(_, name)| *name)
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
  fn brackets_change_the_preset_only_in_the_milkdrop_style() {
    let first = current_name();
    handle_key(Key::Char(']'), VisualizerStyle::Cava);
    assert_eq!(current_name(), first);
    handle_key(Key::Char(']'), VisualizerStyle::MilkDrop);
    assert_ne!(current_name(), first);
    handle_key(Key::Char('['), VisualizerStyle::MilkDrop);
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
    assert_eq!(current_name(), presets::ALL.last().unwrap().name);
  }
}
