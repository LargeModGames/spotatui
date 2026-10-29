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
  pub pane_nothing_to_detach: &'static str,
  pub pane_hidden: &'static str,
  pub pane_no_room: &'static str,
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
  pane_nothing_to_detach:
    "Open lyrics, the visualizer, cover art, the queue or the mini player, then press {key} to detach it",
  pane_hidden: "The terminal is too small to show the panes. Press {key} to close one",
  pane_no_room: "There is no room for another pane in this terminal",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: AppStatusStrings = AppStatusStrings {
  music_error: "Apple Music：{error}",
  audio_backend_failed: "音频后端出错：{error}",
  audio_backend_no_alsa_default:
    "没有音频输出：ALSA 没有默认设备（使用 PipeWire 时请安装 pipewire-alsa）",
  audio_device_paused: "音频设备已更换，播放已暂停。",
  audio_device_moved: "音频设备已更换，播放已切换至新设备。",
  audio_device_waiting: "音频输出设备已断开，正在等待新设备。",
  radio_device_lost: "音频输出设备已断开，电台已停止。",
  radio_ended: "电台流媒体已结束",
  current_source_cannot_like: "当前播放音源不支持喜欢歌曲",

  loading_playlist: "正在加载播放列表…",
  community_pin_hidden: "已隐藏社区播放列表置顶（可在设置中重新开启）",
  native_disconnected: "内置播放已断开，正在尝试恢复。",
  native_reconnecting: "正在重新连接内置播放…",
  native_request_expired: "等待内置播放恢复时，播放请求已过期。",
  resuming_request: "正在恢复播放请求…",
  track_cannot_add: "无法将这首歌曲加入播放列表",
  youtube_no_playlists: "尚无 YouTube 播放列表，请从侧栏创建",
  playlist_destinations_loading: "目标播放列表正在加载，请稍后重试",
  no_editable_playlists: "没有可编辑的播放列表",
  playlist_tracks_only: "只能将歌曲加入播放列表",
  playlist_needs_spotify: "加入播放列表需要正在播放 Spotify 歌曲",
  no_current_track: "当前没有正在播放的歌曲",
  youtube_no_open_playlist: "尚未打开 YouTube 播放列表",
  youtube_playlist: "YouTube 播放列表",
  track_cannot_edit: "无法在播放列表中编辑这首歌曲",
  remove_requires_playlist: "只能在已选播放列表中移除歌曲",
  remove_position_unknown: "无法确定要移除歌曲的位置",
  settings_shortcut_fallback: "此终端无法识别 Ctrl+,；本次使用 Alt+,",
  queued_no_album: "队列中的歌曲没有专辑 ID",
  jump_album_spotify: "跳转到专辑需要正在播放 Spotify 歌曲",
  playing_no_artist: "正在播放的歌曲没有艺人 ID",
  queued_no_artist: "队列中的歌曲没有艺人 ID",
  jump_artist_spotify: "跳转到艺人需要正在播放 Spotify 歌曲",
  queue_no_context: "队列中的歌曲没有所属列表",
  jump_context_spotify: "跳转到所属列表需要正在播放 Spotify 歌曲",
  friend_code_loading: "好友代码尚未加载",
  clipboard_unavailable: "剪贴板不可用",
  clipboard_copy_failed: "复制到剪贴板失败",
  no_playlist_link: "尚未选中播放列表关联",
  highlight_playlist_mirror: "请选中要镜像的播放列表",
  no_mirror_source: "没有其他音源可以镜像此播放列表",
  radio_no_url: "电台没有流媒体 URL",
  radio_position_missing: "未在预期位置找到新增电台，未保存",
  another_source_playing: "当前由其他音源播放",
  track_no_id: "歌曲没有 ID",
  album_no_id: "专辑没有 ID",
  music_wait_pause: "正在等待 Music 暂停后切换音源",
  music_volume_floor: "Apple Music：最低音量略高于 0（Music 无法以音量 0 播放）",
  music_did_not_start: "Apple Music：Music 未开始播放。请检查 Music 窗口是否有对话框或歌曲不可用",
  music_quit: "Apple Music：Music 已退出",
  music_requires_macos: "Apple Music 需要 macOS 和 apple-music 功能",
  all_songs: "全部歌曲",
  music_track_unavailable: "Apple Music：Music 无法播放此歌曲（歌曲已不可用或文件缺失）",
  queue_no_uri: "无法加入队列：歌曲没有 URI",
  radio_cannot_queue: "电台无法加入队列",
  shuffle_on: "随机播放：开",
  shuffle_off: "随机播放：关",
  repeat_off: "循环播放：关",
  repeat_all: "循环播放：全部",
  repeat_one: "循环播放：单曲",
  shuffle_unsupported: "此音源不支持随机播放",
  repeat_unsupported: "此音源不支持循环播放",
  recap_disabled: "已关闭月度回顾提示（可在设置中重新开启）",
  native_no_response: "内置播放在恢复后仍无响应，已取消请求。",
  native_restore_failed: "内置播放连接已恢复，但无法恢复播放。",
  dj_previous_request: "DJ 仍在处理上一个请求",
  dj_auto_queue_wait: "已开启 DJ 自动补充队列，其他 Spotify 设备播放时会等待",
  dj_auto_queue_on: "已开启 DJ 自动补充队列",
  dj_auto_queue_off: "已关闭 DJ 自动补充队列",
  dj_fresh_only: "DJ：仅推荐尚未收藏的歌曲",
  dj_everything: "DJ：从全部歌曲中推荐",
  dj_busy: "DJ 正在处理其他请求",
  dj_shifting: "正在切换音乐氛围",
  dj_keep_brain: "保留当前 DJ 模型",
  nothing_playing: "当前没有播放内容",
  spotify_parked: "按播放以恢复 Spotify",
  spotify_not_connected: "Spotify 未连接。按 `d` 并选择 Spotify 登录。",
  party_needs_spotify: "一起听需要 Spotify。按 `d` 并选择 Spotify 登录。",
  music_cannot_queue: "Apple Music 歌曲无法加入队列",
  friend_code_copied: "已复制好友代码：{code}",
  sync_hint: "播放列表同步：{hint}",
  mirroring: "正在将 {name} 镜像到 {source}",
  radio_favorited: "已收藏电台：{name}",
  radio_already_favorited: "电台已收藏：{name}",
  radio_favorite_failed: "无法收藏电台：{error}",
  radio_configured: "电台已在 config.yml 中配置：{name}",
  radio_removed: "已移除已保存电台：{name}",
  radio_not_favorited: "电台尚未收藏：{name}",
  radio_remove_failed: "无法移除电台：{error}",
  state_save_failed: "保存状态失败：{error}",
  shortcut_save_failed: "保存备用快捷键失败：{error}",
  shortcut_saved: "已将打开设置的快捷键保存为 {key}",
  clipboard_set_failed: "设置剪贴板内容失败：{error}",
  queued: "已加入队列：{name}",
  recap_browser_failed: "回顾已保存至 {path}（无法打开浏览器）",
  dj_shift_dropped: "正在切换音乐氛围（已移除队列中的 {dropped} 首歌曲）",
  dj_brain: "DJ 模型：{name}",
  dj_brain_not_ready: "DJ 模型：{name}（暂不可用：请安装或设置 API 密钥）",
  dj_save_failed: "DJ：无法保存所选项：{error}",
  playlist_searching: "正在播放列表中搜索“{query}”…",
  unfollow_profile_loading: "无法取消关注：用户资料尚未加载",
  no_playback_device_selected: "尚未选择播放设备",
  no_playback_devices: "未找到播放设备",
  playback_device_unavailable: "所选播放设备已不可用",
  playback_device_no_id: "所选播放设备没有 Spotify 设备 ID",
  entry_not_playable: "此条目无法直接播放",
  liked_loading: "喜欢的歌曲仍在加载，请稍候…",
  no_radio_selected: "尚未选择电台",
  switching_device: "正在将播放切换到 {device}",
  source_selected: "音源：{source}",
  announcement_save_failed: "保存已关闭公告的状态失败：{error}",
  link_open_failed: "无法在浏览器中打开 {url}",
  capture_unavailable: "可视化器无法采集系统音频，详情请查看日志。",
  no_radio_playing: "尚未选择或播放电台",
  search_needs_spotify: "搜索需要登录 Spotify",
  copy_needs_spotify: "复制 URL 需要正在播放 Spotify 歌曲",
  shortcut_session_only: "仅本次使用 Alt+,",
  like_source_unavailable: "{source} 不支持喜欢操作",
  search_source_unavailable: "{source} 不支持搜索",
  paused: "已暂停",
  paused_details: "已暂停：{details}",
  visualizer_config: "visualizer.yml：{problem}",
  pane_nothing_to_detach: "先打开歌词、可视化、封面、队列或迷你播放器，再按 {key} 将其分离",
  pane_hidden: "终端太小，无法显示窗格。按 {key} 关闭一个",
  pane_no_room: "此终端没有空间再放一个窗格",
  ..EN
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: AppStatusStrings = AppStatusStrings {
  music_error: "Apple Music：{error}",
  audio_backend_failed: "音訊後端發生錯誤：{error}",
  audio_backend_no_alsa_default:
    "沒有音訊輸出：ALSA 沒有預設裝置（使用 PipeWire 時請安裝 pipewire-alsa）",
  audio_device_paused: "音訊裝置已變更，播放已暫停。",
  audio_device_moved: "音訊裝置已變更，播放已切換至新裝置。",
  audio_device_waiting: "音訊輸出裝置已中斷連線，正在等待新裝置。",
  radio_device_lost: "音訊輸出裝置已中斷連線，電台已停止。",
  radio_ended: "電台串流已結束",
  current_source_cannot_like: "目前的播放來源不支援喜歡歌曲",

  loading_playlist: "正在載入播放清單…",
  community_pin_hidden: "已隱藏社群播放清單置頂（可在設定中重新開啟）",
  native_disconnected: "內建串流已中斷，正在嘗試恢復。",
  native_reconnecting: "正在重新連線內建串流…",
  native_request_expired: "播放要求在內建串流恢復期間已過期。",
  resuming_request: "正在恢復播放要求…",
  track_cannot_add: "無法將這首歌曲加入播放清單",
  youtube_no_playlists: "尚無 YouTube 播放清單，請從側邊欄建立",
  playlist_destinations_loading: "目標播放清單正在載入，請稍後再試",
  no_editable_playlists: "沒有可編輯的播放清單",
  playlist_tracks_only: "只能將歌曲加入播放清單",
  playlist_needs_spotify: "加入播放清單需要正在播放 Spotify 歌曲",
  no_current_track: "目前沒有正在播放的歌曲",
  youtube_no_open_playlist: "尚未開啟 YouTube 播放清單",
  youtube_playlist: "YouTube 播放清單",
  track_cannot_edit: "無法在播放清單中編輯這首歌曲",
  remove_requires_playlist: "只能在已選播放清單中移除歌曲",
  remove_position_unknown: "無法確定要移除歌曲的位置",
  settings_shortcut_fallback: "此終端機無法辨識 Ctrl+,；本次使用 Alt+,",
  queued_no_album: "佇列中的歌曲沒有專輯 ID",
  jump_album_spotify: "跳至專輯需要正在播放 Spotify 歌曲",
  playing_no_artist: "正在播放的歌曲沒有藝人 ID",
  queued_no_artist: "佇列中的歌曲沒有藝人 ID",
  jump_artist_spotify: "跳至藝人需要正在播放 Spotify 歌曲",
  queue_no_context: "佇列中的歌曲沒有所屬清單",
  jump_context_spotify: "跳至所屬清單需要正在播放 Spotify 歌曲",
  friend_code_loading: "好友代碼尚未載入",
  clipboard_unavailable: "剪貼簿無法使用",
  clipboard_copy_failed: "複製到剪貼簿失敗",
  no_playlist_link: "尚未選取播放清單連結",
  highlight_playlist_mirror: "請選取要鏡像的播放清單",
  no_mirror_source: "沒有其他來源可以鏡像此播放清單",
  radio_no_url: "電台沒有串流 URL",
  radio_position_missing: "未在預期位置找到新增電台，未儲存",
  another_source_playing: "目前由其他來源播放",
  track_no_id: "歌曲沒有 ID",
  album_no_id: "專輯沒有 ID",
  music_wait_pause: "正在等待 Music 暫停後切換來源",
  music_volume_floor: "Apple Music：最低音量略高於 0（Music 無法以音量 0 播放）",
  music_did_not_start: "Apple Music：Music 未開始播放。請檢查 Music 視窗是否有對話框或歌曲無法使用",
  music_quit: "Apple Music：Music 已結束",
  music_requires_macos: "Apple Music 需要 macOS 和 apple-music 功能",
  all_songs: "所有歌曲",
  music_track_unavailable: "Apple Music：Music 無法播放此歌曲（歌曲已無法使用或檔案遺失）",
  queue_no_uri: "無法加入佇列：歌曲沒有 URI",
  radio_cannot_queue: "電台無法加入佇列",
  shuffle_on: "隨機播放：開",
  shuffle_off: "隨機播放：關",
  repeat_off: "循環播放：關",
  repeat_all: "循環播放：全部",
  repeat_one: "循環播放：單曲",
  shuffle_unsupported: "此來源不支援隨機播放",
  repeat_unsupported: "此來源不支援循環播放",
  recap_disabled: "已關閉每月回顧提示（可在設定中重新開啟）",
  native_no_response: "內建串流在恢復後仍無回應，已取消要求。",
  native_restore_failed: "內建串流連線已恢復，但無法恢復播放。",
  dj_previous_request: "DJ 仍在處理上一個要求",
  dj_auto_queue_wait: "已開啟 DJ 自動補充佇列，其他 Spotify 裝置播放時會等待",
  dj_auto_queue_on: "已開啟 DJ 自動補充佇列",
  dj_auto_queue_off: "已關閉 DJ 自動補充佇列",
  dj_fresh_only: "DJ：僅推薦尚未收藏的歌曲",
  dj_everything: "DJ：從所有歌曲中推薦",
  dj_busy: "DJ 正在處理其他要求",
  dj_shifting: "正在切換音樂氛圍",
  dj_keep_brain: "保留目前 DJ 模型",
  nothing_playing: "目前沒有播放內容",
  spotify_parked: "按播放以恢復 Spotify",
  spotify_not_connected: "Spotify 未連線。按 `d` 並選擇 Spotify 登入。",
  party_needs_spotify: "一起聽需要 Spotify。按 `d` 並選擇 Spotify 登入。",
  music_cannot_queue: "Apple Music 歌曲無法加入佇列",
  friend_code_copied: "已複製好友代碼：{code}",
  sync_hint: "播放清單同步：{hint}",
  mirroring: "正在將 {name} 鏡像到 {source}",
  radio_favorited: "已收藏電台：{name}",
  radio_already_favorited: "電台已收藏：{name}",
  radio_favorite_failed: "無法收藏電台：{error}",
  radio_configured: "電台已在 config.yml 中設定：{name}",
  radio_removed: "已移除已儲存電台：{name}",
  radio_not_favorited: "電台尚未收藏：{name}",
  radio_remove_failed: "無法移除電台：{error}",
  state_save_failed: "儲存狀態失敗：{error}",
  shortcut_save_failed: "儲存備用快速鍵失敗：{error}",
  shortcut_saved: "已將開啟設定的快速鍵儲存為 {key}",
  clipboard_set_failed: "設定剪貼簿內容失敗：{error}",
  queued: "已加入佇列：{name}",
  recap_browser_failed: "回顧已儲存至 {path}（無法開啟瀏覽器）",
  dj_shift_dropped: "正在切換音樂氛圍（已移除佇列中的 {dropped} 首歌曲）",
  dj_brain: "DJ 模型：{name}",
  dj_brain_not_ready: "DJ 模型：{name}（尚無法使用：請安裝或設定 API 金鑰）",
  dj_save_failed: "DJ：無法儲存所選項目：{error}",
  playlist_searching: "正在播放清單中搜尋「{query}」…",
  unfollow_profile_loading: "無法取消追蹤：使用者資料尚未載入",
  no_playback_device_selected: "尚未選擇播放裝置",
  no_playback_devices: "找不到播放裝置",
  playback_device_unavailable: "所選播放裝置已無法使用",
  playback_device_no_id: "所選播放裝置沒有 Spotify 裝置 ID",
  entry_not_playable: "此項目無法直接播放",
  liked_loading: "喜歡的歌曲仍在載入，請稍候…",
  no_radio_selected: "尚未選擇電台",
  switching_device: "正在將播放切換到 {device}",
  source_selected: "來源：{source}",
  announcement_save_failed: "儲存已關閉公告的狀態失敗：{error}",
  link_open_failed: "無法在瀏覽器中開啟 {url}",
  capture_unavailable: "視覺化工具無法擷取系統音訊，詳情請查看日誌。",
  no_radio_playing: "尚未選擇或播放電台",
  search_needs_spotify: "搜尋需要登入 Spotify",
  copy_needs_spotify: "複製 URL 需要正在播放 Spotify 歌曲",
  shortcut_session_only: "僅本次使用 Alt+,",
  like_source_unavailable: "{source} 不支援喜歡操作",
  search_source_unavailable: "{source} 不支援搜尋",
  paused: "已暫停",
  paused_details: "已暫停：{details}",
  visualizer_config: "visualizer.yml：{problem}",
  pane_nothing_to_detach: "先開啟歌詞、視覺化、封面、佇列或迷你播放器，再按 {key} 將其分離",
  pane_hidden: "終端機太小，無法顯示窗格。按 {key} 關閉一個",
  pane_no_room: "此終端機沒有空間再放一個窗格",
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
  pub fn pane_nothing_to_detach(&self, key: &str) -> String {
    crate::core::i18n::format_template(self.pane_nothing_to_detach, &[("key", key)])
  }
  pub fn pane_hidden(&self, key: &str) -> String {
    crate::core::i18n::format_template(self.pane_hidden, &[("key", key)])
  }
}
