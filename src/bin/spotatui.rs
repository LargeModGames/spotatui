use anyhow::Result;

fn main() -> Result<()> {
  // Debug builds overflow the 1 MiB Windows main-thread stack: CLI mode
  // awaits the whole network future chain inline under `block_on`, and the
  // unoptimized state machine plus its poll frames does not fit. Run the
  // runtime on a thread with an explicit stack size instead of
  // `#[tokio::main]`.
  let handle = std::thread::Builder::new()
    .stack_size(16 * 1024 * 1024)
    .spawn(|| {
      tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the tokio runtime")
        .block_on(spotatui::run_cli())
    })?;
  match handle.join() {
    Ok(result) => result,
    Err(panic) => std::panic::resume_unwind(panic),
  }
}
