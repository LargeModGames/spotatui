//! Settings copy shared by terminal and browser frontends. Values and config keys stay unchanged.

mod en;
mod zh_hans;
mod zh_hant;
pub use en::EN;
pub use zh_hans::ZH_HANS;
pub use zh_hant::ZH_HANT;

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct SettingsStrings {
  pub behavior_ytdlp_path_description: &'static str,
  pub behavior_ytdlp_path_name: &'static str,
  pub behavior_playback_engine_name: &'static str,
  pub behavior_playback_engine_description: &'static str,
  pub behavior_soloist_path_name: &'static str,
  pub behavior_soloist_path_description: &'static str,
  pub soloist_download_button: &'static str,
  pub soloist_downloading_button: &'static str,
  pub behavior_soloist_audio_quality_name: &'static str,
  pub behavior_soloist_audio_quality_description: &'static str,
  pub behavior_local_music_path_description: &'static str,
  pub behavior_local_music_path_name: &'static str,
  pub behavior_subsonic_password_description: &'static str,
  pub behavior_subsonic_password_name: &'static str,
  pub behavior_subsonic_username_description: &'static str,
  pub behavior_subsonic_username_name: &'static str,
  pub behavior_subsonic_url_description: &'static str,
  pub behavior_subsonic_url_name: &'static str,
  pub behavior_language_name: &'static str,
  pub behavior_language_description: &'static str,
  pub behavior_seek_milliseconds_name: &'static str,
  pub behavior_seek_milliseconds_description: &'static str,
  pub behavior_volume_increment_name: &'static str,
  pub behavior_volume_increment_description: &'static str,
  pub behavior_tick_rate_milliseconds_name: &'static str,
  pub behavior_tick_rate_milliseconds_description: &'static str,
  pub behavior_animation_tick_rate_milliseconds_name: &'static str,
  pub behavior_animation_tick_rate_milliseconds_description: &'static str,
  pub behavior_status_message_ttl_percent_name: &'static str,
  pub behavior_status_message_ttl_percent_description: &'static str,
  pub behavior_qobuz_quality_name: &'static str,
  pub behavior_qobuz_quality_description: &'static str,
  pub behavior_playback_poll_seconds_name: &'static str,
  pub behavior_playback_poll_seconds_description: &'static str,
  pub behavior_table_scroll_padding_name: &'static str,
  pub behavior_table_scroll_padding_description: &'static str,
  pub behavior_like_animation_frames_name: &'static str,
  pub behavior_like_animation_frames_description: &'static str,
  pub behavior_enable_text_emphasis_name: &'static str,
  pub behavior_enable_text_emphasis_description: &'static str,
  pub behavior_show_loading_indicator_name: &'static str,
  pub behavior_show_loading_indicator_description: &'static str,
  pub behavior_enforce_wide_search_bar_name: &'static str,
  pub behavior_enforce_wide_search_bar_description: &'static str,
  pub behavior_group_folders_first_name: &'static str,
  pub behavior_group_folders_first_description: &'static str,
  pub behavior_disable_mouse_inputs_name: &'static str,
  pub behavior_disable_mouse_inputs_description: &'static str,
  pub behavior_set_window_title_name: &'static str,
  pub behavior_set_window_title_description: &'static str,
  pub behavior_enable_discord_rpc_name: &'static str,
  pub behavior_enable_discord_rpc_description: &'static str,
  pub behavior_stop_after_current_track_name: &'static str,
  pub behavior_stop_after_current_track_description: &'static str,
  pub behavior_keepawake_enabled_name: &'static str,
  pub behavior_keepawake_enabled_description: &'static str,
  pub behavior_enable_media_keys_name: &'static str,
  pub behavior_enable_media_keys_description: &'static str,
  pub behavior_enable_streaming_name: &'static str,
  pub behavior_enable_streaming_description: &'static str,
  pub behavior_streaming_device_name_name: &'static str,
  pub behavior_streaming_device_name_description: &'static str,
  pub behavior_streaming_bitrate_name: &'static str,
  pub behavior_streaming_bitrate_description: &'static str,
  pub behavior_streaming_audio_cache_name: &'static str,
  pub behavior_streaming_audio_cache_description: &'static str,
  pub behavior_streaming_autoplay_name: &'static str,
  pub behavior_streaming_autoplay_description: &'static str,
  pub behavior_startup_behavior_name: &'static str,
  pub behavior_startup_behavior_description: &'static str,
  pub behavior_player_bar_visualizer_name: &'static str,
  pub behavior_player_bar_visualizer_description: &'static str,
  pub behavior_startup_route_name: &'static str,
  pub behavior_startup_route_description: &'static str,
  pub behavior_default_sort_playlist_tracks_name: &'static str,
  pub behavior_default_sort_playlist_tracks_description: &'static str,
  pub behavior_default_sort_saved_albums_name: &'static str,
  pub behavior_default_sort_saved_albums_description: &'static str,
  pub behavior_default_sort_saved_artists_name: &'static str,
  pub behavior_default_sort_saved_artists_description: &'static str,
  pub behavior_default_sort_recently_played_name: &'static str,
  pub behavior_default_sort_recently_played_description: &'static str,
  pub behavior_sidebar_position_name: &'static str,
  pub behavior_sidebar_position_description: &'static str,
  pub behavior_playbar_position_name: &'static str,
  pub behavior_playbar_position_description: &'static str,
  pub behavior_small_terminal_width_name: &'static str,
  pub behavior_small_terminal_width_description: &'static str,
  pub behavior_small_terminal_height_name: &'static str,
  pub behavior_small_terminal_height_description: &'static str,
  pub behavior_enable_announcements_name: &'static str,
  pub behavior_enable_announcements_description: &'static str,
  pub behavior_enable_lyrics_name: &'static str,
  pub behavior_enable_lyrics_description: &'static str,
  pub behavior_enable_monthly_recap_prompt_name: &'static str,
  pub behavior_enable_monthly_recap_prompt_description: &'static str,
  pub behavior_pin_community_playlist_name: &'static str,
  pub behavior_pin_community_playlist_description: &'static str,
  pub behavior_enable_global_song_count_name: &'static str,
  pub behavior_enable_global_song_count_description: &'static str,
  pub behavior_disable_auto_update_name: &'static str,
  pub behavior_disable_auto_update_description: &'static str,
  pub behavior_auto_update_delay_name: &'static str,
  pub behavior_auto_update_delay_description: &'static str,
  pub behavior_announcement_feed_url_name: &'static str,
  pub behavior_announcement_feed_url_description: &'static str,
  pub behavior_sync_token_name: &'static str,
  pub behavior_sync_token_description: &'static str,
  pub behavior_draw_cover_art_name: &'static str,
  pub behavior_draw_cover_art_description: &'static str,
  pub behavior_draw_cover_art_forced_name: &'static str,
  pub behavior_draw_cover_art_forced_description: &'static str,
  pub behavior_playbar_cover_art_size_percent_name: &'static str,
  pub behavior_playbar_cover_art_size_percent_description: &'static str,
  pub behavior_cover_art_dither_name: &'static str,
  pub behavior_cover_art_dither_description: &'static str,
  pub behavior_cover_art_dither_algorithm_name: &'static str,
  pub behavior_cover_art_dither_algorithm_description: &'static str,
  pub behavior_cover_art_dither_pixel_scale_name: &'static str,
  pub behavior_cover_art_dither_pixel_scale_description: &'static str,
  pub behavior_liked_icon_name: &'static str,
  pub behavior_liked_icon_description: &'static str,
  pub behavior_shuffle_icon_name: &'static str,
  pub behavior_shuffle_icon_description: &'static str,
  pub behavior_repeat_track_icon_name: &'static str,
  pub behavior_repeat_track_icon_description: &'static str,
  pub behavior_repeat_context_icon_name: &'static str,
  pub behavior_repeat_context_icon_description: &'static str,
  pub behavior_playing_icon_name: &'static str,
  pub behavior_playing_icon_description: &'static str,
  pub behavior_paused_icon_name: &'static str,
  pub behavior_paused_icon_description: &'static str,
  pub behavior_gauge_filled_icon_name: &'static str,
  pub behavior_gauge_filled_icon_description: &'static str,
  pub behavior_gauge_unfilled_icon_name: &'static str,
  pub behavior_gauge_unfilled_icon_description: &'static str,
  pub behavior_active_source_icon_name: &'static str,
  pub behavior_active_source_icon_description: &'static str,
  pub behavior_episode_played_icon_name: &'static str,
  pub behavior_episode_played_icon_description: &'static str,
  pub behavior_sort_ascending_icon_name: &'static str,
  pub behavior_sort_ascending_icon_description: &'static str,
  pub behavior_sort_descending_icon_name: &'static str,
  pub behavior_sort_descending_icon_description: &'static str,
  pub behavior_list_highlight_icon_name: &'static str,
  pub behavior_list_highlight_icon_description: &'static str,
  pub keys_back_name: &'static str,
  pub keys_back_description: &'static str,
  pub keys_move_up_name: &'static str,
  pub keys_move_up_description: &'static str,
  pub keys_move_down_name: &'static str,
  pub keys_move_down_description: &'static str,
  pub keys_move_left_name: &'static str,
  pub keys_move_left_description: &'static str,
  pub keys_move_right_name: &'static str,
  pub keys_move_right_description: &'static str,
  pub keys_next_page_name: &'static str,
  pub keys_next_page_description: &'static str,
  pub keys_previous_page_name: &'static str,
  pub keys_previous_page_description: &'static str,
  pub keys_jump_to_start_name: &'static str,
  pub keys_jump_to_start_description: &'static str,
  pub keys_jump_to_end_name: &'static str,
  pub keys_jump_to_end_description: &'static str,
  pub keys_toggle_playback_name: &'static str,
  pub keys_toggle_playback_description: &'static str,
  pub keys_seek_backwards_name: &'static str,
  pub keys_seek_backwards_description: &'static str,
  pub keys_seek_forwards_name: &'static str,
  pub keys_seek_forwards_description: &'static str,
  pub keys_next_track_name: &'static str,
  pub keys_next_track_description: &'static str,
  pub keys_previous_track_name: &'static str,
  pub keys_previous_track_description: &'static str,
  pub keys_force_previous_track_name: &'static str,
  pub keys_force_previous_track_description: &'static str,
  pub keys_shuffle_name: &'static str,
  pub keys_shuffle_description: &'static str,
  pub keys_repeat_name: &'static str,
  pub keys_repeat_description: &'static str,
  pub keys_search_name: &'static str,
  pub keys_search_description: &'static str,
  pub keys_help_name: &'static str,
  pub keys_help_description: &'static str,
  pub keys_open_settings_name: &'static str,
  pub keys_open_settings_description: &'static str,
  pub keys_save_settings_name: &'static str,
  pub keys_save_settings_description: &'static str,
  pub keys_jump_to_album_name: &'static str,
  pub keys_jump_to_album_description: &'static str,
  pub keys_jump_to_artist_album_name: &'static str,
  pub keys_jump_to_artist_album_description: &'static str,
  pub keys_jump_to_context_name: &'static str,
  pub keys_jump_to_context_description: &'static str,
  pub keys_manage_devices_name: &'static str,
  pub keys_manage_devices_description: &'static str,
  pub keys_listening_party_name: &'static str,
  pub keys_listening_party_description: &'static str,
  pub keys_decrease_volume_name: &'static str,
  pub keys_decrease_volume_description: &'static str,
  pub keys_increase_volume_name: &'static str,
  pub keys_increase_volume_description: &'static str,
  pub keys_add_item_to_queue_name: &'static str,
  pub keys_add_item_to_queue_description: &'static str,
  pub keys_show_queue_name: &'static str,
  pub keys_show_queue_description: &'static str,
  pub keys_remove_from_queue_name: &'static str,
  pub keys_remove_from_queue_description: &'static str,
  pub keys_like_track_name: &'static str,
  pub keys_like_track_description: &'static str,
  pub keys_generate_recap_name: &'static str,
  pub keys_generate_recap_description: &'static str,
  pub keys_copy_song_url_name: &'static str,
  pub keys_copy_song_url_description: &'static str,
  pub keys_copy_album_url_name: &'static str,
  pub keys_copy_album_url_description: &'static str,
  pub keys_audio_analysis_name: &'static str,
  pub keys_audio_analysis_description: &'static str,
  pub keys_lyrics_view_name: &'static str,
  pub keys_lyrics_view_description: &'static str,
  pub keys_miniplayer_view_name: &'static str,
  pub keys_miniplayer_view_description: &'static str,
  pub keys_cover_art_view_name: &'static str,
  pub keys_cover_art_view_description: &'static str,
  pub theme_preset_name: &'static str,
  pub theme_preset_description: &'static str,
  pub theme_active_name: &'static str,
  pub theme_active_description: &'static str,
  pub theme_banner_name: &'static str,
  pub theme_banner_description: &'static str,
  pub behavior_banner_gradient_name: &'static str,
  pub behavior_banner_gradient_description: &'static str,
  pub behavior_cover_art_theme_name: &'static str,
  pub behavior_cover_art_theme_description: &'static str,
  pub theme_hint_name: &'static str,
  pub theme_hint_description: &'static str,
  pub theme_hovered_name: &'static str,
  pub theme_hovered_description: &'static str,
  pub theme_selected_name: &'static str,
  pub theme_selected_description: &'static str,
  pub theme_inactive_name: &'static str,
  pub theme_inactive_description: &'static str,
  pub theme_text_name: &'static str,
  pub theme_text_description: &'static str,
  pub theme_error_text_name: &'static str,
  pub theme_error_text_description: &'static str,
  pub theme_error_border_name: &'static str,
  pub theme_error_border_description: &'static str,
  pub theme_playbar_background_name: &'static str,
  pub theme_playbar_background_description: &'static str,
  pub theme_playbar_progress_name: &'static str,
  pub theme_playbar_progress_description: &'static str,
  pub theme_cover_art_dither_color_name: &'static str,
  pub theme_cover_art_dither_color_description: &'static str,
  pub theme_playbar_progress_text_name: &'static str,
  pub theme_playbar_progress_text_description: &'static str,
  pub theme_playbar_text_name: &'static str,
  pub theme_playbar_text_description: &'static str,
  pub theme_highlighted_lyrics_name: &'static str,
  pub theme_highlighted_lyrics_description: &'static str,
  pub theme_background_name: &'static str,
  pub theme_background_description: &'static str,
  pub theme_header_name: &'static str,
  pub theme_header_description: &'static str,
  pub category_behavior: &'static str,
  pub category_icons: &'static str,
  pub category_keybindings: &'static str,
  pub category_theme: &'static str,
  pub on: &'static str,
  pub off: &'static str,
  pub bool_edit_on: &'static str,
  pub bool_edit_off: &'static str,
  pub bool_on: &'static str,
  pub bool_off: &'static str,
  pub tabs_title: &'static str,
  pub effective_key_template: &'static str,
  pub filtered_count_template: &'static str,
  pub item_count_template: &'static str,
  pub category_title_template: &'static str,
  pub no_matches_template: &'static str,
  pub enhancement_on: &'static str,
  pub enhancement_available: &'static str,
  pub enhancement_off: &'static str,
  pub ctrl_comma_ok: &'static str,
  pub ctrl_comma_degraded: &'static str,
  pub ctrl_comma_unknown: &'static str,
  pub terminal_input_template: &'static str,
  pub filter_controls: &'static str,
  pub bool_controls: &'static str,
  pub number_controls: &'static str,
  pub key_controls: &'static str,
  pub cycle_controls: &'static str,
  pub text_controls: &'static str,
  pub navigation_controls_template: &'static str,
  pub filter_edit_template: &'static str,
  pub filter_applied_template: &'static str,
  pub controls_title: &'static str,
  pub unsaved_title: &'static str,
  pub unsaved_message: &'static str,
  pub yes_button: &'static str,
  pub no_button: &'static str,
  pub unsaved_controls: &'static str,
  pub save_failed_template: &'static str,
  pub icon_width_template: &'static str,
  pub invalid_delay_template: &'static str,
  pub invalid_color_template: &'static str,
  pub reserved_key_template: &'static str,
  pub key_conflict_template: &'static str,
  pub option_auto: &'static str,
  pub option_auto_color: &'static str,
  pub option_en: &'static str,
  pub option_zh_hans: &'static str,
  pub option_zh_hant: &'static str,
  pub option_continue: &'static str,
  pub option_play: &'static str,
  pub option_pause: &'static str,
  pub option_home: &'static str,
  pub option_recently_played: &'static str,
  pub option_podcasts: &'static str,
  pub option_discover: &'static str,
  pub option_artists: &'static str,
  pub option_album_list: &'static str,
  pub option_stats: &'static str,
  pub option_default: &'static str,
  pub option_name: &'static str,
  pub option_name_desc: &'static str,
  pub option_date_added: &'static str,
  pub option_date_added_desc: &'static str,
  pub option_artist: &'static str,
  pub option_artist_desc: &'static str,
  pub option_album: &'static str,
  pub option_album_desc: &'static str,
  pub option_duration: &'static str,
  pub option_duration_desc: &'static str,
  pub option_release_date: &'static str,
  pub option_release_date_desc: &'static str,
  pub option_left: &'static str,
  pub option_right: &'static str,
  pub option_hidden: &'static str,
  pub option_bottom: &'static str,
  pub option_top: &'static str,
  pub option_spectrum: &'static str,
  pub option_waveform: &'static str,
  pub option_custom: &'static str,
  pub delay_days: &'static str,
  pub delay_hours: &'static str,
  pub delay_minutes: &'static str,
  pub delay_seconds: &'static str,
  pub delay_number: &'static str,
  pub color_unknown_template: &'static str,
  pub color_empty: &'static str,
  pub color_invalid_digit: &'static str,
  pub color_too_large: &'static str,
  pub color_too_small: &'static str,
  pub option_preset_default: &'static str,
  pub option_preset_terminal: &'static str,
  pub color_reset: &'static str,
  pub color_black: &'static str,
  pub color_red: &'static str,
  pub color_green: &'static str,
  pub color_yellow: &'static str,
  pub color_blue: &'static str,
  pub color_magenta: &'static str,
  pub color_cyan: &'static str,
  pub color_gray: &'static str,
  pub color_dark_gray: &'static str,
  pub color_light_red: &'static str,
  pub color_light_green: &'static str,
  pub color_light_yellow: &'static str,
  pub color_light_blue: &'static str,
  pub color_light_magenta: &'static str,
  pub color_light_cyan: &'static str,
  pub color_white: &'static str,
}

impl SettingsStrings {
  /// Semantic row ids resolve to display copy, never to persisted values.
  pub fn row_text(&self, id: &str) -> Option<(&'static str, &'static str)> {
    Some(match id {
      "behavior.ytdlp_path" => (
        self.behavior_ytdlp_path_name,
        self.behavior_ytdlp_path_description,
      ),
      "behavior.playback_engine" => (
        self.behavior_playback_engine_name,
        self.behavior_playback_engine_description,
      ),
      "behavior.soloist_path" => (
        self.behavior_soloist_path_name,
        self.behavior_soloist_path_description,
      ),
      "behavior.soloist_audio_quality" => (
        self.behavior_soloist_audio_quality_name,
        self.behavior_soloist_audio_quality_description,
      ),
      "behavior.local_music_path" => (
        self.behavior_local_music_path_name,
        self.behavior_local_music_path_description,
      ),
      "behavior.subsonic_password" => (
        self.behavior_subsonic_password_name,
        self.behavior_subsonic_password_description,
      ),
      "behavior.subsonic_username" => (
        self.behavior_subsonic_username_name,
        self.behavior_subsonic_username_description,
      ),
      "behavior.subsonic_url" => (
        self.behavior_subsonic_url_name,
        self.behavior_subsonic_url_description,
      ),
      "behavior.language" => (
        self.behavior_language_name,
        self.behavior_language_description,
      ),
      "behavior.seek_milliseconds" => (
        self.behavior_seek_milliseconds_name,
        self.behavior_seek_milliseconds_description,
      ),
      "behavior.volume_increment" => (
        self.behavior_volume_increment_name,
        self.behavior_volume_increment_description,
      ),
      "behavior.tick_rate_milliseconds" => (
        self.behavior_tick_rate_milliseconds_name,
        self.behavior_tick_rate_milliseconds_description,
      ),
      "behavior.animation_tick_rate_milliseconds" => (
        self.behavior_animation_tick_rate_milliseconds_name,
        self.behavior_animation_tick_rate_milliseconds_description,
      ),
      "behavior.status_message_ttl_percent" => (
        self.behavior_status_message_ttl_percent_name,
        self.behavior_status_message_ttl_percent_description,
      ),
      "behavior.qobuz_quality" => (
        self.behavior_qobuz_quality_name,
        self.behavior_qobuz_quality_description,
      ),
      "behavior.playback_poll_seconds" => (
        self.behavior_playback_poll_seconds_name,
        self.behavior_playback_poll_seconds_description,
      ),
      "behavior.table_scroll_padding" => (
        self.behavior_table_scroll_padding_name,
        self.behavior_table_scroll_padding_description,
      ),
      "behavior.like_animation_frames" => (
        self.behavior_like_animation_frames_name,
        self.behavior_like_animation_frames_description,
      ),
      "behavior.enable_text_emphasis" => (
        self.behavior_enable_text_emphasis_name,
        self.behavior_enable_text_emphasis_description,
      ),
      "behavior.show_loading_indicator" => (
        self.behavior_show_loading_indicator_name,
        self.behavior_show_loading_indicator_description,
      ),
      "behavior.enforce_wide_search_bar" => (
        self.behavior_enforce_wide_search_bar_name,
        self.behavior_enforce_wide_search_bar_description,
      ),
      "behavior.group_folders_first" => (
        self.behavior_group_folders_first_name,
        self.behavior_group_folders_first_description,
      ),
      "behavior.disable_mouse_inputs" => (
        self.behavior_disable_mouse_inputs_name,
        self.behavior_disable_mouse_inputs_description,
      ),
      "behavior.set_window_title" => (
        self.behavior_set_window_title_name,
        self.behavior_set_window_title_description,
      ),
      "behavior.enable_discord_rpc" => (
        self.behavior_enable_discord_rpc_name,
        self.behavior_enable_discord_rpc_description,
      ),
      "behavior.stop_after_current_track" => (
        self.behavior_stop_after_current_track_name,
        self.behavior_stop_after_current_track_description,
      ),
      "behavior.keepawake_enabled" => (
        self.behavior_keepawake_enabled_name,
        self.behavior_keepawake_enabled_description,
      ),
      "behavior.enable_media_keys" => (
        self.behavior_enable_media_keys_name,
        self.behavior_enable_media_keys_description,
      ),
      "behavior.enable_streaming" => (
        self.behavior_enable_streaming_name,
        self.behavior_enable_streaming_description,
      ),
      "behavior.streaming_device_name" => (
        self.behavior_streaming_device_name_name,
        self.behavior_streaming_device_name_description,
      ),
      "behavior.streaming_bitrate" => (
        self.behavior_streaming_bitrate_name,
        self.behavior_streaming_bitrate_description,
      ),
      "behavior.streaming_audio_cache" => (
        self.behavior_streaming_audio_cache_name,
        self.behavior_streaming_audio_cache_description,
      ),
      "behavior.streaming_autoplay" => (
        self.behavior_streaming_autoplay_name,
        self.behavior_streaming_autoplay_description,
      ),
      "behavior.startup_behavior" => (
        self.behavior_startup_behavior_name,
        self.behavior_startup_behavior_description,
      ),
      "behavior.player_bar_visualizer" => (
        self.behavior_player_bar_visualizer_name,
        self.behavior_player_bar_visualizer_description,
      ),
      "behavior.startup_route" => (
        self.behavior_startup_route_name,
        self.behavior_startup_route_description,
      ),
      "behavior.default_sort_playlist_tracks" => (
        self.behavior_default_sort_playlist_tracks_name,
        self.behavior_default_sort_playlist_tracks_description,
      ),
      "behavior.default_sort_saved_albums" => (
        self.behavior_default_sort_saved_albums_name,
        self.behavior_default_sort_saved_albums_description,
      ),
      "behavior.default_sort_saved_artists" => (
        self.behavior_default_sort_saved_artists_name,
        self.behavior_default_sort_saved_artists_description,
      ),
      "behavior.default_sort_recently_played" => (
        self.behavior_default_sort_recently_played_name,
        self.behavior_default_sort_recently_played_description,
      ),
      "behavior.sidebar_position" => (
        self.behavior_sidebar_position_name,
        self.behavior_sidebar_position_description,
      ),
      "behavior.playbar_position" => (
        self.behavior_playbar_position_name,
        self.behavior_playbar_position_description,
      ),
      "behavior.small_terminal_width" => (
        self.behavior_small_terminal_width_name,
        self.behavior_small_terminal_width_description,
      ),
      "behavior.small_terminal_height" => (
        self.behavior_small_terminal_height_name,
        self.behavior_small_terminal_height_description,
      ),
      "behavior.enable_announcements" => (
        self.behavior_enable_announcements_name,
        self.behavior_enable_announcements_description,
      ),
      "behavior.enable_lyrics" => (
        self.behavior_enable_lyrics_name,
        self.behavior_enable_lyrics_description,
      ),
      "behavior.enable_monthly_recap_prompt" => (
        self.behavior_enable_monthly_recap_prompt_name,
        self.behavior_enable_monthly_recap_prompt_description,
      ),
      "behavior.pin_community_playlist" => (
        self.behavior_pin_community_playlist_name,
        self.behavior_pin_community_playlist_description,
      ),
      "behavior.enable_global_song_count" => (
        self.behavior_enable_global_song_count_name,
        self.behavior_enable_global_song_count_description,
      ),
      "behavior.disable_auto_update" => (
        self.behavior_disable_auto_update_name,
        self.behavior_disable_auto_update_description,
      ),
      "behavior.auto_update_delay" => (
        self.behavior_auto_update_delay_name,
        self.behavior_auto_update_delay_description,
      ),
      "behavior.announcement_feed_url" => (
        self.behavior_announcement_feed_url_name,
        self.behavior_announcement_feed_url_description,
      ),
      "behavior.sync_token" => (
        self.behavior_sync_token_name,
        self.behavior_sync_token_description,
      ),
      "behavior.draw_cover_art" => (
        self.behavior_draw_cover_art_name,
        self.behavior_draw_cover_art_description,
      ),
      "behavior.draw_cover_art_forced" => (
        self.behavior_draw_cover_art_forced_name,
        self.behavior_draw_cover_art_forced_description,
      ),
      "behavior.playbar_cover_art_size_percent" => (
        self.behavior_playbar_cover_art_size_percent_name,
        self.behavior_playbar_cover_art_size_percent_description,
      ),
      "behavior.cover_art_dither" => (
        self.behavior_cover_art_dither_name,
        self.behavior_cover_art_dither_description,
      ),
      "behavior.cover_art_dither_algorithm" => (
        self.behavior_cover_art_dither_algorithm_name,
        self.behavior_cover_art_dither_algorithm_description,
      ),
      "behavior.cover_art_dither_pixel_scale" => (
        self.behavior_cover_art_dither_pixel_scale_name,
        self.behavior_cover_art_dither_pixel_scale_description,
      ),
      "behavior.liked_icon" => (
        self.behavior_liked_icon_name,
        self.behavior_liked_icon_description,
      ),
      "behavior.shuffle_icon" => (
        self.behavior_shuffle_icon_name,
        self.behavior_shuffle_icon_description,
      ),
      "behavior.repeat_track_icon" => (
        self.behavior_repeat_track_icon_name,
        self.behavior_repeat_track_icon_description,
      ),
      "behavior.repeat_context_icon" => (
        self.behavior_repeat_context_icon_name,
        self.behavior_repeat_context_icon_description,
      ),
      "behavior.playing_icon" => (
        self.behavior_playing_icon_name,
        self.behavior_playing_icon_description,
      ),
      "behavior.paused_icon" => (
        self.behavior_paused_icon_name,
        self.behavior_paused_icon_description,
      ),
      "behavior.gauge_filled_icon" => (
        self.behavior_gauge_filled_icon_name,
        self.behavior_gauge_filled_icon_description,
      ),
      "behavior.gauge_unfilled_icon" => (
        self.behavior_gauge_unfilled_icon_name,
        self.behavior_gauge_unfilled_icon_description,
      ),
      "behavior.active_source_icon" => (
        self.behavior_active_source_icon_name,
        self.behavior_active_source_icon_description,
      ),
      "behavior.episode_played_icon" => (
        self.behavior_episode_played_icon_name,
        self.behavior_episode_played_icon_description,
      ),
      "behavior.sort_ascending_icon" => (
        self.behavior_sort_ascending_icon_name,
        self.behavior_sort_ascending_icon_description,
      ),
      "behavior.sort_descending_icon" => (
        self.behavior_sort_descending_icon_name,
        self.behavior_sort_descending_icon_description,
      ),
      "behavior.list_highlight_icon" => (
        self.behavior_list_highlight_icon_name,
        self.behavior_list_highlight_icon_description,
      ),
      "keys.back" => (self.keys_back_name, self.keys_back_description),
      "keys.move_up" => (self.keys_move_up_name, self.keys_move_up_description),
      "keys.move_down" => (self.keys_move_down_name, self.keys_move_down_description),
      "keys.move_left" => (self.keys_move_left_name, self.keys_move_left_description),
      "keys.move_right" => (self.keys_move_right_name, self.keys_move_right_description),
      "keys.next_page" => (self.keys_next_page_name, self.keys_next_page_description),
      "keys.previous_page" => (
        self.keys_previous_page_name,
        self.keys_previous_page_description,
      ),
      "keys.jump_to_start" => (
        self.keys_jump_to_start_name,
        self.keys_jump_to_start_description,
      ),
      "keys.jump_to_end" => (
        self.keys_jump_to_end_name,
        self.keys_jump_to_end_description,
      ),
      "keys.toggle_playback" => (
        self.keys_toggle_playback_name,
        self.keys_toggle_playback_description,
      ),
      "keys.seek_backwards" => (
        self.keys_seek_backwards_name,
        self.keys_seek_backwards_description,
      ),
      "keys.seek_forwards" => (
        self.keys_seek_forwards_name,
        self.keys_seek_forwards_description,
      ),
      "keys.next_track" => (self.keys_next_track_name, self.keys_next_track_description),
      "keys.previous_track" => (
        self.keys_previous_track_name,
        self.keys_previous_track_description,
      ),
      "keys.force_previous_track" => (
        self.keys_force_previous_track_name,
        self.keys_force_previous_track_description,
      ),
      "keys.shuffle" => (self.keys_shuffle_name, self.keys_shuffle_description),
      "keys.repeat" => (self.keys_repeat_name, self.keys_repeat_description),
      "keys.search" => (self.keys_search_name, self.keys_search_description),
      "keys.help" => (self.keys_help_name, self.keys_help_description),
      "keys.open_settings" => (
        self.keys_open_settings_name,
        self.keys_open_settings_description,
      ),
      "keys.save_settings" => (
        self.keys_save_settings_name,
        self.keys_save_settings_description,
      ),
      "keys.jump_to_album" => (
        self.keys_jump_to_album_name,
        self.keys_jump_to_album_description,
      ),
      "keys.jump_to_artist_album" => (
        self.keys_jump_to_artist_album_name,
        self.keys_jump_to_artist_album_description,
      ),
      "keys.jump_to_context" => (
        self.keys_jump_to_context_name,
        self.keys_jump_to_context_description,
      ),
      "keys.manage_devices" => (
        self.keys_manage_devices_name,
        self.keys_manage_devices_description,
      ),
      "keys.listening_party" => (
        self.keys_listening_party_name,
        self.keys_listening_party_description,
      ),
      "keys.decrease_volume" => (
        self.keys_decrease_volume_name,
        self.keys_decrease_volume_description,
      ),
      "keys.increase_volume" => (
        self.keys_increase_volume_name,
        self.keys_increase_volume_description,
      ),
      "keys.add_item_to_queue" => (
        self.keys_add_item_to_queue_name,
        self.keys_add_item_to_queue_description,
      ),
      "keys.show_queue" => (self.keys_show_queue_name, self.keys_show_queue_description),
      "keys.remove_from_queue" => (
        self.keys_remove_from_queue_name,
        self.keys_remove_from_queue_description,
      ),
      "keys.like_track" => (self.keys_like_track_name, self.keys_like_track_description),
      "keys.generate_recap" => (
        self.keys_generate_recap_name,
        self.keys_generate_recap_description,
      ),
      "keys.copy_song_url" => (
        self.keys_copy_song_url_name,
        self.keys_copy_song_url_description,
      ),
      "keys.copy_album_url" => (
        self.keys_copy_album_url_name,
        self.keys_copy_album_url_description,
      ),
      "keys.audio_analysis" => (
        self.keys_audio_analysis_name,
        self.keys_audio_analysis_description,
      ),
      "keys.lyrics_view" => (
        self.keys_lyrics_view_name,
        self.keys_lyrics_view_description,
      ),
      "keys.miniplayer_view" => (
        self.keys_miniplayer_view_name,
        self.keys_miniplayer_view_description,
      ),
      "keys.cover_art_view" => (
        self.keys_cover_art_view_name,
        self.keys_cover_art_view_description,
      ),
      "theme.preset" => (self.theme_preset_name, self.theme_preset_description),
      "theme.active" => (self.theme_active_name, self.theme_active_description),
      "theme.banner" => (self.theme_banner_name, self.theme_banner_description),
      "behavior.banner_gradient" => (
        self.behavior_banner_gradient_name,
        self.behavior_banner_gradient_description,
      ),
      "behavior.cover_art_theme" => (
        self.behavior_cover_art_theme_name,
        self.behavior_cover_art_theme_description,
      ),
      "theme.hint" => (self.theme_hint_name, self.theme_hint_description),
      "theme.hovered" => (self.theme_hovered_name, self.theme_hovered_description),
      "theme.selected" => (self.theme_selected_name, self.theme_selected_description),
      "theme.inactive" => (self.theme_inactive_name, self.theme_inactive_description),
      "theme.text" => (self.theme_text_name, self.theme_text_description),
      "theme.error_text" => (
        self.theme_error_text_name,
        self.theme_error_text_description,
      ),
      "theme.error_border" => (
        self.theme_error_border_name,
        self.theme_error_border_description,
      ),
      "theme.playbar_background" => (
        self.theme_playbar_background_name,
        self.theme_playbar_background_description,
      ),
      "theme.playbar_progress" => (
        self.theme_playbar_progress_name,
        self.theme_playbar_progress_description,
      ),
      "theme.cover_art_dither_color" => (
        self.theme_cover_art_dither_color_name,
        self.theme_cover_art_dither_color_description,
      ),
      "theme.playbar_progress_text" => (
        self.theme_playbar_progress_text_name,
        self.theme_playbar_progress_text_description,
      ),
      "theme.playbar_text" => (
        self.theme_playbar_text_name,
        self.theme_playbar_text_description,
      ),
      "theme.highlighted_lyrics" => (
        self.theme_highlighted_lyrics_name,
        self.theme_highlighted_lyrics_description,
      ),
      "theme.background" => (
        self.theme_background_name,
        self.theme_background_description,
      ),
      "theme.header" => (self.theme_header_name, self.theme_header_description),
      _ => return None,
    })
  }

  /// Display a choice without changing its stored or wire value.
  pub fn option_label<'a>(&self, id: &str, raw: &'a str) -> &'a str {
    match (id, raw) {
      ("behavior.language", "auto") => self.option_auto,
      ("behavior.language", "en") => self.option_en,
      ("behavior.language", "zh-Hans") => self.option_zh_hans,
      ("behavior.language", "zh-Hant") => self.option_zh_hant,
      ("behavior.startup_behavior", "Continue") => self.option_continue,
      ("behavior.startup_behavior", "Play") => self.option_play,
      ("behavior.startup_behavior", "Pause") => self.option_pause,
      ("behavior.startup_route", "home") => self.option_home,
      ("behavior.startup_route", "recently_played") => self.option_recently_played,
      ("behavior.startup_route", "podcasts") => self.option_podcasts,
      ("behavior.startup_route", "discover") => self.option_discover,
      ("behavior.startup_route", "artists") => self.option_artists,
      ("behavior.startup_route", "album_list") => self.option_album_list,
      ("behavior.startup_route", "stats") => self.option_stats,
      ("behavior.sidebar_position", "left") => self.option_left,
      ("behavior.sidebar_position", "right") => self.option_right,
      ("behavior.sidebar_position", "hidden") => self.option_hidden,
      ("behavior.playbar_position", "bottom") => self.option_bottom,
      ("behavior.playbar_position", "top") => self.option_top,
      ("behavior.player_bar_visualizer", "Off") => self.off,
      ("behavior.player_bar_visualizer", "Spectrum") => self.option_spectrum,
      ("behavior.player_bar_visualizer", "Waveform") => self.option_waveform,
      ("theme.preset", "Custom") => self.option_custom,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "default",
      ) => self.option_default,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "name",
      ) => self.option_name,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "name:desc",
      ) => self.option_name_desc,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "date_added",
      ) => self.option_date_added,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "date_added:desc",
      ) => self.option_date_added_desc,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "artist",
      ) => self.option_artist,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "artist:desc",
      ) => self.option_artist_desc,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "album",
      ) => self.option_album,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "album:desc",
      ) => self.option_album_desc,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "duration",
      ) => self.option_duration,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "duration:desc",
      ) => self.option_duration_desc,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "release_date",
      ) => self.option_release_date,
      (
        "behavior.default_sort_playlist_tracks"
        | "behavior.default_sort_saved_albums"
        | "behavior.default_sort_saved_artists"
        | "behavior.default_sort_recently_played",
        "release_date:desc",
      ) => self.option_release_date_desc,
      ("theme.preset", "Default (Cyan)") => self.option_preset_default,
      ("theme.preset", "Terminal (ANSI)") => self.option_preset_terminal,
      ("theme.cover_art_dither_color", "auto") => self.option_auto_color,
      (id, "Reset") if id.starts_with("theme.") => self.color_reset,
      (id, "Black") if id.starts_with("theme.") => self.color_black,
      (id, "Red") if id.starts_with("theme.") => self.color_red,
      (id, "Green") if id.starts_with("theme.") => self.color_green,
      (id, "Yellow") if id.starts_with("theme.") => self.color_yellow,
      (id, "Blue") if id.starts_with("theme.") => self.color_blue,
      (id, "Magenta") if id.starts_with("theme.") => self.color_magenta,
      (id, "Cyan") if id.starts_with("theme.") => self.color_cyan,
      (id, "Gray") if id.starts_with("theme.") => self.color_gray,
      (id, "DarkGray") if id.starts_with("theme.") => self.color_dark_gray,
      (id, "LightRed") if id.starts_with("theme.") => self.color_light_red,
      (id, "LightGreen") if id.starts_with("theme.") => self.color_light_green,
      (id, "LightYellow") if id.starts_with("theme.") => self.color_light_yellow,
      (id, "LightBlue") if id.starts_with("theme.") => self.color_light_blue,
      (id, "LightMagenta") if id.starts_with("theme.") => self.color_light_magenta,
      (id, "LightCyan") if id.starts_with("theme.") => self.color_light_cyan,
      (id, "White") if id.starts_with("theme.") => self.color_white,
      _ => raw,
    }
  }

  pub fn effective_key(&self, configured: &str, effective: &str) -> String {
    super::format_template(
      self.effective_key_template,
      &[("configured", configured), ("effective", effective)],
    )
  }

  pub fn filtered_count(&self, visible: usize, total: usize) -> String {
    super::format_template(
      self.filtered_count_template,
      &[
        ("visible", &visible.to_string()),
        ("total", &total.to_string()),
      ],
    )
  }

  pub fn item_count(&self, count: usize) -> String {
    super::format_template(self.item_count_template, &[("count", &count.to_string())])
  }

  pub fn category_title(&self, category: &str, count: &str) -> String {
    super::format_template(
      self.category_title_template,
      &[("category", category), ("count", count)],
    )
  }

  pub fn no_matches(&self, category: &str) -> String {
    super::format_template(self.no_matches_template, &[("category", category)])
  }

  pub fn terminal_input(&self, enhancement: &str, ctrl_comma: &str) -> String {
    super::format_template(
      self.terminal_input_template,
      &[("enhancement", enhancement), ("ctrl_comma", ctrl_comma)],
    )
  }

  pub fn navigation_controls(&self, search: &str, save: &str, back: &str) -> String {
    super::format_template(
      self.navigation_controls_template,
      &[("search", search), ("save", save), ("back", back)],
    )
  }

  pub fn filter_edit(&self, query: &str) -> String {
    super::format_template(self.filter_edit_template, &[("query", query)])
  }

  pub fn filter_applied(&self, query: &str) -> String {
    super::format_template(self.filter_applied_template, &[("query", query)])
  }

  pub fn save_failed(&self, error: &str) -> String {
    super::format_template(self.save_failed_template, &[("error", error)])
  }

  pub fn icon_width(&self, name: &str) -> String {
    super::format_template(self.icon_width_template, &[("name", name)])
  }

  #[cfg(any(feature = "self-update", test))]
  pub fn invalid_delay(&self, error: &str) -> String {
    super::format_template(self.invalid_delay_template, &[("error", error)])
  }

  pub fn invalid_color(&self, error: &str) -> String {
    super::format_template(self.invalid_color_template, &[("error", error)])
  }

  pub fn color_unknown(&self, value: &str) -> String {
    super::format_template(self.color_unknown_template, &[("value", value)])
  }

  pub fn reserved_key(&self, key: &str) -> String {
    super::format_template(self.reserved_key_template, &[("key", key)])
  }

  pub fn key_conflict(&self, key: &str, action: &str) -> String {
    super::format_template(
      self.key_conflict_template,
      &[("key", key), ("action", action)],
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn settings_choice_labels_preserve_raw_tokens_and_unknown_values() {
    let options = crate::core::i18n::Language::OPTIONS;
    assert_eq!(options, &["auto", "en", "zh-Hans", "zh-Hant"]);
    assert_eq!(
      EN.option_label("behavior.language", "auto"),
      "Auto (system)"
    );
    assert_eq!(
      EN.option_label("theme.cover_art_dither_color", "auto"),
      "auto"
    );
    for strings in [&EN, &ZH_HANS, &ZH_HANT] {
      assert_eq!(
        strings.option_label("behavior.language", "auto"),
        strings.option_auto
      );
      assert_eq!(
        strings.option_label("theme.cover_art_dither_color", "auto"),
        strings.option_auto_color
      );
    }
    assert_eq!(EN.option_label("behavior.language", "zh-Hant"), "繁體中文");
    assert_eq!(
      ZH_HANS.option_label("behavior.language", "auto"),
      "自动（跟随系统）"
    );
    assert_eq!(
      ZH_HANT.option_label("behavior.language", "auto"),
      "自動（跟隨系統）"
    );
    assert_eq!(
      ZH_HANT.option_label("behavior.default_sort_playlist_tracks", "date_added:desc"),
      "加入日期（遞減）"
    );
    assert_eq!(
      ZH_HANS.option_label("behavior.streaming_device_name", "default"),
      "default"
    );
    assert_eq!(
      ZH_HANS.option_label("behavior.startup_route", "{name}"),
      "{name}"
    );
    assert_eq!(
      EN.option_label("theme.preset", "Default (Cyan)"),
      "Default (Cyan)"
    );
    assert_eq!(ZH_HANS.option_label("theme.preset", "Nord"), "Nord");
  }

  #[test]
  fn settings_templates_preserve_literal_user_values() {
    assert_eq!(
      EN.key_conflict("{action}", "{key}"),
      "Key {action} is already assigned to {key}"
    );
    assert_eq!(
      ZH_HANS.key_conflict("{action}", "{key}"),
      "按键 {action} 已分配给{key}"
    );
    assert_eq!(
      ZH_HANT.color_unknown("\"{value}\""),
      "未知顏色 \"{value}\"：請使用 LightBlue 等名稱或 \"r, g, b\""
    );
  }
}
