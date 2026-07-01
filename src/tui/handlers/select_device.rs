use super::common_key_events;
use crate::core::app::{ActiveBlock, App, SourceFocus};
use crate::core::source::Source;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    Key::Tab => {
      // Devices are Spotify Connect only, so keep focus pinned to the Source
      // list when Local is active (the Devices panel is dimmed).
      app.source_device_focus = if app.active_source != Source::Spotify {
        SourceFocus::Source
      } else {
        match app.source_device_focus {
          SourceFocus::Source => SourceFocus::Devices,
          SourceFocus::Devices => SourceFocus::Source,
        }
      };
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => match app.source_device_focus {
      SourceFocus::Source => {
        app.source_list_index =
          common_key_events::on_down_press_handler(&Source::ALL, Some(app.source_list_index));
      }
      SourceFocus::Devices => {
        if let Some(p) = &app.devices {
          if let Some(selected_device_index) = app.selected_device_index {
            let next_index =
              common_key_events::on_down_press_handler(&p.devices, Some(selected_device_index));
            app.selected_device_index = Some(next_index);
          }
        }
      }
    },
    k if common_key_events::up_event(k, &app.user_config.keys) => match app.source_device_focus {
      SourceFocus::Source => {
        app.source_list_index =
          common_key_events::on_up_press_handler(&Source::ALL, Some(app.source_list_index));
      }
      SourceFocus::Devices => {
        if let Some(p) = &app.devices {
          if let Some(selected_device_index) = app.selected_device_index {
            let next_index =
              common_key_events::on_up_press_handler(&p.devices, Some(selected_device_index));
            app.selected_device_index = Some(next_index);
          }
        }
      }
    },
    k if common_key_events::high_event(k) => {
      if app.source_device_focus == SourceFocus::Devices {
        if let Some(_p) = &app.devices {
          if app.selected_device_index.is_some() {
            app.selected_device_index = Some(common_key_events::on_high_press_handler());
          }
        }
      }
    }
    k if common_key_events::middle_event(k) => {
      if app.source_device_focus == SourceFocus::Devices {
        if let Some(p) = &app.devices {
          if app.selected_device_index.is_some() {
            let next_index = common_key_events::on_middle_press_handler(&p.devices);
            app.selected_device_index = Some(next_index);
          }
        }
      }
    }
    k if common_key_events::low_event(k) => {
      if app.source_device_focus == SourceFocus::Devices {
        if let Some(p) = &app.devices {
          if app.selected_device_index.is_some() {
            let next_index = common_key_events::on_low_press_handler(&p.devices);
            app.selected_device_index = Some(next_index);
          }
        }
      }
    }
    Key::Enter => match app.source_device_focus {
      SourceFocus::Source => select_source(app),
      SourceFocus::Devices => transfer_to_selected_device(app),
    },
    _ => {}
  }
}

/// Commit the highlighted source as the new active source and close the picker.
/// This is browse-scope only: it never starts or stops playback.
fn select_source(app: &mut App) {
  let source = Source::ALL[app.source_list_index];
  if app.active_source != source {
    app.active_source = source;
    // Mirror the persisted value so it survives restarts.
    app.user_config.behavior.active_source = source;
    if let Err(e) = app.user_config.save_config() {
      log::warn!("[source] failed to persist active_source: {e}");
    }
    // Reset the sidebar playlist cursor to the top of the new source's list.
    app.selected_playlist_index = Some(0);
    match source {
      Source::Local => {
        // Populate the sidebar with local folders for the newly active source.
        app.local_playlists_index = 0;
        app.dispatch(IoEvent::GetLocalPlaylists);
      }
      Source::Subsonic => {
        // Populate the sidebar with the server's playlists.
        app.subsonic_playlists_index = 0;
        app.dispatch(IoEvent::GetSubsonicPlaylists);
      }
      Source::Radio => {
        // Populate the sidebar with the configured stations.
        app.radio_stations_index = 0;
        app.dispatch(IoEvent::GetRadioStations);
      }
      Source::YouTube => {
        // Populate the sidebar with the local YouTube playlists file.
        app.dispatch(IoEvent::GetYouTubePlaylists);
      }
      Source::Spotify => {}
    }
  }
  app.set_status_message(format!("Source: {}", source.label()), 4);
  app.pop_navigation_stack();

  // If focus landed on a block the new source hides (the Library list under any
  // non-Spotify source), move it to the Playlists block so input isn't lost.
  if source != Source::Spotify {
    let route = app.get_current_route();
    if route.active_block == ActiveBlock::Library || route.hovered_block == ActiveBlock::Library {
      app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::MyPlaylists));
    }
  }
}

/// Existing behaviour: transfer Spotify playback to the highlighted device.
fn transfer_to_selected_device(app: &mut App) {
  let Some(index) = app.selected_device_index else {
    app.set_status_message("No playback device selected", 4);
    return;
  };

  let Some(devices) = &app.devices else {
    app.set_status_message("No playback devices found", 4);
    return;
  };

  let Some(device) = devices.devices.get(index) else {
    app.set_status_message("Selected playback device is no longer available", 4);
    return;
  };

  let Some(device_id) = &device.id else {
    app.set_status_message("Selected playback device has no Spotify device id", 4);
    return;
  };

  let device_name = device.name.clone();
  app.dispatch(IoEvent::TransferPlaybackToDevice(device_id.clone(), true));
  app.set_status_message(format!("Switching playback to {}", device_name), 4);
  app.pop_navigation_stack();
}
