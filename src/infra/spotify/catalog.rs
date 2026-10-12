//! Catalogue reads: tracks, albums, artists, shows, recommendations and audio analysis.

use super::json;
use super::metadata::{get_many, get_one};
use super::router::{api_error, query_value, retry_read, upstream_error, Paging};
use super::session::SpotifySession;
use librespot_core::SpotifyId;
use librespot_metadata::{Album, Artist, Episode, Show, Track};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

const RECOMMENDATION_COUNT: usize = 50;

fn uri(kind: &str, id: &str) -> String {
  format!("spotify:{kind}:{id}")
}

pub async fn track(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let track = get_one::<Track>(session, &uri("track", id)).await?;
  Ok(json::track_full(&track))
}

pub async fn tracks_by_ids(session: &SpotifySession, ids: &str) -> anyhow::Result<Value> {
  let uris: Vec<String> = ids
    .split(',')
    .filter(|id| !id.is_empty())
    .map(|id| uri("track", id))
    .collect();
  Ok(json!({ "tracks": full_tracks(session, &uris).await? }))
}

/// Full track objects in order; items Spotify does not know are left out.
pub async fn full_tracks(session: &SpotifySession, uris: &[String]) -> anyhow::Result<Vec<Value>> {
  Ok(
    get_many::<Track>(session, uris)
      .await?
      .into_iter()
      .flatten()
      .map(|track| json::track_full(&track))
      .collect(),
  )
}

async fn simplified_tracks(
  session: &SpotifySession,
  uris: &[String],
) -> anyhow::Result<Vec<Value>> {
  Ok(
    get_many::<Track>(session, uris)
      .await?
      .into_iter()
      .flatten()
      .map(|track| json::track_simplified(&track))
      .collect(),
  )
}

pub async fn album(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let album = get_one::<Album>(session, &uri("album", id)).await?;
  let uris: Vec<String> = album.tracks().map(|track| track.to_uri()).collect();
  let tracks = simplified_tracks(session, &uris).await?;
  let total = tracks.len();
  Ok(json::album_full(
    &album,
    json::page(tracks, 0, total.max(1), total),
  ))
}

pub async fn album_tracks(
  session: &SpotifySession,
  id: &str,
  paging: Paging,
) -> anyhow::Result<Value> {
  let album = get_one::<Album>(session, &uri("album", id)).await?;
  let uris: Vec<String> = album.tracks().map(|track| track.to_uri()).collect();
  let window = paging.window(&uris);
  let items = simplified_tracks(session, window).await?;
  Ok(json::page(items, paging.offset, paging.limit, uris.len()))
}

pub async fn artist(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let artist = get_one::<Artist>(session, &uri("artist", id)).await?;
  Ok(json::artist_full(&artist))
}

pub async fn artists_by_ids(session: &SpotifySession, ids: &str) -> anyhow::Result<Value> {
  let uris: Vec<String> = ids
    .split(',')
    .filter(|id| !id.is_empty())
    .map(|id| uri("artist", id))
    .collect();
  Ok(json!({ "artists": full_artists(session, &uris).await? }))
}

pub async fn artist_top_tracks(
  session: &SpotifySession,
  id: &str,
  market: Option<&str>,
) -> anyhow::Result<Value> {
  let artist = get_one::<Artist>(session, &uri("artist", id)).await?;
  let country = market
    .map(str::to_string)
    .unwrap_or_else(|| session.country());
  let uris: Vec<String> = artist
    .top_tracks
    .for_country(&country)
    .iter()
    .map(|track| track.to_uri())
    .collect();
  Ok(json!({ "tracks": full_tracks(session, &uris).await? }))
}

pub async fn related_artists(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let artist = get_one::<Artist>(session, &uri("artist", id)).await?;
  let uris: Vec<String> = artist.related.iter().map(|a| a.id.to_uri()).collect();
  Ok(json!({ "artists": full_artists(session, &uris).await? }))
}

pub async fn full_artists(session: &SpotifySession, uris: &[String]) -> anyhow::Result<Vec<Value>> {
  Ok(
    get_many::<Artist>(session, uris)
      .await?
      .into_iter()
      .flatten()
      .map(|artist| json::artist_full(&artist))
      .collect(),
  )
}

/// Albums, then singles, then compilations, then appearances, each in its
/// current release.
pub async fn artist_albums(
  session: &SpotifySession,
  id: &str,
  paging: Paging,
) -> anyhow::Result<Value> {
  let artist = get_one::<Artist>(session, &uri("artist", id)).await?;
  let groups = [
    ("album", &artist.albums),
    ("single", &artist.singles),
    ("compilation", &artist.compilations),
    ("appears_on", &artist.appears_on_albums),
  ];
  let entries: Vec<(&str, String)> = groups
    .iter()
    .flat_map(|(group, albums)| {
      albums
        .iter()
        .filter_map(|variants| variants.first())
        .map(move |album| (*group, album.to_uri()))
    })
    .collect();
  let window = paging.window(&entries);
  let uris: Vec<String> = window.iter().map(|(_, uri)| uri.clone()).collect();
  let albums = get_many::<Album>(session, &uris).await?;
  let items = window
    .iter()
    .zip(albums)
    .filter_map(|((group, _), album)| {
      let mut value = json::album_simplified(album.as_deref()?);
      value["album_group"] = json!(group);
      Some(value)
    })
    .collect();
  Ok(json::page(
    items,
    paging.offset,
    paging.limit,
    entries.len(),
  ))
}

pub async fn show(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let show = get_one::<Show>(session, &uri("show", id)).await?;
  let total = show.episodes.len();
  Ok(json::show_full(&show, json::page(Vec::new(), 0, 50, total)))
}

pub async fn show_episodes(
  session: &SpotifySession,
  id: &str,
  paging: Paging,
) -> anyhow::Result<Value> {
  let show = get_one::<Show>(session, &uri("show", id)).await?;
  let uris: Vec<String> = show
    .episodes
    .iter()
    .map(|episode| episode.to_uri())
    .collect();
  let window = paging.window(&uris);
  let items = get_many::<Episode>(session, window)
    .await?
    .into_iter()
    .flatten()
    .map(|episode| json::episode_simplified(&episode))
    .collect();
  Ok(json::page(items, paging.offset, paging.limit, uris.len()))
}

/// The station seed: the first seed track, or else the first seed artist.
fn recommendation_seed(query: &[(&str, String)]) -> Option<String> {
  let first_seed = |key: &str| {
    query_value(query, key)
      .and_then(|ids| ids.split(',').find(|id| !id.is_empty()).map(str::to_string))
  };
  match (first_seed("seed_tracks"), first_seed("seed_artists")) {
    (Some(track), _) => Some(uri("track", &track)),
    (None, Some(artist)) => Some(uri("artist", &artist)),
    (None, None) => None,
  }
}

/// The track uris of a radio station answer, in order.
fn station_track_uris(station: &Value) -> Vec<String> {
  station["tracks"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|track| track["uri"].as_str())
    .filter(|uri| uri.starts_with("spotify:track:"))
    .map(str::to_string)
    .collect()
}

/// A radio station from the first seed track, or else the first seed artist.
pub async fn recommendations(
  session: &SpotifySession,
  query: &[(&str, String)],
) -> anyhow::Result<Value> {
  let seed =
    recommendation_seed(query).ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "No seed"))?;
  let bytes = session
    .session()
    .spclient()
    .get_apollo_station(
      "tracks",
      &seed,
      Some(RECOMMENDATION_COUNT),
      Vec::new(),
      false,
    )
    .await
    .map_err(upstream_error)?;
  let station: Value = serde_json::from_slice(&bytes)?;
  let uris = station_track_uris(&station);
  Ok(json!({ "tracks": simplified_tracks(session, &uris).await? }))
}

/// `GET audio-analysis/{id}`: the same JSON the Web API returned.
pub async fn audio_analysis(session: &SpotifySession, id: &str) -> anyhow::Result<Value> {
  let track = SpotifyId::from_base62(id)
    .map_err(|_| api_error(StatusCode::BAD_REQUEST, "Invalid track id"))?;
  let endpoint = format!("/audio-attributes/v1/audio-analysis/{}", track.to_base62());
  let spclient = session.session().spclient();
  let bytes = retry_read(|| spclient.request_as_json(&Method::GET, &endpoint, None, None))
    .await
    .map_err(upstream_error)?;
  Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn query(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    pairs
      .iter()
      .map(|(key, value)| (*key, value.to_string()))
      .collect()
  }

  #[test]
  fn the_first_seed_track_wins_over_the_seed_artists() {
    let seed = recommendation_seed(&query(&[("seed_artists", "a1"), ("seed_tracks", ",t1,t2")]));
    assert_eq!(seed.as_deref(), Some("spotify:track:t1"));
  }

  #[test]
  fn a_seed_artist_is_used_without_a_seed_track() {
    let seed = recommendation_seed(&query(&[("seed_artists", "a1,a2"), ("seed_tracks", "")]));
    assert_eq!(seed.as_deref(), Some("spotify:artist:a1"));
    assert_eq!(recommendation_seed(&query(&[("limit", "20")])), None);
  }

  #[test]
  fn a_station_answer_keeps_only_its_track_uris_in_order() {
    let station = json!({ "tracks": [
      { "uri": "spotify:track:b" },
      { "uri": "spotify:episode:e" },
      { "name": "no uri" },
      { "uri": "spotify:track:a" },
    ]});
    assert_eq!(
      station_track_uris(&station),
      vec!["spotify:track:b".to_string(), "spotify:track:a".to_string()]
    );
    assert!(station_track_uris(&json!({})).is_empty());
  }
}
