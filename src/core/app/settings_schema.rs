use super::*;

/// Settings screen category tabs
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SettingsCategory {
  #[default]
  Behavior,
  Icons,
  Keybindings,
  Theme,
}

impl SettingsCategory {
  pub fn all() -> &'static [SettingsCategory] {
    &[
      SettingsCategory::Behavior,
      SettingsCategory::Icons,
      SettingsCategory::Keybindings,
      SettingsCategory::Theme,
    ]
  }

  pub fn name(&self) -> &'static str {
    match self {
      SettingsCategory::Behavior => "Behavior",
      SettingsCategory::Icons => "Icons",
      SettingsCategory::Keybindings => "Keybindings",
      SettingsCategory::Theme => "Theme",
    }
  }

  pub fn index(&self) -> usize {
    match self {
      SettingsCategory::Behavior => 0,
      SettingsCategory::Icons => 1,
      SettingsCategory::Keybindings => 2,
      SettingsCategory::Theme => 3,
    }
  }

  pub fn from_index(index: usize) -> Self {
    match index {
      0 => SettingsCategory::Behavior,
      1 => SettingsCategory::Icons,
      2 => SettingsCategory::Keybindings,
      3 => SettingsCategory::Theme,
      _ => SettingsCategory::Behavior,
    }
  }
}

/// Represents a setting's value type
#[derive(Clone, PartialEq, Debug)]
pub enum SettingValue {
  Bool(bool),
  Number(i64),
  String(String),
  Color(String),  // Stored as "R,G,B" or color name
  Key(String),    // Key representation like "ctrl-s" or "a"
  Preset(String), // Theme preset name - cycles through available presets
  /// A value cycling through a fixed list of options: (current, all options).
  Cycle(String, &'static [&'static str]),
}

const STARTUP_ROUTE_SETTING_OPTIONS: &[&str] = &[
  "home",
  "recently_played",
  "podcasts",
  "discover",
  "artists",
  "album_list",
  "stats",
];

const PLAYLIST_TRACK_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "date_added",
  "date_added:desc",
  "artist",
  "artist:desc",
  "album",
  "album:desc",
  "duration",
  "duration:desc",
];

const SAVED_ALBUM_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "date_added",
  "date_added:desc",
  "artist",
  "artist:desc",
];

const SAVED_ARTIST_SORT_SETTING_OPTIONS: &[&str] = &["default", "name", "name:desc"];

const RECENTLY_PLAYED_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "artist",
  "artist:desc",
  "album",
  "album:desc",
];

const SIDEBAR_POSITION_SETTING_OPTIONS: &[&str] = &["left", "right", "hidden"];

const PLAYBAR_POSITION_SETTING_OPTIONS: &[&str] = &["bottom", "top"];

impl SettingValue {
  #[allow(dead_code)]
  pub fn display(&self) -> String {
    match self {
      SettingValue::Bool(v) => if *v { "On" } else { "Off" }.to_string(),
      SettingValue::Number(v) => v.to_string(),
      SettingValue::String(v) => v.clone(),
      SettingValue::Color(v) => v.clone(),
      SettingValue::Key(v) => v.clone(),
      SettingValue::Preset(v) => v.clone(),
      SettingValue::Cycle(v, _) => v.clone(),
    }
  }
}

/// Represents a single configurable setting
#[derive(Clone, Debug, PartialEq)]
pub struct SettingItem {
  pub id: String,   // e.g., "behavior.seek_milliseconds"
  pub name: String, // e.g., "Seek Duration"
  #[allow(dead_code)]
  pub description: String, // e.g., "Milliseconds to skip when seeking" (for future tooltip)
  pub value: SettingValue,
}

impl App {
  /// Load settings for the current category into settings_items
  pub fn load_settings_for_category(&mut self) {
    // Helper to convert Key to displayable string
    fn key_to_string(key: &Key) -> String {
      match key {
        Key::Char(c) => c.to_string(),
        Key::Ctrl(c) => format!("ctrl-{}", c),
        Key::Alt(c) => format!("alt-{}", c),
        Key::Enter => "enter".to_string(),
        Key::Esc => "esc".to_string(),
        Key::Backspace => "backspace".to_string(),
        Key::Delete => "del".to_string(),
        Key::Left => "left".to_string(),
        Key::Right => "right".to_string(),
        Key::Up => "up".to_string(),
        Key::Down => "down".to_string(),
        Key::PageUp => "pageup".to_string(),
        Key::PageDown => "pagedown".to_string(),
        _ => "unknown".to_string(),
      }
    }

    self.settings_items = match self.settings_category {
      SettingsCategory::Behavior => vec![
        SettingItem {
          id: "behavior.seek_milliseconds".to_string(),
          name: "Seek Duration (ms)".to_string(),
          description: "Milliseconds to skip when seeking".to_string(),
          value: SettingValue::Number(self.user_config.behavior.seek_milliseconds as i64),
        },
        SettingItem {
          id: "behavior.volume_increment".to_string(),
          name: "Volume Increment".to_string(),
          description: "Volume change per keypress (0-100)".to_string(),
          value: SettingValue::Number(self.user_config.behavior.volume_increment as i64),
        },
        SettingItem {
          id: "behavior.tick_rate_milliseconds".to_string(),
          name: "Tick Rate (ms)".to_string(),
          description: "UI refresh rate in milliseconds".to_string(),
          value: SettingValue::Number(self.user_config.behavior.tick_rate_milliseconds as i64),
        },
        SettingItem {
          id: "behavior.animation_tick_rate_milliseconds".to_string(),
          name: "Animation Tick Rate (ms)".to_string(),
          description: "Refresh rate for animation-heavy views".to_string(),
          value: SettingValue::Number(
            self.user_config.behavior.animation_tick_rate_milliseconds as i64,
          ),
        },
        SettingItem {
          id: "behavior.status_message_ttl_percent".to_string(),
          name: "Status TTL Percent".to_string(),
          description: "Scale status message duration from 10% to 1000%".to_string(),
          value: SettingValue::Number(self.user_config.behavior.status_message_ttl_percent as i64),
        },
        SettingItem {
          id: "behavior.playback_poll_seconds".to_string(),
          name: "Playback Poll Seconds".to_string(),
          description: "Seconds between regular playback refreshes".to_string(),
          value: SettingValue::Number(self.user_config.behavior.playback_poll_seconds as i64),
        },
        SettingItem {
          id: "behavior.table_scroll_padding".to_string(),
          name: "Table Scroll Padding".to_string(),
          description: "Rows reserved while scrolling tables".to_string(),
          value: SettingValue::Number(self.user_config.behavior.table_scroll_padding as i64),
        },
        SettingItem {
          id: "behavior.like_animation_frames".to_string(),
          name: "Like Animation Frames".to_string(),
          description: "Frames used by the playbar like animation".to_string(),
          value: SettingValue::Number(self.user_config.behavior.like_animation_frames as i64),
        },
        SettingItem {
          id: "behavior.enable_text_emphasis".to_string(),
          name: "Text Emphasis".to_string(),
          description: "Enable bold/italic text styling".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_text_emphasis),
        },
        SettingItem {
          id: "behavior.show_loading_indicator".to_string(),
          name: "Loading Indicator".to_string(),
          description: "Show loading status in UI".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.show_loading_indicator),
        },
        SettingItem {
          id: "behavior.enforce_wide_search_bar".to_string(),
          name: "Wide Search Bar".to_string(),
          description: "Force search bar to take full width".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enforce_wide_search_bar),
        },
        SettingItem {
          id: "behavior.group_folders_first".to_string(),
          name: "Playlist Folders First".to_string(),
          description: "List folders at the top of the Playlists tab".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.group_folders_first),
        },
        SettingItem {
          id: "behavior.disable_mouse_inputs".to_string(),
          name: "Disable Mouse Inputs".to_string(),
          description: "Disable mouse inputs for keyboard-only navigation".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.disable_mouse_inputs),
        },
        SettingItem {
          id: "behavior.set_window_title".to_string(),
          name: "Set Window Title".to_string(),
          description: "Update terminal window title with track info".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.set_window_title),
        },
        SettingItem {
          id: "behavior.enable_discord_rpc".to_string(),
          name: "Discord Rich Presence".to_string(),
          description: "Show your current track in Discord".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_discord_rpc),
        },
        SettingItem {
          id: "behavior.stop_after_current_track".to_string(),
          name: "Stop After Current Track".to_string(),
          description: "Pause playback when the current track finishes".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.stop_after_current_track),
        },
        SettingItem {
          id: "behavior.keepawake_enabled".to_string(),
          name: "Keep System Awake".to_string(),
          description: "Prevent the system from sleeping while music is playing".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.keepawake_enabled),
        },
        SettingItem {
          id: "behavior.enable_media_keys".to_string(),
          name: "Media Key Controls".to_string(),
          description:
            "Let OS media keys, headphone buttons, and remote controls (playerctl, Now Playing) control playback"
              .to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_media_keys),
        },
        SettingItem {
          id: "behavior.startup_behavior".to_string(),
          name: "Startup Behavior".to_string(),
          description: "Playback state when spotatui starts. Continue resumes your last session (including a saved non-Spotify track) exactly as it was; Play always starts; Pause always pauses.".to_string(),
          value: SettingValue::Cycle(
            self
              .user_config
              .behavior
              .startup_behavior
              .name()
              .to_string(),
            crate::core::user_config::StartupBehavior::options(),
          ),
        },
        SettingItem {
          id: "behavior.startup_route".to_string(),
          name: "Startup Route".to_string(),
          description: "Screen shown when spotatui starts".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.startup_route.clone(),
            STARTUP_ROUTE_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_playlist_tracks".to_string(),
          name: "Playlist Track Sort".to_string(),
          description: "Default sort for playlist track tables".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_playlist_tracks.clone(),
            PLAYLIST_TRACK_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_saved_albums".to_string(),
          name: "Saved Album Sort".to_string(),
          description: "Default sort for saved albums".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_saved_albums.clone(),
            SAVED_ALBUM_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_saved_artists".to_string(),
          name: "Saved Artist Sort".to_string(),
          description: "Default sort for saved artists".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_saved_artists.clone(),
            SAVED_ARTIST_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.default_sort_recently_played".to_string(),
          name: "Recently Played Sort".to_string(),
          description: "Default sort for recently played tracks".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.default_sort_recently_played.clone(),
            RECENTLY_PLAYED_SORT_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.sidebar_position".to_string(),
          name: "Sidebar Position".to_string(),
          description: "Place the sidebar left, right, or hide it".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.sidebar_position.clone(),
            SIDEBAR_POSITION_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.playbar_position".to_string(),
          name: "Playbar Position".to_string(),
          description: "Place the playbar at the bottom or top".to_string(),
          value: SettingValue::Cycle(
            self.user_config.behavior.playbar_position.clone(),
            PLAYBAR_POSITION_SETTING_OPTIONS,
          ),
        },
        SettingItem {
          id: "behavior.small_terminal_width".to_string(),
          name: "Small Terminal Width".to_string(),
          description: "Width below which compact layout is used".to_string(),
          value: SettingValue::Number(self.user_config.behavior.small_terminal_width as i64),
        },
        SettingItem {
          id: "behavior.small_terminal_height".to_string(),
          name: "Small Terminal Height".to_string(),
          description: "Height below which compact margins are used".to_string(),
          value: SettingValue::Number(self.user_config.behavior.small_terminal_height as i64),
        },
        SettingItem {
          id: "behavior.enable_announcements".to_string(),
          name: "Remote Announcements".to_string(),
          description: "Show one-time announcements from remote JSON feed".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_announcements),
        },
        SettingItem {
          id: "behavior.enable_monthly_recap_prompt".to_string(),
          name: "Monthly Recap Prompt".to_string(),
          description: "Show a popup once a month when your listening recap is ready".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_monthly_recap_prompt),
        },
        SettingItem {
          id: "behavior.pin_community_playlist".to_string(),
          name: "Community Playlist Pin".to_string(),
          description: "Pin the spotatui community playlist to the top of your Spotify playlists"
            .to_string(),
          value: SettingValue::Bool(self.user_config.behavior.pin_community_playlist),
        },
        #[cfg(feature = "telemetry")]
        SettingItem {
          id: "behavior.enable_global_song_count".to_string(),
          name: "Global Song Counter".to_string(),
          description: "Contribute to the anonymous worldwide song counter. No personal info, song names, or history are sent; only a simple increment when a new song starts.".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.enable_global_song_count),
        },
        #[cfg(feature = "self-update")]
        SettingItem {
          id: "behavior.disable_auto_update".to_string(),
          name: "Disable Auto-Update".to_string(),
          description: "Skip the automatic update check on startup. Use the 'spotatui update' command to update manually.".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.disable_auto_update),
        },
        #[cfg(feature = "self-update")]
        SettingItem {
          id: "behavior.auto_update_delay".to_string(),
          name: "Auto-Update Delay".to_string(),
          description: "How long to wait before installing an available update. Use '0' for immediate, or e.g. '10m', '2h', '7d'. Only applies when auto-update is enabled.".to_string(),
          value: SettingValue::String(self.user_config.behavior.auto_update_delay.clone()),
        },
        SettingItem {
          id: "behavior.announcement_feed_url".to_string(),
          name: "Announcements Feed URL".to_string(),
          description: "Remote JSON feed URL (HTTPS)".to_string(),
          value: SettingValue::String(
            self
              .user_config
              .behavior
              .announcement_feed_url
              .clone()
              .unwrap_or_default(),
          ),
        },
        SettingItem {
          id: "behavior.sync_token".to_string(),
          name: "Sync Token".to_string(),
          description: "API token from spotatui.com to sync listening history".to_string(),
          value: SettingValue::String(
            self
              .user_config
              .behavior
              .sync_token
              .clone()
              .unwrap_or_default(),
          ),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "behavior.draw_cover_art".to_string(),
          name: "Draw Cover Art".to_string(),
          description: "Enable rendering song/episode cover art".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.draw_cover_art),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "behavior.draw_cover_art_forced".to_string(),
          name: "Force Draw Cover Art".to_string(),
          description: "Force rendering of cover art despite terminal support".to_string(),
          value: SettingValue::Bool(self.user_config.behavior.draw_cover_art_forced),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "behavior.playbar_cover_art_size_percent".to_string(),
          name: "Cover Art Size".to_string(),
          description: "Playbar cover art size as a percentage (25-200)".to_string(),
          value: SettingValue::Number(
            self.user_config.behavior.playbar_cover_art_size_percent as i64,
          ),
        },
      ],
      SettingsCategory::Icons => vec![
        SettingItem {
          id: "behavior.liked_icon".to_string(),
          name: "Liked Icon".to_string(),
          description: "Icon for liked songs".to_string(),
          value: SettingValue::String(self.user_config.behavior.liked_icon.clone()),
        },
        SettingItem {
          id: "behavior.shuffle_icon".to_string(),
          name: "Shuffle Icon".to_string(),
          description: "Icon for shuffle mode".to_string(),
          value: SettingValue::String(self.user_config.behavior.shuffle_icon.clone()),
        },
        SettingItem {
          id: "behavior.playing_icon".to_string(),
          name: "Playing Icon".to_string(),
          description: "Icon for playing state".to_string(),
          value: SettingValue::String(self.user_config.behavior.playing_icon.clone()),
        },
        SettingItem {
          id: "behavior.paused_icon".to_string(),
          name: "Paused Icon".to_string(),
          description: "Icon for paused state".to_string(),
          value: SettingValue::String(self.user_config.behavior.paused_icon.clone()),
        },
        SettingItem {
          id: "behavior.gauge_filled_icon".to_string(),
          name: "Gauge Filled Icon".to_string(),
          description: "Single-cell icon for filled gauge segments".to_string(),
          value: SettingValue::String(self.user_config.behavior.gauge_filled_icon.clone()),
        },
        SettingItem {
          id: "behavior.gauge_unfilled_icon".to_string(),
          name: "Gauge Empty Icon".to_string(),
          description: "Single-cell icon for empty gauge segments".to_string(),
          value: SettingValue::String(self.user_config.behavior.gauge_unfilled_icon.clone()),
        },
        SettingItem {
          id: "behavior.active_source_icon".to_string(),
          name: "Active Source Icon".to_string(),
          description: "Icon for the active playback source".to_string(),
          value: SettingValue::String(self.user_config.behavior.active_source_icon.clone()),
        },
        SettingItem {
          id: "behavior.episode_played_icon".to_string(),
          name: "Episode Played Icon".to_string(),
          description: "Single-cell icon for fully played episodes".to_string(),
          value: SettingValue::String(self.user_config.behavior.episode_played_icon.clone()),
        },
        SettingItem {
          id: "behavior.sort_ascending_icon".to_string(),
          name: "Sort Ascending Icon".to_string(),
          description: "Single-cell icon for ascending sort".to_string(),
          value: SettingValue::String(self.user_config.behavior.sort_ascending_icon.clone()),
        },
        SettingItem {
          id: "behavior.sort_descending_icon".to_string(),
          name: "Sort Descending Icon".to_string(),
          description: "Single-cell icon for descending sort".to_string(),
          value: SettingValue::String(self.user_config.behavior.sort_descending_icon.clone()),
        },
        SettingItem {
          id: "behavior.list_highlight_icon".to_string(),
          name: "List Highlight Icon".to_string(),
          description: "Icon shown next to highlighted list rows".to_string(),
          value: SettingValue::String(self.user_config.behavior.list_highlight_icon.clone()),
        },
      ],
      SettingsCategory::Keybindings => vec![
        SettingItem {
          id: "keys.back".to_string(),
          name: "Back".to_string(),
          description: "Go back / quit".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.back)),
        },
        SettingItem {
          id: "keys.move_up".to_string(),
          name: "Move Up".to_string(),
          description: "Move selection up".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_up)),
        },
        SettingItem {
          id: "keys.move_down".to_string(),
          name: "Move Down".to_string(),
          description: "Move selection down".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_down)),
        },
        SettingItem {
          id: "keys.move_left".to_string(),
          name: "Move Left".to_string(),
          description: "Move selection left".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_left)),
        },
        SettingItem {
          id: "keys.move_right".to_string(),
          name: "Move Right".to_string(),
          description: "Move selection right".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.move_right)),
        },
        SettingItem {
          id: "keys.next_page".to_string(),
          name: "Next Page".to_string(),
          description: "Navigate to next page".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.next_page)),
        },
        SettingItem {
          id: "keys.previous_page".to_string(),
          name: "Previous Page".to_string(),
          description: "Navigate to previous page".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.previous_page)),
        },
        SettingItem {
          id: "keys.toggle_playback".to_string(),
          name: "Toggle Playback".to_string(),
          description: "Play/pause".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.toggle_playback)),
        },
        SettingItem {
          id: "keys.seek_backwards".to_string(),
          name: "Seek Backwards".to_string(),
          description: "Seek backwards in track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.seek_backwards)),
        },
        SettingItem {
          id: "keys.seek_forwards".to_string(),
          name: "Seek Forwards".to_string(),
          description: "Seek forwards in track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.seek_forwards)),
        },
        SettingItem {
          id: "keys.next_track".to_string(),
          name: "Next Track".to_string(),
          description: "Skip to next track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.next_track)),
        },
        SettingItem {
          id: "keys.previous_track".to_string(),
          name: "Previous Track".to_string(),
          description: "Go to previous track".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.previous_track)),
        },
        SettingItem {
          id: "keys.force_previous_track".to_string(),
          name: "Force Previous Track".to_string(),
          description: "Always skip to the previous track (ignoring playback position)".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.force_previous_track)),
        },
        SettingItem {
          id: "keys.shuffle".to_string(),
          name: "Shuffle".to_string(),
          description: "Toggle shuffle mode".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.shuffle)),
        },
        SettingItem {
          id: "keys.repeat".to_string(),
          name: "Repeat".to_string(),
          description: "Cycle repeat mode".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.repeat)),
        },
        SettingItem {
          id: "keys.search".to_string(),
          name: "Search".to_string(),
          description: "Open search".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.search)),
        },
        SettingItem {
          id: "keys.help".to_string(),
          name: "Help".to_string(),
          description: "Show help menu".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.help)),
        },
        SettingItem {
          id: "keys.open_settings".to_string(),
          name: "Open Settings".to_string(),
          description: "Open settings menu".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.open_settings)),
        },
        SettingItem {
          id: "keys.save_settings".to_string(),
          name: "Save Settings".to_string(),
          description: "Save settings to file".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.save_settings)),
        },
        SettingItem {
          id: "keys.jump_to_album".to_string(),
          name: "Jump to Album".to_string(),
          description: "Jump to currently playing album".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.jump_to_album)),
        },
        SettingItem {
          id: "keys.jump_to_artist_album".to_string(),
          name: "Jump to Artist".to_string(),
          description: "Jump to artist's albums".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.jump_to_artist_album)),
        },
        SettingItem {
          id: "keys.jump_to_context".to_string(),
          name: "Jump to Context".to_string(),
          description: "Jump to current playback context".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.jump_to_context)),
        },
        SettingItem {
          id: "keys.manage_devices".to_string(),
          name: "Manage Devices".to_string(),
          description: "Open device selection".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.manage_devices)),
        },
        SettingItem {
          id: "keys.decrease_volume".to_string(),
          name: "Decrease Volume".to_string(),
          description: "Decrease playback volume".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.decrease_volume)),
        },
        SettingItem {
          id: "keys.increase_volume".to_string(),
          name: "Increase Volume".to_string(),
          description: "Increase playback volume".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.increase_volume)),
        },
        SettingItem {
          id: "keys.add_item_to_queue".to_string(),
          name: "Add to Queue".to_string(),
          description: "Add selected item to queue".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.add_item_to_queue)),
        },
        SettingItem {
          id: "keys.show_queue".to_string(),
          name: "Show Queue".to_string(),
          description: "Show playback queue".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.show_queue)),
        },
        SettingItem {
          id: "keys.remove_from_queue".to_string(),
          name: "Remove from Queue".to_string(),
          description: "Remove the selected track from the queue".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.remove_from_queue)),
        },
        SettingItem {
          id: "keys.like_track".to_string(),
          name: "Like Track".to_string(),
          description: "Toggle saved state for the currently playing track or episode"
            .to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.like_track)),
        },
        SettingItem {
          id: "keys.generate_recap".to_string(),
          name: "Generate Listening Recap".to_string(),
          description:
            "Generate and open the listening recap HTML card (uses the selected period on the Stats screen, 30 days elsewhere)"
              .to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.generate_recap)),
        },
        SettingItem {
          id: "keys.copy_song_url".to_string(),
          name: "Copy Song URL".to_string(),
          description: "Copy current song URL to clipboard".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.copy_song_url)),
        },
        SettingItem {
          id: "keys.copy_album_url".to_string(),
          name: "Copy Album URL".to_string(),
          description: "Copy current album URL to clipboard".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.copy_album_url)),
        },
        SettingItem {
          id: "keys.audio_analysis".to_string(),
          name: "Audio Analysis".to_string(),
          description: "Open audio analysis view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.audio_analysis)),
        },
        SettingItem {
          id: "keys.lyrics_view".to_string(),
          name: "Lyrics View".to_string(),
          description: "Open lyrics view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.lyrics_view)),
        },
        SettingItem {
          id: "keys.miniplayer_view".to_string(),
          name: "Miniplayer View".to_string(),
          description: "Toggle full-screen playbar view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.miniplayer_view)),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "keys.cover_art_view".to_string(),
          name: "Cover Art View".to_string(),
          description: "Open full-screen cover art view".to_string(),
          value: SettingValue::Key(key_to_string(&self.user_config.keys.cover_art_view)),
        },
      ],
      SettingsCategory::Theme => {
        // The user's own colors, not the live theme: while an album-derived
        // theme is applied the live theme is a blend, and rows built from it
        // would be written back into custom_theme on save.
        let user_theme = self.user_theme();
        vec![
          SettingItem {
            id: "theme.preset".to_string(),
            name: "Theme Preset".to_string(),
            description: "Choose a preset theme or customize below".to_string(),
            value: SettingValue::Preset(self.user_config.current_preset.name().to_string()),
          },
          SettingItem {
            id: "theme.active".to_string(),
            name: "Active Color".to_string(),
            description: "Color for active elements".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.active)),
          },
          SettingItem {
            id: "theme.banner".to_string(),
            name: "Banner Color".to_string(),
            description: "Color for banner text".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.banner)),
          },
          SettingItem {
            id: "behavior.banner_gradient".to_string(),
            name: "Banner Gradient".to_string(),
            description: "Animated RGB gradient on the home banner; off uses the Banner Color"
              .to_string(),
            value: SettingValue::Bool(self.user_config.behavior.banner_gradient),
          },
          #[cfg(feature = "art-decode")]
          SettingItem {
            id: "behavior.cover_art_theme".to_string(),
            name: "Adaptive Theme".to_string(),
            description: "Recolor UI accents from the current cover art, fading on track change"
              .to_string(),
            value: SettingValue::Bool(self.user_config.behavior.cover_art_theme),
          },
          SettingItem {
            id: "theme.hint".to_string(),
            name: "Hint Color".to_string(),
            description: "Color for hints".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.hint)),
          },
          SettingItem {
            id: "theme.hovered".to_string(),
            name: "Hovered Color".to_string(),
            description: "Color for hovered elements".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.hovered)),
          },
          SettingItem {
            id: "theme.selected".to_string(),
            name: "Selected Color".to_string(),
            description: "Color for selected items".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.selected)),
          },
          SettingItem {
            id: "theme.inactive".to_string(),
            name: "Inactive Color".to_string(),
            description: "Color for inactive elements".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.inactive)),
          },
          SettingItem {
            id: "theme.text".to_string(),
            name: "Text Color".to_string(),
            description: "Default text color".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.text)),
          },
          SettingItem {
            id: "theme.error_text".to_string(),
            name: "Error Text Color".to_string(),
            description: "Color for error messages".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.error_text)),
          },
          SettingItem {
            id: "theme.error_border".to_string(),
            name: "Error Border Color".to_string(),
            description: "Border color for error messages".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.error_border)),
          },
          SettingItem {
            id: "theme.playbar_background".to_string(),
            name: "Playbar Background".to_string(),
            description: "Background color for playbar".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_background)),
          },
          SettingItem {
            id: "theme.playbar_progress".to_string(),
            name: "Playbar Progress".to_string(),
            description: "Color for playbar progress".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_progress)),
          },
          SettingItem {
            id: "theme.playbar_progress_text".to_string(),
            name: "Playbar Progress Text".to_string(),
            description: "Color for playbar progress text".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_progress_text)),
          },
          SettingItem {
            id: "theme.playbar_text".to_string(),
            name: "Playbar Text".to_string(),
            description: "Color for playbar text".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_text)),
          },
          SettingItem {
            id: "theme.highlighted_lyrics".to_string(),
            name: "Lyrics Highlight".to_string(),
            description: "Color for current lyrics line".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.highlighted_lyrics)),
          },
          SettingItem {
            id: "theme.background".to_string(),
            name: "Background".to_string(),
            description: "Color for the background".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.background)),
          },
          SettingItem {
            id: "theme.header".to_string(),
            name: "Header".to_string(),
            description: "Color for the header".to_string(),
            value: SettingValue::Color(color_to_string(user_theme.header)),
          },
        ]
      }
    };
    self.settings_selected_index = 0;
    self.settings_saved_items = self.settings_items.clone();
    self.settings_unsaved_prompt_visible = false;
    self.settings_unsaved_prompt_save_selected = true;
  }

  /// Enter the Settings screen on a fresh, unfiltered view of the current
  /// category. Every front door (keybinding, sidebar click, Lua `navigate`)
  /// goes through here so none of them can carry a stale filter in.
  pub fn open_settings_screen(&mut self) {
    self.clear_settings_filter();
    self.load_settings_for_category();
    self.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
  }

  /// Open the settings row filter on a fresh, empty query.
  pub fn begin_settings_filter(&mut self) {
    self.settings_filter.clear();
    self.settings_filter_editing = true;
  }

  /// Stop typing into the settings row filter, leaving it applied.
  pub fn apply_settings_filter(&mut self) {
    self.settings_filter_editing = false;
  }

  /// Drop the settings row filter entirely.
  pub fn clear_settings_filter(&mut self) {
    self.settings_filter.clear();
    self.settings_filter_editing = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn startup_screen_setting_cycle_offers_every_startup_route() {
    // The Settings cycle list and RouteId::STARTUP_OPTIONS are coupled only
    // by convention; a route added to one but not the other silently becomes
    // unselectable in-app (#443, which is how Stats went missing).
    let expected: Vec<&str> = RouteId::STARTUP_OPTIONS
      .iter()
      .map(|route| route.to_config_str())
      .collect();
    assert_eq!(STARTUP_ROUTE_SETTING_OPTIONS, expected.as_slice());
  }
}
