use anyhow::Result;
use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPO_URL: &str = "https://github.com/LargeModGames/spotatui";
const REPO_TAGLINE: &str = "Open-source on GitHub";

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

#[cfg(all(test, feature = "subsonic"))]
mod tests {
  use super::*;

  #[test]
  fn subsonic_activity_carries_no_login_in_its_image() {
    let snapshot = crate::infra::media_metadata::tests::subsonic_snapshot();
    let playback = DiscordPlayback {
      title: snapshot.metadata.title.clone(),
      artist: snapshot.primary_artist(),
      album: snapshot.metadata.album.clone(),
      state: "Artist - Album".to_string(),
      image_url: snapshot.metadata.image_url.clone(),
      duration_ms: snapshot.metadata.duration_ms,
      progress_ms: 0,
      is_playing: true,
    };
    let json = serde_json::to_string(&build_activity(&playback)).unwrap();
    assert!(json.contains("Track"), "{json}");
    for param in ["u=alice", "t=", "s="] {
      assert!(!json.contains(param), "{json}");
    }
  }
}
