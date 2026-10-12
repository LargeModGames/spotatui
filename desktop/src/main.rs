// No console window next to the app window in a release build on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! The spotatui desktop app: a Tauri window around the GUI's loopback page.

use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// How long a quit waits for the app to save the session and stop playback.
const SHUTDOWN_LIMIT: Duration = Duration::from_secs(10);

/// What the event loop and the app thread know of each other.
#[derive(Default)]
struct Life {
  /// Boot is done and the page is served: a quit must wait for a clean shutdown.
  serving: AtomicBool,
  /// The app thread finished its shutdown.
  done: AtomicBool,
}

fn main() -> anyhow::Result<()> {
  // WebKitGTK's DMA-BUF renderer draws a blank window on some GPU drivers.
  #[cfg(target_os = "linux")]
  if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
  }
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
  tauri::async_runtime::set(runtime.handle().clone());
  let life = Arc::new(Life::default());
  let (close_tx, closed) = tokio::sync::watch::channel(false);
  let (done_tx, done_rx) = mpsc::channel::<()>();
  let handle = runtime.handle().clone();
  let worker_life = Arc::clone(&life);
  let app = tauri::Builder::default()
    // First plugin: a second launch focuses this window and exits before it boots.
    .plugin(tauri_plugin_single_instance::init(|app, _, _| {
      if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
      }
    }))
    .setup(move |app| {
      let app = app.handle().clone();
      // `ask_page` blocks its thread, so the app runs on a thread of its own, not a runtime worker.
      std::thread::Builder::new()
        .name("spotatui".into())
        .stack_size(16 << 20)
        .spawn(move || worker(app, handle, worker_life, closed, done_tx))?;
      Ok(())
    })
    .build(tauri::generate_context!())?;
  let code = app.run_return(move |_, event| {
    if let RunEvent::Exit = event {
      if must_wait(life.serving.load(SeqCst), life.done.load(SeqCst)) {
        let _ = close_tx.send(true);
        let _ = done_rx.recv_timeout(SHUTDOWN_LIMIT);
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
) {
  let _guard = ExitOnPanic(app.clone());
  let show_app = app.clone();
  let serving_life = Arc::clone(&life);
  let host = spotatui::GuiHost {
    show: Box::new(move |url| show_main(&show_app, url)),
    serving: Box::new(move || serving_life.serving.store(true, SeqCst)),
    closed,
  };
  let result = runtime.block_on(spotatui::run_gui_hosted(host));
  life.done.store(true, SeqCst);
  let _ = done.send(());
  match result {
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
  fn a_quit_waits_only_for_an_app_that_serves_and_has_not_shut_down() {
    assert!(must_wait(true, false));
    assert!(!must_wait(true, true));
    assert!(!must_wait(false, false));
    assert!(!must_wait(false, true));
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
