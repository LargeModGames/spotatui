use super::common_key_events;
use crate::core::app::{ActiveBlock, App};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use crate::tui::ui::player::PlaybarControl;
use rspotify::model::{context::CurrentPlaybackContext, PlayableItem};
use rspotify::prelude::Id;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::MyPlaylists));
    }
    k => {
      handle_action_key(k, app);
    }
  };
}

pub(crate) fn handle_action_key(key: Key, app: &mut App) -> bool {
  match key {
    k if k == app.user_config.keys.like_track => {
      handle_control(PlaybarControl::Like, app);
      true
    }
    Key::Char('w') => {
      add_currently_playing_track_to_playlist(app);
      true
    }
    _ => false,
  }
}

pub(crate) fn handle_control(control: PlaybarControl, app: &mut App) {
  match control {
    PlaybarControl::Prev => app.previous_track(),
    PlaybarControl::PlayPause => app.toggle_playback(),
    PlaybarControl::Next => app.next_track(),
    PlaybarControl::Shuffle => app.shuffle(),
    PlaybarControl::Repeat => app.repeat(),
    PlaybarControl::Like => toggle_like_currently_playing_item(app),
    PlaybarControl::VolumeDown => app.decrease_volume(),
    PlaybarControl::VolumeUp => app.increase_volume(),
  }
}

pub(crate) fn toggle_like_currently_playing_item(app: &mut App) {
  let queue_now_is_spotify = app.queue_now_is_spotify();
  let queued_spotify_track_uri = queue_now_is_spotify
    .then(|| app.queue_now_spotify_track_uri())
    .flatten();

  if spotify_context_is_suspended(
    app.queue_owns_playback(),
    queue_now_is_spotify,
    app.active_decoded_source(),
  ) {
    app.set_status_message("The current playback source cannot be liked", 4);
    return;
  }

  // A queued Spotify track plays via a direct `player.load` outside the Spirc
  // context, so the cached playback context still names the suspended context's
  // track — resolve the queue slot's own track instead of falling through.
  if queue_now_is_spotify {
    match queued_spotify_track_uri {
      Some(uri) => app.dispatch(IoEvent::ToggleSaveTrack(uri)),
      None => app.set_status_message("The current playback source cannot be liked", 4),
    }
    return;
  }

  if let Some(CurrentPlaybackContext {
    item: Some(item), ..
  }) = app.current_playback_context.to_owned()
  {
    match item {
      PlayableItem::Track(track) => {
        if let Some(track_id) = track.id {
          app.dispatch(IoEvent::ToggleSaveTrack(track_id.uri()));
        }
      }
      PlayableItem::Episode(episode) => {
        app.dispatch(IoEvent::ToggleSaveTrack(episode.id.uri()));
      }
      _ => {}
    };
  };
}

/// Whether Like must not consult the cached Spotify playback context. A queue
/// slot playing a *decoded* item (or any decoded per-source playback) suspends
/// the context; a queue slot playing a *Spotify* track stays eligible — it is
/// liked via the slot's own track, never the cached context.
fn spotify_context_is_suspended(
  queue_owns_playback: bool,
  queue_now_is_spotify: bool,
  decoded_source_active: bool,
) -> bool {
  (queue_owns_playback && !queue_now_is_spotify) || decoded_source_active
}

pub(crate) fn add_currently_playing_track_to_playlist(app: &mut App) {
  if let Some(CurrentPlaybackContext {
    item: Some(item), ..
  }) = app.current_playback_context.to_owned()
  {
    match item {
      PlayableItem::Track(track) => {
        let track_id = track.id.map(|id| id.uri());
        app.begin_add_track_to_playlist_flow(track_id, track.name);
      }
      PlayableItem::Episode(_) => {
        app.set_status_message("Only tracks can be added to playlists".to_string(), 4);
      }
      _ => {}
    };
  } else {
    app.set_status_message("No track currently playing".to_string(), 4);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Up, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::MyPlaylists);
  }

  #[test]
  fn on_add_current_track_without_playback_sets_status_message() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Char('w'), &mut app);

    assert_eq!(
      app.status_message.as_deref(),
      Some("No track currently playing")
    );
  }

  #[test]
  fn non_spotify_playback_cannot_use_cached_spotify_item_for_like() {
    // A decoded queue slot or any decoded per-source playback suspends Like.
    assert!(spotify_context_is_suspended(true, false, false));
    assert!(spotify_context_is_suspended(false, false, true));
    // A queue slot playing a *Spotify* track stays eligible.
    assert!(!spotify_context_is_suspended(true, true, false));
    // Plain Spotify context playback.
    assert!(!spotify_context_is_suspended(false, false, false));
  }

  #[cfg(feature = "streaming")]
  mod queued_spotify_like {
    use super::*;
    use crate::core::plugin_api::TrackInfo;
    use crate::core::user_config::UserConfig;
    use crate::infra::queue::QueueNowPlaying;
    use chrono::Utc;
    use rspotify::model::{
      context::Actions,
      device::Device,
      enums::{CurrentlyPlayingType, RepeatState},
      idtypes::TrackId,
      track::FullTrack,
      DeviceType, SimplifiedAlbum, SimplifiedArtist,
    };
    use std::sync::mpsc::{channel, Receiver};
    use std::time::SystemTime;

    fn queue_track(uri: Option<&str>) -> TrackInfo {
      TrackInfo {
        uri: uri.map(|u| u.to_string()),
        name: "Queued".to_string(),
        artists: vec!["Artist".to_string()],
        album: "Album".to_string(),
        duration_ms: 1000,
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

    /// An app whose cached playback context still names the suspended context's
    /// last Spotify track — the regression target: Like must never save it
    /// while the queue slot owns playback.
    #[allow(deprecated)]
    fn app_with_stale_context() -> (App, Receiver<IoEvent>) {
      let (tx, rx) = channel();
      let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
      app.current_playback_context = Some(CurrentPlaybackContext {
        device: Device {
          id: Some("native-device".to_string()),
          is_active: true,
          is_private_session: false,
          is_restricted: false,
          name: "spotatui".to_string(),
          _type: DeviceType::Computer,
          volume_percent: Some(50),
        },
        repeat_state: RepeatState::Off,
        shuffle_state: false,
        context: None,
        timestamp: Utc::now(),
        progress: None,
        is_playing: true,
        item: Some(PlayableItem::Track(FullTrack {
          album: SimplifiedAlbum::default(),
          artists: vec![SimplifiedArtist::default()],
          available_markets: Vec::new(),
          disc_number: 1,
          duration: chrono::Duration::milliseconds(180_000),
          explicit: false,
          external_ids: Default::default(),
          external_urls: Default::default(),
          href: None,
          id: Some(
            TrackId::from_id("0000000000000000000009")
              .unwrap()
              .into_static(),
          ),
          is_local: false,
          is_playable: Some(true),
          linked_from: None,
          restrictions: None,
          name: "Cached".to_string(),
          popularity: 50,
          preview_url: None,
          track_number: 1,
          r#type: rspotify::model::Type::Track,
        })),
        currently_playing_type: CurrentlyPlayingType::Track,
        actions: Actions::default(),
      });
      (app, rx)
    }

    #[test]
    fn like_saves_the_queued_spotify_track_not_the_cached_context_item() {
      let (mut app, rx) = app_with_stale_context();
      app.queue_now = Some(QueueNowPlaying::Spotify {
        track: queue_track(Some("spotify:track:queued")),
      });

      toggle_like_currently_playing_item(&mut app);

      assert!(
        matches!(rx.try_recv(), Ok(IoEvent::ToggleSaveTrack(uri)) if uri == "spotify:track:queued"),
        "expected the queue slot's own track to be liked"
      );
      assert!(app.status_message.is_none());
    }

    #[test]
    fn like_for_uri_less_queued_track_never_falls_back_to_cached_context() {
      let (mut app, rx) = app_with_stale_context();
      app.queue_now = Some(QueueNowPlaying::Spotify {
        track: queue_track(None),
      });

      toggle_like_currently_playing_item(&mut app);

      assert!(rx.try_recv().is_err(), "nothing may be dispatched");
      assert_eq!(
        app.status_message.as_deref(),
        Some("The current playback source cannot be liked")
      );
    }
  }
}
