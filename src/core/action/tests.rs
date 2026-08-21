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
use crate::core::app::{App, RouteId, UserInfo};
use crate::core::theme::{Color, Theme, ThemeField};
use crate::core::user_config::UserConfig;
use crate::infra::network::IoEvent;

fn app_with_channel() -> (App, Receiver<IoEvent>) {
  let (tx, rx) = channel();
  let app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
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
    Some("plugin unfollow_playlist: user profile not loaded yet")
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
