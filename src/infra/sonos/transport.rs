use crate::core::playback_target::SonosRoom;
use crate::infra::sonos::spotify::SonosSpotifyItem;
use anyhow::{anyhow, Context, Result};
use std::time::Duration;

const AV_TRANSPORT_URN: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const RENDERING_CONTROL_URN: &str = "urn:schemas-upnp-org:service:RenderingControl:1";

pub struct SonosTransport {
  client: reqwest::Client,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SonosPlaybackSnapshot {
  pub title: Option<String>,
  pub artist: Option<String>,
  pub album: Option<String>,
  pub track_uri: Option<String>,
  pub duration_ms: Option<u32>,
  pub position_ms: u32,
  pub is_playing: bool,
  pub volume_percent: Option<u8>,
}

impl SonosTransport {
  pub fn new() -> Result<Self> {
    Ok(Self {
      client: reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .context("failed to build Sonos HTTP client")?,
    })
  }

  pub async fn play_spotify_item(&self, room: &SonosRoom, item: &SonosSpotifyItem) -> Result<()> {
    match self
      .enqueue_spotify_item_with_mode(room, item, true)
      .await?
    {
      Some(first_track_number) if first_track_number > 0 => {
        let track_number = first_track_number.saturating_add(item.queue_track_offset());
        self.play_queue_track(room, track_number).await
      }
      _ => self.play(room).await,
    }
  }

  pub async fn enqueue_spotify_item(
    &self,
    room: &SonosRoom,
    item: &SonosSpotifyItem,
  ) -> Result<Option<u32>> {
    self.enqueue_spotify_item_with_mode(room, item, false).await
  }

  async fn enqueue_spotify_item_with_mode(
    &self,
    room: &SonosRoom,
    item: &SonosSpotifyItem,
    enqueue_as_next: bool,
  ) -> Result<Option<u32>> {
    let mut last_error = None;

    for attempt in item.attempts() {
      match self
        .add_uri_to_queue(
          room,
          &attempt.enqueued_uri,
          &attempt.enqueued_metadata,
          enqueue_as_next,
        )
        .await
      {
        Ok(first_track_number) => return Ok(first_track_number),
        Err(err) => last_error = Some(err),
      }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("Sonos rejected Spotify playback")))
  }

  pub async fn play(&self, room: &SonosRoom) -> Result<()> {
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "Play",
        "<Speed>1</Speed>",
      )
      .await
      .map(|_| ())
  }

  pub async fn pause(&self, room: &SonosRoom) -> Result<()> {
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "Pause",
        "",
      )
      .await
      .map(|_| ())
  }

  pub async fn next(&self, room: &SonosRoom) -> Result<()> {
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "Next",
        "",
      )
      .await
      .map(|_| ())
  }

  pub async fn previous(&self, room: &SonosRoom) -> Result<()> {
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "Previous",
        "",
      )
      .await
      .map(|_| ())
  }

  pub async fn seek(&self, room: &SonosRoom, position_ms: u32) -> Result<()> {
    let body = format!(
      "<Unit>REL_TIME</Unit><Target>{}</Target>",
      format_duration(position_ms)
    );
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "Seek",
        &body,
      )
      .await
      .map(|_| ())
  }

  pub async fn set_volume(&self, room: &SonosRoom, volume: u8) -> Result<()> {
    let body = format!(
      "<Channel>Master</Channel><DesiredVolume>{}</DesiredVolume>",
      volume.min(100)
    );
    self
      .soap(
        room,
        "MediaRenderer/RenderingControl/Control",
        RENDERING_CONTROL_URN,
        "SetVolume",
        &body,
      )
      .await
      .map(|_| ())
  }

  pub async fn volume(&self, room: &SonosRoom) -> Result<u8> {
    let response = self
      .soap(
        room,
        "MediaRenderer/RenderingControl/Control",
        RENDERING_CONTROL_URN,
        "GetVolume",
        "<Channel>Master</Channel>",
      )
      .await?;
    xml_text(&response, "CurrentVolume")
      .and_then(|value| value.parse::<u8>().ok())
      .map(|volume| volume.min(100))
      .ok_or_else(|| anyhow!("Sonos volume response did not include CurrentVolume"))
  }

  pub async fn now_playing(&self, room: &SonosRoom) -> Result<SonosPlaybackSnapshot> {
    let transport_response = self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "GetTransportInfo",
        "",
      )
      .await?;
    let position_response = self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "GetPositionInfo",
        "",
      )
      .await?;
    let volume_percent = self.volume(room).await.ok();
    let metadata = xml_text(&position_response, "TrackMetaData").filter(|value| {
      let trimmed = value.trim();
      !trimmed.is_empty() && trimmed != "NOT_IMPLEMENTED"
    });

    Ok(SonosPlaybackSnapshot {
      title: metadata
        .as_deref()
        .and_then(|xml| xml_text(xml, "dc:title"))
        .filter(|value| !value.trim().is_empty()),
      artist: metadata
        .as_deref()
        .and_then(sonos_artist_from_metadata)
        .filter(|value| !value.trim().is_empty()),
      album: metadata
        .as_deref()
        .and_then(|xml| xml_text(xml, "upnp:album"))
        .filter(|value| !value.trim().is_empty()),
      track_uri: xml_text(&position_response, "TrackURI").filter(|value| {
        let trimmed = value.trim();
        !trimmed.is_empty() && trimmed != "NOT_IMPLEMENTED"
      }),
      duration_ms: xml_text(&position_response, "TrackDuration")
        .as_deref()
        .and_then(parse_sonos_duration_ms),
      position_ms: xml_text(&position_response, "RelTime")
        .as_deref()
        .and_then(parse_sonos_duration_ms)
        .unwrap_or(0),
      is_playing: transport_state_is_playing(
        xml_text(&transport_response, "CurrentTransportState").as_deref(),
      ),
      volume_percent,
    })
  }

  async fn add_uri_to_queue(
    &self,
    room: &SonosRoom,
    enqueued_uri: &str,
    enqueued_metadata: &str,
    enqueue_as_next: bool,
  ) -> Result<Option<u32>> {
    let body = format!(
      "<EnqueuedURI>{}</EnqueuedURI><EnqueuedURIMetaData>{}</EnqueuedURIMetaData><DesiredFirstTrackNumberEnqueued>0</DesiredFirstTrackNumberEnqueued><EnqueueAsNext>{}</EnqueueAsNext>",
      escape_xml(enqueued_uri),
      escape_xml(enqueued_metadata),
      u8::from(enqueue_as_next)
    );
    let response = self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "AddURIToQueue",
        &body,
      )
      .await?;

    Ok(xml_text(&response, "FirstTrackNumberEnqueued").and_then(|value| value.parse::<u32>().ok()))
  }

  async fn play_queue_track(&self, room: &SonosRoom, one_based_track_number: u32) -> Result<()> {
    let queue_uri = format!("x-rincon-queue:{}#0", room.uuid);
    self.set_av_transport_uri(room, &queue_uri, "").await?;
    self.seek_track_number(room, one_based_track_number).await?;
    self.play(room).await
  }

  async fn set_av_transport_uri(&self, room: &SonosRoom, uri: &str, metadata: &str) -> Result<()> {
    let body = format!(
      "<CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
      escape_xml(uri),
      escape_xml(metadata)
    );
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "SetAVTransportURI",
        &body,
      )
      .await
      .map(|_| ())
  }

  async fn seek_track_number(&self, room: &SonosRoom, one_based_track_number: u32) -> Result<()> {
    let body = format!("<Unit>TRACK_NR</Unit><Target>{one_based_track_number}</Target>");
    self
      .soap(
        room,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        "Seek",
        &body,
      )
      .await
      .map(|_| ())
  }

  async fn soap(
    &self,
    room: &SonosRoom,
    control_path: &str,
    service_urn: &str,
    action: &str,
    action_body: &str,
  ) -> Result<String> {
    let url = control_url(&room.location, control_path)?;
    let envelope = soap_envelope(service_urn, action, action_body);
    let response = self
      .client
      .post(&url)
      .header("Content-Type", "text/xml; charset=\"utf-8\"")
      .header("SOAPACTION", format!("\"{service_urn}#{action}\""))
      .body(envelope)
      .send()
      .await
      .with_context(|| format!("failed to send Sonos {action} command to {}", room.name))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
      let detail = upnp_error_detail(&body).unwrap_or_else(|| body.trim().to_string());
      return Err(anyhow!(
        "Sonos room {} rejected {action}: HTTP {status}{}",
        room.name,
        if detail.is_empty() {
          String::new()
        } else {
          format!(" ({detail})")
        }
      ));
    }

    Ok(body)
  }
}

fn control_url(location: &str, control_path: &str) -> Result<String> {
  let parsed = url::Url::parse(location).context("invalid Sonos device description URL")?;
  let origin = parsed.origin().ascii_serialization();
  Ok(format!("{origin}/{control_path}"))
}

fn soap_envelope(service_urn: &str, action: &str, action_body: &str) -> String {
  format!(
    r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action} xmlns:u="{service_urn}"><InstanceID>0</InstanceID>{action_body}</u:{action}></s:Body></s:Envelope>"#
  )
}

fn escape_xml(value: &str) -> String {
  value
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
    .replace('\'', "&apos;")
}

fn xml_text(xml: &str, tag: &str) -> Option<String> {
  let start_tag = format!("<{tag}>");
  let end_tag = format!("</{tag}>");
  let start = xml.find(&start_tag)? + start_tag.len();
  let end = xml[start..].find(&end_tag)? + start;
  Some(unescape_xml(xml[start..end].trim()))
}

fn unescape_xml(value: &str) -> String {
  value
    .replace("&amp;", "&")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&quot;", "\"")
    .replace("&apos;", "'")
}

fn upnp_error_detail(xml: &str) -> Option<String> {
  let code = xml_text(xml, "errorCode")?;
  let description = xml_text(xml, "errorDescription").unwrap_or_default();
  if description.is_empty() {
    Some(format!("UPnP error {code}"))
  } else {
    Some(format!("UPnP error {code}: {description}"))
  }
}

fn sonos_artist_from_metadata(xml: &str) -> Option<String> {
  xml_text(xml, "dc:creator")
    .or_else(|| xml_text(xml, "upnp:artist"))
    .or_else(|| xml_text(xml, "r:albumArtist"))
}

fn transport_state_is_playing(state: Option<&str>) -> bool {
  matches!(
    state.unwrap_or_default().to_ascii_uppercase().as_str(),
    "PLAYING" | "TRANSITIONING"
  )
}

fn parse_sonos_duration_ms(value: &str) -> Option<u32> {
  let trimmed = value.trim();
  if trimmed.is_empty() || trimmed == "NOT_IMPLEMENTED" {
    return None;
  }

  let parts = trimmed.split(':').collect::<Vec<_>>();
  let [hours, minutes, seconds] = parts.as_slice() else {
    return None;
  };

  let hours = hours.parse::<u64>().ok()?;
  let minutes = minutes.parse::<u64>().ok()?;
  let (seconds, fractional) = seconds.split_once('.').unwrap_or((seconds, ""));
  let seconds = seconds.parse::<u64>().ok()?;
  let fractional_ms = if fractional.is_empty() {
    0
  } else {
    let mut millis = fractional.chars().take(3).collect::<String>();
    while millis.len() < 3 {
      millis.push('0');
    }
    millis.parse::<u64>().ok()?
  };
  let total_ms = hours
    .saturating_mul(3_600_000)
    .saturating_add(minutes.saturating_mul(60_000))
    .saturating_add(seconds.saturating_mul(1_000))
    .saturating_add(fractional_ms);
  u32::try_from(total_ms).ok()
}

fn format_duration(position_ms: u32) -> String {
  let total_seconds = position_ms / 1_000;
  let hours = total_seconds / 3_600;
  let minutes = (total_seconds % 3_600) / 60;
  let seconds = total_seconds % 60;
  format!("{hours}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn builds_control_url_from_device_description_location() {
    assert_eq!(
      control_url(
        "http://192.168.1.20:1400/xml/device_description.xml",
        "MediaRenderer/AVTransport/Control"
      )
      .unwrap(),
      "http://192.168.1.20:1400/MediaRenderer/AVTransport/Control"
    );
  }

  #[test]
  fn formats_sonos_seek_duration() {
    assert_eq!(format_duration(3_723_000), "1:02:03");
  }

  #[test]
  fn parses_add_to_queue_response() {
    let body = r#"<s:Envelope><s:Body><u:AddURIToQueueResponse><FirstTrackNumberEnqueued>7</FirstTrackNumberEnqueued></u:AddURIToQueueResponse></s:Body></s:Envelope>"#;

    assert_eq!(
      xml_text(body, "FirstTrackNumberEnqueued"),
      Some("7".to_string())
    );
  }

  #[test]
  fn parses_sonos_duration_ms() {
    assert_eq!(parse_sonos_duration_ms("0:01:23"), Some(83_000));
    assert_eq!(parse_sonos_duration_ms("1:02:03"), Some(3_723_000));
    assert_eq!(parse_sonos_duration_ms("0:00:01.500"), Some(1_500));
    assert_eq!(parse_sonos_duration_ms("NOT_IMPLEMENTED"), None);
  }

  #[test]
  fn parses_sonos_metadata_artist_fallbacks() {
    let metadata = r#"<DIDL-Lite><item><dc:title>Song</dc:title><upnp:artist>Artist</upnp:artist><upnp:album>Album</upnp:album></item></DIDL-Lite>"#;

    assert_eq!(xml_text(metadata, "dc:title"), Some("Song".to_string()));
    assert_eq!(
      sonos_artist_from_metadata(metadata),
      Some("Artist".to_string())
    );
    assert_eq!(xml_text(metadata, "upnp:album"), Some("Album".to_string()));
  }

  #[test]
  fn detects_playing_transport_states() {
    assert!(transport_state_is_playing(Some("PLAYING")));
    assert!(transport_state_is_playing(Some("TRANSITIONING")));
    assert!(!transport_state_is_playing(Some("STOPPED")));
    assert!(!transport_state_is_playing(None));
  }

  #[test]
  fn parses_upnp_error_detail() {
    let body = r#"<s:Envelope><s:Body><s:Fault><detail><UPnPError><errorCode>800</errorCode><errorDescription>Failed to queue item</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>"#;

    assert_eq!(
      upnp_error_detail(body),
      Some("UPnP error 800: Failed to queue item".to_string())
    );
  }
}
