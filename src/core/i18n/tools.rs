//! Visible DJ tool summaries; protocol and model-facing tool text stays unchanged.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct ToolStrings {
  pub skipped: &'static str,
  pub vibe_set: &'static str,
  pub vibe_cleared: &'static str,
  pub vibe_on: &'static str,
  pub vibe_off: &'static str,
  pub vibe_unavailable: &'static str,
  pub creating: &'static str,
  pub adding: &'static str,
  pub removing: &'static str,
  pub following: &'static str,
  pub unfollowing: &'static str,
  pub transfer: &'static str,
  pub liking: &'static str,
  pub unliking: &'static str,
  pub search_failed: &'static str,
  pub no_tracks: &'static str,
  pub found_track: &'static str,
  pub owned: &'static str,
  pub new_track: &'static str,
  pub queued: &'static str,
  pub queue_empty: &'static str,
  pub nothing_playing: &'static str,
  pub now_playing: &'static str,
  pub playing: &'static str,
  pub paused: &'static str,
  pub sent: &'static str,
  pub play_started: &'static str,
  pub missing_feature: &'static str,
  pub missing_track: &'static str,
  pub history_failed: &'static str,
  pub history_task_failed: &'static str,
  pub history_sparse: &'static str,
  pub history_summary: &'static str,
  pub catalogue_unavailable: &'static str,
  pub session_unavailable: &'static str,
  pub uneditable: &'static str,
  pub queue_filter_failed: &'static str,
  pub nothing_queued: &'static str,
  pub unknown_tool: &'static str,
  pub invalid_arguments: &'static str,
  pub shutting_down: &'static str,
  pub dropped: &'static str,
  pub timed_out: &'static str,
  pub brief_artists: &'static str,
  pub brief_tracks: &'static str,
  pub brief_albums: &'static str,
  pub brief_recent: &'static str,
  pub brief_now: &'static str,
  pub brief_vibe: &'static str,
  pub period_week: &'static str,
  pub period_month: &'static str,
  pub period_this_month: &'static str,
  pub period_year: &'static str,
  pub period_all: &'static str,
  pub ownership_incomplete: &'static str,
  pub queue_not_found: &'static str,
  pub queue_owned: &'static str,
  pub queue_duplicate: &'static str,
  pub queue_filter_spotify_only: &'static str,
  pub none_found: &'static str,
  pub none_owned: &'static str,
  pub none_duplicate: &'static str,
  pub none_new: &'static str,
}
pub const EN: ToolStrings = ToolStrings {
  brief_artists: "Top artists",
  brief_tracks: "Top tracks",
  brief_albums: "Top albums",
  brief_recent: "Recently played (do not repeat these)",
  brief_now: "\nCurrently playing: {name}\n",
  brief_vibe: "\nThe listener asked for: {name}\n",
  period_week: "the last 7 days",
  period_month: "the last 30 days",
  period_this_month: "this month",
  period_year: "this year",
  period_all: "all time",
  ownership_incomplete: "\n\nNote: the playlist index is still being built, so [owned] reflects Liked Songs only for this search. Search again for the complete answer.",
  queue_not_found: "\n\nNot found (skipped): {names}",
  queue_owned: "\nAlready in your library (skipped by exclude_owned): {names}",
  queue_duplicate: "\nAlready queued, playing, or recently played (skipped): {names}",
  queue_filter_spotify_only: "\nexclude_owned only checks Spotify tracks, so it skipped nothing here.",
  none_found: "Could not find: {names}",
  none_owned: "Already in your library: {names}",
  none_duplicate: "Already queued, playing, or recently played: {names}",
  none_new: "Every track was already queued or playing.",
  skipped: "Skipped to the next track",
  vibe_set: "DJ vibe set to: {vibe}. {effect}",
  vibe_cleared: "DJ vibe cleared",
  vibe_on: "The built-in DJ will follow it from its next refill.",
  vibe_off: "The built-in DJ's auto-queue is off, so nothing will act on it until the user turns it on; queue tracks yourself in the meantime. get_listening_history returns it either way.",
  vibe_unavailable: "This build has no built-in auto-queue DJ, so nothing will act on it automatically. It is stored and returned by get_listening_history, so you can follow it yourself when you queue.",
  creating: "Creating the playlist \"{name}\" with {count} track(s). It is in list_playlists once Spotify has made it.",
  adding: "Adding {count} track(s) to {playlist}",
  removing: "Removing {uri} at position {position} from {playlist}, if that is the track there",
  following: "Following spotify:artist:{artist}",
  unfollowing: "Unfollowing spotify:artist:{artist}",
  transfer: "Moving playback to {name}",
  liking: "Liking {changed} track(s); {unchanged} already liked",
  unliking: "Unliking {changed} track(s); {unchanged} already not liked",
  search_failed: "Search failed: {error}",
  no_tracks: "No tracks found for \"{query}\"",
  found_track: "{name} — {artists} [{uri}] [{owned}]",
  owned: "owned",
  new_track: "new",
  queued: "Queued {count} track(s):",
  queue_empty: "The queue is empty",
  nothing_playing: "Nothing is playing ({count} track(s) queued)",
  now_playing: "{state}: {label} ({count} track(s) queued)",
  playing: "playing",
  paused: "paused",
  sent: "Sent {uri} to the player",
  play_started: "Playing {label}",
  missing_feature: "Nothing is playing that: this spotatui was built without the `{feature}` feature, so it has no source that can play {uri}. Playback was left alone.",
  missing_track: "Nothing is playing that: {uri} is not in the catalogue, so playback was left alone. Use search_tracks to get a URI that exists.",
  history_failed: "Could not read listening history: {error}",
  history_task_failed: "Listening history task failed: {error}",
  history_sparse: "Very little listening history so far ({count} qualifying plays in {period}). Ask the user what they feel like instead of inferring it.",
  history_summary: "Listening history ({period}, {count} qualifying plays):",
  catalogue_unavailable: "spotatui has no Spotify session, so the catalogue is unavailable. Log in from the spotatui UI, then try again.",
  session_unavailable: "spotatui has no Spotify session. Log in from the spotatui UI, then try again.",
  uneditable: "\"{name}\" is not a playlist the user owns or collaborates on, so it cannot be edited",
  queue_filter_failed: "Could not read your playlists, so exclude_owned cannot be honoured and nothing was queued. Try again in a moment, or call queue_tracks without exclude_owned to queue these anyway.",
  nothing_queued: "Nothing was queued.\n{detail}",
  unknown_tool: "There is no tool called {name}. Use one of the tools listed above.",
  invalid_arguments: "Invalid arguments: {detail}",
  shutting_down: "spotatui is shutting down; the request was not run",
  dropped: "spotatui dropped the request before answering",
  timed_out: "spotatui did not answer in time; it may be busy. Try again in a moment.",
};
// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANS: ToolStrings = ToolStrings {
  brief_artists: "最常听艺人",
  brief_tracks: "最常听歌曲",
  brief_albums: "最常听专辑",
  brief_recent: "最近播放（不要重复）",
  brief_now: "\n正在播放：{name}\n",
  brief_vibe: "\n听众想听：{name}\n",
  period_week: "最近 7 天",
  period_month: "最近 30 天",
  period_this_month: "本月",
  period_year: "今年",
  period_all: "全部时间",
  ownership_incomplete: "\n\n提示：播放列表索引仍在建立，本次搜索的 [已收藏] 仅表示喜欢的歌曲。稍后重新搜索可获得完整结果。",
  queue_not_found: "\n\n未找到（已跳过）：{names}",
  queue_owned: "\n已在音乐库中（因 exclude_owned 跳过）：{names}",
  queue_duplicate: "\n已在队列中、正在播放或最近播放过（已跳过）：{names}",
  queue_filter_spotify_only: "\nexclude_owned 仅检查 Spotify 歌曲，本次未跳过任何歌曲。",
  none_found: "未找到：{names}",
  none_owned: "已在音乐库中：{names}",
  none_duplicate: "已在队列中、正在播放或最近播放过：{names}",
  none_new: "所有歌曲都已在队列中或正在播放。",
  skipped: "已跳至下一首",
  vibe_set: "DJ 氛围已设为：{vibe}。{effect}",
  vibe_cleared: "已清除 DJ 氛围",
  vibe_on: "内置 DJ 将从下次补充队列起遵循此氛围。",
  vibe_off: "内置 DJ 的自动补充队列已关闭，开启后才会生效；目前请手动加入歌曲。get_listening_history 仍会返回此氛围。",
  vibe_unavailable: "此版本没有内置自动补充队列 DJ，不会自动应用此氛围。氛围已保存，可由 get_listening_history 读取后用于加入歌曲。",
  creating: "正在创建播放列表“{name}”，包含 {count} 首歌曲。Spotify 创建后即可由 list_playlists 查看。",
  adding: "正在向 {playlist} 添加 {count} 首歌曲",
  removing: "正在从 {playlist} 的位置 {position} 移除 {uri}（仅在曲目相符时移除）",
  following: "正在关注 spotify:artist:{artist}",
  unfollowing: "正在取消关注 spotify:artist:{artist}",
  transfer: "正在将播放切换至 {name}",
  liking: "正在喜欢 {changed} 首歌曲；{unchanged} 首已喜欢",
  unliking: "正在取消喜欢 {changed} 首歌曲；{unchanged} 首原本未喜欢",
  search_failed: "搜索失败：{error}",
  no_tracks: "未找到与“{query}”相关的歌曲",
  found_track: "{name} — {artists} [{uri}] [{owned}]",
  owned: "已收藏",
  new_track: "未收藏",
  queued: "已加入 {count} 首歌曲：",
  queue_empty: "队列为空",
  nothing_playing: "没有正在播放的内容（队列中有 {count} 首歌曲）",
  now_playing: "{state}：{label}（队列中有 {count} 首歌曲）",
  playing: "正在播放",
  paused: "已暂停",
  sent: "已将 {uri} 交给播放器",
  play_started: "正在播放 {label}",
  missing_feature: "无法播放：此 spotatui 未编译 `{feature}` 功能，没有可播放 {uri} 的音源。当前播放未更改。",
  missing_track: "无法播放：曲库中没有 {uri}，当前播放未更改。请用 search_tracks 获取有效 URI。",
  history_failed: "无法读取收听历史：{error}",
  history_task_failed: "收听历史任务失败：{error}",
  history_sparse: "收听历史较少（{period}有 {count} 次有效播放）。请先询问用户想听什么。",
  history_summary: "收听历史（{period}，{count} 次有效播放）：",
  catalogue_unavailable: "spotatui 尚未登录 Spotify，无法访问曲库。请在 spotatui 界面中登录后重试。",
  session_unavailable: "spotatui 尚未登录 Spotify。请在 spotatui 界面中登录后重试。",
  uneditable: "“{name}”不属于用户，也不是协作播放列表，无法编辑",
  queue_filter_failed: "无法读取播放列表以执行 exclude_owned，未加入任何歌曲。请稍后重试，或在 queue_tracks 中关闭 exclude_owned 后加入。",
  nothing_queued: "未加入任何歌曲。\n{detail}",
  unknown_tool: "没有名为 {name} 的工具。请使用上方列出的工具。",
  invalid_arguments: "参数无效：{detail}",
  shutting_down: "spotatui 正在关闭，未执行此请求",
  dropped: "spotatui 在回应前取消了请求",
  timed_out: "spotatui 未及时回应，可能正在忙碌。请稍后重试。",
  ..EN
};
// Keep ..EN so new English fields compile before their translations are added.
#[allow(clippy::needless_update)]
pub const ZH_HANT: ToolStrings = ToolStrings {
  brief_artists: "最常聽藝人",
  brief_tracks: "最常聽歌曲",
  brief_albums: "最常聽專輯",
  brief_recent: "最近播放（不要重複）",
  brief_now: "\n正在播放：{name}\n",
  brief_vibe: "\n聽眾想聽：{name}\n",
  period_week: "最近 7 天",
  period_month: "最近 30 天",
  period_this_month: "本月",
  period_year: "今年",
  period_all: "全部時間",
  ownership_incomplete: "\n\n提示：播放清單索引仍在建立，本次搜尋的 [已收藏] 僅表示喜歡的歌曲。稍後重新搜尋可取得完整結果。",
  queue_not_found: "\n\n未找到（已略過）：{names}",
  queue_owned: "\n已在音樂庫中（因 exclude_owned 略過）：{names}",
  queue_duplicate: "\n已在佇列中、正在播放或最近播放過（已略過）：{names}",
  queue_filter_spotify_only: "\nexclude_owned 僅檢查 Spotify 歌曲，本次未略過任何歌曲。",
  none_found: "未找到：{names}",
  none_owned: "已在音樂庫中：{names}",
  none_duplicate: "已在佇列中、正在播放或最近播放過：{names}",
  none_new: "所有歌曲都已在佇列中或正在播放。",
  skipped: "已跳至下一首",
  vibe_set: "DJ 氛圍已設為：{vibe}。{effect}",
  vibe_cleared: "已清除 DJ 氛圍",
  vibe_on: "內建 DJ 將從下次補充佇列起遵循此氛圍。",
  vibe_off: "內建 DJ 的自動補充佇列已關閉，開啟後才會生效；目前請手動加入歌曲。get_listening_history 仍會傳回此氛圍。",
  vibe_unavailable: "此版本沒有內建自動補充佇列 DJ，不會自動套用此氛圍。氛圍已儲存，可由 get_listening_history 讀取後用於加入歌曲。",
  creating: "正在建立播放清單「{name}」，包含 {count} 首歌曲。Spotify 建立後即可由 list_playlists 查看。",
  adding: "正在向 {playlist} 新增 {count} 首歌曲",
  removing: "正在從 {playlist} 的位置 {position} 移除 {uri}（僅在曲目相符時移除）",
  following: "正在追蹤 spotify:artist:{artist}",
  unfollowing: "正在取消追蹤 spotify:artist:{artist}",
  transfer: "正在將播放切換至 {name}",
  liking: "正在喜歡 {changed} 首歌曲；{unchanged} 首已喜歡",
  unliking: "正在取消喜歡 {changed} 首歌曲；{unchanged} 首原本未喜歡",
  search_failed: "搜尋失敗：{error}",
  no_tracks: "未找到與「{query}」相關的歌曲",
  found_track: "{name} — {artists} [{uri}] [{owned}]",
  owned: "已收藏",
  new_track: "未收藏",
  queued: "已加入 {count} 首歌曲：",
  queue_empty: "佇列為空",
  nothing_playing: "沒有正在播放的內容（佇列中有 {count} 首歌曲）",
  now_playing: "{state}：{label}（佇列中有 {count} 首歌曲）",
  playing: "正在播放",
  paused: "已暫停",
  sent: "已將 {uri} 交給播放器",
  play_started: "正在播放 {label}",
  missing_feature: "無法播放：此 spotatui 未編譯 `{feature}` 功能，沒有可播放 {uri} 的來源。目前播放未變更。",
  missing_track: "無法播放：曲庫中沒有 {uri}，目前播放未變更。請用 search_tracks 取得有效 URI。",
  history_failed: "無法讀取聆聽紀錄：{error}",
  history_task_failed: "聆聽紀錄工作失敗：{error}",
  history_sparse: "聆聽紀錄較少（{period}有 {count} 次有效播放）。請先詢問使用者想聽什麼。",
  history_summary: "聆聽紀錄（{period}，{count} 次有效播放）：",
  catalogue_unavailable: "spotatui 尚未登入 Spotify，無法存取曲庫。請在 spotatui 介面中登入後重試。",
  session_unavailable: "spotatui 尚未登入 Spotify。請在 spotatui 介面中登入後重試。",
  uneditable: "「{name}」不屬於使用者，也不是協作播放清單，無法編輯",
  queue_filter_failed: "無法讀取播放清單以執行 exclude_owned，未加入任何歌曲。請稍後重試，或在 queue_tracks 中關閉 exclude_owned 後加入。",
  nothing_queued: "未加入任何歌曲。\n{detail}",
  unknown_tool: "沒有名為 {name} 的工具。請使用上方列出的工具。",
  invalid_arguments: "參數無效：{detail}",
  shutting_down: "spotatui 正在關閉，未執行此請求",
  dropped: "spotatui 在回應前取消了請求",
  timed_out: "spotatui 未及時回應，可能正在忙碌。請稍後重試。",
  ..EN
};
#[cfg(any(feature = "dj-core", test))]
impl ToolStrings {
  pub fn brief_now(&self, name: &str) -> String {
    super::format_template(self.brief_now, &[("name", name)])
  }

  pub fn brief_vibe(&self, name: &str) -> String {
    super::format_template(self.brief_vibe, &[("name", name)])
  }

  pub fn queue_not_found(&self, names: &str) -> String {
    super::format_template(self.queue_not_found, &[("names", names)])
  }

  pub fn queue_owned(&self, names: &str) -> String {
    super::format_template(self.queue_owned, &[("names", names)])
  }

  pub fn queue_duplicate(&self, names: &str) -> String {
    super::format_template(self.queue_duplicate, &[("names", names)])
  }

  pub fn none_found(&self, names: &str) -> String {
    super::format_template(self.none_found, &[("names", names)])
  }

  pub fn none_owned(&self, names: &str) -> String {
    super::format_template(self.none_owned, &[("names", names)])
  }

  pub fn none_duplicate(&self, names: &str) -> String {
    super::format_template(self.none_duplicate, &[("names", names)])
  }

  pub fn vibe_set(&self, vibe: &str, effect: &str) -> String {
    super::format_template(self.vibe_set, &[("vibe", vibe), ("effect", effect)])
  }
  pub fn creating(&self, name: &str, count: usize) -> String {
    super::format_template(
      self.creating,
      &[("name", name), ("count", &count.to_string())],
    )
  }
  pub fn adding(&self, count: usize, playlist: &str) -> String {
    super::format_template(
      self.adding,
      &[("count", &count.to_string()), ("playlist", playlist)],
    )
  }
  pub fn removing(&self, uri: &str, position: usize, playlist: &str) -> String {
    super::format_template(
      self.removing,
      &[
        ("uri", uri),
        ("position", &position.to_string()),
        ("playlist", playlist),
      ],
    )
  }
  pub fn following(&self, artist: &str) -> String {
    super::format_template(self.following, &[("artist", artist)])
  }
  pub fn unfollowing(&self, artist: &str) -> String {
    super::format_template(self.unfollowing, &[("artist", artist)])
  }
  pub fn transfer(&self, name: &str) -> String {
    super::format_template(self.transfer, &[("name", name)])
  }
  pub fn liking(&self, changed: usize, unchanged: usize) -> String {
    super::format_template(
      self.liking,
      &[
        ("changed", &changed.to_string()),
        ("unchanged", &unchanged.to_string()),
      ],
    )
  }
  pub fn unliking(&self, changed: usize, unchanged: usize) -> String {
    super::format_template(
      self.unliking,
      &[
        ("changed", &changed.to_string()),
        ("unchanged", &unchanged.to_string()),
      ],
    )
  }
  pub fn search_failed(&self, error: &str) -> String {
    super::format_template(self.search_failed, &[("error", error)])
  }
  pub fn no_tracks(&self, query: &str) -> String {
    super::format_template(self.no_tracks, &[("query", query)])
  }
  pub fn found_track(&self, name: &str, artists: &str, uri: &str, owned: &str) -> String {
    super::format_template(
      self.found_track,
      &[
        ("name", name),
        ("artists", artists),
        ("uri", uri),
        ("owned", owned),
      ],
    )
  }
  pub fn queued(&self, count: usize) -> String {
    super::format_template(self.queued, &[("count", &count.to_string())])
  }
  pub fn nothing_playing(&self, count: usize) -> String {
    super::format_template(self.nothing_playing, &[("count", &count.to_string())])
  }
  pub fn now_playing(&self, state: &str, label: &str, count: usize) -> String {
    super::format_template(
      self.now_playing,
      &[
        ("state", state),
        ("label", label),
        ("count", &count.to_string()),
      ],
    )
  }
  pub fn sent(&self, uri: &str) -> String {
    super::format_template(self.sent, &[("uri", uri)])
  }
  pub fn play_started(&self, label: &str) -> String {
    super::format_template(self.play_started, &[("label", label)])
  }
  pub fn missing_feature(&self, feature: &str, uri: &str) -> String {
    super::format_template(self.missing_feature, &[("feature", feature), ("uri", uri)])
  }
  pub fn missing_track(&self, uri: &str) -> String {
    super::format_template(self.missing_track, &[("uri", uri)])
  }
  pub fn history_failed(&self, error: &str) -> String {
    super::format_template(self.history_failed, &[("error", error)])
  }
  pub fn history_task_failed(&self, error: &str) -> String {
    super::format_template(self.history_task_failed, &[("error", error)])
  }
  pub fn history_sparse(&self, count: usize, period: &str) -> String {
    super::format_template(
      self.history_sparse,
      &[("count", &count.to_string()), ("period", period)],
    )
  }
  pub fn history_summary(&self, period: &str, count: usize) -> String {
    super::format_template(
      self.history_summary,
      &[("period", period), ("count", &count.to_string())],
    )
  }
  pub fn uneditable(&self, name: &str) -> String {
    super::format_template(self.uneditable, &[("name", name)])
  }
  pub fn nothing_queued(&self, detail: &str) -> String {
    super::format_template(self.nothing_queued, &[("detail", detail)])
  }
  #[cfg(any(feature = "ai-dj", test))]
  pub fn unknown_tool(&self, name: &str) -> String {
    super::format_template(self.unknown_tool, &[("name", name)])
  }
  #[cfg(any(feature = "ai-dj", test))]
  pub fn invalid_arguments(&self, detail: &str) -> String {
    super::format_template(self.invalid_arguments, &[("detail", detail)])
  }
}
