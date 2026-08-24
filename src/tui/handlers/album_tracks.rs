use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::{AlbumTableContext, App};
use crate::core::plugin_api::TrackInfo;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => match app.album_table_context {
      AlbumTableContext::Full => {
        if let Some(selected_album) = &app.selected_album_full {
          let next_index = common_key_events::on_down_press_handler(
            &selected_album.album.tracks,
            Some(app.view.saved_album_tracks_index),
          );
          app.view.saved_album_tracks_index = next_index;
        };
      }
      AlbumTableContext::Simplified => {
        if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
          let next_index = common_key_events::on_down_press_handler(
            &selected_album_simplified.tracks.items,
            Some(selected_album_simplified.selected_index),
          );
          selected_album_simplified.selected_index = next_index;
        }
      }
    },
    k if common_key_events::up_event(k, &app.user_config.keys) => match app.album_table_context {
      AlbumTableContext::Full => {
        if let Some(selected_album) = &app.selected_album_full {
          let next_index = common_key_events::on_up_press_handler(
            &selected_album.album.tracks,
            Some(app.view.saved_album_tracks_index),
          );
          app.view.saved_album_tracks_index = next_index;
        };
      }
      AlbumTableContext::Simplified => {
        if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
          let next_index = common_key_events::on_up_press_handler(
            &selected_album_simplified.tracks.items,
            Some(selected_album_simplified.selected_index),
          );
          selected_album_simplified.selected_index = next_index;
        }
      }
    },
    k if common_key_events::high_event(k) => handle_high_event(app),
    k if common_key_events::middle_event(k) => handle_middle_event(app),
    k if common_key_events::low_event(k) => handle_low_event(app),
    Key::Char('s') => handle_save_event(app),
    Key::Char('w') => handle_save_album_event(app),
    Key::Enter => {
      if let Some((uri, offset)) = open_album_start(app) {
        app.apply(Action::PlayContext {
          uri,
          offset: Some(offset),
        });
      }
    }
    //recommended playlist based on selected track
    Key::Char('r') => {
      handle_recommended_tracks(app);
    }
    _ if key == app.user_config.keys.add_item_to_queue => {
      if let Some(track) = selected_album_track(app) {
        app.apply(Action::QueueTrack(track));
      }
    }
    _ => {}
  };
}

/// The track under the cursor of whichever album context is open.
fn selected_album_track(app: &App) -> Option<TrackInfo> {
  match app.album_table_context {
    AlbumTableContext::Full => app
      .selected_album_full
      .as_ref()?
      .album
      .tracks
      .get(app.view.saved_album_tracks_index)
      .cloned(),
    AlbumTableContext::Simplified => {
      let selected = app.selected_album_simplified.as_ref()?;
      selected.tracks.items.get(selected.selected_index).cloned()
    }
  }
}

/// `None` when the album has no URI: skipped instead of a bare resume (every
/// Web API album has one).
fn open_album_start(app: &App) -> Option<(String, usize)> {
  match app.album_table_context {
    AlbumTableContext::Full => {
      let selected = app.selected_album_full.as_ref()?;
      Some((
        selected.album.uri.clone()?,
        app.view.saved_album_tracks_index,
      ))
    }
    AlbumTableContext::Simplified => {
      let selected = app.selected_album_simplified.as_ref()?;
      Some((selected.album.uri.clone()?, selected.selected_index))
    }
  }
}

/// The open album's id, from whichever album context is open.
fn open_album_id(app: &App) -> Option<String> {
  match app.album_table_context {
    AlbumTableContext::Full => app.selected_album_full.as_ref()?.album.id.clone(),
    AlbumTableContext::Simplified => app.selected_album_simplified.as_ref()?.album.id.clone(),
  }
}

fn handle_high_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      let next_index = common_key_events::on_high_press_handler();
      app.view.saved_album_tracks_index = next_index;
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
        let next_index = common_key_events::on_high_press_handler();
        selected_album_simplified.selected_index = next_index;
      }
    }
  }
}

fn handle_middle_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = &app.selected_album_full {
        let next_index = common_key_events::on_middle_press_handler(&selected_album.album.tracks);
        app.view.saved_album_tracks_index = next_index;
      };
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
        let next_index =
          common_key_events::on_middle_press_handler(&selected_album_simplified.tracks.items);
        selected_album_simplified.selected_index = next_index;
      }
    }
  }
}

fn handle_low_event(app: &mut App) {
  match app.album_table_context {
    AlbumTableContext::Full => {
      if let Some(selected_album) = &app.selected_album_full {
        let next_index = common_key_events::on_low_press_handler(&selected_album.album.tracks);
        app.view.saved_album_tracks_index = next_index;
      };
    }
    AlbumTableContext::Simplified => {
      if let Some(selected_album_simplified) = &mut app.selected_album_simplified {
        let next_index =
          common_key_events::on_low_press_handler(&selected_album_simplified.tracks.items);
        selected_album_simplified.selected_index = next_index;
      }
    }
  }
}

fn handle_recommended_tracks(app: &mut App) {
  let Some(track) = selected_album_track(app) else {
    return;
  };
  if let Some(id) = track.id {
    app.apply(Action::RecommendFromTrackId {
      id,
      name: track.name,
    });
  }
}

fn handle_save_event(app: &mut App) {
  // The payload is the bare base62 id, not the track URI.
  if let Some(track_id) = selected_album_track(app).and_then(|track| track.id) {
    app.apply(Action::ToggleSaveTrack(track_id));
  }
}

fn handle_save_album_event(app: &mut App) {
  if let Some(album_id) = open_album_id(app) {
    app.apply(Action::SaveAlbum(album_id));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::action::OpenTarget;
  use crate::core::app::{ActiveBlock, RecommendationsContext};
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::{AlbumInfo, SavedAlbumInfo};
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn track(id: &str, name: &str) -> TrackInfo {
    TrackInfo {
      uri: Some(format!("spotify:track:{id}")),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album One".to_string(),
      duration_ms: 1000,
      id: Some(id.to_string()),
      album_id: Some("album1".to_string()),
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  fn app_on_a_cached_album(album_uri: Option<String>) -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.library.saved_albums.upsert_page_by_offset(Paged {
      items: vec![SavedAlbumInfo {
        album: AlbumInfo {
          id: Some("album1".to_string()),
          uri: album_uri,
          name: "Album One".to_string(),
          total_tracks: Some(2),
          tracks: vec![track("track1", "Track One"), track("track2", "Track Two")],
          ..AlbumInfo::default()
        },
        added_at: String::new(),
      }],
      offset: 0,
      limit: 1,
      total: 1,
      next: None,
      previous: None,
    });
    app.apply(Action::Open(OpenTarget::SavedAlbum("album1".to_string())));
    (app, rx)
  }

  #[test]
  fn enter_starts_the_album_context_at_the_selected_track() {
    let (mut app, rx) = app_on_a_cached_album(Some("spotify:album:album1".to_string()));
    handler(Key::Down, &mut app);

    handler(Key::Enter, &mut app);

    match rx.try_recv() {
      Ok(IoEvent::StartPlayback(context, uris, offset)) => {
        assert_eq!(context.as_deref(), Some("spotify:album:album1"));
        assert!(uris.is_none());
        assert_eq!(offset, Some(1));
      }
      _ => panic!("expected StartPlayback"),
    }
  }

  #[test]
  fn enter_on_an_album_without_a_uri_starts_nothing() {
    let (mut app, rx) = app_on_a_cached_album(None);

    handler(Key::Enter, &mut app);

    assert!(
      rx.try_recv().is_err(),
      "an unaddressable album is skipped, not resumed with an offset"
    );
  }

  #[test]
  fn s_toggles_save_for_the_selected_track() {
    let (mut app, rx) = app_on_a_cached_album(Some("spotify:album:album1".to_string()));
    handler(Key::Down, &mut app);

    handler(Key::Char('s'), &mut app);

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::ToggleSaveTrack(id)) if id == "track2"
    ));
  }

  #[test]
  fn w_saves_the_open_album() {
    let (mut app, rx) = app_on_a_cached_album(Some("spotify:album:album1".to_string()));

    handler(Key::Char('w'), &mut app);

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::CurrentUserSavedAlbumAdd(id)) if id == "album1"
    ));
  }

  #[test]
  fn r_seeds_recommendations_from_the_selected_track() {
    let (mut app, rx) = app_on_a_cached_album(Some("spotify:album:album1".to_string()));

    handler(Key::Char('r'), &mut app);

    assert_eq!(
      app.recommendations_context,
      Some(RecommendationsContext::Song)
    );
    assert_eq!(app.recommendations_seed, "Track One");
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetRecommendationsForTrackId(id, _)) if id == "track1"
    ));
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
