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

  #[cfg(any(feature = "tui", test))]
  pub fn localized_name(&self, strings: &crate::core::i18n::Strings) -> &'static str {
    match self {
      SettingsCategory::Behavior => strings.settings.category_behavior,
      SettingsCategory::Icons => strings.settings.category_icons,
      SettingsCategory::Keybindings => strings.settings.category_keybindings,
      SettingsCategory::Theme => strings.settings.category_theme,
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
#[derive(Clone, PartialEq)]
pub enum SettingValue {
  Bool(bool),
  Number(i64),
  String(String),
  Color(String),  // Stored as "R,G,B" or color name
  Key(String),    // Key representation like "ctrl-s" or "a"
  Preset(String), // Theme preset name - cycles through available presets
  /// A value cycling through a fixed list of options: (current, all options).
  Cycle(String, &'static [&'static str]),
  /// A password: shown masked, edited from empty.
  #[cfg_attr(not(feature = "subsonic"), allow(dead_code))]
  Secret(String),
}

/// Derived except for `Secret`, so a debug print never shows a password.
impl std::fmt::Debug for SettingValue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SettingValue::Bool(v) => f.debug_tuple("Bool").field(v).finish(),
      SettingValue::Number(v) => f.debug_tuple("Number").field(v).finish(),
      SettingValue::String(v) => f.debug_tuple("String").field(v).finish(),
      SettingValue::Color(v) => f.debug_tuple("Color").field(v).finish(),
      SettingValue::Key(v) => f.debug_tuple("Key").field(v).finish(),
      SettingValue::Preset(v) => f.debug_tuple("Preset").field(v).finish(),
      SettingValue::Cycle(v, options) => f.debug_tuple("Cycle").field(v).field(options).finish(),
      SettingValue::Secret(_) => f.write_str("Secret(<hidden>)"),
    }
  }
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

/// The startup routes that open populated without a Spotify session.
const STARTUP_ROUTE_SESSION_FREE_OPTIONS: &[&str] = &["home", "stats"];

/// What a settings row needs; an unmet key row stays with a hint, any other unmet row is hidden.
pub(crate) fn setting_requirement(id: &str) -> Requirement {
  match id {
    "behavior.playback_poll_seconds"
    | "behavior.default_sort_saved_albums"
    | "behavior.default_sort_saved_artists"
    | "behavior.default_sort_recently_played"
    | "behavior.pin_community_playlist"
    | "behavior.liked_icon"
    | "behavior.episode_played_icon"
    | "behavior.enable_streaming"
    | "behavior.streaming_device_name"
    | "behavior.streaming_bitrate"
    | "behavior.streaming_audio_cache"
    | "behavior.streaming_autoplay"
    | "behavior.playback_engine"
    | "behavior.soloist_path"
    | "behavior.soloist_api_key"
    | "behavior.soloist_audio_quality"
    | "keys.jump_to_album"
    | "keys.jump_to_artist_album"
    | "keys.jump_to_context"
    | "keys.listening_party"
    | "keys.copy_song_url"
    | "keys.copy_album_url" => Requirement::SpotifySession,
    // The like key favorites the station under Radio.
    "keys.like_track" => Requirement::AnyOf(&[
      Requirement::Capability(Capability::Like),
      Requirement::Source(Source::Radio),
    ]),
    "keys.search" => Requirement::Capability(Capability::Search),
    _ => Requirement::None,
  }
}

/// The one button a settings row can carry, beside its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingButton {
  DownloadSoloist,
  GetSoloistKey,
}

pub(crate) fn setting_button(id: &str) -> Option<SettingButton> {
  match id {
    "behavior.soloist_path" if cfg!(feature = "soloist") => Some(SettingButton::DownloadSoloist),
    "behavior.soloist_api_key" if cfg!(feature = "soloist") => Some(SettingButton::GetSoloistKey),
    _ => None,
  }
}

impl App {
  /// A row's button label, and whether its work runs (the button is then off).
  pub(crate) fn setting_button_label(&self, id: &str) -> Option<(&'static str, bool)> {
    let strings = &self.strings().settings;
    setting_button(id).map(|button| match button {
      SettingButton::DownloadSoloist if self.soloist_download_runs() => {
        (strings.soloist_downloading_button, true)
      }
      SettingButton::DownloadSoloist => (strings.soloist_download_button, false),
      SettingButton::GetSoloistKey => (strings.soloist_get_key_button, false),
    })
  }

  pub(crate) fn press_setting_button(&mut self, id: &str) {
    match setting_button(id) {
      Some(SettingButton::DownloadSoloist) => {
        #[cfg(feature = "soloist")]
        self.download_soloist();
      }
      Some(SettingButton::GetSoloistKey) => {
        self.open_external_url("https://developer.spotify.com/dashboard/soloist");
      }
      None => {}
    }
  }

  fn soloist_download_runs(&self) -> bool {
    #[cfg(feature = "soloist")]
    {
      self.soloist_downloading
    }
    #[cfg(not(feature = "soloist"))]
    {
      false
    }
  }
}

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
  "release_date",
  "release_date:desc",
];

const SAVED_ALBUM_SORT_SETTING_OPTIONS: &[&str] = &[
  "default",
  "name",
  "name:desc",
  "date_added",
  "date_added:desc",
  "artist",
  "artist:desc",
  "release_date",
  "release_date:desc",
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
      SettingValue::Secret(v) if v.is_empty() => String::new(),
      SettingValue::Secret(_) => "••••••••".to_string(),
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
  /// Relabel both snapshots without changing staged edits or their dirty state.
  pub(crate) fn refresh_settings_labels(&mut self) {
    let strings = &self.strings().settings;
    for item in self
      .settings_items
      .iter_mut()
      .chain(self.settings_saved_items.iter_mut())
    {
      if let Some((name, description)) = strings.row_text(&item.id) {
        item.name = name.to_string();
        item.description = description.to_string();
      }
    }
  }

  /// The Behavior rows, apart from the state of the Settings screen.
  pub(crate) fn behavior_setting_items(&self) -> Vec<SettingItem> {
    let strings = &self.strings().settings;
    vec![
      SettingItem {
        id: "behavior.language".to_string(),
        name: strings.behavior_language_name.to_string(),
        description: strings.behavior_language_description.to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.language.as_str().to_string(),
          crate::core::i18n::Language::OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.seek_milliseconds".to_string(),
        name: strings.behavior_seek_milliseconds_name.to_string(),
        description: strings.behavior_seek_milliseconds_description.to_string(),
        value: SettingValue::Number(self.user_config.behavior.seek_milliseconds as i64),
      },
      SettingItem {
        id: "behavior.volume_increment".to_string(),
        name: strings.behavior_volume_increment_name.to_string(),
        description: strings.behavior_volume_increment_description.to_string(),
        value: SettingValue::Number(self.user_config.behavior.volume_increment as i64),
      },
      SettingItem {
        id: "behavior.tick_rate_milliseconds".to_string(),
        name: strings.behavior_tick_rate_milliseconds_name.to_string(),
        description: strings
          .behavior_tick_rate_milliseconds_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.tick_rate_milliseconds as i64),
      },
      SettingItem {
        id: "behavior.animation_tick_rate_milliseconds".to_string(),
        name: strings
          .behavior_animation_tick_rate_milliseconds_name
          .to_string(),
        description: strings
          .behavior_animation_tick_rate_milliseconds_description
          .to_string(),
        value: SettingValue::Number(
          self.user_config.behavior.animation_tick_rate_milliseconds as i64,
        ),
      },
      SettingItem {
        id: "behavior.status_message_ttl_percent".to_string(),
        name: strings.behavior_status_message_ttl_percent_name.to_string(),
        description: strings
          .behavior_status_message_ttl_percent_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.status_message_ttl_percent as i64),
      },
      #[cfg(feature = "qobuz")]
      SettingItem {
        id: "behavior.qobuz_quality".to_string(),
        name: strings.behavior_qobuz_quality_name.to_string(),
        description: strings.behavior_qobuz_quality_description.to_string(),
        value: SettingValue::Cycle(
          crate::core::user_config::qobuz_quality_label(self.user_config.behavior.qobuz_quality)
            .to_string(),
          crate::core::user_config::QOBUZ_QUALITY_LABELS,
        ),
      },
      #[cfg(feature = "subsonic")]
      SettingItem {
        id: "behavior.subsonic_url".to_string(),
        name: strings.behavior_subsonic_url_name.to_string(),
        description: strings.behavior_subsonic_url_description.to_string(),
        value: SettingValue::String(
          self
            .user_config
            .behavior
            .subsonic_url
            .clone()
            .unwrap_or_default(),
        ),
      },
      #[cfg(feature = "subsonic")]
      SettingItem {
        id: "behavior.subsonic_username".to_string(),
        name: strings.behavior_subsonic_username_name.to_string(),
        description: strings.behavior_subsonic_username_description.to_string(),
        value: SettingValue::String(
          self
            .user_config
            .behavior
            .subsonic_username
            .clone()
            .unwrap_or_default(),
        ),
      },
      #[cfg(feature = "subsonic")]
      SettingItem {
        id: "behavior.subsonic_password".to_string(),
        name: strings.behavior_subsonic_password_name.to_string(),
        description: strings.behavior_subsonic_password_description.to_string(),
        value: SettingValue::Secret(
          self
            .user_config
            .behavior
            .subsonic_password
            .clone()
            .unwrap_or_default(),
        ),
      },
      #[cfg(feature = "local-files")]
      SettingItem {
        id: "behavior.local_music_path".to_string(),
        name: strings.behavior_local_music_path_name.to_string(),
        description: strings.behavior_local_music_path_description.to_string(),
        value: SettingValue::String(
          self
            .user_config
            .behavior
            .local_music_path
            .clone()
            .unwrap_or_default(),
        ),
      },
      #[cfg(feature = "youtube")]
      SettingItem {
        id: "behavior.ytdlp_path".to_string(),
        name: strings.behavior_ytdlp_path_name.to_string(),
        description: strings.behavior_ytdlp_path_description.to_string(),
        value: SettingValue::String(
          self
            .user_config
            .behavior
            .ytdlp_path
            .clone()
            .unwrap_or_default(),
        ),
      },
      SettingItem {
        id: "behavior.playback_poll_seconds".to_string(),
        name: strings.behavior_playback_poll_seconds_name.to_string(),
        description: strings
          .behavior_playback_poll_seconds_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.playback_poll_seconds as i64),
      },
      SettingItem {
        id: "behavior.table_scroll_padding".to_string(),
        name: strings.behavior_table_scroll_padding_name.to_string(),
        description: strings
          .behavior_table_scroll_padding_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.table_scroll_padding as i64),
      },
      SettingItem {
        id: "behavior.like_animation_frames".to_string(),
        name: strings.behavior_like_animation_frames_name.to_string(),
        description: strings
          .behavior_like_animation_frames_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.like_animation_frames as i64),
      },
      SettingItem {
        id: "behavior.enable_text_emphasis".to_string(),
        name: strings.behavior_enable_text_emphasis_name.to_string(),
        description: strings
          .behavior_enable_text_emphasis_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_text_emphasis),
      },
      SettingItem {
        id: "behavior.show_loading_indicator".to_string(),
        name: strings.behavior_show_loading_indicator_name.to_string(),
        description: strings
          .behavior_show_loading_indicator_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.show_loading_indicator),
      },
      SettingItem {
        id: "behavior.enforce_wide_search_bar".to_string(),
        name: strings.behavior_enforce_wide_search_bar_name.to_string(),
        description: strings
          .behavior_enforce_wide_search_bar_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enforce_wide_search_bar),
      },
      SettingItem {
        id: "behavior.group_folders_first".to_string(),
        name: strings.behavior_group_folders_first_name.to_string(),
        description: strings.behavior_group_folders_first_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.group_folders_first),
      },
      SettingItem {
        id: "behavior.disable_mouse_inputs".to_string(),
        name: strings.behavior_disable_mouse_inputs_name.to_string(),
        description: strings
          .behavior_disable_mouse_inputs_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.disable_mouse_inputs),
      },
      SettingItem {
        id: "behavior.set_window_title".to_string(),
        name: strings.behavior_set_window_title_name.to_string(),
        description: strings.behavior_set_window_title_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.set_window_title),
      },
      SettingItem {
        id: "behavior.enable_discord_rpc".to_string(),
        name: strings.behavior_enable_discord_rpc_name.to_string(),
        description: strings.behavior_enable_discord_rpc_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_discord_rpc),
      },
      SettingItem {
        id: "behavior.stop_after_current_track".to_string(),
        name: strings.behavior_stop_after_current_track_name.to_string(),
        description: strings
          .behavior_stop_after_current_track_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.stop_after_current_track),
      },
      SettingItem {
        id: "behavior.keepawake_enabled".to_string(),
        name: strings.behavior_keepawake_enabled_name.to_string(),
        description: strings.behavior_keepawake_enabled_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.keepawake_enabled),
      },
      SettingItem {
        id: "behavior.enable_media_keys".to_string(),
        name: strings.behavior_enable_media_keys_name.to_string(),
        description: strings.behavior_enable_media_keys_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_media_keys),
      },
      #[cfg(feature = "streaming")]
      SettingItem {
        id: "behavior.enable_streaming".to_string(),
        name: strings.behavior_enable_streaming_name.to_string(),
        description: strings.behavior_enable_streaming_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_streaming),
      },
      #[cfg(feature = "streaming")]
      SettingItem {
        id: "behavior.streaming_device_name".to_string(),
        name: strings.behavior_streaming_device_name_name.to_string(),
        description: strings
          .behavior_streaming_device_name_description
          .to_string(),
        value: SettingValue::String(self.user_config.behavior.streaming_device_name.clone()),
      },
      #[cfg(feature = "streaming")]
      SettingItem {
        id: "behavior.streaming_bitrate".to_string(),
        name: strings.behavior_streaming_bitrate_name.to_string(),
        description: strings.behavior_streaming_bitrate_description.to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.streaming_bitrate.to_string(),
          crate::core::user_config::STREAMING_BITRATES,
        ),
      },
      #[cfg(feature = "streaming")]
      SettingItem {
        id: "behavior.streaming_audio_cache".to_string(),
        name: strings.behavior_streaming_audio_cache_name.to_string(),
        description: strings
          .behavior_streaming_audio_cache_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.streaming_audio_cache),
      },
      #[cfg(feature = "streaming")]
      SettingItem {
        id: "behavior.streaming_autoplay".to_string(),
        name: strings.behavior_streaming_autoplay_name.to_string(),
        description: strings.behavior_streaming_autoplay_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.streaming_autoplay),
      },
      // Soloist has no build for Intel Macs.
      #[cfg(all(
        feature = "soloist",
        not(all(target_os = "macos", target_arch = "x86_64"))
      ))]
      SettingItem {
        id: "behavior.playback_engine".to_string(),
        name: strings.behavior_playback_engine_name.to_string(),
        description: strings.behavior_playback_engine_description.to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.playback_engine.name().to_string(),
          crate::core::user_config::PlaybackEngine::options(),
        ),
      },
      #[cfg(all(
        feature = "soloist",
        not(all(target_os = "macos", target_arch = "x86_64"))
      ))]
      SettingItem {
        id: "behavior.soloist_path".to_string(),
        name: strings.behavior_soloist_path_name.to_string(),
        description: strings.behavior_soloist_path_description.to_string(),
        value: SettingValue::String(
          self
            .user_config
            .behavior
            .soloist_path
            .clone()
            .unwrap_or_default(),
        ),
      },
      #[cfg(all(
        feature = "soloist",
        not(all(target_os = "macos", target_arch = "x86_64"))
      ))]
      SettingItem {
        id: "behavior.soloist_api_key".to_string(),
        name: strings.behavior_soloist_api_key_name.to_string(),
        description: strings.behavior_soloist_api_key_description.to_string(),
        value: SettingValue::Secret(self.saved_soloist_key()),
      },
      #[cfg(all(
        feature = "soloist",
        not(all(target_os = "macos", target_arch = "x86_64"))
      ))]
      SettingItem {
        id: "behavior.soloist_audio_quality".to_string(),
        name: strings.behavior_soloist_audio_quality_name.to_string(),
        description: strings
          .behavior_soloist_audio_quality_description
          .to_string(),
        value: SettingValue::Cycle(
          self
            .user_config
            .behavior
            .soloist_audio_quality
            .name()
            .to_string(),
          crate::core::user_config::SoloistQuality::options(),
        ),
      },
      SettingItem {
        id: "behavior.startup_behavior".to_string(),
        name: strings.behavior_startup_behavior_name.to_string(),
        description: strings.behavior_startup_behavior_description.to_string(),
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
      // The desktop app, which shares this config, captures only on these.
      #[cfg(any(target_os = "linux", target_os = "windows"))]
      SettingItem {
        id: "behavior.player_bar_visualizer".to_string(),
        name: strings.behavior_player_bar_visualizer_name.to_string(),
        description: strings
          .behavior_player_bar_visualizer_description
          .to_string(),
        value: SettingValue::Cycle(
          self
            .user_config
            .behavior
            .player_bar_visualizer
            .name()
            .to_string(),
          crate::core::user_config::PlayerBarVisualizer::options(),
        ),
      },
      SettingItem {
        id: "behavior.startup_route".to_string(),
        name: strings.behavior_startup_route_name.to_string(),
        description: strings.behavior_startup_route_description.to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.startup_route.clone(),
          self.startup_route_options(),
        ),
      },
      SettingItem {
        id: "behavior.default_sort_playlist_tracks".to_string(),
        name: strings
          .behavior_default_sort_playlist_tracks_name
          .to_string(),
        description: strings
          .behavior_default_sort_playlist_tracks_description
          .to_string(),
        value: SettingValue::Cycle(
          self
            .user_config
            .behavior
            .default_sort_playlist_tracks
            .clone(),
          PLAYLIST_TRACK_SORT_SETTING_OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.default_sort_saved_albums".to_string(),
        name: strings.behavior_default_sort_saved_albums_name.to_string(),
        description: strings
          .behavior_default_sort_saved_albums_description
          .to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.default_sort_saved_albums.clone(),
          SAVED_ALBUM_SORT_SETTING_OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.default_sort_saved_artists".to_string(),
        name: strings.behavior_default_sort_saved_artists_name.to_string(),
        description: strings
          .behavior_default_sort_saved_artists_description
          .to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.default_sort_saved_artists.clone(),
          SAVED_ARTIST_SORT_SETTING_OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.default_sort_recently_played".to_string(),
        name: strings
          .behavior_default_sort_recently_played_name
          .to_string(),
        description: strings
          .behavior_default_sort_recently_played_description
          .to_string(),
        value: SettingValue::Cycle(
          self
            .user_config
            .behavior
            .default_sort_recently_played
            .clone(),
          RECENTLY_PLAYED_SORT_SETTING_OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.sidebar_position".to_string(),
        name: strings.behavior_sidebar_position_name.to_string(),
        description: strings.behavior_sidebar_position_description.to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.sidebar_position.clone(),
          SIDEBAR_POSITION_SETTING_OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.playbar_position".to_string(),
        name: strings.behavior_playbar_position_name.to_string(),
        description: strings.behavior_playbar_position_description.to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.playbar_position.clone(),
          PLAYBAR_POSITION_SETTING_OPTIONS,
        ),
      },
      SettingItem {
        id: "behavior.small_terminal_width".to_string(),
        name: strings.behavior_small_terminal_width_name.to_string(),
        description: strings
          .behavior_small_terminal_width_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.small_terminal_width as i64),
      },
      SettingItem {
        id: "behavior.small_terminal_height".to_string(),
        name: strings.behavior_small_terminal_height_name.to_string(),
        description: strings
          .behavior_small_terminal_height_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.small_terminal_height as i64),
      },
      SettingItem {
        id: "behavior.enable_announcements".to_string(),
        name: strings.behavior_enable_announcements_name.to_string(),
        description: strings
          .behavior_enable_announcements_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_announcements),
      },
      SettingItem {
        id: "behavior.enable_lyrics".to_string(),
        name: strings.behavior_enable_lyrics_name.to_string(),
        description: strings.behavior_enable_lyrics_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_lyrics),
      },
      SettingItem {
        id: "behavior.enable_monthly_recap_prompt".to_string(),
        name: strings
          .behavior_enable_monthly_recap_prompt_name
          .to_string(),
        description: strings
          .behavior_enable_monthly_recap_prompt_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_monthly_recap_prompt),
      },
      SettingItem {
        id: "behavior.pin_community_playlist".to_string(),
        name: strings.behavior_pin_community_playlist_name.to_string(),
        description: strings
          .behavior_pin_community_playlist_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.pin_community_playlist),
      },
      #[cfg(feature = "telemetry")]
      SettingItem {
        id: "behavior.enable_global_song_count".to_string(),
        name: strings.behavior_enable_global_song_count_name.to_string(),
        description: strings
          .behavior_enable_global_song_count_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.enable_global_song_count),
      },
      #[cfg(feature = "self-update")]
      SettingItem {
        id: "behavior.disable_auto_update".to_string(),
        name: strings.behavior_disable_auto_update_name.to_string(),
        description: strings.behavior_disable_auto_update_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.disable_auto_update),
      },
      #[cfg(feature = "self-update")]
      SettingItem {
        id: "behavior.auto_update_delay".to_string(),
        name: strings.behavior_auto_update_delay_name.to_string(),
        description: strings.behavior_auto_update_delay_description.to_string(),
        value: SettingValue::String(self.user_config.behavior.auto_update_delay.clone()),
      },
      SettingItem {
        id: "behavior.announcement_feed_url".to_string(),
        name: strings.behavior_announcement_feed_url_name.to_string(),
        description: strings
          .behavior_announcement_feed_url_description
          .to_string(),
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
        name: strings.behavior_sync_token_name.to_string(),
        description: strings.behavior_sync_token_description.to_string(),
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
        name: strings.behavior_draw_cover_art_name.to_string(),
        description: strings.behavior_draw_cover_art_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.draw_cover_art),
      },
      #[cfg(feature = "cover-art")]
      SettingItem {
        id: "behavior.draw_cover_art_forced".to_string(),
        name: strings.behavior_draw_cover_art_forced_name.to_string(),
        description: strings
          .behavior_draw_cover_art_forced_description
          .to_string(),
        value: SettingValue::Bool(self.user_config.behavior.draw_cover_art_forced),
      },
      #[cfg(feature = "cover-art")]
      SettingItem {
        id: "behavior.playbar_cover_art_size_percent".to_string(),
        name: strings
          .behavior_playbar_cover_art_size_percent_name
          .to_string(),
        description: strings
          .behavior_playbar_cover_art_size_percent_description
          .to_string(),
        value: SettingValue::Number(
          self.user_config.behavior.playbar_cover_art_size_percent as i64,
        ),
      },
      #[cfg(feature = "cover-art")]
      SettingItem {
        id: "behavior.cover_art_dither".to_string(),
        name: strings.behavior_cover_art_dither_name.to_string(),
        description: strings.behavior_cover_art_dither_description.to_string(),
        value: SettingValue::Bool(self.user_config.behavior.cover_art_dither),
      },
      #[cfg(feature = "cover-art")]
      SettingItem {
        id: "behavior.cover_art_dither_algorithm".to_string(),
        name: strings.behavior_cover_art_dither_algorithm_name.to_string(),
        description: strings
          .behavior_cover_art_dither_algorithm_description
          .to_string(),
        value: SettingValue::Cycle(
          self.user_config.behavior.cover_art_dither_algorithm.clone(),
          &["stucki", "bayer8x8", "atkinson"],
        ),
      },
      #[cfg(feature = "cover-art")]
      SettingItem {
        id: "behavior.cover_art_dither_pixel_scale".to_string(),
        name: strings
          .behavior_cover_art_dither_pixel_scale_name
          .to_string(),
        description: strings
          .behavior_cover_art_dither_pixel_scale_description
          .to_string(),
        value: SettingValue::Number(self.user_config.behavior.cover_art_dither_pixel_scale as i64),
      },
    ]
  }

  /// Load settings for the current category into settings_items
  pub fn load_settings_for_category(&mut self) {
    let strings = &self.strings().settings;
    self.settings_items = match self.view.settings_category {
      SettingsCategory::Behavior => self.behavior_setting_items(),
      SettingsCategory::Icons => vec![
        SettingItem {
          id: "behavior.liked_icon".to_string(),
          name: strings.behavior_liked_icon_name.to_string(),
          description: strings.behavior_liked_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.liked_icon.clone()),
        },
        SettingItem {
          id: "behavior.shuffle_icon".to_string(),
          name: strings.behavior_shuffle_icon_name.to_string(),
          description: strings.behavior_shuffle_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.shuffle_icon.clone()),
        },
        SettingItem {
          id: "behavior.repeat_track_icon".to_string(),
          name: strings.behavior_repeat_track_icon_name.to_string(),
          description: strings.behavior_repeat_track_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.repeat_track_icon.clone()),
        },
        SettingItem {
          id: "behavior.repeat_context_icon".to_string(),
          name: strings.behavior_repeat_context_icon_name.to_string(),
          description: strings.behavior_repeat_context_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.repeat_context_icon.clone()),
        },
        SettingItem {
          id: "behavior.playing_icon".to_string(),
          name: strings.behavior_playing_icon_name.to_string(),
          description: strings.behavior_playing_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.playing_icon.clone()),
        },
        SettingItem {
          id: "behavior.paused_icon".to_string(),
          name: strings.behavior_paused_icon_name.to_string(),
          description: strings.behavior_paused_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.paused_icon.clone()),
        },
        SettingItem {
          id: "behavior.gauge_filled_icon".to_string(),
          name: strings.behavior_gauge_filled_icon_name.to_string(),
          description: strings.behavior_gauge_filled_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.gauge_filled_icon.clone()),
        },
        SettingItem {
          id: "behavior.gauge_unfilled_icon".to_string(),
          name: strings.behavior_gauge_unfilled_icon_name.to_string(),
          description: strings.behavior_gauge_unfilled_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.gauge_unfilled_icon.clone()),
        },
        SettingItem {
          id: "behavior.active_source_icon".to_string(),
          name: strings.behavior_active_source_icon_name.to_string(),
          description: strings.behavior_active_source_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.active_source_icon.clone()),
        },
        SettingItem {
          id: "behavior.episode_played_icon".to_string(),
          name: strings.behavior_episode_played_icon_name.to_string(),
          description: strings.behavior_episode_played_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.episode_played_icon.clone()),
        },
        SettingItem {
          id: "behavior.sort_ascending_icon".to_string(),
          name: strings.behavior_sort_ascending_icon_name.to_string(),
          description: strings.behavior_sort_ascending_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.sort_ascending_icon.clone()),
        },
        SettingItem {
          id: "behavior.sort_descending_icon".to_string(),
          name: strings.behavior_sort_descending_icon_name.to_string(),
          description: strings
            .behavior_sort_descending_icon_description
            .to_string(),
          value: SettingValue::String(self.user_config.behavior.sort_descending_icon.clone()),
        },
        SettingItem {
          id: "behavior.list_highlight_icon".to_string(),
          name: strings.behavior_list_highlight_icon_name.to_string(),
          description: strings.behavior_list_highlight_icon_description.to_string(),
          value: SettingValue::String(self.user_config.behavior.list_highlight_icon.clone()),
        },
      ],
      SettingsCategory::Keybindings => vec![
        SettingItem {
          id: "keys.back".to_string(),
          name: strings.keys_back_name.to_string(),
          description: strings.keys_back_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.back)),
        },
        SettingItem {
          id: "keys.move_up".to_string(),
          name: strings.keys_move_up_name.to_string(),
          description: strings.keys_move_up_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.move_up)),
        },
        SettingItem {
          id: "keys.move_down".to_string(),
          name: strings.keys_move_down_name.to_string(),
          description: strings.keys_move_down_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.move_down)),
        },
        SettingItem {
          id: "keys.move_left".to_string(),
          name: strings.keys_move_left_name.to_string(),
          description: strings.keys_move_left_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.move_left)),
        },
        SettingItem {
          id: "keys.move_right".to_string(),
          name: strings.keys_move_right_name.to_string(),
          description: strings.keys_move_right_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.move_right)),
        },
        SettingItem {
          id: "keys.next_page".to_string(),
          name: strings.keys_next_page_name.to_string(),
          description: strings.keys_next_page_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.next_page)),
        },
        SettingItem {
          id: "keys.previous_page".to_string(),
          name: strings.keys_previous_page_name.to_string(),
          description: strings.keys_previous_page_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.previous_page)),
        },
        SettingItem {
          id: "keys.jump_to_start".to_string(),
          name: strings.keys_jump_to_start_name.to_string(),
          description: strings.keys_jump_to_start_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.jump_to_start)),
        },
        SettingItem {
          id: "keys.jump_to_end".to_string(),
          name: strings.keys_jump_to_end_name.to_string(),
          description: strings.keys_jump_to_end_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.jump_to_end)),
        },
        SettingItem {
          id: "keys.toggle_playback".to_string(),
          name: strings.keys_toggle_playback_name.to_string(),
          description: strings.keys_toggle_playback_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.toggle_playback)),
        },
        SettingItem {
          id: "keys.seek_backwards".to_string(),
          name: strings.keys_seek_backwards_name.to_string(),
          description: strings.keys_seek_backwards_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.seek_backwards)),
        },
        SettingItem {
          id: "keys.seek_forwards".to_string(),
          name: strings.keys_seek_forwards_name.to_string(),
          description: strings.keys_seek_forwards_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.seek_forwards)),
        },
        SettingItem {
          id: "keys.next_track".to_string(),
          name: strings.keys_next_track_name.to_string(),
          description: strings.keys_next_track_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.next_track)),
        },
        SettingItem {
          id: "keys.previous_track".to_string(),
          name: strings.keys_previous_track_name.to_string(),
          description: strings.keys_previous_track_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.previous_track)),
        },
        SettingItem {
          id: "keys.force_previous_track".to_string(),
          name: strings.keys_force_previous_track_name.to_string(),
          description: strings.keys_force_previous_track_description.to_string(),
          value: SettingValue::Key(key_to_config_string(
            &self.user_config.keys.force_previous_track,
          )),
        },
        SettingItem {
          id: "keys.shuffle".to_string(),
          name: strings.keys_shuffle_name.to_string(),
          description: strings.keys_shuffle_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.shuffle)),
        },
        SettingItem {
          id: "keys.repeat".to_string(),
          name: strings.keys_repeat_name.to_string(),
          description: strings.keys_repeat_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.repeat)),
        },
        SettingItem {
          id: "keys.search".to_string(),
          name: strings.keys_search_name.to_string(),
          description: strings.keys_search_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.search)),
        },
        SettingItem {
          id: "keys.help".to_string(),
          name: strings.keys_help_name.to_string(),
          description: strings.keys_help_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.help)),
        },
        SettingItem {
          id: "keys.open_settings".to_string(),
          name: strings.keys_open_settings_name.to_string(),
          description: strings.keys_open_settings_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.open_settings)),
        },
        SettingItem {
          id: "keys.save_settings".to_string(),
          name: strings.keys_save_settings_name.to_string(),
          description: strings.keys_save_settings_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.save_settings)),
        },
        SettingItem {
          id: "keys.jump_to_album".to_string(),
          name: strings.keys_jump_to_album_name.to_string(),
          description: strings.keys_jump_to_album_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.jump_to_album)),
        },
        SettingItem {
          id: "keys.jump_to_artist_album".to_string(),
          name: strings.keys_jump_to_artist_album_name.to_string(),
          description: strings.keys_jump_to_artist_album_description.to_string(),
          value: SettingValue::Key(key_to_config_string(
            &self.user_config.keys.jump_to_artist_album,
          )),
        },
        SettingItem {
          id: "keys.jump_to_context".to_string(),
          name: strings.keys_jump_to_context_name.to_string(),
          description: strings.keys_jump_to_context_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.jump_to_context)),
        },
        SettingItem {
          id: "keys.manage_devices".to_string(),
          name: strings.keys_manage_devices_name.to_string(),
          description: strings.keys_manage_devices_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.manage_devices)),
        },
        SettingItem {
          id: "keys.listening_party".to_string(),
          name: strings.keys_listening_party_name.to_string(),
          description: strings.keys_listening_party_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.listening_party)),
        },
        SettingItem {
          id: "keys.decrease_volume".to_string(),
          name: strings.keys_decrease_volume_name.to_string(),
          description: strings.keys_decrease_volume_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.decrease_volume)),
        },
        SettingItem {
          id: "keys.increase_volume".to_string(),
          name: strings.keys_increase_volume_name.to_string(),
          description: strings.keys_increase_volume_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.increase_volume)),
        },
        SettingItem {
          id: "keys.add_item_to_queue".to_string(),
          name: strings.keys_add_item_to_queue_name.to_string(),
          description: strings.keys_add_item_to_queue_description.to_string(),
          value: SettingValue::Key(key_to_config_string(
            &self.user_config.keys.add_item_to_queue,
          )),
        },
        SettingItem {
          id: "keys.show_queue".to_string(),
          name: strings.keys_show_queue_name.to_string(),
          description: strings.keys_show_queue_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.show_queue)),
        },
        SettingItem {
          id: "keys.remove_from_queue".to_string(),
          name: strings.keys_remove_from_queue_name.to_string(),
          description: strings.keys_remove_from_queue_description.to_string(),
          value: SettingValue::Key(key_to_config_string(
            &self.user_config.keys.remove_from_queue,
          )),
        },
        SettingItem {
          id: "keys.like_track".to_string(),
          name: strings.keys_like_track_name.to_string(),
          description: strings.keys_like_track_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.like_track)),
        },
        SettingItem {
          id: "keys.generate_recap".to_string(),
          name: strings.keys_generate_recap_name.to_string(),
          description: strings.keys_generate_recap_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.generate_recap)),
        },
        SettingItem {
          id: "keys.copy_song_url".to_string(),
          name: strings.keys_copy_song_url_name.to_string(),
          description: strings.keys_copy_song_url_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.copy_song_url)),
        },
        SettingItem {
          id: "keys.copy_album_url".to_string(),
          name: strings.keys_copy_album_url_name.to_string(),
          description: strings.keys_copy_album_url_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.copy_album_url)),
        },
        SettingItem {
          id: "keys.audio_analysis".to_string(),
          name: strings.keys_audio_analysis_name.to_string(),
          description: strings.keys_audio_analysis_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.audio_analysis)),
        },
        SettingItem {
          id: "keys.lyrics_view".to_string(),
          name: strings.keys_lyrics_view_name.to_string(),
          description: strings.keys_lyrics_view_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.lyrics_view)),
        },
        SettingItem {
          id: "keys.miniplayer_view".to_string(),
          name: strings.keys_miniplayer_view_name.to_string(),
          description: strings.keys_miniplayer_view_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.miniplayer_view)),
        },
        #[cfg(feature = "cover-art")]
        SettingItem {
          id: "keys.cover_art_view".to_string(),
          name: strings.keys_cover_art_view_name.to_string(),
          description: strings.keys_cover_art_view_description.to_string(),
          value: SettingValue::Key(key_to_config_string(&self.user_config.keys.cover_art_view)),
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
            name: strings.theme_preset_name.to_string(),
            description: strings.theme_preset_description.to_string(),
            value: SettingValue::Preset(self.user_config.current_preset.name().to_string()),
          },
          SettingItem {
            id: "theme.active".to_string(),
            name: strings.theme_active_name.to_string(),
            description: strings.theme_active_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.active)),
          },
          SettingItem {
            id: "theme.banner".to_string(),
            name: strings.theme_banner_name.to_string(),
            description: strings.theme_banner_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.banner)),
          },
          SettingItem {
            id: "behavior.banner_gradient".to_string(),
            name: strings.behavior_banner_gradient_name.to_string(),
            description: strings.behavior_banner_gradient_description.to_string(),
            value: SettingValue::Bool(self.user_config.behavior.banner_gradient),
          },
          #[cfg(feature = "art-decode")]
          SettingItem {
            id: "behavior.cover_art_theme".to_string(),
            name: strings.behavior_cover_art_theme_name.to_string(),
            description: strings.behavior_cover_art_theme_description.to_string(),
            value: SettingValue::Bool(self.user_config.behavior.cover_art_theme),
          },
          SettingItem {
            id: "theme.hint".to_string(),
            name: strings.theme_hint_name.to_string(),
            description: strings.theme_hint_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.hint)),
          },
          SettingItem {
            id: "theme.hovered".to_string(),
            name: strings.theme_hovered_name.to_string(),
            description: strings.theme_hovered_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.hovered)),
          },
          SettingItem {
            id: "theme.selected".to_string(),
            name: strings.theme_selected_name.to_string(),
            description: strings.theme_selected_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.selected)),
          },
          SettingItem {
            id: "theme.inactive".to_string(),
            name: strings.theme_inactive_name.to_string(),
            description: strings.theme_inactive_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.inactive)),
          },
          SettingItem {
            id: "theme.text".to_string(),
            name: strings.theme_text_name.to_string(),
            description: strings.theme_text_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.text)),
          },
          SettingItem {
            id: "theme.error_text".to_string(),
            name: strings.theme_error_text_name.to_string(),
            description: strings.theme_error_text_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.error_text)),
          },
          SettingItem {
            id: "theme.error_border".to_string(),
            name: strings.theme_error_border_name.to_string(),
            description: strings.theme_error_border_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.error_border)),
          },
          SettingItem {
            id: "theme.playbar_background".to_string(),
            name: strings.theme_playbar_background_name.to_string(),
            description: strings.theme_playbar_background_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_background)),
          },
          SettingItem {
            id: "theme.playbar_progress".to_string(),
            name: strings.theme_playbar_progress_name.to_string(),
            description: strings.theme_playbar_progress_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_progress)),
          },
          #[cfg(feature = "cover-art")]
          SettingItem {
            id: "theme.cover_art_dither_color".to_string(),
            name: strings.theme_cover_art_dither_color_name.to_string(),
            description: strings.theme_cover_art_dither_color_description.to_string(),
            value: SettingValue::Color(
              self
                .user_config
                .cover_art_dither_color
                .map(color_to_string)
                .unwrap_or_else(|| "auto".to_string()),
            ),
          },
          SettingItem {
            id: "theme.playbar_progress_text".to_string(),
            name: strings.theme_playbar_progress_text_name.to_string(),
            description: strings.theme_playbar_progress_text_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_progress_text)),
          },
          SettingItem {
            id: "theme.playbar_text".to_string(),
            name: strings.theme_playbar_text_name.to_string(),
            description: strings.theme_playbar_text_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.playbar_text)),
          },
          SettingItem {
            id: "theme.highlighted_lyrics".to_string(),
            name: strings.theme_highlighted_lyrics_name.to_string(),
            description: strings.theme_highlighted_lyrics_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.highlighted_lyrics)),
          },
          SettingItem {
            id: "theme.background".to_string(),
            name: strings.theme_background_name.to_string(),
            description: strings.theme_background_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.background)),
          },
          SettingItem {
            id: "theme.header".to_string(),
            name: strings.theme_header_name.to_string(),
            description: strings.theme_header_description.to_string(),
            value: SettingValue::Color(color_to_string(user_theme.header)),
          },
        ]
      }
    };
    self.view.settings_selected_index = self
      .settings_items
      .iter()
      .position(|item| self.setting_offered(item))
      .unwrap_or(0);
    self.settings_saved_items = self.settings_items.clone();
    self.view.settings_unsaved_prompt_visible = false;
    self.view.settings_unsaved_prompt_save_selected = true;
  }

  /// Whether a row is drawn now: key rows always, other rows only when their requirement is met.
  pub(crate) fn setting_offered(&self, item: &SettingItem) -> bool {
    matches!(item.value, SettingValue::Key(_))
      || self
        .availability(setting_requirement(&item.id))
        .is_available()
  }

  /// The startup routes the cycle offers; a stored Spotify route stays in it so the cycle cannot strand it.
  fn startup_route_options(&self) -> &'static [&'static str] {
    let current = self.user_config.behavior.startup_route.as_str();
    if self.spotify_connected || !STARTUP_ROUTE_SESSION_FREE_OPTIONS.contains(&current) {
      STARTUP_ROUTE_SETTING_OPTIONS
    } else {
      STARTUP_ROUTE_SESSION_FREE_OPTIONS
    }
  }

  /// Enter the Settings screen on a fresh, unfiltered view of the current
  /// category. Every front door (keybinding, sidebar click, Lua `navigate`)
  /// goes through here so none of them can carry a stale filter in.
  pub fn open_settings_screen(&mut self) {
    self.clear_settings_filter();
    self.load_settings_for_category();
    self.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
  }

  /// Whether the Settings screen holds staged edits that were not saved.
  pub fn has_unsaved_settings_changes(&self) -> bool {
    self.settings_items != self.settings_saved_items
  }

  /// Open the settings row filter on a fresh, empty query.
  pub fn begin_settings_filter(&mut self) {
    self.view.settings_filter.clear();
    self.view.settings_filter_editing = true;
  }

  /// Stop typing into the settings row filter, leaving it applied.
  pub fn apply_settings_filter(&mut self) {
    self.view.settings_filter_editing = false;
  }

  /// Drop the settings row filter entirely.
  pub fn clear_settings_filter(&mut self) {
    self.view.settings_filter.clear();
    self.view.settings_filter_editing = false;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn applying_language_relabels_settings_without_discarding_staged_values() {
    use crate::core::i18n::Language;
    let mut app = App::default();
    app.load_settings_for_category();
    for item in &mut app.settings_items {
      match item.id.as_str() {
        "behavior.language" => {
          item.value = SettingValue::Cycle("zh-Hant".to_string(), Language::OPTIONS);
        }
        "behavior.volume_increment" => item.value = SettingValue::Number(9),
        _ => {}
      }
    }
    let selected = app.view.settings_selected_index;
    app.apply_settings_changes();

    assert_eq!(app.user_config.behavior.language, Language::ZhHant);
    assert_eq!(app.user_config.behavior.volume_increment, 9);
    assert_eq!(app.view.settings_selected_index, selected);
    assert!(app.has_unsaved_settings_changes());
    for items in [&app.settings_items, &app.settings_saved_items] {
      let row = items
        .iter()
        .find(|item| item.id == "behavior.language")
        .unwrap();
      assert_eq!(row.name, "語言");
      assert_eq!(row.description, "介面語言（自動跟隨系統語言）");
    }
    let language = app
      .settings_items
      .iter()
      .find(|item| item.id == "behavior.language")
      .unwrap();
    assert_eq!(
      language.value,
      SettingValue::Cycle("zh-Hant".to_string(), Language::OPTIONS)
    );
  }

  #[test]
  fn relabeling_settings_keeps_the_clean_snapshot_clean() {
    let mut app = App::default();
    for category in SettingsCategory::all() {
      app.view.settings_category = *category;
      app.load_settings_for_category();
      app.user_config.behavior.language = crate::core::i18n::Language::ZhHans;
      app.refresh_language();
      app.refresh_settings_labels();
      assert!(!app.has_unsaved_settings_changes());
      for row in &app.settings_items {
        let (name, description) = app.strings().settings.row_text(&row.id).unwrap();
        assert_eq!(row.name, name);
        assert_eq!(row.description, description);
      }
      app.user_config.behavior.language = crate::core::i18n::Language::En;
      app.refresh_language();
    }
  }

  #[test]
  fn every_sort_field_a_context_offers_is_selectable_in_settings() {
    use crate::core::sort::{SortContext, SortField};
    for (context, options) in [
      (
        SortContext::PlaylistTracks,
        PLAYLIST_TRACK_SORT_SETTING_OPTIONS,
      ),
      (SortContext::SavedAlbums, SAVED_ALBUM_SORT_SETTING_OPTIONS),
      (SortContext::SavedArtists, SAVED_ARTIST_SORT_SETTING_OPTIONS),
      (
        SortContext::RecentlyPlayed,
        RECENTLY_PLAYED_SORT_SETTING_OPTIONS,
      ),
    ] {
      let expected: Vec<String> = context
        .available_fields()
        .iter()
        .flat_map(|field| {
          let token = field.to_config_str();
          if *field == SortField::Default {
            vec![token.to_string()]
          } else {
            vec![token.to_string(), format!("{token}:desc")]
          }
        })
        .collect();
      assert_eq!(expected, options, "{context:?}");
    }
  }

  #[test]
  fn the_icons_tab_edits_both_repeat_icons() {
    let mut app = App::default();
    app.view.settings_category = SettingsCategory::Icons;
    app.load_settings_for_category();
    for (id, value) in [
      ("behavior.repeat_track_icon", "R1"),
      ("behavior.repeat_context_icon", "RA"),
    ] {
      let item = app
        .settings_items
        .iter_mut()
        .find(|item| item.id == id)
        .unwrap_or_else(|| panic!("{id} row"));
      item.value = SettingValue::String(value.to_string());
    }

    app.apply_settings_changes();

    assert_eq!(app.user_config.behavior.repeat_track_icon, "R1");
    assert_eq!(app.user_config.behavior.repeat_context_icon, "RA");
  }

  fn keybinding_rows_with_f5_help_and_home_search() -> App {
    let mut app = App::default();
    app.user_config.keys.help = Key::F5;
    app.user_config.keys.search = Key::Home;
    app.view.settings_category = SettingsCategory::Keybindings;
    app.load_settings_for_category();
    app
  }

  fn key_row(app: &App, id: &str) -> SettingValue {
    app
      .settings_items
      .iter()
      .find(|item| item.id == id)
      .unwrap_or_else(|| panic!("{id} row"))
      .value
      .clone()
  }

  #[test]
  fn keybinding_rows_spell_keys_the_way_config_yml_does() {
    let app = keybinding_rows_with_f5_help_and_home_search();
    assert_eq!(
      key_row(&app, "keys.help"),
      SettingValue::Key("f5".to_string())
    );
    assert_eq!(
      key_row(&app, "keys.search"),
      SettingValue::Key("home".to_string())
    );
    assert_eq!(
      key_row(&app, "keys.toggle_playback"),
      SettingValue::Key("space".to_string())
    );
  }

  #[test]
  fn keybinding_rows_show_jump_and_party_keys() {
    let app = keybinding_rows_with_f5_help_and_home_search();
    for (id, value) in [
      ("keys.jump_to_start", "ctrl-a"),
      ("keys.jump_to_end", "ctrl-e"),
      ("keys.listening_party", "ctrl-p"),
    ] {
      assert_eq!(key_row(&app, id), SettingValue::Key(value.to_string()));
    }
  }

  #[test]
  fn editing_jump_and_party_rows_applies_the_new_keys() {
    let mut app = keybinding_rows_with_f5_help_and_home_search();
    for (id, value) in [
      ("keys.jump_to_start", "f6"),
      ("keys.jump_to_end", "f7"),
      ("keys.listening_party", "f8"),
    ] {
      let item = app
        .settings_items
        .iter_mut()
        .find(|item| item.id == id)
        .unwrap();
      item.value = SettingValue::Key(value.to_string());
    }
    app.apply_settings_changes();
    assert_eq!(app.user_config.keys.jump_to_start, Key::F6);
    assert_eq!(app.user_config.keys.jump_to_end, Key::F7);
    assert_eq!(app.user_config.keys.listening_party, Key::F8);
  }

  #[test]
  fn the_listening_party_key_row_needs_a_spotify_session() {
    let mut app = keybinding_rows_with_f5_help_and_home_search();
    let requirement = setting_requirement("keys.listening_party");
    assert!(!app.availability(requirement).is_available());
    assert!(offered(&app, "keys.listening_party"));
    assert_eq!(setting_requirement("keys.jump_to_start"), Requirement::None);
    assert_eq!(setting_requirement("keys.jump_to_end"), Requirement::None);
    app.spotify_connected = true;
    assert!(app.availability(requirement).is_available());
  }

  #[test]
  fn every_key_row_parses_back_as_a_key() {
    let app = keybinding_rows_with_f5_help_and_home_search();
    let key_rows: Vec<_> = app
      .settings_items
      .iter()
      .filter(|item| item.id.starts_with("keys."))
      .collect();
    assert!(!key_rows.is_empty());
    for item in key_rows {
      let SettingValue::Key(text) = &item.value else {
        panic!("{} is not a key row", item.id);
      };
      assert!(
        crate::core::user_config::parse_key_public(text.clone()).is_ok(),
        "{} shows {text:?}, which does not parse",
        item.id
      );
    }
  }

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

  #[test]
  fn session_free_startup_routes_are_the_routes_with_no_startup_requirement() {
    let expected: Vec<&str> = RouteId::STARTUP_OPTIONS
      .iter()
      .filter(|route| route.startup_requirement() == Requirement::None)
      .map(|route| route.to_config_str())
      .collect();
    assert_eq!(STARTUP_ROUTE_SESSION_FREE_OPTIONS, expected.as_slice());
  }

  #[test]
  fn the_startup_cycle_narrows_without_a_session_unless_the_stored_route_needs_one() {
    let mut app = App::default();
    assert_eq!(
      app.startup_route_options(),
      STARTUP_ROUTE_SESSION_FREE_OPTIONS
    );
    app.user_config.behavior.startup_route = "discover".to_string();
    assert_eq!(app.startup_route_options(), STARTUP_ROUTE_SETTING_OPTIONS);
    app.spotify_connected = true;
    app.user_config.behavior.startup_route = "home".to_string();
    assert_eq!(app.startup_route_options(), STARTUP_ROUTE_SETTING_OPTIONS);
  }

  fn offered(app: &App, id: &str) -> bool {
    let item = app
      .settings_items
      .iter()
      .find(|item| item.id == id)
      .unwrap_or_else(|| panic!("no row {id}"));
    app.setting_offered(item)
  }

  #[test]
  fn without_a_session_the_spotify_rows_are_hidden_and_the_key_rows_stay() {
    let mut app = App::default();
    app.load_settings_for_category();
    assert!(!offered(&app, "behavior.default_sort_saved_albums"));
    assert!(offered(&app, "behavior.default_sort_playlist_tracks"));
    assert!(app.setting_offered(&app.settings_items[app.view.settings_selected_index]));

    app.view.settings_category = SettingsCategory::Keybindings;
    app.load_settings_for_category();
    assert!(offered(&app, "keys.like_track"));

    // The rows follow the session live, without a reload.
    app.view.settings_category = SettingsCategory::Behavior;
    app.load_settings_for_category();
    app.spotify_connected = true;
    assert!(offered(&app, "behavior.default_sort_saved_albums"));
  }

  #[test]
  fn the_like_key_row_is_offered_under_radio() {
    let mut app = App::default_connected();
    app.view.settings_category = SettingsCategory::Keybindings;
    app.load_settings_for_category();
    app.active_source = Source::Radio;
    assert!(offered(&app, "keys.like_track"));
    assert!(app
      .availability(setting_requirement("keys.like_track"))
      .is_available());
    app.active_source = Source::Local;
    assert!(!app
      .availability(setting_requirement("keys.like_track"))
      .is_available());
  }
}
