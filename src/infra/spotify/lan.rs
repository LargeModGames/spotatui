//! Speakers on the local network that are not logged in yet, kept current by an
//! mDNS browse: Google Cast devices and Spotify Connect (zeroconf) speakers.

use super::zeroconf::SUPPORTED_TOKEN_TYPES;
use md5::{Digest, Md5};
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use serde_json::Value;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

const CAST_SERVICE: &str = "_googlecast._tcp.local.";
const CONNECT_SERVICE: &str = "_spotify-connect._tcp.local.";
/// Sonos rooms; across some networks only this record arrives, not the Connect one.
const SONOS_SERVICE: &str = "_sonos._tcp.local.";
const SONOS_ZEROCONF_PORT: u16 = 1400;
const SONOS_ZEROCONF_PATH: &str = "/spotifyzc";
/// Bit 0 of the `ca` capability mask: the device has a video output.
const VIDEO_OUT: u32 = 1;
const INFO_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq)]
pub struct CastDevice {
  pub name: String,
  pub address: SocketAddr,
  pub video: bool,
}

impl CastDevice {
  /// The Connect device id that the Spotify receiver registers: the MD5 of the name.
  pub fn connect_id(&self) -> String {
    Md5::digest(self.name.as_bytes())
      .iter()
      .map(|b| format!("{b:02x}"))
      .collect()
  }
}

/// A Spotify Connect speaker that answers zeroconf `getInfo` at `url`.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectSpeaker {
  pub name: String,
  pub url: String,
  pub device_id: String,
  /// The `deviceType` of `getInfo`, such as `SPEAKER` or `AVR`.
  pub device_type: String,
}

enum Entry {
  Cast(CastDevice),
  Speaker(ConnectSpeaker),
}

/// A device from the TXT name (`fn`), the capability mask (`ca`) and the first IPv4 address.
fn cast_device(
  name: Option<&str>,
  capabilities: Option<&str>,
  ip: Option<IpAddr>,
  port: u16,
) -> Option<CastDevice> {
  let name = name.filter(|name| !name.is_empty())?;
  let capabilities: u32 = capabilities.and_then(|ca| ca.parse().ok()).unwrap_or(0);
  Some(CastDevice {
    name: name.to_string(),
    address: SocketAddr::new(ip?, port),
    video: capabilities & VIDEO_OUT != 0,
  })
}

/// A speaker from its `getInfo` reply, when spotatui can log it in.
fn connect_speaker(url: &str, info: &Value) -> Option<ConnectSpeaker> {
  let token_type = info["tokenType"].as_str()?;
  if !SUPPORTED_TOKEN_TYPES.contains(&token_type) {
    return None;
  }
  let text = |key: &str| info[key].as_str().filter(|value| !value.is_empty());
  Some(ConnectSpeaker {
    name: text("remoteName")?.to_string(),
    url: url.to_string(),
    device_id: text("deviceID")?.to_string(),
    device_type: text("deviceType").unwrap_or("UNKNOWN").to_string(),
  })
}

fn first_ipv4(service: &ResolvedService) -> Option<IpAddr> {
  service
    .get_addresses_v4()
    .into_iter()
    .next()
    .map(IpAddr::V4)
}

fn resolve_cast(service: &ResolvedService) -> Option<Entry> {
  cast_device(
    service.get_property_val_str("fn"),
    service.get_property_val_str("ca"),
    first_ipv4(service),
    service.get_port(),
  )
  .map(Entry::Cast)
}

fn resolve_speaker(service: &ResolvedService) -> Option<Entry> {
  speaker_at(&format!(
    "http://{}:{}{}",
    first_ipv4(service)?,
    service.get_port(),
    service.get_property_val_str("CPath").unwrap_or("/")
  ))
}

fn resolve_sonos(service: &ResolvedService) -> Option<Entry> {
  speaker_at(&format!(
    "http://{}:{SONOS_ZEROCONF_PORT}{SONOS_ZEROCONF_PATH}",
    first_ipv4(service)?
  ))
}

fn speaker_at(url: &str) -> Option<Entry> {
  let info: Value = reqwest::blocking::Client::builder()
    .timeout(INFO_TIMEOUT)
    .build()
    .ok()?
    .get(url)
    .query(&[("action", "getInfo")])
    .send()
    .ok()?
    .error_for_status()
    .ok()?
    .json()
    .ok()?;
  connect_speaker(url, &info).map(Entry::Speaker)
}

fn found() -> MutexGuard<'static, HashMap<String, Entry>> {
  static FOUND: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
  FOUND
    .get_or_init(Default::default)
    .lock()
    .unwrap_or_else(|e| e.into_inner())
}

/// Starts the browse once; a later call tries again after a failed start.
pub fn start() {
  static DAEMON: Mutex<Option<ServiceDaemon>> = Mutex::new(None);
  let mut daemon = DAEMON.lock().unwrap_or_else(|e| e.into_inner());
  if daemon.is_none() {
    match browse() {
      Ok(started) => *daemon = Some(started),
      Err(e) => log::warn!("Speaker browse did not start: {e:#}"),
    }
  }
}

fn browse() -> anyhow::Result<ServiceDaemon> {
  let daemon = ServiceDaemon::new()?;
  watch(&daemon, CAST_SERVICE, resolve_cast)?;
  watch(&daemon, CONNECT_SERVICE, resolve_speaker)?;
  watch(&daemon, SONOS_SERVICE, resolve_sonos)?;
  Ok(daemon)
}

fn watch(
  daemon: &ServiceDaemon,
  service_type: &str,
  resolve: fn(&ResolvedService) -> Option<Entry>,
) -> anyhow::Result<()> {
  let events = daemon.browse(service_type)?;
  std::thread::Builder::new()
    .name(format!("browse {service_type}"))
    .spawn(move || {
      while let Ok(event) = events.recv() {
        match event {
          ServiceEvent::ServiceResolved(service) => {
            let entry = resolve(&service);
            let fullname = service.get_fullname().to_string();
            match entry {
              Some(entry) => found().insert(fullname, entry),
              None => found().remove(&fullname),
            };
          }
          ServiceEvent::ServiceRemoved(_, fullname) => {
            found().remove(&fullname);
          }
          _ => {}
        }
      }
    })?;
  Ok(())
}

/// The Cast devices seen so far, by name.
pub fn cast_devices() -> Vec<CastDevice> {
  let mut devices: Vec<CastDevice> = found()
    .values()
    .filter_map(|entry| match entry {
      Entry::Cast(device) => Some(device.clone()),
      Entry::Speaker(_) => None,
    })
    .collect();
  devices.sort_by(|a, b| a.name.cmp(&b.name));
  devices
}

/// The Spotify Connect speakers seen so far, by name.
pub fn connect_speakers() -> Vec<ConnectSpeaker> {
  let speakers = found()
    .values()
    .filter_map(|entry| match entry {
      Entry::Speaker(speaker) => Some(speaker.clone()),
      Entry::Cast(_) => None,
    })
    .collect();
  one_per_device(speakers)
}

/// A speaker that two services announce, such as a Sonos room, is listed once.
fn one_per_device(mut speakers: Vec<ConnectSpeaker>) -> Vec<ConnectSpeaker> {
  speakers.sort_by(|a, b| (&a.name, &a.url).cmp(&(&b.name, &b.url)));
  let mut seen = std::collections::HashSet::new();
  speakers.retain(|speaker| seen.insert(speaker.device_id.clone()));
  speakers
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  use std::net::Ipv4Addr;

  const IP: Option<IpAddr> = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)));

  #[test]
  fn the_connect_id_is_the_md5_of_the_name() {
    let device = cast_device(Some("abc"), None, IP, 8009).unwrap();
    assert_eq!(device.connect_id(), "900150983cd24fb0d6963f7d28e17f72");
  }

  #[test]
  fn the_capability_mask_tells_a_speaker_from_a_screen() {
    let speaker = cast_device(Some("Kitchen"), Some("198660"), IP, 8009).unwrap();
    let screen = cast_device(Some("Hub"), Some("231941"), IP, 8009).unwrap();
    assert!(!speaker.video);
    assert!(screen.video);
    assert_eq!(speaker.address, SocketAddr::new(IP.unwrap(), 8009));
  }

  #[test]
  fn a_record_without_a_name_or_an_address_is_no_device() {
    assert_eq!(cast_device(None, None, IP, 8009), None);
    assert_eq!(cast_device(Some(""), None, IP, 8009), None);
    assert_eq!(cast_device(Some("Kitchen"), None, None, 8009), None);
  }

  #[test]
  fn a_speaker_comes_from_its_get_info_reply() {
    let url = "http://192.168.1.30:35390/zc";
    let info = json!({
      "remoteName": "Living Room",
      "deviceID": "727bdb7f",
      "deviceType": "SPEAKER",
      "tokenType": "accesstoken",
    });
    assert_eq!(
      connect_speaker(url, &info),
      Some(ConnectSpeaker {
        name: "Living Room".into(),
        url: url.into(),
        device_id: "727bdb7f".into(),
        device_type: "SPEAKER".into(),
      })
    );
  }

  #[test]
  fn a_room_that_two_services_announce_is_listed_once() {
    let speaker = |name: &str, id: &str| ConnectSpeaker {
      name: name.into(),
      url: format!("http://192.168.1.40:1400/spotifyzc#{name}"),
      device_id: id.into(),
      device_type: "SPEAKER".into(),
    };
    let listed = one_per_device(vec![
      speaker("Office", "b"),
      speaker("Kitchen", "a"),
      speaker("Kitchen", "a"),
    ]);
    let names: Vec<&str> = listed.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Kitchen", "Office"]);
  }

  #[test]
  fn a_speaker_with_an_unsupported_token_type_is_not_listed() {
    let info =
      |token_type: &str| json!({ "remoteName": "Room", "deviceID": "id", "tokenType": token_type });
    assert_eq!(connect_speaker("u", &info("default")), None);
    assert!(connect_speaker("u", &info("accesstoken")).is_some());
    assert!(connect_speaker("u", &info("authorization_code")).is_some());
    assert_eq!(
      connect_speaker(
        "u",
        &json!({ "deviceID": "id", "tokenType": "accesstoken" })
      ),
      None
    );
  }
}
