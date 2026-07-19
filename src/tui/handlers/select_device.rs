use super::common_key_events;
use crate::core::app::{ActiveBlock, App, SourceFocus};
use crate::core::playback_target::PlaybackTarget;
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
        let targets = app.playback_targets();
        if let Some(selected_device_index) = app.selected_device_index {
          let next_index =
            common_key_events::on_down_press_handler(&targets, Some(selected_device_index));
          app.selected_device_index = Some(next_index);
        }
      }
    },
    k if common_key_events::up_event(k, &app.user_config.keys) => match app.source_device_focus {
      SourceFocus::Source => {
        app.source_list_index =
          common_key_events::on_up_press_handler(&Source::ALL, Some(app.source_list_index));
      }
      SourceFocus::Devices => {
        let targets = app.playback_targets();
        if let Some(selected_device_index) = app.selected_device_index {
          let next_index =
            common_key_events::on_up_press_handler(&targets, Some(selected_device_index));
          app.selected_device_index = Some(next_index);
        }
      }
    },
    k if common_key_events::high_event(k) => {
      if app.source_device_focus == SourceFocus::Devices
        && app.selected_device_index.is_some()
        && !app.playback_targets().is_empty()
      {
        app.selected_device_index = Some(common_key_events::on_high_press_handler());
      }
    }
    k if common_key_events::middle_event(k) => {
      if app.source_device_focus == SourceFocus::Devices && app.selected_device_index.is_some() {
        let targets = app.playback_targets();
        if !targets.is_empty() {
          app.selected_device_index = Some(common_key_events::on_middle_press_handler(&targets));
        }
      }
    }
    k if common_key_events::low_event(k) => {
      if app.source_device_focus == SourceFocus::Devices && app.selected_device_index.is_some() {
        let targets = app.playback_targets();
        if !targets.is_empty() {
          app.selected_device_index = Some(common_key_events::on_low_press_handler(&targets));
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

  // Adding Spotify from a free-source session: start the in-TUI OAuth login
  // (browser + async callback). This sits OUTSIDE the `active_source != source`
  // guard so re-selecting Spotify after a cancelled/timed-out login retries
  // instead of no-oping (a failed attempt leaves active_source == Spotify but
  // spotify_connected == false). The `pending_login`/`spotify.is_some()` guards in
  // the handler make a redundant dispatch a safe no-op. Connected users just
  // switch back to their already-loaded Spotify data.
  if source == Source::Spotify && !app.spotify_connected {
    app.dispatch(IoEvent::BeginSpotifyLogin);
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

/// Transfer Spotify playback to the highlighted Spotify Connect or Sonos target.
fn transfer_to_selected_device(app: &mut App) {
  let Some(index) = app.selected_device_index else {
    app.set_status_message("No playback device selected", 4);
    return;
  };

  let targets = app.playback_targets();
  let Some(target) = targets.get(index).cloned() else {
    app.set_status_message("Selected playback device is no longer available", 4);
    return;
  };

  match target {
    PlaybackTarget::Spotify { id, name, .. } => {
      app.dispatch(IoEvent::TransferPlaybackToDevice(id, true));
      app.set_status_message(format!("Switching playback to {name}"), 4);
    }
    PlaybackTarget::Sonos { room, .. } => {
      let room_name = room.name.clone();
      app.dispatch(IoEvent::TransferPlaybackToSonosRoom(room.uuid, true));
      app.set_status_message(format!("Selecting Sonos room {room_name}"), 4);
    }
  }
  app.pop_navigation_stack();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::playback_target::SonosRoom;
  use crate::core::user_config::UserConfig;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  #[test]
  fn selecting_sonos_dispatches_room_transfer() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.sonos_rooms.push(SonosRoom {
      uuid: "RINCON_KITCHEN".to_string(),
      name: "Kitchen".to_string(),
      location: "http://192.168.1.20:1400/xml/device_description.xml".to_string(),
    });
    app.selected_device_index = Some(0);

    transfer_to_selected_device(&mut app);

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::TransferPlaybackToSonosRoom(room_uuid, true))
        if room_uuid == "RINCON_KITCHEN"
    ));
  }
}
