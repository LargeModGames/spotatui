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

/// Runs after the first frame, like librespot's deferred start.
pub(crate) fn deferred_soloist_startup(app: Arc<Mutex<App>>, start: SoloistStart) {
  tokio::spawn(async move {
    let fail = |notice: Option<Notice>| {
      let app = Arc::clone(&app);
      async move {
        let mut app = app.lock().await;
        if let Some(notice) = notice {
          app.soloist_notice(notice);
        }
        app.soloist_failed();
      }
    };
    if let Err(unavailable) = crate::infra::spotify::session::current().await {
      log::info!("soloist: not started: {}", unavailable.message());
      fail(None).await;
      return;
    }
    let Some(path) = start.soloist_path else {
      fail(Some(Notice::PathUnset)).await;
      return;
    };
    let Some(key) = auth::current_key() else {
      let file = crate::core::paths::soloist_credentials_path().unwrap_or_default();
      fail(Some(Notice::KeyMissing(file))).await;
      return;
    };
    let (Some(data_dir), Some(cache_dir)) = (
      crate::core::paths::soloist_data_dir(),
      crate::core::paths::soloist_cache_dir(),
    ) else {
      fail(Some(Notice::StartFailed("no state directory".to_string()))).await;
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
