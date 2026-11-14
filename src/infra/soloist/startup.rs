//! The Soloist engine's bring-up: the session gate, the binary, the key, then the supervisor.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::{auth, supervisor, Notice};
use crate::core::app::App;
use crate::core::user_config::{BehaviorConfig, StartupBehavior};

pub(crate) struct SoloistStart {
  pub soloist_path: Option<String>,
  pub device_name: String,
  pub quality: u8,
  pub initial_volume: u8,
  pub startup_behavior: Option<StartupBehavior>,
  pub startup_shuffle: bool,
}

impl SoloistStart {
  pub(crate) fn from_behavior(
    behavior: &BehaviorConfig,
    initial_volume: u8,
    startup_behavior: Option<StartupBehavior>,
    startup_shuffle: bool,
  ) -> Self {
    Self {
      soloist_path: behavior.soloist_path.clone(),
      device_name: behavior.streaming_device_name.clone(),
      quality: behavior.soloist_audio_quality.enumeration(),
      initial_volume,
      startup_behavior,
      startup_shuffle,
    }
  }
}

/// Stop a running Soloist, then start the engine the settings name, with the saved key.
pub(crate) async fn apply_engine(app: Arc<Mutex<App>>) {
  if !supervisor::stop_for_restart(std::time::Duration::from_secs(5)).await {
    log::warn!("soloist: the old engine did not stop; not switching");
    return;
  }
  let start = app.lock().await.apply_playback_engine();
  if let Some(start) = start {
    deferred_soloist_startup(app, start);
  }
}

/// Runs after the first frame, like librespot's deferred start.
pub(crate) fn deferred_soloist_startup(app: Arc<Mutex<App>>, start: SoloistStart) {
  tokio::spawn(async move {
    if let Err(unavailable) = crate::infra::spotify::session::current().await {
      log::info!("soloist: not started: {}", unavailable.message());
      app.lock().await.soloist_failed(None);
      return;
    }
    let Some(path) = start.soloist_path else {
      app.lock().await.soloist_failed(Some(Notice::PathUnset));
      return;
    };
    let Some(key) = auth::current_key() else {
      let file = crate::core::paths::soloist_credentials_path().unwrap_or_default();
      app
        .lock()
        .await
        .soloist_failed(Some(Notice::KeyMissing(file)));
      return;
    };
    let (Some(data_dir), Some(cache_dir)) = (
      crate::core::paths::soloist_data_dir(),
      crate::core::paths::soloist_cache_dir(),
    ) else {
      app
        .lock()
        .await
        .soloist_failed(Some(Notice::StartFailed("no state directory".to_string())));
      return;
    };
    supervisor::start(
      app,
      supervisor::Config {
        path: path.into(),
        device_name: start.device_name,
        key,
        data_dir,
        cache_dir,
        quality: start.quality,
        initial_volume: start.initial_volume,
        startup_behavior: start.startup_behavior,
        startup_shuffle: start.startup_shuffle,
      },
    );
  });
}
