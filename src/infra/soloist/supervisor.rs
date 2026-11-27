//! Starts, watches and stops the Soloist child, and bridges its WebSocket to the App.

use std::collections::BTreeSet;
use std::future::poll_fn;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use futures::{Sink, SinkExt, Stream, StreamExt};
use log::{debug, info, warn};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command as ProcessCommand};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::{oneshot, Mutex, MutexGuard, Notify};
use tokio_tungstenite::tungstenite::{Message, Result as WsResult};

use super::auth::ApiKey;
use super::link::{QueuedCommand, SoloistLink, StreamQuality, VolumeGate};
use super::logline::{self, LogEvent};
use super::process::{self, Exit, SpawnFailure, SpawnPlan};
use super::protocol::{self, Command, Event, Status};
use super::{prefs, Notice};
use crate::core::app::App;
use crate::core::user_config::StartupBehavior;

pub struct Config {
  pub generation: u64,
  pub path: PathBuf,
  pub device_name: String,
  pub key: ApiKey,
  pub data_dir: PathBuf,
  pub cache_dir: PathBuf,
  pub quality: u8,
  pub initial_volume: u8,
  pub startup_behavior: Option<StartupBehavior>,
  pub startup_shuffle: bool,
}

#[derive(Default)]
pub(super) struct Shared {
  shutting_down: AtomicBool,
  stopping_for_restart: AtomicBool,
  running: AtomicBool,
  stop: Notify,
  link: StdMutex<Option<Arc<SoloistLink>>>,
  done: StdMutex<Option<oneshot::Receiver<()>>>,
}

pub(super) fn shared() -> &'static Arc<Shared> {
  static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
  SHARED.get_or_init(Arc::default)
}

/// Soloist's pid as spotatui spawned it, to tell an orphan from a Soloist the user runs.
const CHILD_PID: &str = ".spotatui-child.pid";
const STALE_FILES: [&str; 3] = ["ws.addr", "ws.port", "soloist.pid"];
const START_BUDGET: Duration = Duration::from_secs(20);
const CONNECT_BUDGET: Duration = Duration::from_secs(3);
const OFFLINE_AFTER: Duration = Duration::from_secs(15);
/// How long a running child gets to exit after its socket closes, and to flush its last log lines.
const EXIT_GRACE: Duration = Duration::from_secs(1);
#[cfg(windows)]
pub(super) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn start(app: Arc<Mutex<App>>, config: Config) {
  let shared = shared().clone();
  if shared.is_stopping() || shared.running.swap(true, Ordering::SeqCst) {
    return;
  }
  clear_stale_stop(&shared);
  let (done_tx, done_rx) = oneshot::channel();
  if let Ok(mut done) = shared.done.lock() {
    *done = Some(done_rx);
  }
  tokio::spawn(async move {
    supervise(&app, &config, &shared).await;
    shared.running.store(false, Ordering::SeqCst);
    let _ = done_tx.send(());
  });
}

/// Drop the permit a `stop` left when no supervisor waited, so it cannot stop the next one.
fn clear_stale_stop(shared: &Shared) {
  let _ = futures::FutureExt::now_or_never(shared.stop.notified());
}

/// A supervisor runs: it starts, watches or restarts Soloist.
pub fn running() -> bool {
  shared().running.load(Ordering::SeqCst)
}

fn shutdown_commands(is_active: bool, status: Status) -> Vec<Command> {
  // Soloist's transports can control another device after a handoff.
  if !is_active {
    return Vec::new();
  }
  if status.is_playing() {
    vec![Command::Pause, Command::Deactivate]
  } else {
    vec![Command::Deactivate]
  }
}

/// Release playback if Soloist owns it, and stop the child, waiting at most `timeout`.
pub async fn stop(timeout: Duration) {
  stop_on(shared(), timeout).await;
}

async fn stop_on(shared: &Shared, timeout: Duration) {
  shared.shutting_down.store(true, Ordering::SeqCst);
  stop_child(shared, timeout).await;
}

async fn stop_child(shared: &Shared, timeout: Duration) {
  let link = shared.link.lock().ok().and_then(|link| link.clone());
  if let Some(link) = link {
    let snapshot = link.snapshot();
    for command in shutdown_commands(snapshot.is_active, snapshot.status) {
      link.send(command);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
  }
  shared.stop.notify_one();
  let done = shared.done.lock().ok().and_then(|mut done| done.take());
  if let Some(mut done) = done {
    if tokio::time::timeout(timeout, &mut done).await.is_err() {
      // Still running: keep the signal for `wait_stopped`.
      if let Ok(mut slot) = shared.done.lock() {
        slot.get_or_insert(done);
      }
    }
  }
}

/// Wait, without a limit, until a supervisor a timed-out stop left behind has exited.
async fn wait_stopped_on(shared: &Shared) {
  let done = shared.done.lock().ok().and_then(|mut done| done.take());
  if let Some(done) = done {
    let _ = done.await;
  }
}

impl Shared {
  pub(super) fn is_shutting_down(&self) -> bool {
    self.shutting_down.load(Ordering::SeqCst)
  }

  fn is_stopping(&self) -> bool {
    self.stopping_for_restart.load(Ordering::SeqCst) || self.is_shutting_down()
  }

  /// Stop the old child before switching. Quit leaves the shutdown flag set for the process.
  pub(super) async fn stop_for_restart(&self, timeout: Duration) -> bool {
    if self.is_shutting_down() {
      return false;
    }
    self.stopping_for_restart.store(true, Ordering::SeqCst);
    stop_child(self, timeout).await;
    if self.running.load(Ordering::SeqCst) && !self.is_shutting_down() {
      warn!("soloist: the old engine has not stopped yet; switching once it exits");
      wait_stopped_on(self).await;
    }
    self.stopping_for_restart.store(false, Ordering::SeqCst);
    !self.is_shutting_down()
  }
}

enum Outcome {
  Exited(Exit),
  Fatal(Notice),
  Restart { reactivate: bool },
}

async fn current_app<'a>(
  app: &'a Mutex<App>,
  config: &Config,
  shared: &Shared,
) -> Option<MutexGuard<'a, App>> {
  let app = app.lock().await;
  (!shared.is_stopping() && app.is_current_soloist_start(config.generation)).then_some(app)
}

async fn notify(app: &Mutex<App>, config: &Config, shared: &Shared, notice: Notice) {
  if let Some(mut app) = current_app(app, config, shared).await {
    app.soloist_notice(notice);
  }
}

/// `notify` for a child with an installed link: only while that link is still the App's.
async fn notify_link(
  app: &Mutex<App>,
  config: &Config,
  shared: &Shared,
  link: &Arc<SoloistLink>,
  notice: Notice,
) {
  if let Some(mut app) = current_app(app, config, shared).await {
    if app.is_current_soloist(link) {
      app.soloist_notice(notice);
    }
  }
}

async fn supervise(app: &Arc<Mutex<App>>, config: &Config, shared: &Shared) {
  let mut exits = Vec::new();
  let mut retried_dir = false;
  let mut reactivate = false;
  let mut first_login = true;
  let failure = loop {
    if current_app(app, config, shared).await.is_none() {
      return;
    }
    match run_once(app, config, shared, reactivate, &mut first_login).await {
      Outcome::Restart { reactivate: again } => reactivate = again,
      Outcome::Fatal(notice) => break notice,
      Outcome::Exited(exit) => {
        reactivate = false;
        match exit {
          Exit::Stopped => return,
          Exit::Expired => break Notice::Expired,
          Exit::DataDirInUse if !retried_dir => {
            retried_dir = true;
            clear_orphan(config).await;
          }
          Exit::DataDirInUse => break Notice::DirInUse,
          Exit::Crashed(code) => {
            warn!("soloist: exited with {code:?}");
            let now = Instant::now();
            exits.push(now);
            let Some(delay) = process::restart_delay(&exits, now) else {
              break Notice::KeepsExiting;
            };
            {
              let Some(mut app) = current_app(app, config, shared).await else {
                return;
              };
              app.soloist_starting();
              app.soloist_notice(Notice::Restarting);
            }
            tokio::select! {
              _ = tokio::time::sleep(delay) => {}
              _ = shared.stop.notified() => return,
            }
          }
        }
      }
    }
  };
  if let Some(mut app) = current_app(app, config, shared).await {
    app.soloist_failed(Some(failure));
  }
}

fn spawn(config: &Config) -> std::io::Result<Child> {
  let mut command = quiet_command(&config.path);
  command
    .args(process::args(&SpawnPlan {
      device_name: &config.device_name,
      key: &config.key,
      data_dir: &config.data_dir,
      cache_dir: &config.cache_dir,
      initial_volume: config.initial_volume,
    }))
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  #[cfg(target_os = "linux")]
  // SAFETY: prctl is async-signal-safe; the signal fires when the spawning thread exits.
  unsafe {
    command.pre_exec(|| {
      libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
      Ok(())
    });
  }
  command.spawn()
}

fn spawn_notice(config: &Config, error: &std::io::Error) -> Notice {
  match process::classify_spawn_error(error.kind(), error.raw_os_error()) {
    SpawnFailure::Missing => Notice::Missing(config.path.clone()),
    SpawnFailure::NotExecutable => Notice::NotExecutable(config.path.clone()),
    SpawnFailure::WrongCpu => Notice::WrongCpu,
    SpawnFailure::Other => Notice::StartFailed(error.to_string()),
  }
}

async fn drain(reader: impl AsyncRead + Unpin, events: UnboundedSender<LogEvent>, key: String) {
  let mut reader = BufReader::new(reader);
  let mut buf = Vec::new();
  loop {
    buf.clear();
    match reader.read_until(b'\n', &mut buf).await {
      Ok(0) | Err(_) => break,
      Ok(_) => {}
    }
    let line = logline::strip_ansi(String::from_utf8_lossy(&buf).trim_end());
    debug!("soloist: {}", logline::redact(&line, &key));
    if let Some(event) = logline::parse_line(&line) {
      let _ = events.send(event);
    }
  }
}

async fn write_commands<S>(
  mut sink: S,
  mut commands: UnboundedReceiver<QueuedCommand>,
  link: Weak<SoloistLink>,
) where
  S: Sink<Message> + Unpin,
{
  let mut gate = VolumeGate::default();
  loop {
    let deadline = gate.deadline();
    let queued = tokio::select! {
      command = commands.recv() => match command {
        Some(queued @ QueuedCommand { command: Command::SetVolume(volume), .. }) => {
          match gate.offer(volume, Instant::now()) {
            Some(volume) => QueuedCommand { command: Command::SetVolume(volume), ..queued },
            None => continue,
          }
        },
        Some(command) => command,
        None => break,
      },
      _ = sleep_until(deadline) => match gate.due(Instant::now()) {
        Some(volume) => QueuedCommand {
          command: Command::SetVolume(volume),
          activity_generation: None,
        },
        None => continue,
      },
    };
    // Check the activity generation after the sink is ready.
    if poll_fn(|cx| sink.poll_ready_unpin(cx)).await.is_err() {
      break;
    }
    let Some(link) = link.upgrade() else {
      break;
    };
    let submitted = link.submit_if_current(queued, |command| {
      sink.start_send_unpin(Message::Text(command.to_json().into()))
    });
    drop(link);
    let Some(submitted) = submitted else {
      continue;
    };
    if submitted.is_err() || sink.flush().await.is_err() {
      break;
    }
  }
}

async fn sleep_until(deadline: Option<Instant>) {
  match deadline {
    Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
    None => std::future::pending().await,
  }
}

fn read_pid(path: &Path) -> Option<u32> {
  std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn remove_stale(data_dir: &Path) {
  for file in STALE_FILES {
    let _ = std::fs::remove_file(data_dir.join(file));
  }
}

fn quiet_command(program: impl AsRef<std::ffi::OsStr>) -> ProcessCommand {
  let mut command = ProcessCommand::new(program);
  command
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
  #[cfg(windows)]
  command.creation_flags(CREATE_NO_WINDOW);
  command
}

/// Stop a Soloist that spotatui spawned and lost (a hard kill), when it still runs.
async fn clear_orphan(config: &Config) {
  let (Some(ours), Some(theirs)) = (
    read_pid(&config.data_dir.join(CHILD_PID)),
    read_pid(&config.data_dir.join("soloist.pid")),
  ) else {
    return;
  };
  if ours != theirs {
    return;
  }
  let mut status = quiet_command(&config.path);
  status
    .args(["ctl", "status", "-D"])
    .arg(&config.data_dir)
    .stdout(Stdio::piped());
  let running = tokio::time::timeout(Duration::from_secs(3), status.output())
    .await
    .ok()
    .and_then(Result::ok)
    .and_then(|output| process::parse_ctl_status(&String::from_utf8_lossy(&output.stdout)));
  if running != Some(ours) {
    return;
  }
  warn!("soloist: stopping an orphaned child (pid {ours})");
  let pid = ours.to_string();
  #[cfg(windows)]
  let mut kill = {
    let mut kill = quiet_command("taskkill");
    kill.args(["/PID", &pid, "/F"]);
    kill
  };
  #[cfg(not(windows))]
  let mut kill = {
    let mut kill = quiet_command("kill");
    kill.args(["-TERM", &pid]);
    kill
  };
  let _ = kill.stdout(Stdio::null()).status().await;
  tokio::time::sleep(Duration::from_secs(2)).await;
}

/// SIGTERM first where spotatui can send it, then a hard kill.
async fn terminate(child: &mut Child) {
  #[cfg(target_os = "linux")]
  if let Some(pid) = child.id() {
    // SAFETY: a plain signal to our own child.
    unsafe {
      libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    if tokio::time::timeout(Duration::from_secs(1), child.wait())
      .await
      .is_ok()
    {
      return;
    }
  }
  let _ = child.kill().await;
}

#[derive(Default)]
struct Seen {
  another_session: bool,
  expired: bool,
  bad_key: bool,
  /// Soloist is logged in now; a logout clears it.
  logged_in: bool,
  /// This child logged in at least once, so a later crash is not a rejected key.
  ever_logged_in: bool,
  /// A pairing line read while still logged in: the logout event may arrive just after it.
  pairing_held: bool,
  /// This child's first login ran its one-time steps.
  login_handled: bool,
  pairing_shown: bool,
  expiry_warned: bool,
}

impl Seen {
  /// The login state an event reports; after a logout the pairing prompt may show again.
  fn auth(&mut self, logged_in: bool) -> Option<Notice> {
    if logged_in {
      self.logged_in = true;
      self.ever_logged_in = true;
    } else if std::mem::take(&mut self.logged_in) {
      self.pairing_shown = std::mem::take(&mut self.pairing_held);
      if self.pairing_shown {
        return Some(Notice::NotPaired);
      }
    }
    None
  }

  fn note(&mut self, event: &LogEvent) -> Option<Notice> {
    match event {
      LogEvent::AnotherSession => self.another_session = true,
      LogEvent::Expired => self.expired = true,
      LogEvent::WaitingForLogin if self.logged_in => self.pairing_held = true,
      LogEvent::WaitingForLogin if !self.pairing_shown => {
        self.pairing_shown = true;
        return Some(Notice::NotPaired);
      }
      LogEvent::LoginFailed if !self.bad_key && !self.ever_logged_in => {
        self.bad_key = true;
        return Some(Notice::BadKey);
      }
      LogEvent::ExpiresIn { count, unit } if !self.expiry_warned && expires_soon(*count, unit) => {
        self.expiry_warned = true;
        return Some(Notice::ExpiresSoon {
          count: *count,
          unit: unit.clone(),
        });
      }
      _ => {}
    }
    None
  }

  fn startup_failure(
    &self,
    status: Option<std::io::Result<std::process::ExitStatus>>,
    shared: &Shared,
  ) -> Outcome {
    match status.map(|status| self.exit(status, shared)) {
      // A rejected key stays rejected on a restart; after a login the key was not the cause.
      Some(Exit::Crashed(_)) if self.bad_key && !self.ever_logged_in => {
        Outcome::Fatal(Notice::BadKey)
      }
      Some(exit) => Outcome::Exited(exit),
      None if self.expired => Outcome::Fatal(Notice::Expired),
      None if self.bad_key => Outcome::Fatal(Notice::BadKey),
      None => Outcome::Fatal(Notice::NoWebSocket),
    }
  }

  fn exit(&self, status: std::io::Result<std::process::ExitStatus>, shared: &Shared) -> Exit {
    process::classify_exit(
      status.ok().and_then(|status| status.code()),
      shared.is_stopping(),
      self.another_session,
      self.expired,
    )
  }
}

/// The codec lines arrive one per line; the sample rate closes the set.
#[derive(Default)]
struct PendingQuality {
  codec: Option<String>,
  bitrate: Option<u32>,
}

fn expires_soon(count: u32, unit: &str) -> bool {
  unit.starts_with("hour") || unit.starts_with("minute") || (unit.starts_with("day") && count < 7)
}

/// The id and port from stdout, else from the files of this very child.
fn id_and_port_from_files(data_dir: &Path, pid: Option<u32>) -> (Option<String>, Option<u16>) {
  if pid.is_none() || read_pid(&data_dir.join("soloist.pid")) != pid {
    return (None, None);
  }
  let id = std::fs::read_to_string(data_dir.join(".device_id"))
    .ok()
    .map(|id| id.trim().to_string())
    .filter(|id| !id.is_empty());
  let port = std::fs::read_to_string(data_dir.join("ws.port"))
    .ok()
    .and_then(|port| port.trim().parse().ok());
  (id, port)
}

async fn connect(
  port: u16,
) -> Option<
  tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
  let url = format!("ws://127.0.0.1:{port}");
  for _ in 0..10 {
    if let Ok((socket, _)) = tokio_tungstenite::connect_async(url.as_str()).await {
      return Some(socket);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
  }
  None
}

/// Seed the quality keys for accounts that appeared since the spawn: `None` while Soloist
/// has written no account prefs yet, else whether Soloist must restart to read them.
fn seed_new_accounts(data_dir: &Path, quality: u8, before: &BTreeSet<String>) -> Option<bool> {
  let present = prefs::user_dirs(data_dir);
  if present.is_empty() {
    return None;
  }
  let fresh = prefs::unseeded(&present, &prefs::seeded(data_dir));
  if fresh.is_empty() {
    return Some(false);
  }
  if let Err(error) = prefs::write_quality(data_dir, quality) {
    warn!("soloist: could not write the quality prefs: {error}");
    return Some(false);
  }
  let _ = prefs::mark_seeded(data_dir, &present);
  Some(fresh.iter().any(|user| !before.contains(user)))
}

fn confirms_login(event: &Event) -> bool {
  matches!(
    event,
    Event::Auth {
      logged_in: true,
      ..
    } | Event::State(_)
  )
}

/// Read the final log lines and any queued login confirmation within the same deadline.
/// Classify the exit before publishing notices, so a busy App lock cannot cut the tail short.
#[allow(clippy::too_many_arguments)]
async fn classify_tail(
  app: &Mutex<App>,
  config: &Config,
  shared: &Shared,
  link: Option<&Arc<SoloistLink>>,
  seen: &mut Seen,
  log_rx: &mut UnboundedReceiver<LogEvent>,
  stream: Option<&mut (dyn Stream<Item = WsResult<Message>> + Send + Unpin)>,
  status: Option<std::io::Result<std::process::ExitStatus>>,
  until: tokio::time::Instant,
  pending: Option<Notice>,
) -> Outcome {
  let mut notices: Vec<Notice> = pending.into_iter().collect();
  let stream = stream.filter(|_| !seen.ever_logged_in);
  let mut tail_login = false;
  let drained = tokio::select! {
    biased;
    _ = shared.stop.notified() => false,
    _ = tokio::time::timeout_at(until, async {
      let logs = async {
        while let Some(event) = log_rx.recv().await {
          notices.extend(seen.note(&event));
        }
      };
      let login = async {
        let Some(stream) = stream else { return };
        // The peer can exit with complete frames still in the TCP or WebSocket buffers.
        while let Some(Ok(message)) = stream.next().await {
          let Message::Text(text) = message else { continue };
          if protocol::parse_event(&text).as_ref().is_some_and(confirms_login) {
            tail_login = true;
            break;
          }
        }
      };
      tokio::join!(logs, login);
    }) => true,
  };
  if !drained {
    return Outcome::Exited(Exit::Stopped);
  }
  seen.ever_logged_in |= tail_login;
  let outcome = seen.startup_failure(status, shared);
  for notice in notices {
    if seen.ever_logged_in && matches!(notice, Notice::BadKey) {
      continue;
    }
    match link {
      Some(link) => notify_link(app, config, shared, link, notice).await,
      None => notify(app, config, shared, notice).await,
    }
  }
  outcome
}

async fn run_once(
  app: &Arc<Mutex<App>>,
  config: &Config,
  shared: &Shared,
  reactivate: bool,
  first_login: &mut bool,
) -> Outcome {
  if let Err(error) = std::fs::create_dir_all(&config.data_dir)
    .and_then(|_| std::fs::create_dir_all(&config.cache_dir))
  {
    return Outcome::Fatal(Notice::StartFailed(error.to_string()));
  }
  clear_orphan(config).await;
  remove_stale(&config.data_dir);
  match prefs::write_quality(&config.data_dir, config.quality) {
    Ok(0) => {}
    Ok(written) => {
      info!("soloist: set the quality and normalization keys in {written} prefs file(s)")
    }
    Err(error) => warn!("soloist: could not write the prefs keys: {error}"),
  }
  let users_before = prefs::user_dirs(&config.data_dir);
  let _ = prefs::mark_seeded(&config.data_dir, &users_before);

  let mut child = match spawn(config) {
    Ok(child) => child,
    Err(error) => return Outcome::Fatal(spawn_notice(config, &error)),
  };
  #[cfg(windows)]
  let _job = job::Job::contain(&child);
  let pid = child.id();
  if let Some(pid) = pid {
    let _ = std::fs::write(config.data_dir.join(CHILD_PID), pid.to_string());
  }
  let (log_tx, mut log_rx) = mpsc::unbounded_channel();
  if let Some(stdout) = child.stdout.take() {
    tokio::spawn(drain(
      stdout,
      log_tx.clone(),
      config.key.expose().to_string(),
    ));
  }
  if let Some(stderr) = child.stderr.take() {
    tokio::spawn(drain(
      stderr,
      log_tx.clone(),
      config.key.expose().to_string(),
    ));
  }
  drop(log_tx);

  let mut seen = Seen::default();
  let mut pending_notice = None;
  let deadline = tokio::time::Instant::now() + START_BUDGET;
  let connecting = async {
    let mut device_id = None;
    let mut port = None;
    let mut logs_open = true;
    let file_endpoint = || id_and_port_from_files(&config.data_dir, pid);
    let (file_id, file_port) = loop {
      if device_id.is_some() && port.is_some() {
        break file_endpoint();
      }
      tokio::select! {
        biased;
        _ = tokio::time::sleep_until(deadline) => break file_endpoint(),
        event = log_rx.recv(), if logs_open => match event {
          Some(event) => {
            if let Some(notice) = seen.note(&event) {
              // A child exit can cancel this wait for the App lock.
              pending_notice = Some(notice.clone());
              notify(app, config, shared, notice).await;
              pending_notice = None;
            }
            match event {
              LogEvent::Ready { device_id: id } => device_id = Some(id),
              LogEvent::WsListening { port: listening } => port = Some(listening),
              _ => {}
            }
          }
          None => {
            logs_open = false;
            let files = file_endpoint();
            if device_id.as_ref().or(files.0.as_ref()).is_some() && port.or(files.1).is_some() {
              break files;
            }
          }
        },
      }
    };
    if device_id.is_some() && file_id.is_some() && device_id != file_id {
      warn!("soloist: the device id in the log and in .device_id disagree");
    }
    let (device_id, port) = (device_id.or(file_id)?, port.or(file_port)?);
    let socket = tokio::time::timeout(CONNECT_BUDGET, connect(port))
      .await
      .ok()??;
    Some((device_id, port, socket))
  };
  let result = tokio::select! {
    biased;
    _ = shared.stop.notified() => {
      terminate(&mut child).await;
      return Outcome::Exited(Exit::Stopped);
    }
    status = child.wait() => Err(status),
    socket = connecting => Ok(socket),
  };
  let (device_id, port, socket) = match result {
    Ok(Some(connected)) => connected,
    failure => {
      let status = failure.err().or_else(|| child.try_wait().transpose());
      if status.is_none() {
        terminate(&mut child).await;
      }
      // The pipe readers can still have the final error after wait completes.
      let drain_until = deadline.max(tokio::time::Instant::now() + Duration::from_millis(100));
      return classify_tail(
        app,
        config,
        shared,
        None,
        &mut seen,
        &mut log_rx,
        None,
        status,
        drain_until,
        pending_notice,
      )
      .await;
    }
  };
  info!("soloist: running, WebSocket on port {port}");
  let (sink, mut stream) = socket.split();
  let (command_tx, command_rx) = mpsc::unbounded_channel();
  let link = Arc::new(SoloistLink::new(device_id, command_tx));
  {
    let Some(mut app) = current_app(app, config, shared).await else {
      terminate(&mut child).await;
      return Outcome::Exited(Exit::Stopped);
    };
    if let Ok(mut slot) = shared.link.lock() {
      *slot = Some(link.clone());
    }
    app.install_soloist_link(link.clone());
  }
  let writer = tokio::spawn(write_commands(sink, command_rx, Arc::downgrade(&link)));
  if reactivate {
    link.send(Command::Activate);
  }

  let outcome = watch_connected(
    app,
    config,
    shared,
    &link,
    &mut child,
    &mut seen,
    &mut log_rx,
    &mut stream,
    &users_before,
    first_login,
  )
  .await;
  writer.abort();
  if let Ok(mut slot) = shared.link.lock() {
    *slot = None;
  }
  app.lock().await.uninstall_soloist_link(&link);
  let _ = std::fs::remove_file(config.data_dir.join(CHILD_PID));
  remove_stale(&config.data_dir);
  outcome
}

#[allow(clippy::too_many_arguments)]
async fn watch_connected(
  app: &Mutex<App>,
  config: &Config,
  shared: &Shared,
  link: &Arc<SoloistLink>,
  child: &mut Child,
  seen: &mut Seen,
  log_rx: &mut UnboundedReceiver<LogEvent>,
  stream: &mut (dyn Stream<Item = WsResult<Message>> + Send + Unpin),
  users_before: &BTreeSet<String>,
  first_login: &mut bool,
) -> Outcome {
  let mut quality = PendingQuality::default();
  let mut log_open = true;
  let mut seed_restart = false;
  // Soloist writes a new account's prefs just after it reports the login.
  let mut seed_watch = false;
  let offline_at = tokio::time::Instant::now() + OFFLINE_AFTER;
  let mut offline_pending = !users_before.is_empty();
  loop {
    tokio::select! {
      event = log_rx.recv(), if log_open => {
        let Some(event) = event else {
          log_open = false;
          continue;
        };
        if let Some(notice) = seen.note(&event) {
          notify_link(app, config, shared, link, notice).await;
        }
        match event {
          LogEvent::AverageBitrate(bitrate) => quality.bitrate = Some(bitrate),
          LogEvent::Codec(codec) => quality.codec = Some(codec),
          LogEvent::SampleRate(rate) => {
            if let Some(codec) = quality.codec.take() {
              let measured = StreamQuality {
                codec,
                sample_rate: Some(rate),
                bitrate: quality.bitrate.take(),
              };
              info!("soloist: streaming {}", measured.label());
              let previous = link.quality();
              link.set_quality(measured.clone());
              if let Some(mut app) = current_app(app, config, shared).await {
                app.note_soloist_quality(link, previous, &measured);
              }
            }
          }
          _ => {}
        }
      }
      message = stream.next() => match message {
        Some(Ok(Message::Text(text))) => {
          let Some(event) = protocol::parse_event(&text) else {
            continue;
          };
          if let Event::Error(message) = &event {
            debug!("soloist: command refused: {message}");
          }
          let logged_in = confirms_login(&event);
          if logged_in || matches!(event, Event::Auth { logged_in: false, .. }) {
            if let Some(notice) = seen.auth(logged_in) {
              notify_link(app, config, shared, link, notice).await;
            }
          }
          {
            let current = current_app(app, config, shared)
              .await
              .filter(|app| app.is_current_soloist(link));
            let Some(mut app) = current else {
              // A superseded or stopping child writes nothing to the App, but a device
              // change must still cancel its queued pause or deactivation.
              link.apply(&event);
              continue;
            };
            app.apply_soloist_event(link, &event);
            if logged_in && !seen.login_handled {
              seen.login_handled = true;
              offline_pending = false;
              seed_watch = true;
              if std::mem::take(first_login) {
                app.soloist_logged_in(config.startup_behavior, config.startup_shuffle);
              }
            }
          }
          if seed_watch {
            if let Some(restart) = seed_new_accounts(&config.data_dir, config.quality, users_before) {
              seed_watch = false;
              seed_restart = restart;
            }
          }
          let snapshot = link.snapshot();
          if seed_restart && !snapshot.status.is_playing() {
            notify_link(app, config, shared, link, Notice::RestartingForQuality).await;
            terminate(child).await;
            break Outcome::Restart { reactivate: snapshot.is_active };
          }
        }
        Some(Ok(_)) => {}
        Some(Err(_)) | None => {
          warn!("soloist: the WebSocket closed");
          // An exiting child closes the socket first: give it a moment to exit by itself.
          let status = tokio::select! {
            biased;
            _ = shared.stop.notified() => {
              terminate(child).await;
              break Outcome::Exited(Exit::Stopped);
            }
            status = tokio::time::timeout(EXIT_GRACE, child.wait()) => match status {
              Ok(status) => status,
              Err(_) => {
                terminate(child).await;
                child.wait().await
              }
            },
          };
          let until = tokio::time::Instant::now() + EXIT_GRACE;
          break classify_tail(
            app,
            config,
            shared,
            Some(link),
            seen,
            log_rx,
            None,
            Some(status),
            until,
            None,
          )
          .await;
        }
      },
      status = child.wait() => {
        let until = tokio::time::Instant::now() + EXIT_GRACE;
        break classify_tail(
          app,
          config,
          shared,
          Some(link),
          seen,
          log_rx,
          Some(stream),
          Some(status),
          until,
          None,
        )
        .await;
      }
      _ = tokio::time::sleep_until(offline_at), if offline_pending => {
        offline_pending = false;
        notify_link(app, config, shared, link, Notice::Offline).await;
      }
      _ = shared.stop.notified() => {
        terminate(child).await;
        break Outcome::Exited(Exit::Stopped);
      }
    }
  }
}

#[cfg(windows)]
mod job {
  use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
  use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
  };

  /// A job that kills Soloist when spotatui's handle closes, also on a hard kill of spotatui.
  pub struct Job(HANDLE);

  // SAFETY: a job handle is a kernel object handle, valid on any thread.
  unsafe impl Send for Job {}
  unsafe impl Sync for Job {}

  impl Job {
    pub fn contain(child: &tokio::process::Child) -> Option<Job> {
      let process = child.raw_handle()?;
      // SAFETY: plain Win32 calls on a fresh job handle and our own child's handle.
      unsafe {
        let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if handle.is_null() {
          return None;
        }
        let job = Job(handle);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let limited = SetInformationJobObject(
          handle,
          JobObjectExtendedLimitInformation,
          std::ptr::from_ref(&info).cast(),
          std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) != 0;
        (limited && AssignProcessToJobObject(handle, process as HANDLE) != 0).then_some(job)
      }
    }
  }

  impl Drop for Job {
    fn drop(&mut self) {
      // SAFETY: the handle came from CreateJobObjectW and is closed once.
      unsafe {
        CloseHandle(self.0);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn queued_pauses_and_deactivation_do_not_survive_a_handoff() {
    for reactivated in [false, true] {
      let (link, mut commands) = SoloistLink::test_pair("dev");
      link.apply(&Event::Device { is_active: true });
      link.apply(&Event::Playback(Status::Playing));
      link.pause();
      link.send(Command::Deactivate);
      link.apply(&Event::Device { is_active: false });
      if reactivated {
        link.apply(&Event::Device { is_active: true });
        link.pause();
        link.send(Command::Deactivate);
      }
      commands.close();

      let (sink, mut written) = futures::channel::mpsc::unbounded::<Message>();
      write_commands(sink, commands, Arc::downgrade(&link)).await;

      if reactivated {
        for command in [Command::Pause, Command::Deactivate] {
          assert_eq!(
            written.next().await,
            Some(Message::Text(command.to_json().into()))
          );
        }
      }
      assert!(written.next().await.is_none());
    }
  }

  #[test]
  fn shutdown_leaves_playback_on_another_device_untouched() {
    for status in [
      Status::Playing,
      Status::Buffering,
      Status::Paused,
      Status::Idle,
    ] {
      assert!(shutdown_commands(false, status).is_empty(), "{status:?}");
    }
  }

  #[test]
  fn shutdown_pauses_active_soloist_before_deactivating_it() {
    for status in [Status::Playing, Status::Buffering] {
      assert_eq!(
        shutdown_commands(true, status),
        vec![Command::Pause, Command::Deactivate]
      );
    }
  }

  #[test]
  fn shutdown_deactivates_paused_or_idle_soloist_without_pausing_it() {
    for status in [Status::Idle, Status::Paused] {
      assert_eq!(shutdown_commands(true, status), vec![Command::Deactivate]);
    }
  }

  #[test]
  fn a_stop_with_no_supervisor_does_not_stop_the_next_one() {
    let shared = Shared::default();
    shared.stop.notify_one();
    clear_stale_stop(&shared);
    assert!(futures::FutureExt::now_or_never(shared.stop.notified()).is_none());
  }

  #[test]
  fn the_seed_waits_for_the_account_prefs_then_restarts_once() {
    let dir = tempfile::tempdir().unwrap();
    let before = BTreeSet::new();
    assert_eq!(seed_new_accounts(dir.path(), 5, &before), None);

    let user = dir.path().join("settings").join("Users").join("someone");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("prefs"), "app.first=1\n").unwrap();

    assert_eq!(seed_new_accounts(dir.path(), 5, &before), Some(true));
    let prefs = std::fs::read_to_string(user.join("prefs")).unwrap();
    assert!(
      prefs.contains("audio.play_bitrate_enumeration=5\n"),
      "{prefs}"
    );
    assert_eq!(seed_new_accounts(dir.path(), 5, &before), Some(false));
  }

  #[cfg(unix)]
  fn fake_soloist(dir: &Path, body: &str) -> Config {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join("soloist");
    std::fs::write(
      &path,
      format!("#!/bin/sh\nwhile [ \"$1\" != -D ]; do shift; done\ndata=$2\n{body}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    test_config(path, dir)
  }

  fn test_config(path: PathBuf, dir: &Path) -> Config {
    Config {
      generation: 0,
      path,
      device_name: "spotatui".to_string(),
      key: super::super::auth::key_with(Some("test-key".to_string()), None).unwrap(),
      data_dir: dir.join("data"),
      cache_dir: dir.join("cache"),
      quality: 5,
      initial_volume: 50,
      startup_behavior: None,
      startup_shuffle: false,
    }
  }

  fn fake_config() -> Config {
    test_config(PathBuf::from("soloist"), Path::new("unused"))
  }

  #[cfg(unix)]
  async fn wait_until(mut ready: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
      while !ready().await {
        tokio::time::sleep(Duration::from_millis(10)).await;
      }
    })
    .await
    .expect("the mock Soloist did not reach the expected state");
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_classifies_a_child_that_closes_its_logs_before_exiting() {
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      "exec 1>&- 2>&-\ntouch \"$data/closed\"\n\
       while [ ! -f \"$data/release\" ]; do sleep 0.01; done\nexit 10",
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let run = run_once(&app, &config, &shared, false, &mut first_login);
    tokio::pin!(run);
    tokio::select! {
      _ = wait_until(async || config.data_dir.join("closed").is_file()) => {}
      _ = &mut run => panic!("startup ended before the child exited"),
    }
    assert!(tokio::time::timeout(Duration::from_millis(100), &mut run)
      .await
      .is_err());
    std::fs::write(config.data_dir.join("release"), "").unwrap();

    let outcome = tokio::time::timeout(Duration::from_secs(5), run)
      .await
      .expect("startup did not observe the exit");
    assert!(matches!(outcome, Outcome::Exited(Exit::Expired)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_uses_the_final_log_line_to_classify_a_busy_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      "(sleep 0.15; echo 'another session is running for data directory' >&2) &\nexit 1",
    );
    let app = Arc::new(Mutex::new(App::default()));
    let outcome = run_once(&app, &config, &Shared::default(), false, &mut true).await;
    assert!(matches!(outcome, Outcome::Exited(Exit::DataDirInUse)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_reports_a_rejected_key_from_the_tail_instead_of_restarting() {
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      "(sleep 0.15; echo 'login failed' >&2) &\nexit 1",
    );
    let app = Arc::new(Mutex::new(App::default()));
    tokio::time::timeout(
      Duration::from_secs(2),
      supervise(&app, &config, &Shared::default()),
    )
    .await
    .expect("a rejected key should end startup");
    let app = app.lock().await;
    assert_eq!(
      app.status_message(),
      Some(app.strings().status.infra.soloist_bad_key)
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_shows_expiry_and_pairing_before_the_websocket_is_ready() {
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      "echo 'client expires in 3 days'\n\
       while [ ! -f \"$data/release\" ]; do sleep 0.01; done\n\
       echo 'waiting for login'\nexec sleep 30",
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let strings = app.lock().await.strings().status.infra;
    let expected = strings.soloist_expires_soon(3, strings.soloist_expiry_unit("days"));
    let observe = async {
      wait_until(async || app.lock().await.status_message() == Some(expected.as_str())).await;
      std::fs::write(config.data_dir.join("release"), "").unwrap();
      wait_until(async || app.lock().await.soloist_pairing_device().is_some()).await;
      shared.stop.notify_one();
    };

    let (outcome, ()) = tokio::join!(
      run_once(&app, &config, &shared, false, &mut first_login),
      observe,
    );
    assert!(matches!(outcome, Outcome::Exited(Exit::Stopped)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_observes_a_child_exit_during_the_websocket_handshake() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      &format!(
        "echo 'ready, device_name=spotatui device_id=test-device'\n\
         echo 'websocket server listening on 127.0.0.1:{port}'\n\
         while [ ! -f \"$data/release\" ]; do sleep 0.01; done\nexit 10"
      ),
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let run = run_once(&app, &config, &shared, false, &mut first_login);
    tokio::pin!(run);
    let (_stream, _) = tokio::select! {
      accepted = tokio::time::timeout(Duration::from_secs(5), listener.accept()) => accepted.unwrap().unwrap(),
      _ = &mut run => panic!("startup ended before it opened the socket"),
    };
    std::fs::write(config.data_dir.join("release"), "").unwrap();

    let outcome = tokio::time::timeout(Duration::from_secs(5), run)
      .await
      .expect("startup did not observe the exit");
    assert!(matches!(outcome, Outcome::Exited(Exit::Expired)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_uses_the_current_childs_files_when_the_logs_close() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      &format!(
        "echo $$ > \"$data/soloist.pid\"\necho test-device > \"$data/.device_id\"\n\
         echo {port} > \"$data/ws.port\"\nexec 1>&- 2>&-\nexec sleep 30"
      ),
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let observe = async {
      let (stream, _) = listener.accept().await.unwrap();
      let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
      wait_until(async || {
        app
          .lock()
          .await
          .soloist_link()
          .is_some_and(|link| link.device_id() == "test-device")
      })
      .await;
      shared.stop.notify_one();
      socket
    };
    let (outcome, _socket) = tokio::time::timeout(Duration::from_secs(5), async {
      tokio::join!(
        run_once(&app, &config, &shared, false, &mut first_login),
        observe
      )
    })
    .await
    .expect("startup did not connect from the current child's files");
    assert!(matches!(outcome, Outcome::Exited(Exit::Stopped)));
  }

  #[test]
  fn a_pairing_log_after_login_does_not_reopen_the_prompt() {
    let mut seen = Seen {
      logged_in: true,
      ..Seen::default()
    };
    assert_eq!(seen.note(&LogEvent::WaitingForLogin), None);
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn startup_drops_notices_and_failure_from_a_superseded_child() {
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      "touch \"$data/started\"\n\
       while [ ! -f \"$data/release\" ]; do sleep 0.01; done\n\
       echo 'waiting for login'\necho 'login failed'\nexit 1",
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let replace = async {
      wait_until(async || config.data_dir.join("started").is_file()).await;
      let mut app = app.lock().await;
      app.begin_soloist_start(None, false);
      app.set_status_message("new startup", 30);
      std::fs::write(config.data_dir.join("release"), "").unwrap();
    };
    tokio::time::timeout(Duration::from_secs(6), async {
      tokio::join!(supervise(&app, &config, &shared), replace)
    })
    .await
    .expect("the superseded child did not exit");

    let app = app.lock().await;
    assert!(app.native_backend_pending);
    assert_eq!(app.status_message(), Some("new startup"));
    assert!(app.soloist_pairing_device().is_none());
  }
  #[cfg(unix)]
  fn handshake_script(port: u16, after: &str) -> String {
    format!(
      "echo 'ready, device_name=spotatui device_id=test-device'\n\
       echo 'websocket server listening on 127.0.0.1:{port}'\n{after}"
    )
  }

  #[cfg(unix)]
  async fn accept_soloist(
    listener: &tokio::net::TcpListener,
  ) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    let (stream, _) = listener.accept().await.unwrap();
    tokio_tungstenite::accept_async(stream).await.unwrap()
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_superseded_childs_device_change_still_cancels_its_queued_pause() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(dir.path(), &handshake_script(port, "exec sleep 30"));
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let observe = async {
      let mut socket = accept_soloist(&listener).await;
      wait_until(async || app.lock().await.soloist_link().is_some()).await;
      let link = app.lock().await.soloist_link().unwrap();
      let device = |active: bool| {
        Message::Text(format!(r#"{{"type":"device_changed","is_active":{active}}}"#).into())
      };
      socket.send(device(true)).await.unwrap();
      wait_until(async || link.snapshot().is_active).await;
      // The user saves a new engine: this child is superseded, then the phone takes over.
      app.lock().await.begin_soloist_start(None, false);
      socket.send(device(false)).await.unwrap();
      wait_until(async || !link.snapshot().is_active).await;
      shared.stop.notify_one();
      socket
    };
    let (outcome, _socket) = tokio::time::timeout(Duration::from_secs(8), async {
      tokio::join!(
        run_once(&app, &config, &shared, false, &mut first_login),
        observe
      )
    })
    .await
    .expect("the superseded child's link missed the device change");
    assert!(matches!(outcome, Outcome::Exited(Exit::Stopped)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_superseded_start_does_not_install_its_link_after_the_handshake() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(dir.path(), &handshake_script(port, "exec sleep 30"));
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let observe = async {
      let (stream, _) = listener.accept().await.unwrap();
      app.lock().await.begin_soloist_start(None, false);
      tokio_tungstenite::accept_async(stream).await.unwrap()
    };
    let (outcome, _socket) = tokio::time::timeout(Duration::from_secs(8), async {
      tokio::join!(
        run_once(&app, &config, &shared, false, &mut first_login),
        observe
      )
    })
    .await
    .expect("the superseded child did not stop");
    assert!(matches!(outcome, Outcome::Exited(Exit::Stopped)));
    let app = app.lock().await;
    assert!(app.soloist_link().is_none());
    assert!(app.native_backend_pending);
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn an_exit_right_after_the_handshake_is_classified_from_the_tail() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      &handshake_script(
        port,
        "while [ ! -f \"$data/release\" ]; do sleep 0.01; done\n\
         (sleep 0.15; echo 'login failed' >&2) &\nexit 1",
      ),
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let observe = async {
      let socket = accept_soloist(&listener).await;
      wait_until(async || app.lock().await.soloist_link().is_some()).await;
      std::fs::write(config.data_dir.join("release"), "").unwrap();
      socket
    };
    let (outcome, _socket) = tokio::time::timeout(Duration::from_secs(8), async {
      tokio::join!(
        run_once(&app, &config, &shared, false, &mut first_login),
        observe
      )
    })
    .await
    .expect("the child exit was not observed");
    assert!(matches!(outcome, Outcome::Fatal(Notice::BadKey)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_socket_closed_by_an_exiting_child_waits_for_its_exit_code() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      &handshake_script(
        port,
        "while [ ! -f \"$data/release\" ]; do sleep 0.01; done\n\
         while [ ! -f \"$data/closed\" ]; do sleep 0.01; done\n\
         sleep 0.3\nexit 10",
      ),
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    // The child stays alive until the socket is gone, then exits with the expiry code: only a
    // supervisor that waits for the exit code sees `Expired`, a kill reads as a crash.
    let observe = async {
      let socket = accept_soloist(&listener).await;
      wait_until(async || app.lock().await.soloist_link().is_some()).await;
      std::fs::write(config.data_dir.join("release"), "").unwrap();
      drop(socket);
      std::fs::write(config.data_dir.join("closed"), "").unwrap();
    };
    let (outcome, ()) = tokio::time::timeout(Duration::from_secs(8), async {
      tokio::join!(
        run_once(&app, &config, &shared, false, &mut first_login),
        observe
      )
    })
    .await
    .expect("the child exit was not observed");
    assert!(matches!(outcome, Outcome::Exited(Exit::Expired)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_notice_from_a_replaced_link_is_dropped() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let config = fake_soloist(
      dir.path(),
      &handshake_script(
        port,
        "while [ ! -f \"$data/release\" ]; do sleep 0.01; done\n\
         echo 'login failed'\ntouch \"$data/said\"\nexec sleep 30",
      ),
    );
    let app = Arc::new(Mutex::new(App::default()));
    let shared = Shared::default();
    let mut first_login = true;
    let observe = async {
      let socket = accept_soloist(&listener).await;
      wait_until(async || app.lock().await.soloist_link().is_some()).await;
      let (other, _rx) = SoloistLink::test_pair("other-device");
      app.lock().await.install_soloist_link(other);
      std::fs::write(config.data_dir.join("release"), "").unwrap();
      wait_until(async || config.data_dir.join("said").is_file()).await;
      tokio::time::sleep(Duration::from_millis(200)).await;
      shared.stop.notify_one();
      socket
    };
    let (outcome, _socket) = tokio::time::timeout(Duration::from_secs(8), async {
      tokio::join!(
        run_once(&app, &config, &shared, false, &mut first_login),
        observe
      )
    })
    .await
    .expect("the child did not stop");
    assert!(matches!(outcome, Outcome::Exited(Exit::Stopped)));
    let app = app.lock().await;
    assert_ne!(
      app.status_message(),
      Some(app.strings().status.infra.soloist_bad_key)
    );
  }

  #[tokio::test]
  async fn the_tail_is_classified_before_its_notices_wait_for_the_app() {
    let app = Arc::new(Mutex::new(App::default()));
    let config = fake_config();
    let shared = Shared::default();
    let (log_tx, mut log_rx) = mpsc::unbounded_channel();
    log_tx.send(LogEvent::WaitingForLogin).unwrap();
    log_tx.send(LogEvent::LoginFailed).unwrap();
    drop(log_tx);
    let held = app.clone().lock_owned().await;
    let release = async {
      tokio::time::sleep(Duration::from_millis(300)).await;
      drop(held);
    };
    let mut seen = Seen::default();
    let until = tokio::time::Instant::now() + Duration::from_millis(100);
    let (outcome, ()) = tokio::join!(
      classify_tail(
        &app,
        &config,
        &shared,
        None,
        &mut seen,
        &mut log_rx,
        None,
        None,
        until,
        None
      ),
      release,
    );
    assert!(matches!(outcome, Outcome::Fatal(Notice::BadKey)));
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn a_login_queued_on_the_websocket_keeps_an_exit_restartable() {
    use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};

    let mut child = ProcessCommand::new("/bin/sh")
      .args(["-c", "exit 1"])
      .spawn()
      .unwrap();
    assert_eq!(child.wait().await.unwrap().code(), Some(1));
    let (client, server) = tokio::io::duplex(1024);
    let client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
    let mut server = WebSocketStream::from_raw_socket(server, Role::Server, None).await;
    for logged_in in [false, true] {
      server
        .send(Message::Text(
          format!(r#"{{"type":"auth_state","logged_in":{logged_in}}}"#).into(),
        ))
        .await
        .unwrap();
    }
    drop(server);

    // Leave the log receiver pending and delay the socket's first read result.
    // The already-ready child exit must win the connected loop's select.
    let (_log_tx, mut log_rx) = mpsc::unbounded_channel();
    let mut stream = Box::pin(client.then(|message| async {
      tokio::task::yield_now().await;
      message
    }));
    let mut seen = Seen::default();
    seen.note(&LogEvent::LoginFailed);
    let (link, _commands) = SoloistLink::test_pair("test-device");
    let mut app = App::default();
    app.install_soloist_link(link.clone());
    let app = Mutex::new(app);
    let mut first_login = true;
    let outcome = watch_connected(
      &app,
      &fake_config(),
      &Shared::default(),
      &link,
      &mut child,
      &mut seen,
      &mut log_rx,
      &mut stream,
      &BTreeSet::new(),
      &mut first_login,
    )
    .await;
    assert!(matches!(outcome, Outcome::Exited(Exit::Crashed(Some(1)))));
    assert!(seen.ever_logged_in);
    assert!(
      !seen.login_handled,
      "the exit path must read the login frame"
    );
    let app = app.lock().await;
    assert_ne!(
      app.status_message(),
      Some(app.strings().status.infra.soloist_bad_key)
    );
  }

  #[test]
  fn a_login_failure_log_after_confirmed_login_does_not_report_a_bad_key() {
    for logged_out in [false, true] {
      let mut seen = Seen::default();
      seen.auth(true);
      if logged_out {
        seen.auth(false);
      }
      assert_eq!(seen.note(&LogEvent::LoginFailed), None);
    }
  }

  #[test]
  fn a_pairing_line_read_just_before_the_logout_still_shows_the_prompt() {
    // The other order, logout then line, is `a_logout_lets_the_pairing_prompt_show_again`.
    let mut seen = Seen::default();
    seen.auth(true);
    assert_eq!(seen.note(&LogEvent::WaitingForLogin), None);
    assert_eq!(seen.auth(false), Some(Notice::NotPaired));
    assert_eq!(seen.note(&LogEvent::WaitingForLogin), None);
  }

  #[cfg(unix)]
  #[test]
  fn a_crash_after_a_login_is_not_a_rejected_key() {
    use std::os::unix::process::ExitStatusExt;
    let shared = Shared::default();
    let crashed = || Some(Ok(std::process::ExitStatus::from_raw(1 << 8)));
    let mut seen = Seen::default();
    assert_eq!(seen.note(&LogEvent::LoginFailed), Some(Notice::BadKey));
    assert!(matches!(
      seen.startup_failure(crashed(), &shared),
      Outcome::Fatal(Notice::BadKey)
    ));
    seen.auth(true);
    seen.auth(false);
    assert!(matches!(
      seen.startup_failure(crashed(), &shared),
      Outcome::Exited(Exit::Crashed(Some(1)))
    ));
  }

  #[test]
  fn a_logout_lets_the_pairing_prompt_show_again() {
    let mut seen = Seen::default();
    assert_eq!(
      seen.note(&LogEvent::WaitingForLogin),
      Some(Notice::NotPaired)
    );
    seen.auth(true);
    assert_eq!(seen.auth(false), None);
    assert_eq!(
      seen.note(&LogEvent::WaitingForLogin),
      Some(Notice::NotPaired)
    );
  }

  #[tokio::test]
  async fn an_engine_switch_gives_up_while_quit_waits_for_the_soloist_child() {
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;

    let shared = Shared::default();
    let (done_tx, done_rx) = oneshot::channel();
    *shared.done.lock().unwrap() = Some(done_rx);
    shared.running.store(true, Ordering::SeqCst);

    let (io_tx, io_rx) = std::sync::mpsc::channel();
    let mut app = App::new(io_tx, UserConfig::new(), None);
    app.user_config.behavior.enable_streaming = true;
    let (recovery_tx, mut recovery_rx) = mpsc::unbounded_channel();
    app.stand_by_librespot(recovery_tx);
    app.playback_engine_changed();
    let Ok(IoEvent::ApplyPlaybackEngine(request)) = io_rx.try_recv() else {
      panic!("expected an engine switch request");
    };
    let app = Arc::new(Mutex::new(app));

    let quit = stop_on(&shared, Duration::from_secs(5));
    tokio::pin!(quit);
    assert!(futures::poll!(&mut quit).is_pending());
    assert!(shared.done.lock().unwrap().is_none());

    tokio::time::timeout(
      Duration::from_secs(2),
      super::super::startup::apply_engine_on(Arc::clone(&app), request, &shared),
    )
    .await
    .expect("the switch should give up before the child exits");

    assert!(
      matches!(
        recovery_rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
      ),
      "the engine switch requested a librespot rebuild during quit"
    );
    assert!(shared.shutting_down.load(Ordering::SeqCst));

    shared.running.store(false, Ordering::SeqCst);
    done_tx.send(()).unwrap();
    quit.await;
  }

  #[tokio::test]
  async fn a_timed_out_stop_leaves_the_exit_signal_for_wait_stopped() {
    let shared = Arc::new(Shared::default());
    let (done_tx, done_rx) = oneshot::channel();
    *shared.done.lock().unwrap() = Some(done_rx);
    shared.running.store(true, Ordering::SeqCst);
    let supervisor = {
      let shared = shared.clone();
      tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        shared.running.store(false, Ordering::SeqCst);
        let _ = done_tx.send(());
      })
    };

    stop_on(&shared, Duration::from_millis(20)).await;
    assert!(shared.running.load(Ordering::SeqCst));
    tokio::time::timeout(Duration::from_secs(2), wait_stopped_on(&shared))
      .await
      .unwrap();
    assert!(!shared.running.load(Ordering::SeqCst));
    supervisor.await.unwrap();
  }
}
