use serde::Serialize;
use serde_json::Value;

#[cfg(feature = "spotatui-backend")]
const BACKEND_MODE: &str = "spotatui-library";
#[cfg(not(feature = "spotatui-backend"))]
const BACKEND_MODE: &str = "stub";

#[cfg(feature = "spotatui-backend")]
use spotatui::gui::{dispatch_gui_command, snapshot_app, GuiCommand, GuiSnapshot};

#[cfg(not(feature = "spotatui-backend"))]
mod fallback {
  use serde::{Deserialize, Serialize};

  #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  pub struct GuiSnapshot {
    pub playback: GuiPlayback,
    pub devices: Vec<GuiDevice>,
    pub status: GuiStatus,
  }

  #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  pub struct GuiPlayback {
    pub track: Option<GuiTrack>,
    pub progress_ms: u64,
    pub is_playing: bool,
    pub shuffle: bool,
    pub repeat: Option<String>,
    pub volume_percent: u32,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
  }

  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub struct GuiTrack {
    pub id: Option<String>,
    pub uri: Option<String>,
    pub item_type: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub image_url: Option<String>,
    pub duration_ms: u32,
  }

  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub struct GuiDevice {
    pub id: Option<String>,
    pub name: String,
    pub device_type: String,
    pub is_active: bool,
    pub is_restricted: bool,
    pub volume_percent: Option<u32>,
  }

  #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  pub struct GuiStatus {
    pub is_loading: bool,
    pub message: Option<String>,
    pub error: Option<String>,
    pub route: String,
    pub active_block: String,
    pub is_streaming_active: bool,
  }

  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(tag = "type", rename_all = "snake_case")]
  pub enum GuiCommand {
    RefreshPlayback,
    RefreshDevices,
    Play,
    Pause,
    TogglePlayback,
    NextTrack,
    PreviousTrack,
    Seek { position_ms: u32 },
    ChangeVolume { volume_percent: u8 },
    TransferPlayback { device_id: String, play: bool },
  }
}

#[cfg(not(feature = "spotatui-backend"))]
use fallback::{GuiCommand, GuiSnapshot};

#[cfg(feature = "spotatui-backend")]
use crate::GuiState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchResult {
  backend: &'static str,
  command: Value,
  accepted: bool,
  message: &'static str,
}

// ---------------------------------------------------------------------------
// Real implementations (spotatui-backend feature enabled)
// ---------------------------------------------------------------------------

#[cfg(feature = "spotatui-backend")]
#[tauri::command]
pub async fn get_snapshot(state: tauri::State<'_, GuiState>) -> Result<GuiSnapshot, String> {
  let app = state.app.lock().await;
  Ok(snapshot_app(&app))
}

#[cfg(feature = "spotatui-backend")]
#[tauri::command]
pub async fn dispatch_command(
  state: tauri::State<'_, GuiState>,
  command: GuiCommand,
) -> Result<DispatchResult, String> {
  let mut app = state.app.lock().await;
  dispatch_gui_command(&mut app, command.clone());

  Ok(DispatchResult {
    backend: BACKEND_MODE,
    command: serde_json::to_value(&command).map_err(|e| e.to_string())?,
    accepted: true,
    message: "Command dispatched",
  })
}

// ---------------------------------------------------------------------------
// Stub implementations (no spotatui-backend feature)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "spotatui-backend"))]
#[tauri::command]
pub async fn get_snapshot() -> Result<GuiSnapshot, String> {
  Ok(stub_snapshot())
}

#[cfg(not(feature = "spotatui-backend"))]
#[tauri::command]
pub async fn dispatch_command(command: GuiCommand) -> Result<DispatchResult, String> {
  let command = serde_json::to_value(&command).map_err(|err| err.to_string())?;

  Ok(DispatchResult {
    backend: BACKEND_MODE,
    command,
    accepted: false,
    message:
      "Command received by the desktop shell; stateful backend dispatch is pending integration.",
  })
}

#[cfg(not(feature = "spotatui-backend"))]
fn stub_snapshot() -> GuiSnapshot {
  let mut snapshot = GuiSnapshot::default();
  snapshot.status.message =
    Some("The spotatui GUI backend bridge is not connected to a live App yet.".to_string());
  snapshot
}
