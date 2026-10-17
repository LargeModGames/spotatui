//! Localized application messages.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct AppStatusStrings {
  pub music_error: &'static str,
  pub audio_backend_failed: &'static str,
  pub audio_backend_no_alsa_default: &'static str,
  pub audio_device_paused: &'static str,
  pub audio_device_moved: &'static str,
  pub audio_device_waiting: &'static str,
  pub radio_device_lost: &'static str,
  pub radio_ended: &'static str,
  pub current_source_cannot_like: &'static str,

  pub loading_playlist: &'static str,
  pub community_pin_hidden: &'static str,
  pub native_disconnected: &'static str,
  pub native_reconnecting: &'static str,
  pub native_request_expired: &'static str,
  pub resuming_request: &'static str,
  pub track_cannot_add: &'static str,
  pub youtube_no_playlists: &'static str,
  pub playlist_destinations_loading: &'static str,
  pub no_editable_playlists: &'static str,
  pub playlist_tracks_only: &'static str,
  pub playlist_needs_spotify: &'static str,
  pub no_current_track: &'static str,
  pub youtube_no_open_playlist: &'static str,
  pub youtube_playlist: &'static str,
  pub track_cannot_edit: &'static str,
  pub remove_requires_playlist: &'static str,
  pub remove_position_unknown: &'static str,
  pub settings_shortcut_fallback: &'static str,
  pub queued_no_album: &'static str,
  pub jump_album_spotify: &'static str,
  pub playing_no_artist: &'static str,
  pub queued_no_artist: &'static str,
  pub jump_artist_spotify: &'static str,
  pub queue_no_context: &'static str,
  pub jump_context_spotify: &'static str,
  pub friend_code_loading: &'static str,
  pub clipboard_unavailable: &'static str,
  pub clipboard_copy_failed: &'static str,
  pub no_playlist_link: &'static str,
  pub highlight_playlist_mirror: &'static str,
  pub no_mirror_source: &'static str,
  pub radio_no_url: &'static str,
  pub radio_position_missing: &'static str,
  pub another_source_playing: &'static str,
  pub track_no_id: &'static str,
  pub album_no_id: &'static str,
  pub music_wait_pause: &'static str,
  pub music_volume_floor: &'static str,
  pub music_did_not_start: &'static str,
  pub music_quit: &'static str,
  pub music_requires_macos: &'static str,
  pub all_songs: &'static str,
  pub music_track_unavailable: &'static str,
  pub queue_no_uri: &'static str,
  pub radio_cannot_queue: &'static str,
  pub shuffle_on: &'static str,
  pub shuffle_off: &'static str,
  pub repeat_off: &'static str,
  pub repeat_all: &'static str,
  pub repeat_one: &'static str,
  pub shuffle_unsupported: &'static str,
  pub repeat_unsupported: &'static str,
  pub recap_disabled: &'static str,
  pub native_no_response: &'static str,
  pub native_restore_failed: &'static str,
  pub dj_previous_request: &'static str,
  pub dj_auto_queue_wait: &'static str,
  pub dj_auto_queue_on: &'static str,
  pub dj_auto_queue_off: &'static str,
  pub dj_fresh_only: &'static str,
  pub dj_everything: &'static str,
  pub dj_busy: &'static str,
  pub dj_shifting: &'static str,
  pub dj_keep_brain: &'static str,
  pub nothing_playing: &'static str,
  pub spotify_parked: &'static str,
  pub spotify_not_connected: &'static str,
  pub party_needs_spotify: &'static str,
  pub music_cannot_queue: &'static str,
  pub friend_code_copied: &'static str,
  pub sync_hint: &'static str,
  pub mirroring: &'static str,
  pub radio_favorited: &'static str,
  pub radio_already_favorited: &'static str,
  pub radio_favorite_failed: &'static str,
  pub radio_configured: &'static str,
  pub radio_removed: &'static str,
  pub radio_not_favorited: &'static str,
  pub radio_remove_failed: &'static str,
  pub state_save_failed: &'static str,
  pub shortcut_save_failed: &'static str,
  pub shortcut_saved: &'static str,
  pub clipboard_set_failed: &'static str,
  pub queued: &'static str,
  pub recap_browser_failed: &'static str,
  pub dj_shift_dropped: &'static str,
  pub dj_brain: &'static str,
  pub dj_brain_not_ready: &'static str,
  pub dj_save_failed: &'static str,
  pub playlist_searching: &'static str,
  pub unfollow_profile_loading: &'static str,
  pub no_playback_device_selected: &'static str,
  pub no_playback_devices: &'static str,
  pub playback_device_unavailable: &'static str,
  pub playback_device_no_id: &'static str,
  pub entry_not_playable: &'static str,
  pub liked_loading: &'static str,
  pub no_radio_selected: &'static str,
  pub switching_device: &'static str,
  pub source_selected: &'static str,
  pub announcement_save_failed: &'static str,
  pub link_open_failed: &'static str,
  pub capture_unavailable: &'static str,
  pub no_radio_playing: &'static str,
  pub search_needs_spotify: &'static str,
  pub copy_needs_spotify: &'static str,
  pub shortcut_session_only: &'static str,
  pub like_source_unavailable: &'static str,
  pub search_source_unavailable: &'static str,
  pub paused: &'static str,
  pub paused_details: &'static str,
  pub visualizer_config: &'static str,
}

pub const EN: AppStatusStrings = AppStatusStrings {
  music_error: "Apple Music: {error}",
  audio_backend_failed: "Audio backend failed: {error}",
  audio_backend_no_alsa_default:
    "No audio output: ALSA has no default device (on PipeWire, install pipewire-alsa)",
  audio_device_paused: "Audio device changed - playback paused here.",
  audio_device_moved: "Audio device changed - playback moved to it.",
  audio_device_waiting: "Audio output device lost - waiting for a new one.",
  radio_device_lost: "Audio output device disconnected - radio stopped.",
  radio_ended: "Radio stream ended",
  current_source_cannot_like: "The current playback source cannot be liked",

  loading_playlist: "Loading playlist…",
  community_pin_hidden: "Community playlist pin hidden (re-enable in Settings)",
  native_disconnected: "Native streaming disconnected; attempting recovery.",
  native_reconnecting: "Reconnecting native streaming…",
  native_request_expired: "Playback request expired during native recovery.",
  resuming_request: "Resuming playback request…",
  track_cannot_add: "Track cannot be added to playlist",
  youtube_no_playlists: "No YouTube playlists yet — create one from the sidebar",
  playlist_destinations_loading: "Playlist destinations loading, try again",
  no_editable_playlists: "No editable playlists available",
  playlist_tracks_only: "Only tracks can be added to playlists",
  playlist_needs_spotify: "Add to playlist needs a Spotify track playing",
  no_current_track: "No track currently playing",
  youtube_no_open_playlist: "No YouTube playlist is open",
  youtube_playlist: "YouTube playlist",
  track_cannot_edit: "Track cannot be edited in playlist",
  remove_requires_playlist: "Remove only works in selected playlist views",
  remove_position_unknown: "Cannot resolve track position for removal",
  settings_shortcut_fallback: "Ctrl+, not detected in this terminal; using Alt+, for this session",
  queued_no_album: "The queued track has no album id",
  jump_album_spotify: "Jump to album needs a Spotify track playing",
  playing_no_artist: "The playing track has no artist id",
  queued_no_artist: "The queued track has no artist id",
  jump_artist_spotify: "Jump to artist needs a Spotify track playing",
  queue_no_context: "The queue slot has no play context",
  jump_context_spotify: "Jump to context needs a Spotify track playing",
  friend_code_loading: "Friend code not loaded yet",
  clipboard_unavailable: "Clipboard not available",
  clipboard_copy_failed: "Failed to copy to clipboard",
  no_playlist_link: "No playlist link is highlighted",
  highlight_playlist_mirror: "Highlight a playlist to mirror it",
  no_mirror_source: "No other source can take a mirror of this playlist",
  radio_no_url: "Radio station has no stream URL",
  radio_position_missing: "added radio station was not found at the expected position; not persisted",
  another_source_playing: "Another source owns playback",
  track_no_id: "Track has no ID",
  album_no_id: "Album has no ID",
  music_wait_pause: "Waiting for Music to pause before switching source",
  music_volume_floor: "Apple Music: the volume stops just above 0 (Music refuses to play at 0)",
  music_did_not_start: "Apple Music: Music did not start playing. Check the Music window for a dialog or an unavailable track",
  music_quit: "Apple Music: Music quit",
  music_requires_macos: "Apple Music requires macOS and the apple-music feature",
  all_songs: "All songs",
  music_track_unavailable: "Apple Music: Music cannot play this track (it is no longer available, or its file is missing)",
  queue_no_uri: "Cannot queue: track has no URI",
  radio_cannot_queue: "Radio stations can't be queued",
  shuffle_on: "Shuffle: On",
  shuffle_off: "Shuffle: Off",
  repeat_off: "Repeat: Off",
  repeat_all: "Repeat: All",
  repeat_one: "Repeat: One",
  shuffle_unsupported: "Shuffle does not apply to this source",
  repeat_unsupported: "Repeat does not apply to this source",
  recap_disabled: "Monthly recap prompt disabled (re-enable in Settings)",
  native_no_response: "Native playback did not respond after recovery; request dropped.",
  native_restore_failed: "Native connection recovered, but playback could not be restored.",
  dj_previous_request: "The DJ is still working on the last request",
  dj_auto_queue_wait: "DJ auto-queue on — waits while another Spotify device is playing",
  dj_auto_queue_on: "DJ auto-queue on",
  dj_auto_queue_off: "DJ auto-queue off",
  dj_fresh_only: "DJ: only tracks you don't already have",
  dj_everything: "DJ: recommending from everything",
  dj_busy: "The DJ is already working on something",
  dj_shifting: "Shifting the vibe",
  dj_keep_brain: "keeping the current DJ brain",
  nothing_playing: "Nothing is playing",
  spotify_parked: "Press play to resume Spotify",
  spotify_not_connected: "Spotify not connected. Press `d` and pick Spotify to log in.",
  party_needs_spotify: "Listening Party needs Spotify. Press `d` and pick Spotify to log in.",
  music_cannot_queue: "Apple Music tracks can't be queued",
  friend_code_copied: "Copied friend code: {code}",
  sync_hint: "Playlist sync: {hint}",
  mirroring: "Mirroring {name} onto {source}",
  radio_favorited: "Favorited radio station: {name}",
  radio_already_favorited: "Radio station already favorited: {name}",
  radio_favorite_failed: "Could not favorite radio station: {error}",
  radio_configured: "Radio station is configured in config.yml: {name}",
  radio_removed: "Removed saved radio station: {name}",
  radio_not_favorited: "Radio station is not favorited: {name}",
  radio_remove_failed: "Could not remove radio station: {error}",
  state_save_failed: "Failed to save state: {error}",
  shortcut_save_failed: "Failed to save keybinding fallback: {error}",
  shortcut_saved: "Saved open settings shortcut as {key}",
  clipboard_set_failed: "failed to set clipboard content: {error}",
  queued: "Queued: {name}",
  recap_browser_failed: "Recap saved at {path} (couldn't open browser)",
  dj_shift_dropped: "Shifting the vibe (dropped {dropped} queued track(s))",
  dj_brain: "DJ brain: {name}",
  dj_brain_not_ready: "DJ brain: {name} (not usable yet: install it, or set its API key)",
  dj_save_failed: "DJ: could not save that choice: {error}",
  playlist_searching: "Searching playlist for \"{query}\"...",
  unfollow_profile_loading: "Cannot unfollow: user profile not loaded yet",
  no_playback_device_selected: "No playback device selected",
  no_playback_devices: "No playback devices found",
  playback_device_unavailable: "Selected playback device is no longer available",
  playback_device_no_id: "Selected playback device has no Spotify device id",
  entry_not_playable: "This entry can't be played directly",
  liked_loading: "Liked songs are still loading, please wait...",
  no_radio_selected: "No radio station selected",
  switching_device: "Switching playback to {device}",
  source_selected: "Source: {source}",
  announcement_save_failed: "Failed to persist dismissed announcement: {error}",
  link_open_failed: "Couldn't open {url} in the browser",
  capture_unavailable: "Visualizer can't capture system audio - see the log for details.",
  no_radio_playing: "No radio station selected or playing",
  search_needs_spotify: "Search needs a Spotify session",
  copy_needs_spotify: "Copy URL needs a Spotify track playing",
  shortcut_session_only: "Using Alt+, for this session only",
  like_source_unavailable: "Like isn't available for {source}",
  search_source_unavailable: "Search isn't available for {source}",
  paused: "Paused",
  paused_details: "Paused: {details}",
  visualizer_config: "visualizer.yml: {problem}",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: AppStatusStrings = AppStatusStrings { ..EN };

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: AppStatusStrings = AppStatusStrings { ..EN };

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
impl AppStatusStrings {
  pub fn music_error(&self, error: &str) -> String {
    super::format_template(self.music_error, &[("error", error)])
  }
  pub fn audio_backend_failed(&self, error: &str) -> String {
    super::format_template(self.audio_backend_failed, &[("error", error)])
  }
  pub fn friend_code_copied(&self, code: &str) -> String {
    crate::core::i18n::format_template(self.friend_code_copied, &[("code", code)])
  }
  pub fn sync_hint(&self, hint: &str) -> String {
    crate::core::i18n::format_template(self.sync_hint, &[("hint", hint)])
  }
  pub fn mirroring(&self, name: &str, source: &str) -> String {
    crate::core::i18n::format_template(self.mirroring, &[("name", name), ("source", source)])
  }
  pub fn radio_favorited(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.radio_favorited, &[("name", name)])
  }
  pub fn radio_already_favorited(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.radio_already_favorited, &[("name", name)])
  }
  pub fn radio_favorite_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.radio_favorite_failed, &[("error", error)])
  }
  pub fn radio_configured(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.radio_configured, &[("name", name)])
  }
  pub fn radio_removed(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.radio_removed, &[("name", name)])
  }
  pub fn radio_not_favorited(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.radio_not_favorited, &[("name", name)])
  }
  pub fn radio_remove_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.radio_remove_failed, &[("error", error)])
  }
  pub fn state_save_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.state_save_failed, &[("error", error)])
  }
  pub fn shortcut_save_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.shortcut_save_failed, &[("error", error)])
  }
  pub fn shortcut_saved(&self, key: &str) -> String {
    crate::core::i18n::format_template(self.shortcut_saved, &[("key", key)])
  }
  pub fn clipboard_set_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.clipboard_set_failed, &[("error", error)])
  }
  pub fn queued(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.queued, &[("name", name)])
  }
  pub fn recap_browser_failed(&self, path: &str) -> String {
    crate::core::i18n::format_template(self.recap_browser_failed, &[("path", path)])
  }
  pub fn dj_shift_dropped(&self, dropped: usize) -> String {
    crate::core::i18n::format_template(self.dj_shift_dropped, &[("dropped", &dropped.to_string())])
  }
  pub fn dj_brain(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.dj_brain, &[("name", name)])
  }
  pub fn dj_brain_not_ready(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.dj_brain_not_ready, &[("name", name)])
  }
  pub fn dj_save_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.dj_save_failed, &[("error", error)])
  }
  pub fn playlist_searching(&self, query: &str) -> String {
    crate::core::i18n::format_template(self.playlist_searching, &[("query", query)])
  }
  pub fn switching_device(&self, device: &str) -> String {
    crate::core::i18n::format_template(self.switching_device, &[("device", device)])
  }
  pub fn source_selected(&self, source: &str) -> String {
    crate::core::i18n::format_template(self.source_selected, &[("source", source)])
  }
  pub fn announcement_save_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.announcement_save_failed, &[("error", error)])
  }
  pub fn link_open_failed(&self, url: &str) -> String {
    crate::core::i18n::format_template(self.link_open_failed, &[("url", url)])
  }
  pub fn like_source_unavailable(&self, source: &str) -> String {
    crate::core::i18n::format_template(self.like_source_unavailable, &[("source", source)])
  }
  pub fn search_source_unavailable(&self, source: &str) -> String {
    crate::core::i18n::format_template(self.search_source_unavailable, &[("source", source)])
  }
  pub fn paused_details(&self, details: &str) -> String {
    crate::core::i18n::format_template(self.paused_details, &[("details", details)])
  }
  #[cfg_attr(not(feature = "gui"), allow(dead_code))]
  pub fn visualizer_config(&self, problem: &str) -> String {
    crate::core::i18n::format_template(self.visualizer_config, &[("problem", problem)])
  }
}
