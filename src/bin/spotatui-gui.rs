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
  #[cfg_attr(debug_assertions, allow(unused_variables))]
  let attached = attach_parent_console();
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
  let result = match handle.join() {
    Ok(result) => result,
    Err(panic) => std::panic::resume_unwind(panic),
  };
  // Started from Explorer, a windowed build has nowhere else to show why it stopped.
  #[cfg(all(windows, not(debug_assertions)))]
  if let (Err(e), false) = (&result, attached) {
    show_error(&format!("{e:#}"));
  }
  result
}

fn version_requested() -> bool {
  let requested = std::env::args().any(|arg| arg == "--version" || arg == "-V");
  if requested {
    println!("spotatui-gui {}", env!("CARGO_PKG_VERSION"));
  }
  requested
}

#[cfg(windows)]
fn attach_parent_console() -> bool {
  #[link(name = "kernel32")]
  extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
  }
  // (DWORD)-1, i.e. ATTACH_PARENT_PROCESS.
  const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
  // Failure just means there is no parent console (launched from the
  // desktop), which is exactly the case the windowed subsystem exists for.
  unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 }
}

#[cfg(all(windows, not(debug_assertions)))]
fn show_error(text: &str) {
  #[link(name = "user32")]
  extern "system" {
    fn MessageBoxW(
      window: *mut std::ffi::c_void,
      text: *const u16,
      caption: *const u16,
      kind: u32,
    ) -> i32;
  }
  const MB_ICONERROR: u32 = 0x10;
  let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
  unsafe {
    MessageBoxW(
      std::ptr::null_mut(),
      wide(text).as_ptr(),
      wide("spotatui-gui").as_ptr(),
      MB_ICONERROR,
    );
  }
}
