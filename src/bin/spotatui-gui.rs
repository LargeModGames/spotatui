//! Windowed-binary shim for the browser frontend: serves the page on loopback
//! and opens it. Requires the `gui` cargo feature, which nothing enables by
//! default.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;

// macOS requires spotatui to run on the main thread for media keys.
#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() -> Result<()> {
  if version_requested() {
    return Ok(());
  }
  spotatui::run_gui().await
}

#[cfg(not(target_os = "macos"))]
fn main() -> Result<()> {
  // A windowed binary detaches from the console; re-attach to the parent's so
  // `--version` and errors still print when run from a shell.
  #[cfg(windows)]
  attach_parent_console();
  if version_requested() {
    return Ok(());
  }
  // Same 16 MiB stack as the console shim: debug builds overflow the 1 MiB Windows main thread.
  let handle = std::thread::Builder::new()
    .stack_size(16 * 1024 * 1024)
    .spawn(|| {
      tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the tokio runtime")
        .block_on(spotatui::run_gui())
    })?;
  match handle.join() {
    Ok(result) => result,
    Err(panic) => std::panic::resume_unwind(panic),
  }
}

fn version_requested() -> bool {
  let requested = std::env::args().any(|arg| arg == "--version" || arg == "-V");
  if requested {
    println!("spotatui-gui {}", env!("CARGO_PKG_VERSION"));
  }
  requested
}

#[cfg(windows)]
fn attach_parent_console() {
  #[link(name = "kernel32")]
  extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
  }
  // (DWORD)-1, i.e. ATTACH_PARENT_PROCESS.
  const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
  // Failure just means there is no parent console (launched from the
  // desktop), which is exactly the case the windowed subsystem exists for.
  unsafe {
    AttachConsole(ATTACH_PARENT_PROCESS);
  }
}
