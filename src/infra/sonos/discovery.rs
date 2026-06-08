use crate::core::playback_target::SonosRoom;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

const SSDP_ADDR: &str = "239.255.255.250:1900";
const SONOS_ST: &str = "urn:schemas-upnp-org:device:ZonePlayer:1";
const DISCOVERY_TIMEOUT: Duration = Duration::from_millis(2_000);

pub async fn discover_rooms() -> Result<Vec<SonosRoom>> {
  let socket = UdpSocket::bind("0.0.0.0:0")
    .await
    .context("failed to bind SSDP socket")?;
  let request = format!(
    "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {SONOS_ST}\r\n\r\n"
  );

  socket
    .send_to(request.as_bytes(), SSDP_ADDR)
    .await
    .context("failed to send Sonos SSDP discovery request")?;

  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(2))
    .build()
    .context("failed to build Sonos discovery HTTP client")?;
  let deadline = Instant::now() + DISCOVERY_TIMEOUT;
  let mut buf = vec![0_u8; 4096];
  let mut rooms_by_uuid = HashMap::new();

  loop {
    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
      break;
    };

    let recv = tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await;
    let Ok(Ok((len, _))) = recv else {
      break;
    };
    let response = String::from_utf8_lossy(&buf[..len]);
    let Some(location) = ssdp_header(&response, "location") else {
      continue;
    };

    match room_from_device_description(&client, location).await {
      Ok(room) => {
        rooms_by_uuid.entry(room.uuid.clone()).or_insert(room);
      }
      Err(err) => log::debug!("skipping Sonos SSDP response at {location}: {err}"),
    }
  }

  let mut rooms = rooms_by_uuid.into_values().collect::<Vec<_>>();
  rooms.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
  Ok(rooms)
}

async fn room_from_device_description(
  client: &reqwest::Client,
  location: &str,
) -> Result<SonosRoom> {
  let body = client
    .get(location)
    .send()
    .await
    .with_context(|| format!("failed to fetch Sonos device description from {location}"))?
    .error_for_status()
    .with_context(|| format!("Sonos device description returned an error at {location}"))?
    .text()
    .await
    .context("failed to read Sonos device description")?;

  parse_device_description(location, &body)
}

pub fn parse_device_description(location: &str, xml: &str) -> Result<SonosRoom> {
  let name = xml_text(xml, "roomName")
    .or_else(|| xml_text(xml, "friendlyName"))
    .ok_or_else(|| anyhow!("Sonos device description did not include a room name"))?;
  let udn = xml_text(xml, "UDN")
    .ok_or_else(|| anyhow!("Sonos device description did not include a UDN"))?;
  let uuid = udn.strip_prefix("uuid:").unwrap_or(&udn).to_string();

  Ok(SonosRoom {
    uuid,
    name,
    location: location.to_string(),
  })
}

pub fn ssdp_header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
  response.lines().find_map(|line| {
    let (key, value) = line.split_once(':')?;
    key
      .trim()
      .eq_ignore_ascii_case(name)
      .then(|| value.trim())
      .filter(|value| !value.is_empty())
  })
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_ssdp_location_case_insensitively() {
    let response =
      "HTTP/1.1 200 OK\r\nLOCATION: http://192.168.1.20:1400/xml/device_description.xml\r\n\r\n";
    assert_eq!(
      ssdp_header(response, "location"),
      Some("http://192.168.1.20:1400/xml/device_description.xml")
    );
  }

  #[test]
  fn parses_room_description() {
    let xml =
      r#"<root><device><roomName>Living Room</roomName><UDN>uuid:RINCON_123</UDN></device></root>"#;
    let room =
      parse_device_description("http://192.168.1.20:1400/xml/device_description.xml", xml).unwrap();
    assert_eq!(room.name, "Living Room");
    assert_eq!(room.uuid, "RINCON_123");
    assert_eq!(
      room.location,
      "http://192.168.1.20:1400/xml/device_description.xml"
    );
  }

  #[test]
  fn unescapes_room_names() {
    let xml = r#"<root><device><roomName>Kitchen &amp; Dining</roomName><UDN>uuid:RINCON_456</UDN></device></root>"#;
    let room =
      parse_device_description("http://192.168.1.21:1400/xml/device_description.xml", xml).unwrap();

    assert_eq!(room.name, "Kitchen & Dining");
  }
}
