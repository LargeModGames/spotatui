//! Local-file playback routing.
//!
//! This is the seam that keeps the Spotify [`Network`](crate::infra::network)
//! Spotify-only: [`route_playback_event`] is called from the runtime IoEvent
//! pump *before* `handle_network_event`. When the event targets local playback
//! (a `file://` URI, or any transport control while a local file owns the
//! session) it is handled here against the [`LocalPlayer`] and the event is
//! consumed; otherwise it falls through to the normal Spotify dispatch.
//!
//! ## Device ownership
//!
//! Only one backend holds the audio output device at a time (required on
//! exclusive-ALSA setups, harmless on PipeWire/PulseAudio/WASAPI):
//!
//! * Starting local playback first pauses native Spotify (librespot releases the
//!   device when its sink stops), then opens the local output device.
//! * Starting Spotify playback tears the local player down (dropping it releases
//!   the device) before the event is forwarded to the network.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::player::LocalPlayer;
use super::{file_uri_to_path, track_info_from_path};
use crate::core::app::{App, NativeTrackInfo, NativeTrackKind};
use crate::infra::network::IoEvent;

/// Whether a URI is owned by the local-files source.
fn is_file_uri(uri: &str) -> bool {
  uri.starts_with("file:")
}

/// Intercept playback events that target local files.
///
/// Returns `true` if the event was handled locally (and must **not** be
/// forwarded to the Spotify network), `false` to let the normal dispatch run.
pub async fn route_playback_event(app: &Arc<Mutex<App>>, event: &IoEvent) -> bool {
  match event {
    // Start playing a local file.
    IoEvent::StartPlayback(Some(uri), _, _) if is_file_uri(uri) => {
      start_local(app, uri).await;
      true
    }
    // Bare "resume current" — ours only while a local file owns the session.
    IoEvent::StartPlayback(None, None, None) => {
      if local_active(app).await {
        if let Some(player) = clone_player(app).await {
          player.resume();
          app.lock().await.native_is_playing = Some(true);
        }
        true
      } else {
        false
      }
    }
    // Any other start is a real Spotify play: relinquish the device first, then
    // let the network handle it.
    IoEvent::StartPlayback(..) => {
      teardown_local(app).await;
      false
    }
    IoEvent::PausePlayback if would_route(app).await => {
      if let Some(player) = clone_player(app).await {
        player.pause();
        app.lock().await.native_is_playing = Some(false);
      }
      true
    }
    IoEvent::Seek(position_ms) if would_route(app).await => {
      if let Some(player) = clone_player(app).await {
        let _ = player.seek(Duration::from_millis(*position_ms as u64));
        app.lock().await.song_progress_ms = *position_ms as u128;
      }
      true
    }
    IoEvent::ChangeVolume(volume) if would_route(app).await => {
      if let Some(player) = clone_player(app).await {
        player.set_volume(*volume as f32 / 100.0);
      }
      // Keep the playbar's volume readout in sync.
      app.lock().await.user_config.behavior.volume_percent = *volume;
      true
    }
    // Single-file local playback has no queue yet; swallow skips so they don't
    // reach (and disturb) Spotify.
    IoEvent::NextTrack | IoEvent::PreviousTrack | IoEvent::ForcePreviousTrack => {
      would_route(app).await
    }
    _ => false,
  }
}

/// Whether a local file currently owns the playback session.
async fn local_active(app: &Arc<Mutex<App>>) -> bool {
  app.lock().await.is_local_playback_active
}

/// Whether a transport control should route to the local player: the session is
/// local *and* a player handle exists.
async fn would_route(app: &Arc<Mutex<App>>) -> bool {
  let app = app.lock().await;
  app.is_local_playback_active && app.local_player.is_some()
}

async fn clone_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  app.lock().await.local_player.clone()
}

/// Begin playing the local file at `uri`, taking over the playback session.
async fn start_local(app: &Arc<Mutex<App>>, uri: &str) {
  let path = match file_uri_to_path(uri) {
    Ok(path) => path,
    Err(e) => {
      set_error(app, format!("Invalid local file URI: {e}")).await;
      return;
    }
  };

  // Claim the session *before* pausing librespot: pausing fires a (possibly
  // delayed) `Stopped` event whose handler would otherwise clear the local
  // now-playing state. The event loop suppresses librespot events while this
  // flag is set (see infra::player::events).
  app.lock().await.is_local_playback_active = true;

  // Pause native Spotify so librespot releases the output device.
  #[cfg(feature = "streaming")]
  {
    let streaming = app.lock().await.streaming_player.clone();
    if let Some(player) = streaming {
      player.pause();
    }
  }

  let player = match ensure_player(app).await {
    Some(player) => player,
    None => {
      // Failed to open the output device: relinquish the session we claimed.
      app.lock().await.is_local_playback_active = false;
      return; // error already surfaced
    }
  };

  // Tag reading and decoder construction are blocking file I/O — keep them off
  // the async executor.
  let decode_path = path.clone();
  let decode_player = Arc::clone(&player);
  let result = tokio::task::spawn_blocking(move || {
    let info = track_info_from_path(&decode_path);
    decode_player.play_file(&decode_path).map(|()| info)
  })
  .await;

  match result {
    Ok(Ok(info)) => {
      let display_name = info.name.clone();
      let mut app = app.lock().await;
      // Match the player's output level to the app's configured volume.
      player.set_volume(app.user_config.behavior.volume_percent as f32 / 100.0);
      app.is_local_playback_active = true;
      app.is_streaming_active = false;
      app.native_is_playing = Some(true);
      app.song_progress_ms = 0;
      app.seek_ms = None;
      app.native_track_info = Some(NativeTrackInfo {
        name: info.name,
        artists_display: info.artists.join(", "),
        album: info.album,
        duration_ms: info.duration_ms as u32,
        kind: NativeTrackKind::Track,
      });
      app.set_status_message(format!("\u{266a} {display_name}"), 4);
    }
    Ok(Err(e)) => {
      app.lock().await.is_local_playback_active = false;
      set_error(app, format!("Cannot play local file: {e}")).await;
    }
    Err(e) => {
      app.lock().await.is_local_playback_active = false;
      set_error(app, format!("Local playback task failed: {e}")).await;
    }
  }
}

/// Return the local player, creating it (opening the output device) on first
/// use. The device-open is blocking, so it runs on a blocking thread.
async fn ensure_player(app: &Arc<Mutex<App>>) -> Option<Arc<LocalPlayer>> {
  if let Some(player) = app.lock().await.local_player.clone() {
    return Some(player);
  }

  match tokio::task::spawn_blocking(LocalPlayer::new).await {
    Ok(Ok(player)) => {
      let player = Arc::new(player);
      app.lock().await.local_player = Some(Arc::clone(&player));
      Some(player)
    }
    Ok(Err(e)) => {
      set_error(app, format!("No audio output for local playback: {e}")).await;
      None
    }
    Err(e) => {
      set_error(app, format!("Audio output init failed: {e}")).await;
      None
    }
  }
}

/// Stop and drop the local player (releasing the device) and clear local
/// playback state, if it was active.
async fn teardown_local(app: &Arc<Mutex<App>>) {
  let mut app = app.lock().await;
  if let Some(player) = app.local_player.take() {
    player.stop();
    // `player` is dropped at end of scope; if it was the last reference the
    // keepalive thread exits and the output device is released.
  }
  if app.is_local_playback_active {
    app.is_local_playback_active = false;
    app.native_track_info = None;
    app.song_progress_ms = 0;
  }
}

async fn set_error(app: &Arc<Mutex<App>>, message: String) {
  app.lock().await.set_status_message(message, 6);
}
