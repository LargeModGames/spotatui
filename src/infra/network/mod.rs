pub mod friends;
pub mod ids;
pub mod library;
pub mod mapping;
pub mod metadata;
pub mod playback;
pub mod recommend;
pub mod requests;
pub mod search;
pub mod spotify_source;
pub mod sync;
pub mod user;
pub mod utils;

use crate::core::app::App;
use crate::core::auth;
use crate::core::config::ClientConfig;
use crate::core::plugin_api::{ShowInfo, TrackInfo};
use crate::infra::redirect_uri::redirect_uri_web_server_async;
use anyhow::anyhow;
use rspotify::model::{
  album::SimplifiedAlbum,
  enums::{Country, RepeatState},
  idtypes::{EpisodeId, PlayableId, TrackId},
};
use rspotify::prelude::Id;
// `parse_response_code` / `request_token` for the in-TUI login live on this trait.
use rspotify::clients::OAuthClient;
use rspotify::AuthCodePkceSpotify;
use std::path::PathBuf;
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
  /// Playlist id/URI, page offset.
  GetPlaylistItems(String, u32),
  /// Playlist id/URI, query.
  SearchPlaylistTracks(String, String),
  GetCurrentSavedTracks(Option<u32>),
  /// Context URI (album/artist/playlist), specific playable URIs, start offset.
  StartPlayback(Option<String>, Option<Vec<String>>, Option<usize>),
  UpdateSearchLimits(u32, u32),
  Seek(u32),
  NextTrack,
  PreviousTrack,
  ForcePreviousTrack,
  Shuffle(bool), // desired shuffle state
  Repeat(RepeatState),
  PausePlayback,
  ChangeVolume(u8),
  /// Artist id/URI, display name, market.
  GetArtist(String, String, Option<Country>),
  GetAlbumTracks(Box<SimplifiedAlbum>),
  /// Seed artist ids/URIs, seed track ids/URIs, first seed track, market.
  GetRecommendationsForSeed(
    Option<Vec<String>>,
    Option<Vec<String>>,
    Box<Option<TrackInfo>>,
    Option<Country>,
  ),
  GetCurrentUserSavedAlbums(Option<u32>),
  CurrentUserSavedAlbumsContains(Vec<String>),
  CurrentUserSavedAlbumDelete(String),
  CurrentUserSavedAlbumAdd(String),
  UserUnfollowArtists(Vec<String>),
  UserFollowArtists(Vec<String>),
  /// Owner user id, playlist id/URI, public flag.
  UserFollowPlaylist(String, String, Option<bool>),
  /// Owner user id, playlist id/URI.
  UserUnfollowPlaylist(String, String),
  /// Playlist id/URI, track id/URI.
  AddTrackToPlaylist(String, String),
  /// Playlist id/URI, track id/URI, position.
  RemoveTrackFromPlaylistAtPosition(String, String, usize),
  GetUser,
  /// Playable URI (track or episode) to toggle in saved tracks.
  ToggleSaveTrack(String),
  /// Track id/URI, market.
  GetRecommendationsForTrackId(String, Option<Country>),
  GetRecentlyPlayed,
  /// Pagination cursor: artist id/URI to fetch after.
  GetFollowedArtists(Option<String>),
  UserArtistFollowCheck(Vec<String>),
  GetAlbum(String),
  TransferPlaybackToDevice(String, bool),
  #[allow(dead_code)]
  AutoSelectStreamingDevice(String, bool), // Auto-select a device by name (used for native streaming)
  GetAlbumForTrack(String),
  CurrentUserSavedTracksContains(Vec<String>),
  GetCurrentUserSavedShows(Option<u32>),
  CurrentUserSavedShowsContains(Vec<String>),
  CurrentUserSavedShowDelete(String),
  CurrentUserSavedShowAdd(String),
  GetShowEpisodes(Box<ShowInfo>),
  GetShow(String),
  GetCurrentShowEpisodes(String, Option<u32>),
  /// Playable URI (track or episode) to enqueue.
  AddItemToQueue(String),
  GetQueue,
  /// Advance the native cross-source queue: play the next queued item, or resume
  /// the suspended per-source context when the queue drains. Consumed by
  /// `infra::queue::dispatch::route_queue_event` (wired first in the pump); it
  /// never reaches the Spotify network handler.
  AdvanceNativeQueue,
  /// Resume a suspended native-streaming Spotify context after the native queue
  /// drains (context URI, resume-track URI). Re-loads the context on the native
  /// device via the existing `start_playback` machinery. `allow(dead_code)`:
  /// only constructed under `streaming`, but the handler arm is unconditional.
  #[allow(dead_code)]
  ResumeSpotifyContext(Option<String>, Option<String>),
  IncrementGlobalSongCount,
  FetchGlobalSongCount,
  FetchAnnouncements,
  GetLyrics(String, String, f64),
  /// Get user's top tracks for Discover feature (with time range)
  GetUserTopTracks(crate::core::app::DiscoverTimeRange),
  /// Get Top Artists Mix - fetches top artists and their top tracks
  GetTopArtistsMix,
  /// Fetch all playlist tracks and apply sorting
  FetchAllPlaylistTracksAndSort(String),
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
  /// Create a new playlist: playlist name, track ids/URIs.
  CreateNewPlaylist(String, Vec<String>),
  /// Fetch the current user's own friend code from spotatui.com
  GetFriendCode,
  /// Fetch the current user's friends list from spotatui.com
  GetFriends,
  /// Add a friend by their 6-character friend code
  AddFriendByCode(String),
  /// Add a friend by their spotatui.com user ID
  AddFriendByUserId(String),
  /// Unfollow a friend by their spotatui.com user ID
  UnfollowFriend(String),
  /// Search spotatui.com users by display name or friend code
  SearchFriendUsers(String),
  /// List the folders under the configured local music directory (handled by
  /// `infra::local::dispatch`; a no-op on the Spotify network).
  GetLocalPlaylists,
  /// List the audio files in a local folder, identified by its `file://` URI.
  /// The URI is only read by `infra::local::dispatch` (the `local-files`
  /// feature); without it the event is an inert no-op.
  #[cfg_attr(not(feature = "local-files"), allow(dead_code))]
  GetLocalTracks(String),
  /// List the user's Subsonic server playlists (handled by
  /// `infra::subsonic::dispatch`; a no-op on the Spotify network).
  GetSubsonicPlaylists,
  /// List the tracks of a Subsonic playlist, identified by its
  /// `subsonic:playlist:` URI. Only read by `infra::subsonic::dispatch` (the
  /// `subsonic` feature); without it the event is an inert no-op.
  #[cfg_attr(not(feature = "subsonic"), allow(dead_code))]
  GetSubsonicTracks(String),
  /// Run a Subsonic catalog search and populate `app.search_results`. Only read
  /// by `infra::subsonic::dispatch`; an inert no-op without the `subsonic` feature.
  #[cfg_attr(not(feature = "subsonic"), allow(dead_code))]
  GetSubsonicSearchResults(String),
  /// Load the configured internet-radio stations into the sidebar (handled by
  /// `infra::radio::dispatch`; a no-op on the Spotify network).
  GetRadioStations,
  /// Search the radio-browser.info directory and populate `app.search_results`.
  /// Only read by `infra::radio::dispatch`; an inert no-op without the
  /// `internet-radio` feature.
  #[cfg_attr(not(feature = "internet-radio"), allow(dead_code))]
  GetRadioSearchResults(String),
  /// Run a YouTube search (via yt-dlp) and populate `app.search_results`.
  /// Only read by `infra::youtube::dispatch`; an inert no-op without the
  /// `youtube` feature.
  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  GetYouTubeSearchResults(String),
  /// Load the local YouTube playlists file into the sidebar (handled by
  /// `infra::youtube::dispatch`; a no-op on the Spotify network).
  GetYouTubePlaylists,
  /// Open a local YouTube playlist's tracks in the shared track table,
  /// identified by its `youtube:playlist:` URI.
  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  GetYouTubeTracks(String),
  /// Create a local YouTube playlist with the given name.
  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  CreateYouTubePlaylist(String),
  /// Delete the local YouTube playlist with the given `youtube:playlist:` URI.
  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  DeleteYouTubePlaylist(String),
  /// Add a video (bare id or `youtube:` URI; metadata resolved from the browse
  /// views) to a local YouTube playlist (URI or bare id).
  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  AddTrackToYouTubePlaylist(String, String),
  /// Remove a video (bare id or `youtube:` URI) from a local YouTube playlist.
  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  RemoveTrackFromYouTubePlaylist(String, String),
  /// Start an in-TUI Spotify OAuth login: open the browser and spawn the callback
  /// server. Dispatched from the `d` source picker when Spotify is unconfigured.
  /// Runs without a Spotify session (bypasses the auth gate).
  BeginSpotifyLogin,
  /// Complete an in-TUI Spotify login with the OAuth callback URL received by the
  /// spawned server. Bypasses the auth gate (there is no session yet).
  CompleteSpotifyLogin(String),
  /// Abandon an in-flight in-TUI Spotify login (callback server timed out or
  /// failed), clearing the pending state so the user can retry.
  CancelSpotifyLogin,
  /// Fetch and decode the current track's cover art (album-art URL, source
  /// thumbnail, or a local file's embedded picture). Dispatched by the shared
  /// track-change detector; handled off the `App` lock so the render loop never
  /// blocks on the download/decode. Source-agnostic and independent of Spotify
  /// auth.
  #[cfg(feature = "cover-art")]
  FetchCoverArt(crate::tui::cover_art::CoverArtRequest),
}

/// An in-flight in-TUI Spotify login. Holds the exact PKCE client that generated
/// the authorize URL: PKCE stores the `code_verifier` inside the instance, so the
/// token exchange in `complete_spotify_login` MUST run on this same client.
struct PendingLogin {
  spotify: AuthCodePkceSpotify,
  token_cache_path: PathBuf,
}

pub struct Network {
  /// The authenticated Spotify client, or `None` when spotatui was launched
  /// against a free source (YouTube/Subsonic/Radio/Local) without a Spotify
  /// session. Spotify-bound `IoEvent`s early-return at the auth gate in
  /// `handle_network_event` when this is `None`; the `spotify()` accessor is
  /// only reached from handlers that run behind that gate. In-TUI login
  /// (`CompleteSpotifyLogin`) fills this in live.
  pub spotify: Option<AuthCodePkceSpotify>,
  pub large_search_limit: u32,
  pub small_search_limit: u32,
  pub client_config: ClientConfig,
  pub app: Arc<Mutex<App>>,
  #[cfg(feature = "streaming")]
  native_idle_recovery: playback::NativeIdleRecoveryState,
  pub party_connection: Option<sync::PartyConnection>,
  pub party_incoming_rx: Option<tokio::sync::mpsc::UnboundedReceiver<sync::SyncMessage>>,
  pub token_cache_path: PathBuf,
  /// In-flight in-TUI Spotify login, if any (see `begin_spotify_login`).
  pending_login: Option<PendingLogin>,
}

impl Network {
  #[cfg(feature = "streaming")]
  pub fn new(
    spotify: Option<AuthCodePkceSpotify>,
    client_config: ClientConfig,
    app: &Arc<Mutex<App>>,
    token_cache_path: PathBuf,
  ) -> Self {
    Network {
      spotify,
      large_search_limit: 50,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
      native_idle_recovery: playback::NativeIdleRecoveryState::default(),
      party_connection: None,
      party_incoming_rx: None,
      token_cache_path,
      pending_login: None,
    }
  }

  #[cfg(not(feature = "streaming"))]
  pub fn new(
    spotify: Option<AuthCodePkceSpotify>,
    client_config: ClientConfig,
    app: &Arc<Mutex<App>>,
    token_cache_path: PathBuf,
  ) -> Self {
    Network {
      spotify,
      large_search_limit: 50,
      small_search_limit: 4,
      client_config,
      app: Arc::clone(app),
      party_connection: None,
      party_incoming_rx: None,
      token_cache_path,
      pending_login: None,
    }
  }

  /// Borrow the authenticated Spotify client. Only call this from handlers that
  /// run behind the auth gate in `handle_network_event`: that gate early-returns
  /// for every Spotify-bound event when `self.spotify` is `None`, so a handler
  /// reached past it is guaranteed a live client. The `expect` documents (and
  /// enforces at runtime) that invariant; it is unreachable in normal operation.
  fn spotify(&self) -> &AuthCodePkceSpotify {
    self
      .spotify
      .as_ref()
      .expect("Spotify client present: the auth gate rejects Spotify events when it is None")
  }

  /// True for `IoEvent`s whose handlers never call [`Network::spotify`], so they
  /// run even without a Spotify session (free-source launch). Keep this in sync
  /// with the handlers: an event listed here MUST NOT reach the `spotify()`
  /// accessor. Covered here: auth refresh (a no-op when there is no client),
  /// source-agnostic services (telemetry, announcements, LRCLIB lyrics, cover
  /// art), the spotatui.com friends/party features (their own HTTP/relay
  /// clients), pure-state updates, and the per-source browse events that the
  /// source dispatchers handle upstream (only reaching here as no-ops when their
  /// feature is disabled).
  fn event_bypasses_spotify_auth(io_event: &IoEvent) -> bool {
    #[cfg(feature = "cover-art")]
    if matches!(io_event, IoEvent::FetchCoverArt(_)) {
      return true;
    }
    matches!(
      io_event,
      IoEvent::RefreshAuthentication
        | IoEvent::BeginSpotifyLogin
        | IoEvent::CompleteSpotifyLogin(_)
        | IoEvent::CancelSpotifyLogin
        | IoEvent::FetchGlobalSongCount
        | IoEvent::IncrementGlobalSongCount
        | IoEvent::FetchAnnouncements
        | IoEvent::GetLyrics(..)
        | IoEvent::UpdateSearchLimits(..)
        | IoEvent::GetFriendCode
        | IoEvent::GetFriends
        | IoEvent::AddFriendByCode(_)
        | IoEvent::AddFriendByUserId(_)
        | IoEvent::UnfollowFriend(_)
        | IoEvent::SearchFriendUsers(_)
        | IoEvent::StartParty(_)
        | IoEvent::JoinParty { .. }
        | IoEvent::SetPartyControlMode(_)
        | IoEvent::LeaveParty
        | IoEvent::SyncPlayback
        | IoEvent::PartyPlaybackCommand(_)
        | IoEvent::GetLocalPlaylists
        | IoEvent::GetLocalTracks(_)
        | IoEvent::GetSubsonicPlaylists
        | IoEvent::GetSubsonicTracks(_)
        | IoEvent::GetSubsonicSearchResults(_)
        | IoEvent::GetRadioStations
        | IoEvent::GetRadioSearchResults(_)
        | IoEvent::GetYouTubeSearchResults(_)
        | IoEvent::GetYouTubePlaylists
        | IoEvent::GetYouTubeTracks(_)
        | IoEvent::CreateYouTubePlaylist(_)
        | IoEvent::DeleteYouTubePlaylist(_)
        | IoEvent::AddTrackToYouTubePlaylist(..)
        | IoEvent::RemoveTrackFromYouTubePlaylist(..)
    )
  }

  #[allow(clippy::cognitive_complexity)]
  pub async fn handle_network_event(&mut self, io_event: IoEvent) {
    // Events whose handlers never touch the Spotify client run regardless of
    // whether a Spotify session exists (see `event_bypasses_spotify_auth`).
    // Everything else is Spotify-bound: when launched against a free source with
    // no Spotify session, point the user at the in-TUI login path instead of
    // failing loudly; otherwise ensure the token is fresh before proceeding.
    let bypass_auth = Self::event_bypasses_spotify_auth(&io_event);

    if !bypass_auth {
      if self.spotify.is_none() {
        self
          .show_status_message(
            "Spotify not connected. Press `d` and pick Spotify to log in.".to_string(),
            6,
          )
          .await;
        let mut app = self.app.lock().await;
        app.is_loading = false;
        return;
      }
      if !self.ensure_authentication_fresh(false).await {
        return;
      }
    }

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
        if let Some(id) = ids::playlist_id(&playlist_id) {
          self.get_playlist_tracks(id, playlist_offset).await;
        }
      }
      IoEvent::SearchPlaylistTracks(playlist_id, query) => {
        if let Some(id) = ids::playlist_id(&playlist_id) {
          self.search_playlist_tracks(id, query).await;
        }
      }
      IoEvent::GetCurrentSavedTracks(offset) => {
        self.get_current_user_saved_tracks(offset).await;
      }
      IoEvent::StartPlayback(context_uri, uris, offset) => {
        let context = context_uri.as_deref().and_then(ids::play_context_id);
        let uris = uris.map(|v| ids::playable_ids(&v));
        self.start_playback(context, uris, offset).await;
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
        if let Some(id) = ids::artist_id(&artist_id) {
          self.get_artist(id, input_artist_name, country).await;
        }
      }
      IoEvent::GetAlbumTracks(album) => {
        self.get_album_tracks(album).await;
      }
      IoEvent::GetRecommendationsForSeed(seed_artists, seed_tracks, first_track, country) => {
        let seed_artists = seed_artists.map(|v| ids::artist_ids(&v));
        let seed_tracks = seed_tracks.map(|v| ids::track_ids(&v));
        self
          .get_recommendations_for_seed(seed_artists, seed_tracks, first_track, country)
          .await;
      }
      IoEvent::GetCurrentUserSavedAlbums(offset) => {
        self.get_current_user_saved_albums(offset).await;
      }
      IoEvent::CurrentUserSavedAlbumsContains(album_ids) => {
        self
          .current_user_saved_albums_contains(ids::album_ids(&album_ids))
          .await;
      }
      IoEvent::CurrentUserSavedAlbumDelete(album_id) => {
        if let Some(id) = ids::album_id(&album_id) {
          self.current_user_saved_album_delete(id).await;
        }
      }
      IoEvent::CurrentUserSavedAlbumAdd(album_id) => {
        if let Some(id) = ids::album_id(&album_id) {
          self.current_user_saved_album_add(id).await;
        }
      }
      IoEvent::UserUnfollowArtists(artist_ids) => {
        self
          .user_unfollow_artists(ids::artist_ids(&artist_ids))
          .await;
      }
      IoEvent::UserFollowArtists(artist_ids) => {
        self.user_follow_artists(ids::artist_ids(&artist_ids)).await;
      }
      IoEvent::UserFollowPlaylist(playlist_owner_id, playlist_id, is_public) => {
        if let (Some(owner), Some(id)) = (
          ids::user_id(&playlist_owner_id),
          ids::playlist_id(&playlist_id),
        ) {
          self.user_follow_playlist(owner, id, is_public).await;
        }
      }
      IoEvent::UserUnfollowPlaylist(user_id, playlist_id) => {
        if let (Some(owner), Some(id)) = (ids::user_id(&user_id), ids::playlist_id(&playlist_id)) {
          self.user_unfollow_playlist(owner, id).await;
        }
      }
      IoEvent::AddTrackToPlaylist(playlist_id, track_id) => {
        if let (Some(pid), Some(tid)) = (ids::playlist_id(&playlist_id), ids::track_id(&track_id)) {
          self.add_track_to_playlist(pid, tid).await;
        }
      }
      IoEvent::RemoveTrackFromPlaylistAtPosition(playlist_id, track_id, position) => {
        if let (Some(pid), Some(tid)) = (ids::playlist_id(&playlist_id), ids::track_id(&track_id)) {
          self
            .remove_track_from_playlist_at_position(pid, tid, position)
            .await;
        }
      }

      IoEvent::ToggleSaveTrack(uri) => {
        if let Some(id) = ids::playable_id(&uri) {
          self.toggle_save_track(id).await;
        }
      }
      IoEvent::GetRecommendationsForTrackId(track_id, country) => {
        if let Some(id) = ids::track_id(&track_id) {
          self.get_recommendations_for_track_id(id, country).await;
        }
      }
      IoEvent::GetRecentlyPlayed => {
        self.get_recently_played().await;
      }
      IoEvent::GetFollowedArtists(after) => {
        self
          .get_followed_artists(after.and_then(|s| ids::artist_id(&s)))
          .await;
      }
      IoEvent::UserArtistFollowCheck(artist_ids) => {
        self
          .user_artist_check_follow(ids::artist_ids(&artist_ids))
          .await;
      }
      IoEvent::GetAlbum(album_id) => {
        if let Some(id) = ids::album_id(&album_id) {
          self.get_album(id).await;
        }
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
        if let Some(id) = ids::track_id(&track_id) {
          self.get_album_for_track(id).await;
        }
      }
      IoEvent::Shuffle(shuffle_state) => {
        self.shuffle(shuffle_state).await;
      }
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        self
          .current_user_saved_tracks_contains(ids::track_ids(&track_ids))
          .await;
      }
      IoEvent::GetCurrentUserSavedShows(offset) => {
        self.get_current_user_saved_shows(offset).await;
      }
      IoEvent::CurrentUserSavedShowsContains(show_ids) => {
        self
          .current_user_saved_shows_contains(ids::show_ids(&show_ids))
          .await;
      }
      IoEvent::CurrentUserSavedShowDelete(show_id) => {
        if let Some(id) = ids::show_id(&show_id) {
          self.current_user_saved_shows_delete(id).await;
        }
      }
      IoEvent::CurrentUserSavedShowAdd(show_id) => {
        if let Some(id) = ids::show_id(&show_id) {
          self.current_user_saved_shows_add(id).await;
        }
      }
      IoEvent::GetShowEpisodes(show) => {
        self.get_show_episodes(show).await;
      }
      IoEvent::GetShow(show_id) => {
        if let Some(id) = ids::show_id(&show_id) {
          self.get_show(id).await;
        }
      }
      IoEvent::GetCurrentShowEpisodes(show_id, offset) => {
        if let Some(id) = ids::show_id(&show_id) {
          self.get_current_show_episodes(id, offset).await;
        }
      }
      IoEvent::AddItemToQueue(uri) => {
        if let Some(id) = ids::playable_id(&uri) {
          self.add_item_to_queue(id).await;
        }
      }
      IoEvent::GetQueue => {
        self.get_queue().await;
      }
      // Consumed by the queue router before it reaches the network; only lands
      // here if the router somehow let it through. No Spotify work to do.
      IoEvent::AdvanceNativeQueue => {}
      IoEvent::ResumeSpotifyContext(context_uri, resume_track_uri) => {
        self
          .resume_spotify_context(context_uri, resume_track_uri)
          .await;
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
      #[cfg(feature = "cover-art")]
      IoEvent::FetchCoverArt(request) => {
        self.fetch_cover_art(request).await;
      }
      IoEvent::GetUserTopTracks(time_range) => {
        self.get_user_top_tracks(time_range).await;
      }
      IoEvent::GetTopArtistsMix => {
        self.get_top_artists_mix().await;
      }
      IoEvent::FetchAllPlaylistTracksAndSort(playlist_id) => {
        if let Some(id) = ids::playlist_id(&playlist_id) {
          self.fetch_all_playlist_tracks_and_sort(id).await;
        }
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
        self
          .create_new_playlist(name, ids::track_ids(&track_ids))
          .await;
      }
      IoEvent::GetFriendCode => {
        friends::handle_get_friend_code(self).await;
      }
      IoEvent::GetFriends => {
        friends::handle_get_friends(self).await;
      }
      IoEvent::AddFriendByCode(code) => {
        friends::handle_add_friend_by_code(self, code).await;
      }
      IoEvent::AddFriendByUserId(user_id) => {
        friends::handle_add_friend_by_user_id(self, user_id).await;
      }
      IoEvent::UnfollowFriend(user_id) => {
        friends::handle_unfollow_friend(self, user_id).await;
      }
      IoEvent::SearchFriendUsers(query) => {
        friends::handle_search_friend_users(self, query).await;
      }
      IoEvent::BeginSpotifyLogin => {
        self.begin_spotify_login().await;
      }
      IoEvent::CompleteSpotifyLogin(callback_url) => {
        self.complete_spotify_login(callback_url).await;
      }
      IoEvent::CancelSpotifyLogin => {
        self.cancel_spotify_login().await;
      }
      // Local-files browse events are handled by infra::local::dispatch before
      // reaching the network; they only arrive here when the feature is off.
      IoEvent::GetLocalPlaylists | IoEvent::GetLocalTracks(_) => {}
      // Subsonic browse/search events are handled by infra::subsonic::dispatch
      // before reaching the network; they only arrive here when the feature is off.
      IoEvent::GetSubsonicPlaylists
      | IoEvent::GetSubsonicTracks(_)
      | IoEvent::GetSubsonicSearchResults(_) => {}
      // Radio browse/search events are handled by infra::radio::dispatch before
      // reaching the network; they only arrive here when the feature is off.
      IoEvent::GetRadioStations | IoEvent::GetRadioSearchResults(_) => {}
      // YouTube search/playlist events are handled by infra::youtube::dispatch
      // before reaching the network; they only arrive here when the feature is
      // off.
      IoEvent::GetYouTubeSearchResults(_)
      | IoEvent::GetYouTubePlaylists
      | IoEvent::GetYouTubeTracks(_)
      | IoEvent::CreateYouTubePlaylist(_)
      | IoEvent::DeleteYouTubePlaylist(_)
      | IoEvent::AddTrackToYouTubePlaylist(..)
      | IoEvent::RemoveTrackFromYouTubePlaylist(..) => {}
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
    // Scale by status_message_ttl_percent, matching App::set_status_message.
    let pct = app.user_config.behavior.status_message_ttl_percent as u64;
    let ttl = ((ttl_secs * pct + 50) / 100).max(1);
    app.status_message = Some(message);
    app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(ttl));
  }

  async fn refresh_authentication(&mut self) {
    self.ensure_authentication_fresh(true).await;
  }

  async fn ensure_authentication_fresh(&mut self, force: bool) -> bool {
    // No Spotify session (free-source launch): there is no token to refresh.
    // Spotify-bound events never reach here in that state because the auth gate
    // in `handle_network_event` rejects them first; the only caller that can hit
    // this branch is `RefreshAuthentication`, for which a no-op is correct.
    let Some(spotify) = self.spotify.as_ref() else {
      let mut app = self.app.lock().await;
      app.auth_refresh_in_progress = false;
      return false;
    };
    match auth::refresh_token_and_cache(spotify, &self.token_cache_path, force).await {
      Ok(expiry) => {
        let mut app = self.app.lock().await;
        app.spotify_token_expiry = Some(expiry);
        app.auth_refresh_in_progress = false;
        true
      }
      Err(e) => {
        {
          let mut app = self.app.lock().await;
          app.auth_refresh_in_progress = false;
          app.is_loading = false;
        }
        self.handle_error(anyhow!(e)).await;
        false
      }
    }
  }

  /// Start an in-TUI Spotify OAuth login: build the PKCE client, open the browser,
  /// and spawn a callback server that reports back via `CompleteSpotifyLogin`
  /// (or `CancelSpotifyLogin` on timeout/failure). Non-blocking: the UI keeps
  /// rendering while the browser round-trips.
  async fn begin_spotify_login(&mut self) {
    if self.spotify.is_some() {
      // Already connected — nothing to log into.
      return;
    }
    if self.pending_login.is_some() {
      self
        .show_status_message("Spotify login already in progress...".to_string(), 4)
        .await;
      return;
    }

    let config_paths = match self.client_config.get_or_build_paths() {
      Ok(paths) => paths,
      Err(e) => {
        self
          .show_status_message(format!("Spotify login setup failed: {e}"), 8)
          .await;
        return;
      }
    };

    let (spotify, authorize_url, port, token_cache_path) =
      match auth::prepare_interactive_login(&self.client_config, &config_paths) {
        Ok(prepared) => prepared,
        Err(e) => {
          self
            .show_status_message(format!("Spotify login setup failed: {e}"), 8)
            .await;
          return;
        }
      };

    if let Err(e) = open::that(&authorize_url) {
      log::warn!("[login] failed to open browser automatically: {e}");
      self
        .show_status_message(
          format!("Open this URL in your browser to log in to Spotify: {authorize_url}"),
          30,
        )
        .await;
    } else {
      self
        .show_status_message("Opening browser to log in to Spotify...".to_string(), 12)
        .await;
    }

    // Keep the PKCE client on `self`: PKCE stores the code_verifier inside it and
    // the token exchange in `complete_spotify_login` must reuse this instance.
    self.pending_login = Some(PendingLogin {
      spotify,
      token_cache_path,
    });

    // Drive the callback server off a spawned task so the pump/UI stay responsive.
    // The task carries only the sender + port, never the client.
    let io_tx = self.app.lock().await.io_tx_clone();
    let Some(io_tx) = io_tx else {
      return;
    };
    tokio::spawn(async move {
      let overall_timeout = Duration::from_secs(180);
      match tokio::time::timeout(overall_timeout, redirect_uri_web_server_async(port)).await {
        Ok(Ok(url)) => {
          let _ = io_tx.send(IoEvent::CompleteSpotifyLogin(url));
        }
        Ok(Err(())) => {
          log::warn!("[login] callback server failed to start");
          let _ = io_tx.send(IoEvent::CancelSpotifyLogin);
        }
        Err(_) => {
          log::warn!(
            "[login] login timed out after {}s",
            overall_timeout.as_secs()
          );
          let _ = io_tx.send(IoEvent::CancelSpotifyLogin);
        }
      }
    });
  }

  /// Finish an in-TUI Spotify login from the OAuth callback URL. The token
  /// exchange runs on the SAME PKCE client that produced the authorize URL.
  /// Native streaming still requires a restart (its init happens pre-TUI).
  async fn complete_spotify_login(&mut self, callback_url: String) {
    let Some(pending) = self.pending_login.take() else {
      return;
    };
    let PendingLogin {
      spotify,
      token_cache_path,
    } = pending;

    let Some(code) = spotify.parse_response_code(&callback_url) else {
      self
        .show_status_message("Spotify login failed: invalid callback URL.".to_string(), 8)
        .await;
      return;
    };

    if let Err(e) = spotify.request_token(&code).await {
      self
        .show_status_message(format!("Spotify login failed: {e}"), 8)
        .await;
      return;
    }

    if let Err(e) = auth::save_token_to_file(&spotify, &token_cache_path).await {
      log::warn!("[login] failed to cache token after login: {e}");
    }
    let expiry = auth::token_expiry(&spotify).await.ok();

    self.spotify = Some(spotify);
    self.token_cache_path = token_cache_path;
    {
      let mut app = self.app.lock().await;
      app.spotify_token_expiry = expiry;
      app.spotify_connected = true;
      // Load Spotify data now that a session exists.
      app.dispatch(IoEvent::GetUser);
      app.dispatch(IoEvent::GetPlaylists);
      app.dispatch(IoEvent::GetCurrentPlayback);
    }
    self
      .show_status_message(
        "Spotify connected. Restart spotatui to enable native playback.".to_string(),
        10,
      )
      .await;
  }

  /// Clear an abandoned in-TUI login (callback timed out or failed) so the user
  /// can retry.
  async fn cancel_spotify_login(&mut self) {
    if self.pending_login.take().is_some() {
      self
        .show_status_message(
          "Spotify login timed out. Press `d` and pick Spotify to try again.".to_string(),
          6,
        )
        .await;
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::config::ClientConfig;
  use crate::core::user_config::UserConfig;
  use chrono::{TimeDelta, Utc};
  use rspotify::{Config, Credentials, OAuth, Token};
  use std::time::SystemTime;

  async fn spotify_with_token(token: Token) -> AuthCodePkceSpotify {
    let spotify = AuthCodePkceSpotify::with_config(
      Credentials::new_pkce("test_client_id"),
      OAuth {
        redirect_uri: "http://localhost:8888/callback".to_string(),
        ..Default::default()
      },
      Config::default(),
    );

    let mut token_lock = spotify.token.lock().await.expect("Failed to lock token");
    *token_lock = Some(token);
    drop(token_lock);

    spotify
  }

  fn temp_token_cache_path() -> PathBuf {
    std::env::temp_dir().join(format!(
      "spotatui_network_test_token_{}.json",
      rand::random::<u32>()
    ))
  }

  #[tokio::test]
  async fn pre_event_auth_failure_clears_loading_state() {
    let expired_token_without_refresh = Token {
      access_token: "expired_access_token".to_string(),
      refresh_token: None,
      expires_in: TimeDelta::seconds(3600),
      expires_at: Some(Utc::now() - TimeDelta::seconds(60)),
      scopes: Default::default(),
    };
    let spotify = spotify_with_token(expired_token_without_refresh).await;
    let token_cache_path = temp_token_cache_path();
    let (io_tx, _io_rx) = std::sync::mpsc::channel();
    let app = Arc::new(Mutex::new(App::new(
      io_tx,
      UserConfig::new(),
      Some(SystemTime::now() - Duration::from_secs(60)),
    )));

    {
      let mut app = app.lock().await;
      app.is_loading = true;
      app.auth_refresh_in_progress = true;
    }

    let mut network = Network::new(
      Some(spotify),
      ClientConfig::new(),
      &app,
      token_cache_path.clone(),
    );
    network.handle_network_event(IoEvent::GetUser).await;

    let app = app.lock().await;
    assert!(!app.is_loading);
    assert!(!app.auth_refresh_in_progress);

    let _ = std::fs::remove_file(token_cache_path);
  }
}
