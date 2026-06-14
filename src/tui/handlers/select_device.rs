use super::common_key_events;
use crate::core::app::App;
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(p) = &app.devices {
        if let Some(selected_device_index) = app.selected_device_index {
          let next_index =
            common_key_events::on_down_press_handler(&p.devices, Some(selected_device_index));
          app.selected_device_index = Some(next_index);
        }
      };
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(p) = &app.devices {
        if let Some(selected_device_index) = app.selected_device_index {
          let next_index =
            common_key_events::on_up_press_handler(&p.devices, Some(selected_device_index));
          app.selected_device_index = Some(next_index);
        }
      };
    }
    k if common_key_events::high_event(k) => {
      if let Some(_p) = &app.devices {
        if let Some(_selected_device_index) = app.selected_device_index {
          let next_index = common_key_events::on_high_press_handler();
          app.selected_device_index = Some(next_index);
        }
      };
    }
    k if common_key_events::middle_event(k) => {
      if let Some(p) = &app.devices {
        if let Some(_selected_device_index) = app.selected_device_index {
          let next_index = common_key_events::on_middle_press_handler(&p.devices);
          app.selected_device_index = Some(next_index);
        }
      };
    }
    k if common_key_events::low_event(k) => {
      if let Some(p) = &app.devices {
        if let Some(_selected_device_index) = app.selected_device_index {
          let next_index = common_key_events::on_low_press_handler(&p.devices);
          app.selected_device_index = Some(next_index);
        }
      };
    }
    Key::Enter => {
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
    _ => {}
  }
}
