use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::App;
use crate::core::plugin_api::TrackInfo;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      if let Some(recently_played_result) = &app.recently_played {
        let next_index = common_key_events::on_down_press_handler(
          &recently_played_result.items,
          Some(app.view.recently_played_index),
        );
        app.view.recently_played_index = next_index;
      }
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if let Some(recently_played_result) = &app.recently_played {
        let next_index = common_key_events::on_up_press_handler(
          &recently_played_result.items,
          Some(app.view.recently_played_index),
        );
        app.view.recently_played_index = next_index;
      }
    }
    k if common_key_events::high_event(k) => {
      if let Some(_recently_played_result) = &app.recently_played {
        let next_index = common_key_events::on_high_press_handler();
        app.view.recently_played_index = next_index;
      }
    }
    k if common_key_events::middle_event(k) => {
      if let Some(recently_played_result) = &app.recently_played {
        let next_index = common_key_events::on_middle_press_handler(&recently_played_result.items);
        app.view.recently_played_index = next_index;
      }
    }
    k if common_key_events::low_event(k) => {
      if let Some(recently_played_result) = &app.recently_played {
        let next_index = common_key_events::on_low_press_handler(&recently_played_result.items);
        app.view.recently_played_index = next_index;
      }
    }
    Key::Char('s') => {
      // The bare id, not the URI: a row can be a local file.
      if let Some(id) = selected_recent_track(app).and_then(|track| track.id.clone()) {
        app.apply(Action::ToggleSaveTrack(id));
      }
    }
    Key::Char('w') => {
      let track = selected_recent_track(app).map(|track| (track.id.clone(), track.name.clone()));
      if let Some((track_id, track_name)) = track {
        app.apply(Action::OpenAddTrackDialogFor {
          track_id,
          track_name,
        });
      }
    }
    Key::Enter => {
      let selected = app.view.recently_played_index;
      let request = app.recently_played.as_ref().map(|page| {
        common_key_events::uri_playback_request(
          page.items.iter().map(|track| track.uri.clone()),
          selected,
        )
      });
      if let Some((uris, offset)) = request {
        app.apply(Action::PlayUris { uris, offset });
      }
    }
    Key::Char('r') => {
      let identity =
        selected_recent_track(app).and_then(|track| Some((track.id.clone()?, track.name.clone())));
      if let Some((id, name)) = identity {
        app.apply(Action::RecommendFromTrackId { id, name });
      }
    }
    _ if key == app.user_config.keys.add_item_to_queue => {
      let track = app
        .recently_played
        .as_ref()
        .and_then(|r| r.items.get(app.view.recently_played_index).cloned());
      if let Some(track) = track {
        app.apply(Action::QueueTrack(track));
      }
    }
    _ => {}
  };
}

/// The row under the cursor.
fn selected_recent_track(app: &App) -> Option<&TrackInfo> {
  app
    .recently_played
    .as_ref()?
    .items
    .get(app.view.recently_played_index)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;

  fn recent(uri: Option<&str>, name: &str) -> TrackInfo {
    TrackInfo {
      uri: uri.map(|u| u.to_string()),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 1_000,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  #[test]
  fn enter_skips_uri_less_rows_and_remaps_the_offset() {
    use crate::core::pagination::CursorPaged;
    use crate::core::user_config::UserConfig;
    use crate::infra::network::IoEvent;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.recently_played = Some(CursorPaged {
      items: vec![
        recent(Some("spotify:track:one"), "One"),
        recent(None, "Local"),
        recent(Some("spotify:track:three"), "Three"),
      ],
      ..Default::default()
    });
    app.view.recently_played_index = 2;

    handler(Key::Enter, &mut app);

    match rx.try_recv() {
      Ok(IoEvent::StartPlayback(None, Some(uris), offset)) => {
        assert_eq!(uris, vec!["spotify:track:one", "spotify:track:three"]);
        assert_eq!(offset, Some(1));
      }
      _ => panic!("expected StartPlayback of the playable rows"),
    }
  }

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(
      Some(ActiveBlock::AlbumTracks),
      Some(ActiveBlock::AlbumTracks),
    );

    handler(Key::Left, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::Library);
  }

  #[test]
  fn on_esc() {
    let mut app = App::default();

    handler(Key::Esc, &mut app);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
  }
}
