use crate::core::app::App;
use crate::core::plugin_api::PlaybackState;

/// Discrete events delivered to plugins (mpv model: never per-tick polling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptEvent {
  Start,
  Quit,
  TrackChange,
  PlaybackStateChange,
  Seek,
  VolumeChange,
  QueueChange,
}

impl ScriptEvent {
  /// Lua-facing event name accepted by `spotatui.on`.
  pub(super) fn lua_name(self) -> &'static str {
    match self {
      ScriptEvent::Start => "start",
      ScriptEvent::Quit => "quit",
      ScriptEvent::TrackChange => "track_change",
      ScriptEvent::PlaybackStateChange => "playback_state_change",
      ScriptEvent::Seek => "seek",
      ScriptEvent::VolumeChange => "volume_change",
      ScriptEvent::QueueChange => "queue_change",
    }
  }

  /// Events that receive the current playback table (or nil) as their single argument.
  pub(super) fn passes_playback_arg(self) -> bool {
    matches!(
      self,
      ScriptEvent::TrackChange
        | ScriptEvent::PlaybackStateChange
        | ScriptEvent::Seek
        | ScriptEvent::VolumeChange
    )
  }
}

pub(super) const VALID_EVENT_NAMES: &[&str] = &[
  "start",
  "quit",
  "track_change",
  "playback_state_change",
  "seek",
  "volume_change",
  "queue_change",
];

/// Seek heuristic thresholds (Connect polling can legitimately jump a few seconds forward).
pub(super) const SEEK_BACKWARD_MS: i64 = 1500;
pub(super) const SEEK_FORWARD_MS: i64 = 6500;

/// Collect the queue item uris from `App` (currently-playing first, then upcoming).
pub(super) fn queue_uris(app: &App) -> Vec<String> {
  use crate::core::plugin_api::PlayableInfo;

  let Some(queue) = app.queue.as_ref() else {
    return Vec::new();
  };

  let item_uri = |item: &PlayableInfo| -> Option<String> {
    match item {
      PlayableInfo::Track(t) => t.uri.clone(),
      PlayableInfo::Episode(e) => e.uri.clone(),
    }
  };

  let mut uris = Vec::new();
  if let Some(current) = queue.currently_playing.as_ref() {
    if let Some(u) = item_uri(current) {
      uris.push(u);
    }
  }
  for item in &queue.queue {
    if let Some(u) = item_uri(item) {
      uris.push(u);
    }
  }
  uris
}

/// Pure diff of two snapshots into the set of events to emit. Order is fixed and testable.
pub(crate) fn diff_events(
  old: &Option<PlaybackState>,
  last_queue: &Option<Vec<String>>,
  new: &Option<PlaybackState>,
  new_queue: &Option<Vec<String>>,
) -> Vec<ScriptEvent> {
  let mut events = Vec::new();

  let old_identity = old.as_ref().and_then(track_identity);
  let new_identity = new.as_ref().and_then(track_identity);

  // Track change: identity becomes a different Some, or None -> Some.
  if let Some(new_id) = &new_identity {
    if old_identity.as_ref() != Some(new_id) {
      events.push(ScriptEvent::TrackChange);
    }
  }

  let old_playing = old.as_ref().map(|p| p.is_playing).unwrap_or(false);
  let new_playing = new.as_ref().map(|p| p.is_playing).unwrap_or(false);
  if old_playing != new_playing {
    events.push(ScriptEvent::PlaybackStateChange);
  }

  // Seek: same track, is_playing unchanged, progress jumped beyond tolerance.
  if let (Some(o), Some(n)) = (old, new) {
    let same_track = old_identity.is_some() && old_identity == new_identity;
    if same_track && o.is_playing == n.is_playing {
      let delta = n.progress_ms as i64 - o.progress_ms as i64;
      if !(-SEEK_BACKWARD_MS..=SEEK_FORWARD_MS).contains(&delta) {
        events.push(ScriptEvent::Seek);
      }
    }
  }

  // Volume change: differs and at least one side is Some.
  let old_vol = old.as_ref().and_then(|p| p.volume_percent);
  let new_vol = new.as_ref().and_then(|p| p.volume_percent);
  if old_vol != new_vol && (old_vol.is_some() || new_vol.is_some()) {
    events.push(ScriptEvent::VolumeChange);
  }

  // Queue change: uri list differs.
  if last_queue != new_queue {
    events.push(ScriptEvent::QueueChange);
  }

  events
}

/// Track identity for diffing: uri, falling back to name.
pub(super) fn track_identity(state: &PlaybackState) -> Option<String> {
  let track = state.track.as_ref()?;
  track.uri.clone().or_else(|| Some(track.name.clone()))
}
