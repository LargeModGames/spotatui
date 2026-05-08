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
  backend: Arc<Mutex<GuiBackend>>,
}

#[cfg(feature = "spotatui-backend")]
pub enum GuiBackend {
  Initializing,
  Ready {
    app: Arc<Mutex<App>>,
    /// Kept for direct IoEvent injection when needed; commands currently use
    /// the shared spotatui GUI action dispatch path.
    #[allow(dead_code)]
    io_tx: std::sync::mpsc::Sender<IoEvent>,
  },
  Failed(String),
}

#[cfg(feature = "spotatui-backend")]
impl Default for GuiState {
  fn default() -> Self {
    GuiState {
      backend: Arc::new(Mutex::new(GuiBackend::Initializing)),
    }
  }
}

#[cfg(feature = "spotatui-backend")]
impl GuiState {
  pub async fn snapshot(&self) -> Result<spotatui::gui::GuiSnapshot, String> {
    let backend = self.backend.lock().await;
    match &*backend {
      GuiBackend::Initializing => {
        let mut snapshot = spotatui::gui::GuiSnapshot::default();
        snapshot.status.is_loading = true;
        snapshot.status.message = Some("Spotatui backend is starting".to_string());
        Ok(snapshot)
      }
      GuiBackend::Ready { app, .. } => {
        let app = app.lock().await;
        Ok(spotatui::gui::snapshot_app(&app))
      }
      GuiBackend::Failed(error) => {
        let mut snapshot = spotatui::gui::GuiSnapshot::default();
        snapshot.status.error = Some(error.clone());
        snapshot.status.message = Some("Spotatui backend failed to start".to_string());
        Ok(snapshot)
      }
    }
  }

  pub async fn dispatch(&self, action: spotatui::gui::GuiAction) -> Result<(), String> {
    let backend = self.backend.lock().await;
    match &*backend {
      GuiBackend::Ready { app, .. } => {
        let mut app = app.lock().await;
        spotatui::gui::dispatch_gui_action(&mut app, action);
        Ok(())
      }
      GuiBackend::Initializing => Err("Spotatui backend is still starting".to_string()),
      GuiBackend::Failed(error) => Err(error.clone()),
    }
  }

  async fn set_ready(&self, app: Arc<Mutex<App>>, io_tx: std::sync::mpsc::Sender<IoEvent>) {
    let mut backend = self.backend.lock().await;
    *backend = GuiBackend::Ready { app, io_tx };
  }

  async fn set_failed(&self, error: String) {
    let mut backend = self.backend.lock().await;
    *backend = GuiBackend::Failed(error);
  }
}

#[cfg(feature = "spotatui-backend")]
impl Clone for GuiState {
  fn clone(&self) -> Self {
    GuiState {
      backend: Arc::clone(&self.backend),
    }
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let builder = tauri::Builder::default();

  #[cfg(feature = "spotatui-backend")]
  let builder = builder.setup(|app| {
    let handle = app.handle().clone();
    app.manage(GuiState::default());

    tauri::async_runtime::spawn(async move {
      log::info!("initializing spotatui session for GUI");
      let state = handle.state::<GuiState>().inner().clone();

      let config = SessionConfig::default();
      let mut session = match SpotatuiSession::new(config).await {
        Ok(s) => s,
        Err(e) => {
          log::error!("failed to create spotatui session: {}", e);
          state.set_failed(e.to_string()).await;
          return;
        }
      };

      if let Err(e) = session.start_network_task().await {
        log::error!("failed to start network task: {}", e);
        state.set_failed(e.to_string()).await;
        return;
      }

      log::info!("spotatui session ready");
      let app = session.app();
      let io_tx = session.io_tx().clone();
      state.set_ready(app, io_tx).await;

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
      commands::dispatch_action,
    ])
    .run(tauri::generate_context!())
    .expect("error while running spotagui");
}
