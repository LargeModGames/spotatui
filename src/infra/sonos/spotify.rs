use anyhow::{anyhow, Result};
use rspotify::model::idtypes::{PlayContextId, PlayableId};
use rspotify::prelude::Id;

const SPOTIFY_SERVICE_NUMBERS: [u32; 2] = [2311, 3079];

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

impl SonosSpotifyItem {
  pub fn attempts(&self) -> &[SonosSpotifyAttempt] {
    &self.attempts
  }

  pub fn queue_track_offset(&self) -> u32 {
    self.queue_track_offset
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

pub fn item_from_playback_request(
  context_id: Option<&PlayContextId<'static>>,
  uris: Option<&[PlayableId<'static>]>,
  offset: Option<usize>,
) -> Result<SonosSpotifyItem> {
  let (spotify_uri, queue_track_offset) = if let Some(uris) = uris.filter(|uris| !uris.is_empty()) {
    let index = offset.unwrap_or(0).min(uris.len().saturating_sub(1));
    (
      uris
        .get(index)
        .map(Id::uri)
        .ok_or_else(|| anyhow!("No Spotify URI selected for Sonos playback"))?,
      0,
    )
  } else if let Some(context) = context_id {
    (context.uri(), offset.unwrap_or(0) as u32)
  } else {
    return Err(anyhow!(
      "Sonos cannot start a new Spotify item until a track, album, playlist, show, or episode is selected"
    ));
  };

  item_from_spotify_uri_with_queue_offset(&spotify_uri, queue_track_offset)
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

  for service_number in SPOTIFY_SERVICE_NUMBERS {
    let metadata = sonos_metadata(magic.item_class, &item_id, &title, service_number);

    for prefix in magic.uri_prefixes {
      attempts.push(SonosSpotifyAttempt {
        enqueued_uri: format!("{prefix}{encoded_uri}?sid={service_number}&sn=0"),
        enqueued_metadata: metadata.clone(),
      });
    }

    for prefix in magic.uri_prefixes {
      attempts.push(SonosSpotifyAttempt {
        enqueued_uri: format!("{prefix}{encoded_uri}"),
        enqueued_metadata: metadata.clone(),
      });
    }

    // Older Sonos firmware and some S1 systems have historically accepted the
    // sid=9/sn=7 Spotify URI form used by several UPnP integrations. Keep it as
    // a fallback so S1/S2 differences are handled without separate code paths.
    for prefix in magic.legacy_uri_prefixes {
      attempts.push(SonosSpotifyAttempt {
        enqueued_uri: format!(
          "{prefix}{encoded_uri}?sid=9&flags={}&sn=7",
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

fn parse_spotify_uri(spotify_uri: &str) -> Result<(SpotifyKind, &str)> {
  let mut parts = spotify_uri.split(':');
  match (parts.next(), parts.next(), parts.next()) {
    (Some("spotify"), Some(kind), Some(id)) if !id.is_empty() => {
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
    assert!(attempts.iter().any(
      |attempt| attempt.enqueued_uri == "x-sonos-spotify:spotify%3atrack%3aabc123?sid=2311&sn=0"
    ));
    assert!(attempts.iter().any(|attempt| attempt.enqueued_uri
      == "x-sonos-spotify:spotify%3atrack%3aabc123?sid=9&flags=8224&sn=7"));
    assert!(attempts[0]
      .enqueued_metadata
      .contains("SA_RINCON2311_X_#Svc2311-0-Token"));
    assert!(attempts[0]
      .enqueued_metadata
      .contains("object.item.audioItem.musicTrack"));
  }

  #[test]
  fn converts_playlist_uri_to_sonos_container() {
    let item = item_from_spotify_uri("spotify:playlist:a&b").unwrap();

    assert!(item.attempts().iter().any(|attempt| attempt
      .enqueued_uri
      .contains("x-rincon-cpcontainer:1006206cspotify%3aplaylist%3aa&b")));
    assert!(item.attempts()[0]
      .enqueued_metadata
      .contains("spotify%3aplaylist%3aa&amp;b"));
  }

  #[test]
  fn context_playback_preserves_offset_for_queue_seek() {
    let context = PlayContextId::Album(
      rspotify::model::idtypes::AlbumId::from_id("0000000000000000000001")
        .unwrap()
        .into_static(),
    );

    let item = item_from_playback_request(Some(&context), None, Some(4)).unwrap();

    assert_eq!(item.spotify_uri, "spotify:album:0000000000000000000001");
    assert_eq!(item.queue_track_offset(), 4);
  }

  #[test]
  fn uses_selected_uri_before_context() {
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

    let item = item_from_playback_request(Some(&context), Some(&uris), Some(1)).unwrap();

    assert_eq!(item.spotify_uri, "spotify:track:0000000000000000000003");
  }

  #[test]
  fn rejects_unsupported_types() {
    assert!(item_from_spotify_uri("spotify:artist:abc").is_err());
  }
}
