//! Native-streaming startup that every frontend shares. This file holds the
//! pure saved-device decision; `launch` holds the credential gate and the
//! deferred librespot bring-up.

use crate::core::user_config::StartupBehavior;

#[cfg(feature = "streaming")]
pub(super) mod launch;
#[cfg(feature = "soloist")]
pub(super) mod soloist;

/// Soloist runs only when it is compiled in and Spotify ships a build for this CPU.
#[cfg(all(feature = "streaming", any(feature = "tui", feature = "gui")))]
pub(super) fn effective_engine(
  setting: crate::core::user_config::PlaybackEngine,
  soloist_compiled: bool,
  intel_mac: bool,
) -> crate::core::user_config::PlaybackEngine {
  use crate::core::user_config::PlaybackEngine;
  if setting == PlaybackEngine::Soloist && soloist_compiled && !intel_mac {
    PlaybackEngine::Soloist
  } else {
    PlaybackEngine::Librespot
  }
}

#[derive(Debug, PartialEq, Eq)]
enum StartupDeviceEvent {
  Transfer {
    device_id: String,
    persist_device_id: bool,
  },
  AutoSelectStreaming {
    device_name: String,
    persist_device_id: bool,
  },
}

#[derive(Debug, PartialEq, Eq)]
struct StartupDeviceDecision {
  event: Option<StartupDeviceEvent>,
  status_message: Option<String>,
}

fn startup_device_decision(
  startup_behavior: StartupBehavior,
  saved_device_id: Option<String>,
  devices_snapshot: Option<&[rspotify::model::device::Device]>,
  native_device_name: &str,
  native_id: Option<&str>,
  strings: &crate::core::i18n::Strings,
) -> StartupDeviceDecision {
  if startup_behavior != StartupBehavior::Play {
    return StartupDeviceDecision {
      event: None,
      status_message: None,
    };
  }

  let event = match saved_device_id {
    Some(saved_device_id) => {
      if let Some(devices) = devices_snapshot {
        let mut saved_device_available = false;
        let mut by_id = None;
        let mut by_name = None;

        for device in devices {
          if device.id.as_ref() == Some(&saved_device_id) {
            saved_device_available = true;
            break;
          }

          if by_id.is_none() && native_id.is_some() && device.id.as_deref() == native_id {
            by_id = device.id.clone();
          }
          if by_name.is_none() && device.name.eq_ignore_ascii_case(native_device_name) {
            by_name = device.id.clone();
          }
        }
        // Another device can carry the native name; the id is exact.
        let native_device_id = by_id.or(by_name);

        if saved_device_available {
          Some(StartupDeviceEvent::Transfer {
            device_id: saved_device_id,
            persist_device_id: true,
          })
        } else {
          native_device_id.map_or_else(
            || {
              Some(StartupDeviceEvent::AutoSelectStreaming {
                device_name: native_device_name.to_string(),
                persist_device_id: false,
              })
            },
            |device_id| {
              Some(StartupDeviceEvent::Transfer {
                device_id,
                persist_device_id: false,
              })
            },
          )
        }
      } else {
        Some(StartupDeviceEvent::Transfer {
          device_id: saved_device_id,
          persist_device_id: true,
        })
      }
    }
    None => Some(StartupDeviceEvent::AutoSelectStreaming {
      device_name: native_device_name.to_string(),
      persist_device_id: true,
    }),
  };

  let status_message = matches!(
    event,
    Some(
      StartupDeviceEvent::Transfer {
        persist_device_id: false,
        ..
      } | StartupDeviceEvent::AutoSelectStreaming {
        persist_device_id: false,
        ..
      }
    )
  )
  .then(|| {
    strings
      .status
      .runtime
      .saved_device_unavailable(native_device_name)
  });

  StartupDeviceDecision {
    event,
    status_message,
  }
}

#[cfg(test)]
mod tests {
  use super::{startup_device_decision, StartupDeviceEvent};
  use crate::core::user_config::StartupBehavior;
  use rspotify::model::{device::Device, DeviceType};

  const NATIVE_NAME: &str = "spotatui";
  const NATIVE_ID: &str = "native-device";
  const EXTERNAL_ID: &str = "phone-device";

  #[allow(deprecated)]
  fn device(id: &str, name: &str) -> Device {
    Device {
      id: Some(id.to_string()),
      is_active: false,
      is_private_session: false,
      is_restricted: false,
      name: name.to_string(),
      _type: if cfg!(target_os = "android") {
        DeviceType::Smartphone
      } else {
        DeviceType::Computer
      },
      volume_percent: Some(50),
    }
  }

  fn startup_device_event(
    startup_behavior: StartupBehavior,
    saved_device_id: Option<String>,
    devices_snapshot: Option<&[Device]>,
  ) -> Option<StartupDeviceEvent> {
    startup_device_decision(
      startup_behavior,
      saved_device_id,
      devices_snapshot,
      NATIVE_NAME,
      Some(NATIVE_ID),
      &crate::core::i18n::EN,
    )
    .event
  }

  #[test]
  fn continue_without_saved_device_does_not_transfer() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(StartupBehavior::Continue, None, Some(&devices)),
      None
    );
  }

  #[test]
  fn continue_with_saved_native_device_does_not_transfer() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Continue,
        Some(NATIVE_ID.to_string()),
        Some(&devices),
      ),
      None
    );
  }

  #[test]
  fn continue_with_saved_external_device_does_not_transfer() {
    let devices = vec![
      device(EXTERNAL_ID, "Jay's phone"),
      device(NATIVE_ID, NATIVE_NAME),
    ];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Continue,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices),
      ),
      None
    );
  }

  #[test]
  fn play_with_saved_available_device_transfers_to_saved_device() {
    let devices = vec![
      device(EXTERNAL_ID, "Jay's phone"),
      device(NATIVE_ID, NATIVE_NAME),
    ];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Play,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices),
      ),
      Some(StartupDeviceEvent::Transfer {
        device_id: EXTERNAL_ID.to_string(),
        persist_device_id: true,
      })
    );
  }

  #[test]
  fn play_without_saved_device_auto_selects_native_fallback() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(StartupBehavior::Play, None, Some(&devices)),
      Some(StartupDeviceEvent::AutoSelectStreaming {
        device_name: NATIVE_NAME.to_string(),
        persist_device_id: true,
      })
    );
  }

  #[test]
  fn continue_with_unavailable_saved_device_does_not_fall_back_to_native() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Continue,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices),
      ),
      None
    );
  }

  #[test]
  fn play_with_unavailable_saved_device_transfers_to_native_without_persisting() {
    let devices = vec![device(NATIVE_ID, NATIVE_NAME)];

    let decision = startup_device_decision(
      StartupBehavior::Play,
      Some(EXTERNAL_ID.to_string()),
      Some(&devices),
      NATIVE_NAME,
      Some(NATIVE_ID),
      &crate::core::i18n::EN,
    );

    assert_eq!(
      decision.event,
      Some(StartupDeviceEvent::Transfer {
        device_id: NATIVE_ID.to_string(),
        persist_device_id: false,
      })
    );
    assert_eq!(
      decision.status_message,
      Some(format!("Saved device unavailable; using {}", NATIVE_NAME))
    );
  }

  #[test]
  fn play_with_unavailable_saved_device_picks_the_native_id_over_a_row_with_its_name() {
    let devices = vec![
      device("leftover-device", NATIVE_NAME),
      device(NATIVE_ID, NATIVE_NAME),
    ];

    assert_eq!(
      startup_device_event(
        StartupBehavior::Play,
        Some(EXTERNAL_ID.to_string()),
        Some(&devices)
      ),
      Some(StartupDeviceEvent::Transfer {
        device_id: NATIVE_ID.to_string(),
        persist_device_id: false,
      })
    );
  }

  #[test]
  fn play_with_unavailable_saved_device_auto_selects_native_without_persisting() {
    let devices = vec![device("other-device", "Other speaker")];

    let decision = startup_device_decision(
      StartupBehavior::Play,
      Some(EXTERNAL_ID.to_string()),
      Some(&devices),
      NATIVE_NAME,
      Some(NATIVE_ID),
      &crate::core::i18n::EN,
    );

    assert_eq!(
      decision.event,
      Some(StartupDeviceEvent::AutoSelectStreaming {
        device_name: NATIVE_NAME.to_string(),
        persist_device_id: false,
      })
    );
    assert_eq!(
      decision.status_message,
      Some(format!("Saved device unavailable; using {}", NATIVE_NAME))
    );
  }

  #[test]
  fn play_with_saved_device_and_no_snapshot_transfers_to_saved_device() {
    let decision = startup_device_decision(
      StartupBehavior::Play,
      Some(EXTERNAL_ID.to_string()),
      None,
      NATIVE_NAME,
      Some(NATIVE_ID),
      &crate::core::i18n::EN,
    );

    assert_eq!(
      decision.event,
      Some(StartupDeviceEvent::Transfer {
        device_id: EXTERNAL_ID.to_string(),
        persist_device_id: true,
      })
    );
    assert_eq!(decision.status_message, None);
  }
}

#[cfg(all(test, feature = "streaming", any(feature = "tui", feature = "gui")))]
mod engine_tests {
  use super::effective_engine;
  use crate::core::user_config::PlaybackEngine;

  #[test]
  fn effective_engine_falls_back_when_soloist_is_not_compiled() {
    assert_eq!(
      effective_engine(PlaybackEngine::Soloist, false, false),
      PlaybackEngine::Librespot
    );
    assert_eq!(
      effective_engine(PlaybackEngine::Soloist, true, false),
      PlaybackEngine::Soloist
    );
  }

  #[test]
  fn effective_engine_picks_librespot_on_an_intel_mac() {
    assert_eq!(
      effective_engine(PlaybackEngine::Soloist, true, true),
      PlaybackEngine::Librespot
    );
    assert_eq!(
      effective_engine(PlaybackEngine::Librespot, true, false),
      PlaybackEngine::Librespot
    );
  }
}
