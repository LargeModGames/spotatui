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

pub enum IoEvent {
  GetCurrentPlayback,
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
  Shuffle(bool),
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
  GetLyrics(String, String, f64),
}

#[derive(Clone)]
pub struct Network {
  pub spotify: AuthCodeSpotify,
  large_search_limit: u32,
  small_search_limit: u32,
  pub client_config: ClientConfig,
  pub app: Arc<Mutex<App>>,
}

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct LrcResponse {
  syncedLyrics: Option<String>,
  plainLyrics: Option<String>,
}

impl Network {
  pub fn new(spotify: AuthCodeSpotify, client_config: ClientConfig, app: &Arc<Mutex<App>>) -> Self {
    Network {
      spotify,
      large_search_limit: 20,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
    }
  }

  #[allow(clippy::cognitive_complexity)]
  pub async fn handle_network_event(&mut self, io_event: IoEvent) {
    match io_event {
      IoEvent::RefreshAuthentication => {
        self.refresh_authentication().await;
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
      IoEvent::GetLyrics(track, artist, duration) => {
        self.get_lyrics(track, artist, duration).await;
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
    let context = self
      .spotify
      .current_playback(
        None,
        Some(&[AdditionalType::Episode, AdditionalType::Track]),
      )
      .await;

    match context {
      Ok(Some(c)) => {
        let mut app = self.app.lock().await;
        app.current_playback_context = Some(c.clone());
        app.instant_since_last_current_playback_poll = Instant::now();

        if let Some(item) = c.item {
          match item {
            PlayableItem::Track(track) => {
              if let Some(track_id) = track.id {
                let track_id_str = track_id.id().to_string();

                // Check if this is a new track and telemetry is enabled
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
                app.dispatch(IoEvent::CurrentUserSavedTracksContains(vec![
                  track_id.into_static()
                ]));
              };
            }
            PlayableItem::Episode(_episode) => { /*should map this to following the podcast show*/ }
          }
        };
      }
      Ok(None) => {
        let mut app = self.app.lock().await;
        app.instant_since_last_current_playback_poll = Instant::now();
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }

    let mut app = self.app.lock().await;
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
    let mut app = self.app.lock().await;
    app.track_table.tracks = tracks.clone();

    // Clamp selected_index to valid range after loading new tracks
    if !app.track_table.tracks.is_empty() {
      let max_index = app.track_table.tracks.len().saturating_sub(1);
      if app.track_table.selected_index > max_index {
        app.track_table.selected_index = max_index;
      }
    } else {
      app.track_table.selected_index = 0;
    }

    // Send this event round with typed TrackId (don't block here)
    let track_ids: Vec<TrackId<'static>> = tracks
      .into_iter()
      .filter_map(|item| item.id.map(|id| id.into_static()))
      .collect();

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
      self.spotify.resume_playback(device_id, None).await
    };

    match result {
      Ok(()) => {
        // Re-enable shuffle if it was on before
        if should_disable_shuffle && original_shuffle_state {
          // Small delay to let playback start before re-enabling shuffle
          tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
          let _ = self.spotify.shuffle(true, device_id).await;
        }

        let mut app = self.app.lock().await;
        app.song_progress_ms = 0;
        app.dispatch(IoEvent::GetCurrentPlayback);
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

  async fn seek(&mut self, position_ms: u32) {
    let device_id = self.client_config.device_id.as_deref();
    // rspotify 0.12 uses chrono::TimeDelta for seek_track
    let position = TimeDelta::milliseconds(position_ms as i64);

    match self.spotify.seek_track(position, device_id).await {
      Ok(()) => {
        // Wait between seek and status query.
        // Without it, the Spotify API may return the old progress.
        tokio::time::sleep(Duration::from_millis(1000)).await;
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn next_track(&mut self) {
    match self
      .spotify
      .next_track(self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn previous_track(&mut self) {
    match self
      .spotify
      .previous_track(self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn shuffle(&mut self, shuffle_state: bool) {
    match self
      .spotify
      .shuffle(!shuffle_state, self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        // Update the UI eagerly (otherwise the UI will wait until the next 5 second interval
        // due to polling playback context)
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.shuffle_state = !shuffle_state;
        };
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
    match self
      .spotify
      .pause_playback(self.client_config.device_id.as_deref())
      .await
    {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn change_volume(&mut self, volume_percent: u8) {
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
    #[allow(deprecated)]
    let related_artist = self.spotify.artist_related_artists(artist_id);

    if let Ok((albums, top_tracks, related_artist)) = try_join!(albums, top_tracks, related_artist)
    {
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
    const TELEMETRY_ENDPOINT: &str = "https://spotatui-counter.spotatui.workers.dev";

    // Fire-and-forget telemetry request (no PII, anonymous increment)
    tokio::spawn(async {
      if let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
      {
        let _ = client.post(TELEMETRY_ENDPOINT).send().await;
      }
    });
  }

  #[cfg(not(feature = "telemetry"))]
  async fn increment_global_song_count(&self) {
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
