use anyhow::{anyhow, Result};
use rspotify::model::idtypes::{PlayContextId, PlayableId};
use rspotify::prelude::Id;

const SPOTIFY_SERVICE_TYPES: [u32; 2] = [2311, 3079];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SonosSpotifyItem {
  pub spotify_uri: String,
  pub title: String,
  queue_track_offset: u32,
  attempts: Vec<SonosSpotifyAttempt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SonosSpotifyAttempt {
  pub enqueued_uri: String,
  pub enqueued_metadata: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SonosSpotifyPlaybackRequest {
  Context(SonosSpotifyItem),
  UriList {
    items: Vec<SonosSpotifyItem>,
    selected_index: u32,
  },
}

impl SonosSpotifyItem {
  pub fn attempts(&self) -> &[SonosSpotifyAttempt] {
    &self.attempts
  }

  pub fn queue_track_offset(&self) -> u32 {
    self.queue_track_offset
  }

  /// Build attempts using an account advertised by `/status/accounts`.
  /// `service_type` is the Sonos account Type (for example 2311), while the
  /// URI `sid` is the underlying service ID: `(service_type - 7) / 256`.
  pub fn attempts_for_account(
    &self,
    service_type: u32,
    serial_number: &str,
  ) -> Vec<SonosSpotifyAttempt> {
    let Some(service_id) = service_type
      .checked_sub(7)
      .filter(|value| value % 256 == 0)
      .map(|value| value / 256)
    else {
      return Vec::new();
    };
    let Ok((kind, _)) = parse_spotify_uri(&self.spotify_uri) else {
      return Vec::new();
    };
    account_attempts(
      &self.spotify_uri,
      &self.title,
      kind,
      service_type,
      service_id,
      serial_number,
    )
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpotifyKind {
  Album,
  Episode,
  Playlist,
  Show,
  Track,
}

struct SpotifyMagic {
  item_class: &'static str,
  item_id_prefix: &'static str,
  uri_prefixes: &'static [&'static str],
  legacy_uri_prefixes: &'static [&'static str],
  legacy_flags: u32,
}

pub fn playback_request_from_spotify(
  context_id: Option<&PlayContextId<'static>>,
  uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Result<SonosSpotifyPlaybackRequest> {
  if let Some(context) = context_id {
    let item = item_from_spotify_uri_with_queue_offset(
      &context.uri(),
      u32::try_from(offset.unwrap_or(0)).unwrap_or(u32::MAX),
    )?;
    return Ok(SonosSpotifyPlaybackRequest::Context(item));
  }

  if let Some(uris) = uris.filter(|uris| !uris.is_empty()) {
    let selected_index = offset.unwrap_or(0).min(uris.len().saturating_sub(1));
    let items = uris
      .iter()
      .map(|uri| item_from_spotify_uri(&uri.uri()))
      .collect::<Result<Vec<_>>>()?;
    return Ok(SonosSpotifyPlaybackRequest::UriList {
      items,
      selected_index: u32::try_from(selected_index).unwrap_or(u32::MAX),
    });
  }

  Err(anyhow!(
    "Sonos cannot start new Spotify playback until a track, album, playlist, show, or episode is selected"
  ))
}

pub fn item_from_spotify_uri(spotify_uri: &str) -> Result<SonosSpotifyItem> {
  item_from_spotify_uri_with_queue_offset(spotify_uri, 0)
}

fn item_from_spotify_uri_with_queue_offset(
  spotify_uri: &str,
  queue_track_offset: u32,
) -> Result<SonosSpotifyItem> {
  let (kind, _) = parse_spotify_uri(spotify_uri)?;
  let magic = spotify_magic(kind);
  let encoded_uri = percent_encode_colons(spotify_uri);
  let item_id = format!("{}{}", magic.item_id_prefix, encoded_uri);
  let title = spotify_uri.to_string();
  let mut attempts = Vec::new();

  for service_type in SPOTIFY_SERVICE_TYPES {
    let service_id = (service_type - 7) / 256;
    let metadata = sonos_metadata(magic.item_class, &item_id, &title, service_type);

    for prefix in magic.uri_prefixes {
      attempts.push(SonosSpotifyAttempt {
        enqueued_uri: format!("{prefix}{encoded_uri}?sid={service_id}&sn=0"),
        enqueued_metadata: metadata.clone(),
      });
      attempts.push(SonosSpotifyAttempt {
        enqueued_uri: format!("{prefix}{encoded_uri}"),
        enqueued_metadata: metadata.clone(),
      });
    }

    // Some S1 metadata uses the older container prefix and explicit flags.
    // Without an advertised account serial, keep the fallback internally
    // consistent by using the derived service ID and neutral serial zero.
    for prefix in magic.legacy_uri_prefixes {
      attempts.push(SonosSpotifyAttempt {
        enqueued_uri: format!(
          "{prefix}{encoded_uri}?sid={service_id}&flags={}&sn=0",
          magic.legacy_flags
        ),
        enqueued_metadata: metadata.clone(),
      });
    }
  }

  Ok(SonosSpotifyItem {
    spotify_uri: spotify_uri.to_string(),
    title,
    queue_track_offset,
    attempts,
  })
}

fn account_attempts(
  spotify_uri: &str,
  title: &str,
  kind: SpotifyKind,
  service_type: u32,
  service_id: u32,
  serial_number: &str,
) -> Vec<SonosSpotifyAttempt> {
  let magic = spotify_magic(kind);
  let encoded_uri = percent_encode_colons(spotify_uri);
  let item_id = format!("{}{}", magic.item_id_prefix, encoded_uri);
  let metadata = sonos_metadata(magic.item_class, &item_id, title, service_type);
  let mut attempts = Vec::new();

  for prefix in magic.uri_prefixes {
    attempts.push(SonosSpotifyAttempt {
      enqueued_uri: format!("{prefix}{encoded_uri}?sid={service_id}&sn={serial_number}"),
      enqueued_metadata: metadata.clone(),
    });
  }
  for prefix in magic.legacy_uri_prefixes {
    attempts.push(SonosSpotifyAttempt {
      enqueued_uri: format!(
        "{prefix}{encoded_uri}?sid={service_id}&flags={}&sn={serial_number}",
        magic.legacy_flags
      ),
      enqueued_metadata: metadata.clone(),
    });
  }
  attempts
}

fn parse_spotify_uri(spotify_uri: &str) -> Result<(SpotifyKind, &str)> {
  let mut parts = spotify_uri.split(':');
  match (parts.next(), parts.next(), parts.next(), parts.next()) {
    (Some("spotify"), Some(kind), Some(id), None)
      if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric()) =>
    {
      let kind = match kind {
        "album" => SpotifyKind::Album,
        "episode" => SpotifyKind::Episode,
        "playlist" => SpotifyKind::Playlist,
        "show" => SpotifyKind::Show,
        "track" => SpotifyKind::Track,
        _ => return Err(anyhow!("Unsupported Spotify item type for Sonos: {kind}")),
      };
      Ok((kind, id))
    }
    _ => Err(anyhow!("Unsupported Spotify URI for Sonos: {spotify_uri}")),
  }
}

fn spotify_magic(kind: SpotifyKind) -> SpotifyMagic {
  match kind {
    SpotifyKind::Album => SpotifyMagic {
      item_class: "object.container.album.musicAlbum",
      item_id_prefix: "00040000",
      uri_prefixes: &["x-rincon-cpcontainer:1004206c"],
      legacy_uri_prefixes: &["x-rincon-cpcontainer:0004206c"],
      legacy_flags: 8300,
    },
    SpotifyKind::Playlist => SpotifyMagic {
      item_class: "object.container.playlistContainer",
      item_id_prefix: "1006206c",
      uri_prefixes: &["x-rincon-cpcontainer:1006206c"],
      legacy_uri_prefixes: &["x-rincon-cpcontainer:1006206c"],
      legacy_flags: 8300,
    },
    SpotifyKind::Show => SpotifyMagic {
      item_class: "object.container.playlistContainer",
      item_id_prefix: "1006206c",
      uri_prefixes: &["x-rincon-cpcontainer:1006206c"],
      legacy_uri_prefixes: &["x-rincon-cpcontainer:1004206c"],
      legacy_flags: 8300,
    },
    SpotifyKind::Track | SpotifyKind::Episode => SpotifyMagic {
      item_class: "object.item.audioItem.musicTrack",
      item_id_prefix: "00032020",
      uri_prefixes: &["x-sonos-spotify:"],
      legacy_uri_prefixes: &["x-sonos-spotify:"],
      legacy_flags: 8224,
    },
  }
}

fn escape_xml(value: &str) -> String {
  value
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
    .replace('\'', "&apos;")
}

fn percent_encode_colons(value: &str) -> String {
  value.replace(':', "%3a")
}

fn sonos_metadata(item_class: &str, item_id: &str, title: &str, service_number: u32) -> String {
  let escaped_id = escape_xml(item_id);
  let escaped_title = escape_xml(title);
  let escaped_class = escape_xml(item_class);
  let service_token = format!("SA_RINCON{service_number}_X_#Svc{service_number}-0-Token");
  let escaped_service_token = escape_xml(&service_token);

  format!(
    r#"<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:r="urn:schemas-rinconnetworks-com:metadata-1-0/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"><item id="{escaped_id}" parentID="-1" restricted="true"><dc:title>{escaped_title}</dc:title><upnp:class>{escaped_class}</upnp:class><desc id="cdudn" nameSpace="urn:schemas-rinconnetworks-com:metadata-1-0/">{escaped_service_token}</desc></item></DIDL-Lite>"#
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn converts_track_uri_to_sonos_attempts() {
    let item = item_from_spotify_uri("spotify:track:abc123").unwrap();
    let attempts = item.attempts();

    assert_eq!(item.spotify_uri, "spotify:track:abc123");
    assert!(
      attempts.iter().any(
        |attempt| attempt.enqueued_uri == "x-sonos-spotify:spotify%3atrack%3aabc123?sid=9&sn=0"
      )
    );
    assert!(attempts.iter().any(|attempt| attempt.enqueued_uri
      == "x-sonos-spotify:spotify%3atrack%3aabc123?sid=9&flags=8224&sn=0"));
    assert!(attempts[0]
      .enqueued_metadata
      .contains("SA_RINCON2311_X_#Svc2311-0-Token"));
    assert!(attempts[0]
      .enqueued_metadata
      .contains("object.item.audioItem.musicTrack"));
  }

  #[test]
  fn uses_discovered_service_id_and_account_serial() {
    let item = item_from_spotify_uri("spotify:track:abc123").unwrap();
    let attempts = item.attempts_for_account(2311, "2");

    assert!(
      attempts.iter().any(
        |attempt| attempt.enqueued_uri == "x-sonos-spotify:spotify%3atrack%3aabc123?sid=9&sn=2"
      )
    );
    assert!(attempts.iter().all(|attempt| attempt
      .enqueued_metadata
      .contains("SA_RINCON2311_X_#Svc2311-0-Token")));
    assert!(item.attempts_for_account(2300, "2").is_empty());
  }

  #[test]
  fn converts_playlist_uri_to_sonos_container() {
    let item = item_from_spotify_uri("spotify:playlist:abc123").unwrap();

    assert!(item.attempts().iter().any(|attempt| attempt
      .enqueued_uri
      .contains("x-rincon-cpcontainer:1006206cspotify%3aplaylist%3aabc123")));
  }

  #[test]
  fn rejects_malformed_spotify_ids() {
    assert!(item_from_spotify_uri("spotify:track:a&b").is_err());
    assert!(item_from_spotify_uri("spotify:track:abc:extra").is_err());
  }

  #[test]
  fn context_playback_preserves_offset_for_queue_seek() {
    let context = PlayContextId::Album(
      rspotify::model::idtypes::AlbumId::from_id("0000000000000000000001")
        .unwrap()
        .into_static(),
    );

    let request = playback_request_from_spotify(Some(&context), None, Some(4)).unwrap();
    let SonosSpotifyPlaybackRequest::Context(item) = request else {
      panic!("expected context playback");
    };

    assert_eq!(item.spotify_uri, "spotify:album:0000000000000000000001");
    assert_eq!(item.queue_track_offset(), 4);
  }

  #[test]
  fn context_takes_precedence_and_preserves_playlist_continuation() {
    let context = PlayContextId::Album(
      rspotify::model::idtypes::AlbumId::from_id("0000000000000000000001")
        .unwrap()
        .into_static(),
    );
    let uris = vec![
      PlayableId::Track(
        rspotify::model::idtypes::TrackId::from_id("0000000000000000000002")
          .unwrap()
          .into_static(),
      ),
      PlayableId::Track(
        rspotify::model::idtypes::TrackId::from_id("0000000000000000000003")
          .unwrap()
          .into_static(),
      ),
    ];

    let request = playback_request_from_spotify(Some(&context), Some(&uris), Some(1)).unwrap();
    let SonosSpotifyPlaybackRequest::Context(item) = request else {
      panic!("expected context playback");
    };

    assert_eq!(item.spotify_uri, "spotify:album:0000000000000000000001");
    assert_eq!(item.queue_track_offset(), 1);
  }

  #[test]
  fn uri_list_keeps_all_tracks_and_selected_index() {
    let uris = vec![
      PlayableId::Track(
        rspotify::model::idtypes::TrackId::from_id("0000000000000000000002")
          .unwrap()
          .into_static(),
      ),
      PlayableId::Track(
        rspotify::model::idtypes::TrackId::from_id("0000000000000000000003")
          .unwrap()
          .into_static(),
      ),
    ];

    let request = playback_request_from_spotify(None, Some(&uris), Some(1)).unwrap();
    let SonosSpotifyPlaybackRequest::UriList {
      items,
      selected_index,
    } = request
    else {
      panic!("expected URI-list playback");
    };

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].spotify_uri, "spotify:track:0000000000000000000002");
    assert_eq!(items[1].spotify_uri, "spotify:track:0000000000000000000003");
    assert_eq!(selected_index, 1);
  }

  #[test]
  fn rejects_unsupported_types() {
    assert!(item_from_spotify_uri("spotify:artist:abc").is_err());
  }
}
