//! Google Cast speakers as Spotify Connect devices: a minimal CASTV2 sender
//! that starts the Spotify receiver app and logs it in to the account.

use super::lan::CastDevice;
use super::router::upstream_error;
use super::session::SpotifySession;
use anyhow::{anyhow, bail, Context as _};
use reqwest::Method;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

const SPOTIFY_APP: &str = "CC32E753";
const RECEIVER: &str = "receiver-0";
const CONNECTION_NS: &str = "urn:x-cast:com.google.cast.tp.connection";
const RECEIVER_NS: &str = "urn:x-cast:com.google.cast.receiver";
const HEARTBEAT_NS: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
const SPOTIFY_NS: &str = "urn:x-cast:com.spotify.chromecast.secure.v1";
const SENDER: &str = "sender-0";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Starting the receiver app on a speaker takes a few seconds.
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_MESSAGE: usize = 64 * 1024;

/// One CASTV2 frame with a UTF-8 payload.
#[derive(Debug, Default, PartialEq)]
struct CastMessage {
  source: String,
  destination: String,
  namespace: String,
  payload: String,
}

impl CastMessage {
  fn encode(&self) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint_field(&mut out, 1, 0);
    put_string_field(&mut out, 2, &self.source);
    put_string_field(&mut out, 3, &self.destination);
    put_string_field(&mut out, 4, &self.namespace);
    put_varint_field(&mut out, 5, 0);
    put_string_field(&mut out, 6, &self.payload);
    out
  }

  fn decode(mut bytes: &[u8]) -> Option<Self> {
    let mut message = Self::default();
    while !bytes.is_empty() {
      let key = take_varint(&mut bytes)?;
      match key & 7 {
        0 => {
          take_varint(&mut bytes)?;
        }
        2 => {
          let len = usize::try_from(take_varint(&mut bytes)?).ok()?;
          if len > bytes.len() {
            return None;
          }
          let (value, rest) = bytes.split_at(len);
          bytes = rest;
          let text = String::from_utf8_lossy(value).into_owned();
          match key >> 3 {
            2 => message.source = text,
            3 => message.destination = text,
            4 => message.namespace = text,
            6 => message.payload = text,
            _ => {}
          }
        }
        _ => return None,
      }
    }
    Some(message)
  }
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
  while value >= 0x80 {
    out.push(value as u8 | 0x80);
    value >>= 7;
  }
  out.push(value as u8);
}

fn put_varint_field(out: &mut Vec<u8>, field: u64, value: u64) {
  put_varint(out, field << 3);
  put_varint(out, value);
}

fn put_string_field(out: &mut Vec<u8>, field: u64, value: &str) {
  put_varint(out, field << 3 | 2);
  put_varint(out, value.len() as u64);
  out.extend_from_slice(value.as_bytes());
}

fn take_varint(bytes: &mut &[u8]) -> Option<u64> {
  let mut value = 0u64;
  for shift in (0..64).step_by(7) {
    let (&byte, rest) = bytes.split_first()?;
    *bytes = rest;
    value |= u64::from(byte & 0x7f) << shift;
    if byte & 0x80 == 0 {
      return Some(value);
    }
  }
  None
}

/// Cast devices present a self-signed certificate: the chain is not checked,
/// the handshake signature is.
#[derive(Debug)]
struct SelfSigned(Arc<CryptoProvider>);

impl ServerCertVerifier for SelfSigned {
  fn verify_server_cert(
    &self,
    _end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    _now: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    Ok(ServerCertVerified::assertion())
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
      message,
      cert,
      dss,
      &self.0.signature_verification_algorithms,
    )
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
      message,
      cert,
      dss,
      &self.0.signature_verification_algorithms,
    )
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.0.signature_verification_algorithms.supported_schemes()
  }
}

fn tls_config() -> anyhow::Result<ClientConfig> {
  let provider = Arc::new(ring::default_provider());
  Ok(
    ClientConfig::builder_with_provider(provider.clone())
      .with_safe_default_protocol_versions()?
      .dangerous()
      .with_custom_certificate_verifier(Arc::new(SelfSigned(provider)))
      .with_no_client_auth(),
  )
}

struct CastChannel {
  stream: TlsStream<TcpStream>,
}

impl CastChannel {
  /// Connects to the device and opens the virtual connection to its receiver.
  async fn open(address: SocketAddr) -> anyhow::Result<Self> {
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
      .await
      .map_err(|_| anyhow!("no answer from {address}"))??;
    let connector = TlsConnector::from(Arc::new(tls_config()?));
    let stream = tokio::time::timeout(
      CONNECT_TIMEOUT,
      connector.connect(ServerName::IpAddress(address.ip().into()), tcp),
    )
    .await
    .map_err(|_| anyhow!("no TLS handshake with {address}"))??;
    let mut channel = Self { stream };
    channel
      .send(CONNECTION_NS, RECEIVER, json!({ "type": "CONNECT" }))
      .await?;
    Ok(channel)
  }

  async fn send(
    &mut self,
    namespace: &str,
    destination: &str,
    payload: Value,
  ) -> anyhow::Result<()> {
    let frame = CastMessage {
      source: SENDER.into(),
      destination: destination.into(),
      namespace: namespace.into(),
      payload: payload.to_string(),
    }
    .encode();
    self
      .stream
      .write_all(&(frame.len() as u32).to_be_bytes())
      .await?;
    self.stream.write_all(&frame).await?;
    Ok(self.stream.flush().await?)
  }

  /// The next message that is not a heartbeat. Answers each ping on the way.
  async fn receive(&mut self) -> anyhow::Result<CastMessage> {
    loop {
      let mut len = [0u8; 4];
      self.stream.read_exact(&mut len).await?;
      let len = u32::from_be_bytes(len) as usize;
      if len > MAX_MESSAGE {
        bail!("a {len}-byte Cast message");
      }
      let mut frame = vec![0u8; len];
      self.stream.read_exact(&mut frame).await?;
      let message = CastMessage::decode(&frame).context("a malformed Cast message")?;
      if message.namespace != HEARTBEAT_NS {
        return Ok(message);
      }
      if message.payload.contains("\"PING\"") {
        self
          .send(HEARTBEAT_NS, &message.source, json!({ "type": "PONG" }))
          .await?;
      }
    }
  }

  /// Sends `payload` and returns the first JSON reply on `namespace` that `done` accepts.
  async fn request(
    &mut self,
    namespace: &str,
    destination: &str,
    payload: Value,
    done: impl Fn(&Value) -> bool,
  ) -> anyhow::Result<Value> {
    let kind = payload["type"].as_str().unwrap_or_default().to_string();
    tokio::time::timeout(REPLY_TIMEOUT, async {
      self.send(namespace, destination, payload).await?;
      loop {
        let message = self.receive().await?;
        if message.namespace != namespace {
          continue;
        }
        let reply: Value = serde_json::from_str(&message.payload)?;
        if done(&reply) {
          return Ok(reply);
        }
      }
    })
    .await
    .map_err(|_| anyhow!("no reply to {kind}"))?
  }
}

/// The transport id of the running Spotify receiver in a `RECEIVER_STATUS`.
fn spotify_transport(status: &Value) -> Option<String> {
  status["status"]["applications"]
    .as_array()?
    .iter()
    .find(|app| app["appId"] == SPOTIFY_APP)?["transportId"]
    .as_str()
    .map(str::to_owned)
}

/// Starts the Spotify receiver on `device` and logs it in to the account. The
/// device then joins the connect-state cluster under [`CastDevice::connect_id`].
pub async fn log_in(session: &SpotifySession, device: &CastDevice) -> anyhow::Result<()> {
  let mut channel = CastChannel::open(device.address).await?;
  let status = channel
    .request(
      RECEIVER_NS,
      RECEIVER,
      json!({ "type": "LAUNCH", "appId": SPOTIFY_APP, "requestId": 1 }),
      |reply| reply["type"] == "LAUNCH_ERROR" || spotify_transport(reply).is_some(),
    )
    .await?;
  let transport = spotify_transport(&status)
    .ok_or_else(|| anyhow!("the Spotify receiver did not start: {}", status["reason"]))?;
  channel
    .send(CONNECTION_NS, &transport, json!({ "type": "CONNECT" }))
    .await?;

  let device_id = device.connect_id();
  let info = channel
    .request(
      SPOTIFY_NS,
      &transport,
      json!({
        "type": "getInfo",
        "payload": { "remoteName": device.name, "deviceID": device_id, "deviceAPI_isGroup": false },
      }),
      |reply| reply["type"] == "getInfoResponse",
    )
    .await?;
  let info = &info["payload"];
  log::info!(
    "Cast receiver on {}: tokenType {}, version {}",
    device.name,
    info["tokenType"],
    info["version"]
  );
  let client_id = info["clientID"]
    .as_str()
    .ok_or_else(|| anyhow!("no clientID in getInfoResponse"))?;
  let token = device_token(session, client_id, &device_id).await?;

  let reply = channel
    .request(
      SPOTIFY_NS,
      &transport,
      json!({ "type": "addUser", "payload": { "blob": token, "tokenType": "accesstoken" } }),
      |reply| {
        matches!(
          reply["type"].as_str(),
          Some("addUserResponse" | "addUserError")
        )
      },
    )
    .await?;
  if reply["type"] == "addUserError" {
    bail!("the speaker refused the login: {}", reply["payload"]);
  }
  Ok(())
}

/// An access token for the receiver's client id, bound to the device id.
async fn device_token(
  session: &SpotifySession,
  client_id: &str,
  device_id: &str,
) -> anyhow::Result<String> {
  let body = json!({ "clientId": client_id, "deviceId": device_id }).to_string();
  let bytes = session
    .session()
    .spclient()
    .request_as_json(&Method::POST, "/device-auth/v1/refresh", None, Some(&body))
    .await
    .map_err(upstream_error)?;
  let reply: Value = serde_json::from_slice(&bytes)?;
  reply["accessToken"]
    .as_str()
    .map(str::to_owned)
    .ok_or_else(|| anyhow!("no accessToken in the device-auth reply"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_cast_message_survives_a_round_trip() {
    let message = CastMessage {
      source: SENDER.into(),
      destination: RECEIVER.into(),
      namespace: RECEIVER_NS.into(),
      payload: json!({ "type": "GET_STATUS", "requestId": 1 }).to_string(),
    };
    assert_eq!(CastMessage::decode(&message.encode()), Some(message));
  }

  #[test]
  fn a_long_payload_gets_a_multi_byte_length() {
    let message = CastMessage {
      payload: "x".repeat(300),
      ..Default::default()
    };
    let decoded = CastMessage::decode(&message.encode()).unwrap();
    assert_eq!(decoded.payload.len(), 300);
  }

  #[test]
  fn a_truncated_frame_does_not_decode() {
    let frame = CastMessage {
      payload: "{}".into(),
      ..Default::default()
    }
    .encode();
    assert_eq!(CastMessage::decode(&frame[..frame.len() - 1]), None);
  }

  #[test]
  fn the_spotify_transport_comes_from_the_receiver_status() {
    let status = json!({ "type": "RECEIVER_STATUS", "status": { "applications": [
      { "appId": "E8C28D3C", "transportId": "backdrop" },
      { "appId": SPOTIFY_APP, "transportId": "web-5" },
    ] } });
    assert_eq!(spotify_transport(&status).as_deref(), Some("web-5"));
    assert_eq!(spotify_transport(&json!({ "type": "LAUNCH_ERROR" })), None);
  }
}
