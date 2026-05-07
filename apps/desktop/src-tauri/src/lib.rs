mod commands;

use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "spotatui-backend")]
use spotatui::core::app::App;
#[cfg(feature = "spotatui-backend")]
use spotatui::gui::{SessionConfig, SpotatuiSession};
#[cfg(feature = "spotatui-backend")]
use spotatui::infra::network::IoEvent;
#[cfg(feature = "spotatui-backend")]
use tauri::Manager;

/// Shared state accessible to all Tauri command handlers.
///
/// Holds the pieces extracted from `SpotatuiSession` after initialization:
/// - `app`: the shared `App` behind a tokio mutex (App is Send but not Sync
///   due to `Cell<u16>`, so the mutex guards concurrent access).
/// - `io_tx`: the sender half of the io channel; `std::sync::mpsc::Sender` is
///   both Send + Sync, safe to share across threads.
#[cfg(feature = "spotatui-backend")]
pub struct GuiState {
  app: Arc<Mutex<App>>,
  /// Kept for direct IoEvent injection when needed; commands currently use
  /// `dispatch_gui_command` which sends through the App's internal channel.
  #[allow(dead_code)]
  io_tx: std::sync::mpsc::Sender<IoEvent>,
}

#[cfg(feature = "spotatui-backend")]
impl GuiState {
  fn from_session(session: &SpotatuiSession) -> Self {
    GuiState {
      app: session.app(),
      io_tx: session.io_tx().clone(),
    }
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default();

  #[cfg(feature = "spotatui-backend")]
  let builder = builder.setup(|app| {
    let handle = app.handle().clone();

    tauri::async_runtime::spawn(async move {
      log::info!("initializing spotatui session for GUI");

      let config = SessionConfig::default();
      let mut session = match SpotatuiSession::new(config).await {
        Ok(s) => s,
        Err(e) => {
          log::error!("failed to create spotatui session: {}", e);
          return;
        }
      };

      if let Err(e) = session.start_network_task().await {
        log::error!("failed to start network task: {}", e);
        return;
      }

      log::info!("spotatui session ready");
      let state = GuiState::from_session(&session);
      handle.manage(state);

      // Keep the session alive for the lifetime of the app. The session holds
      // platform integration handles (MPRIS, Discord RPC, etc.) that must not
      // be dropped.
      std::mem::forget(session);
    });

    Ok(())
  });

  builder
    .invoke_handler(tauri::generate_handler![
      commands::get_snapshot,
      commands::dispatch_command,
    ])
    .run(tauri::generate_context!())
    .expect("error while running spotagui");
}
