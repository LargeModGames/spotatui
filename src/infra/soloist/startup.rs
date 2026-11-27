//! The Soloist engine's bring-up: the session gate, the binary, the key, then the supervisor.

use std::sync::Arc;

use tokio::sync::Mutex;

use super::{auth, supervisor, Notice};
use crate::core::app::App;
use crate::core::user_config::{BehaviorConfig, StartupBehavior};

// Engine changes and download completions run on the concurrent service lane.
static ENGINE_SWITCH: Mutex<()> = Mutex::const_new(());

pub(crate) struct SoloistStart {
  pub generation: u64,
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
    generation: u64,
    initial_volume: u8,
    startup_behavior: Option<StartupBehavior>,
    startup_shuffle: bool,
  ) -> Self {
    Self {
      generation,
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
/// A request a later engine change superseded does nothing; the later one switches.
pub(crate) async fn apply_engine(app: Arc<Mutex<App>>, request: u64) {
  apply_engine_on(app, request, supervisor::shared()).await;
}

pub(super) async fn apply_engine_on(
  app: Arc<Mutex<App>>,
  request: u64,
  shared: &supervisor::Shared,
) {
  let _switch = ENGINE_SWITCH.lock().await;
  {
    let app = app.lock().await;
    if app.io_tx_clone().is_none() || !app.is_current_engine_request(request) {
      return;
    }
  }
  if !shared
    .stop_for_restart(std::time::Duration::from_secs(5))
    .await
  {
    return;
  }
  let start = {
    let mut app = app.lock().await;
    if app.io_tx_clone().is_none()
      || shared.is_shutting_down()
      || !app.is_current_engine_request(request)
    {
      return;
    }
    app.apply_playback_engine()
  };
  if let Some(start) = start {
    deferred_soloist_startup(app, start);
  }
}

/// Runs after the first frame, like librespot's deferred start.
pub(crate) fn deferred_soloist_startup(app: Arc<Mutex<App>>, start: SoloistStart) {
  tokio::spawn(async move {
    let session = crate::infra::spotify::session::current().await;
    let _switch = ENGINE_SWITCH.lock().await;
    let mut current = app.lock().await;
    if current.io_tx_clone().is_none()
      || supervisor::shared().is_shutting_down()
      || !current.is_current_soloist_start(start.generation)
    {
      return;
    }
    if let Err(unavailable) = session {
      log::info!("soloist: not started: {}", unavailable.message());
      current.soloist_failed(None);
      return;
    }
    let Some(path) = start.soloist_path else {
      current.soloist_failed(Some(Notice::PathUnset));
      return;
    };
    let Some(key) = auth::current_key() else {
      let file = crate::core::paths::soloist_credentials_path().unwrap_or_default();
      current.soloist_failed(Some(Notice::KeyMissing(file)));
      return;
    };
    let (Some(data_dir), Some(cache_dir)) = (
      crate::core::paths::soloist_data_dir(),
      crate::core::paths::soloist_cache_dir(),
    ) else {
      current.soloist_failed(Some(Notice::StartFailed("no state directory".to_string())));
      return;
    };
    supervisor::start(
      Arc::clone(&app),
      supervisor::Config {
        generation: start.generation,
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
