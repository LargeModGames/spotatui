use crate::core::source::Source;
use crate::tui::event::Key;
use anyhow::{anyhow, Result};
use ratatui::style::{Color, Style};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{fs, path::PathBuf};

const FILE_NAME: &str = "config.yml";
const CONFIG_DIR: &str = ".config";
const APP_CONFIG_DIR: &str = "spotatui";
pub const DEFAULT_TICK_RATE_MILLISECONDS: u64 = 250;
pub const DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS: u64 = 16;
pub const MAX_TICK_RATE_MILLISECONDS: u64 = 999;
#[cfg(feature = "cover-art")]
pub const MIN_PLAYBAR_COVER_ART_SIZE_PERCENT: u16 = 25;
#[cfg(feature = "cover-art")]
pub const MAX_PLAYBAR_COVER_ART_SIZE_PERCENT: u16 = 200;

#[cfg(feature = "cover-art")]
pub fn clamp_playbar_cover_art_size_percent(value: u16) -> u16 {
  value.clamp(
    MIN_PLAYBAR_COVER_ART_SIZE_PERCENT,
    MAX_PLAYBAR_COVER_ART_SIZE_PERCENT,
  )
}

#[cfg(feature = "cover-art")]
pub fn normalize_playbar_cover_art_size_percent(value: i64) -> u16 {
  value.clamp(
    MIN_PLAYBAR_COVER_ART_SIZE_PERCENT as i64,
    MAX_PLAYBAR_COVER_ART_SIZE_PERCENT as i64,
  ) as u16
}

pub fn validate_tick_rate_milliseconds(value: u64, label: &str) -> Result<u64> {
  if (1..=MAX_TICK_RATE_MILLISECONDS).contains(&value) {
    Ok(value)
  } else {
    Err(anyhow!("{label} must be between 1 and 999 milliseconds"))
  }
}

pub fn normalize_tick_rate_milliseconds(value: i64) -> u64 {
  value.clamp(1, MAX_TICK_RATE_MILLISECONDS as i64) as u64
}

/// Parse a human-readable update delay into seconds.
/// Accepts: "0", "30s", "10m", "2h", "7d", or a bare second count.
pub fn parse_update_delay_secs(value: &str) -> Result<u64, String> {
  let value = value.trim();
  if value == "0" || value.is_empty() {
    return Ok(0);
  }

  for (suffix, multiplier, label) in [
    ("d", 86400_u64, "days"),
    ("h", 3600_u64, "hours"),
    ("m", 60_u64, "minutes"),
    ("s", 1_u64, "seconds"),
  ] {
    if let Some(amount) = value.strip_suffix(suffix) {
      return amount
        .trim()
        .parse::<u64>()
        .map(|v| v * multiplier)
        .map_err(|_| format!("Invalid {label} value"));
    }
  }

  value
    .parse::<u64>()
    .map_err(|_| "Invalid numeric value or unknown suffix".to_string())
}

#[cfg(feature = "self-update")]
pub fn format_update_delay_secs(secs: u64) -> String {
  if secs >= 86400 {
    format!("{}d", secs / 86400)
  } else if secs >= 3600 {
    format!("{}h", secs / 3600)
  } else if secs >= 60 {
    format!("{}m", secs / 60)
  } else {
    format!("{}s", secs)
  }
}

pub(crate) fn default_app_config_dir() -> Option<PathBuf> {
  dirs::home_dir().map(|home| home.join(CONFIG_DIR).join(APP_CONFIG_DIR))
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserTheme {
  pub preset: Option<String>,
  pub active: Option<String>,
  pub banner: Option<String>,
  pub error_border: Option<String>,
  pub error_text: Option<String>,
  pub hint: Option<String>,
  pub hovered: Option<String>,
  pub inactive: Option<String>,
  pub playbar_background: Option<String>,
  pub playbar_progress: Option<String>,
  pub playbar_progress_text: Option<String>,
  pub playbar_text: Option<String>,
  pub selected: Option<String>,
  pub text: Option<String>,
  pub background: Option<String>,
  pub header: Option<String>,
  pub highlighted_lyrics: Option<String>,
}

#[derive(Copy, Clone, Debug)]
pub struct Theme {
  #[allow(dead_code)]
  pub analysis_bar: Color,
  #[allow(dead_code)]
  pub analysis_bar_text: Color,
  #[allow(dead_code)]
  pub active: Color,
  pub banner: Color,
  pub error_border: Color,
  pub error_text: Color,
  pub hint: Color,
  pub hovered: Color,
  pub inactive: Color,
  pub playbar_background: Color,
  pub playbar_progress: Color,
  pub playbar_progress_text: Color,
  pub playbar_text: Color,
  pub selected: Color,
  pub text: Color,
  pub background: Color,
  pub header: Color,
  pub highlighted_lyrics: Color,
}

impl Theme {
  pub fn base_style(&self) -> Style {
    Style::default().fg(self.text).bg(self.background)
  }
}

impl Default for Theme {
  fn default() -> Self {
    // Use RGB colors for cross-terminal compatibility
    // Named ANSI colors (like Color::Cyan) can be remapped by terminal themes
    // causing inconsistent appearance across different terminals
    Theme {
      analysis_bar: Color::Rgb(0, 200, 200), // LightCyan equivalent
      analysis_bar_text: Color::Reset,
      active: Color::Rgb(0, 180, 180),       // Cyan equivalent
      banner: Color::Rgb(0, 200, 200),       // LightCyan equivalent
      error_border: Color::Rgb(200, 0, 0),   // Red equivalent
      error_text: Color::Rgb(255, 100, 100), // LightRed equivalent
      hint: Color::Rgb(200, 200, 0),         // Yellow equivalent
      hovered: Color::Rgb(180, 0, 180),      // Magenta equivalent
      inactive: Color::Rgb(128, 128, 128),   // Gray equivalent
      playbar_background: Color::Reset,
      playbar_progress: Color::Rgb(0, 200, 200), // LightCyan equivalent
      playbar_progress_text: Color::Rgb(255, 255, 255), // Bright white for visibility
      playbar_text: Color::Reset,
      selected: Color::Rgb(0, 200, 200), // LightCyan equivalent
      text: Color::Reset,
      background: Color::Reset,
      header: Color::Reset,
      highlighted_lyrics: Color::Rgb(0, 200, 200), // LightCyan equivalent
    }
  }
}

/// Available theme presets
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ThemePreset {
  #[default]
  Default,
  PookiePink,
  Spotify,
  Vesper,
  Dracula,
  Nord,
  SolarizedDark,
  Monokai,
  Gruvbox,
  GruvboxLight,
  CatppuccinMocha,
  Custom, // When user has manually customized colors
}

impl ThemePreset {
  pub fn all() -> &'static [ThemePreset] {
    &[
      ThemePreset::Default,
      ThemePreset::PookiePink,
      ThemePreset::Spotify,
      ThemePreset::Vesper,
      ThemePreset::Dracula,
      ThemePreset::Nord,
      ThemePreset::SolarizedDark,
      ThemePreset::Monokai,
      ThemePreset::Gruvbox,
      ThemePreset::GruvboxLight,
      ThemePreset::CatppuccinMocha,
      ThemePreset::Custom,
    ]
  }

  pub fn name(&self) -> &'static str {
    match self {
      ThemePreset::Default => "Default (Cyan)",
      ThemePreset::PookiePink => "Pookie Pink",
      ThemePreset::Spotify => "Spotify",
      ThemePreset::Vesper => "Vesper",
      ThemePreset::Dracula => "Dracula",
      ThemePreset::Nord => "Nord",
      ThemePreset::SolarizedDark => "Solarized Dark",
      ThemePreset::Monokai => "Monokai",
      ThemePreset::Gruvbox => "Gruvbox",
      ThemePreset::GruvboxLight => "Gruvbox Light",
      ThemePreset::CatppuccinMocha => "Catppuccin Mocha",
      ThemePreset::Custom => "Custom",
    }
  }

  pub fn from_name(name: &str) -> Self {
    match name {
      "Default (Cyan)" => ThemePreset::Default,
      "Pookie Pink" => ThemePreset::PookiePink,
      "Spotify" => ThemePreset::Spotify,
      "Vesper" => ThemePreset::Vesper,
      "Dracula" => ThemePreset::Dracula,
      "Nord" => ThemePreset::Nord,
      "Solarized Dark" => ThemePreset::SolarizedDark,
      "Monokai" => ThemePreset::Monokai,
      "Gruvbox" => ThemePreset::Gruvbox,
      "Gruvbox Light" => ThemePreset::GruvboxLight,
      "Catppuccin Mocha" => ThemePreset::CatppuccinMocha,
      _ => ThemePreset::Custom,
    }
  }

  pub fn next(&self) -> Self {
    let presets = Self::all();
    let current_idx = presets.iter().position(|p| p == self).unwrap_or(0);
    let next_idx = (current_idx + 1) % presets.len();
    presets[next_idx]
  }

  pub fn prev(&self) -> Self {
    let presets = Self::all();
    let current_idx = presets.iter().position(|p| p == self).unwrap_or(0);
    let prev_idx = if current_idx == 0 {
      presets.len() - 1
    } else {
      current_idx - 1
    };
    presets[prev_idx]
  }

  /// Get the theme colors for this preset
  pub fn to_theme(self) -> Theme {
    match self {
      ThemePreset::Default => Theme::default(),
      ThemePreset::PookiePink => Theme {
        analysis_bar: Color::Rgb(255, 255, 255),         // White
        analysis_bar_text: Color::Rgb(165, 30, 100),     // Dark pink
        active: Color::Rgb(150, 25, 92),                 // Deep pink
        banner: Color::Rgb(255, 145, 205),               // Light-medium pink
        error_border: Color::Rgb(175, 0, 75),            // Deep rose
        error_text: Color::Rgb(255, 215, 235),           // Light pink-white
        hint: Color::Rgb(255, 235, 245),                 // Soft white-pink
        hovered: Color::Rgb(220, 85, 155),               // Mid pink for hover
        inactive: Color::Rgb(255, 195, 225),             // Muted pink
        playbar_background: Color::Rgb(245, 115, 180),   // Pink
        playbar_progress: Color::Rgb(255, 255, 255),     // White
        playbar_progress_text: Color::Rgb(175, 35, 105), // Dark pink
        playbar_text: Color::Rgb(255, 255, 255),         // White
        selected: Color::Rgb(125, 20, 80),               // Deeper pink for selected row
        text: Color::Rgb(255, 255, 255),                 // White
        background: Color::Rgb(245, 115, 180),           // Pink background
        header: Color::Rgb(255, 255, 255),               // White
        highlighted_lyrics: Color::Rgb(255, 230, 245),   // Light pink-white
      },
      ThemePreset::Vesper => Theme {
        analysis_bar: Color::Rgb(153, 255, 228),     // Mint (#99FFE4)
        analysis_bar_text: Color::Rgb(16, 16, 16),   // Near-black (#101010)
        active: Color::Rgb(255, 199, 153),           // Accent orange (#FFC799)
        banner: Color::Rgb(255, 199, 153),           // Accent orange
        error_border: Color::Rgb(255, 128, 128),     // Error red (#FF8080)
        error_text: Color::Rgb(255, 128, 128),       // Error red
        hint: Color::Rgb(255, 199, 153),             // Accent orange
        hovered: Color::Rgb(255, 207, 168),          // Hover orange (#FFCFA8)
        inactive: Color::Rgb(190, 190, 190),         // Higher-contrast muted gray
        playbar_background: Color::Rgb(22, 22, 22),  // Elevated bg (#161616)
        playbar_progress: Color::Rgb(153, 255, 228), // Mint
        playbar_progress_text: Color::Rgb(255, 255, 255), // White for readability
        playbar_text: Color::Rgb(210, 210, 210),     // Higher-contrast playbar text
        selected: Color::Rgb(255, 199, 153),         // Accent orange
        text: Color::Rgb(255, 255, 255),             // White
        background: Color::Rgb(16, 16, 16),          // Base bg (#101010)
        header: Color::Rgb(255, 255, 255),           // White
        highlighted_lyrics: Color::Rgb(153, 255, 228), // Mint
      },
      ThemePreset::Dracula => Theme {
        analysis_bar: Color::Rgb(189, 147, 249),      // Purple
        analysis_bar_text: Color::Rgb(248, 248, 242), // Foreground
        active: Color::Rgb(80, 250, 123),             // Green
        banner: Color::Rgb(255, 121, 198),            // Pink
        error_border: Color::Rgb(255, 85, 85),        // Red
        error_text: Color::Rgb(255, 85, 85),
        hint: Color::Rgb(241, 250, 140),    // Yellow
        hovered: Color::Rgb(189, 147, 249), // Purple
        inactive: Color::Rgb(98, 114, 164), // Comment
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(80, 250, 123), // Green
        playbar_progress_text: Color::Rgb(248, 248, 242),
        playbar_text: Color::Rgb(248, 248, 242),
        selected: Color::Rgb(139, 233, 253), // Cyan
        text: Color::Rgb(248, 248, 242),
        background: Color::Reset,
        header: Color::Rgb(255, 121, 198),             // Pink
        highlighted_lyrics: Color::Rgb(255, 121, 198), // Pink
      },
      ThemePreset::Nord => Theme {
        analysis_bar: Color::Rgb(136, 192, 208),      // Nord8 (frost)
        analysis_bar_text: Color::Rgb(236, 239, 244), // Nord6
        active: Color::Rgb(163, 190, 140),            // Nord14 (green)
        banner: Color::Rgb(136, 192, 208),            // Nord8
        error_border: Color::Rgb(191, 97, 106),       // Nord11 (red)
        error_text: Color::Rgb(191, 97, 106),
        hint: Color::Rgb(235, 203, 139),    // Nord13 (yellow)
        hovered: Color::Rgb(180, 142, 173), // Nord15 (purple)
        inactive: Color::Rgb(76, 86, 106),  // Nord3
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(136, 192, 208), // Nord8
        playbar_progress_text: Color::Rgb(236, 239, 244),
        playbar_text: Color::Rgb(236, 239, 244),
        selected: Color::Rgb(129, 161, 193), // Nord9
        text: Color::Rgb(236, 239, 244),     // Nord6
        background: Color::Reset,
        header: Color::Rgb(136, 192, 208),
        highlighted_lyrics: Color::Rgb(136, 192, 208), // Nord8 (frost)
      },
      ThemePreset::SolarizedDark => Theme {
        analysis_bar: Color::Rgb(38, 139, 210),       // Blue
        analysis_bar_text: Color::Rgb(253, 246, 227), // Base3
        active: Color::Rgb(133, 153, 0),              // Green
        banner: Color::Rgb(38, 139, 210),             // Blue
        error_border: Color::Rgb(220, 50, 47),        // Red
        error_text: Color::Rgb(220, 50, 47),
        hint: Color::Rgb(181, 137, 0),      // Yellow
        hovered: Color::Rgb(211, 54, 130),  // Magenta
        inactive: Color::Rgb(88, 110, 117), // Base01
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(42, 161, 152), // Cyan
        playbar_progress_text: Color::Rgb(253, 246, 227),
        playbar_text: Color::Rgb(147, 161, 161), // Base1
        selected: Color::Rgb(42, 161, 152),      // Cyan
        text: Color::Rgb(147, 161, 161),         // Base1
        background: Color::Reset,
        header: Color::Rgb(38, 139, 210),
        highlighted_lyrics: Color::Rgb(38, 139, 210), // Blue
      },
      ThemePreset::Monokai => Theme {
        analysis_bar: Color::Rgb(102, 217, 239),      // Cyan
        analysis_bar_text: Color::Rgb(248, 248, 242), // Foreground
        active: Color::Rgb(166, 226, 46),             // Green
        banner: Color::Rgb(249, 38, 114),             // Pink
        error_border: Color::Rgb(249, 38, 114),       // Pink (error)
        error_text: Color::Rgb(249, 38, 114),
        hint: Color::Rgb(230, 219, 116),    // Yellow
        hovered: Color::Rgb(174, 129, 255), // Purple
        inactive: Color::Rgb(117, 113, 94), // Comment
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(166, 226, 46), // Green
        playbar_progress_text: Color::Rgb(248, 248, 242),
        playbar_text: Color::Rgb(248, 248, 242),
        selected: Color::Rgb(102, 217, 239), // Cyan
        text: Color::Rgb(248, 248, 242),
        background: Color::Reset,
        header: Color::Rgb(249, 38, 114),
        highlighted_lyrics: Color::Rgb(249, 38, 114), // Pink
      },
      ThemePreset::Gruvbox => Theme {
        analysis_bar: Color::Rgb(131, 165, 152),      // Aqua
        analysis_bar_text: Color::Rgb(235, 219, 178), // fg
        active: Color::Rgb(184, 187, 38),             // Green
        banner: Color::Rgb(254, 128, 25),             // Orange
        error_border: Color::Rgb(251, 73, 52),        // Red
        error_text: Color::Rgb(251, 73, 52),
        hint: Color::Rgb(250, 189, 47),      // Yellow
        hovered: Color::Rgb(211, 134, 155),  // Purple
        inactive: Color::Rgb(146, 131, 116), // Gray
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(184, 187, 38), // Green
        playbar_progress_text: Color::Rgb(235, 219, 178),
        playbar_text: Color::Rgb(235, 219, 178),
        selected: Color::Rgb(131, 165, 152), // Aqua
        text: Color::Rgb(235, 219, 178),     // fg
        background: Color::Reset,
        header: Color::Rgb(254, 128, 25),             // Orange
        highlighted_lyrics: Color::Rgb(254, 128, 25), // Orange
      },
      ThemePreset::GruvboxLight => Theme {
        analysis_bar: Color::Rgb(66, 123, 88),     // Aqua
        analysis_bar_text: Color::Rgb(60, 56, 54), // fg
        active: Color::Rgb(121, 116, 14),          // Green
        banner: Color::Rgb(175, 58, 3),            // Orange
        error_border: Color::Rgb(157, 0, 6),       // Red
        error_text: Color::Rgb(157, 0, 6),
        hint: Color::Rgb(181, 118, 20),                // Yellow
        hovered: Color::Rgb(143, 63, 113),             // Purple
        inactive: Color::Rgb(146, 131, 116),           // Gray
        playbar_background: Color::Rgb(251, 241, 199), // bg
        playbar_progress: Color::Rgb(121, 116, 14),    // Green
        playbar_progress_text: Color::Rgb(60, 56, 54),
        playbar_text: Color::Rgb(60, 56, 54),
        selected: Color::Rgb(66, 123, 88), // Aqua
        text: Color::Rgb(60, 56, 54),      // fg
        background: Color::Rgb(251, 241, 199),
        header: Color::Rgb(175, 58, 3),             // Orange
        highlighted_lyrics: Color::Rgb(175, 58, 3), // Orange
      },
      ThemePreset::CatppuccinMocha => Theme {
        analysis_bar: Color::Rgb(166, 227, 161),      // Green
        analysis_bar_text: Color::Rgb(205, 214, 244), // Text
        active: Color::Rgb(180, 190, 254),            // Lavender
        banner: Color::Rgb(180, 190, 254),            // Lavender
        error_border: Color::Rgb(243, 139, 168),      // Red
        error_text: Color::Rgb(243, 139, 168),        // Red
        hint: Color::Rgb(250, 179, 135),              // Peach
        hovered: Color::Rgb(137, 180, 250),           // Blue
        inactive: Color::Rgb(108, 112, 134),          // Overlay 0
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(180, 190, 254), // Lavender
        playbar_progress_text: Color::Rgb(88, 91, 112), // Surface 2
        playbar_text: Color::Rgb(186, 194, 222),     // Subtext 1
        selected: Color::Rgb(180, 190, 254),         // Lavender
        text: Color::Rgb(205, 214, 244),             // Text
        background: Color::Reset,
        header: Color::Rgb(180, 190, 254),             // Lavender
        highlighted_lyrics: Color::Rgb(180, 190, 254), // Lavender
      },
      ThemePreset::Spotify => Theme {
        analysis_bar: Color::Rgb(29, 185, 84), // Spotify Green #1DB954
        analysis_bar_text: Color::Rgb(255, 255, 255), // White
        active: Color::Rgb(29, 185, 84),       // Spotify Green
        banner: Color::Rgb(29, 185, 84),       // Spotify Green
        error_border: Color::Rgb(230, 76, 76), // Soft red
        error_text: Color::Rgb(230, 76, 76),
        hint: Color::Rgb(179, 179, 179),  // Gray hint
        hovered: Color::Rgb(29, 185, 84), // Spotify Green
        inactive: Color::Rgb(83, 83, 83), // Dark gray
        playbar_background: Color::Reset,
        playbar_progress: Color::Rgb(29, 185, 84), // Spotify Green
        playbar_progress_text: Color::Rgb(255, 255, 255),
        playbar_text: Color::Rgb(179, 179, 179), // Light gray
        selected: Color::Rgb(29, 185, 84),       // Spotify Green
        text: Color::Rgb(255, 255, 255),         // White
        background: Color::Reset,
        header: Color::Rgb(29, 185, 84),             // Spotify Green
        highlighted_lyrics: Color::Rgb(29, 185, 84), // Spotify Green
      },
      ThemePreset::Custom => Theme::default(), // Won't be used directly
    }
  }
}

/// Available audio visualizer styles
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum VisualizerStyle {
  /// Equalizer mode: Uses tui-equalizer with half-block bars and brightness effect
  ///
  /// Note: Older configs may contain `Classic`; it is accepted as an alias for `Equalizer`.
  #[default]
  #[serde(alias = "Classic")]
  Equalizer,
  /// BarGraph mode: Uses tui-bar-graph with Braille patterns for high-resolution display
  BarGraph,
}

impl VisualizerStyle {
  pub fn all() -> &'static [VisualizerStyle] {
    &[VisualizerStyle::Equalizer, VisualizerStyle::BarGraph]
  }

  pub fn name(&self) -> &'static str {
    match self {
      VisualizerStyle::Equalizer => "Equalizer",
      VisualizerStyle::BarGraph => "Bar Graph",
    }
  }

  pub fn next(&self) -> Self {
    let styles = Self::all();
    let current_idx = styles.iter().position(|s| s == self).unwrap_or(0);
    let next_idx = (current_idx + 1) % styles.len();
    styles[next_idx]
  }
}

/// Controls the playback state on startup, both for Spotify and for a persisted
/// non-Spotify session (local/Subsonic/radio/YouTube) that spotatui resumes on
/// launch.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupBehavior {
  /// Restore the last state: leave Spotify playback as-is, and resume a persisted
  /// non-Spotify session to the exact play/pause state it had when spotatui last
  /// closed (a track playing at exit resumes playing). This is the default.
  #[default]
  Continue,
  /// Always start playing on launch (Spotify, or the restored session).
  Play,
  /// Always pause on launch (Spotify, or the restored session).
  Pause,
}

impl StartupBehavior {
  pub fn name(self) -> &'static str {
    match self {
      StartupBehavior::Continue => "Continue",
      StartupBehavior::Play => "Play",
      StartupBehavior::Pause => "Pause",
    }
  }

  pub fn options() -> &'static [&'static str] {
    &["Continue", "Play", "Pause"]
  }

  pub fn from_name(name: &str) -> Self {
    match name {
      "Play" => StartupBehavior::Play,
      "Pause" => StartupBehavior::Pause,
      _ => StartupBehavior::Continue,
    }
  }
}

fn parse_key(key: String) -> Result<Key> {
  fn get_single_char(string: &str) -> char {
    match string.chars().next() {
      Some(c) => c,
      None => panic!(),
    }
  }

  match key.len() {
    1 => Ok(Key::Char(get_single_char(key.as_str()))),
    _ => {
      let sections: Vec<&str> = key.split('-').collect();

      if sections.len() > 2 {
        return Err(anyhow!(
          "Shortcut can only have 2 keys, \"{}\" has {}",
          key,
          sections.len()
        ));
      }

      match sections[0].to_lowercase().as_str() {
        "ctrl" => Ok(Key::Ctrl(get_single_char(sections[1]))),
        "alt" => Ok(Key::Alt(get_single_char(sections[1]))),
        "left" => Ok(Key::Left),
        "right" => Ok(Key::Right),
        "up" => Ok(Key::Up),
        "down" => Ok(Key::Down),
        "backspace" | "delete" => Ok(Key::Backspace),
        "del" => Ok(Key::Delete),
        "esc" | "escape" => Ok(Key::Esc),
        "pageup" => Ok(Key::PageUp),
        "pagedown" => Ok(Key::PageDown),
        "space" => Ok(Key::Char(' ')),
        "enter" => Ok(Key::Enter),
        "tab" => Ok(Key::Tab),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "ins" | "insert" => Ok(Key::Ins),
        "f0" => Ok(Key::F0),
        "f1" => Ok(Key::F1),
        "f2" => Ok(Key::F2),
        "f3" => Ok(Key::F3),
        "f4" => Ok(Key::F4),
        "f5" => Ok(Key::F5),
        "f6" => Ok(Key::F6),
        "f7" => Ok(Key::F7),
        "f8" => Ok(Key::F8),
        "f9" => Ok(Key::F9),
        "f10" => Ok(Key::F10),
        "f11" => Ok(Key::F11),
        "f12" => Ok(Key::F12),
        _ => Err(anyhow!("The key \"{}\" is unknown.", sections[0])),
      }
    }
  }
}

/// Public version of parse_key for use in app.rs
pub fn parse_key_public(key: String) -> Result<Key> {
  parse_key(key)
}

fn check_reserved_keys(key: Key) -> Result<()> {
  let reserved = [
    Key::Char('H'),
    Key::Char('M'),
    Key::Char('L'),
    Key::Up,
    Key::Down,
    Key::Left,
    Key::Right,
    Key::Backspace,
    Key::Enter,
  ];
  for item in reserved.iter() {
    if key == *item {
      // TODO: Add pretty print for key
      return Err(anyhow!(
        "The key {:?} is reserved and cannot be remapped",
        key
      ));
    }
  }
  Ok(())
}

/// Public version of check_reserved_keys for use in handlers
pub fn check_reserved_keys_public(key: Key) -> Result<()> {
  check_reserved_keys(key)
}

#[derive(Clone)]
pub struct UserConfigPaths {
  pub config_file_path: PathBuf,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyBindingsString {
  back: Option<String>,
  move_up: Option<String>,
  move_down: Option<String>,
  move_left: Option<String>,
  move_right: Option<String>,
  next_page: Option<String>,
  previous_page: Option<String>,
  jump_to_start: Option<String>,
  jump_to_end: Option<String>,
  jump_to_album: Option<String>,
  jump_to_artist_album: Option<String>,
  jump_to_context: Option<String>,
  manage_devices: Option<String>,
  decrease_volume: Option<String>,
  increase_volume: Option<String>,
  toggle_playback: Option<String>,
  seek_backwards: Option<String>,
  seek_forwards: Option<String>,
  next_track: Option<String>,
  previous_track: Option<String>,
  force_previous_track: Option<String>,
  help: Option<String>,
  shuffle: Option<String>,
  repeat: Option<String>,
  search: Option<String>,
  submit: Option<String>,
  copy_song_url: Option<String>,
  copy_album_url: Option<String>,
  audio_analysis: Option<String>,
  #[serde(alias = "basic_view")]
  lyrics_view: Option<String>,
  miniplayer_view: Option<String>,
  cover_art_view: Option<String>,
  add_item_to_queue: Option<String>,
  show_queue: Option<String>,
  open_settings: Option<String>,
  save_settings: Option<String>,
  listening_party: Option<String>,
  like_track: Option<String>,
  generate_recap: Option<String>,
}

#[derive(Clone)]
pub struct KeyBindings {
  pub back: Key,
  pub move_up: Key,
  pub move_down: Key,
  pub move_left: Key,
  pub move_right: Key,
  pub next_page: Key,
  pub previous_page: Key,
  pub jump_to_start: Key,
  pub jump_to_end: Key,
  pub jump_to_album: Key,
  pub jump_to_artist_album: Key,
  pub jump_to_context: Key,
  pub manage_devices: Key,
  pub decrease_volume: Key,
  pub increase_volume: Key,
  pub toggle_playback: Key,
  pub seek_backwards: Key,
  pub seek_forwards: Key,
  pub next_track: Key,
  pub previous_track: Key,
  pub force_previous_track: Key,
  pub help: Key,
  pub shuffle: Key,
  pub repeat: Key,
  pub search: Key,
  pub submit: Key,
  pub copy_song_url: Key,
  pub copy_album_url: Key,
  pub audio_analysis: Key,
  pub lyrics_view: Key,
  pub miniplayer_view: Key,
  pub cover_art_view: Key,
  pub add_item_to_queue: Key,
  pub show_queue: Key,
  pub open_settings: Key,
  pub save_settings: Key,
  pub listening_party: Key,
  pub like_track: Key,
  pub generate_recap: Key,
}

/// One internet-radio station in the config file: a display name plus the
/// direct stream URL. The same shape is used in `BehaviorConfigString` (file)
/// and `BehaviorConfig` (in-memory) — there is nothing to convert.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RadioStationConfig {
  pub name: String,
  pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioStationAddOutcome {
  Added,
  AlreadyExists,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BehaviorConfigString {
  pub seek_milliseconds: Option<u32>,
  pub volume_increment: Option<u8>,
  pub volume_percent: Option<u8>,
  pub tick_rate_milliseconds: Option<u64>,
  pub animation_tick_rate_milliseconds: Option<u64>,
  pub enable_text_emphasis: Option<bool>,
  pub show_loading_indicator: Option<bool>,
  pub enforce_wide_search_bar: Option<bool>,
  pub enable_global_song_count: Option<bool>,
  pub disable_mouse_inputs: Option<bool>,
  pub enable_discord_rpc: Option<bool>,
  pub discord_rpc_client_id: Option<String>,
  pub enable_announcements: Option<bool>,
  pub announcement_feed_url: Option<String>,
  pub seen_announcement_ids: Option<Vec<String>>,
  pub shuffle_enabled: Option<bool>,
  pub active_source: Option<String>,
  pub liked_icon: Option<String>,
  pub shuffle_icon: Option<String>,
  pub repeat_track_icon: Option<String>,
  pub repeat_context_icon: Option<String>,
  pub playing_icon: Option<String>,
  pub paused_icon: Option<String>,
  pub set_window_title: Option<bool>,
  pub visualizer_style: Option<VisualizerStyle>,
  pub dismissed_announcements: Option<Vec<String>>,
  pub relay_server_url: Option<String>,
  pub stop_after_current_track: Option<bool>,
  pub sidebar_width_percent: Option<u8>,
  pub playbar_height_rows: Option<u16>,
  pub library_height_percent: Option<u8>,
  pub startup_behavior: Option<StartupBehavior>,
  pub disable_auto_update: Option<bool>,
  pub auto_update_delay: Option<String>,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art: Option<bool>,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art_forced: Option<bool>,
  #[cfg(feature = "cover-art")]
  pub playbar_cover_art_size_percent: Option<u16>,
  pub keepawake_enabled: Option<bool>,
  pub enable_media_keys: Option<bool>,
  pub sync_token: Option<String>,
  pub local_music_path: Option<String>,
  pub subsonic_url: Option<String>,
  pub subsonic_username: Option<String>,
  pub subsonic_password: Option<String>,
  pub radio_stations: Option<Vec<RadioStationConfig>>,
  pub ytdlp_path: Option<String>,
}

#[derive(Clone)]
pub struct BehaviorConfig {
  pub seek_milliseconds: u32,
  pub volume_increment: u8,
  pub volume_percent: u8,
  pub tick_rate_milliseconds: u64,
  pub animation_tick_rate_milliseconds: u64,
  pub enable_text_emphasis: bool,
  pub show_loading_indicator: bool,
  pub enforce_wide_search_bar: bool,
  pub enable_global_song_count: bool,
  pub disable_mouse_inputs: bool,
  pub enable_discord_rpc: bool,
  pub discord_rpc_client_id: Option<String>,
  pub enable_announcements: bool,
  pub announcement_feed_url: Option<String>,
  pub seen_announcement_ids: Vec<String>,
  pub shuffle_enabled: bool,
  /// The last active source — persisted so it survives restarts.
  pub active_source: Source,
  pub liked_icon: String,
  pub shuffle_icon: String,
  pub repeat_track_icon: String,
  pub repeat_context_icon: String,
  pub playing_icon: String,
  pub paused_icon: String,
  pub set_window_title: bool,
  pub visualizer_style: VisualizerStyle,
  pub dismissed_announcements: Vec<String>,
  pub relay_server_url: String,
  pub stop_after_current_track: bool,
  pub sidebar_width_percent: u8,
  pub playbar_height_rows: u16,
  pub library_height_percent: u8,
  pub startup_behavior: StartupBehavior,
  pub disable_auto_update: bool,
  pub auto_update_delay: String,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art: bool,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art_forced: bool,
  #[cfg(feature = "cover-art")]
  pub playbar_cover_art_size_percent: u16,
  pub keepawake_enabled: bool,
  /// When false, spotatui ignores OS media-control commands (headphone
  /// play/pause/skip buttons, media keys, MPRIS/SMTC/Now Playing, playerctl).
  /// It still publishes track metadata to the OS; it just stops reacting.
  pub enable_media_keys: bool,
  pub sync_token: Option<String>,
  /// Filesystem path to the local music library root (browsed by the Local
  /// Files screen). Defaults to the OS music directory; `None` if unavailable.
  pub local_music_path: Option<String>,
  /// Base URL of the Subsonic/OpenSubsonic server (e.g.
  /// `https://demo.navidrome.org`). `None` until configured.
  pub subsonic_url: Option<String>,
  /// Subsonic account username.
  pub subsonic_username: Option<String>,
  /// Subsonic account password. **Stored in plaintext in the YAML config** —
  /// prefer the `SPOTATUI_SUBSONIC_PASSWORD` environment variable, which
  /// overrides this field at connection time and is never written to disk.
  pub subsonic_password: Option<String>,
  /// The user's internet-radio station list, shown in the sidebar when the
  /// Radio source is active. Stations found via the in-app directory search
  /// are not persisted here (yet) — this list is hand-maintained in the config.
  pub radio_stations: Vec<RadioStationConfig>,
  /// Path to the `yt-dlp` binary used by the YouTube source. `None` resolves
  /// plain `yt-dlp` through `$PATH`.
  pub ytdlp_path: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserConfigString {
  keybindings: Option<KeyBindingsString>,
  behavior: Option<BehaviorConfigString>,
  theme: Option<UserTheme>,
  plugin_commands: Option<HashMap<String, String>>,
}

#[derive(Clone)]
pub struct UserConfig {
  pub keys: KeyBindings,
  pub theme: Theme,
  pub current_preset: ThemePreset,
  pub custom_theme: Theme,
  pub behavior: BehaviorConfig,
  pub path_to_config: Option<UserConfigPaths>,
  /// Keybindings for plugin commands: key -> command name.
  pub plugin_command_keys: HashMap<Key, String>,
}

impl UserConfig {
  /// Get the spotatui app config directory (~/.config/spotatui).
  /// Returns None if $HOME is not set.
  #[cfg(feature = "self-update")]
  pub fn get_app_config_dir() -> Option<PathBuf> {
    default_app_config_dir()
  }

  pub fn new() -> UserConfig {
    // Detect platform for platform-specific defaults
    #[cfg(target_os = "macos")]
    let is_macos = true;
    #[cfg(not(target_os = "macos"))]
    let is_macos = false;

    UserConfig {
      theme: Default::default(),
      current_preset: ThemePreset::Default,
      custom_theme: Default::default(),
      keys: KeyBindings {
        back: Key::Char('q'),
        move_up: Key::Char('k'),
        move_down: Key::Char('j'),
        move_left: Key::Char('h'),
        move_right: Key::Char('l'),
        next_page: Key::Ctrl('d'),
        previous_page: Key::Ctrl('u'),
        jump_to_start: Key::Ctrl('a'),
        jump_to_end: Key::Ctrl('e'),
        jump_to_album: Key::Char('a'),
        jump_to_artist_album: Key::Char('A'),
        jump_to_context: Key::Char('o'),
        manage_devices: Key::Char('d'),
        decrease_volume: Key::Char('-'),
        increase_volume: Key::Char('+'),
        toggle_playback: Key::Char(' '),
        seek_backwards: Key::Char('<'),
        seek_forwards: Key::Char('>'),
        next_track: Key::Char('n'),
        previous_track: Key::Char('p'),
        force_previous_track: Key::Char('P'),
        help: Key::Char('?'),
        shuffle: Key::Ctrl('s'),
        repeat: Key::Ctrl('r'),
        search: Key::Char('/'),
        submit: Key::Enter,
        copy_song_url: Key::Char('c'),
        copy_album_url: Key::Char('C'),
        audio_analysis: Key::Char('v'),
        lyrics_view: Key::Char('B'),
        miniplayer_view: Key::Char('T'),
        cover_art_view: Key::Char('G'),
        add_item_to_queue: Key::Char('z'),
        show_queue: Key::Char('Q'),
        // On macOS, use Ctrl+, for settings since Alt+, produces ≤ on most keyboard layouts
        // On other platforms, keep Alt+, for consistency with many apps
        open_settings: if is_macos {
          Key::Ctrl(',')
        } else {
          Key::Alt(',')
        },
        save_settings: Key::Alt('s'),
        listening_party: Key::Ctrl('p'),
        like_track: Key::Char('F'),
        generate_recap: Key::Char('R'),
      },
      plugin_command_keys: HashMap::new(),
      behavior: BehaviorConfig {
        seek_milliseconds: 5 * 1000,
        volume_increment: 10,
        volume_percent: 100,
        tick_rate_milliseconds: DEFAULT_TICK_RATE_MILLISECONDS,
        animation_tick_rate_milliseconds: DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
        enable_text_emphasis: true,
        show_loading_indicator: true,
        enforce_wide_search_bar: false,
        enable_global_song_count: true,
        disable_mouse_inputs: false,
        enable_discord_rpc: true,
        discord_rpc_client_id: None,
        enable_announcements: true,
        announcement_feed_url: None,
        seen_announcement_ids: Vec::new(),
        shuffle_enabled: false,
        active_source: Source::default(),
        liked_icon: "♥".to_string(),
        shuffle_icon: "🔀".to_string(),
        repeat_track_icon: "🔂".to_string(),
        repeat_context_icon: "🔁".to_string(),
        playing_icon: "▶".to_string(),
        paused_icon: "⏸".to_string(),
        set_window_title: true,
        visualizer_style: VisualizerStyle::default(),
        dismissed_announcements: Vec::new(),
        relay_server_url: "wss://spotatui-party.spotatui.workers.dev/ws".to_string(),
        stop_after_current_track: false,
        sidebar_width_percent: 20,
        playbar_height_rows: 6,
        library_height_percent: 30,
        startup_behavior: StartupBehavior::Continue,
        disable_auto_update: false,
        auto_update_delay: "0".to_string(),
        #[cfg(feature = "cover-art")]
        draw_cover_art: true,
        #[cfg(feature = "cover-art")]
        draw_cover_art_forced: false,
        #[cfg(feature = "cover-art")]
        playbar_cover_art_size_percent: 100,
        keepawake_enabled: true,
        enable_media_keys: true,
        sync_token: None,
        local_music_path: dirs::audio_dir().map(|p| p.to_string_lossy().to_string()),
        subsonic_url: None,
        subsonic_username: None,
        subsonic_password: None,
        radio_stations: Vec::new(),
        ytdlp_path: None,
      },
      path_to_config: None,
    }
  }

  pub fn get_or_build_paths(&mut self) -> Result<()> {
    match default_app_config_dir() {
      Some(app_config_dir) => {
        let home_config_dir = app_config_dir
          .parent()
          .ok_or_else(|| anyhow!("Invalid app config directory"))?;

        if !home_config_dir.exists() {
          fs::create_dir(home_config_dir)?;
        }

        if !app_config_dir.exists() {
          fs::create_dir(&app_config_dir)?;
        }

        // Restrict the app's own config directory (holds config.yml, which
        // carries the Subsonic password and party sync_token in cleartext,
        // plus the Spotify token cache) to owner-only. Never touch
        // `home_config_dir` (`~/.config`) — that's shared with every other
        // application on the system.
        #[cfg(unix)]
        {
          use std::os::unix::fs::PermissionsExt;
          fs::set_permissions(&app_config_dir, fs::Permissions::from_mode(0o700))?;
        }

        let config_file_path = &app_config_dir.join(FILE_NAME);

        let paths = UserConfigPaths {
          config_file_path: config_file_path.to_path_buf(),
        };
        self.path_to_config = Some(paths);
        Ok(())
      }
      None => Err(anyhow!("No $HOME directory found for client config")),
    }
  }

  pub fn load_keybindings(&mut self, keybindings: KeyBindingsString) -> Result<()> {
    macro_rules! to_keys {
      ($name: ident) => {
        if let Some(key_string) = keybindings.$name {
          self.keys.$name = parse_key(key_string)?;
        }
      };
    }

    to_keys!(back);
    to_keys!(move_up);
    to_keys!(move_down);
    to_keys!(move_left);
    to_keys!(move_right);
    to_keys!(next_page);
    to_keys!(previous_page);
    to_keys!(jump_to_start);
    to_keys!(jump_to_end);
    to_keys!(jump_to_album);
    to_keys!(jump_to_artist_album);
    to_keys!(jump_to_context);
    to_keys!(manage_devices);
    to_keys!(decrease_volume);
    to_keys!(increase_volume);
    to_keys!(toggle_playback);
    to_keys!(seek_backwards);
    to_keys!(seek_forwards);
    to_keys!(next_track);
    to_keys!(previous_track);
    to_keys!(force_previous_track);
    to_keys!(help);
    to_keys!(shuffle);
    to_keys!(repeat);
    to_keys!(search);
    to_keys!(submit);
    to_keys!(copy_song_url);
    to_keys!(copy_album_url);
    to_keys!(audio_analysis);
    to_keys!(lyrics_view);
    to_keys!(miniplayer_view);
    to_keys!(cover_art_view);
    to_keys!(add_item_to_queue);
    to_keys!(show_queue);
    to_keys!(open_settings);
    to_keys!(save_settings);
    to_keys!(listening_party);
    to_keys!(like_track);
    to_keys!(generate_recap);

    Ok(())
  }

  pub fn load_theme(&mut self, theme: UserTheme) -> Result<()> {
    // Individual color fields populate the custom_theme — they only
    // become the active theme when current_preset is Custom.
    macro_rules! to_theme_item {
      ($name: ident) => {
        if let Some(theme_item) = theme.$name {
          self.custom_theme.$name = parse_theme_item(&theme_item)?;
        }
      };
    }
    // Check if any colour values exist in config already`
    let has_color_values = theme.active.is_some()
      || theme.banner.is_some()
      || theme.error_border.is_some()
      || theme.error_text.is_some()
      || theme.hint.is_some()
      || theme.hovered.is_some()
      || theme.inactive.is_some()
      || theme.playbar_background.is_some()
      || theme.playbar_progress.is_some()
      || theme.playbar_progress_text.is_some()
      || theme.playbar_text.is_some()
      || theme.selected.is_some()
      || theme.text.is_some()
      || theme.background.is_some()
      || theme.header.is_some()
      || theme.highlighted_lyrics.is_some();

    to_theme_item!(active);
    to_theme_item!(banner);
    to_theme_item!(error_border);
    to_theme_item!(error_text);
    to_theme_item!(hint);
    to_theme_item!(hovered);
    to_theme_item!(inactive);
    to_theme_item!(playbar_background);
    to_theme_item!(playbar_progress);
    to_theme_item!(playbar_progress_text);
    to_theme_item!(playbar_text);
    to_theme_item!(selected);
    to_theme_item!(text);
    to_theme_item!(background);
    to_theme_item!(header);
    to_theme_item!(highlighted_lyrics);

    // If the preset value exists in the config, we load it
    if let Some(preset_name) = theme.preset {
      self.current_preset = ThemePreset::from_name(&preset_name);
    } else if has_color_values {
      // If there is no preset value, or it is malformed,
      // and if the config exists and has some theme colours set:
      // we handle backwards compatibility for old theme configs.
      // Set to Custom on first load after the upgrade.
      self.current_preset = ThemePreset::Custom;
    }

    self.theme = match self.current_preset {
      ThemePreset::Custom => self.custom_theme,
      preset => preset.to_theme(),
    };

    Ok(())
  }

  pub fn load_behaviorconfig(&mut self, behavior_config: BehaviorConfigString) -> Result<()> {
    if let Some(behavior_string) = behavior_config.seek_milliseconds {
      self.behavior.seek_milliseconds = behavior_string;
    }

    if let Some(behavior_string) = behavior_config.volume_increment {
      if behavior_string > 100 {
        return Err(anyhow!(
          "Volume increment must be between 0 and 100, is {}",
          behavior_string,
        ));
      }
      self.behavior.volume_increment = behavior_string;
    }

    if let Some(volume) = behavior_config.volume_percent {
      self.behavior.volume_percent = volume.min(100);
    }

    let loaded_tick_rate = behavior_config.tick_rate_milliseconds;
    let loaded_animation_tick_rate = behavior_config.animation_tick_rate_milliseconds;

    if let Some(tick_rate) = loaded_tick_rate {
      let tick_rate = validate_tick_rate_milliseconds(tick_rate, "Tick rate")?;
      // Before animation ticks existed, save_config wrote the old 16ms default
      // into user configs. Treat the legacy 16ms normal tick as the old default
      // when animation ticks are absent or still equal to the animation default,
      // so upgraded users get the new normal UI cadence without manual edits.
      self.behavior.tick_rate_milliseconds = if tick_rate
        == DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS
        && loaded_animation_tick_rate
          .map(|animation_tick_rate| {
            animation_tick_rate == DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS
          })
          .unwrap_or(true)
      {
        DEFAULT_TICK_RATE_MILLISECONDS
      } else {
        tick_rate
      };
    }

    if let Some(tick_rate) = loaded_animation_tick_rate {
      self.behavior.animation_tick_rate_milliseconds =
        validate_tick_rate_milliseconds(tick_rate, "Animation tick rate")?;
    }

    if let Some(text_emphasis) = behavior_config.enable_text_emphasis {
      self.behavior.enable_text_emphasis = text_emphasis;
    }

    if let Some(loading_indicator) = behavior_config.show_loading_indicator {
      self.behavior.show_loading_indicator = loading_indicator;
    }

    if let Some(wide_search_bar) = behavior_config.enforce_wide_search_bar {
      self.behavior.enforce_wide_search_bar = wide_search_bar;
    }

    if let Some(liked_icon) = behavior_config.liked_icon {
      self.behavior.liked_icon = liked_icon;
    }

    if let Some(paused_icon) = behavior_config.paused_icon {
      self.behavior.paused_icon = paused_icon;
    }

    if let Some(playing_icon) = behavior_config.playing_icon {
      self.behavior.playing_icon = playing_icon;
    }

    if let Some(shuffle_icon) = behavior_config.shuffle_icon {
      self.behavior.shuffle_icon = shuffle_icon;
    }

    if let Some(repeat_track_icon) = behavior_config.repeat_track_icon {
      self.behavior.repeat_track_icon = repeat_track_icon;
    }

    if let Some(repeat_context_icon) = behavior_config.repeat_context_icon {
      self.behavior.repeat_context_icon = repeat_context_icon;
    }

    if let Some(set_window_title) = behavior_config.set_window_title {
      self.behavior.set_window_title = set_window_title;
    }

    if let Some(enable_global_song_count) = behavior_config.enable_global_song_count {
      self.behavior.enable_global_song_count = enable_global_song_count;
    }

    if let Some(disable_mouse_inputs) = behavior_config.disable_mouse_inputs {
      self.behavior.disable_mouse_inputs = disable_mouse_inputs;
    }

    if let Some(enable_discord_rpc) = behavior_config.enable_discord_rpc {
      self.behavior.enable_discord_rpc = enable_discord_rpc;
    }

    if let Some(enable_announcements) = behavior_config.enable_announcements {
      self.behavior.enable_announcements = enable_announcements;
    }

    if let Some(announcement_feed_url) = behavior_config.announcement_feed_url {
      let trimmed = announcement_feed_url.trim();
      self.behavior.announcement_feed_url = if trimmed.is_empty() {
        None
      } else {
        Some(trimmed.to_string())
      };
    }

    if let Some(seen_announcement_ids) = behavior_config.seen_announcement_ids {
      self.behavior.seen_announcement_ids = seen_announcement_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    }

    if let Some(discord_rpc_client_id) = behavior_config.discord_rpc_client_id {
      self.behavior.discord_rpc_client_id = Some(discord_rpc_client_id);
    }

    if let Some(shuffle_enabled) = behavior_config.shuffle_enabled {
      self.behavior.shuffle_enabled = shuffle_enabled;
    }

    if let Some(active_source_str) = behavior_config.active_source {
      self.behavior.active_source = Source::from_config_str(&active_source_str);
    }

    if let Some(visualizer_style) = behavior_config.visualizer_style {
      self.behavior.visualizer_style = visualizer_style;
    }

    if let Some(dismissed_announcements) = behavior_config.dismissed_announcements {
      self.behavior.dismissed_announcements = dismissed_announcements
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    }

    if let Some(relay_server_url) = behavior_config.relay_server_url {
      let trimmed = relay_server_url.trim();
      if !trimmed.is_empty() {
        self.behavior.relay_server_url = trimmed.to_string();
      }
    }

    if let Some(sync_token) = behavior_config.sync_token {
      let trimmed = sync_token.trim();
      if trimmed.is_empty() {
        self.behavior.sync_token = None;
      } else {
        self.behavior.sync_token = Some(trimmed.to_string());
      }
    }

    if let Some(stop_after_current_track) = behavior_config.stop_after_current_track {
      self.behavior.stop_after_current_track = stop_after_current_track;
    }

    if let Some(sidebar_width_percent) = behavior_config.sidebar_width_percent {
      self.behavior.sidebar_width_percent = sidebar_width_percent.min(100);
    }

    if let Some(playbar_height_rows) = behavior_config.playbar_height_rows {
      self.behavior.playbar_height_rows = playbar_height_rows;
    }

    if let Some(library_height_percent) = behavior_config.library_height_percent {
      self.behavior.library_height_percent = library_height_percent.min(100);
    }

    if let Some(startup_behavior) = behavior_config.startup_behavior {
      self.behavior.startup_behavior = startup_behavior;
    }

    if let Some(disable_auto_update) = behavior_config.disable_auto_update {
      self.behavior.disable_auto_update = disable_auto_update;
    }

    if let Some(auto_update_delay) = behavior_config.auto_update_delay {
      parse_update_delay_secs(&auto_update_delay)
        .map_err(|e| anyhow!("Invalid auto-update delay: {e}"))?;
      self.behavior.auto_update_delay = auto_update_delay;
    }

    #[cfg(feature = "cover-art")]
    if let Some(draw_cover_art) = behavior_config.draw_cover_art {
      self.behavior.draw_cover_art = draw_cover_art;
    }

    #[cfg(feature = "cover-art")]
    if let Some(draw_cover_art_forced) = behavior_config.draw_cover_art_forced {
      self.behavior.draw_cover_art_forced = draw_cover_art_forced;
    }
    #[cfg(feature = "cover-art")]
    if let Some(playbar_cover_art_size_percent) = behavior_config.playbar_cover_art_size_percent {
      self.behavior.playbar_cover_art_size_percent =
        clamp_playbar_cover_art_size_percent(playbar_cover_art_size_percent);
    }
    if let Some(keepawake_enabled) = behavior_config.keepawake_enabled {
      self.behavior.keepawake_enabled = keepawake_enabled;
    }
    if let Some(enable_media_keys) = behavior_config.enable_media_keys {
      self.behavior.enable_media_keys = enable_media_keys;
    }
    if let Some(local_music_path) = behavior_config.local_music_path {
      let trimmed = local_music_path.trim();
      self.behavior.local_music_path = if trimmed.is_empty() {
        None
      } else {
        Some(trimmed.to_string())
      };
    }
    // Subsonic server config: trim-to-None so blank keys read as unset.
    let trim_to_none = |value: Option<String>| -> Option<String> {
      value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
          None
        } else {
          Some(trimmed.to_string())
        }
      })
    };
    if let Some(subsonic_url) = trim_to_none(behavior_config.subsonic_url) {
      self.behavior.subsonic_url = Some(subsonic_url);
    }
    if let Some(subsonic_username) = trim_to_none(behavior_config.subsonic_username) {
      self.behavior.subsonic_username = Some(subsonic_username);
    }
    if let Some(subsonic_password) = trim_to_none(behavior_config.subsonic_password) {
      self.behavior.subsonic_password = Some(subsonic_password);
    }
    if let Some(radio_stations) = behavior_config.radio_stations {
      // Drop entries missing a name or URL rather than failing the whole
      // config; the dispatch filters again defensively at load time.
      self.behavior.radio_stations = radio_stations
        .into_iter()
        .filter(|s| !s.name.trim().is_empty() && !s.url.trim().is_empty())
        .collect();
    }
    if let Some(ytdlp_path) = trim_to_none(behavior_config.ytdlp_path) {
      self.behavior.ytdlp_path = Some(ytdlp_path);
    }
    Ok(())
  }

  fn named_action_keys(&self) -> Vec<Key> {
    let k = &self.keys;
    vec![
      k.back,
      k.move_up,
      k.move_down,
      k.move_left,
      k.move_right,
      k.next_page,
      k.previous_page,
      k.jump_to_start,
      k.jump_to_end,
      k.jump_to_album,
      k.jump_to_artist_album,
      k.jump_to_context,
      k.manage_devices,
      k.decrease_volume,
      k.increase_volume,
      k.toggle_playback,
      k.seek_backwards,
      k.seek_forwards,
      k.next_track,
      k.previous_track,
      k.force_previous_track,
      k.help,
      k.shuffle,
      k.repeat,
      k.search,
      k.submit,
      k.copy_song_url,
      k.copy_album_url,
      k.audio_analysis,
      k.lyrics_view,
      k.miniplayer_view,
      k.cover_art_view,
      k.add_item_to_queue,
      k.show_queue,
      k.open_settings,
      k.save_settings,
      k.listening_party,
      k.like_track,
      k.generate_recap,
    ]
  }

  pub fn load_plugin_commands(&mut self, entries: HashMap<String, String>) {
    let named_keys = self.named_action_keys();
    let mut result: HashMap<Key, String> = HashMap::new();
    for (cmd_name, key_str) in entries {
      let key = match parse_key(key_str.clone()) {
        Ok(k) => k,
        Err(e) => {
          log::warn!(
            "[config] plugin_commands: skipping '{cmd_name}': invalid key '{key_str}': {e}"
          );
          continue;
        }
      };
      if let Err(e) = check_reserved_keys(key) {
        log::warn!("[config] plugin_commands: skipping '{cmd_name}': {e}");
        continue;
      }
      if named_keys.contains(&key) {
        log::warn!(
          "[config] plugin_commands: skipping '{cmd_name}': key '{key_str}' collides with a named action"
        );
        continue;
      }
      result.insert(key, cmd_name);
    }
    self.plugin_command_keys = result;
  }

  pub fn load_config(&mut self) -> Result<()> {
    let paths = match &self.path_to_config {
      Some(path) => path,
      None => {
        self.get_or_build_paths()?;
        self.path_to_config.as_ref().unwrap()
      }
    };
    if paths.config_file_path.exists() {
      let config_string = fs::read_to_string(&paths.config_file_path)?;
      // serde fails if file is empty
      if config_string.trim().is_empty() {
        return Ok(());
      }

      let config_yml: UserConfigString = serde_yaml::from_str(&config_string)?;

      if let Some(keybindings) = config_yml.keybindings.clone() {
        self.load_keybindings(keybindings)?;
      }

      if let Some(behavior) = config_yml.behavior {
        self.load_behaviorconfig(behavior)?;
      }
      if let Some(theme) = config_yml.theme {
        self.load_theme(theme)?;
      }
      if let Some(plugin_commands) = config_yml.plugin_commands {
        self.load_plugin_commands(plugin_commands);
      }

      Ok(())
    } else {
      Ok(())
    }
  }

  /// Save the current configuration to the config file
  pub fn save_config(&self) -> Result<()> {
    let paths = match &self.path_to_config {
      Some(path) => path,
      None => return Err(anyhow!("Config path not initialized")),
    };

    // Helper to build behavior config from current values
    let build_behavior = || BehaviorConfigString {
      seek_milliseconds: Some(self.behavior.seek_milliseconds),
      volume_increment: Some(self.behavior.volume_increment),
      volume_percent: Some(self.behavior.volume_percent),
      tick_rate_milliseconds: Some(self.behavior.tick_rate_milliseconds),
      animation_tick_rate_milliseconds: Some(self.behavior.animation_tick_rate_milliseconds),
      enable_text_emphasis: Some(self.behavior.enable_text_emphasis),
      show_loading_indicator: Some(self.behavior.show_loading_indicator),
      enforce_wide_search_bar: Some(self.behavior.enforce_wide_search_bar),
      enable_global_song_count: Some(self.behavior.enable_global_song_count),
      disable_mouse_inputs: Some(self.behavior.disable_mouse_inputs),
      enable_discord_rpc: Some(self.behavior.enable_discord_rpc),
      discord_rpc_client_id: self.behavior.discord_rpc_client_id.clone(),
      enable_announcements: Some(self.behavior.enable_announcements),
      announcement_feed_url: self.behavior.announcement_feed_url.clone(),
      seen_announcement_ids: Some(self.behavior.seen_announcement_ids.clone()),
      shuffle_enabled: Some(self.behavior.shuffle_enabled),
      active_source: Some(self.behavior.active_source.to_config_str().to_string()),
      liked_icon: Some(self.behavior.liked_icon.clone()),
      shuffle_icon: Some(self.behavior.shuffle_icon.clone()),
      repeat_track_icon: Some(self.behavior.repeat_track_icon.clone()),
      repeat_context_icon: Some(self.behavior.repeat_context_icon.clone()),
      playing_icon: Some(self.behavior.playing_icon.clone()),
      paused_icon: Some(self.behavior.paused_icon.clone()),
      set_window_title: Some(self.behavior.set_window_title),
      visualizer_style: Some(self.behavior.visualizer_style),
      dismissed_announcements: Some(self.behavior.dismissed_announcements.clone()),
      relay_server_url: Some(self.behavior.relay_server_url.clone()),
      sync_token: self.behavior.sync_token.clone(),
      local_music_path: self.behavior.local_music_path.clone(),
      subsonic_url: self.behavior.subsonic_url.clone(),
      subsonic_username: self.behavior.subsonic_username.clone(),
      subsonic_password: self.behavior.subsonic_password.clone(),
      radio_stations: if self.behavior.radio_stations.is_empty() {
        None
      } else {
        Some(self.behavior.radio_stations.clone())
      },
      ytdlp_path: self.behavior.ytdlp_path.clone(),
      stop_after_current_track: Some(self.behavior.stop_after_current_track),
      sidebar_width_percent: Some(self.behavior.sidebar_width_percent),
      playbar_height_rows: Some(self.behavior.playbar_height_rows),
      library_height_percent: Some(self.behavior.library_height_percent),
      startup_behavior: Some(self.behavior.startup_behavior),
      disable_auto_update: Some(self.behavior.disable_auto_update),
      auto_update_delay: Some(self.behavior.auto_update_delay.clone()),
      #[cfg(feature = "cover-art")]
      draw_cover_art: Some(self.behavior.draw_cover_art),
      #[cfg(feature = "cover-art")]
      draw_cover_art_forced: Some(self.behavior.draw_cover_art_forced),
      #[cfg(feature = "cover-art")]
      playbar_cover_art_size_percent: Some(self.behavior.playbar_cover_art_size_percent),
      keepawake_enabled: Some(self.behavior.keepawake_enabled),
      enable_media_keys: Some(self.behavior.enable_media_keys),
    };

    // Helper to convert Key to config string
    let key_to_config_string = |key: Key| -> String {
      match key {
        Key::Char(' ') => "space".to_string(),
        Key::Char(c) => c.to_string(),
        Key::Ctrl(c) => format!("ctrl-{}", c),
        Key::Alt(c) => format!("alt-{}", c),
        Key::Enter => "enter".to_string(),
        Key::Tab => "tab".to_string(),
        Key::Esc => "esc".to_string(),
        Key::Backspace => "backspace".to_string(),
        Key::Delete => "del".to_string(),
        Key::Left => "left".to_string(),
        Key::Right => "right".to_string(),
        Key::Up => "up".to_string(),
        Key::Down => "down".to_string(),
        Key::Home => "home".to_string(),
        Key::End => "end".to_string(),
        Key::Ins => "ins".to_string(),
        Key::PageUp => "pageup".to_string(),
        Key::PageDown => "pagedown".to_string(),
        Key::F0 => "f0".to_string(),
        Key::F1 => "f1".to_string(),
        Key::F2 => "f2".to_string(),
        Key::F3 => "f3".to_string(),
        Key::F4 => "f4".to_string(),
        Key::F5 => "f5".to_string(),
        Key::F6 => "f6".to_string(),
        Key::F7 => "f7".to_string(),
        Key::F8 => "f8".to_string(),
        Key::F9 => "f9".to_string(),
        Key::F10 => "f10".to_string(),
        Key::F11 => "f11".to_string(),
        Key::F12 => "f12".to_string(),
        _ => "unknown".to_string(),
      }
    };

    // Helper to build keybindings config from current values
    let build_keybindings = || KeyBindingsString {
      back: Some(key_to_config_string(self.keys.back)),
      move_up: Some(key_to_config_string(self.keys.move_up)),
      move_down: Some(key_to_config_string(self.keys.move_down)),
      move_left: Some(key_to_config_string(self.keys.move_left)),
      move_right: Some(key_to_config_string(self.keys.move_right)),
      next_page: Some(key_to_config_string(self.keys.next_page)),
      previous_page: Some(key_to_config_string(self.keys.previous_page)),
      jump_to_start: Some(key_to_config_string(self.keys.jump_to_start)),
      jump_to_end: Some(key_to_config_string(self.keys.jump_to_end)),
      jump_to_album: Some(key_to_config_string(self.keys.jump_to_album)),
      jump_to_artist_album: Some(key_to_config_string(self.keys.jump_to_artist_album)),
      jump_to_context: Some(key_to_config_string(self.keys.jump_to_context)),
      manage_devices: Some(key_to_config_string(self.keys.manage_devices)),
      decrease_volume: Some(key_to_config_string(self.keys.decrease_volume)),
      increase_volume: Some(key_to_config_string(self.keys.increase_volume)),
      toggle_playback: Some(key_to_config_string(self.keys.toggle_playback)),
      seek_backwards: Some(key_to_config_string(self.keys.seek_backwards)),
      seek_forwards: Some(key_to_config_string(self.keys.seek_forwards)),
      next_track: Some(key_to_config_string(self.keys.next_track)),
      previous_track: Some(key_to_config_string(self.keys.previous_track)),
      force_previous_track: Some(key_to_config_string(self.keys.force_previous_track)),
      help: Some(key_to_config_string(self.keys.help)),
      shuffle: Some(key_to_config_string(self.keys.shuffle)),
      repeat: Some(key_to_config_string(self.keys.repeat)),
      search: Some(key_to_config_string(self.keys.search)),
      submit: Some(key_to_config_string(self.keys.submit)),
      copy_song_url: Some(key_to_config_string(self.keys.copy_song_url)),
      copy_album_url: Some(key_to_config_string(self.keys.copy_album_url)),
      audio_analysis: Some(key_to_config_string(self.keys.audio_analysis)),
      lyrics_view: Some(key_to_config_string(self.keys.lyrics_view)),
      miniplayer_view: Some(key_to_config_string(self.keys.miniplayer_view)),
      cover_art_view: Some(key_to_config_string(self.keys.cover_art_view)),
      add_item_to_queue: Some(key_to_config_string(self.keys.add_item_to_queue)),
      show_queue: Some(key_to_config_string(self.keys.show_queue)),
      open_settings: Some(key_to_config_string(self.keys.open_settings)),
      save_settings: Some(key_to_config_string(self.keys.save_settings)),
      listening_party: Some(key_to_config_string(self.keys.listening_party)),
      like_track: Some(key_to_config_string(self.keys.like_track)),
      generate_recap: Some(key_to_config_string(self.keys.generate_recap)),
    };

    // Helper to build theme config from current values
    let build_theme = || UserTheme {
      preset: Some(self.current_preset.name().to_string()),
      active: Some(color_to_string(self.custom_theme.active)),
      banner: Some(color_to_string(self.custom_theme.banner)),
      error_border: Some(color_to_string(self.custom_theme.error_border)),
      error_text: Some(color_to_string(self.custom_theme.error_text)),
      hint: Some(color_to_string(self.custom_theme.hint)),
      hovered: Some(color_to_string(self.custom_theme.hovered)),
      inactive: Some(color_to_string(self.custom_theme.inactive)),
      playbar_background: Some(color_to_string(self.custom_theme.playbar_background)),
      playbar_progress: Some(color_to_string(self.custom_theme.playbar_progress)),
      playbar_progress_text: Some(color_to_string(self.custom_theme.playbar_progress_text)),
      playbar_text: Some(color_to_string(self.custom_theme.playbar_text)),
      selected: Some(color_to_string(self.custom_theme.selected)),
      text: Some(color_to_string(self.custom_theme.text)),
      background: Some(color_to_string(self.custom_theme.background)),
      header: Some(color_to_string(self.custom_theme.header)),
      highlighted_lyrics: Some(color_to_string(self.custom_theme.highlighted_lyrics)),
    };

    // If the file exists, try to read it first to preserve keybindings
    let final_config = if paths.config_file_path.exists() {
      let config_string = fs::read_to_string(&paths.config_file_path)?;
      if !config_string.trim().is_empty() {
        let mut existing: UserConfigString = serde_yaml::from_str(&config_string)?;
        // Update behavior, theme, and keybindings
        existing.behavior = Some(build_behavior());
        existing.theme = Some(build_theme());
        existing.keybindings = Some(build_keybindings());
        existing
      } else {
        UserConfigString {
          keybindings: Some(build_keybindings()),
          behavior: Some(build_behavior()),
          theme: Some(build_theme()),
          plugin_commands: None,
        }
      }
    } else {
      UserConfigString {
        keybindings: Some(build_keybindings()),
        behavior: Some(build_behavior()),
        theme: Some(build_theme()),
        plugin_commands: None,
      }
    };

    // Serialize to a String/bytes first, then write via a private-file helper
    // (0o600 on Unix — this file carries the Subsonic password and party
    // sync_token in cleartext, so it deserves the same protection as the
    // Spotify token cache) using a temp-file + atomic rename, so a crash
    // mid-write can't corrupt the config. Do not log `content_yml`: it may
    // contain the plaintext password/sync_token.
    let content_yml = serde_yaml::to_string(&final_config)?;
    let tmp_path = paths.config_file_path.with_extension("yml.tmp");
    crate::core::auth::write_private_file(&tmp_path, content_yml.as_bytes())?;
    fs::rename(&tmp_path, &paths.config_file_path)?;

    Ok(())
  }

  pub fn padded_liked_icon(&self) -> String {
    format!("{} ", &self.behavior.liked_icon)
  }

  pub fn add_radio_station(
    &mut self,
    name: impl AsRef<str>,
    url: impl AsRef<str>,
  ) -> Result<RadioStationAddOutcome> {
    let name = name.as_ref().trim();
    let url = url.as_ref().trim();

    if name.is_empty() {
      return Err(anyhow!("Radio station name is empty"));
    }
    if url.is_empty() {
      return Err(anyhow!("Radio station URL is empty"));
    }

    if self
      .behavior
      .radio_stations
      .iter()
      .any(|station| station.url.trim() == url)
    {
      return Ok(RadioStationAddOutcome::AlreadyExists);
    }

    self.behavior.radio_stations.push(RadioStationConfig {
      name: name.to_string(),
      url: url.to_string(),
    });

    if let Err(error) = self.save_config() {
      self.behavior.radio_stations.pop();
      return Err(error);
    }

    Ok(RadioStationAddOutcome::Added)
  }

  pub fn remove_radio_station_by_url(
    &mut self,
    url: impl AsRef<str>,
  ) -> Result<Option<RadioStationConfig>> {
    let url = url.as_ref().trim();
    if url.is_empty() {
      return Err(anyhow!("Radio station URL is empty"));
    }

    let Some(index) = self
      .behavior
      .radio_stations
      .iter()
      .position(|station| station.url.trim() == url)
    else {
      return Ok(None);
    };

    let removed = self.behavior.radio_stations.remove(index);

    if let Err(error) = self.save_config() {
      self.behavior.radio_stations.insert(index, removed);
      return Err(error);
    }

    Ok(Some(removed))
  }

  pub fn mark_announcement_seen(&mut self, announcement_id: impl Into<String>) {
    let id = announcement_id.into();
    if id.is_empty() {
      return;
    }

    if !self
      .behavior
      .seen_announcement_ids
      .iter()
      .any(|seen| seen == &id)
    {
      self.behavior.seen_announcement_ids.push(id);
    }
  }

  #[cfg(feature = "cover-art")]
  pub fn do_draw_cover_art(&self, full_image_support: bool) -> bool {
    self.behavior.draw_cover_art && (self.behavior.draw_cover_art_forced || full_image_support)
  }
}

pub fn parse_theme_item(theme_item: &str) -> Result<Color> {
  let color = match theme_item {
    "Reset" => Color::Reset,
    "Black" => Color::Black,
    "Red" => Color::Red,
    "Green" => Color::Green,
    "Yellow" => Color::Yellow,
    "Blue" => Color::Blue,
    "Magenta" => Color::Magenta,
    "Cyan" => Color::Cyan,
    "Gray" => Color::Gray,
    "DarkGray" => Color::DarkGray,
    "LightRed" => Color::LightRed,
    "LightGreen" => Color::LightGreen,
    "LightYellow" => Color::LightYellow,
    "LightBlue" => Color::LightBlue,
    "LightMagenta" => Color::LightMagenta,
    "LightCyan" => Color::LightCyan,
    "White" => Color::White,
    _ => {
      let colors = theme_item.split(',').collect::<Vec<&str>>();
      if let (Some(r), Some(g), Some(b)) = (colors.first(), colors.get(1), colors.get(2)) {
        Color::Rgb(
          r.trim().parse::<u8>()?,
          g.trim().parse::<u8>()?,
          b.trim().parse::<u8>()?,
        )
      } else {
        println!("Unexpected color {}", theme_item);
        Color::Black
      }
    }
  };

  Ok(color)
}

pub fn color_to_string(color: Color) -> String {
  match color {
    Color::Reset => "Reset".to_string(),
    Color::Black => "Black".to_string(),
    Color::Red => "Red".to_string(),
    Color::Green => "Green".to_string(),
    Color::Yellow => "Yellow".to_string(),
    Color::Blue => "Blue".to_string(),
    Color::Magenta => "Magenta".to_string(),
    Color::Cyan => "Cyan".to_string(),
    Color::Gray => "Gray".to_string(),
    Color::DarkGray => "DarkGray".to_string(),
    Color::LightRed => "LightRed".to_string(),
    Color::LightGreen => "LightGreen".to_string(),
    Color::LightYellow => "LightYellow".to_string(),
    Color::LightBlue => "LightBlue".to_string(),
    Color::LightMagenta => "LightMagenta".to_string(),
    Color::LightCyan => "LightCyan".to_string(),
    Color::White => "White".to_string(),
    Color::Rgb(r, g, b) => format!("{}, {}, {}", r, g, b),
    _ => "Reset".to_string(),
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_parse_key() {
    use super::parse_key;
    use crate::tui::event::Key;
    assert_eq!(parse_key(String::from("j")).unwrap(), Key::Char('j'));
    assert_eq!(parse_key(String::from("J")).unwrap(), Key::Char('J'));
    assert_eq!(parse_key(String::from("ctrl-j")).unwrap(), Key::Ctrl('j'));
    assert_eq!(parse_key(String::from("ctrl-J")).unwrap(), Key::Ctrl('J'));
    assert_eq!(parse_key(String::from("-")).unwrap(), Key::Char('-'));
    assert_eq!(parse_key(String::from("esc")).unwrap(), Key::Esc);
    assert_eq!(parse_key(String::from("del")).unwrap(), Key::Delete);
    // Test new keys
    assert_eq!(parse_key(String::from("enter")).unwrap(), Key::Enter);
    assert_eq!(parse_key(String::from("tab")).unwrap(), Key::Tab);
    assert_eq!(parse_key(String::from("home")).unwrap(), Key::Home);
    assert_eq!(parse_key(String::from("end")).unwrap(), Key::End);
    assert_eq!(parse_key(String::from("ins")).unwrap(), Key::Ins);
    assert_eq!(parse_key(String::from("insert")).unwrap(), Key::Ins);
    assert_eq!(parse_key(String::from("f0")).unwrap(), Key::F0);
    assert_eq!(parse_key(String::from("f1")).unwrap(), Key::F1);
    assert_eq!(parse_key(String::from("f2")).unwrap(), Key::F2);
    assert_eq!(parse_key(String::from("f3")).unwrap(), Key::F3);
    assert_eq!(parse_key(String::from("f4")).unwrap(), Key::F4);
    assert_eq!(parse_key(String::from("f5")).unwrap(), Key::F5);
    assert_eq!(parse_key(String::from("f6")).unwrap(), Key::F6);
    assert_eq!(parse_key(String::from("f7")).unwrap(), Key::F7);
    assert_eq!(parse_key(String::from("f8")).unwrap(), Key::F8);
    assert_eq!(parse_key(String::from("f9")).unwrap(), Key::F9);
    assert_eq!(parse_key(String::from("f10")).unwrap(), Key::F10);
    assert_eq!(parse_key(String::from("f11")).unwrap(), Key::F11);
    assert_eq!(parse_key(String::from("f12")).unwrap(), Key::F12);
  }

  #[test]
  fn parse_theme_item_test() {
    use super::parse_theme_item;
    use ratatui::style::Color;
    assert_eq!(parse_theme_item("Reset").unwrap(), Color::Reset);
    assert_eq!(parse_theme_item("Black").unwrap(), Color::Black);
    assert_eq!(parse_theme_item("Red").unwrap(), Color::Red);
    assert_eq!(parse_theme_item("Green").unwrap(), Color::Green);
    assert_eq!(parse_theme_item("Yellow").unwrap(), Color::Yellow);
    assert_eq!(parse_theme_item("Blue").unwrap(), Color::Blue);
    assert_eq!(parse_theme_item("Magenta").unwrap(), Color::Magenta);
    assert_eq!(parse_theme_item("Cyan").unwrap(), Color::Cyan);
    assert_eq!(parse_theme_item("Gray").unwrap(), Color::Gray);
    assert_eq!(parse_theme_item("DarkGray").unwrap(), Color::DarkGray);
    assert_eq!(parse_theme_item("LightRed").unwrap(), Color::LightRed);
    assert_eq!(parse_theme_item("LightGreen").unwrap(), Color::LightGreen);
    assert_eq!(parse_theme_item("LightYellow").unwrap(), Color::LightYellow);
    assert_eq!(parse_theme_item("LightBlue").unwrap(), Color::LightBlue);
    assert_eq!(
      parse_theme_item("LightMagenta").unwrap(),
      Color::LightMagenta
    );
    assert_eq!(parse_theme_item("LightCyan").unwrap(), Color::LightCyan);
    assert_eq!(parse_theme_item("White").unwrap(), Color::White);
    assert_eq!(
      parse_theme_item("23, 43, 45").unwrap(),
      Color::Rgb(23, 43, 45)
    );
  }

  #[test]
  fn test_reserved_key() {
    use super::check_reserved_keys;
    use crate::tui::event::Key;

    assert!(
      check_reserved_keys(Key::Enter).is_err(),
      "Enter key should be reserved"
    );
  }

  #[test]
  fn test_startup_behavior_deserialization() {
    use super::{BehaviorConfigString, StartupBehavior};

    let config: BehaviorConfigString = serde_yaml::from_str("startup_behavior: pause").unwrap();
    assert_eq!(config.startup_behavior, Some(StartupBehavior::Pause));

    let config: BehaviorConfigString = serde_yaml::from_str("startup_behavior: play").unwrap();
    assert_eq!(config.startup_behavior, Some(StartupBehavior::Play));

    let config: BehaviorConfigString = serde_yaml::from_str("startup_behavior: continue").unwrap();
    assert_eq!(config.startup_behavior, Some(StartupBehavior::Continue));

    // Missing field defaults to None (not overriding the config default)
    let config: BehaviorConfigString = serde_yaml::from_str("{}").unwrap();
    assert_eq!(config.startup_behavior, None);
  }

  #[test]
  fn tick_rates_load_defaults_explicit_values_and_legacy_defaults() {
    use super::{
      BehaviorConfigString, UserConfig, DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
      DEFAULT_TICK_RATE_MILLISECONDS,
    };

    for (yaml, expected_tick_rate, expected_animation_tick_rate) in [
      (
        "",
        DEFAULT_TICK_RATE_MILLISECONDS,
        DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
      ),
      (
        "tick_rate_milliseconds: 500\nanimation_tick_rate_milliseconds: 20",
        500,
        20,
      ),
      (
        "tick_rate_milliseconds: 100",
        100,
        DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
      ),
      (
        "tick_rate_milliseconds: 16",
        DEFAULT_TICK_RATE_MILLISECONDS,
        DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
      ),
      (
        "tick_rate_milliseconds: 16\nanimation_tick_rate_milliseconds: 16",
        DEFAULT_TICK_RATE_MILLISECONDS,
        DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
      ),
    ] {
      let behavior: BehaviorConfigString = serde_yaml::from_str(yaml).unwrap();
      let mut config = UserConfig::new();
      config.load_behaviorconfig(behavior).unwrap();

      assert_eq!(config.behavior.tick_rate_milliseconds, expected_tick_rate);
      assert_eq!(
        config.behavior.animation_tick_rate_milliseconds,
        expected_animation_tick_rate
      );
    }
  }

  #[test]
  fn zero_tick_rates_are_rejected() {
    use super::{BehaviorConfigString, UserConfig};

    for yaml in [
      "tick_rate_milliseconds: 0",
      "animation_tick_rate_milliseconds: 0",
    ] {
      let behavior: BehaviorConfigString = serde_yaml::from_str(yaml).unwrap();
      let mut config = UserConfig::new();

      assert!(config.load_behaviorconfig(behavior).is_err());
    }
  }

  #[test]
  fn parse_update_delay_secs_accepts_supported_units() {
    use super::parse_update_delay_secs;

    assert_eq!(parse_update_delay_secs("0"), Ok(0));
    assert_eq!(parse_update_delay_secs(""), Ok(0));
    assert_eq!(parse_update_delay_secs("7d"), Ok(7 * 86400));
    assert_eq!(parse_update_delay_secs("2h"), Ok(2 * 3600));
    assert_eq!(parse_update_delay_secs("10m"), Ok(10 * 60));
    assert_eq!(parse_update_delay_secs("30s"), Ok(30));
    assert_eq!(parse_update_delay_secs("120"), Ok(120));
    assert!(parse_update_delay_secs("bogus").is_err());
  }

  #[test]
  fn invalid_auto_update_delay_is_rejected() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str("auto_update_delay: bogus").unwrap();
    let mut config = UserConfig::new();

    assert!(config.load_behaviorconfig(behavior).is_err());
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn missing_playbar_cover_art_size_keeps_default() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str("{}").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();

    assert_eq!(config.behavior.playbar_cover_art_size_percent, 100);
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_art_size_loads_from_yaml() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString =
      serde_yaml::from_str("playbar_cover_art_size_percent: 150").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();

    assert_eq!(config.behavior.playbar_cover_art_size_percent, 150);
  }

  #[cfg(feature = "cover-art")]
  #[test]
  fn playbar_cover_art_size_clamps_out_of_range_values() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString =
      serde_yaml::from_str("playbar_cover_art_size_percent: 10").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.playbar_cover_art_size_percent, 25);

    let behavior: BehaviorConfigString =
      serde_yaml::from_str("playbar_cover_art_size_percent: 250").unwrap();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.playbar_cover_art_size_percent, 200);
  }

  #[test]
  fn plugin_commands_valid_entry_lands_in_plugin_command_keys() {
    use super::UserConfig;
    use crate::tui::event::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    let mut entries = HashMap::new();
    entries.insert("toggle_lyrics".to_string(), "ctrl-g".to_string());
    config.load_plugin_commands(entries);
    assert_eq!(
      config.plugin_command_keys.get(&Key::Ctrl('g')),
      Some(&"toggle_lyrics".to_string())
    );
  }

  #[test]
  fn plugin_commands_reserved_key_is_skipped() {
    use super::UserConfig;
    use crate::tui::event::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    let mut entries = HashMap::new();
    // Enter is a reserved key
    entries.insert("submit_action".to_string(), "enter".to_string());
    config.load_plugin_commands(entries);
    assert!(!config.plugin_command_keys.contains_key(&Key::Enter));
  }

  #[test]
  fn plugin_commands_named_action_collision_is_skipped() {
    use super::UserConfig;
    use crate::tui::event::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    // 'q' is the default 'back' key
    let mut entries = HashMap::new();
    entries.insert("my_cmd".to_string(), "q".to_string());
    config.load_plugin_commands(entries);
    assert!(!config.plugin_command_keys.contains_key(&Key::Char('q')));
  }

  #[test]
  fn plugin_commands_invalid_key_string_is_skipped() {
    use super::UserConfig;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    let mut entries = HashMap::new();
    entries.insert("my_cmd".to_string(), "not-a-real-key".to_string());
    config.load_plugin_commands(entries);
    assert!(config.plugin_command_keys.is_empty());
  }

  #[test]
  fn active_source_local_round_trips_through_config() {
    use super::{BehaviorConfigString, UserConfig};
    use crate::core::source::Source;

    // "Local" in YAML deserialized and resolved → Source::Local
    let behavior: BehaviorConfigString = serde_yaml::from_str("active_source: Local").unwrap();
    assert_eq!(behavior.active_source, Some("Local".to_string()));

    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.active_source, Source::Local);
  }

  #[test]
  fn radio_stations_round_trip_through_config() {
    use super::{BehaviorConfigString, RadioStationConfig, UserConfig};

    let yaml = r#"
radio_stations:
  - name: SomaFM Groove Salad
    url: https://ice1.somafm.com/groovesalad-128-mp3
  - name: ""
    url: https://blank-name.example/dropped
"#;
    let behavior: BehaviorConfigString = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(behavior.radio_stations.as_ref().map(Vec::len), Some(2));

    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    // The blank-name entry is dropped at load; the valid one survives intact.
    assert_eq!(
      config.behavior.radio_stations,
      vec![RadioStationConfig {
        name: "SomaFM Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      }]
    );
  }

  #[test]
  fn radio_stations_missing_field_defaults_to_empty() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str("{}").unwrap();
    assert_eq!(behavior.radio_stations, None);

    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert!(config.behavior.radio_stations.is_empty());
  }

  #[test]
  fn adding_radio_station_persists_trimmed_unique_entry() {
    use super::{
      RadioStationAddOutcome, RadioStationConfig, UserConfig, UserConfigPaths, UserConfigString,
    };

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: config_path.clone(),
    });

    let outcome = config
      .add_radio_station(
        " SomaFM Groove Salad ",
        " https://ice1.somafm.com/groovesalad-128-mp3 ",
      )
      .unwrap();
    assert_eq!(outcome, RadioStationAddOutcome::Added);
    assert_eq!(
      config.behavior.radio_stations,
      vec![RadioStationConfig {
        name: "SomaFM Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      }]
    );

    let saved = std::fs::read_to_string(config_path).unwrap();
    let saved: UserConfigString = serde_yaml::from_str(&saved).unwrap();
    assert_eq!(
      saved
        .behavior
        .unwrap()
        .radio_stations
        .unwrap()
        .first()
        .cloned(),
      Some(RadioStationConfig {
        name: "SomaFM Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      })
    );
  }

  #[test]
  fn adding_radio_station_dedupes_by_stream_url() {
    use super::{RadioStationAddOutcome, RadioStationConfig, UserConfig, UserConfigPaths};

    let dir = tempfile::tempdir().unwrap();
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: dir.path().join("config.yml"),
    });
    config.behavior.radio_stations = vec![RadioStationConfig {
      name: "Existing".to_string(),
      url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
    }];

    let outcome = config
      .add_radio_station(
        "Duplicate Name",
        " https://ice1.somafm.com/groovesalad-128-mp3 ",
      )
      .unwrap();

    assert_eq!(outcome, RadioStationAddOutcome::AlreadyExists);
    assert_eq!(config.behavior.radio_stations.len(), 1);
    assert_eq!(config.behavior.radio_stations[0].name, "Existing");
  }

  #[test]
  fn removing_radio_station_persists_by_stream_url() {
    use super::{RadioStationConfig, UserConfig, UserConfigPaths, UserConfigString};

    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: config_path.clone(),
    });
    config.behavior.radio_stations = vec![
      RadioStationConfig {
        name: "Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      },
      RadioStationConfig {
        name: "Secret Agent".to_string(),
        url: "https://ice1.somafm.com/secretagent-128-mp3".to_string(),
      },
    ];

    let removed = config
      .remove_radio_station_by_url(" https://ice1.somafm.com/groovesalad-128-mp3 ")
      .unwrap();

    assert_eq!(
      removed.map(|station| station.name),
      Some("Groove Salad".to_string())
    );
    assert_eq!(config.behavior.radio_stations.len(), 1);
    assert_eq!(config.behavior.radio_stations[0].name, "Secret Agent");

    let saved = std::fs::read_to_string(config_path).unwrap();
    let saved: UserConfigString = serde_yaml::from_str(&saved).unwrap();
    assert_eq!(
      saved
        .behavior
        .unwrap()
        .radio_stations
        .unwrap()
        .iter()
        .map(|station| station.name.as_str())
        .collect::<Vec<_>>(),
      vec!["Secret Agent"]
    );
  }

  #[test]
  fn active_source_missing_field_defaults_to_spotify() {
    use super::{BehaviorConfigString, UserConfig};
    use crate::core::source::Source;

    // No active_source key in config → field is None → default Spotify preserved
    let behavior: BehaviorConfigString = serde_yaml::from_str("{}").unwrap();
    assert_eq!(behavior.active_source, None);

    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.active_source, Source::Spotify);
  }

  #[test]
  fn active_source_unknown_string_falls_back_to_spotify() {
    use crate::core::source::Source;

    // Unknown/garbage strings must not panic and fall back to Spotify
    assert_eq!(Source::from_config_str("Tidal"), Source::Spotify);
    assert_eq!(Source::from_config_str(""), Source::Spotify);
    assert_eq!(Source::from_config_str("local"), Source::Spotify); // case-sensitive
  }

  #[test]
  fn active_source_to_config_str_matches_from_config_str() {
    use crate::core::source::Source;

    // Round-trip: to_config_str → from_config_str must be identity for both variants
    assert_eq!(
      Source::from_config_str(Source::Spotify.to_config_str()),
      Source::Spotify
    );
    assert_eq!(
      Source::from_config_str(Source::Local.to_config_str()),
      Source::Local
    );
  }
}
