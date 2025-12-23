//! Streaming player implementation using librespot
//!
//! Handles authentication, session management, and audio playback with Spotify Connect.

use anyhow::{anyhow, Context, Result};
use librespot_connect::{ConnectConfig, LoadRequest, Spirc};
use librespot_core::{
  authentication::Credentials,
  cache::Cache,
  config::{DeviceType, SessionConfig},
  session::Session,
  spclient::TransferRequest,
  SpotifyUri,
};
use librespot_oauth::OAuthClientBuilder;
use librespot_playback::{
  audio_backend,
  config::{AudioFormat, PlayerConfig},
  convert::Converter,
  decoder::AudioPacket,
  mixer::{softmixer::SoftMixer, Mixer, MixerConfig},
  player::{Player, PlayerEventChannel},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

#[derive(Default)]
struct NullSink;

impl audio_backend::Open for NullSink {
  fn open(_: Option<String>, _: AudioFormat) -> Self {
    Self
  }
}

impl audio_backend::Sink for NullSink {
  fn write(&mut self, _: AudioPacket, _: &mut Converter) -> audio_backend::SinkResult<()> {
    Ok(())
  }
}

/// OAuth scopes required for streaming (based on spotify-player)
const STREAMING_SCOPES: [&str; 6] = [
  "streaming",
  "user-read-playback-state",
  "user-modify-playback-state",
  "user-read-currently-playing",
  "user-library-read",
  "user-read-private",
];

/// spotify-player's client_id - known to work with librespot
/// Using this because librespot requires a client_id with specific permissions
/// that regular Spotify developer apps may not have.
const SPOTIFY_PLAYER_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

/// spotify-player's redirect_uri - must match what's registered with their client_id
const SPOTIFY_PLAYER_REDIRECT_URI: &str = "http://127.0.0.1:8989/login";

/// Configuration for the streaming player
#[derive(Clone, Debug)]
pub struct StreamingConfig {
  /// Name shown in Spotify Connect device list
  pub device_name: String,
  /// Audio bitrate (96, 160, 320)
  pub bitrate: u16,
  /// Enable audio caching
  pub audio_cache: bool,
  /// Cache directory path
  pub cache_path: Option<PathBuf>,
  /// Initial volume (0-100)
  pub initial_volume: u8,
}

impl Default for StreamingConfig {
  fn default() -> Self {
    Self {
      device_name: "spotatui".to_string(),
      bitrate: 320,
      audio_cache: false,
      cache_path: None,
      initial_volume: 100,
    }
  }
}

/// Player state for tracking playback
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct PlayerState {
  pub is_playing: bool,
  pub track_id: Option<String>,
  pub position_ms: u32,
  pub duration_ms: u32,
  pub volume: u16,
}

/// Streaming player that wraps librespot functionality
///
/// This player registers as a Spotify Connect device and handles
/// native audio playback through the configured audio backend.
pub struct StreamingPlayer {
  #[allow(dead_code)]
  spirc: Spirc,
  #[allow(dead_code)]
  session: Session,
  #[allow(dead_code)]
  player: Arc<Player>,
  #[allow(dead_code)]
  mixer: Arc<SoftMixer>,
  config: StreamingConfig,
  #[allow(dead_code)]
  state: Arc<Mutex<PlayerState>>,
}

#[allow(dead_code)]
impl StreamingPlayer {
  /// Create a new streaming player using librespot-oauth for authentication
  ///
  /// This will check for cached credentials first, and if not found,
  /// will open a browser for OAuth authentication.
  ///
  /// # Arguments
  /// * `client_id` - Spotify application client ID
  /// * `redirect_uri` - OAuth redirect URI (must match Spotify app settings)
  /// * `config` - Streaming configuration options
  pub async fn new(_client_id: &str, _redirect_uri: &str, config: StreamingConfig) -> Result<Self> {
    println!("[streaming] Step 1/8: Setting up cache paths...");

    // Set up cache paths
    let cache_path = config.cache_path.clone().or_else(get_default_cache_path);
    let audio_cache_path = if config.audio_cache {
      cache_path.as_ref().map(|p| p.join("audio"))
    } else {
      None
    };

    // Ensure cache directories exist
    if let Some(ref path) = cache_path {
      std::fs::create_dir_all(path).ok();
    }
    if let Some(ref path) = audio_cache_path {
      std::fs::create_dir_all(path).ok();
    }

    let cache = Cache::new(cache_path.clone(), None, audio_cache_path, None)?;
    println!("[streaming] Step 2/8: Cache initialized");

    // Try to get credentials from cache first
    let credentials = if let Some(cached_creds) = cache.credentials() {
      println!("[streaming] Step 3/8: Using cached streaming credentials");
      cached_creds
    } else {
      // Need to authenticate with librespot-oauth using builder pattern
      println!("[streaming] Step 3/8: Streaming authentication required - opening browser...");

      // Use spotify-player's client_id and redirect_uri for OAuth (works with librespot)
      let client_builder = OAuthClientBuilder::new(
        SPOTIFY_PLAYER_CLIENT_ID,
        SPOTIFY_PLAYER_REDIRECT_URI,
        STREAMING_SCOPES.to_vec(),
      )
      .open_in_browser();

      let oauth_client = client_builder
        .build()
        .map_err(|e| anyhow!("Failed to build OAuth client: {:?}", e))?;

      let token = oauth_client
        .get_access_token()
        .map_err(|e| anyhow!("OAuth authentication failed: {:?}", e))?;

      Credentials::with_access_token(token.access_token)
    };

    println!("[streaming] Step 4/8: Creating session...");

    // Create session configuration using spotify-player's client_id
    let session_config = SessionConfig {
      client_id: SPOTIFY_PLAYER_CLIENT_ID.to_string(),
      ..Default::default()
    };

    // Create session (Spirc will handle connection)
    let session = Session::new(session_config, Some(cache));
    println!(
      "[streaming] Step 4/8: Session created with device_id={}",
      session.device_id()
    );

    // Set up player configuration
    let player_config = PlayerConfig {
      bitrate: match config.bitrate {
        96 => librespot_playback::config::Bitrate::Bitrate96,
        160 => librespot_playback::config::Bitrate::Bitrate160,
        _ => librespot_playback::config::Bitrate::Bitrate320,
      },
      // Enable periodic position updates for real-time playbar progress
      position_update_interval: Some(std::time::Duration::from_secs(1)),
      ..Default::default()
    };

    println!("[streaming] Step 5/8: Creating audio mixer...");

    // Create mixer using SoftMixer directly (like spotify-player does)
    let mixer =
      Arc::new(SoftMixer::open(MixerConfig::default()).context("Failed to open SoftMixer")?);

    // Convert volume from 0-100 to 0-65535
    let volume_u16 = (f64::from(config.initial_volume.min(100)) / 100.0 * 65535.0).round() as u16;
    mixer.set_volume(volume_u16);
    println!(
      "[streaming] Step 5/8: Mixer created, volume set to {}",
      config.initial_volume
    );

    let requested_backend = std::env::var("SPOTATUI_STREAMING_AUDIO_BACKEND").ok();
    let requested_device = std::env::var("SPOTATUI_STREAMING_AUDIO_DEVICE").ok();

    println!("[streaming] Step 6/8: Probing audio backend...");
    if let Some(ref name) = requested_backend {
      println!("[streaming]   Requested backend: {}", name);
    }
    if let Some(ref device) = requested_device {
      println!("[streaming]   Requested device: {}", device);
    }

    // Create audio backend
    let backend = match audio_backend::find(requested_backend.clone()) {
      Some(backend) => {
        println!("[streaming] Step 6/8: Audio backend found");

        // Early probe: try to create the sink NOW to catch crashes before Spirc
        println!("[streaming]   Probing audio device (this may crash on some systems)...");
        let probe_device = requested_device.clone();
        let probe_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
          // Create and immediately drop a test sink
          let _test_sink = backend(probe_device, AudioFormat::default());
          println!("[streaming]   Audio probe successful!");
        }));

        if probe_result.is_err() {
          eprintln!(
            "[streaming] WARNING: Audio backend probe panicked! Falling back to null sink."
          );
          eprintln!(
            "[streaming] TIP: Set SPOTATUI_STREAMING_AUDIO_BACKEND=pipe to disable audio output."
          );
          None
        } else {
          Some(backend)
        }
      }
      None => {
        if let Some(name) = requested_backend.as_deref() {
          return Err(anyhow!(
            "Unknown audio backend '{}'. Available backends: {}",
            name,
            audio_backend::BACKENDS
              .iter()
              .map(|(n, _)| *n)
              .collect::<Vec<_>>()
              .join(", ")
          ));
        }
        eprintln!(
          "[streaming] No audio backend available; falling back to a null sink (no audio). \
This build may have been compiled without an audio backend feature."
        );
        None
      }
    };

    println!("[streaming] Step 7/8: Creating librespot Player...");

    // Create player
    let player = Player::new(
      player_config,
      session.clone(),
      mixer.get_soft_volume(),
      move || {
        let result = std::panic::catch_unwind(|| match backend {
          Some(factory) => factory(requested_device.clone(), AudioFormat::default()),
          None => Box::new(NullSink::default()),
        });
        match result {
          Ok(sink) => sink,
          Err(_) => {
            eprintln!(
              "[streaming] Failed to initialize audio output backend; falling back to a null sink (no audio). \
Set SPOTATUI_STREAMING_AUDIO_DEVICE to select an output device, or SPOTATUI_STREAMING_AUDIO_BACKEND to select a backend."
            );
            Box::new(NullSink::default())
          }
        }
      },
    );
    println!("[streaming] Step 7/8: Player created");

    // Create Connect configuration
    let connect_config = ConnectConfig {
      name: config.device_name.clone(),
      device_type: DeviceType::Computer,
      initial_volume: volume_u16,
      is_group: false,
      disable_volume: false,
      volume_steps: 64,
    };

    println!("[streaming] Step 8/8: Initializing Spirc (Spotify Connect)...");
    println!("[streaming]   This connects to Spotify servers and may take a few seconds...");

    let init_timeout_secs = std::env::var("SPOTATUI_STREAMING_INIT_TIMEOUT_SECS")
      .ok()
      .and_then(|v| v.parse::<u64>().ok())
      .filter(|&v| v > 0)
      .unwrap_or(30);

    // Create Spirc (Spotify Connect controller)
    let spirc_new = Spirc::new(
      connect_config,
      session.clone(),
      credentials,
      player.clone(),
      mixer.clone(),
    );

    let (spirc, spirc_task) = match timeout(Duration::from_secs(init_timeout_secs), spirc_new).await
    {
      Ok(Ok(result)) => {
        println!("[streaming] Spirc initialized successfully");
        result
      }
      Ok(Err(e)) => {
        println!("[streaming] Spirc creation error: {:?}", e);
        return Err(anyhow!("Failed to create Spirc: {:?}", e));
      }
      Err(_) => {
        println!(
          "[streaming] Spirc initialization timed out after {}s",
          init_timeout_secs
        );
        return Err(anyhow!(
          "Spirc initialization timed out after {}s (set SPOTATUI_STREAMING_INIT_TIMEOUT_SECS to adjust)",
          init_timeout_secs
        ));
      }
    };

    // Spawn the Spirc task to run in the background
    tokio::spawn(spirc_task);

    println!("[streaming] Streaming connection established!");

    Ok(Self {
      spirc,
      session,
      player,
      mixer,
      config,
      state: Arc::new(Mutex::new(PlayerState::default())),
    })
  }

  /// Get the device name
  pub fn device_name(&self) -> &str {
    &self.config.device_name
  }

  /// Check if the session is connected
  pub fn is_connected(&self) -> bool {
    !self.player.is_invalid()
  }

  /// Play a track by its Spotify URI (e.g., "spotify:track:xxxx")
  pub async fn play_uri(&self, uri: &str) -> Result<()> {
    let spotify_uri =
      SpotifyUri::from_uri(uri).map_err(|e| anyhow!("Invalid Spotify URI '{}': {:?}", uri, e))?;

    self.player.load(spotify_uri, true, 0);

    let mut state = self.state.lock().await;
    state.is_playing = true;
    state.track_id = Some(uri.to_string());
    state.position_ms = 0;

    Ok(())
  }

  /// Load a new playback context/tracks via Spotify Connect (Spirc).
  ///
  /// Prefer this over `player.load()` when you want Connect state (queue, context)
  /// to stay consistent.
  pub fn load(&self, request: LoadRequest) -> Result<()> {
    self
      .spirc
      .load(request)
      .map_err(|e| anyhow!("Failed to load playback via Spirc: {:?}", e))
  }

  /// Play a track by its Spotify ID (will be converted to URI)
  pub async fn play_track(&self, track_id: &str) -> Result<()> {
    let uri = format!("spotify:track:{}", track_id);
    self.play_uri(&uri).await
  }

  /// Pause playback
  pub fn pause(&self) {
    // Prefer going through Spirc so Connect state stays consistent.
    let _ = self.spirc.pause();
    self.player.pause();
  }

  /// Resume playback
  pub fn play(&self) {
    // Prefer going through Spirc so Connect state stays consistent.
    // Also call the underlying player directly as a best-effort fallback.
    let _ = self.spirc.play();
    self.player.play();
  }

  /// Stop playback
  pub fn stop(&self) {
    self.player.stop();
  }

  /// Skip to the next track
  pub fn next(&self) {
    let _ = self.spirc.next();
  }

  /// Skip to the previous track  
  pub fn prev(&self) {
    let _ = self.spirc.prev();
  }

  /// Seek to a position in the current track (in milliseconds)
  pub fn seek(&self, position_ms: u32) {
    self.player.seek(position_ms);
  }

  /// Toggle shuffle mode via the underlying Spotify Connect session
  pub fn set_shuffle(&self, shuffle: bool) -> Result<()> {
    Ok(self.spirc.shuffle(shuffle)?)
  }

  /// Set the volume (0-100)
  pub fn set_volume(&self, volume: u8) {
    let volume_u16 = (f64::from(volume.min(100)) / 100.0 * 65535.0).round() as u16;
    self.mixer.set_volume(volume_u16);
  }

  /// Get the current volume (0-100)
  pub fn get_volume(&self) -> u8 {
    let volume_u16 = self.mixer.volume();
    ((volume_u16 as f64 / 65535.0) * 100.0).round() as u8
  }

  /// Get the current player state
  pub async fn get_state(&self) -> PlayerState {
    self.state.lock().await.clone()
  }

  /// Check if the player is invalid (e.g., session disconnected)
  pub fn is_invalid(&self) -> bool {
    self.player.is_invalid()
  }

  /// Activate the device (make it the active playback device)
  pub fn activate(&self) {
    let _ = self.spirc.activate();
  }

  /// Transfer playback to this device via Spotify Connect.
  ///
  /// This is the most reliable way to become the active device; `activate()`
  /// can be a no-op when we're not currently active.
  pub fn transfer(&self, request: Option<TransferRequest>) -> Result<()> {
    self
      .spirc
      .transfer(request)
      .map_err(|e| anyhow!("Failed to transfer playback via Spirc: {:?}", e))
  }

  /// Shutdown the player
  pub fn shutdown(&self) {
    let _ = self.spirc.shutdown();
  }

  /// Get a channel to receive player events (track changes, play/pause, seek, etc.)
  pub fn get_event_channel(&self) -> PlayerEventChannel {
    self.player.get_player_event_channel()
  }
}

// Re-export PlayerEvent for use in other modules
pub use librespot_playback::player::PlayerEvent;

/// Helper to get the default cache path for streaming
pub fn get_default_cache_path() -> Option<PathBuf> {
  dirs::home_dir().map(|home| {
    home
      .join(".config")
      .join("spotatui")
      .join("streaming_cache")
  })
}
