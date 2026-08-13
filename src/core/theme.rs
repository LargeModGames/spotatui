//! Frontend-neutral color and theme types.
//!
//! `Color` is structurally identical to `ratatui::style::Color` so the theme
//! system carries no terminal-toolkit dependency; the TUI converts at the
//! render boundary (`tui/theme.rs`). The on-disk `config.yml` format is
//! hand-parsed by [`parse_theme_item`] / [`color_to_string`] and must stay
//! byte-identical across this move.

use anyhow::Result;

/// A color in the terminal's color model. Named variants follow the 16 ANSI
/// palette entries; `Reset` is the frontend's default foreground/background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
  Reset,
  Black,
  Red,
  Green,
  Yellow,
  Blue,
  Magenta,
  Cyan,
  Gray,
  DarkGray,
  LightRed,
  LightGreen,
  LightYellow,
  LightBlue,
  LightMagenta,
  LightCyan,
  White,
  Rgb(u8, u8, u8),
  Indexed(u8),
}

#[derive(Copy, Clone, Debug, PartialEq)]
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
  Terminal,
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
  TokyoNight,
  Custom, // When user has manually customized colors
}

impl ThemePreset {
  pub fn all() -> &'static [ThemePreset] {
    &[
      ThemePreset::Default,
      ThemePreset::Terminal,
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
      ThemePreset::TokyoNight,
      ThemePreset::Custom,
    ]
  }

  pub fn name(&self) -> &'static str {
    match self {
      ThemePreset::Default => "Default (Cyan)",
      ThemePreset::Terminal => "Terminal (ANSI)",
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
      ThemePreset::TokyoNight => "Tokyo Night",
      ThemePreset::Custom => "Custom",
    }
  }

  pub fn from_name(name: &str) -> Self {
    match name {
      "Default (Cyan)" => ThemePreset::Default,
      "Terminal (ANSI)" => ThemePreset::Terminal,
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
      "Tokyo Night" => ThemePreset::TokyoNight,
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

  /// Default banner-gradient state for this preset. The Terminal preset
  /// exists to follow the terminal's ANSI palette live, which the gradient's
  /// fixed RGB colors would defeat, so it defaults to a solid banner.
  pub fn default_banner_gradient(&self) -> bool {
    !matches!(self, ThemePreset::Terminal)
  }

  /// Get the theme colors for this preset
  pub fn to_theme(self) -> Theme {
    match self {
      ThemePreset::Default => Theme::default(),
      // Deliberately uses named ANSI colors (unlike the RGB rationale in
      // Theme::default) so terminal-palette tools like pywal restyle the UI
      // live, without restarting spotatui.
      ThemePreset::Terminal => Theme {
        analysis_bar: Color::Cyan,
        analysis_bar_text: Color::Reset,
        active: Color::Cyan,
        banner: Color::Cyan,
        error_border: Color::Red,
        error_text: Color::LightRed,
        hint: Color::Yellow,
        hovered: Color::Magenta,
        inactive: Color::DarkGray,
        playbar_background: Color::Reset,
        playbar_progress: Color::Cyan,
        playbar_progress_text: Color::Reset,
        playbar_text: Color::Reset,
        selected: Color::Cyan,
        text: Color::Reset,
        background: Color::Reset,
        header: Color::Reset,
        highlighted_lyrics: Color::Cyan,
      },
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
      ThemePreset::TokyoNight => Theme {
        analysis_bar: Color::Rgb(122, 162, 247), //Darker blue #7aa2f7
        analysis_bar_text: Color::Rgb(42, 195, 222), // Light blue #2ac3de
        active: Color::Rgb(115, 218, 202),       // Teal #73daca
        banner: Color::Rgb(42, 195, 222),        // Light blue #2ac3de
        error_border: Color::Rgb(247, 118, 142), // Red #f7768e
        error_text: Color::Rgb(247, 118, 142),   // Red #f7768e
        hint: Color::Rgb(255, 158, 100),         // Orange #ff9e64
        hovered: Color::Rgb(157, 124, 216),      // Purple #9d7cd8
        inactive: Color::Rgb(192, 202, 245),     // Light blue #c0caf5
        playbar_background: Color::Rgb(36, 40, 59), // Gray #24283b
        playbar_progress: Color::Rgb(122, 162, 247), //Darker blue #7aa2f7
        playbar_progress_text: Color::Rgb(169, 177, 214), // Light purple #a9b1d6
        playbar_text: Color::Rgb(122, 162, 247), //Darker blue #7aa2f7
        selected: Color::Rgb(125, 207, 255),     // Light blue #7dcfff
        text: Color::Rgb(169, 177, 214),         // Light purple #a9b1d6
        background: Color::Rgb(36, 40, 59),      // Gray #24283b
        header: Color::Rgb(122, 162, 247),       // Blue #7aa2f7,
        highlighted_lyrics: Color::Rgb(42, 195, 222), // Light blue #2ac3de
      },
      ThemePreset::Custom => Theme::default(), // Won't be used directly
    }
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

/// Concrete RGB values for the colors a terminal resolves itself. A frontend
/// that has no terminal palette (a GUI) supplies one of these to [`resolve`].
///
/// `Reset` means "the frontend's default"; a caller that needs distinct
/// foreground and background defaults uses two palettes differing only in
/// `reset`.
// No production caller until a second frontend needs concrete RGB; exercised
// by tests today.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
  /// RGB for [`Color::Reset`].
  pub reset: [u8; 3],
  /// RGB for the 16 named ANSI entries, indexed by ANSI number
  /// (0 = Black … 7 = Gray, 8 = DarkGray … 15 = White).
  pub ansi: [[u8; 3]; 16],
}

impl Default for Palette {
  /// The xterm default palette, with white as the `Reset` fallback.
  fn default() -> Self {
    Palette {
      reset: [255, 255, 255],
      ansi: [
        [0, 0, 0],       // Black
        [205, 0, 0],     // Red
        [0, 205, 0],     // Green
        [205, 205, 0],   // Yellow
        [0, 0, 238],     // Blue
        [205, 0, 205],   // Magenta
        [0, 205, 205],   // Cyan
        [229, 229, 229], // Gray
        [127, 127, 127], // DarkGray
        [255, 0, 0],     // LightRed
        [0, 255, 0],     // LightGreen
        [255, 255, 0],   // LightYellow
        [92, 92, 255],   // LightBlue
        [255, 0, 255],   // LightMagenta
        [0, 255, 255],   // LightCyan
        [255, 255, 255], // White
      ],
    }
  }
}

/// Resolve any [`Color`] to concrete RGB. `Rgb` passes through; named ANSI
/// colors and `Reset` come from `palette`; `Indexed` follows the xterm-256
/// layout (16 palette entries, 6×6×6 color cube, 24-step gray ramp).
// No production caller until a second frontend needs concrete RGB; exercised
// by tests today.
#[allow(dead_code)]
pub fn resolve(color: Color, palette: &Palette) -> [u8; 3] {
  match color {
    Color::Reset => palette.reset,
    Color::Black => palette.ansi[0],
    Color::Red => palette.ansi[1],
    Color::Green => palette.ansi[2],
    Color::Yellow => palette.ansi[3],
    Color::Blue => palette.ansi[4],
    Color::Magenta => palette.ansi[5],
    Color::Cyan => palette.ansi[6],
    Color::Gray => palette.ansi[7],
    Color::DarkGray => palette.ansi[8],
    Color::LightRed => palette.ansi[9],
    Color::LightGreen => palette.ansi[10],
    Color::LightYellow => palette.ansi[11],
    Color::LightBlue => palette.ansi[12],
    Color::LightMagenta => palette.ansi[13],
    Color::LightCyan => palette.ansi[14],
    Color::White => palette.ansi[15],
    Color::Rgb(r, g, b) => [r, g, b],
    Color::Indexed(i) => match i {
      0..=15 => palette.ansi[i as usize],
      16..=231 => {
        const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let i = i - 16;
        [
          STEPS[(i / 36) as usize],
          STEPS[((i / 6) % 6) as usize],
          STEPS[(i % 6) as usize],
        ]
      }
      232..=255 => {
        let v = 8 + 10 * (i - 232);
        [v, v, v]
      }
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_theme_item_test() {
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
  fn terminal_preset_colors_round_trip_through_config() {
    let theme = ThemePreset::Terminal.to_theme();
    for color in [
      theme.analysis_bar,
      theme.analysis_bar_text,
      theme.active,
      theme.banner,
      theme.error_border,
      theme.error_text,
      theme.hint,
      theme.hovered,
      theme.inactive,
      theme.playbar_background,
      theme.playbar_progress,
      theme.playbar_progress_text,
      theme.playbar_text,
      theme.selected,
      theme.text,
      theme.background,
      theme.header,
      theme.highlighted_lyrics,
    ] {
      assert_eq!(parse_theme_item(&color_to_string(color)).unwrap(), color);
    }
  }

  #[test]
  fn resolve_maps_every_color_form_to_rgb() {
    let palette = Palette::default();

    assert_eq!(resolve(Color::Reset, &palette), palette.reset);
    assert_eq!(resolve(Color::Rgb(1, 2, 3), &palette), [1, 2, 3]);
    // The 17 named variants all land on a palette entry (Reset checked above).
    for (color, ansi_index) in [
      (Color::Black, 0),
      (Color::Red, 1),
      (Color::Green, 2),
      (Color::Yellow, 3),
      (Color::Blue, 4),
      (Color::Magenta, 5),
      (Color::Cyan, 6),
      (Color::Gray, 7),
      (Color::DarkGray, 8),
      (Color::LightRed, 9),
      (Color::LightGreen, 10),
      (Color::LightYellow, 11),
      (Color::LightBlue, 12),
      (Color::LightMagenta, 13),
      (Color::LightCyan, 14),
      (Color::White, 15),
    ] {
      assert_eq!(resolve(color, &palette), palette.ansi[ansi_index]);
      // Indexed 0-15 aliases the same entries.
      assert_eq!(
        resolve(Color::Indexed(ansi_index as u8), &palette),
        palette.ansi[ansi_index]
      );
    }
    // Cube corners and the gray ramp follow the xterm-256 formula.
    assert_eq!(resolve(Color::Indexed(16), &palette), [0, 0, 0]);
    assert_eq!(resolve(Color::Indexed(231), &palette), [255, 255, 255]);
    assert_eq!(resolve(Color::Indexed(196), &palette), [255, 0, 0]);
    assert_eq!(resolve(Color::Indexed(232), &palette), [8, 8, 8]);
    assert_eq!(resolve(Color::Indexed(255), &palette), [238, 238, 238]);
  }
}
