//! Recently played from the local listening history: the internal API has no
//! route for it.

use super::json;
use super::metadata::get_many;
use super::session::SpotifySession;
use crate::infra::history::{load_listens, ListenRecord};
use chrono::{DateTime, Utc};
use librespot_metadata::Track;
use serde_json::{json, Value};

/// Spotify's own rule: a listen counts as played after 30 s.
const MIN_PLAYED_MS: u64 = 30_000;

async fn listens() -> Vec<ListenRecord> {
  tokio::task::spawn_blocking(load_listens)
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default()
}

fn played_spotify_track_uri(listen: &ListenRecord) -> Option<&str> {
  listen
    .item_uri
    .as_deref()
    .filter(|uri| uri.starts_with("spotify:track:") && listen.listened_ms >= MIN_PLAYED_MS)
}

pub async fn recently_played(session: &SpotifySession, limit: usize) -> anyhow::Result<Value> {
  let listens = listens().await;
  let recent: Vec<(&str, DateTime<Utc>)> = listens
    .iter()
    .rev()
    .filter_map(|listen| played_spotify_track_uri(listen).map(|uri| (uri, listen.started_at)))
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::infra::history::{HistoryItemKind, HistoryPlaybackSource};

  fn listen(uri: &str, listened_ms: u64) -> ListenRecord {
    ListenRecord {
      started_at: Utc::now(),
      ended_at: Utc::now(),
      listened_ms,
      duration_ms: 200_000,
      qualified: false,
      title: "Song".to_string(),
      artists: vec!["Artist".to_string()],
      album: "Album".to_string(),
      item_kind: HistoryItemKind::Track,
      item_id: None,
      item_uri: Some(uri.to_string()),
      context_uri: None,
      source: HistoryPlaybackSource::NativeContext,
      image_url: None,
    }
  }

  #[test]
  fn recently_played_skips_stub_listens_and_other_sources() {
    let track = "spotify:track:abc";
    assert_eq!(
      played_spotify_track_uri(&listen(track, 204_915)),
      Some(track)
    );
    assert_eq!(played_spotify_track_uri(&listen(track, 994)), None);
    assert_eq!(
      played_spotify_track_uri(&listen("qobuz:track:1", 204_915)),
      None
    );
  }
}
