#![cfg_attr(
  not(any(
    feature = "discord-rpc",
    all(feature = "mpris", target_os = "linux"),
    all(feature = "macos-media", target_os = "macos"),
    all(feature = "windows-media", target_os = "windows")
  )),
  allow(dead_code)
)]

use crate::core::app::{App, NativePlaybackOrigin, NativeTrackKind};
use crate::tui::ui::util::create_artist_string;
use rspotify::model::{PlayableItem, RepeatState};
use rspotify::prelude::Id;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackItemKind {
  Track,
  Episode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackSource {
  NativeContext,
  NativeRawList,
  ExternalDevice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackMetadata {
  pub title: String,
  pub artists: Vec<String>,
  pub album: String,
  pub image_url: Option<String>,
  pub duration_ms: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackSnapshot {
  pub metadata: PlaybackMetadata,
  pub item_kind: PlaybackItemKind,
  pub item_id: Option<String>,
  pub item_uri: Option<String>,
  pub context_uri: Option<String>,
  pub source: PlaybackSource,
  pub progress_ms: u128,
  pub is_playing: bool,
  pub shuffle: bool,
  pub repeat: Option<RepeatState>,
}

impl PlaybackSnapshot {
  pub fn primary_artist(&self) -> String {
    self.metadata.artists.join(", ")
  }
}

pub fn current_playback_snapshot(app: &App) -> Option<PlaybackSnapshot> {
  // A non-Spotify decoded source (local / subsonic / internet-radio / youtube)
  // owns the audio sink while its `*_playback` field is `Some`. Starting such a
  // source only *pauses* librespot and never clears the Spotify context, so
  // without this branch the snapshot (window title, Discord RPC, and the
  // MPRIS/macOS fallback path) would keep showing the stale paused Spotify
  // track. Progress and play-state are read live from the owning source's
  // player, so they stay correct regardless of librespot's frozen position.
  #[cfg(any(
    feature = "local-files",
    feature = "subsonic",
    feature = "internet-radio",
    feature = "youtube"
  ))]
  if let Some(snapshot) = source_playback_snapshot(app) {
    return Some(snapshot);
  }

  let context = app.current_playback_context.as_ref();
  let use_native_metadata = app.is_streaming_active && app.native_track_info.is_some();

  let (metadata, item_kind, item_id, item_uri) = if use_native_metadata {
    let native_info = app.native_track_info.as_ref()?;
    let item_kind = match native_info.kind {
      NativeTrackKind::Track => PlaybackItemKind::Track,
      NativeTrackKind::Episode => PlaybackItemKind::Episode,
    };
    let item_id = app.last_track_id.clone();
    let item_uri = item_id
      .as_deref()
      .map(|id| playback_uri_for_item_kind(item_kind, id));
    (
      PlaybackMetadata {
        title: native_info.name.clone(),
        artists: vec![native_info.artists_display.clone()],
        album: native_info.album.clone(),
        image_url: image_url_from_context_item(context.and_then(|ctx| ctx.item.as_ref())),
        duration_ms: native_info.duration_ms,
      },
      item_kind,
      item_id,
      item_uri,
    )
  } else {
    metadata_and_identity_from_context_item(context.and_then(|ctx| ctx.item.as_ref()))?
  };

  let is_playing = if use_native_metadata {
    app
      .native_is_playing
      .unwrap_or_else(|| context.map(|context| context.is_playing).unwrap_or(false))
  } else {
    context.map(|context| context.is_playing).unwrap_or(false)
  };
  let shuffle = context
    .map(|context| context.shuffle_state)
    .unwrap_or(app.user_config.behavior.shuffle_enabled);
  let repeat = context.map(|context| context.repeat_state);
  let context_uri = context
    .and_then(|ctx| ctx.context.as_ref())
    .map(|context| context.uri.clone());
  let source = if app.is_streaming_active {
    match app.native_playback_origin.unwrap_or_else(|| {
      if context_uri.is_some() {
        NativePlaybackOrigin::Context
      } else {
        NativePlaybackOrigin::RawList
      }
    }) {
      NativePlaybackOrigin::Context => PlaybackSource::NativeContext,
      NativePlaybackOrigin::RawList => PlaybackSource::NativeRawList,
    }
  } else {
    PlaybackSource::ExternalDevice
  };

  Some(PlaybackSnapshot {
    metadata,
    item_kind,
    item_id,
    item_uri,
    context_uri,
    source,
    progress_ms: app.song_progress_ms,
    is_playing,
    shuffle,
    repeat,
  })
}

/// Build a [`PlaybackSnapshot`] for whichever non-Spotify decoded source
/// currently owns playback (at most one `*_playback` is `Some`). Metadata comes
/// from the source's stored track info; progress and play-state are read live
/// from its player. Returns `None` when no such source is active.
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "internet-radio",
  feature = "youtube"
))]
fn source_playback_snapshot(app: &App) -> Option<PlaybackSnapshot> {
  // The native queue slot playing a decoded track wins over every per-source
  // context: it is what is actually audible, and it drives the playbar / MPRIS /
  // cover art / lyrics via the shared track-change detector.
  #[cfg(feature = "audio-decode")]
  if let Some(crate::infra::queue::QueueNowPlaying::Decoded(d)) = app.queue_now.as_ref() {
    return Some(source_snapshot(
      d.track.name.clone(),
      d.track.artists.clone(),
      d.track.album.clone(),
      d.track.duration_ms as u32,
      d.track.uri.clone(),
      d.track.image_url.clone(),
      d.player.position().as_millis(),
      !d.player.is_paused(),
      app,
    ));
  }

  #[cfg(feature = "local-files")]
  if let Some(local) = app.local_playback.as_ref() {
    return Some(source_snapshot(
      local.name.clone(),
      vec![local.artists.clone()],
      local.album.clone(),
      local.duration_ms as u32,
      local.queue.get(local.index).cloned(),
      // Local files carry embedded art (read via `extract_embedded_cover`), not
      // a URL, so the snapshot's `image_url` stays `None`.
      None,
      local.player.position().as_millis(),
      !local.player.is_paused(),
      app,
    ));
  }

  #[cfg(feature = "subsonic")]
  if let Some(subsonic) = app.subsonic_playback.as_ref() {
    let track = subsonic.tracks.get(subsonic.index)?;
    return Some(source_snapshot(
      track.name.clone(),
      track.artists.clone(),
      track.album.clone(),
      track.duration_ms as u32,
      track.uri.clone(),
      track.image_url.clone(),
      subsonic.player.position().as_millis(),
      !subsonic.player.is_paused(),
      app,
    ));
  }

  #[cfg(feature = "youtube")]
  if let Some(youtube) = app.youtube_playback.as_ref() {
    let track = youtube.tracks.get(youtube.index)?;
    return Some(source_snapshot(
      track.name.clone(),
      track.artists.clone(),
      track.album.clone(),
      track.duration_ms as u32,
      track.uri.clone(),
      track.image_url.clone(),
      youtube.player.position().as_millis(),
      !youtube.player.is_paused(),
      app,
    ));
  }

  #[cfg(feature = "internet-radio")]
  if let Some(radio) = app.radio_playback.as_ref() {
    // Prefer the live ICY "Artist - Title" when the stream provides it; fall
    // back to the station's own tags. A live stream has no track duration.
    let artists = radio.now_playing_title().unwrap_or_else(|| {
      if radio.station.artists.is_empty() {
        radio.station.album.clone()
      } else {
        radio.station.artists.join(", ")
      }
    });
    return Some(source_snapshot(
      radio.station.name.clone(),
      vec![artists],
      radio.station.album.clone(),
      0,
      radio.station.uri.clone(),
      // Live radio streams carry no per-track cover art.
      None,
      radio.player.position().as_millis(),
      !radio.player.is_paused(),
      app,
    ));
  }

  None
}

/// Assemble a [`PlaybackSnapshot`] from a decoded source's fields. `image_url`
/// is a directly-fetchable cover-art URL when the source provides one (Subsonic
/// getCoverArt, YouTube thumbnail) and `None` otherwise (local files carry
/// embedded art fetched separately; radio has none). Sources are always treated
/// as a single track and take shuffle from the user config (no per-source
/// shuffle state).
#[cfg(any(
  feature = "local-files",
  feature = "subsonic",
  feature = "internet-radio",
  feature = "youtube"
))]
#[allow(clippy::too_many_arguments)]
fn source_snapshot(
  title: String,
  artists: Vec<String>,
  album: String,
  duration_ms: u32,
  item_uri: Option<String>,
  image_url: Option<String>,
  progress_ms: u128,
  is_playing: bool,
  app: &App,
) -> PlaybackSnapshot {
  PlaybackSnapshot {
    metadata: PlaybackMetadata {
      title,
      artists,
      album,
      image_url,
      duration_ms,
    },
    item_kind: PlaybackItemKind::Track,
    item_id: None,
    item_uri,
    context_uri: None,
    source: PlaybackSource::ExternalDevice,
    progress_ms,
    is_playing,
    shuffle: app.user_config.behavior.shuffle_enabled,
    repeat: None,
  }
}

fn metadata_and_identity_from_context_item(
  item: Option<&PlayableItem>,
) -> Option<(
  PlaybackMetadata,
  PlaybackItemKind,
  Option<String>,
  Option<String>,
)> {
  match item? {
    PlayableItem::Track(track) => {
      let item_id = track.id.as_ref().map(|id| id.id().to_string());
      Some((
        PlaybackMetadata {
          title: track.name.clone(),
          artists: vec![create_artist_string(&track.artists)],
          album: track.album.name.clone(),
          image_url: track.album.images.first().map(|image| image.url.clone()),
          duration_ms: track.duration.num_milliseconds() as u32,
        },
        PlaybackItemKind::Track,
        item_id.clone(),
        item_id
          .as_deref()
          .map(|id| playback_uri_for_item_kind(PlaybackItemKind::Track, id)),
      ))
    }
    PlayableItem::Episode(episode) => {
      let item_id = Some(episode.id.id().to_string());
      Some((
        PlaybackMetadata {
          title: episode.name.clone(),
          artists: vec![episode.show.name.clone()],
          album: String::new(),
          image_url: episode.images.first().map(|image| image.url.clone()),
          duration_ms: episode.duration.num_milliseconds() as u32,
        },
        PlaybackItemKind::Episode,
        item_id.clone(),
        item_id
          .as_deref()
          .map(|id| playback_uri_for_item_kind(PlaybackItemKind::Episode, id)),
      ))
    }
    PlayableItem::Unknown(_) => None,
  }
}

fn playback_uri_for_item_kind(item_kind: PlaybackItemKind, id: &str) -> String {
  match item_kind {
    PlaybackItemKind::Track => format!("spotify:track:{id}"),
    PlaybackItemKind::Episode => format!("spotify:episode:{id}"),
  }
}

fn image_url_from_context_item(item: Option<&PlayableItem>) -> Option<String> {
  match item? {
    PlayableItem::Track(track) => track.album.images.first().map(|image| image.url.clone()),
    PlayableItem::Episode(episode) => episode.images.first().map(|image| image.url.clone()),
    PlayableItem::Unknown(_) => None,
  }
}

/// Static metadata for an active local-files playback session, decoupled from
/// the live [`crate::infra::local::LocalPlaybackState`] (which embeds an audio
/// device that cannot be constructed in a headless test). Used to drive the OS
/// media integrations (MPRIS / macOS Now Playing) when a local file owns the
/// playback session.
///
/// Compiled only when both the local-files source and an OS media integration
/// that consumes it (MPRIS on Linux, Now Playing on macOS) are enabled, so
/// other combos (e.g. local-files + discord-rpc alone) stay free of dead-code
/// warnings under `-D warnings`.
#[cfg(all(
  feature = "local-files",
  any(
    all(feature = "mpris", target_os = "linux"),
    all(feature = "macos-media", target_os = "macos")
  )
))]
#[derive(Clone, Debug, PartialEq)]
pub struct LocalMediaMetadata {
  pub title: String,
  pub artists: Vec<String>,
  pub album: String,
  pub duration_ms: u32,
}

/// Choose the metadata the OS media integration should display.
///
/// When a local file owns the playback session (`local` is `Some`), its static
/// metadata wins so media keys / Now Playing follow local playback instead of
/// the stale Spotify context. Otherwise the Spotify-derived `spotify` metadata
/// (if any) is used.
///
/// This is a pure function over plain fields so it is unit-testable without a
/// live D-Bus connection or an audio device.
#[cfg(all(
  feature = "local-files",
  any(
    all(feature = "mpris", target_os = "linux"),
    all(feature = "macos-media", target_os = "macos")
  )
))]
pub fn select_media_metadata(
  local: Option<LocalMediaMetadata>,
  spotify: Option<PlaybackMetadata>,
) -> Option<PlaybackMetadata> {
  match local {
    Some(local) => Some(PlaybackMetadata {
      title: local.title,
      artists: local.artists,
      album: local.album,
      image_url: None,
      duration_ms: local.duration_ms,
    }),
    None => spotify,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{NativePlaybackOrigin, NativeTrackInfo, NativeTrackKind};
  use chrono::{Duration, Utc};
  use rspotify::model::{
    context::{Actions, CurrentPlaybackContext},
    idtypes::{EpisodeId, ShowId},
    show::{FullEpisode, SimplifiedShow},
    track::FullTrack,
    CurrentlyPlayingType, Device, DeviceType, Image, PlayableItem, RepeatState, SimplifiedAlbum,
    SimplifiedArtist, Type,
  };
  use std::{collections::HashMap, sync::mpsc::channel, time::SystemTime};

  fn app() -> App {
    let (tx, _rx) = channel();
    App::new(
      tx,
      crate::core::user_config::UserConfig::new(),
      Some(SystemTime::now()),
    )
  }

  #[allow(deprecated)]
  fn playback_context(item: PlayableItem, is_playing: bool) -> CurrentPlaybackContext {
    CurrentPlaybackContext {
      device: Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk Speaker".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(42),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: true,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing,
      item: Some(item),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    }
  }

  #[allow(deprecated)]
  fn track() -> FullTrack {
    FullTrack {
      album: SimplifiedAlbum {
        name: "Album".to_string(),
        images: vec![Image {
          height: Some(640),
          url: "https://example.com/cover.jpg".to_string(),
          width: Some(640),
        }],
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: Duration::milliseconds(181_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: None,
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: "Track".to_string(),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: Type::Track,
    }
  }

  #[allow(deprecated)]
  fn episode() -> FullEpisode {
    FullEpisode {
      audio_preview_url: None,
      description: "Description".to_string(),
      duration: Duration::milliseconds(2_400_000),
      explicit: false,
      external_urls: HashMap::new(),
      href: "https://example.com/episode".to_string(),
      id: EpisodeId::from_id("0zTOsY4qQhZQ6JcZx7aG4P")
        .unwrap()
        .into_static(),
      images: vec![Image {
        height: Some(640),
        url: "https://example.com/episode.jpg".to_string(),
        width: Some(640),
      }],
      is_externally_hosted: false,
      is_playable: true,
      language: "en".to_string(),
      languages: vec!["en".to_string()],
      name: "Episode".to_string(),
      release_date: "2024-01-01".to_string(),
      release_date_precision: rspotify::model::DatePrecision::Day,
      resume_point: None,
      show: SimplifiedShow {
        available_markets: Vec::new(),
        copyrights: Vec::new(),
        description: "Show description".to_string(),
        explicit: false,
        external_urls: HashMap::new(),
        href: "https://example.com/show".to_string(),
        id: ShowId::from_id("6mD5pBAZpHeQOdT0bFvB1V")
          .unwrap()
          .into_static(),
        images: Vec::new(),
        is_externally_hosted: None,
        languages: vec!["en".to_string()],
        media_type: "audio".to_string(),
        name: "Show".to_string(),
        publisher: "Publisher".to_string(),
      },
      r#type: Type::Episode,
    }
  }

  #[test]
  fn extracts_native_track_info() {
    let mut app = app();
    app.song_progress_ms = 12_000;
    app.is_streaming_active = true;
    app.native_is_playing = Some(true);
    app.native_track_info = Some(NativeTrackInfo {
      name: "Native Track".to_string(),
      artists_display: "Native Artist".to_string(),
      album: "Native Album".to_string(),
      duration_ms: 123_000,
      kind: NativeTrackKind::Track,
    });
    app.native_playback_origin = Some(NativePlaybackOrigin::RawList);

    let snapshot = current_playback_snapshot(&app).unwrap();

    assert_eq!(snapshot.metadata.title, "Native Track");
    assert_eq!(snapshot.metadata.artists, vec!["Native Artist"]);
    assert_eq!(snapshot.metadata.album, "Native Album");
    assert_eq!(snapshot.metadata.duration_ms, 123_000);
    assert_eq!(snapshot.item_kind, PlaybackItemKind::Track);
    assert_eq!(snapshot.source, PlaybackSource::NativeRawList);
    assert_eq!(snapshot.progress_ms, 12_000);
    assert!(snapshot.is_playing);
  }

  #[test]
  fn ignores_stale_native_play_state_for_api_metadata() {
    let mut app = app();
    app.native_is_playing = Some(false);
    app.current_playback_context = Some(playback_context(PlayableItem::Track(track()), true));

    let snapshot = current_playback_snapshot(&app).unwrap();

    assert_eq!(snapshot.metadata.title, "Track");
    assert!(snapshot.is_playing);
  }

  #[test]
  fn ignores_stale_native_metadata_when_streaming_is_inactive() {
    let mut app = app();
    app.native_is_playing = Some(false);
    app.native_track_info = Some(NativeTrackInfo {
      name: "Native Track".to_string(),
      artists_display: "Native Artist".to_string(),
      album: "Native Album".to_string(),
      duration_ms: 123_000,
      kind: NativeTrackKind::Track,
    });
    app.current_playback_context = Some(playback_context(PlayableItem::Track(track()), true));

    let snapshot = current_playback_snapshot(&app).unwrap();

    assert_eq!(snapshot.metadata.title, "Track");
    assert!(snapshot.is_playing);
  }

  #[test]
  fn extracts_spotify_track() {
    let mut app = app();
    app.song_progress_ms = 32_000;
    app.current_playback_context = Some(playback_context(PlayableItem::Track(track()), true));

    let snapshot = current_playback_snapshot(&app).unwrap();

    assert_eq!(snapshot.metadata.title, "Track");
    assert_eq!(snapshot.metadata.artists, vec!["Artist"]);
    assert_eq!(snapshot.metadata.album, "Album");
    assert_eq!(
      snapshot.metadata.image_url.as_deref(),
      Some("https://example.com/cover.jpg")
    );
    assert_eq!(snapshot.metadata.duration_ms, 181_000);
    assert!(snapshot.shuffle);
  }

  #[test]
  fn extracts_spotify_episode() {
    let mut app = app();
    app.current_playback_context = Some(playback_context(PlayableItem::Episode(episode()), false));

    let snapshot = current_playback_snapshot(&app).unwrap();

    assert_eq!(snapshot.metadata.title, "Episode");
    assert_eq!(snapshot.metadata.artists, vec!["Show"]);
    assert_eq!(snapshot.metadata.album, "");
    assert_eq!(
      snapshot.metadata.image_url.as_deref(),
      Some("https://example.com/episode.jpg")
    );
    assert_eq!(snapshot.metadata.duration_ms, 2_400_000);
    assert!(!snapshot.is_playing);
  }

  #[test]
  fn empty_playback_has_no_snapshot() {
    let app = app();

    assert_eq!(current_playback_snapshot(&app), None);
  }

  #[cfg(all(
    feature = "local-files",
    any(
      all(feature = "mpris", target_os = "linux"),
      all(feature = "macos-media", target_os = "macos")
    )
  ))]
  #[test]
  fn local_metadata_is_selected_when_local_active() {
    use super::{select_media_metadata, LocalMediaMetadata};

    let local = LocalMediaMetadata {
      title: "Local Song".to_string(),
      artists: vec!["Local Artist".to_string()],
      album: "Local Album".to_string(),
      duration_ms: 200_000,
    };
    let spotify = PlaybackMetadata {
      title: "Spotify Song".to_string(),
      artists: vec!["Spotify Artist".to_string()],
      album: "Spotify Album".to_string(),
      image_url: Some("https://example.com/cover.jpg".to_string()),
      duration_ms: 181_000,
    };

    let selected = select_media_metadata(Some(local), Some(spotify)).unwrap();

    assert_eq!(selected.title, "Local Song");
    assert_eq!(selected.artists, vec!["Local Artist"]);
    assert_eq!(selected.album, "Local Album");
    assert_eq!(selected.duration_ms, 200_000);
    // Local sessions carry no album art URL.
    assert_eq!(selected.image_url, None);
  }

  #[cfg(all(
    feature = "local-files",
    any(
      all(feature = "mpris", target_os = "linux"),
      all(feature = "macos-media", target_os = "macos")
    )
  ))]
  #[test]
  fn spotify_metadata_is_selected_when_local_inactive() {
    use super::select_media_metadata;

    let spotify = PlaybackMetadata {
      title: "Spotify Song".to_string(),
      artists: vec!["Spotify Artist".to_string()],
      album: "Spotify Album".to_string(),
      image_url: Some("https://example.com/cover.jpg".to_string()),
      duration_ms: 181_000,
    };

    let selected = select_media_metadata(None, Some(spotify.clone())).unwrap();

    assert_eq!(selected, spotify);
  }

  #[cfg(all(
    feature = "local-files",
    any(
      all(feature = "mpris", target_os = "linux"),
      all(feature = "macos-media", target_os = "macos")
    )
  ))]
  #[test]
  fn no_metadata_when_neither_source_active() {
    use super::select_media_metadata;

    assert_eq!(select_media_metadata(None, None), None);
  }
}
