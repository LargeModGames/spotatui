//! The spotatui core inside the Android app: it serves the GUI protocol on
//! loopback, and the app's screens connect to it like the browser page does.

#[cfg(target_os = "android")]
mod android;

use anyhow::{Context, Result, anyhow};
use std::sync::{Mutex, mpsc};
use std::time::Duration;
use tokio::sync::watch;

/// How long a start waits for the core to serve its page.
const START_LIMIT: Duration = Duration::from_secs(30);

/// The app's private directories, from the Android Context.
pub struct Dirs {
  pub files: String,
  pub cache: String,
}

/// The running core: its launch URL, and the sender of its `closed` signal,
/// kept because a dropped sender reads as a closed window and stops the core.
static STARTED: Mutex<Option<(String, watch::Sender<bool>)>> = Mutex::new(None);

/// Starts the core once and returns its launch URL; a later call returns the first URL.
pub fn start(dirs: &Dirs) -> Result<String> {
  let mut started = STARTED
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  if let Some((url, _)) = started.as_ref() {
    return Ok(url.clone());
  }
  set_dirs(dirs);
  let (close, closed) = watch::channel(false);
  let (url_tx, url_rx) = mpsc::channel::<Result<String, String>>();
  let error_tx = url_tx.clone();
  std::thread::Builder::new()
    .name("spotatui".into())
    .stack_size(16 << 20)
    .spawn(move || {
      let host = spotatui::GuiHost {
        show: Box::new(move |url| {
          let _ = url_tx.send(Ok(url.to_string()));
          Ok(())
        }),
        serving: Box::new(|| log::info!("spotatui core is serving")),
        closed,
        updates: None,
        window: None,
      };
      let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(anyhow::Error::from)
        .and_then(|runtime| runtime.block_on(spotatui::run_gui_hosted(host)));
      if let Err(e) = result {
        let _ = error_tx.send(Err(format!("{e:#}")));
      }
    })
    .context("could not start the spotatui thread")?;
  let url = url_rx
    .recv_timeout(START_LIMIT)
    .map_err(|_| anyhow!("the spotatui core did not start"))?
    .map_err(|e| anyhow!(e))?;
  *started = Some((url.clone(), close));
  Ok(url)
}

/// Points the core's config, state, cache and log paths into the app's own directories.
fn set_dirs(dirs: &Dirs) {
  // The core's cache holds the Spotify login, so it must not be in the cache
  // directory Android clears when storage runs low. Only the logs go there.
  let vars = [
    ("XDG_CONFIG_HOME", format!("{}/config", dirs.files)),
    ("XDG_STATE_HOME", format!("{}/state", dirs.files)),
    ("XDG_CACHE_HOME", format!("{}/cache", dirs.files)),
    ("TMPDIR", dirs.cache.clone()),
  ];
  for (name, value) in vars {
    // SAFETY: the core's threads do not exist yet, and no JVM thread reads these variables.
    unsafe { std::env::set_var(name, value) };
  }
}
