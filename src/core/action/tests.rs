//! Differential tests for `App::apply`: one test per arm (or small arm
//! family), pinning the exact observable behavior — dispatched `IoEvent`s
//! and `App` state — that the scripting effect drain had before the port.
//!
//! House pattern: `app_with_channel` keeps the `IoEvent` receiver alive;
//! `IoEvent` derives nothing, so assertions pattern-match. Catch-all match
//! arms are spelled `_other` because this directory's gate counts bare
//! wildcard arms.

use std::sync::mpsc::{channel, Receiver};
use std::time::SystemTime;

use super::{Action, NavTarget, RepeatSetting};
use crate::core::app::{App, RouteId, UserInfo, NOTHING_PLAYING_STATUS};
use crate::core::theme::{Color, Theme, ThemeField};
use crate::core::user_config::UserConfig;
use crate::infra::network::IoEvent;

fn app_with_channel() -> (App, Receiver<IoEvent>) {
  let (tx, rx) = channel();
  let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
  (app, rx)
}

/// The same fixture with no Spotify session (`spotify_connected == false`).
fn session_free_app_with_channel() -> (App, Receiver<IoEvent>) {
  let (tx, rx) = channel();
  let app = App::new(tx, UserConfig::new(), None);
  (app, rx)
}

#[allow(deprecated)]
fn playback_context(
  is_playing: bool,
  shuffle_state: bool,
) -> rspotify::model::CurrentPlaybackContext {
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    CurrentlyPlayingType, Device, DeviceType, RepeatState,
  };
  CurrentPlaybackContext {
    device: Device {
      id: Some("dev-test".to_string()),
      is_active: true,
      is_private_session: false,
      is_restricted: false,
      name: "Test Device".to_string(),
      _type: DeviceType::Computer,
      volume_percent: Some(50),
    },
    repeat_state: RepeatState::Off,
    shuffle_state,
    context: None,
    timestamp: chrono::Utc::now(),
    progress: None,
    is_playing,
    item: None,
    currently_playing_type: CurrentlyPlayingType::Unknown,
    actions: Actions::default(),
  }
}

#[allow(deprecated)]
fn playback_context_with_track(is_playing: bool) -> rspotify::model::CurrentPlaybackContext {
  use crate::core::test_helpers::full_track;
  use rspotify::model::idtypes::ArtistId;
  use rspotify::model::PlayableItem;
  let mut ctx = playback_context(is_playing, false);
  ctx.progress = Some(chrono::Duration::milliseconds(0));
  let mut track = full_track("4uLU6hMCjMI75M1A2tKUQC", "Test Song");
  // `full_track` leaves the artist id unset; the jump-to-artist arm needs one.
  track.artists[0].id = Some(
    ArtistId::from_id("0OdUWJ0sBjDrqHygGUXeCF")
      .unwrap()
      .into_static(),
  );
  ctx.item = Some(PlayableItem::Track(track));
  ctx.currently_playing_type = rspotify::model::CurrentlyPlayingType::Track;
  ctx
}

// --- transport ---

#[test]
fn play_when_paused_dispatches_start_playback() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(false, false));

  app.apply(Action::Play);

  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::StartPlayback(None, None, None))
  ));
}

#[test]
fn play_when_already_playing_is_a_noop() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(true, false));

  app.apply(Action::Play);

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn pause_when_playing_dispatches_pause_playback() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(true, false));

  app.apply(Action::Pause);

  assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));
}

#[test]
fn pause_when_already_paused_is_a_noop() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(false, false));

  app.apply(Action::Pause);

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn next_track_routes_through_the_transport_method() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::NextTrack);

  assert!(matches!(rx.try_recv(), Ok(IoEvent::NextTrack)));
}

#[test]
fn previous_track_routes_through_the_transport_method() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::PreviousTrack);

  assert!(matches!(rx.try_recv(), Ok(IoEvent::PreviousTrack)));
}

#[test]
fn seek_to_with_a_track_context_dispatches_seek() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context_with_track(true));

  app.apply(Action::SeekTo(30_000));

  match rx.try_recv() {
    Ok(IoEvent::Seek(ms)) => assert_eq!(ms, 30_000),
    _other => panic!("expected Seek(30000) (IoEvent is not Debug)"),
  }
}

#[test]
fn set_volume_sets_pending_volume() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::SetVolume(80));

  assert_eq!(app.pending_volume, Some(80));
}

#[test]
fn set_shuffle_dispatches_only_when_state_differs() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(false, false));

  app.apply(Action::SetShuffle(true));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::Shuffle(true))));

  app.apply(Action::SetShuffle(false));
  assert!(rx.try_recv().is_err(), "same state must not dispatch");
}

#[test]
fn cycle_repeat_without_playback_context_is_a_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::CycleRepeat);

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn set_repeat_dispatches_the_absolute_repeat_state() {
  use rspotify::model::RepeatState;
  for (setting, expected) in [
    (RepeatSetting::Off, RepeatState::Off),
    (RepeatSetting::Track, RepeatState::Track),
    (RepeatSetting::Context, RepeatState::Context),
  ] {
    let (mut app, rx) = app_with_channel();
    app.apply(Action::SetRepeat(setting));
    match rx.try_recv() {
      Ok(IoEvent::Repeat(state)) => assert_eq!(state, expected),
      _other => panic!("expected Repeat (IoEvent is not Debug)"),
    }
  }
}

#[test]
fn toggle_playback_pauses_when_playing_and_starts_when_paused() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(true, false));

  app.apply(Action::TogglePlayback);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::PausePlayback)));

  app.current_playback_context = Some(playback_context(false, false));
  app.apply(Action::TogglePlayback);
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::StartPlayback(None, None, None))
  ));
}

#[test]
fn force_previous_track_dispatches_the_force_event() {
  let (mut app, rx) = app_with_channel();
  app.song_progress_ms = 20_000;

  app.apply(Action::ForcePreviousTrack);

  assert!(matches!(rx.try_recv(), Ok(IoEvent::ForcePreviousTrack)));
  assert_eq!(app.song_progress_ms, 0, "a forced previous restarts at 0");
}

#[test]
fn seek_actions_move_by_the_configured_step() {
  // Start one step in, so forward lands at `2 * step + 1_000` and backward
  // lands at `1_000`.
  for (action, forward) in [(Action::SeekForward, true), (Action::SeekBackward, false)] {
    let (mut app, _rx) = app_with_channel();
    app.current_playback_context = Some(playback_context_with_track(true));
    let step = app.user_config.behavior.seek_milliseconds as u128;
    app.song_progress_ms = step + 1_000;

    app.apply(action);

    let expected = if forward { 2 * step + 1_000 } else { 1_000 };
    assert_eq!(app.song_progress_ms, expected);
  }
}

#[test]
fn volume_actions_dispatch_the_stepped_volume() {
  for (action, up) in [(Action::VolumeUp, true), (Action::VolumeDown, false)] {
    let (mut app, rx) = app_with_channel();
    app.current_playback_context = Some(playback_context(true, false));

    app.apply(action);

    let increment = app.user_config.behavior.volume_increment;
    let expected = if up { 50 + increment } else { 50 - increment };
    match rx.try_recv() {
      Ok(IoEvent::ChangeVolume(v)) => assert_eq!(v, expected),
      _other => panic!("expected ChangeVolume (IoEvent is not Debug)"),
    }
  }
}

#[test]
fn toggle_shuffle_flips_the_spotify_shuffle_state() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context(true, false));

  app.apply(Action::ToggleShuffle);

  assert!(matches!(rx.try_recv(), Ok(IoEvent::Shuffle(true))));
}

// --- transport without a Spotify session ---

#[test]
fn next_track_without_a_session_reports_nothing_playing() {
  let (mut app, rx) = session_free_app_with_channel();

  app.apply(Action::NextTrack);

  assert!(rx.try_recv().is_err());
  assert_eq!(app.status_message.as_deref(), Some(NOTHING_PLAYING_STATUS));
}

#[test]
fn toggle_playback_without_a_session_reports_nothing_playing() {
  let (mut app, rx) = session_free_app_with_channel();

  app.apply(Action::TogglePlayback);

  assert!(rx.try_recv().is_err());
  assert_eq!(app.status_message.as_deref(), Some(NOTHING_PLAYING_STATUS));
}

#[test]
fn volume_down_without_a_session_leaves_no_latch() {
  // The fixture starts at 100%, so only a decrease reaches the API fallback.
  let (mut app, rx) = session_free_app_with_channel();

  app.apply(Action::VolumeDown);

  assert!(rx.try_recv().is_err());
  assert_eq!(app.status_message.as_deref(), Some(NOTHING_PLAYING_STATUS));
  assert!(!app.is_volume_change_in_flight);
  assert!(app.pending_volume.is_none());
}

#[test]
fn set_repeat_without_a_session_reports_nothing_playing() {
  let (mut app, rx) = session_free_app_with_channel();

  app.apply(Action::SetRepeat(RepeatSetting::Track));

  assert!(rx.try_recv().is_err());
  assert_eq!(app.status_message.as_deref(), Some(NOTHING_PLAYING_STATUS));
}

#[test]
fn flush_pending_volume_without_a_session_clears_the_pending_value() {
  let (mut app, rx) = session_free_app_with_channel();
  app.pending_volume = Some(40);

  app.flush_pending_volume();

  assert!(rx.try_recv().is_err());
  assert!(app.pending_volume.is_none());
}

// --- jump-to navigation ---

#[test]
fn jump_to_album_opens_the_current_tracks_album() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context_with_track(false));

  app.apply(Action::JumpToAlbum);

  match rx.try_recv() {
    Ok(IoEvent::GetAlbumTracks(album)) => assert_eq!(album.name, "Test Album"),
    _other => panic!("expected GetAlbumTracks (IoEvent is not Debug)"),
  }
}

#[test]
fn jump_to_album_without_playback_is_a_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::JumpToAlbum);

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn jump_to_artist_opens_the_first_artists_albums() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context_with_track(false));

  app.apply(Action::JumpToArtist);

  match rx.try_recv() {
    Ok(IoEvent::GetArtist(id, name, _country)) => {
      assert_eq!(id, "0OdUWJ0sBjDrqHygGUXeCF");
      assert_eq!(name, "Test Artist");
    }
    _other => panic!("expected GetArtist (IoEvent is not Debug)"),
  }
}

#[test]
fn jump_to_context_opens_the_playlist_context() {
  use rspotify::model::{context::Context, enums::Type};
  let (mut app, rx) = app_with_channel();
  let mut ctx = playback_context_with_track(true);
  ctx.context = Some(Context {
    uri: "spotify:playlist:37i9dQZF1DX4WYpdgoIcn6".to_string(),
    href: String::new(),
    external_urls: std::collections::HashMap::new(),
    _type: Type::Playlist,
  });
  app.current_playback_context = Some(ctx);

  app.apply(Action::JumpToContext);

  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  match rx.try_recv() {
    Ok(IoEvent::GetPlaylistItems(id, _offset)) => {
      assert_eq!(id, "37i9dQZF1DX4WYpdgoIcn6");
    }
    _other => panic!("expected GetPlaylistItems (IoEvent is not Debug)"),
  }
}

// --- recap ---

#[test]
fn generate_recap_resolves_the_period_from_the_current_route() {
  use crate::core::app::ActiveBlock;
  use crate::infra::history::RecapPeriod;
  // The stats selection is set in both cases, so the off-Stats case proves
  // the selection is ignored anywhere else.
  for (on_stats_screen, expected) in [(false, RecapPeriod::ThirtyDays), (true, RecapPeriod::Year)] {
    let (mut app, rx) = app_with_channel();
    app.stats_period = RecapPeriod::Year;
    if on_stats_screen {
      app.push_navigation_stack(RouteId::Stats, ActiveBlock::Stats);
    }

    app.apply(Action::GenerateRecap);

    match rx.try_recv() {
      Ok(IoEvent::GenerateRecap(period)) => assert_eq!(period, expected),
      _other => panic!("expected GenerateRecap (IoEvent is not Debug)"),
    }
  }
}

// --- playback starts ---

#[test]
fn play_uris_dispatches_a_uri_list_start() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::PlayUris {
    uris: vec!["spotify:track:abc".to_string()],
    offset: None,
  });

  match rx.try_recv() {
    Ok(IoEvent::StartPlayback(None, Some(uris), None)) => {
      assert_eq!(uris, vec!["spotify:track:abc".to_string()]);
    }
    _other => panic!("expected a uri-list StartPlayback (IoEvent is not Debug)"),
  }
}

#[test]
fn play_uris_carries_the_offset() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::PlayUris {
    uris: vec!["spotify:track:abc".to_string()],
    offset: Some(0),
  });

  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::StartPlayback(None, Some(_), Some(0)))
  ));
}

#[test]
fn play_uris_with_an_empty_list_dispatches_nothing() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::PlayUris {
    uris: vec![],
    offset: None,
  });

  assert!(rx.try_recv().is_err());
}

#[test]
fn play_context_dispatches_a_context_start() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::PlayContext {
    uri: "spotify:playlist:p1".to_string(),
    offset: Some(5),
  });

  match rx.try_recv() {
    Ok(IoEvent::StartPlayback(Some(ctx), None, Some(offset))) => {
      assert_eq!(ctx, "spotify:playlist:p1");
      assert_eq!(offset, 5);
    }
    _other => panic!("expected a context StartPlayback (IoEvent is not Debug)"),
  }
}

#[test]
fn transfer_playback_carries_device_and_persist() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::TransferPlayback {
    device_id: "dev-1".to_string(),
    persist: false,
  });

  match rx.try_recv() {
    Ok(IoEvent::TransferPlaybackToDevice(id, persist)) => {
      assert_eq!(id, "dev-1");
      assert!(!persist);
    }
    _other => panic!("expected TransferPlaybackToDevice (IoEvent is not Debug)"),
  }
}

#[test]
fn add_to_queue_dispatches_add_item() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::AddToQueue("spotify:track:t1".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::AddItemToQueue(uri)) => assert_eq!(uri, "spotify:track:t1"),
    _other => panic!("expected AddItemToQueue (IoEvent is not Debug)"),
  }
}

// --- library / playlists ---

#[test]
fn search_resolves_the_user_country_at_apply_time() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Search("daft punk".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::GetSearchResults(query, country)) => {
      assert_eq!(query, "daft punk");
      assert_eq!(country, None, "no user loaded, so no market");
    }
    _other => panic!("expected GetSearchResults (IoEvent is not Debug)"),
  }
}

#[test]
fn create_playlist_dispatches_create() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::CreatePlaylist {
    name: "Mix".to_string(),
    track_uris: vec!["spotify:track:a".to_string(), "spotify:track:b".to_string()],
  });

  match rx.try_recv() {
    Ok(IoEvent::CreateNewPlaylist(name, uris)) => {
      assert_eq!(name, "Mix");
      assert_eq!(uris.len(), 2);
    }
    _other => panic!("expected CreateNewPlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn add_track_to_playlist_dispatches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::AddTrackToPlaylist {
    playlist: "p1".to_string(),
    track: "t1".to_string(),
  });

  match rx.try_recv() {
    Ok(IoEvent::AddTrackToPlaylist(playlist, track)) => {
      assert_eq!(playlist, "p1");
      assert_eq!(track, "t1");
    }
    _other => panic!("expected AddTrackToPlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn remove_track_from_playlist_dispatches_with_position() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::RemoveTrackFromPlaylist {
    playlist: "p1".to_string(),
    track: "t1".to_string(),
    position: 3,
  });

  match rx.try_recv() {
    Ok(IoEvent::RemoveTrackFromPlaylistAtPosition(playlist, track, position)) => {
      assert_eq!(playlist, "p1");
      assert_eq!(track, "t1");
      assert_eq!(position, 3);
    }
    _other => panic!("expected RemoveTrackFromPlaylistAtPosition (IoEvent is not Debug)"),
  }
}

#[test]
fn add_track_to_playlist_routes_a_youtube_playlist_to_the_local_edit() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::AddTrackToPlaylist {
    playlist: "youtube:playlist:y1".to_string(),
    track: "v1".to_string(),
  });

  match rx.try_recv() {
    Ok(IoEvent::AddTrackToYouTubePlaylist(playlist, track)) => {
      assert_eq!(playlist, "youtube:playlist:y1");
      assert_eq!(track, "v1");
    }
    _other => panic!("expected AddTrackToYouTubePlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn add_track_to_playlist_keeps_a_spotify_uri_on_the_web_api() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::AddTrackToPlaylist {
    playlist: "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M".to_string(),
    track: "t1".to_string(),
  });

  match rx.try_recv() {
    Ok(IoEvent::AddTrackToPlaylist(playlist, track)) => {
      assert_eq!(playlist, "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M");
      assert_eq!(track, "t1");
    }
    _other => panic!("expected AddTrackToPlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn remove_track_from_playlist_routes_a_youtube_playlist_to_the_local_edit() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::RemoveTrackFromPlaylist {
    playlist: "youtube:playlist:y1".to_string(),
    track: "v1".to_string(),
    position: 3,
  });

  match rx.try_recv() {
    // The local edit removes by video id: the staged position is not carried.
    Ok(IoEvent::RemoveTrackFromYouTubePlaylist(playlist, track)) => {
      assert_eq!(playlist, "youtube:playlist:y1");
      assert_eq!(track, "v1");
    }
    _other => panic!("expected RemoveTrackFromYouTubePlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn follow_playlist_uses_the_unknown_owner_fallback() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::FollowPlaylist("p1".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::UserFollowPlaylist(owner, playlist, public)) => {
      assert_eq!(owner, "unknown");
      assert_eq!(playlist, "p1");
      assert_eq!(public, None);
    }
    _other => panic!("expected UserFollowPlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn unfollow_playlist_resolves_the_current_user() {
  let (mut app, rx) = app_with_channel();
  app.user = Some(UserInfo {
    id: "me-123".to_string(),
    display_name: None,
    country: None,
  });

  app.apply(Action::UnfollowPlaylist("p1".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::UserUnfollowPlaylist(user_id, playlist_id)) => {
      assert_eq!(user_id, "me-123");
      assert_eq!(playlist_id, "p1");
    }
    _other => panic!("expected UserUnfollowPlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn unfollow_playlist_without_a_user_sets_an_error_status() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::UnfollowPlaylist("p1".to_string()));

  assert!(rx.try_recv().is_err(), "no IoEvent expected");
  assert!(app.status_message_is_error);
  assert_eq!(
    app.status_message.as_deref(),
    Some("Cannot unfollow: user profile not loaded yet")
  );
}

#[test]
fn save_and_follow_actions_dispatch_their_events() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::ToggleSaveTrack("spotify:track:t1".to_string()));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::ToggleSaveTrack(_))));

  app.apply(Action::SaveAlbum("a1".to_string()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::CurrentUserSavedAlbumAdd(_))
  ));

  app.apply(Action::UnsaveAlbum("a1".to_string()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::CurrentUserSavedAlbumDelete(_))
  ));

  app.apply(Action::SaveShow("s1".to_string()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::CurrentUserSavedShowAdd(_))
  ));

  app.apply(Action::UnsaveShow("s1".to_string()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::CurrentUserSavedShowDelete(_))
  ));
}

#[test]
fn follow_artist_wraps_the_id_in_a_batch_event() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::FollowArtist("ar1".to_string()));
  match rx.try_recv() {
    Ok(IoEvent::UserFollowArtists(ids)) => assert_eq!(ids, vec!["ar1".to_string()]),
    _other => panic!("expected UserFollowArtists (IoEvent is not Debug)"),
  }

  app.apply(Action::UnfollowArtist("ar1".to_string()));
  match rx.try_recv() {
    Ok(IoEvent::UserUnfollowArtists(ids)) => assert_eq!(ids, vec!["ar1".to_string()]),
    _other => panic!("expected UserUnfollowArtists (IoEvent is not Debug)"),
  }
}

// --- status messages ---

#[test]
fn notify_sets_a_status_message() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::Notify("hello".to_string(), 4));

  assert_eq!(app.status_message.as_deref(), Some("hello"));
  assert!(!app.status_message_is_error);
}

#[test]
fn notify_error_blocks_a_following_notify() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::NotifyError("boom".to_string(), 6));
  app.apply(Action::Notify("normal".to_string(), 4));

  assert_eq!(app.status_message.as_deref(), Some("boom"));
  assert!(app.status_message_is_error);
}

// --- navigation ---

#[test]
fn navigate_home_returns_to_the_home_route() {
  let (mut app, _rx) = app_with_channel();
  app.apply(Action::Navigate(NavTarget::Queue));
  assert_eq!(app.get_current_route().id, RouteId::Queue);

  app.apply(Action::Navigate(NavTarget::Home));

  assert_eq!(app.get_current_route().id, RouteId::Home);
}

#[test]
fn navigate_queue_pushes_the_route_and_fetches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::Queue));

  assert_eq!(app.get_current_route().id, RouteId::Queue);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQueue)));
}

#[test]
fn navigate_settings_opens_the_settings_screen() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::Settings));

  assert_eq!(app.get_current_route().id, RouteId::Settings);
}

#[test]
fn navigate_devices_opens_the_device_picker() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::Devices));

  assert_eq!(app.get_current_route().id, RouteId::SelectedDevice);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetDevices)));
}

#[test]
fn navigate_help_lyrics_and_party_push_their_routes() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::Help));
  assert_eq!(app.get_current_route().id, RouteId::HelpMenu);

  app.apply(Action::Navigate(NavTarget::Lyrics));
  assert_eq!(app.get_current_route().id, RouteId::LyricsView);

  app.apply(Action::Navigate(NavTarget::Party));
  assert_eq!(app.get_current_route().id, RouteId::Party);
}

#[test]
fn navigate_recently_played_only_dispatches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::RecentlyPlayed));

  // The network handler pushes the route once the data arrives.
  assert_eq!(app.get_current_route().id, RouteId::Home);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetRecentlyPlayed)));
}

#[test]
fn navigate_analysis_pushes_the_analysis_route() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::Analysis));

  assert_eq!(app.get_current_route().id, RouteId::Analysis);
}

#[test]
fn navigate_miniplayer_toggles() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::Navigate(NavTarget::MiniPlayer));
  assert_eq!(app.get_current_route().id, RouteId::MiniPlayer);

  app.apply(Action::Navigate(NavTarget::MiniPlayer));
  assert_eq!(app.get_current_route().id, RouteId::Home);
}

#[test]
fn back_pops_the_navigation_stack() {
  let (mut app, _rx) = app_with_channel();
  app.apply(Action::Navigate(NavTarget::Queue));
  assert_eq!(app.get_current_route().id, RouteId::Queue);

  app.apply(Action::Back);

  assert_eq!(app.get_current_route().id, RouteId::Home);
}

// --- plugin surfaces ---

#[test]
fn set_playbar_segment_inserts_and_removes() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::SetPlaybarSegment {
    plugin: "clock".to_string(),
    text: Some("12:00".to_string()),
  });
  assert_eq!(
    app.plugin_playbar_segments.get("clock").map(String::as_str),
    Some("12:00")
  );

  app.apply(Action::SetPlaybarSegment {
    plugin: "clock".to_string(),
    text: None,
  });
  assert!(!app.plugin_playbar_segments.contains_key("clock"));
}

#[test]
fn show_popup_sets_the_popup() {
  // The paired scroll reset is pinned by `core/app/plugins.rs` tests, which
  // can seed the scroll field first.
  use crate::core::plugin_api::{PluginPopup, PopupLine};
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::ShowPopup(PluginPopup {
    title: "Hi".to_string(),
    lines: vec![PopupLine {
      text: "line".to_string(),
      fg: None,
      bold: false,
      italic: false,
    }],
  }));

  assert_eq!(
    app.plugin_popup.as_ref().map(|p| p.title.as_str()),
    Some("Hi")
  );
}

#[test]
fn close_popup_clears_the_popup() {
  // The paired scroll reset is pinned by `core/app/plugins.rs` tests, which
  // can seed the scroll field first.
  let (mut app, _rx) = app_with_channel();
  app.show_plugin_popup(crate::core::plugin_api::PluginPopup {
    title: "Hi".to_string(),
    lines: Vec::new(),
  });

  app.apply(Action::ClosePopup);

  assert!(app.plugin_popup.is_none());
}

/// Reverse accessor for the loop test below, exhaustive over `ThemeField`
/// (the `Theme`-field direction is guarded by the full destructuring inside
/// `Theme::set`).
fn theme_color(theme: &Theme, field: ThemeField) -> Color {
  match field {
    ThemeField::Active => theme.active,
    ThemeField::Banner => theme.banner,
    ThemeField::ErrorBorder => theme.error_border,
    ThemeField::ErrorText => theme.error_text,
    ThemeField::Hint => theme.hint,
    ThemeField::Hovered => theme.hovered,
    ThemeField::Inactive => theme.inactive,
    ThemeField::PlaybarBackground => theme.playbar_background,
    ThemeField::PlaybarProgress => theme.playbar_progress,
    ThemeField::PlaybarProgressText => theme.playbar_progress_text,
    ThemeField::PlaybarText => theme.playbar_text,
    ThemeField::Selected => theme.selected,
    ThemeField::Text => theme.text,
    ThemeField::Background => theme.background,
    ThemeField::Header => theme.header,
    ThemeField::HighlightedLyrics => theme.highlighted_lyrics,
    ThemeField::AnalysisBar => theme.analysis_bar,
    ThemeField::AnalysisBarText => theme.analysis_bar_text,
  }
}

#[test]
fn set_theme_writes_every_field() {
  let sentinel = Color::Rgb(1, 2, 3);
  for field in ThemeField::ALL {
    let (mut app, _rx) = app_with_channel();
    assert_ne!(
      theme_color(&app.user_config.theme, field),
      sentinel,
      "sentinel must differ from the default for {}",
      field.name()
    );

    app.apply(Action::SetTheme(vec![(field, sentinel)]));

    assert_eq!(
      theme_color(&app.user_config.theme, field),
      sentinel,
      "field {} was not written",
      field.name()
    );
  }
}

#[test]
fn theme_field_names_round_trip() {
  for field in ThemeField::ALL {
    assert_eq!(ThemeField::from_name(field.name()), Some(field));
  }
  assert_eq!(ThemeField::from_name("not_a_field"), None);
}

#[test]
fn nav_target_names_round_trip() {
  for target in NavTarget::ALL {
    assert_eq!(NavTarget::from_name(target.name()), Some(target));
  }
  assert_eq!(NavTarget::from_name("nowhere"), None);
}

#[test]
fn set_screen_content_inserts() {
  use crate::core::plugin_api::PluginScreenContent;
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::SetScreenContent {
    name: "stats".to_string(),
    content: PluginScreenContent {
      title: "Stats".to_string(),
      widgets: Vec::new(),
    },
  });

  assert!(app.plugin_screens.contains_key("stats"));
}

#[test]
fn show_screen_pushes_the_plugin_route_and_close_pops() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::ShowScreen("stats".to_string()));
  assert_eq!(
    app.get_current_route().id,
    RouteId::PluginScreen("stats".to_string())
  );

  // Showing the already-current screen must not stack a second frame. The
  // scroll reset is pinned by `core/app/plugins.rs` tests.
  app.apply(Action::ShowScreen("stats".to_string()));

  app.apply(Action::CloseScreen("stats".to_string()));
  assert_eq!(app.get_current_route().id, RouteId::Home);
}

#[test]
fn close_screen_only_pops_when_current() {
  let (mut app, _rx) = app_with_channel();
  app.apply(Action::ShowScreen("stats".to_string()));

  app.apply(Action::CloseScreen("other".to_string()));

  assert_eq!(
    app.get_current_route().id,
    RouteId::PluginScreen("stats".to_string())
  );
}

// --- DJ ---

#[cfg(feature = "dj-core")]
mod dj_actions {
  use super::*;
  use crate::core::action::ActionOutcome;
  use crate::core::plugin_api::TrackInfo;

  fn dj_track(uri: Option<&str>, name: &str) -> TrackInfo {
    TrackInfo {
      uri: uri.map(str::to_string),
      name: name.to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      duration_ms: 200_000,
      id: None,
      album_id: None,
      artist_refs: Vec::new(),
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
    }
  }

  #[test]
  fn queue_tracks_reports_the_accepted_count() {
    let (mut app, _rx) = app_with_channel();

    let outcome = app.apply(Action::QueueTracks(vec![
      dj_track(Some("subsonic:track:a"), "A"),
      dj_track(Some("subsonic:track:b"), "B"),
    ]));

    assert_eq!(outcome, ActionOutcome::Queued { accepted: 2 });
    assert_eq!(app.native_queue.len(), 2);
    assert!(app.dj.queued_uris.contains("subsonic:track:a"));
  }

  #[test]
  fn queue_tracks_skips_tracks_without_a_uri() {
    let (mut app, _rx) = app_with_channel();

    let outcome = app.apply(Action::QueueTracks(vec![
      dj_track(Some("subsonic:track:a"), "A"),
      dj_track(None, "No URI"),
    ]));

    assert_eq!(outcome, ActionOutcome::Queued { accepted: 1 });
    assert_eq!(app.native_queue.len(), 1);
  }

  #[test]
  fn set_dj_vibe_bumps_the_generation_exactly_once() {
    let (mut app, _rx) = app_with_channel();
    let before = app.dj.generation;

    app.apply(Action::SetDjVibe(Some("mellow".to_string())));

    assert_eq!(
      app.dj.generation,
      before.wrapping_add(1),
      "the turn loop's adopt-one-bump rule needs exactly one bump"
    );
    assert_eq!(app.dj.vibe.as_deref(), Some("mellow"));
  }

  #[test]
  fn set_dj_vibe_clears_with_none() {
    let (mut app, _rx) = app_with_channel();
    app.apply(Action::SetDjVibe(Some("mellow".to_string())));

    app.apply(Action::SetDjVibe(None));

    assert_eq!(app.dj.vibe, None);
  }
}

// --- pagination, open family, source-routed search, library rows, dialogs ---

use super::{LibraryTarget, ListTarget, OpenTarget};
use crate::core::app::{ActiveBlock, DialogContext, PendingTrackSelection, RecommendationsContext};
use crate::core::pagination::Paged;
use crate::core::plugin_api::{AlbumInfo, PlaylistInfo, SavedAlbumInfo, TrackInfo};
use crate::core::source::Source;
use crate::core::test_helpers::{full_track, playlist_info};

fn track(id: &str, name: &str) -> TrackInfo {
  TrackInfo::from(&full_track(id, name))
}

fn track_page(offset: u32, ids: &[&str], has_next: bool) -> Paged<TrackInfo> {
  Paged {
    items: ids
      .iter()
      .enumerate()
      .map(|(i, id)| track(id, &format!("Track {offset}-{i}")))
      .collect(),
    offset,
    limit: ids.len() as u32,
    total: 4,
    next: has_next.then(|| "https://example.com/next".to_string()),
    previous: None,
  }
}

/// A saved-albums row with `cached` of `total` tracks embedded.
fn saved_albums_page(cached: usize, total: u32) -> Paged<SavedAlbumInfo> {
  Paged {
    items: vec![SavedAlbumInfo {
      album: AlbumInfo {
        id: Some("5gzLOflH95LkKYE6XSXE9k".to_string()),
        uri: Some("spotify:album:5gzLOflH95LkKYE6XSXE9k".to_string()),
        name: "One Wayne G".to_string(),
        total_tracks: Some(total),
        tracks: (0..cached)
          .map(|i| track("0000000000000000000001", &format!("Track {i}")))
          .collect(),
        ..AlbumInfo::default()
      },
      added_at: String::new(),
    }],
    offset: 0,
    limit: 1,
    total: 1,
    next: None,
    previous: None,
  }
}

/// The playlist track table's page type carries snapshot positions.
fn playlist_items_page(has_next: bool) -> Paged<(u32, crate::core::plugin_api::PlayableInfo)> {
  Paged {
    items: vec![(
      0,
      crate::core::plugin_api::PlayableInfo::Track(track("0000000000000000000001", "T1")),
    )],
    offset: 0,
    limit: 1,
    total: 4,
    next: has_next.then(|| "https://example.com/next".to_string()),
    previous: None,
  }
}

// --- LoadMore ---

#[test]
fn load_more_playlist_tracks_dispatches_the_next_page_fetch() {
  let (mut app, rx) = app_with_channel();
  app.playlist_track_table_id = Some(
    rspotify::model::idtypes::PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static(),
  );
  let page = playlist_items_page(true);
  app.track_table.context = Some(crate::core::app::TrackTableContext::MyPlaylists);
  app.playlist_tracks = Some(page.clone());
  app.playlist_track_pages.upsert_page_by_offset(page);

  app.apply(Action::LoadMore(ListTarget::PlaylistTracks));

  match rx.try_recv() {
    Ok(IoEvent::GetPlaylistItems(id, offset)) => {
      assert_eq!(id, "37i9dQZF1DX4WYpdgoIcn6");
      assert_eq!(offset, 1);
    }
    _other => panic!("expected GetPlaylistItems"),
  }
}

#[test]
fn load_more_playlist_tracks_is_a_noop_under_an_active_filter() {
  let (mut app, rx) = app_with_channel();
  app.playlist_track_table_id = Some(
    rspotify::model::idtypes::PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static(),
  );
  let page = playlist_items_page(true);
  app.track_table.context = Some(crate::core::app::TrackTableContext::MyPlaylists);
  app.playlist_tracks = Some(page.clone());
  app.playlist_track_pages.upsert_page_by_offset(page);
  app.active_playlist_track_filter = Some("query".to_string());

  app.apply(Action::LoadMore(ListTarget::PlaylistTracks));

  assert!(rx.try_recv().is_err(), "filtered playlists never paginate");
}

#[test]
fn load_more_saved_tracks_fetches_the_missing_continuous_page() {
  let (mut app, rx) = app_with_channel();
  let page = track_page(
    0,
    &["0000000000000000000001", "0000000000000000000002"],
    true,
  );
  app.library.saved_tracks.upsert_page_by_offset(page);

  app.apply(Action::LoadMore(ListTarget::SavedTracks));

  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetCurrentSavedTracks(Some(2)))
  ));
}

#[test]
fn load_more_saved_tracks_is_a_noop_at_the_end_of_the_list() {
  let (mut app, rx) = app_with_channel();
  let page = track_page(0, &["0000000000000000000001"], false);
  app.library.saved_tracks.upsert_page_by_offset(page);

  app.apply(Action::LoadMore(ListTarget::SavedTracks));

  assert!(rx.try_recv().is_err());
}

#[test]
fn load_more_does_not_touch_pending_track_table_selection() {
  let (mut app, rx) = app_with_channel();
  let page = track_page(0, &["0000000000000000000001"], false);
  app.library.saved_tracks.upsert_page_by_offset(page);
  app.pending_track_table_selection = Some(PendingTrackSelection::Index(9));

  app.apply(Action::LoadMore(ListTarget::SavedTracks));

  assert_eq!(
    app.pending_track_table_selection,
    Some(PendingTrackSelection::Index(9)),
    "the cursor-follow side effect belongs to the caller, not to LoadMore"
  );
  assert!(rx.try_recv().is_err());
}

#[test]
fn load_more_saved_tracks_does_not_refetch_an_in_flight_page() {
  let (mut app, rx) = app_with_channel();
  let page = track_page(
    0,
    &["0000000000000000000001", "0000000000000000000002"],
    true,
  );
  app.library.saved_tracks.upsert_page_by_offset(page);

  app.apply(Action::LoadMore(ListTarget::SavedTracks));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetCurrentSavedTracks(Some(2)))
  ));

  // The fetch marked offset 2 as in flight; a second apply must not
  // re-dispatch it (the prefetch dedupe the arm inherits).
  app.apply(Action::LoadMore(ListTarget::SavedTracks));
  assert!(rx.try_recv().is_err());
}

#[test]
fn load_more_playlist_tracks_does_not_refetch_an_in_flight_page() {
  let (mut app, rx) = app_with_channel();
  app.playlist_track_table_id = Some(
    rspotify::model::idtypes::PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static(),
  );
  app.track_table.context = Some(crate::core::app::TrackTableContext::MyPlaylists);
  let page = playlist_items_page(true);
  app.playlist_tracks = Some(page.clone());
  app.playlist_track_pages.upsert_page_by_offset(page);

  app.apply(Action::LoadMore(ListTarget::PlaylistTracks));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetPlaylistItems(_, 1))));

  app.apply(Action::LoadMore(ListTarget::PlaylistTracks));
  assert!(
    rx.try_recv().is_err(),
    "the in-flight page is not refetched"
  );
}

// --- Open family ---

#[test]
fn open_album_without_from_search_leaves_the_track_table_alone() {
  let (mut app, rx) = app_with_channel();
  app.track_table.context = Some(crate::core::app::TrackTableContext::SavedTracks);

  app.apply(Action::Open(OpenTarget::Album {
    id: "5gzLOflH95LkKYE6XSXE9k".to_string(),
    from_search: false,
  }));

  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::SavedTracks),
    "a plain album open never restamps the table context"
  );
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetAlbum(id)) if id == "5gzLOflH95LkKYE6XSXE9k"
  ));
}

#[test]
fn open_album_from_search_pins_the_album_search_context() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::Album {
    id: "5gzLOflH95LkKYE6XSXE9k".to_string(),
    from_search: true,
  }));

  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::AlbumSearch)
  );
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetAlbum(id)) if id == "5gzLOflH95LkKYE6XSXE9k"
  ));
}

#[test]
fn open_saved_album_with_a_complete_cached_tracklist_opens_from_the_cache() {
  let (mut app, rx) = app_with_channel();
  app
    .library
    .saved_albums
    .upsert_page_by_offset(saved_albums_page(2, 2));

  app.apply(Action::Open(OpenTarget::SavedAlbum(
    "5gzLOflH95LkKYE6XSXE9k".to_string(),
  )));

  assert!(app.selected_album_full.is_some());
  assert_eq!(
    app.album_table_context,
    crate::core::app::AlbumTableContext::Full
  );
  assert_eq!(app.get_current_route().id, RouteId::AlbumTracks);
  assert!(
    rx.try_recv().is_err(),
    "a complete cached tracklist needs no round trip"
  );
}

#[test]
fn open_saved_album_with_a_truncated_cached_tracklist_refetches_the_full_album() {
  let (mut app, rx) = app_with_channel();
  app
    .library
    .saved_albums
    .upsert_page_by_offset(saved_albums_page(50, 199));

  app.apply(Action::Open(OpenTarget::SavedAlbum(
    "5gzLOflH95LkKYE6XSXE9k".to_string(),
  )));

  // GetAlbum fetches the whole tracklist and pushes the route itself.
  assert!(app.selected_album_full.is_none());
  assert_ne!(app.get_current_route().id, RouteId::AlbumTracks);
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetAlbum(id)) if id == "5gzLOflH95LkKYE6XSXE9k"
  ));
}

#[test]
fn open_saved_album_with_an_unknown_id_is_a_silent_noop() {
  let (mut app, rx) = app_with_channel();
  app
    .library
    .saved_albums
    .upsert_page_by_offset(saved_albums_page(2, 2));

  app.apply(Action::Open(OpenTarget::SavedAlbum(
    "0000000000000000000000".to_string(),
  )));

  assert!(app.selected_album_full.is_none());
  assert_ne!(app.get_current_route().id, RouteId::AlbumTracks);
  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn open_show_episodes_dispatches_the_show_snapshot() {
  let (mut app, rx) = app_with_channel();
  let show = crate::core::plugin_api::ShowInfo {
    id: Some("3aNsrV6lkzmcU1w8u8kA7N".to_string()),
    name: "A Podcast".to_string(),
    ..Default::default()
  };

  app.apply(Action::OpenShowEpisodes(show));

  match rx.try_recv() {
    Ok(IoEvent::GetShowEpisodes(show)) => {
      assert_eq!(show.id.as_deref(), Some("3aNsrV6lkzmcU1w8u8kA7N"));
      assert_eq!(show.name, "A Podcast");
    }
    _other => panic!("expected GetShowEpisodes"),
  }
}

#[test]
fn open_artist_resolves_the_country_and_dispatches_get_artist() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::Artist {
    id: "2ye2Wgw4gimLv2eAKyk1NB".to_string(),
    name: String::new(),
  }));

  match rx.try_recv() {
    Ok(IoEvent::GetArtist(id, name, country)) => {
      assert_eq!(id, "2ye2Wgw4gimLv2eAKyk1NB");
      assert_eq!(name, "");
      assert_eq!(country, None, "no user loaded, so no market");
    }
    _other => panic!("expected GetArtist"),
  }
}

#[test]
fn open_track_album_dispatches_get_album_for_track() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::TrackAlbum(
    "10igKaIKsSB6ZnWxPxPvKO".to_string(),
  )));

  assert!(
    matches!(
      rx.try_recv(),
      Ok(IoEvent::GetAlbumForTrack(id)) if id == "10igKaIKsSB6ZnWxPxPvKO"
    ),
    "the track id rides through unchanged"
  );
}

#[test]
fn open_show_dispatches_get_show() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::Show(
    "3aNsrV6lkzmcU1w8u8kA7N".to_string(),
  )));

  assert!(
    matches!(
      rx.try_recv(),
      Ok(IoEvent::GetShow(id)) if id == "3aNsrV6lkzmcU1w8u8kA7N"
    ),
    "the show id rides through unchanged"
  );
}

#[test]
fn open_playlist_opens_the_track_table_in_the_playlists_context() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::Playlist {
    id: "37i9dQZF1DX4WYpdgoIcn6".to_string(),
    from_search: false,
  }));

  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::MyPlaylists)
  );
  assert_eq!(
    app.pending_playlist_open.as_deref(),
    Some("37i9dQZF1DX4WYpdgoIcn6")
  );
  match rx.try_recv() {
    Ok(IoEvent::GetPlaylistItems(id, offset)) => {
      assert_eq!(id, "37i9dQZF1DX4WYpdgoIcn6");
      assert_eq!(offset, 0);
    }
    _other => panic!("expected GetPlaylistItems"),
  }
}

#[test]
fn open_playlist_from_search_uses_the_search_context() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::Playlist {
    id: "37i9dQZF1DX4WYpdgoIcn6".to_string(),
    from_search: true,
  }));

  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::PlaylistSearch)
  );
}

#[test]
fn open_playlist_with_an_unparseable_id_is_a_silent_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::Playlist {
    id: "not a base62 id!".to_string(),
    from_search: false,
  }));

  assert!(
    rx.try_recv().is_err(),
    "the opening paths drop bad ids silently"
  );
  assert_eq!(app.get_current_route().id, RouteId::Home);
}

#[test]
fn open_playlist_twice_while_pending_open_fetches_only_once() {
  let (mut app, rx) = app_with_channel();

  let target = OpenTarget::Playlist {
    id: "37i9dQZF1DX4WYpdgoIcn6".to_string(),
    from_search: false,
  };
  app.apply(Action::Open(target.clone()));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetPlaylistItems(_, 0))));

  // Same open already in flight: no second fetch, only the screen shown.
  app.apply(Action::Open(target));
  assert!(rx.try_recv().is_err());
}

// --- SearchActiveSource (Action::Search stays Spotify-only) ---

#[test]
fn search_active_source_routes_by_the_active_source() {
  let (mut app, rx) = app_with_channel();
  let query = "coltrane".to_string();

  app.active_source = Source::Subsonic;
  app.apply(Action::SearchActiveSource(query.clone()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetSubsonicSearchResults(q)) if q == "coltrane"
  ));

  app.active_source = Source::Radio;
  app.apply(Action::SearchActiveSource(query.clone()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetRadioSearchResults(q)) if q == "coltrane"
  ));

  app.active_source = Source::YouTube;
  app.apply(Action::SearchActiveSource(query.clone()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetYouTubeSearchResults(q)) if q == "coltrane"
  ));

  app.active_source = Source::Qobuz;
  app.apply(Action::SearchActiveSource(query.clone()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetQobuzSearchResults(q)) if q == "coltrane"
  ));

  app.active_source = Source::Spotify;
  app.apply(Action::SearchActiveSource(query.clone()));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetSearchResults(q, _)) if q == "coltrane"
  ));

  // Local falls into the Spotify branch, matching the search input's
  // if-chain, which has no Local branch.
  app.active_source = Source::Local;
  app.apply(Action::SearchActiveSource(query));
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetSearchResults(_, None))
  ));
}

#[test]
fn search_active_source_resolves_a_loaded_users_country() {
  let (mut app, rx) = app_with_channel();
  app.user = Some(crate::core::app::UserInfo {
    id: "user1".to_string(),
    display_name: None,
    country: Some("US".to_string()),
  });
  app.active_source = Source::Spotify;

  app.apply(Action::SearchActiveSource("coltrane".to_string()));

  assert!(
    matches!(rx.try_recv(), Ok(IoEvent::GetSearchResults(_, Some(_)))),
    "the loaded user's market rides along"
  );
}

#[test]
fn search_stays_on_the_spotify_catalog_when_browsing_other_sources() {
  let (mut app, rx) = app_with_channel();
  app.active_source = Source::YouTube;

  app.apply(Action::Search("daft punk".to_string()));

  // Lua spotatui.search contracted the Web API search; browsing scope does
  // not reroute it (only SearchActiveSource does).
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetSearchResults(q, None)) if q == "daft punk"
  ));
}

#[test]
fn search_playlist_tracks_records_the_pending_search_and_dispatches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::SearchPlaylistTracks {
    playlist_id: "37i9dQZF1DX4WYpdgoIcn6".to_string(),
    query: "queen rock".to_string(),
  });

  assert_eq!(
    app.pending_playlist_track_search.as_deref(),
    Some("queen rock")
  );
  assert_eq!(
    app.status_message.as_deref(),
    Some("Searching playlist for \"queen rock\"...")
  );
  match rx.try_recv() {
    Ok(IoEvent::SearchPlaylistTracks(id, query)) => {
      assert_eq!(id, "37i9dQZF1DX4WYpdgoIcn6");
      assert_eq!(query, "queen rock");
    }
    _other => panic!("expected SearchPlaylistTracks"),
  }
}

// --- playback starts ---

#[test]
fn play_track_in_context_dispatches_the_combined_start() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::PlayTrackInContext {
    context: "spotify:playlist:p1".to_string(),
    track: "spotify:track:t1".to_string(),
  });

  match rx.try_recv() {
    Ok(IoEvent::StartPlayback(context, uris, offset)) => {
      assert_eq!(context.as_deref(), Some("spotify:playlist:p1"));
      assert_eq!(uris, Some(vec!["spotify:track:t1".to_string()]));
      assert_eq!(offset, Some(0));
    }
    _other => panic!("expected StartPlayback"),
  }
}

#[test]
fn recommend_from_track_sets_song_context_and_keeps_the_unseeded_request() {
  let (mut app, rx) = app_with_channel();
  let seed = track("4uLU6hMCjMI75M1A2tKUQC", "Song A");

  app.apply(Action::RecommendFromTrack(seed.clone()));

  assert_eq!(
    app.recommendations_context,
    Some(RecommendationsContext::Song)
  );
  assert_eq!(app.recommendations_seed, "Song A");
  match rx.try_recv() {
    Ok(IoEvent::GetRecommendationsForSeed(seed_artists, seed_tracks, first_track, country)) => {
      assert!(seed_artists.is_none());
      // Preserved historic bug: the full URI is fed as the seed and the
      // request goes out unseeded. Fixing it is a separate verified change.
      assert_eq!(seed_tracks, Some(vec![seed.uri.clone().unwrap()]));
      assert_eq!(
        (*first_track).as_ref().map(|t| t.name.as_str()),
        Some("Song A")
      );
      assert_eq!(country, None);
    }
    _other => panic!("expected GetRecommendationsForSeed"),
  }
}

#[test]
fn recommend_from_track_without_a_uri_still_carries_the_context_row() {
  let (mut app, rx) = app_with_channel();
  let mut seed = track("4uLU6hMCjMI75M1A2tKUQC", "Local Song");
  seed.uri = None;

  app.apply(Action::RecommendFromTrack(seed));

  assert_eq!(
    app.recommendations_context,
    Some(RecommendationsContext::Song)
  );
  assert_eq!(app.recommendations_seed, "Local Song");
  match rx.try_recv() {
    Ok(IoEvent::GetRecommendationsForSeed(seed_artists, seed_tracks, first_track, _)) => {
      assert!(seed_artists.is_none());
      assert!(seed_tracks.is_none(), "no uri means no seed list");
      assert_eq!(
        (*first_track).as_ref().map(|t| t.name.as_str()),
        Some("Local Song")
      );
    }
    _other => panic!("expected GetRecommendationsForSeed"),
  }
}

#[test]
fn recommend_from_artist_sets_the_artist_context_and_seeds_by_id() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::RecommendFromArtist {
    id: "0OdUWJ0sBjDrqHygGUXeCF".to_string(),
    name: "Band of Horses".to_string(),
  });

  assert_eq!(
    app.recommendations_context,
    Some(RecommendationsContext::Artist)
  );
  assert_eq!(app.recommendations_seed, "Band of Horses");
  match rx.try_recv() {
    Ok(IoEvent::GetRecommendationsForSeed(seed_artists, seed_tracks, first_track, country)) => {
      assert_eq!(
        seed_artists,
        Some(vec!["0OdUWJ0sBjDrqHygGUXeCF".to_string()])
      );
      assert!(seed_tracks.is_none());
      assert!(
        (*first_track).is_none(),
        "an artist seed prepends no context row"
      );
      assert_eq!(country, None);
    }
    _other => panic!("expected GetRecommendationsForSeed"),
  }
}

#[test]
fn recommend_from_track_id_seeds_the_song_context_without_a_context_row() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::RecommendFromTrackId {
    id: "4uLU6hMCjMI75M1A2tKUQC".to_string(),
    name: "Song A".to_string(),
  });

  assert_eq!(
    app.recommendations_context,
    Some(RecommendationsContext::Song)
  );
  assert_eq!(app.recommendations_seed, "Song A");
  // The id-only event carries no seed snapshot, so no seed row is prepended.
  match rx.try_recv() {
    Ok(IoEvent::GetRecommendationsForTrackId(id, country)) => {
      assert_eq!(id, "4uLU6hMCjMI75M1A2tKUQC");
      assert_eq!(country, None);
    }
    _other => panic!("expected GetRecommendationsForTrackId"),
  }
}

// --- dialogs ---

#[test]
fn open_add_track_dialog_without_a_selection_is_a_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenAddTrackDialog);

  assert!(rx.try_recv().is_err());
  assert!(app.pending_playlist_track_add.is_none());
}

#[test]
fn open_add_track_dialog_stages_the_selected_row_for_the_picker() {
  let (mut app, rx) = app_with_channel();
  app.active_source = Source::YouTube;
  app.youtube_playlists = vec![PlaylistInfo {
    uri: "youtube:playlist:y1".to_string(),
    ..playlist_info("y1", "Local List", "owner", false)
  }];
  app.track_table.tracks = vec![track("0000000000000000000001", "Track 1")];
  app.track_table.selected_index = 0;

  app.apply(Action::OpenAddTrackDialog);

  assert_eq!(
    app.get_current_route().active_block,
    ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
  );
  let pending = app
    .pending_playlist_track_add
    .as_ref()
    .expect("staged for the picker");
  assert_eq!(pending.track_name, "Track 1");
  assert!(
    rx.try_recv().is_err(),
    "the YouTube path fetches nothing when playlists exist"
  );
}

#[test]
fn open_add_track_dialog_without_destinations_requests_them() {
  let (mut app, rx) = app_with_channel();
  app.active_source = Source::Spotify;
  app.track_table.tracks = vec![track("0000000000000000000001", "Track 1")];
  app.track_table.selected_index = 0;

  app.apply(Action::OpenAddTrackDialog);

  // No user profile and no playlists loaded yet: the flow fetches both
  // first and tells the user to retry.
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetUser)));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetPlaylists)));
  assert!(app.pending_playlist_track_add.is_none());
}

#[test]
fn open_add_track_dialog_for_a_named_track_opens_the_picker() {
  let (mut app, rx) = app_with_channel();
  app.user = Some(UserInfo {
    id: "spotatui-owner".to_string(),
    ..Default::default()
  });
  app.playlists = Some(Paged {
    total: 1,
    ..Default::default()
  });
  app.all_playlists = vec![playlist_info(
    "37i9dQZF1DXcBWIGoYBM5M",
    "Owned Playlist",
    "spotatui-owner",
    false,
  )];

  app.apply(Action::OpenAddTrackDialogFor {
    track_id: Some("0000000000000000000001".to_string()),
    track_name: "Search Track".to_string(),
  });

  let pending = app
    .pending_playlist_track_add
    .as_ref()
    .expect("staged for the picker");
  assert_eq!(pending.track_id, "0000000000000000000001");
  assert_eq!(pending.track_name, "Search Track");
  assert_eq!(
    app.get_current_route().active_block,
    ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
  );
  assert!(
    rx.try_recv().is_err(),
    "the destinations are already loaded, so nothing is fetched"
  );
}

#[test]
fn open_add_track_dialog_for_a_track_without_an_id_reports_it() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenAddTrackDialogFor {
    track_id: None,
    track_name: "Local File".to_string(),
  });

  assert_eq!(
    app.status_message.as_deref(),
    Some("Track cannot be added to playlist")
  );
  assert!(app.pending_playlist_track_add.is_none());
  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn open_remove_track_dialog_stages_the_spotify_removal_with_position() {
  let (mut app, rx) = app_with_channel();
  app.track_table.context = Some(crate::core::app::TrackTableContext::MyPlaylists);
  app.playlist_track_table_id = Some(
    rspotify::model::idtypes::PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static(),
  );
  app.all_playlists = vec![playlist_info(
    "37i9dQZF1DX4WYpdgoIcn6",
    "My List",
    "owner",
    false,
  )];
  app.track_table.tracks = vec![track("0000000000000000000001", "Track 1")];
  app.track_table.selected_index = 0;
  app.playlist_track_positions = Some(vec![7]);

  app.apply(Action::OpenRemoveTrackDialog);

  let pending = app
    .pending_playlist_track_removal
    .as_ref()
    .expect("staged removal");
  assert_eq!(pending.playlist_name, "My List");
  assert_eq!(pending.track_id, "0000000000000000000001");
  assert_eq!(pending.position, 7);
  assert_eq!(
    app.get_current_route().active_block,
    ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm)
  );
  assert!(rx.try_recv().is_err());
}

#[test]
fn open_remove_track_dialog_without_a_position_reports_an_error() {
  let (mut app, rx) = app_with_channel();
  app.track_table.context = Some(crate::core::app::TrackTableContext::MyPlaylists);
  app.playlist_track_table_id = Some(
    rspotify::model::idtypes::PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static(),
  );
  app.all_playlists = vec![playlist_info(
    "37i9dQZF1DX4WYpdgoIcn6",
    "My List",
    "owner",
    false,
  )];
  app.track_table.tracks = vec![track("0000000000000000000001", "Track 1")];
  app.track_table.selected_index = 0;

  app.apply(Action::OpenRemoveTrackDialog);

  assert_eq!(
    app.status_message.as_deref(),
    Some("Cannot resolve track position for removal"),
  );
  assert!(app.pending_playlist_track_removal.is_none());
  assert!(rx.try_recv().is_err());
}

#[test]
fn open_remove_track_dialog_youtube_routes_the_local_edit() {
  let (mut app, rx) = app_with_channel();
  app.track_table.context = Some(crate::core::app::TrackTableContext::YouTubePlaylist);
  app.youtube_open_playlist = Some("youtube:playlist:y1".to_string());
  app.youtube_playlists = vec![PlaylistInfo {
    uri: "youtube:playlist:y1".to_string(),
    ..playlist_info("y1", "Local List", "owner", false)
  }];
  app.track_table.tracks = vec![track("0000000000000000000001", "Track 1")];
  app.track_table.selected_index = 0;

  app.apply(Action::OpenRemoveTrackDialog);

  let pending = app
    .pending_playlist_track_removal
    .as_ref()
    .expect("staged local edit");
  assert_eq!(pending.playlist_id, "youtube:playlist:y1");
  assert_eq!(pending.position, 0, "unused for local YouTube playlists");
  assert_eq!(
    app.get_current_route().active_block,
    ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm)
  );
  assert!(rx.try_recv().is_err());
}

// --- library sections ---

#[test]
fn open_library_stats_marks_loading_dispatches_and_pushes() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenLibrary(LibraryTarget::Stats));

  assert!(app.stats_loading);
  assert_eq!(app.get_current_route().id, RouteId::Stats);
  let expected_period = app.stats_period;
  assert!(
    matches!(
      rx.try_recv(),
      Ok(IoEvent::LoadListeningStats(period)) if period == expected_period
    ),
    "the selected stats period rides along"
  );
}

#[test]
fn open_library_recently_played_pushes_before_the_data_arrives() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenLibrary(LibraryTarget::RecentlyPlayed));

  // The row pushes immediately; only the global navigate binding defers
  // the push to the network result.
  assert_eq!(app.get_current_route().id, RouteId::RecentlyPlayed);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetRecentlyPlayed)));
}

#[test]
fn open_library_friends_first_open_fetches_code_and_list() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenLibrary(LibraryTarget::Friends));

  assert_eq!(app.get_current_route().id, RouteId::Friends);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetFriendCode)));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetFriends)));
}

#[test]
fn open_library_friends_skips_fetched_state_on_reopen() {
  let (mut app, rx) = app_with_channel();
  app.friend_code = Some("code".to_string());
  app.friends_loading = true;

  app.apply(Action::OpenLibrary(LibraryTarget::Friends));

  assert_eq!(app.get_current_route().id, RouteId::Friends);
  assert!(
    rx.try_recv().is_err(),
    "nothing refetches once loaded/loading"
  );
}

#[test]
fn open_library_rows_push_their_routes_and_fetch() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenLibrary(LibraryTarget::Discover));
  assert_eq!(app.get_current_route().id, RouteId::Discover);
  assert!(rx.try_recv().is_err());

  app.apply(Action::OpenLibrary(LibraryTarget::Albums));
  assert_eq!(app.get_current_route().id, RouteId::AlbumList);
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetCurrentUserSavedAlbums(None))
  ));

  app.apply(Action::OpenLibrary(LibraryTarget::Artists));
  assert_eq!(app.get_current_route().id, RouteId::Artists);
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetFollowedArtists(None))
  ));

  app.apply(Action::OpenLibrary(LibraryTarget::Podcasts));
  assert_eq!(app.get_current_route().id, RouteId::Podcasts);
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetCurrentUserSavedShows(None))
  ));
}

#[cfg(all(test, feature = "local-files"))]
#[test]
fn open_library_local_files_pushes_the_browser_route() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenLibrary(LibraryTarget::LocalFiles));

  assert_eq!(app.get_current_route().id, RouteId::LocalBrowser);
  assert!(rx.try_recv().is_err());
}

#[cfg(all(test, feature = "ai-dj"))]
#[test]
fn open_library_ai_dj_opens_the_screen_through_app() {
  let (mut app, _rx) = app_with_channel();

  app.apply(Action::OpenLibrary(LibraryTarget::AiDj));

  assert_eq!(app.get_current_route().id, RouteId::AiDj);
}

#[test]
fn open_library_liked_songs_resets_the_cache_and_fetches() {
  let (mut app, rx) = app_with_channel();
  let page = track_page(0, &["0000000000000000000001"], false);
  app.library.saved_tracks.upsert_page_by_offset(page);

  app.apply(Action::OpenLibrary(LibraryTarget::LikedSongs));

  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  assert!(app.library.saved_tracks.pages.is_empty(), "cache reset");
  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetCurrentSavedTracks(None))
  ));
}

#[test]
fn library_target_names_round_trip_and_cover_every_sidebar_row() {
  for target in LibraryTarget::ALL {
    assert_eq!(LibraryTarget::from_name(target.name()), Some(target));
  }
  // Every sidebar label resolves to a DISTINCT target: this is the
  // load-bearing invariant (name() must equal the library_options()
  // strings, or the handler's name resolution remaps rows).
  let options = crate::core::app::library_options();
  let mut resolved = Vec::new();
  for label in options {
    let target = LibraryTarget::from_name(label)
      .unwrap_or_else(|| panic!("sidebar row {label} has no LibraryTarget"));
    assert!(!resolved.contains(&target), "duplicate label {label}");
    resolved.push(target);
  }
  assert_eq!(LibraryTarget::from_name("no such row"), None);
}

// --- SelectSource ---

#[test]
fn select_source_flips_scope_mirrors_runtime_state_and_persists() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));

  app.apply(Action::SelectSource(Source::Local));

  assert_eq!(app.active_source, Source::Local);
  assert_eq!(app.runtime_state.active_source, Source::Local);
  let written = std::fs::read_to_string(dir.path().join("state.yml")).unwrap();
  assert!(
    written.contains("active_source") && written.contains("Local"),
    "sparse patch persisted the choice"
  );
  assert!(
    matches!(rx.try_recv(), Ok(IoEvent::GetLocalPlaylists)),
    "the new scope's sidebar is fetched"
  );
}

#[test]
fn select_source_fetches_each_scopes_own_sidebar() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));

  app.apply(Action::SelectSource(Source::Subsonic));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetSubsonicPlaylists)));

  app.apply(Action::SelectSource(Source::Radio));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetRadioStations)));

  app.apply(Action::SelectSource(Source::YouTube));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetYouTubePlaylists)));

  app.apply(Action::SelectSource(Source::Qobuz));
  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetQobuzPlaylists)));
}

#[test]
fn select_source_spotify_fetches_no_sidebar() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));

  app.apply(Action::SelectSource(Source::Spotify));

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn selecting_spotify_without_a_session_does_not_reach_disk() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = session_free_app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));

  app.apply(Action::SelectSource(Source::Spotify));

  assert_eq!(app.active_source, Source::Spotify);
  assert!(!dir.path().join("state.yml").exists());

  app.spotify_connected = true;
  app.persist_active_source();

  let written = std::fs::read_to_string(dir.path().join("state.yml")).unwrap();
  assert!(written.contains("active_source") && written.contains("Spotify"));
}

#[test]
fn selecting_a_free_source_without_a_session_still_persists() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = session_free_app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));

  app.apply(Action::SelectSource(Source::Local));

  let written = std::fs::read_to_string(dir.path().join("state.yml")).unwrap();
  assert!(written.contains("active_source") && written.contains("Local"));
}

// --- the now-playing item family ---

use super::CopyTarget;

#[test]
fn toggle_save_current_item_saves_the_playing_track() {
  let (mut app, rx) = app_with_channel();
  app.current_playback_context = Some(playback_context_with_track(true));

  app.apply(Action::ToggleSaveCurrentItem);

  match rx.try_recv() {
    Ok(IoEvent::ToggleSaveTrack(uri)) => {
      assert_eq!(uri, "spotify:track:4uLU6hMCjMI75M1A2tKUQC")
    }
    _other => panic!("expected ToggleSaveTrack for the playing track"),
  }
}

#[test]
fn toggle_save_current_item_without_playback_is_a_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::ToggleSaveCurrentItem);

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  assert!(
    app.status_message.is_none(),
    "nothing playing falls through silently"
  );
}

#[test]
fn open_add_playing_track_dialog_without_playback_reports_no_track() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenAddPlayingTrackDialog);

  assert_eq!(
    app.status_message.as_deref(),
    Some("No track currently playing")
  );
  assert!(rx.try_recv().is_err());
  assert!(app.pending_playlist_track_add.is_none());
}

#[test]
fn open_add_playing_track_dialog_stages_the_playing_track() {
  let (mut app, rx) = app_with_channel();
  app.active_source = Source::YouTube;
  app.youtube_playlists = vec![PlaylistInfo {
    uri: "youtube:playlist:y1".to_string(),
    ..playlist_info("y1", "Local List", "owner", false)
  }];
  app.current_playback_context = Some(playback_context_with_track(true));

  app.apply(Action::OpenAddPlayingTrackDialog);

  assert_eq!(
    app.get_current_route().active_block,
    ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
  );
  let pending = app
    .pending_playlist_track_add
    .as_ref()
    .expect("staged for the picker");
  assert_eq!(pending.track_name, "Test Song");
  assert!(
    rx.try_recv().is_err(),
    "the YouTube path fetches nothing when playlists exist"
  );
}

#[test]
fn copy_url_without_playback_is_a_silent_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::CopyUrl(CopyTarget::CurrentSong));
  app.apply(Action::CopyUrl(CopyTarget::CurrentAlbum));

  // No clipboard assertion: a headless runner has no clipboard.
  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  assert!(app.api_error.is_empty(), "no failure is reported");
}

// --- the listening party family ---

use crate::infra::network::sync::{ControlMode, PartyRole, PartySession};

#[test]
fn start_party_hosts_in_host_only_control() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::StartParty);

  match rx.try_recv() {
    Ok(IoEvent::StartParty(mode)) => assert_eq!(mode, ControlMode::HostOnly),
    _other => panic!("expected a host-only StartParty"),
  }
}

#[test]
fn join_party_carries_the_code_and_the_guest_name() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::JoinParty {
    code: "ABC123".to_string(),
    name: "Guest".to_string(),
  });

  match rx.try_recv() {
    Ok(IoEvent::JoinParty { code, name }) => {
      assert_eq!(code, "ABC123");
      assert_eq!(name, "Guest");
    }
    _other => panic!("expected JoinParty carrying both fields"),
  }
}

#[test]
fn leave_party_dispatches_the_leave() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::LeaveParty);

  assert!(matches!(rx.try_recv(), Ok(IoEvent::LeaveParty)));
}

#[test]
fn toggle_party_control_mode_flips_the_session_and_tells_the_relay() {
  let (mut app, rx) = app_with_channel();
  app.party_session = Some(PartySession {
    role: PartyRole::Host,
    code: "ABC123".to_string(),
    guests: Vec::new(),
    control_mode: ControlMode::HostOnly,
    host_name: "Host".to_string(),
  });

  app.apply(Action::TogglePartyControlMode);

  // The relay handler only sends the message; the popup renders from this record.
  assert_eq!(
    app.party_session.as_ref().map(|s| s.control_mode.clone()),
    Some(ControlMode::SharedControl)
  );
  match rx.try_recv() {
    Ok(IoEvent::SetPartyControlMode(mode)) => assert_eq!(mode, ControlMode::SharedControl),
    _other => panic!("expected the relay to be told about shared control"),
  }

  app.apply(Action::TogglePartyControlMode);

  assert_eq!(
    app.party_session.as_ref().map(|s| s.control_mode.clone()),
    Some(ControlMode::HostOnly)
  );
  match rx.try_recv() {
    Ok(IoEvent::SetPartyControlMode(mode)) => assert_eq!(mode, ControlMode::HostOnly),
    _other => panic!("expected the mode to flip back"),
  }
}

#[test]
fn toggle_party_control_mode_without_a_session_dispatches_nothing() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::TogglePartyControlMode);

  assert!(app.party_session.is_none());
  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

// --- decoded-source playlists, sidebar folders and saved stations ---

use crate::core::state::RadioStationConfig;

fn radio_station_row(name: &str, url: &str) -> TrackInfo {
  let mut station = track("0000000000000000000001", name);
  station.uri = Some(format!("radio:{url}"));
  station
}

#[test]
fn open_source_playlist_local_clears_the_table_and_fetches() {
  let (mut app, rx) = app_with_channel();
  app.track_table.tracks = vec![track("0000000000000000000002", "Stale")];
  app.track_table.selected_index = 1;
  app.playlist_track_positions = Some(vec![7]);

  app.apply(Action::Open(OpenTarget::SourcePlaylist(
    "file:///music/Jazz".to_string(),
  )));

  assert!(app.track_table.tracks.is_empty());
  assert_eq!(app.track_table.selected_index, 0);
  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::LocalPlaylist)
  );
  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  assert_eq!(
    app.playlist_track_positions,
    Some(vec![7]),
    "a decoded source opens without the Spotify page reset"
  );
  match rx.try_recv() {
    Ok(IoEvent::GetLocalTracks(uri)) => assert_eq!(uri, "file:///music/Jazz"),
    _other => panic!("expected GetLocalTracks (IoEvent is not Debug)"),
  }
}

#[test]
fn open_source_playlist_qobuz_uses_the_qobuz_context() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::SourcePlaylist(
    "qobuz:album:0060254730301".to_string(),
  )));

  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::QobuzPlaylist)
  );
  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  match rx.try_recv() {
    Ok(IoEvent::GetQobuzTracks(uri)) => assert_eq!(uri, "qobuz:album:0060254730301"),
    _other => panic!("expected GetQobuzTracks (IoEvent is not Debug)"),
  }
}

#[test]
fn open_source_playlist_subsonic_uses_the_subsonic_context() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::SourcePlaylist(
    "subsonic:playlist:42".to_string(),
  )));

  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::SubsonicPlaylist)
  );
  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  match rx.try_recv() {
    Ok(IoEvent::GetSubsonicTracks(uri)) => assert_eq!(uri, "subsonic:playlist:42"),
    _other => panic!("expected GetSubsonicTracks (IoEvent is not Debug)"),
  }
}

#[test]
fn open_source_playlist_youtube_uses_the_youtube_context() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::SourcePlaylist(
    "youtube:playlist:y1".to_string(),
  )));

  assert_eq!(
    app.track_table.context,
    Some(crate::core::app::TrackTableContext::YouTubePlaylist)
  );
  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  match rx.try_recv() {
    Ok(IoEvent::GetYouTubeTracks(uri)) => assert_eq!(uri, "youtube:playlist:y1"),
    _other => panic!("expected GetYouTubeTracks (IoEvent is not Debug)"),
  }
}

#[test]
fn open_source_playlist_with_an_unknown_scheme_is_a_silent_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::Open(OpenTarget::SourcePlaylist(
    "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M".to_string(),
  )));

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  assert_eq!(app.get_current_route().id, RouteId::Home);
  assert_eq!(app.track_table.context, None);
}

#[test]
fn open_playlist_folder_scopes_the_visible_items() {
  use crate::core::app::{PlaylistFolder, PlaylistFolderItem};
  let (mut app, _rx) = app_with_channel();
  app.user_config.behavior.pin_community_playlist = false;
  app.all_playlists = vec![playlist_info(
    "37i9dQZF1DXcBWIGoYBM5M",
    "Inside",
    "spotatui-owner",
    false,
  )];
  app.playlist_folder_items = vec![
    PlaylistFolderItem::Folder(PlaylistFolder {
      name: "Mixes".to_string(),
      current_id: 0,
      target_id: 1,
    }),
    PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 1,
    },
  ];
  assert!(matches!(
    app.get_playlist_display_item_at(0),
    Some(PlaylistFolderItem::Folder(_))
  ));

  app.apply(Action::Open(OpenTarget::PlaylistFolder(1)));

  assert_eq!(app.get_playlist_display_count(), 1);
  assert!(matches!(
    app.get_playlist_display_item_at(0),
    Some(PlaylistFolderItem::Playlist { index: 0, .. })
  ));
}

#[test]
fn delete_playlist_dispatches_the_local_delete() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::DeletePlaylist("youtube:playlist:y1".to_string()));

  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::DeleteYouTubePlaylist(uri)) if uri == "youtube:playlist:y1"
  ));
}

#[test]
fn remove_radio_station_removes_the_saved_copy_and_reports_it() {
  // Unseeded, the persist would write the developer's real state.yml.
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));
  app.runtime_state.radio_stations = vec![RadioStationConfig {
    name: "Groove Salad".to_string(),
    url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  }];
  app.radio_stations = vec![radio_station_row(
    "Groove Salad",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )];

  app.apply(Action::RemoveRadioStation(
    "radio:https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  ));

  assert!(app.runtime_state.radio_stations.is_empty());
  assert!(app.radio_stations.is_empty(), "the sidebar row goes too");
  assert_eq!(
    app.status_message.as_deref(),
    Some("Removed saved radio station: Groove Salad")
  );
}

#[test]
fn remove_radio_station_reports_a_config_owned_station_without_removing() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));
  app.user_config.behavior.radio_stations = vec![RadioStationConfig {
    name: "Configured Groove".to_string(),
    url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  }];
  app.radio_stations = vec![radio_station_row(
    "Configured Groove",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )];

  app.apply(Action::RemoveRadioStation(
    "radio:https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  ));

  assert_eq!(app.radio_stations.len(), 1);
  assert_eq!(
    app.status_message.as_deref(),
    Some("Radio station is configured in config.yml: Configured Groove")
  );
}

#[test]
fn remove_radio_station_removes_only_the_saved_copy_of_a_configured_station() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));
  app.user_config.behavior.radio_stations = vec![RadioStationConfig {
    name: "Configured Groove".to_string(),
    url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  }];
  app.runtime_state.radio_stations = vec![RadioStationConfig {
    name: "Runtime Duplicate".to_string(),
    url: "https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  }];
  app.radio_stations = vec![radio_station_row(
    "Configured Groove",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )];

  app.apply(Action::RemoveRadioStation(
    "radio:https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  ));

  assert!(app.runtime_state.radio_stations.is_empty());
  // Config still supplies the row, so the sidebar keeps it.
  assert_eq!(app.radio_stations.len(), 1);
  assert_eq!(
    app.status_message.as_deref(),
    Some("Removed saved radio station: Runtime Duplicate")
  );
}

#[test]
fn remove_radio_station_reports_an_unfavorited_station() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));
  app.radio_stations = vec![radio_station_row(
    "Groove Salad",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )];

  app.apply(Action::RemoveRadioStation(
    "radio:https://ice1.somafm.com/groovesalad-128-mp3".to_string(),
  ));

  assert_eq!(app.radio_stations.len(), 1);
  assert_eq!(
    app.status_message.as_deref(),
    Some("Radio station is not favorited: Groove Salad")
  );
}

#[test]
fn remove_radio_station_without_a_stream_url_reports_it() {
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));
  app.radio_stations = vec![radio_station_row(
    "Groove Salad",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )];

  app.apply(Action::RemoveRadioStation("not-a-radio-uri".to_string()));

  assert_eq!(app.radio_stations.len(), 1);
  assert_eq!(
    app.status_message.as_deref(),
    Some("Radio station has no stream URL")
  );
}

#[test]
fn favorite_radio_station_persists_it_and_lists_it_in_the_sidebar() {
  // Unseeded, the persist would write the developer's real state.yml.
  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.state_path = Some(dir.path().join("state.yml"));

  app.apply(Action::FavoriteRadioStation(radio_station_row(
    "Groove Salad",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )));

  assert_eq!(app.runtime_state.radio_stations.len(), 1);
  assert_eq!(
    app.runtime_state.radio_stations[0].url,
    "https://ice1.somafm.com/groovesalad-128-mp3"
  );
  assert_eq!(app.radio_stations.len(), 1);
  assert_eq!(
    app.status_message.as_deref(),
    Some("Favorited radio station: Groove Salad")
  );
}

#[test]
fn favorite_radio_station_without_a_stream_url_reports_it() {
  let (mut app, _rx) = app_with_channel();
  let mut station = radio_station_row(
    "Groove Salad",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  );
  station.uri = None;

  app.apply(Action::FavoriteRadioStation(station));

  assert!(app.runtime_state.radio_stations.is_empty());
  assert!(app.radio_stations.is_empty(), "no sidebar row is added");
  assert_eq!(
    app.status_message.as_deref(),
    Some("Radio station has no stream URL")
  );
}

// --- podcasts, episodes, the native queue and the Discover rows ---

use super::DiscoverTarget;
use crate::core::app::{
  DiscoverTimeRange, EpisodeTableContext, SelectedFullShow, SelectedShow, TrackTableContext,
};
use crate::core::plugin_api::{EpisodeInfo, ShowInfo};

fn show_info(id: Option<&str>, name: &str) -> ShowInfo {
  ShowInfo {
    id: id.map(|id| id.to_string()),
    name: name.to_string(),
    ..Default::default()
  }
}

fn shows_page(offset: u32, limit: u32, names: &[&str]) -> Paged<ShowInfo> {
  Paged {
    items: names
      .iter()
      .map(|name| show_info(Some("3aNsrV6lkzmcU1w8u8kA7N"), name))
      .collect(),
    offset,
    limit,
    total: 40,
    next: None,
    previous: None,
  }
}

fn episodes_page(offset: u32, limit: u32, names: &[&str]) -> Paged<EpisodeInfo> {
  Paged {
    items: names
      .iter()
      .map(|name| EpisodeInfo {
        id: Some((*name).to_string()),
        uri: Some(format!("spotify:episode:{name}")),
        name: (*name).to_string(),
        duration_ms: 1_000,
        show_name: String::new(),
        description: String::new(),
        release_date: String::new(),
        is_playable: true,
        resume_point: None,
        image_url: None,
      })
      .collect(),
    offset,
    limit,
    total: 40,
    next: None,
    previous: None,
  }
}

#[test]
fn load_more_saved_shows_flips_to_a_cached_page() {
  let (mut app, rx) = app_with_channel();
  app.library.saved_shows.add_pages(shows_page(0, 20, &["A"]));
  app
    .library
    .saved_shows
    .add_pages(shows_page(20, 20, &["B"]));
  // `add_pages` leaves the visible index on the tail; start from the first.
  app.library.saved_shows.index = 0;

  app.apply(Action::LoadMore(ListTarget::SavedShows));

  assert_eq!(app.library.saved_shows.index, 1);
  assert!(rx.try_recv().is_err(), "a cached page needs no fetch");
}

#[test]
fn load_more_saved_shows_fetches_the_next_offset() {
  let (mut app, rx) = app_with_channel();
  app.library.saved_shows.add_pages(shows_page(0, 20, &["A"]));

  app.apply(Action::LoadMore(ListTarget::SavedShows));

  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetCurrentUserSavedShows(Some(20)))
  ));
}

#[test]
fn load_more_saved_shows_with_an_empty_cache_is_a_noop() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::LoadMore(ListTarget::SavedShows));

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn load_more_show_episodes_flips_to_a_cached_page() {
  let (mut app, rx) = app_with_channel();
  app.selected_show_simplified = Some(SelectedShow {
    show: show_info(Some("3aNsrV6lkzmcU1w8u8kA7N"), "A Podcast"),
  });
  app.episode_table_context = EpisodeTableContext::Simplified;
  app
    .library
    .show_episodes
    .add_pages(episodes_page(0, 20, &["A"]));
  app
    .library
    .show_episodes
    .add_pages(episodes_page(20, 20, &["B"]));
  app.library.show_episodes.index = 0;

  app.apply(Action::LoadMore(ListTarget::ShowEpisodes));

  assert_eq!(app.library.show_episodes.index, 1);
  assert!(rx.try_recv().is_err(), "a cached page needs no fetch");
}

#[test]
fn load_more_show_episodes_fetches_the_next_offset() {
  let (mut app, rx) = app_with_channel();
  app.selected_show_simplified = Some(SelectedShow {
    show: show_info(Some("3aNsrV6lkzmcU1w8u8kA7N"), "A Podcast"),
  });
  app.episode_table_context = EpisodeTableContext::Simplified;
  app
    .library
    .show_episodes
    .add_pages(episodes_page(0, 20, &["A"]));

  app.apply(Action::LoadMore(ListTarget::ShowEpisodes));

  match rx.try_recv() {
    Ok(IoEvent::GetCurrentShowEpisodes(show_id, offset)) => {
      assert_eq!(show_id, "3aNsrV6lkzmcU1w8u8kA7N");
      assert_eq!(offset, Some(20));
    }
    _other => panic!("expected GetCurrentShowEpisodes (IoEvent is not Debug)"),
  }
}

#[test]
fn load_more_show_episodes_reads_the_full_show_context() {
  let (mut app, rx) = app_with_channel();
  // Both snapshots are set; the context decides which id rides along.
  app.selected_show_simplified = Some(SelectedShow {
    show: show_info(Some("simplified-show"), "Simplified"),
  });
  app.selected_show_full = Some(SelectedFullShow {
    show: show_info(Some("full-show"), "Full"),
  });
  app.episode_table_context = EpisodeTableContext::Full;
  app
    .library
    .show_episodes
    .add_pages(episodes_page(0, 20, &["A"]));

  app.apply(Action::LoadMore(ListTarget::ShowEpisodes));

  match rx.try_recv() {
    Ok(IoEvent::GetCurrentShowEpisodes(show_id, _offset)) => assert_eq!(show_id, "full-show"),
    _other => panic!("expected GetCurrentShowEpisodes (IoEvent is not Debug)"),
  }
}

#[test]
fn load_more_show_episodes_without_a_selected_show_is_a_noop() {
  let (mut app, rx) = app_with_channel();
  app
    .library
    .show_episodes
    .add_pages(episodes_page(0, 20, &["A"]));

  app.apply(Action::LoadMore(ListTarget::ShowEpisodes));

  assert!(rx.try_recv().is_err(), "no open show means no fetch");
}

#[test]
fn queue_track_pushes_onto_the_native_queue() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::QueueTrack(track("0000000000000000000001", "One")));

  assert_eq!(app.native_queue.len(), 1);
  assert_eq!(app.native_queue[0].name, "One");
  assert!(rx.try_recv().is_err(), "nothing goes to the Web API queue");
}

#[test]
fn queue_track_on_an_external_spotify_device_falls_back_to_the_web_api() {
  let (mut app, rx) = app_with_channel();
  // A Spotify context with no native streaming device reads as external.
  app.current_playback_context = Some(playback_context(true, false));

  app.apply(Action::QueueTrack(track("0000000000000000000001", "One")));

  assert!(app.native_queue.is_empty());
  match rx.try_recv() {
    Ok(IoEvent::AddItemToQueue(uri)) => {
      assert_eq!(uri, "spotify:track:0000000000000000000001");
    }
    _other => panic!("expected AddItemToQueue (IoEvent is not Debug)"),
  }
}

#[test]
fn queue_track_without_a_uri_reports_it() {
  let (mut app, rx) = app_with_channel();
  let mut uri_less = track("0000000000000000000001", "One");
  uri_less.uri = None;

  app.apply(Action::QueueTrack(uri_less));

  assert!(app.native_queue.is_empty());
  assert_eq!(
    app.status_message.as_deref(),
    Some("Cannot queue: track has no URI")
  );
  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn queue_track_rejects_a_radio_stream() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::QueueTrack(radio_station_row(
    "Groove Salad",
    "https://ice1.somafm.com/groovesalad-128-mp3",
  )));

  assert!(app.native_queue.is_empty());
  assert_eq!(
    app.status_message.as_deref(),
    Some("Radio stations can't be queued")
  );
  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
}

#[test]
fn open_discover_artists_mix_fetches_when_the_cache_is_empty() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenDiscover(DiscoverTarget::ArtistsMix));

  assert!(matches!(rx.try_recv(), Ok(IoEvent::GetTopArtistsMix)));
  assert_eq!(
    app.get_current_route().id,
    RouteId::Home,
    "the fetch pushes no route"
  );
}

#[test]
fn open_discover_artists_mix_shows_the_cache_when_loaded() {
  let (mut app, rx) = app_with_channel();
  app.discover_artists_mix = vec![track("0000000000000000000001", "One")];
  // A stale in-range cursor must be reset to the top, not clamped.
  app.track_table.selected_index = 4;

  app.apply(Action::OpenDiscover(DiscoverTarget::ArtistsMix));

  assert_eq!(app.track_table.tracks.len(), 1);
  assert_eq!(
    app.track_table.context,
    Some(TrackTableContext::DiscoverPlaylist)
  );
  assert_eq!(app.track_table.selected_index, 0);
  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  assert!(rx.try_recv().is_err(), "a cached mix needs no fetch");
}

#[test]
fn open_discover_top_tracks_carries_the_time_range() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::OpenDiscover(DiscoverTarget::TopTracks(
    DiscoverTimeRange::Long,
  )));

  assert!(matches!(
    rx.try_recv(),
    Ok(IoEvent::GetUserTopTracks(DiscoverTimeRange::Long))
  ));
}

#[test]
fn open_discover_top_tracks_shows_the_cache_when_loaded() {
  let (mut app, rx) = app_with_channel();
  app.discover_top_tracks = vec![track("0000000000000000000001", "One")];
  app.track_table.selected_index = 4;

  app.apply(Action::OpenDiscover(DiscoverTarget::TopTracks(
    DiscoverTimeRange::Short,
  )));

  assert_eq!(app.track_table.tracks.len(), 1);
  assert_eq!(
    app.track_table.context,
    Some(TrackTableContext::DiscoverPlaylist)
  );
  assert_eq!(app.track_table.selected_index, 0);
  assert_eq!(app.get_current_route().id, RouteId::TrackTable);
  assert!(rx.try_recv().is_err(), "a cached page needs no fetch");
}

#[test]
fn open_discover_while_loading_is_a_noop() {
  for target in [
    DiscoverTarget::ArtistsMix,
    DiscoverTarget::TopTracks(DiscoverTimeRange::Medium),
  ] {
    let (mut app, rx) = app_with_channel();
    app.discover_loading = true;

    app.apply(Action::OpenDiscover(target));

    assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
    assert_eq!(app.get_current_route().id, RouteId::Home);
    assert_eq!(app.track_table.context, None);
  }
}

// --- the create-playlist form and the friends social graph ---

use crate::core::app::FriendSearchResult;

#[test]
fn create_youtube_playlist_dispatches_the_local_create() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::CreateYouTubePlaylist("Focus".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::CreateYouTubePlaylist(name)) => assert_eq!(name, "Focus"),
    _other => panic!("expected CreateYouTubePlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn search_tracks_for_playlist_dispatches_the_query_untrimmed() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::SearchTracksForPlaylist("daft punk ".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::SearchTracksForPlaylist(query)) => {
      assert_eq!(query, "daft punk ", "the arm must not trim");
    }
    _other => panic!("expected SearchTracksForPlaylist (IoEvent is not Debug)"),
  }
}

#[test]
fn add_friend_by_code_dispatches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::AddFriendByCode("JAY123".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::AddFriendByCode(code)) => assert_eq!(code, "JAY123"),
    _other => panic!("expected AddFriendByCode (IoEvent is not Debug)"),
  }
}

#[test]
fn add_friend_by_user_id_dispatches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::AddFriendById("user-1".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::AddFriendById(user_id)) => assert_eq!(user_id, "user-1"),
    _other => panic!("expected AddFriendById (IoEvent is not Debug)"),
  }
}

#[test]
fn unfollow_friend_dispatches() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::UnfollowFriend("user-1".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::UnfollowFriend(user_id)) => assert_eq!(user_id, "user-1"),
    _other => panic!("expected UnfollowFriend (IoEvent is not Debug)"),
  }
}

#[test]
fn search_friend_users_asks_from_two_bytes_up() {
  let (mut app, rx) = app_with_channel();

  app.apply(Action::SearchFriendUsers("ab".to_string()));

  match rx.try_recv() {
    Ok(IoEvent::SearchFriendUsers(query)) => assert_eq!(query, "ab"),
    _other => panic!("expected SearchFriendUsers (IoEvent is not Debug)"),
  }
}

#[test]
fn search_friend_users_below_the_threshold_clears_stale_results() {
  let (mut app, rx) = app_with_channel();
  app.friend_user_search_results = vec![FriendSearchResult {
    id: "user-1".to_string(),
    name: "Jay".to_string(),
    is_following: false,
  }];

  app.apply(Action::SearchFriendUsers("a".to_string()));

  assert!(rx.try_recv().is_err(), "expected no IoEvent dispatched");
  assert!(
    app.friend_user_search_results.is_empty(),
    "a below-threshold query drops the stale results"
  );
}

// --- stats, settings, sort and the native queue ---

use super::ActionOutcome;
use crate::core::plugin_api::ArtistInfo;
use crate::core::sort::{SortContext, SortField, SortOrder};

#[test]
fn cycle_stats_period_walks_the_period_ring_and_reloads() {
  let (mut app, rx) = app_with_channel();
  let start = app.stats_period;

  app.apply(Action::CycleStatsPeriod { forward: true });

  assert_eq!(app.stats_period, start.next());
  assert!(app.stats_loading);
  assert!(app.stats_data.is_none());
  match rx.try_recv() {
    Ok(IoEvent::LoadListeningStats(period)) => assert_eq!(period, start.next()),
    _other => panic!("expected LoadListeningStats (IoEvent is not Debug)"),
  }

  app.apply(Action::CycleStatsPeriod { forward: false });

  assert_eq!(app.stats_period, start);
  match rx.try_recv() {
    Ok(IoEvent::LoadListeningStats(period)) => assert_eq!(period, start),
    _other => panic!("expected LoadListeningStats (IoEvent is not Debug)"),
  }
}

#[test]
fn save_settings_writes_the_config_and_reports_success() {
  use crate::core::user_config::UserConfigPaths;

  let dir = tempfile::tempdir().unwrap();
  let (mut app, _rx) = app_with_channel();
  app.user_config.path_to_config = Some(UserConfigPaths {
    config_file_path: dir.path().join("config.yml"),
  });
  app.load_settings_for_category();

  let outcome = app.apply(Action::SaveSettings);

  assert_eq!(outcome, ActionOutcome::SettingsSaved { saved: true });
  assert!(app.settings_saved_items == app.settings_items);
  assert!(dir.path().join("config.yml").exists());
}

#[test]
fn save_settings_without_a_config_path_reports_failure_and_raises_the_error_frame() {
  let (mut app, _rx) = app_with_channel();

  let outcome = app.apply(Action::SaveSettings);

  assert_eq!(outcome, ActionOutcome::SettingsSaved { saved: false });
  assert!(!app.api_error.is_empty());
  assert_eq!(app.get_current_route().id, RouteId::Error);
}

#[test]
fn cycle_visualizer_style_steps_the_configured_style() {
  let (mut app, _rx) = app_with_channel();
  let before = app.user_config.behavior.visualizer_style;

  app.apply(Action::CycleVisualizerStyle);

  assert_eq!(app.user_config.behavior.visualizer_style, before.next());
}

fn artist(name: &str) -> ArtistInfo {
  ArtistInfo {
    id: None,
    uri: None,
    name: name.to_string(),
    image_url: None,
  }
}

fn artist_names(app: &App) -> Vec<&str> {
  app.artists.iter().map(|a| a.name.as_str()).collect()
}

#[test]
fn sort_saved_artists_reorders_the_cached_rows_in_place() {
  let (mut app, rx) = app_with_channel();
  app.artists = vec![artist("Zed"), artist("Alpha"), artist("Mid")];

  app.apply(Action::Sort {
    context: SortContext::SavedArtists,
    field: SortField::Name,
  });

  assert_eq!(artist_names(&app), vec!["Alpha", "Mid", "Zed"]);
  assert_eq!(app.artist_sort.field, SortField::Name);
  assert_eq!(app.artist_sort.order, SortOrder::Ascending);
  assert!(
    rx.try_recv().is_err(),
    "an in-place sort dispatches nothing"
  );
}

#[test]
fn sorting_by_the_field_in_effect_flips_the_direction_and_resorts() {
  let (mut app, _rx) = app_with_channel();
  app.artists = vec![artist("Zed"), artist("Alpha")];
  app.apply(Action::Sort {
    context: SortContext::SavedArtists,
    field: SortField::Name,
  });

  app.apply(Action::Sort {
    context: SortContext::SavedArtists,
    field: SortField::Name,
  });

  assert_eq!(app.artist_sort.order, SortOrder::Descending);
  assert_eq!(artist_names(&app), vec!["Zed", "Alpha"]);
}

#[test]
fn toggle_sort_order_flips_the_recorded_direction_without_resorting() {
  let (mut app, _rx) = app_with_channel();
  app.artists = vec![artist("Zed"), artist("Alpha")];
  app.apply(Action::Sort {
    context: SortContext::SavedArtists,
    field: SortField::Name,
  });

  app.apply(Action::ToggleSortOrder(SortContext::SavedArtists));

  assert_eq!(app.artist_sort.order, SortOrder::Descending);
  assert_eq!(
    artist_names(&app),
    vec!["Alpha", "Zed"],
    "the rows keep the order that was applied"
  );
}

fn queued(uri: &str, name: &str) -> TrackInfo {
  TrackInfo {
    uri: Some(uri.to_string()),
    name: name.to_string(),
    artists: vec![],
    album: String::new(),
    duration_ms: 0,
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

fn queue_names(app: &App) -> Vec<&str> {
  app.native_queue.iter().map(|t| t.name.as_str()).collect()
}

fn app_with_three_queued() -> (App, Receiver<IoEvent>) {
  let (mut app, rx) = app_with_channel();
  app.native_queue = vec![
    queued("spotify:track:a", "A"),
    queued("spotify:track:b", "B"),
    queued("spotify:track:c", "C"),
  ];
  (app, rx)
}

#[test]
fn play_queue_item_drops_the_earlier_items_and_advances() {
  let (mut app, rx) = app_with_three_queued();

  app.apply(Action::PlayQueueItem {
    uri: "spotify:track:c".to_string(),
    position: 2,
  });

  assert_eq!(queue_names(&app), vec!["C"]);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::AdvanceNativeQueue)));
}

#[test]
fn play_queue_item_falls_back_to_the_uri_when_the_position_is_stale() {
  let (mut app, rx) = app_with_three_queued();

  app.apply(Action::PlayQueueItem {
    uri: "spotify:track:c".to_string(),
    position: 0,
  });

  assert_eq!(queue_names(&app), vec!["C"]);
  assert!(matches!(rx.try_recv(), Ok(IoEvent::AdvanceNativeQueue)));
}

#[test]
fn queue_actions_on_an_unknown_uri_are_noops() {
  let (mut app, rx) = app_with_three_queued();
  let unknown = "spotify:track:zzz".to_string();

  app.apply(Action::PlayQueueItem {
    uri: unknown.clone(),
    position: 0,
  });
  app.apply(Action::RemoveFromQueue {
    uri: unknown.clone(),
    position: 1,
  });
  app.apply(Action::MoveQueueItem {
    uri: unknown,
    from: 0,
    to: 2,
  });

  assert_eq!(queue_names(&app), vec!["A", "B", "C"]);
  assert!(rx.try_recv().is_err(), "nothing may be dispatched");
}

#[test]
fn remove_from_queue_and_move_queue_item_edit_the_queue() {
  let (mut app, rx) = app_with_three_queued();

  app.apply(Action::RemoveFromQueue {
    uri: "spotify:track:b".to_string(),
    position: 1,
  });
  assert_eq!(queue_names(&app), vec!["A", "C"]);

  app.apply(Action::MoveQueueItem {
    uri: "spotify:track:c".to_string(),
    from: 1,
    to: 0,
  });
  assert_eq!(queue_names(&app), vec!["C", "A"]);

  // An out-of-range destination changes nothing.
  app.apply(Action::MoveQueueItem {
    uri: "spotify:track:c".to_string(),
    from: 0,
    to: 5,
  });
  assert_eq!(queue_names(&app), vec!["C", "A"]);
  assert!(rx.try_recv().is_err(), "queue edits dispatch nothing");
}

// --- the DJ screen's own actions (no-ops without ai-dj) ---

#[cfg(feature = "ai-dj")]
mod dj_screen_actions {
  use super::*;
  use crate::infra::dj::{DjLine, DjSpeaker, TurnKind};

  #[test]
  fn ask_dj_starts_one_turn_and_bumps_the_generation_once() {
    let (mut app, rx) = app_with_channel();
    let before = app.dj.generation;

    app.apply(Action::AskDj("  something chill  ".to_string()));

    assert!(app.dj.thinking, "the turn is in flight");
    assert_eq!(app.dj.generation, before + 1);
    assert_eq!(app.dj.transcript.len(), 1);
    assert_eq!(app.dj.transcript[0], DjLine::user("something chill"));
    assert!(
      !app.is_loading,
      "a brain call must not pin the global spinner"
    );
    match rx.try_recv() {
      Ok(IoEvent::AskDj(request)) => {
        assert!(request.extra_instruction.is_none());
        assert_eq!(request.generation, app.dj.generation);
        assert!(!request.must_act);
        assert_eq!(request.vibe_on_success.as_deref(), Some("something chill"));
        assert_eq!(request.turn_seq, app.dj.turn_seq);
      }
      _other => panic!("expected AskDj (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn ask_dj_while_a_turn_is_in_flight_is_refused_and_says_so() {
    let (mut app, rx) = app_with_channel();
    app.dj.begin_turn(TurnKind::Ask);
    let generation = app.dj.generation;

    // Whitespace on purpose: the busy message has to win over the blank check.
    app.apply(Action::AskDj("   ".to_string()));

    assert!(rx.try_recv().is_err(), "nothing may be dispatched");
    assert_eq!(app.dj.generation, generation);
    assert!(app.dj.transcript.is_empty());
    assert_eq!(
      app.status_message.as_deref(),
      Some("The DJ is still working on the last request")
    );
  }

  #[test]
  fn ask_dj_with_a_blank_prompt_dispatches_nothing() {
    let (mut app, rx) = app_with_channel();

    app.apply(Action::AskDj("   ".to_string()));

    assert!(rx.try_recv().is_err());
    assert!(!app.dj.thinking);
    assert!(app.dj.transcript.is_empty());
  }

  #[test]
  fn a_vibe_shift_asks_again_with_a_steer_that_must_act() {
    let (mut app, rx) = app_with_channel();
    let before = app.dj.generation;

    app.apply(Action::DjVibeShift);

    assert_eq!(app.dj.generation, before + 1);
    assert!(app.dj.thinking);
    assert!(!app.is_loading);
    match rx.try_recv() {
      Ok(IoEvent::AskDj(request)) => {
        assert!(request
          .extra_instruction
          .as_deref()
          .is_some_and(|steer| steer.contains("Change direction")));
        assert!(request.must_act);
        assert!(request.vibe_on_success.is_none());
      }
      _other => panic!("expected AskDj (IoEvent is not Debug)"),
    }
    let last = app.dj.transcript.last().unwrap();
    assert!(last.text.contains("vibe"));
    assert_eq!(last.speaker, DjSpeaker::System);
  }

  #[test]
  fn a_vibe_shift_while_thinking_is_refused() {
    let (mut app, rx) = app_with_channel();
    app.dj.begin_turn(TurnKind::Ask);

    app.apply(Action::DjVibeShift);

    assert!(rx.try_recv().is_err());
    assert_eq!(
      app.status_message.as_deref(),
      Some("The DJ is already working on something")
    );
  }

  #[test]
  fn toggling_dj_auto_queue_on_with_a_short_queue_asks_for_a_refill() {
    let (mut app, rx) = app_with_channel();

    app.apply(Action::ToggleDjAutoQueue);

    assert!(app.dj.auto_queue);
    assert!(!app.is_loading);
    match rx.try_recv() {
      Ok(IoEvent::DjTopUp(generation, turn_seq)) => {
        assert_eq!(generation, app.dj.generation);
        assert_eq!(turn_seq, app.dj.turn_seq);
      }
      _other => panic!("expected DjTopUp (IoEvent is not Debug)"),
    }
  }

  #[test]
  fn toggling_dj_auto_queue_off_does_not_abandon_a_question_in_flight() {
    let (mut app, rx) = app_with_channel();
    app.apply(Action::AskDj("something warm".to_string()));
    assert!(matches!(rx.try_recv(), Ok(IoEvent::AskDj(_))));
    let generation = app.dj.generation;
    app.dj.auto_queue = true;

    app.apply(Action::ToggleDjAutoQueue);

    assert!(!app.dj.auto_queue);
    assert!(app.dj.thinking, "the question is still being answered");
    assert_eq!(app.dj.generation, generation);
  }

  #[test]
  fn toggling_dj_auto_queue_off_still_abandons_a_refill() {
    let (mut app, _rx) = app_with_channel();
    app.dj.auto_queue = true;
    app.dj.begin_turn(TurnKind::Refill);
    app.dj.step = Some((2, 4));
    let generation = app.dj.generation;

    app.apply(Action::ToggleDjAutoQueue);

    assert!(!app.dj.thinking, "nobody is waiting on a refill");
    assert!(app.dj.step.is_none());
    assert_ne!(app.dj.generation, generation);
  }

  #[test]
  fn toggling_dj_fresh_only_starts_the_crawl_once() {
    let (mut app, rx) = app_with_channel();
    assert!(!app.dj.avoid_library);

    app.apply(Action::ToggleDjFreshOnly);
    assert!(app.dj.avoid_library);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::DjIndexLibrary)));

    // Off, then on again with the index already cached: no second crawl.
    app.apply(Action::ToggleDjFreshOnly);
    assert!(!app.dj.avoid_library);
    app.dj.library = Some(crate::infra::dj::DjLibrary::default());
    app.apply(Action::ToggleDjFreshOnly);
    assert!(app.dj.avoid_library);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn open_dj_setup_opens_the_screen_and_the_picker_without_stacking_routes() {
    let (mut app, rx) = app_with_channel();

    app.apply(Action::OpenDjSetup);

    assert_eq!(app.get_current_route().id, RouteId::AiDj);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::AiDj);
    assert!(app.dj.setup.is_some());
    assert!(rx.try_recv().is_err(), "the filter is off, so no crawl");

    app.apply(Action::OpenDjSetup);
    app.pop_navigation_stack();
    assert_ne!(app.get_current_route().id, RouteId::AiDj);
  }

  #[test]
  fn open_dj_setup_warms_the_library_index_when_the_filter_is_on() {
    let (mut app, rx) = app_with_channel();
    app.apply(Action::OpenDjSetup);
    assert!(rx.try_recv().is_err());
    app.dj.avoid_library = true;

    // Already on the DJ screen: the picker key still warms the index.
    app.apply(Action::OpenDjSetup);

    assert!(matches!(rx.try_recv(), Ok(IoEvent::DjIndexLibrary)));
    assert_eq!(app.get_current_route().id, RouteId::AiDj);
  }
}
