//! Streaming player implementation using librespot
//!
//! Handles authentication, session management, and audio playback with Spotify Connect.

use crate::infra::spotify::credentials::{
  clear_cached_streaming_credentials, get_default_cache_path, streaming_proxy_from_env,
  SPOTIFY_PLAYER_CLIENT_ID,
};
use anyhow::{anyhow, Context, Result};
use librespot_connect::{ConnectConfig, LoadRequest, SavedPlaybackState, Spirc};
use librespot_core::{
  authentication::Credentials,
  cache::Cache,
  config::{DeviceType, SessionConfig},
  error::ErrorKind,
  session::Session,
  spclient::TransferRequest,
  SpotifyUri,
};
use librespot_playback::{
  audio_backend,
  config::{AudioFormat, PlayerConfig},
  convert::Converter,
  decoder::AudioPacket,
  mixer::{softmixer::SoftMixer, Mixer, MixerConfig},
  player::{Player, PlayerEventChannel},
};
use log::{error, info, warn};
use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

const FAST_SESSION_RECONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PLAY_AFTER_TRANSFER_WINDOW: Duration = Duration::from_secs(15);

type SpircTaskHandle = tokio::task::JoinHandle<Option<SavedPlaybackState>>;

/// One-shot play for the paused load a resume-after-handoff transfer ends in (#693).
#[derive(Debug, Default)]
struct PlayAfterTransfer(Option<Instant>);

impl PlayAfterTransfer {
  fn arm(&mut self, now: Instant) {
    self.0 = Some(now);
  }

  fn cancel(&mut self) {
    self.0 = None;
  }

  fn take(&mut self, now: Instant) -> bool {
    self
      .0
      .take()
      .is_some_and(|at| now.saturating_duration_since(at) < PLAY_AFTER_TRANSFER_WINDOW)
  }
}

struct ActiveConnection {
  spirc: Spirc,
  session: Session,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpircCommandMode {
  Connected,
  Reconnecting,
  Failed,
  Shutdown,
}

enum DeferredPlayerCommand {
  Load(LoadRequest),
  Play,
  Pause,
  Stop,
  Next,
  Prev,
  Seek(u32),
  Shuffle(bool),
  Repeat(bool),
  RepeatTrack(bool),
  Activate,
  Transfer(Option<TransferRequest>),
  DirectLoad(SpotifyUri, bool),
}

impl DeferredPlayerCommand {
  fn apply(self, spirc: &Spirc, player: &Player) -> std::result::Result<(), librespot_core::Error> {
    match self {
      Self::Load(request) => spirc.load(request),
      // Play/Pause go to both the Player and Spirc: Spirc's handlers no-op
      // whenever its play_status doesn't cover the current audio (any track
      // direct-loaded via `player.load` leaves it Stopped, since Spirc drops
      // events with a play_request_id it didn't issue), while the direct Player
      // command always acts on whatever is actually loaded. The double-send is
      // safe in both directions: the Player ignores commands invalid for its
      // state, and Spirc re-syncs its status from the resulting Paused/Playing
      // events.
      Self::Play => {
        player.play();
        spirc.play()
      }
      Self::Pause => {
        player.pause();
        spirc.pause()
      }
      Self::Stop => {
        player.stop();
        Ok(())
      }
      Self::Next => spirc.next(),
      Self::Prev => spirc.prev(),
      Self::Seek(position_ms) => {
        player.seek(position_ms);
        Ok(())
      }
      Self::Shuffle(shuffle) => spirc.shuffle(shuffle),
      Self::Repeat(repeat) => spirc.repeat(repeat),
      Self::RepeatTrack(repeat) => spirc.repeat_track(repeat),
      Self::Activate => spirc.activate(),
      Self::Transfer(request) => spirc.transfer(request),
      Self::DirectLoad(uri, start_playing) => {
        player.load(uri, start_playing, 0);
        Ok(())
      }
    }
  }

  fn apply_to_buffered_player(&self, player: &Player) {
    match self {
      Self::Play => player.play(),
      Self::Pause => player.pause(),
      Self::Stop => player.stop(),
      Self::Seek(position_ms) => player.seek(*position_ms),
      _ => {}
    }
  }
}

struct SpircCommandRouter {
  mode: SpircCommandMode,
  deferred: Vec<DeferredPlayerCommand>,
}

impl Default for SpircCommandRouter {
  fn default() -> Self {
    Self {
      mode: SpircCommandMode::Connected,
      deferred: Vec::new(),
    }
  }
}

/// Runtime state of the embedded Spotify Connect connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamingConnectionState {
  Connected { generation: u64 },
  Reconnecting { generation: u64 },
  Failed { generation: u64 },
  Shutdown,
}

impl StreamingConnectionState {
  fn accepts_commands(self) -> bool {
    matches!(self, Self::Connected { .. } | Self::Reconnecting { .. })
  }
}

struct SpircSupervisorContext {
  spirc_task: SpircTaskHandle,
  connection: Arc<RwLock<ActiveConnection>>,
  player: Arc<Player>,
  mixer: Arc<SoftMixer>,
  cache: Cache,
  credentials: Credentials,
  session_config: SessionConfig,
  connect_config: ConnectConfig,
  shutdown_requested: Arc<AtomicBool>,
  shutdown_rx: tokio::sync::watch::Receiver<bool>,
  connection_state_tx: tokio::sync::watch::Sender<StreamingConnectionState>,
  command_router: Arc<std::sync::Mutex<SpircCommandRouter>>,
}

fn spawn_spirc_supervisor(ctx: SpircSupervisorContext) {
  tokio::spawn(run_spirc_supervisor(ctx));
}

async fn run_spirc_supervisor(mut ctx: SpircSupervisorContext) {
  let mut generation = 0_u64;

  loop {
    let saved_state = tokio::select! {
      result = &mut ctx.spirc_task => match result {
        Ok(saved) => saved,
        Err(error) => {
          warn!("Spirc task failed: {error}");
          let mut router = ctx
            .command_router
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          router.mode = SpircCommandMode::Failed;
          router.deferred.clear();
          drop(router);
          let _ = ctx
            .connection_state_tx
            .send(StreamingConnectionState::Failed { generation });
          return;
        }
      },
      changed = ctx.shutdown_rx.changed() => {
        if changed.is_err() || *ctx.shutdown_rx.borrow() {
          let mut router = ctx
            .command_router
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          router.mode = SpircCommandMode::Shutdown;
          router.deferred.clear();
          let _ = ctx.connection_state_tx.send(StreamingConnectionState::Shutdown);
          return;
        }
        continue;
      }
    };

    if ctx.shutdown_requested.load(Ordering::Relaxed) {
      let mut router = ctx
        .command_router
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      router.mode = SpircCommandMode::Shutdown;
      router.deferred.clear();
      let _ = ctx
        .connection_state_tx
        .send(StreamingConnectionState::Shutdown);
      return;
    }

    let Some(saved_state) = saved_state else {
      warn!("Spirc task exited without recoverable playback state");
      let mut router = ctx
        .command_router
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      router.mode = SpircCommandMode::Failed;
      router.deferred.clear();
      drop(router);
      let _ = ctx
        .connection_state_tx
        .send(StreamingConnectionState::Failed { generation });
      return;
    };

    generation = generation.wrapping_add(1);
    info!(
      "Spotify access-point session lost; starting fast reconnect generation {}",
      generation
    );
    ctx
      .command_router
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .mode = SpircCommandMode::Reconnecting;
    let _ = ctx
      .connection_state_tx
      .send(StreamingConnectionState::Reconnecting { generation });

    let replacement_session = Session::new(ctx.session_config.clone(), Some(ctx.cache.clone()));
    // Match librespot's standalone supervisor: the existing Player starts using
    // the replacement Session before the restored Spirc begins processing events.
    ctx.player.set_session(replacement_session.clone());
    ctx
      .connection
      .write()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .session = replacement_session.clone();

    let reconnect = Spirc::with_saved_state(
      ctx.connect_config.clone(),
      replacement_session.clone(),
      ctx.credentials.clone(),
      Arc::clone(&ctx.player),
      ctx.mixer.clone(),
      Some(saved_state),
    );

    let reconnect_result = tokio::select! {
      result = timeout(FAST_SESSION_RECONNECT_TIMEOUT, reconnect) => Some(result),
      changed = ctx.shutdown_rx.changed() => {
        if changed.is_err() || *ctx.shutdown_rx.borrow() {
          replacement_session.shutdown();
          let mut router = ctx
            .command_router
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          router.mode = SpircCommandMode::Shutdown;
          router.deferred.clear();
          let _ = ctx.connection_state_tx.send(StreamingConnectionState::Shutdown);
          return;
        }
        None
      }
    };

    let Some(reconnect_result) = reconnect_result else {
      continue;
    };

    match reconnect_result {
      Ok(Ok((replacement_spirc, replacement_task))) => {
        {
          let mut router = ctx
            .command_router
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          // shutdown() may have run while the reconnect was completing; it only
          // reached the previous Spirc. Installing the replacement would revive
          // the router and leak the new session as a ghost Connect device
          // (#297), so tear it down instead. Holding the router lock for the
          // install keeps shutdown() ordered before or after the whole swap.
          if router.mode == SpircCommandMode::Shutdown {
            drop(router);
            let _ = replacement_spirc.shutdown();
            replacement_session.shutdown();
            // Drive the task so it processes the shutdown command and exits.
            tokio::spawn(replacement_task);
            return;
          }
          {
            let mut connection = ctx
              .connection
              .write()
              .unwrap_or_else(std::sync::PoisonError::into_inner);
            *connection = ActiveConnection {
              spirc: replacement_spirc,
              session: replacement_session,
            };
          }
          router.mode = SpircCommandMode::Connected;
          let commands = std::mem::take(&mut router.deferred);
          let connection = ctx
            .connection
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
          for command in commands {
            if let Err(error) = command.apply(&connection.spirc, &ctx.player) {
              warn!("deferred native command failed after fast reconnect: {error:?}");
            }
          }
        }
        ctx.spirc_task = tokio::spawn(replacement_task);
        let _ = ctx
          .connection_state_tx
          .send(StreamingConnectionState::Connected { generation });
        info!(
          "Spotify access-point session fast reconnect generation {} established",
          generation
        );
      }
      Ok(Err(error)) => {
        replacement_session.shutdown();
        let mut router = ctx
          .command_router
          .lock()
          .unwrap_or_else(std::sync::PoisonError::into_inner);
        router.mode = SpircCommandMode::Failed;
        router.deferred.clear();
        drop(router);
        warn!(
          "Spotify access-point fast reconnect generation {} failed: {:?}",
          generation, error
        );
        let _ = ctx
          .connection_state_tx
          .send(StreamingConnectionState::Failed { generation });
        return;
      }
      Err(_) => {
        replacement_session.shutdown();
        let mut router = ctx
          .command_router
          .lock()
          .unwrap_or_else(std::sync::PoisonError::into_inner);
        router.mode = SpircCommandMode::Failed;
        router.deferred.clear();
        drop(router);
        warn!(
          "Spotify access-point fast reconnect generation {} timed out after {}s",
          generation,
          FAST_SESSION_RECONNECT_TIMEOUT.as_secs()
        );
        let _ = ctx
          .connection_state_tx
          .send(StreamingConnectionState::Failed { generation });
        return;
      }
    }
  }
}

struct RecoveringSink {
  inner: Option<Box<dyn audio_backend::Sink>>,
  make_sink: Box<dyn Fn() -> Box<dyn audio_backend::Sink>>,
  error: Arc<std::sync::Mutex<Option<String>>>,
  failed: bool,
}

impl RecoveringSink {
  fn new<F>(make_sink: F, error: Arc<std::sync::Mutex<Option<String>>>) -> Self
  where
    F: Fn() -> Box<dyn audio_backend::Sink> + 'static,
  {
    Self {
      inner: None,
      make_sink: Box::new(make_sink),
      error,
      failed: false,
    }
  }

  fn payload_to_string(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
      Ok(s) => *s,
      Err(payload) => match payload.downcast::<&'static str>() {
        Ok(s) => s.to_string(),
        Err(_) => "unknown panic payload".to_string(),
      },
    }
  }

  fn panic_to_sink_error(
    context: &'static str,
    payload: Box<dyn Any + Send>,
  ) -> audio_backend::SinkError {
    let msg = Self::payload_to_string(payload);
    audio_backend::SinkError::StateChange(format!("Audio backend panic in {context}: {msg}"))
  }

  fn create_sink(&mut self) -> audio_backend::SinkResult<()> {
    if self.inner.is_some() {
      return Ok(());
    }

    let make_sink = &self.make_sink;
    match catch_unwind(AssertUnwindSafe(make_sink)) {
      Ok(sink) => {
        self.inner = Some(sink);
        Ok(())
      }
      Err(payload) => {
        let err = Self::panic_to_sink_error("open", payload);
        error!("{err}");
        Err(err)
      }
    }
  }

  fn with_inner<T, F>(&mut self, context: &'static str, op: F) -> audio_backend::SinkResult<T>
  where
    F: FnOnce(&mut dyn audio_backend::Sink) -> audio_backend::SinkResult<T>,
  {
    self.create_sink()?;

    let Some(sink) = self.inner.as_mut() else {
      return Err(audio_backend::SinkError::NotConnected(
        "Audio sink unavailable".to_string(),
      ));
    };

    match catch_unwind(AssertUnwindSafe(|| op(sink.as_mut()))) {
      Ok(Ok(value)) => Ok(value),
      Ok(Err(err)) => {
        warn!("Audio backend {context} error: {err}");
        self.inner = None;
        Err(err)
      }
      Err(payload) => {
        let err = Self::panic_to_sink_error(context, payload);
        error!("{err}");
        self.inner = None;
        Err(err)
      }
    }
  }
}

impl audio_backend::Sink for RecoveringSink {
  fn start(&mut self) -> audio_backend::SinkResult<()> {
    self.failed = false;
    match self.with_inner("start", |sink| sink.start()) {
      Ok(()) => {
        // A working sink supersedes any stale error recorded before there was
        // a player to surface it; leaving it would report a live failure later.
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = None;
      }
      Err(err) => {
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(err.to_string());
        self.failed = true;
      }
    }
    // A sink error must not invalidate librespot's player thread. Keep consuming
    // packets silently so the TUI can surface the failure and remain usable.
    Ok(())
  }

  fn stop(&mut self) -> audio_backend::SinkResult<()> {
    if self.inner.is_none() {
      return Ok(());
    }

    // Avoid process exits in librespot when sink.stop() errors.
    let _ = self.with_inner("stop", |sink| sink.stop());
    self.inner = None;
    Ok(())
  }

  fn write(
    &mut self,
    packet: AudioPacket,
    converter: &mut Converter,
  ) -> audio_backend::SinkResult<()> {
    // Unlike start(), write errors must propagate: a blocking sink.write() is
    // librespot's only backpressure, and its packet-error path pauses playback
    // without exiting the process.
    if self.failed {
      return Err(audio_backend::SinkError::NotConnected(
        "Audio sink unavailable".to_string(),
      ));
    }
    if let Err(err) = self.with_inner("write", |sink| sink.write(packet, converter)) {
      *self.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(err.to_string());
      self.failed = true;
      return Err(err);
    }
    Ok(())
  }
}

/// Configuration for the streaming player
#[derive(Clone, Debug)]
pub struct StreamingConfig {
  /// Name shown in Spotify Connect device list
  pub device_name: String,
  /// Audio bitrate (96, 160, 320)
  pub bitrate: u16,
  /// Enable audio caching
  pub audio_cache: bool,
  /// Play related songs when a list ends
  pub autoplay: bool,
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
      autoplay: true,
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
  connection: Arc<RwLock<ActiveConnection>>,
  command_router: Arc<std::sync::Mutex<SpircCommandRouter>>,
  #[allow(dead_code)]
  player: Arc<Player>,
  #[allow(dead_code)]
  mixer: Arc<SoftMixer>,
  config: StreamingConfig,
  #[allow(dead_code)]
  state: Arc<Mutex<PlayerState>>,
  shutdown_requested: Arc<AtomicBool>,
  shutdown_tx: tokio::sync::watch::Sender<bool>,
  connection_state_tx: tokio::sync::watch::Sender<StreamingConnectionState>,
  connection_state_rx: tokio::sync::watch::Receiver<StreamingConnectionState>,
  audio_backend_error: Arc<std::sync::Mutex<Option<String>>>,
  play_after_transfer: std::sync::Mutex<PlayAfterTransfer>,
}

#[allow(dead_code)]
impl StreamingPlayer {
  /// Get the current librespot session (for API calls like rootlist).
  pub fn session(&self) -> Session {
    self.with_connection(|connection| connection.session.clone())
  }

  /// A streaming player from the cached Spotify login; never opens a browser.
  pub async fn new_cache_only(config: StreamingConfig) -> Result<Self> {
    // Set up cache paths
    let cache_path = config.cache_path.clone().or_else(get_default_cache_path);
    let audio_cache_path = if config.audio_cache {
      cache_path.as_ref().map(|p| p.join("audio"))
    } else {
      None
    };

    // Ensure cache directories exist
    if let Some(ref path) = cache_path {
      crate::core::paths::ensure_private_dir(path)?;
    }
    if let Some(ref path) = audio_cache_path {
      std::fs::create_dir_all(path).ok();
    }

    let cache = Cache::new(cache_path.clone(), None, audio_cache_path, None)?;

    let credentials = cache
      .credentials()
      .ok_or_else(|| anyhow!("No cached Spotify login; log in to Spotify first"))?;

    // Create session configuration using spotify-player's client_id
    let mut session_config = SessionConfig {
      client_id: SPOTIFY_PLAYER_CLIENT_ID.to_string(),
      // The account has no autoplay attribute: Spotify keeps autoplay per device.
      autoplay: Some(config.autoplay),
      ..Default::default()
    };
    // Reuse a persisted device id so every launch and recovery registers as the
    // same Spotify Connect device instead of accumulating ghost entries (#297).
    if let Some(device_id) = get_or_create_device_id(cache_path.as_deref())? {
      session_config.device_id = device_id;
    }
    // librespot dials the access point over a raw TCP socket, so it needs the
    // proxy handed to it explicitly; the Web API half of the app gets it from
    // the environment through reqwest.
    //
    // This covers the access point only. librespot's `DealerManager::start`
    // passes `None` as the dealer websocket's proxy, so on a network that
    // blocks direct outbound connections entirely the dealer still cannot
    // connect and the Connect device will not register. Fixing that needs a
    // change in the librespot fork.
    if let Some(proxy) = streaming_proxy_from_env() {
      info!("native streaming will connect through proxy {proxy}");
      session_config.proxy = Some(proxy);
    }

    // Create session (Spirc will handle connection)
    let session = Session::new(session_config.clone(), Some(cache.clone()));

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

    // Create mixer using SoftMixer directly (like spotify-player does)
    let mixer =
      Arc::new(SoftMixer::open(MixerConfig::default()).context("Failed to open SoftMixer")?);

    // Convert volume from 0-100 to 0-65535
    let volume_u16 = (f64::from(config.initial_volume.min(100)) / 100.0 * 65535.0).round() as u16;
    mixer.set_volume(volume_u16);

    let requested_backend = std::env::var("SPOTATUI_STREAMING_AUDIO_BACKEND").ok();
    let requested_device = std::env::var("SPOTATUI_STREAMING_AUDIO_DEVICE").ok();

    // Create audio backend
    let backend =
      audio_backend::find(requested_backend.clone()).ok_or_else(|| match requested_backend {
        Some(name) => anyhow!(
          "Unknown audio backend '{}'. Available backends: {}",
          name,
          audio_backend::BACKENDS
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(", ")
        ),
        None => anyhow!("No audio backend available"),
      })?;

    // Create player
    let audio_backend_error = Arc::new(std::sync::Mutex::new(None));
    let sink_error = Arc::clone(&audio_backend_error);
    let player = Player::new(
      player_config,
      session.clone(),
      mixer.get_soft_volume(),
      move || {
        Box::new(RecoveringSink::new(
          move || backend(requested_device.clone(), AudioFormat::default()),
          Arc::clone(&sink_error),
        ))
      },
    );

    // Create Connect configuration
    let connect_config = ConnectConfig {
      name: config.device_name.clone(),
      device_type: DeviceType::Computer,
      initial_volume: volume_u16,
      is_group: false,
      disable_volume: false,
      volume_steps: 64,
      emit_set_queue_events: false,
    };

    info!("Initializing Spirc with device_id={}", session.device_id());

    let init_timeout = spirc_init_timeout();

    // Create Spirc (Spotify Connect controller)
    let spirc_new = Spirc::new(
      connect_config.clone(),
      session.clone(),
      credentials.clone(),
      player.clone(),
      mixer.clone(),
    );
    let (spirc, spirc_task) = match timeout(init_timeout, spirc_new).await {
      Ok(Ok(result)) => result,
      Ok(Err(e)) => {
        // Only discard cached credentials when Spotify actually rejected them.
        // A transient failure (network down, service unavailable) must NOT wipe
        // valid credentials and force a browser re-auth next launch — same
        // reasoning as the timeout arm below.
        if matches!(
          e.kind,
          ErrorKind::Unauthenticated | ErrorKind::PermissionDenied
        ) {
          clear_cached_streaming_credentials(&cache_path);
        }
        warn!("Spirc creation error: {:?}", e);
        return Err(anyhow!("Failed to create Spirc: {:?}", e));
      }
      Err(_) => {
        // Timeout means the network was slow, not that credentials are bad.
        // Do NOT clear credentials, they may be valid for the next startup.
        // Streaming is skipped for this session; main.rs falls back to Web API.
        return Err(anyhow!(
          "Spirc initialization timed out after {}s. Streaming skipped for this session. \
             Set SPOTATUI_STREAMING_INIT_TIMEOUT_SECS to adjust.",
          init_timeout.as_secs()
        ));
      }
    };

    let connection = Arc::new(RwLock::new(ActiveConnection { spirc, session }));
    let command_router = Arc::new(std::sync::Mutex::new(SpircCommandRouter::default()));
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let initial_connection_state = StreamingConnectionState::Connected { generation: 0 };
    let (connection_state_tx, connection_state_rx) =
      tokio::sync::watch::channel(initial_connection_state);

    spawn_spirc_supervisor(SpircSupervisorContext {
      spirc_task: tokio::spawn(spirc_task),
      connection: Arc::clone(&connection),
      player: Arc::clone(&player),
      mixer: Arc::clone(&mixer),
      cache,
      credentials,
      session_config,
      connect_config,
      shutdown_requested: Arc::clone(&shutdown_requested),
      shutdown_rx,
      connection_state_tx: connection_state_tx.clone(),
      command_router: Arc::clone(&command_router),
    });

    info!("Streaming connection established!");

    Ok(Self {
      connection,
      command_router,
      player,
      mixer,
      config,
      state: Arc::new(Mutex::new(PlayerState::default())),
      shutdown_requested,
      shutdown_tx,
      connection_state_tx,
      connection_state_rx,
      audio_backend_error,
      play_after_transfer: std::sync::Mutex::new(PlayAfterTransfer::default()),
    })
  }

  fn with_connection<T>(&self, f: impl FnOnce(&ActiveConnection) -> T) -> T {
    let connection = self
      .connection
      .read()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&connection)
  }

  fn with_spirc<T>(&self, f: impl FnOnce(&Spirc) -> T) -> T {
    self.with_connection(|connection| f(&connection.spirc))
  }

  fn route_command(&self, command: DeferredPlayerCommand) -> Result<()> {
    let mut router = self
      .command_router
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
    match router.mode {
      SpircCommandMode::Connected => self
        .with_connection(|connection| command.apply(&connection.spirc, &self.player))
        .map_err(|error| anyhow!("Native playback command failed: {error:?}")),
      SpircCommandMode::Reconnecting => {
        command.apply_to_buffered_player(&self.player);
        router.deferred.push(command);
        Ok(())
      }
      SpircCommandMode::Failed => Err(anyhow!("Native streaming connection failed")),
      SpircCommandMode::Shutdown => Err(anyhow!("Native streaming player is shut down")),
    }
  }

  /// Get the device name
  pub fn device_name(&self) -> &str {
    &self.config.device_name
  }

  /// Get the Spotify Connect device id for this session.
  pub fn device_id(&self) -> String {
    self.with_connection(|connection| connection.session.device_id().to_string())
  }

  /// Check if the session is connected
  pub fn is_connected(&self) -> bool {
    matches!(
      self.connection_state(),
      StreamingConnectionState::Connected { .. }
    ) && self.with_connection(|connection| !connection.session.is_invalid())
      && !self.player.is_invalid()
  }

  pub fn is_recovering(&self) -> bool {
    matches!(
      self.connection_state(),
      StreamingConnectionState::Reconnecting { .. }
    )
  }

  /// Whether the native backend can accept commands now or queue them for its
  /// in-place reconnect.
  pub fn is_available(&self) -> bool {
    self.connection_state().accepts_commands()
  }

  pub fn connection_state(&self) -> StreamingConnectionState {
    *self.connection_state_rx.borrow()
  }

  pub fn take_audio_backend_error(&self) -> Option<String> {
    self
      .audio_backend_error
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .take()
  }

  /// Play a track by its Spotify URI (e.g., "spotify:track:xxxx")
  pub async fn play_uri(&self, uri: &str) -> Result<()> {
    let spotify_uri =
      SpotifyUri::from_uri(uri).map_err(|e| anyhow!("Invalid Spotify URI '{}': {:?}", uri, e))?;

    self.route_command(DeferredPlayerCommand::DirectLoad(spotify_uri, true))?;

    let mut state = self.state.lock().await;
    state.is_playing = true;
    state.track_id = Some(uri.to_string());
    state.position_ms = 0;

    Ok(())
  }

  /// Load a track by URI without starting playback. Used to restore a paused
  /// queue slot after recovery so the user's pause survives the rebuild.
  pub async fn load_uri_paused(&self, uri: &str) -> Result<()> {
    let spotify_uri =
      SpotifyUri::from_uri(uri).map_err(|e| anyhow!("Invalid Spotify URI '{}': {:?}", uri, e))?;

    self.route_command(DeferredPlayerCommand::DirectLoad(spotify_uri, false))?;

    let mut state = self.state.lock().await;
    state.is_playing = false;
    state.track_id = Some(uri.to_string());
    state.position_ms = 0;

    Ok(())
  }

  /// Load a new playback context/tracks via Spotify Connect (Spirc).
  ///
  /// Prefer this over `player.load()` when you want Connect state (queue, context)
  /// to stay consistent.
  pub fn load(&self, request: LoadRequest) -> Result<()> {
    self.route_command(DeferredPlayerCommand::Load(request))
  }

  /// Play a track by its Spotify ID (will be converted to URI)
  pub async fn play_track(&self, track_id: &str) -> Result<()> {
    let uri = format!("spotify:track:{}", track_id);
    self.play_uri(&uri).await
  }

  /// Hint the player to prefetch a track's audio so a subsequent
  /// [`play_uri`](Self::play_uri) starts near-instantly. Used by the native
  /// queue to warm its next Spotify item, matching the preloading Spirc does
  /// within a context. Best-effort: an unparseable URI is silently ignored.
  pub fn preload_uri(&self, uri: &str) {
    if let Ok(spotify_uri) = SpotifyUri::from_uri(uri) {
      self.player.preload(spotify_uri);
    }
  }

  /// Pause playback
  pub fn pause(&self) {
    self.cancel_play_after_transfer();
    if let Err(error) = self.route_command(DeferredPlayerCommand::Pause) {
      warn!("native pause failed: {error}");
    }
  }

  /// Resume playback
  pub fn play(&self) {
    if let Err(error) = self.route_command(DeferredPlayerCommand::Play) {
      warn!("native play failed: {error}");
    }
  }

  /// Stop playback
  pub fn stop(&self) {
    if let Err(error) = self.route_command(DeferredPlayerCommand::Stop) {
      warn!("native stop failed: {error}");
    }
  }

  /// Skip to the next track
  pub fn next(&self) {
    if let Err(error) = self.route_command(DeferredPlayerCommand::Next) {
      warn!("native next failed: {error}");
    }
  }

  /// Skip to the previous track
  pub fn prev(&self) {
    if let Err(error) = self.route_command(DeferredPlayerCommand::Prev) {
      warn!("native previous failed: {error}");
    }
  }

  /// Seek to a position in the current track (in milliseconds)
  pub fn seek(&self, position_ms: u32) {
    if let Err(error) = self.route_command(DeferredPlayerCommand::Seek(position_ms)) {
      warn!("native seek failed: {error}");
    }
  }

  /// Toggle shuffle mode via the underlying Spotify Connect session
  pub fn set_shuffle(&self, shuffle: bool) -> Result<()> {
    self.route_command(DeferredPlayerCommand::Shuffle(shuffle))
  }

  /// Set repeat mode via the underlying Spotify Connect session
  /// Handles cycling between Off -> Context -> Track -> Off
  pub fn set_repeat(&self, current_state: rspotify::model::enums::RepeatState) -> Result<()> {
    use rspotify::model::enums::RepeatState;

    match current_state {
      RepeatState::Off => {
        self.route_command(DeferredPlayerCommand::Repeat(true))?;
        self.route_command(DeferredPlayerCommand::RepeatTrack(false))?;
      }
      RepeatState::Context => {
        self.route_command(DeferredPlayerCommand::RepeatTrack(true))?;
      }
      RepeatState::Track => {
        self.route_command(DeferredPlayerCommand::Repeat(false))?;
        self.route_command(DeferredPlayerCommand::RepeatTrack(false))?;
      }
    }
    Ok(())
  }

  /// Set repeat mode directly to a specific state (for MPRIS)
  pub fn set_repeat_mode(&self, target_state: rspotify::model::enums::RepeatState) -> Result<()> {
    use rspotify::model::enums::RepeatState;

    match target_state {
      RepeatState::Off => {
        self.route_command(DeferredPlayerCommand::Repeat(false))?;
        self.route_command(DeferredPlayerCommand::RepeatTrack(false))?;
      }
      RepeatState::Context => {
        self.route_command(DeferredPlayerCommand::Repeat(true))?;
        self.route_command(DeferredPlayerCommand::RepeatTrack(false))?;
      }
      RepeatState::Track => {
        self.route_command(DeferredPlayerCommand::Repeat(true))?;
        self.route_command(DeferredPlayerCommand::RepeatTrack(true))?;
      }
    }
    Ok(())
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
    !self.is_connected()
  }

  /// Activate the device (make it the active playback device)
  pub fn activate(&self) {
    // Not fatal (transfer() is the reliable route), but a failure here used to
    // vanish entirely, hiding zombie sessions from the logs.
    if let Err(error) = self.route_command(DeferredPlayerCommand::Activate) {
      warn!("spirc activate failed: {error}");
    }
  }

  /// Transfer playback to this device via Spotify Connect.
  ///
  /// This is the most reliable way to become the active device; `activate()`
  /// can be a no-op when we're not currently active.
  pub fn transfer(&self, request: Option<TransferRequest>) -> Result<()> {
    self.route_command(DeferredPlayerCommand::Transfer(request))
  }

  /// Pull the last playback over and play it once loaded; a pause cancels it.
  /// False when the transfer could not be sent.
  pub fn transfer_and_play(&self) -> bool {
    self.play_after_transfer_slot().arm(Instant::now());
    if let Err(error) = self.transfer(None) {
      warn!("native transfer after handoff failed: {error}");
      self.cancel_play_after_transfer();
      return false;
    }
    self.activate();
    true
  }

  /// One-shot, for the paused load the transfer ends in.
  pub fn take_play_after_transfer(&self) -> bool {
    self.play_after_transfer_slot().take(Instant::now())
  }

  /// Audio already playing needs no deferred play; a later remote pause must stick.
  pub fn cancel_play_after_transfer(&self) {
    self.play_after_transfer_slot().cancel();
  }

  fn play_after_transfer_slot(&self) -> std::sync::MutexGuard<'_, PlayAfterTransfer> {
    self
      .play_after_transfer
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  /// Shutdown the player
  pub fn shutdown(&self) {
    self.shutdown_requested.store(true, Ordering::Relaxed);
    {
      let mut router = self
        .command_router
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      router.mode = SpircCommandMode::Shutdown;
      router.deferred.clear();
    }
    let _ = self
      .connection_state_tx
      .send(StreamingConnectionState::Shutdown);
    let _ = self.shutdown_tx.send(true);
    let _ = self.with_spirc(Spirc::shutdown);
  }

  pub fn connection_state_receiver(
    &self,
  ) -> tokio::sync::watch::Receiver<StreamingConnectionState> {
    self.connection_state_rx.clone()
  }

  /// Get a channel to receive player events (track changes, play/pause, seek, etc.)
  pub fn get_event_channel(&self) -> PlayerEventChannel {
    self.player.get_player_event_channel()
  }
}

impl Drop for StreamingPlayer {
  fn drop(&mut self) {
    // Backstop: stop the spirc when the last reference drops so a replaced
    // player can't linger as a ghost Connect device (#297). Idempotent.
    self.shutdown_requested.store(true, Ordering::Relaxed);
    {
      let mut router = self
        .command_router
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      router.mode = SpircCommandMode::Shutdown;
      router.deferred.clear();
    }
    let _ = self
      .connection_state_tx
      .send(StreamingConnectionState::Shutdown);
    let _ = self.shutdown_tx.send(true);
    let _ = self.with_spirc(Spirc::shutdown);
  }
}

// Re-export player events and their disconnect reason for use in other modules.
pub use librespot_playback::player::{PlayerEvent, SessionDisconnectReason};

/// The Spirc handshake bound: `SPOTATUI_STREAMING_INIT_TIMEOUT_SECS`, else 30 s.
fn spirc_init_timeout() -> Duration {
  let secs = std::env::var("SPOTATUI_STREAMING_INIT_TIMEOUT_SECS")
    .ok()
    .and_then(|v| v.parse::<u64>().ok())
    .filter(|&v| v > 0)
    .unwrap_or(30);
  Duration::from_secs(secs)
}

/// The bound on a whole player build: the Spirc handshake plus 15 s for the
/// setup around it.
pub fn player_build_timeout() -> Duration {
  spirc_init_timeout().saturating_add(Duration::from_secs(15))
}

/// Stable Connect device id, persisted in the streaming cache dir so every
/// launch and every in-app recovery registers as the same device (#297).
fn get_or_create_device_id(cache_path: Option<&Path>) -> Result<Option<String>> {
  let Some(cache_path) = cache_path else {
    return Ok(None);
  };
  crate::core::paths::ensure_private_dir(cache_path)?;
  let id_file = cache_path.join("device_id");
  match std::fs::read_to_string(&id_file) {
    Ok(existing) => {
      let trimmed = existing.trim();
      if !trimmed.is_empty() {
        return Ok(Some(trimmed.to_string()));
      }
    }
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
    Err(e) => return Err(e).with_context(|| format!("reading {}", id_file.display())),
  }
  let id = new_device_id_string();
  std::fs::write(&id_file, &id).with_context(|| format!("writing {}", id_file.display()))?;
  Ok(Some(id))
}

/// Hyphenated UUID-v4-shaped string, matching librespot's default device id format.
fn new_device_id_string() -> String {
  use rand::Rng;
  let mut b = [0u8; 16];
  rand::rng().fill_bytes(&mut b);
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  format!(
    "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
    b[0],
    b[1],
    b[2],
    b[3],
    b[4],
    b[5],
    b[6],
    b[7],
    b[8],
    b[9],
    b[10],
    b[11],
    b[12],
    b[13],
    b[14],
    b[15]
  )
}

#[cfg(test)]
mod tests {
  use super::{
    get_or_create_device_id, new_device_id_string, PlayAfterTransfer, RecoveringSink,
    StreamingConnectionState, PLAY_AFTER_TRANSFER_WINDOW,
  };
  use crate::infra::spotify::credentials::{
    migrate_legacy_streaming_cache_if_unclaimed, proxy_url_from_candidates, redact_raw_proxy,
  };
  use librespot_playback::{audio_backend, convert::Converter, decoder::AudioPacket};
  use std::sync::Arc;
  use std::time::Duration;

  struct StubSink {
    start_result: fn() -> audio_backend::SinkResult<()>,
  }

  impl audio_backend::Sink for StubSink {
    fn start(&mut self) -> audio_backend::SinkResult<()> {
      (self.start_result)()
    }

    fn stop(&mut self) -> audio_backend::SinkResult<()> {
      Ok(())
    }

    fn write(
      &mut self,
      _packet: AudioPacket,
      _converter: &mut Converter,
    ) -> audio_backend::SinkResult<()> {
      Ok(())
    }
  }

  fn failing_start() -> audio_backend::SinkResult<()> {
    Err(audio_backend::SinkError::StateChange(
      "stub backend failure".to_string(),
    ))
  }

  fn stub_recovering_sink(
    start_result: fn() -> audio_backend::SinkResult<()>,
  ) -> (RecoveringSink, Arc<std::sync::Mutex<Option<String>>>) {
    let error = Arc::new(std::sync::Mutex::new(None));
    let sink = RecoveringSink::new(
      move || Box::new(StubSink { start_result }) as Box<dyn audio_backend::Sink>,
      Arc::clone(&error),
    );
    (sink, error)
  }

  #[test]
  fn failing_backend_start_returns_ok_and_records_error() {
    use audio_backend::Sink;

    let (mut sink, error) = stub_recovering_sink(failing_start);

    // The regression guard for #384: a failing backend must not propagate the
    // error into librespot's player thread.
    sink.start().expect("start() must fail softly");
    assert!(sink.failed);
    let recorded = error.lock().unwrap().take();
    assert!(recorded.unwrap().contains("stub backend failure"));
  }

  #[test]
  fn successful_start_clears_stale_backend_error() {
    use audio_backend::Sink;

    let (mut sink, error) = stub_recovering_sink(|| Ok(()));
    *error.lock().unwrap() = Some("stale error from a previous sink".to_string());

    sink.start().expect("start() should succeed");
    assert!(!sink.failed);
    assert!(error.lock().unwrap().is_none());
  }

  #[test]
  fn streaming_proxy_prefers_the_first_non_empty_candidate() {
    let proxy = proxy_url_from_candidates([None, Some("  "), Some("http://proxy.example:3128")])
      .expect("a well-formed http proxy should be accepted");
    assert_eq!(proxy.as_str(), "http://proxy.example:3128/");
  }

  #[test]
  fn streaming_proxy_is_none_without_candidates() {
    assert!(proxy_url_from_candidates([None, Some(""), Some("   ")]).is_none());
  }

  #[test]
  fn streaming_proxy_rejects_a_socks_proxy() {
    assert!(proxy_url_from_candidates([Some("socks5://proxy.example:1080")]).is_none());
  }

  /// librespot's `CONNECT` is plaintext and it never wraps the proxy leg in
  /// TLS, so an `https` proxy URL would resolve to port 443 and speak HTTP at
  /// a port expecting a TLS client hello.
  #[test]
  fn streaming_proxy_rejects_an_https_proxy() {
    assert!(proxy_url_from_candidates([Some("https://proxy.example:3128")]).is_none());
    assert!(proxy_url_from_candidates([Some("https://proxy.example")]).is_none());
  }

  #[test]
  fn streaming_proxy_rejects_an_unparseable_value() {
    assert!(proxy_url_from_candidates([Some("proxy.example:3128")]).is_none());
  }

  /// librespot logs the URL it is handed and never sends a
  /// `Proxy-Authorization` header, so credentials are dropped before it sees
  /// them rather than being carried into the log for no benefit.
  #[test]
  fn streaming_proxy_strips_credentials() {
    let proxy = proxy_url_from_candidates([Some("http://user:s3cret@proxy.example:3128")]).unwrap();
    assert_eq!(proxy.as_str(), "http://proxy.example:3128/");
    assert!(proxy.username().is_empty());
    assert_eq!(proxy.password(), None);
    assert!(!proxy.as_str().contains("s3cret"));
  }

  #[test]
  fn raw_proxy_credentials_are_redacted_for_logging() {
    assert_eq!(
      redact_raw_proxy("socks5://user:s3cret@proxy.example:1080"),
      "socks5://<redacted>@proxy.example:1080"
    );
    assert_eq!(
      redact_raw_proxy("http://user:s3cret@proxy.example:3128/path"),
      "http://<redacted>@proxy.example:3128/path"
    );
    // Nothing to hide, nothing changed - the value stays diagnosable.
    assert_eq!(
      redact_raw_proxy("http://proxy.example:3128"),
      "http://proxy.example:3128"
    );
    assert_eq!(redact_raw_proxy("proxy.example:3128"), "proxy.example:3128");
  }

  /// A `@` in the path must not be mistaken for userinfo and swallow the host.
  #[test]
  fn raw_proxy_redaction_only_touches_the_authority() {
    assert_eq!(
      redact_raw_proxy("http://proxy.example:3128/a@b"),
      "http://proxy.example:3128/a@b"
    );
  }

  #[test]
  fn reconnecting_connection_remains_command_capable() {
    assert!(StreamingConnectionState::Connected { generation: 0 }.accepts_commands());
    assert!(StreamingConnectionState::Reconnecting { generation: 1 }.accepts_commands());
    assert!(!StreamingConnectionState::Failed { generation: 1 }.accepts_commands());
    assert!(!StreamingConnectionState::Shutdown.accepts_commands());
  }

  #[test]
  fn device_id_string_is_uuid_v4_shaped() {
    let id = new_device_id_string();
    assert_eq!(id.len(), 36);
    for (i, c) in id.char_indices() {
      match i {
        8 | 13 | 18 | 23 => assert_eq!(c, '-'),
        14 => assert_eq!(c, '4'),
        19 => assert!(matches!(c, '8' | '9' | 'a' | 'b')),
        _ => assert!(c.is_ascii_hexdigit()),
      }
    }
  }

  #[test]
  fn device_id_persists_across_calls() {
    let dir = std::env::temp_dir().join(format!("spotatui_device_id_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let first = get_or_create_device_id(Some(&dir)).unwrap().unwrap();
    let second = get_or_create_device_id(Some(&dir)).unwrap().unwrap();
    assert_eq!(first, second);
    assert_eq!(
      std::fs::read_to_string(dir.join("device_id"))
        .unwrap()
        .trim(),
      first
    );

    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn device_id_none_without_cache_path() {
    assert!(get_or_create_device_id(None).unwrap().is_none());
  }

  #[test]
  fn device_id_errors_when_cache_path_setup_fails() {
    let dir = tempfile::tempdir().unwrap();
    let cache_path = dir.path().join("not_a_directory");
    std::fs::write(&cache_path, "not a directory").unwrap();

    let err = get_or_create_device_id(Some(&cache_path)).unwrap_err();
    assert!(err.to_string().contains("creating"));
  }

  #[cfg(unix)]
  #[test]
  fn device_id_errors_when_persistence_write_fails() {
    let dir = tempfile::tempdir().unwrap();
    let cache_path = dir.path().join("cache");
    std::fs::create_dir_all(&cache_path).unwrap();
    std::os::unix::fs::symlink(
      dir.path().join("missing_parent").join("device_id"),
      cache_path.join("device_id"),
    )
    .unwrap();

    let err = get_or_create_device_id(Some(&cache_path)).unwrap_err();
    assert!(err.to_string().contains("writing"));
  }

  #[test]
  fn legacy_streaming_cache_migration_preserves_connect_device_id() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("config");
    let cache_path = dir.path().join("cache").join("streaming_cache");
    let legacy_cache_path = config_dir.join("streaming_cache");
    std::fs::create_dir_all(&legacy_cache_path).unwrap();
    std::fs::write(legacy_cache_path.join("device_id"), "legacy-device-id\n").unwrap();
    std::fs::write(
      legacy_cache_path.join("credentials.json"),
      "legacy credentials",
    )
    .unwrap();

    assert!(migrate_legacy_streaming_cache_if_unclaimed(&config_dir, &cache_path).unwrap());

    assert!(!legacy_cache_path.exists());
    assert_eq!(
      get_or_create_device_id(Some(&cache_path)).unwrap().unwrap(),
      "legacy-device-id"
    );
    assert_eq!(
      std::fs::read_to_string(cache_path.join("credentials.json")).unwrap(),
      "legacy credentials"
    );
  }

  #[test]
  fn play_after_transfer_fires_once() {
    let mut pending = PlayAfterTransfer::default();
    let armed_at = std::time::Instant::now();
    assert!(!pending.take(armed_at));

    pending.arm(armed_at);
    assert!(pending.take(armed_at + Duration::from_secs(1)));
    assert!(!pending.take(armed_at + Duration::from_secs(2)));
  }

  #[test]
  fn play_after_transfer_expires() {
    let mut pending = PlayAfterTransfer::default();
    let armed_at = std::time::Instant::now();

    pending.arm(armed_at);
    assert!(!pending.take(armed_at + PLAY_AFTER_TRANSFER_WINDOW));
  }

  #[test]
  fn a_pause_cancels_play_after_transfer() {
    let mut pending = PlayAfterTransfer::default();
    let armed_at = std::time::Instant::now();

    pending.arm(armed_at);
    pending.cancel();
    assert!(!pending.take(armed_at));
  }
}
