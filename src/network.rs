use crate::app::{
  ActiveBlock, AlbumTableContext, App, Artist, ArtistBlock, EpisodeTableContext, RouteId,
  ScrollableResultPages, SelectedAlbum, SelectedFullAlbum, SelectedFullShow, SelectedShow,
  TrackTableContext,
};
use crate::config::ClientConfig;
use crate::ui::util::create_artist_string;
use anyhow::anyhow;
use chrono::TimeDelta;
use rspotify::{
  model::{
    album::SimplifiedAlbum,
    artist::FullArtist,
    enums::{AdditionalType, Country, RepeatState, SearchType},
    idtypes::{AlbumId, ArtistId, PlayContextId, PlayableId, PlaylistId, ShowId, TrackId, UserId},
    page::Page,
    playlist::{PlaylistItem, SimplifiedPlaylist},
    recommend::Recommendations,
    search::SearchResult,
    show::SimplifiedShow,
    track::FullTrack,
    Market, PlayableItem,
  },
  prelude::*,
  AuthCodeSpotify,
};
use serde::Deserialize;
use std::{
  sync::Arc,
  time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tokio::try_join;

#[cfg(feature = "streaming")]
use crate::player::StreamingPlayer;

pub enum IoEvent {
  GetCurrentPlayback,
  /// After a track transition (e.g., EndOfTrack), ensure we don't end up paused on the next item.
  /// The payload is the previous track identifier (either base62 id or a `spotify:track:` URI).
  EnsurePlaybackContinues(String),
  RefreshAuthentication,
  GetPlaylists,
  GetDevices,
  GetSearchResults(String, Option<Country>),
  SetTracksToTable(Vec<FullTrack>),
  GetMadeForYouPlaylistItems(PlaylistId<'static>, u32),
  GetPlaylistItems(PlaylistId<'static>, u32),
  GetCurrentSavedTracks(Option<u32>),
  StartPlayback(
    Option<PlayContextId<'static>>,
    Option<Vec<PlayableId<'static>>>,
    Option<usize>,
  ),
  UpdateSearchLimits(u32, u32),
  Seek(u32),
  NextTrack,
  PreviousTrack,
  Shuffle(bool), // desired shuffle state
  Repeat(RepeatState),
  PausePlayback,
  ChangeVolume(u8),
  GetArtist(ArtistId<'static>, String, Option<Country>),
  GetAlbumTracks(Box<SimplifiedAlbum>),
  GetRecommendationsForSeed(
    Option<Vec<ArtistId<'static>>>,
    Option<Vec<TrackId<'static>>>,
    Box<Option<FullTrack>>,
    Option<Country>,
  ),
  GetCurrentUserSavedAlbums(Option<u32>),
  CurrentUserSavedAlbumsContains(Vec<AlbumId<'static>>),
  CurrentUserSavedAlbumDelete(AlbumId<'static>),
  CurrentUserSavedAlbumAdd(AlbumId<'static>),
  UserUnfollowArtists(Vec<ArtistId<'static>>),
  UserFollowArtists(Vec<ArtistId<'static>>),
  UserFollowPlaylist(UserId<'static>, PlaylistId<'static>, Option<bool>),
  UserUnfollowPlaylist(UserId<'static>, PlaylistId<'static>),
  MadeForYouSearchAndAdd(String, Option<Country>),
  GetUser,
  ToggleSaveTrack(PlayableId<'static>),
  GetRecommendationsForTrackId(TrackId<'static>, Option<Country>),
  GetRecentlyPlayed,
  GetFollowedArtists(Option<ArtistId<'static>>),
  SetArtistsToTable(Vec<FullArtist>),
  UserArtistFollowCheck(Vec<ArtistId<'static>>),
  GetAlbum(AlbumId<'static>),
  TransferPlaybackToDevice(String),
  #[allow(dead_code)]
  AutoSelectStreamingDevice(String), // Auto-select a device by name (used for native streaming)
  GetAlbumForTrack(TrackId<'static>),
  CurrentUserSavedTracksContains(Vec<TrackId<'static>>),
  GetCurrentUserSavedShows(Option<u32>),
  CurrentUserSavedShowsContains(Vec<ShowId<'static>>),
  CurrentUserSavedShowDelete(ShowId<'static>),
  CurrentUserSavedShowAdd(ShowId<'static>),
  GetShowEpisodes(Box<SimplifiedShow>),
  GetShow(ShowId<'static>),
  GetCurrentShowEpisodes(ShowId<'static>, Option<u32>),
  AddItemToQueue(PlayableId<'static>),
  IncrementGlobalSongCount,
  FetchGlobalSongCount,
  GetLyrics(String, String, f64),
  /// Start playback from the user's saved tracks collection (Liked Songs)
  /// Takes the absolute position in the collection to start from
  /// NOTE: Currently unused - Spotify Web API doesn't support collection context URI
  /// Keeping for potential future use if Spotify adds support
  #[allow(dead_code)]
  StartCollectionPlayback(usize),
  /// Pre-fetch all saved tracks pages in background for seamless playback
  PreFetchAllSavedTracks,
  /// Pre-fetch all tracks from a playlist in background
  PreFetchAllPlaylistTracks(PlaylistId<'static>),
}

pub struct Network {
  pub spotify: AuthCodeSpotify,
  large_search_limit: u32,
  small_search_limit: u32,
  pub client_config: ClientConfig,
  pub app: Arc<Mutex<App>>,
  #[cfg(feature = "streaming")]
  streaming_player: Option<Arc<StreamingPlayer>>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct LrcResponse {
  syncedLyrics: Option<String>,
  plainLyrics: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GlobalSongCountResponse {
  count: u64,
}

impl Network {
  #[cfg(feature = "streaming")]
  pub fn new(
    spotify: AuthCodeSpotify,
    client_config: ClientConfig,
    app: &Arc<Mutex<App>>,
    streaming_player: Option<Arc<StreamingPlayer>>,
  ) -> Self {
    Network {
      spotify,
      large_search_limit: 50,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
      streaming_player,
    }
  }

  #[cfg(not(feature = "streaming"))]
  pub fn new(spotify: AuthCodeSpotify, client_config: ClientConfig, app: &Arc<Mutex<App>>) -> Self {
    Network {
      spotify,
      large_search_limit: 50,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
    }
  }

  /// Check if we're using native streaming AND it's the active playback device
  /// This ensures commands are routed correctly when user selects a different device like spotifyd
  #[cfg(feature = "streaming")]
  async fn is_native_streaming_active_for_playback(&self) -> bool {
    let player_connected = self
      .streaming_player
      .as_ref()
      .is_some_and(|p| p.is_connected());

    if !player_connected {
      return false;
    }

    // If user has explicitly selected a device (saved device_id), check if it matches native
    // If device_id is set, it means user made an explicit choice - respect that
    if let Some(ref device_id) = self.client_config.device_id {
      // Get native player's device name from streaming player
      if let Some(ref player) = self.streaming_player {
        // Native streaming device name is set - if saved device_id doesn't look like it's from native,
        // use API path. Unfortunately we don't have the device_id of native player easily,
        // so we rely on context checking below.
        let _ = (device_id, player); // Silence unused warnings
      }
    }

    // Get the native streaming device name
    let native_device_name = self
      .streaming_player
      .as_ref()
      .map(|p| p.device_name().to_lowercase());

    // Check if the current playback device matches the native streaming device
    let app = self.app.lock().await;
    if let Some(ref ctx) = app.current_playback_context {
      if let Some(ref native_name) = native_device_name {
        // Compare device names (case-insensitive partial match for flexibility)
        let current_device_name = ctx.device.name.to_lowercase();
        return current_device_name.contains(native_name)
          || native_name.contains(&current_device_name);
      }
    }

    // If no context yet, use API path - safer until we know which device is active
    // This ensures spotifyd works on first button press after startup
    false
  }

  /// Quick check if native streaming is connected (doesn't verify active device)
  #[cfg(feature = "streaming")]
  fn is_native_streaming_active(&self) -> bool {
    self
      .streaming_player
      .as_ref()
      .is_some_and(|p| p.is_connected())
  }

  #[allow(clippy::cognitive_complexity)]
  pub async fn handle_network_event(&mut self, io_event: IoEvent) {
    match io_event {
      IoEvent::RefreshAuthentication => {
        self.refresh_authentication().await;
      }
      IoEvent::EnsurePlaybackContinues(previous_track_id) => {
        self.ensure_playback_continues(previous_track_id).await;
      }
      IoEvent::GetPlaylists => {
        self.get_current_user_playlists().await;
      }
      IoEvent::GetUser => {
        self.get_user().await;
      }
      IoEvent::GetDevices => {
        self.get_devices().await;
      }
      IoEvent::GetCurrentPlayback => {
        self.get_current_playback().await;
      }
      IoEvent::SetTracksToTable(full_tracks) => {
        self.set_tracks_to_table(full_tracks).await;
      }
      IoEvent::GetSearchResults(search_term, country) => {
        self.get_search_results(search_term, country).await;
      }
      IoEvent::GetMadeForYouPlaylistItems(playlist_id, made_for_you_offset) => {
        self
          .get_made_for_you_playlist_tracks(playlist_id, made_for_you_offset)
          .await;
      }
      IoEvent::GetPlaylistItems(playlist_id, playlist_offset) => {
        self.get_playlist_tracks(playlist_id, playlist_offset).await;
      }
      IoEvent::GetCurrentSavedTracks(offset) => {
        self.get_current_user_saved_tracks(offset).await;
      }
      IoEvent::StartPlayback(context_uri, uris, offset) => {
        self.start_playback(context_uri, uris, offset).await;
      }
      IoEvent::UpdateSearchLimits(large_search_limit, small_search_limit) => {
        self.large_search_limit = large_search_limit;
        self.small_search_limit = small_search_limit;
      }
      IoEvent::Seek(position_ms) => {
        self.seek(position_ms).await;
      }
      IoEvent::NextTrack => {
        self.next_track().await;
      }
      IoEvent::PreviousTrack => {
        self.previous_track().await;
      }
      IoEvent::Repeat(repeat_state) => {
        self.repeat(repeat_state).await;
      }
      IoEvent::PausePlayback => {
        self.pause_playback().await;
      }
      IoEvent::ChangeVolume(volume) => {
        self.change_volume(volume).await;
      }
      IoEvent::GetArtist(artist_id, input_artist_name, country) => {
        self.get_artist(artist_id, input_artist_name, country).await;
      }
      IoEvent::GetAlbumTracks(album) => {
        self.get_album_tracks(album).await;
      }
      IoEvent::GetRecommendationsForSeed(seed_artists, seed_tracks, first_track, country) => {
        self
          .get_recommendations_for_seed(seed_artists, seed_tracks, first_track, country)
          .await;
      }
      IoEvent::GetCurrentUserSavedAlbums(offset) => {
        self.get_current_user_saved_albums(offset).await;
      }
      IoEvent::CurrentUserSavedAlbumsContains(album_ids) => {
        self.current_user_saved_albums_contains(album_ids).await;
      }
      IoEvent::CurrentUserSavedAlbumDelete(album_id) => {
        self.current_user_saved_album_delete(album_id).await;
      }
      IoEvent::CurrentUserSavedAlbumAdd(album_id) => {
        self.current_user_saved_album_add(album_id).await;
      }
      IoEvent::UserUnfollowArtists(artist_ids) => {
        self.user_unfollow_artists(artist_ids).await;
      }
      IoEvent::UserFollowArtists(artist_ids) => {
        self.user_follow_artists(artist_ids).await;
      }
      IoEvent::UserFollowPlaylist(playlist_owner_id, playlist_id, is_public) => {
        self
          .user_follow_playlist(playlist_owner_id, playlist_id, is_public)
          .await;
      }
      IoEvent::UserUnfollowPlaylist(user_id, playlist_id) => {
        self.user_unfollow_playlist(user_id, playlist_id).await;
      }
      IoEvent::MadeForYouSearchAndAdd(search_term, country) => {
        self.made_for_you_search_and_add(search_term, country).await;
      }
      IoEvent::ToggleSaveTrack(track_id) => {
        self.toggle_save_track(track_id).await;
      }
      IoEvent::GetRecommendationsForTrackId(track_id, country) => {
        self
          .get_recommendations_for_track_id(track_id, country)
          .await;
      }
      IoEvent::GetRecentlyPlayed => {
        self.get_recently_played().await;
      }
      IoEvent::GetFollowedArtists(after) => {
        self.get_followed_artists(after).await;
      }
      IoEvent::SetArtistsToTable(full_artists) => {
        self.set_artists_to_table(full_artists).await;
      }
      IoEvent::UserArtistFollowCheck(artist_ids) => {
        self.user_artist_check_follow(artist_ids).await;
      }
      IoEvent::GetAlbum(album_id) => {
        self.get_album(album_id).await;
      }
      IoEvent::TransferPlaybackToDevice(device_id) => {
        self.transfert_playback_to_device(device_id).await;
      }
      IoEvent::AutoSelectStreamingDevice(device_name) => {
        self.auto_select_streaming_device(device_name).await;
      }
      IoEvent::GetAlbumForTrack(track_id) => {
        self.get_album_for_track(track_id).await;
      }
      IoEvent::Shuffle(shuffle_state) => {
        self.shuffle(shuffle_state).await;
      }
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        self.current_user_saved_tracks_contains(track_ids).await;
      }
      IoEvent::GetCurrentUserSavedShows(offset) => {
        self.get_current_user_saved_shows(offset).await;
      }
      IoEvent::CurrentUserSavedShowsContains(show_ids) => {
        self.current_user_saved_shows_contains(show_ids).await;
      }
      IoEvent::CurrentUserSavedShowDelete(show_id) => {
        self.current_user_saved_shows_delete(show_id).await;
      }
      IoEvent::CurrentUserSavedShowAdd(show_id) => {
        self.current_user_saved_shows_add(show_id).await;
      }
      IoEvent::GetShowEpisodes(show) => {
        self.get_show_episodes(show).await;
      }
      IoEvent::GetShow(show_id) => {
        self.get_show(show_id).await;
      }
      IoEvent::GetCurrentShowEpisodes(show_id, offset) => {
        self.get_current_show_episodes(show_id, offset).await;
      }
      IoEvent::AddItemToQueue(item) => {
        self.add_item_to_queue(item).await;
      }
      IoEvent::IncrementGlobalSongCount => {
        self.increment_global_song_count().await;
      }
      IoEvent::FetchGlobalSongCount => {
        self.fetch_global_song_count().await;
      }
      IoEvent::GetLyrics(track, artist, duration) => {
        self.get_lyrics(track, artist, duration).await;
      }
      IoEvent::StartCollectionPlayback(offset) => {
        self.start_collection_playback(offset).await;
      }
      IoEvent::PreFetchAllSavedTracks => {
        // Spawn prefetch as a separate task to avoid blocking playback
        let spotify = self.spotify.clone();
        let app = self.app.clone();
        let large_search_limit = self.large_search_limit;
        tokio::spawn(async move {
          Self::prefetch_all_saved_tracks_task(spotify, app, large_search_limit).await;
        });
      }
      IoEvent::PreFetchAllPlaylistTracks(playlist_id) => {
        // Spawn prefetch as a separate task to avoid blocking playback
        let spotify = self.spotify.clone();
        let app = self.app.clone();
        let large_search_limit = self.large_search_limit;
        tokio::spawn(async move {
          Self::prefetch_all_playlist_tracks_task(spotify, app, large_search_limit, playlist_id)
            .await;
        });
      }
    };

    let mut app = self.app.lock().await;
    app.is_loading = false;
  }

  async fn handle_error(&mut self, e: anyhow::Error) {
    let mut app = self.app.lock().await;
    app.handle_error(e);
  }

  async fn get_user(&mut self) {
    match self.spotify.me().await {
      Ok(user) => {
        let mut app = self.app.lock().await;
        app.user = Some(user);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_devices(&mut self) {
    if let Ok(devices_vec) = self.spotify.device().await {
      let mut app = self.app.lock().await;
      app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
      if !devices_vec.is_empty() {
        // Wrap Vec<Device> in DevicePayload
        let result = rspotify::model::device::DevicePayload {
          devices: devices_vec,
        };
        app.devices = Some(result);
        // Select the first device in the list
        app.selected_device_index = Some(0);
      }
    }
  }

  async fn get_current_playback(&mut self) {
    // When using native streaming, the Spotify API returns stale server-side state
    // that doesn't reflect recent local changes (volume, shuffle, repeat, play/pause).
    // We need to preserve these local states and restore them after getting the API response.
    #[cfg(feature = "streaming")]
    let local_state: Option<(Option<u8>, bool, rspotify::model::RepeatState, bool)> =
      if self.is_native_streaming_active() {
        let app = self.app.lock().await;
        if let Some(ref ctx) = app.current_playback_context {
          let volume = self.streaming_player.as_ref().map(|p| p.get_volume());
          Some((volume, ctx.shuffle_state, ctx.repeat_state, ctx.is_playing))
        } else {
          // No existing context yet. DON'T override API values
          // Let the first API response set the true state from Spotify
          // The startup IoEvent::Shuffle call will sync our preference to Spotify
          None
        }
      } else {
        None
      };

    let context = self
      .spotify
      .current_playback(
        None,
        Some(&[AdditionalType::Episode, AdditionalType::Track]),
      )
      .await;

    let mut app = self.app.lock().await;

    match context {
      Ok(Some(mut c)) => {
        app.instant_since_last_current_playback_poll = Instant::now();

        // Process track info before storing context (avoids cloning)
        if let Some(ref item) = c.item {
          match item {
            PlayableItem::Track(track) => {
              if let Some(ref track_id) = track.id {
                let track_id_str = track_id.id().to_string();

                // Check if this is a new track
                if app.last_track_id.as_ref() != Some(&track_id_str) {
                  if app.user_config.behavior.enable_global_song_count {
                    app.dispatch(IoEvent::IncrementGlobalSongCount);
                  }

                  // Trigger lyrics fetch
                  let duration_secs = track.duration.num_seconds() as f64;
                  app.dispatch(IoEvent::GetLyrics(
                    track.name.clone(),
                    create_artist_string(&track.artists),
                    duration_secs,
                  ));
                }

                app.last_track_id = Some(track_id_str);
                app.dispatch(IoEvent::CurrentUserSavedTracksContains(vec![track_id
                  .clone()
                  .into_static()]));
              };
            }
            PlayableItem::Episode(_episode) => { /*should map this to following the podcast show*/ }
          }
        };

        // Preserve local streaming states (API returns stale server-side state)
        // Note: We only preserve volume, shuffle, and repeat - NOT is_playing.
        // is_playing must come from the API response or player events, otherwise
        // skipping tracks causes the new track to appear paused (stale state from old track).
        #[cfg(feature = "streaming")]
        if let Some((volume, shuffle, repeat, _is_playing)) = local_state {
          if let Some(vol) = volume {
            c.device.volume_percent = Some(vol.into());
          }
          c.shuffle_state = shuffle;
          c.repeat_state = repeat;
        }

        // On first load with native streaming AND native device is active,
        // override API shuffle with saved preference.
        // Skip this if using external device like spotifyd
        #[cfg(feature = "streaming")]
        if local_state.is_none() {
          // Check if the device we just got from API matches native streaming
          let native_device_name = self
            .streaming_player
            .as_ref()
            .map(|p| p.device_name().to_lowercase());
          let current_device_name = c.device.name.to_lowercase();
          let is_native_device = native_device_name
            .as_ref()
            .is_some_and(|n| current_device_name.contains(n) || n.contains(&current_device_name));

          if is_native_device {
            c.shuffle_state = app.user_config.behavior.shuffle_enabled;
            // Proactively set native shuffle on first load to keep backend in sync
            if let Some(ref player) = self.streaming_player {
              let _ = player.set_shuffle(app.user_config.behavior.shuffle_enabled);
            }
          }
        }

        app.current_playback_context = Some(c);

        // Update is_streaming_active based on whether the current device matches native streaming
        // This ensures correct polling interval (1s for external devices, 5s for native)
        #[cfg(feature = "streaming")]
        {
          let native_device_name = self
            .streaming_player
            .as_ref()
            .map(|p| p.device_name().to_lowercase());
          if let Some(ref ctx) = app.current_playback_context {
            if let Some(ref native_name) = native_device_name {
              let current_device_name = ctx.device.name.to_lowercase();
              let is_native_device = current_device_name.contains(native_name)
                || native_name.contains(&current_device_name);
              app.is_streaming_active = is_native_device;
            }
          }
        }

        // Only clear native track info if API data matches the native player's track
        // This prevents stale API responses (returning old track) from clearing
        // the correct native track info we got from TrackChanged event
        if let Some(ref native_info) = app.native_track_info {
          if let Some(ref ctx) = app.current_playback_context {
            if let Some(ref item) = ctx.item {
              let api_track_name = match item {
                PlayableItem::Track(t) => &t.name,
                PlayableItem::Episode(e) => &e.name,
              };
              // Only clear if names match (API caught up to native player)
              if api_track_name == &native_info.name {
                app.native_track_info = None;
              }
            }
          }
        } else {
          app.native_track_info = None;
        }
      }
      Ok(None) => {
        app.instant_since_last_current_playback_poll = Instant::now();
      }
      Err(e) => {
        drop(app); // Release lock before error handler
        self.handle_error(anyhow!(e)).await;
        return;
      }
    }

    app.seek_ms.take();
    app.is_fetching_current_playback = false;
  }

  async fn current_user_saved_tracks_contains(&mut self, ids: Vec<TrackId<'_>>) {
    match self
      .spotify
      .current_user_saved_tracks_contains(ids.clone())
      .await
    {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in ids.iter().enumerate() {
          if let Some(is_liked) = is_saved_vec.get(i) {
            if *is_liked {
              app.liked_song_ids_set.insert(id.id().to_string());
            } else {
              // The song is not liked, so check if it should be removed
              if app.liked_song_ids_set.contains(id.id()) {
                app.liked_song_ids_set.remove(id.id());
              }
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_playlist_tracks(&mut self, playlist_id: PlaylistId<'_>, playlist_offset: u32) {
    // Use playlist_items_manual to fetch pages incrementally with proper metadata
    match self
      .spotify
      .playlist_items_manual(
        playlist_id,
        None,
        None,
        Some(self.large_search_limit),
        Some(playlist_offset),
      )
      .await
    {
      Ok(playlist_tracks) => {
        self.set_playlist_tracks_to_table(&playlist_tracks).await;

        let mut app = self.app.lock().await;
        app.playlist_tracks = Some(playlist_tracks);
        app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn set_playlist_tracks_to_table(&mut self, playlist_track_page: &Page<PlaylistItem>) {
    let tracks = playlist_track_page
      .items
      .clone()
      .into_iter()
      .filter_map(|item| item.track)
      .filter_map(|track| match track {
        PlayableItem::Track(full_track) => Some(full_track),
        PlayableItem::Episode(_) => None,
      })
      .collect::<Vec<FullTrack>>();
    self.set_tracks_to_table(tracks).await;
  }

  async fn set_tracks_to_table(&mut self, tracks: Vec<FullTrack>) {
    // Extract track IDs before moving tracks (avoids clone of entire vector)
    let track_ids: Vec<TrackId<'static>> = tracks
      .iter()
      .filter_map(|item| item.id.as_ref().map(|id| id.clone().into_static()))
      .collect();

    let mut app = self.app.lock().await;

    // Clamp selected_index to valid range after loading new tracks
    let track_count = tracks.len();
    if track_count > 0 {
      let max_index = track_count.saturating_sub(1);
      if app.track_table.selected_index > max_index {
        app.track_table.selected_index = max_index;
      }
    } else {
      app.track_table.selected_index = 0;
    }

    app.track_table.tracks = tracks; // Move instead of clone

    app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
  }

  async fn set_artists_to_table(&mut self, artists: Vec<FullArtist>) {
    let mut app = self.app.lock().await;
    app.artists = artists;
  }

  async fn get_made_for_you_playlist_tracks(
    &mut self,
    playlist_id: PlaylistId<'_>,
    made_for_you_offset: u32,
  ) {
    match self
      .spotify
      .playlist_items_manual(
        playlist_id,
        None,
        None,
        Some(self.large_search_limit),
        Some(made_for_you_offset),
      )
      .await
    {
      Ok(made_for_you_tracks) => {
        self
          .set_playlist_tracks_to_table(&made_for_you_tracks)
          .await;

        let mut app = self.app.lock().await;
        app.made_for_you_tracks = Some(made_for_you_tracks);
        if app.get_current_route().id != RouteId::TrackTable {
          app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_user_saved_shows(&mut self, offset: Option<u32>) {
    match self
      .spotify
      .get_saved_show_manual(Some(self.large_search_limit), offset)
      .await
    {
      Ok(saved_shows) => {
        // not to show a blank page
        if !saved_shows.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.saved_shows.add_pages(saved_shows);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_shows_contains(&mut self, show_ids: Vec<ShowId<'_>>) {
    match self.spotify.check_users_saved_shows(show_ids.clone()).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in show_ids.iter().enumerate() {
          if let Some(is_saved) = is_saved_vec.get(i) {
            if *is_saved {
              app.saved_show_ids_set.insert(id.id().to_string());
            } else {
              // The show is not saved, so check if it should be removed
              if app.saved_show_ids_set.contains(id.id()) {
                app.saved_show_ids_set.remove(id.id());
              }
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_show_episodes(&mut self, show: Box<SimplifiedShow>) {
    let show_id = show.id.clone();
    match self
      .spotify
      .get_shows_episodes_manual(show_id, None, Some(self.large_search_limit), Some(0))
      .await
    {
      Ok(episodes) => {
        if !episodes.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.show_episodes = ScrollableResultPages::new();
          app.library.show_episodes.add_pages(episodes);

          app.selected_show_simplified = Some(SelectedShow { show: *show });

          app.episode_table_context = EpisodeTableContext::Simplified;

          app.push_navigation_stack(RouteId::PodcastEpisodes, ActiveBlock::EpisodeTable);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_show(&mut self, show_id: ShowId<'_>) {
    match self.spotify.get_a_show(show_id, None).await {
      Ok(show) => {
        let selected_show = SelectedFullShow { show };

        let mut app = self.app.lock().await;

        app.selected_show_full = Some(selected_show);

        app.episode_table_context = EpisodeTableContext::Full;
        app.push_navigation_stack(RouteId::PodcastEpisodes, ActiveBlock::EpisodeTable);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_show_episodes(&mut self, show_id: ShowId<'_>, offset: Option<u32>) {
    match self
      .spotify
      .get_shows_episodes_manual(show_id, None, Some(self.large_search_limit), offset)
      .await
    {
      Ok(episodes) => {
        if !episodes.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.show_episodes.add_pages(episodes);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_search_results(&mut self, search_term: String, country: Option<Country>) {
    // Don't pass market to search - when market is specified, Spotify doesn't return
    // available_markets field, but rspotify 0.14 models require it for tracks/albums.
    // We'll handle null playlist fields by searching playlists separately without requiring all fields.
    let _market = country.map(Market::Country);

    let search_track = self.spotify.search(
      &search_term,
      SearchType::Track,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_artist = self.spotify.search(
      &search_term,
      SearchType::Artist,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_album = self.spotify.search(
      &search_term,
      SearchType::Album,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_playlist = self.spotify.search(
      &search_term,
      SearchType::Playlist,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    let search_show = self.spotify.search(
      &search_term,
      SearchType::Show,
      None,
      None, // include_external
      Some(self.small_search_limit),
      Some(0),
    );

    // Run all futures concurrently
    let (main_search, playlist_search) = tokio::join!(
      async { try_join!(search_track, search_artist, search_album, search_show) },
      search_playlist
    );

    // Handle main search results
    let (track_result, artist_result, album_result, show_result) = match main_search {
      Ok((
        SearchResult::Tracks(tracks),
        SearchResult::Artists(artists),
        SearchResult::Albums(albums),
        SearchResult::Shows(shows),
      )) => (Some(tracks), Some(artists), Some(albums), Some(shows)),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
      _ => return,
    };

    // Handle playlist search separately since it can fail with null fields from Spotify API
    // Silently ignore playlist errors - this is a known Spotify API issue
    let playlist_result = match playlist_search {
      Ok(SearchResult::Playlists(playlists)) => Some(playlists),
      Err(_) => None,
      _ => None,
    };

    let mut app = self.app.lock().await;

    if let Some(ref album_results) = album_result {
      let artist_ids = album_results
        .items
        .iter()
        .filter_map(|item| {
          item
            .id
            .as_ref()
            .map(|id| ArtistId::from_id(id.id()).unwrap().into_static())
        })
        .collect();

      // Check if these artists are followed
      app.dispatch(IoEvent::UserArtistFollowCheck(artist_ids));

      let album_ids = album_results
        .items
        .iter()
        .filter_map(|album| {
          album
            .id
            .as_ref()
            .map(|id| AlbumId::from_id(id.id()).unwrap().into_static())
        })
        .collect();

      // Check if these albums are saved
      app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(album_ids));
    }

    if let Some(ref show_results) = show_result {
      let show_ids = show_results
        .items
        .iter()
        .map(|show| show.id.clone().into_static())
        .collect();

      // check if these shows are saved
      app.dispatch(IoEvent::CurrentUserSavedShowsContains(show_ids));
    }

    app.search_results.tracks = track_result;
    app.search_results.artists = artist_result;
    app.search_results.albums = album_result;
    app.search_results.playlists = playlist_result;
    app.search_results.shows = show_result;
  }

  async fn get_current_user_saved_tracks(&mut self, offset: Option<u32>) {
    match self
      .spotify
      .current_user_saved_tracks_manual(None, Some(self.large_search_limit), offset)
      .await
    {
      Ok(saved_tracks) => {
        let mut app = self.app.lock().await;
        app.track_table.tracks = saved_tracks
          .items
          .clone()
          .into_iter()
          .map(|item| item.track)
          .collect::<Vec<FullTrack>>();

        saved_tracks.items.iter().for_each(|item| {
          if let Some(track_id) = &item.track.id {
            app.liked_song_ids_set.insert(track_id.to_string());
          }
        });

        app.library.saved_tracks.add_pages(saved_tracks);
        app.track_table.context = Some(TrackTableContext::SavedTracks);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn start_playback(
    &mut self,
    context_id: Option<PlayContextId<'_>>,
    uris: Option<Vec<PlayableId<'_>>>,
    offset: Option<usize>,
  ) {
    let device_id = self.client_config.device_id.as_deref();

    // If we're playing a specific track (with offset), temporarily disable shuffle
    // to ensure the selected track plays first
    let should_disable_shuffle = offset.is_some() && uris.is_some();
    let mut original_shuffle_state = false;

    if should_disable_shuffle {
      // Get current shuffle state
      if let Ok(Some(playback)) = self.spotify.current_playback(None, None::<Vec<_>>).await {
        original_shuffle_state = playback.shuffle_state;
        if original_shuffle_state {
          // Temporarily disable shuffle
          let _ = self.spotify.shuffle(false, device_id).await;
          // Small delay to let the shuffle state update
          tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
      }
    }

    // Check if we have both context and uris - this means play specific track within context
    let has_both = context_id.is_some() && uris.is_some();

    let result = if has_both {
      // Special case: Play a specific track within a context
      // This ensures the selected track plays first, even with shuffle enabled
      let context = context_id.unwrap();
      let track_uris = uris.unwrap();

      if let Some(first_uri) = track_uris.first() {
        // Convert PlayableId to a URI string
        let uri_string = match first_uri {
          PlayableId::Track(track_id) => format!("spotify:track:{}", track_id.id()),
          PlayableId::Episode(episode_id) => format!("spotify:episode:{}", episode_id.id()),
        };

        let offset = rspotify::model::Offset::Uri(uri_string);
        self
          .spotify
          .start_context_playback(context, device_id, Some(offset), None)
          .await
      } else {
        self
          .spotify
          .start_context_playback(context, device_id, None, None)
          .await
      }
    } else if let Some(context_id) = context_id {
      // Play from context without specifying a starting track
      self
        .spotify
        .start_context_playback(context_id, device_id, None, None)
        .await
    } else if let Some(mut uris) = uris {
      // For URI-based playback, reorder the list to put the selected track first
      // This ensures the user's selected track plays first, regardless of shuffle mode
      if let Some(offset_pos) = offset {
        if offset_pos < uris.len() && offset_pos > 0 {
          // Move the track at offset_pos to the front
          let selected = uris.remove(offset_pos);
          uris.insert(0, selected);
        }
      }

      self
        .spotify
        .start_uris_playback(uris, device_id, None, None)
        .await
    } else {
      // Resume playback - use native player if available for instant response
      #[cfg(feature = "streaming")]
      if self.is_native_streaming_active() {
        if let Some(ref player) = self.streaming_player {
          player.play();
          // Update UI state immediately
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
          }
          return;
        }
      }
      self.spotify.resume_playback(device_id, None).await
    };

    match result {
      Ok(()) => {
        // Re-enable shuffle if it was on before
        if should_disable_shuffle && original_shuffle_state {
          // Small delay to let playback start before re-enabling shuffle
          tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
          let _ = self.spotify.shuffle(true, device_id).await;
        }

        // Reset progress and update playing state immediately
        {
          let mut app = self.app.lock().await;
          app.song_progress_ms = 0;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
          }
        }

        // Wait for Spotify's API to sync before fetching updated state
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        self.get_current_playback().await;
      }
      Err(e) => {
        // Re-enable shuffle even on error if it was on before
        if should_disable_shuffle && original_shuffle_state {
          let _ = self.spotify.shuffle(true, device_id).await;
        }
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  /// Start playback from the user's saved tracks collection (Liked Songs)
  /// Uses a direct HTTP call since rspotify doesn't support the collection context URI
  async fn start_collection_playback(&mut self, offset: usize) {
    // Get user ID to construct collection context URI
    let user_id = {
      let app = self.app.lock().await;
      app.user.as_ref().map(|u| u.id.to_string())
    };

    let user_id = match user_id {
      Some(id) => id,
      None => {
        self.handle_error(anyhow!("User not logged in")).await;
        return;
      }
    };

    // Get access token from rspotify client
    let token = {
      let token_lock = self
        .spotify
        .token
        .lock()
        .await
        .expect("Failed to lock token");
      token_lock.as_ref().map(|t| t.access_token.clone())
    };

    let access_token = match token {
      Some(t) => t,
      None => {
        self
          .handle_error(anyhow!("No access token available"))
          .await;
        return;
      }
    };

    // Construct the collection context URI: spotify:user:{user_id}:collection
    let context_uri = format!("spotify:user:{}:collection", user_id);

    // Build the request body
    let mut body = serde_json::json!({
      "context_uri": context_uri,
      "offset": { "position": offset }
    });

    // Add device_id if configured
    if let Some(ref device_id) = self.client_config.device_id {
      body["device_id"] = serde_json::json!(device_id);
    }

    // Make the API request using reqwest
    let client = reqwest::Client::new();
    let url = match self.client_config.device_id.as_ref() {
      Some(device_id) => format!(
        "https://api.spotify.com/v1/me/player/play?device_id={}",
        device_id
      ),
      None => "https://api.spotify.com/v1/me/player/play".to_string(),
    };

    let result = client
      .put(&url)
      .header("Authorization", format!("Bearer {}", access_token))
      .header("Content-Type", "application/json")
      .json(&body)
      .send()
      .await;

    match result {
      Ok(response) => {
        if response.status().is_success() {
          // Reset progress and update playing state immediately
          {
            let mut app = self.app.lock().await;
            app.song_progress_ms = 0;
            if let Some(ctx) = &mut app.current_playback_context {
              ctx.is_playing = true;
            }
          }

          // Wait for Spotify's API to sync before fetching updated state
          tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
          self.get_current_playback().await;
        } else {
          let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
          self
            .handle_error(anyhow!(
              "Failed to start collection playback: {}",
              error_text
            ))
            .await;
        }
      }
      Err(e) => {
        self
          .handle_error(anyhow!("HTTP request failed: {}", e))
          .await;
      }
    }
  }

  /// Pre-fetch all saved tracks pages in background for seamless playback
  /// This loads all remaining pages that haven't been loaded yet
  /// Runs as a separate async task to avoid blocking other operations
  async fn prefetch_all_saved_tracks_task(
    spotify: AuthCodeSpotify,
    app: Arc<Mutex<App>>,
    large_search_limit: u32,
  ) {
    // Get current state
    let (current_total, pages_loaded) = {
      let app = app.lock().await;
      if let Some(saved_tracks) = app.library.saved_tracks.get_results(Some(0)) {
        (
          saved_tracks.total,
          app.library.saved_tracks.pages.len() as u32,
        )
      } else {
        return; // No saved tracks loaded yet
      }
    };

    // Calculate how many tracks we already have
    let tracks_loaded = pages_loaded * large_search_limit;

    // Fetch remaining pages (limit to reasonable amount to avoid memory issues)
    let max_tracks_to_prefetch = 500; // ~10 pages
    let mut offset = tracks_loaded;

    while offset < current_total && offset < tracks_loaded + max_tracks_to_prefetch {
      match spotify
        .current_user_saved_tracks_manual(None, Some(large_search_limit), Some(offset))
        .await
      {
        Ok(saved_tracks) => {
          let mut app = app.lock().await;
          // Add liked song IDs to the set
          saved_tracks.items.iter().for_each(|item| {
            if let Some(track_id) = &item.track.id {
              app.liked_song_ids_set.insert(track_id.to_string());
            }
          });
          // Add page to the saved tracks
          app.library.saved_tracks.pages.push(saved_tracks);
        }
        Err(_e) => {
          // Silently fail in background task - don't show errors to user
          break;
        }
      }
      offset += large_search_limit;
    }
  }

  /// Pre-fetch all tracks from a playlist in background
  /// Runs as a separate async task to avoid blocking other operations
  async fn prefetch_all_playlist_tracks_task(
    spotify: AuthCodeSpotify,
    app: Arc<Mutex<App>>,
    large_search_limit: u32,
    playlist_id: PlaylistId<'static>,
  ) {
    // Get current playlist state
    let current_total = {
      let app = app.lock().await;
      if let Some(playlist_tracks) = &app.playlist_tracks {
        playlist_tracks.total
      } else {
        return;
      }
    };

    // Get current offset
    let current_offset = {
      let app = app.lock().await;
      app.playlist_offset
    };

    // Fetch remaining pages (limit to avoid memory issues)
    let max_tracks_to_prefetch = 500; // ~10 pages
    let mut offset = current_offset + large_search_limit;

    while offset < current_total && offset < current_offset + max_tracks_to_prefetch {
      match spotify
        .playlist_items_manual(
          playlist_id.clone(),
          None,
          None,
          Some(large_search_limit),
          Some(offset),
        )
        .await
      {
        Ok(playlist_page) => {
          // Store the fetched tracks
          let mut app = app.lock().await;
          // Extend playlist tracks items
          if let Some(ref mut existing) = app.playlist_tracks {
            existing.items.extend(playlist_page.items);
            existing.total = playlist_page.total; // Update total in case it changed
          }
        }
        Err(_e) => {
          // Silently fail in background task - don't show errors to user
          break;
        }
      }
      offset += large_search_limit;
    }
  }

  async fn seek(&mut self, position_ms: u32) {
    // Use native streaming player for instant seek (no network delay)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active() {
      if let Some(ref player) = self.streaming_player {
        player.seek(position_ms);
        // Update UI immediately without API polling
        let mut app = self.app.lock().await;
        app.song_progress_ms = position_ms as u128;
        app.seek_ms = None;
        return;
      }
    }

    // Fallback to API-based seek
    let device_id = self.client_config.device_id.as_deref();
    let position = TimeDelta::milliseconds(position_ms as i64);

    match self.spotify.seek_track(position, device_id).await {
      Ok(()) => {
        // Reduced delay for API seek (still needed for server sync)
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn next_track(&mut self) {
    // Use native streaming player for instant skip (no network delay)
    // BUT only if native streaming device is the active playback device
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback().await {
      if let Some(ref player) = self.streaming_player {
        player.activate();
        player.next();
        // librespot can occasionally land in a paused state after a skip.
        // Schedule a short delayed resume to avoid racing the track transition.
        let player = Arc::clone(player);
        tokio::spawn(async move {
          tokio::time::sleep(Duration::from_millis(300)).await;
          player.activate();
          player.play();
        });
        // Reset progress immediately for UI feedback
        let mut app = self.app.lock().await;
        app.song_progress_ms = 0;
        // The TrackChanged event will trigger GetCurrentPlayback for full metadata
        return;
      }
    }

    // Store the current track ID before skipping
    let (old_track_id, was_playing) = {
      let mut app = self.app.lock().await;
      // Reset progress immediately for instant UI feedback
      app.song_progress_ms = 0;
      (
        app.last_track_id.clone(),
        app
          .current_playback_context
          .as_ref()
          .map(|c| c.is_playing)
          .unwrap_or(false),
      )
    };

    // Fallback to API-based skip
    match self
      .spotify
      .next_track(self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        // Retry mechanism: Poll multiple times until we get updated metadata
        // Spotify's API can be slow to update after a skip command
        for attempt in 0..5 {
          let delay = if attempt == 0 { 100 } else { 200 * attempt }; // 100ms, 200ms, 400ms, 600ms, 800ms
          tokio::time::sleep(Duration::from_millis(delay as u64)).await;
          self.get_current_playback().await;

          // Check if we got the new track - if so, stop retrying
          let app = self.app.lock().await;
          let current_track_id = app.last_track_id.clone();
          if current_track_id != old_track_id {
            // Successfully got new track metadata
            break;
          }
        }

        // If we were playing before the skip but the next track lands paused, resume.
        if was_playing {
          let should_resume = {
            let app = self.app.lock().await;
            let track_changed = app.last_track_id != old_track_id;
            let is_paused = app
              .current_playback_context
              .as_ref()
              .map(|c| !c.is_playing)
              .unwrap_or(false);
            track_changed && is_paused
          };

          if should_resume {
            if let Err(e) = self
              .spotify
              .resume_playback(self.client_config.device_id.as_deref(), None)
              .await
            {
              self.handle_error(anyhow!(e)).await;
            } else {
              tokio::time::sleep(Duration::from_millis(250)).await;
              self.get_current_playback().await;
            }
          }
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn previous_track(&mut self) {
    // Use native streaming player for instant skip (no network delay)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active() {
      if let Some(ref player) = self.streaming_player {
        player.activate();
        player.prev();
        // librespot can occasionally land in a paused state after a skip.
        // Schedule a short delayed resume to avoid racing the track transition.
        let player = Arc::clone(player);
        tokio::spawn(async move {
          tokio::time::sleep(Duration::from_millis(300)).await;
          player.activate();
          player.play();
        });
        // Reset progress immediately for UI feedback
        let mut app = self.app.lock().await;
        app.song_progress_ms = 0;
        // The TrackChanged event will trigger GetCurrentPlayback for full metadata
        return;
      }
    }

    // Store the current track ID before skipping
    let (old_track_id, was_playing) = {
      let mut app = self.app.lock().await;
      // Reset progress immediately for instant UI feedback
      app.song_progress_ms = 0;
      (
        app.last_track_id.clone(),
        app
          .current_playback_context
          .as_ref()
          .map(|c| c.is_playing)
          .unwrap_or(false),
      )
    };

    // Fallback to API-based skip
    match self
      .spotify
      .previous_track(self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        // Retry mechanism: Poll multiple times until we get updated metadata
        // Spotify's API can be slow to update after a skip command
        for attempt in 0..5 {
          let delay = if attempt == 0 { 100 } else { 200 * attempt }; // 100ms, 200ms, 400ms, 600ms, 800ms
          tokio::time::sleep(Duration::from_millis(delay as u64)).await;
          self.get_current_playback().await;

          // Check if we got the new track - if so, stop retrying
          let app = self.app.lock().await;
          let current_track_id = app.last_track_id.clone();
          if current_track_id != old_track_id {
            // Successfully got new track metadata
            break;
          }
        }

        // If we were playing before the skip but the next track lands paused, resume.
        if was_playing {
          let should_resume = {
            let app = self.app.lock().await;
            let track_changed = app.last_track_id != old_track_id;
            let is_paused = app
              .current_playback_context
              .as_ref()
              .map(|c| !c.is_playing)
              .unwrap_or(false);
            track_changed && is_paused
          };

          if should_resume {
            if let Err(e) = self
              .spotify
              .resume_playback(self.client_config.device_id.as_deref(), None)
              .await
            {
              self.handle_error(anyhow!(e)).await;
            } else {
              tokio::time::sleep(Duration::from_millis(250)).await;
              self.get_current_playback().await;
            }
          }
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn shuffle(&mut self, desired_shuffle_state: bool) {
    let new_shuffle_state = desired_shuffle_state;

    // Prefer native streaming control when available
    #[cfg(feature = "streaming")]
    if let Some(ref player) = self.streaming_player {
      player.activate();
      match player.set_shuffle(new_shuffle_state) {
        Ok(()) => {
          // Update UI immediately
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.shuffle_state = new_shuffle_state;
          }
          app.user_config.behavior.shuffle_enabled = new_shuffle_state;
          let _ = app.user_config.save_config();
          return;
        }
        Err(e) => {
          // Fall back to API path, but surface the native error for visibility
          self.handle_error(anyhow!(e)).await;
        }
      }
    }

    // Fallback: API-based shuffle (updates UI after API call succeeds)
    match self
      .spotify
      .shuffle(new_shuffle_state, self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        // Update the UI eagerly (otherwise the UI will wait until the next 5 second interval
        // due to polling playback context)
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.shuffle_state = new_shuffle_state;
        };
        app.user_config.behavior.shuffle_enabled = new_shuffle_state;
        let _ = app.user_config.save_config();
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn repeat(&mut self, repeat_state: RepeatState) {
    let next_repeat_state = match repeat_state {
      RepeatState::Off => RepeatState::Context,
      RepeatState::Context => RepeatState::Track,
      RepeatState::Track => RepeatState::Off,
    };

    // When using native streaming, update UI immediately for instant feedback
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active() {
      {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.repeat_state = next_repeat_state;
        }
      }
      // Still send to API to sync server state (fire and forget - don't wait)
      let _ = self
        .spotify
        .repeat(next_repeat_state, self.client_config.device_id.as_deref())
        .await;
      return;
    }

    // Fallback: API-based repeat (updates UI after API call succeeds)
    match self
      .spotify
      .repeat(next_repeat_state, self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.repeat_state = next_repeat_state;
        };
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn pause_playback(&mut self) {
    // Use native streaming player for instant pause (no network delay)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active() {
      if let Some(ref player) = self.streaming_player {
        player.pause();
        // Update UI state immediately
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = false;
        }
        return;
      }
    }

    // Fallback to API-based pause
    match self
      .spotify
      .pause_playback(self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        // Update UI immediately instead of full playback poll
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = false;
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn ensure_playback_continues(&mut self, previous_track_id: String) {
    // Let the backend transition to the next item first.
    tokio::time::sleep(Duration::from_millis(250)).await;
    self.get_current_playback().await;

    let (current_track_id, is_playing) = {
      let app = self.app.lock().await;
      let current_track_id = app
        .current_playback_context
        .as_ref()
        .and_then(|ctx| ctx.item.as_ref())
        .and_then(|item| match item {
          PlayableItem::Track(track) => track.id.as_ref().map(|id| id.id().to_string()),
          _ => None,
        });
      let is_playing = app
        .current_playback_context
        .as_ref()
        .map(|ctx| ctx.is_playing)
        .unwrap_or(false);
      (current_track_id, is_playing)
    };

    let Some(current_id) = current_track_id else {
      return;
    };

    let current_uri = format!("spotify:track:{current_id}");
    let is_new_track = previous_track_id != current_id && previous_track_id != current_uri;
    let should_resume = is_new_track && !is_playing;

    if should_resume {
      self.start_playback(None, None, None).await;
      // Refresh state so UI/clients converge quickly.
      self.get_current_playback().await;
    }
  }

  async fn change_volume(&mut self, volume_percent: u8) {
    // Use native streaming player for instant volume change (no network delay)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active() {
      if let Some(ref player) = self.streaming_player {
        player.set_volume(volume_percent);
        // Update UI state immediately
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.device.volume_percent = Some(volume_percent.into());
        }
        // Persist volume setting
        app.user_config.behavior.volume_percent = volume_percent;
        let _ = app.user_config.save_config();
        return;
      }
    }

    // Fallback to API-based volume control
    match self
      .spotify
      .volume(volume_percent, self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.device.volume_percent = Some(volume_percent.into());
        };
        // Persist volume setting
        app.user_config.behavior.volume_percent = volume_percent;
        let _ = app.user_config.save_config();
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn get_artist(
    &mut self,
    artist_id: ArtistId<'_>,
    input_artist_name: String,
    country: Option<Country>,
  ) {
    // Convert Country to Market for rspotify 0.12 API
    let market = country.map(Market::Country);

    // Use artist_albums_manual for explicit pagination control
    let albums = self.spotify.artist_albums_manual(
      artist_id.clone(),
      None,
      market,
      Some(self.large_search_limit),
      Some(0),
    );
    let artist_name = if input_artist_name.is_empty() {
      self
        .spotify
        .artist(artist_id.clone())
        .await
        .map(|full_artist| full_artist.name)
        .unwrap_or_default()
    } else {
      input_artist_name
    };
    let top_tracks = self.spotify.artist_top_tracks(artist_id.clone(), market);

    // Fetch required data (albums and top_tracks)
    match try_join!(albums, top_tracks) {
      Ok((albums, top_tracks)) => {
        // Try to fetch related artists, but don't fail if it's unavailable (deprecated endpoint)
        #[allow(deprecated)]
        let related_artist = self
          .spotify
          .artist_related_artists(artist_id.clone())
          .await
          .unwrap_or_else(|_| Vec::new());

        let mut app = self.app.lock().await;

        app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(
          albums
            .items
            .iter()
            .filter_map(|item| {
              item
                .id
                .as_ref()
                .map(|id| AlbumId::from_id(id.id()).unwrap().into_static())
            })
            .collect(),
        ));

        app.artist = Some(Artist {
          artist_name,
          albums,
          related_artists: related_artist,
          top_tracks,
          selected_album_index: 0,
          selected_related_artist_index: 0,
          selected_top_track_index: 0,
          artist_hovered_block: ArtistBlock::TopTracks,
          artist_selected_block: ArtistBlock::Empty,
        });
        app.push_navigation_stack(RouteId::Artist, ActiveBlock::ArtistBlock);
      }
      Err(e) => {
        eprintln!("DEBUG: Error fetching artist: {:?}", e);
        self
          .handle_error(anyhow!("Failed to fetch artist: {}", e))
          .await;
      }
    }
  }

  async fn get_album_tracks(&mut self, album: Box<SimplifiedAlbum>) {
    if let Some(album_id) = &album.id {
      match self
        .spotify
        .album_track_manual(
          album_id.clone(),
          None,
          Some(self.large_search_limit),
          Some(0),
        )
        .await
      {
        Ok(tracks) => {
          let track_ids = tracks
            .items
            .iter()
            .filter_map(|item| {
              item
                .id
                .as_ref()
                .map(|id| TrackId::from_id(id.id()).unwrap().into_static())
            })
            .collect::<Vec<TrackId<'static>>>();

          let mut app = self.app.lock().await;
          app.selected_album_simplified = Some(SelectedAlbum {
            album: *album,
            tracks,
            selected_index: 0,
          });

          app.album_table_context = AlbumTableContext::Simplified;
          app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
        }
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
        }
      }
    }
  }

  async fn get_recommendations_for_seed(
    &mut self,
    seed_artists: Option<Vec<ArtistId<'static>>>,
    seed_tracks: Option<Vec<TrackId<'static>>>,
    first_track: Box<Option<FullTrack>>,
    country: Option<Country>,
  ) {
    let market = country.map(Market::Country);
    let seed_genres: Option<Vec<&str>> = None;

    match self
      .spotify
      .recommendations(
        [],                            // attributes (empty for now)
        seed_artists,                  // seed_artists
        seed_genres,                   // seed_genres
        seed_tracks,                   // seed_tracks
        market,                        // market
        Some(self.large_search_limit), // limit
      )
      .await
    {
      Ok(result) => {
        if let Some(mut recommended_tracks) = self.extract_recommended_tracks(&result).await {
          //custom first track
          if let Some(track) = *first_track {
            recommended_tracks.insert(0, track);
          }

          let track_ids = recommended_tracks
            .iter()
            .filter_map(|x| {
              x.id
                .as_ref()
                .map(|id| PlayableId::Track(id.clone().into_static()))
            })
            .collect::<Vec<PlayableId>>();

          self.set_tracks_to_table(recommended_tracks.clone()).await;

          let mut app = self.app.lock().await;
          app.recommended_tracks = recommended_tracks;
          app.track_table.context = Some(TrackTableContext::RecommendedTracks);

          if app.get_current_route().id != RouteId::Recommendations {
            app.push_navigation_stack(RouteId::Recommendations, ActiveBlock::TrackTable);
          };

          app.dispatch(IoEvent::StartPlayback(None, Some(track_ids), Some(0)));
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn extract_recommended_tracks(
    &mut self,
    recommendations: &Recommendations,
  ) -> Option<Vec<FullTrack>> {
    let track_ids = recommendations
      .tracks
      .iter()
      .filter_map(|track| track.id.clone())
      .collect::<Vec<TrackId>>();

    if let Ok(result) = self.spotify.tracks(track_ids, None).await {
      return Some(result);
    }

    None
  }

  async fn get_recommendations_for_track_id(
    &mut self,
    track_id: TrackId<'_>,
    country: Option<Country>,
  ) {
    if let Ok(track) = self.spotify.track(track_id.clone(), None).await {
      let track_id_list = vec![track_id.into_static()];
      self
        .get_recommendations_for_seed(None, Some(track_id_list), Box::new(Some(track)), country)
        .await;
    }
  }

  async fn toggle_save_track(&mut self, playable_id: PlayableId<'_>) {
    match playable_id {
      PlayableId::Track(track_id) => {
        match self
          .spotify
          .current_user_saved_tracks_contains([track_id.clone()])
          .await
        {
          Ok(saved) => {
            if saved.first() == Some(&true) {
              match self
                .spotify
                .current_user_saved_tracks_delete([track_id.clone()])
                .await
              {
                Ok(()) => {
                  let mut app = self.app.lock().await;
                  app.liked_song_ids_set.remove(track_id.id());
                }
                Err(e) => {
                  self.handle_error(anyhow!(e)).await;
                }
              }
            } else {
              match self
                .spotify
                .current_user_saved_tracks_add([track_id.clone()])
                .await
              {
                Ok(()) => {
                  let mut app = self.app.lock().await;
                  app.liked_song_ids_set.insert(track_id.id().to_string());
                }
                Err(e) => {
                  self.handle_error(anyhow!(e)).await;
                }
              }
            }
          }
          Err(e) => {
            self.handle_error(anyhow!(e)).await;
          }
        }
      }
      PlayableId::Episode(episode_id) => {
        // To save an episode, you save the show.
        // First, get the episode to find the show ID
        match self.spotify.get_an_episode(episode_id, None).await {
          Ok(episode) => {
            let show_id = episode.show.id;
            match self
              .spotify
              .check_users_saved_shows([show_id.clone()])
              .await
            {
              Ok(saved) => {
                if saved.first() == Some(&true) {
                  match self
                    .spotify
                    .remove_users_saved_shows([show_id.clone()], None)
                    .await
                  {
                    Ok(()) => {
                      let mut app = self.app.lock().await;
                      app.saved_show_ids_set.remove(show_id.id());
                    }
                    Err(e) => {
                      self.handle_error(anyhow!(e)).await;
                    }
                  }
                } else {
                  match self.spotify.save_shows([show_id.clone()]).await {
                    Ok(()) => {
                      let mut app = self.app.lock().await;
                      app.saved_show_ids_set.insert(show_id.id().to_string());
                    }
                    Err(e) => {
                      self.handle_error(anyhow!(e)).await;
                    }
                  }
                }
              }
              Err(e) => {
                self.handle_error(anyhow!(e)).await;
              }
            }
          }
          Err(e) => {
            self.handle_error(anyhow!(e)).await;
          }
        }
      }
    }
  }

  async fn get_followed_artists(&mut self, after: Option<ArtistId<'_>>) {
    // Convert after ID to string for the API call
    let after_str = after.as_ref().map(|id| id.id());

    match self
      .spotify
      .current_user_followed_artists(after_str, Some(self.large_search_limit))
      .await
    {
      Ok(saved_artists) => {
        let mut app = self.app.lock().await;
        app.artists = saved_artists.items.to_owned();
        app.library.saved_artists.add_pages(saved_artists);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn user_artist_check_follow(&mut self, artist_ids: Vec<ArtistId<'_>>) {
    if let Ok(are_followed) = self
      .spotify
      .user_artist_check_follow(artist_ids.clone())
      .await
    {
      let mut app = self.app.lock().await;
      artist_ids
        .iter()
        .zip(are_followed.iter())
        .for_each(|(id, &is_followed)| {
          if is_followed {
            app.followed_artist_ids_set.insert(id.id().to_string());
          } else {
            app.followed_artist_ids_set.remove(id.id());
          }
        });
    }
  }

  async fn get_current_user_saved_albums(&mut self, offset: Option<u32>) {
    match self
      .spotify
      .current_user_saved_albums_manual(None, Some(self.large_search_limit), offset)
      .await
    {
      Ok(saved_albums) => {
        // not to show a blank page
        if !saved_albums.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.saved_albums.add_pages(saved_albums);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn current_user_saved_albums_contains(&mut self, album_ids: Vec<AlbumId<'_>>) {
    if let Ok(are_followed) = self
      .spotify
      .current_user_saved_albums_contains(album_ids.clone())
      .await
    {
      let mut app = self.app.lock().await;
      album_ids
        .iter()
        .zip(are_followed.iter())
        .for_each(|(id, &is_followed)| {
          if is_followed {
            app.saved_album_ids_set.insert(id.id().to_string());
          } else {
            app.saved_album_ids_set.remove(id.id());
          }
        });
    }
  }

  pub async fn current_user_saved_album_delete(&mut self, album_id: AlbumId<'_>) {
    match self
      .spotify
      .current_user_saved_albums_delete([album_id.clone()])
      .await
    {
      Ok(_) => {
        self.get_current_user_saved_albums(None).await;
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.remove(album_id.id());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn current_user_saved_album_add(&mut self, album_id: AlbumId<'_>) {
    match self
      .spotify
      .current_user_saved_albums_add([album_id.clone()])
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.insert(album_id.id().to_string());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_shows_delete(&mut self, show_id: ShowId<'_>) {
    match self
      .spotify
      .remove_users_saved_shows([show_id.clone()], None)
      .await
    {
      Ok(_) => {
        self.get_current_user_saved_shows(None).await;
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.remove(show_id.id());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_shows_add(&mut self, show_id: ShowId<'_>) {
    match self.spotify.save_shows([show_id.clone()]).await {
      Ok(_) => {
        self.get_current_user_saved_shows(None).await;
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.insert(show_id.id().to_string());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_unfollow_artists(&mut self, artist_ids: Vec<ArtistId<'_>>) {
    match self.spotify.user_unfollow_artists(artist_ids.clone()).await {
      Ok(_) => {
        self.get_followed_artists(None).await;
        let mut app = self.app.lock().await;
        artist_ids.iter().for_each(|id| {
          app.followed_artist_ids_set.remove(id.id());
        });
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_follow_artists(&mut self, artist_ids: Vec<ArtistId<'_>>) {
    match self.spotify.user_follow_artists(artist_ids.clone()).await {
      Ok(_) => {
        self.get_followed_artists(None).await;
        let mut app = self.app.lock().await;
        artist_ids.iter().for_each(|id| {
          app.followed_artist_ids_set.insert(id.id().to_string());
        });
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_follow_playlist(
    &mut self,
    _playlist_owner_id: UserId<'_>,
    playlist_id: PlaylistId<'_>,
    is_public: Option<bool>,
  ) {
    match self.spotify.playlist_follow(playlist_id, is_public).await {
      Ok(_) => {
        self.get_current_user_playlists().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_unfollow_playlist(&mut self, _user_id: UserId<'_>, playlist_id: PlaylistId<'_>) {
    match self.spotify.playlist_unfollow(playlist_id).await {
      Ok(_) => {
        self.get_current_user_playlists().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn made_for_you_search_and_add(&mut self, search_string: String, country: Option<Country>) {
    const SPOTIFY_ID: &str = "spotify";
    let market = country.map(Market::Country);

    match self
      .spotify
      .search(
        &search_string,
        SearchType::Playlist,
        market,
        None, // include_external
        Some(self.large_search_limit),
        Some(0),
      )
      .await
    {
      Ok(SearchResult::Playlists(mut search_playlists)) => {
        let mut filtered_playlists = search_playlists
          .items
          .iter()
          .filter(|playlist| {
            playlist.owner.id.to_string() == SPOTIFY_ID && playlist.name == search_string
          })
          .map(|playlist| playlist.to_owned())
          .collect::<Vec<SimplifiedPlaylist>>();

        let mut app = self.app.lock().await;
        if !app.library.made_for_you_playlists.pages.is_empty() {
          app
            .library
            .made_for_you_playlists
            .get_mut_results(None)
            .unwrap()
            .items
            .append(&mut filtered_playlists);
        } else {
          search_playlists.items = filtered_playlists;
          app
            .library
            .made_for_you_playlists
            .add_pages(search_playlists);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
      _ => {}
    }
  }

  async fn get_current_user_playlists(&mut self) {
    let playlists = self
      .spotify
      .current_user_playlists_manual(Some(self.large_search_limit), None)
      .await;

    match playlists {
      Ok(p) => {
        let mut app = self.app.lock().await;
        app.playlists = Some(p);
        // Select the first playlist
        app.selected_playlist_index = Some(0);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn get_recently_played(&mut self) {
    match self
      .spotify
      .current_user_recently_played(Some(self.large_search_limit), None)
      .await
    {
      Ok(result) => {
        let track_ids = result
          .items
          .iter()
          .filter_map(|item| {
            item
              .track
              .id
              .as_ref()
              .map(|id| TrackId::from_id(id.id()).unwrap().into_static())
          })
          .collect::<Vec<TrackId<'static>>>();

        self.current_user_saved_tracks_contains(track_ids).await;

        let mut app = self.app.lock().await;

        app.recently_played.result = Some(result.clone());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_album(&mut self, album_id: AlbumId<'_>) {
    match self.spotify.album(album_id, None).await {
      Ok(album) => {
        let selected_album = SelectedFullAlbum {
          album,
          selected_index: 0,
        };

        let mut app = self.app.lock().await;

        app.selected_album_full = Some(selected_album);
        app.album_table_context = AlbumTableContext::Full;
        app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_album_for_track(&mut self, track_id: TrackId<'_>) {
    match self.spotify.track(track_id, None).await {
      Ok(track) => {
        // It is unclear when the id can ever be None, but perhaps a track can be album-less. If
        // so, there isn't much to do here anyways, since we're looking for the parent album.
        let album_id = match track.album.id {
          Some(id) => id,
          None => return,
        };

        if let Ok(album) = self.spotify.album(album_id, None).await {
          // The way we map to the UI is zero-indexed, but Spotify is 1-indexed.
          let zero_indexed_track_number = track.track_number - 1;
          let selected_album = SelectedFullAlbum {
            album,
            // Overflow should be essentially impossible here, so we prefer the cleaner 'as'.
            selected_index: zero_indexed_track_number as usize,
          };

          let mut app = self.app.lock().await;

          app.selected_album_full = Some(selected_album.clone());
          app.saved_album_tracks_index = selected_album.selected_index;
          app.album_table_context = AlbumTableContext::Full;
          app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn transfert_playback_to_device(&mut self, device_id: String) {
    match self.spotify.transfer_playback(&device_id, Some(true)).await {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    match self.client_config.set_device_id(device_id) {
      Ok(()) => {
        let mut app = self.app.lock().await;
        app.pop_navigation_stack();
      }
      Err(e) => {
        self.handle_error(e).await;
      }
    };
  }

  /// Auto-select a streaming device by name (used for native spotatui streaming)
  /// This will retry a few times since the device may take a moment to appear in Spotify's device list
  async fn auto_select_streaming_device(&mut self, device_name: String) {
    // Retry a few times since the device may not appear immediately
    for attempt in 0..5 {
      if attempt > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
      }

      match self.spotify.device().await {
        Ok(devices) => {
          // Find the device by name (case-insensitive)
          if let Some(device) = devices
            .iter()
            .find(|d| d.name.to_lowercase() == device_name.to_lowercase())
          {
            if let Some(device_id) = &device.id {
              // Transfer playback to this device
              match self.spotify.transfer_playback(device_id, Some(false)).await {
                Ok(()) => {
                  // Save device ID to config
                  let _ = self.client_config.set_device_id(device_id.clone());
                  return;
                }
                Err(_) => {
                  // Transfer failed, will retry
                  continue;
                }
              }
            }
          }
        }
        Err(_) => {
          // Failed to get devices, will retry
          continue;
        }
      }
    }
    // Silently fail after retries - user can still manually select device with 'd'
  }

  async fn refresh_authentication(&mut self) {
    // The new rspotify client handles token refreshing automatically.
    // This function is now a no-op.
  }

  async fn add_item_to_queue(&mut self, item: PlayableId<'_>) {
    match self
      .spotify
      .add_item_to_queue(item, self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => (),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  #[cfg(feature = "telemetry")]
  async fn increment_global_song_count(&self) {
    self.update_global_song_count(reqwest::Method::POST).await;
  }

  #[cfg(feature = "telemetry")]
  async fn fetch_global_song_count(&self) {
    self.update_global_song_count(reqwest::Method::GET).await;
  }

  #[cfg(feature = "telemetry")]
  async fn update_global_song_count(&self, method: reqwest::Method) {
    const TELEMETRY_ENDPOINT: &str = "https://spotatui-counter.spotatui.workers.dev";

    let app = Arc::clone(&self.app);

    // Fire-and-forget to avoid blocking other network events
    tokio::spawn(async move {
      let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
      {
        Ok(client) => client,
        Err(_) => {
          let mut app = app.lock().await;
          app.global_song_count_failed = true;
          return;
        }
      };

      let response = client
        .request(method, TELEMETRY_ENDPOINT)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await;

      let parsed_response = match response {
        Ok(resp) => resp.json::<GlobalSongCountResponse>().await,
        Err(e) => Err(e),
      };

      match parsed_response {
        Ok(data) => {
          let mut app = app.lock().await;
          app.global_song_count = Some(data.count);
          app.global_song_count_failed = false;
        }
        Err(_) => {
          let mut app = app.lock().await;
          app.global_song_count_failed = true;
        }
      }
    });
  }

  #[cfg(not(feature = "telemetry"))]
  async fn increment_global_song_count(&self) {
    // No-op when telemetry feature is disabled
  }

  #[cfg(not(feature = "telemetry"))]
  async fn fetch_global_song_count(&self) {
    // No-op when telemetry feature is disabled
  }

  async fn get_lyrics(&mut self, track_name: String, artist_name: String, duration_sec: f64) {
    use crate::app::LyricsStatus;

    // Set loading state
    {
      let mut app = self.app.lock().await;
      app.lyrics_status = LyricsStatus::Loading;
      app.lyrics = None;
    }

    let client = reqwest::Client::new();
    let params = [
      ("artist_name", artist_name),
      ("track_name", track_name),
      ("duration", duration_sec.to_string()),
    ];

    match client
      .get("https://lrclib.net/api/get")
      .query(&params)
      .send()
      .await
    {
      Ok(resp) => {
        if let Ok(lrc_resp) = resp.json::<LrcResponse>().await {
          if let Some(synced) = lrc_resp.syncedLyrics {
            let parsed = self.parse_lrc(&synced);
            let mut app = self.app.lock().await;
            app.lyrics = Some(parsed);
            app.lyrics_status = LyricsStatus::Found;
          } else if let Some(plain) = lrc_resp.plainLyrics {
            let mut app = self.app.lock().await;
            app.lyrics = Some(vec![(0, plain)]);
            app.lyrics_status = LyricsStatus::Found;
          } else {
            let mut app = self.app.lock().await;
            app.lyrics_status = LyricsStatus::NotFound;
          }
        } else {
          let mut app = self.app.lock().await;
          app.lyrics_status = LyricsStatus::NotFound;
        }
      }
      Err(_) => {
        let mut app = self.app.lock().await;
        app.lyrics_status = LyricsStatus::NotFound;
      }
    }
  }

  fn parse_lrc(&self, lrc: &str) -> Vec<(u128, String)> {
    let mut lyrics = Vec::new();
    for line in lrc.lines() {
      if let Some(idx) = line.find(']') {
        if line.starts_with('[') && idx < line.len() {
          let time_part = &line[1..idx];
          let text_part = line[idx + 1..].trim().to_string();
          if let Some(time) = self.parse_time_ms(time_part) {
            lyrics.push((time, text_part));
          }
        }
      }
    }
    lyrics
  }

  fn parse_time_ms(&self, time_str: &str) -> Option<u128> {
    // mm:ss.xx
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
      return None;
    }
    let min: u128 = parts[0].parse().ok()?;

    let sec_parts: Vec<&str> = parts[1].split('.').collect();
    if sec_parts.len() != 2 {
      return None;
    }

    let sec: u128 = sec_parts[0].parse().ok()?;
    let ms_part = sec_parts[1];
    let ms: u128 = ms_part.parse().ok()?;

    // Detect if ms is 2 digits (centiseconds) or 3 digits
    let ms_val = if ms_part.len() == 2 { ms * 10 } else { ms };

    Some(min * 60000 + sec * 1000 + ms_val)
  }
}
