//! Windowed-binary shim for the future GUI frontend.
//!
//! Exists now so the binary name, the `windows_subsystem` attribute (a
//! crate-root attribute that cannot vary at runtime), and the console-attach
//! plumbing are settled before the frontend lands. Requires the `gui` cargo
//! feature, which nothing enables by default.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
  // A windowed binary detaches from the console; re-attach to the parent's so
  // `spotatui-gui --version` still prints when run from a shell.
  #[cfg(windows)]
  attach_parent_console();

  if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
    println!("spotatui-gui {}", env!("CARGO_PKG_VERSION"));
    return;
  }

  eprintln!(
    "spotatui-gui: the GUI frontend has not landed yet; run `spotatui` for the terminal UI"
  );
  std::process::exit(1);
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
