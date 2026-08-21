//! `App::apply`: the single write path from an [`Action`] into `App`.
//!
//! Ported from the scripting engine's effect drain, which this replaces.
//! Every arm routes through the same `App` method as the equivalent
//! keybinding, so native-streaming fast paths and throttling/coalescing are
//! automatically honoured.

use crate::core::app::{ActiveBlock, App, RouteId};
use crate::core::plugin_api;
use crate::infra::network::IoEvent;

use super::{Action, ActionOutcome, NavTarget, RepeatSetting};

impl App {
  /// Apply one frontend-neutral action. See the module doc for the rules
  /// every arm follows.
  pub fn apply(&mut self, action: Action) -> ActionOutcome {
    match action {
      Action::Play => {
        if !effective_is_playing(self) {
          self.toggle_playback();
        }
      }
      Action::Pause => {
        if effective_is_playing(self) {
          self.toggle_playback();
        }
      }
      Action::TogglePlayback => self.toggle_playback(),
      Action::NextTrack => self.next_track(),
      Action::PreviousTrack => self.previous_track(),
      Action::ForcePreviousTrack => self.force_previous_track(),
      Action::SeekTo(ms) => self.seek_to(ms),
      Action::SeekForward => self.seek_forwards(),
      Action::SeekBackward => self.seek_backwards(),
      Action::SetVolume(v) => self.set_volume_percent(v),
      Action::VolumeUp => self.increase_volume(),
      Action::VolumeDown => self.decrease_volume(),
      Action::SetShuffle(desired) => {
        let current = plugin_api::playback_state(self)
          .map(|p| p.shuffle)
          .unwrap_or(false);
        if current != desired {
          self.shuffle();
        }
      }
      Action::ToggleShuffle => self.shuffle(),
      Action::CycleRepeat => self.repeat(),
      Action::SetRepeat(setting) => {
        use rspotify::model::RepeatState;
        let state = match setting {
          RepeatSetting::Off => RepeatState::Off,
          RepeatSetting::Track => RepeatState::Track,
          RepeatSetting::Context => RepeatState::Context,
        };
        self.dispatch(IoEvent::Repeat(state));
      }
      Action::PlayUris { uris, offset } => self.start_playback_uris(uris, offset),
      Action::PlayContext { uri, offset } => self.start_playback_context(uri, offset),
      Action::TransferPlayback { device_id, persist } => {
        self.dispatch(IoEvent::TransferPlaybackToDevice(device_id, persist));
      }
      Action::AddToQueue(uri) => self.dispatch(IoEvent::AddItemToQueue(uri)),
      Action::Search(query) => {
        let country = self.get_user_country();
        self.dispatch(IoEvent::GetSearchResults(query, country));
      }
      Action::CreatePlaylist { name, track_uris } => {
        self.dispatch(IoEvent::CreateNewPlaylist(name, track_uris));
      }
      Action::AddTrackToPlaylist { playlist, track } => {
        self.dispatch(IoEvent::AddTrackToPlaylist(playlist, track));
      }
      Action::RemoveTrackFromPlaylist {
        playlist,
        track,
        position,
      } => {
        self.dispatch(IoEvent::RemoveTrackFromPlaylistAtPosition(
          playlist, track, position,
        ));
      }
      Action::FollowPlaylist(playlist) => {
        // The network handler ignores the owner-id parameter; "unknown"
        // mirrors the fallback the built-in follow flow uses.
        self.dispatch(IoEvent::UserFollowPlaylist(
          "unknown".to_string(),
          playlist,
          None,
        ));
      }
      Action::UnfollowPlaylist(playlist_id) => {
        let user_id = self.user.as_ref().map(|u| u.id.clone());
        if let Some(user_id) = user_id {
          self.dispatch(IoEvent::UserUnfollowPlaylist(user_id, playlist_id));
        } else {
          self.set_error_status_message(
            "plugin unfollow_playlist: user profile not loaded yet".to_string(),
            4,
          );
        }
      }
      Action::ToggleSaveTrack(uri) => self.dispatch(IoEvent::ToggleSaveTrack(uri)),
      Action::SaveAlbum(id) => self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(id)),
      Action::UnsaveAlbum(id) => self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id)),
      Action::SaveShow(id) => self.dispatch(IoEvent::CurrentUserSavedShowAdd(id)),
      Action::UnsaveShow(id) => self.dispatch(IoEvent::CurrentUserSavedShowDelete(id)),
      Action::FollowArtist(id) => self.dispatch(IoEvent::UserFollowArtists(vec![id])),
      Action::UnfollowArtist(id) => self.dispatch(IoEvent::UserUnfollowArtists(vec![id])),
      Action::Notify(msg, ttl) => self.set_status_message(msg, ttl),
      Action::NotifyError(msg, ttl) => self.set_error_status_message(msg, ttl),
      Action::Navigate(target) => apply_navigate(self, target),
      Action::Back => {
        self.pop_navigation_stack();
      }
      Action::JumpToAlbum => self.jump_to_album(),
      Action::JumpToArtist => self.jump_to_artist_album(),
      Action::JumpToContext => self.jump_to_context(),
      Action::GenerateRecap => self.generate_recap(),
      Action::SetPlaybarSegment { plugin, text } => match text {
        Some(t) => {
          self.plugin_playbar_segments.insert(plugin, t);
        }
        None => {
          self.plugin_playbar_segments.remove(&plugin);
        }
      },
      Action::ShowPopup(popup) => self.show_plugin_popup(popup),
      Action::ClosePopup => self.close_plugin_popup(),
      Action::SetTheme(pairs) => {
        for (field, color) in pairs {
          self.user_config.theme.set(field, color);
        }
      }
      Action::SetScreenContent { name, content } => {
        self.plugin_screens.insert(name, content);
      }
      Action::ShowScreen(name) => self.open_plugin_screen(name),
      Action::CloseScreen(name) => {
        if self.get_current_route().id == RouteId::PluginScreen(name) {
          self.pop_navigation_stack();
        }
      }
      Action::QueueTracks(tracks) => {
        #[cfg(feature = "dj-core")]
        {
          return ActionOutcome::Queued {
            accepted: self.extend_native_queue_from_dj(tracks),
          };
        }
        #[cfg(not(feature = "dj-core"))]
        {
          let _ = tracks;
          return ActionOutcome::Queued { accepted: 0 };
        }
      }
      Action::SetDjVibe(vibe) => {
        #[cfg(feature = "dj-core")]
        {
          // A new standing direction invalidates any refill already in
          // flight for the old one. Exactly one bump: the in-TUI agent
          // tells its own set_dj_vibe bump apart from the listener's by
          // asserting the generation moved by exactly 1.
          self.dj.bump_generation();
          self.dj.vibe = vibe;
        }
        #[cfg(not(feature = "dj-core"))]
        {
          let _ = vibe;
        }
      }
    }
    ActionOutcome::Applied
  }
}

/// Returns `true` when the current playback state indicates active playback.
fn effective_is_playing(app: &App) -> bool {
  plugin_api::playback_state(app)
    .map(|p| p.is_playing)
    .unwrap_or(false)
}

/// Replicate the matching keybinding for each nav target.
fn apply_navigate(app: &mut App, target: NavTarget) {
  match target {
    NavTarget::Home => app.push_navigation_stack(RouteId::Home, ActiveBlock::Empty),
    NavTarget::Queue => {
      app.dispatch(IoEvent::GetQueue);
      app.push_navigation_stack(RouteId::Queue, ActiveBlock::Queue);
    }
    NavTarget::Settings => app.open_settings_screen(),
    NavTarget::Devices => app.open_source_device_picker(),
    // The help KEY also clears the help filter first (`help_menu::open`); that
    // reset needs the TUI's filtered-docs count, so it cannot live here.
    NavTarget::Help => app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu),
    NavTarget::Lyrics => app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView),
    // The network handler pushes the route once the data arrives, exactly like
    // the keybinding.
    NavTarget::RecentlyPlayed => app.dispatch(IoEvent::GetRecentlyPlayed),
    NavTarget::Party => app.push_navigation_stack(RouteId::Party, ActiveBlock::Party),
    NavTarget::Analysis => app.get_audio_analysis(),
    NavTarget::MiniPlayer => {
      if app.get_current_route().id == RouteId::MiniPlayer {
        app.pop_navigation_stack();
      } else {
        app.push_navigation_stack(RouteId::MiniPlayer, ActiveBlock::MiniPlayer);
      }
    }
  }
}
