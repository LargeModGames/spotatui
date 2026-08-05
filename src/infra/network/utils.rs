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
  #[serde(default)]
  duration: Option<f64>,
}

impl LrcResponse {
  fn has_lyrics(&self) -> bool {
    let has_text =
      |value: &Option<String>| value.as_deref().is_some_and(|text| !text.trim().is_empty());
    has_text(&self.syncedLyrics) || has_text(&self.plainLyrics)
  }
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
  async fn get_lyrics(&mut self, track: String, artists: Vec<String>, duration: f64);
  async fn increment_global_song_count(&mut self);
  async fn fetch_global_song_count(&mut self);
  async fn fetch_announcements(&mut self);
}

impl UtilsNetwork for Network {
  async fn get_lyrics(&mut self, track: String, artists: Vec<String>, duration: f64) {
    // The identity latch and the plugin-facing `lyrics_state_is_current` gate
    // both key on the joined display credit (`primary_artist()`), so build the
    // identity from the same join here.
    let request_identity = (track.clone(), artists.join(", "));
    let client = super::requests::shared_http_client();

    // Update state to loading
    {
      let mut app = self.app.lock().await;
      if app.desired_lyrics_identity.as_ref() != Some(&request_identity) {
        return;
      }
      app.lyrics_status = LyricsStatus::Loading;
      app.lyrics = None;
      app.lyrics_synced = false;
    }

    let lrc_resp = fetch_lrclib_lyrics(client, &track, &artists, duration).await;

    let mut app = self.app.lock().await;
    if app.desired_lyrics_identity.as_ref() != Some(&request_identity) {
      return;
    }
    match lrc_resp {
      Some(lrc_resp) => {
        // Prefer timestamped ("synced") lyrics. If LRCLIB only has plain
        // (unsynced) lyrics, still show them as static text rather than
        // reporting "not found" — many tracks only have plain lyrics.
        let synced = lrc_resp
          .syncedLyrics
          .as_deref()
          .map(parse_synced_lyrics)
          .unwrap_or_default();

        if !synced.is_empty() {
          app.lyrics = Some(synced);
          app.lyrics_synced = true;
          app.lyrics_status = LyricsStatus::Found;
        } else if let Some(plain) = lrc_resp
          .plainLyrics
          .as_deref()
          .filter(|text| !text.trim().is_empty())
        {
          app.lyrics = Some(synthesize_plain_lyrics(plain, duration));
          app.lyrics_synced = false;
          app.lyrics_status = LyricsStatus::Found;
        } else {
          app.lyrics_status = LyricsStatus::NotFound;
        }
      }
      None => {
        app.lyrics_status = LyricsStatus::NotFound;
      }
    }
    app
      .plugin_data_generations
      .bump(crate::core::app::PluginDataKind::Lyrics);
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

    let client = super::requests::shared_http_client();

    let response = match client
      .get(&resolved_url)
      .header(reqwest::header::ACCEPT, "application/json")
      .timeout(Duration::from_secs(5))
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

/// Look up lyrics on LRCLIB. `/api/get` is an exact signature match (title +
/// artist + duration must all agree with LRCLIB's record, duration in whole
/// seconds), so it 404s on small metadata differences even when LRCLIB has the
/// song. When it misses, fall back to the fuzzy `/api/search` endpoint and pick
/// the best-matching result.
///
/// Both endpoints are tried across every artist candidate (see
/// [`artist_query_candidates`]): all `/api/get` candidates first, then all
/// `/api/search` candidates, so an exact hit on the primary artist always beats
/// a fuzzy hit on the full multi-artist credit.
async fn fetch_lrclib_lyrics(
  client: &reqwest::Client,
  track: &str,
  artists: &[String],
  duration: f64,
) -> Option<LrcResponse> {
  let candidates = artist_query_candidates(artists);

  for artist in &candidates {
    if let Some(lrc_resp) = lrclib_get(client, track, artist, duration).await {
      return Some(lrc_resp);
    }
  }

  for artist in &candidates {
    if let Some(lrc_resp) = lrclib_search(client, track, artist, duration).await {
      return Some(lrc_resp);
    }
  }

  None
}

/// The LRCLIB artist strings to try, in order. LRCLIB indexes each track under a
/// single artist string, almost always the primary (first) credited artist. A
/// collaboration reaches here as the joined `"A, B"` credit that
/// `primary_artist()` builds for display, which matches neither `/api/get`
/// (exact) nor `/api/search`, so those tracks find no lyrics even when LRCLIB
/// has them (issue #410). Try the full credit first (correct for solo tracks and
/// acts whose own name contains a comma, e.g. "Earth, Wind & Fire"), then the
/// first credited artist alone. The first artist is taken from the structured
/// list, never by splitting the joined string, so a comma inside an artist name
/// is preserved.
fn artist_query_candidates(artists: &[String]) -> Vec<String> {
  let joined = artists.join(", ");
  let mut candidates = vec![joined.clone()];
  if let Some(first) = artists.first() {
    let first = first.trim();
    if !first.is_empty() && first != joined {
      candidates.push(first.to_string());
    }
  }
  candidates
}

/// Exact `/api/get` lookup for one artist string. Returns the record only when
/// the request succeeds and it actually carries lyrics.
async fn lrclib_get(
  client: &reqwest::Client,
  track: &str,
  artist: &str,
  duration: f64,
) -> Option<LrcResponse> {
  let query = [
    ("track_name", track.to_string()),
    ("artist_name", artist.to_string()),
    ("duration", (duration.round() as u64).to_string()),
  ];
  let resp = client
    .get("https://lrclib.net/api/get")
    .query(&query)
    .send()
    .await
    .ok()?;
  if !resp.status().is_success() {
    return None;
  }
  let lrc_resp = resp.json::<LrcResponse>().await.ok()?;
  lrc_resp.has_lyrics().then_some(lrc_resp)
}

/// Fuzzy `/api/search` lookup for one artist string, reduced to the best hit via
/// [`pick_search_result`].
async fn lrclib_search(
  client: &reqwest::Client,
  track: &str,
  artist: &str,
  duration: f64,
) -> Option<LrcResponse> {
  let query = [
    ("track_name", track.to_string()),
    ("artist_name", artist.to_string()),
  ];
  let resp = client
    .get("https://lrclib.net/api/search")
    .query(&query)
    .send()
    .await
    .ok()?;
  if !resp.status().is_success() {
    return None;
  }
  let results = resp.json::<Vec<LrcResponse>>().await.ok()?;
  pick_search_result(results, duration)
}

/// Pick the best `/api/search` hit: prefer synced lyrics over plain-only, then
/// the result whose duration is closest to the playing track's (so synced
/// timestamps line up). With an unknown duration (e.g. `0.0`), duration is
/// ignored.
fn pick_search_result(results: Vec<LrcResponse>, duration: f64) -> Option<LrcResponse> {
  results
    .into_iter()
    .filter(LrcResponse::has_lyrics)
    .min_by_key(|result| {
      let synced_rank = if result
        .syncedLyrics
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty())
      {
        0u64
      } else {
        1
      };
      let duration_diff_ms = if duration > 0.0 {
        result
          .duration
          .map(|d| ((d - duration).abs() * 1000.0) as u64)
          .unwrap_or(u64::MAX)
      } else {
        0
      };
      (synced_rank, duration_diff_ms)
    })
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
  use super::{
    artist_query_candidates, parse_synced_lyrics, pick_search_result, synthesize_plain_lyrics,
    LrcResponse,
  };

  #[test]
  fn artist_candidates_single_artist_has_no_fallback() {
    // A solo track queries once; nothing extra to try.
    assert_eq!(
      artist_query_candidates(&["Kygo".to_string()]),
      vec!["Kygo".to_string()]
    );
  }

  #[test]
  fn artist_candidates_add_primary_artist_for_collaborations() {
    // LRCLIB stores "Take Me Back" under "Kygo" alone, so the joined credit must
    // fall back to the first artist (issue #410).
    assert_eq!(
      artist_query_candidates(&["Kygo".to_string(), "Max McNown".to_string()]),
      vec!["Kygo, Max McNown".to_string(), "Kygo".to_string()]
    );
  }

  #[test]
  fn artist_candidates_preserve_comma_inside_artist_name() {
    // Splitting the joined display string on ", " would corrupt an act whose own
    // name contains a comma; the primary artist must come from the structured
    // list instead.
    assert_eq!(
      artist_query_candidates(&["Earth, Wind & Fire".to_string(), "Guest".to_string()]),
      vec![
        "Earth, Wind & Fire, Guest".to_string(),
        "Earth, Wind & Fire".to_string(),
      ]
    );
  }

  #[test]
  fn artist_candidates_empty_list_yields_single_empty_query() {
    assert_eq!(artist_query_candidates(&[]), vec![String::new()]);
  }

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

  #[allow(non_snake_case)]
  fn search_result(
    syncedLyrics: Option<&str>,
    plainLyrics: Option<&str>,
    duration: Option<f64>,
  ) -> LrcResponse {
    LrcResponse {
      syncedLyrics: syncedLyrics.map(str::to_string),
      plainLyrics: plainLyrics.map(str::to_string),
      duration,
    }
  }

  #[test]
  fn search_prefers_synced_result_with_closest_duration() {
    let results = vec![
      search_result(None, Some("plain only"), Some(200.0)),
      search_result(Some("[00:01.00] far"), None, Some(300.0)),
      search_result(Some("[00:01.00] close"), None, Some(201.0)),
    ];
    let picked = pick_search_result(results, 200.0).unwrap();
    assert_eq!(picked.syncedLyrics.as_deref(), Some("[00:01.00] close"));
  }

  #[test]
  fn search_falls_back_to_plain_when_no_synced_result() {
    let results = vec![
      search_result(None, None, Some(200.0)),
      search_result(None, Some("plain"), Some(200.0)),
    ];
    let picked = pick_search_result(results, 200.0).unwrap();
    assert_eq!(picked.plainLyrics.as_deref(), Some("plain"));
  }

  #[test]
  fn search_returns_none_when_no_result_has_lyrics() {
    let results = vec![search_result(None, Some("   "), Some(200.0))];
    assert!(pick_search_result(results, 200.0).is_none());
  }
}
