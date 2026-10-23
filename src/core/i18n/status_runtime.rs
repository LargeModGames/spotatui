//! Startup and routing messages, shared by both frontends.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct RuntimeStatusStrings {
  pub local_file_missing: &'static str,
  pub mcp_listening: &'static str,
  pub mcp_start_failed: &'static str,
  pub native_start_failed: &'static str,
  pub premium_required: &'static str,
  pub native_offline: &'static str,
  pub saved_device_unavailable: &'static str,
  pub apple_music_needs_macos: &'static str,
  pub feature_missing: &'static str,
  pub spotify_login_failed: &'static str,
  pub song_counter_title: &'static str,
  pub song_counter_body: &'static str,
  pub song_counter_question: &'static str,
  pub song_counter_save_failed: &'static str,
  pub song_counter_thanks: &'static str,
  pub song_counter_opted_out: &'static str,
  pub forgetting_spotify_login: &'static str,
}

pub const EN: RuntimeStatusStrings = RuntimeStatusStrings {
  local_file_missing: "Cannot find local file: {path}",
  mcp_listening: "MCP server listening on 127.0.0.1:{port}",
  mcp_start_failed: "MCP server failed to start: {error}",
  native_start_failed: "Native streaming didn't start; using Spotify Connect for now. Restart spotatui to reconnect native playback.",
  premium_required: "Spotify Premium is required for playback",
  native_offline: "Could not connect to Spotify: native streaming disabled",
  saved_device_unavailable: "Saved device unavailable; using {device}",
  apple_music_needs_macos: "Apple Music playback needs macOS",
  feature_missing: "This spotatui was built without the `{feature}` feature, so nothing can play that.",
  spotify_login_failed: "Spotify login failed: {error}. Press `d` and pick Spotify to try again.",
  song_counter_title: "Global Song Counter",
  song_counter_body: "\nspotatui can contribute to a global counter showing total\nsongs played by all users worldwide.\n\nPrivacy: This feature is completely anonymous.\n• No personal information is collected\n• No song names, artists, or listening history\n• Only a simple increment once a song has played for 30 seconds",
  song_counter_question: "\nWould you like to participate? (Y/n): ",
  song_counter_save_failed: "Could not save your answer to {path} (read-only config?). Set `enable_global_song_count` under `behavior:` there to stop this prompt.\n",
  song_counter_thanks: "Thank you for participating!\n",
  song_counter_opted_out: "Opted out. You can change this anytime in Settings -> Behavior.\n",
  forgetting_spotify_login: "\nForgetting the saved Spotify login...",
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: RuntimeStatusStrings = RuntimeStatusStrings {
  local_file_missing: "找不到本地文件：{path}",
  mcp_listening: "MCP 服务器正在监听 127.0.0.1:{port}",
  mcp_start_failed: "MCP 服务器启动失败：{error}",
  native_start_failed: "内置播放器未能启动，暂时使用 Spotify Connect。请重启 spotatui 以恢复内置播放。",
  premium_required: "播放需要 Spotify Premium",
  native_offline: "无法连接 Spotify：已停用内置播放",
  saved_device_unavailable: "保存的设备不可用，正在使用 {device}",
  apple_music_needs_macos: "Apple Music 播放需要 macOS",
  feature_missing: "此 spotatui 未编译 `{feature}` 功能，无法播放此内容。",
  spotify_login_failed: "Spotify 登录失败：{error}。按 `d` 并选择 Spotify 重试。",
  song_counter_title: "全球播放计数",
  song_counter_body: "\nspotatui 可以为全球计数器贡献数据，\n统计所有用户播放的歌曲总数。\n\n隐私：此功能完全匿名。\n• 不收集个人信息\n• 不收集歌曲名、艺人或收听历史\n• 每首歌播放满 30 秒后，仅将计数加一",
  song_counter_question: "\n是否参与？(Y/n): ",
  song_counter_save_failed: "无法将选择保存到 {path}（配置可能为只读）。请在该文件的 `behavior:` 下设置 `enable_global_song_count`，以关闭此提示。\n",
  song_counter_thanks: "感谢参与！\n",
  song_counter_opted_out: "已选择不参与。你可随时在“设置 → 行为”中更改。\n",
  forgetting_spotify_login: "\n正在清除保存的 Spotify 登录信息…",
  ..EN
};

// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: RuntimeStatusStrings = RuntimeStatusStrings {
  local_file_missing: "找不到本機檔案：{path}",
  mcp_listening: "MCP 伺服器正在監聽 127.0.0.1:{port}",
  mcp_start_failed: "MCP 伺服器啟動失敗：{error}",
  native_start_failed: "內建串流未能啟動，暫時使用 Spotify Connect。請重新啟動 spotatui 以恢復內建串流。",
  premium_required: "播放需要 Spotify Premium",
  native_offline: "無法連線至 Spotify：已停用內建串流",
  saved_device_unavailable: "儲存的裝置無法使用，正在使用 {device}",
  apple_music_needs_macos: "Apple Music 播放需要 macOS",
  feature_missing: "此 spotatui 未編譯 `{feature}` 功能，無法播放此內容。",
  spotify_login_failed: "Spotify 登入失敗：{error}。按 `d` 並選擇 Spotify 重試。",
  song_counter_title: "全球播放計數",
  song_counter_body: "\nspotatui 可以為全球計數器貢獻資料，\n統計所有使用者播放的歌曲總數。\n\n隱私：此功能完全匿名。\n• 不收集個人資訊\n• 不收集歌曲名稱、藝人或聆聽紀錄\n• 每首歌播放滿 30 秒後，僅將計數加一",
  song_counter_question: "\n是否參與？(Y/n): ",
  song_counter_save_failed: "無法將選擇儲存至 {path}（設定檔可能為唯讀）。請在該檔案的 `behavior:` 下設定 `enable_global_song_count`，以關閉此提示。\n",
  song_counter_thanks: "感謝參與！\n",
  song_counter_opted_out: "已選擇不參與。你可隨時在「設定 → 行為」中變更。\n",
  forgetting_spotify_login: "\n正在清除儲存的 Spotify 登入資訊…",
  ..EN
};

// The full table is shipped on every feature leg; some producers are optional.
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
impl RuntimeStatusStrings {
  pub fn song_counter_save_failed(&self, path: &str) -> String {
    super::format_template(self.song_counter_save_failed, &[("path", path)])
  }
  pub fn local_file_missing(&self, path: &str) -> String {
    super::format_template(self.local_file_missing, &[("path", path)])
  }
  pub fn mcp_listening(&self, port: u16) -> String {
    super::format_template(self.mcp_listening, &[("port", &port.to_string())])
  }
  pub fn mcp_start_failed(&self, error: &str) -> String {
    super::format_template(self.mcp_start_failed, &[("error", error)])
  }
  pub fn saved_device_unavailable(&self, device: &str) -> String {
    super::format_template(self.saved_device_unavailable, &[("device", device)])
  }
  pub fn feature_missing(&self, feature: &str) -> String {
    super::format_template(self.feature_missing, &[("feature", feature)])
  }
  pub fn spotify_login_failed(&self, error: &str) -> String {
    super::format_template(self.spotify_login_failed, &[("error", error)])
  }
}
