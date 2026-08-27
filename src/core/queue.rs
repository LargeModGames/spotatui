//! Native cross-source playback queue: source identity and suspension records.
//!
//! The queue itself is a `Vec<TrackInfo>` on [`App`](crate::core::app::App); this
//! module holds the small value types that classify a queue item by its source
//! (URI scheme) and record how to resume the underlying per-source context once
//! the queue drains.

/// Which source a queue item plays through, derived from its URI scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueItemSource {
  Spotify,
  LocalFile,
  Subsonic,
  YouTube,
  Qobuz,
}

/// Classify a queue item by its URI scheme. Anything that is not a local file,
/// Subsonic, YouTube, or Qobuz URI is treated as Spotify (the `spotify:track:`
/// scheme). Radio URIs (`radio:`) are never queued, so they are rejected before
/// reaching this function.
pub fn queue_item_source(uri: &str) -> QueueItemSource {
  if uri.starts_with("file:") {
    QueueItemSource::LocalFile
  } else if uri.starts_with("subsonic:") {
    QueueItemSource::Subsonic
  } else if uri.starts_with("youtube:") {
    QueueItemSource::YouTube
  } else if uri.starts_with("qobuz:") {
    QueueItemSource::Qobuz
  } else {
    QueueItemSource::Spotify
  }
}

/// Whether a URI names something that can actually be queued or played.
///
/// [`queue_item_source`] classifies by scheme and treats everything it does not
/// recognise as Spotify, which is the right default for an item that already
/// came out of the player. It is the wrong default for a URI handed in from
/// outside: free text, an `https://open.spotify.com/...` link, and
/// `spotify:album:`/`spotify:playlist:` would all be classified Spotify and then
/// queued as if they were tracks.
///
/// So callers taking a URI from an agent gate on this first. `spotify:track:` is
/// the only Spotify form that names a single track; the other three schemes are
/// opaque handles minted by their own sources, which cannot be validated further
/// here. `radio:` is deliberately absent — a live stream is not a finite track
/// and `App::add_track_to_native_queue` rejects it.
///
/// A bare scheme with nothing behind it (`spotify:track:`, `file:`) names no
/// track at all, so the content after the prefix has to be non-empty: a model
/// that emits the prefix and stops must be told the argument is wrong rather
/// than have an empty handle queued for it.
#[cfg_attr(not(any(feature = "mcp-server", feature = "ai-dj")), allow(dead_code))]
pub fn is_playable_track_uri(uri: &str) -> bool {
  ["spotify:track:", "file:", "subsonic:", "youtube:", "qobuz:"]
    .iter()
    .any(|prefix| {
      uri
        .strip_prefix(prefix)
        .is_some_and(|rest| !rest.trim().is_empty())
    })
}

/// The Cargo feature that would make `uri`'s source playable, if this build is
/// missing it.
///
/// [`is_playable_track_uri`] answers "is this the right *kind* of URI", which is
/// a property of the scheme alone. Whether anything in *this* binary can consume
/// it is a second question: the per-source routers are each `#[cfg]`-gated, so an
/// `--features mcp-server` build accepts `youtube:` as well-formed and then has
/// nothing to play it with. Callers taking a URI from an agent ask both, so the
/// agent is told which build it is talking to rather than being told the track
/// started.
/// Spotify is deliberately never reported here: without `streaming` it still
/// plays through the Web API on an external Connect device, so
/// [`source_available`]'s stricter answer (which is about the *native* player)
/// would refuse a URI that works.
#[cfg_attr(not(any(feature = "mcp-server", feature = "ai-dj")), allow(dead_code))]
pub fn missing_source_feature(uri: &str) -> Option<&'static str> {
  let source = queue_item_source(uri);
  if source == QueueItemSource::Spotify || source_available(source) {
    return None;
  }
  Some(match source {
    QueueItemSource::LocalFile => "local-files",
    QueueItemSource::Subsonic => "subsonic",
    QueueItemSource::YouTube => "youtube",
    QueueItemSource::Qobuz => "qobuz",
    QueueItemSource::Spotify => unreachable!("returned above"),
  })
}

/// A short, human-readable tag for a queue item's source, shown in the Queue
/// screen next to each row.
pub fn source_label(source: QueueItemSource) -> &'static str {
  match source {
    QueueItemSource::Spotify => "Spotify",
    QueueItemSource::LocalFile => "Local",
    QueueItemSource::Subsonic => "Subsonic",
    QueueItemSource::YouTube => "YouTube",
    QueueItemSource::Qobuz => "Qobuz",
  }
}

/// Whether this build can actually play a queue item from the given source.
/// A slim build (no source features) can only play Spotify tracks via native
/// streaming; each alternative source is gated on its own Cargo feature. The queue
/// consults this to skip unplayable items with a status message instead of
/// stalling the queue.
pub fn source_available(source: QueueItemSource) -> bool {
  match source {
    QueueItemSource::Spotify => cfg!(feature = "streaming"),
    QueueItemSource::LocalFile => cfg!(feature = "local-files"),
    QueueItemSource::Subsonic => cfg!(feature = "subsonic"),
    QueueItemSource::YouTube => cfg!(feature = "youtube"),
    QueueItemSource::Qobuz => cfg!(feature = "qobuz"),
  }
}

/// How to resume the underlying per-source context after the native queue
/// drains. Recorded when a track is queued over an active context.
///
/// `resume_index: None` means the context was exhausted, so it should be torn
/// down rather than resumed. In a slim build (no source features) this enum has
/// zero variants — the `App` field is `Option<SuspendedContext>`, so that is a
/// valid, always-`None` type.
#[derive(Debug, Clone)]
pub enum SuspendedContext {
  /// Snapshot of the native-Spotify context to resume once the queue drains:
  /// the context uri and the resume-target track uri (the head of the Spotify
  /// mirror queue at suspension time).
  #[cfg(feature = "streaming")]
  Spotify {
    context_uri: Option<String>,
    resume_track_uri: Option<String>,
  },
  /// A native-Spotify client-side shuffle session
  /// ([`App::native_spotify_shuffle`](crate::core::app::App::native_spotify_shuffle)):
  /// resumes by index into the session's app-owned play order, with no context
  /// reload and no reshuffle. `generation` binds the resume to the session it was
  /// snapshotted from, so a session replaced while the queue drains cannot
  /// inherit a stale index (the resume handler applies the index only while the
  /// live session's generation still matches).
  #[cfg(feature = "streaming")]
  SpotifyShuffled {
    resume_index: Option<usize>,
    generation: u64,
    /// Context snapshot taken at suspension time (context uri + the mirror
    /// queue's head), so this can be converted to a [`Self::Spotify`] resume
    /// when the session it indexes into is invalidated while the queue plays
    /// (disconnect recovery, failed context fetch). Captured at creation:
    /// by conversion time `current_playback_context` may describe the queued
    /// track instead of the suspended context.
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
  #[cfg(feature = "qobuz")]
  Qobuz {
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
    assert_eq!(queue_item_source("qobuz:track:42"), QueueItemSource::Qobuz);
    // Unknown schemes fall back to Spotify.
    assert_eq!(
      queue_item_source("something-else"),
      QueueItemSource::Spotify
    );
  }

  #[test]
  fn only_single_track_uris_are_playable() {
    for uri in [
      "spotify:track:7o2AeQZzfCERsRmOM86EcB",
      "file:/home/jay/Music/a.flac",
      "subsonic:track:1",
      "youtube:5NV6Rdv1a3I",
      "qobuz:track:42",
    ] {
      assert!(is_playable_track_uri(uri), "{uri} should be playable");
    }
    // The cases that used to be fabricated into a track and reported as queued.
    // `queue_item_source` classifies every one of these as Spotify, which is
    // exactly why the queue path cannot gate on it alone.
    for uri in [
      "not-a-uri",
      "",
      "spotify:album:1DFixLWuPkv3KT3TnV35m3",
      "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M",
      "spotify:episode:512ojhOuo1ktJprKbVcKyQ",
      "spotify:artist:4tZwfgrHOc3mvqYlEYSvVi",
      "https://open.spotify.com/track/7o2AeQZzfCERsRmOM86EcB",
      "radio:https://example.com/stream.aac",
      // A bare scheme names no track: the prefix is right and there is nothing
      // behind it.
      "spotify:track:",
      "spotify:track:   ",
      "file:",
      "subsonic:",
      "youtube:",
      "qobuz:",
    ] {
      assert!(!is_playable_track_uri(uri), "{uri} should not be playable");
    }
  }

  #[test]
  fn source_labels_are_stable() {
    assert_eq!(source_label(QueueItemSource::Spotify), "Spotify");
    assert_eq!(source_label(QueueItemSource::LocalFile), "Local");
    assert_eq!(source_label(QueueItemSource::Subsonic), "Subsonic");
    assert_eq!(source_label(QueueItemSource::YouTube), "YouTube");
    assert_eq!(source_label(QueueItemSource::Qobuz), "Qobuz");
  }

  #[test]
  fn spotify_is_never_reported_as_missing_a_feature() {
    // Without `streaming` a Spotify track still plays through the Web API on an
    // external Connect device, so gating it on `source_available` would refuse a
    // URI that works. This is the one scheme that must always pass.
    assert_eq!(missing_source_feature("spotify:track:abc"), None);
  }

  #[test]
  fn a_scheme_whose_source_is_not_compiled_in_names_the_feature() {
    // The point of the whole helper: `is_playable_track_uri` says these are
    // well-formed, but the routers that consume them are `#[cfg]`-gated, so an
    // `--features mcp-server` build would accept and then silently drop them.
    for (uri, feature, compiled_in) in [
      (
        "file:/music/a.flac",
        "local-files",
        cfg!(feature = "local-files"),
      ),
      ("subsonic:abc", "subsonic", cfg!(feature = "subsonic")),
      ("youtube:abc", "youtube", cfg!(feature = "youtube")),
      ("qobuz:track:1", "qobuz", cfg!(feature = "qobuz")),
    ] {
      assert!(is_playable_track_uri(uri), "{uri} should be well-formed");
      assert_eq!(
        missing_source_feature(uri),
        if compiled_in { None } else { Some(feature) },
        "{uri}"
      );
    }
  }
}
