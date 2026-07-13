use crate::core::playback_target::SonosRoom;
use crate::infra::sonos::spotify::SonosSpotifyItem;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const AV_TRANSPORT_URN: &str = "urn:schemas-upnp-org:service:AVTransport:1";
const RENDERING_CONTROL_URN: &str = "urn:schemas-upnp-org:service:RenderingControl:1";
const ZONE_GROUP_TOPOLOGY_URN: &str = "urn:schemas-upnp-org:service:ZoneGroupTopology:1";
const MAX_SONOS_RESPONSE_BYTES: usize = 1_048_576;
const SPOTIFY_ACCOUNT_CACHE_TTL: Duration = Duration::from_secs(300);
const SPOTIFY_ACCOUNT_FAILURE_CACHE_TTL: Duration = Duration::from_secs(30);

pub struct SonosTransport {
  client: reqwest::Client,
  coordinators: Mutex<HashMap<String, (SonosRoom, Instant)>>,
  spotify_accounts: Mutex<HashMap<String, (Vec<SpotifyAccount>, Instant)>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpotifyAccount {
  service_type: u32,
  serial_number: String,
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
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build Sonos HTTP client")?,
      coordinators: Mutex::new(HashMap::new()),
      spotify_accounts: Mutex::new(HashMap::new()),
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
    let mut attempts = Vec::new();
    if let Ok(accounts) = self.spotify_accounts_for_room(room).await {
      for account in accounts {
        attempts.extend(item.attempts_for_account(account.service_type, &account.serial_number));
      }
    }
    attempts.extend_from_slice(item.attempts());

    for attempt in &attempts {
      match self
        .add_uri_to_queue(
          room,
          &attempt.enqueued_uri,
          &attempt.enqueued_metadata,
          enqueue_as_next,
        )
        .await
      {
        Ok(Some(first_track_number)) if first_track_number > 0 => {
          return Ok(Some(first_track_number));
        }
        Ok(first_track_number) if !enqueue_as_next => return Ok(first_track_number),
        Ok(_) => {
          last_error = Some(anyhow!(
            "Sonos accepted the item but did not report its queue position"
          ));
        }
        Err(err) => last_error = Some(err),
      }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("Sonos rejected Spotify playback")))
  }

  pub async fn play(&self, room: &SonosRoom) -> Result<()> {
    self
      .av_transport_soap(room, "Play", "<Speed>1</Speed>")
      .await
      .map(|_| ())
  }

  pub async fn pause(&self, room: &SonosRoom) -> Result<()> {
    self.av_transport_soap(room, "Pause", "").await.map(|_| ())
  }

  pub async fn next(&self, room: &SonosRoom) -> Result<()> {
    self.av_transport_soap(room, "Next", "").await.map(|_| ())
  }

  pub async fn previous(&self, room: &SonosRoom) -> Result<()> {
    self
      .av_transport_soap(room, "Previous", "")
      .await
      .map(|_| ())
  }

  pub async fn seek(&self, room: &SonosRoom, position_ms: u32) -> Result<()> {
    let body = format!(
      "<Unit>REL_TIME</Unit><Target>{}</Target>",
      format_duration(position_ms)
    );
    self
      .av_transport_soap(room, "Seek", &body)
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
    let transport_response = self.av_transport_soap(room, "GetTransportInfo", "").await?;
    let position_response = self.av_transport_soap(room, "GetPositionInfo", "").await?;
    let volume_percent = self.volume(room).await.ok();
    let metadata = xml_text(&position_response, "TrackMetaData").filter(|value| {
      let trimmed = value.trim();
      !trimmed.is_empty() && trimmed != "NOT_IMPLEMENTED"
    });

    let track_uri = xml_text(&position_response, "TrackURI").filter(|value| {
      let trimmed = value.trim();
      !trimmed.is_empty() && trimmed != "NOT_IMPLEMENTED"
    });
    if let Some(account) = track_uri
      .as_deref()
      .and_then(spotify_account_from_track_uri)
    {
      let mut cache = self.spotify_accounts.lock().await;
      let (accounts, fetched_at) = cache
        .entry(room.uuid.clone())
        .or_insert_with(|| (Vec::new(), Instant::now()));
      if !accounts.contains(&account) {
        accounts.push(account);
      }
      *fetched_at = Instant::now();
    }

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
      track_uri,
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
    let response = self.av_transport_soap(room, "AddURIToQueue", &body).await?;

    Ok(xml_text(&response, "FirstTrackNumberEnqueued").and_then(|value| value.parse::<u32>().ok()))
  }

  async fn play_queue_track(&self, room: &SonosRoom, one_based_track_number: u32) -> Result<()> {
    let coordinator = self.av_transport_room(room).await;
    let queue_uri = format!("x-rincon-queue:{}#0", coordinator.uuid);
    self
      .set_av_transport_uri(&coordinator, &queue_uri, "")
      .await?;
    self
      .seek_track_number(&coordinator, one_based_track_number)
      .await?;
    self.play(&coordinator).await
  }

  async fn set_av_transport_uri(&self, room: &SonosRoom, uri: &str, metadata: &str) -> Result<()> {
    let body = format!(
      "<CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
      escape_xml(uri),
      escape_xml(metadata)
    );
    self
      .av_transport_soap(room, "SetAVTransportURI", &body)
      .await
      .map(|_| ())
  }

  async fn seek_track_number(&self, room: &SonosRoom, one_based_track_number: u32) -> Result<()> {
    let body = format!("<Unit>TRACK_NR</Unit><Target>{one_based_track_number}</Target>");
    self
      .av_transport_soap(room, "Seek", &body)
      .await
      .map(|_| ())
  }

  async fn av_transport_soap(
    &self,
    room: &SonosRoom,
    action: &str,
    action_body: &str,
  ) -> Result<String> {
    let coordinator = self.av_transport_room(room).await;
    self
      .soap(
        &coordinator,
        "MediaRenderer/AVTransport/Control",
        AV_TRANSPORT_URN,
        action,
        action_body,
      )
      .await
  }

  async fn av_transport_room(&self, room: &SonosRoom) -> SonosRoom {
    if let Some((coordinator, fetched_at)) = self.coordinators.lock().await.get(&room.uuid) {
      if fetched_at.elapsed() < Duration::from_secs(30) {
        return coordinator.clone();
      }
    }

    let Some(coordinator) = self
      .soap_without_instance(
        room,
        "ZoneGroupTopology/Control",
        ZONE_GROUP_TOPOLOGY_URN,
        "GetZoneGroupState",
        "",
      )
      .await
      .ok()
      .and_then(|response| coordinator_from_zone_group_state(room, &response))
    else {
      // Older firmware and transient topology failures can still accept direct
      // AVTransport commands. Do not cache the fallback so the next command can
      // retry coordinator resolution.
      return room.clone();
    };

    let fetched_at = Instant::now();
    let mut coordinators = self.coordinators.lock().await;
    coordinators.insert(room.uuid.clone(), (coordinator.clone(), fetched_at));
    coordinators.insert(coordinator.uuid.clone(), (coordinator.clone(), fetched_at));
    coordinator
  }

  async fn spotify_accounts_for_room(&self, room: &SonosRoom) -> Result<Vec<SpotifyAccount>> {
    if let Some((accounts, fetched_at)) = self.spotify_accounts.lock().await.get(&room.uuid) {
      let ttl = if accounts.is_empty() {
        SPOTIFY_ACCOUNT_FAILURE_CACHE_TTL
      } else {
        SPOTIFY_ACCOUNT_CACHE_TTL
      };
      if fetched_at.elapsed() < ttl {
        return Ok(accounts.clone());
      }
    }

    let url = control_url(&room.location, "status/accounts")?;
    let response = match self.client.get(url).send().await {
      Ok(response) => response,
      Err(error) => {
        self
          .spotify_accounts
          .lock()
          .await
          .insert(room.uuid.clone(), (Vec::new(), Instant::now()));
        return Err(
          anyhow!(error).context(format!("failed to read Sonos accounts from {}", room.name)),
        );
      }
    };
    if !response.status().is_success() {
      let status = response.status();
      self
        .spotify_accounts
        .lock()
        .await
        .insert(room.uuid.clone(), (Vec::new(), Instant::now()));
      return Err(anyhow!("Sonos account discovery returned {status}"));
    }
    let accounts =
      spotify_accounts_from_xml(&response_text_limited(response, MAX_SONOS_RESPONSE_BYTES).await?);
    self
      .spotify_accounts
      .lock()
      .await
      .insert(room.uuid.clone(), (accounts.clone(), Instant::now()));
    Ok(accounts)
  }

  async fn soap(
    &self,
    room: &SonosRoom,
    control_path: &str,
    service_urn: &str,
    action: &str,
    action_body: &str,
  ) -> Result<String> {
    let envelope = soap_envelope(service_urn, action, action_body);
    self
      .send_soap(room, control_path, service_urn, action, envelope)
      .await
  }

  async fn soap_without_instance(
    &self,
    room: &SonosRoom,
    control_path: &str,
    service_urn: &str,
    action: &str,
    action_body: &str,
  ) -> Result<String> {
    let envelope = soap_envelope_without_instance(service_urn, action, action_body);
    self
      .send_soap(room, control_path, service_urn, action, envelope)
      .await
  }

  async fn send_soap(
    &self,
    room: &SonosRoom,
    control_path: &str,
    service_urn: &str,
    action: &str,
    envelope: String,
  ) -> Result<String> {
    let url = control_url(&room.location, control_path)?;
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
    let body = response_text_limited(response, MAX_SONOS_RESPONSE_BYTES).await?;
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

async fn response_text_limited(
  mut response: reqwest::Response,
  max_bytes: usize,
) -> Result<String> {
  if response
    .content_length()
    .is_some_and(|length| length > max_bytes as u64)
  {
    return Err(anyhow!("Sonos response exceeded {max_bytes} bytes"));
  }
  let mut body = Vec::new();
  while let Some(chunk) = response
    .chunk()
    .await
    .context("failed to read Sonos response")?
  {
    if body.len().saturating_add(chunk.len()) > max_bytes {
      return Err(anyhow!("Sonos response exceeded {max_bytes} bytes"));
    }
    body.extend_from_slice(&chunk);
  }
  Ok(String::from_utf8_lossy(&body).into_owned())
}

fn coordinator_from_zone_group_state(
  selected_room: &SonosRoom,
  response: &str,
) -> Option<SonosRoom> {
  let state = xml_text(response, "ZoneGroupState")?;
  let selected_member = format!("UUID=\"{}\"", selected_room.uuid);
  let mut remaining = state.as_str();

  while let Some(group_start) = remaining.find("<ZoneGroup ") {
    remaining = &remaining[group_start..];
    let opening_end = remaining.find('>')? + 1;
    let group_end = remaining.find("</ZoneGroup>")? + "</ZoneGroup>".len();
    let opening = &remaining[..opening_end];
    let group = &remaining[..group_end];
    if group.contains(&selected_member) {
      let coordinator_uuid = xml_attribute(opening, "Coordinator")?;
      let mut members = group;
      while let Some(member_start) = members.find("<ZoneGroupMember ") {
        members = &members[member_start..];
        let member_end = members.find('>')? + 1;
        let member = &members[..member_end];
        if xml_attribute(member, "UUID").as_deref() == Some(coordinator_uuid.as_str()) {
          let location = xml_attribute(member, "Location")?;
          if !is_valid_sonos_control_location(&location) {
            return None;
          }
          return Some(SonosRoom {
            uuid: coordinator_uuid,
            name: xml_attribute(member, "ZoneName").unwrap_or_else(|| selected_room.name.clone()),
            location,
          });
        }
        members = &members[member_end..];
      }
      return None;
    }
    remaining = &remaining[group_end..];
  }
  None
}

fn spotify_account_from_track_uri(track_uri: &str) -> Option<SpotifyAccount> {
  if !track_uri.starts_with("x-sonos-spotify:") {
    return None;
  }
  let query = track_uri.split_once('?')?.1;
  let mut service_id = None;
  let mut serial_number = None;
  for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
    match key.as_ref() {
      "sid" => service_id = value.parse::<u32>().ok(),
      "sn" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
        serial_number = Some(value.into_owned());
      }
      _ => {}
    }
  }
  let service_type = service_id?.checked_mul(256)?.checked_add(7)?;
  let serial_number = serial_number?;
  matches!(service_type, 2311 | 3079).then_some(SpotifyAccount {
    service_type,
    serial_number,
  })
}

fn spotify_accounts_from_xml(xml: &str) -> Vec<SpotifyAccount> {
  let mut accounts = Vec::new();
  let mut remaining = xml;
  while let Some(start) = remaining.find("<Account ") {
    remaining = &remaining[start..];
    let Some(end) = remaining.find('>') else {
      break;
    };
    let tag = &remaining[..=end];
    let service_type = xml_attribute(tag, "Type").and_then(|value| value.parse::<u32>().ok());
    let serial_number = xml_attribute(tag, "SerialNum");
    let deleted = xml_attribute(tag, "Deleted").as_deref() == Some("1");
    if let (Some(service_type @ (2311 | 3079)), Some(serial_number)) = (service_type, serial_number)
    {
      if !deleted
        && !serial_number.is_empty()
        && serial_number.bytes().all(|byte| byte.is_ascii_digit())
      {
        accounts.push(SpotifyAccount {
          service_type,
          serial_number,
        });
      }
    }
    remaining = &remaining[end + 1..];
  }
  accounts
}

fn xml_attribute(tag: &str, name: &str) -> Option<String> {
  let prefix = format!("{name}=\"");
  let start = tag.find(&prefix)? + prefix.len();
  let end = tag[start..].find('"')? + start;
  Some(unescape_xml(&tag[start..end]))
}

fn control_url(location: &str, control_path: &str) -> Result<String> {
  if !is_valid_sonos_control_location(location) {
    return Err(anyhow!("invalid Sonos control location"));
  }
  let parsed = url::Url::parse(location).context("invalid Sonos device description URL")?;
  let origin = parsed.origin().ascii_serialization();
  Ok(format!("{origin}/{control_path}"))
}

fn is_valid_sonos_control_location(location: &str) -> bool {
  let Ok(parsed) = url::Url::parse(location) else {
    return false;
  };
  if parsed.scheme() != "http" {
    return false;
  }
  #[cfg(test)]
  if matches!(
    parsed.host(),
    Some(url::Host::Ipv4(ip)) if ip.is_loopback()
  ) {
    return true;
  }
  if parsed.port_or_known_default() != Some(1400) {
    return false;
  }
  match parsed.host() {
    Some(url::Host::Ipv4(ip)) => ip.is_private() || ip.is_link_local(),
    Some(url::Host::Ipv6(ip)) => ip.is_unique_local() || ip.is_unicast_link_local(),
    _ => false,
  }
}

fn soap_envelope(service_urn: &str, action: &str, action_body: &str) -> String {
  format!(
    r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action} xmlns:u="{service_urn}"><InstanceID>0</InstanceID>{action_body}</u:{action}></s:Body></s:Envelope>"#
  )
}

fn soap_envelope_without_instance(service_urn: &str, action: &str, action_body: &str) -> String {
  format!(
    r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action} xmlns:u="{service_urn}">{action_body}</u:{action}></s:Body></s:Envelope>"#
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
  if minutes >= 60 || seconds >= 60 {
    return None;
  }
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
    assert_eq!(parse_sonos_duration_ms("0:60:00"), None);
    assert_eq!(parse_sonos_duration_ms("0:00:60"), None);
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
  fn resolves_selected_group_member_to_coordinator() {
    let selected = SonosRoom {
      uuid: "RINCON_BEDROOM".to_string(),
      name: "Bedroom".to_string(),
      location: "http://192.168.1.21:1400/xml/device_description.xml".to_string(),
    };
    let response = r#"<s:Envelope><s:Body><u:GetZoneGroupStateResponse><ZoneGroupState>&lt;ZoneGroups&gt;&lt;ZoneGroup Coordinator=&quot;RINCON_LIVING&quot;&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_LIVING&quot; ZoneName=&quot;Living Room&quot; Location=&quot;http://192.168.1.20:1400/xml/device_description.xml&quot;/&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_BEDROOM&quot; ZoneName=&quot;Bedroom&quot; Location=&quot;http://192.168.1.21:1400/xml/device_description.xml&quot;/&gt;&lt;/ZoneGroup&gt;&lt;/ZoneGroups&gt;</ZoneGroupState></u:GetZoneGroupStateResponse></s:Body></s:Envelope>"#;

    let coordinator = coordinator_from_zone_group_state(&selected, response).unwrap();

    assert_eq!(coordinator.uuid, "RINCON_LIVING");
    assert_eq!(coordinator.name, "Living Room");
    assert_eq!(
      coordinator.location,
      "http://192.168.1.20:1400/xml/device_description.xml"
    );
  }

  #[test]
  fn rejects_non_lan_topology_coordinator_location() {
    let selected = SonosRoom {
      uuid: "RINCON_BEDROOM".to_string(),
      name: "Bedroom".to_string(),
      location: "http://192.168.1.21:1400/xml/device_description.xml".to_string(),
    };
    let response = r#"<ZoneGroupState>&lt;ZoneGroups&gt;&lt;ZoneGroup Coordinator=&quot;RINCON_LIVING&quot;&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_LIVING&quot; Location=&quot;http://8.8.8.8:1400/private&quot;/&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_BEDROOM&quot;/&gt;&lt;/ZoneGroup&gt;&lt;/ZoneGroups&gt;</ZoneGroupState>"#;

    assert_eq!(coordinator_from_zone_group_state(&selected, response), None);
    assert!(control_url("http://8.8.8.8:1400/private", "control").is_err());
    assert!(control_url("https://192.168.1.20:1400/private", "control").is_err());
  }

  #[test]
  fn infers_spotify_account_from_existing_track_uri() {
    assert_eq!(
      spotify_account_from_track_uri("x-sonos-spotify:spotify%3atrack%3aabc?sid=9&flags=8224&sn=2"),
      Some(SpotifyAccount {
        service_type: 2311,
        serial_number: "2".to_string(),
      })
    );
    assert_eq!(spotify_account_from_track_uri("https://example.com"), None);
  }

  #[test]
  fn parses_spotify_accounts() {
    let xml = r#"<ZPSupportInfo><Accounts><Account Type="2311" SerialNum="1"><UN>hidden</UN></Account><Account Type="3079" SerialNum="3"/><Account Type="44551" SerialNum="4"/><Account Type="2311" SerialNum="5" Deleted="1"/></Accounts></ZPSupportInfo>"#;

    assert_eq!(
      spotify_accounts_from_xml(xml),
      vec![
        SpotifyAccount {
          service_type: 2311,
          serial_number: "1".to_string(),
        },
        SpotifyAccount {
          service_type: 3079,
          serial_number: "3".to_string(),
        },
      ]
    );
  }

  #[tokio::test]
  async fn spotify_playback_routes_queue_sequence_through_coordinator() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}");
    let server_base = base.clone();
    let server = tokio::spawn(async move {
      let mut requests = Vec::new();
      for _ in 0..6 {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
          let mut chunk = [0_u8; 4096];
          let read = stream.read(&mut chunk).await.unwrap();
          if read == 0 {
            break;
          }
          request.extend_from_slice(&chunk[..read]);
          let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
          };
          let headers = String::from_utf8_lossy(&request[..header_end]);
          let content_length = headers
            .lines()
            .find_map(|line| {
              line
                .to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
          if request.len() >= header_end + 4 + content_length {
            break;
          }
        }

        let request = String::from_utf8_lossy(&request).into_owned();
        let body = if request.starts_with("GET /status/accounts ") {
          r#"<Accounts><Account Type="2311" SerialNum="2"/></Accounts>"#.to_string()
        } else if request.contains("GetZoneGroupState") {
          format!(
            r#"<GetZoneGroupStateResponse><ZoneGroupState>&lt;ZoneGroups&gt;&lt;ZoneGroup Coordinator=&quot;RINCON_COORD&quot;&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_COORD&quot; ZoneName=&quot;Living Room&quot; Location=&quot;{server_base}/xml/device_description.xml&quot;/&gt;&lt;ZoneGroupMember UUID=&quot;RINCON_MEMBER&quot;/&gt;&lt;/ZoneGroup&gt;&lt;/ZoneGroups&gt;</ZoneGroupState></GetZoneGroupStateResponse>"#
          )
        } else if request.contains("AddURIToQueue") {
          "<AddURIToQueueResponse><FirstTrackNumberEnqueued>4</FirstTrackNumberEnqueued></AddURIToQueueResponse>".to_string()
        } else {
          "<Response/>".to_string()
        };
        let response = format!(
          "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
          body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        requests.push(request);
      }
      requests
    });

    let room = SonosRoom {
      uuid: "RINCON_MEMBER".to_string(),
      name: "Bedroom".to_string(),
      location: format!("{base}/xml/device_description.xml"),
    };
    let transport = SonosTransport::new().unwrap();
    let item = crate::infra::sonos::spotify::item_from_spotify_uri("spotify:track:abc123").unwrap();

    transport.play_spotify_item(&room, &item).await.unwrap();
    let requests = server.await.unwrap();

    assert!(requests[0].starts_with("GET /status/accounts "));
    assert!(requests[1].contains("GetZoneGroupState"));
    assert!(requests[2].contains("AddURIToQueue"));
    assert!(requests[2].contains("sid=9&amp;sn=2"));
    assert!(requests[3].contains("SetAVTransportURI"));
    assert!(requests[3].contains("x-rincon-queue:RINCON_COORD#0"));
    assert!(requests[4].contains("<Unit>TRACK_NR</Unit><Target>4</Target>"));
    assert!(requests[5].contains("<u:Play"));
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
