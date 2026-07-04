use super::Network;
use crate::core::app::{Announcement, AnnouncementLevel, LyricsStatus};
use chrono::{DateTime, Utc};
use serde::{de::Error as _, Deserialize, Deserializer};
use std::collections::HashSet;
use std::env;
use std::time::{Duration, Instant};

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
struct LrcResponse {
  syncedLyrics: Option<String>,
  plainLyrics: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct GlobalSongCountResponse {
  #[serde(deserialize_with = "deserialize_global_song_count")]
  count: u64,
}

const TELEMETRY_ENDPOINT: &str = "https://spotatui-counter.spotatui.workers.dev";

fn deserialize_global_song_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
  D: Deserializer<'de>,
{
  #[derive(Deserialize)]
  #[serde(untagged)]
  enum CountValue {
    Number(u64),
    String(String),
  }

  match CountValue::deserialize(deserializer)? {
    CountValue::Number(value) => Ok(value),
    CountValue::String(value) => {
      let sanitized = value.replace(',', "");
      sanitized
        .parse::<u64>()
        .map_err(|_| D::Error::custom("invalid global song count"))
    }
  }
}

#[derive(Deserialize, Debug)]
struct AnnouncementFeedResponse {
  #[allow(dead_code)]
  version: Option<u8>,
  #[serde(default)]
  announcements: Vec<AnnouncementRecord>,
}

#[derive(Deserialize, Debug)]
struct AnnouncementRecord {
  id: String,
  title: Option<String>,
  body: String,
  level: Option<String>,
  url: Option<String>,
  starts_at: Option<String>,
  ends_at: Option<String>,
}

pub trait UtilsNetwork {
  async fn get_lyrics(&mut self, track: String, artist: String, duration: f64);
  async fn increment_global_song_count(&mut self);
  async fn fetch_global_song_count(&mut self);
  async fn fetch_announcements(&mut self);
}

impl UtilsNetwork for Network {
  async fn get_lyrics(&mut self, track: String, artist: String, duration: f64) {
    let client = super::requests::shared_http_client();
    let query = vec![
      ("track_name", track.clone()),
      ("artist_name", artist.clone()),
      ("duration", duration.to_string()),
    ];

    // Update state to loading
    {
      let mut app = self.app.lock().await;
      app.lyrics_status = LyricsStatus::Loading;
      app.lyrics = None;
    }

    match client
      .get("https://lrclib.net/api/get")
      .query(&query)
      .send()
      .await
    {
      Ok(resp) => {
        if resp.status().is_success() {
          if let Ok(lrc_resp) = resp.json::<LrcResponse>().await {
            // Prefer timestamped ("synced") lyrics. If LRCLIB only has plain
            // (unsynced) lyrics, still show them as static text rather than
            // reporting "not found" — many tracks only have plain lyrics.
            let synced = lrc_resp
              .syncedLyrics
              .as_deref()
              .map(parse_synced_lyrics)
              .unwrap_or_default();

            let mut app = self.app.lock().await;
            if !synced.is_empty() {
              app.lyrics = Some(synced);
              app.lyrics_status = LyricsStatus::Found;
            } else if let Some(plain) = lrc_resp
              .plainLyrics
              .as_deref()
              .filter(|text| !text.trim().is_empty())
            {
              app.lyrics = Some(synthesize_plain_lyrics(plain, duration));
              app.lyrics_status = LyricsStatus::Found;
            } else {
              app.lyrics_status = LyricsStatus::NotFound;
            }
          } else {
            let mut app = self.app.lock().await;
            app.lyrics_status = LyricsStatus::NotFound;
          }
        } else {
          let mut app = self.app.lock().await;
          app.lyrics_status = LyricsStatus::NotFound;
        }
      }
      Err(_) => {
        let mut app = self.app.lock().await;
        app.lyrics_status = LyricsStatus::NotFound;
      }
    }
  }

  async fn increment_global_song_count(&mut self) {
    let client = super::requests::shared_http_client();
    // Fire and forget
    let _ = client
      .post(TELEMETRY_ENDPOINT)
      .header(reqwest::header::ACCEPT, "application/json")
      .timeout(Duration::from_secs(5))
      .send()
      .await;
  }

  async fn fetch_global_song_count(&mut self) {
    let client = super::requests::shared_http_client();
    match client
      .get(TELEMETRY_ENDPOINT)
      .header(reqwest::header::ACCEPT, "application/json")
      .timeout(Duration::from_secs(5))
      .send()
      .await
    {
      Ok(resp) => {
        if let Ok(data) = resp.json::<GlobalSongCountResponse>().await {
          let mut app = self.app.lock().await;
          app.global_song_count = Some(data.count);
          app.global_song_count_failed = false;
        } else {
          let mut app = self.app.lock().await;
          app.global_song_count_failed = true;
        }
      }
      Err(_) => {
        let mut app = self.app.lock().await;
        app.global_song_count_failed = true;
      }
    }
  }

  async fn fetch_announcements(&mut self) {
    const MAX_ANNOUNCEMENT_FEED_BYTES: usize = 256 * 1024;
    const ANNOUNCEMENTS_ENV_KEY: &str = "SPOTATUI_ANNOUNCEMENTS_URL";
    const DEFAULT_ANNOUNCEMENTS_URL: &str =
      "https://raw.githubusercontent.com/LargeModGames/spotatui/main/announcements.json";

    let (announcements_enabled, feed_url, seen_ids) = {
      let app = self.app.lock().await;
      (
        app.user_config.behavior.enable_announcements,
        app.user_config.behavior.announcement_feed_url.clone(),
        app.user_config.behavior.seen_announcement_ids.clone(),
      )
    };

    if !announcements_enabled {
      return;
    }

    let env_feed_url = env::var(ANNOUNCEMENTS_ENV_KEY)
      .ok()
      .map(|v| v.trim().to_string())
      .filter(|v| !v.is_empty());

    let resolved_url = env_feed_url
      .or(feed_url)
      .filter(|url| !url.trim().is_empty())
      .unwrap_or_else(|| DEFAULT_ANNOUNCEMENTS_URL.to_string());

    if !resolved_url.starts_with("https://") {
      return;
    }

    let client = match reqwest::Client::builder()
      .timeout(Duration::from_secs(5))
      .build()
    {
      Ok(client) => client,
      Err(_) => return,
    };

    let response = match client
      .get(&resolved_url)
      .header(reqwest::header::ACCEPT, "application/json")
      .send()
      .await
    {
      Ok(response) => response,
      Err(_) => return,
    };

    if !response.status().is_success() {
      return;
    }

    if response
      .content_length()
      .is_some_and(|length| length > MAX_ANNOUNCEMENT_FEED_BYTES as u64)
    {
      return;
    }

    let body = match response.bytes().await {
      Ok(bytes) if bytes.len() <= MAX_ANNOUNCEMENT_FEED_BYTES => bytes,
      _ => return,
    };

    let feed: AnnouncementFeedResponse = match serde_json::from_slice(&body) {
      Ok(feed) => feed,
      Err(_) => return,
    };

    let now = Utc::now();
    let seen_ids = seen_ids.into_iter().collect::<HashSet<String>>();
    let mut feed_ids_seen = HashSet::new();
    let mut announcements = Vec::new();

    for record in feed.announcements {
      let id = record.id.trim().to_string();
      if id.is_empty() || seen_ids.contains(&id) || !feed_ids_seen.insert(id.clone()) {
        continue;
      }

      let body = record.body.trim().to_string();
      if body.is_empty() {
        continue;
      }

      let starts_at = match record.starts_at.as_deref().map(parse_announcement_datetime) {
        Some(Some(value)) => Some(value),
        Some(None) => continue,
        None => None,
      };

      let ends_at = match record.ends_at.as_deref().map(parse_announcement_datetime) {
        Some(Some(value)) => Some(value),
        Some(None) => continue,
        None => None,
      };

      if let Some(start) = starts_at {
        if now < start {
          continue;
        }
      }

      if let Some(end) = ends_at {
        if now > end {
          continue;
        }
      }

      let url = record
        .url
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty() && url.starts_with("https://"));

      announcements.push(Announcement {
        id,
        title: record
          .title
          .map(|title| title.trim().to_string())
          .filter(|title| !title.is_empty())
          .unwrap_or_else(|| "Announcement".to_string()),
        body,
        level: parse_announcement_level(record.level.as_deref()),
        url,
        received_at: Instant::now(),
      });
    }

    if announcements.is_empty() {
      return;
    }

    let mut app = self.app.lock().await;
    let had_active_announcement = app.active_announcement.is_some();
    app.enqueue_announcements(announcements);

    if !had_active_announcement && app.active_announcement.is_some() {
      app.push_navigation_stack(
        crate::core::app::RouteId::AnnouncementPrompt,
        crate::core::app::ActiveBlock::AnnouncementPrompt,
      );
    }
  }
}

/// Parse LRC-format synced lyrics (`[mm:ss.xx] text` lines) into `(ms, line)`
/// pairs. Lines without a valid leading timestamp are dropped, so a body of
/// plain (unsynced) lyrics parses to an empty vec.
fn parse_synced_lyrics(text: &str) -> Vec<(u128, String)> {
  text
    .lines()
    .filter_map(|line| {
      let idx = line.find(']')?;
      if idx <= 1 || !line.starts_with('[') {
        return None;
      }
      let timestamp = &line[1..idx];
      let content = line[idx + 1..].trim().to_string();

      let parts: Vec<&str> = timestamp.split(':').collect();
      if parts.len() != 2 {
        return None;
      }
      let mins = parts[0].parse::<u64>().unwrap_or(0);
      let secs_parts: Vec<&str> = parts[1].split('.').collect();
      let secs = secs_parts[0].parse::<u64>().unwrap_or(0);
      let ms = if secs_parts.len() > 1 {
        // Handle 2- or 3-digit fractional seconds.
        let ms_str = secs_parts[1];
        let ms_val = ms_str.parse::<u64>().unwrap_or(0);
        if ms_str.len() == 2 {
          ms_val * 10
        } else {
          ms_val
        }
      } else {
        0
      };

      let total_ms = (mins * 60 * 1000) + (secs * 1000) + ms;
      Some((total_ms as u128, content))
    })
    .collect()
}

/// Turn plain (unsynced) lyrics into `(ms, line)` pairs with synthetic,
/// evenly-spaced timestamps across the track duration. This lets the existing
/// synced-lyrics renderer display them as static text that scrolls approximately
/// in time. With an unknown duration (e.g. `0.0`), every line gets timestamp `0`
/// so the text simply renders from the top.
fn synthesize_plain_lyrics(text: &str, duration_secs: f64) -> Vec<(u128, String)> {
  let lines: Vec<String> = text.lines().map(|line| line.trim().to_string()).collect();
  let line_count = lines.len().max(1) as f64;
  let total_ms = if duration_secs > 0.0 {
    duration_secs * 1000.0
  } else {
    0.0
  };
  lines
    .into_iter()
    .enumerate()
    .map(|(idx, line)| {
      let ts = ((idx as f64 / line_count) * total_ms) as u128;
      (ts, line)
    })
    .collect()
}

fn parse_announcement_datetime(value: &str) -> Option<DateTime<Utc>> {
  DateTime::parse_from_rfc3339(value)
    .ok()
    .map(|dt| dt.with_timezone(&Utc))
}

fn parse_announcement_level(level: Option<&str>) -> AnnouncementLevel {
  match level.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
    Some("critical") => AnnouncementLevel::Critical,
    Some("warning") => AnnouncementLevel::Warning,
    _ => AnnouncementLevel::Info,
  }
}

#[cfg(test)]
mod tests {
  use super::{parse_synced_lyrics, synthesize_plain_lyrics};

  #[test]
  fn parses_timestamped_lyric_lines_and_drops_untimed_ones() {
    let text = "[00:12.34] Hello\n[01:05.00] World\nno timestamp here";
    let parsed = parse_synced_lyrics(text);
    assert_eq!(
      parsed,
      vec![
        (12_340u128, "Hello".to_string()),
        (65_000, "World".to_string())
      ]
    );
  }

  #[test]
  fn plain_unsynced_lyrics_parse_to_empty_synced() {
    // A body of plain lyrics (no timestamps) yields no synced lines, which is
    // what triggers the plain-lyrics fallback in `get_lyrics`.
    assert!(parse_synced_lyrics("just\nplain\nwords").is_empty());
  }

  #[test]
  fn synthesizes_evenly_spaced_timestamps_across_duration() {
    let parsed = synthesize_plain_lyrics("a\nb\nc\nd", 4.0);
    assert_eq!(
      parsed,
      vec![
        (0u128, "a".to_string()),
        (1_000, "b".to_string()),
        (2_000, "c".to_string()),
        (3_000, "d".to_string()),
      ]
    );
  }

  #[test]
  fn synthesizes_zero_timestamps_when_duration_unknown() {
    let parsed = synthesize_plain_lyrics("a\nb", 0.0);
    assert_eq!(parsed, vec![(0u128, "a".to_string()), (0, "b".to_string())]);
  }
}
