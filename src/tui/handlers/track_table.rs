use super::common_key_events;
use crate::core::action::{Action, ListTarget};
use crate::core::app::{App, TrackTable, TrackTableContext};
use crate::tui::event::Key;
use rspotify::prelude::Id;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      let current_index = app.view.track_table_index;
      let tracks_len = app.track_table.tracks.len();

      if tracks_len == 0 {
        return;
      }

      // Check if we're at the last track and there are more tracks to load
      if current_index == tracks_len - 1 {
        match &app.track_table.context {
          Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => {
            if app.current_playlist_has_more_tracks() {
              app.select_row_when_next_page_lands(tracks_len);
              app.apply(Action::LoadMore(ListTarget::PlaylistTracks));
              return;
            }
            app.view.track_table_index = 0;
            return;
          }
          Some(TrackTableContext::DiscoverPlaylist) => {
            // Discover playlists don't support pagination
          }
          Some(TrackTableContext::SavedTracks) => {
            if app.current_saved_tracks_has_more_tracks() {
              app.select_row_when_next_page_lands(tracks_len);
              app.apply(Action::LoadMore(ListTarget::SavedTracks));
              return;
            }
            app.view.track_table_index = 0;
            return;
          }
          _ => {}
        }
      }

      let next_index = common_key_events::on_down_press_handler(
        &app.track_table.tracks,
        Some(app.view.track_table_index),
      );
      app.view.track_table_index = next_index;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      if app.track_table.tracks.is_empty() {
        return;
      }

      let next_index = common_key_events::on_up_press_handler(
        &app.track_table.tracks,
        Some(app.view.track_table_index),
      );
      app.view.track_table_index = next_index;
    }
    k if common_key_events::high_event(k) => {
      let next_index = common_key_events::on_high_press_handler();
      app.view.track_table_index = next_index;
    }
    k if common_key_events::middle_event(k) => {
      let next_index = common_key_events::on_middle_press_handler(&app.track_table.tracks);
      app.view.track_table_index = next_index;
    }
    k if common_key_events::low_event(k) => {
      let next_index = common_key_events::on_low_press_handler(&app.track_table.tracks);
      app.view.track_table_index = next_index;
    }
    Key::Enter => {
      on_enter(app);
    }
    // Scroll down
    k if k == app.user_config.keys.next_page => {
      if let Some(context) = &app.track_table.context {
        match context {
          TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
            // Self-guarding: a no-op when there is no next page.
            app.apply(Action::LoadMore(ListTarget::PlaylistTracks));
          }
          TrackTableContext::RecommendedTracks => {}
          TrackTableContext::SavedTracks => {
            app.apply(Action::LoadMore(ListTarget::SavedTracks));
          }
          TrackTableContext::AlbumSearch => {}
          TrackTableContext::DiscoverPlaylist => {}
          // Local folders and Subsonic/YouTube/Qobuz playlists have no pagination.
          TrackTableContext::LocalPlaylist
          | TrackTableContext::SubsonicPlaylist
          | TrackTableContext::YouTubePlaylist
          | TrackTableContext::QobuzPlaylist => {}
        }
      };
    }
    // Scroll up
    k if k == app.user_config.keys.previous_page => {
      if let Some(context) = &app.track_table.context {
        match context {
          TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
            app.view.track_table_index = 0;
          }
          TrackTableContext::RecommendedTracks => {}
          TrackTableContext::SavedTracks => {
            app.view.track_table_index = 0;
          }
          TrackTableContext::AlbumSearch => {}
          TrackTableContext::DiscoverPlaylist => {}
          // Local folders and Subsonic/YouTube/Qobuz playlists have no pagination.
          TrackTableContext::LocalPlaylist
          | TrackTableContext::SubsonicPlaylist
          | TrackTableContext::YouTubePlaylist
          | TrackTableContext::QobuzPlaylist => {}
        }
      };
    }
    Key::Char('w') => {
      app.apply(Action::OpenAddTrackDialog);
    }
    Key::Char('x') => {
      app.apply(Action::OpenRemoveTrackDialog);
    }
    Key::Char('q') if app.is_playlist_track_filter_active() => {
      app.clear_playlist_track_filter();
    }
    Key::Char('s') => handle_save_track_event(app),
    Key::Char('S') => play_random_song(app),
    k if k == app.user_config.keys.jump_to_end => jump_to_end(app),
    k if k == app.user_config.keys.jump_to_start => jump_to_start(app),
    //recommended song radio
    Key::Char('r') => {
      handle_recommended_tracks(app);
    }
    _ if key == app.user_config.keys.add_item_to_queue => on_queue(app),
    // Open sort menu
    Key::Char(',') => {
      super::sort_menu::open_sort_menu(app, crate::core::sort::SortContext::PlaylistTracks);
    }
    _ => {}
  }
}

fn handle_save_track_event(app: &mut App) {
  if let Some(track) = app.track_table.tracks.get(app.view.track_table_index) {
    if let Some(playable_id) = track.uri.clone() {
      app.apply(Action::ToggleSaveTrack(playable_id));
    }
  };
}

fn handle_recommended_tracks(app: &mut App) {
  if let Some(track) = app
    .track_table
    .tracks
    .get(app.view.track_table_index)
    .cloned()
  {
    app.apply(Action::RecommendFromTrack(track));
  };
}

/// Play a random track from the table's context. Offsets are computed here so
/// `apply` stays deterministic.
fn play_random_song(app: &mut App) {
  if let Some(context) = &app.track_table.context {
    match context {
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
        let context_id = current_playlist_context_id(app);
        let track_json = current_playlist_total_tracks(app);

        // The no-id degenerate case (unreachable through normal navigation,
        // since the table only opens with an id set) is skipped rather than
        // dispatched as a bare resume with a random offset. An empty playlist
        // reports a total of 0, and `random_range(0..0)` panics, so that is
        // skipped too.
        if let (Some(uri), Some(val)) = (context_id, track_json) {
          if val == 0 {
            return;
          }
          app.apply(Action::PlayContext {
            uri,
            offset: Some(rand::random_range(0..val as usize)),
          });
        }
      }
      TrackTableContext::RecommendedTracks => {}
      TrackTableContext::SavedTracks => {
        let playable_ids: Vec<String> = app
          .track_table
          .tracks
          .iter()
          .filter_map(|track| track.uri.clone())
          .collect();
        if !playable_ids.is_empty() {
          let rand_idx = rand::random_range(0..playable_ids.len());
          app.apply(Action::PlayUris {
            uris: playable_ids,
            offset: Some(rand_idx),
          });
        }
      }
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::DiscoverPlaylist => {
        // Play random track from currently displayed discover playlist, but keep the full list
        // so next/previous can continue within the mix.
        let mut playable_ids: Vec<String> = Vec::new();
        for track in &app.track_table.tracks {
          if let Some(playable_id) = track.uri.clone() {
            playable_ids.push(playable_id);
          }
        }
        if !playable_ids.is_empty() {
          let rand_idx = rand::random_range(0..playable_ids.len());
          app.apply(Action::PlayUris {
            uris: playable_ids,
            offset: Some(rand_idx),
          });
        }
      }
      TrackTableContext::LocalPlaylist | TrackTableContext::SubsonicPlaylist => {
        // Single-file playback: play one random track from the folder/playlist.
        // Wire shape changed with the Action conversion: this used to send the
        // track in the context slot (`StartPlayback(Some(uri), None, None)`).
        // A one-item URI list is equivalent: the local, Subsonic and YouTube
        // routers start a one-track queue at offset 0 for both shapes, and the
        // queue router clears its slot on both.
        let playable_ids: Vec<String> = app
          .track_table
          .tracks
          .iter()
          .filter_map(|track| track.uri.clone())
          .collect();
        if !playable_ids.is_empty() {
          let rand_idx = rand::random_range(0..playable_ids.len());
          app.apply(Action::PlayUris {
            uris: vec![playable_ids[rand_idx].clone()],
            offset: None,
          });
        }
      }
      TrackTableContext::YouTubePlaylist | TrackTableContext::QobuzPlaylist => {
        // Queue the whole playlist and start at a random offset, so
        // Next/Previous and auto-advance keep working within the playlist.
        let playable_ids: Vec<String> = app
          .track_table
          .tracks
          .iter()
          .filter_map(|track| track.uri.clone())
          .collect();
        if !playable_ids.is_empty() {
          let rand_idx = rand::random_range(0..playable_ids.len());
          app.apply(Action::PlayUris {
            uris: playable_ids,
            offset: Some(rand_idx),
          });
        }
      }
    }
  };
}

fn jump_to_end(app: &mut App) {
  if !app.track_table.tracks.is_empty() {
    app.view.track_table_index = app.track_table.tracks.len() - 1;
  }
}

fn on_enter(app: &mut App) {
  let TrackTable { context, tracks } = &app.track_table;
  let selected_index = &app.view.track_table_index;
  if let Some(context) = &context {
    match context {
      TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch => {
        if let Some(track) = tracks.get(*selected_index) {
          // Get the track ID to play
          let track_playable_id = track.uri.clone();
          let context_id = current_playlist_context_id(app);

          // If we have a track ID, play it directly within the context
          // This ensures the selected track plays first, even with shuffle on
          if let Some(playable_id) = track_playable_id {
            if let Some(context_uri) = context_id {
              app.apply(Action::PlayTrackInContext {
                context: context_uri,
                track: playable_id,
              });
            } else {
              // Degenerate no-id case: the same context-less event this
              // path has always sent.
              app.apply(Action::PlayUris {
                uris: vec![playable_id],
                offset: Some(0), // Play the first (and only) track in the URIs list
              });
            }
          } else if let Some(context_uri) = context_id {
            // Fallback to context playback with offset
            app.apply(Action::PlayContext {
              uri: context_uri,
              offset: app.selected_playlist_track_position(),
            });
          }
          // The no-id, no-uri degenerate case (unreachable through normal
          // navigation) is skipped rather than dispatched as a bare resume
          // with an offset.
        };
      }
      TrackTableContext::RecommendedTracks | TrackTableContext::DiscoverPlaylist => {
        // The whole list is sent so playback can continue past the selection.
        let (uris, offset) = common_key_events::uri_playback_request(
          tracks.iter().map(|track| track.uri.clone()),
          *selected_index,
        );
        if !uris.is_empty() {
          app.apply(Action::PlayUris {
            uris,
            offset: Some(offset.unwrap_or(0)),
          });
        }
      }
      TrackTableContext::SavedTracks => {
        if let Some((all_playable_ids, absolute_offset)) = saved_tracks_playback_request(app) {
          app.apply(Action::PlayUris {
            uris: all_playable_ids,
            offset: Some(absolute_offset),
          });
        }
      }
      TrackTableContext::AlbumSearch => {}
      TrackTableContext::LocalPlaylist
      | TrackTableContext::SubsonicPlaylist
      | TrackTableContext::YouTubePlaylist
      | TrackTableContext::QobuzPlaylist => {
        // Queue the whole folder/playlist (in displayed order) and start at the
        // selected track, so Next/Previous/auto-advance have a queue to move
        // through. Routed to the local, subsonic or youtube player by URI
        // scheme (infra::local / infra::subsonic / infra::youtube dispatch).
        let (uris, offset) = common_key_events::uri_playback_request(
          tracks.iter().map(|track| track.uri.clone()),
          *selected_index,
        );
        if !uris.is_empty() {
          app.apply(Action::PlayUris {
            uris,
            offset: Some(offset.unwrap_or(0)),
          });
        }
      }
    }
  };
}

fn on_queue(app: &mut App) {
  // Every context except AlbumSearch holds full `TrackInfo` rows, so the queue
  // action collapses to one path: clone the selected row and hand it to the
  // native cross-source queue, which routes by URI scheme (Spotify tracks on an
  // external device still fall back to the Web-API queue). AlbumSearch rows lack
  // full track info, so it stays a no-op.
  match app.track_table.context {
    Some(TrackTableContext::AlbumSearch) | None => return,
    Some(_) => {}
  }
  if let Some(track) = app
    .track_table
    .tracks
    .get(app.view.track_table_index)
    .cloned()
  {
    app.apply(Action::QueueTrack(track));
  }
}

fn jump_to_start(app: &mut App) {
  app.view.track_table_index = 0;
}

/// The active playlist's `spotify:playlist:` context URI, for `StartPlayback`.
fn current_playlist_context_id(app: &App) -> Option<String> {
  app.current_playlist_track_table_id().map(|id| id.uri())
}

fn current_playlist_total_tracks(app: &App) -> Option<u32> {
  app.current_playlist_track_total()
}

fn saved_tracks_playback_request(app: &App) -> Option<(Vec<String>, usize)> {
  let (uris, offset) = common_key_events::uri_playback_request(
    app.track_table.tracks.iter().map(|track| track.uri.clone()),
    app.view.track_table_index,
  );
  offset.map(|offset| (uris, offset))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::PendingTrackSelection;
  use crate::core::pagination::Paged;
  use crate::core::plugin_api::{PlayableInfo, TrackInfo};
  use crate::core::test_helpers::full_track;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use chrono::Utc;
  use rspotify::model::{idtypes::PlaylistId, page::Page, track::SavedTrack};
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn saved_track(id: &str, name: &str) -> SavedTrack {
    SavedTrack {
      added_at: Utc::now(),
      track: full_track(id, name),
    }
  }

  /// A Spotify playback context on some non-native device. In a slim (no
  /// streaming) build, any Spotify context reads as an external device.
  #[allow(deprecated)]
  fn external_spotify_context() -> rspotify::model::context::CurrentPlaybackContext {
    use rspotify::model::{
      context::{Actions, CurrentPlaybackContext},
      CurrentlyPlayingType, Device, DeviceType, RepeatState,
    };
    CurrentPlaybackContext {
      device: Device {
        id: Some("external-device".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Phone".to_string(),
        _type: DeviceType::Smartphone,
        volume_percent: Some(50),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: true,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  fn playlist_item(position: u32, id: &str, name: &str) -> (u32, PlayableInfo) {
    (
      position,
      PlayableInfo::Track(TrackInfo::from(&full_track(id, name))),
    )
  }

  fn saved_tracks_page(offset: u32, ids: &[&str], has_next: bool) -> Paged<TrackInfo> {
    saved_tracks_page_with_total(offset, ids, has_next, 4)
  }

  fn saved_tracks_page_with_total(
    offset: u32,
    ids: &[&str],
    has_next: bool,
    total: u32,
  ) -> Paged<TrackInfo> {
    let rspotify_page = Page {
      href: "https://example.com/me/tracks".to_string(),
      items: ids
        .iter()
        .enumerate()
        .map(|(index, id)| saved_track(id, &format!("Track {offset}-{index}")))
        .collect(),
      limit: ids.len() as u32,
      next: has_next.then(|| "https://example.com/me/tracks?next".to_string()),
      offset,
      previous: None,
      total,
    };
    crate::infra::network::mapping::map_page(&rspotify_page, |st| TrackInfo::from(&st.track))
  }

  fn app_with_saved_tracks() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.track_table.context = Some(TrackTableContext::SavedTracks);
    (app, rx)
  }

  #[test]
  fn saved_tracks_playback_request_uses_page_zero_selection() {
    let (mut app, _rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &[
        "0000000000000000000001",
        "0000000000000000000002",
        "0000000000000000000003",
      ],
      false,
    );
    app.view.track_table_index = 1;
    app.track_table.tracks = page.items.iter().cloned().collect();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    let (uris, offset) = saved_tracks_playback_request(&app).unwrap();

    assert_eq!(offset, 1);
    assert_eq!(uris.len(), 3);
    assert_eq!(uris[offset], "spotify:track:0000000000000000000002");
  }

  #[test]
  fn saved_tracks_playback_request_uses_continuous_row_selection() {
    let (mut app, _rx) = app_with_saved_tracks();
    let first_page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let second_page = saved_tracks_page(
      2,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    );
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(first_page.clone());
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(second_page.clone());
    app.library.saved_tracks.index = 1;
    app.view.track_table_index = 3;
    app.track_table.tracks = first_page
      .items
      .iter()
      .chain(second_page.items.iter())
      .cloned()
      .collect();

    let (uris, offset) = saved_tracks_playback_request(&app).unwrap();

    assert_eq!(offset, 3);
    assert_eq!(uris.len(), 4);
    assert_eq!(uris[offset], "spotify:track:0000000000000000000004");
  }

  #[test]
  fn saved_tracks_queue_pushes_selected_row_to_native_queue() {
    let (mut app, rx) = app_with_saved_tracks();
    let first_page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let second_page = saved_tracks_page(
      2,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    );
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(first_page.clone());
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(second_page.clone());
    app.library.saved_tracks.index = 1;
    app.view.track_table_index = 3;
    app.track_table.tracks = first_page
      .items
      .iter()
      .chain(second_page.items.iter())
      .cloned()
      .collect();

    on_queue(&mut app);

    // With no external Spotify device active, the track lands in the native
    // queue and no Web-API AddItemToQueue is dispatched.
    assert_eq!(app.native_queue.len(), 1);
    assert_eq!(
      app.native_queue[0].uri.as_deref(),
      Some("spotify:track:0000000000000000000004")
    );
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn saved_tracks_queue_on_external_device_dispatches_web_api_add() {
    let (mut app, rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );
    app.library.saved_tracks.upsert_page_by_offset(page.clone());
    app.library.saved_tracks.index = 0;
    app.view.track_table_index = 1;
    app.track_table.tracks = page.items.to_vec();
    // Simulate controlling an external Spotify Connect device: any Spotify
    // context with no native streaming device counts as external in the slim
    // build, so `z` keeps today's Web-API queue behavior.
    app.current_playback_context = Some(external_spotify_context());

    on_queue(&mut app);

    match rx.recv().unwrap() {
      IoEvent::AddItemToQueue(uri) => {
        assert_eq!(uri, "spotify:track:0000000000000000000002");
      }
      other => panic!("unexpected event: {:?}", event_name(&other)),
    }
    assert!(app.native_queue.is_empty());
  }

  #[test]
  fn random_play_on_an_empty_playlist_does_nothing() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    // Seeded through App methods, not field writes: the handler write counter
    // scans this whole file. The open dispatches the first page fetch.
    app.open_playlist_tracks(
      PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
        .unwrap()
        .into_static(),
      TrackTableContext::MyPlaylists,
    );
    assert!(rx.try_recv().is_ok(), "the open fetched its first page");
    app.playlist_track_pages.upsert_page_by_offset(Paged {
      items: vec![],
      limit: 50,
      next: None,
      offset: 0,
      previous: None,
      total: 0,
    });

    // `random_range(0..0)` panics; the guard must skip the start instead.
    handler(Key::Char('S'), &mut app);

    assert!(rx.try_recv().is_err(), "an empty playlist starts nothing");
  }

  #[test]
  fn filtered_playlist_down_wraps_without_fetching_next_page() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(
      PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
        .unwrap()
        .into_static(),
    );
    app.playlist_tracks = Some(Paged {
      items: vec![],
      limit: 2,
      next: Some("https://example.com/playlists/test/items?next".to_string()),
      offset: 0,
      previous: None,
      total: 4,
    });
    app.active_playlist_track_filter = Some("track".to_string());
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];
    app.view.track_table_index = 1;

    handler(Key::Down, &mut app);

    assert_eq!(app.view.track_table_index, 0);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn q_clears_playlist_filter_and_restores_cached_rows() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static();
    let first_page = Paged {
      items: vec![
        playlist_item(0, "0000000000000000000001", "Track 1"),
        playlist_item(1, "0000000000000000000002", "Track 2"),
      ],
      limit: 2,
      next: None,
      offset: 0,
      previous: None,
      total: 2,
    };

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.playlist_track_pages.upsert_page_by_offset(first_page);
    app.active_playlist_track_filter = Some("track 2".to_string());
    app.track_table.tracks = vec![TrackInfo::from(&full_track(
      "0000000000000000000002",
      "Track 2",
    ))];
    app.playlist_track_positions = Some(vec![1]);

    handler(Key::Char('q'), &mut app);

    assert!(app.active_playlist_track_filter.is_none());
    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1]));
  }

  #[test]
  fn enter_dispatches_saved_tracks_playback_for_selected_song() {
    let (mut app, rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );
    app.view.track_table_index = 1;
    app.track_table.tracks = page.items.iter().cloned().collect();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    handler(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::StartPlayback(None, Some(uris), Some(offset)) => {
        assert_eq!(offset, 1);
        assert_eq!(uris[offset], "spotify:track:0000000000000000000002");
      }
      other => panic!("unexpected event: {:?}", event_name(&other)),
    }
  }

  #[test]
  fn empty_track_table_down_event_does_not_panic() {
    let (mut app, _rx) = app_with_saved_tracks();
    app.track_table.tracks.clear();
    app.view.track_table_index = 0;

    handler(Key::Down, &mut app);

    assert_eq!(app.view.track_table_index, 0);
  }

  #[test]
  fn down_on_last_saved_track_loads_next_continuous_row() {
    let (mut app, rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    app.view.track_table_index = 1;
    app.track_table.tracks = page.items.iter().cloned().collect();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    handler(Key::Down, &mut app);

    assert_eq!(
      app.pending_track_table_selection(),
      Some(PendingTrackSelection::Index(2))
    );
    match rx.recv().unwrap() {
      IoEvent::GetCurrentSavedTracks(Some(offset)) => assert_eq!(offset, 2),
      other => panic!("unexpected event: {:?}", event_name(&other)),
    }
  }

  #[test]
  fn next_page_on_saved_tracks_dispatches_without_moving_the_cursor() {
    let (mut app, rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    // Seeded through App methods and a key press, not field writes: the
    // handler write counter scans this whole file, tests included.
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.set_saved_tracks_to_table_continuous();
    handler(Key::Down, &mut app);
    assert_eq!(app.view.track_table_index, 1);
    assert!(rx.try_recv().is_err(), "a mid-list Down fetches nothing");

    handler(Key::Ctrl('d'), &mut app);

    // next_page fetches but does NOT set pending selection: the cursor
    // clamps into the loaded rows instead of following into the new page
    // (that follow is the down-at-last-row path's behavior).
    assert_eq!(app.pending_track_table_selection(), None);
    match rx.recv().unwrap() {
      IoEvent::GetCurrentSavedTracks(Some(offset)) => assert_eq!(offset, 2),
      other => panic!("unexpected event: {:?}", event_name(&other)),
    }
    assert_eq!(app.view.track_table_index, 1);
  }

  #[test]
  fn down_on_absolute_last_saved_track_wraps_to_start() {
    let (mut app, rx) = app_with_saved_tracks();
    let page = saved_tracks_page_with_total(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
      2,
    );
    app.view.track_table_index = 1;
    app.track_table.tracks = page.items.to_vec();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    handler(Key::Down, &mut app);

    assert_eq!(app.view.track_table_index, 0);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn up_on_first_saved_tracks_row_wraps_to_last_loaded_track() {
    let (mut app, _rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    app.view.track_table_index = 0;
    app.track_table.tracks = page.items.to_vec();
    app.library.saved_tracks.upsert_page_by_offset(page);
    app.library.saved_tracks.index = 0;

    handler(Key::Up, &mut app);

    assert_eq!(app.view.track_table_index, 1);
  }

  #[test]
  fn up_on_first_playlist_row_wraps_to_last_loaded_track() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];
    app.view.track_table_index = 0;
    app.playlist_offset = 0;

    handler(Key::Up, &mut app);

    assert_eq!(app.view.track_table_index, 1);
  }

  #[test]
  fn saved_tracks_playback_request_ignores_duplicate_page_offsets() {
    let (mut app, _rx) = app_with_saved_tracks();
    let page = saved_tracks_page(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );
    app.library.saved_tracks.add_pages(page.clone());
    app.library.saved_tracks.add_pages(page);
    app.library.saved_tracks.index = 0;
    app.view.track_table_index = 1;
    app.track_table.tracks = app.library.saved_tracks.pages[0].items.to_vec();

    let (uris, offset) = saved_tracks_playback_request(&app).unwrap();

    assert_eq!(offset, 1);
    assert_eq!(uris.len(), 2);
    assert_eq!(uris[offset], "spotify:track:0000000000000000000002");
  }

  fn event_name(event: &IoEvent) -> &'static str {
    match event {
      IoEvent::StartPlayback(_, _, _) => "StartPlayback",
      _ => "other",
    }
  }
}
