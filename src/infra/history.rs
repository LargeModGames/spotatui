use crate::core::app::{ActiveBlock, App, RecapPromptState, RouteId};
use crate::infra::media_metadata::{
  current_playback_snapshot, PlaybackItemKind, PlaybackSnapshot, PlaybackSource,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

const HISTORY_SUBDIR: &str = "history";
const LISTENS_FILE_NAME: &str = "listens.jsonl";
const SYNC_BASE_URL: &str = "https://spotatui.com";
const CLOUD_SYNC_PATH: &str = "/api/sync";
const NOW_PLAYING_SYNC_PATH: &str = "/api/sync/now-playing";
/// Heartbeat interval: must be well under the 5-minute online threshold used by the website.
const NOW_PLAYING_HEARTBEAT_SECS: u64 = 60;
/// Push immediately when observed progress deviates this far from the progress
/// expected since the last push (a seek), keeping the widget's bar honest.
const NOW_PLAYING_SEEK_THRESHOLD_MS: u128 = 5_000;
const SYNC_BASE_URL_ENV_KEY: &str = "SPOTATUI_SYNC_BASE_URL";
const MAX_INTERVAL_MS: u64 = 5_000;
const REPLAY_RESET_THRESHOLD_MS: u128 = 15_000;
const REPLAY_PREVIOUS_PROGRESS_FLOOR_MS: u128 = 30_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryPlaybackSource {
  NativeContext,
  NativeRawList,
  ExternalDevice,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryItemKind {
  Track,
  Episode,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ListenRecord {
  pub started_at: DateTime<Utc>,
  pub ended_at: DateTime<Utc>,
  pub listened_ms: u64,
  pub duration_ms: u32,
  pub qualified: bool,
  pub title: String,
  pub artists: Vec<String>,
  pub album: String,
  pub item_kind: HistoryItemKind,
  pub item_id: Option<String>,
  pub item_uri: Option<String>,
  pub context_uri: Option<String>,
  pub source: HistoryPlaybackSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionIdentity {
  title: String,
  artists: Vec<String>,
  album: String,
  item_id: Option<String>,
  item_uri: Option<String>,
  context_uri: Option<String>,
  source: HistoryPlaybackSource,
}

#[derive(Clone, Debug)]
struct ActiveListenSession {
  started_at: DateTime<Utc>,
  identity: SessionIdentity,
  title: String,
  artists: Vec<String>,
  album: String,
  item_kind: HistoryItemKind,
  item_id: Option<String>,
  item_uri: Option<String>,
  context_uri: Option<String>,
  source: HistoryPlaybackSource,
  duration_ms: u32,
  listened_ms: u64,
  last_progress_ms: u128,
  last_is_playing: bool,
}

#[derive(Default)]
struct HistoryCollector {
  current: Option<ActiveListenSession>,
  last_observed_at: Option<Instant>,
}

#[derive(Clone, Debug, Serialize)]
struct NowPlayingPayload {
  title: String,
  artists: Vec<String>,
  album: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  image_url: Option<String>,
  duration_ms: u32,
  progress_ms: u64,
  is_playing: bool,
  is_live: bool,
  /// Client Unix millis when `progress_ms` was sampled, so the backend can
  /// extrapolate the widget's progress bar between heartbeats.
  progress_at_ms: i64,
}

impl NowPlayingPayload {
  fn from_snapshot(snapshot: &PlaybackSnapshot, sampled_at_ms: i64) -> Self {
    Self {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      image_url: snapshot.metadata.image_url.clone(),
      duration_ms: snapshot.metadata.duration_ms,
      progress_ms: snapshot.progress_ms.min(u64::MAX as u128) as u64,
      is_playing: snapshot.is_playing,
      is_live: snapshot.is_live,
      progress_at_ms: sampled_at_ms,
    }
  }
}

#[derive(Debug)]
enum NowPlayingAction {
  Push(NowPlayingPayload),
  Clear,
}

/// State captured at the last push; `Some` exactly while the cloud holds
/// now-playing state for this session.
struct PushedState {
  title: String,
  artists: Vec<String>,
  is_playing: bool,
  progress_ms: u128,
  at: Instant,
}

/// Decides when the cloud now-playing state needs an update. Pushes on track
/// change, play/pause flip, seek discontinuity, and a heartbeat (also while
/// paused, so the public widget can show "paused" instead of decaying to
/// offline); clears once when track playback stops mid-session.
#[derive(Default)]
struct NowPlayingTracker {
  pushed: Option<PushedState>,
}

impl NowPlayingTracker {
  fn observe(
    &mut self,
    snapshot: Option<&PlaybackSnapshot>,
    now: Instant,
  ) -> Option<NowPlayingAction> {
    let snap = snapshot.filter(|s| s.item_kind == PlaybackItemKind::Track);
    let Some(snap) = snap else {
      return self.pushed.take().map(|_| NowPlayingAction::Clear);
    };

    let should_push = match &self.pushed {
      None => true,
      Some(pushed) => {
        let identity_changed =
          pushed.title != snap.metadata.title || pushed.artists != snap.metadata.artists;
        let play_state_changed = pushed.is_playing != snap.is_playing;
        let heartbeat_due = now.duration_since(pushed.at).as_secs() >= NOW_PLAYING_HEARTBEAT_SECS;
        let expected_progress = if pushed.is_playing {
          pushed.progress_ms + now.duration_since(pushed.at).as_millis()
        } else {
          pushed.progress_ms
        };
        let seek_detected =
          snap.progress_ms.abs_diff(expected_progress) > NOW_PLAYING_SEEK_THRESHOLD_MS;
        identity_changed || play_state_changed || heartbeat_due || seek_detected
      }
    };

    if should_push {
      self.pushed = Some(PushedState {
        title: snap.metadata.title.clone(),
        artists: snap.metadata.artists.clone(),
        is_playing: snap.is_playing,
        progress_ms: snap.progress_ms,
        at: now,
      });
      return Some(NowPlayingAction::Push(NowPlayingPayload::from_snapshot(
        snap,
        Utc::now().timestamp_millis(),
      )));
    }
    None
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecapPeriod {
  SevenDays,
  ThirtyDays,
  Month,
  Year,
  All,
}

impl RecapPeriod {
  pub const ALL_PERIODS: [RecapPeriod; 5] = [
    RecapPeriod::SevenDays,
    RecapPeriod::ThirtyDays,
    RecapPeriod::Month,
    RecapPeriod::Year,
    RecapPeriod::All,
  ];

  pub fn next(self) -> Self {
    self.cycle(1)
  }

  pub fn prev(self) -> Self {
    self.cycle(Self::ALL_PERIODS.len() - 1)
  }

  fn cycle(self, offset: usize) -> Self {
    let index = Self::ALL_PERIODS
      .iter()
      .position(|period| *period == self)
      .unwrap_or(0);
    Self::ALL_PERIODS[(index + offset) % Self::ALL_PERIODS.len()]
  }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreakSummary {
  pub current_days: u32,
  pub longest_days: u32,
  pub today_ms: u64,
}

/// Aggregated listening stats backing the in-app Stats screen.
#[derive(Clone, Debug)]
pub struct StatsData {
  pub total_plays: usize,
  pub total_time_ms: u64,
  pub top_tracks: Vec<RankedEntry>,
  pub top_artists: Vec<RankedEntry>,
  pub top_albums: Vec<RankedEntry>,
  pub days: Vec<RankedEntry>,
}

const STATS_LIST_LIMIT: usize = 20;

pub fn build_stats_data(filtered: &[ListenRecord]) -> StatsData {
  StatsData {
    total_plays: filtered.len(),
    total_time_ms: filtered.iter().map(|record| record.listened_ms).sum(),
    top_tracks: aggregate_top_tracks(filtered, STATS_LIST_LIMIT),
    top_artists: aggregate_top_artists(filtered, STATS_LIST_LIMIT),
    top_albums: aggregate_top_albums(filtered, STATS_LIST_LIMIT),
    days: aggregate_days(filtered),
  }
}

pub fn compute_streaks(listens: &[ListenRecord]) -> StreakSummary {
  streaks_from_day_totals(&qualified_day_totals(listens))
}

/// Listened milliseconds per local day, qualified records only. The history
/// collector keeps one of these updated incrementally so streaks don't require
/// re-reading the listens file after every track.
fn qualified_day_totals(listens: &[ListenRecord]) -> BTreeMap<NaiveDate, u64> {
  let mut totals: BTreeMap<NaiveDate, u64> = BTreeMap::new();
  for record in listens.iter().filter(|record| record.qualified) {
    let date = record.ended_at.with_timezone(&Local).date_naive();
    *totals.entry(date).or_default() += record.listened_ms;
  }
  totals
}

fn streaks_from_day_totals(day_totals: &BTreeMap<NaiveDate, u64>) -> StreakSummary {
  let today = Local::now().date_naive();
  let days: BTreeSet<NaiveDate> = day_totals.keys().copied().collect();
  streaks_from_days(
    &days,
    today,
    day_totals.get(&today).copied().unwrap_or_default(),
  )
}

fn streaks_from_days(days: &BTreeSet<NaiveDate>, today: NaiveDate, today_ms: u64) -> StreakSummary {
  let mut longest_days = 0u32;
  let mut run = 0u32;
  let mut prev: Option<NaiveDate> = None;
  let mut last_run = 0u32;
  for &day in days {
    run = match prev {
      Some(prev_day) if day == prev_day + Duration::days(1) => run + 1,
      _ => 1,
    };
    longest_days = longest_days.max(run);
    last_run = run;
    prev = Some(day);
  }

  // A streak stays alive until midnight: count the trailing run if it ends
  // today or yesterday.
  let current_days = match prev {
    Some(last_day) if last_day == today || last_day + Duration::days(1) == today => last_run,
    _ => 0,
  };

  StreakSummary {
    current_days,
    longest_days,
    today_ms,
  }
}

/// Spawn a cloud history sync unless one is already in flight. Overlapping
/// syncs would race on the last-synced timestamp and double-send records;
/// a skipped sync is harmless because every sync sends everything newer than
/// that timestamp, so the next trigger (next song or exit) catches up.
fn spawn_cloud_history_sync(
  client: &reqwest::Client,
  sync_token: &str,
  in_flight: &Arc<std::sync::atomic::AtomicBool>,
) {
  use std::sync::atomic::Ordering;

  if in_flight.swap(true, Ordering::AcqRel) {
    return;
  }
  let client = client.clone();
  let token = sync_token.to_string();
  let in_flight = Arc::clone(in_flight);
  tokio::spawn(async move {
    if let Err(e) = sync_history_to_cloud_with_client(&client, &token).await {
      log::warn!("failed to sync listening history to cloud: {}", e);
    }
    in_flight.store(false, Ordering::Release);
  });
}

pub fn spawn_history_collector(app: Arc<Mutex<App>>) {
  tokio::spawn(async move {
    let mut collector = HistoryCollector::default();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    let mut last_auto_check = Instant::now();
    let http_client = crate::infra::network::requests::shared_http_client().clone();

    // Check on startup
    perform_auto_recap_check(&app).await;

    // Streak cache: load the full history once, then keep per-day totals
    // updated incrementally from the records this collector appends (avoids
    // re-reading the ever-growing listens file after every track).
    let mut day_totals: Option<BTreeMap<NaiveDate, u64>> =
      match tokio::task::spawn_blocking(load_listens).await {
        Ok(Ok(listens)) => Some(qualified_day_totals(&listens)),
        Ok(Err(error)) => {
          log::warn!("failed to load listens for streak cache: {}", error);
          None
        }
        Err(error) => {
          log::warn!("streak cache task failed: {}", error);
          None
        }
      };
    if let Some(totals) = &day_totals {
      app.lock().await.listening_streaks = Some(streaks_from_day_totals(totals));
    }

    // Sync history to cloud on startup
    let sync_token_opt: Option<String> = if let Ok(app_guard) = app.try_lock() {
      app_guard.user_config.behavior.sync_token.clone()
    } else {
      None
    };

    let history_sync_in_flight = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Some(ref token) = sync_token_opt {
      spawn_cloud_history_sync(&http_client, token, &history_sync_in_flight);
    }

    // Now-playing tracking state
    let mut now_playing_tracker = NowPlayingTracker::default();

    loop {
      interval.tick().await;

      if last_auto_check.elapsed().as_secs() >= 3600 {
        last_auto_check = Instant::now();
        perform_auto_recap_check(&app).await;
      }

      let snapshot = if let Ok(app) = app.try_lock() {
        current_playback_snapshot(&app)
      } else {
        continue;
      };

      // Now-playing sync: push on track change, play/pause flip, seek, or
      // heartbeat; clear once when track playback stops.
      if let Some(ref token) = sync_token_opt {
        let action = now_playing_tracker.observe(snapshot.as_ref(), Instant::now());
        match action {
          Some(NowPlayingAction::Push(payload)) => {
            let token_clone = token.clone();
            let client = http_client.clone();
            tokio::spawn(async move {
              if let Err(e) = sync_now_playing_to_cloud(&client, &token_clone, &payload).await {
                log::warn!("failed to sync now-playing: {}", e);
              }
            });
          }
          Some(NowPlayingAction::Clear) => {
            let token_clone = token.clone();
            let client = http_client.clone();
            tokio::spawn(async move {
              if let Err(e) = clear_now_playing_with_client(&client, &token_clone).await {
                log::warn!("failed to clear now-playing: {}", e);
              }
            });
          }
          None => {}
        }
      }

      match collector.observe(snapshot) {
        Ok(Some(record)) => {
          if record.qualified {
            if let Some(totals) = &mut day_totals {
              let date = record.ended_at.with_timezone(&Local).date_naive();
              *totals.entry(date).or_default() += record.listened_ms;
              app.lock().await.listening_streaks = Some(streaks_from_day_totals(totals));
            }
          }
          // Push the finished listen to the cloud right away instead of only
          // at startup/exit; long sessions otherwise pile up records that
          // stall the exit sync.
          if let Some(ref token) = sync_token_opt {
            spawn_cloud_history_sync(&http_client, token, &history_sync_in_flight);
          }
        }
        Ok(None) => {}
        Err(error) => {
          log::warn!("listening history collector failed: {}", error);
        }
      }
    }
  });
}

/// Default output path for the shareable recap page, used by the automatic
/// monthly check and the in-app `generate_recap` key.
pub fn recap_output_path() -> Result<PathBuf> {
  let home = dirs::home_dir().ok_or_else(|| anyhow!("No $HOME directory found for history"))?;
  Ok(
    home
      .join(".config")
      .join("spotatui")
      .join("spotatui-recap.html"),
  )
}

fn last_recap_file_path() -> Result<PathBuf> {
  let home = dirs::home_dir().ok_or_else(|| anyhow!("No $HOME directory found for history"))?;
  Ok(
    home
      .join(".config")
      .join("spotatui")
      .join(HISTORY_SUBDIR)
      .join("last_recap_at.txt"),
  )
}

fn auto_recap_is_due(last_recap_contents: Option<&str>, now_ts: i64) -> bool {
  match last_recap_contents {
    Some(content) => match content.trim().parse::<i64>() {
      Ok(ts) => now_ts - ts >= 30 * 24 * 3600,
      Err(_) => true,
    },
    None => true,
  }
}

async fn perform_auto_recap_check(app: &Arc<Mutex<App>>) {
  if !app
    .lock()
    .await
    .user_config
    .behavior
    .enable_monthly_recap_prompt
  {
    return;
  }

  let path = match last_recap_file_path() {
    Ok(p) => p,
    Err(e) => {
      log::warn!("failed to get last recap file path: {}", e);
      return;
    }
  };

  let now = Utc::now();
  if !auto_recap_is_due(fs::read_to_string(&path).ok().as_deref(), now.timestamp()) {
    return;
  }

  let output_path = match recap_output_path() {
    Ok(p) => p,
    Err(_) => return,
  };

  // The export reads and renders the whole history file; keep it off the
  // runtime worker threads.
  let export_path = output_path.clone();
  let result = tokio::task::spawn_blocking(move || {
    export_history_recap(RecapPeriod::ThirtyDays, &export_path)
  })
  .await
  .map_err(anyhow::Error::from)
  .and_then(|result| result);

  match result {
    Ok(count) => {
      if count > 0 {
        if let Err(e) = fs::write(&path, now.timestamp().to_string()) {
          log::warn!("failed to write last recap timestamp: {}", e);
        }

        let mut app_guard = app.lock().await;
        if app_guard.get_current_route().active_block == ActiveBlock::Input {
          // Never steal keystrokes from an input field; fall back to a status
          // message instead of the popup.
          app_guard.set_status_message(
            format!(
              "Monthly listening recap ready at {} ({} listens)",
              output_path.display(),
              count
            ),
            10,
          );
        } else {
          app_guard.recap_prompt = Some(RecapPromptState {
            path: output_path,
            listens: count,
          });
          app_guard.push_navigation_stack(RouteId::RecapPrompt, ActiveBlock::RecapPrompt);
        }
      }
    }
    Err(e) => {
      log::warn!("failed to automatically generate recap: {}", e);
    }
  }
}

pub fn export_history_recap(period: RecapPeriod, output_path: &Path) -> Result<usize> {
  let listens = load_listens()?;
  let filtered = filter_listens_for_period(&listens, period);
  // The share card is toggleable between the requested period and All Time
  // (or Last 30 Days when All Time itself was requested).
  let alt_period = if period == RecapPeriod::All {
    RecapPeriod::ThirtyDays
  } else {
    RecapPeriod::All
  };
  let alt_filtered = filter_listens_for_period(&listens, alt_period);
  let html = render_history_recap_html(period, &filtered, alt_period, &alt_filtered);

  if let Some(parent) = output_path.parent() {
    fs::create_dir_all(parent)?;
  }
  fs::write(output_path, html)?;

  Ok(filtered.len())
}

pub fn parse_recap_period(value: &str) -> Result<RecapPeriod> {
  match value {
    "7d" => Ok(RecapPeriod::SevenDays),
    "30d" => Ok(RecapPeriod::ThirtyDays),
    "month" => Ok(RecapPeriod::Month),
    "year" => Ok(RecapPeriod::Year),
    "all" => Ok(RecapPeriod::All),
    _ => Err(anyhow!("unsupported recap period '{}'", value)),
  }
}

pub fn load_listens() -> Result<Vec<ListenRecord>> {
  let path = listens_file_path()?;
  if !path.exists() {
    return Ok(Vec::new());
  }

  let file = fs::File::open(path)?;
  let reader = BufReader::new(file);
  let mut listens = Vec::new();
  for line in reader.lines() {
    let line = line?;
    if line.trim().is_empty() {
      continue;
    }

    match serde_json::from_str::<ListenRecord>(&line) {
      Ok(record) => listens.push(record),
      Err(error) => {
        log::warn!("skipping malformed history line: {}", error);
      }
    }
  }

  Ok(listens)
}

impl HistoryCollector {
  /// Returns the listen record appended to the history file, if any.
  fn observe(&mut self, snapshot: Option<PlaybackSnapshot>) -> Result<Option<ListenRecord>> {
    let mut appended = None;
    let now_utc = Utc::now();
    let now_instant = Instant::now();

    if let Some(current) = &mut self.current {
      if let Some(last_observed_at) = self.last_observed_at {
        let elapsed_ms = now_instant
          .saturating_duration_since(last_observed_at)
          .as_millis()
          .min(MAX_INTERVAL_MS as u128) as u64;
        if current.last_is_playing {
          current.listened_ms = current.listened_ms.saturating_add(elapsed_ms);
        }
      }
    }
    self.last_observed_at = Some(now_instant);

    let snapshot = snapshot.filter(|snapshot| snapshot.item_kind == PlaybackItemKind::Track);
    match snapshot {
      Some(snapshot) => {
        let identity = SessionIdentity::from_snapshot(&snapshot);
        let should_roll = self.current.as_ref().is_some_and(|current| {
          current.identity != identity
            || (current.last_progress_ms > REPLAY_PREVIOUS_PROGRESS_FLOOR_MS
              && snapshot.progress_ms + REPLAY_RESET_THRESHOLD_MS < current.last_progress_ms)
        });

        if should_roll {
          appended = self.finalize_current(now_utc)?;
        }

        if self.current.is_none() {
          self.current = Some(ActiveListenSession::from_snapshot(snapshot, now_utc));
          return Ok(appended);
        }

        if let Some(current) = &mut self.current {
          current.duration_ms = snapshot.metadata.duration_ms;
          current.last_progress_ms = snapshot.progress_ms;
          current.last_is_playing = snapshot.is_playing;
        }
      }
      None => {
        appended = self.finalize_current(now_utc)?;
      }
    }

    Ok(appended)
  }

  fn finalize_current(&mut self, ended_at: DateTime<Utc>) -> Result<Option<ListenRecord>> {
    let Some(current) = self.current.take() else {
      return Ok(None);
    };

    if current.listened_ms == 0 {
      return Ok(None);
    }

    let record = ListenRecord::from_active_session(current, ended_at);
    append_listen_record(&record)?;
    Ok(Some(record))
  }
}

impl SessionIdentity {
  fn from_snapshot(snapshot: &PlaybackSnapshot) -> Self {
    Self {
      title: snapshot.metadata.title.clone(),
      artists: snapshot.metadata.artists.clone(),
      album: snapshot.metadata.album.clone(),
      item_id: snapshot.item_id.clone(),
      item_uri: snapshot.item_uri.clone(),
      context_uri: snapshot.context_uri.clone(),
      source: history_source_from_snapshot(snapshot),
    }
  }
}

impl ActiveListenSession {
  fn from_snapshot(snapshot: PlaybackSnapshot, started_at: DateTime<Utc>) -> Self {
    let source = history_source_from_snapshot(&snapshot);
    let identity = SessionIdentity::from_snapshot(&snapshot);
    let PlaybackSnapshot {
      metadata,
      item_id,
      item_uri,
      context_uri,
      source: _,
      progress_ms,
      is_playing,
      ..
    } = snapshot;
    let title = metadata.title;
    let artists = metadata.artists;
    let album = metadata.album;
    let duration_ms = metadata.duration_ms;
    Self {
      started_at,
      identity,
      title,
      artists,
      album,
      item_kind: HistoryItemKind::Track,
      item_id,
      item_uri,
      context_uri,
      source,
      duration_ms,
      listened_ms: 0,
      last_progress_ms: progress_ms,
      last_is_playing: is_playing,
    }
  }
}

impl ListenRecord {
  fn from_active_session(session: ActiveListenSession, ended_at: DateTime<Utc>) -> Self {
    let qualified = qualifies_listen(session.duration_ms, session.listened_ms);
    Self {
      started_at: session.started_at,
      ended_at,
      listened_ms: session.listened_ms,
      duration_ms: session.duration_ms,
      qualified,
      title: session.title,
      artists: session.artists,
      album: session.album,
      item_kind: session.item_kind,
      item_id: session.item_id,
      item_uri: session.item_uri,
      context_uri: session.context_uri,
      source: session.source,
    }
  }
}

fn listens_file_path() -> Result<PathBuf> {
  let home = dirs::home_dir().ok_or_else(|| anyhow!("No $HOME directory found for history"))?;
  Ok(
    home
      .join(".config")
      .join("spotatui")
      .join(HISTORY_SUBDIR)
      .join(LISTENS_FILE_NAME),
  )
}

fn append_listen_record(record: &ListenRecord) -> Result<()> {
  let path = listens_file_path()?;
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent)?;
  }

  let mut file = OpenOptions::new().create(true).append(true).open(path)?;
  serde_json::to_writer(&mut file, record)?;
  writeln!(file)?;
  Ok(())
}

fn qualifies_listen(duration_ms: u32, listened_ms: u64) -> bool {
  if duration_ms <= 30_000 {
    return false;
  }

  let threshold_ms = u64::from(duration_ms / 2).min(240_000);
  listened_ms >= threshold_ms
}

fn history_source_from_snapshot(snapshot: &PlaybackSnapshot) -> HistoryPlaybackSource {
  match snapshot.source {
    PlaybackSource::NativeContext => HistoryPlaybackSource::NativeContext,
    PlaybackSource::NativeRawList => HistoryPlaybackSource::NativeRawList,
    PlaybackSource::ExternalDevice => HistoryPlaybackSource::ExternalDevice,
  }
}

pub fn filter_listens_for_period(
  listens: &[ListenRecord],
  period: RecapPeriod,
) -> Vec<ListenRecord> {
  let now = Utc::now();
  listens
    .iter()
    .filter(|record| record.qualified)
    .filter(|record| match period {
      RecapPeriod::SevenDays => record.ended_at >= now - Duration::days(7),
      RecapPeriod::ThirtyDays => record.ended_at >= now - Duration::days(30),
      RecapPeriod::Month => {
        record.ended_at.year() == now.year() && record.ended_at.month() == now.month()
      }
      RecapPeriod::Year => record.ended_at.year() == now.year(),
      RecapPeriod::All => true,
    })
    .cloned()
    .collect()
}

/// The raw (unescaped) values the share card shows for one period.
struct CardData {
  period: RecapPeriod,
  track_title: String,
  track_artist: String,
  top_artist: String,
  top_album: String,
  plays: usize,
  time: String,
}

impl CardData {
  fn from_listens(period: RecapPeriod, listens: &[ListenRecord]) -> Self {
    Self::from_tops(
      period,
      aggregate_top_tracks(listens, 1).first(),
      aggregate_top_artists(listens, 1).first(),
      aggregate_top_albums(listens, 1).first(),
      listens.len(),
      listens.iter().map(|record| record.listened_ms).sum(),
    )
  }

  /// Build from already-aggregated ranked lists; the first entry of each wins.
  fn from_tops(
    period: RecapPeriod,
    top_track: Option<&RankedEntry>,
    top_artist: Option<&RankedEntry>,
    top_album: Option<&RankedEntry>,
    plays: usize,
    total_ms: u64,
  ) -> Self {
    let top_track_raw = top_track
      .map(|entry| entry.display.as_str())
      .unwrap_or("No data");
    let (track_title, track_artist) = if top_track_raw == "No data" {
      ("No data".to_string(), "No data".to_string())
    } else {
      (
        top_track_raw
          .split(" - ")
          .next()
          .unwrap_or(top_track_raw)
          .to_string(),
        top_track_raw.split(" - ").nth(1).unwrap_or("").to_string(),
      )
    };

    Self {
      period,
      track_title,
      track_artist,
      top_artist: top_artist
        .map(|entry| entry.display.as_str())
        .unwrap_or("No data")
        .to_string(),
      top_album: top_album
        .map(|entry| entry.display.as_str())
        .unwrap_or("No data")
        .to_string(),
      plays,
      time: format_duration(total_ms),
    }
  }

  /// Render as a JS object literal for the recap page's inline script.
  fn to_js_object(&self) -> String {
    format!(
      "{{ period: {}, track: {}, artist: {}, topArtist: {}, topAlbum: {}, plays: {}, time: {} }}",
      js_string(period_label(self.period)),
      js_string(&self.track_title),
      js_string(&self.track_artist),
      js_string(&self.top_artist),
      js_string(&self.top_album),
      self.plays,
      js_string(&self.time),
    )
  }
}

fn render_history_recap_html(
  period: RecapPeriod,
  listens: &[ListenRecord],
  alt_period: RecapPeriod,
  alt_listens: &[ListenRecord],
) -> String {
  let top_tracks = aggregate_top_tracks(listens, 5);
  let top_artists = aggregate_top_artists(listens, 5);
  let top_albums = aggregate_top_albums(listens, 5);
  let listening_days = aggregate_days(listens);
  let listening_hours = aggregate_hours(listens);

  // The primary card's No. 1 entries fall out of the limit-5 lists above; only
  // the alternative period needs its own aggregation.
  let card = CardData::from_tops(
    period,
    top_tracks.first(),
    top_artists.first(),
    top_albums.first(),
    listens.len(),
    listens.iter().map(|record| record.listened_ms).sum(),
  );
  let alt_card = CardData::from_listens(alt_period, alt_listens);

  format!(
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>spotatui recap</title>
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@300;400;500;600;700;800&family=JetBrains+Mono:wght@400;700&display=swap" rel="stylesheet">
  <style>
    :root {{
      --bg: #05080c;
      --accent: #1db954;
      --accent-glow: rgba(29, 185, 84, 0.15);
      --card-gradient: linear-gradient(135deg, #0b1a11 0%, #050a0f 100%);
      --text: #f1f5f9;
      --text-muted: #94a3b8;
      --border: rgba(255, 255, 255, 0.05);
      --font-sans: "Plus Jakarta Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      --font-mono: "JetBrains Mono", monospace;
    }}
    * {{
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }}
    body {{
      min-height: 100vh;
      background: radial-gradient(circle at 50% 0%, #0e2417 0%, #05080c 70%);
      color: var(--text);
      font-family: var(--font-sans);
      padding: 40px 20px 60px;
    }}
    .app-container {{
      max-width: 1200px;
      margin: 0 auto;
      width: 100%;
    }}
    .app-header {{
      text-align: center;
      margin-bottom: 40px;
    }}
    .app-header h1 {{
      font-size: 2.2rem;
      font-weight: 800;
      letter-spacing: -0.03em;
      font-family: var(--font-sans);
      margin-bottom: 6px;
    }}
    .brand-logo {{
      color: var(--text);
    }}
    .brand-accent {{
      color: var(--accent);
      text-shadow: 0 0 10px rgba(29, 185, 84, 0.4);
    }}
    .app-header p {{
      color: var(--text-muted);
      font-size: 0.9rem;
      letter-spacing: 0.12em;
      text-transform: uppercase;
      font-weight: 600;
    }}
    .app-desc {{
      color: var(--text-muted);
      font-size: 0.95rem;
      max-width: 580px;
      margin: 12px auto 0;
      line-height: 1.5;
      text-transform: none !important;
      letter-spacing: normal !important;
      font-weight: 400 !important;
    }}
    .dashboard-grid {{
      display: grid;
      grid-template-columns: 440px 1fr;
      gap: 40px;
      align-items: start;
    }}
    @media (max-width: 1024px) {{
      .dashboard-grid {{
        grid-template-columns: 1fr;
        gap: 32px;
      }}
      .share-column {{
        display: flex;
        flex-direction: column;
        align-items: center;
      }}
    }}
    .share-column {{
      position: sticky;
      top: 40px;
    }}
    .card-wrapper {{
      width: 440px;
      height: 660px;
      display: flex;
      justify-content: center;
      align-items: center;
      border-radius: 28px;
    }}
    @media (max-width: 480px) {{
      .card-wrapper {{
        transform: scale(0.8);
        height: 528px;
        margin-bottom: -40px;
      }}
    }}
    @media (max-width: 380px) {{
      .card-wrapper {{
        transform: scale(0.68);
        height: 448px;
        margin-bottom: -80px;
      }}
    }}
    .share-card {{
      width: 440px;
      height: 660px;
      background: var(--card-gradient);
      border: 1px solid rgba(29, 185, 84, 0.22);
      border-radius: 28px;
      padding: 32px;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
      position: relative;
      overflow: hidden;
      box-shadow: 0 25px 50px -12px rgba(0,0,0,0.8), 0 0 40px rgba(29, 185, 84, 0.12);
      box-sizing: border-box;
    }}
    .card-glow {{
      position: absolute;
      top: -150px;
      left: 50%;
      transform: translateX(-50%);
      width: 300px;
      height: 300px;
      background: radial-gradient(circle, rgba(29, 185, 84, 0.18) 0%, transparent 70%);
      pointer-events: none;
      z-index: 0;
    }}
    .card-header {{
      position: relative;
      height: 32px;
      z-index: 1;
      width: 100%;
      margin-bottom: 10px;
    }}
    .card-brand {{
      position: absolute;
      left: 0;
      top: 0;
      font-family: var(--font-sans);
      font-weight: 800;
      font-size: 1.25rem;
      letter-spacing: -0.02em;
    }}
    .period-badge {{
      position: absolute;
      right: 0;
      top: 0;
      font-size: 0.72rem;
      font-weight: 700;
      color: var(--accent);
      background: rgba(29, 185, 84, 0.12);
      border: 1px solid rgba(29, 185, 84, 0.3);
      padding: 5px 12px;
      border-radius: 99px;
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }}
    .cover-container {{
      position: relative;
      width: 174px;
      height: 174px;
      margin: 0 auto;
      border-radius: 20px;
      overflow: hidden;
      border: 1px solid rgba(255, 255, 255, 0.08);
      box-shadow: 0 15px 35px rgba(0, 0, 0, 0.5), 0 0 30px rgba(29, 185, 84, 0.15);
      z-index: 1;
    }}
    .cover-placeholder {{
      position: absolute;
      top: 0; left: 0; right: 0; bottom: 0;
      background: linear-gradient(135deg, #1b2820 0%, #05080c 100%);
      display: flex;
      justify-content: center;
      align-items: center;
      z-index: 1;
    }}
    .cover-img {{
      position: absolute;
      top: 0; left: 0; right: 0; bottom: 0;
      width: 100%;
      height: 100%;
      object-fit: cover;
      z-index: 2;
      display: none;
    }}
    .top-track-card {{
      background: rgba(255, 255, 255, 0.02);
      border: 1px solid rgba(255, 255, 255, 0.04);
      border-radius: 18px;
      padding: 16px 20px;
      text-align: center;
      backdrop-filter: blur(10px);
      z-index: 1;
      width: 100%;
    }}
    .rank-badge {{
      font-family: var(--font-mono);
      font-size: 0.68rem;
      color: var(--accent);
      background: rgba(29, 185, 84, 0.1);
      padding: 3px 8px;
      border-radius: 6px;
      letter-spacing: 0.08em;
      font-weight: 700;
      display: inline-block;
      margin-bottom: 8px;
      border: 1px solid rgba(29, 185, 84, 0.18);
    }}
    .track-title {{
      font-size: 1.15rem;
      font-weight: 800;
      color: #ffffff;
      margin-bottom: 4px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }}
    .track-artist {{
      font-size: 0.88rem;
      color: var(--text-muted);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }}
    .card-stats-grid {{
      display: flex;
      flex-direction: column;
      gap: 12px;
      z-index: 1;
      width: 100%;
    }}
    .stats-row {{
      display: flex;
      gap: 12px;
      width: 100%;
    }}
    .stat-pill {{
      background: rgba(255, 255, 255, 0.02);
      border: 1px solid rgba(255, 255, 255, 0.03);
      border-radius: 14px;
      padding: 10px 14px;
      display: flex;
      flex-direction: column;
      flex: 1;
      min-width: 0;
      box-sizing: border-box;
    }}
    .stat-pill.full-width {{
      flex: none;
      width: 100%;
    }}
    .stat-label {{
      font-size: 0.7rem;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.04em;
      margin-bottom: 3px;
      font-weight: 500;
    }}
    .stat-value {{
      font-size: 0.92rem;
      font-weight: 700;
      color: #ffffff;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }}
    .card-footer {{
      border-top: 1px solid rgba(255, 255, 255, 0.04);
      padding-top: 14px;
      position: relative;
      height: 30px;
      z-index: 1;
      width: 100%;
    }}
    .card-footer-left {{
      position: absolute;
      left: 0;
      top: 14px;
      font-size: 0.72rem;
      color: var(--text-muted);
    }}
    .card-footer-right {{
      position: absolute;
      right: 0;
      top: 14px;
      font-size: 0.72rem;
      font-family: var(--font-mono);
      color: var(--accent);
    }}
    .card-toggle {{
      display: flex;
      gap: 8px;
      width: 100%;
      max-width: 440px;
      margin-bottom: 16px;
    }}
    .toggle-btn {{
      flex: 1;
      background: rgba(255, 255, 255, 0.03);
      border: 1px solid rgba(255, 255, 255, 0.08);
      border-radius: 99px;
      color: var(--text-muted);
      font-size: 0.8rem;
      font-weight: 700;
      font-family: var(--font-sans);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      padding: 9px 16px;
      cursor: pointer;
      transition: all 0.2s ease;
    }}
    .toggle-btn:hover {{
      color: var(--text);
      border-color: rgba(29, 185, 84, 0.3);
    }}
    .toggle-btn.active {{
      background: rgba(29, 185, 84, 0.12);
      border-color: rgba(29, 185, 84, 0.4);
      color: var(--accent);
    }}
    .download-container {{
      margin-top: 20px;
      width: 100%;
      max-width: 440px;
      display: flex;
      justify-content: center;
    }}
    .download-btn {{
      background: linear-gradient(90deg, var(--accent) 0%, #10b981 100%);
      border: none;
      border-radius: 14px;
      color: #ffffff;
      font-size: 0.95rem;
      font-weight: 700;
      padding: 14px 28px;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 10px;
      box-shadow: 0 10px 20px rgba(29, 185, 84, 0.25);
      transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
      width: 100%;
      justify-content: center;
      font-family: var(--font-sans);
    }}
    .download-btn:hover {{
      transform: translateY(-2px);
      box-shadow: 0 15px 30px rgba(29, 185, 84, 0.35);
    }}
    .download-btn:active {{
      transform: translateY(0);
    }}
    .animate-spin {{
      animation: spin 1s linear infinite;
    }}
    .details-column {{
      min-width: 0;
    }}
    .details-section-title {{
      font-size: 1.25rem;
      font-weight: 800;
      margin-bottom: 20px;
      letter-spacing: 0.05em;
      text-transform: uppercase;
      color: #38bdf8;
      border-left: 3px solid var(--accent);
      padding-left: 12px;
      line-height: 1;
    }}
    .grid {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
      gap: 20px;
      margin-bottom: 24px;
    }}
    .panel {{
      background: rgba(10, 18, 30, 0.5);
      border: 1px solid var(--border);
      border-radius: 20px;
      padding: 24px;
      backdrop-filter: blur(20px);
    }}
    .panel h2 {{
      margin: 0 0 16px 0;
      font-size: 1.1rem;
      font-weight: 700;
      color: #f8fafc;
    }}
    .ranked-list {{
      list-style: none;
      padding: 0;
      margin: 0;
    }}
    .ranked-list li {{
      display: flex;
      align-items: center;
      gap: 14px;
      padding: 10px 14px;
      border-radius: 12px;
      background: rgba(255, 255, 255, 0.01);
      border: 1px solid rgba(255, 255, 255, 0.02);
      margin-bottom: 8px;
      transition: all 0.2s ease;
    }}
    .ranked-list li:hover {{
      background: rgba(29, 185, 84, 0.04);
      border-color: rgba(29, 185, 84, 0.15);
      transform: translateX(4px);
    }}
    .rank {{
      font-family: var(--font-mono);
      font-size: 0.9rem;
      font-weight: 800;
      color: var(--accent);
      width: 20px;
    }}
    .recent-icon {{
      color: var(--accent);
      display: flex;
      align-items: center;
      justify-content: center;
      width: 20px;
    }}
    .entry-details {{
      flex: 1;
      min-width: 0;
    }}
    .entry-details strong {{
      display: block;
      font-size: 0.92rem;
      font-weight: 600;
      color: #f1f5f9;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }}
    .entry-artist {{
      font-size: 0.82rem;
      color: var(--accent);
      margin-top: 2px;
      font-weight: 500;
    }}
    .subtle {{
      font-size: 0.8rem;
      color: var(--text-muted);
      margin-top: 2px;
    }}
    .bars {{
      display: grid;
      gap: 12px;
    }}
    .bar-row {{
      display: grid;
      gap: 6px;
    }}
    .bar-label {{
      display: flex;
      justify-content: space-between;
      font-family: var(--font-mono);
      font-size: 0.78rem;
      color: var(--text-muted);
    }}
    .bar {{
      height: 8px;
      border-radius: 99px;
      background: rgba(255, 255, 255, 0.03);
      overflow: hidden;
      position: relative;
    }}
    .bar > span {{
      display: block;
      height: 100%;
      border-radius: 99px;
      background: linear-gradient(90deg, var(--accent) 0%, #10b981 100%);
      box-shadow: 0 0 10px rgba(29, 185, 84, 0.3);
    }}
    footer {{
      margin-top: 48px;
      text-align: center;
      font-size: 0.8rem;
      color: #64748b;
      line-height: 1.6;
      max-width: 600px;
      margin-left: auto;
      margin-right: auto;
      border-top: 1px solid var(--border);
      padding-top: 24px;
    }}
  </style>
</head>
<body>
  <div class="app-container">
    <header class="app-header">
      <h1><span class="brand-logo">spota</span><span class="brand-accent">tui</span></h1>
      <p>Listening History Recap</p>
      <p class="app-desc">{summary}</p>
    </header>

    <div class="dashboard-grid">
      <!-- Left Column: The Share Card Area -->
      <div class="share-column">
        <div class="card-toggle">
          <button id="card-toggle-0" class="toggle-btn active" onclick="setActiveCard(0)">{title}</button>
          <button id="card-toggle-1" class="toggle-btn" onclick="setActiveCard(1)">{alt_title}</button>
        </div>
        <div class="card-wrapper">
          <div id="share-card" class="share-card">
            <!-- Ambient glowing node inside the card -->
            <div class="card-glow"></div>

            <div class="card-header">
              <span class="card-brand">spota<span style="color:var(--accent)">tui</span></span>
              <span id="card-period" class="period-badge">{title}</span>
            </div>
            
            <div class="cover-container">
              <div id="cover-art-placeholder" class="cover-placeholder">
                <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.2)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"></path><circle cx="6" cy="18" r="3"></circle><circle cx="18" cy="16" r="3"></circle></svg>
              </div>
              <img id="cover-art-img" class="cover-img" alt="Cover Art" crossorigin="anonymous">
            </div>
            
            <div class="top-track-card">
              <span class="rank-badge">NO. 1 TRACK</span>
              <div id="card-track-title" class="track-title">{top_track_title}</div>
              <div id="card-track-artist" class="track-artist">{top_track_artist}</div>
            </div>

            <div class="card-stats-grid">
              <div class="stats-row">
                <div class="stat-pill">
                  <span class="stat-label">Qualified Plays</span>
                  <span id="card-plays" class="stat-value">{total_plays}</span>
                </div>
                <div class="stat-pill">
                  <span class="stat-label">Listening Time</span>
                  <span id="card-time" class="stat-value">{total_time}</span>
                </div>
              </div>
              <div class="stat-pill full-width">
                <span class="stat-label">Top Artist</span>
                <span id="card-top-artist" class="stat-value">{top_artist_name}</span>
              </div>
              <div class="stat-pill full-width">
                <span class="stat-label">Top Album</span>
                <span id="card-top-album" class="stat-value">{top_album_title}</span>
              </div>
            </div>
            
            <div class="card-footer">
              <span class="card-footer-left">generated by <strong>spotatui</strong></span>
              <span class="card-footer-right">github.com/LargeModGames/spotatui</span>
            </div>
          </div>
        </div>
        
        <div class="download-container">
          <button class="download-btn" onclick="downloadCard()">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
            Download Share Card
          </button>
        </div>
      </div>
      
      <!-- Right Column: Detailed Insights Dashboard -->
      <div class="details-column">
        <div class="details-section-title">Detailed Analytics</div>
        
        <div class="grid">
          <article class="panel">
            <h2>Top Tracks</h2>
            {top_tracks_html}
          </article>
          <article class="panel">
            <h2>Top Artists</h2>
            {top_artists_html}
          </article>
          <article class="panel">
            <h2>Top Albums</h2>
            {top_albums_html}
          </article>
          <article class="panel">
            <h2>Recent Plays</h2>
            {recent_html}
          </article>
        </div>
        
        <div class="grid">
          <article class="panel">
            <h2>Listening by Day</h2>
            {days_html}
          </article>
          <article class="panel">
            <h2>Listening by Hour</h2>
            {hours_html}
          </article>
        </div>
      </div>
    </div>

    <footer>
      History is generated from spotatui local listening records and starts when the feature is enabled. Short or skipped plays are stored but excluded from headline recap totals.
    </footer>
  </div>

  <script>
    // Raw (JS-escaped, not HTML-escaped) card data emitted by spotatui: the
    // requested period first, the toggle alternative second.
    const CARDS = [{card_js}, {alt_card_js}];
    let activeCard = 0;
    // Cover-art URL per card: undefined = not fetched yet, null = none found.
    const coverUrls = [undefined, undefined];

    function showCoverPlaceholder() {{
      document.getElementById('cover-art-img').style.display = 'none';
      document.getElementById('cover-art-placeholder').style.display = 'flex';
    }}

    function applyCover(index) {{
      const img = document.getElementById('cover-art-img');
      const url = coverUrls[index];
      if (!url) {{
        showCoverPlaceholder();
        return;
      }}
      showCoverPlaceholder();
      img.onload = () => {{
        // Only reveal if this card is still the active one.
        if (activeCard === index) {{
          img.style.display = 'block';
          document.getElementById('cover-art-placeholder').style.display = 'none';
        }}
      }};
      if (img.src === url && img.complete && img.naturalWidth > 0) {{
        img.onload();
      }} else {{
        img.src = url;
      }}
    }}

    function loadCoverFor(index) {{
      if (coverUrls[index] !== undefined) {{
        applyCover(index);
        return;
      }}
      const card = CARDS[index];
      if (!card.track || card.track === 'No data') {{
        coverUrls[index] = null;
        applyCover(index);
        return;
      }}
      // Clean parentheses or bracket noise for high search precision
      const cleanTitle = card.track.replace(/\([^)]*\)/g, '').replace(/\[[^\]]*\]/g, '').trim();
      const query = encodeURIComponent(`${{cleanTitle}} ${{card.artist}}`);
      const url = `https://itunes.apple.com/search?term=${{query}}&limit=1&entity=song`;

      fetch(url)
        .then(res => res.json())
        .then(data => {{
          coverUrls[index] = (data.results && data.results.length > 0)
            ? data.results[0].artworkUrl100.replace('100x100bb', '600x600bb')
            : null;
          if (activeCard === index) applyCover(index);
        }})
        .catch(err => {{
          console.error('Error fetching cover art:', err);
          coverUrls[index] = null;
          if (activeCard === index) applyCover(index);
        }});
    }}

    function setActiveCard(index) {{
      activeCard = index;
      const card = CARDS[index];
      document.getElementById('card-toggle-0').classList.toggle('active', index === 0);
      document.getElementById('card-toggle-1').classList.toggle('active', index === 1);
      document.getElementById('card-period').textContent = card.period;
      document.getElementById('card-track-title').textContent = card.track;
      document.getElementById('card-track-artist').textContent = card.artist;
      document.getElementById('card-plays').textContent = String(card.plays);
      document.getElementById('card-time').textContent = card.time;
      document.getElementById('card-top-artist').textContent = card.topArtist;
      document.getElementById('card-top-album').textContent = card.topAlbum;
      loadCoverFor(index);
    }}

    document.addEventListener('DOMContentLoaded', () => {{
      loadCoverFor(0);
    }});

    // ── Share-card renderer ────────────────────────────────────────────────
    // Draws the card directly with the canvas 2D API: no external libraries,
    // deterministic output, works offline.
    const CANVAS_SCALE = 3;
    const CARD_W = 440;
    const CARD_H = 660;
    const ACCENT = '#1db954';
    const TEXT = '#f1f5f9';
    const MUTED = '#94a3b8';

    function roundRectPath(ctx, x, y, w, h, r) {{
      r = Math.min(r, w / 2, h / 2);
      ctx.beginPath();
      ctx.moveTo(x + r, y);
      ctx.arcTo(x + w, y, x + w, y + h, r);
      ctx.arcTo(x + w, y + h, x, y + h, r);
      ctx.arcTo(x, y + h, x, y, r);
      ctx.arcTo(x, y, x + w, y, r);
      ctx.closePath();
    }}

    function fitText(ctx, text, maxWidth) {{
      if (ctx.measureText(text).width <= maxWidth) return text;
      let t = text;
      while (t.length > 1 && ctx.measureText(t + '…').width > maxWidth) {{
        t = t.slice(0, -1);
      }}
      return t + '…';
    }}

    function drawStatPill(ctx, x, y, w, h, label, value) {{
      ctx.fillStyle = 'rgba(255, 255, 255, 0.02)';
      roundRectPath(ctx, x, y, w, h, 14);
      ctx.fill();
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.textAlign = 'left';
      ctx.fillStyle = MUTED;
      ctx.font = '500 11px "Plus Jakarta Sans", sans-serif';
      ctx.fillText(label.toUpperCase(), x + 14, y + 21);
      ctx.fillStyle = '#ffffff';
      ctx.font = '700 15px "Plus Jakarta Sans", sans-serif';
      ctx.fillText(fitText(ctx, value, w - 28), x + 14, y + 41);
    }}

    function drawCardToCanvas() {{
      const card = CARDS[activeCard];
      const canvas = document.createElement('canvas');
      canvas.width = CARD_W * CANVAS_SCALE;
      canvas.height = CARD_H * CANVAS_SCALE;
      const ctx = canvas.getContext('2d');
      ctx.scale(CANVAS_SCALE, CANVAS_SCALE);

      // Card background: gradient fill inside rounded border
      const bg = ctx.createLinearGradient(0, 0, CARD_W, CARD_H);
      bg.addColorStop(0, '#0b1a11');
      bg.addColorStop(1, '#050a0f');
      ctx.fillStyle = bg;
      roundRectPath(ctx, 0, 0, CARD_W, CARD_H, 28);
      ctx.fill();
      ctx.save();
      ctx.clip();

      // Ambient glow at the top
      const glow = ctx.createRadialGradient(CARD_W / 2, 0, 0, CARD_W / 2, 0, 220);
      glow.addColorStop(0, 'rgba(29, 185, 84, 0.18)');
      glow.addColorStop(1, 'rgba(29, 185, 84, 0)');
      ctx.fillStyle = glow;
      ctx.fillRect(0, 0, CARD_W, 240);

      // Header: brand left, period badge right
      ctx.textAlign = 'left';
      ctx.font = '800 20px "Plus Jakarta Sans", sans-serif';
      ctx.fillStyle = TEXT;
      ctx.fillText('spota', 32, 52);
      ctx.fillStyle = ACCENT;
      ctx.fillText('tui', 32 + ctx.measureText('spota').width, 52);

      ctx.font = '700 11px "Plus Jakarta Sans", sans-serif';
      const badgeText = card.period.toUpperCase();
      const badgeW = ctx.measureText(badgeText).width + 24;
      ctx.fillStyle = 'rgba(29, 185, 84, 0.12)';
      roundRectPath(ctx, CARD_W - 32 - badgeW, 34, badgeW, 24, 12);
      ctx.fill();
      ctx.strokeStyle = 'rgba(29, 185, 84, 0.3)';
      ctx.lineWidth = 1;
      ctx.stroke();
      ctx.fillStyle = ACCENT;
      ctx.fillText(badgeText, CARD_W - 32 - badgeW + 12, 50);

      // Cover art (or placeholder) centered
      const coverSize = 174;
      const coverX = (CARD_W - coverSize) / 2;
      const coverY = 96;
      ctx.save();
      roundRectPath(ctx, coverX, coverY, coverSize, coverSize, 20);
      ctx.clip();
      const coverGrad = ctx.createLinearGradient(coverX, coverY, coverX + coverSize, coverY + coverSize);
      coverGrad.addColorStop(0, '#1b2820');
      coverGrad.addColorStop(1, '#05080c');
      ctx.fillStyle = coverGrad;
      ctx.fillRect(coverX, coverY, coverSize, coverSize);
      const img = document.getElementById('cover-art-img');
      if (img && img.style.display === 'block' && img.complete && img.naturalWidth > 0) {{
        try {{
          ctx.drawImage(img, coverX, coverY, coverSize, coverSize);
        }} catch (e) {{
          console.warn('cover art could not be drawn, keeping placeholder:', e);
        }}
      }} else {{
        // Placeholder music note
        ctx.fillStyle = 'rgba(255, 255, 255, 0.2)';
        ctx.font = '48px "Plus Jakarta Sans", sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('♪', coverX + coverSize / 2, coverY + coverSize / 2 + 16);
      }}
      ctx.restore();
      roundRectPath(ctx, coverX, coverY, coverSize, coverSize, 20);
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
      ctx.lineWidth = 1;
      ctx.stroke();

      // Top-track panel
      const panelY = 300;
      ctx.fillStyle = 'rgba(255, 255, 255, 0.02)';
      roundRectPath(ctx, 32, panelY, CARD_W - 64, 104, 18);
      ctx.fill();
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.04)';
      ctx.stroke();

      ctx.textAlign = 'center';
      ctx.font = '700 10px "JetBrains Mono", monospace';
      const rankText = 'NO. 1 TRACK';
      const rankW = ctx.measureText(rankText).width + 16;
      ctx.fillStyle = 'rgba(29, 185, 84, 0.1)';
      roundRectPath(ctx, (CARD_W - rankW) / 2, panelY + 14, rankW, 20, 6);
      ctx.fill();
      ctx.strokeStyle = 'rgba(29, 185, 84, 0.18)';
      ctx.stroke();
      ctx.fillStyle = ACCENT;
      ctx.fillText(rankText, CARD_W / 2, panelY + 28);

      ctx.fillStyle = '#ffffff';
      ctx.font = '800 18px "Plus Jakarta Sans", sans-serif';
      ctx.fillText(fitText(ctx, card.track, CARD_W - 104), CARD_W / 2, panelY + 60);
      ctx.fillStyle = MUTED;
      ctx.font = '400 14px "Plus Jakarta Sans", sans-serif';
      ctx.fillText(fitText(ctx, card.artist, CARD_W - 104), CARD_W / 2, panelY + 84);

      // Stat pills
      const pillW = (CARD_W - 64 - 12) / 2;
      drawStatPill(ctx, 32, 424, pillW, 54, 'Qualified Plays', String(card.plays));
      drawStatPill(ctx, 32 + pillW + 12, 424, pillW, 54, 'Listening Time', card.time);
      drawStatPill(ctx, 32, 490, CARD_W - 64, 54, 'Top Artist', card.topArtist);
      drawStatPill(ctx, 32, 556, CARD_W - 64, 54, 'Top Album', card.topAlbum);

      // Footer
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.04)';
      ctx.beginPath();
      ctx.moveTo(32, 622);
      ctx.lineTo(CARD_W - 32, 622);
      ctx.stroke();
      ctx.textAlign = 'left';
      ctx.fillStyle = MUTED;
      ctx.font = '400 11px "Plus Jakarta Sans", sans-serif';
      ctx.fillText('generated by spotatui', 32, 642);
      ctx.textAlign = 'right';
      ctx.fillStyle = ACCENT;
      ctx.font = '400 11px "JetBrains Mono", monospace';
      ctx.fillText('github.com/LargeModGames/spotatui', CARD_W - 32, 642);
      ctx.restore();

      // Card border on top of everything
      roundRectPath(ctx, 0.5, 0.5, CARD_W - 1, CARD_H - 1, 28);
      ctx.strokeStyle = 'rgba(29, 185, 84, 0.22)';
      ctx.lineWidth = 1;
      ctx.stroke();

      return canvas;
    }}

    function downloadCard() {{
      const btn = document.querySelector('.download-btn');
      const originalText = btn.innerHTML;
      btn.disabled = true;
      btn.innerHTML = `
        <svg class="animate-spin" xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 1 1 21.2 8H18" /></svg>
        Generating...
      `;
      btn.style.opacity = '0.7';

      const restore = () => {{
        btn.disabled = false;
        btn.innerHTML = originalText;
        btn.style.opacity = '1';
      }};

      document.fonts.ready.then(() => {{
        try {{
          const canvas = drawCardToCanvas();
          const link = document.createElement('a');
          const dateStr = new Date().toISOString().slice(0, 10);
          const periodSlug = CARDS[activeCard].period.toLowerCase().replace(/[^a-z0-9]+/g, '-');
          link.download = `spotatui-recap-${{periodSlug}}-${{dateStr}}.png`;
          link.href = canvas.toDataURL('image/png');
          link.click();
        }} catch (err) {{
          console.error(err);
          alert('Could not render image. Please try again!');
        }}
        restore();
      }}).catch(restore);
    }}
  </script>
</body>
</html>
"#,
    title = escape_html(period_label(period)),
    summary = escape_html(if listens.is_empty() {
      "No qualified local listening history was recorded for this window yet."
    } else {
      "A shareable HTML snapshot of your qualified local listening history, generated directly from spotatui."
    }),
    alt_title = escape_html(period_label(alt_period)),
    total_plays = card.plays,
    total_time = escape_html(&card.time),
    top_track_title = escape_html(&card.track_title),
    top_track_artist = escape_html(&card.track_artist),
    top_artist_name = escape_html(&card.top_artist),
    top_album_title = escape_html(&card.top_album),
    card_js = card.to_js_object(),
    alt_card_js = alt_card.to_js_object(),
    top_tracks_html = render_ranked_entries(&top_tracks, "No tracks yet."),
    top_artists_html = render_ranked_entries(&top_artists, "No artists yet."),
    top_albums_html = render_ranked_entries(&top_albums, "No albums yet."),
    recent_html = render_recent_entries(listens),
    days_html = render_bar_entries(&listening_days),
    hours_html = render_bar_entries(&listening_hours),
  )
}

#[derive(Clone, Debug)]
pub struct RankedEntry {
  pub display: String,
  pub detail: String,
  pub value: u64,
  pub uri: Option<String>,
}

pub fn aggregate_top_tracks(listens: &[ListenRecord], limit: usize) -> Vec<RankedEntry> {
  let mut totals: BTreeMap<String, (String, u64, u64, Option<String>)> = BTreeMap::new();
  for record in listens {
    let key = record
      .item_id
      .clone()
      .or_else(|| record.item_uri.clone())
      .unwrap_or_else(|| format!("{}::{}", record.title, record.artists.join(", ")));
    let entry = totals.entry(key).or_insert_with(|| {
      (
        format!("{} - {}", record.title, record.artists.join(", ")),
        0,
        0,
        record.item_uri.clone(),
      )
    });
    entry.1 += record.listened_ms;
    entry.2 += 1;
  }

  sort_ranked_entries(
    totals
      .into_values()
      .map(|(display, listened_ms, plays, uri)| RankedEntry {
        display,
        detail: format!("{} plays · {}", plays, format_duration(listened_ms)),
        value: listened_ms,
        uri,
      })
      .collect(),
    limit,
  )
}

fn split_artists(combo: &str) -> Vec<String> {
  let normalized = combo.replace(" and ", ", ").replace(" & ", ", ");
  normalized
    .split(',')
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect()
}

pub fn aggregate_top_artists(listens: &[ListenRecord], limit: usize) -> Vec<RankedEntry> {
  let mut totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();
  for record in listens {
    for artist_combo in &record.artists {
      let individual_artists = split_artists(artist_combo);
      for artist in individual_artists {
        let entry = totals.entry(artist).or_insert((0, 0));
        entry.0 += record.listened_ms;
        entry.1 += 1;
      }
    }
  }

  sort_ranked_entries(
    totals
      .into_iter()
      .map(|(artist, (listened_ms, plays))| RankedEntry {
        display: artist,
        detail: format!("{} track hits · {}", plays, format_duration(listened_ms)),
        value: listened_ms,
        uri: None,
      })
      .collect(),
    limit,
  )
}

pub fn aggregate_top_albums(listens: &[ListenRecord], limit: usize) -> Vec<RankedEntry> {
  let mut totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();
  for record in listens {
    if record.album.trim().is_empty() {
      continue;
    }
    let entry = totals.entry(record.album.clone()).or_insert((0, 0));
    entry.0 += record.listened_ms;
    entry.1 += 1;
  }

  sort_ranked_entries(
    totals
      .into_iter()
      .map(|(album, (listened_ms, plays))| RankedEntry {
        display: album,
        detail: format!("{} plays · {}", plays, format_duration(listened_ms)),
        value: listened_ms,
        uri: None,
      })
      .collect(),
    limit,
  )
}

pub fn aggregate_days(listens: &[ListenRecord]) -> Vec<RankedEntry> {
  let mut totals: BTreeMap<String, u64> = BTreeMap::new();
  for record in listens {
    let label = record.ended_at.format("%Y-%m-%d").to_string();
    *totals.entry(label).or_default() += record.listened_ms;
  }

  totals
    .into_iter()
    .rev()
    .take(10)
    .map(|(label, listened_ms)| RankedEntry {
      display: label,
      detail: format_duration(listened_ms),
      value: listened_ms,
      uri: None,
    })
    .collect::<Vec<_>>()
    .into_iter()
    .rev()
    .collect()
}

fn aggregate_hours(listens: &[ListenRecord]) -> Vec<RankedEntry> {
  let mut totals: BTreeMap<u32, u64> = BTreeMap::new();
  for record in listens {
    let local_hour = record.ended_at.with_timezone(&chrono::Local).hour();
    *totals.entry(local_hour).or_default() += record.listened_ms;
  }

  (0..24)
    .map(|hour| RankedEntry {
      display: format!("{hour:02}:00"),
      detail: format_duration(*totals.get(&hour).unwrap_or(&0)),
      value: *totals.get(&hour).unwrap_or(&0),
      uri: None,
    })
    .collect()
}

fn sort_ranked_entries(mut entries: Vec<RankedEntry>, limit: usize) -> Vec<RankedEntry> {
  entries.sort_by(|left, right| {
    right
      .value
      .cmp(&left.value)
      .then_with(|| left.display.cmp(&right.display))
  });
  entries.truncate(limit);
  entries
}

fn render_ranked_entries(entries: &[RankedEntry], empty_label: &str) -> String {
  if entries.is_empty() {
    return format!(r#"<p class="subtle">{}</p>"#, escape_html(empty_label));
  }

  let items = entries
    .iter()
    .enumerate()
    .map(|(i, entry)| {
      let parts: Vec<&str> = entry.display.split(" - ").collect();
      let (title, subtitle) = if parts.len() == 2 {
        (parts[0], format!("<div class=\"entry-artist\">{}</div>", escape_html(parts[1])))
      } else {
        (entry.display.as_str(), "".to_string())
      };

      let detail_html = format!(r#"<div class="subtle">{}</div>"#, escape_html(&entry.detail));
      format!(
        "<li><span class=\"rank\">#{}</span><div class=\"entry-details\"><strong>{}</strong>{}{}</div></li>",
        i + 1,
        escape_html(title),
        subtitle,
        detail_html
      )
    })
    .collect::<Vec<_>>()
    .join("");
  format!("<ul class=\"ranked-list\">{items}</ul>")
}

fn render_recent_entries(listens: &[ListenRecord]) -> String {
  let recent_records = listens.iter().rev().take(5).collect::<Vec<_>>();

  if recent_records.is_empty() {
    return r#"<p class="subtle">No recent plays.</p>"#.to_string();
  }

  let items = recent_records
    .iter()
    .map(|record| {
      let local_time = record.ended_at.with_timezone(&chrono::Local);
      let time_str = local_time.format("%b %d, %H:%M").to_string();
      format!(
        "<li><span class=\"recent-icon\"><svg xmlns=\"http://www.w3.org/2000/svg\" width=\"14\" height=\"14\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"12\" cy=\"12\" r=\"10\"></circle><polyline points=\"12 6 12 12 16 14\"></polyline></svg></span><div class=\"entry-details\"><strong>{}</strong><div class=\"entry-artist\">{}</div><div class=\"subtle\">listened at {}</div></div></li>",
        escape_html(&record.title),
        escape_html(&record.artists.join(", ")),
        escape_html(&time_str)
      )
    })
    .collect::<Vec<_>>()
    .join("");
  format!("<ul class=\"ranked-list\">{items}</ul>")
}

fn render_bar_entries(entries: &[RankedEntry]) -> String {
  if entries.is_empty() {
    return r#"<p class="subtle">No data yet.</p>"#.to_string();
  }

  let max_value = entries
    .iter()
    .map(|entry| entry.value)
    .max()
    .unwrap_or(1)
    .max(1);
  let rows = entries
    .iter()
    .map(|entry| {
      let width = (entry.value as f64 / max_value as f64) * 100.0;
      format!(
        r#"<div class="bar-row">
  <div class="bar-label"><span>{}</span><span>{}</span></div>
  <div class="bar"><span style="width:{:.2}%"></span></div>
</div>"#,
        escape_html(&entry.display),
        escape_html(&entry.detail),
        width
      )
    })
    .collect::<Vec<_>>()
    .join("");
  format!(r#"<div class="bars">{rows}</div>"#)
}

pub fn period_label(period: RecapPeriod) -> &'static str {
  match period {
    RecapPeriod::SevenDays => "Last 7 Days",
    RecapPeriod::ThirtyDays => "Last 30 Days",
    RecapPeriod::Month => "This Month",
    RecapPeriod::Year => "This Year",
    RecapPeriod::All => "All Time",
  }
}

pub fn format_duration(total_ms: u64) -> String {
  let total_minutes = total_ms / 1000 / 60;
  let hours = total_minutes / 60;
  let minutes = total_minutes % 60;
  if hours == 0 {
    format!("{minutes}m")
  } else {
    format!("{hours}h {minutes}m")
  }
}

fn escape_html(input: &str) -> String {
  input
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
    .replace('\'', "&#39;")
}

/// Encode a string as a JS string literal (quotes included). Unlike
/// `escape_html`, the value round-trips exactly; needed for the recap page's
/// inline `<script>` data. `</` is escaped so a value can never close the
/// script tag early.
fn js_string(input: &str) -> String {
  serde_json::to_string(input)
    .unwrap_or_else(|_| "\"\"".to_string())
    .replace("</", "<\\/")
}

fn last_synced_file_path() -> Result<PathBuf> {
  let home = dirs::home_dir().ok_or_else(|| anyhow!("No $HOME directory found for history"))?;
  Ok(
    home
      .join(".config")
      .join("spotatui")
      .join(HISTORY_SUBDIR)
      .join("last_synced_at.txt"),
  )
}

/// Resolve a sync endpoint, honoring the `SPOTATUI_SYNC_BASE_URL` override
/// (useful for pointing the client at a local backend during development).
fn resolve_sync_url(path: &str) -> String {
  let base = match std::env::var(SYNC_BASE_URL_ENV_KEY) {
    Ok(base) if !base.trim().is_empty() => base.trim().trim_end_matches('/').to_string(),
    _ => SYNC_BASE_URL.to_string(),
  };
  format!("{}{}", base, path)
}

/// Post the current now-playing state to the cloud dashboard / public widget.
/// Uses a shared HTTP client to reuse the connection pool across frequent calls.
async fn sync_now_playing_to_cloud(
  client: &reqwest::Client,
  sync_token: &str,
  payload: &NowPlayingPayload,
) -> Result<()> {
  let response = client
    .post(resolve_sync_url(NOW_PLAYING_SYNC_PATH))
    .header("Authorization", format!("Bearer {}", sync_token))
    .json(payload)
    .send()
    .await?;

  if !response.status().is_success() {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    return Err(anyhow!("now-playing sync failed ({}): {}", status, body));
  }
  Ok(())
}

/// Clear the now-playing status (playback stopped or the TUI is exiting).
async fn clear_now_playing_with_client(client: &reqwest::Client, sync_token: &str) -> Result<()> {
  let response = client
    .delete(resolve_sync_url(NOW_PLAYING_SYNC_PATH))
    .header("Authorization", format!("Bearer {}", sync_token))
    .send()
    .await?;

  if !response.status().is_success() {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    return Err(anyhow!("clear now-playing failed ({}): {}", status, body));
  }
  Ok(())
}

/// Clear the now-playing status when the TUI exits.
pub async fn clear_now_playing_from_cloud(sync_token: &str) -> Result<()> {
  clear_now_playing_with_client(
    crate::infra::network::requests::shared_http_client(),
    sync_token,
  )
  .await
}

/// Sync new listen records to the cloud using the given HTTP client.
async fn sync_history_to_cloud_with_client(
  client: &reqwest::Client,
  sync_token: &str,
) -> Result<()> {
  let path = last_synced_file_path()?;

  use chrono::TimeZone;
  let last_synced_at = match fs::read_to_string(&path) {
    Ok(content) => DateTime::parse_from_rfc3339(content.trim())
      .map(|dt| dt.with_timezone(&Utc))
      .unwrap_or_else(|_| Utc.timestamp_opt(0, 0).unwrap()),
    Err(_) => Utc.timestamp_opt(0, 0).unwrap(),
  };

  let listens = load_listens()?;
  let new_listens: Vec<&ListenRecord> = listens
    .iter()
    .filter(|record| record.ended_at > last_synced_at)
    .collect();

  if new_listens.is_empty() {
    log::info!("no new listening history records to sync");
    return Ok(());
  }

  let response = client
    .post(resolve_sync_url(CLOUD_SYNC_PATH))
    .header("Authorization", format!("Bearer {}", sync_token))
    .json(&new_listens)
    .send()
    .await?;

  if response.status().is_success() {
    if let Some(last_record) = new_listens.last() {
      fs::write(&path, last_record.ended_at.to_rfc3339())?;
    }
    log::info!(
      "successfully synchronized listening history to cloud ({} tracks)",
      new_listens.len()
    );
  } else {
    let status = response.status();
    let err_body = response.text().await.unwrap_or_default();
    log::warn!(
      "failed to synchronize history: {} (status {})",
      err_body,
      status
    );
    return Err(anyhow!("Sync failed: {}", err_body));
  }

  Ok(())
}

pub async fn sync_history_to_cloud(sync_token: &str) -> Result<()> {
  sync_history_to_cloud_with_client(
    crate::infra::network::requests::shared_http_client(),
    sync_token,
  )
  .await
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::TimeZone;

  fn record_at(day: u32, listened_ms: u64, qualified: bool) -> ListenRecord {
    let timestamp = Utc.with_ymd_and_hms(2026, 5, day, 12, 0, 0).unwrap();
    ListenRecord {
      started_at: timestamp,
      ended_at: timestamp,
      listened_ms,
      duration_ms: 180_000,
      qualified,
      title: format!("Track {day}"),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      item_kind: HistoryItemKind::Track,
      item_id: Some(format!("id-{day}")),
      item_uri: Some(format!("spotify:track:id-{day}")),
      context_uri: None,
      source: HistoryPlaybackSource::NativeContext,
    }
  }

  #[test]
  fn qualification_requires_minimum_duration_and_progress() {
    assert!(!qualifies_listen(30_000, 15_000));
    assert!(!qualifies_listen(180_000, 80_000));
    assert!(qualifies_listen(180_000, 90_000));
    assert!(qualifies_listen(800_000, 240_000));
  }

  #[test]
  fn all_time_filter_keeps_only_qualified_records() {
    let records = vec![record_at(20, 100_000, false), record_at(21, 120_000, true)];
    let filtered = filter_listens_for_period(&records, RecapPeriod::All);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].title, "Track 21");
  }

  #[test]
  fn cloud_sync_uses_public_spotatui_domain() {
    assert_eq!(
      resolve_sync_url(CLOUD_SYNC_PATH),
      "https://spotatui.com/api/sync"
    );
  }

  #[test]
  fn now_playing_uses_public_spotatui_domain() {
    assert_eq!(
      resolve_sync_url(NOW_PLAYING_SYNC_PATH),
      "https://spotatui.com/api/sync/now-playing"
    );
  }

  fn snapshot(title: &str, is_playing: bool) -> PlaybackSnapshot {
    PlaybackSnapshot {
      metadata: crate::infra::media_metadata::PlaybackMetadata {
        title: title.to_string(),
        artists: vec!["Artist".to_string()],
        album: "Album".to_string(),
        image_url: Some("https://i.scdn.co/image/abc".to_string()),
        duration_ms: 200_000,
      },
      item_kind: PlaybackItemKind::Track,
      item_id: None,
      item_uri: None,
      context_uri: None,
      source: PlaybackSource::ExternalDevice,
      progress_ms: 10_000,
      is_playing,
      is_live: false,
      shuffle: false,
      repeat: None,
    }
  }

  #[test]
  fn now_playing_payload_serializes_contract_fields() {
    let payload = NowPlayingPayload::from_snapshot(&snapshot("Song", true), 1_700_000_000_000);
    let value = serde_json::to_value(&payload).unwrap();
    let object = value.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
      keys,
      [
        "album",
        "artists",
        "duration_ms",
        "image_url",
        "is_live",
        "is_playing",
        "progress_at_ms",
        "progress_ms",
        "title",
      ]
    );
    assert_eq!(object["title"], "Song");
    assert_eq!(object["artists"], serde_json::json!(["Artist"]));
    assert_eq!(object["progress_at_ms"], 1_700_000_000_000_i64);
  }

  #[test]
  fn now_playing_payload_omits_missing_image_url() {
    let mut snap = snapshot("Song", true);
    snap.metadata.image_url = None;
    let value = serde_json::to_value(NowPlayingPayload::from_snapshot(&snap, 0)).unwrap();
    assert!(value.as_object().unwrap().get("image_url").is_none());
  }

  #[test]
  fn now_playing_payload_maps_live_radio_snapshot() {
    let mut snap = snapshot("SomaFM Groove Salad", true);
    snap.is_live = true;
    snap.metadata.duration_ms = 0;
    snap.metadata.image_url = None;
    let payload = NowPlayingPayload::from_snapshot(&snap, 5);
    assert!(payload.is_live);
    assert_eq!(payload.duration_ms, 0);
    assert!(payload.image_url.is_none());
  }

  #[test]
  fn tracker_pushes_on_track_change_and_not_within_heartbeat() {
    let mut tracker = NowPlayingTracker::default();
    let t0 = Instant::now();
    assert!(matches!(
      tracker.observe(Some(&snapshot("One", true)), t0),
      Some(NowPlayingAction::Push(_))
    ));
    // Same track one second later, progress advancing in step: no push.
    let mut later = snapshot("One", true);
    later.progress_ms = 11_000;
    assert!(tracker
      .observe(Some(&later), t0 + std::time::Duration::from_secs(1))
      .is_none());
    assert!(matches!(
      tracker.observe(
        Some(&snapshot("Two", true)),
        t0 + std::time::Duration::from_secs(2)
      ),
      Some(NowPlayingAction::Push(_))
    ));
  }

  #[test]
  fn tracker_pushes_on_play_pause_flip() {
    let mut tracker = NowPlayingTracker::default();
    let t0 = Instant::now();
    tracker.observe(Some(&snapshot("One", true)), t0);
    let action = tracker.observe(
      Some(&snapshot("One", false)),
      t0 + std::time::Duration::from_secs(1),
    );
    assert!(matches!(
      action,
      Some(NowPlayingAction::Push(ref payload)) if !payload.is_playing
    ));
  }

  #[test]
  fn tracker_heartbeats_even_when_paused() {
    let mut tracker = NowPlayingTracker::default();
    let t0 = Instant::now();
    tracker.observe(Some(&snapshot("One", false)), t0);
    let action = tracker.observe(
      Some(&snapshot("One", false)),
      t0 + std::time::Duration::from_secs(NOW_PLAYING_HEARTBEAT_SECS + 1),
    );
    assert!(matches!(action, Some(NowPlayingAction::Push(_))));
  }

  #[test]
  fn tracker_pushes_on_seek_discontinuity() {
    let mut tracker = NowPlayingTracker::default();
    let t0 = Instant::now();
    tracker.observe(Some(&snapshot("One", true)), t0);
    let mut seeked = snapshot("One", true);
    seeked.progress_ms = 120_000;
    let action = tracker.observe(Some(&seeked), t0 + std::time::Duration::from_secs(2));
    assert!(matches!(action, Some(NowPlayingAction::Push(_))));
  }

  #[test]
  fn tracker_clears_once_when_playback_stops() {
    let mut tracker = NowPlayingTracker::default();
    let t0 = Instant::now();
    tracker.observe(Some(&snapshot("One", true)), t0);
    assert!(matches!(
      tracker.observe(None, t0 + std::time::Duration::from_secs(1)),
      Some(NowPlayingAction::Clear)
    ));
    assert!(tracker
      .observe(None, t0 + std::time::Duration::from_secs(2))
      .is_none());
  }

  fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
  }

  #[test]
  fn streak_counts_consecutive_run_ending_today() {
    let today = day(2026, 5, 21);
    let days: BTreeSet<NaiveDate> = [day(2026, 5, 19), day(2026, 5, 20), today]
      .into_iter()
      .collect();
    let summary = streaks_from_days(&days, today, 90_000);
    assert_eq!(summary.current_days, 3);
    assert_eq!(summary.longest_days, 3);
    assert_eq!(summary.today_ms, 90_000);
  }

  #[test]
  fn streak_survives_until_midnight_when_today_has_no_listen() {
    let today = day(2026, 5, 21);
    let days: BTreeSet<NaiveDate> = [day(2026, 5, 19), day(2026, 5, 20)].into_iter().collect();
    let summary = streaks_from_days(&days, today, 0);
    assert_eq!(summary.current_days, 2);
    assert_eq!(summary.today_ms, 0);
  }

  #[test]
  fn streak_resets_after_a_gap() {
    let today = day(2026, 5, 21);
    let days: BTreeSet<NaiveDate> = [day(2026, 5, 15), day(2026, 5, 16), day(2026, 5, 17)]
      .into_iter()
      .collect();
    let summary = streaks_from_days(&days, today, 0);
    assert_eq!(summary.current_days, 0);
    assert_eq!(summary.longest_days, 3);
  }

  #[test]
  fn longest_streak_can_exceed_current() {
    let today = day(2026, 5, 21);
    let days: BTreeSet<NaiveDate> = [
      day(2026, 5, 10),
      day(2026, 5, 11),
      day(2026, 5, 12),
      day(2026, 5, 13),
      day(2026, 5, 20),
      today,
    ]
    .into_iter()
    .collect();
    let summary = streaks_from_days(&days, today, 0);
    assert_eq!(summary.current_days, 2);
    assert_eq!(summary.longest_days, 4);
  }

  #[test]
  fn streak_is_zero_with_no_listens() {
    let summary = streaks_from_days(&BTreeSet::new(), day(2026, 5, 21), 0);
    assert_eq!(summary, StreakSummary::default());
  }

  #[test]
  fn compute_streaks_ignores_unqualified_records() {
    let records = vec![record_at(19, 60_000, false), record_at(20, 60_000, true)];
    let summary = compute_streaks(&records);
    assert_eq!(summary.longest_days, 1);
  }

  #[test]
  fn top_tracks_respect_limit_and_carry_uri() {
    let records: Vec<ListenRecord> = (1..=8)
      .map(|d| record_at(d, 60_000 * d as u64, true))
      .collect();
    let entries = aggregate_top_tracks(&records, 3);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].display, "Track 8 - Artist");
    assert_eq!(entries[0].uri.as_deref(), Some("spotify:track:id-8"));
  }

  #[test]
  fn stats_data_totals_only_reflect_given_records() {
    let records = vec![record_at(20, 100_000, true), record_at(21, 50_000, true)];
    let stats = build_stats_data(&records);
    assert_eq!(stats.total_plays, 2);
    assert_eq!(stats.total_time_ms, 150_000);
    assert_eq!(stats.top_tracks.len(), 2);
  }

  #[test]
  fn recap_html_uses_canvas_renderer_not_html2canvas() {
    let records = vec![record_at(20, 120_000, true)];
    let html = render_history_recap_html(RecapPeriod::All, &records, RecapPeriod::ThirtyDays, &[]);
    assert!(!html.contains("html2canvas"));
    assert!(html.contains("getContext('2d')"));
  }

  #[test]
  fn recap_html_escapes_titles_for_both_dom_and_js() {
    let mut record = record_at(20, 120_000, true);
    record.title = r#"Mr. Brightside "Live" & Loud </script>"#.to_string();
    let records = [record];
    let html = render_history_recap_html(RecapPeriod::All, &records, RecapPeriod::ThirtyDays, &[]);
    // DOM side stays HTML-escaped
    assert!(html.contains("&quot;Live&quot; &amp; Loud"));
    // JS side round-trips the raw value as a valid string literal, with the
    // script-closing sequence neutralized
    assert!(html.contains(r#"\"Live\" & Loud"#));
    assert!(html.contains(r#"<\/script>"#));
    assert!(!html.contains(r#"& Loud </script>"#));
  }

  #[test]
  fn recap_html_embeds_both_card_periods_with_toggle() {
    let this_month = record_at(20, 120_000, true);
    let html = render_history_recap_html(
      RecapPeriod::ThirtyDays,
      std::slice::from_ref(&this_month),
      RecapPeriod::All,
      &[this_month.clone(), record_at(21, 60_000, true)],
    );
    // Toggle buttons for both periods
    assert!(html.contains(r#"onclick="setActiveCard(0)">Last 30 Days<"#));
    assert!(html.contains(r#"onclick="setActiveCard(1)">All Time<"#));
    // Both card datasets are embedded with their own play counts
    assert!(html.contains(r#"period: "Last 30 Days", track:"#));
    assert!(html.contains(r#"period: "All Time", track:"#));
    assert!(html.contains("plays: 1,"));
    assert!(html.contains("plays: 2,"));
  }

  #[test]
  fn js_string_round_trips_special_characters() {
    assert_eq!(js_string("plain"), r#""plain""#);
    assert_eq!(js_string(r#"a "b" & c"#), r#""a \"b\" & c""#);
    assert_eq!(js_string("</script>"), r#""<\/script>""#);
  }

  #[test]
  fn auto_recap_due_on_missing_invalid_or_old_timestamp() {
    let now = 1_800_000_000_i64;
    assert!(auto_recap_is_due(None, now));
    assert!(auto_recap_is_due(Some("not a number"), now));
    assert!(auto_recap_is_due(
      Some(&(now - 31 * 24 * 3600).to_string()),
      now
    ));
    assert!(!auto_recap_is_due(
      Some(&(now - 24 * 3600).to_string()),
      now
    ));
  }
}
