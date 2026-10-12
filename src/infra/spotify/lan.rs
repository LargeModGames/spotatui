//! Google Cast speakers on the local network, kept current by an mDNS browse.

use md5::{Digest, Md5};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, OnceLock};

const CAST_SERVICE: &str = "_googlecast._tcp.local.";
/// Bit 0 of the `ca` capability mask: the device has a video output.
const VIDEO_OUT: u32 = 1;

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

fn found() -> &'static Mutex<HashMap<String, CastDevice>> {
  static FOUND: OnceLock<Mutex<HashMap<String, CastDevice>>> = OnceLock::new();
  FOUND.get_or_init(Default::default)
}

/// Starts the browse once; later calls do nothing.
pub fn start() {
  static DAEMON: OnceLock<Option<ServiceDaemon>> = OnceLock::new();
  DAEMON.get_or_init(|| match browse() {
    Ok(daemon) => Some(daemon),
    Err(e) => {
      log::warn!("Cast device browse did not start: {e:#}");
      None
    }
  });
}

fn browse() -> anyhow::Result<ServiceDaemon> {
  let daemon = ServiceDaemon::new()?;
  let events = daemon.browse(CAST_SERVICE)?;
  std::thread::Builder::new()
    .name("cast-browse".into())
    .spawn(move || {
      while let Ok(event) = events.recv() {
        let mut found = found().lock().unwrap_or_else(|e| e.into_inner());
        match event {
          ServiceEvent::ServiceResolved(service) => {
            let ip = service
              .get_addresses_v4()
              .into_iter()
              .next()
              .map(IpAddr::V4);
            match cast_device(
              service.get_property_val_str("fn"),
              service.get_property_val_str("ca"),
              ip,
              service.get_port(),
            ) {
              Some(device) => {
                found.insert(service.get_fullname().to_string(), device);
              }
              None => {
                found.remove(service.get_fullname());
              }
            }
          }
          ServiceEvent::ServiceRemoved(_, fullname) => {
            found.remove(&fullname);
          }
          _ => {}
        }
      }
    })?;
  Ok(daemon)
}

/// The Cast devices seen so far, by name.
pub fn cast_devices() -> Vec<CastDevice> {
  let mut devices: Vec<CastDevice> = found()
    .lock()
    .unwrap_or_else(|e| e.into_inner())
    .values()
    .cloned()
    .collect();
  devices.sort_by(|a, b| a.name.cmp(&b.name));
  devices
}

#[cfg(test)]
mod tests {
  use super::*;
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
}
