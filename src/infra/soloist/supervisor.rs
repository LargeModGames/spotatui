//! Starts, watches and stops the Soloist child, and bridges its WebSocket to the App.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use futures::{Sink, SinkExt, StreamExt};
use log::{debug, info, warn};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command as ProcessCommand};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::{oneshot, Mutex, Notify};
use tokio_tungstenite::tungstenite::Message;

use super::auth::ApiKey;
use super::link::{SoloistLink, StreamQuality, VolumeGate};
use super::logline::{self, LogEvent};
use super::process::{self, Exit, SpawnFailure, SpawnPlan};
use super::protocol::{self, Command, Event};
use super::{prefs, Notice};
use crate::core::app::App;
use crate::core::user_config::StartupBehavior;

pub struct Config {
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
struct Shared {
  shutting_down: AtomicBool,
  running: AtomicBool,
  stop: Notify,
  link: StdMutex<Option<Arc<SoloistLink>>>,
  done: StdMutex<Option<oneshot::Receiver<()>>>,
}

fn shared() -> &'static Arc<Shared> {
  static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
  SHARED.get_or_init(Arc::default)
}

/// Soloist's pid as spotatui spawned it, to tell an orphan from a Soloist the user runs.
const CHILD_PID: &str = ".spotatui-child.pid";
const STALE_FILES: [&str; 3] = ["ws.addr", "ws.port", "soloist.pid"];
const START_BUDGET: Duration = Duration::from_secs(20);
const OFFLINE_AFTER: Duration = Duration::from_secs(15);
#[cfg(windows)]
pub(super) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn start(app: Arc<Mutex<App>>, config: Config) {
  let shared = shared().clone();
  if shared.shutting_down.load(Ordering::SeqCst) {
    return;
  }
  clear_stale_stop(&shared);
  shared.running.store(true, Ordering::SeqCst);
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

/// Pause, give up the Connect device, and stop the child, waiting at most `timeout`.
pub async fn stop(timeout: Duration) {
  let shared = shared();
  shared.shutting_down.store(true, Ordering::SeqCst);
  let link = shared.link.lock().ok().and_then(|link| link.clone());
  if let Some(link) = link {
    link.pause();
    link.send(Command::Deactivate);
    tokio::time::sleep(Duration::from_millis(300)).await;
  }
  shared.stop.notify_one();
  let done = shared.done.lock().ok().and_then(|mut done| done.take());
  if let Some(done) = done {
    let _ = tokio::time::timeout(timeout, done).await;
  }
}

/// `stop`, after which `start` works again; true when no supervisor runs any more.
pub async fn stop_for_restart(timeout: Duration) -> bool {
  stop(timeout).await;
  shared().shutting_down.store(false, Ordering::SeqCst);
  !running()
}

enum Outcome {
  Exited(Exit),
  Fatal(Notice),
  Restart { reactivate: bool },
}

async fn notify(app: &Arc<Mutex<App>>, notice: Notice) {
  app.lock().await.soloist_notice(notice);
}

async fn supervise(app: &Arc<Mutex<App>>, config: &Config, shared: &Shared) {
  let mut exits = Vec::new();
  let mut retried_dir = false;
  let mut reactivate = false;
  let mut first_login = true;
  loop {
    if shared.shutting_down.load(Ordering::SeqCst) {
      break;
    }
    match run_once(app, config, shared, reactivate, &mut first_login).await {
      Outcome::Restart { reactivate: again } => reactivate = again,
      Outcome::Fatal(notice) => {
        app.lock().await.soloist_failed(Some(notice));
        break;
      }
      Outcome::Exited(exit) => {
        reactivate = false;
        match exit {
          Exit::Stopped => break,
          Exit::Expired => {
            app.lock().await.soloist_failed(Some(Notice::Expired));
            break;
          }
          Exit::DataDirInUse if !retried_dir => {
            retried_dir = true;
            clear_orphan(config).await;
          }
          Exit::DataDirInUse => {
            app.lock().await.soloist_failed(Some(Notice::DirInUse));
            break;
          }
          Exit::Crashed(code) => {
            warn!("soloist: exited with {code:?}");
            let now = Instant::now();
            exits.push(now);
            let Some(delay) = process::restart_delay(&exits, now) else {
              app.lock().await.soloist_failed(Some(Notice::KeepsExiting));
              break;
            };
            {
              let mut app = app.lock().await;
              app.soloist_starting();
              app.soloist_notice(Notice::Restarting);
            }
            tokio::select! {
              _ = tokio::time::sleep(delay) => {}
              _ = shared.stop.notified() => break,
            }
          }
        }
      }
    }
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

async fn write_commands<S>(mut sink: S, mut commands: UnboundedReceiver<Command>)
where
  S: Sink<Message> + Unpin,
{
  let mut gate = VolumeGate::default();
  loop {
    let deadline = gate.deadline();
    let command = tokio::select! {
      command = commands.recv() => match command {
        Some(Command::SetVolume(volume)) => match gate.offer(volume, Instant::now()) {
          Some(volume) => Command::SetVolume(volume),
          None => continue,
        },
        Some(command) => command,
        None => break,
      },
      _ = sleep_until(deadline) => match gate.due(Instant::now()) {
        Some(volume) => Command::SetVolume(volume),
        None => continue,
      },
    };
    if sink
      .send(Message::Text(command.to_json().into()))
      .await
      .is_err()
    {
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
  logged_in: bool,
  pairing_shown: bool,
  expiry_warned: bool,
}

impl Seen {
  fn note(&mut self, event: &LogEvent) {
    match event {
      LogEvent::AnotherSession => self.another_session = true,
      LogEvent::Expired => self.expired = true,
      _ => {}
    }
  }

  fn exit(&self, status: std::io::Result<std::process::ExitStatus>, shared: &Shared) -> Exit {
    process::classify_exit(
      status.ok().and_then(|status| status.code()),
      shared.shutting_down.load(Ordering::SeqCst),
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
    tokio::spawn(drain(stderr, log_tx, config.key.expose().to_string()));
  }

  let mut seen = Seen::default();
  let mut device_id = None;
  let mut port = None;
  let deadline = tokio::time::Instant::now() + START_BUDGET;
  while device_id.is_none() || port.is_none() {
    tokio::select! {
      event = log_rx.recv() => match event {
        Some(event) => {
          seen.note(&event);
          match event {
            LogEvent::Ready { device_id: id } => device_id = Some(id),
            LogEvent::WsListening { port: listening } => port = Some(listening),
            _ => {}
          }
        }
        None => break,
      },
      status = child.wait() => return Outcome::Exited(seen.exit(status, shared)),
      _ = tokio::time::sleep_until(deadline) => break,
      _ = shared.stop.notified() => {
        terminate(&mut child).await;
        return Outcome::Exited(Exit::Stopped);
      }
    }
  }
  let (file_id, file_port) = id_and_port_from_files(&config.data_dir, pid);
  if device_id.is_some() && file_id.is_some() && device_id != file_id {
    warn!("soloist: the device id in the log and in .device_id disagree");
  }
  let (Some(device_id), Some(port)) = (device_id.or(file_id), port.or(file_port)) else {
    terminate(&mut child).await;
    return Outcome::Fatal(Notice::NoWebSocket);
  };
  let Some(socket) = connect(port).await else {
    terminate(&mut child).await;
    return Outcome::Fatal(Notice::NoWebSocket);
  };
  info!("soloist: running, WebSocket on port {port}");
  let (sink, mut stream) = socket.split();
  let (command_tx, command_rx) = mpsc::unbounded_channel();
  let writer = tokio::spawn(write_commands(sink, command_rx));
  let link = Arc::new(SoloistLink::new(device_id, command_tx));
  if let Ok(mut slot) = shared.link.lock() {
    *slot = Some(link.clone());
  }
  app.lock().await.install_soloist_link(link.clone());
  if reactivate {
    link.send(Command::Activate);
  }

  let mut quality = PendingQuality::default();
  let mut log_open = true;
  let mut seed_restart = false;
  // Soloist writes a new account's prefs just after it reports the login.
  let mut seed_watch = false;
  let offline_at = tokio::time::Instant::now() + OFFLINE_AFTER;
  let mut offline_pending = !users_before.is_empty();
  let outcome = loop {
    tokio::select! {
      event = log_rx.recv(), if log_open => {
        let Some(event) = event else {
          log_open = false;
          continue;
        };
        seen.note(&event);
        match event {
          LogEvent::WaitingForLogin if !seen.pairing_shown => {
            seen.pairing_shown = true;
            notify(app, Notice::NotPaired).await;
          }
          LogEvent::LoginFailed => notify(app, Notice::BadKey).await,
          LogEvent::ExpiresIn { count, unit } if !seen.expiry_warned && expires_soon(count, &unit) => {
            seen.expiry_warned = true;
            notify(app, Notice::ExpiresSoon { count, unit }).await;
          }
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
              app.lock().await.note_soloist_quality(&link, previous, &measured);
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
          let logged_in = matches!(event, Event::Auth { logged_in: true, .. } | Event::State(_));
          app.lock().await.apply_soloist_event(&link, &event);
          if logged_in && !seen.logged_in {
            seen.logged_in = true;
            offline_pending = false;
            seed_watch = true;
            if std::mem::take(first_login) {
              app
                .lock()
                .await
                .soloist_logged_in(config.startup_behavior, config.startup_shuffle);
            }
          }
          if seed_watch {
            if let Some(restart) = seed_new_accounts(&config.data_dir, config.quality, &users_before) {
              seed_watch = false;
              seed_restart = restart;
            }
          }
          let snapshot = link.snapshot();
          if seed_restart && !snapshot.status.is_playing() {
            notify(app, Notice::RestartingForQuality).await;
            terminate(&mut child).await;
            break Outcome::Restart { reactivate: snapshot.is_active };
          }
        }
        Some(Ok(_)) => {}
        Some(Err(_)) | None => {
          warn!("soloist: the WebSocket closed");
          terminate(&mut child).await;
          break Outcome::Exited(Exit::Crashed(None));
        }
      },
      status = child.wait() => break Outcome::Exited(seen.exit(status, shared)),
      _ = tokio::time::sleep_until(offline_at), if offline_pending => {
        offline_pending = false;
        notify(app, Notice::Offline).await;
      }
      _ = shared.stop.notified() => {
        terminate(&mut child).await;
        break Outcome::Exited(Exit::Stopped);
      }
    }
  };
  writer.abort();
  if let Ok(mut slot) = shared.link.lock() {
    *slot = None;
  }
  app.lock().await.uninstall_soloist_link(&link);
  let _ = std::fs::remove_file(config.data_dir.join(CHILD_PID));
  remove_stale(&config.data_dir);
  outcome
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
}
