//! Render-boundary conversion from the frontend-neutral theme types
//! (`core/theme.rs`) to ratatui's, plus the style helpers that need ratatui
//! types and therefore cannot live on the core structs.

use ratatui::style::{Modifier, Style};

use crate::core::theme::{Color, Theme};
use crate::core::user_config::BehaviorConfig;

impl From<Color> for ratatui::style::Color {
  fn from(color: Color) -> Self {
    match color {
      Color::Reset => ratatui::style::Color::Reset,
      Color::Black => ratatui::style::Color::Black,
      Color::Red => ratatui::style::Color::Red,
      Color::Green => ratatui::style::Color::Green,
      Color::Yellow => ratatui::style::Color::Yellow,
      Color::Blue => ratatui::style::Color::Blue,
      Color::Magenta => ratatui::style::Color::Magenta,
      Color::Cyan => ratatui::style::Color::Cyan,
      Color::Gray => ratatui::style::Color::Gray,
      Color::DarkGray => ratatui::style::Color::DarkGray,
      Color::LightRed => ratatui::style::Color::LightRed,
      Color::LightGreen => ratatui::style::Color::LightGreen,
      Color::LightYellow => ratatui::style::Color::LightYellow,
      Color::LightBlue => ratatui::style::Color::LightBlue,
      Color::LightMagenta => ratatui::style::Color::LightMagenta,
      Color::LightCyan => ratatui::style::Color::LightCyan,
      Color::White => ratatui::style::Color::White,
      Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
      Color::Indexed(i) => ratatui::style::Color::Indexed(i),
    }
  }
}

/// Ratatui-typed helpers on the core [`Theme`].
pub trait ThemeExt {
  fn base_style(&self) -> Style;
}

impl ThemeExt for Theme {
  fn base_style(&self) -> Style {
    Style::default()
      .fg(self.text.into())
      .bg(self.background.into())
  }
}

/// Ratatui-typed helpers on [`BehaviorConfig`].
pub trait EmphasisExt {
  /// Return the emphasis modifier to apply to emphasized text, gated on
  /// `enable_text_emphasis`. Callers pass the modifier they *want*
  /// (e.g. `Modifier::BOLD`, `Modifier::BOLD | Modifier::ITALIC`) and get
  /// `Modifier::empty()` when emphasis is disabled — so a single call site
  /// replaces the previous unconditional `Modifier::BOLD`.
  fn emphasis(&self, m: Modifier) -> Modifier;
}

impl EmphasisExt for BehaviorConfig {
  fn emphasis(&self, m: Modifier) -> Modifier {
    if self.enable_text_emphasis {
      m
    } else {
      Modifier::empty()
    }
  }
}
