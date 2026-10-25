//! The handle the App holds for the running Soloist child.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;

use super::protocol::{Command, Event, PositionAnchor, Repeat, Status};

/// What the WebSocket last said about Soloist.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
  pub logged_in: bool,
  pub is_active: bool,
  pub status: Status,
  pub item_is_episode: bool,
  pub position: Option<PositionAnchor>,
  pub volume: Option<u8>,
  pub shuffle: bool,
  pub repeat: Repeat,
}

impl Snapshot {
  fn apply(&mut self, event: &Event) {
    match event {
      Event::Auth {
        logged_in,
        is_active,
      } => {
        self.logged_in = *logged_in;
        self.is_active = *is_active;
      }
      Event::State(state) => {
        self.logged_in = true;
        self.is_active = state.is_active;
        self.status = state.status;
        self.item_is_episode = state.item_is_episode;
        self.position = state.position;
        self.volume = state.volume.or(self.volume);
        self.shuffle = state.shuffle;
        self.repeat = state.repeat;
      }
      Event::Track { is_episode, .. } => self.item_is_episode = *is_episode,
      Event::Playback(status) => self.status = *status,
      Event::Volume(volume) => self.volume = Some(*volume),
      Event::Device { is_active } => self.is_active = *is_active,
      Event::Context(_) => {}
      Event::Options { shuffle, repeat } => {
        self.shuffle = shuffle.unwrap_or(self.shuffle);
        self.repeat = repeat.unwrap_or(self.repeat);
      }
      Event::Position(anchor) => self.position = Some(*anchor),
      Event::Error(_) => {}
    }
  }
}

/// The stream Soloist reports in its log for the current track.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamQuality {
  pub codec: String,
  pub sample_rate: Option<u32>,
  pub bitrate: Option<u32>,
}

impl StreamQuality {
  pub fn is_lossless(&self) -> bool {
    self.codec == "flac"
  }

  /// `FLAC 44.1` or `Vorbis 320`, worded like the Qobuz badge.
  pub fn label(&self) -> String {
    if self.is_lossless() {
      return match self.sample_rate {
        Some(rate) if rate % 1000 == 0 => format!("FLAC {}", rate / 1000),
        Some(rate) => format!("FLAC {:.1}", f64::from(rate) / 1000.0),
        None => "FLAC".to_string(),
      };
    }
    let codec = match self.codec.as_str() {
      "vorbis" => "Vorbis".to_string(),
      "mp3" | "aac" => self.codec.to_ascii_uppercase(),
      other => other.to_string(),
    };
    match self.bitrate {
      Some(bitrate) => format!("{codec} {}", bitrate / 1000),
      None => codec,
    }
  }
}

const VOLUME_WINDOW: Duration = Duration::from_millis(150);
/// How long a volume echo that differs from the last request is ignored.
const VOLUME_ECHO: Duration = Duration::from_secs(1);

/// At most one volume send per window; the latest value wins.
#[derive(Debug, Default)]
pub struct VolumeGate {
  last_sent: Option<Instant>,
  pending: Option<u8>,
}

impl VolumeGate {
  /// The value to send now, or `None` when it waits for the window.
  pub fn offer(&mut self, volume: u8, now: Instant) -> Option<u8> {
    match self.last_sent {
      Some(sent) if now.saturating_duration_since(sent) < VOLUME_WINDOW => {
        self.pending = Some(volume);
        None
      }
      _ => {
        self.last_sent = Some(now);
        self.pending = None;
        Some(volume)
      }
    }
  }

  /// When the waiting value may go out.
  pub fn deadline(&self) -> Option<Instant> {
    self
      .pending
      .and(self.last_sent.map(|sent| sent + VOLUME_WINDOW))
  }

  pub fn due(&mut self, now: Instant) -> Option<u8> {
    let volume = self.pending?;
    (now >= self.deadline()?).then(|| {
      self.pending = None;
      self.last_sent = Some(now);
      volume
    })
  }
}

pub struct SoloistLink {
  device_id: String,
  commands: UnboundedSender<Command>,
  snapshot: Mutex<Snapshot>,
  wanted_volume: Mutex<Option<(u8, Instant)>>,
  quality: Mutex<Option<StreamQuality>>,
}

impl SoloistLink {
  pub fn new(device_id: String, commands: UnboundedSender<Command>) -> Self {
    Self {
      device_id,
      commands,
      snapshot: Mutex::new(Snapshot::default()),
      wanted_volume: Mutex::new(None),
      quality: Mutex::new(None),
    }
  }

  pub fn device_id(&self) -> &str {
    &self.device_id
  }

  pub fn send(&self, command: Command) {
    let _ = self.commands.send(command);
  }

  pub fn snapshot(&self) -> Snapshot {
    self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
  }

  /// Pause unless Soloist is already paused or idle: a pause there is refused.
  pub fn pause(&self) {
    if self.snapshot().status.is_playing() {
      self.send(Command::Pause);
    }
  }

  pub fn set_volume(&self, volume: u8) {
    if let Ok(mut wanted) = self.wanted_volume.lock() {
      *wanted = Some((volume, Instant::now()));
    }
    self.send(Command::SetVolume(volume));
  }

  /// Fold an event into the snapshot; a stale volume echo during a held key is dropped.
  pub fn apply(&self, event: &Event) {
    if let Event::Volume(volume) = event {
      if let Ok(mut wanted) = self.wanted_volume.lock() {
        match *wanted {
          Some((want, _)) if want == *volume => *wanted = None,
          Some((_, at)) if at.elapsed() < VOLUME_ECHO => return,
          _ => *wanted = None,
        }
      }
    }
    if let Ok(mut snapshot) = self.snapshot.lock() {
      snapshot.apply(event);
    }
  }

  pub fn quality(&self) -> Option<StreamQuality> {
    self.quality.lock().ok().and_then(|q| q.clone())
  }

  pub fn set_quality(&self, quality: StreamQuality) {
    if let Ok(mut slot) = self.quality.lock() {
      *slot = Some(quality);
    }
  }

  #[cfg(test)]
  pub fn test_pair(
    device_id: &str,
  ) -> (
    std::sync::Arc<Self>,
    tokio::sync::mpsc::UnboundedReceiver<Command>,
  ) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (
      std::sync::Arc::new(Self::new(device_id.to_string(), tx)),
      rx,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn volume_gate_sends_only_the_latest_value_per_window() {
    let start = Instant::now();
    let mut gate = VolumeGate::default();
    assert_eq!(gate.offer(10, start), Some(10));
    assert_eq!(gate.offer(11, start + Duration::from_millis(20)), None);
    assert_eq!(gate.offer(12, start + Duration::from_millis(40)), None);
    assert_eq!(gate.due(start + Duration::from_millis(100)), None);
    assert_eq!(gate.deadline(), Some(start + VOLUME_WINDOW));
    assert_eq!(gate.due(start + VOLUME_WINDOW), Some(12));
    assert_eq!(gate.deadline(), None);
  }

  #[test]
  fn volume_echo_during_a_held_key_does_not_move_the_snapshot() {
    let (link, _rx) = SoloistLink::test_pair("dev");
    link.set_volume(40);
    link.apply(&Event::Volume(35));
    assert_eq!(link.snapshot().volume, None);
    link.apply(&Event::Volume(40));
    assert_eq!(link.snapshot().volume, Some(40));
    link.apply(&Event::Volume(30));
    assert_eq!(link.snapshot().volume, Some(30));
  }

  #[test]
  fn pause_is_skipped_when_soloist_is_already_paused() {
    let (link, mut rx) = SoloistLink::test_pair("dev");
    link.pause();
    assert!(rx.try_recv().is_err());
    link.apply(&Event::Playback(Status::Playing));
    link.pause();
    assert_eq!(rx.try_recv().ok(), Some(Command::Pause));
  }

  #[test]
  fn quality_labels_follow_the_qobuz_wording() {
    let flac = StreamQuality {
      codec: "flac".to_string(),
      sample_rate: Some(44100),
      bitrate: Some(812_000),
    };
    assert_eq!(flac.label(), "FLAC 44.1");
    assert!(flac.is_lossless());
    let vorbis = StreamQuality {
      codec: "vorbis".to_string(),
      sample_rate: Some(44100),
      bitrate: Some(160_000),
    };
    assert_eq!(vorbis.label(), "Vorbis 160");
    assert!(!vorbis.is_lossless());
  }
}
