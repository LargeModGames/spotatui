use crate::core::format::Template;
use crate::core::input::Key;
use crate::core::state::{sanitized_radio_stations, RadioStationConfig};
// Re-exported so existing `crate::core::user_config::{Theme, ...}` importers
// keep working now that the theme types live in `core/theme.rs`.
pub use crate::core::theme::{color_to_string, parse_theme_item, Theme, ThemePreset};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{fs, path::PathBuf};

const FILE_NAME: &str = "config.yml";
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
  crate::core::paths::app_config_dir()
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

// `Theme`, `ThemePreset`, `parse_theme_item` and `color_to_string` moved to
// `core/theme.rs` (re-exported above).

/// Available audio visualizer styles
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum VisualizerStyle {
  /// Cava mode: cava's engine and mirrored stereo layout drawn in eighth blocks
  ///
  /// Note: Older configs may contain `Equalizer` (a removed style) or `Classic`
  /// (its even older name); both are accepted as aliases so existing config
  /// files keep loading and roll over to Cava.
  #[default]
  #[serde(alias = "Classic", alias = "Equalizer")]
  Cava,
  /// BarGraph mode: Uses tui-bar-graph with Braille patterns for high-resolution display
  BarGraph,
}

impl VisualizerStyle {
  pub fn all() -> &'static [VisualizerStyle] {
    &[VisualizerStyle::Cava, VisualizerStyle::BarGraph]
  }

  pub fn name(&self) -> &'static str {
    match self {
      VisualizerStyle::Cava => "Cava",
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
  // "ctrl" with no dash and "ctrl-" with nothing after the dash are config
  // typos; they must surface as config errors naming the binding, never a
  // panic before the UI starts (#441).
  fn modifier_char(key: &str, sections: &[&str]) -> Result<char> {
    let mut chars = sections
      .get(1)
      .map(|section| section.chars())
      .ok_or_else(|| {
        anyhow!(
          "The shortcut \"{}\" is missing its key, e.g. \"ctrl-a\"",
          key
        )
      })?;
    match (chars.next(), chars.next()) {
      (Some(c), None) => Ok(c),
      _ => Err(anyhow!(
        "The shortcut \"{}\" must combine the modifier with exactly one key, e.g. \"ctrl-a\"",
        key
      )),
    }
  }

  match key.len() {
    1 => match key.chars().next() {
      Some(c) => Ok(Key::Char(c)),
      None => Err(anyhow!("The key binding is empty")),
    },
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
        "ctrl" => Ok(Key::Ctrl(modifier_char(&key, &sections)?)),
        "alt" => Ok(Key::Alt(modifier_char(&key, &sections)?)),
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
  #[cfg(feature = "ai-dj")]
  dj_open: Option<String>,
  #[cfg(feature = "ai-dj")]
  dj_toggle_auto_queue: Option<String>,
  #[cfg(feature = "ai-dj")]
  dj_vibe_shift: Option<String>,
  #[cfg(feature = "ai-dj")]
  dj_toggle_fresh_only: Option<String>,
  #[cfg(feature = "ai-dj")]
  dj_pick_model: Option<String>,
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
  remove_from_queue: Option<String>,
  open_settings: Option<String>,
  save_settings: Option<String>,
  listening_party: Option<String>,
  like_track: Option<String>,
  generate_recap: Option<String>,
}

#[derive(Clone, PartialEq)]
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
  /// Open the AI DJ screen. A global binding rather than only a sidebar row: the
  /// Library panel is not drawn for non-Spotify sources, so the row alone would
  /// make the DJ unreachable there.
  #[cfg(feature = "ai-dj")]
  pub dj_open: Key,
  #[cfg(feature = "ai-dj")]
  pub dj_toggle_auto_queue: Key,
  #[cfg(feature = "ai-dj")]
  pub dj_vibe_shift: Key,
  /// Toggle "only tracks I do not already have" for DJ recommendations.
  #[cfg(feature = "ai-dj")]
  pub dj_toggle_fresh_only: Key,
  /// Reopen the "which AI, which model" picker. Which brain the DJ uses is the one
  /// setting that costs money (or quota) per turn, so it has to be changeable
  /// without hand-editing YAML.
  #[cfg(feature = "ai-dj")]
  pub dj_pick_model: Key,
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
  pub remove_from_queue: Key,
  pub open_settings: Key,
  pub save_settings: Key,
  pub listening_party: Key,
  pub like_track: Key,
  pub generate_recap: Key,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BehaviorConfigString {
  pub seek_milliseconds: Option<u32>,
  pub volume_increment: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volume_percent: Option<u8>,
  pub tick_rate_milliseconds: Option<u64>,
  pub animation_tick_rate_milliseconds: Option<u64>,
  pub enable_text_emphasis: Option<bool>,
  pub banner_gradient: Option<bool>,
  pub show_loading_indicator: Option<bool>,
  pub enforce_wide_search_bar: Option<bool>,
  pub group_folders_first: Option<bool>,
  pub enable_global_song_count: Option<bool>,
  pub disable_mouse_inputs: Option<bool>,
  pub enable_discord_rpc: Option<bool>,
  pub discord_rpc_client_id: Option<String>,
  pub enable_announcements: Option<bool>,
  pub announcement_feed_url: Option<String>,
  pub enable_monthly_recap_prompt: Option<bool>,
  pub pin_community_playlist: Option<bool>,
  pub liked_icon: Option<String>,
  pub shuffle_icon: Option<String>,
  pub repeat_track_icon: Option<String>,
  pub repeat_context_icon: Option<String>,
  pub playing_icon: Option<String>,
  pub paused_icon: Option<String>,
  pub set_window_title: Option<bool>,
  pub visualizer_style: Option<VisualizerStyle>,
  pub relay_server_url: Option<String>,
  pub stop_after_current_track: Option<bool>,
  pub startup_behavior: Option<StartupBehavior>,
  pub disable_auto_update: Option<bool>,
  pub auto_update_delay: Option<String>,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art: Option<bool>,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art_forced: Option<bool>,
  #[cfg(feature = "cover-art")]
  pub playbar_cover_art_size_percent: Option<u16>,
  #[cfg(feature = "cover-art")]
  pub cover_art_theme: Option<bool>,
  #[cfg(feature = "mcp-server")]
  pub mcp_enabled: Option<bool>,
  #[cfg(feature = "ai-dj")]
  pub dj_backend: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_agent_command: Option<Vec<String>>,
  #[cfg(feature = "ai-dj")]
  pub dj_agent_prompt_via: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_agent_timeout_secs: Option<u64>,
  #[cfg(feature = "ai-dj")]
  pub dj_agent_model: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_model: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_base_url: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_api_key: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_batch_size: Option<usize>,
  #[cfg(feature = "ai-dj")]
  pub dj_history_period: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_avoid_library: Option<bool>,
  #[cfg(feature = "ai-dj")]
  pub dj_configured: Option<bool>,
  pub keepawake_enabled: Option<bool>,
  pub enable_media_keys: Option<bool>,
  pub sync_token: Option<String>,
  pub local_music_path: Option<String>,
  pub subsonic_url: Option<String>,
  pub subsonic_username: Option<String>,
  pub subsonic_password: Option<String>,
  pub ytdlp_path: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub radio_stations: Option<Vec<RadioStationConfig>>,
  // --- Phase 2: icons / glyphs / labels (defaults = today's glyphs) ---
  pub gauge_filled_icon: Option<String>,
  pub gauge_unfilled_icon: Option<String>,
  pub active_source_icon: Option<String>,
  pub episode_played_icon: Option<String>,
  pub sort_ascending_icon: Option<String>,
  pub sort_descending_icon: Option<String>,
  pub list_highlight_icon: Option<String>,
  pub playbar_control_labels: Option<HashMap<String, String>>,
  // --- Phase 3: behavior constants / startup / sort ---
  pub status_message_ttl_percent: Option<u16>,
  pub playback_poll_seconds: Option<u64>,
  pub table_scroll_padding: Option<u16>,
  pub like_animation_frames: Option<u8>,
  pub startup_route: Option<String>,
  pub default_sort_playlist_tracks: Option<String>,
  pub default_sort_saved_albums: Option<String>,
  pub default_sort_saved_artists: Option<String>,
  pub default_sort_recently_played: Option<String>,
  // --- Phase 6: layout arrangement ---
  pub sidebar_position: Option<String>,
  pub playbar_position: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sidebar_width_percent: Option<u8>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub playbar_height_rows: Option<u16>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub library_height_percent: Option<u8>,
  pub small_terminal_width: Option<u16>,
  pub small_terminal_height: Option<u16>,
}

#[derive(Clone)]
pub struct BehaviorConfig {
  pub seek_milliseconds: u32,
  pub volume_increment: u8,
  /// User-configured initial volume default. It seeds `state.yml` only when
  /// runtime state has no saved volume yet; later runtime volume changes stay
  /// authoritative.
  pub volume_percent: Option<u8>,
  pub tick_rate_milliseconds: u64,
  pub animation_tick_rate_milliseconds: u64,
  pub enable_text_emphasis: bool,
  /// When false, the home banner is drawn in the theme's banner color instead
  /// of the animated RGB gradient, so ANSI palettes (e.g. pywal) restyle it live.
  pub banner_gradient: bool,
  pub show_loading_indicator: bool,
  pub enforce_wide_search_bar: bool,
  pub group_folders_first: bool,
  pub enable_global_song_count: bool,
  pub disable_mouse_inputs: bool,
  pub enable_discord_rpc: bool,
  pub discord_rpc_client_id: Option<String>,
  pub enable_announcements: bool,
  pub announcement_feed_url: Option<String>,
  pub enable_monthly_recap_prompt: bool,
  /// Pin the public "spotatui community" playlist to the top of the Spotify
  /// playlists sidebar.
  pub pin_community_playlist: bool,
  pub liked_icon: String,
  pub shuffle_icon: String,
  pub repeat_track_icon: String,
  pub repeat_context_icon: String,
  pub playing_icon: String,
  pub paused_icon: String,
  pub set_window_title: bool,
  pub visualizer_style: VisualizerStyle,
  pub relay_server_url: String,
  pub stop_after_current_track: bool,
  pub startup_behavior: StartupBehavior,
  pub disable_auto_update: bool,
  pub auto_update_delay: String,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art: bool,
  #[cfg(feature = "cover-art")]
  pub draw_cover_art_forced: bool,
  #[cfg(feature = "cover-art")]
  pub playbar_cover_art_size_percent: u16,
  /// Recolor the UI accents from the current track's cover art, fading on
  /// track change. Off by default so a chosen preset stays untouched.
  #[cfg(feature = "cover-art")]
  pub cover_art_theme: bool,
  /// Whether to open the local MCP control socket so `spotatui mcp` (and through
  /// it Claude Code, Codex, or any MCP client) can drive playback.
  ///
  /// Off by default. Opening a socket that can control the player and read
  /// listening history is a security-posture change, so it is a deliberate act;
  /// the socket binds loopback-only and requires the token from
  /// `~/.config/spotatui/mcp.json`.
  #[cfg(feature = "mcp-server")]
  pub mcp_enabled: bool,
  /// Which DJ brain to use: `agent_cli`, `anthropic`, or `openai_compat`.
  #[cfg(feature = "ai-dj")]
  pub dj_backend: String,
  /// argv for the `agent_cli` backend. A config field rather than a hardcoded
  /// table so any headless agent works and spotatui never tracks their flags.
  #[cfg(feature = "ai-dj")]
  pub dj_agent_command: Vec<String>,
  /// How the prompt reaches that CLI: `stdin` or `arg`.
  ///
  /// `None` means "unset: the preset decides". It has to be optional, because a
  /// resolved `String` here can never fall through to the preset's own mode, and
  /// getting it wrong is not cosmetic: `agy` ignores stdin entirely, so a prompt
  /// written there is silently dropped and the DJ answers something else.
  #[cfg(feature = "ai-dj")]
  pub dj_agent_prompt_via: Option<String>,
  #[cfg(feature = "ai-dj")]
  pub dj_agent_timeout_secs: u64,
  /// Model for the `agent_cli` backend, passed as that CLI's own model flag
  /// (`claude --model haiku`).
  ///
  /// Separate from [`Self::dj_model`] on purpose: they are different namespaces.
  /// `dj_model` is an API model id billed per token; this is a CLI alias spent
  /// against the subscription the CLI is already logged into. One field would make
  /// `claude-haiku-4-5` and `haiku` interchangeable, and they are not.
  #[cfg(feature = "ai-dj")]
  pub dj_agent_model: Option<String>,
  /// Model id for the API backends (`anthropic`, `openai_compat`).
  #[cfg(feature = "ai-dj")]
  pub dj_model: Option<String>,
  /// Base URL for `openai_compat`. Defaults to Ollama's local endpoint.
  #[cfg(feature = "ai-dj")]
  pub dj_base_url: Option<String>,
  /// API key for the HTTP backends.
  ///
  /// **Stored in plaintext in the YAML config** — prefer the
  /// `SPOTATUI_DJ_API_KEY` environment variable, which overrides this field at
  /// request time and is never written to disk. The config directory is `0700`
  /// on unix and carries a `.gitignore`, but a plaintext secret is still a
  /// plaintext secret.
  #[cfg(feature = "ai-dj")]
  pub dj_api_key: Option<String>,
  /// How many tracks the DJ queues per round. Clamped to the resolver's cap.
  #[cfg(feature = "ai-dj")]
  pub dj_batch_size: usize,
  /// History window the taste brief summarises: `7d`, `30d`, `month`, `year`,
  /// `all`.
  #[cfg(feature = "ai-dj")]
  pub dj_history_period: String,
  /// Start the DJ in "only tracks I do not already have" mode: reject anything in
  /// Liked Songs or the listener's own playlists instead of recommending it.
  ///
  /// Only the starting value; the DJ screen toggles it per session, because which
  /// mode is wanted depends on the ask. Turning it on costs one crawl of every
  /// playlist the first time it is used in a session.
  #[cfg(feature = "ai-dj")]
  pub dj_avoid_library: bool,
  /// Whether the DJ's backend and model have ever been chosen deliberately.
  ///
  /// `Option<bool>`, not `bool`. Key *presence* cannot be the signal: `save_config`
  /// writes every `dj_*` key unconditionally as `Some(...)`, and it saves from hot
  /// paths (volume, shuffle, sidebar resize, shutdown, first run), so every install
  /// that has ever changed its volume already has the whole `dj_*` block on disk.
  /// A bare `bool` is worse still: the first unrelated save would write `false` and
  /// pin it there forever. Only the `Option` fields survive `build_behavior`
  /// untouched, so this round-trips as `null` until the picker sets it.
  #[cfg(feature = "ai-dj")]
  pub dj_configured: Option<bool>,
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
  /// Path to the `yt-dlp` binary used by the YouTube source. `None` resolves
  /// plain `yt-dlp` through `$PATH`.
  pub ytdlp_path: Option<String>,
  /// User-authored stations shown alongside stations saved at runtime.
  /// In-app favorite/remove actions mutate `state.yml`, not this list.
  pub radio_stations: Vec<RadioStationConfig>,
  // --- Phase 2: icons / glyphs / labels ---
  pub gauge_filled_icon: String,
  pub gauge_unfilled_icon: String,
  pub active_source_icon: String,
  pub episode_played_icon: String,
  pub sort_ascending_icon: String,
  pub sort_descending_icon: String,
  pub list_highlight_icon: String,
  /// Optional override of playbar control button labels, keyed by
  /// `prev`/`play_pause`/`next`/`shuffle`/`repeat`/`like`/`vol_down`/`vol_up`.
  pub playbar_control_labels: HashMap<String, String>,
  // --- Phase 3: behavior constants / startup / sort ---
  pub status_message_ttl_percent: u16,
  pub playback_poll_seconds: u64,
  pub table_scroll_padding: u16,
  pub like_animation_frames: u8,
  pub startup_route: String,
  pub default_sort_playlist_tracks: String,
  pub default_sort_saved_albums: String,
  pub default_sort_saved_artists: String,
  pub default_sort_recently_played: String,
  // --- Phase 6: layout arrangement ---
  pub sidebar_position: String,
  pub playbar_position: String,
  /// User-configured initial pane-size defaults. They seed `state.yml` only
  /// when runtime state has no saved sizes yet; later runtime resize changes
  /// stay authoritative.
  pub sidebar_width_percent: Option<u8>,
  pub playbar_height_rows: Option<u16>,
  pub library_height_percent: Option<u8>,
  pub small_terminal_width: u16,
  pub small_terminal_height: u16,
}

/// The DJ brains a config may name. Module-level rather than a local `const` in
/// the load validator so the validator, the picker and [`BehaviorConfig::dj_is_configured`]
/// cannot drift apart.
#[cfg(feature = "ai-dj")]
pub const DJ_BACKENDS: [&str; 3] = ["agent_cli", "anthropic", "openai_compat"];

/// The shipped default backend. Also what "not configured" compares against.
#[cfg(feature = "ai-dj")]
pub const DEFAULT_DJ_BACKEND: &str = "agent_cli";

/// The shipped default `agent_cli` argv.
///
/// A function rather than a `const` because it allocates, and one source of truth
/// because [`BehaviorConfig::dj_is_configured`] asks "is this still the value
/// spotatui shipped?" — a second copy would answer that question wrongly the first
/// time either changed.
#[cfg(feature = "ai-dj")]
pub fn default_dj_agent_command() -> Vec<String> {
  vec!["claude".to_string(), "-p".to_string()]
}

impl BehaviorConfig {
  // `emphasis` moved to `tui/theme.rs::EmphasisExt` (it returns a terminal
  // `Modifier` type, which core no longer names).

  /// Has the DJ already been set up, by the picker or by hand?
  ///
  /// Two signals, and both are needed. The marker covers "the picker ran, or the
  /// user dismissed it". Value-differs-from-default covers the user who configured
  /// the DJ in their YAML before the picker existed: those values can only have
  /// been typed, because `save_config` writes the defaults back verbatim.
  ///
  /// Deliberately absent: `dj_batch_size`, `dj_history_period`, `dj_avoid_library`
  /// and the timeout are tuning, not a choice of AI. So is `dj_agent_prompt_via`,
  /// and that one is the trap — `save_config` already wrote `stdin` into every
  /// existing install, so counting it would mean nobody is ever asked, including
  /// the Claude Pro users this picker exists for.
  #[cfg(feature = "ai-dj")]
  pub fn dj_is_configured(&self) -> bool {
    if self.dj_configured == Some(true) {
      return true;
    }
    self.dj_backend != DEFAULT_DJ_BACKEND
      || self.dj_agent_command != default_dj_agent_command()
      || self.dj_agent_model.is_some()
      || self.dj_model.is_some()
      || self.dj_api_key.is_some()
      || self.dj_base_url.is_some()
  }
}

// ===== Phase 4: format templates =====

/// Placeholder keys available to every format template, in index order.
pub const FORMAT_KEYS: &[&str] = &[
  "state", "device", "source", "queue", "shuffle", "repeat", "volume", "party",
];

/// On-disk format config: all templates optional, defaulting to today's output.
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormatConfigString {
  pub playbar_status: Option<String>,
  pub playbar_status_source: Option<String>,
  pub window_title: Option<String>,
}

/// Parsed format templates. Defaults reproduce today's `format!` output
/// byte-for-byte.
#[derive(Clone, Debug, PartialEq)]
pub struct FormatConfig {
  /// Spotify playbar title: `"{state} ({device} | Shuffle: {shuffle} | Repeat: {repeat} | Volume: {volume}%){party}"`
  pub playbar_status: Template,
  /// Local-source playbar title: `"{state} ({source}{queue}{shuffle}{repeat} | Volume: {volume}%)"`.
  /// The `{shuffle}` / `{repeat}` placeholders carry their own ` | Label: value`
  /// prefix (like `{artist}` in the window title) and render empty for sources
  /// without the controls (internet radio, native queue slots), so their labels
  /// never leak as blank controls.
  pub playbar_status_source: Template,
  /// Window title: `"{state}: {artist} - {title}"`
  pub window_title: Template,
}

impl FormatConfig {
  /// Today's hardcoded Spotify playbar format string.
  pub const DEFAULT_PLAYBAR_STATUS: &'static str =
    "{state} ({device} | Shuffle: {shuffle} | Repeat: {repeat} | Volume: {volume}%){party}";
  /// Today's hardcoded local-source playbar format string. `{shuffle}` and
  /// `{repeat}` include their own ` | Label: value` prefix so they can render
  /// empty (hiding the whole segment) for sources without those controls.
  pub const DEFAULT_PLAYBAR_STATUS_SOURCE: &'static str =
    "{state} ({source}{queue}{shuffle}{repeat} | Volume: {volume}%)";
  /// Today's hardcoded window-title format string: `"{title} — {artist}"`.
  /// (The artist segment is composed by the call site and omitted when empty.)
  pub const DEFAULT_WINDOW_TITLE: &'static str = "{title}{artist}";

  /// The keys valid for window-title templates (a subset: artist/title are
  /// resolved at the call site, not via FORMAT_KEYS).
  pub const WINDOW_TITLE_KEYS: &'static [&'static str] = &["title", "artist"];

  pub fn default_templates() -> Self {
    Self {
      playbar_status: Template::parse(Self::DEFAULT_PLAYBAR_STATUS, FORMAT_KEYS)
        .expect("default playbar_status template must parse"),
      playbar_status_source: Template::parse(Self::DEFAULT_PLAYBAR_STATUS_SOURCE, FORMAT_KEYS)
        .expect("default playbar_status_source template must parse"),
      window_title: Template::parse(Self::DEFAULT_WINDOW_TITLE, Self::WINDOW_TITLE_KEYS)
        .expect("default window_title template must parse"),
    }
  }
}

impl Default for FormatConfig {
  fn default() -> Self {
    Self::default_templates()
  }
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserConfigString {
  keybindings: Option<KeyBindingsString>,
  behavior: Option<BehaviorConfigString>,
  theme: Option<UserTheme>,
  plugin_commands: Option<HashMap<String, String>>,
  format: Option<FormatConfigString>,
  tables: Option<TablesConfigString>,
}

// ===== Phase 5: table columns =====

/// A single on-disk column spec. `header` overrides the default display text.
/// Exactly one of `width_percent` / `width` may be set; both set (or neither
/// for a column that expects a fixed default) — specifying both is a hard
/// error. When neither is set, the column's built-in default width applies.
#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColumnSpec {
  /// Defaulted so an entry missing `id` fails that table's resolution (a
  /// recoverable, warn-level error) instead of failing the whole YAML parse.
  #[serde(default)]
  pub id: String,
  pub header: Option<String>,
  pub width_percent: Option<f32>,
  pub width: Option<u16>,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TablesConfigString {
  pub songs: Option<Vec<ColumnSpec>>,
  pub album_tracks: Option<Vec<ColumnSpec>>,
  pub albums: Option<Vec<ColumnSpec>>,
  pub podcasts: Option<Vec<ColumnSpec>>,
  pub episodes: Option<Vec<ColumnSpec>>,
  pub recently_played: Option<Vec<ColumnSpec>>,
}

/// Validated (but not yet render-bound) per-table column lists. Defaults are
/// represented as empty `Vec`s; the rendering layer substitutes the built-in
/// default columns when a table is empty. This keeps `core` free of `tui`
/// dependencies — the column registry lives in `tui::ui::columns`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TablesConfig {
  pub songs: Vec<ColumnSpec>,
  pub album_tracks: Vec<ColumnSpec>,
  pub albums: Vec<ColumnSpec>,
  pub podcasts: Vec<ColumnSpec>,
  pub episodes: Vec<ColumnSpec>,
  pub recently_played: Vec<ColumnSpec>,
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
  /// Parsed format templates (Phase 4).
  pub format: FormatConfig,
  /// Resolved per-table column layouts (Phase 5).
  pub tables: TablesConfig,
}

impl UserConfig {
  /// Get the spotatui app config directory.
  /// Returns None if neither an absolute XDG config directory nor $HOME is set.
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
        #[cfg(feature = "ai-dj")]
        dj_open: Key::Ctrl('j'),
        #[cfg(feature = "ai-dj")]
        dj_toggle_auto_queue: Key::Ctrl('t'),
        #[cfg(feature = "ai-dj")]
        dj_vibe_shift: Key::Ctrl('y'),
        // Ctrl+O for "only new". The readline-ish keys the DJ prompt implements
        // (Ctrl+A/B/D/E/F/H/U) are all spoken for, and this has to work while the
        // prompt has focus.
        #[cfg(feature = "ai-dj")]
        dj_toggle_fresh_only: Key::Ctrl('o'),
        // Ctrl+G, chosen because it is the only unused modifier key that also
        // survives the DJ prompt: Ctrl+L is the macOS settings alias, Ctrl+N is
        // `down_event` for every list in the app, and Ctrl+K / Ctrl+W kill a line in
        // the search input. Needs a modifier at all because the DJ prompt takes
        // every bare character.
        #[cfg(feature = "ai-dj")]
        dj_pick_model: Key::Ctrl('g'),
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
        remove_from_queue: Key::Char('x'),
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
        volume_percent: None,
        tick_rate_milliseconds: DEFAULT_TICK_RATE_MILLISECONDS,
        animation_tick_rate_milliseconds: DEFAULT_ANIMATION_TICK_RATE_MILLISECONDS,
        enable_text_emphasis: true,
        banner_gradient: true,
        show_loading_indicator: true,
        enforce_wide_search_bar: false,
        group_folders_first: false,
        enable_global_song_count: true,
        disable_mouse_inputs: false,
        enable_discord_rpc: true,
        discord_rpc_client_id: None,
        enable_announcements: true,
        announcement_feed_url: None,
        enable_monthly_recap_prompt: true,
        pin_community_playlist: true,
        liked_icon: "♥".to_string(),
        shuffle_icon: "🔀".to_string(),
        repeat_track_icon: "🔂".to_string(),
        repeat_context_icon: "🔁".to_string(),
        playing_icon: "▶".to_string(),
        paused_icon: "⏸".to_string(),
        set_window_title: true,
        visualizer_style: VisualizerStyle::default(),
        relay_server_url: "wss://spotatui-party.spotatui.workers.dev/ws".to_string(),
        stop_after_current_track: false,
        startup_behavior: StartupBehavior::Continue,
        disable_auto_update: false,
        auto_update_delay: "0".to_string(),
        #[cfg(feature = "cover-art")]
        draw_cover_art: true,
        #[cfg(feature = "cover-art")]
        draw_cover_art_forced: false,
        #[cfg(feature = "cover-art")]
        playbar_cover_art_size_percent: 100,
        #[cfg(feature = "cover-art")]
        cover_art_theme: false,
        #[cfg(feature = "mcp-server")]
        mcp_enabled: false,
        #[cfg(feature = "ai-dj")]
        dj_backend: DEFAULT_DJ_BACKEND.to_string(),
        #[cfg(feature = "ai-dj")]
        dj_agent_command: default_dj_agent_command(),
        // Unset, so the preset's own delivery mode applies. Pinning "stdin" here
        // would silently break `agy`, which reads the prompt from argv only.
        #[cfg(feature = "ai-dj")]
        dj_agent_prompt_via: None,
        #[cfg(feature = "ai-dj")]
        dj_agent_timeout_secs: 90,
        #[cfg(feature = "ai-dj")]
        dj_agent_model: None,
        #[cfg(feature = "ai-dj")]
        dj_model: None,
        #[cfg(feature = "ai-dj")]
        dj_base_url: None,
        #[cfg(feature = "ai-dj")]
        dj_api_key: None,
        #[cfg(feature = "ai-dj")]
        dj_batch_size: crate::infra::dj::DEFAULT_BATCH,
        #[cfg(feature = "ai-dj")]
        dj_history_period: "30d".to_string(),
        // Off by default: the filter costs a playlist crawl, and "more like this"
        // is a perfectly reasonable thing to want from a DJ.
        #[cfg(feature = "ai-dj")]
        dj_avoid_library: false,
        #[cfg(feature = "ai-dj")]
        dj_configured: None,
        keepawake_enabled: true,
        enable_media_keys: true,
        sync_token: None,
        local_music_path: dirs::audio_dir().map(|p| p.to_string_lossy().to_string()),
        subsonic_url: None,
        subsonic_username: None,
        subsonic_password: None,
        ytdlp_path: None,
        radio_stations: Vec::new(),
        // --- Phase 2: icons / glyphs / labels (defaults = today's glyphs) ---
        gauge_filled_icon: "⣿".to_string(),
        gauge_unfilled_icon: "⣉".to_string(),
        active_source_icon: "●".to_string(),
        episode_played_icon: "✔".to_string(),
        sort_ascending_icon: "↑".to_string(),
        sort_descending_icon: "↓".to_string(),
        list_highlight_icon: "▶".to_string(),
        playbar_control_labels: HashMap::new(),
        // --- Phase 3: behavior constants / startup / sort ---
        status_message_ttl_percent: 100,
        playback_poll_seconds: 5,
        table_scroll_padding: 5,
        like_animation_frames: 10,
        startup_route: "home".to_string(),
        default_sort_playlist_tracks: "default".to_string(),
        default_sort_saved_albums: "default".to_string(),
        default_sort_saved_artists: "default".to_string(),
        default_sort_recently_played: "default".to_string(),
        // --- Phase 6: layout arrangement ---
        sidebar_position: "left".to_string(),
        playbar_position: "bottom".to_string(),
        sidebar_width_percent: None,
        playbar_height_rows: None,
        library_height_percent: None,
        small_terminal_width: 150,
        small_terminal_height: 45,
      },
      path_to_config: None,
      // Phase 4 / 5: parsed templates + resolved columns default to today's
      // built-in output (empty TablesConfig == built-in default columns).
      format: FormatConfig::default_templates(),
      tables: TablesConfig::default(),
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
          match parse_key(key_string) {
            Ok(key) => self.keys.$name = key,
            // One typo'd binding must not stop the app from launching:
            // warn and keep that binding's default, like the rest of the
            // config validation (#441).
            Err(e) => log::warn!(
              "[config] keybindings.{}: {e}; keeping the default",
              stringify!($name)
            ),
          }
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
    #[cfg(feature = "ai-dj")]
    to_keys!(dj_open);
    #[cfg(feature = "ai-dj")]
    to_keys!(dj_toggle_auto_queue);
    #[cfg(feature = "ai-dj")]
    to_keys!(dj_vibe_shift);
    #[cfg(feature = "ai-dj")]
    to_keys!(dj_toggle_fresh_only);
    #[cfg(feature = "ai-dj")]
    to_keys!(dj_pick_model);
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
    to_keys!(remove_from_queue);
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

  /// Resolve the effective banner-gradient state once both the behavior and
  /// theme sections have loaded: an explicit `banner_gradient` in the config
  /// always wins; otherwise the theme preset decides (the Terminal preset
  /// defaults to a solid banner so it can follow the terminal palette).
  fn resolve_banner_gradient(&mut self, explicit: Option<bool>) {
    self.behavior.banner_gradient =
      explicit.unwrap_or_else(|| self.current_preset.default_banner_gradient());
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
      self.behavior.volume_percent = Some(volume.min(100));
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

    if let Some(banner_gradient) = behavior_config.banner_gradient {
      self.behavior.banner_gradient = banner_gradient;
    }

    if let Some(loading_indicator) = behavior_config.show_loading_indicator {
      self.behavior.show_loading_indicator = loading_indicator;
    }

    if let Some(wide_search_bar) = behavior_config.enforce_wide_search_bar {
      self.behavior.enforce_wide_search_bar = wide_search_bar;
    }

    if let Some(group_folders_first) = behavior_config.group_folders_first {
      self.behavior.group_folders_first = group_folders_first;
    }

    if let Some(liked_icon) = behavior_config.liked_icon {
      self.behavior.liked_icon = liked_icon;
    }

    if let Some(paused_icon) = behavior_config.paused_icon {
      self.behavior.paused_icon = paused_icon;
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

    if let Some(enable_monthly_recap_prompt) = behavior_config.enable_monthly_recap_prompt {
      self.behavior.enable_monthly_recap_prompt = enable_monthly_recap_prompt;
    }

    if let Some(pin_community_playlist) = behavior_config.pin_community_playlist {
      self.behavior.pin_community_playlist = pin_community_playlist;
    }

    if let Some(announcement_feed_url) = behavior_config.announcement_feed_url {
      let trimmed = announcement_feed_url.trim();
      self.behavior.announcement_feed_url = if trimmed.is_empty() {
        None
      } else {
        Some(trimmed.to_string())
      };
    }

    if let Some(discord_rpc_client_id) = behavior_config.discord_rpc_client_id {
      self.behavior.discord_rpc_client_id = Some(discord_rpc_client_id);
    }

    if let Some(visualizer_style) = behavior_config.visualizer_style {
      self.behavior.visualizer_style = visualizer_style;
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

    if let Some(radio_stations) = behavior_config.radio_stations {
      self.behavior.radio_stations = sanitized_radio_stations(&radio_stations);
    }

    if let Some(sidebar_width_percent) = behavior_config.sidebar_width_percent {
      self.behavior.sidebar_width_percent = Some(sidebar_width_percent.min(100));
    }

    if let Some(playbar_height_rows) = behavior_config.playbar_height_rows {
      self.behavior.playbar_height_rows = Some(playbar_height_rows);
    }

    if let Some(library_height_percent) = behavior_config.library_height_percent {
      self.behavior.library_height_percent = Some(library_height_percent.min(100));
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
    #[cfg(feature = "cover-art")]
    if let Some(cover_art_theme) = behavior_config.cover_art_theme {
      self.behavior.cover_art_theme = cover_art_theme;
    }
    #[cfg(feature = "mcp-server")]
    if let Some(mcp_enabled) = behavior_config.mcp_enabled {
      self.behavior.mcp_enabled = mcp_enabled;
    }
    #[cfg(feature = "ai-dj")]
    {
      if let Some(dj_backend) = behavior_config.dj_backend {
        let normalized = dj_backend.trim().to_ascii_lowercase();
        if DJ_BACKENDS.contains(&normalized.as_str()) {
          self.behavior.dj_backend = normalized;
        } else {
          log::warn!(
            "behavior.dj_backend '{dj_backend}' is not one of {DJ_BACKENDS:?}; keeping '{}'",
            self.behavior.dj_backend
          );
        }
      }
      if let Some(dj_agent_command) = behavior_config.dj_agent_command {
        // Only argv[0] has to be a real word: it is the program to exec, and a
        // blank one reaches `spawn` as a confusing ENOENT. Later arguments are
        // left alone, since a CLI can legitimately take an empty one.
        if dj_agent_command
          .first()
          .is_some_and(|program| !program.trim().is_empty())
        {
          self.behavior.dj_agent_command = dj_agent_command;
        } else {
          log::warn!("behavior.dj_agent_command has no program to run; keeping the default");
        }
      }
      if let Some(dj_agent_prompt_via) = behavior_config.dj_agent_prompt_via {
        match crate::infra::dj::brain::agent_cli::PromptDelivery::from_config_str(
          &dj_agent_prompt_via,
        ) {
          // Store the canonical form, so `arg`/`argv`/`last-arg` all read back the
          // same way.
          Some(delivery) => {
            self.behavior.dj_agent_prompt_via = Some(delivery.to_config_str().to_string())
          }
          None => log::warn!(
            "behavior.dj_agent_prompt_via '{dj_agent_prompt_via}' is not 'stdin' or 'arg';              keeping '{}'",
            self
              .behavior
              .dj_agent_prompt_via
              .as_deref()
              .unwrap_or("unset")
          ),
        }
      }
      if let Some(secs) = behavior_config.dj_agent_timeout_secs {
        // A sub-second timeout would kill every agent before it started.
        self.behavior.dj_agent_timeout_secs = secs.clamp(5, 600);
      }
      if let Some(dj_agent_model) = behavior_config.dj_agent_model {
        self.behavior.dj_agent_model =
          Some(dj_agent_model).filter(|model| !model.trim().is_empty());
      }
      if let Some(dj_model) = behavior_config.dj_model {
        self.behavior.dj_model = Some(dj_model).filter(|model| !model.trim().is_empty());
      }
      if let Some(dj_base_url) = behavior_config.dj_base_url {
        self.behavior.dj_base_url = Some(dj_base_url).filter(|url| !url.trim().is_empty());
      }
      if let Some(dj_api_key) = behavior_config.dj_api_key {
        self.behavior.dj_api_key = Some(dj_api_key).filter(|key| !key.trim().is_empty());
      }
      if let Some(dj_batch_size) = behavior_config.dj_batch_size {
        self.behavior.dj_batch_size = dj_batch_size.clamp(1, crate::infra::dj::MAX_BATCH);
      }
      if let Some(dj_history_period) = behavior_config.dj_history_period {
        const PERIODS: [&str; 5] = ["7d", "30d", "month", "year", "all"];
        let normalized = dj_history_period.trim().to_ascii_lowercase();
        if PERIODS.contains(&normalized.as_str()) {
          self.behavior.dj_history_period = normalized;
        } else {
          log::warn!(
            "behavior.dj_history_period '{dj_history_period}' is not one of {PERIODS:?};              keeping '{}'",
            self.behavior.dj_history_period
          );
        }
      }
      if let Some(dj_avoid_library) = behavior_config.dj_avoid_library {
        self.behavior.dj_avoid_library = dj_avoid_library;
      }
      // No validation to do: the only value that means anything is `true`, and a
      // config that says `false` is telling us the picker has not run yet.
      if let Some(dj_configured) = behavior_config.dj_configured {
        self.behavior.dj_configured = Some(dj_configured);
      }
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
    if let Some(ytdlp_path) = trim_to_none(behavior_config.ytdlp_path) {
      self.behavior.ytdlp_path = Some(ytdlp_path);
    }

    // ===== Phase 2: icons / glyphs / labels =====
    // Width-restricted glyphs (column math depends on them) are validated to
    // exactly one terminal column; free-form labels are accepted as-is.
    // A bad glyph degrades to the built-in default with a warning rather than
    // failing config load (the app must stay launchable on a typo).
    let load_width1_icon = |dest: &mut String, value: Option<String>, field: &str| {
      if let Some(icon) = value {
        let icon = icon.trim().to_string();
        if icon.is_empty() {
          log::warn!("[config] {field} must not be empty; using default");
          return;
        }
        let width: usize = unicode_width::UnicodeWidthStr::width(icon.as_str());
        if width != 1 {
          log::warn!(
            "[config] {field} must be exactly one terminal column wide (got {width} columns): {icon}; using default"
          );
          return;
        }
        *dest = icon;
      }
    };
    load_width1_icon(
      &mut self.behavior.gauge_filled_icon,
      behavior_config.gauge_filled_icon,
      "gauge_filled_icon",
    );
    load_width1_icon(
      &mut self.behavior.gauge_unfilled_icon,
      behavior_config.gauge_unfilled_icon,
      "gauge_unfilled_icon",
    );
    // playing_icon prefixes the title cell of the playing row (padded to two
    // columns in padded_playing_icon), so it must be exactly one column wide.
    load_width1_icon(
      &mut self.behavior.playing_icon,
      behavior_config.playing_icon,
      "playing_icon",
    );
    // active_source_icon, list_highlight_icon render in free space, not a
    // fixed-width column → accept any non-empty glyph.
    if let Some(icon) = behavior_config.active_source_icon {
      let icon = icon.trim().to_string();
      if !icon.is_empty() {
        self.behavior.active_source_icon = icon;
      }
    }
    if let Some(icon) = behavior_config.list_highlight_icon {
      let icon = icon.trim().to_string();
      if !icon.is_empty() {
        self.behavior.list_highlight_icon = icon;
      }
    }
    // episode_played_icon renders in a width-2 "played" column (tables.rs),
    // so it must be exactly one column wide (the leading space is added at
    // the call site).
    load_width1_icon(
      &mut self.behavior.episode_played_icon,
      behavior_config.episode_played_icon,
      "episode_played_icon",
    );
    // sort direction icons render in a width-1 column.
    load_width1_icon(
      &mut self.behavior.sort_ascending_icon,
      behavior_config.sort_ascending_icon,
      "sort_ascending_icon",
    );
    load_width1_icon(
      &mut self.behavior.sort_descending_icon,
      behavior_config.sort_descending_icon,
      "sort_descending_icon",
    );
    // playbar control labels: free-form strings keyed by control id. Keep only
    // the known keys so typos don't silently no-op; empty values are dropped
    // (falling back to the built-in label).
    if let Some(labels) = behavior_config.playbar_control_labels {
      let allowed = [
        "prev",
        "play_pause",
        "next",
        "shuffle",
        "repeat",
        "like",
        "vol_down",
        "vol_up",
      ];
      let mut kept = HashMap::new();
      for (key, val) in labels {
        let key = key.trim().to_string();
        let val = val.trim().to_string();
        if !allowed.contains(&key.as_str()) {
          log::warn!(
            "[config] playbar_control_labels: skipping unknown key '{key}' (allowed: {})",
            allowed.join(", ")
          );
          continue;
        }
        if val.is_empty() {
          // empty == reset to default; drop the override
          continue;
        }
        kept.insert(key, val);
      }
      self.behavior.playbar_control_labels = kept;
    }

    // ===== Phase 3: behavior constants / startup / sort =====
    if let Some(pct) = behavior_config.status_message_ttl_percent {
      self.behavior.status_message_ttl_percent = pct.clamp(10, 1000);
    }
    if let Some(secs) = behavior_config.playback_poll_seconds {
      if secs < 1 {
        return Err(anyhow!(
          "playback_poll_seconds must be at least 1, is {secs}"
        ));
      }
      self.behavior.playback_poll_seconds = secs;
    }
    if let Some(padding) = behavior_config.table_scroll_padding {
      self.behavior.table_scroll_padding = padding;
    }
    if let Some(frames) = behavior_config.like_animation_frames {
      if frames < 1 {
        return Err(anyhow!(
          "like_animation_frames must be at least 1, is {frames}"
        ));
      }
      self.behavior.like_animation_frames = frames;
    }
    if let Some(route) = behavior_config.startup_route {
      let route = route.trim().to_string();
      if !route.is_empty() {
        // Validation of the route id happens in App::apply_startup_route();
        // store the raw string here so an unknown value degrades to Home + warn
        // rather than failing config load.
        self.behavior.startup_route = route;
      }
    }
    // Per-context default sort: "<field>" or "<field>:desc". Validate against
    // the context's available fields; a typo degrades to the default order
    // with a warning rather than failing config load.
    let load_sort_default = |dest: &mut String,
                             value: Option<String>,
                             ctx: crate::core::sort::SortContext,
                             field: &str| {
      if let Some(spec) = value {
        let spec = spec.trim().to_string();
        if spec.is_empty() {
          return;
        }
        match crate::core::sort::SortState::parse(&spec, ctx) {
          Ok(_) => *dest = spec,
          Err(e) => log::warn!("[config] {field}: {e}; using default sort"),
        }
      }
    };
    load_sort_default(
      &mut self.behavior.default_sort_playlist_tracks,
      behavior_config.default_sort_playlist_tracks,
      crate::core::sort::SortContext::PlaylistTracks,
      "default_sort_playlist_tracks",
    );
    load_sort_default(
      &mut self.behavior.default_sort_saved_albums,
      behavior_config.default_sort_saved_albums,
      crate::core::sort::SortContext::SavedAlbums,
      "default_sort_saved_albums",
    );
    load_sort_default(
      &mut self.behavior.default_sort_saved_artists,
      behavior_config.default_sort_saved_artists,
      crate::core::sort::SortContext::SavedArtists,
      "default_sort_saved_artists",
    );
    load_sort_default(
      &mut self.behavior.default_sort_recently_played,
      behavior_config.default_sort_recently_played,
      crate::core::sort::SortContext::RecentlyPlayed,
      "default_sort_recently_played",
    );

    // ===== Phase 6: layout arrangement =====
    if let Some(pos) = behavior_config.sidebar_position {
      let pos = pos.trim().to_string();
      match pos.as_str() {
        "left" | "right" | "hidden" => self.behavior.sidebar_position = pos,
        _ => log::warn!(
          "[config] sidebar_position '{pos}' is invalid (expected left|right|hidden); using left"
        ),
      }
    }
    if let Some(pos) = behavior_config.playbar_position {
      let pos = pos.trim().to_string();
      match pos.as_str() {
        "bottom" | "top" => self.behavior.playbar_position = pos,
        _ => log::warn!(
          "[config] playbar_position '{pos}' is invalid (expected bottom|top); using bottom"
        ),
      }
    }
    if let Some(w) = behavior_config.small_terminal_width {
      self.behavior.small_terminal_width = w.max(1);
    }
    if let Some(h) = behavior_config.small_terminal_height {
      self.behavior.small_terminal_height = h.max(1);
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
      #[cfg(feature = "ai-dj")]
      k.dj_open,
      #[cfg(feature = "ai-dj")]
      k.dj_toggle_auto_queue,
      #[cfg(feature = "ai-dj")]
      k.dj_vibe_shift,
      #[cfg(feature = "ai-dj")]
      k.dj_toggle_fresh_only,
      #[cfg(feature = "ai-dj")]
      k.dj_pick_model,
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
      k.remove_from_queue,
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

      let explicit_banner_gradient = config_yml
        .behavior
        .as_ref()
        .and_then(|behavior| behavior.banner_gradient);
      if let Some(behavior) = config_yml.behavior {
        self.load_behaviorconfig(behavior)?;
      }
      if let Some(theme) = config_yml.theme {
        self.load_theme(theme)?;
      }
      self.resolve_banner_gradient(explicit_banner_gradient);
      if let Some(plugin_commands) = config_yml.plugin_commands {
        self.load_plugin_commands(plugin_commands);
      }
      if let Some(format) = config_yml.format {
        self.load_formatconfig(format);
      }
      if let Some(tables) = config_yml.tables {
        self.load_tablesconfig(tables);
      }

      Ok(())
    } else {
      Ok(())
    }
  }

  /// Validate and apply format templates (Phase 4). Each template is parsed
  /// against `FORMAT_KEYS` (or the window-title subset); a parse error
  /// degrades that template to the built-in default with a warning listing
  /// the valid keys, so a typo never blocks app launch.
  pub fn load_formatconfig(&mut self, format: FormatConfigString) {
    if let Some(s) = format.playbar_status {
      match Template::parse(s.trim(), FORMAT_KEYS) {
        Ok(t) => self.format.playbar_status = t,
        Err(e) => log::warn!("[config] format.playbar_status: {e}; using default"),
      }
    }
    if let Some(s) = format.playbar_status_source {
      match Template::parse(s.trim(), FORMAT_KEYS) {
        Ok(t) => self.format.playbar_status_source = t,
        Err(e) => log::warn!("[config] format.playbar_status_source: {e}; using default"),
      }
    }
    if let Some(s) = format.window_title {
      match Template::parse(s.trim(), FormatConfig::WINDOW_TITLE_KEYS) {
        Ok(t) => self.format.window_title = t,
        Err(e) => log::warn!("[config] format.window_title: {e}; using default"),
      }
    }
  }

  /// Validate and apply table column specs (Phase 5). Unknown / duplicate
  /// ids, empty lists, or both-widths-set degrade that table to its built-in
  /// default columns with a warning listing valid ids, so a typo never
  /// blocks app launch.
  pub fn load_tablesconfig(&mut self, tables: TablesConfigString) {
    // Each table is validated against its registry of valid column ids (kept
    // in the rendering layer). Empty specs are dropped (== built-in defaults).
    let load = |table: &'static str, specs: Option<Vec<ColumnSpec>>| -> Vec<ColumnSpec> {
      match resolve_table_specs(table, specs) {
        Ok(specs) => specs,
        Err(e) => {
          log::warn!("[config] {e}; using default columns");
          Vec::new()
        }
      }
    };
    self.tables.songs = load("songs", tables.songs);
    self.tables.album_tracks = load("album_tracks", tables.album_tracks);
    self.tables.albums = load("albums", tables.albums);
    self.tables.podcasts = load("podcasts", tables.podcasts);
    self.tables.episodes = load("episodes", tables.episodes);
    self.tables.recently_played = load("recently_played", tables.recently_played);
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
      volume_percent: self.behavior.volume_percent,
      tick_rate_milliseconds: Some(self.behavior.tick_rate_milliseconds),
      animation_tick_rate_milliseconds: Some(self.behavior.animation_tick_rate_milliseconds),
      enable_text_emphasis: Some(self.behavior.enable_text_emphasis),
      banner_gradient: Some(self.behavior.banner_gradient),
      show_loading_indicator: Some(self.behavior.show_loading_indicator),
      enforce_wide_search_bar: Some(self.behavior.enforce_wide_search_bar),
      group_folders_first: Some(self.behavior.group_folders_first),
      enable_global_song_count: Some(self.behavior.enable_global_song_count),
      disable_mouse_inputs: Some(self.behavior.disable_mouse_inputs),
      enable_discord_rpc: Some(self.behavior.enable_discord_rpc),
      discord_rpc_client_id: self.behavior.discord_rpc_client_id.clone(),
      enable_announcements: Some(self.behavior.enable_announcements),
      announcement_feed_url: self.behavior.announcement_feed_url.clone(),
      enable_monthly_recap_prompt: Some(self.behavior.enable_monthly_recap_prompt),
      pin_community_playlist: Some(self.behavior.pin_community_playlist),
      liked_icon: Some(self.behavior.liked_icon.clone()),
      shuffle_icon: Some(self.behavior.shuffle_icon.clone()),
      repeat_track_icon: Some(self.behavior.repeat_track_icon.clone()),
      repeat_context_icon: Some(self.behavior.repeat_context_icon.clone()),
      playing_icon: Some(self.behavior.playing_icon.clone()),
      paused_icon: Some(self.behavior.paused_icon.clone()),
      set_window_title: Some(self.behavior.set_window_title),
      visualizer_style: Some(self.behavior.visualizer_style),
      relay_server_url: Some(self.behavior.relay_server_url.clone()),
      sync_token: self.behavior.sync_token.clone(),
      local_music_path: self.behavior.local_music_path.clone(),
      subsonic_url: self.behavior.subsonic_url.clone(),
      subsonic_username: self.behavior.subsonic_username.clone(),
      subsonic_password: self.behavior.subsonic_password.clone(),
      ytdlp_path: self.behavior.ytdlp_path.clone(),
      radio_stations: if self.behavior.radio_stations.is_empty() {
        None
      } else {
        Some(self.behavior.radio_stations.clone())
      },
      stop_after_current_track: Some(self.behavior.stop_after_current_track),
      startup_behavior: Some(self.behavior.startup_behavior),
      disable_auto_update: Some(self.behavior.disable_auto_update),
      auto_update_delay: Some(self.behavior.auto_update_delay.clone()),
      #[cfg(feature = "cover-art")]
      draw_cover_art: Some(self.behavior.draw_cover_art),
      #[cfg(feature = "cover-art")]
      draw_cover_art_forced: Some(self.behavior.draw_cover_art_forced),
      #[cfg(feature = "cover-art")]
      playbar_cover_art_size_percent: Some(self.behavior.playbar_cover_art_size_percent),
      #[cfg(feature = "cover-art")]
      cover_art_theme: Some(self.behavior.cover_art_theme),
      #[cfg(feature = "mcp-server")]
      mcp_enabled: Some(self.behavior.mcp_enabled),
      #[cfg(feature = "ai-dj")]
      dj_backend: Some(self.behavior.dj_backend.clone()),
      #[cfg(feature = "ai-dj")]
      dj_agent_command: Some(self.behavior.dj_agent_command.clone()),
      // Passed through rather than wrapped in `Some`: it is already an `Option`,
      // and `null` on disk is what lets the preset decide.
      #[cfg(feature = "ai-dj")]
      dj_agent_prompt_via: self.behavior.dj_agent_prompt_via.clone(),
      #[cfg(feature = "ai-dj")]
      dj_agent_timeout_secs: Some(self.behavior.dj_agent_timeout_secs),
      #[cfg(feature = "ai-dj")]
      dj_agent_model: self.behavior.dj_agent_model.clone(),
      #[cfg(feature = "ai-dj")]
      dj_model: self.behavior.dj_model.clone(),
      #[cfg(feature = "ai-dj")]
      dj_base_url: self.behavior.dj_base_url.clone(),
      #[cfg(feature = "ai-dj")]
      dj_api_key: self.behavior.dj_api_key.clone(),
      #[cfg(feature = "ai-dj")]
      dj_batch_size: Some(self.behavior.dj_batch_size),
      #[cfg(feature = "ai-dj")]
      dj_history_period: Some(self.behavior.dj_history_period.clone()),
      #[cfg(feature = "ai-dj")]
      dj_avoid_library: Some(self.behavior.dj_avoid_library),
      // Never `Some(false)`: an automatic save must not answer the picker's
      // question on the user's behalf.
      #[cfg(feature = "ai-dj")]
      dj_configured: self.behavior.dj_configured,
      keepawake_enabled: Some(self.behavior.keepawake_enabled),
      enable_media_keys: Some(self.behavior.enable_media_keys),
      // --- Phase 2/3/6 new fields (persist whatever the user set) ---
      gauge_filled_icon: Some(self.behavior.gauge_filled_icon.clone()),
      gauge_unfilled_icon: Some(self.behavior.gauge_unfilled_icon.clone()),
      active_source_icon: Some(self.behavior.active_source_icon.clone()),
      episode_played_icon: Some(self.behavior.episode_played_icon.clone()),
      sort_ascending_icon: Some(self.behavior.sort_ascending_icon.clone()),
      sort_descending_icon: Some(self.behavior.sort_descending_icon.clone()),
      list_highlight_icon: Some(self.behavior.list_highlight_icon.clone()),
      playbar_control_labels: if self.behavior.playbar_control_labels.is_empty() {
        None
      } else {
        Some(self.behavior.playbar_control_labels.clone())
      },
      status_message_ttl_percent: Some(self.behavior.status_message_ttl_percent),
      playback_poll_seconds: Some(self.behavior.playback_poll_seconds),
      table_scroll_padding: Some(self.behavior.table_scroll_padding),
      like_animation_frames: Some(self.behavior.like_animation_frames),
      startup_route: Some(self.behavior.startup_route.clone()),
      default_sort_playlist_tracks: Some(self.behavior.default_sort_playlist_tracks.clone()),
      default_sort_saved_albums: Some(self.behavior.default_sort_saved_albums.clone()),
      default_sort_saved_artists: Some(self.behavior.default_sort_saved_artists.clone()),
      default_sort_recently_played: Some(self.behavior.default_sort_recently_played.clone()),
      sidebar_position: Some(self.behavior.sidebar_position.clone()),
      playbar_position: Some(self.behavior.playbar_position.clone()),
      sidebar_width_percent: self.behavior.sidebar_width_percent,
      playbar_height_rows: self.behavior.playbar_height_rows,
      library_height_percent: self.behavior.library_height_percent,
      small_terminal_width: Some(self.behavior.small_terminal_width),
      small_terminal_height: Some(self.behavior.small_terminal_height),
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
      #[cfg(feature = "ai-dj")]
      dj_open: Some(key_to_config_string(self.keys.dj_open)),
      #[cfg(feature = "ai-dj")]
      dj_toggle_auto_queue: Some(key_to_config_string(self.keys.dj_toggle_auto_queue)),
      #[cfg(feature = "ai-dj")]
      dj_vibe_shift: Some(key_to_config_string(self.keys.dj_vibe_shift)),
      #[cfg(feature = "ai-dj")]
      dj_toggle_fresh_only: Some(key_to_config_string(self.keys.dj_toggle_fresh_only)),
      #[cfg(feature = "ai-dj")]
      dj_pick_model: Some(key_to_config_string(self.keys.dj_pick_model)),
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
      remove_from_queue: Some(key_to_config_string(self.keys.remove_from_queue)),
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
          format: None,
          tables: None,
        }
      }
    } else {
      UserConfigString {
        keybindings: Some(build_keybindings()),
        behavior: Some(build_behavior()),
        theme: Some(build_theme()),
        plugin_commands: None,
        format: None,
        tables: None,
      }
    };

    // Serialize to a String/bytes first, then write via a private-file helper
    // (0o600 on Unix — this file can carry the Subsonic password in
    // cleartext, so it deserves the same protection as the Spotify token
    // cache) using a temp-file + atomic rename, so a crash mid-write can't
    // corrupt the config. Do not log `content_yml`: it may contain plaintext
    // credentials.
    let content_yml = serde_yaml::to_string(&final_config)?;
    crate::core::auth::write_private_file_atomic(&paths.config_file_path, content_yml.as_bytes())?;

    Ok(())
  }
  pub fn padded_liked_icon(&self) -> String {
    format!("{} ", self.behavior.liked_icon)
  }

  /// The configured `playing_icon` followed by a single trailing space, for
  /// prepending to the title cell of the currently-playing row. Width-2
  /// (the icon is validated to one column at load time).
  pub fn padded_playing_icon(&self) -> String {
    format!("{} ", self.behavior.playing_icon)
  }
  #[cfg(feature = "cover-art")]
  pub fn do_draw_cover_art(&self, full_image_support: bool) -> bool {
    self.behavior.draw_cover_art && (self.behavior.draw_cover_art_forced || full_image_support)
  }

  /// Whether anything needs the cover art fetched and decoded: the art pane,
  /// or the adaptive theme, which extracts its palette from the decoded image
  /// even when the pane itself is hidden (disabled, or a terminal without
  /// image support). Draw sites keep their own `do_draw_cover_art` gate.
  #[cfg(feature = "cover-art")]
  pub fn needs_cover_art(&self, full_image_support: bool) -> bool {
    self.do_draw_cover_art(full_image_support) || self.behavior.cover_art_theme
  }
}

/// Canonical valid column ids per table. This is the single source of truth
/// shared by config validation (here) and the rendering registry
/// (`tui::ui::columns`). Adding a column id means adding it here *and* to the
/// registry; the round-trip test guards the two staying in sync.
pub fn valid_column_ids(table: &str) -> &'static [&'static str] {
  match table {
    "songs" | "album_tracks" | "recently_played" => {
      &["liked", "index", "title", "artist", "album", "length"]
    }
    "albums" => &["title", "artist", "date", "liked"],
    "podcasts" => &["title", "publisher"],
    "episodes" => &["played", "date", "title", "duration"],
    _ => &[],
  }
}

/// Validate a single table's column specs: unknown id, duplicate id, empty
/// list, or both widths set are hard errors. An empty/absent list yields an
/// empty `Vec` (== built-in default columns at render time).
fn resolve_table_specs(
  table: &'static str,
  specs: Option<Vec<ColumnSpec>>,
) -> Result<Vec<ColumnSpec>> {
  let Some(specs) = specs else {
    return Ok(Vec::new());
  };
  let valid = valid_column_ids(table);
  let mut seen: Vec<String> = Vec::new();
  let mut out = Vec::with_capacity(specs.len());
  for spec in specs {
    if spec.id.trim().is_empty() {
      return Err(anyhow!(
        "tables.{table}: column with empty id (valid ids: {})",
        valid.join(", ")
      ));
    }
    let id = spec.id.trim().to_string();
    if !valid.contains(&id.as_str()) {
      return Err(anyhow!(
        "tables.{table}: unknown column id '{id}' (valid: {})",
        valid.join(", ")
      ));
    }
    if seen.iter().any(|s| s == &id) {
      return Err(anyhow!("tables.{table}: duplicate column id '{id}'"));
    }
    if spec.width_percent.is_some() && spec.width.is_some() {
      return Err(anyhow!(
        "tables.{table}: column '{id}' sets both width_percent and width — pick one"
      ));
    }
    if let Some(pct) = spec.width_percent {
      if !(0.0..=100.0).contains(&pct) {
        return Err(anyhow!(
          "tables.{table}: column '{id}' width_percent {pct} out of range 0..=100"
        ));
      }
      if pct == 0.0 {
        return Err(anyhow!(
          "tables.{table}: column '{id}' has width_percent 0 (it would be invisible) — remove the column instead"
        ));
      }
    }
    if spec.width == Some(0) {
      return Err(anyhow!(
        "tables.{table}: column '{id}' has width 0 (it would be invisible) — remove the column instead"
      ));
    }
    seen.push(id.clone());
    out.push(ColumnSpec {
      id,
      header: spec
        .header
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty()),
      width_percent: spec.width_percent,
      width: spec.width,
    });
  }
  if out.is_empty() {
    return Err(anyhow!(
      "tables.{table}: column list must not be empty (omit the key to use defaults)"
    ));
  }
  let percent_sum: f32 = out.iter().filter_map(|c| c.width_percent).sum();
  if percent_sum > 100.0 {
    return Err(anyhow!(
      "tables.{table}: width_percent values sum to {percent_sum} (must be <= 100) — trailing columns would be clipped"
    ));
  }
  Ok(out)
}

#[cfg(test)]
mod tests {
  #[test]
  fn removed_visualizer_styles_deserialize_as_cava() {
    use super::VisualizerStyle;
    // Configs written before the Equalizer style was removed (or before it
    // was renamed from Classic) must keep loading, rolling over to Cava.
    for old_name in ["Equalizer", "Classic", "Cava"] {
      let style: VisualizerStyle = serde_yaml::from_str(old_name).unwrap();
      assert_eq!(style, VisualizerStyle::Cava, "{old_name}");
    }
    let style: VisualizerStyle = serde_yaml::from_str("BarGraph").unwrap();
    assert_eq!(style, VisualizerStyle::BarGraph);
  }

  #[test]
  fn test_parse_key() {
    use super::parse_key;
    use crate::core::input::Key;
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
  fn malformed_modifier_bindings_error_instead_of_panicking() {
    use super::parse_key;
    // "ctrl"/"alt" without a key (with or without the dash) used to index or
    // panic out of bounds during config load, aborting before the UI started;
    // multi-character suffixes were silently truncated to their first char.
    for bad in ["ctrl", "ctrl-", "ctrl-ab", "alt", "alt-", "alt-ab"] {
      let err = parse_key(bad.to_string()).unwrap_err();
      assert!(
        err.to_string().contains(bad),
        "error for {bad:?} must name the binding: {err}"
      );
    }
    assert!(parse_key(String::new()).is_err());
  }

  #[test]
  fn a_malformed_keybinding_keeps_the_default_and_still_loads() {
    use super::{KeyBindingsString, UserConfig};
    use crate::core::input::Key;

    let mut config = UserConfig::new();
    let default_back = config.keys.back;
    let bindings = KeyBindingsString {
      back: Some("ctrl-".to_string()),
      move_up: Some("w".to_string()),
      ..KeyBindingsString::default()
    };
    // The bad binding is warned about and skipped; the load succeeds and the
    // valid binding in the same section still applies.
    config.load_keybindings(bindings).unwrap();
    assert_eq!(config.keys.back, default_back);
    assert_eq!(config.keys.move_up, Key::Char('w'));
  }

  #[test]
  fn banner_gradient_defaults_off_for_terminal_preset_unless_explicit() {
    use super::{ThemePreset, UserConfig};

    let mut config = UserConfig::new();

    // No explicit config value: the preset decides
    config.current_preset = ThemePreset::Terminal;
    config.resolve_banner_gradient(None);
    assert!(!config.behavior.banner_gradient);

    config.current_preset = ThemePreset::Default;
    config.resolve_banner_gradient(None);
    assert!(config.behavior.banner_gradient);

    // An explicit value always wins over the preset default
    config.current_preset = ThemePreset::Terminal;
    config.resolve_banner_gradient(Some(true));
    assert!(config.behavior.banner_gradient);
  }

  /// Golden round-trip over a real-shaped `config.yml` fixture: the theme
  /// parser is hand-rolled (not serde-derived on `Color`), so moving it to
  /// `core/theme.rs` must keep the on-disk format byte-identical. Every value
  /// string in the fixture has to come back out of the real save path
  /// unchanged, and re-loading the saved file has to yield the same colors.
  #[test]
  fn theme_config_round_trips_byte_identically_through_save() {
    use super::{
      color_to_string, parse_theme_item, ThemePreset, UserConfig, UserConfigPaths,
      UserConfigString, UserTheme,
    };
    use crate::core::theme::Color;

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/theme_roundtrip.yml");
    let raw = std::fs::read_to_string(path).expect("theme fixture must exist");
    let fixture: UserConfigString =
      serde_yaml::from_str(&raw).expect("theme fixture must deserialize");
    let fixture_theme = fixture.theme.expect("fixture must carry a theme section");

    let mut config = UserConfig::new();
    config
      .load_theme(fixture_theme.clone())
      .expect("fixture theme must load");
    assert_eq!(config.current_preset, ThemePreset::Custom);

    // Spot-check the parse: named colors and both "r, g, b" triples.
    assert_eq!(config.theme.active, Color::Reset);
    assert_eq!(config.theme.banner, Color::Black);
    assert_eq!(config.theme.playbar_text, Color::Gray);
    assert_eq!(config.theme.header, Color::Rgb(23, 43, 45));
    assert_eq!(config.theme.highlighted_lyrics, Color::Rgb(255, 145, 205));

    // Save through the real path and compare the theme section field by
    // field: every string must be byte-identical to the fixture's.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: config_path.clone(),
    });
    config.save_config().unwrap();
    let saved_raw = std::fs::read_to_string(&config_path).unwrap();
    let saved: UserConfigString = serde_yaml::from_str(&saved_raw).unwrap();
    let saved_theme = saved.theme.expect("save_config must write the theme");
    let fields = |t: &UserTheme| {
      [
        t.preset.clone(),
        t.active.clone(),
        t.banner.clone(),
        t.error_border.clone(),
        t.error_text.clone(),
        t.hint.clone(),
        t.hovered.clone(),
        t.inactive.clone(),
        t.playbar_background.clone(),
        t.playbar_progress.clone(),
        t.playbar_progress_text.clone(),
        t.playbar_text.clone(),
        t.selected.clone(),
        t.text.clone(),
        t.background.clone(),
        t.header.clone(),
        t.highlighted_lyrics.clone(),
      ]
    };
    assert_eq!(fields(&saved_theme), fields(&fixture_theme));

    // Re-loading the saved file yields the same active theme.
    let mut reloaded = UserConfig::new();
    reloaded.load_theme(saved_theme).unwrap();
    assert_eq!(reloaded.theme, config.theme);

    // The three named colors the 16 fixture fields could not carry, plus the
    // fallback for `Indexed`, which has no on-disk representation today.
    for color in [Color::LightBlue, Color::LightMagenta, Color::LightCyan] {
      assert_eq!(parse_theme_item(&color_to_string(color)).unwrap(), color);
    }
    assert_eq!(color_to_string(Color::Indexed(42)), "Reset");
  }

  #[test]
  fn test_reserved_key() {
    use super::check_reserved_keys;
    use crate::core::input::Key;

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

  /// A config written by a build *with* the DJ features must still load in a
  /// build without them, and vice versa.
  ///
  /// This is a real cross-build hazard, not a hypothetical: a user can save from
  /// the Settings screen on an `ai-dj` build and then run a slim release binary.
  /// `BehaviorConfigString` has no `deny_unknown_fields`, so serde ignores keys it
  /// does not know — this test is what keeps that true if the derive ever changes.
  #[test]
  fn unknown_behavior_keys_are_ignored_rather_than_fatal() {
    use super::{BehaviorConfigString, UserConfig};

    // Every DJ/MCP key, as an `ai-dj` + `mcp-server` build would write them,
    // alongside a key that no build has ever had.
    let yaml = "
mcp_enabled: true
dj_backend: openai_compat
dj_agent_command:
  - claude
  - -p
dj_agent_prompt_via: stdin
dj_agent_timeout_secs: 120
dj_agent_model: haiku
dj_model: some-model
dj_base_url: http://localhost:11434/v1
dj_api_key: secret
dj_batch_size: 7
dj_history_period: 7d
dj_configured: true
a_key_from_the_future: 42
volume_increment: 5
";
    let parsed: BehaviorConfigString =
      serde_yaml::from_str(yaml).expect("unknown keys must not fail the parse");
    // The key every build does understand still came through.
    assert_eq!(parsed.volume_increment, Some(5));

    // And the merge accepts it without erroring.
    let mut config = UserConfig::new();
    config
      .load_behaviorconfig(parsed)
      .expect("loading must not fail");
    assert_eq!(config.behavior.volume_increment, 5);
  }

  /// The DJ keys a build *does* understand survive a save/load round trip.
  #[cfg(feature = "ai-dj")]
  #[test]
  fn dj_behavior_keys_round_trip_through_yaml() {
    use super::{BehaviorConfigString, UserConfig};

    let mut config = UserConfig::new();
    config.behavior.dj_backend = "openai_compat".to_string();
    config.behavior.dj_batch_size = 7;
    config.behavior.dj_history_period = "7d".to_string();
    config.behavior.dj_agent_command = vec!["codex".to_string()];
    config.behavior.dj_agent_prompt_via = Some("arg".to_string());
    config.behavior.dj_agent_model = Some("haiku".to_string());
    config.behavior.dj_agent_timeout_secs = 240;
    config.behavior.dj_model = Some("some-model".to_string());
    config.behavior.dj_base_url = Some("http://localhost:1234/v1".to_string());
    config.behavior.dj_api_key = Some("sk-not-a-real-key".to_string());
    config.behavior.dj_avoid_library = true;
    config.behavior.dj_configured = Some(true);

    // Mirrors what `save_config` writes, then reads it back. Every persisted DJ
    // key belongs here: one left out is one a persistence regression keeps green.
    let written = serde_yaml::to_string(&BehaviorConfigString {
      dj_backend: Some(config.behavior.dj_backend.clone()),
      dj_batch_size: Some(config.behavior.dj_batch_size),
      dj_history_period: Some(config.behavior.dj_history_period.clone()),
      dj_agent_command: Some(config.behavior.dj_agent_command.clone()),
      dj_agent_prompt_via: config.behavior.dj_agent_prompt_via.clone(),
      dj_agent_model: config.behavior.dj_agent_model.clone(),
      dj_agent_timeout_secs: Some(config.behavior.dj_agent_timeout_secs),
      dj_model: config.behavior.dj_model.clone(),
      dj_base_url: config.behavior.dj_base_url.clone(),
      dj_api_key: config.behavior.dj_api_key.clone(),
      dj_avoid_library: Some(config.behavior.dj_avoid_library),
      dj_configured: config.behavior.dj_configured,
      ..Default::default()
    })
    .unwrap();

    let mut reloaded = UserConfig::new();
    reloaded
      .load_behaviorconfig(serde_yaml::from_str(&written).unwrap())
      .unwrap();
    assert_eq!(reloaded.behavior.dj_backend, "openai_compat");
    assert_eq!(reloaded.behavior.dj_batch_size, 7);
    assert_eq!(reloaded.behavior.dj_history_period, "7d");
    assert_eq!(reloaded.behavior.dj_agent_command, vec!["codex"]);
    assert_eq!(
      reloaded.behavior.dj_agent_prompt_via.as_deref(),
      Some("arg")
    );
    assert_eq!(reloaded.behavior.dj_agent_model.as_deref(), Some("haiku"));
    assert_eq!(reloaded.behavior.dj_agent_timeout_secs, 240);
    assert_eq!(reloaded.behavior.dj_model.as_deref(), Some("some-model"));
    assert_eq!(
      reloaded.behavior.dj_base_url.as_deref(),
      Some("http://localhost:1234/v1")
    );
    assert_eq!(
      reloaded.behavior.dj_api_key.as_deref(),
      Some("sk-not-a-real-key")
    );
    assert!(reloaded.behavior.dj_avoid_library);
    assert_eq!(reloaded.behavior.dj_configured, Some(true));
  }

  /// argv[0] is the program to exec, so a blank one is no command at all.
  #[cfg(feature = "ai-dj")]
  #[test]
  fn a_dj_agent_command_without_a_program_keeps_the_default() {
    let default = super::default_dj_agent_command();

    let config = loaded("dj_agent_command:\n  - '   '\n  - -p");
    assert_eq!(
      config.behavior.dj_agent_command, default,
      "a blank argv[0] reaches spawn as a confusing ENOENT"
    );

    // A blank *later* argument is the user's business, so it still loads.
    let config = loaded("dj_agent_command:\n  - claude\n  - ''");
    assert_eq!(config.behavior.dj_agent_command, vec!["claude", ""]);
  }

  /// The reopen binding is really registered, not just declared.
  ///
  /// Several sites have to agree for a binding to work and a miss in any of them
  /// fails silently. This covers the three that are reachable without a config file
  /// on disk: the shipped default, the rebind path, and the named-action list that
  /// stops a plugin shadowing it.
  #[cfg(feature = "ai-dj")]
  #[test]
  fn the_dj_model_picker_binding_round_trips_and_is_reserved() {
    use super::{KeyBindingsString, UserConfig};
    use crate::core::input::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    assert_eq!(config.keys.dj_pick_model, Key::Ctrl('g'));

    config
      .load_keybindings(KeyBindingsString {
        dj_pick_model: Some("ctrl-x".to_string()),
        ..Default::default()
      })
      .unwrap();
    assert_eq!(config.keys.dj_pick_model, Key::Ctrl('x'));

    // Named actions are off limits to Lua `plugin_commands`, or a plugin silently
    // steals the binding.
    let mut entries = HashMap::new();
    entries.insert("steal_it".to_string(), "ctrl-x".to_string());
    config.load_plugin_commands(entries);
    assert!(config.plugin_command_keys.is_empty());
  }

  /// The exact YAML an existing install carries: every `dj_*` key present, every
  /// one of them written by an automatic `save_config` rather than typed.
  #[cfg(feature = "ai-dj")]
  fn machine_written_dj_defaults() -> &'static str {
    "
dj_backend: agent_cli
dj_agent_command:
  - claude
  - -p
dj_agent_prompt_via: stdin
dj_agent_timeout_secs: 90
dj_batch_size: 6
dj_history_period: 30d
dj_avoid_library: false
dj_model: null
dj_base_url: null
dj_api_key: null
"
  }

  #[cfg(feature = "ai-dj")]
  fn loaded(yaml: &str) -> super::UserConfig {
    use super::{BehaviorConfigString, UserConfig};
    let mut config = UserConfig::new();
    config
      .load_behaviorconfig(serde_yaml::from_str::<BehaviorConfigString>(yaml).unwrap())
      .unwrap();
    config
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn a_fresh_config_is_not_treated_as_a_configured_dj() {
    use super::UserConfig;
    assert!(!UserConfig::new().behavior.dj_is_configured());
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn an_install_carrying_only_machine_written_dj_defaults_is_still_asked_once() {
    // The test that keeps the predicate from silently excluding everybody: every
    // key below is present on disk today, so presence can never be the signal.
    // `dj_agent_prompt_via: stdin` in particular is the trap.
    let config = loaded(machine_written_dj_defaults());
    assert!(
      !config.behavior.dj_is_configured(),
      "a config written entirely by save_config has not chosen anything"
    );
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn the_completion_marker_makes_a_default_config_count_as_configured() {
    let yaml = format!("{}dj_configured: true\n", machine_written_dj_defaults());
    assert!(loaded(&yaml).behavior.dj_is_configured());
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn a_hand_edited_dj_backend_counts_as_configured_without_the_marker() {
    assert!(loaded("dj_backend: anthropic").behavior.dj_is_configured());
    assert!(loaded("dj_agent_command:\n  - agy")
      .behavior
      .dj_is_configured());
    assert!(loaded("dj_agent_model: haiku").behavior.dj_is_configured());
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn a_configured_api_key_counts_as_configured_without_the_marker() {
    assert!(loaded("dj_api_key: sk-whatever")
      .behavior
      .dj_is_configured());
    assert!(loaded("dj_base_url: http://localhost:11434/v1")
      .behavior
      .dj_is_configured());
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn tuning_the_dj_is_not_choosing_an_ai() {
    // Batch size and the history window are knobs, not a backend choice, so a user
    // who touched them is still owed the question.
    let config = loaded("dj_batch_size: 3\ndj_history_period: 7d\ndj_avoid_library: true");
    assert!(!config.behavior.dj_is_configured());
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn the_configured_marker_stays_null_until_something_sets_it() {
    use super::{UserConfig, UserConfigPaths, UserConfigString};

    // Through the real save path, because that is where the hazard is: `save_config`
    // runs from volume changes and shutdown, and if it wrote `false` it would answer
    // the picker's question on the user's behalf and pin the answer forever.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yml");
    let mut config = UserConfig::new();
    config.path_to_config = Some(UserConfigPaths {
      config_file_path: config_path.clone(),
    });

    config.save_config().unwrap();
    let on_disk = |path: &std::path::Path| -> super::BehaviorConfigString {
      let raw = std::fs::read_to_string(path).unwrap();
      serde_yaml::from_str::<UserConfigString>(&raw)
        .unwrap()
        .behavior
        .unwrap()
    };
    let saved = on_disk(&config_path);
    assert_eq!(
      saved.dj_configured, None,
      "an automatic save answers nothing"
    );
    assert_eq!(
      saved.dj_agent_prompt_via, None,
      "and leaves the delivery mode for the preset to decide"
    );

    config.behavior.dj_configured = Some(true);
    config.save_config().unwrap();
    assert_eq!(on_disk(&config_path).dj_configured, Some(true));
  }

  #[cfg(feature = "ai-dj")]
  #[test]
  fn an_invalid_dj_agent_prompt_via_keeps_the_previous_value() {
    let config = loaded("dj_agent_prompt_via: carrier pigeon");
    assert_eq!(
      config.behavior.dj_agent_prompt_via, None,
      "unset stays unset, so the preset still decides"
    );

    let config = loaded("dj_agent_prompt_via: argv");
    assert_eq!(
      config.behavior.dj_agent_prompt_via.as_deref(),
      Some("arg"),
      "the canonical form is stored, not what the user typed"
    );
  }

  #[test]
  fn plugin_commands_valid_entry_lands_in_plugin_command_keys() {
    use super::UserConfig;
    use crate::core::input::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    let mut entries = HashMap::new();
    // Ctrl+K rather than Ctrl+G: with `ai-dj` built in, Ctrl+G is `dj_pick_model`
    // and `load_plugin_commands` correctly refuses to shadow a named action, so the
    // old fixture would only have passed in the slim build.
    entries.insert("toggle_lyrics".to_string(), "ctrl-k".to_string());
    config.load_plugin_commands(entries);
    assert_eq!(
      config.plugin_command_keys.get(&Key::Ctrl('k')),
      Some(&"toggle_lyrics".to_string())
    );
  }

  #[test]
  fn plugin_commands_reserved_key_is_skipped() {
    use super::UserConfig;
    use crate::core::input::Key;
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
    use crate::core::input::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    // 'q' is the default 'back' key
    let mut entries = HashMap::new();
    entries.insert("my_cmd".to_string(), "q".to_string());
    config.load_plugin_commands(entries);
    assert!(!config.plugin_command_keys.contains_key(&Key::Char('q')));
  }

  #[test]
  fn plugin_commands_remove_from_queue_collision_is_skipped() {
    use super::UserConfig;
    use crate::core::input::Key;
    use std::collections::HashMap;

    let mut config = UserConfig::new();
    // 'x' is the default 'remove_from_queue' key; it was missing from the
    // named-action collision list, so a plugin could shadow it silently.
    let mut entries = HashMap::new();
    entries.insert("my_cmd".to_string(), "x".to_string());
    config.load_plugin_commands(entries);
    assert!(!config.plugin_command_keys.contains_key(&Key::Char('x')));
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
  fn sync_token_loads_as_user_config_and_trims_blank_to_none() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str("sync_token: ' token '\n").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.sync_token, Some("token".to_string()));

    let behavior: BehaviorConfigString = serde_yaml::from_str("sync_token: '   '\n").unwrap();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.sync_token, None);
  }

  #[test]
  fn volume_percent_loads_as_configured_runtime_default() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str("volume_percent: 150\n").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.volume_percent, Some(100));

    let behavior: BehaviorConfigString = serde_yaml::from_str("{}").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.volume_percent, None);
  }

  #[test]
  fn pane_sizes_load_as_configured_runtime_defaults() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str(
      r#"
sidebar_width_percent: 120
playbar_height_rows: 9
library_height_percent: 101
"#,
    )
    .unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.sidebar_width_percent, Some(100));
    assert_eq!(config.behavior.playbar_height_rows, Some(9));
    assert_eq!(config.behavior.library_height_percent, Some(100));

    let behavior: BehaviorConfigString = serde_yaml::from_str("{}").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.sidebar_width_percent, None);
    assert_eq!(config.behavior.playbar_height_rows, None);
    assert_eq!(config.behavior.library_height_percent, None);
  }

  #[test]
  fn radio_stations_load_as_user_config() {
    use super::{BehaviorConfigString, UserConfig};
    use crate::core::state::RadioStationConfig;

    let behavior: BehaviorConfigString = serde_yaml::from_str(
      r#"
radio_stations:
  - name: " Groove Salad "
    url: " https://ice1.somafm.com/groovesalad-128-mp3 "
  - name: Duplicate
    url: https://ice1.somafm.com/groovesalad-128-mp3
  - name: ""
    url: https://blank-name.example/dropped
"#,
    )
    .unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(
      config.behavior.radio_stations,
      vec![RadioStationConfig {
        name: "Groove Salad".to_string(),
        url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
      }]
    );
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

  #[test]
  fn example_config_loads_without_falling_back() {
    use super::{UserConfig, UserConfigString};

    // The shipped example must always be valid: it deserializes, and every
    // section applies as written instead of degrading to defaults.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/config.example.yml");
    let raw = std::fs::read_to_string(path).expect("example config must exist");
    let yml: UserConfigString =
      serde_yaml::from_str(&raw).expect("example config must deserialize");

    let mut config = UserConfig::new();
    if let Some(behavior) = yml.behavior {
      config
        .load_behaviorconfig(behavior)
        .expect("example behavior section must load");
    }
    if let Some(format) = yml.format {
      config.load_formatconfig(format);
    }
    if let Some(tables) = yml.tables {
      config.load_tablesconfig(tables);
    }

    // Spot-check that the documented values were applied, not defaulted away.
    assert_eq!(config.behavior.startup_route, "home");
    assert_eq!(config.behavior.playing_icon, "▶");
    // Every documented table resolves to its example columns (an empty Vec
    // would mean that table's spec was rejected and fell back to defaults).
    assert_eq!(config.tables.songs.len(), 5);
    assert_eq!(config.tables.album_tracks.len(), 5);
    assert_eq!(config.tables.albums.len(), 3);
    assert_eq!(config.tables.podcasts.len(), 2);
    assert_eq!(config.tables.episodes.len(), 4);
    assert_eq!(config.tables.recently_played.len(), 4);
    assert_eq!(
      config.tables.songs[2].header.as_deref(),
      Some("Band"),
      "header override from the example must survive resolution"
    );
  }

  #[test]
  fn structural_behavior_errors_degrade_to_defaults_instead_of_failing_load() {
    use super::{BehaviorConfigString, UserConfig};

    // A two-column playing icon, an empty gauge icon, an invalid sort field,
    // and an unknown playbar label key must all warn-and-fallback: the app
    // must stay launchable on a config typo.
    let yaml = r#"
playing_icon: "WW"
gauge_filled_icon: ""
default_sort_saved_albums: "dtae_added"
playbar_control_labels:
  bogus_key: "x"
  play_pause: "PLAY"
"#;
    let behavior: BehaviorConfigString = serde_yaml::from_str(yaml).unwrap();
    let mut config = UserConfig::new();
    let defaults = UserConfig::new();

    config.load_behaviorconfig(behavior).unwrap();

    assert_eq!(config.behavior.playing_icon, defaults.behavior.playing_icon);
    assert_eq!(
      config.behavior.gauge_filled_icon,
      defaults.behavior.gauge_filled_icon
    );
    assert_eq!(
      config.behavior.default_sort_saved_albums,
      defaults.behavior.default_sort_saved_albums
    );
    // The unknown key is skipped, the valid one is kept.
    assert_eq!(
      config.behavior.playbar_control_labels.get("play_pause"),
      Some(&"PLAY".to_string())
    );
    assert!(!config
      .behavior
      .playbar_control_labels
      .contains_key("bogus_key"));
  }

  #[test]
  fn playing_icon_is_width_validated_like_other_fixed_cell_icons() {
    use super::{BehaviorConfigString, UserConfig};

    let behavior: BehaviorConfigString = serde_yaml::from_str("playing_icon: \"»\"").unwrap();
    let mut config = UserConfig::new();
    config.load_behaviorconfig(behavior).unwrap();
    assert_eq!(config.behavior.playing_icon, "»");
  }

  #[test]
  fn invalid_format_template_falls_back_to_default() {
    use super::{FormatConfig, FormatConfigString, UserConfig};

    let mut config = UserConfig::new();
    config.load_formatconfig(FormatConfigString {
      window_title: Some("{bogus}".to_string()),
      playbar_status: Some("{unbalanced".to_string()),
      ..Default::default()
    });

    let defaults = FormatConfig::default();
    assert_eq!(config.format.window_title, defaults.window_title);
    assert_eq!(config.format.playbar_status, defaults.playbar_status);
  }

  #[test]
  fn invalid_table_columns_fall_back_to_default_columns() {
    use super::{ColumnSpec, TablesConfigString, UserConfig};

    let mut config = UserConfig::new();
    config.load_tablesconfig(TablesConfigString {
      songs: Some(vec![ColumnSpec {
        id: "bogus".to_string(),
        ..Default::default()
      }]),
      albums: Some(vec![ColumnSpec {
        id: "title".to_string(),
        ..Default::default()
      }]),
      ..Default::default()
    });

    // The bad table degrades to defaults (empty == built-in columns); the
    // valid table is kept.
    assert!(config.tables.songs.is_empty());
    assert_eq!(config.tables.albums.len(), 1);
  }

  #[test]
  fn column_spec_missing_id_is_recoverable_not_a_parse_error() {
    use super::{TablesConfigString, UserConfig};

    // Missing `id` must not fail YAML deserialization of the whole config;
    // it degrades that table to defaults during resolution.
    let tables: TablesConfigString = serde_yaml::from_str("songs:\n  - { width_percent: 40 }\n")
      .expect("missing id must not fail deserialization");
    let mut config = UserConfig::new();
    config.load_tablesconfig(tables);
    assert!(config.tables.songs.is_empty());
  }

  #[test]
  fn table_specs_reject_zero_and_oversubscribed_widths() {
    use super::{resolve_table_specs, ColumnSpec};

    let col = |id: &str, pct: Option<f32>, width: Option<u16>| ColumnSpec {
      id: id.to_string(),
      header: None,
      width_percent: pct,
      width,
    };

    assert!(resolve_table_specs("songs", Some(vec![col("title", Some(0.0), None)])).is_err());
    assert!(resolve_table_specs("songs", Some(vec![col("title", None, Some(0))])).is_err());
    assert!(resolve_table_specs(
      "songs",
      Some(vec![
        col("title", Some(70.0), None),
        col("artist", Some(70.0), None)
      ])
    )
    .is_err());
    // A valid subset still resolves.
    assert!(resolve_table_specs(
      "songs",
      Some(vec![
        col("title", Some(60.0), None),
        col("artist", Some(40.0), None)
      ])
    )
    .is_ok());
  }
}
