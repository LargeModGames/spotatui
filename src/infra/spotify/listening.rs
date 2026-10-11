//! Recently played and top items from the local listening history: the
//! internal API has no route for them.

use super::catalog::{full_artists, full_tracks};
use super::json;
use super::metadata::get_many;
use super::session::SpotifySession;
use crate::infra::history::{load_listens, ListenRecord};
use chrono::{DateTime, Duration, Utc};
use librespot_metadata::Track;
use serde_json::{json, Value};
use std::collections::HashMap;

/// How many top tracks to weigh when ranking artists.
const ARTIST_RANKING_TRACKS: usize = 100;

async fn listens() -> Vec<ListenRecord> {
  tokio::task::spawn_blocking(load_listens)
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default()
}

fn spotify_track_uri(listen: &ListenRecord) -> Option<&str> {
  listen
    .item_uri
    .as_deref()
    .filter(|uri| uri.starts_with("spotify:track:"))
}

pub async fn recently_played(session: &SpotifySession, limit: usize) -> anyhow::Result<Value> {
  let listens = listens().await;
  let recent: Vec<(&str, DateTime<Utc>)> = listens
    .iter()
    .rev()
    .filter_map(|listen| spotify_track_uri(listen).map(|uri| (uri, listen.started_at)))
    .take(limit)
    .collect();
  let uris: Vec<String> = recent.iter().map(|(uri, _)| uri.to_string()).collect();
  let tracks = get_many::<Track>(session, &uris).await?;
  let items: Vec<Value> = recent
    .iter()
    .zip(tracks)
    .filter_map(|((_, played_at), track)| {
      Some(json!({ "track": json::track_full(track.as_deref()?), "played_at": played_at.to_rfc3339() }))
    })
    .collect();
  Ok(json!({ "href": "", "items": items, "limit": limit, "next": null }))
}

/// `short_term` is four weeks, `medium_term` six months, `long_term` all time.
fn range_start(time_range: Option<&str>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
  match time_range {
    Some("short_term") => Some(now - Duration::weeks(4)),
    Some("long_term") => None,
    _ => Some(now - Duration::days(182)),
  }
}

/// Track uris by qualified plays in the range, most played first.
fn ranked_track_uris(listens: &[ListenRecord], since: Option<DateTime<Utc>>) -> Vec<String> {
  let mut plays: HashMap<&str, usize> = HashMap::new();
  for listen in listens {
    if !listen.qualified || since.is_some_and(|since| listen.started_at < since) {
      continue;
    }
    if let Some(uri) = spotify_track_uri(listen) {
      *plays.entry(uri).or_default() += 1;
    }
  }
  let mut ranked: Vec<(&str, usize)> = plays.into_iter().collect();
  ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
  ranked.into_iter().map(|(uri, _)| uri.to_string()).collect()
}

pub async fn top_tracks(
  session: &SpotifySession,
  time_range: Option<&str>,
  limit: usize,
) -> anyhow::Result<Value> {
  let listens = listens().await;
  let ranked = ranked_track_uris(&listens, range_start(time_range, Utc::now()));
  let top: Vec<String> = ranked.into_iter().take(limit).collect();
  let items = full_tracks(session, &top).await?;
  let total = items.len();
  Ok(json::page(items, 0, limit, total))
}

pub async fn top_artists(
  session: &SpotifySession,
  time_range: Option<&str>,
  limit: usize,
) -> anyhow::Result<Value> {
  let listens = listens().await;
  let ranked = ranked_track_uris(&listens, range_start(time_range, Utc::now()));
  let top: Vec<String> = ranked.into_iter().take(ARTIST_RANKING_TRACKS).collect();
  let tracks = get_many::<Track>(session, &top).await?;
  let mut weights: Vec<(String, usize)> = Vec::new();
  for (rank, track) in tracks.into_iter().enumerate() {
    let Some(track) = track else { continue };
    let weight = ARTIST_RANKING_TRACKS - rank;
    for artist in track.artists.iter() {
      let uri = artist.id.to_uri();
      match weights.iter_mut().find(|(known, _)| *known == uri) {
        Some((_, total)) => *total += weight,
        None => weights.push((uri, weight)),
      }
    }
  }
  weights.sort_by_key(|(_, weight)| std::cmp::Reverse(*weight));
  let uris: Vec<String> = weights
    .into_iter()
    .take(limit)
    .map(|(uri, _)| uri)
    .collect();
  let items = full_artists(session, &uris).await?;
  let total = items.len();
  Ok(json::page(items, 0, limit, total))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::history::{HistoryItemKind, HistoryPlaybackSource};

  fn listen(uri: &str, days_ago: i64, qualified: bool) -> ListenRecord {
    let started_at = Utc::now() - Duration::days(days_ago);
    ListenRecord {
      started_at,
      ended_at: started_at,
      listened_ms: 0,
      duration_ms: 0,
      qualified,
      title: String::new(),
      artists: Vec::new(),
      album: String::new(),
      item_kind: HistoryItemKind::Track,
      item_id: None,
      item_uri: Some(uri.to_string()),
      context_uri: None,
      source: HistoryPlaybackSource::NativeContext,
      image_url: None,
    }
  }

  #[test]
  fn top_tracks_count_only_qualified_spotify_plays_in_the_range() {
    let listens = vec![
      listen("spotify:track:a", 1, true),
      listen("spotify:track:b", 1, true),
      listen("spotify:track:b", 2, true),
      listen("spotify:track:c", 1, false),
      listen("spotify:track:d", 60, true),
      listen("file:///song.mp3", 1, true),
    ];
    let since = range_start(Some("short_term"), Utc::now());
    assert_eq!(
      ranked_track_uris(&listens, since),
      vec!["spotify:track:b", "spotify:track:a"]
    );
    assert_eq!(ranked_track_uris(&listens, None).len(), 3);
  }
}
