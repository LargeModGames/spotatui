use crate::core::user_config::UserConfig;
use crate::infra::network::{search::SPOTIFY_SEARCH_LIMIT, IoEvent, Network};

use super::{
  util::{Flag, JumpDirection, Type},
  CliApp,
};

use anyhow::{anyhow, Result};
use clap::ArgMatches;

/// The Spotify state a subcommand reads before it acts.
#[derive(Debug, PartialEq, Eq)]
struct ProbePlan {
  devices: bool,
  playback: bool,
}

/// `seek` and the status line fetch their own playback, so they skip that probe.
fn probe_plan(cmd: &str, matches: &ArgMatches) -> ProbePlan {
  match cmd {
    "playback" => ProbePlan {
      devices: true,
      playback: [
        "toggle",
        "like",
        "dislike",
        "shuffle",
        "repeat",
        "share-track",
        "share-album",
      ]
      .into_iter()
      .any(|flag| matches.get_flag(flag)),
    },
    // The start seeds its shuffle state from the current playback.
    "play" => ProbePlan {
      devices: true,
      playback: true,
    },
    "list" => ProbePlan {
      devices: matches.get_flag("devices"),
      playback: false,
    },
    _ => ProbePlan {
      devices: false,
      playback: false,
    },
  }
}

// Handle the different subcommands
pub async fn handle_matches(
  matches: &ArgMatches,
  cmd: String,
  net: Network,
  config: UserConfig,
) -> Result<String> {
  let mut cli = CliApp::new(net, config);

  let plan = probe_plan(&cmd, matches);
  if plan.devices {
    cli.net.handle_network_event(IoEvent::GetDevices).await;
  }
  if plan.playback {
    cli
      .net
      .handle_network_event(IoEvent::GetCurrentPlayback)
      .await;
  }

  let devices_list = match cli.net.app.lock().await.devices() {
    Some(p) => p
      .devices
      .iter()
      .filter_map(|d| d.id.clone())
      .collect::<Vec<String>>(),
    None => Vec::new(),
  };

  // If the device_id is not specified, select the first available device
  let device_id = cli.net.app.lock().await.saved_spotify_device_id();
  let needs_device = match &device_id {
    Some(id) => !devices_list.contains(id),
    None => true,
  };
  if needs_device {
    // Select the first device available
    if let Some(d) = devices_list.first() {
      cli.net.app.lock().await.remember_spotify_device(d);
    }
  }

  // Only `playback` and `play` define `device`; a plain `get_one` panics
  // under clap's debug assertions when `list`/`search` reach this shared path.
  if let Some(d) = matches.try_get_one::<String>("device").ok().flatten() {
    cli.set_device(d.to_string()).await?;
  }

  // Evalute the subcommand
  let output = match cmd.as_str() {
    "playback" => {
      let format = matches.get_one::<String>("format").unwrap();

      // Commands that are 'single'
      if matches.get_flag("share-track") {
        return cli.share_track_or_episode().await;
      } else if matches.get_flag("share-album") {
        return cli.share_album_or_show().await;
      }

      // Run the action, and print out the status
      // No 'else if's because multiple different commands are possible
      if matches.get_flag("toggle") {
        cli.toggle_playback().await;
      }
      if let Some(d) = matches.get_one::<String>("transfer") {
        cli.transfer_playback(d).await?;
      }
      // Handle flags (like, dislike, shuffle, repeat)
      let flags = Flag::from_matches(matches);
      for f in flags {
        cli.mark(f).await?;
      }
      if matches.get_count("next") > 0 || matches.get_count("previous") > 0 {
        // Fetch current playback first so we know what track we're skipping from
        cli
          .net
          .handle_network_event(IoEvent::GetCurrentPlayback)
          .await;
        let old_id = cli.net.app.lock().await.cached_playback_item_id();

        let (direction, amount) = JumpDirection::from_matches(matches);
        for _ in 0..amount {
          cli.jump(&direction).await;
        }

        // Nothing playing: the status line has no old track to be wrong about,
        // and waiting would only spend the shared rate window.
        if let Some(old_id) = old_id {
          wait_for_playback_to_move_on(&mut cli, old_id).await;
        }
      }
      if let Some(vol) = matches.get_one::<String>("volume") {
        cli.volume(vol.to_string()).await?;
      }
      if let Some(secs) = matches.get_one::<String>("seek") {
        cli.seek(secs.to_string()).await?;
      }

      // Print out the status if no errors were found
      cli.get_status(format.to_string()).await
    }
    "play" => {
      let queue = matches.get_flag("queue");
      let random = matches.get_flag("random");
      let format = matches.get_one::<String>("format").unwrap();

      if let Some(uri) = matches.get_one::<String>("uri") {
        cli.play_uri(uri.to_string(), queue, random).await;
      } else if let Some(name) = matches.get_one::<String>("name") {
        let category = Type::play_from_matches(matches);
        cli.play(name.to_string(), category, queue, random).await?;
      }

      cli.get_status(format.to_string()).await
    }
    "list" => {
      let format = matches.get_one::<String>("format").unwrap().to_string();

      // Update the limits for the list and search functions
      // I think the small and big search limits are very confusing
      // so I just set them both to max, is this okay?
      if let Some(max) = matches.get_one::<String>("limit") {
        cli.update_query_limits(max.to_string(), 50).await?;
      }

      let category = Type::list_from_matches(matches);
      Ok(cli.list(category, &format).await)
    }
    "search" => {
      let format = matches.get_one::<String>("format").unwrap().to_string();

      // Update the limits for the list and search functions
      // I think the small and big search limits are very confusing
      // so I just set them both to max, is this okay?
      if let Some(max) = matches.get_one::<String>("limit") {
        cli
          .update_query_limits(max.to_string(), SPOTIFY_SEARCH_LIMIT)
          .await?;
      }

      let category = Type::search_from_matches(matches);
      Ok(
        cli
          .query(
            matches.get_one::<String>("search").unwrap().to_string(),
            format,
            category,
          )
          .await,
      )
    }
    // Clap enforces that one of the things above is specified
    _ => unreachable!(),
  };

  // Check if there was an error
  let api_error = cli.net.app.lock().await.api_error().to_owned();
  if api_error.is_empty() {
    output
  } else {
    Err(anyhow!("{}", api_error))
  }
}

const JUMP_SETTLE: std::time::Duration = std::time::Duration::from_secs(2);
const JUMP_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Wait until the playback Spotify reports no longer names `old`. Gives up after [`JUMP_SETTLE`].
async fn wait_for_playback_to_move_on(cli: &mut CliApp, old: String) {
  let deadline = tokio::time::Instant::now() + JUMP_SETTLE;
  loop {
    // Wraps the request too, so a slow or rate-gated one cannot stretch the wait.
    let fetched = tokio::time::timeout_at(
      deadline,
      cli.net.handle_network_event(IoEvent::GetCurrentPlayback),
    )
    .await;
    if fetched.is_err() {
      return;
    }
    if cli
      .net
      .app
      .lock()
      .await
      .cached_playback_item_id()
      .as_deref()
      != Some(old.as_str())
    {
      return;
    }
    let now = tokio::time::Instant::now();
    if now >= deadline {
      return;
    }
    tokio::time::sleep(JUMP_POLL_INTERVAL.min(deadline - now)).await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::App;
  use crate::infra::network::metadata::tests::{fake_spotify_login, read_http_request};
  use std::sync::Arc;
  use tokio::io::AsyncWriteExt;
  use tokio::sync::Mutex;

  const NOTHING: ProbePlan = ProbePlan {
    devices: false,
    playback: false,
  };

  fn plan(cmd: &str, args: &[&str]) -> ProbePlan {
    let command = match cmd {
      "playback" => crate::cli::playback_subcommand(),
      "play" => crate::cli::play_subcommand(),
      "list" => crate::cli::list_subcommand(),
      _ => crate::cli::search_subcommand(),
    };
    let matches = command
      .try_get_matches_from(std::iter::once(cmd).chain(args.iter().copied()))
      .unwrap();
    probe_plan(cmd, &matches)
  }

  #[test]
  fn search_and_the_library_lists_probe_nothing() {
    assert_eq!(plan("search", &["queen", "--tracks"]), NOTHING);
    assert_eq!(plan("list", &["--playlists"]), NOTHING);
    assert_eq!(plan("list", &["--liked"]), NOTHING);
  }

  #[test]
  fn the_device_list_probes_only_the_devices() {
    assert_eq!(
      plan("list", &["--devices"]),
      ProbePlan {
        devices: true,
        playback: false,
      }
    );
  }

  #[test]
  fn play_probes_the_devices_and_the_playback() {
    assert_eq!(
      plan("play", &["--uri", "spotify:track:1"]),
      ProbePlan {
        devices: true,
        playback: true,
      }
    );
  }

  #[test]
  fn playback_reads_the_playback_first_only_for_flags_that_act_on_it() {
    for flag in [
      "--toggle",
      "--like",
      "--dislike",
      "--shuffle",
      "--repeat",
      "--share-track",
      "--share-album",
    ] {
      assert!(plan("playback", &[flag]).playback, "{flag}");
    }
    for args in [
      &["--next"][..],
      &["--previous"],
      &["--volume", "40"],
      &["--seek", "+10"],
      &["--status"],
    ] {
      assert_eq!(
        plan("playback", args),
        ProbePlan {
          devices: true,
          playback: false,
        },
        "{args:?}"
      );
    }
  }

  /// A Spotify API that answers every request with an empty `204` and keeps each request line.
  async fn recording_network() -> (
    Network,
    Arc<std::sync::Mutex<Vec<String>>>,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    crate::infra::network::requests::fake::FakeSpotify,
  ) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    let server = tokio::spawn(async move {
      loop {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let line = request.lines().next().unwrap_or_default().to_string();
        log.lock().unwrap().push(line);
        stream
          .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
          .await
          .unwrap();
      }
    });

    let (tx, _rx) = std::sync::mpsc::channel();
    let mut app = App::new(tx, UserConfig::new(), Some(std::time::SystemTime::now()));
    let state_dir = tempfile::tempdir().unwrap();
    app.state_path = Some(state_dir.path().join("state.yml"));
    let app = Arc::new(Mutex::new(app));
    let (spotify, _fake) = fake_spotify_login(base_url);
    let net = Network::new(Some(spotify), &app);
    (net, seen, server, state_dir, _fake)
  }

  #[tokio::test]
  async fn a_search_sends_no_device_or_playback_request() {
    // Generous: every HTTP test shares the API pacing limiter.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
      let (net, seen, server, _state_dir, _fake) = recording_network().await;
      let matches = crate::cli::search_subcommand()
        .try_get_matches_from(["search", "queen", "--tracks"])
        .unwrap();

      let _ = handle_matches(&matches, "search".to_string(), net, UserConfig::new()).await;
      server.abort();

      let seen = seen.lock().unwrap();
      assert!(
        seen.iter().any(|line| line.contains("/v1/search")),
        "{seen:?}"
      );
      assert!(
        !seen.iter().any(|line| line.contains("/v1/me/player")),
        "{seen:?}"
      );
    })
    .await
    .expect("search test timed out");
  }

  fn is_playback_fetch(line: &str) -> bool {
    line.starts_with("GET /v1/me/player?") || line.starts_with("GET /v1/me/player ")
  }

  async fn requests_of(cmd: &str, args: &[&str]) -> Vec<String> {
    let (net, seen, server, _state_dir, _fake) = recording_network().await;
    let command = match cmd {
      "playback" => crate::cli::playback_subcommand(),
      _ => crate::cli::list_subcommand(),
    };
    let matches = command
      .try_get_matches_from(std::iter::once(cmd).chain(args.iter().copied()))
      .unwrap();
    let _ = handle_matches(&matches, cmd.to_string(), net, UserConfig::new()).await;
    server.abort();
    let seen = seen.lock().unwrap().clone();
    seen
  }

  #[tokio::test]
  async fn the_device_list_fetches_the_devices_and_not_the_playback() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
      let seen = requests_of("list", &["--devices"]).await;
      assert!(
        seen
          .iter()
          .any(|line| line.contains("/v1/me/player/devices")),
        "{seen:?}"
      );
      assert!(!seen.iter().any(|line| is_playback_fetch(line)), "{seen:?}");
    })
    .await
    .expect("device list test timed out");
  }

  #[tokio::test]
  async fn only_a_flag_that_reads_the_playback_fetches_it_before_acting() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
      let toggle = requests_of("playback", &["--toggle"]).await;
      let fetches = toggle.iter().filter(|line| is_playback_fetch(line)).count();
      assert_eq!(fetches, 2, "the probe and the status line: {toggle:?}");

      let next = requests_of("playback", &["--next"]).await;
      let fetches = next.iter().filter(|line| is_playback_fetch(line)).count();
      // --next now checks the track before jumping to avoid race conditions
      assert_eq!(fetches, 2, "the pre-jump check and the status line: {next:?}");
    })
      .await
      .expect("playback probe test timed out");
  }

  #[tokio::test]
  async fn the_status_line_does_not_fetch_the_liked_songs() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
      let (net, seen, server, _state_dir, _fake) = recording_network().await;
      let mut cli = CliApp::new(net, UserConfig::new());

      let status = cli.get_status("%t".to_string()).await;
      server.abort();

      assert!(status.is_err(), "nothing plays, so there is no status line");
      let seen = seen.lock().unwrap();
      assert!(
        seen.iter().any(|line| line.contains("/v1/me/player")),
        "{seen:?}"
      );
      assert!(
        !seen.iter().any(|line| line.contains("/v1/me/tracks")),
        "{seen:?}"
      );
    })
    .await
    .expect("status test timed out");
  }
}
