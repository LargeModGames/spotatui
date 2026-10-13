// No console window next to the app window in a release build on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! The spotatui desktop app: a Tauri window around the GUI's loopback page.

mod updater;

use spotatui::{UpdateLink, UpdateStatus};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use updater::Pending;

/// How long a quit waits for the app to save the session and stop playback.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(10);

/// What the event loop and the app thread know of each other.
#[derive(Default)]
struct Life {
  /// Boot is done and the page is served: a quit must wait for a clean shutdown.
  serving: AtomicBool,
  /// The app thread finished its shutdown.
  done: AtomicBool,
  /// `OPEN`, then `INSTALLING` or `GAVE_UP`, never both.
  install: AtomicU8,
}

const OPEN: u8 = 0;
const INSTALLING: u8 = 1;
const GAVE_UP: u8 = 2;

impl Life {
  fn begin_install(&self) -> bool {
    self
      .install
      .compare_exchange(OPEN, INSTALLING, SeqCst, SeqCst)
      .is_ok()
  }

  /// False when an install already runs: stopping it would leave a half-written app.
  fn give_up(&self) -> bool {
    self
      .install
      .compare_exchange(OPEN, GAVE_UP, SeqCst, SeqCst)
      .is_ok()
  }
}

/// Where Homebrew puts yt-dlp and ffmpeg; an app opened from Finder gets a PATH without them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const MACOS_INSTALL_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// `path` with each of `dirs` it lacks added at the end.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn with_dirs(path: Option<std::ffi::OsString>, dirs: &[&str]) -> std::ffi::OsString {
  let mut paths: Vec<std::path::PathBuf> = path
    .as_deref()
    .map(|path| std::env::split_paths(path).collect())
    .unwrap_or_default();
  for dir in dirs {
    if !paths.iter().any(|known| known == std::path::Path::new(dir)) {
      paths.push(dir.into());
    }
  }
  std::env::join_paths(paths).unwrap_or_else(|_| path.unwrap_or_default())
}

fn main() -> anyhow::Result<()> {
  // WebKitGTK's DMA-BUF renderer draws a blank window on some GPU drivers.
  #[cfg(target_os = "linux")]
  if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
  }
  #[cfg(target_os = "macos")]
  std::env::set_var(
    "PATH",
    with_dirs(std::env::var_os("PATH"), &MACOS_INSTALL_DIRS),
  );
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
  tauri::async_runtime::set(runtime.handle().clone());
  let life = Arc::new(Life::default());
  let (close_tx, closed) = tokio::sync::watch::channel(false);
  let (done_tx, done_rx) = mpsc::channel::<()>();
  let handle = runtime.handle().clone();
  let worker_life = Arc::clone(&life);
  let (status, status_rx) = tokio::sync::watch::channel(UpdateStatus::Idle);
  let (requests, request_rx) = tokio::sync::mpsc::unbounded_channel();
  let pending = Arc::new(Pending::default());
  let updates = UpdateLink {
    status: status_rx,
    requests,
  };
  let worker_pending = Arc::clone(&pending);
  let app = tauri::Builder::default()
    // First plugin: a second launch focuses this window and exits before it boots.
    .plugin(tauri_plugin_single_instance::init(|app, _, _| {
      if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
      }
    }))
    .plugin(tauri_plugin_updater::Builder::new().build())
    .setup(move |app| {
      let app = app.handle().clone();
      tauri::async_runtime::spawn(updater::serve(app.clone(), status, request_rx, pending));
      // `ask_page` blocks its thread, so the app runs on a thread of its own, not a runtime worker.
      std::thread::Builder::new()
        .name("spotatui".into())
        .stack_size(16 << 20)
        .spawn(move || {
          worker(
            app,
            handle,
            worker_life,
            closed,
            done_tx,
            updates,
            worker_pending,
          )
        })?;
      Ok(())
    })
    .build(tauri::generate_context!())?;
  let code = app.run_return(move |_, event| {
    if let RunEvent::Exit = event {
      if must_wait(life.serving.load(SeqCst), life.done.load(SeqCst)) {
        let _ = close_tx.send(true);
        if done_rx.recv_timeout(SHUTDOWN_LIMIT).is_err() && !life.give_up() {
          let _ = done_rx.recv();
        }
      }
    }
  });
  // No join: an app thread stuck in boot must not keep the process alive.
  std::process::exit(code);
}

/// A quit waits for the app only while it serves and has not shut down yet.
fn must_wait(serving: bool, done: bool) -> bool {
  serving && !done
}

fn worker(
  app: AppHandle,
  runtime: tokio::runtime::Handle,
  life: Arc<Life>,
  closed: tokio::sync::watch::Receiver<bool>,
  done: mpsc::Sender<()>,
  updates: UpdateLink,
  pending: Arc<Pending>,
) {
  let _guard = ExitOnPanic(app.clone());
  let show_app = app.clone();
  let window_app = app.clone();
  let restore = Restore::default();
  let serving_life = Arc::clone(&life);
  let host = spotatui::GuiHost {
    show: Box::new(move |url| show_main(&show_app, url)),
    serving: Box::new(move || serving_life.serving.store(true, SeqCst)),
    closed,
    updates: Some(updates),
    window: Some(Box::new(move |mode| set_window(&window_app, mode, &restore))),
  };
  let result = runtime.block_on(spotatui::run_gui_hosted(host));
  // Before `done`: a quit from the window ends the process right after it.
  let restart = result.is_ok() && pending.is_ready() && life.begin_install() && pending.install();
  life.done.store(true, SeqCst);
  let _ = done.send(());
  match result {
    Ok(()) if restart => app.request_restart(),
    Ok(()) => app.exit(0),
    Err(e) => show_error(&app, &format!("{e:#}")),
  }
}

/// Opens the main window on the loopback page; it never leaves that origin.
fn show_main(app: &AppHandle, url: &str) -> anyhow::Result<()> {
  let page: tauri::Url = url.parse()?;
  let origin = (page.host_str().map(str::to_string), page.port());
  WebviewWindowBuilder::new(app, "main", WebviewUrl::External(page))
    .title("spotatui")
    .inner_size(1280.0, 800.0)
    .min_inner_size(800.0, 600.0)
    .on_navigation(move |to| (to.host_str().map(str::to_string), to.port()) == origin)
    .build()?;
  Ok(())
}

/// The main window's place before it shrank to the mini player: position, inner size, maximized.
type Restore = std::sync::Mutex<Option<(tauri::PhysicalPosition<i32>, tauri::PhysicalSize<u32>, bool)>>;

/// The mini player's window size, in logical pixels.
const MINI_SIZE: (f64, f64) = (360.0, 420.0);

/// Shrinks the main window to the mini player and keeps it on top, or puts it back.
fn set_window(app: &AppHandle, mode: spotatui::WindowMode, restore: &Restore) {
  let Some(window) = app.get_webview_window("main") else {
    return;
  };
  let mut saved = restore.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let result = match mode {
    spotatui::WindowMode::Mini => (|| {
      if saved.is_none() {
        // Unmaximized first, so the saved bounds are the normal ones.
        let maximized = window.is_maximized()?;
        window.unmaximize()?;
        *saved = Some((window.outer_position()?, window.inner_size()?, maximized));
      }
      window.set_min_size(None::<tauri::Size>)?;
      window.set_size(tauri::LogicalSize::new(MINI_SIZE.0, MINI_SIZE.1))?;
      window.set_always_on_top(true)
    })(),
    spotatui::WindowMode::Full => (|| {
      window.set_always_on_top(false)?;
      window.set_min_size(Some(tauri::LogicalSize::new(800.0, 600.0)))?;
      if let Some((position, size, maximized)) = saved.take() {
        // Back on the first monitor before the resize, so its scale applies.
        window.set_position(position)?;
        window.set_size(size)?;
        if maximized {
          window.maximize()?;
        }
      }
      Ok(())
    })(),
  };
  if let Err(e) = result {
    log::warn!("could not resize the window: {e}");
  }
}

/// Replaces the main window with a window that keeps the error until the user closes it.
fn show_error(app: &AppHandle, message: &str) {
  let shown = WebviewWindowBuilder::new(app, "error", WebviewUrl::App("index.html".into()))
    .title("spotatui")
    .inner_size(640.0, 360.0)
    .initialization_script(error_script(message))
    .build();
  if let Some(main) = app.get_webview_window("main") {
    let _ = main.close();
  }
  if shown.is_err() {
    app.exit(1);
  }
}

/// The script that hands the error page its message, as a JSON string literal.
fn error_script(message: &str) -> String {
  let text = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_string());
  // `</` would end an inline script early.
  format!("window.__SPOTATUI_ERROR = {};", text.replace("</", "<\\/"))
}

/// Ends the process when the app thread panics, so no window stays up without an app.
struct ExitOnPanic(AppHandle);

impl Drop for ExitOnPanic {
  fn drop(&mut self) {
    if std::thread::panicking() {
      self.0.exit(101);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_install_dirs_join_the_path_once_and_last() {
    let path = std::env::join_paths(["/usr/bin", "/usr/local/bin"]).unwrap();
    let joined = with_dirs(Some(path), &MACOS_INSTALL_DIRS);
    let dirs: Vec<_> = std::env::split_paths(&joined).collect();
    assert_eq!(
      dirs,
      ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"].map(std::path::PathBuf::from)
    );
    let alone: Vec<_> = std::env::split_paths(&with_dirs(None, &MACOS_INSTALL_DIRS)).collect();
    assert_eq!(alone, MACOS_INSTALL_DIRS.map(std::path::PathBuf::from));
  }

  #[test]
  fn a_quit_waits_only_for_an_app_that_serves_and_has_not_shut_down() {
    assert!(must_wait(true, false));
    assert!(!must_wait(true, true));
    assert!(!must_wait(false, false));
    assert!(!must_wait(false, true));
  }

  #[test]
  fn an_install_and_a_quit_that_stops_waiting_exclude_each_other() {
    let life = Life::default();
    assert!(life.begin_install());
    assert!(!life.give_up());
    let life = Life::default();
    assert!(life.give_up());
    assert!(!life.begin_install());
  }

  #[test]
  fn the_error_script_escapes_quotes_and_a_closing_script_tag() {
    let script = error_script("bad \"lock\" </script>");
    assert_eq!(
      script,
      r#"window.__SPOTATUI_ERROR = "bad \"lock\" <\/script>";"#
    );
  }

  #[test]
  fn the_desktop_version_matches_the_root_package() {
    let version = |manifest: &str| {
      manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .map(|value| value.trim_matches('"').to_string())
    };
    let root = version(include_str!("../../Cargo.toml"));
    let desktop = version(include_str!("../Cargo.toml"));
    assert!(root.is_some());
    assert_eq!(root, desktop);
  }
}
