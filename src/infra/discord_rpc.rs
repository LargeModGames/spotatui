use crate::core::app::App;
use crate::core::playback_metadata::extract_playable_metadata;
use crate::core::user_config::UserConfig;
use anyhow::Result;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_DISCORD_CLIENT_ID: &str = "1464235043462447166";
const REPO_URL: &str = "https://github.com/LargeModGames/spotatui";
const REPO_TAGLINE: &str = "Open-source on GitHub";

#[derive(Clone, Debug, PartialEq)]
struct DiscordTrackInfo {
  title: String,
  artist: String,
  album: String,
  image_url: Option<String>,
  duration_ms: u32,
}

#[derive(Default)]
pub struct DiscordPresenceState {
  last_track: Option<DiscordTrackInfo>,
  last_is_playing: Option<bool>,
  last_progress_ms: u128,
}

#[derive(Clone, Debug)]
pub struct DiscordPlayback {
  pub title: String,
  pub artist: String,
  pub album: String,
  pub state: String,
  pub image_url: Option<String>,
  pub duration_ms: u32,
  pub progress_ms: u128,
  pub is_playing: bool,
}

enum DiscordRpcCommand {
  SetActivity(DiscordPlayback),
  ClearActivity,
}

pub struct DiscordRpcManager {
  command_tx: Sender<DiscordRpcCommand>,
}

pub fn resolve_app_id(user_config: &UserConfig) -> Option<String> {
  std::env::var("SPOTATUI_DISCORD_APP_ID")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .or_else(|| user_config.behavior.discord_rpc_client_id.clone())
    .or_else(|| Some(DEFAULT_DISCORD_CLIENT_ID.to_string()))
}

pub fn update_presence(manager: &DiscordRpcManager, state: &mut DiscordPresenceState, app: &App) {
  let playback = build_playback(app);

  match playback {
    Some(playback) => {
      let track_info = DiscordTrackInfo {
        title: playback.title.clone(),
        artist: playback.artist.clone(),
        album: playback.album.clone(),
        image_url: playback.image_url.clone(),
        duration_ms: playback.duration_ms,
      };

      let track_changed = state.last_track.as_ref() != Some(&track_info);
      let playing_changed = state.last_is_playing != Some(playback.is_playing);
      let progress_delta = playback.progress_ms.abs_diff(state.last_progress_ms);
      let progress_changed = progress_delta > 5000;

      if track_changed || playing_changed || progress_changed {
        manager.set_activity(&playback);
        state.last_track = Some(track_info);
        state.last_is_playing = Some(playback.is_playing);
        state.last_progress_ms = playback.progress_ms;
      }
    }
    None => {
      if state.last_track.is_some() {
        manager.clear();
        state.last_track = None;
        state.last_is_playing = None;
        state.last_progress_ms = 0;
      }
    }
  }
}

impl DiscordRpcManager {
  pub fn new(app_id: String) -> Result<Self> {
    let (command_tx, command_rx) = mpsc::channel();

    thread::spawn(move || run_discord_rpc_loop(app_id, command_rx));

    Ok(Self { command_tx })
  }

  pub fn set_activity(&self, playback: &DiscordPlayback) {
    let _ = self
      .command_tx
      .send(DiscordRpcCommand::SetActivity(playback.clone()));
  }

  pub fn clear(&self) {
    let _ = self.command_tx.send(DiscordRpcCommand::ClearActivity);
  }
}

fn build_playback(app: &App) -> Option<DiscordPlayback> {
  let (track_info, is_playing) = if let Some(native_info) = &app.native_track_info {
    let is_playing = app.native_is_playing.unwrap_or(true);
    (
      DiscordTrackInfo {
        title: native_info.name.clone(),
        artist: native_info.artists_display.clone(),
        album: native_info.album.clone(),
        image_url: None,
        duration_ms: native_info.duration_ms,
      },
      is_playing,
    )
  } else if let Some(context) = &app.current_playback_context {
    let is_playing = if app.is_streaming_active {
      app.native_is_playing.unwrap_or(context.is_playing)
    } else {
      context.is_playing
    };

    let item = context.item.as_ref()?;
    let m = extract_playable_metadata(item)?;
    (
      DiscordTrackInfo {
        title: m.title,
        artist: m.artist,
        album: m.album,
        image_url: m.art_url,
        duration_ms: m.duration_ms,
      },
      is_playing,
    )
  } else {
    return None;
  };

  let base_state = if track_info.album.is_empty() {
    track_info.artist.clone()
  } else {
    format!("{} - {}", track_info.artist, track_info.album)
  };
  let state = if is_playing {
    base_state
  } else if base_state.is_empty() {
    "Paused".to_string()
  } else {
    format!("Paused: {}", base_state)
  };

  Some(DiscordPlayback {
    title: track_info.title,
    artist: track_info.artist,
    album: track_info.album,
    state,
    image_url: track_info.image_url,
    duration_ms: track_info.duration_ms,
    progress_ms: app.song_progress_ms,
    is_playing,
  })
}

fn run_discord_rpc_loop(app_id: String, command_rx: Receiver<DiscordRpcCommand>) {
  let mut client: Option<DiscordIpcClient> = None;
  let mut last_connect_attempt = Instant::now() - Duration::from_secs(30);

  for command in command_rx {
    if !ensure_connected(&app_id, &mut client, &mut last_connect_attempt) {
      continue;
    }

    let mut disconnect = false;

    if let Some(ref mut ipc_client) = client {
      let result = match command {
        DiscordRpcCommand::SetActivity(playback) => {
          let activity = build_activity(&playback);
          ipc_client.set_activity(activity)
        }
        DiscordRpcCommand::ClearActivity => ipc_client.clear_activity(),
      };

      if result.is_err() {
        let _ = ipc_client.close();
        disconnect = true;
      }
    }

    if disconnect {
      client = None;
    }
  }

  if let Some(ref mut client) = client {
    let _ = client.clear_activity();
    let _ = client.close();
  }
}

fn ensure_connected(
  app_id: &str,
  client: &mut Option<DiscordIpcClient>,
  last_connect_attempt: &mut Instant,
) -> bool {
  if client.is_some() {
    return true;
  }

  if last_connect_attempt.elapsed() < Duration::from_secs(5) {
    return false;
  }

  *last_connect_attempt = Instant::now();

  let mut new_client = DiscordIpcClient::new(app_id);
  match new_client.connect() {
    Ok(()) => {
      *client = Some(new_client);
      true
    }
    Err(_) => false,
  }
}

fn build_activity(playback: &DiscordPlayback) -> activity::Activity<'_> {
  let mut activity = activity::Activity::new()
    .details(&playback.title)
    .details_url(REPO_URL)
    .state(&playback.state)
    .state_url(REPO_URL)
    .activity_type(activity::ActivityType::Listening);

  if let Some(image_url) = playback.image_url.as_deref() {
    let assets = activity::Assets::new()
      .large_image(image_url)
      .large_text(REPO_URL)
      .small_text(REPO_TAGLINE);
    activity = activity.assets(assets);
  }

  if playback.is_playing && playback.duration_ms > 0 {
    let now_secs = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .unwrap_or_default()
      .as_secs() as i64;
    let progress_secs = (playback.progress_ms / 1000) as i64;
    let duration_secs = (playback.duration_ms as i64) / 1000;
    let start = now_secs.saturating_sub(progress_secs);
    let end = start.saturating_add(duration_secs);

    let timestamps = activity::Timestamps::new().start(start).end(end);
    activity = activity.timestamps(timestamps);
  }

  activity
}
