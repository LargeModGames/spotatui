use crate::core::app::{
  ActiveBlock, App, CreatePlaylistFocus, CreatePlaylistStage, PlaylistFolderItem, RouteId,
  SearchResultBlock, SettingValue, TrackTableContext, LIBRARY_OPTIONS,
};
use crate::core::auth;
use crate::core::config::ClientConfig;
use crate::core::user_config::{StartupBehavior, UserConfig, UserConfigPaths};
#[cfg(feature = "discord-rpc")]
use crate::infra::discord_rpc;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
use crate::infra::macos_media;
use crate::infra::media_metadata::{current_playback_snapshot, PlaybackMetadata};
#[cfg(all(feature = "mpris", target_os = "linux"))]
use crate::infra::mpris;
use crate::infra::network::sync::{ControlMode, PartyRole, PlaybackAction};
use crate::infra::network::{IoEvent, Network};
#[cfg(feature = "streaming")]
use crate::infra::player;
#[cfg(feature = "streaming")]
use crate::infra::player::StreamingPlayer;

use anyhow::Result;
use log::info;
#[cfg(feature = "streaming")]
use log::warn;
#[cfg(feature = "streaming")]
use rspotify::clients::OAuthClient;
use rspotify::model::{
  album::{SavedAlbum, SimplifiedAlbum},
  artist::{FullArtist, SimplifiedArtist},
  idtypes::{EpisodeId, TrackId},
  page::{CursorBasedPage, Page},
  playlist::SimplifiedPlaylist,
  show::{Show, SimplifiedShow},
  track::{FullTrack, SimplifiedTrack},
  Device, DeviceType, PlayHistory, PlayableItem, RepeatState,
};
use rspotify::prelude::Id;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
#[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
use std::sync::atomic::AtomicBool;
#[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
use std::sync::atomic::AtomicU64;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// GUI snapshot/command types (used by both TUI and Tauri frontends)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSnapshot {
  pub playback: GuiPlayback,
  pub devices: Vec<GuiDevice>,
  pub status: GuiStatus,
  pub user: Option<GuiUser>,
  pub library: GuiLibrary,
  pub playlists: Vec<GuiPlaylist>,
  pub playlist_folders: Vec<GuiPlaylistFolderEntry>,
  pub track_table: GuiTrackTable,
  pub queue: Vec<GuiTrack>,
  pub recently_played: Vec<GuiTrack>,
  pub search: GuiSearchResults,
  pub albums: GuiAlbumList,
  pub artists: GuiArtistList,
  pub podcasts: GuiPodcastList,
  pub lyrics: GuiLyrics,
  pub discover: GuiDiscover,
  pub settings: GuiSettings,
  pub dialog: GuiDialog,
  pub sort: GuiSort,
  pub party: GuiParty,
  pub create_playlist: GuiCreatePlaylist,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiPlayback {
  pub track: Option<GuiTrack>,
  pub progress_ms: u64,
  pub is_playing: bool,
  pub shuffle: bool,
  pub repeat: Option<String>,
  pub volume_percent: u32,
  pub device_id: Option<String>,
  pub device_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiTrack {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub item_type: String,
  pub title: String,
  pub artists: Vec<String>,
  pub album: Option<String>,
  pub image_url: Option<String>,
  pub duration_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiDevice {
  pub id: Option<String>,
  pub name: String,
  pub device_type: String,
  pub is_active: bool,
  pub is_restricted: bool,
  pub volume_percent: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiStatus {
  pub is_loading: bool,
  pub message: Option<String>,
  pub error: Option<String>,
  pub route: String,
  pub active_block: String,
  pub is_streaming_active: bool,
  pub route_id: String,
  pub hovered_block: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuiAction {
  OpenHome,
  OpenSearch { query: Option<String> },
  OpenLibraryItem { index: usize },
  OpenSavedTracks,
  OpenPlaylist { playlist_id: String },
  OpenQueue,
  OpenSettings,
  OpenParty,
  OpenLyrics,
  OpenCreatePlaylist,
  Back,
  RefreshPlayback,
  RefreshDevices,
  RefreshPlaylists,
  Play,
  Pause,
  TogglePlayback,
  NextTrack,
  PreviousTrack,
  ForcePreviousTrack,
  Seek { position_ms: u32 },
  ChangeVolume { volume_percent: u8 },
  ToggleShuffle,
  ToggleRepeat,
  TransferPlayback { device_id: String, play: bool },
  Search { query: String },
  SelectTrack { index: usize },
  MoveTrackSelection { delta: i32 },
  TrackTableNextPage,
  TrackTablePreviousPage,
  PlaySelectedTrack,
  QueueSelectedTrack,
  ToggleSaveSelectedTrack,
  OpenAddSelectedTrackToPlaylist,
  OpenRemoveSelectedTrackFromPlaylist,
  PlayRandomTrack,
  RecommendationsForSelectedTrack,
  AddItemToQueue { uri: String },
  ToggleSaveTrack { uri: String },
  StartParty { control_mode: ControlMode },
  JoinParty { code: String, name: String },
  LeaveParty,
  PartyPlaybackCommand { action: PlaybackAction },
}

pub type GuiCommand = GuiAction;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiUser {
  pub id: String,
  pub display_name: Option<String>,
  pub country: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiLibrary {
  pub options: Vec<String>,
  pub selected_index: usize,
  pub saved_tracks: GuiPageInfo,
  pub saved_albums: GuiPageInfo,
  pub saved_artists: GuiCursorInfo,
  pub saved_shows: GuiPageInfo,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiPageInfo {
  pub offset: u32,
  pub limit: u32,
  pub total: u32,
  pub page_index: usize,
  pub page_count: usize,
  pub has_previous: bool,
  pub has_next: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiCursorInfo {
  pub page_index: usize,
  pub page_count: usize,
  pub has_previous: bool,
  pub has_next: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiPlaylist {
  pub id: String,
  pub uri: String,
  pub name: String,
  pub owner: String,
  pub description: Option<String>,
  pub image_url: Option<String>,
  pub track_count: u32,
  pub collaborative: bool,
  pub editable: bool,
  pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiPlaylistFolderEntry {
  pub kind: String,
  pub id: Option<String>,
  pub name: String,
  pub index: usize,
  pub depth: usize,
  pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiTrackTable {
  pub context: Option<String>,
  pub selected_index: usize,
  pub tracks: Vec<GuiTrack>,
  pub page: GuiPageInfo,
  pub playlist_id: Option<String>,
  pub playlist_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSearchResults {
  pub query: String,
  pub selected_block: String,
  pub hovered_block: String,
  pub tracks: Vec<GuiTrack>,
  pub albums: Vec<GuiAlbum>,
  pub artists: Vec<GuiArtist>,
  pub playlists: Vec<GuiPlaylist>,
  pub shows: Vec<GuiShow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiAlbumList {
  pub selected_index: usize,
  pub albums: Vec<GuiAlbum>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiArtistList {
  pub selected_index: usize,
  pub artists: Vec<GuiArtist>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiPodcastList {
  pub selected_index: usize,
  pub shows: Vec<GuiShow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiAlbum {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  pub artists: Vec<String>,
  pub image_url: Option<String>,
  pub release_date: Option<String>,
  pub total_tracks: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiArtist {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  pub image_url: Option<String>,
  pub followers: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiShow {
  pub id: Option<String>,
  pub uri: Option<String>,
  pub name: String,
  pub publisher: Option<String>,
  pub description: Option<String>,
  pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiLyrics {
  pub status: String,
  pub lines: Vec<GuiLyricLine>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiLyricLine {
  pub timestamp_ms: u64,
  pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiDiscover {
  pub selected_index: usize,
  pub time_range: String,
  pub loading: bool,
  pub top_tracks: Vec<GuiTrack>,
  pub artists_mix: Vec<GuiTrack>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSettings {
  pub category: String,
  pub selected_index: usize,
  pub edit_mode: bool,
  pub edit_buffer: String,
  pub unsaved_prompt_visible: bool,
  pub items: Vec<GuiSettingItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSettingItem {
  pub id: String,
  pub name: String,
  pub description: String,
  pub value: String,
  pub value_type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiDialog {
  pub kind: Option<String>,
  pub message: Option<String>,
  pub confirm: bool,
  pub pending_track_name: Option<String>,
  pub playlist_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSort {
  pub visible: bool,
  pub selected_index: usize,
  pub context: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiParty {
  pub status: String,
  pub role: Option<String>,
  pub code: Option<String>,
  pub host_name: Option<String>,
  pub guests: Vec<String>,
  pub control_mode: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiCreatePlaylist {
  pub name: String,
  pub stage: String,
  pub focus: String,
  pub search_input: String,
  pub selected_result: usize,
  pub tracks: Vec<GuiTrack>,
  pub search_results: Vec<GuiTrack>,
}

pub fn snapshot_app(app: &App) -> GuiSnapshot {
  GuiSnapshot {
    playback: GuiPlayback::from_app(app),
    devices: app
      .devices
      .as_ref()
      .map(|payload| payload.devices.iter().map(GuiDevice::from).collect())
      .unwrap_or_default(),
    status: GuiStatus::from_app(app),
    user: app.user.as_ref().map(GuiUser::from),
    library: GuiLibrary::from_app(app),
    playlists: playlists_from_app(app),
    playlist_folders: playlist_folders_from_app(app),
    track_table: GuiTrackTable::from_app(app),
    queue: queue_from_app(app),
    recently_played: recently_played_from_app(app),
    search: GuiSearchResults::from_app(app),
    albums: GuiAlbumList::from_app(app),
    artists: GuiArtistList::from_app(app),
    podcasts: GuiPodcastList::from_app(app),
    lyrics: GuiLyrics::from_app(app),
    discover: GuiDiscover::from_app(app),
    settings: GuiSettings::from_app(app),
    dialog: GuiDialog::from_app(app),
    sort: GuiSort::from_app(app),
    party: GuiParty::from_app(app),
    create_playlist: GuiCreatePlaylist::from_app(app),
  }
}

pub fn dispatch_gui_command(app: &mut App, command: GuiCommand) {
  dispatch_gui_action(app, command);
}

pub fn dispatch_gui_action(app: &mut App, action: GuiAction) {
  match action {
    GuiAction::OpenHome => crate::tui::actions::open_home(app),
    GuiAction::OpenSearch { query } => crate::tui::actions::open_search(app, query),
    GuiAction::OpenLibraryItem { index } => crate::tui::actions::open_library_item(app, index),
    GuiAction::OpenSavedTracks => crate::tui::actions::open_saved_tracks(app),
    GuiAction::OpenPlaylist { playlist_id } => {
      crate::tui::actions::select_playlist_by_id(app, &playlist_id);
    }
    GuiAction::OpenQueue => crate::tui::actions::open_queue(app),
    GuiAction::OpenSettings => crate::tui::actions::open_settings(app),
    GuiAction::OpenParty => crate::tui::actions::open_party(app),
    GuiAction::OpenLyrics => crate::tui::actions::open_lyrics(app),
    GuiAction::OpenCreatePlaylist => crate::tui::actions::open_create_playlist(app),
    GuiAction::Back => {
      app.pop_navigation_stack();
    }
    GuiAction::RefreshPlayback => app.dispatch(IoEvent::GetCurrentPlayback),
    GuiAction::RefreshDevices => app.dispatch(IoEvent::GetDevices),
    GuiAction::RefreshPlaylists => app.dispatch(IoEvent::GetPlaylists),
    GuiAction::Play => app.dispatch(IoEvent::StartPlayback(None, None, None)),
    GuiAction::Pause => app.dispatch(IoEvent::PausePlayback),
    GuiAction::TogglePlayback => app.toggle_playback(),
    GuiAction::NextTrack => app.next_track(),
    GuiAction::PreviousTrack => app.previous_track(),
    GuiAction::ForcePreviousTrack => app.force_previous_track(),
    GuiAction::Seek { position_ms } => app.dispatch(IoEvent::Seek(position_ms)),
    GuiAction::ChangeVolume { volume_percent } => {
      app.dispatch(IoEvent::ChangeVolume(volume_percent))
    }
    GuiAction::ToggleShuffle => app.shuffle(),
    GuiAction::ToggleRepeat => app.repeat(),
    GuiAction::TransferPlayback { device_id, play } => {
      app.dispatch(IoEvent::TransferPlaybackToDevice(device_id, play));
    }
    GuiAction::Search { query } => {
      app.input = query.chars().collect();
      app.input_idx = app.input.len();
      app.input_cursor_position = app.input.len() as u16;
      app.dispatch(IoEvent::GetSearchResults(query, app.get_user_country()));
      app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);
    }
    GuiAction::SelectTrack { index } => crate::tui::actions::select_track_table_row(app, index),
    GuiAction::MoveTrackSelection { delta } => {
      crate::tui::actions::move_track_selection(app, delta)
    }
    GuiAction::TrackTableNextPage => crate::tui::actions::track_table_next_page(app),
    GuiAction::TrackTablePreviousPage => crate::tui::actions::track_table_previous_page(app),
    GuiAction::PlaySelectedTrack => crate::tui::actions::play_selected_track(app),
    GuiAction::QueueSelectedTrack => crate::tui::actions::queue_selected_track(app),
    GuiAction::ToggleSaveSelectedTrack => crate::tui::actions::toggle_save_selected_track(app),
    GuiAction::OpenAddSelectedTrackToPlaylist => {
      crate::tui::actions::open_add_selected_track_to_playlist(app);
    }
    GuiAction::OpenRemoveSelectedTrackFromPlaylist => {
      crate::tui::actions::open_remove_selected_track_from_playlist(app);
    }
    GuiAction::PlayRandomTrack => crate::tui::actions::play_random_track(app),
    GuiAction::RecommendationsForSelectedTrack => {
      crate::tui::actions::recommendations_for_selected_track(app);
    }
    GuiAction::AddItemToQueue { uri } => {
      if let Some(playable_id) = playable_id_from_uri(&uri) {
        app.dispatch(IoEvent::AddItemToQueue(playable_id));
      }
    }
    GuiAction::ToggleSaveTrack { uri } => {
      if let Some(playable_id) = playable_id_from_uri(&uri) {
        app.dispatch(IoEvent::ToggleSaveTrack(playable_id));
      }
    }
    GuiAction::StartParty { control_mode } => crate::tui::actions::start_party(app, control_mode),
    GuiAction::JoinParty { code, name } => crate::tui::actions::join_party(app, code, name),
    GuiAction::LeaveParty => crate::tui::actions::leave_party(app),
    GuiAction::PartyPlaybackCommand { action } => {
      crate::tui::actions::party_playback_command(app, action);
    }
  }
}

// ---------------------------------------------------------------------------
// Session types: shared initialization for both TUI and GUI frontends
// ---------------------------------------------------------------------------

/// Startup options accepted by both the TUI runner and the GUI host.
pub struct SessionConfig {
  /// Tick rate in milliseconds (TUI only; GUI frontends can ignore this).
  pub tick_rate: u16,
  /// Optional path to a user config file.
  pub config_path: Option<PathBuf>,
  /// When true, skip the automatic update check.
  pub no_update: bool,
}

impl Default for SessionConfig {
  fn default() -> Self {
    SessionConfig {
      tick_rate: 250,
      config_path: None,
      no_update: false,
    }
  }
}

/// Holds the shared state that both the TUI and GUI frontends need after
/// initialization (auth, config, the App, and platform integrations).
///
/// The session is created in two phases:
/// 1. [`SpotatuiSession::new`] -- loads configs, authenticates, creates the App.
/// 2. [`SpotatuiSession::start_network_task`] -- initializes streaming player,
///    platform integrations, and spawns the network event handler task.
pub struct SpotatuiSession {
  app: Arc<Mutex<App>>,
  user_config: UserConfig,
  client_config: ClientConfig,
  spotify: Option<rspotify::AuthCodePkceSpotify>,
  token_cache_path: PathBuf,
  #[cfg(feature = "streaming")]
  redirect_uri: String,
  io_tx: std::sync::mpsc::Sender<IoEvent>,
  io_rx: Option<std::sync::mpsc::Receiver<IoEvent>>,

  // Streaming player (present when feature=streaming and the account supports it)
  #[cfg(feature = "streaming")]
  streaming_player: Option<Arc<StreamingPlayer>>,

  // Platform integrations
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  mpris_manager: Option<Arc<mpris::MprisManager>>,
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  macos_media_manager: Option<Arc<macos_media::MacMediaManager>>,

  // Shared atomics for lock-free position/playing state
  #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
  shared_position: Arc<AtomicU64>,
  #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
  shared_is_playing: Arc<AtomicBool>,

  // Discord RPC handle
  #[cfg(feature = "discord-rpc")]
  discord_rpc_manager: Option<discord_rpc::DiscordRpcManager>,
  #[cfg(not(feature = "discord-rpc"))]
  #[allow(dead_code)] // only accessed when discord-rpc feature is enabled
  discord_rpc_manager: Option<()>,
}

impl SpotatuiSession {
  /// Authenticate with Spotify and prepare the shared `App`.
  ///
  /// This performs the subset of startup work that is identical for both the
  /// TUI and the GUI: loading configs, authenticating, creating the io channel,
  /// and constructing the `App`.
  pub async fn new(config: SessionConfig) -> Result<Self> {
    let mut user_config = UserConfig::new();
    if let Some(config_file_path) = config.config_path {
      let path = UserConfigPaths { config_file_path };
      user_config.path_to_config.replace(path);
    }
    user_config.load_config()?;
    info!("user config loaded successfully");

    if config.tick_rate > 0 && config.tick_rate < 1000 {
      user_config.behavior.tick_rate_milliseconds = config.tick_rate as u64;
    }

    let mut client_config = ClientConfig::new();
    client_config.load_config()?;
    info!("client authentication config loaded");

    let config_paths = client_config.get_or_build_paths()?;
    let authenticated = auth::authenticate_with_fallback(&mut client_config, &config_paths).await?;
    let spotify = authenticated.spotify;
    let token_cache_path = authenticated.token_cache_path;
    #[cfg(feature = "streaming")]
    let redirect_uri = authenticated.redirect_uri;

    // Persist whatever token is now in memory
    if let Err(e) = auth::save_token_to_file(&spotify, &token_cache_path).await {
      log::warn!("Failed to cache token on startup: {}", e);
    }
    let token_expiry = auth::token_expiry(&spotify).await?;

    let (io_tx, io_rx) = std::sync::mpsc::channel::<IoEvent>();
    info!("app state initialized");

    let app = Arc::new(Mutex::new(App::new(
      io_tx.clone(),
      user_config.clone(),
      token_expiry,
    )));

    #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
    let shared_position = Arc::new(AtomicU64::new(0));
    #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
    let shared_is_playing = Arc::new(AtomicBool::new(false));

    Ok(SpotatuiSession {
      app,
      user_config,
      client_config,
      spotify: Some(spotify),
      token_cache_path,
      #[cfg(feature = "streaming")]
      redirect_uri,
      io_tx,
      io_rx: Some(io_rx),
      #[cfg(feature = "streaming")]
      streaming_player: None,
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      mpris_manager: None,
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      macos_media_manager: None,
      #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
      shared_position,
      #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
      shared_is_playing,
      #[cfg(feature = "discord-rpc")]
      discord_rpc_manager: None,
      #[cfg(not(feature = "discord-rpc"))]
      discord_rpc_manager: None,
    })
  }

  /// Initialize streaming player and platform integrations, then spawn the
  /// network event handler task.
  ///
  /// This is the second phase of startup. It must be called after `new()` and
  /// consumes the io receiver, so it can only be called once.
  pub async fn start_network_task(&mut self) -> Result<()> {
    let io_rx = self
      .io_rx
      .take()
      .ok_or_else(|| anyhow::anyhow!("network task already started"))?;

    let initial_shuffle_enabled = self.user_config.behavior.shuffle_enabled;
    let initial_startup_behavior = self.user_config.behavior.startup_behavior;

    // ── Streaming player ────────────────────────────────────────────────
    #[cfg(feature = "streaming")]
    {
      let (streaming_supported_for_account, streaming_startup_status_message) =
        if self.client_config.enable_streaming {
          crate::runtime::account_supports_native_streaming(
            self.spotify.as_ref().expect("spotify already consumed"),
          )
          .await
        } else {
          (false, None)
        };

      if let Some(message) = streaming_startup_status_message {
        let mut app_mut = self.app.lock().await;
        app_mut.set_status_message(message, 12);
      }

      if self.client_config.enable_streaming && streaming_supported_for_account {
        info!("initializing native streaming player");
        let streaming_config = player::StreamingConfig {
          device_name: self.client_config.streaming_device_name.clone(),
          bitrate: self.client_config.streaming_bitrate,
          audio_cache: self.client_config.streaming_audio_cache,
          cache_path: player::get_default_cache_path(),
          initial_volume: self.user_config.behavior.volume_percent,
        };

        let client_id = self.client_config.client_id.clone();
        let redirect_uri = self.redirect_uri.clone();

        let internal_timeout_secs: u64 = std::env::var("SPOTATUI_STREAMING_INIT_TIMEOUT_SECS")
          .ok()
          .and_then(|v| v.parse().ok())
          .filter(|&v: &u64| v > 0)
          .unwrap_or(30);
        let outer_timeout =
          std::time::Duration::from_secs(internal_timeout_secs.saturating_add(15));

        let init_task = tokio::spawn(async move {
          player::StreamingPlayer::new(&client_id, &redirect_uri, streaming_config).await
        });
        let abort_handle = init_task.abort_handle();

        self.streaming_player = match tokio::time::timeout(outer_timeout, init_task).await {
          Ok(Ok(Ok(p))) => {
            info!(
              "native streaming player initialized as '{}'",
              p.device_name()
            );
            Some(Arc::new(p))
          }
          Ok(Ok(Err(e))) => {
            info!(
              "failed to initialize streaming: {} - falling back to web api",
              e
            );
            None
          }
          Ok(Err(e)) => {
            info!(
              "streaming initialization panicked: {} - falling back to web api",
              e
            );
            None
          }
          Err(_) => {
            abort_handle.abort();
            warn!(
              "streaming initialization hung unexpectedly (outer timeout {}s) - falling back to web api",
              outer_timeout.as_secs()
            );
            None
          }
        };

        if self.streaming_player.is_some() {
          info!("native playback enabled - spotatui is available as a spotify connect device");
        }

        // Store streaming player reference in App
        {
          let mut app_mut = self.app.lock().await;
          app_mut.streaming_player = self.streaming_player.clone();
        }
      }
    }

    // ── Shared position/is_playing clones for various handlers ──────────
    #[cfg(feature = "streaming")]
    let shared_position_for_events = Arc::clone(&self.shared_position);
    #[cfg(feature = "streaming")]
    let shared_is_playing_for_events = Arc::clone(&self.shared_is_playing);
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let shared_is_playing_for_mpris = Arc::clone(&self.shared_is_playing);
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    let shared_position_for_mpris = Arc::clone(&self.shared_position);
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    let shared_is_playing_for_macos = Arc::clone(&self.shared_is_playing);

    #[cfg(feature = "streaming")]
    let (streaming_recovery_tx, streaming_recovery_rx) =
      tokio::sync::mpsc::unbounded_channel::<player::StreamingRecoveryRequest>();

    // ── MPRIS (Linux) ──────────────────────────────────────────────────
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    {
      self.mpris_manager = match mpris::MprisManager::new() {
        Ok(mgr) => {
          info!("mpris d-bus interface registered - media keys and playerctl enabled");
          Some(Arc::new(mgr))
        }
        Err(e) => {
          info!(
            "failed to initialize mpris: {} - media key control disabled",
            e
          );
          None
        }
      };

      if let Some(ref mpris) = self.mpris_manager {
        let mut app_mut = self.app.lock().await;
        app_mut.mpris_manager = Some(Arc::clone(mpris));
      }
    }

    // ── macOS Now Playing ───────────────────────────────────────────────
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    {
      self.macos_media_manager = if self.streaming_player.is_some() {
        match macos_media::MacMediaManager::new() {
          Ok(mgr) => {
            info!("macos now playing interface registered - media keys enabled");
            Some(Arc::new(mgr))
          }
          Err(e) => {
            info!(
              "failed to initialize macos media control: {} - media keys disabled",
              e
            );
            None
          }
        }
      } else {
        None
      };
    }

    // ── Discord RPC ─────────────────────────────────────────────────────
    #[cfg(feature = "discord-rpc")]
    {
      self.discord_rpc_manager = if self.user_config.behavior.enable_discord_rpc {
        match crate::runtime::resolve_discord_app_id(&self.user_config)
          .and_then(|app_id| discord_rpc::DiscordRpcManager::new(app_id).ok())
        {
          Some(mgr) => {
            info!("discord rich presence enabled");
            Some(mgr)
          }
          None => {
            info!("discord rich presence failed to initialize");
            None
          }
        }
      } else {
        info!("discord rich presence disabled");
        None
      };
    }

    // ── Spawn MPRIS event handler ───────────────────────────────────────
    #[cfg(all(feature = "mpris", target_os = "linux"))]
    if let Some(ref mpris) = self.mpris_manager {
      if let Some(event_rx) = mpris.take_event_rx() {
        #[cfg(feature = "streaming")]
        let streaming_player_for_mpris = self.streaming_player.clone();
        let mpris_for_seek = Arc::clone(mpris);
        let app_for_mpris = Arc::clone(&self.app);
        tokio::spawn(async move {
          crate::runtime::handle_mpris_events(
            event_rx,
            #[cfg(feature = "streaming")]
            streaming_player_for_mpris,
            shared_is_playing_for_mpris,
            shared_position_for_mpris,
            mpris_for_seek,
            app_for_mpris,
          )
          .await;
        });
      }
    }

    // ── Spawn macOS media event handler ─────────────────────────────────
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    if let Some(ref macos_media) = self.macos_media_manager {
      if let Some(event_rx) = macos_media.take_event_rx() {
        let app_for_macos = Arc::clone(&self.app);
        tokio::spawn(async move {
          crate::runtime::handle_macos_media_events(
            event_rx,
            app_for_macos,
            shared_is_playing_for_macos,
          )
          .await;
        });
      }
    }

    // ── Keep macOS Now Playing metadata synced ──────────────────────────
    #[cfg(all(feature = "macos-media", target_os = "macos"))]
    if let Some(ref macos_media) = self.macos_media_manager {
      let macos_media_for_metadata = Arc::clone(macos_media);
      let app_for_macos_metadata = Arc::clone(&self.app);
      tokio::spawn(async move {
        let mut last_metadata: Option<super::runtime::MacosMetadata> = None;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

        loop {
          interval.tick().await;
          if let Ok(app) = app_for_macos_metadata.try_lock() {
            crate::runtime::update_macos_metadata(
              &macos_media_for_metadata,
              &mut last_metadata,
              &app,
            );
          }
        }
      });
    }

    // ── Spawn player event listener ─────────────────────────────────────
    #[cfg(feature = "streaming")]
    if let Some(ref player) = self.streaming_player {
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      let mpris_for_events = self.mpris_manager.clone();
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      let macos_media_for_events = self.macos_media_manager.clone();

      player::spawn_player_event_handler(player::PlayerEventContext {
        player: Arc::clone(player),
        app: Arc::clone(&self.app),
        shared_position: shared_position_for_events,
        shared_is_playing: shared_is_playing_for_events,
        recovery_tx: streaming_recovery_tx.clone(),
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        mpris_manager: mpris_for_events,
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        macos_media_manager: macos_media_for_events,
      });
    }

    #[cfg(feature = "streaming")]
    {
      #[cfg(all(feature = "mpris", target_os = "linux"))]
      let mpris_for_recovery = self.mpris_manager.clone();
      #[cfg(all(feature = "macos-media", target_os = "macos"))]
      let macos_media_for_recovery = self.macos_media_manager.clone();

      player::spawn_streaming_recovery_handler(player::StreamingRecoveryContext {
        app: Arc::clone(&self.app),
        shared_position: Arc::clone(&self.shared_position),
        shared_is_playing: Arc::clone(&self.shared_is_playing),
        recovery_rx: streaming_recovery_rx,
        recovery_tx: streaming_recovery_tx.clone(),
        client_config: self.client_config.clone(),
        redirect_uri: self.redirect_uri.clone(),
        #[cfg(all(feature = "mpris", target_os = "linux"))]
        mpris_manager: mpris_for_recovery,
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        macos_media_manager: macos_media_for_recovery,
      });
    }

    // ── Spawn the main network event handler ────────────────────────────
    // The spotify client is moved into the spawned task here. After this,
    // the session no longer holds a reference to it.
    let app = Arc::clone(&self.app);
    let spotify = self
      .spotify
      .take()
      .expect("spotify client already consumed");
    let client_config = self.client_config.clone();
    let final_token_cache_path = self.token_cache_path.clone();
    #[cfg(feature = "streaming")]
    let streaming_device_name = self
      .streaming_player
      .as_ref()
      .map(|p| p.device_name().to_string());

    info!("spawning spotify network event handler");
    tokio::spawn(async move {
      let mut network = Network::new(spotify, client_config, &app, final_token_cache_path);

      // Auto-select the saved playback device when available
      #[cfg(feature = "streaming")]
      if let Some(device_name) = streaming_device_name {
        let saved_device_id = network.client_config.device_id.clone();
        let mut devices_snapshot = None;

        if let Ok(devices_vec) = network.spotify.device().await {
          let mut app = network.app.lock().await;
          app.devices = Some(rspotify::model::device::DevicePayload {
            devices: devices_vec.clone(),
          });
          devices_snapshot = Some(devices_vec);
        }

        let mut status_message = None;
        let startup_event = match saved_device_id {
          Some(saved_device_id) => {
            if let Some(devices_vec) = devices_snapshot.as_ref() {
              if devices_vec
                .iter()
                .any(|device| device.id.as_ref() == Some(&saved_device_id))
              {
                Some(IoEvent::TransferPlaybackToDevice(saved_device_id, true))
              } else {
                status_message = Some(format!("Saved device unavailable; using {}", device_name));
                let native_device_id = devices_vec
                  .iter()
                  .find(|device| device.name.eq_ignore_ascii_case(&device_name))
                  .and_then(|device| device.id.clone());
                if let Some(native_device_id) = native_device_id {
                  Some(IoEvent::TransferPlaybackToDevice(native_device_id, false))
                } else {
                  Some(IoEvent::AutoSelectStreamingDevice(
                    device_name.clone(),
                    false,
                  ))
                }
              }
            } else {
              Some(IoEvent::TransferPlaybackToDevice(saved_device_id, true))
            }
          }
          None => Some(IoEvent::AutoSelectStreamingDevice(
            device_name.clone(),
            true,
          )),
        };

        if let Some(message) = status_message {
          let mut app = network.app.lock().await;
          app.status_message = Some(message);
          app.status_message_expires_at =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
        }

        if let Some(event) = startup_event {
          network.handle_network_event(event).await;
        }
      }

      // Apply saved shuffle preference on startup
      network
        .handle_network_event(IoEvent::Shuffle(initial_shuffle_enabled))
        .await;

      // Apply configured startup play behavior
      match initial_startup_behavior {
        StartupBehavior::Continue => {}
        StartupBehavior::Play => {
          network
            .handle_network_event(IoEvent::StartPlayback(None, None, None))
            .await;
        }
        StartupBehavior::Pause => {
          network.handle_network_event(IoEvent::PausePlayback).await;
        }
      }

      network.handle_network_event(IoEvent::GetPlaylists).await;
      network.handle_network_event(IoEvent::GetUser).await;
      network.handle_network_event(IoEvent::GetDevices).await;
      network
        .handle_network_event(IoEvent::GetCurrentPlayback)
        .await;

      {
        let mut app = network.app.lock().await;
        if app.user_config.behavior.enable_global_song_count {
          app.dispatch(IoEvent::FetchGlobalSongCount);
        }
        app.dispatch(IoEvent::FetchAnnouncements);
      }

      start_network_event_loop(io_rx, &mut network).await;
    });

    Ok(())
  }

  // -- Accessors --

  /// Get a reference to the shared `App`.
  pub fn app(&self) -> Arc<Mutex<App>> {
    Arc::clone(&self.app)
  }

  /// Get a `GuiSnapshot` of the current app state.
  pub async fn snapshot(&self) -> GuiSnapshot {
    let app = self.app.lock().await;
    snapshot_app(&app)
  }

  /// Send a `GuiCommand` to the app. Delegates to `dispatch_gui_command` so
  /// that toggle logic correctly reads the current playing state.
  pub async fn dispatch(&self, command: GuiCommand) {
    let mut app = self.app.lock().await;
    dispatch_gui_command(&mut app, command);
  }

  /// Get a reference to the io event sender (for external dispatch).
  pub fn io_tx(&self) -> &std::sync::mpsc::Sender<IoEvent> {
    &self.io_tx
  }

  /// Get the user config reference.
  pub fn user_config(&self) -> &UserConfig {
    &self.user_config
  }

  /// Get the client config reference.
  pub fn client_config(&self) -> &ClientConfig {
    &self.client_config
  }

  /// Get the shared position atomic (for UI refresh loops).
  #[cfg(any(feature = "streaming", all(feature = "mpris", target_os = "linux")))]
  pub fn shared_position(&self) -> Arc<AtomicU64> {
    Arc::clone(&self.shared_position)
  }

  /// Get the Discord RPC manager (for UI event loop).
  #[cfg(feature = "discord-rpc")]
  pub fn discord_rpc_manager(&self) -> Option<&discord_rpc::DiscordRpcManager> {
    self.discord_rpc_manager.as_ref()
  }

  /// Get the MPRIS manager for UI event loop.
  #[cfg(all(feature = "mpris", target_os = "linux"))]
  pub fn mpris_manager(&self) -> Option<Arc<mpris::MprisManager>> {
    self.mpris_manager.clone()
  }
}

/// Drives the network event loop: receives IoEvents and processes them.
async fn start_network_event_loop(
  io_rx: std::sync::mpsc::Receiver<IoEvent>,
  network: &mut Network,
) {
  loop {
    match io_rx.try_recv() {
      Ok(io_event) => {
        network.handle_network_event(io_event).await;
      }
      Err(std::sync::mpsc::TryRecvError::Empty) => {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
      }
      Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
    }
    network.process_party_messages().await;
  }
}

// ---------------------------------------------------------------------------
// Private helpers for GUI snapshot types
// ---------------------------------------------------------------------------

impl GuiPlayback {
  pub fn from_app(app: &App) -> Self {
    let snapshot = current_playback_snapshot(app);
    let context = app.current_playback_context.as_ref();
    let context_item = context.and_then(|context| context.item.as_ref());
    let track = snapshot
      .as_ref()
      .map(|snapshot| GuiTrack::from_metadata_and_item(&snapshot.metadata, context_item));

    GuiPlayback {
      track,
      progress_ms: snapshot
        .as_ref()
        .map(|snapshot| snapshot.progress_ms.min(u64::MAX as u128) as u64)
        .unwrap_or_else(|| app.song_progress_ms.min(u64::MAX as u128) as u64),
      is_playing: snapshot
        .as_ref()
        .map(|snapshot| snapshot.is_playing)
        .unwrap_or(false),
      shuffle: snapshot
        .as_ref()
        .map(|snapshot| snapshot.shuffle)
        .unwrap_or(app.user_config.behavior.shuffle_enabled),
      repeat: snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.repeat.map(repeat_state_label)),
      volume_percent: app.desired_volume(),
      device_id: context.and_then(|context| context.device.id.clone()),
      device_name: context.map(|context| context.device.name.clone()),
    }
  }
}

impl GuiTrack {
  fn from_metadata_and_item(metadata: &PlaybackMetadata, item: Option<&PlayableItem>) -> Self {
    let item_identity = item.map(playable_identity).unwrap_or_default();

    GuiTrack {
      id: item_identity.id,
      uri: item_identity.uri,
      item_type: item_identity.item_type,
      title: metadata.title.clone(),
      artists: metadata.artists.clone(),
      album: (!metadata.album.is_empty()).then(|| metadata.album.clone()),
      image_url: metadata.image_url.clone(),
      duration_ms: metadata.duration_ms,
    }
  }
}

impl From<&Device> for GuiDevice {
  fn from(device: &Device) -> Self {
    GuiDevice {
      id: device.id.clone(),
      name: device.name.clone(),
      device_type: device_type_label(&device._type).to_string(),
      is_active: device.is_active,
      is_restricted: device.is_restricted,
      volume_percent: device.volume_percent,
    }
  }
}

impl GuiStatus {
  pub fn from_app(app: &App) -> Self {
    let route = app.get_current_route();

    GuiStatus {
      is_loading: app.is_loading,
      message: app.status_message.clone(),
      error: (!app.api_error.trim().is_empty()).then(|| app.api_error.clone()),
      route: format!("{:?}", route.id),
      active_block: format!("{:?}", route.active_block),
      is_streaming_active: app.is_streaming_active,
      route_id: route_id_label(&route.id).to_string(),
      hovered_block: format!("{:?}", route.hovered_block),
    }
  }
}

#[derive(Default)]
struct PlayableIdentity {
  id: Option<String>,
  uri: Option<String>,
  item_type: String,
}

fn playable_identity(item: &PlayableItem) -> PlayableIdentity {
  match item {
    PlayableItem::Track(track) => PlayableIdentity {
      id: track.id.as_ref().map(|id| id.id().to_string()),
      uri: track.id.as_ref().map(|id| id.uri()),
      item_type: "track".to_string(),
    },
    PlayableItem::Episode(episode) => PlayableIdentity {
      id: Some(episode.id.id().to_string()),
      uri: Some(episode.id.uri()),
      item_type: "episode".to_string(),
    },
    PlayableItem::Unknown(value) => PlayableIdentity {
      id: value
        .get("id")
        .and_then(|id| id.as_str())
        .map(ToString::to_string),
      uri: value
        .get("uri")
        .and_then(|uri| uri.as_str())
        .map(ToString::to_string),
      item_type: value
        .get("type")
        .and_then(|item_type| item_type.as_str())
        .unwrap_or("unknown")
        .to_string(),
    },
  }
}

fn repeat_state_label(repeat_state: RepeatState) -> String {
  match repeat_state {
    RepeatState::Off => "off",
    RepeatState::Track => "track",
    RepeatState::Context => "context",
  }
  .to_string()
}

fn device_type_label(device_type: &DeviceType) -> &'static str {
  match device_type {
    DeviceType::Computer => "computer",
    DeviceType::Tablet => "tablet",
    DeviceType::Smartphone => "smartphone",
    DeviceType::Smartwatch => "smartwatch",
    DeviceType::Speaker => "speaker",
    DeviceType::Tv => "tv",
    DeviceType::Avr => "avr",
    DeviceType::Stb => "stb",
    DeviceType::AudioDongle => "audio_dongle",
    DeviceType::GameConsole => "game_console",
    DeviceType::CastVideo => "cast_video",
    DeviceType::CastAudio => "cast_audio",
    DeviceType::Automobile => "automobile",
    DeviceType::Unknown => "unknown",
  }
}

fn route_id_label(route_id: &RouteId) -> &'static str {
  match route_id {
    RouteId::Analysis => "analysis",
    RouteId::AlbumTracks => "album_tracks",
    RouteId::AlbumList => "albums",
    RouteId::Artist => "artist",
    RouteId::LyricsView => "lyrics",
    RouteId::CoverArtView => "cover_art",
    RouteId::Error => "error",
    RouteId::Home => "home",
    RouteId::RecentlyPlayed => "recently_played",
    RouteId::Search => "search",
    RouteId::SelectedDevice => "devices",
    RouteId::TrackTable => "track_table",
    RouteId::Discover => "discover",
    RouteId::Artists => "artists",
    RouteId::Podcasts => "podcasts",
    RouteId::PodcastEpisodes => "podcast_episodes",
    RouteId::Recommendations => "recommendations",
    RouteId::Dialog => "dialog",
    RouteId::AnnouncementPrompt => "announcement",
    RouteId::ExitPrompt => "exit",
    RouteId::Settings => "settings",
    RouteId::HelpMenu => "help",
    RouteId::Queue => "queue",
    RouteId::Party => "party",
    RouteId::CreatePlaylist => "create_playlist",
  }
}

impl From<&rspotify::model::PrivateUser> for GuiUser {
  fn from(user: &rspotify::model::PrivateUser) -> Self {
    GuiUser {
      id: user.id.id().to_string(),
      display_name: user.display_name.clone(),
      country: None,
    }
  }
}

impl GuiLibrary {
  fn from_app(app: &App) -> Self {
    GuiLibrary {
      options: LIBRARY_OPTIONS
        .iter()
        .map(|item| (*item).to_string())
        .collect(),
      selected_index: app.library.selected_index,
      saved_tracks: page_info(
        app.library.saved_tracks.get_results(None),
        app.library.saved_tracks.index,
        app.library.saved_tracks.pages.len(),
      ),
      saved_albums: page_info(
        app.library.saved_albums.get_results(None),
        app.library.saved_albums.index,
        app.library.saved_albums.pages.len(),
      ),
      saved_artists: cursor_info(
        app.library.saved_artists.get_results(None),
        app.library.saved_artists.index,
        app.library.saved_artists.pages.len(),
      ),
      saved_shows: page_info(
        app.library.saved_shows.get_results(None),
        app.library.saved_shows.index,
        app.library.saved_shows.pages.len(),
      ),
    }
  }
}

impl GuiTrackTable {
  fn from_app(app: &App) -> Self {
    let page = match app.track_table.context {
      Some(TrackTableContext::SavedTracks) => page_info(
        app.library.saved_tracks.get_results(None),
        app.library.saved_tracks.index,
        app.library.saved_tracks.pages.len(),
      ),
      Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch) => page_info(
        app.current_playlist_track_page(),
        app.playlist_track_pages.index,
        app.playlist_track_pages.pages.len(),
      ),
      _ => GuiPageInfo::default(),
    };
    let playlist_id = app
      .current_playlist_track_table_id()
      .map(|playlist_id| playlist_id.id().to_string());

    GuiTrackTable {
      context: app
        .track_table
        .context
        .as_ref()
        .map(|context| format!("{:?}", context)),
      selected_index: app.track_table.selected_index,
      tracks: app.track_table.tracks.iter().map(GuiTrack::from).collect(),
      page,
      playlist_name: playlist_id
        .as_ref()
        .and_then(|id| {
          app
            .all_playlists
            .iter()
            .find(|playlist| playlist.id.id() == id)
        })
        .map(|playlist| playlist.name.clone()),
      playlist_id,
    }
  }
}

impl GuiSearchResults {
  fn from_app(app: &App) -> Self {
    let search = &app.search_results;
    GuiSearchResults {
      query: app.input.iter().collect(),
      selected_block: search_block_label(&search.selected_block).to_string(),
      hovered_block: search_block_label(&search.hovered_block).to_string(),
      tracks: search
        .tracks
        .as_ref()
        .map(|page| page.items.iter().map(GuiTrack::from).collect())
        .unwrap_or_default(),
      albums: search
        .albums
        .as_ref()
        .map(|page| page.items.iter().map(GuiAlbum::from).collect())
        .unwrap_or_default(),
      artists: search
        .artists
        .as_ref()
        .map(|page| page.items.iter().map(GuiArtist::from).collect())
        .unwrap_or_default(),
      playlists: search
        .playlists
        .as_ref()
        .map(|page| {
          page
            .items
            .iter()
            .map(|playlist| GuiPlaylist::from_playlist(playlist, app))
            .collect()
        })
        .unwrap_or_default(),
      shows: search
        .shows
        .as_ref()
        .map(|page| page.items.iter().map(GuiShow::from).collect())
        .unwrap_or_default(),
    }
  }
}

impl GuiAlbumList {
  fn from_app(app: &App) -> Self {
    GuiAlbumList {
      selected_index: app.album_list_index,
      albums: app
        .library
        .saved_albums
        .get_results(None)
        .map(|page| page.items.iter().map(GuiAlbum::from).collect())
        .unwrap_or_default(),
    }
  }
}

impl GuiArtistList {
  fn from_app(app: &App) -> Self {
    GuiArtistList {
      selected_index: app.artists_list_index,
      artists: app.artists.iter().map(GuiArtist::from).collect(),
    }
  }
}

impl GuiPodcastList {
  fn from_app(app: &App) -> Self {
    GuiPodcastList {
      selected_index: app.shows_list_index,
      shows: app
        .library
        .saved_shows
        .get_results(None)
        .map(|page| page.items.iter().map(GuiShow::from).collect())
        .unwrap_or_default(),
    }
  }
}

impl GuiLyrics {
  fn from_app(app: &App) -> Self {
    GuiLyrics {
      status: format!("{:?}", app.lyrics_status),
      lines: app
        .lyrics
        .as_ref()
        .map(|lines| {
          lines
            .iter()
            .map(|(timestamp, text)| GuiLyricLine {
              timestamp_ms: (*timestamp).min(u64::MAX as u128) as u64,
              text: text.clone(),
            })
            .collect()
        })
        .unwrap_or_default(),
    }
  }
}

impl GuiDiscover {
  fn from_app(app: &App) -> Self {
    GuiDiscover {
      selected_index: app.discover_selected_index,
      time_range: app.discover_time_range.label().to_string(),
      loading: app.discover_loading,
      top_tracks: app.discover_top_tracks.iter().map(GuiTrack::from).collect(),
      artists_mix: app
        .discover_artists_mix
        .iter()
        .map(GuiTrack::from)
        .collect(),
    }
  }
}

impl GuiSettings {
  fn from_app(app: &App) -> Self {
    GuiSettings {
      category: app.settings_category.name().to_string(),
      selected_index: app.settings_selected_index,
      edit_mode: app.settings_edit_mode,
      edit_buffer: app.settings_edit_buffer.clone(),
      unsaved_prompt_visible: app.settings_unsaved_prompt_visible,
      items: app
        .settings_items
        .iter()
        .map(GuiSettingItem::from)
        .collect(),
    }
  }
}

impl From<&crate::core::app::SettingItem> for GuiSettingItem {
  fn from(item: &crate::core::app::SettingItem) -> Self {
    let (value, value_type) = match &item.value {
      SettingValue::Bool(value) => (if *value { "On" } else { "Off" }.to_string(), "bool"),
      SettingValue::Number(value) => (value.to_string(), "number"),
      SettingValue::String(value) => (value.clone(), "string"),
      SettingValue::Color(value) => (value.clone(), "color"),
      SettingValue::Key(value) => (value.clone(), "key"),
      SettingValue::Preset(value) => (value.clone(), "preset"),
      SettingValue::Cycle(value, _) => (value.clone(), "cycle"),
    };

    GuiSettingItem {
      id: item.id.clone(),
      name: item.name.clone(),
      description: item.description.clone(),
      value,
      value_type: value_type.to_string(),
    }
  }
}

impl GuiDialog {
  fn from_app(app: &App) -> Self {
    let route = app.get_current_route();
    let kind = match route.active_block {
      ActiveBlock::Dialog(context) => Some(format!("{:?}", context)),
      _ => None,
    };
    GuiDialog {
      kind,
      message: app.dialog.clone(),
      confirm: app.confirm,
      pending_track_name: app
        .pending_playlist_track_add
        .as_ref()
        .map(|pending| pending.track_name.clone())
        .or_else(|| {
          app
            .pending_playlist_track_removal
            .as_ref()
            .map(|pending| pending.track_name.clone())
        }),
      playlist_name: app
        .pending_playlist_track_removal
        .as_ref()
        .map(|pending| pending.playlist_name.clone()),
    }
  }
}

impl GuiSort {
  fn from_app(app: &App) -> Self {
    GuiSort {
      visible: app.sort_menu_visible,
      selected_index: app.sort_menu_selected,
      context: app
        .sort_context
        .as_ref()
        .map(|context| format!("{:?}", context)),
    }
  }
}

impl GuiParty {
  fn from_app(app: &App) -> Self {
    let session = app.party_session.as_ref();
    GuiParty {
      status: app.party_status.to_string(),
      role: session.map(|session| match session.role {
        PartyRole::Host => "host".to_string(),
        PartyRole::Guest => "guest".to_string(),
      }),
      code: session.map(|session| session.code.clone()),
      host_name: session.map(|session| session.host_name.clone()),
      guests: session
        .map(|session| session.guests.clone())
        .unwrap_or_default(),
      control_mode: session.map(|session| session.control_mode.to_string()),
    }
  }
}

impl GuiCreatePlaylist {
  fn from_app(app: &App) -> Self {
    GuiCreatePlaylist {
      name: app.create_playlist_name.iter().collect(),
      stage: match app.create_playlist_stage {
        CreatePlaylistStage::Name => "name",
        CreatePlaylistStage::AddTracks => "add_tracks",
      }
      .to_string(),
      focus: match app.create_playlist_focus {
        CreatePlaylistFocus::SearchInput => "search_input",
        CreatePlaylistFocus::SearchResults => "search_results",
        CreatePlaylistFocus::AddedTracks => "added_tracks",
      }
      .to_string(),
      search_input: app.create_playlist_search_input.iter().collect(),
      selected_result: app.create_playlist_selected_result,
      tracks: app
        .create_playlist_tracks
        .iter()
        .map(GuiTrack::from)
        .collect(),
      search_results: app
        .create_playlist_search_results
        .iter()
        .map(GuiTrack::from)
        .collect(),
    }
  }
}

impl GuiPlaylist {
  fn from_playlist(playlist: &SimplifiedPlaylist, app: &App) -> Self {
    let id = playlist.id.id().to_string();
    GuiPlaylist {
      id: id.clone(),
      uri: playlist.id.uri(),
      name: playlist.name.clone(),
      owner: playlist
        .owner
        .display_name
        .clone()
        .unwrap_or_else(|| playlist.owner.id.id().to_string()),
      description: None,
      image_url: playlist.images.first().map(|image| image.url.clone()),
      track_count: playlist.items.total,
      collaborative: playlist.collaborative,
      editable: app.playlist_is_editable(playlist),
      selected: app
        .current_playlist_track_table_id()
        .as_ref()
        .is_some_and(|playlist_id| playlist_id.id() == id),
    }
  }
}

impl From<&FullTrack> for GuiTrack {
  fn from(track: &FullTrack) -> Self {
    GuiTrack {
      id: track.id.as_ref().map(|id| id.id().to_string()),
      uri: track.id.as_ref().map(|id| id.uri()),
      item_type: "track".to_string(),
      title: track.name.clone(),
      artists: track.artists.iter().map(artist_name).collect(),
      album: Some(track.album.name.clone()),
      image_url: track.album.images.first().map(|image| image.url.clone()),
      duration_ms: track.duration.num_milliseconds().max(0) as u32,
    }
  }
}

impl From<&SimplifiedTrack> for GuiTrack {
  fn from(track: &SimplifiedTrack) -> Self {
    GuiTrack {
      id: track.id.as_ref().map(|id| id.id().to_string()),
      uri: track.id.as_ref().map(|id| id.uri()),
      item_type: "track".to_string(),
      title: track.name.clone(),
      artists: track.artists.iter().map(artist_name).collect(),
      album: None,
      image_url: None,
      duration_ms: track.duration.num_milliseconds().max(0) as u32,
    }
  }
}

impl From<&PlayableItem> for GuiTrack {
  fn from(item: &PlayableItem) -> Self {
    match item {
      PlayableItem::Track(track) => GuiTrack::from(track),
      PlayableItem::Episode(episode) => GuiTrack {
        id: Some(episode.id.id().to_string()),
        uri: Some(episode.id.uri()),
        item_type: "episode".to_string(),
        title: episode.name.clone(),
        artists: vec![episode.show.name.clone()],
        album: Some(episode.show.name.clone()),
        image_url: episode.images.first().map(|image| image.url.clone()),
        duration_ms: episode.duration.num_milliseconds().max(0) as u32,
      },
      PlayableItem::Unknown(value) => GuiTrack {
        id: value
          .get("id")
          .and_then(|id| id.as_str())
          .map(ToString::to_string),
        uri: value
          .get("uri")
          .and_then(|uri| uri.as_str())
          .map(ToString::to_string),
        item_type: value
          .get("type")
          .and_then(|item_type| item_type.as_str())
          .unwrap_or("unknown")
          .to_string(),
        title: value
          .get("name")
          .and_then(|name| name.as_str())
          .unwrap_or("Unknown")
          .to_string(),
        artists: Vec::new(),
        album: None,
        image_url: None,
        duration_ms: 0,
      },
    }
  }
}

impl From<&SimplifiedAlbum> for GuiAlbum {
  fn from(album: &SimplifiedAlbum) -> Self {
    GuiAlbum {
      id: album.id.as_ref().map(|id| id.id().to_string()),
      uri: album.id.as_ref().map(|id| id.uri()),
      name: album.name.clone(),
      artists: album.artists.iter().map(artist_name).collect(),
      image_url: album.images.first().map(|image| image.url.clone()),
      release_date: album.release_date.clone().and_then(non_empty),
      total_tracks: None,
    }
  }
}

impl From<&SavedAlbum> for GuiAlbum {
  fn from(saved_album: &SavedAlbum) -> Self {
    let album = &saved_album.album;
    GuiAlbum {
      id: Some(album.id.id().to_string()),
      uri: Some(album.id.uri()),
      name: album.name.clone(),
      artists: album.artists.iter().map(artist_name).collect(),
      image_url: album.images.first().map(|image| image.url.clone()),
      release_date: non_empty(album.release_date.clone()),
      total_tracks: Some(album.tracks.total),
    }
  }
}

impl From<&FullArtist> for GuiArtist {
  fn from(artist: &FullArtist) -> Self {
    GuiArtist {
      id: Some(artist.id.id().to_string()),
      uri: Some(artist.id.uri()),
      name: artist.name.clone(),
      image_url: artist.images.first().map(|image| image.url.clone()),
      followers: None,
    }
  }
}

impl From<&SimplifiedShow> for GuiShow {
  fn from(show: &SimplifiedShow) -> Self {
    GuiShow {
      id: Some(show.id.id().to_string()),
      uri: Some(show.id.uri()),
      name: show.name.clone(),
      publisher: None,
      description: non_empty(show.description.clone()),
      image_url: show.images.first().map(|image| image.url.clone()),
    }
  }
}

impl From<&Show> for GuiShow {
  fn from(show: &Show) -> Self {
    let show = &show.show;
    GuiShow {
      id: Some(show.id.id().to_string()),
      uri: Some(show.id.uri()),
      name: show.name.clone(),
      publisher: None,
      description: non_empty(show.description.clone()),
      image_url: show.images.first().map(|image| image.url.clone()),
    }
  }
}

fn playlists_from_app(app: &App) -> Vec<GuiPlaylist> {
  if !app.all_playlists.is_empty() {
    return app
      .all_playlists
      .iter()
      .map(|playlist| GuiPlaylist::from_playlist(playlist, app))
      .collect();
  }

  app
    .playlists
    .as_ref()
    .map(|page| {
      page
        .items
        .iter()
        .map(|playlist| GuiPlaylist::from_playlist(playlist, app))
        .collect()
    })
    .unwrap_or_default()
}

fn playlist_folders_from_app(app: &App) -> Vec<GuiPlaylistFolderEntry> {
  app
    .get_playlist_display_items()
    .into_iter()
    .enumerate()
    .filter_map(|(display_index, item)| match item {
      PlaylistFolderItem::Folder(folder) => Some(GuiPlaylistFolderEntry {
        kind: "folder".to_string(),
        id: Some(folder.target_id.to_string()),
        name: folder.name.clone(),
        index: display_index,
        depth: app.current_playlist_folder_id,
        selected: app.selected_playlist_index == Some(display_index),
      }),
      PlaylistFolderItem::Playlist { index, .. } => {
        app
          .all_playlists
          .get(*index)
          .map(|playlist| GuiPlaylistFolderEntry {
            kind: "playlist".to_string(),
            id: Some(playlist.id.id().to_string()),
            name: playlist.name.clone(),
            index: display_index,
            depth: app.current_playlist_folder_id,
            selected: app.selected_playlist_index == Some(display_index),
          })
      }
    })
    .collect()
}

fn queue_from_app(app: &App) -> Vec<GuiTrack> {
  app
    .queue
    .as_ref()
    .map(|queue| queue.queue.iter().map(GuiTrack::from).collect())
    .unwrap_or_default()
}

fn recently_played_from_app(app: &App) -> Vec<GuiTrack> {
  app
    .recently_played
    .result
    .as_ref()
    .map(|page| page.items.iter().map(play_history_track).collect())
    .unwrap_or_default()
}

fn play_history_track(history: &PlayHistory) -> GuiTrack {
  GuiTrack::from(&history.track)
}

fn page_info<T: DeserializeOwned>(
  page: Option<&Page<T>>,
  page_index: usize,
  page_count: usize,
) -> GuiPageInfo {
  match page {
    Some(page) => GuiPageInfo {
      offset: page.offset,
      limit: page.limit,
      total: page.total,
      page_index,
      page_count,
      has_previous: page.offset > 0 || page.previous.is_some(),
      has_next: page.next.is_some(),
    },
    None => GuiPageInfo {
      page_index,
      page_count,
      ..GuiPageInfo::default()
    },
  }
}

fn cursor_info<T>(
  page: Option<&CursorBasedPage<T>>,
  page_index: usize,
  page_count: usize,
) -> GuiCursorInfo {
  GuiCursorInfo {
    page_index,
    page_count,
    has_previous: page_index > 0,
    has_next: page.and_then(|page| page.next.as_ref()).is_some(),
  }
}

fn search_block_label(block: &SearchResultBlock) -> &'static str {
  match block {
    SearchResultBlock::AlbumSearch => "albums",
    SearchResultBlock::SongSearch => "tracks",
    SearchResultBlock::ArtistSearch => "artists",
    SearchResultBlock::PlaylistSearch => "playlists",
    SearchResultBlock::ShowSearch => "shows",
    SearchResultBlock::Empty => "empty",
  }
}

fn artist_name(artist: &SimplifiedArtist) -> String {
  artist.name.clone()
}

fn non_empty(value: String) -> Option<String> {
  (!value.trim().is_empty()).then_some(value)
}

fn playable_id_from_uri(uri: &str) -> Option<rspotify::model::PlayableId<'static>> {
  if let Ok(track_id) = TrackId::from_uri(uri) {
    return Some(rspotify::model::PlayableId::Track(track_id.into_static()));
  }

  if let Ok(episode_id) = EpisodeId::from_uri(uri) {
    return Some(rspotify::model::PlayableId::Episode(
      episode_id.into_static(),
    ));
  }

  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::NativeTrackInfo;
  use crate::core::user_config::UserConfig;
  use chrono::Utc;
  use rspotify::model::context::{Actions, CurrentPlaybackContext};
  use rspotify::model::{CurrentlyPlayingType, Device, DevicePayload, DeviceType};
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  #[test]
  fn serializes_gui_commands_as_snake_case_messages() {
    let command = GuiCommand::Seek {
      position_ms: 12_345,
    };
    let json = serde_json::to_string(&command).unwrap();

    assert_eq!(json, r#"{"type":"seek","position_ms":12345}"#);
    assert_eq!(serde_json::from_str::<GuiCommand>(&json).unwrap(), command);
  }

  #[test]
  fn snapshots_native_playback_into_gui_types() {
    let mut app = App::default();
    app.is_streaming_active = true;
    app.native_is_playing = Some(true);
    app.song_progress_ms = 42_000;
    app.native_track_info = Some(NativeTrackInfo {
      name: "Quiet Light".to_string(),
      artists_display: "The Lanterns".to_string(),
      album: "Evening".to_string(),
      duration_ms: 180_000,
    });
    app.devices = Some(DevicePayload {
      devices: vec![Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(55),
      }],
    });

    let snapshot = snapshot_app(&app);

    assert_eq!(snapshot.playback.progress_ms, 42_000);
    assert!(snapshot.playback.is_playing);
    assert_eq!(
      snapshot.playback.track.as_ref().unwrap().title,
      "Quiet Light"
    );
    assert_eq!(
      snapshot.playback.track.as_ref().unwrap().artists,
      vec!["The Lanterns".to_string()]
    );
    assert_eq!(snapshot.devices[0].device_type, "computer");
    assert!(serde_json::to_value(snapshot).unwrap().is_object());
  }

  #[test]
  fn dispatches_gui_commands_through_app_channel() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());

    dispatch_gui_command(&mut app, GuiCommand::Seek { position_ms: 777 });

    match rx.try_recv().unwrap() {
      IoEvent::Seek(position_ms) => assert_eq!(position_ms, 777),
      _ => panic!("expected seek event"),
    }
  }

  #[test]
  fn snapshot_includes_gui_navigation_and_library_state() {
    let mut app = App::default();
    app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
    app.load_settings_for_category();

    let snapshot = snapshot_app(&app);

    assert_eq!(snapshot.status.route_id, "settings");
    assert_eq!(
      snapshot.library.options,
      LIBRARY_OPTIONS
        .iter()
        .map(|item| (*item).to_string())
        .collect::<Vec<_>>()
    );
    assert!(!snapshot.settings.items.is_empty());
  }

  #[test]
  fn open_saved_tracks_action_reuses_app_pagination_state_and_dispatches_io() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());

    dispatch_gui_action(&mut app, GuiAction::OpenSavedTracks);

    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
    match rx.try_recv().unwrap() {
      IoEvent::GetCurrentSavedTracks(None) => {}
      _ => panic!("expected saved tracks load"),
    }
  }

  #[test]
  fn toggle_playback_uses_current_playing_state() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.current_playback_context = Some(playback_context(true));

    dispatch_gui_command(&mut app, GuiCommand::TogglePlayback);

    match rx.try_recv().unwrap() {
      IoEvent::PausePlayback => {}
      _ => panic!("expected pause event"),
    }

    app.current_playback_context = Some(playback_context(false));
    dispatch_gui_command(&mut app, GuiCommand::TogglePlayback);

    match rx.try_recv().unwrap() {
      IoEvent::StartPlayback(None, None, None) => {}
      _ => panic!("expected start event"),
    }
  }

  fn playback_context(is_playing: bool) -> CurrentPlaybackContext {
    CurrentPlaybackContext {
      device: Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(55),
      },
      repeat_state: rspotify::model::enums::RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing,
      item: None,
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }
}
