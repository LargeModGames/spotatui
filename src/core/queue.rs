//! Native cross-source playback queue: source identity and suspension records.
//!
//! The queue itself is a `Vec<TrackInfo>` on [`App`](crate::core::app::App); this
//! module holds the small value types that classify a queue item by its source
//! (URI scheme) and record how to resume the underlying per-source context once
//! the queue drains. Phase 1 only populates and displays the queue — the
//! playback engine that consumes these types lands in Phase 2, so several items
//! here are intentionally unused until then.

/// Which source a queue item plays through, derived from its URI scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueItemSource {
  Spotify,
  LocalFile,
  Subsonic,
  YouTube,
}

/// Classify a queue item by its URI scheme. Anything that is not a local file,
/// Subsonic, or YouTube URI is treated as Spotify (the `spotify:track:` scheme).
/// Radio URIs (`radio:`) are never queued, so they are rejected before reaching
/// this function.
pub fn queue_item_source(uri: &str) -> QueueItemSource {
  if uri.starts_with("file:") {
    QueueItemSource::LocalFile
  } else if uri.starts_with("subsonic:") {
    QueueItemSource::Subsonic
  } else if uri.starts_with("youtube:") {
    QueueItemSource::YouTube
  } else {
    QueueItemSource::Spotify
  }
}

/// A short, human-readable tag for a queue item's source, shown in the Queue
/// screen next to each row.
pub fn source_label(source: QueueItemSource) -> &'static str {
  match source {
    QueueItemSource::Spotify => "Spotify",
    QueueItemSource::LocalFile => "Local",
    QueueItemSource::Subsonic => "Subsonic",
    QueueItemSource::YouTube => "YouTube",
  }
}

/// Whether this build can actually play a queue item from the given source.
/// A slim build (no source features) can only play Spotify tracks via native
/// streaming; each alternative source is gated on its own Cargo feature. Phase 2
/// consults this to skip unplayable items with a status message instead of
/// stalling the queue.
pub fn source_available(source: QueueItemSource) -> bool {
  match source {
    QueueItemSource::Spotify => cfg!(feature = "streaming"),
    QueueItemSource::LocalFile => cfg!(feature = "local-files"),
    QueueItemSource::Subsonic => cfg!(feature = "subsonic"),
    QueueItemSource::YouTube => cfg!(feature = "youtube"),
  }
}

/// How to resume the underlying per-source context after the native queue
/// drains. Recorded when a track is queued over an active context (Phase 2).
///
/// `resume_index: None` means the context was exhausted, so it should be torn
/// down rather than resumed. In a slim build (no source features) this enum has
/// zero variants — the `App` field is `Option<SuspendedContext>`, so that is a
/// valid, always-`None` type.
#[derive(Debug, Clone)]
pub enum SuspendedContext {
  // Constructed in Phase 3 (Spirc preemption); defined now so the enum is
  // stable and the resume path can already match it.
  #[cfg(feature = "streaming")]
  #[allow(dead_code)]
  Spotify {
    context_uri: Option<String>,
    resume_track_uri: Option<String>,
  },
  #[cfg(feature = "local-files")]
  Local {
    resume_index: Option<usize>,
    resume_position_ms: u64,
  },
  #[cfg(feature = "subsonic")]
  Subsonic {
    resume_index: Option<usize>,
    resume_position_ms: u64,
  },
  #[cfg(feature = "youtube")]
  YouTube {
    resume_index: Option<usize>,
    resume_position_ms: u64,
  },
  /// A live radio stream can't be paused/resumed, so resuming it means
  /// reconnecting. The suspended station row is kept to re-open the stream when
  /// the queue drains (the radio session itself is torn down at suspension so
  /// the queue slot can take the output device).
  #[cfg(feature = "internet-radio")]
  Radio {
    station: crate::core::plugin_api::TrackInfo,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn classifies_uri_schemes() {
    assert_eq!(
      queue_item_source("spotify:track:abc"),
      QueueItemSource::Spotify
    );
    assert_eq!(
      queue_item_source("file:///music/a.mp3"),
      QueueItemSource::LocalFile
    );
    assert_eq!(
      queue_item_source("subsonic:track:42"),
      QueueItemSource::Subsonic
    );
    assert_eq!(
      queue_item_source("youtube:5NV6Rdv1a3I"),
      QueueItemSource::YouTube
    );
    // Unknown schemes fall back to Spotify.
    assert_eq!(
      queue_item_source("something-else"),
      QueueItemSource::Spotify
    );
  }

  #[test]
  fn source_labels_are_stable() {
    assert_eq!(source_label(QueueItemSource::Spotify), "Spotify");
    assert_eq!(source_label(QueueItemSource::LocalFile), "Local");
    assert_eq!(source_label(QueueItemSource::Subsonic), "Subsonic");
    assert_eq!(source_label(QueueItemSource::YouTube), "YouTube");
  }
}
