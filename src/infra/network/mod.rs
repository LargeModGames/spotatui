pub mod library;
pub mod metadata;
pub mod playback;
pub mod recommend;
pub mod requests;
pub mod search;
pub mod sync;
pub mod user;
pub mod utils;

use crate::core::app::App;
use crate::core::config::ClientConfig;
use anyhow::anyhow;
use rspotify::clients::BaseClient;
use rspotify::model::{
  album::SimplifiedAlbum,
  artist::FullArtist,
  enums::{Country, RepeatState},
  idtypes::{
    AlbumId, ArtistId, EpisodeId, PlayContextId, PlayableId, PlaylistId, ShowId, TrackId, UserId,
  },
  show::SimplifiedShow,
  track::FullTrack,
};
use rspotify::prelude::Id;
use rspotify::AuthCodePkceSpotify;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// Re-export traits
use self::library::LibraryNetwork;
use self::metadata::MetadataNetwork;
use self::playback::PlaybackNetwork;
use self::recommend::RecommendationNetwork;
use self::search::SearchNetwork;
use self::user::UserNetwork;
use self::utils::UtilsNetwork;

pub enum IoEvent {
  GetCurrentPlayback,
  /// After a track transition (e.g., EndOfTrack), ensure we don't end up paused on the next item.
  /// The payload is the previous track identifier (either base62 id or a `spotify:track:` URI).
  #[allow(dead_code)]
  EnsurePlaybackContinues(String),
  RefreshAuthentication,
  GetPlaylists,
  GetDevices,
  GetSearchResults(String, Option<Country>),
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
  ForcePreviousTrack,
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
  AddTrackToPlaylist(PlaylistId<'static>, TrackId<'static>),
  RemoveTrackFromPlaylistAtPosition(PlaylistId<'static>, TrackId<'static>, usize),
  GetUser,
  ToggleSaveTrack(PlayableId<'static>),
  GetRecommendationsForTrackId(TrackId<'static>, Option<Country>),
  GetRecentlyPlayed,
  GetFollowedArtists(Option<ArtistId<'static>>),
  SetArtistsToTable(Vec<FullArtist>),
  UserArtistFollowCheck(Vec<ArtistId<'static>>),
  GetAlbum(AlbumId<'static>),
  TransferPlaybackToDevice(String, bool),
  #[allow(dead_code)]
  AutoSelectStreamingDevice(String, bool), // Auto-select a device by name (used for native streaming)
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
  GetQueue,
  IncrementGlobalSongCount,
  FetchGlobalSongCount,
  FetchAnnouncements,
  GetLyrics(String, String, f64),
  /// Pre-fetch the next saved tracks page in background for smoother page transitions
  PreFetchSavedTracksPage {
    offset: u32,
    generation: u64,
  },
  /// Pre-fetch the next playlist page in background for smoother page transitions
  PreFetchPlaylistTracksPage {
    playlist_id: PlaylistId<'static>,
    offset: u32,
    generation: u64,
  },
  /// Get user's top tracks for Discover feature (with time range)
  GetUserTopTracks(crate::core::app::DiscoverTimeRange),
  /// Get Top Artists Mix - fetches top artists and their top tracks
  GetTopArtistsMix,
  /// Fetch all playlist tracks and apply sorting
  FetchAllPlaylistTracksAndSort(PlaylistId<'static>),
  /// Start hosting a listening party
  StartParty(sync::ControlMode),
  /// Join an existing listening party by code
  JoinParty {
    code: String,
    name: String,
  },
  /// Update the host control mode in the relay
  SetPartyControlMode(sync::ControlMode),
  /// Leave the current listening party
  LeaveParty,
  /// Broadcast current playback state to party guests (host only)
  SyncPlayback,
  /// Send a playback command to the party host (guest only, Phase 2)
  #[allow(dead_code)]
  PartyPlaybackCommand(sync::PlaybackAction),
  /// Search tracks to add to a new playlist
  SearchTracksForPlaylist(String),
  /// Create a new playlist with the given name and track IDs
  CreateNewPlaylist(String, Vec<TrackId<'static>>),
}

pub struct Network {
  pub spotify: AuthCodePkceSpotify,
  pub large_search_limit: u32,
  pub small_search_limit: u32,
  pub client_config: ClientConfig,
  pub app: Arc<Mutex<App>>,
  pub party_connection: Option<sync::PartyConnection>,
  pub party_incoming_rx: Option<tokio::sync::mpsc::UnboundedReceiver<sync::SyncMessage>>,
}

impl Network {
  #[cfg(feature = "streaming")]
  pub fn new(
    spotify: AuthCodePkceSpotify,
    client_config: ClientConfig,
    app: &Arc<Mutex<App>>,
  ) -> Self {
    Network {
      spotify,
      large_search_limit: 50,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
      party_connection: None,
      party_incoming_rx: None,
    }
  }

  #[cfg(not(feature = "streaming"))]
  pub fn new(
    spotify: AuthCodePkceSpotify,
    client_config: ClientConfig,
    app: &Arc<Mutex<App>>,
  ) -> Self {
    Network {
      spotify,
      large_search_limit: 50,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
      party_connection: None,
      party_incoming_rx: None,
    }
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
      IoEvent::ForcePreviousTrack => {
        self.force_previous_track().await;
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
      IoEvent::AddTrackToPlaylist(playlist_id, track_id) => {
        self.add_track_to_playlist(playlist_id, track_id).await;
      }
      IoEvent::RemoveTrackFromPlaylistAtPosition(playlist_id, track_id, position) => {
        self
          .remove_track_from_playlist_at_position(playlist_id, track_id, position)
          .await;
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
      IoEvent::TransferPlaybackToDevice(device_id, persist_device_id) => {
        self
          .transfert_playback_to_device(device_id, persist_device_id)
          .await;
      }
      #[cfg(feature = "streaming")]
      IoEvent::AutoSelectStreamingDevice(device_name, persist_device_id) => {
        self
          .auto_select_streaming_device(device_name, persist_device_id)
          .await;
      }
      #[cfg(not(feature = "streaming"))]
      IoEvent::AutoSelectStreamingDevice(..) => {} // No-op without native streaming
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
      IoEvent::GetQueue => {
        self.get_queue().await;
      }
      IoEvent::IncrementGlobalSongCount => {
        self.increment_global_song_count().await;
      }
      IoEvent::FetchGlobalSongCount => {
        self.fetch_global_song_count().await;
      }
      IoEvent::FetchAnnouncements => {
        self.fetch_announcements().await;
      }
      IoEvent::GetLyrics(track, artist, duration) => {
        self.get_lyrics(track, artist, duration).await;
      }
      IoEvent::PreFetchSavedTracksPage { offset, generation } => {
        self.spawn_saved_tracks_prefetch(offset, generation);
      }
      IoEvent::PreFetchPlaylistTracksPage {
        playlist_id,
        offset,
        generation,
      } => {
        self.spawn_playlist_tracks_prefetch(playlist_id, offset, generation);
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
      IoEvent::StartParty(control_mode) => {
        self.start_party(control_mode).await;
      }
      IoEvent::JoinParty { code, name } => {
        self.join_party(code, name).await;
      }
      IoEvent::SetPartyControlMode(control_mode) => {
        self.set_party_control_mode(control_mode).await;
      }
      IoEvent::LeaveParty => {
        self.leave_party().await;
      }
      IoEvent::SyncPlayback => {
        self.sync_playback().await;
      }
      IoEvent::PartyPlaybackCommand(action) => {
        self.party_playback_command(action).await;
      }
      IoEvent::SearchTracksForPlaylist(query) => {
        self.search_tracks_for_playlist(query).await;
      }
      IoEvent::CreateNewPlaylist(name, track_ids) => {
        self.create_new_playlist(name, track_ids).await;
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

  async fn show_status_message(&self, message: String, ttl_secs: u64) {
    let mut app = self.app.lock().await;
    app.status_message = Some(message);
    app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl_secs));
  }

  async fn refresh_authentication(&mut self) {
    // refresh_token() calls refetch_token() AND stores the result in self.spotify.token.
    // Using refetch_token() directly would return the new token without storing it.
    if let Err(e) = self.spotify.refresh_token().await {
      self.handle_error(anyhow!(e)).await;
      return;
    }

    // Update app.spotify_token_expiry so the main loop doesn't keep dispatching
    // RefreshAuthentication on every tick after the original token expires.
    let new_expiry = {
      let token_lock = self
        .spotify
        .token
        .lock()
        .await
        .expect("Failed to lock token");
      token_lock.as_ref().map(|t| {
        let secs = t.expires_in.num_seconds().max(0) as u64;
        std::time::SystemTime::now()
          .checked_add(Duration::from_secs(secs))
          .unwrap_or_else(std::time::SystemTime::now)
      })
    };

    if let Some(expiry) = new_expiry {
      let mut app = self.app.lock().await;
      app.spotify_token_expiry = expiry;
    }
  }

  async fn start_party(&mut self, control_mode: sync::ControlMode) {
    {
      let mut app = self.app.lock().await;
      app.party_status = sync::PartyStatus::Connecting;
    }

    let relay_url = {
      let app = self.app.lock().await;
      app.user_config.behavior.relay_server_url.clone()
    };

    let mode_str = match &control_mode {
      sync::ControlMode::HostOnly => "host_only",
      sync::ControlMode::SharedControl => "shared_control",
    };

    match sync::connect_to_relay(&relay_url, "create", &[("control_mode", mode_str)]).await {
      Ok((conn, read)) => {
        let (incoming_tx, incoming_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(sync::start_party_reader(read, incoming_tx));
        self.party_connection = Some(conn);
        self.party_incoming_rx = Some(incoming_rx);

        let mut app = self.app.lock().await;
        app.party_status = sync::PartyStatus::Hosting;
        app.party_session = Some(sync::PartySession {
          role: sync::PartyRole::Host,
          code: String::new(),
          guests: Vec::new(),
          control_mode,
          host_name: "Host".to_string(),
        });
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.party_status = sync::PartyStatus::Disconnected;
        app.handle_error(anyhow!("Failed to start party: {}", e));
      }
    }
  }

  async fn join_party(&mut self, code: String, name: String) {
    {
      let mut app = self.app.lock().await;
      app.party_status = sync::PartyStatus::Connecting;
    }

    let relay_url = {
      let app = self.app.lock().await;
      app.user_config.behavior.relay_server_url.clone()
    };

    match sync::connect_to_relay(&relay_url, "join", &[("code", &code), ("name", &name)]).await {
      Ok((conn, read)) => {
        let (incoming_tx, incoming_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(sync::start_party_reader(read, incoming_tx));
        self.party_connection = Some(conn);
        self.party_incoming_rx = Some(incoming_rx);

        let mut app = self.app.lock().await;
        app.party_status = sync::PartyStatus::Joined;
        app.party_session = Some(sync::PartySession {
          role: sync::PartyRole::Guest,
          code: code.to_uppercase(),
          guests: Vec::new(),
          control_mode: sync::ControlMode::default(),
          host_name: String::new(),
        });
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.party_status = sync::PartyStatus::Disconnected;
        app.handle_error(anyhow!("Failed to join party: {}", e));
      }
    }
  }

  async fn leave_party(&mut self) {
    if let Some(conn) = &mut self.party_connection {
      conn.close().await;
    }
    self.party_connection = None;
    self.party_incoming_rx = None;

    let mut app = self.app.lock().await;
    app.party_status = sync::PartyStatus::Disconnected;
    app.party_session = None;
  }

  async fn sync_playback(&mut self) {
    let sync_state = {
      let app = self.app.lock().await;
      let session = match &app.party_session {
        Some(s) if s.role == sync::PartyRole::Host => s,
        _ => return,
      };
      let _ = session;

      let (track_uri, is_playing) = match &app.current_playback_context {
        Some(ctx) => {
          let uri = match &ctx.item {
            Some(rspotify::model::PlayableItem::Track(t)) => {
              t.id.as_ref().map(|id| id.uri()).unwrap_or_default()
            }
            Some(rspotify::model::PlayableItem::Episode(e)) => e.id.uri(),
            Some(_) | None => return,
          };
          (uri, ctx.is_playing)
        }
        None => return,
      };

      sync::SyncMessage::SyncState {
        track_uri,
        position_ms: app.song_progress_ms as u64,
        is_playing,
        timestamp: sync::now_ms(),
      }
    };

    if let Some(conn) = &mut self.party_connection {
      if let Err(e) = conn.send(&sync_state).await {
        log::error!("Failed to send sync state: {}", e);
      }
    }
  }

  async fn set_party_control_mode(&mut self, control_mode: sync::ControlMode) {
    let control_mode = match control_mode {
      sync::ControlMode::HostOnly => "host_only",
      sync::ControlMode::SharedControl => "shared_control",
    };

    let msg = sync::SyncMessage::SetControlMode {
      control_mode: control_mode.to_string(),
    };

    if let Some(conn) = &mut self.party_connection {
      if let Err(e) = conn.send(&msg).await {
        log::error!("Failed to send control mode update: {}", e);
      }
    }
  }

  async fn party_playback_command(&mut self, action: sync::PlaybackAction) {
    let msg = sync::SyncMessage::PlaybackCommand { action, from: None };
    if let Some(conn) = &mut self.party_connection {
      if let Err(e) = conn.send(&msg).await {
        log::error!("Failed to send playback command: {}", e);
      }
    }
  }

  pub async fn process_party_messages(&mut self) {
    let messages: Vec<sync::SyncMessage> = {
      match &mut self.party_incoming_rx {
        Some(rx) => {
          let mut msgs = Vec::new();
          while let Ok(msg) = rx.try_recv() {
            msgs.push(msg);
          }
          msgs
        }
        None => return,
      }
    };

    for msg in messages {
      match msg {
        sync::SyncMessage::RoomCreated { code, .. } => {
          let mut app = self.app.lock().await;
          if let Some(session) = &mut app.party_session {
            session.code = code;
          }
        }
        sync::SyncMessage::JoinedRoom { host_name } => {
          let mut app = self.app.lock().await;
          if let Some(session) = &mut app.party_session {
            session.host_name = host_name;
          }
        }
        sync::SyncMessage::GuestJoined { name } => {
          let mut app = self.app.lock().await;
          if let Some(session) = &mut app.party_session {
            if !session.guests.contains(&name) {
              session.guests.push(name.clone());
            }
          }
          app.status_message = Some(format!("{} joined the party", name));
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(3));
        }
        sync::SyncMessage::GuestLeft { name } => {
          let mut app = self.app.lock().await;
          if let Some(session) = &mut app.party_session {
            if let Some(pos) = session.guests.iter().position(|g| g == &name) {
              session.guests.remove(pos);
            }
          }
          app.status_message = Some(format!("{} left the party", name));
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(3));
        }
        sync::SyncMessage::SetControlMode { control_mode } => {
          let mut app = self.app.lock().await;
          if let Some(session) = &mut app.party_session {
            session.control_mode = match control_mode.as_str() {
              "shared_control" => sync::ControlMode::SharedControl,
              _ => sync::ControlMode::HostOnly,
            };
          }
        }
        sync::SyncMessage::SyncState {
          track_uri,
          position_ms,
          is_playing,
          timestamp,
        } => {
          self
            .handle_incoming_sync_state(track_uri, position_ms, is_playing, timestamp)
            .await;
        }
        sync::SyncMessage::PlaybackCommand { action, .. } => {
          self.handle_incoming_playback_command(action).await;
        }
        sync::SyncMessage::RoomClosed => {
          self.party_connection = None;
          let mut app = self.app.lock().await;
          app.party_status = sync::PartyStatus::Disconnected;
          app.party_session = None;
          app.status_message = Some("Party ended".to_string());
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(5));
        }
        sync::SyncMessage::Error { message } => {
          self.party_connection = None;
          self.party_incoming_rx = None;
          let mut app = self.app.lock().await;
          app.party_status = sync::PartyStatus::Disconnected;
          app.party_session = None;
          app.handle_error(anyhow!("Party: {}", message));
        }
        _ => {}
      }
    }
  }

  async fn handle_incoming_sync_state(
    &mut self,
    track_uri: String,
    position_ms: u64,
    is_playing: bool,
    timestamp: u64,
  ) {
    let is_guest = {
      let app = self.app.lock().await;
      matches!(
        &app.party_session,
        Some(s) if s.role == sync::PartyRole::Guest
      )
    };
    if !is_guest {
      return;
    }

    // Latency compensation: estimate how much time passed since the host sent this state
    let now = sync::now_ms();
    let transit_ms = if now > timestamp {
      (now - timestamp).min(5000) // cap at 5s to avoid wild jumps from clock skew
    } else {
      0
    };
    let compensated_position = if is_playing {
      position_ms + transit_ms
    } else {
      position_ms
    };

    let (current_uri, current_is_playing, current_progress) = {
      let app = self.app.lock().await;
      let uri = match &app.current_playback_context {
        Some(ctx) => match &ctx.item {
          Some(rspotify::model::PlayableItem::Track(t)) => {
            t.id.as_ref().map(|id| id.uri()).unwrap_or_default()
          }
          Some(rspotify::model::PlayableItem::Episode(e)) => e.id.uri(),
          Some(_) | None => String::new(),
        },
        None => String::new(),
      };
      let playing = app
        .current_playback_context
        .as_ref()
        .map(|c| c.is_playing)
        .unwrap_or(false);
      let progress = app.song_progress_ms as u64;
      (uri, playing, progress)
    };

    let mut switched_track = false;

    // Track change takes priority
    if current_uri != track_uri && !track_uri.is_empty() {
      let playable: Option<PlayableId<'static>> = if let Ok(id) = TrackId::from_uri(&track_uri) {
        let p: PlayableId<'_> = id.into();
        Some(p.into_static())
      } else if let Ok(id) = EpisodeId::from_uri(&track_uri) {
        let p: PlayableId<'_> = id.into();
        Some(p.into_static())
      } else {
        None
      };
      if let Some(playable_id) = playable {
        self
          .start_playback(None, Some(vec![playable_id]), None)
          .await;
        switched_track = true;
      }
    }

    // Play/pause sync
    // After a track switch, explicitly apply host pause state since starting playback may
    // begin playing even when host is paused.
    if (switched_track && !is_playing) || (!switched_track && current_is_playing != is_playing) {
      if is_playing {
        self.start_playback(None, None, None).await;
      } else {
        self.pause_playback().await;
      }
    }

    // Position drift correction (>3s triggers seek)
    let drift = current_progress.abs_diff(compensated_position);

    if drift > 3000 && current_uri == track_uri {
      self.seek(compensated_position as u32).await;
    }
  }

  async fn handle_incoming_playback_command(&mut self, action: sync::PlaybackAction) {
    let is_host = {
      let app = self.app.lock().await;
      matches!(
        &app.party_session,
        Some(s) if s.role == sync::PartyRole::Host
      )
    };
    if !is_host {
      return;
    }

    match action {
      sync::PlaybackAction::Play => {
        self.start_playback(None, None, None).await;
      }
      sync::PlaybackAction::Pause => {
        self.pause_playback().await;
      }
      sync::PlaybackAction::NextTrack => {
        self.next_track().await;
      }
      sync::PlaybackAction::PrevTrack => {
        self.previous_track().await;
      }
      sync::PlaybackAction::Seek { position_ms } => {
        self.seek(position_ms as u32).await;
      }
      sync::PlaybackAction::PlayTrack { uri } => {
        let playable: Option<PlayableId<'static>> = if let Ok(id) = TrackId::from_uri(&uri) {
          let p: PlayableId<'_> = id.into();
          Some(p.into_static())
        } else if let Ok(id) = EpisodeId::from_uri(&uri) {
          let p: PlayableId<'_> = id.into();
          Some(p.into_static())
        } else {
          None
        };
        if let Some(playable_id) = playable {
          self
            .start_playback(None, Some(vec![playable_id]), None)
            .await;
        }
      }
    }

    // After executing, broadcast updated state
    self.sync_playback().await;
  }
}
