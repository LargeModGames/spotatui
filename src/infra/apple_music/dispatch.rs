//! A bounded, serial Music worker, separate from the serial IoEvent pump.
//! A foreign start returns after pause, or proof that its Apple Event was not delivered.
use super::{parse_playlists, parse_snapshot, parse_tracks, parse_uri, Browse, Command, MusicUri};
use crate::{core::app::App, infra::network::IoEvent};
use anyhow::{bail, ensure, Result};
use std::{
  future::Future,
  sync::{Arc, Weak},
  time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex};

pub(crate) trait Client: Send + Sync + 'static {
  fn execute(&self, command: Command) -> impl Future<Output = Result<String>> + Send;
}

enum Work {
  Transport {
    generation: u64,
    /// `RemoteState::intent_revision` when the command was queued.
    revision: u64,
    command: Command,
  },
  Handoff {
    generation: u64,
    event: IoEvent,
  },
  Browse {
    generation: u64,
    request: Browse,
    offset: usize,
  },
}

pub(crate) struct Router {
  app: Arc<Mutex<App>>,
  tx: mpsc::Sender<Work>,
}

/// Validate every URI before claiming playback. Mixed-source lists never reach
/// Music or fall through to Spotify; no cross-source queue in this version.
fn start_command(
  context: &Option<String>,
  uris: &Option<Vec<String>>,
  offset: Option<usize>,
) -> Result<Option<Command>> {
  let is_apple = |u: &str| u.starts_with("applemusic:");
  if !context.as_deref().is_some_and(is_apple)
    && !uris
      .as_ref()
      .is_some_and(|list| list.iter().any(|u| is_apple(u)))
  {
    return Ok(None);
  }
  if let Some(context) = context {
    let container = parse_uri(context)?;
    let track = uris
      .as_ref()
      .map(|list| -> Result<String> {
        ensure!(
          list.len() == 1,
          "Apple Music accepts one selected track in a playlist"
        );
        match parse_uri(&list[0])? {
          MusicUri::Track(id) => Ok(id),
          _ => bail!("Expected an Apple Music track"),
        }
      })
      .transpose()?;
    return Ok(Some(Command::Play {
      container,
      track,
      offset: offset.unwrap_or(0),
    }));
  }
  let list = uris.as_ref().expect("Apple URI list was found");
  for uri in list {
    ensure!(
      matches!(parse_uri(uri)?, MusicUri::Track(_)),
      "Apple Music URI lists must contain library tracks only"
    );
  }
  let uri = list
    .get(offset.unwrap_or(0))
    .ok_or_else(|| anyhow::anyhow!("Apple Music track offset is out of range"))?;
  Ok(Some(Command::Play {
    container: parse_uri(uri)?,
    track: None,
    offset: 0,
  }))
}

impl Router {
  #[cfg(all(feature = "apple-music", target_os = "macos"))]
  pub(crate) fn new(app: &Arc<Mutex<App>>) -> Self {
    Self::with_client(app, super::macos::MacClient)
  }

  pub(crate) fn with_client<C: Client>(app: &Arc<Mutex<App>>, client: C) -> Self {
    let (tx, rx) = mpsc::channel(32);
    tokio::spawn(worker(Arc::downgrade(app), rx, client));
    Self {
      app: Arc::clone(app),
      tx,
    }
  }

  /// Returning None consumes the event immediately; no Apple call is awaited.
  pub(crate) async fn route_apple_music_event(&self, event: IoEvent) -> Option<IoEvent> {
    if let IoEvent::AppleMusicHandoff { generation, event } = event {
      return self
        .app
        .lock()
        .await
        .finish_apple_music_handoff(generation)
        .then_some(*event);
    }

    // Reserve before changing ownership. A busy helper must not lose a start
    // or invalidate the live claim without accepting the corresponding work.
    let mut app = self.app.lock().await;
    let volume_generation = match &event {
      IoEvent::AppleMusicVolume { generation, .. } => Some(*generation),
      _ => None,
    };
    let work = match event {
      IoEvent::AppleMusicVolume { generation, volume } => {
        if !app.apple_music_owns_playback()
          || app.apple_music_state().generation != generation
          || app.apple_music_state().switching
          || app.apple_music_state().quit_seen.is_some()
        {
          app.finish_apple_music_volume(generation);
          app.is_loading = false;
          return None;
        }
        Work::Transport {
          generation,
          revision: app.apple_music_state().intent_revision,
          command: Command::Volume(volume),
        }
      }
      IoEvent::AppleMusicPage {
        generation,
        request,
        offset,
      } => {
        if !app.apple_music_browse_is_current(&request, generation) {
          return None;
        }
        Work::Browse {
          generation,
          request,
          offset,
        }
      }
      IoEvent::StartPlayback(ref context, ref uris, offset)
        if context.is_some() || uris.is_some() =>
      {
        let command = match start_command(context, uris, offset) {
          Ok(command) => command,
          Err(error) => {
            app.report_apple_music_error(&error);
            return None;
          }
        };
        if command.is_none() && !app.apple_music_owns_playback() {
          app.cancel_recovered_apple_music_handoff();
          return Some(event);
        }
        let permit = match self.tx.try_reserve() {
          Ok(permit) => permit,
          Err(_) => {
            app.set_error_status_message("Music is busy; retry the playback request", 4);
            app.resync_apple_music_list();
            return None;
          }
        };
        match command {
          Some(command) => {
            if let Err(error) = command.arguments() {
              app.report_apple_music_error(&error);
              return None;
            }
            let generation = app.claim_apple_music();
            app.note_apple_music_command_queued();
            permit.send(Work::Transport {
              generation,
              revision: app.apple_music_state().intent_revision,
              command,
            });
          }
          None => {
            let generation = app.begin_apple_music_handoff();
            permit.send(Work::Handoff { generation, event });
          }
        }
        app.is_loading = false;
        return None;
      }
      other if app.apple_music_owns_playback() => {
        if app.apple_music_state().switching
          && crate::infra::network::Network::event_is_transport(&other)
        {
          app.set_status_message("Waiting for Music to pause before switching source", 4);
          return None;
        }
        // Music was just found quit: a key now would launch it again and
        // keep the claim. Starting a Music track still works.
        if app.apple_music_state().quit_seen.is_some()
          && crate::infra::network::Network::event_is_transport(&other)
        {
          app.set_status_message("Music is not running; choose a Music track to start it", 4);
          return None;
        }
        let command = match other {
          IoEvent::StartPlayback(None, None, None) => Command::Resume,
          IoEvent::PausePlayback => Command::Pause,
          IoEvent::NextTrack => Command::Next,
          IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => Command::Previous,
          IoEvent::Seek(position) => Command::Seek(position),
          IoEvent::ChangeVolume(volume) => Command::Volume(volume),
          IoEvent::Shuffle(_) | IoEvent::Repeat(_) => {
            app.set_status_message("Use Music to change shuffle or repeat", 4);
            return None;
          }
          IoEvent::AdvanceNativeQueue | IoEvent::FinishNativeQueue | IoEvent::AddItemToQueue(_) => {
            app.set_status_message(crate::core::queue::APPLE_MUSIC_QUEUE_UNSUPPORTED, 4);
            return None;
          }
          IoEvent::TransferPlaybackToDevice(..) => {
            app.set_status_message("Start a Spotify track to switch playback from Music", 4);
            return None;
          }
          _ => return Some(other),
        };
        Work::Transport {
          generation: app.apple_music_state().generation,
          revision: app.apple_music_state().intent_revision,
          command,
        }
      }
      other => return Some(other),
    };
    let transport = matches!(work, Work::Transport { .. });
    if self.tx.try_send(work).is_err() {
      if let Some(generation) = volume_generation {
        app.finish_apple_music_volume(generation);
      }
      app.set_error_status_message("Music is busy; retry the request", 4);
    } else if transport {
      app.note_apple_music_command_queued();
    }
    app.is_loading = false;
    None
  }
}

/// Music can report "playing" for a moment after a pause (about 0.35s
/// measured), so re-read before deciding the pause did not take: up to 2s.
async fn confirm_paused<C: Client>(client: &C, first: super::Snapshot) -> Result<super::Snapshot> {
  let mut snapshot = first;
  for _ in 0..8 {
    if !snapshot.playing {
      return Ok(snapshot);
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    snapshot = parse_snapshot(&client.execute(Command::Snapshot).await?)?;
  }
  ensure!(
    !snapshot.playing,
    "Music did not acknowledge pause; the other source was not started"
  );
  Ok(snapshot)
}

/// After a failed status read, polling continues at a growing interval
/// (2 s, 4 s, ... up to 30 s) instead of every second. Stopping for good would
/// freeze the playbar and the end-of-track continuation while Music kept
/// playing. A poll error is reported at most once a minute: an error status
/// holds back every ordinary status message while it lasts, so a Music that
/// fails and recovers in turn must not keep one up.
#[derive(Default)]
struct PollBackoff {
  failures: u32,
  next: Option<Instant>,
  reported_at: Option<Instant>,
}

impl PollBackoff {
  const MAX_WAIT: Duration = Duration::from_secs(30);
  const REPORT_GAP: Duration = Duration::from_secs(60);

  fn ready(&self, now: Instant) -> bool {
    self.next.is_none_or(|at| now >= at)
  }

  /// A status read or a command failed: wait longer before the next read.
  fn failed(&mut self, now: Instant) {
    self.failures = self.failures.saturating_add(1);
    let wait = Duration::from_secs(1 << self.failures.min(5)).min(Self::MAX_WAIT);
    self.next = Some(now + wait);
  }

  /// Whether a poll error may be shown now; records it when it may.
  fn take_report(&mut self, now: Instant) -> bool {
    if self
      .reported_at
      .is_some_and(|at| now.duration_since(at) < Self::REPORT_GAP)
    {
      return false;
    }
    self.reported_at = Some(now);
    true
  }

  /// Music answered: poll every tick again. The report gap is kept.
  fn succeeded(&mut self) {
    self.failures = 0;
    self.next = None;
  }
}

async fn worker<C: Client>(weak: Weak<Mutex<App>>, mut rx: mpsc::Receiver<Work>, client: C) {
  let mut poll = tokio::time::interval(Duration::from_secs(1));
  poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  // Explicit commands always report their own failures; poll errors are
  // rate-limited (see `PollBackoff`).
  let mut backoff = PollBackoff::default();
  loop {
    let work = tokio::select! {
      biased;
      next = rx.recv() => match next { Some(work) => Some(work), None => break },
      _ = poll.tick() => None,
    };
    let Some(app) = weak.upgrade() else {
      break;
    };
    match work {
      Some(Work::Transport {
        generation,
        revision,
        command,
      }) => {
        let volume = matches!(command, Command::Volume(_));
        if !owns_generation(&app, generation).await {
          let mut app = app.lock().await;
          app.finish_apple_music_command();
          if volume {
            app.finish_apple_music_volume(generation);
          }
          continue;
        }
        let result = client
          .execute(command)
          .await
          .and_then(|json| parse_snapshot(&json));
        let mut app = app.lock().await;
        app.finish_apple_music_command();
        if volume {
          app.finish_apple_music_volume(generation);
        }
        match result {
          Ok(snapshot) => {
            let refused = snapshot.started == Some(false);
            app.accept_apple_music_snapshot(generation, snapshot);
            if refused {
              app.note_apple_music_did_not_start(generation, revision);
            }
            backoff.succeeded();
          }
          Err(error) => {
            app.apple_music_failed(generation, error);
            backoff.failed(Instant::now());
          }
        }
      }
      Some(Work::Handoff { generation, event }) => {
        if !owns_generation(&app, generation).await {
          continue;
        }
        let result = match client
          .execute(Command::Pause)
          .await
          .and_then(|json| parse_snapshot(&json))
        {
          Ok(snapshot) => confirm_paused(&client, snapshot).await,
          Err(error) => Err(error),
        };
        let mut app = app.lock().await;
        match result {
          Ok(snapshot) if app.apple_music_state().generation == generation => {
            app.acknowledge_apple_music_pause(generation, snapshot);
            app.dispatch_without_spinner(IoEvent::AppleMusicHandoff {
              generation,
              event: Box::new(event),
            });
          }
          Ok(_) => {}
          Err(error) => {
            if app.apple_music_failed(generation, error) {
              let generation = app.defer_recovered_apple_music_handoff();
              app.dispatch_without_spinner(IoEvent::AppleMusicHandoff {
                generation,
                event: Box::new(event),
              });
            }
          }
        }
      }
      Some(Work::Browse {
        generation,
        request,
        offset,
      }) => {
        if !app
          .lock()
          .await
          .apple_music_browse_is_current(&request, generation)
        {
          continue;
        }
        let result = client
          .execute(Command::Browse(request.clone(), offset))
          .await;
        let mut app = app.lock().await;
        if !app.apple_music_browse_is_current(&request, generation) {
          continue;
        }
        let next = result.and_then(|json| {
          if request == Browse::Playlists {
            let page = parse_playlists(&json)?;
            ensure!(
              page.offset == offset,
              "Music library changed during loading; reload it"
            );
            let end = page.next;
            app.append_apple_music_playlists(page.items);
            Ok((end, page.total))
          } else {
            let page = parse_tracks(&json)?;
            ensure!(
              page.offset == offset,
              "Music library changed during loading; reload it"
            );
            let end = page.next;
            app.append_apple_music_tracks(page.items, matches!(request, Browse::Search(_)));
            Ok((end, page.total))
          }
        });
        match next {
          Ok((end, total)) if end < total => {
            app.set_status_message(format!("Loading Apple Music library: {end}/{total}"), 4);
            // Re-enter at the tail of the pump/worker so transport is served
            // between pages. The same generation guards every append.
            app.dispatch_without_spinner(IoEvent::AppleMusicPage {
              generation,
              request,
              offset: end,
            });
          }
          Ok((end, _)) => app.set_status_message(format!("Apple Music: loaded {end} items"), 3),
          Err(error) => app.report_apple_music_error(&error),
        }
      }
      None => {
        let generation = {
          let app = app.lock().await;
          if !app.apple_music_owns_playback()
            || app.apple_music_state().switching
            || !backoff.ready(Instant::now())
          {
            continue;
          }
          app.apple_music_state().generation
        };
        let result = client
          .execute(Command::Snapshot)
          .await
          .and_then(|json| parse_snapshot(&json));
        let mut app = app.lock().await;
        match result {
          Ok(snapshot) => {
            app.accept_apple_music_snapshot(generation, snapshot);
            backoff.succeeded();
          }
          Err(error) => {
            let now = Instant::now();
            backoff.failed(now);
            // A read that a newer start overtook says nothing about Music now.
            let current = app.apple_music_state().generation == generation;
            if current && backoff.take_report(now) {
              app.set_error_status_message(
                format!("Music status unavailable: {error}. Retrying in the background."),
                8,
              );
            }
          }
        }
      }
    }
  }
}

async fn owns_generation(app: &Arc<Mutex<App>>, generation: u64) -> bool {
  let app = app.lock().await;
  app.apple_music_owns_playback() && app.apple_music_state().generation == generation
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;

  #[test]
  fn a_failed_status_read_backs_off_and_resumes_instead_of_stopping() {
    let start = Instant::now();
    let mut backoff = PollBackoff::default();
    assert!(backoff.ready(start));
    // Each failure doubles the wait from 2 s, capped at 30 s.
    for wait in [2, 4, 8, 16, 30, 30, 30] {
      backoff.failed(start);
      assert!(
        !backoff.ready(start + Duration::from_secs(wait - 1)),
        "{wait}"
      );
      assert!(backoff.ready(start + Duration::from_secs(wait)), "{wait}");
    }
    // A good read resumes polling every tick; the next failure waits 2 s again.
    backoff.succeeded();
    assert!(backoff.ready(start));
    backoff.failed(start);
    assert!(!backoff.ready(start + Duration::from_secs(1)));
    assert!(backoff.ready(start + Duration::from_secs(2)));
  }

  #[test]
  fn a_poll_error_is_reported_at_most_once_a_minute_even_when_reads_recover_in_between() {
    let start = Instant::now();
    let mut backoff = PollBackoff::default();
    assert!(backoff.take_report(start));
    backoff.succeeded();
    assert!(!backoff.take_report(start + Duration::from_secs(5)));
    assert!(!backoff.take_report(start + Duration::from_secs(59)));
    assert!(backoff.take_report(start + PollBackoff::REPORT_GAP));
  }

  /// Fails the first `failing` reads, then reports the volume it was given.
  struct FlakyClient {
    reads: std::sync::atomic::AtomicUsize,
    failing: usize,
    volume: u8,
  }

  impl Client for FlakyClient {
    async fn execute(&self, _command: Command) -> Result<String> {
      let read = self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      if read < self.failing {
        bail!("Music timed out");
      }
      Ok(format!(
        r#"{{"running":true,"playing":true,"track":null,"position":0,"volume":{}}}"#,
        self.volume
      ))
    }
  }

  #[tokio::test]
  async fn apple_music_polling_resumes_after_a_failed_read() {
    let app = Arc::new(Mutex::new(App::default()));
    app.lock().await.claim_apple_music();
    let (_tx, rx) = mpsc::channel(4);
    let client = FlakyClient {
      reads: std::sync::atomic::AtomicUsize::new(0),
      failing: 1,
      volume: 77,
    };
    tokio::spawn(worker(Arc::downgrade(&app), rx, client));
    // The first read fails at once; the retry comes 2 s later.
    tokio::time::sleep(Duration::from_millis(500)).await;
    {
      let app = app.lock().await;
      assert!(app.apple_music_state().snapshot.is_none());
      assert!(app
        .status_message()
        .is_some_and(|m| m.contains("Music status unavailable")));
    }
    tokio::time::sleep(Duration::from_millis(3_000)).await;
    let app = app.lock().await;
    assert_eq!(
      app.apple_music_state().snapshot.as_ref().map(|s| s.volume),
      Some(77)
    );
  }

  struct FakeClient {
    calls: Arc<Mutex<Vec<Command>>>,
    fail_pause: bool,
  }
  impl Client for FakeClient {
    async fn execute(&self, command: Command) -> Result<String> {
      self.calls.lock().await.push(command.clone());
      tokio::time::sleep(Duration::from_millis(40)).await;
      if self.fail_pause && command == Command::Pause {
        bail!("permission denied");
      }
      Ok(r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50}"#.into())
    }
  }

  /// Answers "playing" for the first `stale` reads, then "paused".
  struct StaleClient {
    reads: std::sync::atomic::AtomicUsize,
    stale: usize,
  }

  impl Client for StaleClient {
    async fn execute(&self, _command: Command) -> Result<String> {
      let read = self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      let playing = read < self.stale;
      Ok(format!(
        r#"{{"running":true,"playing":{playing},"track":null,"position":0,"volume":50}}"#
      ))
    }
  }

  #[tokio::test]
  async fn apple_music_a_late_pause_report_still_hands_off() {
    let stale = |n| StaleClient {
      reads: std::sync::atomic::AtomicUsize::new(0),
      stale: n,
    };
    let first =
      parse_snapshot(r#"{"running":true,"playing":true,"track":null,"position":0,"volume":50}"#)
        .unwrap();
    // One more "playing" read, then paused: accepted.
    assert!(
      !confirm_paused(&stale(1), first.clone())
        .await
        .unwrap()
        .playing
    );
    // Never pauses within the window: the other source must not start.
    assert!(confirm_paused(&stale(usize::MAX), first).await.is_err());
  }

  fn apple_start() -> IoEvent {
    IoEvent::StartPlayback(None, Some(vec!["applemusic:0123456789ABCDEF".into()]), None)
  }

  #[derive(Clone, Copy)]
  enum Rejection {
    Permission,
    Spawn,
    Launch,
    Timeout,
    Unknown,
  }

  struct RejectingClient(Rejection);

  impl Client for RejectingClient {
    async fn execute(&self, _command: Command) -> Result<String> {
      use super::super::CommandError;
      Err(match self.0 {
        Rejection::Permission => CommandError::AutomationDenied.into(),
        Rejection::Spawn => CommandError::HelperSpawn(std::io::Error::new(
          std::io::ErrorKind::NotFound,
          "missing helper",
        ))
        .into(),
        Rejection::Launch => CommandError::LaunchFailed(anyhow::anyhow!("open failed")).into(),
        Rejection::Timeout => anyhow::anyhow!("Music helper timed out"),
        Rejection::Unknown => anyhow::anyhow!("Music helper failed, error -1728"),
      })
    }
  }

  async fn wait_for_failure(app: &Arc<Mutex<App>>) {
    tokio::time::timeout(Duration::from_secs(2), async {
      while !app.lock().await.status_message_is_error() {
        tokio::task::yield_now().await;
      }
    })
    .await
    .unwrap();
  }

  #[tokio::test]
  async fn apple_music_undelivered_start_releases_the_claim_for_the_next_source() {
    for failure in [Rejection::Permission, Rejection::Spawn, Rejection::Launch] {
      let (tx, _rx) = channel();
      let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
      let router = Router::with_client(&app, RejectingClient(failure));
      assert!(router
        .route_apple_music_event(apple_start())
        .await
        .is_none());
      wait_for_failure(&app).await;
      assert!(!app.lock().await.apple_music_owns_playback());
      assert!(matches!(
        router
          .route_apple_music_event(IoEvent::StartPlayback(
            None,
            Some(vec!["spotify:track:next".into()]),
            None,
          ))
          .await,
        Some(IoEvent::StartPlayback(..))
      ));
    }
  }

  #[tokio::test]
  async fn apple_music_undelivered_handoff_pause_releases_its_held_start_once() {
    for failure in [Rejection::Permission, Rejection::Spawn, Rejection::Launch] {
      let (tx, rx) = channel();
      let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
      app.lock().await.claim_apple_music();
      let router = Router::with_client(&app, RejectingClient(failure));
      assert!(router
        .route_apple_music_event(IoEvent::StartPlayback(
          None,
          Some(vec!["file:///held.flac".into()]),
          None,
        ))
        .await
        .is_none());
      wait_for_failure(&app).await;
      assert!(!app.lock().await.apple_music_owns_playback());
      let event = rx
        .try_recv()
        .expect("the held start must return to the pump");
      assert!(matches!(
        router.route_apple_music_event(event).await,
        Some(IoEvent::StartPlayback(_, Some(uris), _)) if uris == ["file:///held.flac"]
      ));
      assert!(rx.try_recv().is_err());
      assert!(app
        .lock()
        .await
        .apple_music_state()
        .recovered_handoff
        .is_none());
    }
  }

  #[tokio::test]
  async fn apple_music_recovered_handoff_cannot_overtake_a_newer_foreign_start() {
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    app.lock().await.claim_apple_music();
    let router = Router::with_client(&app, RejectingClient(Rejection::Permission));
    router
      .route_apple_music_event(IoEvent::StartPlayback(
        None,
        Some(vec!["file:///old.flac".into()]),
        None,
      ))
      .await;
    wait_for_failure(&app).await;
    let held = rx.try_recv().unwrap();
    assert!(matches!(
      router
        .route_apple_music_event(IoEvent::StartPlayback(
          None,
          Some(vec!["spotify:track:new".into()]),
          None,
        ))
        .await,
      Some(IoEvent::StartPlayback(..))
    ));
    assert!(router.route_apple_music_event(held).await.is_none());
  }

  #[tokio::test]
  async fn apple_music_uncertain_start_or_handoff_failure_keeps_the_claim() {
    for failure in [Rejection::Timeout, Rejection::Unknown] {
      for handoff in [false, true] {
        let (tx, rx) = channel();
        let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
        let event = if handoff {
          app.lock().await.claim_apple_music();
          IoEvent::StartPlayback(None, Some(vec!["file:///held.flac".into()]), None)
        } else {
          apple_start()
        };
        let router = Router::with_client(&app, RejectingClient(failure));
        router.route_apple_music_event(event).await;
        wait_for_failure(&app).await;
        assert!(app.lock().await.apple_music_owns_playback());
        assert!(rx.try_recv().is_err());
      }
    }
  }

  #[tokio::test]
  async fn apple_music_rejected_volume_clears_its_in_flight_slot() {
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    let generation = app.lock().await.claim_apple_music();
    let (tx, _worker_rx) = mpsc::channel(1);
    tx.try_send(Work::Transport {
      generation,
      revision: 0,
      command: Command::Pause,
    })
    .ok()
    .unwrap();
    let router = Router {
      app: Arc::clone(&app),
      tx,
    };
    app.lock().await.set_apple_music_volume(73);
    assert_eq!(
      app.lock().await.apple_music_state().volume_in_flight,
      Some(73)
    );
    router.route_apple_music_event(rx.try_recv().unwrap()).await;
    assert!(app
      .lock()
      .await
      .apple_music_state()
      .volume_in_flight
      .is_none());
    assert!(app.lock().await.status_message_is_error());
  }

  #[tokio::test]
  async fn apple_music_queued_volume_cannot_escape_after_release_or_change_a_new_claim() {
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    let generation = app.lock().await.claim_apple_music();
    app.lock().await.set_apple_music_volume(73);
    let old_volume = rx.try_recv().unwrap();
    assert!(app.lock().await.apple_music_failed(
      generation,
      super::super::CommandError::AutomationDenied.into(),
    ));
    let (tx, mut work) = mpsc::channel(4);
    let router = Router {
      app: Arc::clone(&app),
      tx,
    };
    // Consumed even with no Music claim, instead of falling through to Spotify.
    assert!(router.route_apple_music_event(old_volume).await.is_none());
    assert!(work.try_recv().is_err());

    app.lock().await.claim_apple_music();
    app.lock().await.set_apple_music_volume(84);
    assert!(router
      .route_apple_music_event(IoEvent::AppleMusicVolume {
        generation,
        volume: 73,
      })
      .await
      .is_none());
    assert!(work.try_recv().is_err());
    assert_eq!(
      app.lock().await.apple_music_state().volume_in_flight,
      Some(84)
    );
    router.route_apple_music_event(rx.try_recv().unwrap()).await;
    assert!(matches!(
      work.try_recv(),
      Ok(Work::Transport {
        command: Command::Volume(84),
        ..
      })
    ));
  }

  #[tokio::test]
  async fn apple_music_stale_volume_work_does_not_clear_a_newer_in_flight_volume() {
    let (tx, _rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    let old = app.lock().await.claim_apple_music();
    app.lock().await.claim_apple_music();
    app.lock().await.set_apple_music_volume(73);
    let (tx, rx) = mpsc::channel(1);
    tx.try_send(Work::Transport {
      generation: old,
      revision: 0,
      command: Command::Volume(20),
    })
    .ok()
    .unwrap();
    drop(tx);
    let calls = Arc::new(Mutex::new(vec![]));
    worker(
      Arc::downgrade(&app),
      rx,
      FakeClient {
        calls: Arc::clone(&calls),
        fail_pause: false,
      },
    )
    .await;
    assert!(calls.lock().await.is_empty());
    assert_eq!(
      app.lock().await.apple_music_state().volume_in_flight,
      Some(73)
    );
  }

  #[tokio::test]
  async fn apple_music_completed_volume_clears_its_in_flight_slot() {
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    app.lock().await.claim_apple_music();
    app.lock().await.set_apple_music_volume(73);
    let router = Router::with_client(
      &app,
      FakeClient {
        calls: Arc::new(Mutex::new(vec![])),
        fail_pause: false,
      },
    );
    router.route_apple_music_event(rx.try_recv().unwrap()).await;
    tokio::time::timeout(Duration::from_secs(2), async {
      while app
        .lock()
        .await
        .apple_music_state()
        .volume_in_flight
        .is_some()
      {
        tokio::task::yield_now().await;
      }
    })
    .await
    .unwrap();
    assert_eq!(app.lock().await.apple_music_state().pending_commands, 0);
  }

  #[test]
  fn apple_music_start_rejects_mixed_sources_before_claiming() {
    assert!(start_command(
      &None,
      &Some(vec![
        "applemusic:0123456789ABCDEF".into(),
        "spotify:track:x".into()
      ]),
      None
    )
    .is_err());
    assert!(start_command(
      &None,
      &Some(vec!["applemusic:0123456789ABCDEF".into()]),
      Some(2)
    )
    .is_err());
    assert!(start_command(
      &Some("spotify:playlist:x".into()),
      &Some(vec!["applemusic:0123456789ABCDEF".into()]),
      None
    )
    .is_err());
  }

  #[tokio::test]
  async fn apple_music_slow_helper_does_not_hold_app_or_pump_and_handoff_waits_for_pause() {
    struct GatedClient {
      started: mpsc::UnboundedSender<Command>,
      finish: Arc<tokio::sync::Semaphore>,
    }
    impl Client for GatedClient {
      async fn execute(&self, command: Command) -> Result<String> {
        self.started.send(command).unwrap();
        self.finish.acquire().await.unwrap().forget();
        Ok(r#"{"running":true,"playing":false,"track":null,"position":0,"volume":50}"#.into())
      }
    }
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    let (started, mut calls) = mpsc::unbounded_channel();
    let finish = Arc::new(tokio::sync::Semaphore::new(0));
    let router = Router::with_client(
      &app,
      GatedClient {
        started,
        finish: Arc::clone(&finish),
      },
    );
    assert!(tokio::time::timeout(
      Duration::from_secs(1),
      router.route_apple_music_event(apple_start())
    )
    .await
    .unwrap()
    .is_none());
    assert!(matches!(
      tokio::time::timeout(Duration::from_secs(1), calls.recv())
        .await
        .unwrap(),
      Some(Command::Play { .. })
    ));
    // The helper has actually started and cannot finish without our permit.
    drop(
      tokio::time::timeout(Duration::from_secs(1), app.lock())
        .await
        .unwrap(),
    );
    assert!(tokio::time::timeout(
      Duration::from_secs(1),
      router.route_apple_music_event(IoEvent::StartPlayback(
        None,
        Some(vec!["spotify:track:x".into()]),
        None
      ))
    )
    .await
    .unwrap()
    .is_none());
    assert!(rx.try_recv().is_err());
    finish.add_permits(1);
    assert_eq!(
      tokio::time::timeout(Duration::from_secs(1), calls.recv())
        .await
        .unwrap(),
      Some(Command::Pause)
    );
    assert!(rx.try_recv().is_err());
    assert!(app.lock().await.apple_music_owns_playback());
    finish.add_permits(1);
    tokio::time::timeout(Duration::from_secs(2), async {
      loop {
        if let Ok(event) = rx.try_recv() {
          assert!(matches!(
            router.route_apple_music_event(event).await,
            Some(IoEvent::StartPlayback(..))
          ));
          break;
        }
        tokio::task::yield_now().await;
      }
    })
    .await
    .unwrap();
    assert!(!app.lock().await.apple_music_owns_playback());
  }

  #[tokio::test]
  async fn apple_music_keys_do_not_relaunch_a_music_just_found_quit() {
    let (tx, _rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    {
      let mut app = app.lock().await;
      let generation = app.claim_apple_music();
      app.accept_apple_music_snapshot(
        generation,
        parse_snapshot(r#"{"running":false,"playing":false,"track":null,"position":0,"volume":0}"#)
          .unwrap(),
      );
      assert!(app.apple_music_owns_playback());
    }
    let calls = Arc::new(Mutex::new(vec![]));
    let router = Router::with_client(
      &app,
      FakeClient {
        calls: Arc::clone(&calls),
        fail_pause: false,
      },
    );
    assert!(router
      .route_apple_music_event(IoEvent::ChangeVolume(40))
      .await
      .is_none());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!calls.lock().await.contains(&Command::Volume(40)));
    assert!(app
      .lock()
      .await
      .status_message()
      .is_some_and(|m| m.contains("not running")));
  }

  #[tokio::test]
  async fn apple_music_failed_pause_never_releases_another_start() {
    let (tx, rx) = channel();
    let app = Arc::new(Mutex::new(App::new(tx, UserConfig::new(), None)));
    app.lock().await.claim_apple_music();
    let router = Router::with_client(
      &app,
      FakeClient {
        calls: Arc::new(Mutex::new(vec![])),
        fail_pause: true,
      },
    );
    router
      .route_apple_music_event(IoEvent::StartPlayback(
        None,
        Some(vec!["file:///track.flac".into()]),
        None,
      ))
      .await;
    tokio::time::timeout(Duration::from_secs(2), async {
      while !app
        .lock()
        .await
        .status_message()
        .is_some_and(|m| m.contains("permission denied"))
      {
        tokio::task::yield_now().await;
      }
    })
    .await
    .unwrap();
    assert!(rx.try_recv().is_err());
    assert!(app.lock().await.apple_music_owns_playback());
  }
}
