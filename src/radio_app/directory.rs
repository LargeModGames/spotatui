use super::config::Station;
use anyhow::{anyhow, Context, Result};
use rand::seq::SliceRandom;
use serde::Deserialize;
use std::time::Duration;

const MIRRORS: [&str; 3] = [
  "https://de1.api.radio-browser.info",
  "https://de2.api.radio-browser.info",
  "https://fi1.api.radio-browser.info",
];
const TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Deserialize)]
struct DirectoryStation {
  name: String,
  url: String,
  #[serde(default)]
  url_resolved: String,
  #[serde(default)]
  lastcheckok: u8,
}

pub async fn search(query: &str) -> Result<Vec<Station>> {
  let query = query.trim();
  if query.is_empty() {
    return Ok(Vec::new());
  }

  let client = reqwest::Client::builder()
    .user_agent(concat!("degen-radio/", env!("CARGO_PKG_VERSION")))
    .timeout(TIMEOUT)
    .build()
    .context("building radio directory client")?;
  let path = format!(
    "/json/stations/search?name={}&limit=30&hidebroken=true&order=votes&reverse=true",
    url_encode(query)
  );
  let mut mirrors = MIRRORS;
  mirrors.shuffle(&mut rand::rng());
  let mut last_error = anyhow!("no radio directory mirrors configured");

  for mirror in mirrors {
    let url = format!("{mirror}{path}");
    match client
      .get(url)
      .send()
      .await
      .and_then(|response| response.error_for_status())
    {
      Ok(response) => match response.json::<Vec<DirectoryStation>>().await {
        Ok(rows) => {
          return Ok(
            rows
              .into_iter()
              .filter(|row| row.lastcheckok == 1)
              .filter_map(|row| {
                let url = if row.url_resolved.is_empty() {
                  row.url
                } else {
                  row.url_resolved
                };
                let name = row.name.trim().to_owned();
                (!name.is_empty() && !url.is_empty()).then_some(Station { name, url })
              })
              .collect(),
          )
        }
        Err(error) => last_error = error.into(),
      },
      Err(error) => last_error = error.into(),
    }
  }

  Err(last_error.context("all radio directory mirrors failed"))
}

fn url_encode(value: &str) -> String {
  let mut output = String::with_capacity(value.len());
  for byte in value.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        output.push(byte as char)
      }
      b' ' => output.push('+'),
      other => output.push_str(&format!("%{other:02X}")),
    }
  }
  output
}
