#[cfg(any(feature = "discord-rpc", feature = "mpris", feature = "macos-media"))]
use rspotify::model::PlayableItem;

/// Metadata extracted from the currently playing track/episode.
///
/// Used by Discord RPC, MPRIS, and macOS media integrations to avoid
/// duplicating PlayableItem match logic across three modules.
#[cfg(any(feature = "discord-rpc", feature = "mpris", feature = "macos-media"))]
pub struct PlaybackMetadata {
  pub title: String,
  pub artist: String,
  pub album: String,
  pub duration_ms: u32,
  pub art_url: Option<String>,
}

/// Extract track/episode metadata from a `PlayableItem`.
///
/// Returns `None` for items that aren't tracks or episodes (e.g. ad breaks).
#[cfg(any(feature = "discord-rpc", feature = "mpris", feature = "macos-media"))]
pub fn extract_playable_metadata(item: &PlayableItem) -> Option<PlaybackMetadata> {
  match item {
    PlayableItem::Track(track) => Some(PlaybackMetadata {
      title: track.name.clone(),
      artist: track
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", "),
      album: track.album.name.clone(),
      duration_ms: track.duration.num_milliseconds() as u32,
      art_url: track.album.images.first().map(|img| img.url.clone()),
    }),
    PlayableItem::Episode(episode) => Some(PlaybackMetadata {
      title: episode.name.clone(),
      artist: episode.show.name.clone(),
      album: String::new(),
      duration_ms: episode.duration.num_milliseconds() as u32,
      art_url: episode.images.first().map(|img| img.url.clone()),
    }),
    _ => None,
  }
}
