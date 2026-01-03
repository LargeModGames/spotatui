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
    playlist::PlaylistItem,
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
#[cfg(feature = "streaming")]
use librespot_connect::{LoadRequest, LoadRequestOptions, PlayingTrack};

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
  /// Get user's top tracks for Discover feature (with time range)
  GetUserTopTracks(crate::app::DiscoverTimeRange),
  /// Get Top Artists Mix - fetches top artists and their top tracks
  GetTopArtistsMix,
  /// Fetch all playlist tracks and apply sorting
  FetchAllPlaylistTracksAndSort(PlaylistId<'static>),
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

    // First, check if the current playback device matches the native streaming device
    {
      let app = self.app.lock().await;
      if let Some(ref ctx) = app.current_playback_context {
        if let (Some(current_id), Some(native_id)) =
          (ctx.device.id.as_ref(), app.native_device_id.as_ref())
        {
          if current_id == native_id {
            return true;
          }
        }
      }
    }

    // Fallback: strict name match (case-insensitive).
    let native_device_name = self
      .streaming_player
      .as_ref()
      .map(|p| p.device_name().to_lowercase());
    {
      let app = self.app.lock().await;
      if let (Some(ref ctx), Some(native_name)) =
        (&app.current_playback_context, native_device_name.as_ref())
      {
        let current_device_name = ctx.device.name.to_lowercase();
        if current_device_name == native_name.as_str() {
          return true;
        }
      }
    }

    // If no context yet (e.g., at startup), fall back to the app state flag which is
    // set when the native streaming device is activated/selected.
    let app = self.app.lock().await;
    app.is_streaming_active
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
      IoEvent::GetUserTopTracks(time_range) => {
        self.get_user_top_tracks(time_range).await;
      }
      IoEvent::GetTopArtistsMix => {
        self.get_top_artists_mix().await;
      }
      IoEvent::FetchAllPlaylistTracksAndSort(playlist_id) => {
        self.fetch_all_playlist_tracks_and_sort(playlist_id).await;
      }
    };

    {
      let mut app = self.app.lock().await;
      app.is_loading = false;
    }
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
    let local_state: Option<(Option<u8>, bool, rspotify::model::RepeatState, Option<bool>)> =
      if self.is_native_streaming_active() {
        let app = self.app.lock().await;
        if let Some(ref ctx) = app.current_playback_context {
          let volume = self.streaming_player.as_ref().map(|p| p.get_volume());
          Some((
            volume,
            ctx.shuffle_state,
            ctx.repeat_state,
            app.native_is_playing,
          ))
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

        // Detect whether the native spotatui streaming device is the active Spotify device.
        // This prevents native-only state overrides (volume, play/pause, etc.) from leaking
        // into external devices like the Spotify desktop/mobile app.
        #[cfg(feature = "streaming")]
        let is_native_device = self.streaming_player.as_ref().is_some_and(|p| {
          if let (Some(current_id), Some(native_id)) =
            (c.device.id.as_ref(), app.native_device_id.as_ref())
          {
            return current_id == native_id;
          }
          let native_name = p.device_name().to_lowercase();
          c.device.name.to_lowercase() == native_name
        });

        #[cfg(feature = "streaming")]
        if is_native_device && app.native_device_id.is_none() {
          if let Some(id) = c.device.id.clone() {
            app.native_device_id = Some(id);
          }
        }

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
        #[cfg(feature = "streaming")]
        if is_native_device {
          if let Some((volume, shuffle, repeat, native_is_playing)) = local_state {
            if let Some(vol) = volume {
              c.device.volume_percent = Some(vol.into());
            }
            c.shuffle_state = shuffle;
            c.repeat_state = repeat;
            // Preserve play/pause state from native player events when available.
            if let Some(is_playing) = native_is_playing {
              c.is_playing = is_playing;
            }
          }
        }

        // On first load with native streaming AND native device is active,
        // override API shuffle with saved preference.
        // Skip this if using external device like spotifyd
        #[cfg(feature = "streaming")]
        if local_state.is_none() && is_native_device {
          c.shuffle_state = app.user_config.behavior.shuffle_enabled;
          // Proactively set native shuffle on first load to keep backend in sync
          if let Some(ref player) = self.streaming_player {
            let _ = player.set_shuffle(app.user_config.behavior.shuffle_enabled);
          }
        }

        app.current_playback_context = Some(c);

        // Update is_streaming_active based on whether the current device matches native streaming
        // This ensures correct polling interval (1s for external devices, 5s for native)
        #[cfg(feature = "streaming")]
        {
          app.is_streaming_active = is_native_device;
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
    // Check if we should use native streaming for playback
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback().await {
      if let Some(ref player) = self.streaming_player {
        // Ensure the native player is actually the active Spotify Connect device.
        // `activate()` can be a no-op when we're not active, so prefer `transfer()`.
        let _ = player.transfer(None);
        // Give Connect a moment to apply the transfer before we load/play.
        tokio::time::sleep(Duration::from_millis(200)).await;
        player.activate();

        // For resume playback (no context, no uris)
        if context_id.is_none() && uris.is_none() {
          player.play();
          // Update UI state immediately
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
          }
          return;
        }

        // For URI-based or context playback, use Spirc load directly.
        // The Web API may return 404 for native streaming devices (different OAuth session).
        let mut options = LoadRequestOptions {
          start_playing: true,
          seek_to: 0,
          context_options: None,
          playing_track: None,
        };

        let request = match (context_id, uris) {
          (Some(context), Some(track_uris)) => {
            if let Some(first_uri) = track_uris.first() {
              options.playing_track = Some(PlayingTrack::Uri(first_uri.uri()));
            } else if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
              options.playing_track = Some(PlayingTrack::Index(i));
            }
            LoadRequest::from_context_uri(context.uri(), options)
          }
          (Some(context), None) => {
            if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
              options.playing_track = Some(PlayingTrack::Index(i));
            }
            LoadRequest::from_context_uri(context.uri(), options)
          }
          (None, Some(track_uris)) => {
            if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
              options.playing_track = Some(PlayingTrack::Index(i));
            }
            let uris = track_uris.into_iter().map(|u| u.uri()).collect::<Vec<_>>();
            LoadRequest::from_tracks(uris, options)
          }
          (None, None) => {
            // Handled above as resume playback.
            player.play();
            let mut app = self.app.lock().await;
            if let Some(ctx) = &mut app.current_playback_context {
              ctx.is_playing = true;
            }
            return;
          }
        };

        match player.load(request) {
          Ok(()) => {
            // Best-effort: ensure we end up playing (some Connect flows load paused).
            player.play();

            // Update UI state immediately (TrackChanged/Playing events will refine state).
            let mut app = self.app.lock().await;
            app.song_progress_ms = 0;
            app.native_is_playing = Some(true);
            if let Some(ctx) = &mut app.current_playback_context {
              ctx.is_playing = true;
            }
          }
          Err(e) => {
            self.handle_error(e).await;
          }
        }

        return;
      }
    }

    // If Spotify reports a current playback context, target the active device by omitting
    // `device_id` (more robust when the user switches devices outside spotatui).
    // When there's no active playback context, fall back to the last saved device_id.
    let has_playback_context = {
      let app = self.app.lock().await;
      app.current_playback_context.is_some()
    };
    let device_id = if has_playback_context {
      None
    } else {
      self.client_config.device_id.as_deref()
    };

    // If we're starting playback at a specific position within a context (via a numeric offset),
    // temporarily disable shuffle to ensure the selected item plays first.
    // (When we start by explicit track URI, we don't need to touch shuffle.)
    let should_disable_shuffle = context_id.is_some() && uris.is_none() && offset.is_some();
    let mut original_shuffle_state = false;

    #[cfg(feature = "streaming")]
    let is_native = self.is_native_streaming_active_for_playback().await;
    #[cfg(not(feature = "streaming"))]
    let is_native = false;

    if should_disable_shuffle && !is_native {
      // Get current shuffle state (skip for native - API might fail)
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
        let offset = rspotify::model::Offset::Uri(first_uri.uri());
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
      // Play from context, optionally starting at an offset within the context.
      let offset = offset
        .and_then(|i| i64::try_from(i).ok())
        .map(|i| rspotify::model::Offset::Position(chrono::Duration::milliseconds(i)));
      self
        .spotify
        .start_context_playback(context_id, device_id, offset, None)
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
      if self.is_native_streaming_active_for_playback().await {
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
        if should_disable_shuffle && original_shuffle_state && !is_native {
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
        // For native streaming, if the API fails (404), try using native player directly
        #[cfg(feature = "streaming")]
        if is_native {
          if let Some(ref player) = self.streaming_player {
            // Activate and play - the Spirc will handle the actual playback
            player.activate();
            player.play();

            // Update UI state
            {
              let mut app = self.app.lock().await;
              app.song_progress_ms = 0;
              if let Some(ctx) = &mut app.current_playback_context {
                ctx.is_playing = true;
              }
            }

            // Small delay then fetch updated state
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            self.get_current_playback().await;
            return;
          }
        }

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
          {
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
          tokio::task::yield_now().await;
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
          {
            let mut app = app.lock().await;
            // Extend playlist tracks items
            if let Some(ref mut existing) = app.playlist_tracks {
              existing.items.extend(playlist_page.items);
              existing.total = playlist_page.total; // Update total in case it changed
            }
          }
          tokio::task::yield_now().await;
        }
        Err(_e) => {
          break;
        }
      }
      offset += large_search_limit;
    }
  }

  /// Fetch all tracks from a playlist and apply sorting
  async fn fetch_all_playlist_tracks_and_sort(&mut self, playlist_id: PlaylistId<'_>) {
    use rspotify::model::PlayableItem;

    // Get current playlist total and sort state
    let (total, sort_state) = {
      let app = self.app.lock().await;
      let total = app.playlist_tracks.as_ref().map(|p| p.total).unwrap_or(0);
      let sort_state = app.playlist_sort;
      (total, sort_state)
    };

    if total == 0 {
      return;
    }

    // Collect all playlist items
    let mut all_items: Vec<rspotify::model::playlist::PlaylistItem> = Vec::new();
    let mut offset = 0;

    while offset < total {
      match self
        .spotify
        .playlist_items_manual(
          playlist_id.clone(),
          None,
          None,
          Some(self.large_search_limit),
          Some(offset),
        )
        .await
      {
        Ok(page) => {
          all_items.extend(page.items);
        }
        Err(_e) => {
          break;
        }
      }
      offset += self.large_search_limit;
    }

    // Sort all items
    all_items.sort_by(|a, b| {
      let track_a = a.track.as_ref().and_then(|t| match t {
        PlayableItem::Track(track) => Some(track),
        PlayableItem::Episode(_) => None,
      });
      let track_b = b.track.as_ref().and_then(|t| match t {
        PlayableItem::Track(track) => Some(track),
        PlayableItem::Episode(_) => None,
      });

      let cmp = match (track_a, track_b) {
        (Some(ta), Some(tb)) => match sort_state.field {
          crate::sort::SortField::Default => std::cmp::Ordering::Equal,
          crate::sort::SortField::Name => ta.name.to_lowercase().cmp(&tb.name.to_lowercase()),
          crate::sort::SortField::Artist => {
            let artist_a = ta
              .artists
              .first()
              .map(|ar| ar.name.to_lowercase())
              .unwrap_or_default();
            let artist_b = tb
              .artists
              .first()
              .map(|ar| ar.name.to_lowercase())
              .unwrap_or_default();
            artist_a.cmp(&artist_b)
          }
          crate::sort::SortField::Album => ta
            .album
            .name
            .to_lowercase()
            .cmp(&tb.album.name.to_lowercase()),
          crate::sort::SortField::Duration => ta.duration.cmp(&tb.duration),
          crate::sort::SortField::DateAdded => a.added_at.cmp(&b.added_at),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
      };

      if sort_state.order == crate::sort::SortOrder::Descending {
        cmp.reverse()
      } else {
        cmp
      }
    });

    // Extract tracks and update app state
    let sorted_tracks: Vec<rspotify::model::FullTrack> = all_items
      .iter()
      .filter_map(|item| item.track.as_ref())
      .filter_map(|track| match track {
        PlayableItem::Track(full_track) => Some(full_track.clone()),
        PlayableItem::Episode(_) => None,
      })
      .collect();

    // Update app state with sorted data
    let mut app = self.app.lock().await;

    // Update playlist_tracks with sorted items
    if let Some(ref mut playlist_tracks) = app.playlist_tracks {
      playlist_tracks.items = all_items;
      playlist_tracks.total = total;
    }

    // Update track table with sorted tracks
    app.track_table.tracks = sorted_tracks;
    app.track_table.selected_index = 0;
  }

  async fn seek(&mut self, position_ms: u32) {
    // Use native streaming player for instant seek (no network delay)
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback().await {
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
    let position = TimeDelta::milliseconds(position_ms as i64);

    // Don't pin commands to a saved device_id; target Spotify's currently active device.
    match self.spotify.seek_track(position, None).await {
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

    // Store if playing before skip for auto-resume
    let was_playing = {
      let mut app = self.app.lock().await;
      // Reset progress immediately for instant UI feedback
      app.song_progress_ms = 0;
      app
        .current_playback_context
        .as_ref()
        .map(|c| c.is_playing)
        .unwrap_or(false)
    };

    // API-based skip for external players (spotifyd, etc.)
    match self.spotify.next_track(None).await {
      Ok(()) => {
        // Small delay to let the external player process the skip
        tokio::time::sleep(Duration::from_millis(200)).await;

        // For external players, proactively resume if we were playing
        // Spotifyd often lands in paused state after skip
        if was_playing {
          let _ = self.spotify.resume_playback(None, None).await;
        }

        // Small delay then fetch updated state
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn previous_track(&mut self) {
    // Use native streaming player for instant skip (no network delay)
    // BUT only if native streaming device is the active playback device
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback().await {
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

    // Store if playing before skip for auto-resume
    let was_playing = {
      let mut app = self.app.lock().await;
      // Reset progress immediately for instant UI feedback
      app.song_progress_ms = 0;
      app
        .current_playback_context
        .as_ref()
        .map(|c| c.is_playing)
        .unwrap_or(false)
    };

    // API-based skip for external players (spotifyd, etc.)
    match self.spotify.previous_track(None).await {
      Ok(()) => {
        // Small delay to let the external player process the skip
        tokio::time::sleep(Duration::from_millis(200)).await;

        // For external players, proactively resume if we were playing
        // Spotifyd often lands in paused state after skip
        if was_playing {
          let _ = self.spotify.resume_playback(None, None).await;
        }

        // Small delay then fetch updated state
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn shuffle(&mut self, desired_shuffle_state: bool) {
    let new_shuffle_state = desired_shuffle_state;
    let is_startup_sync = {
      let app = self.app.lock().await;
      app.current_playback_context.is_none()
    };

    // Prefer native streaming control when available AND active as playback device
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback().await {
      if let Some(ref player) = self.streaming_player {
        // Try to set shuffle on the native player
        let shuffle_result = player.set_shuffle(new_shuffle_state);

        // Update UI and save config regardless of Spirc result
        // (Spirc might not be fully ready yet but we still want to save the preference)
        {
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.shuffle_state = new_shuffle_state;
          }
          app.user_config.behavior.shuffle_enabled = new_shuffle_state;
          let _ = app.user_config.save_config();
        }

        // Log error but don't show error screen - suppress to avoid startup popups
        if let Err(_e) = shuffle_result {
          // Silently ignore - Spirc might not be ready yet
          // The shuffle state will be applied when playback starts
        }

        // Don't fall back to API - it will fail with 404 for native devices
        return;
      }
    }

    // Fallback: API-based shuffle for external devices (spotifyd, phone, etc.)
    // Don't pin commands to a saved device_id; target Spotify's currently active device.
    match self.spotify.shuffle(new_shuffle_state, None).await {
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
        // On startup we try to apply the saved shuffle preference before there is any active
        // playback device. Spotify returns a 404 in this case; don't show the error screen.
        if is_startup_sync {
          if let rspotify::ClientError::Http(http) = &e {
            if let rspotify::http::HttpError::StatusCode(response) = http.as_ref() {
              if response.status().as_u16() == 404 {
                let mut app = self.app.lock().await;
                app.user_config.behavior.shuffle_enabled = new_shuffle_state;
                let _ = app.user_config.save_config();
                return;
              }
            }
          }
        }
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

    // When using native streaming, update UI immediately and send command to player
    // The Web API returns 404 for native streaming devices
    #[cfg(feature = "streaming")]
    if self.is_native_streaming_active_for_playback().await {
      if let Some(ref player) = self.streaming_player {
        // Send the repeat command to the native player (current state, not next state)
        if let Err(e) = player.set_repeat(repeat_state) {
          self.handle_error(anyhow!(e)).await;
        }
      }
      // Update UI after sending command
      let mut app = self.app.lock().await;
      if let Some(ctx) = &mut app.current_playback_context {
        ctx.repeat_state = next_repeat_state;
      }
      return;
    }

    // Fallback: API-based repeat for external devices (updates UI after API call succeeds)
    // Don't pin commands to a saved device_id; target Spotify's currently active device.
    match self.spotify.repeat(next_repeat_state, None).await {
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
    if self.is_native_streaming_active_for_playback().await {
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
    // Don't pin commands to a saved device_id; target Spotify's currently active device.
    match self.spotify.pause_playback(None).await {
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
    if self.is_native_streaming_active_for_playback().await {
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
    // Don't pin commands to a saved device_id; target Spotify's currently active device.
    match self.spotify.volume(volume_percent, None).await {
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
                  // Trigger the "Like" animation
                  app.liked_song_animation_frame = Some(10);
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
    // Check if we're selecting the native streaming device
    // Native streaming uses Spirc (Spotify Connect) which doesn't work with the Web API's transfer_playback
    #[cfg(feature = "streaming")]
    {
      let is_native_device = if let Some(ref player) = self.streaming_player {
        // Get the device name from the streaming player
        let native_name = player.device_name().to_lowercase();
        // Check if any device with this device_id matches the native device name
        if let Ok(devices) = self.spotify.device().await {
          devices
            .iter()
            .any(|d| d.id.as_ref() == Some(&device_id) && d.name.to_lowercase() == native_name)
        } else {
          false
        }
      } else {
        false
      };

      if is_native_device {
        // For native streaming device, use Spirc activation instead of Web API
        // The Web API returns 404 for native streaming devices because they use
        // a different OAuth session (librespot-oauth)
        if let Some(ref player) = self.streaming_player {
          let _ = player.transfer(None);
          player.activate();

          // Mark streaming as active
          {
            let mut app = self.app.lock().await;
            app.is_streaming_active = true;
            app.native_device_id = Some(device_id.clone());
          }

          // Refresh playback state after a brief delay to let Connect register
          tokio::time::sleep(std::time::Duration::from_millis(500)).await;
          self.get_current_playback().await;

          // Save device ID and pop navigation
          match self.client_config.set_device_id(device_id) {
            Ok(()) => {
              let mut app = self.app.lock().await;
              app.pop_navigation_stack();
            }
            Err(e) => {
              self.handle_error(e).await;
            }
          };
          return;
        }
      }
    }

    // Standard path for external devices (spotifyd, phone, etc.)
    match self.spotify.transfer_playback(&device_id, Some(true)).await {
      Ok(()) => {
        // We're explicitly selecting an external device, so immediately switch the UI/state
        // out of native streaming mode (the next playback poll will confirm).
        #[cfg(feature = "streaming")]
        {
          let mut app = self.app.lock().await;
          app.is_streaming_active = false;
          app.native_device_id = None;
        }
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
    // For native streaming, we use Spirc activation directly instead of the Web API's transfer_playback
    // The Web API returns 404 for native streaming devices because they use
    // a different OAuth session (librespot-oauth)

    // Wait a moment for the streaming player to fully register with Spotify Connect
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Activate the native streaming device via Spirc
    if let Some(ref player) = self.streaming_player {
      let _ = player.transfer(None);
      player.activate();

      // Mark streaming as active
      {
        let mut app = self.app.lock().await;
        app.is_streaming_active = true;
      }

      // Now try to get the device_id from Spotify's device list and save it
      // Retry a few times since the device may take a moment to appear
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
                // Save device ID to config (don't use transfer_playback - just save the ID)
                let _ = self.client_config.set_device_id(device_id.clone());
                let mut app = self.app.lock().await;
                app.native_device_id = Some(device_id.clone());
                return;
              }
            }
          }
          Err(_) => {
            // Failed to get devices, will retry
            continue;
          }
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
    match self.spotify.add_item_to_queue(item, None).await {
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

  /// Fetch user's top tracks for the Discover feature
  async fn get_user_top_tracks(&mut self, time_range: crate::app::DiscoverTimeRange) {
    use crate::app::DiscoverTimeRange;
    use futures::stream::StreamExt;
    use rspotify::model::TimeRange;

    let spotify_time_range = match time_range {
      DiscoverTimeRange::Short => TimeRange::ShortTerm,
      DiscoverTimeRange::Medium => TimeRange::MediumTerm,
      DiscoverTimeRange::Long => TimeRange::LongTerm,
    };

    {
      let mut app = self.app.lock().await;
      app.discover_loading = true;
    }

    let spotify = self.spotify.clone();
    let mut stream = spotify.current_user_top_tracks(Some(spotify_time_range));
    let mut tracks = vec![];

    while let Some(item) = stream.next().await {
      match item {
        Ok(track) => tracks.push(track),
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          break;
        }
      }
      if tracks.len() >= 50 {
        break;
      }
    }

    let mut app = self.app.lock().await;
    app.discover_top_tracks = tracks.clone();
    app.discover_loading = false;

    // Automatically switch to track table to show results
    app.track_table.tracks = tracks;
    app.track_table.context = Some(crate::app::TrackTableContext::DiscoverPlaylist);
    app.track_table.selected_index = 0;
    app.push_navigation_stack(
      crate::app::RouteId::TrackTable,
      crate::app::ActiveBlock::TrackTable,
    );
  }

  /// Fetch Top Artists Mix - fetches top artists and then their top tracks to create a mix
  async fn get_top_artists_mix(&mut self) {
    use futures::stream::StreamExt;
    use rand::seq::SliceRandom;
    use rspotify::model::TimeRange;

    {
      let mut app = self.app.lock().await;
      app.discover_loading = true;
    }

    let spotify = self.spotify.clone();
    let mut artists_stream = spotify.current_user_top_artists(Some(TimeRange::MediumTerm));
    let mut artists = vec![];

    while let Some(item) = artists_stream.next().await {
      match item {
        Ok(artist) => artists.push(artist),
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
          break;
        }
      }
      if artists.len() >= 10 {
        break;
      }
    }

    let mut all_tracks = vec![];
    let user_country = {
      let app = self.app.lock().await;
      app.get_user_country()
    };
    let market = user_country.map(Market::Country);

    for artist in artists {
      if let Ok(top_tracks) = self
        .spotify
        .artist_top_tracks(artist.id.clone(), market)
        .await
      {
        // Take a few top tracks from each artist
        all_tracks.extend(top_tracks.into_iter().take(3));
      }
    }

    // Shuffle the mix
    {
      let mut rng = rand::thread_rng();
      all_tracks.shuffle(&mut rng);
    }

    let mut app = self.app.lock().await;
    app.discover_artists_mix = all_tracks.clone();
    app.discover_loading = false;

    // Automatically switch to track table to show results
    app.track_table.tracks = all_tracks;
    app.track_table.context = Some(crate::app::TrackTableContext::DiscoverPlaylist);
    app.track_table.selected_index = 0;
    app.push_navigation_stack(
      crate::app::RouteId::TrackTable,
      crate::app::ActiveBlock::TrackTable,
    );
  }
}
