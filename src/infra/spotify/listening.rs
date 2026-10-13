//! Recently played from the local listening history: the internal API has no
//! route for it.

use super::json;
use super::metadata::get_many;
use super::session::SpotifySession;
use crate::infra::history::{load_listens, ListenRecord};
use chrono::{DateTime, Utc};
use librespot_metadata::Track;
use serde_json::{json, Value};

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
