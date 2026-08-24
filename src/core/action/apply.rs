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
      Action::PlayTrackInContext { context, track } => {
        self.start_playback_track_in_context(context, track);
      }
      Action::TransferPlayback { device_id, persist } => {
        self.dispatch(IoEvent::TransferPlaybackToDevice(device_id, persist));
      }
      Action::AddToQueue(uri) => self.dispatch(IoEvent::AddItemToQueue(uri)),
      Action::QueueTrack(track) => self.add_track_to_native_queue(track),
      Action::PlayQueueItem { uri, position } => self.play_queue_item(&uri, position),
      Action::RemoveFromQueue { uri, position } => {
        self.remove_queue_item(&uri, position);
      }
      Action::MoveQueueItem { uri, from, to } => {
        self.move_queue_item(&uri, from, to);
      }
      Action::Search(query) => {
        let country = self.get_user_country();
        self.dispatch(IoEvent::GetSearchResults(query, country));
      }
      Action::SearchActiveSource(query) => match self.active_source {
        crate::core::source::Source::Subsonic => {
          self.dispatch(IoEvent::GetSubsonicSearchResults(query));
        }
        crate::core::source::Source::Radio => {
          self.dispatch(IoEvent::GetRadioSearchResults(query));
        }
        crate::core::source::Source::YouTube => {
          self.dispatch(IoEvent::GetYouTubeSearchResults(query));
        }
        // Spotify and Local both land on the Web API search, exactly like
        // the search input's if-chain (which has no Local branch).
        crate::core::source::Source::Spotify | crate::core::source::Source::Local => {
          let country = self.get_user_country();
          self.dispatch(IoEvent::GetSearchResults(query, country));
        }
      },
      Action::SearchPlaylistTracks { playlist_id, query } => {
        self.pending_playlist_track_search = Some(query.clone());
        self.set_status_message(format!("Searching playlist for \"{query}\"..."), 60);
        self.dispatch(IoEvent::SearchPlaylistTracks(playlist_id, query));
      }
      Action::CreatePlaylist { name, track_uris } => {
        self.dispatch(IoEvent::CreateNewPlaylist(name, track_uris));
      }
      Action::CreateYouTubePlaylist(name) => {
        self.dispatch(IoEvent::CreateYouTubePlaylist(name));
      }
      Action::SearchTracksForPlaylist(query) => {
        self.dispatch(IoEvent::SearchTracksForPlaylist(query));
      }
      Action::AddTrackToPlaylist { playlist, track } => {
        self.add_track_to_playlist(playlist, track);
      }
      Action::RemoveTrackFromPlaylist {
        playlist,
        track,
        position,
      } => self.remove_track_from_playlist(playlist, track, position),
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
            "Cannot unfollow: user profile not loaded yet".to_string(),
            4,
          );
        }
      }
      Action::DeletePlaylist(uri) => self.dispatch(IoEvent::DeleteYouTubePlaylist(uri)),
      Action::ToggleSaveTrack(uri) => self.dispatch(IoEvent::ToggleSaveTrack(uri)),
      Action::ToggleSaveCurrentItem => self.toggle_save_current_item(),
      Action::SaveAlbum(id) => self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(id)),
      Action::UnsaveAlbum(id) => self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id)),
      Action::SaveShow(id) => self.dispatch(IoEvent::CurrentUserSavedShowAdd(id)),
      Action::UnsaveShow(id) => self.dispatch(IoEvent::CurrentUserSavedShowDelete(id)),
      Action::FollowArtist(id) => self.dispatch(IoEvent::UserFollowArtists(vec![id])),
      Action::UnfollowArtist(id) => self.dispatch(IoEvent::UserUnfollowArtists(vec![id])),
      Action::AddFriendByCode(code) => self.dispatch(IoEvent::AddFriendByCode(code)),
      Action::AddFriendByUserId(user_id) => self.dispatch(IoEvent::AddFriendByUserId(user_id)),
      Action::UnfollowFriend(user_id) => self.dispatch(IoEvent::UnfollowFriend(user_id)),
      Action::SearchFriendUsers(query) => self.search_friend_users(query),
      Action::FavoriteRadioStation(station) => self.favorite_radio_station(station),
      Action::RemoveRadioStation(uri) => self.remove_saved_radio_station(uri),
      Action::Notify(msg, ttl) => self.set_status_message(msg, ttl),
      Action::NotifyError(msg, ttl) => self.set_error_status_message(msg, ttl),
      Action::Navigate(target) => apply_navigate(self, target),
      Action::Back => {
        self.pop_navigation_stack();
      }
      Action::LoadMore(target) => match target {
        super::ListTarget::PlaylistTracks => self.get_playlist_tracks_next(),
        super::ListTarget::SavedTracks => self.get_current_user_saved_tracks_next(),
        super::ListTarget::SavedShows => self.get_current_user_saved_shows_next(),
        super::ListTarget::ShowEpisodes => self.get_episode_table_next_page(),
      },
      Action::Sort { context, field } => self.apply_sort_field(context, field),
      Action::ToggleSortOrder(context) => self.toggle_sort_order(context),
      Action::Open(target) => match target {
        super::OpenTarget::Album { id, from_search } => {
          if from_search {
            self.track_table.context = Some(crate::core::app::TrackTableContext::AlbumSearch);
          }
          self.dispatch(IoEvent::GetAlbum(id));
        }
        super::OpenTarget::SavedAlbum(id) => self.open_saved_album(id),
        super::OpenTarget::Artist { id, name } => self.get_artist(id, name),
        super::OpenTarget::Playlist { id, from_search } => {
          // Same silent no-op on an unparseable id as today's opening paths.
          if let Some(playlist_id) = crate::infra::network::ids::playlist_id(&id) {
            let context = if from_search {
              crate::core::app::TrackTableContext::PlaylistSearch
            } else {
              crate::core::app::TrackTableContext::MyPlaylists
            };
            self.open_playlist_tracks(playlist_id, context);
          }
        }
        super::OpenTarget::SourcePlaylist(uri) => self.open_source_playlist_tracks(uri),
        super::OpenTarget::PlaylistFolder(target_id) => self.open_playlist_folder(target_id),
        super::OpenTarget::Show(id) => self.dispatch(IoEvent::GetShow(id)),
        super::OpenTarget::TrackAlbum(track_id) => {
          self.dispatch(IoEvent::GetAlbumForTrack(track_id));
        }
      },
      Action::OpenShowEpisodes(show) => {
        self.dispatch(IoEvent::GetShowEpisodes(Box::new(show)));
      }
      Action::OpenLibrary(target) => self.open_library_section(target),
      Action::OpenDiscover(target) => self.open_discover_mix(target),
      Action::SelectSource(source) => self.set_active_source(source),
      Action::LoadSourceSidebar(source) => self.load_source_sidebar(source),
      Action::OpenAddTrackDialog => self.begin_add_track_to_playlist_flow_from_selection(),
      Action::OpenAddTrackDialogFor {
        track_id,
        track_name,
      } => self.begin_add_track_to_playlist_flow(track_id, track_name),
      Action::OpenAddPlayingTrackDialog => self.begin_add_playing_track_to_playlist_flow(),
      Action::OpenRemoveTrackDialog => self.begin_remove_track_from_playlist_flow(),
      Action::JumpToAlbum => self.jump_to_album(),
      Action::JumpToArtist => self.jump_to_artist_album(),
      Action::JumpToContext => self.jump_to_context(),
      Action::CopyUrl(target) => match target {
        super::CopyTarget::CurrentSong => self.copy_song_url(),
        super::CopyTarget::CurrentAlbum => self.copy_album_url(),
      },
      Action::GenerateRecap => self.generate_recap(),
      Action::CycleStatsPeriod { forward } => self.cycle_stats_period(forward),
      Action::RecommendFromTrack(track) => self.load_recommendations_for_track(track),
      Action::RecommendFromArtist { id, name } => self.load_recommendations_for_artist(id, name),
      Action::RecommendFromTrackId { id, name } => {
        self.load_recommendations_for_track_id(id, name);
      }
      Action::StartParty => self.start_party(),
      Action::JoinParty { code, name } => self.join_party(code, name),
      Action::LeaveParty => self.leave_party(),
      Action::TogglePartyControlMode => self.toggle_party_control_mode(),
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
      Action::SaveSettings => {
        return ActionOutcome::SettingsSaved {
          saved: self.save_settings_from_items(),
        };
      }
      Action::CycleVisualizerStyle => self.cycle_visualizer_style(),
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
      // Gated on `ai-dj`, not `dj-core`: the bodies need the in-app DJ's events.
      Action::AskDj(text) => {
        #[cfg(feature = "ai-dj")]
        {
          self.ask_dj(text);
        }
        #[cfg(not(feature = "ai-dj"))]
        {
          let _ = text;
        }
      }
      Action::DjVibeShift => {
        #[cfg(feature = "ai-dj")]
        self.dj_vibe_shift();
      }
      Action::ToggleDjAutoQueue => {
        #[cfg(feature = "ai-dj")]
        self.toggle_dj_auto_queue();
      }
      Action::ToggleDjFreshOnly => {
        #[cfg(feature = "ai-dj")]
        self.toggle_dj_fresh_only();
      }
      Action::OpenDjSetup => {
        #[cfg(feature = "ai-dj")]
        self.open_dj_setup();
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
