//! Localized terminal screen labels.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct ScreenStrings {
  pub stats_total: &'static str,
  pub folder_track: &'static str,
  pub discover_save: &'static str,
  pub discover_play: &'static str,
  pub discover_open: &'static str,
  pub discover_loading_title: &'static str,
  pub discover_empty: &'static str,
  pub made_for_you_loading: &'static str,
  pub made_for_you: &'static str,
  pub search_playlist: &'static str,
  pub search_liked: &'static str,
  pub search: &'static str,
  pub help: &'static str,
  pub settings: &'static str,
  pub type_help: &'static str,
  pub radio_stations: &'static str,
  pub songs: &'static str,
  pub artists: &'static str,
  pub albums: &'static str,
  pub playlists: &'static str,
  pub podcasts: &'static str,
  pub spotify_playlist_warning: &'static str,
  pub local_files_empty: &'static str,
  pub local_files: &'static str,
  pub artist: &'static str,
  pub recommendations: &'static str,
  pub saved_albums: &'static str,
  pub episodes: &'static str,
  pub recent_tracks: &'static str,
  pub no_top_tracks: &'static str,
  pub related_artists: &'static str,
  pub playing: &'static str,
  pub paused: &'static str,
  pub on: &'static str,
  pub off: &'static str,
  pub repeat_track: &'static str,
  pub repeat_all: &'static str,
  pub local_source_short: &'static str,
  pub radio_source_short: &'static str,
  pub queue: &'static str,
  pub waiting_music: &'static str,
  pub position_stale: &'static str,
  pub no_playback: &'static str,
  pub choose_source_device: &'static str,
  pub choices_cached: &'static str,
  pub welcome_spotatui: &'static str,
  pub source_active: &'static str,
  pub source: &'static str,
  pub spotify_devices: &'static str,
  pub devices: &'static str,
  pub no_devices: &'static str,
  pub timing_estimated: &'static str,
  pub lyrics_title: &'static str,
  pub lyrics_browsing: &'static str,
  pub lyrics_loading: &'static str,
  pub lyrics_from: &'static str,
  pub lyrics_missing: &'static str,
  pub lyrics_credit: &'static str,
  pub nothing_playing: &'static str,
  pub lyrics_start_track: &'static str,
  pub stats: &'static str,
  pub stats_loading: &'static str,
  pub stats_empty: &'static str,
  pub top_tracks: &'static str,
  pub top_artists: &'static str,
  pub top_albums: &'static str,
  pub stats_period_empty: &'static str,
  pub last_ten_days: &'static str,
  pub select_track_hint: &'static str,
  pub play_hint: &'static str,
  pub period_hint: &'static str,
  pub share_card_hint: &'static str,
  pub back_hint: &'static str,
  pub friends_title: &'static str,
  pub account_required: &'static str,
  pub friends_account_line1: &'static str,
  pub friends_account_line2: &'static str,
  pub sign_up_at: &'static str,
  pub sync_token_line1: &'static str,
  pub sync_token_line2: &'static str,
  pub esc_back: &'static str,
  pub loading: &'static str,
  pub copy_code_short: &'static str,
  pub your_code: &'static str,
  pub friends_no_match: &'static str,
  pub friends_loading: &'static str,
  pub friends_empty: &'static str,
  pub friends_no_online: &'static str,
  pub search_placeholder: &'static str,
  pub add_friend_short: &'static str,
  pub idle: &'static str,
  pub offline: &'static str,
  pub navigate_hint: &'static str,
  pub copy_code_hint: &'static str,
  pub add_friend_hint: &'static str,
  pub unfollow_hint: &'static str,
  pub filter_hint: &'static str,
  pub type_search_hint: &'static str,
  pub add_friend_title: &'static str,
  pub friend_code_tab: &'static str,
  pub friend_name_tab: &'static str,
  pub switch_hint: &'static str,
  pub add_hint: &'static str,
  pub cancel_hint: &'static str,
  pub friend_code_placeholder: &'static str,
  pub friend_code_enter: &'static str,
  pub friend_search_placeholder: &'static str,
  pub users_empty: &'static str,
  pub following_tag: &'static str,
  pub discover: &'static str,
  pub navigate: &'static str,
  pub select: &'static str,
  pub playlist_sync: &'static str,
  pub syncing_suffix: &'static str,
  pub playlist_links_empty: &'static str,
  pub details: &'static str,
  pub never: &'static str,
  pub sync_now_hint: &'static str,
  pub remove_link_hint: &'static str,
  pub select_hint: &'static str,
  pub welcome: &'static str,
  pub streak_keep: &'static str,
  pub global_counter_unavailable: &'static str,
  pub global_counter_loading: &'static str,
  pub global_counter_disabled: &'static str,
  pub global_counter_build_disabled: &'static str,
  pub discord_community: &'static str,
  pub spotify_pitch: &'static str,
  pub pitches: &'static str,
  pub spectrum: &'static str,
  pub capturing_audio: &'static str,
  pub capture_paused: &'static str,
  pub no_capture: &'static str,
  pub capture_hint_linux: &'static str,
  pub capture_hint_windows: &'static str,
  pub capture_hint_macos: &'static str,
  pub capture_hint_other: &'static str,
  pub audio_waiting: &'static str,
  pub pitch_unavailable: &'static str,
  pub pitch_failed: &'static str,
  pub pitch_loading: &'static str,
  pub cover_art_not_built: &'static str,
  pub type_key: &'static str,
  pub folder_tracks: &'static str,
  pub artist_top_tracks: &'static str,
  pub songs_searching: &'static str,
  pub songs_filtered: &'static str,
  pub lyrics_offset: &'static str,
  pub lyrics_notes: &'static str,
  pub friends_all: &'static str,
  pub friends_online: &'static str,
  pub day_streak: &'static str,
  pub streak_best_today: &'static str,
  pub stats_totals: &'static str,
  pub listening_streak: &'static str,
  pub listened_today: &'static str,
  pub global_counter: &'static str,
  pub visualization_title: &'static str,
  pub chart_title: &'static str,
  pub peak: &'static str,
  pub plugin_screen_empty: &'static str,
  pub loading_track: &'static str,
  pub shuffle_segment: &'static str,
  pub repeat_segment: &'static str,
  pub party_listeners: &'static str,
  pub party_following: &'static str,
  pub status_title: &'static str,
  pub live_elapsed: &'static str,
  pub device_navigation: &'static str,
  pub reopen_devices: &'static str,
  pub playbar_status: &'static str,
  pub playbar_source_status: &'static str,
  pub ready_status: &'static str,
  pub control_prev: &'static str,
  pub control_play: &'static str,
  pub control_next: &'static str,
  pub control_shuffle: &'static str,
  pub control_repeat: &'static str,
  pub control_like: &'static str,
  pub control_vol_down: &'static str,
  pub control_vol_up: &'static str,
  pub column_title: &'static str,
  pub column_name: &'static str,
  pub column_artist: &'static str,
  pub column_album: &'static str,
  pub column_length: &'static str,
  pub column_year: &'static str,
  pub column_release_date: &'static str,
  pub column_date: &'static str,
  pub column_duration: &'static str,
  pub column_publisher: &'static str,
  pub title_by_artist: &'static str,
  pub recommendations_song: &'static str,
  pub recommendations_artist: &'static str,
  pub no_mirrors: &'static str,
  pub sync_no_match: &'static str,
  pub sync_cannot_match: &'static str,
  pub sync_search_failed: &'static str,
  pub sync_link_row: &'static str,
  pub sync_mirror_detail: &'static str,
  pub sync_last_run: &'static str,
  pub sync_run_totals: &'static str,
  pub sync_run_skipped: &'static str,
  pub sync_run_failed: &'static str,
  pub dj_auto_queue: &'static str,
  pub dj_fresh_indexing: &'static str,
  pub dj_fresh: &'static str,
  pub dj_auto_queue_hint: &'static str,
  pub dj_vibe_hint: &'static str,
  pub dj_model_hint: &'static str,
  pub dj_scroll_hint: &'static str,
  pub dj_send_hint: &'static str,
  pub dj_clear_hint: &'static str,
  pub dj_empty: &'static str,
  pub dj_example_focus: &'static str,
  pub dj_example_slower: &'static str,
  pub dj_user_prefix: &'static str,
  pub dj_thinking: &'static str,
  pub dj_setup: &'static str,
  pub dj_choose_model: &'static str,
  pub dj_model_name: &'static str,
  pub dj_setup_backend_hint: &'static str,
  pub dj_select_hint: &'static str,
  pub dj_choose_hint: &'static str,
  pub dj_back_hint: &'static str,
  pub dj_custom_hint: &'static str,
  pub dj_use_model_hint: &'static str,
  pub dj_ask: &'static str,
  pub dj_using: &'static str,
  pub dj_change_model: &'static str,
  pub dj_thinking_step: &'static str,
  pub dj_endpoint_hint: &'static str,
  pub dj_provider_model_hint: &'static str,
  pub dj_for_backend: &'static str,
  pub more_below: &'static str,
  pub more_above: &'static str,
  pub more_above_below: &'static str,
  pub visualizer_style_hint: &'static str,
  pub bar_graph: &'static str,
  pub minor_key: &'static str,
  pub major_key: &'static str,
  pub cover_loading: &'static str,
  pub cover_source_unavailable: &'static str,
  pub cover_failed: &'static str,
  pub cover_empty: &'static str,
  pub log_location: &'static str,
  pub report_bugs: &'static str,
  pub unknown: &'static str,
  pub album_type_album: &'static str,
  pub album_type_single: &'static str,
  pub album_type_compilation: &'static str,
  pub album_type_appears_on: &'static str,
}

pub const EN: ScreenStrings = ScreenStrings {
  stats_total: "  {plays} play · {duration} listened",
  folder_track: "{name} ({count} track)",
  discover_save: "Save to library",
  discover_play: "Play",
  discover_open: "Open",
  discover_loading_title: "Discover [Loading...]",
  discover_empty: "Nothing here yet.",
  made_for_you_loading: "Loading Made For You…",
  made_for_you: "Made For You",
  search_playlist: "Search Playlist",
  search_liked: "Search Liked Songs",
  search: "Search",
  help: "Help",
  settings: "Settings",
  type_help: "Type ?",
  radio_stations: "Radio Stations",
  songs: "Songs",
  artists: "Artists",
  albums: "Albums",
  playlists: "Playlists",
  podcasts: "Podcasts",
  spotify_playlist_warning: "Cannot display Spotify created playlists. Try a more specific search to find user-created playlists.",
  local_files_empty: "Local Files (no folders — set behavior.local_music_path)",
  local_files: "Local Files",
  artist: "Artist",
  recommendations: "Recommendations",
  saved_albums: "Saved Albums",
  episodes: "Episodes",
  recent_tracks: "Recently Played Tracks",
  no_top_tracks: "No top tracks",
  related_artists: "Related artists",
  playing: "Playing",
  paused: "Paused",
  on: "On",
  off: "Off",
  repeat_track: "Track",
  repeat_all: "All",
  local_source_short: "Local",
  radio_source_short: "Radio",
  queue: "Queue",
  waiting_music: "Waiting for Music…",
  position_stale: "position stale (tick loop stopped)",
  no_playback: "No active playback",
  choose_source_device: "Choose your music source and Spotify playback device.",
  choices_cached: "Your choices are cached so you can jump straight back in when you next open `spotatui`.",
  welcome_spotatui: "Welcome to spotatui!",
  source_active: "  (active)",
  source: "Source",
  spotify_devices: "Devices (Spotify only)",
  devices: "Devices",
  no_devices: "No devices found: Make sure a device is active",
  timing_estimated: "timing estimated",
  lyrics_title: " Lyrics ",
  lyrics_browsing: "browsing · ↑/↓ scroll · Esc follow",
  lyrics_loading: "Fetching lyrics…",
  lyrics_from: "from LRCLIB",
  lyrics_missing: "No lyrics for this track",
  lyrics_credit: "lyrics provided by LRCLIB",
  nothing_playing: "Nothing playing",
  lyrics_start_track: "start a track to see lyrics",
  stats: "Stats",
  stats_loading: "  [Loading...]",
  stats_empty: "No listening history recorded yet. Play something!",
  top_tracks: "Top Tracks",
  top_artists: "Top Artists",
  top_albums: "Top Albums",
  stats_period_empty: "No plays in this period yet.",
  last_ten_days: "Last 10 Days",
  select_track_hint: " Select track  ",
  play_hint: " Play  ",
  period_hint: " Change period  ",
  share_card_hint: " Open share card  ",
  back_hint: " Back",
  friends_title: " Friends ",
  account_required: "Account Required",
  friends_account_line1: "Friends require a spotatui web account to sync your",
  friends_account_line2: "listening data and connect with other users.",
  sign_up_at: "  Sign up at: ",
  sync_token_line1: "After signing up, copy your sync token from the dashboard",
  sync_token_line2: "and paste it into Settings → Behavior → sync_token",
  esc_back: "Press Esc to go back",
  loading: "Loading...",
  copy_code_short: "  c — copy",
  your_code: " YOUR CODE  ",
  friends_no_match: "No friends match your search",
  friends_loading: "Loading friends...",
  friends_empty: "No friends yet — press 'a' to add one!",
  friends_no_online: "No friends online right now",
  search_placeholder: "search…",
  add_friend_short: "+ Add Friend",
  idle: "idle",
  offline: "offline",
  navigate_hint: " Navigate  ",
  copy_code_hint: " Copy code  ",
  add_friend_hint: " Add friend  ",
  unfollow_hint: " Unfollow  ",
  filter_hint: " Filter  ",
  type_search_hint: "type to search  ",
  add_friend_title: " Add Friend ",
  friend_code_tab: " By Friend Code ",
  friend_name_tab: " Search by Name ",
  switch_hint: " Switch  ",
  add_hint: " Add  ",
  cancel_hint: " Cancel",
  friend_code_placeholder: "Enter friend code...",
  friend_code_enter: "Type code then press Enter",
  friend_search_placeholder: "Type a username or code...",
  users_empty: "No users found",
  following_tag: " [following]",
  discover: "Discover",
  navigate: "Navigate",
  select: "Select",
  playlist_sync: "Playlist sync",
  syncing_suffix: " (syncing...)",
  playlist_links_empty: "No playlist links. Highlight a playlist in the sidebar and press m to mirror it.",
  details: "Details",
  never: "never",
  sync_now_hint: " Sync now  ",
  remove_link_hint: " Remove link  ",
  select_hint: " Select  ",
  welcome: "Welcome!",
  streak_keep: " · listen today to keep it!",
  global_counter_unavailable: "Global song counter unavailable right now.",
  global_counter_loading: "Loading global song count...",
  global_counter_disabled: "Global song counter disabled (Settings -> Behavior).",
  global_counter_build_disabled: "Global song counter unavailable (telemetry disabled in this build).",
  discord_community: "Join the community on Discord: https://spotatui.com/discord",
  spotify_pitch: "Spotify Pitch",
  pitches: "Pitches",
  spectrum: "Spectrum",
  capturing_audio: "[>] Capturing audio",
  capture_paused: "[||] Paused",
  no_capture: "No audio capture available",
  capture_hint_linux: "Hint: Ensure PipeWire or PulseAudio is running with a monitor device",
  capture_hint_windows: "Hint: Audio loopback should work automatically on Windows",
  capture_hint_macos: "Hint: macOS requires a virtual audio device like BlackHole",
  capture_hint_other: "Hint: Audio capture may not be supported on this platform",
  audio_waiting: "Waiting for audio input...",
  pitch_unavailable: "Spotify has no pitch analysis for this track",
  pitch_failed: "Could not load the pitch analysis; trying again soon",
  pitch_loading: "Loading the pitch analysis...",
  cover_art_not_built: "Built without cover art",
  type_key: "Type {key}",
  folder_tracks: "{name} ({count} tracks)",
  artist_top_tracks: "{artist} - Top Tracks",
  songs_searching: "Songs (searching: {query}...)",
  songs_filtered: "Songs (filtered: {query})",
  lyrics_offset: "offset {seconds}s",
  lyrics_notes: " Lyrics ({notes}) ",
  friends_all: " All ({count}) ",
  friends_online: " Online ({count}) ",
  day_streak: "{days}-day streak",
  streak_best_today: " (best {days}) · {duration} today",
  stats_totals: "  {plays} plays · {duration} listened",
  listening_streak: "{days}-day listening streak",
  listened_today: " · {duration} today",
  global_counter: "Global songs played with spotatui: {count}",
  visualization_title: "Audio Visualization ({style})",
  chart_title: "{chart} | {fps} FPS | Press {key} to exit",
  peak: "Peak: {percent}%",
  plugin_screen_empty: "plugin screen '{name}' has no content yet",
  loading_track: "{name} (loading…)",
  shuffle_segment: " | Shuffle: {mode}",
  repeat_segment: " | Repeat: {mode}",
  party_listeners: " | Party: {count} listeners",
  party_following: " | Party: following {host}",
  status_title: "Status: {message}",
  live_elapsed: "LIVE • {elapsed}",
  device_navigation: "Use `{down}`/`{up}` or arrow keys to move, `Tab` to switch panels, <Enter> to select.",
  reopen_devices: "Reopen this screen any time by pressing `{key}`.",
  playbar_status: "{state} ({device} | Shuffle: {shuffle} | Repeat: {repeat} | Volume: {volume}%){party}",
  playbar_source_status: "{state} ({source}{queue}{shuffle}{repeat} | Volume: {volume}%)",
  ready_status: "Ready   ({device} | Shuffle: {shuffle} | Repeat: {repeat} | Volume: {volume}%)",
  control_prev: "[Prev]",
  control_play: "[Play/Pause]",
  control_next: "[Next]",
  control_shuffle: "[Shuffle]",
  control_repeat: "[Repeat]",
  control_like: "[Like]",
  control_vol_down: "[Vol-]",
  control_vol_up: "[Vol+]",
  column_title: "Title",
  column_name: "Name",
  column_artist: "Artist",
  column_album: "Album",
  column_length: "Length",
  column_year: "Year",
  column_release_date: "Release Date",
  column_date: "Date",
  column_duration: "Duration",
  column_publisher: "Publisher(s)",
  title_by_artist: "{title} by {artist}",
  recommendations_song: "Recommendations based on Song '{seed}'",
  recommendations_artist: "Recommendations based on Artist '{seed}'",
  no_mirrors: "no mirrors",
  sync_no_match: "no match",
  sync_cannot_match: "cannot sync",
  sync_search_failed: "search failed: {error}",
  sync_link_row: "{name} [{source}] -> {mirrors} ({unmatched} unmatched)",
  sync_mirror_detail: "{source}: {matched} matched, {unmatched} unmatched, last run {last_run}",
  sync_last_run: "Last run: {report}",
  sync_run_totals: "{name}: {added} added, {removed} removed, {unmatched} unmatched",
  sync_run_skipped: "{name}: skipped ({reason})",
  sync_run_failed: "{name}: failed ({reason})",
  dj_auto_queue: "auto-queue on",
  dj_fresh_indexing: "fresh only (indexing…)",
  dj_fresh: "fresh only",
  dj_auto_queue_hint: "auto-queue",
  dj_vibe_hint: "vibe shift",
  dj_model_hint: "AI/model",
  dj_scroll_hint: "↑↓ scroll",
  dj_send_hint: "Enter send",
  dj_clear_hint: "Esc clear/back",
  dj_empty: "Ask the DJ for something, or turn auto-queue on.",
  dj_example_focus: "\"something chill for focusing\"",
  dj_example_slower: "\"more like this but slower\"",
  dj_user_prefix: "you  ",
  dj_thinking: "dj   …thinking",
  dj_setup: "Set up the AI DJ",
  dj_choose_model: "Choose a model",
  dj_model_name: "Model name",
  dj_setup_backend_hint: "↑↓ select · 1-9 jump · Enter choose · Esc keep the current brain",
  dj_select_hint: "↑↓ select",
  dj_choose_hint: "Enter choose",
  dj_back_hint: "Esc back",
  dj_custom_hint: "last row types a name",
  dj_use_model_hint: "Enter to use it · Esc back",
  dj_ask: "Ask the DJ",
  dj_using: "Using {label}.",
  dj_change_model: "{key} to change which AI and model the DJ uses.",
  dj_thinking_step: "dj   …thinking ({step}/{total})",
  dj_endpoint_hint: "for {name} · set behavior.dj_base_url for the endpoint",
  dj_provider_model_hint: "for {name} · as provider/model",
  dj_for_backend: "for {name}",
  more_below: "… {count} more below",
  more_above: "… {count} more above",
  more_above_below: "… {above} above, {below} below",
  visualizer_style_hint: "Press 'V' to cycle visualizer style",
  bar_graph: "Bar Graph",
  minor_key: "{name} minor",
  major_key: "{name} major",
  cover_loading: "Loading cover art...",
  cover_source_unavailable: "No cover art for this source",
  cover_failed: "Cover art unavailable",
  cover_empty: "No cover art available",
  log_location: "Log located in {path}",
  report_bugs: "Please report any bugs or missing features to https://github.com/LargeModGames/spotatui",
  unknown: "unknown",
  album_type_album: "album",
  album_type_single: "single",
  album_type_compilation: "compilation",
  album_type_appears_on: "appears_on",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: ScreenStrings = ScreenStrings {
  folder_track: "{name}（{count} 首歌曲）",
  search_playlist: "搜索播放列表",
  search_liked: "搜索喜欢的歌曲",
  search: "搜索",
  help: "帮助",
  settings: "设置",
  type_help: "按 ?",
  radio_stations: "电台",
  songs: "歌曲",
  artists: "艺人",
  albums: "专辑",
  playlists: "播放列表",
  podcasts: "播客",
  spotify_playlist_warning:
    "无法显示 Spotify 创建的播放列表。请尝试更具体的搜索，查找用户创建的播放列表。",
  local_files_empty: "本地文件（没有文件夹，请设置 behavior.local_music_path）",
  local_files: "本地文件",
  artist: "艺人",
  recommendations: "推荐",
  saved_albums: "已收藏专辑",
  episodes: "单集",
  recent_tracks: "最近播放的歌曲",
  no_top_tracks: "暂无热门歌曲",
  related_artists: "相关艺人",
  playing: "播放中",
  paused: "已暂停",
  on: "开",
  off: "关",
  repeat_track: "单曲",
  repeat_all: "全部",
  local_source_short: "本地",
  radio_source_short: "电台",
  queue: "队列",
  waiting_music: "正在等待 Music…",
  position_stale: "播放进度已过期（刷新循环已停止）",
  no_playback: "当前没有播放内容",
  choose_source_device: "选择音源和 Spotify 播放设备。",
  choices_cached: "所选项会被记住，下次打开 `spotatui` 即可直接使用。",
  welcome_spotatui: "欢迎使用 spotatui！",
  source_active: "  （当前）",
  source: "音源",
  spotify_devices: "设备（仅 Spotify）",
  devices: "设备",
  no_devices: "未找到设备：请确保有设备处于活动状态",
  timing_estimated: "时间为估算值",
  lyrics_title: " 歌词 ",
  lyrics_browsing: "浏览中 · ↑/↓ 滚动 · Esc 跟随",
  lyrics_loading: "正在获取歌词…",
  lyrics_from: "来自 LRCLIB",
  lyrics_missing: "此歌曲暂无歌词",
  lyrics_credit: "歌词由 LRCLIB 提供",
  nothing_playing: "当前没有播放内容",
  lyrics_start_track: "播放歌曲以查看歌词",
  spotify_pitch: "Spotify 音高",
  pitches: "音高",
  spectrum: "频谱",
  capturing_audio: "[>] 正在采集音频",
  capture_paused: "[||] 已暂停",
  no_capture: "无法采集音频",
  capture_hint_linux: "提示：请确保 PipeWire 或 PulseAudio 正在运行且有监听设备",
  capture_hint_windows: "提示：Windows 上通常可自动使用音频回环",
  capture_hint_macos: "提示：macOS 需要 BlackHole 等虚拟音频设备",
  capture_hint_other: "提示：此平台可能不支持音频采集",
  audio_waiting: "正在等待音频输入…",
  pitch_unavailable: "Spotify 没有此歌曲的音高分析",
  pitch_failed: "无法加载音高分析，稍后将重试",
  pitch_loading: "正在加载音高分析…",
  cover_art_not_built: "此版本未包含封面图片功能",
  type_key: "按 {key}",
  folder_tracks: "{name}（{count} 首歌曲）",
  artist_top_tracks: "{artist} - 热门歌曲",
  songs_searching: "歌曲（正在搜索：{query}…）",
  songs_filtered: "歌曲（筛选：{query}）",
  lyrics_offset: "偏移 {seconds} 秒",
  lyrics_notes: " 歌词（{notes}） ",
  visualization_title: "音频可视化（{style}）",
  chart_title: "{chart} | {fps} FPS | 按 {key} 退出",
  peak: "峰值：{percent}%",
  plugin_screen_empty: "插件页面“{name}”暂无内容",
  loading_track: "{name}（加载中…）",
  shuffle_segment: " | 随机播放：{mode}",
  repeat_segment: " | 循环播放：{mode}",
  party_listeners: " | 一起听：{count} 位听众",
  party_following: " | 一起听：跟随 {host}",
  status_title: "状态：{message}",
  live_elapsed: "直播 • {elapsed}",
  device_navigation: "用 `{up}`/`{down}` 或方向键移动，`Tab` 切换面板，<Enter> 选择。",
  reopen_devices: "随时按 `{key}` 可重新打开此页面。",
  playbar_status:
    "{state}（{device} | 随机播放：{shuffle} | 循环播放：{repeat} | 音量：{volume}%）{party}",
  playbar_source_status: "{state}（{source}{queue}{shuffle}{repeat} | 音量：{volume}%）",
  ready_status: "就绪（{device} | 随机播放：{shuffle} | 循环播放：{repeat} | 音量：{volume}%）",
  control_prev: "[上一首]",
  control_play: "[播放/暂停]",
  control_next: "[下一首]",
  control_shuffle: "[随机]",
  control_repeat: "[循环]",
  control_like: "[喜欢]",
  control_vol_down: "[音量-]",
  control_vol_up: "[音量+]",
  column_title: "标题",
  column_name: "名称",
  column_artist: "艺人",
  column_album: "专辑",
  column_length: "时长",
  column_year: "年份",
  column_release_date: "发行日期",
  column_date: "日期",
  column_duration: "时长",
  column_publisher: "发布者",
  title_by_artist: "{title} · {artist}",
  recommendations_song: "基于歌曲“{seed}”的推荐",
  recommendations_artist: "基于艺人“{seed}”的推荐",
  more_below: "… 下方还有 {count} 项",
  more_above: "… 上方还有 {count} 项",
  more_above_below: "… 上方 {above} 项，下方 {below} 项",
  visualizer_style_hint: "按 V 切换可视化样式",
  bar_graph: "条形图",
  minor_key: "{name} 小调",
  major_key: "{name} 大调",
  cover_loading: "正在加载封面…",
  cover_source_unavailable: "此音源没有封面",
  cover_failed: "封面不可用",
  cover_empty: "暂无封面",
  log_location: "日志位于 {path}",
  report_bugs: "如有错误或缺少的功能，请反馈至 https://github.com/LargeModGames/spotatui",
  unknown: "未知",
  album_type_album: "专辑",
  album_type_single: "单曲",
  album_type_compilation: "精选集",
  album_type_appears_on: "参与作品",
  ..EN
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: ScreenStrings = ScreenStrings {
  folder_track: "{name}（{count} 首歌曲）",
  search_playlist: "搜尋播放清單",
  search_liked: "搜尋喜歡的歌曲",
  search: "搜尋",
  help: "說明",
  settings: "設定",
  type_help: "按 ?",
  radio_stations: "電台",
  songs: "歌曲",
  artists: "藝人",
  albums: "專輯",
  playlists: "播放清單",
  podcasts: "Podcast",
  spotify_playlist_warning:
    "無法顯示 Spotify 建立的播放清單。請嘗試更明確的搜尋，尋找使用者建立的播放清單。",
  local_files_empty: "本機檔案（無資料夾，請設定 behavior.local_music_path）",
  local_files: "本機檔案",
  artist: "藝人",
  recommendations: "推薦",
  saved_albums: "已收藏專輯",
  episodes: "單集",
  recent_tracks: "最近播放的歌曲",
  no_top_tracks: "尚無熱門歌曲",
  related_artists: "相關藝人",
  playing: "播放中",
  paused: "已暫停",
  on: "開",
  off: "關",
  repeat_track: "單曲",
  repeat_all: "全部",
  local_source_short: "本機",
  radio_source_short: "電台",
  queue: "佇列",
  waiting_music: "正在等待 Music…",
  position_stale: "播放進度已過期（更新迴圈已停止）",
  no_playback: "目前沒有播放內容",
  choose_source_device: "選擇音樂來源和 Spotify 播放裝置。",
  choices_cached: "所選項目會被記住，下次開啟 `spotatui` 即可直接使用。",
  welcome_spotatui: "歡迎使用 spotatui！",
  source_active: "  （目前）",
  source: "來源",
  spotify_devices: "裝置（僅 Spotify）",
  devices: "裝置",
  no_devices: "找不到裝置：請確認有裝置處於作用中",
  timing_estimated: "時間為估計值",
  lyrics_title: " 歌詞 ",
  lyrics_browsing: "瀏覽中 · ↑/↓ 捲動 · Esc 跟隨",
  lyrics_loading: "正在取得歌詞…",
  lyrics_from: "來自 LRCLIB",
  lyrics_missing: "此歌曲尚無歌詞",
  lyrics_credit: "歌詞由 LRCLIB 提供",
  nothing_playing: "目前沒有播放內容",
  lyrics_start_track: "播放歌曲以查看歌詞",
  spotify_pitch: "Spotify 音高",
  pitches: "音高",
  spectrum: "頻譜",
  capturing_audio: "[>] 正在擷取音訊",
  capture_paused: "[||] 已暫停",
  no_capture: "無法擷取音訊",
  capture_hint_linux: "提示：請確認 PipeWire 或 PulseAudio 正在執行且有監聽裝置",
  capture_hint_windows: "提示：Windows 上通常可自動使用音訊回送",
  capture_hint_macos: "提示：macOS 需要 BlackHole 等虛擬音訊裝置",
  capture_hint_other: "提示：此平台可能不支援音訊擷取",
  audio_waiting: "正在等待音訊輸入…",
  pitch_unavailable: "Spotify 沒有此歌曲的音高分析",
  pitch_failed: "無法載入音高分析，稍後將重試",
  pitch_loading: "正在載入音高分析…",
  cover_art_not_built: "此版本未包含封面圖片功能",
  type_key: "按 {key}",
  folder_tracks: "{name}（{count} 首歌曲）",
  artist_top_tracks: "{artist} - 熱門歌曲",
  songs_searching: "歌曲（正在搜尋：{query}…）",
  songs_filtered: "歌曲（篩選：{query}）",
  lyrics_offset: "偏移 {seconds} 秒",
  lyrics_notes: " 歌詞（{notes}） ",
  visualization_title: "音訊視覺化（{style}）",
  chart_title: "{chart} | {fps} FPS | 按 {key} 離開",
  peak: "峰值：{percent}%",
  plugin_screen_empty: "外掛程式頁面「{name}」尚無內容",
  loading_track: "{name}（載入中…）",
  shuffle_segment: " | 隨機播放：{mode}",
  repeat_segment: " | 循環播放：{mode}",
  party_listeners: " | 一起聽：{count} 位聽眾",
  party_following: " | 一起聽：跟隨 {host}",
  status_title: "狀態：{message}",
  live_elapsed: "直播 • {elapsed}",
  device_navigation: "用 `{up}`/`{down}` 或方向鍵移動，`Tab` 切換面板，<Enter> 選擇。",
  reopen_devices: "隨時按 `{key}` 可重新開啟此頁面。",
  playbar_status:
    "{state}（{device} | 隨機播放：{shuffle} | 循環播放：{repeat} | 音量：{volume}%）{party}",
  playbar_source_status: "{state}（{source}{queue}{shuffle}{repeat} | 音量：{volume}%）",
  ready_status: "就緒（{device} | 隨機播放：{shuffle} | 循環播放：{repeat} | 音量：{volume}%）",
  control_prev: "[上一首]",
  control_play: "[播放/暫停]",
  control_next: "[下一首]",
  control_shuffle: "[隨機]",
  control_repeat: "[循環]",
  control_like: "[喜歡]",
  control_vol_down: "[音量-]",
  control_vol_up: "[音量+]",
  column_title: "標題",
  column_name: "名稱",
  column_artist: "藝人",
  column_album: "專輯",
  column_length: "長度",
  column_year: "年份",
  column_release_date: "發行日期",
  column_date: "日期",
  column_duration: "長度",
  column_publisher: "發行單位",
  title_by_artist: "{title} · {artist}",
  recommendations_song: "根據歌曲「{seed}」的推薦",
  recommendations_artist: "根據藝人「{seed}」的推薦",
  more_below: "… 下方還有 {count} 項",
  more_above: "… 上方還有 {count} 項",
  more_above_below: "… 上方 {above} 項，下方 {below} 項",
  visualizer_style_hint: "按 V 切換視覺化樣式",
  bar_graph: "長條圖",
  minor_key: "{name} 小調",
  major_key: "{name} 大調",
  cover_loading: "正在載入封面…",
  cover_source_unavailable: "此來源沒有封面",
  cover_failed: "封面無法使用",
  cover_empty: "尚無封面",
  log_location: "日誌位於 {path}",
  report_bugs: "如有錯誤或缺少的功能，請回報至 https://github.com/LargeModGames/spotatui",
  unknown: "未知",
  album_type_album: "專輯",
  album_type_single: "單曲",
  album_type_compilation: "精選輯",
  album_type_appears_on: "參與作品",
  ..EN
};

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
impl ScreenStrings {
  /// Album type tokens remain unchanged in domain data; only known display labels translate.
  pub fn album_type<'a>(&'a self, kind: Option<&'a str>) -> &'a str {
    match kind {
      Some("album") => self.album_type_album,
      Some("single") => self.album_type_single,
      Some("compilation") => self.album_type_compilation,
      Some("appears_on") => self.album_type_appears_on,
      Some(other) => other,
      None => self.unknown,
    }
  }

  pub fn type_key(&self, key: &str) -> String {
    crate::core::i18n::format_template(self.type_key, &[("key", key)])
  }
  pub fn folder_tracks(&self, name: &str, count: u32) -> String {
    crate::core::i18n::format_template(
      if count == 1 {
        self.folder_track
      } else {
        self.folder_tracks
      },
      &[("name", name), ("count", &count.to_string())],
    )
  }
  pub fn artist_top_tracks(&self, artist: &str) -> String {
    crate::core::i18n::format_template(self.artist_top_tracks, &[("artist", artist)])
  }
  pub fn songs_searching(&self, query: &str) -> String {
    crate::core::i18n::format_template(self.songs_searching, &[("query", query)])
  }
  pub fn songs_filtered(&self, query: &str) -> String {
    crate::core::i18n::format_template(self.songs_filtered, &[("query", query)])
  }
  pub fn lyrics_offset(&self, seconds: &str) -> String {
    crate::core::i18n::format_template(self.lyrics_offset, &[("seconds", seconds)])
  }
  pub fn lyrics_notes(&self, notes: &str) -> String {
    crate::core::i18n::format_template(self.lyrics_notes, &[("notes", notes)])
  }
  pub fn friends_all(&self, count: usize) -> String {
    crate::core::i18n::format_template(self.friends_all, &[("count", &count.to_string())])
  }
  pub fn friends_online(&self, count: usize) -> String {
    crate::core::i18n::format_template(self.friends_online, &[("count", &count.to_string())])
  }
  pub fn day_streak(&self, days: u32) -> String {
    crate::core::i18n::format_template(self.day_streak, &[("days", &days.to_string())])
  }
  pub fn streak_best_today(&self, days: u32, duration: &str) -> String {
    crate::core::i18n::format_template(
      self.streak_best_today,
      &[("days", &days.to_string()), ("duration", duration)],
    )
  }
  pub fn stats_totals(&self, plays: u64, duration: &str) -> String {
    crate::core::i18n::format_template(
      if plays == 1 {
        self.stats_total
      } else {
        self.stats_totals
      },
      &[("plays", &plays.to_string()), ("duration", duration)],
    )
  }
  pub fn listening_streak(&self, days: u32) -> String {
    crate::core::i18n::format_template(self.listening_streak, &[("days", &days.to_string())])
  }
  pub fn listened_today(&self, duration: &str) -> String {
    crate::core::i18n::format_template(self.listened_today, &[("duration", duration)])
  }
  pub fn global_counter(&self, count: &str) -> String {
    crate::core::i18n::format_template(self.global_counter, &[("count", count)])
  }
  pub fn visualization_title(&self, style: &str) -> String {
    crate::core::i18n::format_template(self.visualization_title, &[("style", style)])
  }
  pub fn chart_title(&self, chart: &str, fps: &str, key: &str) -> String {
    crate::core::i18n::format_template(
      self.chart_title,
      &[("chart", chart), ("fps", fps), ("key", key)],
    )
  }
  pub fn peak(&self, percent: &str) -> String {
    crate::core::i18n::format_template(self.peak, &[("percent", percent)])
  }
  pub fn plugin_screen_empty(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.plugin_screen_empty, &[("name", name)])
  }
  pub fn loading_track(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.loading_track, &[("name", name)])
  }
  pub fn shuffle_segment(&self, mode: &str) -> String {
    crate::core::i18n::format_template(self.shuffle_segment, &[("mode", mode)])
  }
  pub fn repeat_segment(&self, mode: &str) -> String {
    crate::core::i18n::format_template(self.repeat_segment, &[("mode", mode)])
  }
  pub fn party_listeners(&self, count: usize) -> String {
    crate::core::i18n::format_template(self.party_listeners, &[("count", &count.to_string())])
  }
  pub fn party_following(&self, host: &str) -> String {
    crate::core::i18n::format_template(self.party_following, &[("host", host)])
  }
  pub fn status_title(&self, message: &str) -> String {
    crate::core::i18n::format_template(self.status_title, &[("message", message)])
  }
  pub fn live_elapsed(&self, elapsed: &str) -> String {
    crate::core::i18n::format_template(self.live_elapsed, &[("elapsed", elapsed)])
  }
  pub fn device_navigation(&self, up: &str, down: &str) -> String {
    crate::core::i18n::format_template(self.device_navigation, &[("up", up), ("down", down)])
  }
  pub fn reopen_devices(&self, key: &str) -> String {
    crate::core::i18n::format_template(self.reopen_devices, &[("key", key)])
  }
  pub fn ready_status(&self, device: &str, shuffle: &str, repeat: &str, volume: &str) -> String {
    crate::core::i18n::format_template(
      self.ready_status,
      &[
        ("device", device),
        ("shuffle", shuffle),
        ("repeat", repeat),
        ("volume", volume),
      ],
    )
  }
  pub fn title_by_artist(&self, title: &str, artist: &str) -> String {
    crate::core::i18n::format_template(
      self.title_by_artist,
      &[("title", title), ("artist", artist)],
    )
  }
  pub fn recommendations_song(&self, seed: &str) -> String {
    crate::core::i18n::format_template(self.recommendations_song, &[("seed", seed)])
  }
  pub fn recommendations_artist(&self, seed: &str) -> String {
    crate::core::i18n::format_template(self.recommendations_artist, &[("seed", seed)])
  }
  pub fn sync_search_failed(&self, error: &str) -> String {
    crate::core::i18n::format_template(self.sync_search_failed, &[("error", error)])
  }
  pub fn sync_link_row(&self, name: &str, source: &str, mirrors: &str, unmatched: usize) -> String {
    crate::core::i18n::format_template(
      self.sync_link_row,
      &[
        ("name", name),
        ("source", source),
        ("mirrors", mirrors),
        ("unmatched", &unmatched.to_string()),
      ],
    )
  }
  pub fn sync_mirror_detail(
    &self,
    source: &str,
    matched: usize,
    unmatched: usize,
    last_run: &str,
  ) -> String {
    crate::core::i18n::format_template(
      self.sync_mirror_detail,
      &[
        ("source", source),
        ("matched", &matched.to_string()),
        ("unmatched", &unmatched.to_string()),
        ("last_run", last_run),
      ],
    )
  }
  pub fn sync_last_run(&self, report: &str) -> String {
    crate::core::i18n::format_template(self.sync_last_run, &[("report", report)])
  }
  pub fn sync_run_totals(
    &self,
    name: &str,
    added: usize,
    removed: usize,
    unmatched: usize,
  ) -> String {
    crate::core::i18n::format_template(
      self.sync_run_totals,
      &[
        ("name", name),
        ("added", &added.to_string()),
        ("removed", &removed.to_string()),
        ("unmatched", &unmatched.to_string()),
      ],
    )
  }
  pub fn sync_run_skipped(&self, name: &str, reason: &str) -> String {
    crate::core::i18n::format_template(self.sync_run_skipped, &[("name", name), ("reason", reason)])
  }
  pub fn sync_run_failed(&self, name: &str, reason: &str) -> String {
    crate::core::i18n::format_template(self.sync_run_failed, &[("name", name), ("reason", reason)])
  }
  pub fn dj_using(&self, label: &str) -> String {
    crate::core::i18n::format_template(self.dj_using, &[("label", label)])
  }
  pub fn dj_change_model(&self, key: &str) -> String {
    crate::core::i18n::format_template(self.dj_change_model, &[("key", key)])
  }
  pub fn dj_thinking_step(&self, step: &str, total: &str) -> String {
    crate::core::i18n::format_template(self.dj_thinking_step, &[("step", step), ("total", total)])
  }
  pub fn dj_endpoint_hint(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.dj_endpoint_hint, &[("name", name)])
  }
  pub fn dj_provider_model_hint(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.dj_provider_model_hint, &[("name", name)])
  }
  pub fn dj_for_backend(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.dj_for_backend, &[("name", name)])
  }
  pub fn more_below(&self, count: usize) -> String {
    crate::core::i18n::format_template(self.more_below, &[("count", &count.to_string())])
  }
  pub fn more_above(&self, count: usize) -> String {
    crate::core::i18n::format_template(self.more_above, &[("count", &count.to_string())])
  }
  pub fn more_above_below(&self, above: usize, below: usize) -> String {
    crate::core::i18n::format_template(
      self.more_above_below,
      &[("above", &above.to_string()), ("below", &below.to_string())],
    )
  }
  pub fn minor_key(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.minor_key, &[("name", name)])
  }
  pub fn major_key(&self, name: &str) -> String {
    crate::core::i18n::format_template(self.major_key, &[("name", name)])
  }
  pub fn log_location(&self, path: &str) -> String {
    crate::core::i18n::format_template(self.log_location, &[("path", path)])
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn chinese_screen_templates_keep_user_values_literal() {
    for (strings, folder, searching, plugin) in [
      (
        &ZH_HANS,
        "摇滚{count}（3 首歌曲）",
        "歌曲（正在搜索：{query}…）",
        "插件页面“{name}”暂无内容",
      ),
      (
        &ZH_HANT,
        "摇滚{count}（3 首歌曲）",
        "歌曲（正在搜尋：{query}…）",
        "外掛程式頁面「{name}」尚無內容",
      ),
    ] {
      assert_eq!(strings.folder_tracks("摇滚{count}", 3), folder);
      assert_eq!(strings.songs_searching("{query}"), searching);
      assert_eq!(strings.plugin_screen_empty("{name}"), plugin);
    }
  }

  #[test]
  fn chinese_playbar_segments_are_all_translated() {
    for (strings, repeat, listeners, following) in [
      (
        &ZH_HANS,
        " | 循环播放：全部",
        " | 一起听：2 位听众",
        " | 一起听：跟随 {host}",
      ),
      (
        &ZH_HANT,
        " | 循環播放：全部",
        " | 一起聽：2 位聽眾",
        " | 一起聽：跟隨 {host}",
      ),
    ] {
      assert_eq!(strings.repeat_segment("全部"), repeat);
      assert_eq!(strings.party_listeners(2), listeners);
      assert_eq!(strings.party_following("{host}"), following);
    }
  }
}
