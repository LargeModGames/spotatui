use super::{IoEvent, Network};
#[cfg(feature = "streaming")]
use crate::core::{
  app::{App, NativePlaybackOrigin},
  config::ClientConfig,
};
#[cfg(feature = "streaming")]
use crate::infra::player::{select_native, PlaybackBackend};
use anyhow::anyhow;
use chrono::TimeDelta;
#[cfg(feature = "streaming")]
use log::info;
use reqwest::Method;
#[cfg(feature = "streaming")]
use rspotify::model::device::DevicePayload;
use rspotify::model::{
  context::CurrentUserQueue,
  enums::RepeatState,
  idtypes::{PlayContextId, PlayableId},
  PlayableItem,
};
use rspotify::prelude::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

#[cfg(feature = "streaming")]
use librespot_connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
#[cfg(feature = "streaming")]
use std::sync::Arc;

const MAX_API_PLAYBACK_URIS: usize = 100;

#[cfg(feature = "streaming")]
const MAX_NATIVE_IDLE_RECOVERY_ATTEMPTS: u8 = 2;

#[cfg(feature = "streaming")]
const NATIVE_IDLE_RECOVERY_RETRY_INTERVAL: Duration = Duration::from_secs(5);

pub trait PlaybackNetwork {
  async fn get_current_playback(&mut self);
  async fn start_playback(
    &mut self,
    context_id: Option<PlayContextId<'static>>,
    uris: Option<Vec<PlayableId<'static>>>,
    offset: Option<usize>,
  );
  async fn pause_playback(&mut self);
  async fn next_track(&mut self);
  async fn previous_track(&mut self);
  async fn force_previous_track(&mut self);
  async fn seek(&mut self, position_ms: u32);
  async fn shuffle(&mut self, shuffle_state: bool);
  async fn repeat(&mut self, repeat_state: RepeatState);
  async fn change_volume(&mut self, volume: u8);
  async fn transfert_playback_to_device(&mut self, device_id: String, persist_device_id: bool);
  #[cfg(feature = "streaming")]
  async fn auto_select_streaming_device(&mut self, device_name: String, persist_device_id: bool);
  async fn ensure_playback_continues(&mut self, previous_track_id: String);
  /// Resume a native-Spotify context suspended under the native queue, targeting
  /// the resume track via an offset URI. Falls back to playing just the track
  /// when there is no context, and to a "Queue finished" status when neither is
  /// known.
  async fn resume_spotify_context(
    &mut self,
    context_uri: Option<String>,
    resume_track_uri: Option<String>,
  );
  #[allow(dead_code)]
  async fn add_item_to_queue(&mut self, item: PlayableId<'static>);
  async fn get_queue(&mut self);
  /// Fetch, decode and store the current track's cover art (off the `App` lock).
  #[cfg(feature = "cover-art")]
  async fn fetch_cover_art(&mut self, request: crate::tui::cover_art::CoverArtRequest);
}

fn trim_api_playback_uris(
  track_uris: Vec<PlayableId<'static>>,
  offset: Option<usize>,
) -> (Vec<PlayableId<'static>>, Option<usize>) {
  if track_uris.len() <= MAX_API_PLAYBACK_URIS {
    return (track_uris, offset);
  }

  let selected_index = offset.unwrap_or(0).min(track_uris.len().saturating_sub(1));
  let preferred_history = MAX_API_PLAYBACK_URIS / 5;
  let mut start = selected_index.saturating_sub(preferred_history);
  let end = (start + MAX_API_PLAYBACK_URIS).min(track_uris.len());

  if end - start < MAX_API_PLAYBACK_URIS {
    start = end.saturating_sub(MAX_API_PLAYBACK_URIS);
  }

  // Spotify rejects oversized URI payloads, so URI-list playback is capped
  // to a window that still contains the selected track.
  let trimmed_uris = track_uris[start..end]
    .iter()
    .map(PlayableId::clone_static)
    .collect::<Vec<_>>();

  (trimmed_uris, Some(selected_index - start))
}

fn api_playback_offset_json(
  context_uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Option<Value> {
  if let Some(first_uri) = context_uris.and_then(|uris| uris.first()) {
    return Some(json!({ "uri": first_uri.uri() }));
  }

  offset.map(|index| json!({ "position": index }))
}

fn api_playback_body(
  context_id: Option<&PlayContextId<'static>>,
  uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Option<Value> {
  match (context_id, uris) {
    (Some(context), track_uris) => {
      let mut body = json!({ "context_uri": context.uri() });
      if let Some(offset) = api_playback_offset_json(track_uris, offset) {
        body["offset"] = offset;
      }
      Some(body)
    }
    (None, Some(track_uris)) => {
      let mut body = json!({
        "uris": track_uris.iter().map(|uri| uri.uri()).collect::<Vec<_>>()
      });
      if let Some(offset) = api_playback_offset_json(None, offset) {
        body["offset"] = offset;
      }
      Some(body)
    }
    (None, None) => None,
  }
}

fn playable_item_id(item: &PlayableItem) -> Option<String> {
  match item {
    PlayableItem::Track(track) => track.id.as_ref().map(|id| id.id().to_string()),
    PlayableItem::Episode(episode) => Some(episode.id.id().to_string()),
    PlayableItem::Unknown(_) => None,
  }
}

fn playable_item_name(item: &PlayableItem) -> Option<&str> {
  match item {
    PlayableItem::Track(track) => Some(&track.name),
    PlayableItem::Episode(episode) => Some(&episode.name),
    PlayableItem::Unknown(_) => None,
  }
}

#[cfg(feature = "streaming")]
#[derive(Debug, PartialEq, Eq)]
enum NativePlaybackRoute {
  ContextApi { device_id: String },
  NativeLoad,
}

#[cfg(feature = "streaming")]
#[derive(Clone, Copy, Debug, Default)]
struct NativeActivationContext {
  player_connected: bool,
  current_device_id_present: bool,
  current_device_is_confirmed_native: bool,
  current_device_name_matches_native: bool,
  native_has_fresh_activity: bool,
  saved_device_matches_native: bool,
  saved_external_confirmed_available: bool,
}

#[cfg(feature = "streaming")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeDevicePreferenceUpdate {
  Persist,
  KeepExistingPreference,
}

#[cfg(feature = "streaming")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NativeIdleRecoveryPhase {
  #[default]
  Armed,
  Idle {
    attempts: u8,
    last_attempt: Instant,
  },
}

#[cfg(feature = "streaming")]
#[derive(Debug, Default)]
pub(super) struct NativeIdleRecoveryState {
  player_instance: Option<usize>,
  phase: NativeIdleRecoveryPhase,
}

#[cfg(feature = "streaming")]
impl NativeIdleRecoveryState {
  fn observe_player_instance(&mut self, player_instance: Option<usize>) {
    if self.player_instance != player_instance {
      self.player_instance = player_instance;
      self.phase = NativeIdleRecoveryPhase::Armed;
    }
  }

  fn observe_available_playback(&mut self) {
    self.phase = NativeIdleRecoveryPhase::Armed;
  }

  fn should_attempt_idle_recovery(&mut self, now: Instant) -> bool {
    match self.phase {
      NativeIdleRecoveryPhase::Armed => {
        self.phase = NativeIdleRecoveryPhase::Idle {
          attempts: 1,
          last_attempt: now,
        };
        true
      }
      NativeIdleRecoveryPhase::Idle {
        attempts,
        last_attempt,
      } if attempts < MAX_NATIVE_IDLE_RECOVERY_ATTEMPTS
        && now.duration_since(last_attempt) >= NATIVE_IDLE_RECOVERY_RETRY_INTERVAL =>
      {
        self.phase = NativeIdleRecoveryPhase::Idle {
          attempts: attempts + 1,
          last_attempt: now,
        };
        true
      }
      NativeIdleRecoveryPhase::Idle { .. } => false,
    }
  }

  fn settle_current_episode(&mut self, now: Instant) {
    self.phase = NativeIdleRecoveryPhase::Idle {
      attempts: MAX_NATIVE_IDLE_RECOVERY_ATTEMPTS,
      last_attempt: now,
    };
  }
}

#[cfg(feature = "streaming")]
fn should_activate_native_for_playback(context: NativeActivationContext) -> bool {
  if !context.player_connected {
    return false;
  }

  let current_device_is_stale_native_name =
    context.current_device_name_matches_native && !context.native_has_fresh_activity;
  let current_device_is_usable_external = context.current_device_id_present
    && !context.current_device_is_confirmed_native
    && !current_device_is_stale_native_name;

  if current_device_is_usable_external {
    return false;
  }

  if context.saved_device_matches_native {
    return true;
  }

  !context.saved_external_confirmed_available
}

#[cfg(feature = "streaming")]
fn native_device_preference_update(
  saved_device_id: Option<&str>,
  explicit_persist: bool,
  saved_device_matches_native: bool,
) -> NativeDevicePreferenceUpdate {
  if explicit_persist || saved_device_id.is_none() || saved_device_matches_native {
    NativeDevicePreferenceUpdate::Persist
  } else {
    NativeDevicePreferenceUpdate::KeepExistingPreference
  }
}

#[cfg(feature = "streaming")]
fn native_idle_device_preference_update(
  saved_device_id: Option<&str>,
  saved_device_matches_native: bool,
) -> Option<NativeDevicePreferenceUpdate> {
  let update = native_device_preference_update(saved_device_id, false, saved_device_matches_native);
  (update != NativeDevicePreferenceUpdate::KeepExistingPreference).then_some(update)
}

#[cfg(feature = "streaming")]
fn saved_device_matches_native_player(
  saved_device_id: Option<&str>,
  native_device_id: Option<&str>,
  devices: Option<&DevicePayload>,
  native_device_name: &str,
) -> bool {
  saved_device_id.is_some_and(|saved| {
    native_device_id == Some(saved)
      || devices.is_some_and(|payload| {
        payload.devices.iter().any(|device| {
          device.id.as_deref() == Some(saved)
            && device.name.eq_ignore_ascii_case(native_device_name)
        })
      })
  })
}

#[cfg(feature = "streaming")]
fn persist_native_device_id_if_needed(
  client_config: &mut ClientConfig,
  app: &mut App,
  native_device_id: &str,
  update: NativeDevicePreferenceUpdate,
) {
  if update == NativeDevicePreferenceUpdate::KeepExistingPreference {
    return;
  }

  if client_config.device_id.as_deref() == Some(native_device_id) {
    return;
  }

  if let Err(e) = client_config.set_device_id(native_device_id.to_string()) {
    app.handle_error(anyhow!(e));
  }
}

#[cfg(feature = "streaming")]
fn reconcile_native_idle_device_if_preferred(
  client_config: &mut ClientConfig,
  app: &mut App,
  player: &crate::infra::player::StreamingPlayer,
  recovery: &mut NativeIdleRecoveryState,
) {
  if !player.is_connected() {
    return;
  }

  let native_device_id = player.device_id();
  let saved_device_matches_native = saved_device_matches_native_player(
    client_config.device_id.as_deref(),
    Some(&native_device_id),
    app.devices.as_ref(),
    player.device_name(),
  );
  let Some(native_preference_update) = native_idle_device_preference_update(
    client_config.device_id.as_deref(),
    saved_device_matches_native,
  ) else {
    return;
  };

  let now = Instant::now();
  if recovery.should_attempt_idle_recovery(now) {
    let _ = player.transfer(None);
    player.activate();
    app.last_device_activation = Some(now);
  }

  app.mark_native_streaming_device_available(
    native_device_id.clone(),
    player.device_name().to_string(),
    player.get_volume(),
  );
  persist_native_device_id_if_needed(
    client_config,
    app,
    &native_device_id,
    native_preference_update,
  );
}

#[cfg(feature = "streaming")]
fn spotify_payload_confirms_native_device(payload: &DevicePayload, native_device_id: &str) -> bool {
  payload
    .devices
    .iter()
    .any(|device| device.id.as_deref() == Some(native_device_id))
}

#[cfg(feature = "streaming")]
fn is_no_active_device_error(e: &anyhow::Error) -> bool {
  let text = e.to_string().to_ascii_lowercase();
  text.contains("no_active_device") || text.contains("no active device")
}

fn api_confirms_native_info_is_current(
  native_name: &str,
  item: &PlayableItem,
  last_track_id: Option<&str>,
) -> bool {
  if playable_item_name(item) == Some(native_name) {
    return true;
  }

  playable_item_id(item)
    .as_deref()
    .is_some_and(|api_id| Some(api_id) == last_track_id)
}

#[cfg(feature = "streaming")]
#[derive(Clone, Copy, Debug)]
struct StaleApiItemContext {
  native_info_present: bool,
  api_item_present: bool,
  api_confirms_native_info: bool,
  native_track_id_present: bool,
  api_item_matches_native_track: bool,
  native_streaming_was_active: bool,
  native_activation_pending: bool,
  api_device_is_native: bool,
}

#[cfg(feature = "streaming")]
fn stale_api_item_should_preserve_native_context(context: StaleApiItemContext) -> bool {
  context.api_item_present
    && !context.api_confirms_native_info
    && (context.native_info_present
      || (context.native_track_id_present && !context.api_item_matches_native_track))
    && (context.native_streaming_was_active
      || context.native_activation_pending
      || context.api_device_is_native)
}

/// Get the currently active streaming player, if any.
/// Note: This logic is duplicated in `main.rs` as `active_streaming_player()`.
/// Both are identical; the difference is input type (Network vs. App Arc).
/// A future refactor could consolidate to a shared location like `src/core/app.rs`.
#[cfg(feature = "streaming")]
async fn current_streaming_player(
  network: &Network,
) -> Option<Arc<crate::infra::player::StreamingPlayer>> {
  let app = network.app.lock().await;
  app.streaming_player.clone()
}

#[cfg(feature = "streaming")]
async fn is_native_streaming_active_for_playback(network: &Network) -> bool {
  let app = network.app.lock().await;
  let streaming_player = app.streaming_player.clone();
  let player_connected = streaming_player.as_ref().is_some_and(|p| p.is_connected());

  if !player_connected {
    return false;
  }

  let native_device_name = streaming_player
    .as_ref()
    .map(|p| p.device_name().to_lowercase());

  // If no context yet (e.g., at startup), use the app state flag which is
  // set when the native streaming device is activated/selected.
  let Some(ref ctx) = app.current_playback_context else {
    return app.is_streaming_active;
  };

  // First, check if the current playback device matches the native streaming device ID
  if let (Some(current_id), Some(native_id)) =
    (ctx.device.id.as_ref(), app.native_device_id.as_ref())
  {
    if current_id == native_id {
      return true;
    }
  }

  // Fallback: strict name match (case-insensitive), but only while we have
  // fresh native activity or a recent explicit activation. After a recovery,
  // Spotify can keep returning the old "spotatui" device while the new native
  // player is connected but stopped/not active.
  if let Some(native_name) = native_device_name.as_ref() {
    let current_device_name = ctx.device.name.to_lowercase();
    if current_device_name == native_name.as_str() && app.has_fresh_native_activity() {
      return true;
    }
  }

  // The user explicitly selected the native device very recently; honor that
  // intent even when the API context hasn't caught up yet (the brief pre-poll
  // window). `is_streaming_active` is re-derived from real Spotify state on the
  // next poll, so this cannot reintroduce the #254 device hijack. (#282)
  if app.is_streaming_active
    && app
      .last_device_activation
      .is_some_and(|instant| instant.elapsed() < Duration::from_secs(5))
  {
    return true;
  }

  // No match - not the active device
  false
}

/// Resolve the transport backend for a *symmetric* playback operation
/// (pause/next/previous/seek/shuffle/repeat/volume).
///
/// Native streaming is chosen only when it is the active device *and* a player
/// handle is present; otherwise the operation falls through to the Spotify Web
/// API. This wraps the existing selection logic without changing it: the two
/// awaited lookups happen in the same order as the original inline
/// `if is_native_streaming_active_for_playback(..).await { if let Some(player) =
/// current_streaming_player(..).await { .. } }` guard, so behaviour is identical.
#[cfg(feature = "streaming")]
async fn symmetric_playback_backend(network: &Network) -> PlaybackBackend {
  let is_native_active = is_native_streaming_active_for_playback(network).await;
  // Only look up the player when native is active, mirroring the original
  // short-circuit (`if is_native { if let Some(player) ... }`) so the Connect
  // path does not take the app lock the inline code never acquired.
  let player = if is_native_active {
    current_streaming_player(network).await
  } else {
    None
  };
  if select_native(is_native_active, player.is_some()) {
    // `select_native` guarantees `player` is `Some` here.
    PlaybackBackend::Native(player.expect("player present when native selected"))
  } else {
    PlaybackBackend::Connect
  }
}

/// Resolve the transport backend for `start_playback`.
///
/// Unlike the symmetric operations, native streaming is selected when it is
/// already active *or* when the activation heuristics say it should be
/// activated for this playback. The `||` short-circuit and "fetch the player
/// only when native applies" ordering are preserved exactly; a true predicate
/// with a missing player still falls through to the Web API.
#[cfg(feature = "streaming")]
async fn start_playback_backend(network: &Network) -> PlaybackBackend {
  let is_native_active = is_native_streaming_active_for_playback(network).await
    || should_activate_native_streaming_for_playback(network).await;
  let player = if is_native_active {
    current_streaming_player(network).await
  } else {
    None
  };
  if select_native(is_native_active, player.is_some()) {
    PlaybackBackend::Native(player.expect("player present when native selected"))
  } else {
    PlaybackBackend::Connect
  }
}

/// Resolve the transport backend for `transfert_playback_to_device`.
///
/// The native player is selected only when the *transfer target* `device_id`
/// refers to the native streaming device, identified either by matching a
/// cached device whose name equals the native device name, or by matching the
/// recorded `native_device_id`. This mirrors the previous inline
/// `is_native_transfer` computation exactly; an unrelated target falls through
/// to the Web API transfer.
#[cfg(feature = "streaming")]
async fn transfer_playback_backend(network: &Network, device_id: &str) -> PlaybackBackend {
  let player = current_streaming_player(network).await;
  let is_native_transfer = if let Some(ref player) = player {
    let native_name = player.device_name().to_lowercase();
    let app = network.app.lock().await;
    let matches_cached_device = app.devices.as_ref().is_some_and(|payload| {
      payload
        .devices
        .iter()
        .any(|d| d.id.as_deref() == Some(device_id) && d.name.to_lowercase() == native_name)
    });
    matches_cached_device || app.native_device_id.as_deref() == Some(device_id)
  } else {
    false
  };

  if select_native(is_native_transfer, player.is_some()) {
    PlaybackBackend::Native(player.expect("player present when native selected"))
  } else {
    PlaybackBackend::Connect
  }
}

#[cfg(feature = "streaming")]
async fn should_activate_native_streaming_for_playback(network: &Network) -> bool {
  let saved_device_id = network.client_config.device_id.as_deref();
  let app = network.app.lock().await;
  let Some(player) = app.streaming_player.as_ref() else {
    return false;
  };

  if !player.is_connected() {
    return false;
  }

  let native_name = player.device_name();
  let native_device_id = app.native_device_id.as_deref();
  let current_device = app.current_playback_context.as_ref().map(|ctx| &ctx.device);
  let current_device_id = current_device.and_then(|device| device.id.as_deref());
  let current_device_name_matches_native =
    current_device.is_some_and(|device| device.name.eq_ignore_ascii_case(native_name));
  let native_has_fresh_activity = app.has_fresh_native_activity();

  let saved_device_matches_native = saved_device_matches_native_player(
    saved_device_id,
    native_device_id,
    app.devices.as_ref(),
    native_name,
  );

  let saved_external_confirmed_available = saved_device_id.is_some_and(|saved| {
    app.devices.as_ref().is_some_and(|payload| {
      payload.devices.iter().any(|device| {
        device.id.as_deref() == Some(saved) && !device.name.eq_ignore_ascii_case(native_name)
      })
    })
  });

  should_activate_native_for_playback(NativeActivationContext {
    player_connected: true,
    current_device_id_present: current_device_id.is_some(),
    current_device_is_confirmed_native: native_device_id
      .is_some_and(|id| current_device_id == Some(id)),
    current_device_name_matches_native,
    native_has_fresh_activity,
    saved_device_matches_native,
    saved_external_confirmed_available,
  })
}

#[cfg(feature = "streaming")]
async fn request_native_streaming_recovery_if_disconnected(network: &Network) -> bool {
  let mut app = network.app.lock().await;
  app.request_native_streaming_recovery_if_disconnected(true)
}

#[cfg(feature = "streaming")]
async fn requested_native_playback_origin(
  network: &Network,
  context_id: &Option<PlayContextId<'static>>,
  uris: &Option<Vec<PlayableId<'static>>>,
) -> NativePlaybackOrigin {
  if context_id.is_some() {
    return NativePlaybackOrigin::Context;
  }

  if uris.is_some() {
    return NativePlaybackOrigin::RawList;
  }

  let app = network.app.lock().await;
  if let Some(origin) = app.native_playback_origin {
    return origin;
  }

  if app
    .current_playback_context
    .as_ref()
    .and_then(|ctx| ctx.context.as_ref())
    .is_some()
  {
    NativePlaybackOrigin::Context
  } else {
    NativePlaybackOrigin::RawList
  }
}

#[cfg(feature = "streaming")]
async fn resolve_native_playback_route(
  network: &Network,
  context_id: &Option<PlayContextId<'static>>,
) -> NativePlaybackRoute {
  if context_id.is_none() {
    return NativePlaybackRoute::NativeLoad;
  }

  let app = network.app.lock().await;
  match app.native_device_id.clone() {
    Some(device_id) => NativePlaybackRoute::ContextApi { device_id },
    None => NativePlaybackRoute::NativeLoad,
  }
}

#[cfg(feature = "streaming")]
fn native_load_request(
  context_id: Option<PlayContextId<'static>>,
  uris: Option<Vec<PlayableId<'static>>>,
  offset: Option<usize>,
) -> Option<LoadRequest> {
  let mut options = LoadRequestOptions {
    start_playing: true,
    seek_to: 0,
    context_options: None,
    playing_track: None,
  };

  match (context_id, uris) {
    (Some(context), Some(track_uris)) => {
      if let Some(first_uri) = track_uris.first() {
        options.playing_track = Some(PlayingTrack::Uri(first_uri.uri()));
      } else if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
        options.playing_track = Some(PlayingTrack::Index(i));
      }
      Some(LoadRequest::from_context_uri(context.uri(), options))
    }
    (Some(context), None) => {
      if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
        options.playing_track = Some(PlayingTrack::Index(i));
      }
      Some(LoadRequest::from_context_uri(context.uri(), options))
    }
    (None, Some(track_uris)) => {
      if let Some(i) = offset.and_then(|i| u32::try_from(i).ok()) {
        options.playing_track = Some(PlayingTrack::Index(i));
      }
      let uris = track_uris.into_iter().map(|u| u.uri()).collect::<Vec<_>>();
      Some(LoadRequest::from_tracks(uris, options))
    }
    (None, None) => None,
  }
}

impl PlaybackNetwork for Network {
  async fn get_current_playback(&mut self) {
    // When using native streaming, the Spotify API returns stale server-side state
    // that doesn't reflect recent local changes (volume, shuffle, repeat, play/pause).
    // We need to preserve these local states and restore them after getting the API response.
    #[cfg(feature = "streaming")]
    let streaming_player = current_streaming_player(self).await;
    #[cfg(feature = "streaming")]
    self.native_idle_recovery.observe_player_instance(
      streaming_player
        .as_ref()
        .filter(|player| player.is_connected())
        .map(|player| Arc::as_ptr(player) as usize),
    );
    #[cfg(feature = "streaming")]
    // Check if native streaming is active by examining the pre-fetched player
    // (avoids redundant lock call from is_native_streaming_active)
    let local_state: Option<(Option<u8>, bool, rspotify::model::RepeatState, Option<bool>)> =
      if streaming_player.as_ref().is_some_and(|p| p.is_connected()) {
        let app = self.app.lock().await;
        if let Some(ref ctx) = app.current_playback_context {
          let volume = streaming_player.as_ref().map(|p| p.get_volume());
          Some((
            volume,
            ctx.shuffle_state,
            ctx.repeat_state,
            app.native_is_playing,
          ))
        } else {
          None
        }
      } else {
        None
      };

    let context = self
      .spotify_get_typed::<Option<rspotify::model::CurrentPlaybackContext>>(
        "me/player",
        &[("additional_types", "episode,track".to_string())],
      )
      .await;

    let mut app = self.app.lock().await;

    // Cover-art download (network + synchronous image decode) must NOT happen
    // Cover art is fetched by the shared track-change detector (see `runner.rs`),
    // which dispatches `IoEvent::FetchCoverArt` off the `App` lock for every
    // source. This handler no longer fetches art inline.
    match context {
      #[allow(unused_mut)]
      Ok(Some(mut c)) => {
        #[cfg(feature = "streaming")]
        self.native_idle_recovery.observe_available_playback();
        app.instant_since_last_current_playback_poll = Instant::now();

        // Detect whether the native spotatui streaming device is the active Spotify device.
        #[cfg(feature = "streaming")]
        let is_native_device = streaming_player.as_ref().is_some_and(|p| {
          if let (Some(current_id), Some(native_id)) =
            (c.device.id.as_ref(), app.native_device_id.as_ref())
          {
            return current_id == native_id;
          }

          let native_name = p.device_name().to_lowercase();
          c.device.name.to_lowercase() == native_name && app.has_fresh_native_activity()
        });

        #[cfg(feature = "streaming")]
        if is_native_device && app.native_device_id.is_none() {
          if let Some(id) = c.device.id.clone() {
            app.native_device_id = Some(id);
          }
        }

        #[cfg(feature = "streaming")]
        let native_streaming_was_active = app.is_streaming_active;
        #[cfg(feature = "streaming")]
        let native_activation_was_pending = app.native_activation_pending;
        let native_track_id_before_api = app.last_track_id.clone();
        #[cfg(feature = "streaming")]
        let native_track_id_present = native_track_id_before_api.is_some();
        #[cfg(feature = "streaming")]
        let api_item_matches_native_track = c
          .item
          .as_ref()
          .and_then(playable_item_id)
          .as_deref()
          .is_some_and(|api_id| Some(api_id) == native_track_id_before_api.as_deref());
        let api_item_confirms_native_info = app
          .native_track_info
          .as_ref()
          .zip(c.item.as_ref())
          .is_some_and(|(native_info, item)| {
            api_confirms_native_info_is_current(
              &native_info.name,
              item,
              native_track_id_before_api.as_deref(),
            )
          });
        #[cfg(feature = "streaming")]
        let stale_api_item_for_native =
          stale_api_item_should_preserve_native_context(StaleApiItemContext {
            native_info_present: app.native_track_info.is_some(),
            api_item_present: c.item.is_some(),
            api_confirms_native_info: api_item_confirms_native_info,
            native_track_id_present,
            api_item_matches_native_track,
            native_streaming_was_active,
            native_activation_pending: native_activation_was_pending,
            api_device_is_native: is_native_device,
          });
        #[cfg(not(feature = "streaming"))]
        let stale_api_item_for_native =
          app.native_track_info.is_some() && c.item.is_some() && !api_item_confirms_native_info;

        // Process track info before storing context (avoids cloning)
        if !stale_api_item_for_native {
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

                    // Lyrics (and cover art) are now driven by the shared
                    // track-change detector in the UI tick, which works for every
                    // source — see `runner.rs`. No per-source dispatch here.

                    app.dispatch(IoEvent::CurrentUserSavedTracksContains(vec![
                      track_id_str.clone()
                    ]));
                  }

                  app.last_track_id = Some(track_id_str);
                };
              }
              PlayableItem::Episode(_episode) => { /*should map this to following the podcast show*/
              }
              _ => {}
            }
          };
        }

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

        // Check if Spotify finally caught up to the user's volume change.
        // If the API now returns what the user asked for, we can clear pending_volume
        // and let the API take over again. If not, this response is stale — ignore it.
        if let Some(pending) = app.pending_volume {
          let api_vol = c.device.volume_percent.unwrap_or(0) as u8;
          if api_vol == pending {
            app.pending_volume = None;
            app.last_dispatched_volume = None;
          } else {
            // API hasn't caught up yet — keep showing the user's intended value
            if let Some(ctx) = app.current_playback_context.as_ref() {
              c.device.volume_percent = ctx.device.volume_percent;
            }
          }
        }

        // On first load with native streaming AND native device is active,
        // override API shuffle with saved preference.
        #[cfg(feature = "streaming")]
        if local_state.is_none() && is_native_device {
          c.shuffle_state = app.user_config.behavior.shuffle_enabled;
          // Proactively set native shuffle on first load to keep backend in sync
          if let Some(ref player) = streaming_player {
            let _ = player.set_shuffle(app.user_config.behavior.shuffle_enabled);
          }
        }

        if !stale_api_item_for_native {
          // Cover art (Spotify album/episode image) is fetched by the shared
          // track-change detector in `runner.rs`, from the snapshot's image URL.
          app.current_playback_context = Some(c);
        }

        // Update is_streaming_active based on whether the current device matches native streaming
        #[cfg(feature = "streaming")]
        {
          if stale_api_item_for_native {
            app.is_streaming_active = true;
            app.native_activation_pending = false;
          } else {
            app.is_streaming_active = is_native_device;
          }

          if is_native_device {
            app.native_activation_pending = false;
          }
        }

        // Keep native metadata authoritative while the native player is active.
        // Spotify's playback endpoint can lag behind librespot by several seconds
        // and report a different item; TrackChanged/Stopped events own this field.
        #[cfg(feature = "streaming")]
        if app.native_track_info.is_some()
          && !stale_api_item_for_native
          && (!is_native_device || api_item_confirms_native_info)
        {
          app.native_track_info = None;
        }
      }
      Ok(None) => {
        #[cfg(feature = "streaming")]
        if let Some(player) = streaming_player.as_ref() {
          reconcile_native_idle_device_if_preferred(
            &mut self.client_config,
            &mut app,
            player,
            &mut self.native_idle_recovery,
          );
        }
        app.instant_since_last_current_playback_poll = Instant::now();
      }
      Err(e) => {
        app.is_fetching_current_playback = false;

        let err = anyhow!(e);

        if err.to_string().contains("429")
          || err.to_string().contains("Too Many Requests")
          || err.to_string().contains("Too many requests")
        {
          app.status_message = Some(
            "Spotify rate limit hit. Retrying automatically; please wait a few seconds."
              .to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(6));
          app.instant_since_last_current_playback_poll = Instant::now();
          return;
        }

        if err
          .to_string()
          .to_lowercase()
          .contains("error sending request for url")
          || err.to_string().contains("connection reset")
          || err.to_string().contains("connection refused")
          || err.to_string().contains("timed out")
          || err.to_string().contains("temporary failure")
          || err.to_string().contains("dns")
        {
          app.status_message = Some(
            "Temporary Spotify network error while polling playback; retrying automatically."
              .to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(5));
          app.instant_since_last_current_playback_poll = Instant::now();
          return;
        }

        if err.to_string().contains("504")
          || err.to_string().contains("503")
          || err.to_string().contains("502")
          || err.to_string().contains("Gateway Timeout")
          || err.to_string().contains("Service Unavailable")
          || err.to_string().contains("Bad Gateway")
        {
          app.status_message = Some(
            "Spotify server temporarily unavailable (5xx); retrying automatically.".to_string(),
          );
          app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(10));
          app.instant_since_last_current_playback_poll = Instant::now();
          return;
        }

        // 404 = no active device/player; treat as idle, not an error
        if err.to_string().contains("404") || err.to_string().contains("Not Found") {
          app.current_playback_context = None;
          #[cfg(feature = "streaming")]
          if let Some(player) = streaming_player.as_ref() {
            reconcile_native_idle_device_if_preferred(
              &mut self.client_config,
              &mut app,
              player,
              &mut self.native_idle_recovery,
            );
          }
          app.instant_since_last_current_playback_poll = Instant::now();
          app.is_fetching_current_playback = false;
          return;
        }

        app.handle_error(err);
        return;
      }
    }

    app.seek_ms.take();
    app.is_fetching_current_playback = false;
  }

  /// Fetch and decode the current track's cover art, then store it. Runs entirely
  /// off the `App` lock (the download/decode is the slow part and must never hold
  /// the render loop's mutex, #142); the guard is only re-acquired at the end to
  /// store the finished image and update the status. Cover art is non-essential,
  /// so a failure only logs and flips the status to `Failed` (never surfaces a
  /// blocking error).
  #[cfg(feature = "cover-art")]
  async fn fetch_cover_art(&mut self, request: crate::tui::cover_art::CoverArtRequest) {
    use crate::core::app::CoverArtStatus;
    use crate::tui::cover_art::{CoverArt, CoverArtRequest};

    let key = request.key().to_string();

    // Skip the download/decode when we already hold art for this exact key
    // (e.g. consecutive tracks that share an album cover): just mark it loaded.
    {
      let mut app = self.app.lock().await;
      if app.cover_art.get_url().as_deref() == Some(key.as_str()) {
        app.cover_art_status = CoverArtStatus::Loaded;
        return;
      }
    }

    let result = match request {
      CoverArtRequest::Url(url) => CoverArt::fetch_and_decode(&url).await,
      #[cfg(feature = "local-files")]
      CoverArtRequest::LocalFile { path, .. } => {
        // Tag read + image decode are blocking; keep them off the async runtime.
        match tokio::task::spawn_blocking(move || {
          crate::infra::local::extract_embedded_cover(&path)
        })
        .await
        {
          Ok(inner) => inner,
          Err(join_err) => Err(anyhow!(join_err)),
        }
      }
    };

    let mut app = self.app.lock().await;
    match result {
      Ok(img) => {
        app.cover_art.store_decoded(key, img);
        app.cover_art_status = CoverArtStatus::Loaded;
      }
      Err(err) => {
        log::warn!("cover art load failed: {err}");
        // Drop any stale art so the pane shows the "unavailable" placeholder
        // rather than the previous track's image.
        app.cover_art.clear();
        app.cover_art_status = CoverArtStatus::Failed;
      }
    }
  }

  async fn start_playback(
    &mut self,
    context_id: Option<PlayContextId<'static>>,
    uris: Option<Vec<PlayableId<'static>>>,
    offset: Option<usize>,
  ) {
    let (uris, offset) = if context_id.is_none() {
      match uris {
        Some(track_uris) => {
          let (trimmed_uris, trimmed_offset) = trim_api_playback_uris(track_uris, offset);
          (Some(trimmed_uris), trimmed_offset)
        }
        None => (None, offset),
      }
    } else {
      (uris, offset)
    };

    let desired_shuffle_state = {
      let app = self.app.lock().await;
      app
        .current_playback_context
        .as_ref()
        .map(|ctx| ctx.shuffle_state)
        .unwrap_or(app.user_config.behavior.shuffle_enabled)
    };

    // Check if we should use native streaming for playback
    #[cfg(feature = "streaming")]
    if request_native_streaming_recovery_if_disconnected(self).await {
      return;
    }

    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = start_playback_backend(self).await {
      let requested_origin = requested_native_playback_origin(self, &context_id, &uris).await;
      let activation_time = Instant::now();
      let native_device_id = player.device_id();
      let (should_transfer, native_preference_update) = {
        let app = self.app.lock().await;
        let saved_device_matches_native = saved_device_matches_native_player(
          self.client_config.device_id.as_deref(),
          Some(&native_device_id),
          app.devices.as_ref(),
          player.device_name(),
        );
        let activation_pending = app.native_activation_pending;
        let recent_activation = app
          .last_device_activation
          .is_some_and(|instant| instant.elapsed() < Duration::from_secs(5));
        let should_transfer = if activation_pending {
          !recent_activation
        } else {
          !app.is_streaming_active && !recent_activation
        };

        (
          should_transfer,
          native_device_preference_update(
            self.client_config.device_id.as_deref(),
            false,
            saved_device_matches_native,
          ),
        )
      };

      if should_transfer {
        let _ = player.transfer(None);
      }

      player.activate();
      self
        .native_idle_recovery
        .settle_current_episode(activation_time);
      {
        let mut app = self.app.lock().await;
        app.is_streaming_active = true;
        app.last_device_activation = Some(activation_time);
        app.native_activation_pending = false;
        app.native_playback_origin = Some(requested_origin);
        app.native_device_id = Some(native_device_id.clone());
        persist_native_device_id_if_needed(
          &mut self.client_config,
          &mut app,
          &native_device_id,
          native_preference_update,
        );
      }
      let native_route = resolve_native_playback_route(self, &context_id).await;

      // For resume playback (no context, no uris)
      if context_id.is_none() && uris.is_none() {
        let can_resume_direct_native = {
          let app = self.app.lock().await;
          app.native_track_info.is_some() || app.last_track_id.is_some()
        };

        if can_resume_direct_native {
          info!("starting native resume playback via direct player route");
          player.play();
          let mut app = self.app.lock().await;
          if let Some(ctx) = &mut app.current_playback_context {
            ctx.is_playing = true;
          }
        } else {
          info!(
            "starting native resume playback via Spotify API on device {}",
            native_device_id
          );
          match self
            .spotify_api_request_json(
              Method::PUT,
              "me/player/play",
              &[("device_id", native_device_id.clone())],
              None,
            )
            .await
          {
            Ok(_) => {
              let mut app = self.app.lock().await;
              app.native_device_id = Some(native_device_id);
              if let Some(ctx) = &mut app.current_playback_context {
                ctx.is_playing = true;
              }
              app.dispatch(IoEvent::GetCurrentPlayback);
            }
            Err(e) => {
              let mut app = self.app.lock().await;
              app.set_status_message(
                format!("No playback to resume on {}.", player.device_name()),
                4,
              );
              info!("native resume via Spotify API failed: {}", e);
            }
          }
        }
        return;
      }

      if let (NativePlaybackRoute::ContextApi { device_id }, Some(context)) =
        (&native_route, context_id.clone())
      {
        info!(
          "starting native playback via Spotify context route on device {}",
          device_id
        );
        let body = api_playback_body(Some(&context), uris.as_deref(), offset);
        match self
          .spotify_api_request_json(
            Method::PUT,
            "me/player/play",
            &[("device_id", device_id.clone())],
            body,
          )
          .await
        {
          Ok(_) => {
            if let Err(e) = self
              .spotify_api_request_json(
                Method::PUT,
                "me/player/shuffle",
                &[
                  ("state", desired_shuffle_state.to_string()),
                  ("device_id", device_id.clone()),
                ],
                None,
              )
              .await
            {
              let mut app = self.app.lock().await;
              app.handle_error(anyhow!(e));
            }

            let mut app = self.app.lock().await;
            app.instant_since_last_current_playback_poll = Instant::now() - Duration::from_secs(6);
            if let Some(ctx) = &mut app.current_playback_context {
              ctx.is_playing = true;
              ctx.shuffle_state = desired_shuffle_state;
            }
            app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
            app.dispatch(IoEvent::GetCurrentPlayback);
            return;
          }
          Err(e) => {
            info!(
                "native context playback via Spotify API failed; falling back to direct native load: {}",
                e
              );
          }
        }
      }

      let Some(request) = native_load_request(context_id, uris, offset) else {
        return;
      };

      info!("starting native playback via direct load route");
      if let Err(e) = player.load(request) {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!("Failed to start native playback: {}", e));
      } else {
        let _ = player.set_shuffle(desired_shuffle_state);
        // Optimistic UI update
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = true;
          ctx.shuffle_state = desired_shuffle_state;
        }
        app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
      }
      return;
    }

    let body = api_playback_body(context_id.as_ref(), uris.as_deref(), offset);
    let result = self
      .spotify_api_request_json(Method::PUT, "me/player/play", &[], body)
      .await;

    match result {
      Ok(_) => {
        if let Err(e) = self
          .spotify_api_request_json(
            Method::PUT,
            "me/player/shuffle",
            &[("state", desired_shuffle_state.to_string())],
            None,
          )
          .await
        {
          let mut app = self.app.lock().await;
          app.handle_error(anyhow!(e));
        }

        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = true;
          ctx.shuffle_state = desired_shuffle_state;
        }
        app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
      }
      Err(e) => {
        #[cfg(feature = "streaming")]
        if is_no_active_device_error(&e) {
          if let Some(player) = current_streaming_player(self).await {
            if player.is_connected() {
              let requested_origin =
                requested_native_playback_origin(self, &context_id, &uris).await;
              let activation_time = Instant::now();
              let native_device_id = player.device_id();
              player.activate();
              self
                .native_idle_recovery
                .settle_current_episode(activation_time);
              {
                let mut app = self.app.lock().await;
                let saved_device_matches_native = saved_device_matches_native_player(
                  self.client_config.device_id.as_deref(),
                  Some(&native_device_id),
                  app.devices.as_ref(),
                  player.device_name(),
                );
                let native_preference_update = native_device_preference_update(
                  self.client_config.device_id.as_deref(),
                  false,
                  saved_device_matches_native,
                );
                app.is_streaming_active = true;
                app.native_activation_pending = false;
                app.native_playback_origin = Some(requested_origin);
                app.native_device_id = Some(native_device_id.clone());
                app.last_device_activation = Some(activation_time);
                app.instant_since_last_current_playback_poll =
                  activation_time - Duration::from_secs(6);
                persist_native_device_id_if_needed(
                  &mut self.client_config,
                  &mut app,
                  &native_device_id,
                  native_preference_update,
                );
              }

              if let Some(request) = native_load_request(context_id, uris, offset) {
                info!("default Spotify playback had no active device; falling back to native load");
                if let Err(load_err) = player.load(request) {
                  let mut app = self.app.lock().await;
                  app.handle_error(anyhow!("Failed to start native playback: {}", load_err));
                } else {
                  let _ = player.set_shuffle(desired_shuffle_state);
                  let mut app = self.app.lock().await;
                  if let Some(ctx) = &mut app.current_playback_context {
                    ctx.is_playing = true;
                    ctx.shuffle_state = desired_shuffle_state;
                  }
                  app.user_config.behavior.shuffle_enabled = desired_shuffle_state;
                }
                return;
              }

              info!(
                "default Spotify resume had no active device; retrying on native device {}",
                native_device_id
              );
              match self
                .spotify_api_request_json(
                  Method::PUT,
                  "me/player/play",
                  &[("device_id", native_device_id.clone())],
                  None,
                )
                .await
              {
                Ok(_) => {
                  let mut app = self.app.lock().await;
                  if let Some(ctx) = &mut app.current_playback_context {
                    ctx.is_playing = true;
                  }
                  app.dispatch(IoEvent::GetCurrentPlayback);
                }
                Err(resume_err) => {
                  let mut app = self.app.lock().await;
                  app.set_status_message(
                    format!("No playback to resume on {}.", player.device_name()),
                    4,
                  );
                  info!("native resume fallback failed: {}", resume_err);
                }
              }
              return;
            }
          }
        }

        let mut app = self.app.lock().await;
        app.handle_error(e);
      }
    }
  }

  async fn pause_playback(&mut self) {
    // Check if using native streaming
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      player.pause();
      // Update UI state immediately
      let mut app = self.app.lock().await;
      if let Some(ctx) = &mut app.current_playback_context {
        ctx.is_playing = false;
      }
      return;
    }

    match self
      .spotify_api_request_json(Method::PUT, "me/player/pause", &[], None)
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.is_playing = false;
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn next_track(&mut self) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      player.next();
      return;
    }

    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/next", &[], None)
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn previous_track(&mut self) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      player.prev();
      return;
    }

    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/previous", &[], None)
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn force_previous_track(&mut self) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      player.prev();
      tokio::time::sleep(std::time::Duration::from_millis(500)).await;
      player.prev();
      return;
    }

    // First previous_track restarts the current track (if past Spotify's ~3s
    // threshold). After a short delay the second call actually skips to the
    // previous track, since the position is now back at 0.
    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/previous", &[], None)
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
      return;
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    if let Err(e) = self
      .spotify_api_request_json(Method::POST, "me/player/previous", &[], None)
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn seek(&mut self, position_ms: u32) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      player.seek(position_ms);
      return;
    }

    if let Err(e) = self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/seek",
        &[("position_ms", position_ms.to_string())],
        None,
      )
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    }
  }

  async fn shuffle(&mut self, shuffle_state: bool) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      let _ = player.set_shuffle(shuffle_state);
      let mut app = self.app.lock().await;
      if let Some(ctx) = &mut app.current_playback_context {
        ctx.shuffle_state = shuffle_state;
      }
      return;
    }

    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/shuffle",
        &[("state", shuffle_state.to_string())],
        None,
      )
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.shuffle_state = shuffle_state;
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn repeat(&mut self, repeat_state: RepeatState) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      let _ = player.set_repeat(repeat_state);
      let mut app = self.app.lock().await;
      if let Some(ctx) = &mut app.current_playback_context {
        ctx.repeat_state = repeat_state;
      }
      return;
    }

    let repeat_state_param: &'static str = repeat_state.into();
    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/repeat",
        &[("state", repeat_state_param.to_string())],
        None,
      )
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.repeat_state = repeat_state;
        }
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  /// Sends the volume change to Spotify, either through the native streaming
  /// player or the Web API depending on which device is active.
  ///
  /// On success we clear the in-flight flag but keep `pending_volume` around.
  /// It only gets cleared when `get_current_playback` comes back with a matching
  /// volume — that's our signal that Spotify actually caught up.
  ///
  /// On error we bail and clear everything so the UI falls back to whatever
  /// the API last reported.
  async fn change_volume(&mut self, volume: u8) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = symmetric_playback_backend(self).await {
      player.set_volume(volume);
      let mut app = self.app.lock().await;
      if let Some(ctx) = &mut app.current_playback_context {
        ctx.device.volume_percent = Some(volume.into());
      }
      app.is_volume_change_in_flight = false;
      app.last_dispatched_volume = Some(volume);
      // Keep pending_volume set — cleared when API confirms the value matches
      return;
    }

    match self
      .spotify_api_request_json(
        Method::PUT,
        "me/player/volume",
        &[("volume_percent", volume.to_string())],
        None,
      )
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        if let Some(ctx) = &mut app.current_playback_context {
          ctx.device.volume_percent = Some(volume.into());
        }
        app.is_volume_change_in_flight = false;
        app.last_dispatched_volume = Some(volume);
        // Keep pending_volume set — cleared when get_current_playback confirms
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.is_volume_change_in_flight = false;
        app.pending_volume = None;
        app.last_dispatched_volume = None;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn transfert_playback_to_device(&mut self, device_id: String, persist_device_id: bool) {
    #[cfg(feature = "streaming")]
    if let PlaybackBackend::Native(player) = transfer_playback_backend(self, &device_id).await {
      let activation_time = Instant::now();
      let native_device_id = player.device_id();
      let _ = player.transfer(None);
      player.activate();
      self
        .native_idle_recovery
        .settle_current_episode(activation_time);
      let mut app = self.app.lock().await;
      let saved_device_matches_native = saved_device_matches_native_player(
        self.client_config.device_id.as_deref(),
        Some(&native_device_id),
        app.devices.as_ref(),
        player.device_name(),
      );
      let native_preference_update = native_device_preference_update(
        self.client_config.device_id.as_deref(),
        persist_device_id,
        saved_device_matches_native,
      );
      app.is_streaming_active = true;
      app.native_activation_pending = true;
      app.native_playback_origin = None;
      app.native_device_id = Some(native_device_id.clone());
      // Drop the stale previous-device context so playback routing follows the
      // native intent (is_streaming_active) until the next poll repopulates it
      // — mirrors the non-native transfer branch below. Without this, the first
      // play can leak to the official Spotify client / 404 (#282).
      app.current_playback_context = None;
      app.last_device_activation = Some(activation_time);
      app.instant_since_last_current_playback_poll = activation_time - Duration::from_secs(6);
      persist_native_device_id_if_needed(
        &mut self.client_config,
        &mut app,
        &native_device_id,
        native_preference_update,
      );
      return;
    }

    if let Err(e) = self
      .spotify_api_request_json(
        Method::PUT,
        "me/player",
        &[],
        Some(json!({
          "device_ids": [device_id.clone()],
          "play": true
        })),
      )
      .await
    {
      let mut app = self.app.lock().await;
      app.handle_error(anyhow!(e));
    } else {
      let mut app = self.app.lock().await;
      if persist_device_id {
        // Update via client_config helper to save to file
        if let Err(e) = self.client_config.set_device_id(device_id) {
          app.handle_error(anyhow!(e));
        }
      }
      app.current_playback_context = None;

      #[cfg(feature = "streaming")]
      {
        // If transferring away from native, update flag
        app.is_streaming_active = false;
        app.native_playback_origin = None;
      }
    }
  }

  #[cfg(feature = "streaming")]
  async fn auto_select_streaming_device(&mut self, device_name: String, persist_device_id: bool) {
    if let Some(player) = current_streaming_player(self).await {
      let activation_time = Instant::now();
      let native_device_id = player.device_id();
      let (should_transfer, native_preference_update) = {
        let app = self.app.lock().await;
        let saved_device_matches_native = saved_device_matches_native_player(
          self.client_config.device_id.as_deref(),
          Some(&native_device_id),
          app.devices.as_ref(),
          player.device_name(),
        );
        let recent_activation = app
          .last_device_activation
          .is_some_and(|instant| instant.elapsed() < Duration::from_secs(5));
        (
          !app.native_activation_pending && !app.is_streaming_active && !recent_activation,
          native_device_preference_update(
            self.client_config.device_id.as_deref(),
            persist_device_id,
            saved_device_matches_native,
          ),
        )
      };

      {
        let mut app = self.app.lock().await;
        app.is_streaming_active = true;
        app.native_activation_pending = true;
        app.last_device_activation = Some(activation_time);
        app.instant_since_last_current_playback_poll = activation_time - Duration::from_secs(6);
      }

      if should_transfer {
        let _ = player.transfer(None);
      }
      player.activate();
      self
        .native_idle_recovery
        .settle_current_episode(activation_time);

      {
        let mut app = self.app.lock().await;
        app.is_streaming_active = true;
        app.native_activation_pending = false;
        app.native_device_id = Some(native_device_id.clone());
        app.last_device_activation = Some(activation_time);
        app.instant_since_last_current_playback_poll = activation_time - Duration::from_secs(6);
        persist_native_device_id_if_needed(
          &mut self.client_config,
          &mut app,
          &native_device_id,
          native_preference_update,
        );
      }

      for _ in 0..2 {
        tokio::time::sleep(Duration::from_millis(200)).await;

        match self
          .spotify_get_typed::<DevicePayload>("me/player/devices", &[])
          .await
        {
          Ok(payload) => {
            let native_confirmed =
              spotify_payload_confirms_native_device(&payload, &native_device_id);
            let name_seen = payload
              .devices
              .iter()
              .any(|device| device.name.eq_ignore_ascii_case(&device_name));

            if native_confirmed || name_seen {
              let mut app = self.app.lock().await;
              app.devices = Some(payload);
              app
                .plugin_data_generations
                .bump(crate::core::app::PluginDataKind::Devices);
            }

            if native_confirmed {
              return;
            }
          }
          Err(_) => continue,
        }
      }
    }
  }

  async fn ensure_playback_continues(&mut self, previous_track_id: String) {
    #[cfg(feature = "streaming")]
    if is_native_streaming_active_for_playback(self).await {
      // Native player handles queue automatically
      return;
    }

    // Check if we are paused/stopped
    let context = self
      .spotify_get_typed::<Option<rspotify::model::CurrentPlaybackContext>>("me/player", &[])
      .await;

    if let Ok(Some(ctx)) = context {
      if !ctx.is_playing {
        // If we are stopped but shouldn't be (e.g. track finished), try to skip to next
        // Use a heuristic: if the current item is the SAME as the previous one and we are at 0:00,
        // it might mean Spotify stopped. Or if we are just null.
        if let Some(item) = ctx.item {
          let current_id = match item {
            PlayableItem::Track(t) => t.id.map(|id| id.id().to_string()),
            PlayableItem::Episode(e) => Some(e.id.id().to_string()),
            _ => None,
          };

          if current_id == Some(previous_track_id)
            && ctx
              .progress
              .map(|d: TimeDelta| d.num_milliseconds())
              .unwrap_or(0)
              == 0
          {
            self.next_track().await;
          }
        }
      }
    }
  }

  async fn resume_spotify_context(
    &mut self,
    context_uri: Option<String>,
    resume_track_uri: Option<String>,
  ) {
    use crate::infra::network::ids;
    let context = context_uri.as_deref().and_then(ids::play_context_id);
    let track = resume_track_uri.as_deref().and_then(ids::playable_id);

    // Reuse the existing `start_playback` machinery (device activation/transfer
    // included). Passing the resume track as a single-item `uris` alongside the
    // context yields an offset-by-uri start (see `api_playback_offset_json` /
    // `native_load_request`), i.e. the context resumes at that track. A plain
    // `spirc.play()` can't do this after a direct `player.load`, which is why
    // the context is re-loaded here.
    match (context, track) {
      (Some(context), Some(track)) => {
        self
          .start_playback(Some(context), Some(vec![track]), None)
          .await;
      }
      (Some(context), None) => {
        self.start_playback(Some(context), None, None).await;
      }
      (None, Some(track)) => {
        self.start_playback(None, Some(vec![track]), None).await;
      }
      (None, None) => {
        let mut app = self.app.lock().await;
        app.set_status_message("Queue finished", 3);
      }
    }
  }

  async fn add_item_to_queue(&mut self, item: PlayableId<'static>) {
    match self
      .spotify_api_request_json(
        Method::POST,
        "me/player/queue",
        &[("uri", item.uri())],
        None,
      )
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.status_message = Some("Added to queue".to_string());
        app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(3));
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.handle_error(anyhow!(e));
      }
    }
  }

  async fn get_queue(&mut self) {
    match self
      .spotify_get_typed::<CurrentUserQueue>("me/player/queue", &[])
      .await
    {
      Ok(q) => {
        use crate::core::app::QueueState;
        use crate::infra::network::mapping;
        let domain_queue = QueueState {
          currently_playing: q
            .currently_playing
            .as_ref()
            .and_then(mapping::playable_info),
          queue: q.queue.iter().filter_map(mapping::playable_info).collect(),
        };
        let mut app = self.app.lock().await;
        app.queue = Some(domain_queue);
        app
          .plugin_data_generations
          .bump(crate::core::app::PluginDataKind::Queue);
      }
      Err(e) => {
        let mut app = self.app.lock().await;
        app.queue = None;
        // Bump on failure too: completion (not success) is what plugin data
        // requests wait on.
        app
          .plugin_data_generations
          .bump(crate::core::app::PluginDataKind::Queue);
        app.status_message = Some("Could not load queue (no active device?)".to_string());
        app.status_message_expires_at = Some(Instant::now() + Duration::from_secs(3));
        log::warn!("get_queue failed: {}", e);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rspotify::model::{
    artist::SimplifiedArtist, idtypes::TrackId, track::FullTrack, SimplifiedAlbum,
  };
  #[cfg(feature = "streaming")]
  use rspotify::model::{device::Device, DeviceType};
  use rspotify::prelude::Id;
  use std::collections::HashMap;

  fn playable_track(id: &str) -> PlayableId<'static> {
    PlayableId::Track(TrackId::from_id(id).unwrap().into_static())
  }

  #[allow(deprecated)]
  fn full_track(id: &str, name: &str) -> PlayableItem {
    PlayableItem::Track(FullTrack {
      album: SimplifiedAlbum {
        name: "Album".to_string(),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: TimeDelta::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: Some(TrackId::from_id(id).unwrap().into_static()),
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: name.to_string(),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    })
  }

  #[cfg(feature = "streaming")]
  #[allow(deprecated)]
  fn playback_device(id: &str, name: &str) -> Device {
    Device {
      id: Some(id.to_string()),
      is_active: false,
      is_private_session: false,
      is_restricted: false,
      name: name.to_string(),
      _type: DeviceType::Computer,
      volume_percent: Some(50),
    }
  }

  #[test]
  fn trim_api_playback_uris_leaves_small_requests_unchanged() {
    let uris = vec![
      playable_track("0000000000000000000001"),
      playable_track("0000000000000000000002"),
    ];

    let (trimmed, offset) = trim_api_playback_uris(uris.clone(), Some(1));

    assert_eq!(trimmed, uris);
    assert_eq!(offset, Some(1));
  }

  #[test]
  fn trim_api_playback_uris_keeps_selected_track_inside_window() {
    let uris = (0..150)
      .map(|index| playable_track(&format!("{index:022}")))
      .collect::<Vec<_>>();

    let (trimmed, offset) = trim_api_playback_uris(uris.clone(), Some(60));

    assert_eq!(trimmed.len(), MAX_API_PLAYBACK_URIS);
    assert_eq!(offset, Some(20));
    assert_eq!(trimmed[offset.unwrap()].uri(), uris[60].uri());
  }

  #[test]
  fn trim_api_playback_uris_slides_window_near_end() {
    let uris = (0..150)
      .map(|index| playable_track(&format!("{index:022}")))
      .collect::<Vec<_>>();

    let (trimmed, offset) = trim_api_playback_uris(uris.clone(), Some(149));

    assert_eq!(trimmed.len(), MAX_API_PLAYBACK_URIS);
    assert_eq!(offset, Some(99));
    assert_eq!(trimmed[offset.unwrap()].uri(), uris[149].uri());
  }

  #[test]
  fn api_playback_offset_uses_track_uri_for_context_playback() {
    let uris = vec![
      playable_track("0000000000000000000001"),
      playable_track("0000000000000000000002"),
    ];

    let offset = api_playback_offset_json(Some(&uris), Some(1));

    assert_eq!(
      offset,
      Some(json!({ "uri": "spotify:track:0000000000000000000001" }))
    );
  }

  #[test]
  fn api_playback_offset_uses_position_for_uri_list_playback() {
    let offset = api_playback_offset_json(None, Some(1));

    assert_eq!(offset, Some(json!({ "position": 1 })));
  }

  #[test]
  fn api_playback_offset_falls_back_to_position_when_context_has_no_uri() {
    let offset = api_playback_offset_json(None, Some(3));

    assert_eq!(offset, Some(json!({ "position": 3 })));
  }

  #[test]
  fn api_confirms_native_info_when_names_match() {
    let item = full_track("0000000000000000000001", "Current Song");

    assert!(api_confirms_native_info_is_current(
      "Current Song",
      &item,
      Some("different-id")
    ));
  }

  #[test]
  fn api_confirms_native_info_when_current_id_matches_even_if_name_differs() {
    let item = full_track("0000000000000000000001", "Stranger Thing");

    assert!(api_confirms_native_info_is_current(
      "Greater Together",
      &item,
      Some("0000000000000000000001")
    ));
  }

  #[test]
  fn api_does_not_confirm_stale_api_item_for_different_native_track() {
    let item = full_track("0000000000000000000001", "Old API Song");

    assert!(!api_confirms_native_info_is_current(
      "New Native Song",
      &item,
      Some("0000000000000000000002")
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn no_active_device_error_matches_spotify_no_device_signals() {
    assert!(is_no_active_device_error(&anyhow!(
      "{}",
      r#"Spotify API 404 Not Found failed: {"error":{"reason":"NO_ACTIVE_DEVICE"}}"#
    )));
    assert!(is_no_active_device_error(&anyhow!(
      "Spotify API 404 Not Found failed: No active device found"
    )));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn no_active_device_error_does_not_match_generic_not_found() {
    assert!(!is_no_active_device_error(&anyhow!(
      "Spotify API 404 Not Found failed: playlist not found"
    )));
    assert!(!is_no_active_device_error(&anyhow!(
      "Spotify API 404 failed for https://example.test/404"
    )));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_device_preference_update_persists_when_no_saved_device() {
    assert_eq!(
      native_device_preference_update(None, false, false),
      NativeDevicePreferenceUpdate::Persist
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_device_preference_update_persists_when_explicitly_requested() {
    assert_eq!(
      native_device_preference_update(Some("phone-device"), true, false),
      NativeDevicePreferenceUpdate::Persist
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_device_preference_update_keeps_existing_saved_device_for_fallback() {
    assert_eq!(
      native_device_preference_update(Some("phone-device"), false, false),
      NativeDevicePreferenceUpdate::KeepExistingPreference
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_device_preference_update_refreshes_saved_native_device() {
    assert_eq!(
      native_device_preference_update(Some("old-native-device"), false, true),
      NativeDevicePreferenceUpdate::Persist
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn idle_poll_exposes_native_device_when_it_is_preferred() {
    assert_eq!(
      native_idle_device_preference_update(None, false),
      Some(NativeDevicePreferenceUpdate::Persist)
    );
    assert_eq!(
      native_idle_device_preference_update(Some("old-native-device"), true),
      Some(NativeDevicePreferenceUpdate::Persist)
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn idle_poll_preserves_saved_external_device() {
    assert_eq!(
      native_idle_device_preference_update(Some("phone-device"), false),
      None
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn idle_recovery_is_limited_to_two_spaced_attempts() {
    let mut recovery = NativeIdleRecoveryState::default();
    recovery.observe_player_instance(Some(1));
    let started_at = Instant::now();

    assert!(recovery.should_attempt_idle_recovery(started_at));
    assert!(!recovery.should_attempt_idle_recovery(
      started_at + NATIVE_IDLE_RECOVERY_RETRY_INTERVAL - Duration::from_millis(1)
    ));
    assert!(recovery.should_attempt_idle_recovery(started_at + NATIVE_IDLE_RECOVERY_RETRY_INTERVAL));
    assert!(!recovery.should_attempt_idle_recovery(
      started_at + NATIVE_IDLE_RECOVERY_RETRY_INTERVAL + NATIVE_IDLE_RECOVERY_RETRY_INTERVAL
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn healthy_playback_rearms_idle_recovery() {
    let mut recovery = NativeIdleRecoveryState::default();
    recovery.observe_player_instance(Some(1));
    let started_at = Instant::now();

    assert!(recovery.should_attempt_idle_recovery(started_at));
    recovery.observe_available_playback();

    assert!(recovery.should_attempt_idle_recovery(started_at + Duration::from_millis(1)));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn replacement_player_rearms_idle_recovery() {
    let mut recovery = NativeIdleRecoveryState::default();
    recovery.observe_player_instance(Some(1));
    let started_at = Instant::now();
    recovery.settle_current_episode(started_at);

    recovery.observe_player_instance(Some(2));

    assert!(recovery.should_attempt_idle_recovery(started_at + Duration::from_millis(1)));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn explicit_activation_settles_current_idle_episode() {
    let mut recovery = NativeIdleRecoveryState::default();
    recovery.observe_player_instance(Some(1));
    let started_at = Instant::now();

    recovery.settle_current_episode(started_at);

    assert!(
      !recovery.should_attempt_idle_recovery(started_at + NATIVE_IDLE_RECOVERY_RETRY_INTERVAL)
    );
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_payload_confirms_native_device_by_id() {
    let payload = DevicePayload {
      devices: vec![playback_device("native-device", "spotatui")],
    };

    assert!(spotify_payload_confirms_native_device(
      &payload,
      "native-device"
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn spotify_payload_does_not_confirm_stale_native_name_with_different_id() {
    let payload = DevicePayload {
      devices: vec![playback_device("stale-device", "spotatui")],
    };

    assert!(!spotify_payload_confirms_native_device(
      &payload,
      "native-device"
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_uses_native_when_no_current_device_or_saved_device() {
    assert!(should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: true,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_uses_native_when_saved_device_is_unavailable() {
    assert!(should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: true,
        saved_external_confirmed_available: false,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_keeps_confirmed_external_device() {
    assert!(!should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: true,
        current_device_id_present: true,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_keeps_confirmed_saved_external_device() {
    assert!(!should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: true,
        saved_external_confirmed_available: true,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_uses_native_for_saved_native_device() {
    assert!(should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: true,
        saved_device_matches_native: true,
        saved_external_confirmed_available: true,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_uses_native_for_stale_native_name_match() {
    assert!(should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: true,
        current_device_id_present: true,
        current_device_name_matches_native: true,
        native_has_fresh_activity: false,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn native_activation_ignores_disconnected_player() {
    assert!(!should_activate_native_for_playback(
      NativeActivationContext {
        player_connected: false,
        saved_device_matches_native: true,
        ..Default::default()
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_api_item_keeps_native_metadata_when_native_was_active() {
    assert!(stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_api_item_keeps_native_metadata_during_activation() {
    assert!(stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: false,
        native_activation_pending: true,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_api_item_keeps_native_context_before_native_metadata_arrives() {
    assert!(stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: false,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn stale_native_metadata_clears_after_playback_leaves_native_device() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: false,
        native_streaming_was_active: false,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn confirmed_api_item_no_longer_keeps_native_metadata() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: true,
        api_item_present: true,
        api_confirms_native_info: true,
        native_track_id_present: true,
        api_item_matches_native_track: true,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: true,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn matching_api_item_without_native_metadata_can_update_context() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: false,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: true,
        api_item_matches_native_track: true,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  #[cfg(feature = "streaming")]
  #[test]
  fn api_item_without_native_track_id_can_update_context() {
    assert!(!stale_api_item_should_preserve_native_context(
      StaleApiItemContext {
        native_info_present: false,
        api_item_present: true,
        api_confirms_native_info: false,
        native_track_id_present: false,
        api_item_matches_native_track: false,
        native_streaming_was_active: true,
        native_activation_pending: false,
        api_device_is_native: false,
      },
    ));
  }

  /// With neither a context uri nor a resume track, resuming has nothing to do:
  /// it reports the queue as finished rather than issuing a playback request.
  #[tokio::test]
  async fn resume_spotify_context_with_nothing_known_finishes_the_queue() {
    use crate::core::app::App;
    use crate::core::config::ClientConfig;
    use crate::core::user_config::UserConfig;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    let (io_tx, _rx) = channel();
    let app = std::sync::Arc::new(tokio::sync::Mutex::new(App::new(
      io_tx,
      UserConfig::new(),
      Some(SystemTime::now()),
    )));
    // No Spotify client is needed: the both-None arm never reaches `spotify()`.
    let mut network = Network::new(
      None,
      ClientConfig::new(),
      &app,
      std::env::temp_dir().join("spotatui_resume_context_test.json"),
    );

    network.resume_spotify_context(None, None).await;

    let guard = app.lock().await;
    assert_eq!(guard.status_message.as_deref(), Some("Queue finished"));
  }
}
