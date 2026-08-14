//! Frontend-neutral viewport geometry.
//!
//! `Viewport` is the rendering surface size in the frontend's own cells
//! (terminal cells for the TUI). It replaces `ratatui::layout::Size` in `App`
//! state so `core/` carries no terminal-toolkit types.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
  pub width: u16,
  pub height: u16,
}
