//! Spotify Connect speakers on the local network that are not logged in: the
//! zeroconf `getInfo` and `addUser` calls of the official client.

use super::session::SpotifySession;
use anyhow::{anyhow, bail};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::Rng as _;
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);
/// The zeroconf API version this client speaks.
const VERSION: &str = "2.10.0";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
/// The `getInfo` token types that [`log_in`] can answer.
pub const SUPPORTED_TOKEN_TYPES: &[&str] = &["accesstoken"];

fn client() -> anyhow::Result<reqwest::Client> {
  Ok(reqwest::Client::builder().timeout(TIMEOUT).build()?)
}

/// The `getInfo` reply of the speaker at `url` (`http://host:port/cpath`).
async fn get_info(url: &str) -> anyhow::Result<Value> {
  Ok(
    client()?
      .get(url)
      .query(&[("action", "getInfo")])
      .send()
      .await?
      .error_for_status()?
      .json()
      .await?,
  )
}

/// An access token for the speaker's client id, from the token exchange of the official client.
async fn connect_token(session: &SpotifySession, audience: &str) -> anyhow::Result<String> {
  let core = session.session();
  let subject = core.login5().auth_token().await?.access_token;
  let own_client = core.client_id();
  let form = [
    (
      "grant_type",
      "urn:ietf:params:oauth:grant-type:token-exchange",
    ),
    ("subject_token", subject.as_str()),
    (
      "subject_token_type",
      "urn:ietf:params:oauth:token-type:access_token",
    ),
    (
      "requested_token_type",
      "urn:ietf:params:oauth:token-type:access_token",
    ),
    ("resource", "urn:spotify:resources:connect"),
    ("client_id", own_client.as_str()),
    ("audience", audience),
    ("scope", "streaming"),
  ];
  let reply = client()?.post(TOKEN_URL).form(&form).send().await?;
  let status = reply.status();
  let body: Value = reply.json().await?;
  body["access_token"]
    .as_str()
    .map(str::to_owned)
    .ok_or_else(|| anyhow!("the token exchange failed ({status}): {}", body["error"]))
}

/// Logs the speaker at `url` in to the account.
pub async fn log_in(session: &SpotifySession, url: &str) -> anyhow::Result<()> {
  let info = get_info(url).await?;
  let token_type = info["tokenType"].as_str().unwrap_or_default();
  if !SUPPORTED_TOKEN_TYPES.contains(&token_type) {
    bail!("the speaker asks for token type {token_type:?}, which is not supported yet");
  }
  let client_id = info["clientID"]
    .as_str()
    .ok_or_else(|| anyhow!("no clientID in getInfo"))?;
  let token = connect_token(session, client_id).await?;

  // An access-token login skips the key exchange, but the field must be present.
  let mut client_key = [0u8; 96];
  rand::rng().fill_bytes(&mut client_key);
  let mut login_id = [0u8; 16];
  rand::rng().fill_bytes(&mut login_id);
  let login_id: String = login_id.iter().map(|b| format!("{b:02x}")).collect();
  let own_id = session.session().device_id().to_string();
  let form = [
    ("action", "addUser"),
    ("userName", &session.username()),
    ("blob", &token),
    ("clientKey", &BASE64.encode(client_key)),
    ("tokenType", "accesstoken"),
    ("loginId", &login_id),
    ("deviceName", "spotatui"),
    ("deviceId", &own_id),
    ("version", VERSION),
  ];
  let reply: Value = client()?
    .post(url)
    .form(&form)
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
  if reply["status"] != 101 {
    bail!(
      "the speaker refused the login: {} ({})",
      reply["statusString"],
      reply["spotifyError"]
    );
  }
  Ok(())
}
