use crate::event::Key;
use anyhow::{anyhow, Result};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::{
  fs,
  path::{Path, PathBuf},
};

const FILE_NAME: &str = "config.yml";
const CONFIG_DIR: &str = ".config";
const APP_CONFIG_DIR: &str = "spotatui";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UserTheme {
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
      active: Color::Rgb(0, 180, 180),            // Cyan equivalent
      banner: Color::Rgb(0, 200, 200),            // LightCyan equivalent
      error_border: Color::Rgb(200, 0, 0),        // Red equivalent
      error_text: Color::Rgb(255, 100, 100),      // LightRed equivalent
      hint: Color::Rgb(200, 200, 0),              // Yellow equivalent
      hovered: Color::Rgb(180, 0, 180),           // Magenta equivalent
      inactive: Color::Rgb(128, 128, 128),        // Gray equivalent
      playbar_background: Color::Rgb(20, 20, 20), // Near-black
      playbar_progress: Color::Rgb(0, 200, 200),  // LightCyan equivalent
      playbar_progress_text: Color::Rgb(255, 255, 255), // Bright white for visibility
      playbar_text: Color::Reset,
      selected: Color::Rgb(0, 200, 200), // LightCyan equivalent
      text: Color::Reset,
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
  Spotify,
  Dracula,
  Nord,
  SolarizedDark,
  Monokai,
  Gruvbox,
  CatppuccinMocha,
  Custom, // When user has manually customized colors
}

impl ThemePreset {
  pub fn all() -> &'static [ThemePreset] {
    &[
      ThemePreset::Default,
      ThemePreset::Spotify,
      ThemePreset::Dracula,
      ThemePreset::Nord,
      ThemePreset::SolarizedDark,
      ThemePreset::Monokai,
      ThemePreset::Gruvbox,
      ThemePreset::CatppuccinMocha,
    ]
  }

  pub fn name(&self) -> &'static str {
    match self {
      ThemePreset::Default => "Default (Cyan)",
      ThemePreset::Spotify => "Spotify",
      ThemePreset::Dracula => "Dracula",
      ThemePreset::Nord => "Nord",
      ThemePreset::SolarizedDark => "Solarized Dark",
      ThemePreset::Monokai => "Monokai",
      ThemePreset::Gruvbox => "Gruvbox",
      ThemePreset::CatppuccinMocha => "Catppuccin Mocha",
      ThemePreset::Custom => "Custom",
    }
  }

  pub fn from_name(name: &str) -> Self {
    match name {
      "Default (Cyan)" => ThemePreset::Default,
      "Spotify" => ThemePreset::Spotify,
      "Dracula" => ThemePreset::Dracula,
      "Nord" => ThemePreset::Nord,
      "Solarized Dark" => ThemePreset::SolarizedDark,
      "Monokai" => ThemePreset::Monokai,
      "Gruvbox" => ThemePreset::Gruvbox,
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
      ThemePreset::Dracula => Theme {
        analysis_bar: Color::Rgb(189, 147, 249),      // Purple
        analysis_bar_text: Color::Rgb(248, 248, 242), // Foreground
        active: Color::Rgb(80, 250, 123),             // Green
        banner: Color::Rgb(255, 121, 198),            // Pink
        error_border: Color::Rgb(255, 85, 85),        // Red
        error_text: Color::Rgb(255, 85, 85),
        hint: Color::Rgb(241, 250, 140),            // Yellow
        hovered: Color::Rgb(189, 147, 249),         // Purple
        inactive: Color::Rgb(98, 114, 164),         // Comment
        playbar_background: Color::Rgb(40, 42, 54), // Background
        playbar_progress: Color::Rgb(80, 250, 123), // Green
        playbar_progress_text: Color::Rgb(248, 248, 242),
        playbar_text: Color::Rgb(248, 248, 242),
        selected: Color::Rgb(139, 233, 253), // Cyan
        text: Color::Rgb(248, 248, 242),
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
        hint: Color::Rgb(235, 203, 139),            // Nord13 (yellow)
        hovered: Color::Rgb(180, 142, 173),         // Nord15 (purple)
        inactive: Color::Rgb(76, 86, 106),          // Nord3
        playbar_background: Color::Rgb(46, 52, 64), // Nord0
        playbar_progress: Color::Rgb(136, 192, 208), // Nord8
        playbar_progress_text: Color::Rgb(236, 239, 244),
        playbar_text: Color::Rgb(236, 239, 244),
        selected: Color::Rgb(129, 161, 193), // Nord9
        text: Color::Rgb(236, 239, 244),     // Nord6
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
        hint: Color::Rgb(181, 137, 0),              // Yellow
        hovered: Color::Rgb(211, 54, 130),          // Magenta
        inactive: Color::Rgb(88, 110, 117),         // Base01
        playbar_background: Color::Rgb(0, 43, 54),  // Base03
        playbar_progress: Color::Rgb(42, 161, 152), // Cyan
        playbar_progress_text: Color::Rgb(253, 246, 227),
        playbar_text: Color::Rgb(147, 161, 161), // Base1
        selected: Color::Rgb(42, 161, 152),      // Cyan
        text: Color::Rgb(147, 161, 161),         // Base1
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
        hint: Color::Rgb(230, 219, 116),            // Yellow
        hovered: Color::Rgb(174, 129, 255),         // Purple
        inactive: Color::Rgb(117, 113, 94),         // Comment
        playbar_background: Color::Rgb(39, 40, 34), // Background
        playbar_progress: Color::Rgb(166, 226, 46), // Green
        playbar_progress_text: Color::Rgb(248, 248, 242),
        playbar_text: Color::Rgb(248, 248, 242),
        selected: Color::Rgb(102, 217, 239), // Cyan
        text: Color::Rgb(248, 248, 242),
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
        hint: Color::Rgb(250, 189, 47),             // Yellow
        hovered: Color::Rgb(211, 134, 155),         // Purple
        inactive: Color::Rgb(146, 131, 116),        // Gray
        playbar_background: Color::Rgb(40, 40, 40), // bg
        playbar_progress: Color::Rgb(184, 187, 38), // Green
        playbar_progress_text: Color::Rgb(235, 219, 178),
        playbar_text: Color::Rgb(235, 219, 178),
        selected: Color::Rgb(131, 165, 152),          // Aqua
        text: Color::Rgb(235, 219, 178),              // fg
        header: Color::Rgb(254, 128, 25),             // Orange
        highlighted_lyrics: Color::Rgb(254, 128, 25), // Orange
      },
      ThemePreset::CatppuccinMocha => Theme {
        analysis_bar: Color::Rgb(166, 227, 161),        // Green
        analysis_bar_text: Color::Rgb(205, 214, 244),   // Text
        active: Color::Rgb(180, 190, 254),              // Lavender
        banner: Color::Rgb(180, 190, 254),              // Lavender
        error_border: Color::Rgb(243, 139, 168),        // Red
        error_text: Color::Rgb(243, 139, 168),          // Red
        hint: Color::Rgb(250, 179, 135),                // Peach
        hovered: Color::Rgb(137, 180, 250),             // Blue
        inactive: Color::Rgb(108, 112, 134),            // Overlay 0
        playbar_background: Color::Rgb(49, 50, 68),     // Surface 0
        playbar_progress: Color::Rgb(180, 190, 254),    // Lavender
        playbar_progress_text: Color::Rgb(88, 91, 112), // Surface 2
        playbar_text: Color::Rgb(186, 194, 222),        // Subtext 1
        selected: Color::Rgb(180, 190, 254),            // Lavender
        text: Color::Rgb(205, 214, 244),                // Text
        header: Color::Rgb(180, 190, 254),              // Lavender
        highlighted_lyrics: Color::Rgb(180, 190, 254),  // Lavender
      },
      ThemePreset::Spotify => Theme {
        analysis_bar: Color::Rgb(29, 185, 84), // Spotify Green #1DB954
        analysis_bar_text: Color::Rgb(255, 255, 255), // White
        active: Color::Rgb(29, 185, 84),       // Spotify Green
        banner: Color::Rgb(29, 185, 84),       // Spotify Green
        error_border: Color::Rgb(230, 76, 76), // Soft red
        error_text: Color::Rgb(230, 76, 76),
        hint: Color::Rgb(179, 179, 179),            // Gray hint
        hovered: Color::Rgb(29, 185, 84),           // Spotify Green
        inactive: Color::Rgb(83, 83, 83),           // Dark gray
        playbar_background: Color::Rgb(24, 24, 24), // Near black
        playbar_progress: Color::Rgb(29, 185, 84),  // Spotify Green
        playbar_progress_text: Color::Rgb(255, 255, 255),
        playbar_text: Color::Rgb(179, 179, 179), // Light gray
        selected: Color::Rgb(29, 185, 84),       // Spotify Green
        text: Color::Rgb(255, 255, 255),         // White
        header: Color::Rgb(29, 185, 84),         // Spotify Green
        highlighted_lyrics: Color::Rgb(29, 185, 84), // Spotify Green
      },
      ThemePreset::Custom => Theme::default(), // Won't be used directly
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
        _ => Err(anyhow!("The key \"{}\" is unknown.", sections[0])),
      }
    }
  }
}

fn check_reserved_keys(key: Key) -> Result<()> {
  let reserved = [
    Key::Char('h'),
    Key::Char('j'),
    Key::Char('k'),
    Key::Char('l'),
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

#[derive(Clone)]
pub struct UserConfigPaths {
  pub config_file_path: PathBuf,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyBindingsString {
  back: Option<String>,
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
  help: Option<String>,
  shuffle: Option<String>,
  repeat: Option<String>,
  search: Option<String>,
  submit: Option<String>,
  copy_song_url: Option<String>,
  copy_album_url: Option<String>,
  audio_analysis: Option<String>,
  basic_view: Option<String>,
  add_item_to_queue: Option<String>,
}

#[derive(Clone)]
pub struct KeyBindings {
  pub back: Key,
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
  pub help: Key,
  pub shuffle: Key,
  pub repeat: Key,
  pub search: Key,
  pub submit: Key,
  pub copy_song_url: Key,
  pub copy_album_url: Key,
  pub audio_analysis: Key,
  pub basic_view: Key,
  pub add_item_to_queue: Key,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BehaviorConfigString {
  pub seek_milliseconds: Option<u32>,
  pub volume_increment: Option<u8>,
  pub volume_percent: Option<u8>,
  pub tick_rate_milliseconds: Option<u64>,
  pub enable_text_emphasis: Option<bool>,
  pub show_loading_indicator: Option<bool>,
  pub enforce_wide_search_bar: Option<bool>,
  pub enable_global_song_count: Option<bool>,
  pub shuffle_enabled: Option<bool>,
  pub liked_icon: Option<String>,
  pub shuffle_icon: Option<String>,
  pub repeat_track_icon: Option<String>,
  pub repeat_context_icon: Option<String>,
  pub playing_icon: Option<String>,
  pub paused_icon: Option<String>,
  pub set_window_title: Option<bool>,
}

#[derive(Clone)]
pub struct BehaviorConfig {
  pub seek_milliseconds: u32,
  pub volume_increment: u8,
  pub volume_percent: u8,
  pub tick_rate_milliseconds: u64,
  pub enable_text_emphasis: bool,
  pub show_loading_indicator: bool,
  pub enforce_wide_search_bar: bool,
  pub enable_global_song_count: bool,
  pub shuffle_enabled: bool,
  pub liked_icon: String,
  pub shuffle_icon: String,
  pub repeat_track_icon: String,
  pub repeat_context_icon: String,
  pub playing_icon: String,
  pub paused_icon: String,
  pub set_window_title: bool,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserConfigString {
  keybindings: Option<KeyBindingsString>,
  behavior: Option<BehaviorConfigString>,
  theme: Option<UserTheme>,
}

#[derive(Clone)]
pub struct UserConfig {
  pub keys: KeyBindings,
  pub theme: Theme,
  pub behavior: BehaviorConfig,
  pub path_to_config: Option<UserConfigPaths>,
}

impl UserConfig {
  pub fn new() -> UserConfig {
    UserConfig {
      theme: Default::default(),
      keys: KeyBindings {
        back: Key::Char('q'),
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
        help: Key::Char('?'),
        shuffle: Key::Ctrl('s'),
        repeat: Key::Ctrl('r'),
        search: Key::Char('/'),
        submit: Key::Enter,
        copy_song_url: Key::Char('c'),
        copy_album_url: Key::Char('C'),
        audio_analysis: Key::Char('v'),
        basic_view: Key::Char('B'),
        add_item_to_queue: Key::Char('z'),
      },
      behavior: BehaviorConfig {
        seek_milliseconds: 5 * 1000,
        volume_increment: 10,
        volume_percent: 100,
        tick_rate_milliseconds: 16,
        enable_text_emphasis: true,
        show_loading_indicator: true,
        enforce_wide_search_bar: false,
        enable_global_song_count: true,
        shuffle_enabled: false,
        liked_icon: "♥".to_string(),
        shuffle_icon: "🔀".to_string(),
        repeat_track_icon: "🔂".to_string(),
        repeat_context_icon: "🔁".to_string(),
        playing_icon: "▶".to_string(),
        paused_icon: "⏸".to_string(),
        set_window_title: true,
      },
      path_to_config: None,
    }
  }

  pub fn get_or_build_paths(&mut self) -> Result<()> {
    match dirs::home_dir() {
      Some(home) => {
        let path = Path::new(&home);
        let home_config_dir = path.join(CONFIG_DIR);
        let app_config_dir = home_config_dir.join(APP_CONFIG_DIR);

        if !home_config_dir.exists() {
          fs::create_dir(&home_config_dir)?;
        }

        if !app_config_dir.exists() {
          fs::create_dir(&app_config_dir)?;
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
          check_reserved_keys(self.keys.$name)?;
        }
      };
    }

    to_keys!(back);
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
    to_keys!(help);
    to_keys!(shuffle);
    to_keys!(repeat);
    to_keys!(search);
    to_keys!(submit);
    to_keys!(copy_song_url);
    to_keys!(copy_album_url);
    to_keys!(audio_analysis);
    to_keys!(basic_view);
    to_keys!(add_item_to_queue);

    Ok(())
  }

  pub fn load_theme(&mut self, theme: UserTheme) -> Result<()> {
    macro_rules! to_theme_item {
      ($name: ident) => {
        if let Some(theme_item) = theme.$name {
          self.theme.$name = parse_theme_item(&theme_item)?;
        }
      };
    }

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
    to_theme_item!(header);
    to_theme_item!(highlighted_lyrics);
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

    if let Some(tick_rate) = behavior_config.tick_rate_milliseconds {
      if tick_rate >= 1000 {
        return Err(anyhow!("Tick rate must be below 1000"));
      } else {
        self.behavior.tick_rate_milliseconds = tick_rate;
      }
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

    if let Some(shuffle_enabled) = behavior_config.shuffle_enabled {
      self.behavior.shuffle_enabled = shuffle_enabled;
    }

    Ok(())
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
      enable_text_emphasis: Some(self.behavior.enable_text_emphasis),
      show_loading_indicator: Some(self.behavior.show_loading_indicator),
      enforce_wide_search_bar: Some(self.behavior.enforce_wide_search_bar),
      enable_global_song_count: Some(self.behavior.enable_global_song_count),
      shuffle_enabled: Some(self.behavior.shuffle_enabled),
      liked_icon: Some(self.behavior.liked_icon.clone()),
      shuffle_icon: Some(self.behavior.shuffle_icon.clone()),
      repeat_track_icon: Some(self.behavior.repeat_track_icon.clone()),
      repeat_context_icon: Some(self.behavior.repeat_context_icon.clone()),
      playing_icon: Some(self.behavior.playing_icon.clone()),
      paused_icon: Some(self.behavior.paused_icon.clone()),
      set_window_title: Some(self.behavior.set_window_title),
    };

    // Helper to build theme config from current values
    let build_theme = || UserTheme {
      active: Some(color_to_string(self.theme.active)),
      banner: Some(color_to_string(self.theme.banner)),
      error_border: Some(color_to_string(self.theme.error_border)),
      error_text: Some(color_to_string(self.theme.error_text)),
      hint: Some(color_to_string(self.theme.hint)),
      hovered: Some(color_to_string(self.theme.hovered)),
      inactive: Some(color_to_string(self.theme.inactive)),
      playbar_background: Some(color_to_string(self.theme.playbar_background)),
      playbar_progress: Some(color_to_string(self.theme.playbar_progress)),
      playbar_progress_text: Some(color_to_string(self.theme.playbar_progress_text)),
      playbar_text: Some(color_to_string(self.theme.playbar_text)),
      selected: Some(color_to_string(self.theme.selected)),
      text: Some(color_to_string(self.theme.text)),
      header: Some(color_to_string(self.theme.header)),
      highlighted_lyrics: Some(color_to_string(self.theme.highlighted_lyrics)),
    };

    // If the file exists, try to read it first to preserve keybindings
    let final_config = if paths.config_file_path.exists() {
      let config_string = fs::read_to_string(&paths.config_file_path)?;
      if !config_string.trim().is_empty() {
        let mut existing: UserConfigString = serde_yaml::from_str(&config_string)?;
        // Update behavior and theme
        existing.behavior = Some(build_behavior());
        existing.theme = Some(build_theme());
        existing
      } else {
        UserConfigString {
          keybindings: None,
          behavior: Some(build_behavior()),
          theme: Some(build_theme()),
        }
      }
    } else {
      UserConfigString {
        keybindings: None,
        behavior: Some(build_behavior()),
        theme: Some(build_theme()),
      }
    };

    let content_yml = serde_yaml::to_string(&final_config)?;
    let mut config_file = fs::File::create(&paths.config_file_path)?;
    std::io::Write::write_all(&mut config_file, content_yml.as_bytes())?;

    Ok(())
  }

  pub fn padded_liked_icon(&self) -> String {
    format!("{} ", &self.behavior.liked_icon)
  }
}

fn parse_theme_item(theme_item: &str) -> Result<Color> {
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

fn color_to_string(color: Color) -> String {
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
    use crate::event::Key;
    assert_eq!(parse_key(String::from("j")).unwrap(), Key::Char('j'));
    assert_eq!(parse_key(String::from("J")).unwrap(), Key::Char('J'));
    assert_eq!(parse_key(String::from("ctrl-j")).unwrap(), Key::Ctrl('j'));
    assert_eq!(parse_key(String::from("ctrl-J")).unwrap(), Key::Ctrl('J'));
    assert_eq!(parse_key(String::from("-")).unwrap(), Key::Char('-'));
    assert_eq!(parse_key(String::from("esc")).unwrap(), Key::Esc);
    assert_eq!(parse_key(String::from("del")).unwrap(), Key::Delete);
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
    use crate::event::Key;

    assert!(
      check_reserved_keys(Key::Enter).is_err(),
      "Enter key should be reserved"
    );
  }
}
