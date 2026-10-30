//! User-visible infrastructure status messages. Raw diagnostics remain untranslated.

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct InfraStatusStrings {
  pub sync_spotify_not_connected: &'static str,
  pub sync_qobuz_not_logged_in: &'static str,
  pub sync_subsonic_not_configured: &'static str,
  pub sync_source_not_compiled: &'static str,
  pub sync_source_unsupported: &'static str,
  pub sync_source_unreachable: &'static str,
  pub search_query_unavailable: &'static str,
  pub made_for_you_unavailable: &'static str,
  pub made_for_you_missing: &'static str,
  pub history_track_hit: &'static str,
  pub history_play: &'static str,
  pub local_replay_failed: &'static str,
  pub local_no_tracks: &'static str,
  pub local_no_folder: &'static str,
  pub local_uri_invalid: &'static str,
  pub local_play_failed: &'static str,
  pub local_task_failed: &'static str,
  pub local_audio_unavailable: &'static str,
  pub audio_init_failed: &'static str,
  pub local_scan_failed: &'static str,
  pub local_scan_task_failed: &'static str,
  pub local_read_failed: &'static str,
  pub local_tracks_scan_failed: &'static str,
  pub local_exhausted: &'static str,
  pub subsonic_audio_unavailable: &'static str,
  pub youtube_audio_unavailable: &'static str,
  pub radio_audio_unavailable: &'static str,
  pub qobuz_audio_unavailable: &'static str,
  pub subsonic_no_tracks: &'static str,
  pub subsonic_uri_invalid: &'static str,
  pub subsonic_replay_failed: &'static str,
  pub subsonic_playlists_failed: &'static str,
  pub subsonic_playlist_failed: &'static str,
  pub subsonic_search_failed: &'static str,
  pub subsonic_download_failed: &'static str,
  pub subsonic_play_failed: &'static str,
  pub subsonic_task_failed: &'static str,
  pub subsonic_no_server: &'static str,
  pub radio_no_skip: &'static str,
  pub radio_empty: &'static str,
  pub radio_uri_invalid: &'static str,
  pub radio_probe_timeout: &'static str,
  pub radio_search_failed: &'static str,
  pub radio_open_failed: &'static str,
  pub radio_play_failed: &'static str,
  pub radio_task_failed: &'static str,
  pub radio_playing: &'static str,
  pub youtube_playlist_missing: &'static str,
  pub youtube_metadata_missing: &'static str,
  pub youtube_no_tracks: &'static str,
  pub youtube_no_live: &'static str,
  pub youtube_uri_invalid: &'static str,
  pub youtube_replay_failed: &'static str,
  pub youtube_search_failed: &'static str,
  pub youtube_playlists_failed: &'static str,
  pub youtube_create_failed: &'static str,
  pub youtube_delete_failed: &'static str,
  pub youtube_add_failed: &'static str,
  pub youtube_remove_failed: &'static str,
  pub youtube_download_failed: &'static str,
  pub youtube_task_failed: &'static str,
  pub youtube_created: &'static str,
  pub youtube_deleted: &'static str,
  pub youtube_removed: &'static str,
  pub fetching: &'static str,
  pub youtube_added: &'static str,
  pub youtube_already_added: &'static str,
  pub youtube_decode_failed: &'static str,
  pub qobuz_login_expired: &'static str,
  pub qobuz_login_busy: &'static str,
  pub qobuz_browser_login: &'static str,
  pub qobuz_logged_in: &'static str,
  pub qobuz_no_tracks: &'static str,
  pub qobuz_uri_invalid: &'static str,
  pub qobuz_replay_failed: &'static str,
  pub qobuz_login_url: &'static str,
  pub qobuz_login_failed: &'static str,
  pub qobuz_step_constants: &'static str,
  pub qobuz_step_login: &'static str,
  pub qobuz_step_library: &'static str,
  pub qobuz_step_tracks: &'static str,
  pub qobuz_step_search: &'static str,
  pub qobuz_step_stream: &'static str,
  pub qobuz_operation_failed: &'static str,
  pub music_busy_playback: &'static str,
  pub music_wait_pause: &'static str,
  pub music_not_running: &'static str,
  pub music_shuffle_repeat: &'static str,
  pub music_switch_spotify: &'static str,
  pub music_busy: &'static str,
  pub music_queue_unsupported: &'static str,
  pub music_loading: &'static str,
  pub music_loaded: &'static str,
  pub music_status_unavailable: &'static str,
  pub queue_repeat_unsupported: &'static str,
  pub queue_shuffle_unsupported: &'static str,
  pub queue_no_uri: &'static str,
  pub queue_finished: &'static str,
  pub audio_disconnected: &'static str,
  pub radio_not_resumed: &'static str,
  pub source_playback_unavailable: &'static str,
  pub queue_skip_disconnected: &'static str,
  pub queue_play_failed: &'static str,
  pub queue_restore_failed: &'static str,
  pub queue_playing: &'static str,
  pub queue_audio_unavailable: &'static str,
  pub spotify_not_connected: &'static str,
  pub playlist_id_invalid: &'static str,
  pub music_requires_macos: &'static str,
  pub spotify_not_compiled: &'static str,
  pub spotify_connected: &'static str,
  pub party_disconnected: &'static str,
  pub party_ended: &'static str,
  pub spotify_login_timeout: &'static str,
  pub spotify_login_failed: &'static str,
  pub spotify_rate_wait: &'static str,
  pub spotify_pair: &'static str,
  pub recap_created: &'static str,
  pub history_failed: &'static str,
  pub recap_failed: &'static str,
  pub party_start_failed: &'static str,
  pub party_join_failed: &'static str,
  pub party_error: &'static str,
  pub party_joined: &'static str,
  pub party_left: &'static str,
  pub native_reconnecting: &'static str,
  pub native_settling: &'static str,
  pub shuffle_not_applied: &'static str,
  pub spotify_rate_retry: &'static str,
  pub spotify_poll_retry: &'static str,
  pub spotify_server_retry: &'static str,
  pub spotify_poll_rejected: &'static str,
  pub native_resuming: &'static str,
  pub native_reconnect_resume: &'static str,
  pub native_connection_restored: &'static str,
  pub native_no_state: &'static str,
  pub native_restore_retry: &'static str,
  pub other_source_playing: &'static str,
  pub spotify_playing_elsewhere: &'static str,
  pub queue_added: &'static str,
  pub queue_load_failed: &'static str,
  pub native_nothing_to_resume: &'static str,
  pub native_start_failed: &'static str,
  pub native_shuffle_failed: &'static str,
  pub native_resume_failed: &'static str,
  pub shuffle_context_failed: &'static str,
  pub shuffle_capped: &'static str,
  pub spotify_profile_retry: &'static str,
  pub friend_added: &'static str,
  pub friend_unfollowed: &'static str,
  pub friend_added_code: &'static str,
  pub friend_add_failed: &'static str,
  pub friend_unfollow_failed: &'static str,
  pub playlist_external_unavailable: &'static str,
  pub liked_check_failed: &'static str,
  pub playlist_search_scope: &'static str,
  pub playlist_search_matches: &'static str,
  pub playlist_refresh_partial: &'static str,
  pub playlist_added: &'static str,
  pub playlist_removed: &'static str,
  pub playlist_created_no_id: &'static str,
  pub playlist_added_named: &'static str,
  pub playlist_created: &'static str,
  pub native_recovered: &'static str,
  pub native_lost_reconnecting: &'static str,
  pub native_reconnected: &'static str,
  pub native_restored: &'static str,
  pub native_restored_paused: &'static str,
  pub native_skip_unavailable: &'static str,
  pub native_many_unavailable: &'static str,
  pub soloist_path_unset: &'static str,
  pub soloist_missing: &'static str,
  pub soloist_not_executable: &'static str,
  pub soloist_wrong_cpu: &'static str,
  pub soloist_start_failed: &'static str,
  pub soloist_key_missing: &'static str,
  pub soloist_not_paired: &'static str,
  pub soloist_offline: &'static str,
  pub soloist_bad_key: &'static str,
  pub soloist_expired: &'static str,
  pub soloist_expires_soon: &'static str,
  pub soloist_unit_day: &'static str,
  pub soloist_unit_days: &'static str,
  pub soloist_unit_hour: &'static str,
  pub soloist_unit_hours: &'static str,
  pub soloist_unit_minute: &'static str,
  pub soloist_unit_minutes: &'static str,
  pub soloist_dir_in_use: &'static str,
  pub soloist_keeps_exiting: &'static str,
  pub soloist_no_websocket: &'static str,
  pub soloist_restarting: &'static str,
  pub soloist_restart_quality: &'static str,
  pub soloist_streams: &'static str,
  pub soloist_lossy_hint: &'static str,
  pub soloist_dropped: &'static str,
  pub soloist_engine_unavailable: &'static str,
  pub soloist_queue_refused: &'static str,
  pub soloist_paired: &'static str,
  pub engine_switching_soloist: &'static str,
  pub engine_switching_librespot: &'static str,
  pub engine_next_launch: &'static str,
  pub soloist_download_started: &'static str,
  pub soloist_downloading: &'static str,
  pub soloist_downloaded: &'static str,
  pub soloist_downloaded_engine_off: &'static str,
  pub soloist_downloaded_next_launch: &'static str,
  pub soloist_download_failed: &'static str,
  pub soloist_no_build: &'static str,
  pub soloist_key_saved: &'static str,
  pub soloist_key_restarting: &'static str,
  pub soloist_key_env_wins: &'static str,
  pub soloist_key_save_failed: &'static str,
  pub native_recovery_dropped: &'static str,
  pub native_recovery_failed: &'static str,
  pub native_lost_recovering: &'static str,
  pub native_stalled: &'static str,
  pub native_stopped: &'static str,
  pub native_moved: &'static str,
  pub native_disconnected: &'static str,
  pub native_stopped_recovering: &'static str,
  pub sync_busy: &'static str,
  pub sync_no_links: &'static str,
  pub sync_dry_run: &'static str,
  pub sync_removed: &'static str,
  pub sync_failed: &'static str,
  pub sync_link_failed: &'static str,
  pub sync_remove_failed: &'static str,
  pub sync_summary: &'static str,
  pub sync_using_playlist: &'static str,
  pub sync_progress: &'static str,
  pub dj_indexed: &'static str,
  pub dj_index_capped: &'static str,
  pub dj_queued: &'static str,
  pub dj_not_found: &'static str,
  pub dj_owned: &'static str,
  pub dj_duplicate: &'static str,
  pub dj_status: &'static str,
  pub dj_library_failed: &'static str,
  pub dj_topped_up: &'static str,
  pub dj_nothing_queued: &'static str,
  pub dj_error: &'static str,
  pub dj_steps_exhausted: &'static str,
  pub history_plays: &'static str,
  pub history_track_hits: &'static str,
  pub history_minutes: &'static str,
  pub history_hours_minutes: &'static str,
  pub setup_no_api_key: &'static str,
  pub setup_keep_command: &'static str,
  pub setup_anthropic: &'static str,
  pub setup_openai: &'static str,
  pub setup_own_command: &'static str,
  pub setup_not_installed: &'static str,
  pub setup_anthropic_billing: &'static str,
  pub setup_needs_key: &'static str,
  pub setup_openai_endpoint: &'static str,
  pub setup_claude_plan: &'static str,
  pub setup_codex_plan: &'static str,
  pub setup_agy_plan: &'static str,
  pub setup_copilot_plan: &'static str,
  pub setup_opencode_plan: &'static str,
  pub setup_gemini_legacy: &'static str,
  pub setup_agent_subscription: &'static str,
  pub setup_keep_as_is: &'static str,
  pub setup_own_model_note: &'static str,
  pub setup_current: &'static str,
  pub setup_cli_default: &'static str,
  pub setup_no_model_flag: &'static str,
  pub setup_builtin_default: &'static str,
  pub setup_custom: &'static str,
  pub setup_type_provider_model: &'static str,
  pub setup_type_model: &'static str,
  pub setup_haiku_note: &'static str,
  pub setup_sonnet_note: &'static str,
  pub setup_opus_note: &'static str,
  pub setup_fable_note: &'static str,
  pub setup_cheapest: &'static str,
  pub setup_heaviest_gemini: &'static str,
  pub setup_heaviest_all: &'static str,
  pub setup_price_haiku: &'static str,
  pub setup_price_sonnet: &'static str,
  pub setup_price_opus: &'static str,
  pub setup_price_fable: &'static str,
  pub history_week: &'static str,
  pub history_month: &'static str,
  pub history_all: &'static str,
  pub history_this_month: &'static str,
  pub history_this_year: &'static str,
  pub mcp_status: &'static str,
  pub network_error: &'static str,
  pub network_rate_error: &'static str,
  pub monthly_recap_ready: &'static str,
  pub queue_local_source: &'static str,
  pub unnamed_folder: &'static str,
  pub local_root_folder: &'static str,
  pub local_unknown_track: &'static str,
  pub cast_no_answer: &'static str,
  pub cast_no_tls: &'static str,
  pub cast_message_size: &'static str,
  pub cast_malformed: &'static str,
  pub cast_no_reply: &'static str,
  pub cast_receiver_failed: &'static str,
  pub cast_client_id_missing: &'static str,
  pub cast_login_refused: &'static str,
  pub cast_access_token_missing: &'static str,
  pub speaker_token_failed: &'static str,
  pub speaker_token_unsupported: &'static str,
  pub speaker_client_id_missing: &'static str,
  pub speaker_login_refused: &'static str,
  pub speaker_not_joined: &'static str,
  pub playlist_changed: &'static str,
  pub spotify_api_error: &'static str,
}

// Each helper's callers sit behind an optional source or frontend, so only the
// build with all of them (CI's all-sources leg) checks that none is dead.
#[cfg_attr(
  not(all(
    feature = "tui",
    feature = "streaming",
    feature = "discord-rpc",
    feature = "scripting",
    feature = "mcp-server",
    feature = "ai-dj",
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube",
    feature = "qobuz"
  )),
  allow(dead_code)
)]
impl InfraStatusStrings {
  pub fn soloist_missing(&self, path: &str) -> String {
    super::format_template(self.soloist_missing, &[("path", path)])
  }
  pub fn soloist_not_executable(&self, path: &str) -> String {
    super::format_template(self.soloist_not_executable, &[("path", path)])
  }
  pub fn soloist_start_failed(&self, error: &str) -> String {
    super::format_template(self.soloist_start_failed, &[("error", error)])
  }
  pub fn soloist_downloading(&self, percent: u8) -> String {
    super::format_template(
      self.soloist_downloading,
      &[("percent", &percent.to_string())],
    )
  }
  pub fn soloist_download_failed(&self, error: &str) -> String {
    super::format_template(self.soloist_download_failed, &[("error", error)])
  }
  pub fn soloist_key_save_failed(&self, error: &str) -> String {
    super::format_template(self.soloist_key_save_failed, &[("error", error)])
  }
  pub fn soloist_key_missing(&self, path: &str) -> String {
    super::format_template(self.soloist_key_missing, &[("path", path)])
  }
  /// The unit Soloist logs, in this language; an unknown one stays as logged.
  pub fn soloist_expiry_unit<'a>(&self, logged: &'a str) -> &'a str {
    match logged {
      "day" => self.soloist_unit_day,
      "days" => self.soloist_unit_days,
      "hour" => self.soloist_unit_hour,
      "hours" => self.soloist_unit_hours,
      "minute" => self.soloist_unit_minute,
      "minutes" => self.soloist_unit_minutes,
      _ => logged,
    }
  }
  pub fn soloist_expires_soon(&self, count: u32, unit: &str) -> String {
    super::format_template(
      self.soloist_expires_soon,
      &[("count", &count.to_string()), ("unit", unit)],
    )
  }
  pub fn soloist_streams(&self, quality: &str) -> String {
    super::format_template(self.soloist_streams, &[("quality", quality)])
  }
  pub fn soloist_lossy_hint(&self, quality: &str) -> String {
    super::format_template(self.soloist_lossy_hint, &[("quality", quality)])
  }
  pub fn soloist_dropped(&self, quality: &str) -> String {
    super::format_template(self.soloist_dropped, &[("quality", quality)])
  }
  pub fn sync_source_not_compiled(&self, source: &str, feature: &str) -> String {
    super::format_template(
      self.sync_source_not_compiled,
      &[("source", source), ("feature", feature)],
    )
  }
  pub fn sync_source_unsupported(&self, source: &str) -> String {
    super::format_template(self.sync_source_unsupported, &[("source", source)])
  }
  pub fn sync_source_unreachable(&self, source: &str, error: &str) -> String {
    super::format_template(
      self.sync_source_unreachable,
      &[("source", source), ("error", error)],
    )
  }
  pub fn search_query_unavailable(&self, operation: &str) -> String {
    super::format_template(self.search_query_unavailable, &[("operation", operation)])
  }
  pub fn cast_no_answer(&self, address: &str) -> String {
    super::format_template(self.cast_no_answer, &[("address", address)])
  }
  pub fn cast_no_tls(&self, address: &str) -> String {
    super::format_template(self.cast_no_tls, &[("address", address)])
  }
  pub fn cast_message_size(&self, size: usize) -> String {
    super::format_template(self.cast_message_size, &[("size", &size.to_string())])
  }
  pub fn cast_no_reply(&self, kind: &str) -> String {
    super::format_template(self.cast_no_reply, &[("kind", kind)])
  }
  pub fn cast_receiver_failed(&self, reason: &str) -> String {
    super::format_template(self.cast_receiver_failed, &[("reason", reason)])
  }
  pub fn cast_login_refused(&self, reply: &str) -> String {
    super::format_template(self.cast_login_refused, &[("reply", reply)])
  }
  pub fn speaker_token_failed(&self, status: &str, error: &str) -> String {
    super::format_template(
      self.speaker_token_failed,
      &[("status", status), ("error", error)],
    )
  }
  pub fn speaker_token_unsupported(&self, kind: &str) -> String {
    super::format_template(self.speaker_token_unsupported, &[("kind", kind)])
  }
  pub fn speaker_login_refused(&self, message: &str, error: &str) -> String {
    super::format_template(
      self.speaker_login_refused,
      &[("message", message), ("error", error)],
    )
  }
  pub fn speaker_not_joined(&self, name: &str) -> String {
    super::format_template(self.speaker_not_joined, &[("name", name)])
  }
  pub fn spotify_api_error(&self, status: &str, body: &str) -> String {
    super::format_template(
      self.spotify_api_error,
      &[("status", status), ("body", body)],
    )
  }
  pub fn local_uri_invalid(&self, error: &str) -> String {
    super::format_template(self.local_uri_invalid, &[("error", error)])
  }
  pub fn local_play_failed(&self, error: &str) -> String {
    super::format_template(self.local_play_failed, &[("error", error)])
  }
  pub fn local_task_failed(&self, error: &str) -> String {
    super::format_template(self.local_task_failed, &[("error", error)])
  }
  pub fn local_audio_unavailable(&self, error: &str) -> String {
    super::format_template(self.local_audio_unavailable, &[("error", error)])
  }
  pub fn audio_init_failed(&self, error: &str) -> String {
    super::format_template(self.audio_init_failed, &[("error", error)])
  }
  pub fn local_scan_failed(&self, error: &str) -> String {
    super::format_template(self.local_scan_failed, &[("error", error)])
  }
  pub fn local_scan_task_failed(&self, error: &str) -> String {
    super::format_template(self.local_scan_task_failed, &[("error", error)])
  }
  pub fn local_read_failed(&self, error: &str) -> String {
    super::format_template(self.local_read_failed, &[("error", error)])
  }
  pub fn local_tracks_scan_failed(&self, error: &str) -> String {
    super::format_template(self.local_tracks_scan_failed, &[("error", error)])
  }
  pub fn local_exhausted(&self, message: &str) -> String {
    super::format_template(self.local_exhausted, &[("message", message)])
  }
  pub fn subsonic_audio_unavailable(&self, error: &str) -> String {
    super::format_template(self.subsonic_audio_unavailable, &[("error", error)])
  }
  pub fn youtube_audio_unavailable(&self, error: &str) -> String {
    super::format_template(self.youtube_audio_unavailable, &[("error", error)])
  }
  pub fn radio_audio_unavailable(&self, error: &str) -> String {
    super::format_template(self.radio_audio_unavailable, &[("error", error)])
  }
  pub fn qobuz_audio_unavailable(&self, error: &str) -> String {
    super::format_template(self.qobuz_audio_unavailable, &[("error", error)])
  }
  pub fn subsonic_playlists_failed(&self, error: &str) -> String {
    super::format_template(self.subsonic_playlists_failed, &[("error", error)])
  }
  pub fn subsonic_playlist_failed(&self, error: &str) -> String {
    super::format_template(self.subsonic_playlist_failed, &[("error", error)])
  }
  pub fn subsonic_search_failed(&self, error: &str) -> String {
    super::format_template(self.subsonic_search_failed, &[("error", error)])
  }
  pub fn subsonic_download_failed(&self, error: &str) -> String {
    super::format_template(self.subsonic_download_failed, &[("error", error)])
  }
  pub fn subsonic_play_failed(&self, error: &str) -> String {
    super::format_template(self.subsonic_play_failed, &[("error", error)])
  }
  pub fn subsonic_task_failed(&self, error: &str) -> String {
    super::format_template(self.subsonic_task_failed, &[("error", error)])
  }
  pub fn radio_search_failed(&self, error: &str) -> String {
    super::format_template(self.radio_search_failed, &[("error", error)])
  }
  pub fn radio_open_failed(&self, error: &str) -> String {
    super::format_template(self.radio_open_failed, &[("error", error)])
  }
  pub fn radio_play_failed(&self, error: &str) -> String {
    super::format_template(self.radio_play_failed, &[("error", error)])
  }
  pub fn radio_task_failed(&self, error: &str) -> String {
    super::format_template(self.radio_task_failed, &[("error", error)])
  }
  pub fn radio_playing(&self, name: &str) -> String {
    super::format_template(self.radio_playing, &[("name", name)])
  }
  pub fn youtube_search_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_search_failed, &[("error", error)])
  }
  pub fn youtube_playlists_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_playlists_failed, &[("error", error)])
  }
  pub fn youtube_create_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_create_failed, &[("error", error)])
  }
  pub fn youtube_delete_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_delete_failed, &[("error", error)])
  }
  pub fn youtube_add_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_add_failed, &[("error", error)])
  }
  pub fn youtube_remove_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_remove_failed, &[("error", error)])
  }
  pub fn youtube_download_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_download_failed, &[("error", error)])
  }
  pub fn youtube_task_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_task_failed, &[("error", error)])
  }
  pub fn youtube_created(&self, name: &str) -> String {
    super::format_template(self.youtube_created, &[("name", name)])
  }
  pub fn youtube_deleted(&self, name: &str) -> String {
    super::format_template(self.youtube_deleted, &[("name", name)])
  }
  pub fn youtube_removed(&self, name: &str) -> String {
    super::format_template(self.youtube_removed, &[("name", name)])
  }
  pub fn fetching(&self, name: &str) -> String {
    super::format_template(self.fetching, &[("name", name)])
  }
  pub fn youtube_added(&self, name: &str, playlist: &str) -> String {
    super::format_template(
      self.youtube_added,
      &[("name", name), ("playlist", playlist)],
    )
  }
  pub fn youtube_already_added(&self, name: &str, playlist: &str) -> String {
    super::format_template(
      self.youtube_already_added,
      &[("name", name), ("playlist", playlist)],
    )
  }
  pub fn youtube_decode_failed(&self, error: &str) -> String {
    super::format_template(self.youtube_decode_failed, &[("error", error)])
  }
  pub fn qobuz_login_url(&self, url: &str) -> String {
    super::format_template(self.qobuz_login_url, &[("url", url)])
  }
  pub fn qobuz_login_failed(&self, error: &str) -> String {
    super::format_template(self.qobuz_login_failed, &[("error", error)])
  }
  pub fn qobuz_operation_failed(&self, step: &str, error: &str) -> String {
    super::format_template(
      self.qobuz_operation_failed,
      &[("step", step), ("error", error)],
    )
  }
  // Apple Music exists only in a macOS build with the `apple-music` feature.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub fn music_loading(&self, end: usize, total: usize) -> String {
    super::format_template(
      self.music_loading,
      &[("end", &end.to_string()), ("total", &total.to_string())],
    )
  }
  // Apple Music exists only in a macOS build with the `apple-music` feature.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub fn music_loaded(&self, count: usize) -> String {
    super::format_template(self.music_loaded, &[("count", &count.to_string())])
  }
  // Apple Music exists only in a macOS build with the `apple-music` feature.
  #[cfg_attr(
    not(all(feature = "apple-music", target_os = "macos")),
    allow(dead_code)
  )]
  pub fn music_status_unavailable(&self, error: &str) -> String {
    super::format_template(self.music_status_unavailable, &[("error", error)])
  }
  pub fn source_playback_unavailable(&self, source: &str) -> String {
    super::format_template(self.source_playback_unavailable, &[("source", source)])
  }
  pub fn queue_skip_disconnected(&self, name: &str) -> String {
    super::format_template(self.queue_skip_disconnected, &[("name", name)])
  }
  pub fn queue_play_failed(&self, name: &str, error: &str) -> String {
    super::format_template(self.queue_play_failed, &[("name", name), ("error", error)])
  }
  pub fn queue_restore_failed(&self, name: &str, error: &str) -> String {
    super::format_template(
      self.queue_restore_failed,
      &[("name", name), ("error", error)],
    )
  }
  pub fn queue_playing(&self, name: &str) -> String {
    super::format_template(self.queue_playing, &[("name", name)])
  }
  pub fn queue_audio_unavailable(&self, error: &str) -> String {
    super::format_template(self.queue_audio_unavailable, &[("error", error)])
  }
  pub fn spotify_login_failed(&self, error: &str) -> String {
    super::format_template(self.spotify_login_failed, &[("error", error)])
  }
  pub fn spotify_rate_wait(&self, seconds: u64) -> String {
    super::format_template(self.spotify_rate_wait, &[("seconds", &seconds.to_string())])
  }
  pub fn spotify_pair(&self, url: &str, code: &str) -> String {
    super::format_template(self.spotify_pair, &[("url", url), ("code", code)])
  }
  pub fn recap_created(&self, path: &str, count: usize) -> String {
    super::format_template(
      self.recap_created,
      &[("path", path), ("count", &count.to_string())],
    )
  }
  pub fn history_failed(&self, error: &str) -> String {
    super::format_template(self.history_failed, &[("error", error)])
  }
  pub fn recap_failed(&self, error: &str) -> String {
    super::format_template(self.recap_failed, &[("error", error)])
  }
  pub fn party_start_failed(&self, error: &str) -> String {
    super::format_template(self.party_start_failed, &[("error", error)])
  }
  pub fn party_join_failed(&self, error: &str) -> String {
    super::format_template(self.party_join_failed, &[("error", error)])
  }
  pub fn party_error(&self, error: &str) -> String {
    super::format_template(self.party_error, &[("error", error)])
  }
  pub fn party_joined(&self, name: &str) -> String {
    super::format_template(self.party_joined, &[("name", name)])
  }
  pub fn party_left(&self, name: &str) -> String {
    super::format_template(self.party_left, &[("name", name)])
  }
  pub fn native_nothing_to_resume(&self, device: &str) -> String {
    super::format_template(self.native_nothing_to_resume, &[("device", device)])
  }
  pub fn native_start_failed(&self, error: &str) -> String {
    super::format_template(self.native_start_failed, &[("error", error)])
  }
  pub fn native_shuffle_failed(&self, error: &str) -> String {
    super::format_template(self.native_shuffle_failed, &[("error", error)])
  }
  pub fn native_resume_failed(&self, error: &str) -> String {
    super::format_template(self.native_resume_failed, &[("error", error)])
  }
  pub fn shuffle_capped(&self, count: usize) -> String {
    super::format_template(self.shuffle_capped, &[("count", &count.to_string())])
  }
  pub fn friend_added_code(&self, code: &str) -> String {
    super::format_template(self.friend_added_code, &[("code", code)])
  }
  pub fn friend_add_failed(&self, error: &str) -> String {
    super::format_template(self.friend_add_failed, &[("error", error)])
  }
  pub fn friend_unfollow_failed(&self, error: &str) -> String {
    super::format_template(self.friend_unfollow_failed, &[("error", error)])
  }
  pub fn liked_check_failed(&self, error: &str) -> String {
    super::format_template(self.liked_check_failed, &[("error", error)])
  }
  pub fn playlist_search_scope(&self, count: u32) -> String {
    super::format_template(self.playlist_search_scope, &[("count", &count.to_string())])
  }
  pub fn playlist_search_matches(&self, count: usize, query: &str, scope: &str) -> String {
    super::format_template(
      self.playlist_search_matches,
      &[
        ("count", &count.to_string()),
        ("query", query),
        ("scope", scope),
      ],
    )
  }
  pub fn playlist_added_named(&self, name: &str) -> String {
    super::format_template(self.playlist_added_named, &[("name", name)])
  }
  pub fn playlist_created(&self, name: &str) -> String {
    super::format_template(self.playlist_created, &[("name", name)])
  }
  pub fn native_recovery_dropped(&self, error: &str) -> String {
    super::format_template(self.native_recovery_dropped, &[("error", error)])
  }
  pub fn native_recovery_failed(&self, error: &str) -> String {
    super::format_template(self.native_recovery_failed, &[("error", error)])
  }
  pub fn sync_failed(&self, error: &str) -> String {
    super::format_template(self.sync_failed, &[("error", error)])
  }
  pub fn sync_link_failed(&self, error: &str) -> String {
    super::format_template(self.sync_link_failed, &[("error", error)])
  }
  pub fn sync_remove_failed(&self, error: &str) -> String {
    super::format_template(self.sync_remove_failed, &[("error", error)])
  }
  pub fn sync_summary(&self, dry: &str, added: usize, removed: usize, unmatched: usize) -> String {
    super::format_template(
      self.sync_summary,
      &[
        ("dry", dry),
        ("added", &added.to_string()),
        ("removed", &removed.to_string()),
        ("unmatched", &unmatched.to_string()),
      ],
    )
  }
  pub fn sync_using_playlist(&self, source: &str, name: &str) -> String {
    super::format_template(
      self.sync_using_playlist,
      &[("source", source), ("name", name)],
    )
  }
  pub fn sync_progress(&self, name: &str, done: usize, total: usize) -> String {
    super::format_template(
      self.sync_progress,
      &[
        ("name", name),
        ("done", &done.to_string()),
        ("total", &total.to_string()),
      ],
    )
  }
  pub fn dj_indexed(&self, tracks: usize, playlists: usize) -> String {
    super::format_template(
      self.dj_indexed,
      &[
        ("tracks", &tracks.to_string()),
        ("playlists", &playlists.to_string()),
      ],
    )
  }
  pub fn dj_index_capped(&self, count: usize) -> String {
    super::format_template(self.dj_index_capped, &[("count", &count.to_string())])
  }
  pub fn dj_queued(&self, count: usize) -> String {
    super::format_template(self.dj_queued, &[("count", &count.to_string())])
  }
  pub fn dj_not_found(&self, count: usize) -> String {
    super::format_template(self.dj_not_found, &[("count", &count.to_string())])
  }
  pub fn dj_owned(&self, count: usize) -> String {
    super::format_template(self.dj_owned, &[("count", &count.to_string())])
  }
  pub fn dj_duplicate(&self, count: usize) -> String {
    super::format_template(self.dj_duplicate, &[("count", &count.to_string())])
  }
  pub fn dj_status(&self, summary: &str) -> String {
    super::format_template(self.dj_status, &[("summary", summary)])
  }
  pub fn dj_library_failed(&self, error: &str) -> String {
    super::format_template(self.dj_library_failed, &[("error", error)])
  }
  pub fn dj_error(&self, backend: &str, error: &str) -> String {
    super::format_template(self.dj_error, &[("backend", backend), ("error", error)])
  }
  pub fn dj_steps_exhausted(&self, count: usize) -> String {
    super::format_template(self.dj_steps_exhausted, &[("count", &count.to_string())])
  }
  pub fn history_plays(&self, count: u64, duration: &str) -> String {
    super::format_template(
      if count == 1 {
        self.history_play
      } else {
        self.history_plays
      },
      &[("count", &count.to_string()), ("duration", duration)],
    )
  }
  pub fn history_track_hits(&self, count: u64, duration: &str) -> String {
    super::format_template(
      if count == 1 {
        self.history_track_hit
      } else {
        self.history_track_hits
      },
      &[("count", &count.to_string()), ("duration", duration)],
    )
  }
  pub fn history_minutes(&self, minutes: u64) -> String {
    super::format_template(self.history_minutes, &[("minutes", &minutes.to_string())])
  }
  pub fn history_hours_minutes(&self, hours: u64, minutes: u64) -> String {
    super::format_template(
      self.history_hours_minutes,
      &[
        ("hours", &hours.to_string()),
        ("minutes", &minutes.to_string()),
      ],
    )
  }
  pub fn setup_no_api_key(&self, agent: &str) -> String {
    super::format_template(self.setup_no_api_key, &[("agent", agent)])
  }
  pub fn setup_keep_command(&self, command: &str) -> String {
    super::format_template(self.setup_keep_command, &[("command", command)])
  }
  pub fn setup_needs_key(&self, variable: &str) -> String {
    super::format_template(self.setup_needs_key, &[("variable", variable)])
  }
  pub fn setup_builtin_default(&self, model: &str) -> String {
    super::format_template(self.setup_builtin_default, &[("model", model)])
  }
  pub fn mcp_status(&self, message: &str) -> String {
    super::format_template(self.mcp_status, &[("message", message)])
  }
  pub fn network_error(&self, error: &str) -> String {
    super::format_template(self.network_error, &[("error", error)])
  }
  pub fn network_rate_error(&self, error: &str) -> String {
    super::format_template(self.network_rate_error, &[("error", error)])
  }
  pub fn monthly_recap_ready(&self, path: &str, count: usize) -> String {
    super::format_template(
      self.monthly_recap_ready,
      &[("path", path), ("count", &count.to_string())],
    )
  }
}

pub const EN: InfraStatusStrings = InfraStatusStrings {
  sync_spotify_not_connected: "Spotify is not connected",
  sync_qobuz_not_logged_in: "Qobuz is not logged in",
  sync_subsonic_not_configured: "No Subsonic server configured (set behavior.subsonic_url)",
  sync_source_not_compiled: "{source} is not compiled into this build (feature `{feature}`)",
  sync_source_unsupported: "{source} playlists cannot be synced",
  sync_source_unreachable: "{source} unreachable: {error}",
  search_query_unavailable: "Search is unavailable: every known {operation} hash is retired",
  made_for_you_unavailable: "Made For You is unavailable: every known browsePage hash is retired",
  made_for_you_missing: "Spotify did not send the Made For You page",
  history_track_hit: "{count} track hit · {duration}",
  history_play: "{count} play · {duration}",
  cast_no_answer: "no answer from {address}",
  cast_no_tls: "no TLS handshake with {address}",
  cast_message_size: "a {size}-byte Cast message",
  cast_malformed: "a malformed Cast message",
  cast_no_reply: "no reply to {kind}",
  cast_receiver_failed: "the Spotify receiver did not start: {reason}",
  cast_client_id_missing: "no clientID in getInfoResponse",
  cast_login_refused: "the speaker refused the login: {reply}",
  cast_access_token_missing: "no accessToken in the device-auth reply",
  speaker_token_failed: "the token exchange failed ({status}): {error}",
  speaker_token_unsupported: "the speaker asks for token type {kind}, which is not supported yet",
  speaker_client_id_missing: "no clientID in getInfo",
  speaker_login_refused: "the speaker refused the login: {message} ({error})",
  speaker_not_joined: "{name} did not join after its login",
  playlist_changed: "The playlist changed since it loaded; open it again and retry",
  spotify_api_error: "Spotify API {status} failed: {body}",
  local_replay_failed: "Cannot replay local file",
  local_no_tracks: "No local tracks to play",
  local_no_folder: "No local music folder configured (set behavior.local_music_path)",
  local_uri_invalid: "Invalid local file URI: {error}",
  local_play_failed: "Cannot play local file: {error}",
  local_task_failed: "Local playback task failed: {error}",
  local_audio_unavailable: "No audio output for local playback: {error}",
  audio_init_failed: "Audio output init failed: {error}",
  local_scan_failed: "Cannot scan music folder: {error}",
  local_scan_task_failed: "Local folder scan failed: {error}",
  local_read_failed: "Cannot read folder: {error}",
  local_tracks_scan_failed: "Local track scan failed: {error}",
  local_exhausted: "{message} — no playable tracks left, stopping",
  subsonic_audio_unavailable: "No audio output for Subsonic playback: {error}",
  youtube_audio_unavailable: "No audio output for YouTube playback: {error}",
  radio_audio_unavailable: "No audio output for radio playback: {error}",
  qobuz_audio_unavailable: "No audio output for Qobuz playback: {error}",
  subsonic_no_tracks: "No Subsonic tracks to play",
  subsonic_uri_invalid: "Invalid Subsonic track URI",
  subsonic_replay_failed: "Cannot replay Subsonic track",
  subsonic_playlists_failed: "Cannot load Subsonic playlists: {error}",
  subsonic_playlist_failed: "Cannot load Subsonic playlist: {error}",
  subsonic_search_failed: "Subsonic search failed: {error}",
  subsonic_download_failed: "Cannot download Subsonic track: {error}",
  subsonic_play_failed: "Cannot play Subsonic track: {error}",
  subsonic_task_failed: "Subsonic playback task failed: {error}",
  subsonic_no_server: "No Subsonic server configured (set behavior.subsonic_url)",
  radio_no_skip: "Live radio: no track skipping",
  radio_empty: "No radio stations configured or saved; search to add some",
  radio_uri_invalid: "Invalid radio URI",
  radio_probe_timeout: "Cannot play radio stream: no recognisable audio format in time.",
  radio_search_failed: "Radio search failed: {error}",
  radio_open_failed: "Cannot open radio stream: {error}",
  radio_play_failed: "Cannot play radio stream: {error}",
  radio_task_failed: "Radio playback task failed: {error}",
  radio_playing: "♪ {name} (live)",
  youtube_playlist_missing: "That YouTube playlist no longer exists",
  youtube_metadata_missing: "Cannot resolve that video's metadata",
  youtube_no_tracks: "No YouTube videos to play",
  youtube_no_live: "Live streams aren't supported on the YouTube source yet",
  youtube_uri_invalid: "Invalid YouTube URI",
  youtube_replay_failed: "Cannot replay YouTube track",
  youtube_search_failed: "YouTube search failed: {error}",
  youtube_playlists_failed: "Cannot load YouTube playlists: {error}",
  youtube_create_failed: "Cannot create YouTube playlist: {error}",
  youtube_delete_failed: "Cannot delete YouTube playlist: {error}",
  youtube_add_failed: "Cannot add video to playlist: {error}",
  youtube_remove_failed: "Cannot remove video from playlist: {error}",
  youtube_download_failed: "Cannot download YouTube audio: {error}",
  youtube_task_failed: "YouTube playback task failed: {error}",
  youtube_created: "Created YouTube playlist \"{name}\"",
  youtube_deleted: "Deleted YouTube playlist \"{name}\"",
  youtube_removed: "Removed \"{name}\" from playlist",
  fetching: "Fetching {name}…",
  youtube_added: "Added \"{name}\" to {playlist}",
  youtube_already_added: "\"{name}\" is already in {playlist}",
  youtube_decode_failed: "Cannot play YouTube audio: {error} (installing ffmpeg fixes most container issues)",
  qobuz_login_expired: "Qobuz: login expired, press `d` and pick Qobuz to log in again",
  qobuz_login_busy: "Qobuz login already in progress...",
  qobuz_browser_login: "Qobuz: open the browser window to log in",
  qobuz_logged_in: "Qobuz: logged in",
  qobuz_no_tracks: "No Qobuz tracks to play",
  qobuz_uri_invalid: "Invalid Qobuz track URI",
  qobuz_replay_failed: "Cannot replay Qobuz track",
  qobuz_login_url: "Open this URL in your browser to log in to Qobuz: {url}",
  qobuz_login_failed: "Qobuz login failed: {error}",
  qobuz_step_constants: "web player constants",
  qobuz_step_login: "login",
  qobuz_step_library: "library",
  qobuz_step_tracks: "tracks",
  qobuz_step_search: "search",
  qobuz_step_stream: "stream",
  qobuz_operation_failed: "Qobuz: {step}: {error}",
  music_busy_playback: "Music is busy; retry the playback request",
  music_wait_pause: "Waiting for Music to pause before switching source",
  music_not_running: "Music is not running; choose a Music track to start it",
  music_shuffle_repeat: "Use Music to change shuffle or repeat",
  music_switch_spotify: "Start a Spotify track to switch playback from Music",
  music_busy: "Music is busy; retry the request",
  music_queue_unsupported: "Apple Music tracks can't be queued",
  music_loading: "Loading Apple Music library: {end}/{total}",
  music_loaded: "Apple Music: loaded {count} items",
  music_status_unavailable: "Music status unavailable: {error}. Retrying in the background.",
  queue_repeat_unsupported: "Repeat does not apply to this source",
  queue_shuffle_unsupported: "Shuffle does not apply to this source",
  queue_no_uri: "Skipped a queued track with no URI",
  queue_finished: "Queue finished",
  audio_disconnected: "Audio output device disconnected.",
  radio_not_resumed: "Playback is paused - radio not resumed.",
  source_playback_unavailable: "{source} playback isn't available in this build",
  queue_skip_disconnected: "Native streaming isn't connected; skipped \"{name}\"",
  queue_play_failed: "Cannot play {name}: {error}",
  queue_restore_failed: "Cannot restore {name}: {error}",
  queue_playing: "♪ {name} (queue)",
  queue_audio_unavailable: "No audio output for queue playback: {error}",
  spotify_not_connected: "Spotify not connected. Press `d` and pick Spotify to log in.",
  playlist_id_invalid: "Invalid playlist identifier.",
  music_requires_macos: "Apple Music requires macOS and the apple-music feature",
  spotify_not_compiled: "This build of spotatui has no Spotify support",
  spotify_connected: "Spotify connected. Restart spotatui to enable native playback.",
  party_disconnected: "Listening Party ended: Spotify is not connected.",
  party_ended: "Party ended",
  spotify_login_timeout: "Spotify login timed out. Press `d` and pick Spotify to try again.",
  spotify_login_failed: "Spotify login failed: {error}",
  spotify_rate_wait: "Spotify rate limit: waiting {seconds}s",
  spotify_pair: "Log in to Spotify: open {url} and confirm the code {code}",
  recap_created: "Listening recap generated at {path} ({count} listens)",
  history_failed: "failed to load listening history: {error}",
  recap_failed: "Failed to generate recap: {error}",
  party_start_failed: "Failed to start party: {error}",
  party_join_failed: "Failed to join party: {error}",
  party_error: "Party: {error}",
  party_joined: "{name} joined the party",
  party_left: "{name} left the party",
  native_reconnecting: "Reconnecting native streaming…",
  native_settling: "Native streaming is still settling…",
  shuffle_not_applied: "Playback started; couldn't apply shuffle.",
  spotify_rate_retry: "Spotify rate limit hit. Retrying automatically; please wait a few seconds.",
  spotify_poll_retry: "Temporary Spotify network error while polling playback; retrying automatically.",
  spotify_server_retry: "Spotify server temporarily unavailable (5xx); retrying automatically.",
  spotify_poll_rejected: "Spotify keeps rejecting the playback poll; retrying automatically.",
  native_resuming: "Resuming the last playback here…",
  native_reconnect_resume: "Reconnecting native streaming; playback will resume.",
  native_connection_restored: "Native connection restored; restoring playback.",
  native_no_state: "Native connection recovered, but there was no playback state to restore.",
  native_restore_retry: "Native playback restore failed; reconnecting again.",
  other_source_playing: "Another source owns playback",
  spotify_playing_elsewhere: "Playback remains on another Spotify device.",
  queue_added: "Added to queue",
  queue_load_failed: "Could not load queue (no active device?)",
  native_nothing_to_resume: "No playback to resume on {device}.",
  native_start_failed: "Failed to start native playback: {error}",
  native_shuffle_failed: "Failed to apply shuffle: {error}",
  native_resume_failed: "Failed to resume playback: {error}",
  shuffle_context_failed: "Shuffle: failed to load the context's track list",
  shuffle_capped: "Large context: shuffling the first {count} tracks",
  spotify_profile_retry: "Spotify rate limit hit while loading profile. Retrying automatically.",
  friend_added: "Added friend",
  friend_unfollowed: "Unfollowed",
  friend_added_code: "Added friend (code: {code})",
  friend_add_failed: "Could not add friend: {error}",
  friend_unfollow_failed: "Failed to unfollow: {error}",
  playlist_external_unavailable: "Spotify Development Mode blocks playlist contents owned by another user. Only playlists you own or collaborate on are available.",
  liked_check_failed: "Could not check liked track state: {error}",
  playlist_search_scope: " in the first {count} tracks",
  playlist_search_matches: "{count} playlist tracks match \"{query}\"{scope}",
  playlist_refresh_partial: "Playlist refresh incomplete; kept the previous list.",
  playlist_added: "Added to playlist",
  playlist_removed: "Removed from playlist",
  playlist_created_no_id: "Playlist created but could not get its ID",
  playlist_added_named: "Added to {name}",
  playlist_created: "Playlist \"{name}\" created!",
  native_recovered: "Native streaming recovered.",
  native_lost_reconnecting: "Native streaming connection lost; reconnecting.",
  native_reconnected: "Native streaming connection recovered.",
  native_restored: "Native playback restored.",
  native_restored_paused: "Native playback restored in paused state.",
  native_skip_unavailable: "Couldn't play this track natively (unavailable or blocked); skipping.",
  native_many_unavailable: "Several tracks in a row couldn't be played natively, so playback was stopped. They may be unavailable on your account or region. Press 'd' to switch to an official Spotify Connect device.",
  soloist_path_unset: "Soloist is not installed. In Settings, Soloist Path, pick Download from Spotify.",
  soloist_missing: "Soloist not found at {path}. In Settings, Soloist Path, pick Download from Spotify.",
  soloist_not_executable: "Soloist at {path} is not executable: chmod +x {path}",
  soloist_wrong_cpu: "This Soloist build does not run on this CPU. In Settings, Soloist Path, pick Download from Spotify.",
  soloist_start_failed: "Soloist could not start: {error}",
  soloist_key_missing: "Soloist needs an API key. In Settings, Soloist API Key, pick Get a key, then enter the key (it is saved in {path}).",
  soloist_not_paired: "Pair spotatui: open Spotify on a computer on this network and pick 'spotatui'.",
  soloist_offline: "Soloist cannot reach Spotify.",
  soloist_bad_key: "Soloist rejected the API key. In Settings, Soloist API Key, enter a new one.",
  soloist_expired: "Soloist build expired. In Settings, Soloist Path, pick Download from Spotify for a new one.",
  soloist_expires_soon: "Soloist build expires in {count} {unit}: download a new one soon.",
  soloist_unit_day: "day",
  soloist_unit_days: "days",
  soloist_unit_hour: "hour",
  soloist_unit_hours: "hours",
  soloist_unit_minute: "minute",
  soloist_unit_minutes: "minutes",
  soloist_dir_in_use: "Another Soloist uses spotatui's Soloist data folder.",
  soloist_keeps_exiting: "Soloist keeps exiting; see the log.",
  soloist_no_websocket: "Soloist started without its WebSocket.",
  soloist_restarting: "Soloist stopped; restarting it.",
  soloist_restart_quality: "Restarting Soloist to apply the audio quality.",
  soloist_streams: "Soloist streams {quality}.",
  soloist_lossy_hint: "Soloist streams {quality}. For lossless, pick spotatui in the Spotify mobile app's device menu and set Lossless.",
  soloist_dropped: "Soloist dropped to {quality}.",
  soloist_engine_unavailable: "This build has no Soloist engine; playing through librespot.",
  soloist_queue_refused: "Queueing other sources behind Spotify needs the librespot engine.",
  soloist_download_started: "Downloading Soloist from Spotify. By using Soloist, you accept the Spotify Terms and Conditions of Use.",
  soloist_downloading: "Downloading Soloist from Spotify: {percent}%",
  soloist_downloaded: "Soloist downloaded; starting it.",
  soloist_downloaded_engine_off: "Soloist downloaded. Set Playback Engine to Soloist to play through it.",
  soloist_downloaded_next_launch: "Soloist downloaded. spotatui uses it from the next launch.",
  soloist_download_failed: "Soloist download failed: {error}",
  soloist_no_build: "Spotify has no Soloist build for this system.",
  soloist_paired: "Paired: spotatui plays through Soloist.",
  engine_switching_soloist: "Switching the playback engine to Soloist.",
  engine_switching_librespot: "Switching the playback engine to librespot.",
  engine_next_launch: "The new playback engine starts at the next launch.",
  soloist_key_saved: "Soloist API key saved.",
  soloist_key_restarting: "Soloist API key saved; restarting Soloist with it.",
  soloist_key_env_wins: "Soloist API key saved, but SPOTATUI_SOLOIST_API_KEY is set and wins over it.",
  soloist_key_save_failed: "Could not save the Soloist API key: {error}",
  native_recovery_dropped: "Native recovery failed; playback request dropped: {error}",
  native_recovery_failed: "Native recovery failed: {error}",
  native_lost_recovering: "Native streaming connection lost; attempting recovery.",
  native_stalled: "Native streaming stalled; attempting recovery.",
  native_stopped: "Native streaming stopped.",
  native_moved: "Playback moved to another Spotify device.",
  native_disconnected: "Native streaming disconnected; attempting recovery.",
  native_stopped_recovering: "Native streaming stopped; attempting recovery.",
  sync_busy: "Playlist sync already running",
  sync_no_links: "Playlist sync: no links",
  sync_dry_run: " (dry run)",
  sync_removed: "Playlist link removed; the mirror playlists stay",
  sync_failed: "Playlist sync failed: {error}",
  sync_link_failed: "Playlist link failed: {error}",
  sync_remove_failed: "Removing the link failed: {error}",
  sync_summary: "Playlist sync{dry}: {added} added, {removed} removed, {unmatched} unmatched",
  sync_using_playlist: "Using the existing {source} playlist \"{name}\"",
  sync_progress: "Playlist sync: {name} {done}/{total}",
  dj_indexed: "Indexed {tracks} track(s) across {playlists} playlist(s)",
  dj_index_capped: " (stopped at {count}; the rest is not filtered)",
  dj_queued: "queued {count}",
  dj_not_found: "{count} not found",
  dj_owned: "{count} already yours",
  dj_duplicate: "{count} already queued",
  dj_status: "DJ: {summary}",
  dj_library_failed: "DJ: could not read your playlists ({error}); recommending unfiltered",
  dj_topped_up: "DJ: topped up the queue",
  dj_nothing_queued: "DJ: nothing was queued that time. Ask again, or try a different direction.",
  dj_error: "DJ error ({backend}): {error}",
  dj_steps_exhausted: "Stopped after {count} steps. Ask again if that is not where you wanted to end up.",
  history_plays: "{count} plays · {duration}",
  history_track_hits: "{count} track hits · {duration}",
  history_minutes: "{minutes}m",
  history_hours_minutes: "{hours}h {minutes}m",
  setup_no_api_key: "{agent} (no API key)",
  setup_keep_command: "{command} (keep)",
  setup_anthropic: "anthropic (API key)",
  setup_openai: "openai_compat (local or self-hosted)",
  setup_own_command: "your own command, run exactly as written",
  setup_not_installed: " · not on PATH",
  setup_anthropic_billing: "Anthropic API, pay per token",
  setup_needs_key: "needs {variable} (or behavior.dj_api_key)",
  setup_openai_endpoint: "any OpenAI-compatible endpoint, e.g. Ollama on localhost",
  setup_claude_plan: "uses your Claude Pro/Max plan",
  setup_codex_plan: "uses your ChatGPT plan",
  setup_agy_plan: "uses your Antigravity plan",
  setup_copilot_plan: "uses your GitHub Copilot plan",
  setup_opencode_plan: "uses whatever provider opencode is logged into",
  setup_gemini_legacy: "legacy Gemini CLI, superseded by agy",
  setup_agent_subscription: "uses that CLI's own subscription",
  setup_keep_as_is: "Keep this command as it is",
  setup_own_model_note: "spotatui adds no model flag to a command it does not own",
  setup_current: "currently configured",
  setup_cli_default: "Use the CLI's default",
  setup_no_model_flag: "no model flag",
  setup_builtin_default: "Use the built-in default ({model})",
  setup_custom: "Custom…",
  setup_type_provider_model: "type a model name, as provider/model",
  setup_type_model: "type a model name",
  setup_haiku_note: "cheapest, easiest on a Pro plan",
  setup_sonnet_note: "balanced",
  setup_opus_note: "heavy, a Pro plan hits its limit fast",
  setup_fable_note: "heaviest, hits it faster still",
  setup_cheapest: "cheapest",
  setup_heaviest_gemini: "heaviest Gemini",
  setup_heaviest_all: "heaviest of all",
  setup_price_haiku: "$1/$5 per MTok, 200K ctx",
  setup_price_sonnet: "$3/$15 per MTok, 1M ctx",
  setup_price_opus: "$5/$25 per MTok, 1M ctx",
  setup_price_fable: "$10/$50 per MTok, 1M ctx",
  history_week: "Last 7 Days",
  history_month: "Last 30 Days",
  history_all: "All Time",
  history_this_month: "This Month",
  history_this_year: "This Year",
  mcp_status: "MCP: {message}",
  network_error: "{error}",
  network_rate_error: "{error}",
  monthly_recap_ready: "Monthly listening recap ready at {path} ({count} listens)",
  queue_local_source: "Local",
  unnamed_folder: "Unnamed Folder",
  local_root_folder: "(root)",
  local_unknown_track: "Unknown Track",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: InfraStatusStrings = InfraStatusStrings {
  sync_spotify_not_connected: "Spotify 未连接",
  sync_qobuz_not_logged_in: "Qobuz 未登录",
  sync_subsonic_not_configured: "尚未配置 Subsonic 服务器（请设置 behavior.subsonic_url）",
  sync_source_not_compiled: "此版本未编译 {source}（功能 `{feature}`）",
  sync_source_unsupported: "无法同步 {source} 播放列表",
  sync_source_unreachable: "无法连接 {source}：{error}",
  search_query_unavailable: "搜索不可用：所有已知的 {operation} 查询标识均已失效",
  made_for_you_unavailable: "为你精选不可用：所有已知的 browsePage 查询标识均已失效",
  made_for_you_missing: "Spotify 未返回为你精选页面",
  history_track_hit: "曲目播放 {count} 次 · {duration}",
  history_play: "{count} 次播放 · {duration}",
  cast_no_answer: "{address} 没有响应",
  cast_no_tls: "与 {address} 的 TLS 握手未完成",
  cast_message_size: "Cast 消息大小为 {size} 字节",
  cast_malformed: "Cast 消息格式错误",
  cast_no_reply: "{kind} 未收到回复",
  cast_receiver_failed: "Spotify 接收器未启动：{reason}",
  cast_client_id_missing: "getInfoResponse 中缺少 clientID",
  cast_login_refused: "音箱拒绝登录：{reply}",
  cast_access_token_missing: "device-auth 回复中缺少 accessToken",
  speaker_token_failed: "令牌交换失败（{status}）：{error}",
  speaker_token_unsupported: "音箱请求的令牌类型 {kind} 尚不支持",
  speaker_client_id_missing: "getInfo 中缺少 clientID",
  speaker_login_refused: "音箱拒绝登录：{message}（{error}）",
  speaker_not_joined: "{name} 登录后未连接",
  playlist_changed: "播放列表在加载后发生了变化，请重新打开后再试",
  spotify_api_error: "Spotify API {status} 请求失败：{body}",
  local_replay_failed: "无法重新播放本地文件",
  local_no_tracks: "没有可播放的本地曲目",
  local_no_folder: "尚未设置本地音乐文件夹（请设置 behavior.local_music_path）",
  local_uri_invalid: "本地文件 URI 无效：{error}",
  local_play_failed: "无法播放本地文件：{error}",
  local_task_failed: "本地播放任务失败：{error}",
  local_audio_unavailable: "本地播放无音频输出：{error}",
  audio_init_failed: "音频输出初始化失败：{error}",
  local_scan_failed: "无法扫描音乐文件夹：{error}",
  local_scan_task_failed: "本地文件夹扫描失败：{error}",
  local_read_failed: "无法读取文件夹：{error}",
  local_tracks_scan_failed: "本地曲目扫描失败：{error}",
  local_exhausted: "{message} — 已无可播放曲目，停止播放",
  subsonic_audio_unavailable: "Subsonic 播放无音频输出：{error}",
  youtube_audio_unavailable: "YouTube 播放无音频输出：{error}",
  radio_audio_unavailable: "电台播放无音频输出：{error}",
  qobuz_audio_unavailable: "Qobuz 播放无音频输出：{error}",
  subsonic_no_tracks: "没有可播放的 Subsonic 曲目",
  subsonic_uri_invalid: "Subsonic 曲目 URI 无效",
  subsonic_replay_failed: "无法重新播放 Subsonic 曲目",
  subsonic_playlists_failed: "无法加载 Subsonic 播放列表：{error}",
  subsonic_playlist_failed: "无法加载 Subsonic 播放列表：{error}",
  subsonic_search_failed: "Subsonic 搜索失败：{error}",
  subsonic_download_failed: "无法下载 Subsonic 曲目：{error}",
  subsonic_play_failed: "无法播放 Subsonic 曲目：{error}",
  subsonic_task_failed: "Subsonic 播放任务失败：{error}",
  subsonic_no_server: "尚未设置 Subsonic 服务器（请设置 behavior.subsonic_url）",
  radio_no_skip: "电台直播无法切换曲目",
  radio_empty: "尚未设置或保存电台；请搜索并添加",
  radio_uri_invalid: "电台 URI 无效",
  radio_probe_timeout: "无法播放电台流：未能及时识别音频格式。",
  radio_search_failed: "电台搜索失败：{error}",
  radio_open_failed: "无法打开电台流：{error}",
  radio_play_failed: "无法播放电台流：{error}",
  radio_task_failed: "电台播放任务失败：{error}",
  radio_playing: "♪ {name}（直播）",
  youtube_playlist_missing: "该 YouTube 播放列表已不存在",
  youtube_metadata_missing: "无法获取该视频的元数据",
  youtube_no_tracks: "没有可播放的 YouTube 视频",
  youtube_no_live: "YouTube 音源暂不支持直播",
  youtube_uri_invalid: "YouTube URI 无效",
  youtube_replay_failed: "无法重新播放 YouTube 曲目",
  youtube_search_failed: "YouTube 搜索失败：{error}",
  youtube_playlists_failed: "无法加载 YouTube 播放列表：{error}",
  youtube_create_failed: "无法创建 YouTube 播放列表：{error}",
  youtube_delete_failed: "无法删除 YouTube 播放列表：{error}",
  youtube_add_failed: "无法将视频添加到播放列表：{error}",
  youtube_remove_failed: "无法从播放列表移除视频：{error}",
  youtube_download_failed: "无法下载 YouTube 音频：{error}",
  youtube_task_failed: "YouTube 播放任务失败：{error}",
  youtube_created: "已创建 YouTube 播放列表“{name}”",
  youtube_deleted: "已删除 YouTube 播放列表“{name}”",
  youtube_removed: "已从播放列表移除“{name}”",
  fetching: "正在获取 {name}…",
  youtube_added: "已将“{name}”添加到 {playlist}",
  youtube_already_added: "“{name}”已在 {playlist} 中",
  youtube_decode_failed: "无法播放 YouTube 音频：{error}（安装 ffmpeg 可解决大多数容器格式问题）",
  qobuz_login_expired: "Qobuz：登录已过期，请按 `d` 并选择 Qobuz 重新登录",
  qobuz_login_busy: "Qobuz 正在登录…",
  qobuz_browser_login: "Qobuz：请打开浏览器窗口登录",
  qobuz_logged_in: "Qobuz：已登录",
  qobuz_no_tracks: "没有可播放的 Qobuz 曲目",
  qobuz_uri_invalid: "Qobuz 曲目 URI 无效",
  qobuz_replay_failed: "无法重新播放 Qobuz 曲目",
  qobuz_login_url: "请在浏览器中打开此 URL 登录 Qobuz：{url}",
  qobuz_login_failed: "Qobuz 登录失败：{error}",
  qobuz_step_constants: "网页播放器参数",
  qobuz_step_login: "登录",
  qobuz_step_library: "音乐库",
  qobuz_step_tracks: "曲目",
  qobuz_step_search: "搜索",
  qobuz_step_stream: "音频流",
  qobuz_operation_failed: "Qobuz：{step}失败：{error}",
  music_busy_playback: "Music 正忙，请重试播放请求",
  music_wait_pause: "正在等待 Music 暂停，再切换音源",
  music_not_running: "Music 未运行，请选择 Music 曲目启动",
  music_shuffle_repeat: "请在 Music 中更改随机播放或循环播放",
  music_switch_spotify: "播放 Spotify 曲目即可从 Music 切换",
  music_busy: "Music 正忙，请重试请求",
  music_queue_unsupported: "Apple Music 曲目无法加入队列",
  music_loading: "正在加载 Apple Music 音乐库：{end}/{total}",
  music_loaded: "Apple Music：已加载 {count} 项",
  music_status_unavailable: "无法获取 Music 状态：{error}。正在后台重试。",
  queue_repeat_unsupported: "此音源不支持循环播放",
  queue_shuffle_unsupported: "此音源不支持随机播放",
  queue_no_uri: "已跳过队列中缺少 URI 的曲目",
  queue_finished: "队列播放完毕",
  audio_disconnected: "音频输出设备已断开。",
  radio_not_resumed: "播放已暂停，未恢复电台播放。",
  source_playback_unavailable: "此版本不支持 {source} 播放",
  queue_skip_disconnected: "内置播放未连接，已跳过“{name}”",
  queue_play_failed: "无法播放 {name}：{error}",
  queue_restore_failed: "无法恢复 {name}：{error}",
  queue_playing: "♪ {name}（队列）",
  queue_audio_unavailable: "队列播放无音频输出：{error}",
  spotify_not_connected: "Spotify 未连接。请按 `d` 并选择 Spotify 登录。",
  playlist_id_invalid: "播放列表标识无效。",
  music_requires_macos: "Apple Music 需要 macOS 和 apple-music 功能",
  spotify_not_compiled: "此版本的 spotatui 不支持 Spotify",
  spotify_connected: "Spotify 已连接。重启 spotatui 以启用内置播放。",
  party_disconnected: "一起听已结束：Spotify 未连接。",
  party_ended: "一起听已结束",
  spotify_login_timeout: "Spotify 登录超时。请按 `d` 并选择 Spotify 重试。",
  spotify_login_failed: "Spotify 登录失败：{error}",
  spotify_rate_wait: "Spotify 请求受限：等待 {seconds} 秒",
  spotify_pair: "登录 Spotify：打开 {url} 并确认配对码 {code}",
  recap_created: "收听回顾已生成于 {path}（{count} 次收听）",
  history_failed: "无法加载收听历史：{error}",
  recap_failed: "生成收听回顾失败：{error}",
  party_start_failed: "发起一起听失败：{error}",
  party_join_failed: "加入一起听失败：{error}",
  party_error: "一起听：{error}",
  party_joined: "{name} 加入了一起听",
  party_left: "{name} 离开了一起听",
  native_reconnecting: "正在重新连接内置播放…",
  native_settling: "内置播放仍在准备中…",
  shuffle_not_applied: "已开始播放，但无法启用随机播放。",
  spotify_rate_retry: "Spotify 请求受限。正在自动重试，请稍候。",
  spotify_poll_retry: "获取播放状态时发生临时 Spotify 网络错误，正在自动重试。",
  spotify_server_retry: "Spotify 服务器暂时不可用（5xx），正在自动重试。",
  spotify_poll_rejected: "Spotify 持续拒绝播放状态请求，正在自动重试。",
  native_resuming: "正在此设备恢复上次播放…",
  native_reconnect_resume: "正在重新连接内置播放，随后将恢复播放。",
  native_connection_restored: "内置播放连接已恢复，正在恢复播放。",
  native_no_state: "内置播放连接已恢复，但没有可恢复的播放状态。",
  native_restore_retry: "内置播放恢复失败，正在再次连接。",
  other_source_playing: "其他音源正在播放",
  spotify_playing_elsewhere: "仍在另一台 Spotify 设备上播放。",
  queue_added: "已加入队列",
  queue_load_failed: "无法加载队列（没有活动设备？）",
  native_nothing_to_resume: "{device} 上没有可恢复的播放。",
  native_start_failed: "启动内置播放失败：{error}",
  native_shuffle_failed: "启用随机播放失败：{error}",
  native_resume_failed: "恢复播放失败：{error}",
  shuffle_context_failed: "随机播放：无法加载所属列表的曲目",
  shuffle_capped: "列表较大：随机播放前 {count} 首曲目",
  spotify_profile_retry: "加载个人资料时 Spotify 请求受限，正在自动重试。",
  friend_added: "已添加好友",
  friend_unfollowed: "已取消关注",
  friend_added_code: "已添加好友（代码：{code}）",
  friend_add_failed: "无法添加好友：{error}",
  friend_unfollow_failed: "取消关注失败：{error}",
  playlist_external_unavailable: "Spotify Development Mode 禁止读取其他用户拥有的播放列表内容。只能访问你拥有或参与协作的播放列表。",
  liked_check_failed: "无法检查曲目的喜欢状态：{error}",
  playlist_search_scope: "（仅搜索前 {count} 首曲目）",
  playlist_search_matches: "播放列表中有 {count} 首曲目匹配“{query}”{scope}",
  playlist_refresh_partial: "播放列表刷新不完整，已保留原列表。",
  playlist_added: "已添加到播放列表",
  playlist_removed: "已从播放列表移除",
  playlist_created_no_id: "播放列表已创建，但无法获取其 ID",
  playlist_added_named: "已添加到 {name}",
  playlist_created: "播放列表“{name}”已创建！",
  native_recovered: "内置播放已恢复。",
  native_lost_reconnecting: "内置播放连接已断开，正在重新连接。",
  native_reconnected: "内置播放连接已恢复。",
  native_restored: "内置播放已恢复。",
  native_restored_paused: "内置播放已恢复，保持暂停。",
  native_skip_unavailable: "此曲目无法使用内置播放（不可用或被限制），正在跳过。",
  native_many_unavailable: "多首曲目连续无法使用内置播放，已停止播放。你的账号或所在地区可能无法使用这些曲目。请按 'd' 切换到官方 Spotify Connect 设备。",
  native_recovery_dropped: "内置播放恢复失败，播放请求已放弃：{error}",
  native_recovery_failed: "内置播放恢复失败：{error}",
  native_lost_recovering: "内置播放连接已断开，正在尝试恢复。",
  native_stalled: "内置播放已停滞，正在尝试恢复。",
  native_stopped: "内置播放已停止。",
  native_moved: "播放已转移到另一台 Spotify 设备。",
  native_disconnected: "内置播放已断开，正在尝试恢复。",
  native_stopped_recovering: "内置播放已停止，正在尝试恢复。",
  sync_busy: "播放列表同步已在运行",
  sync_no_links: "播放列表同步：没有关联",
  sync_dry_run: "（模拟运行）",
  sync_removed: "播放列表关联已移除，镜像播放列表保留",
  sync_failed: "播放列表同步失败：{error}",
  sync_link_failed: "播放列表关联失败：{error}",
  sync_remove_failed: "移除关联失败：{error}",
  sync_summary: "播放列表同步{dry}：添加 {added} 首，移除 {removed} 首，未匹配 {unmatched} 首",
  sync_using_playlist: "使用现有 {source} 播放列表“{name}”",
  sync_progress: "播放列表同步：{name} {done}/{total}",
  dj_indexed: "已索引 {playlists} 个播放列表中的 {tracks} 首曲目",
  dj_index_capped: "（已在 {count} 首处停止，其余曲目未过滤）",
  dj_queued: "已加入队列 {count} 首",
  dj_not_found: "未找到 {count} 首",
  dj_owned: "已拥有 {count} 首",
  dj_duplicate: "已在队列中 {count} 首",
  dj_status: "DJ：{summary}",
  dj_library_failed: "DJ：无法读取你的播放列表（{error}），将不加过滤地推荐",
  dj_topped_up: "DJ：已补充队列",
  dj_nothing_queued: "DJ：这次未加入任何曲目。请再试一次，或换个方向。",
  dj_error: "DJ 错误（{backend}）：{error}",
  dj_steps_exhausted: "已在 {count} 步后停止。如果结果不符合预期，请再次提问。",
  history_plays: "{count} 次播放 · {duration}",
  history_track_hits: "曲目播放 {count} 次 · {duration}",
  history_minutes: "{minutes} 分钟",
  history_hours_minutes: "{hours} 小时 {minutes} 分钟",
  setup_no_api_key: "{agent}（无需 API key）",
  setup_keep_command: "{command}（保留）",
  setup_anthropic: "anthropic（API key）",
  setup_openai: "openai_compat（本地或自托管）",
  setup_own_command: "使用你自己的命令，按原样运行",
  setup_not_installed: " · 不在 PATH 中",
  setup_anthropic_billing: "Anthropic API，按 token 计费",
  setup_needs_key: "需要 {variable}（或 behavior.dj_api_key）",
  setup_openai_endpoint: "任意兼容 OpenAI 的端点，如 localhost 上的 Ollama",
  setup_claude_plan: "使用你的 Claude Pro/Max 套餐",
  setup_codex_plan: "使用你的 ChatGPT 套餐",
  setup_agy_plan: "使用你的 Antigravity 套餐",
  setup_copilot_plan: "使用你的 GitHub Copilot 套餐",
  setup_opencode_plan: "使用 opencode 当前登录的服务提供商",
  setup_gemini_legacy: "旧版 Gemini CLI，已由 agy 取代",
  setup_agent_subscription: "使用该 CLI 自己的订阅",
  setup_keep_as_is: "保留此命令不变",
  setup_own_model_note: "spotatui 不会向自定义命令添加模型参数",
  setup_current: "当前配置",
  setup_cli_default: "使用 CLI 的默认模型",
  setup_no_model_flag: "不传入模型参数",
  setup_builtin_default: "使用内置默认模型（{model}）",
  setup_custom: "自定义…",
  setup_type_provider_model: "输入模型名称，格式为 provider/model",
  setup_type_model: "输入模型名称",
  setup_haiku_note: "最省费用，对 Pro 套餐额度压力最小",
  setup_sonnet_note: "均衡",
  setup_opus_note: "用量较大，Pro 套餐容易用满额度",
  setup_fable_note: "用量最大，更容易用满额度",
  setup_cheapest: "最省费用",
  setup_heaviest_gemini: "Gemini 中用量最大",
  setup_heaviest_all: "用量最大",
  setup_price_haiku: "每百万 token $1/$5，200K 上下文",
  setup_price_sonnet: "每百万 token $3/$15，1M 上下文",
  setup_price_opus: "每百万 token $5/$25，1M 上下文",
  setup_price_fable: "每百万 token $10/$50，1M 上下文",
  history_week: "最近 7 天",
  history_month: "最近 30 天",
  history_all: "全部时间",
  history_this_month: "本月",
  history_this_year: "今年",
  mcp_status: "MCP：{message}",
  network_error: "Spotify 请求失败：{error}",
  network_rate_error: "Spotify 请求受限：{error}",
  monthly_recap_ready: "月度收听回顾已生成于 {path}（{count} 次收听）",
  queue_local_source: "本地",
  unnamed_folder: "未命名文件夹",
  local_root_folder: "（根目录）",
  local_unknown_track: "未知曲目",
  engine_next_launch: "新的播放引擎将在下次启动时生效。",
  engine_switching_librespot: "正在将播放引擎切换到 librespot。",
  engine_switching_soloist: "正在将播放引擎切换到 Soloist。",
  soloist_bad_key: "Soloist 拒绝了此 API 密钥。请在设置的“Soloist API 密钥”中填入新密钥。",
  soloist_dir_in_use: "另一个 Soloist 正在使用 spotatui 的 Soloist 数据文件夹。",
  soloist_download_failed: "Soloist 下载失败：{error}",
  soloist_download_started: "正在从 Spotify 下载 Soloist。使用 Soloist 即表示你接受 Spotify 使用条款。",
  soloist_downloaded: "Soloist 下载完成，正在启动。",
  soloist_downloaded_engine_off: "Soloist 下载完成。将播放引擎设为 Soloist 即可通过它播放。",
  soloist_downloaded_next_launch: "Soloist 下载完成，spotatui 将从下次启动起使用它。",
  soloist_downloading: "正在从 Spotify 下载 Soloist：{percent}%",
  soloist_dropped: "Soloist 音质降至 {quality}。",
  soloist_engine_unavailable: "此 spotatui 版本没有 Soloist 引擎，改用 librespot 播放。",
  soloist_expired: "Soloist 版本已过期。请在设置的“Soloist 路径”中选择“从 Spotify 下载”获取新版本。",
  soloist_expires_soon: "Soloist 版本将在 {count} {unit}后过期，请尽快下载新版本。",
  soloist_unit_day: "天",
  soloist_unit_days: "天",
  soloist_unit_hour: "小时",
  soloist_unit_hours: "小时",
  soloist_unit_minute: "分钟",
  soloist_unit_minutes: "分钟",
  soloist_keeps_exiting: "Soloist 反复退出，请查看日志。",
  soloist_key_env_wins: "Soloist API 密钥已保存，但已设置环境变量 SPOTATUI_SOLOIST_API_KEY，会优先使用环境变量。",
  soloist_key_missing: "Soloist 需要 API 密钥。请在设置的“Soloist API 密钥”中选择“获取密钥”，然后填入密钥（保存在 {path}）。",
  soloist_key_restarting: "Soloist API 密钥已保存，正在用它重启 Soloist。",
  soloist_key_save_failed: "无法保存 Soloist API 密钥：{error}",
  soloist_key_saved: "Soloist API 密钥已保存。",
  soloist_lossy_hint: "Soloist 正在播放 {quality}。如需无损，请在 Spotify 手机版的设备菜单中选择 spotatui 并设为无损。",
  soloist_missing: "在 {path} 找不到 Soloist。请在设置的“Soloist 路径”中选择“从 Spotify 下载”。",
  soloist_no_build: "Spotify 没有适用于此系统的 Soloist 版本。",
  soloist_no_websocket: "Soloist 已启动，但没有打开 WebSocket。",
  soloist_not_executable: "{path} 处的 Soloist 不可执行：chmod +x {path}",
  soloist_not_paired: "配对 spotatui：在同一网络的电脑上打开 Spotify，然后选择“spotatui”。",
  soloist_offline: "Soloist 无法连接 Spotify。",
  soloist_paired: "配对成功：spotatui 通过 Soloist 播放。",
  soloist_path_unset: "尚未安装 Soloist。请在设置的“Soloist 路径”中选择“从 Spotify 下载”。",
  soloist_queue_refused: "要把其他音源加入队列排在 Spotify 之后，需要 librespot 引擎。",
  soloist_restart_quality: "正在重启 Soloist 以应用音质设置。",
  soloist_restarting: "Soloist 已停止，正在重启。",
  soloist_start_failed: "Soloist 无法启动：{error}",
  soloist_streams: "Soloist 正在播放 {quality}。",
  soloist_wrong_cpu: "此 Soloist 版本无法在这个 CPU 上运行。请在设置的“Soloist 路径”中选择“从 Spotify 下载”。",
  ..EN
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: InfraStatusStrings = InfraStatusStrings {
  sync_spotify_not_connected: "Spotify 未連線",
  sync_qobuz_not_logged_in: "Qobuz 尚未登入",
  sync_subsonic_not_configured: "尚未設定 Subsonic 伺服器（請設定 behavior.subsonic_url）",
  sync_source_not_compiled: "此版本未編譯 {source}（功能 `{feature}`）",
  sync_source_unsupported: "無法同步 {source} 播放清單",
  sync_source_unreachable: "無法連線至 {source}：{error}",
  search_query_unavailable: "搜尋無法使用：所有已知的 {operation} 查詢識別碼均已失效",
  made_for_you_unavailable: "為你精選無法使用：所有已知的 browsePage 查詢識別碼均已失效",
  made_for_you_missing: "Spotify 未傳回為你精選頁面",
  history_track_hit: "曲目播放 {count} 次 · {duration}",
  history_play: "{count} 次播放 · {duration}",
  cast_no_answer: "{address} 沒有回應",
  cast_no_tls: "與 {address} 的 TLS 交握未完成",
  cast_message_size: "Cast 訊息大小為 {size} 位元組",
  cast_malformed: "Cast 訊息格式錯誤",
  cast_no_reply: "{kind} 未收到回覆",
  cast_receiver_failed: "Spotify 接收器未啟動：{reason}",
  cast_client_id_missing: "getInfoResponse 中缺少 clientID",
  cast_login_refused: "喇叭拒絕登入：{reply}",
  cast_access_token_missing: "device-auth 回覆中缺少 accessToken",
  speaker_token_failed: "權杖交換失敗（{status}）：{error}",
  speaker_token_unsupported: "喇叭要求的權杖類型 {kind} 尚未支援",
  speaker_client_id_missing: "getInfo 中缺少 clientID",
  speaker_login_refused: "喇叭拒絕登入：{message}（{error}）",
  speaker_not_joined: "{name} 登入後未連線",
  playlist_changed: "播放清單在載入後發生了變更，請重新開啟後再試",
  spotify_api_error: "Spotify API {status} 請求失敗：{body}",
  local_replay_failed: "無法重新播放本機檔案",
  local_no_tracks: "沒有可播放的本機曲目",
  local_no_folder: "尚未設定本機音樂資料夾（請設定 behavior.local_music_path）",
  local_uri_invalid: "本機檔案 URI 無效：{error}",
  local_play_failed: "無法播放本機檔案：{error}",
  local_task_failed: "本機播放工作失敗：{error}",
  local_audio_unavailable: "本機播放無音訊輸出：{error}",
  audio_init_failed: "音訊輸出初始化失敗：{error}",
  local_scan_failed: "無法掃描音樂資料夾：{error}",
  local_scan_task_failed: "本機資料夾掃描失敗：{error}",
  local_read_failed: "無法讀取資料夾：{error}",
  local_tracks_scan_failed: "本機曲目掃描失敗：{error}",
  local_exhausted: "{message} — 已無可播放曲目，停止播放",
  subsonic_audio_unavailable: "Subsonic 播放無音訊輸出：{error}",
  youtube_audio_unavailable: "YouTube 播放無音訊輸出：{error}",
  radio_audio_unavailable: "電台播放無音訊輸出：{error}",
  qobuz_audio_unavailable: "Qobuz 播放無音訊輸出：{error}",
  subsonic_no_tracks: "沒有可播放的 Subsonic 曲目",
  subsonic_uri_invalid: "Subsonic 曲目 URI 無效",
  subsonic_replay_failed: "無法重新播放 Subsonic 曲目",
  subsonic_playlists_failed: "無法載入 Subsonic 播放清單：{error}",
  subsonic_playlist_failed: "無法載入 Subsonic 播放清單：{error}",
  subsonic_search_failed: "Subsonic 搜尋失敗：{error}",
  subsonic_download_failed: "無法下載 Subsonic 曲目：{error}",
  subsonic_play_failed: "無法播放 Subsonic 曲目：{error}",
  subsonic_task_failed: "Subsonic 播放工作失敗：{error}",
  subsonic_no_server: "尚未設定 Subsonic 伺服器（請設定 behavior.subsonic_url）",
  radio_no_skip: "電台直播無法切換曲目",
  radio_empty: "尚未設定或儲存電台；請搜尋並新增",
  radio_uri_invalid: "電台 URI 無效",
  radio_probe_timeout: "無法播放電台串流：未能及時辨識音訊格式。",
  radio_search_failed: "電台搜尋失敗：{error}",
  radio_open_failed: "無法開啟電台串流：{error}",
  radio_play_failed: "無法播放電台串流：{error}",
  radio_task_failed: "電台播放工作失敗：{error}",
  radio_playing: "♪ {name}（直播）",
  youtube_playlist_missing: "該 YouTube 播放清單已不存在",
  youtube_metadata_missing: "無法取得該影片的中繼資料",
  youtube_no_tracks: "沒有可播放的 YouTube 影片",
  youtube_no_live: "YouTube 來源暫不支援直播",
  youtube_uri_invalid: "YouTube URI 無效",
  youtube_replay_failed: "無法重新播放 YouTube 曲目",
  youtube_search_failed: "YouTube 搜尋失敗：{error}",
  youtube_playlists_failed: "無法載入 YouTube 播放清單：{error}",
  youtube_create_failed: "無法建立 YouTube 播放清單：{error}",
  youtube_delete_failed: "無法刪除 YouTube 播放清單：{error}",
  youtube_add_failed: "無法將影片加入播放清單：{error}",
  youtube_remove_failed: "無法從播放清單移除影片：{error}",
  youtube_download_failed: "無法下載 YouTube 音訊：{error}",
  youtube_task_failed: "YouTube 播放工作失敗：{error}",
  youtube_created: "已建立 YouTube 播放清單「{name}」",
  youtube_deleted: "已刪除 YouTube 播放清單「{name}」",
  youtube_removed: "已從播放清單移除「{name}」",
  fetching: "正在取得 {name}…",
  youtube_added: "已將「{name}」加入 {playlist}",
  youtube_already_added: "「{name}」已在 {playlist} 中",
  youtube_decode_failed: "無法播放 YouTube 音訊：{error}（安裝 ffmpeg 可解決大多數容器格式問題）",
  qobuz_login_expired: "Qobuz：登入已過期，請按 `d` 並選擇 Qobuz 重新登入",
  qobuz_login_busy: "Qobuz 正在登入…",
  qobuz_browser_login: "Qobuz：請開啟瀏覽器視窗登入",
  qobuz_logged_in: "Qobuz：已登入",
  qobuz_no_tracks: "沒有可播放的 Qobuz 曲目",
  qobuz_uri_invalid: "Qobuz 曲目 URI 無效",
  qobuz_replay_failed: "無法重新播放 Qobuz 曲目",
  qobuz_login_url: "請在瀏覽器中開啟此 URL 登入 Qobuz：{url}",
  qobuz_login_failed: "Qobuz 登入失敗：{error}",
  qobuz_step_constants: "網頁播放器參數",
  qobuz_step_login: "登入",
  qobuz_step_library: "音樂庫",
  qobuz_step_tracks: "曲目",
  qobuz_step_search: "搜尋",
  qobuz_step_stream: "音訊串流",
  qobuz_operation_failed: "Qobuz：{step}失敗：{error}",
  music_busy_playback: "Music 忙碌中，請重試播放請求",
  music_wait_pause: "正在等待 Music 暫停，再切換來源",
  music_not_running: "Music 未執行，請選擇 Music 曲目啟動",
  music_shuffle_repeat: "請在 Music 中更改隨機播放或循環播放",
  music_switch_spotify: "播放 Spotify 曲目即可從 Music 切換",
  music_busy: "Music 忙碌中，請重試請求",
  music_queue_unsupported: "Apple Music 曲目無法加入佇列",
  music_loading: "正在載入 Apple Music 音樂庫：{end}/{total}",
  music_loaded: "Apple Music：已載入 {count} 項",
  music_status_unavailable: "無法取得 Music 狀態：{error}。正在背景重試。",
  queue_repeat_unsupported: "此來源不支援循環播放",
  queue_shuffle_unsupported: "此來源不支援隨機播放",
  queue_no_uri: "已略過佇列中缺少 URI 的曲目",
  queue_finished: "佇列播放完畢",
  audio_disconnected: "音訊輸出裝置已中斷連線。",
  radio_not_resumed: "播放已暫停，未恢復電台播放。",
  source_playback_unavailable: "此版本不支援 {source} 播放",
  queue_skip_disconnected: "內建串流未連線，已略過「{name}」",
  queue_play_failed: "無法播放 {name}：{error}",
  queue_restore_failed: "無法恢復 {name}：{error}",
  queue_playing: "♪ {name}（佇列）",
  queue_audio_unavailable: "佇列播放無音訊輸出：{error}",
  spotify_not_connected: "Spotify 未連線。請按 `d` 並選擇 Spotify 登入。",
  playlist_id_invalid: "播放清單識別碼無效。",
  music_requires_macos: "Apple Music 需要 macOS 和 apple-music 功能",
  spotify_not_compiled: "此版本的 spotatui 不支援 Spotify",
  spotify_connected: "Spotify 已連線。重新啟動 spotatui 以啟用內建串流。",
  party_disconnected: "一起聽已結束：Spotify 未連線。",
  party_ended: "一起聽已結束",
  spotify_login_timeout: "Spotify 登入逾時。請按 `d` 並選擇 Spotify 重試。",
  spotify_login_failed: "Spotify 登入失敗：{error}",
  spotify_rate_wait: "Spotify 請求受限：等待 {seconds} 秒",
  spotify_pair: "登入 Spotify：開啟 {url} 並確認配對碼 {code}",
  recap_created: "聆聽回顧已產生於 {path}（{count} 次聆聽）",
  history_failed: "無法載入聆聽紀錄：{error}",
  recap_failed: "產生聆聽回顧失敗：{error}",
  party_start_failed: "發起一起聽失敗：{error}",
  party_join_failed: "加入一起聽失敗：{error}",
  party_error: "一起聽：{error}",
  party_joined: "{name} 加入了一起聽",
  party_left: "{name} 離開了一起聽",
  native_reconnecting: "正在重新連線內建串流…",
  native_settling: "內建串流仍在準備中…",
  shuffle_not_applied: "已開始播放，但無法啟用隨機播放。",
  spotify_rate_retry: "Spotify 請求受限。正在自動重試，請稍候。",
  spotify_poll_retry: "取得播放狀態時發生暫時性 Spotify 網路錯誤，正在自動重試。",
  spotify_server_retry: "Spotify 伺服器暫時無法使用（5xx），正在自動重試。",
  spotify_poll_rejected: "Spotify 持續拒絕播放狀態請求，正在自動重試。",
  native_resuming: "正在此裝置恢復上次播放…",
  native_reconnect_resume: "正在重新連線內建串流，隨後將恢復播放。",
  native_connection_restored: "內建串流連線已恢復，正在恢復播放。",
  native_no_state: "內建串流連線已恢復，但沒有可恢復的播放狀態。",
  native_restore_retry: "內建串流恢復失敗，正在再次連線。",
  other_source_playing: "其他來源正在播放",
  spotify_playing_elsewhere: "仍在另一台 Spotify 裝置上播放。",
  queue_added: "已加入佇列",
  queue_load_failed: "無法載入佇列（沒有使用中的裝置？）",
  native_nothing_to_resume: "{device} 上沒有可恢復的播放。",
  native_start_failed: "啟動內建串流失敗：{error}",
  native_shuffle_failed: "啟用隨機播放失敗：{error}",
  native_resume_failed: "恢復播放失敗：{error}",
  shuffle_context_failed: "隨機播放：無法載入所屬清單的曲目",
  shuffle_capped: "清單較大：隨機播放前 {count} 首曲目",
  spotify_profile_retry: "載入個人資料時 Spotify 請求受限，正在自動重試。",
  friend_added: "已新增好友",
  friend_unfollowed: "已取消追蹤",
  friend_added_code: "已新增好友（代碼：{code}）",
  friend_add_failed: "無法新增好友：{error}",
  friend_unfollow_failed: "取消追蹤失敗：{error}",
  playlist_external_unavailable: "Spotify Development Mode 禁止讀取其他使用者擁有的播放清單內容。只能存取你擁有或參與協作的播放清單。",
  liked_check_failed: "無法檢查曲目的喜歡狀態：{error}",
  playlist_search_scope: "（僅搜尋前 {count} 首曲目）",
  playlist_search_matches: "播放清單中有 {count} 首曲目符合「{query}」{scope}",
  playlist_refresh_partial: "播放清單重新整理不完整，已保留原清單。",
  playlist_added: "已加入播放清單",
  playlist_removed: "已從播放清單移除",
  playlist_created_no_id: "播放清單已建立，但無法取得其 ID",
  playlist_added_named: "已加入 {name}",
  playlist_created: "播放清單「{name}」已建立！",
  native_recovered: "內建串流已恢復。",
  native_lost_reconnecting: "內建串流連線已中斷，正在重新連線。",
  native_reconnected: "內建串流連線已恢復。",
  native_restored: "內建串流已恢復。",
  native_restored_paused: "內建串流已恢復，保持暫停。",
  native_skip_unavailable: "無法透過內建串流播放此曲目（無法使用或受限制），正在略過。",
  native_many_unavailable: "多首曲目連續無法透過內建串流播放，已停止播放。你的帳號或所在地區可能無法使用這些曲目。請按 'd' 切換到官方 Spotify Connect 裝置。",
  native_recovery_dropped: "內建串流恢復失敗，播放請求已放棄：{error}",
  native_recovery_failed: "內建串流恢復失敗：{error}",
  native_lost_recovering: "內建串流連線已中斷，正在嘗試恢復。",
  native_stalled: "內建串流已停滯，正在嘗試恢復。",
  native_stopped: "內建串流已停止。",
  native_moved: "播放已轉移到另一台 Spotify 裝置。",
  native_disconnected: "內建串流已中斷，正在嘗試恢復。",
  native_stopped_recovering: "內建串流已停止，正在嘗試恢復。",
  sync_busy: "播放清單同步已在執行",
  sync_no_links: "播放清單同步：沒有連結",
  sync_dry_run: "（模擬執行）",
  sync_removed: "播放清單連結已移除，鏡像播放清單保留",
  sync_failed: "播放清單同步失敗：{error}",
  sync_link_failed: "播放清單連結失敗：{error}",
  sync_remove_failed: "移除連結失敗：{error}",
  sync_summary: "播放清單同步{dry}：新增 {added} 首，移除 {removed} 首，未對應 {unmatched} 首",
  sync_using_playlist: "使用現有 {source} 播放清單「{name}」",
  sync_progress: "播放清單同步：{name} {done}/{total}",
  dj_indexed: "已索引 {playlists} 個播放清單中的 {tracks} 首曲目",
  dj_index_capped: "（已在 {count} 首處停止，其餘曲目未篩選）",
  dj_queued: "已加入佇列 {count} 首",
  dj_not_found: "找不到 {count} 首",
  dj_owned: "已擁有 {count} 首",
  dj_duplicate: "已在佇列中 {count} 首",
  dj_status: "DJ：{summary}",
  dj_library_failed: "DJ：無法讀取你的播放清單（{error}），將不加篩選地推薦",
  dj_topped_up: "DJ：已補充佇列",
  dj_nothing_queued: "DJ：這次未加入任何曲目。請再試一次，或換個方向。",
  dj_error: "DJ 錯誤（{backend}）：{error}",
  dj_steps_exhausted: "已在 {count} 步後停止。如果結果不符合預期，請再次提問。",
  history_plays: "{count} 次播放 · {duration}",
  history_track_hits: "曲目播放 {count} 次 · {duration}",
  history_minutes: "{minutes} 分鐘",
  history_hours_minutes: "{hours} 小時 {minutes} 分鐘",
  setup_no_api_key: "{agent}（不需 API key）",
  setup_keep_command: "{command}（保留）",
  setup_anthropic: "anthropic（API key）",
  setup_openai: "openai_compat（本機或自行架設）",
  setup_own_command: "使用你自己的指令，依原樣執行",
  setup_not_installed: " · 不在 PATH 中",
  setup_anthropic_billing: "Anthropic API，依 token 計費",
  setup_needs_key: "需要 {variable}（或 behavior.dj_api_key）",
  setup_openai_endpoint: "任何相容 OpenAI 的端點，例如 localhost 上的 Ollama",
  setup_claude_plan: "使用你的 Claude Pro/Max 方案",
  setup_codex_plan: "使用你的 ChatGPT 方案",
  setup_agy_plan: "使用你的 Antigravity 方案",
  setup_copilot_plan: "使用你的 GitHub Copilot 方案",
  setup_opencode_plan: "使用 opencode 目前登入的服務供應商",
  setup_gemini_legacy: "舊版 Gemini CLI，已由 agy 取代",
  setup_agent_subscription: "使用該 CLI 自己的訂閱",
  setup_keep_as_is: "保留此指令不變",
  setup_own_model_note: "spotatui 不會向自訂指令新增模型參數",
  setup_current: "目前設定",
  setup_cli_default: "使用 CLI 的預設模型",
  setup_no_model_flag: "不傳入模型參數",
  setup_builtin_default: "使用內建預設模型（{model}）",
  setup_custom: "自訂…",
  setup_type_provider_model: "輸入模型名稱，格式為 provider/model",
  setup_type_model: "輸入模型名稱",
  setup_haiku_note: "最省費用，對 Pro 方案額度負擔最小",
  setup_sonnet_note: "均衡",
  setup_opus_note: "用量較大，Pro 方案容易用滿額度",
  setup_fable_note: "用量最大，更容易用滿額度",
  setup_cheapest: "最省費用",
  setup_heaviest_gemini: "Gemini 中用量最大",
  setup_heaviest_all: "用量最大",
  setup_price_haiku: "每百萬 token $1/$5，200K 上下文",
  setup_price_sonnet: "每百萬 token $3/$15，1M 上下文",
  setup_price_opus: "每百萬 token $5/$25，1M 上下文",
  setup_price_fable: "每百萬 token $10/$50，1M 上下文",
  history_week: "最近 7 天",
  history_month: "最近 30 天",
  history_all: "全部時間",
  history_this_month: "本月",
  history_this_year: "今年",
  mcp_status: "MCP：{message}",
  network_error: "Spotify 請求失敗：{error}",
  network_rate_error: "Spotify 請求受限：{error}",
  monthly_recap_ready: "每月聆聽回顧已產生於 {path}（{count} 次聆聽）",
  queue_local_source: "本機",
  unnamed_folder: "未命名資料夾",
  local_root_folder: "（根目錄）",
  local_unknown_track: "未知曲目",
  engine_next_launch: "新的播放引擎將在下次啟動時生效。",
  engine_switching_librespot: "正在將播放引擎切換到 librespot。",
  engine_switching_soloist: "正在將播放引擎切換到 Soloist。",
  soloist_bad_key: "Soloist 拒絕了此 API 金鑰。請在設定的「Soloist API 金鑰」中填入新金鑰。",
  soloist_dir_in_use: "另一個 Soloist 正在使用 spotatui 的 Soloist 資料目錄。",
  soloist_download_failed: "Soloist 下載失敗：{error}",
  soloist_download_started: "正在從 Spotify 下載 Soloist。使用 Soloist 即表示你接受 Spotify 使用條款。",
  soloist_downloaded: "Soloist 下載完成，正在啟動。",
  soloist_downloaded_engine_off: "Soloist 下載完成。將播放引擎設為 Soloist 即可透過它播放。",
  soloist_downloaded_next_launch: "Soloist 下載完成，spotatui 將從下次啟動起使用它。",
  soloist_downloading: "正在從 Spotify 下載 Soloist：{percent}%",
  soloist_dropped: "Soloist 音質降至 {quality}。",
  soloist_engine_unavailable: "此 spotatui 版本沒有 Soloist 引擎，改用 librespot 播放。",
  soloist_expired: "Soloist 版本已過期。請在設定的「Soloist 路徑」中選擇「從 Spotify 下載」取得新版本。",
  soloist_expires_soon: "Soloist 版本將在 {count} {unit}後過期，請盡快下載新版本。",
  soloist_unit_day: "天",
  soloist_unit_days: "天",
  soloist_unit_hour: "小時",
  soloist_unit_hours: "小時",
  soloist_unit_minute: "分鐘",
  soloist_unit_minutes: "分鐘",
  soloist_keeps_exiting: "Soloist 反覆結束，請查看日誌。",
  soloist_key_env_wins: "Soloist API 金鑰已儲存，但已設定環境變數 SPOTATUI_SOLOIST_API_KEY，會優先使用環境變數。",
  soloist_key_missing: "Soloist 需要 API 金鑰。請在設定的「Soloist API 金鑰」中選擇「取得金鑰」，然後填入金鑰（儲存在 {path}）。",
  soloist_key_restarting: "Soloist API 金鑰已儲存，正在用它重新啟動 Soloist。",
  soloist_key_save_failed: "無法儲存 Soloist API 金鑰：{error}",
  soloist_key_saved: "Soloist API 金鑰已儲存。",
  soloist_lossy_hint: "Soloist 正在播放 {quality}。如需無損，請在 Spotify 手機版的裝置選單中選擇 spotatui 並設為無損。",
  soloist_missing: "在 {path} 找不到 Soloist。請在設定的「Soloist 路徑」中選擇「從 Spotify 下載」。",
  soloist_no_build: "Spotify 沒有適用於此系統的 Soloist 版本。",
  soloist_no_websocket: "Soloist 已啟動，但沒有開啟 WebSocket。",
  soloist_not_executable: "{path} 的 Soloist 無法執行：chmod +x {path}",
  soloist_not_paired: "配對 spotatui：在同一網路的電腦上開啟 Spotify，然後選擇「spotatui」。",
  soloist_offline: "Soloist 無法連線至 Spotify。",
  soloist_paired: "配對成功：spotatui 透過 Soloist 播放。",
  soloist_path_unset: "尚未安裝 Soloist。請在設定的「Soloist 路徑」中選擇「從 Spotify 下載」。",
  soloist_queue_refused: "要把其他來源加入佇列排在 Spotify 之後，需要 librespot 引擎。",
  soloist_restart_quality: "正在重新啟動 Soloist 以套用音質設定。",
  soloist_restarting: "Soloist 已停止，正在重新啟動。",
  soloist_start_failed: "Soloist 無法啟動：{error}",
  soloist_streams: "Soloist 正在播放 {quality}。",
  soloist_wrong_cpu: "此 Soloist 版本無法在這個 CPU 上執行。請在設定的「Soloist 路徑」中選擇「從 Spotify 下載」。",
  ..EN
};
