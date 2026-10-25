//! Popup and dialog copy. User names, titles, paths, codes and URLs are inserted literally.

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct PopupStrings {
  pub resume_template: &'static str,
  pub now_playing: &'static str,
  pub queue_empty_template: &'static str,
  pub queue_up_next: &'static str,
  pub queue_title_template: &'static str,
  pub source_local: &'static str,
  pub api_response: &'static str,
  pub playback_check: &'static str,
  pub check_premium: &'static str,
  pub check_device: &'static str,
  pub check_spotifyd: &'static str,
  pub playback_hint: &'static str,
  pub error_back: &'static str,
  pub error_title: &'static str,
  pub log_hint: &'static str,
  pub log_file: &'static str,
  pub delete_playlist_question: &'static str,
  pub question_mark: &'static str,
  pub confirm: &'static str,
  pub remove_track_question: &'static str,
  pub track_template: &'static str,
  pub playlist_template: &'static str,
  pub remove_track: &'static str,
  pub fallback_terminal: &'static str,
  pub fallback_question: &'static str,
  pub save_shortcut_template: &'static str,
  pub save_shortcut_fallback: &'static str,
  pub remove_link_question: &'static str,
  pub mirrors_stay: &'static str,
  pub remove_link: &'static str,
  pub ok: &'static str,
  pub cancel: &'static str,
  pub add_track_title: &'static str,
  pub selected_track: &'static str,
  pub choose_playlist_template: &'static str,
  pub no_editable_playlists: &'static str,
  pub collaborative_playlist_template: &'static str,
  pub playlist_picker_controls_template: &'static str,
  pub mirror_playlist: &'static str,
  pub selected_playlist: &'static str,
  pub mirror_onto_template: &'static str,
  pub no_mirror_source: &'static str,
  pub mirror_picker_controls_template: &'static str,
  pub announcement_info: &'static str,
  pub announcement_warning: &'static str,
  pub announcement_critical: &'static str,
  pub announcement_more_template: &'static str,
  pub announcement_dismiss: &'static str,
  pub announcement_title: &'static str,
  pub recap_ready: &'static str,
  pub recap_listens_template: &'static str,
  pub recap_browser: &'static str,
  pub recap_controls: &'static str,
  pub recap_title: &'static str,
  pub community_pinned: &'static str,
  pub community_details: &'static str,
  pub community_add: &'static str,
  pub community_choice: &'static str,
  pub community_controls: &'static str,
  pub community_title: &'static str,
  pub exit_question: &'static str,
  pub exit_yes_no: &'static str,
  pub exit_controls: &'static str,
  pub exit_title: &'static str,
  pub sort_tracks: &'static str,
  pub sort_albums: &'static str,
  pub sort_artists: &'static str,
  pub sort_title: &'static str,
  pub sort_default: &'static str,
  pub sort_name: &'static str,
  pub sort_date_added: &'static str,
  pub sort_artist: &'static str,
  pub sort_duration: &'static str,
  pub sort_album: &'static str,
  pub sort_release_date: &'static str,
  pub party_code_prompt: &'static str,
  pub party_name_prompt: &'static str,
  pub party_join_enter: &'static str,
  pub party_name_required: &'static str,
  pub party_code_length_template: &'static str,
  pub party_name_length_template: &'static str,
  pub party_code_then_name: &'static str,
  pub esc_cancel: &'static str,
  pub listening_party: &'static str,
  pub connecting: &'static str,
  pub host_party: &'static str,
  pub join_party: &'static str,
  pub esc_close: &'static str,
  pub party_hosting: &'static str,
  pub generating: &'static str,
  pub share_code: &'static str,
  pub control_label: &'static str,
  pub control_host_only: &'static str,
  pub control_shared: &'static str,
  pub waiting_guests: &'static str,
  pub listener_one: &'static str,
  pub listeners_template: &'static str,
  pub party_toggle_control: &'static str,
  pub party_leave: &'static str,
  pub party_close_menu: &'static str,
  pub party_guest: &'static str,
  pub host_label: &'static str,
  pub room_label: &'static str,
  pub mode_label: &'static str,
  pub following_host: &'static str,
  pub party_hosting_title: &'static str,
  pub party_joined_title: &'static str,
  pub party_title: &'static str,
  pub plugin_close_hint: &'static str,
  pub fallback_session_only: &'static str,
  pub create_playlist_title: &'static str,
  pub playlist_name_label: &'static str,
  pub create_continue_hint: &'static str,
  pub add_tracks_title_template: &'static str,
  pub create_search_title: &'static str,
  pub unknown_artist: &'static str,
  pub create_results_title: &'static str,
  pub added_tracks_title_template: &'static str,
  pub soloist_pair_title: &'static str,
  pub soloist_pair_open: &'static str,
  pub soloist_pair_pick_template: &'static str,
  pub soloist_pair_waiting: &'static str,
  pub soloist_pair_close: &'static str,
  pub soloist_offer_title: &'static str,
  pub soloist_offer_body: &'static str,
  pub soloist_offer_step_download: &'static str,
  pub soloist_offer_step_key: &'static str,
  pub soloist_offer_step_engine: &'static str,
  pub soloist_offer_set_up: &'static str,
  pub soloist_offer_not_now: &'static str,
  pub soloist_offer_controls: &'static str,
}

pub const EN: PopupStrings = PopupStrings {
  resume_template: "Resumes: {station}",
  now_playing: "Now playing: ",
  queue_empty_template: "Queue is empty — press {key} on a track to add it",
  queue_up_next: "Up next from context:",
  queue_title_template: "Queue  ({key} remove · J/K move · Enter play · Esc back)",
  source_local: "Local",
  api_response: "Api response: ",
  playback_check: "If you are trying to play a track, please check that",
  check_premium: " 1. You have a Spotify Premium Account",
  check_device: " 2. Your playback device is active and selected - press `d` to go to device selection menu",
  check_spotifyd: " 3. If you're using spotifyd as a playback device, your device name must not contain spaces",
  playback_hint: "Hint: a playback device must be either an official spotify client or a light weight alternative such as spotifyd",
  error_back: "\nPress <Esc> to return",
  error_title: "Error",
  log_hint: "Rerun with --debug for more detail. Read it before posting publicly.",
  log_file: "Log file: ",
  delete_playlist_question: "Are you sure you want to delete the playlist: ",
  question_mark: "?",
  confirm: "Confirm",
  remove_track_question: "Remove this track from playlist?",
  track_template: "Track: {name}",
  playlist_template: "Playlist: {name}",
  remove_track: "Remove Track",
  fallback_terminal: "Ctrl+, is not reported by this terminal stack.",
  fallback_question: "Use fallback shortcut for Open Settings?",
  save_shortcut_template: "Save as: {key}",
  save_shortcut_fallback: "Save Shortcut Fallback",
  remove_link_question: "Remove the playlist link for:",
  mirrors_stay: "The mirror playlists stay.",
  remove_link: "Remove Link",
  ok: "Ok",
  cancel: "Cancel",
  add_track_title: "Add Track To Playlist",
  selected_track: "Selected track",
  choose_playlist_template: "Choose a playlist for: {track}",
  no_editable_playlists: "No editable playlists available",
  collaborative_playlist_template: "{name} - {owner} (collab)",
  playlist_picker_controls_template: "Enter add/open | q cancel | {down}/{up} or arrows move | H/M/L jump",
  mirror_playlist: "Mirror Playlist",
  selected_playlist: "Selected playlist",
  mirror_onto_template: "Mirror \"{name}\" onto:",
  no_mirror_source: "No other source can take a mirror",
  mirror_picker_controls_template: "Enter mirror | q cancel | {down}/{up} or arrows move",
  announcement_info: "INFO",
  announcement_warning: "WARNING",
  announcement_critical: "CRITICAL",
  announcement_more_template: "More: {url}",
  announcement_dismiss: "[Press ENTER or ESC to dismiss]",
  announcement_title: " Announcement ",
  recap_ready: "Your monthly listening recap is ready! 🎉",
  recap_listens_template: "{count} listens made it into your 30-day recap.",
  recap_browser: "Open it in the browser to view and download your share card.",
  recap_controls: "[ENTER] Open   [ESC] Later   [d] Don't show this again",
  recap_title: " Monthly Recap ",
  community_pinned: "📌 spotatui community playlist pinned",
  community_details: "The \"spotatui community\" playlist is now pinned to the top of your Spotify playlists.",
  community_add: "Add songs to it any time with the /queue command in the spotatui Discord.",
  community_choice: "Keep it pinned, or hide it (you can re-enable it later in Settings).",
  community_controls: "[Enter] keep   [h] hide   (Esc keeps it)",
  community_title: " Community Playlist ",
  exit_question: "Exit spotatui?",
  exit_yes_no: "Press Y for Yes or N for No",
  exit_controls: "[ENTER = Yes, ESC = No]",
  exit_title: " Confirm Exit ",
  sort_tracks: "Sort Tracks",
  sort_albums: "Sort Albums",
  sort_artists: "Sort Artists",
  sort_title: "Sort",
  sort_default: "Default",
  sort_name: "Name",
  sort_date_added: "Date Added",
  sort_artist: "Artist",
  sort_duration: "Duration",
  sort_album: "Album",
  sort_release_date: "Release Date",
  party_code_prompt: "Enter 6-character party code:",
  party_name_prompt: "Enter your name:",
  party_join_enter: "Press Enter to join",
  party_name_required: "Type a display name to continue",
  party_code_length_template: "{count}/6 characters",
  party_name_length_template: "Name length: {count}/32",
  party_code_then_name: "Code fills first, then name input",
  esc_cancel: "Esc to cancel",
  listening_party: "Listening Party",
  connecting: "Connecting...",
  host_party: "Host a Party",
  join_party: "Join a Party",
  esc_close: "Esc to close",
  party_hosting: "Hosting Listening Party",
  generating: "Generating...",
  share_code: "Share this code: ",
  control_label: "Control: ",
  control_host_only: "Host Only",
  control_shared: "Shared Control",
  waiting_guests: "Waiting for guests...",
  listener_one: "1 listener:",
  listeners_template: "{count} listeners:",
  party_toggle_control: "c - toggle control mode",
  party_leave: "l - leave party",
  party_close_menu: "Esc to close menu",
  party_guest: "Listening Party (Guest)",
  host_label: "Host: ",
  room_label: "Room: ",
  mode_label: "Mode: ",
  following_host: "Following host playback",
  party_hosting_title: "Party (Hosting)",
  party_joined_title: "Party (Joined)",
  party_title: "Party",
  plugin_close_hint: "(Esc to close)",
  fallback_session_only: "Using Alt+, for this session only",
  create_playlist_title: "Create Playlist (Esc to cancel)",
  playlist_name_label: "Playlist name:",
  create_continue_hint: "Press Enter to continue, Esc to cancel",
  add_tracks_title_template: "Add Tracks to \"{name}\" (Enter=create, Tab=switch panel, Esc=cancel)",
  create_search_title: "Search (Enter to search)",
  unknown_artist: "Unknown",
  create_results_title: "Results (Enter to add)",
  added_tracks_title_template: "Added ({count}) — d=remove, Enter=create",
  soloist_pair_title: "Pair spotatui with Spotify",
  soloist_pair_open: "1. Open Spotify on a computer on this network.",
  soloist_pair_pick_template: "2. In its device menu, pick '{name}'.",
  soloist_pair_waiting: "Waiting for Spotify…",
  soloist_pair_close: "Close",
  soloist_offer_title: "Play through Soloist?",
  soloist_offer_body: "Spotify refuses librespot the audio keys for these tracks, as it does for some accounts. Soloist, Spotify's own player, can play them. It needs Premium. In Settings:",
  soloist_offer_step_download: "1. Soloist Path: Download from Spotify.",
  soloist_offer_step_key: "2. Soloist API Key: Get a key, then enter it.",
  soloist_offer_step_engine: "3. Playback Engine: Soloist.",
  soloist_offer_set_up: "Set up Soloist",
  soloist_offer_not_now: "Not now",
  soloist_offer_controls: "Enter: open Settings   Esc: not now",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: PopupStrings = PopupStrings {
  resume_template: "随后恢复：{station}",
  now_playing: "正在播放：",
  queue_empty_template: "队列为空 — 选中曲目后按 {key} 添加",
  queue_up_next: "所属列表的后续曲目：",
  queue_title_template: "队列（{key} 移除 · J/K 移动 · Enter 播放 · Esc 返回）",
  source_local: "本地",
  api_response: "API 响应：",
  playback_check: "如果无法播放曲目，请确认：",
  check_premium: " 1. 你拥有 Spotify Premium 账号",
  check_device: " 2. 播放设备已启用并选中 — 按 `d` 打开设备选择菜单",
  check_spotifyd: " 3. 如果使用 spotifyd 播放，设备名称不能含有空格",
  playback_hint: "提示：播放设备必须使用 Spotify 官方客户端或 spotifyd 等轻量替代客户端",
  error_back: "\n按 <Esc> 返回",
  error_title: "错误",
  log_hint: "使用 --debug 重新运行可查看详情。公开发布日志前请先检查内容。",
  log_file: "日志文件：",
  delete_playlist_question: "即将删除以下播放列表：",
  question_mark: "确定吗？",
  confirm: "确认",
  remove_track_question: "从播放列表中移除这首曲目吗？",
  track_template: "曲目：{name}",
  playlist_template: "播放列表：{name}",
  remove_track: "移除曲目",
  fallback_terminal: "当前终端无法传递 Ctrl+, 按键。",
  fallback_question: "使用备用快捷键打开设置吗？",
  save_shortcut_template: "保存为：{key}",
  save_shortcut_fallback: "保存备用快捷键",
  remove_link_question: "移除以下播放列表的同步链接：",
  mirrors_stay: "镜像播放列表会保留。",
  remove_link: "移除链接",
  ok: "确定",
  cancel: "取消",
  add_track_title: "将曲目加入播放列表",
  selected_track: "所选曲目",
  choose_playlist_template: "为曲目选择播放列表：{track}",
  no_editable_playlists: "没有可编辑的播放列表",
  collaborative_playlist_template: "{name} - {owner}（协作）",
  playlist_picker_controls_template:
    "Enter 添加/打开 | q 取消 | {down}/{up} 或方向键移动 | H/M/L 跳转",
  mirror_playlist: "镜像播放列表",
  selected_playlist: "所选播放列表",
  mirror_onto_template: "将“{name}”镜像到：",
  no_mirror_source: "没有其他音源可接收镜像",
  mirror_picker_controls_template: "Enter 镜像 | q 取消 | {down}/{up} 或方向键移动",
  announcement_info: "信息",
  announcement_warning: "警告",
  announcement_critical: "紧急",
  announcement_more_template: "详情：{url}",
  announcement_dismiss: "[按 ENTER 或 ESC 关闭]",
  announcement_title: " 公告 ",
  recap_ready: "你的月度听歌回顾已生成！🎉",
  recap_listens_template: "你的 30 天回顾收录了 {count} 次播放。",
  recap_browser: "在浏览器中打开，可查看并下载分享卡片。",
  recap_controls: "[ENTER] 打开   [ESC] 稍后   [d] 不再提示",
  recap_title: " 月度回顾 ",
  community_pinned: "📌 spotatui 社区播放列表已置顶",
  community_details: "“spotatui community”播放列表已置顶于你的 Spotify 播放列表。",
  community_add: "随时可在 spotatui Discord 中使用 /queue 命令添加歌曲。",
  community_choice: "保留置顶或隐藏（之后可在设置中重新启用）。",
  community_controls: "[Enter] 保留   [h] 隐藏   （Esc 保留）",
  community_title: " 社区播放列表 ",
  exit_question: "退出 spotatui？",
  exit_yes_no: "按 Y 确认，按 N 取消",
  exit_controls: "[ENTER = 确认，ESC = 取消]",
  exit_title: " 确认退出 ",
  sort_tracks: "曲目排序",
  sort_albums: "专辑排序",
  sort_artists: "艺人排序",
  sort_title: "排序",
  sort_default: "默认",
  sort_name: "名称",
  sort_date_added: "添加日期",
  sort_artist: "艺人",
  sort_duration: "时长",
  sort_album: "专辑",
  sort_release_date: "发行日期",
  party_code_prompt: "输入 6 位房间码：",
  party_name_prompt: "输入你的名字：",
  party_join_enter: "按 Enter 加入",
  party_name_required: "输入显示名称以继续",
  party_code_length_template: "{count}/6 个字符",
  party_name_length_template: "名称长度：{count}/32",
  party_code_then_name: "先输入房间码，再输入名称",
  esc_cancel: "Esc 取消",
  listening_party: "一起听",
  connecting: "连接中…",
  host_party: "发起一起听",
  join_party: "加入一起听",
  esc_close: "Esc 关闭",
  party_hosting: "正在主持一起听",
  generating: "生成中…",
  share_code: "分享此房间码：",
  control_label: "控制：",
  control_host_only: "仅房主",
  control_shared: "共同控制",
  waiting_guests: "等待听众加入…",
  listener_one: "1 位听众：",
  listeners_template: "{count} 位听众：",
  party_toggle_control: "c - 切换控制模式",
  party_leave: "l - 离开一起听",
  party_close_menu: "Esc 关闭菜单",
  party_guest: "一起听（听众）",
  host_label: "房主：",
  room_label: "房间：",
  mode_label: "模式：",
  following_host: "跟随房主播放",
  party_hosting_title: "一起听（房主）",
  party_joined_title: "一起听（已加入）",
  party_title: "一起听",
  plugin_close_hint: "（Esc 关闭）",
  fallback_session_only: "仅在本次会话中使用 Alt+,",
  create_playlist_title: "创建播放列表（Esc 取消）",
  playlist_name_label: "播放列表名称：",
  create_continue_hint: "按 Enter 继续，Esc 取消",
  add_tracks_title_template: "向“{name}”添加曲目（Enter=创建，Tab=切换面板，Esc=取消）",
  create_search_title: "搜索（Enter 搜索）",
  unknown_artist: "未知",
  create_results_title: "结果（Enter 添加）",
  added_tracks_title_template: "已添加（{count}）— d=移除，Enter=创建",
  ..EN
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: PopupStrings = PopupStrings {
  resume_template: "隨後恢復：{station}",
  now_playing: "正在播放：",
  queue_empty_template: "佇列為空 — 選取曲目後按 {key} 加入",
  queue_up_next: "所屬清單的後續曲目：",
  queue_title_template: "佇列（{key} 移除 · J/K 移動 · Enter 播放 · Esc 返回）",
  source_local: "本機",
  api_response: "API 回應：",
  playback_check: "如果無法播放曲目，請確認：",
  check_premium: " 1. 你擁有 Spotify Premium 帳號",
  check_device: " 2. 播放裝置已啟用並選取 — 按 `d` 開啟裝置選擇選單",
  check_spotifyd: " 3. 如果使用 spotifyd 播放，裝置名稱不能含有空格",
  playback_hint: "提示：播放裝置必須使用 Spotify 官方用戶端或 spotifyd 等輕量替代用戶端",
  error_back: "\n按 <Esc> 返回",
  error_title: "錯誤",
  log_hint: "使用 --debug 重新執行可查看詳情。公開發布日誌前請先檢查內容。",
  log_file: "日誌檔案：",
  delete_playlist_question: "即將刪除以下播放清單：",
  question_mark: "確定嗎？",
  confirm: "確認",
  remove_track_question: "從播放清單中移除這首曲目嗎？",
  track_template: "曲目：{name}",
  playlist_template: "播放清單：{name}",
  remove_track: "移除曲目",
  fallback_terminal: "目前終端機無法傳遞 Ctrl+, 按鍵。",
  fallback_question: "使用備用快速鍵開啟設定嗎？",
  save_shortcut_template: "儲存為：{key}",
  save_shortcut_fallback: "儲存備用快速鍵",
  remove_link_question: "移除以下播放清單的同步連結：",
  mirrors_stay: "鏡像播放清單會保留。",
  remove_link: "移除連結",
  ok: "確定",
  cancel: "取消",
  add_track_title: "將曲目加入播放清單",
  selected_track: "所選曲目",
  choose_playlist_template: "為曲目選擇播放清單：{track}",
  no_editable_playlists: "沒有可編輯的播放清單",
  collaborative_playlist_template: "{name} - {owner}（協作）",
  playlist_picker_controls_template:
    "Enter 加入/開啟 | q 取消 | {down}/{up} 或方向鍵移動 | H/M/L 跳轉",
  mirror_playlist: "鏡像播放清單",
  selected_playlist: "所選播放清單",
  mirror_onto_template: "將「{name}」鏡像到：",
  no_mirror_source: "沒有其他來源可接收鏡像",
  mirror_picker_controls_template: "Enter 鏡像 | q 取消 | {down}/{up} 或方向鍵移動",
  announcement_info: "資訊",
  announcement_warning: "警告",
  announcement_critical: "緊急",
  announcement_more_template: "詳情：{url}",
  announcement_dismiss: "[按 ENTER 或 ESC 關閉]",
  announcement_title: " 公告 ",
  recap_ready: "你的每月聆聽回顧已產生！🎉",
  recap_listens_template: "你的 30 天回顧收錄了 {count} 次播放。",
  recap_browser: "在瀏覽器中開啟，可查看並下載分享卡片。",
  recap_controls: "[ENTER] 開啟   [ESC] 稍後   [d] 不再提示",
  recap_title: " 每月回顧 ",
  community_pinned: "📌 spotatui 社群播放清單已置頂",
  community_details: "「spotatui community」播放清單已置頂於你的 Spotify 播放清單。",
  community_add: "隨時可在 spotatui Discord 中使用 /queue 指令加入歌曲。",
  community_choice: "保留置頂或隱藏（之後可在設定中重新啟用）。",
  community_controls: "[Enter] 保留   [h] 隱藏   （Esc 保留）",
  community_title: " 社群播放清單 ",
  exit_question: "結束 spotatui？",
  exit_yes_no: "按 Y 確認，按 N 取消",
  exit_controls: "[ENTER = 確認，ESC = 取消]",
  exit_title: " 確認結束 ",
  sort_tracks: "曲目排序",
  sort_albums: "專輯排序",
  sort_artists: "藝人排序",
  sort_title: "排序",
  sort_default: "預設",
  sort_name: "名稱",
  sort_date_added: "加入日期",
  sort_artist: "藝人",
  sort_duration: "長度",
  sort_album: "專輯",
  sort_release_date: "發行日期",
  party_code_prompt: "輸入 6 碼房間碼：",
  party_name_prompt: "輸入你的名字：",
  party_join_enter: "按 Enter 加入",
  party_name_required: "輸入顯示名稱以繼續",
  party_code_length_template: "{count}/6 個字元",
  party_name_length_template: "名稱長度：{count}/32",
  party_code_then_name: "先輸入房間碼，再輸入名稱",
  esc_cancel: "Esc 取消",
  listening_party: "一起聽",
  connecting: "連線中…",
  host_party: "發起一起聽",
  join_party: "加入一起聽",
  esc_close: "Esc 關閉",
  party_hosting: "正在主持一起聽",
  generating: "產生中…",
  share_code: "分享此房間碼：",
  control_label: "控制：",
  control_host_only: "僅房主",
  control_shared: "共同控制",
  waiting_guests: "等待聽眾加入…",
  listener_one: "1 位聽眾：",
  listeners_template: "{count} 位聽眾：",
  party_toggle_control: "c - 切換控制模式",
  party_leave: "l - 離開一起聽",
  party_close_menu: "Esc 關閉選單",
  party_guest: "一起聽（聽眾）",
  host_label: "房主：",
  room_label: "房間：",
  mode_label: "模式：",
  following_host: "跟隨房主播放",
  party_hosting_title: "一起聽（房主）",
  party_joined_title: "一起聽（已加入）",
  party_title: "一起聽",
  plugin_close_hint: "（Esc 關閉）",
  fallback_session_only: "僅在本次工作階段中使用 Alt+,",
  create_playlist_title: "建立播放清單（Esc 取消）",
  playlist_name_label: "播放清單名稱：",
  create_continue_hint: "按 Enter 繼續，Esc 取消",
  add_tracks_title_template: "向「{name}」加入曲目（Enter=建立，Tab=切換面板，Esc=取消）",
  create_search_title: "搜尋（Enter 搜尋）",
  unknown_artist: "未知",
  create_results_title: "結果（Enter 加入）",
  added_tracks_title_template: "已加入（{count}）— d=移除，Enter=建立",
  ..EN
};

#[cfg(any(feature = "tui", feature = "gui", test))]
impl PopupStrings {
  pub fn soloist_pair_pick(&self, name: &str) -> String {
    super::format_template(self.soloist_pair_pick_template, &[("name", name)])
  }
}

#[cfg(any(feature = "tui", test))]
impl PopupStrings {
  #[cfg(any(feature = "internet-radio", test))]
  pub fn resume(&self, station: &str) -> String {
    super::format_template(self.resume_template, &[("station", station)])
  }

  pub fn queue_empty(&self, key: &str) -> String {
    super::format_template(self.queue_empty_template, &[("key", key)])
  }

  pub fn queue_title(&self, key: &str) -> String {
    super::format_template(self.queue_title_template, &[("key", key)])
  }

  pub fn track(&self, name: &str) -> String {
    super::format_template(self.track_template, &[("name", name)])
  }

  pub fn playlist(&self, name: &str) -> String {
    super::format_template(self.playlist_template, &[("name", name)])
  }

  pub fn save_shortcut(&self, key: &str) -> String {
    super::format_template(self.save_shortcut_template, &[("key", key)])
  }

  pub fn choose_playlist(&self, track: &str) -> String {
    super::format_template(self.choose_playlist_template, &[("track", track)])
  }

  pub fn collaborative_playlist(&self, name: &str, owner: &str) -> String {
    super::format_template(
      self.collaborative_playlist_template,
      &[("name", name), ("owner", owner)],
    )
  }

  pub fn playlist_picker_controls(&self, down: &str, up: &str) -> String {
    super::format_template(
      self.playlist_picker_controls_template,
      &[("down", down), ("up", up)],
    )
  }

  pub fn mirror_onto(&self, name: &str) -> String {
    super::format_template(self.mirror_onto_template, &[("name", name)])
  }

  pub fn mirror_picker_controls(&self, down: &str, up: &str) -> String {
    super::format_template(
      self.mirror_picker_controls_template,
      &[("down", down), ("up", up)],
    )
  }

  pub fn announcement_more(&self, url: &str) -> String {
    super::format_template(self.announcement_more_template, &[("url", url)])
  }

  pub fn recap_listens(&self, count: usize) -> String {
    super::format_template(
      self.recap_listens_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn party_code_length(&self, count: usize) -> String {
    super::format_template(
      self.party_code_length_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn party_name_length(&self, count: usize) -> String {
    super::format_template(
      self.party_name_length_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn listeners(&self, count: usize) -> String {
    super::format_template(self.listeners_template, &[("count", &count.to_string())])
  }

  pub fn add_tracks_title(&self, name: &str) -> String {
    super::format_template(self.add_tracks_title_template, &[("name", name)])
  }

  pub fn added_tracks_title(&self, count: usize) -> String {
    super::format_template(
      self.added_tracks_title_template,
      &[("count", &count.to_string())],
    )
  }

  pub fn sort_field(&self, field: crate::core::sort::SortField) -> &'static str {
    use crate::core::sort::SortField;
    match field {
      SortField::Default => self.sort_default,
      SortField::Name => self.sort_name,
      SortField::DateAdded => self.sort_date_added,
      SortField::Artist => self.sort_artist,
      SortField::Duration => self.sort_duration,
      SortField::Album => self.sort_album,
      SortField::ReleaseDate => self.sort_release_date,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn popup_templates_keep_english_and_literal_user_titles() {
    assert_eq!(EN.mirror_onto("{name}"), "Mirror \"{name}\" onto:");
    assert_eq!(
      EN.collaborative_playlist("{owner}", "{name}"),
      "{owner} - {name} (collab)"
    );
    assert_eq!(ZH_HANS.mirror_onto("{name}"), "将“{name}”镜像到：");
    assert_eq!(
      ZH_HANT.collaborative_playlist("{owner}", "{name}"),
      "{owner} - {name}（協作）"
    );
    assert_eq!(
      EN.queue_empty("a"),
      "Queue is empty — press a on a track to add it"
    );
    assert_eq!(
      EN.recap_listens(42),
      "42 listens made it into your 30-day recap."
    );
    assert_eq!(
      EN.added_tracks_title(3),
      "Added (3) — d=remove, Enter=create"
    );
  }
}
