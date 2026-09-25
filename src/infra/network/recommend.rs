use super::{ids, IoEvent, Network};
use crate::core::app::{ActiveBlock, RouteId, TrackTableContext};
use crate::core::plugin_api::TrackInfo;
use crate::core::spotify_access::RestrictedEndpoint;
use crate::infra::network::requests::{is_forbidden_error, is_not_found_error};
use anyhow::anyhow;
use rspotify::model::{
  enums::Country,
  idtypes::{ArtistId, TrackId},
  track::{FullTrack, SimplifiedTrack},
};
use rspotify::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct RecommendationsResponse {
  tracks: Vec<SimplifiedTrack>,
}

#[derive(Deserialize)]
struct TracksResponse {
  tracks: Vec<FullTrack>,
}

fn country_code(country: Country) -> String {
  let code: &'static str = country.into();
  code.to_string()
}

fn is_development_mode_error(error: &anyhow::Error) -> bool {
  is_not_found_error(error) || is_forbidden_error(error)
}

pub trait RecommendationNetwork {
  async fn get_recommendations_for_seed(
    &mut self,
    seed_artists: Option<Vec<ArtistId<'static>>>,
    seed_tracks: Option<Vec<TrackId<'static>>>,
    first_track: Box<Option<TrackInfo>>,
    country: Option<Country>,
  );
  async fn get_recommendations_for_track_id(
    &mut self,
    track_id: TrackId<'static>,
    country: Option<Country>,
  );
}

impl RecommendationNetwork for Network {
  async fn get_recommendations_for_seed(
    &mut self,
    seed_artists: Option<Vec<ArtistId<'static>>>,
    seed_tracks: Option<Vec<TrackId<'static>>>,
    first_track: Box<Option<TrackInfo>>,
    country: Option<Country>,
  ) {
    if self
      .endpoint_is_out_of_reach("Recommendation", RestrictedEndpoint::Recommendations)
      .await
    {
      return;
    }

    let limit = self.large_search_limit;
    let mut query = vec![("limit", limit.to_string())];
    if let Some(country) = country {
      query.push(("market", country_code(country)));
    }
    if let Some(seed_artists) = seed_artists.as_ref() {
      query.push((
        "seed_artists",
        seed_artists
          .iter()
          .map(|id| id.id().to_string())
          .collect::<Vec<_>>()
          .join(","),
      ));
    }
    if let Some(seed_tracks) = seed_tracks.as_ref() {
      query.push((
        "seed_tracks",
        seed_tracks
          .iter()
          .map(|id| id.id().to_string())
          .collect::<Vec<_>>()
          .join(","),
      ));
    }

    match self
      .spotify_get_typed::<RecommendationsResponse>("recommendations", &query)
      .await
    {
      Ok(recommendations) => {
        // Convert SimplifiedTrack to FullTrack (best effort)
        // SimplifiedTrack doesn't have album field which FullTrack needs.
        // This is tricky. Recommendations usually return SimplifiedTracks.
        // We probably need to fetch FullTracks or fake it.
        // For now, let's map what we can and use a dummy album or fail.
        // Actually, we can fetch the full tracks using the IDs.
        let track_ids: Vec<TrackId> = recommendations
          .tracks
          .iter()
          .filter_map(|t| t.id.clone())
          .collect();

        let ids = track_ids
          .iter()
          .map(|id| id.id().to_string())
          .collect::<Vec<_>>()
          .join(",");
        let full_tracks = if ids.is_empty() {
          Vec::new()
        } else {
          match self
            .spotify_get_typed::<TracksResponse>("tracks", &[("ids", ids)])
            .await
          {
            Ok(res) => res.tracks,
            Err(e) if is_development_mode_error(&e) => {
              self
                .raise_and_remind_unavailable("Recommendation", RestrictedEndpoint::TracksByIds)
                .await;
              return;
            }
            Err(e) => {
              self.handle_error(anyhow!(e)).await;
              return;
            }
          }
        };

        let mut app = self.app.lock().await;
        // Check if these tracks are liked (only the rspotify recommendations,
        // not the prepended domain-layer seed track).
        let track_check = ids::track_check_ids(full_tracks.iter().map(|t| t.id.as_ref()));
        if !track_check.is_empty() {
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_check));
        }
        app.track_table.tracks = full_tracks.iter().map(TrackInfo::from).collect();

        // Prepend the seed track if available so user knows context
        if let Some(track) = *first_track {
          app.track_table.tracks.insert(0, track);
        }
        app.track_table.context = Some(TrackTableContext::RecommendedTracks);
        app.push_navigation_stack(RouteId::Recommendations, ActiveBlock::TrackTable);
      }
      Err(e) if is_development_mode_error(&e) => {
        self
          .raise_and_remind_unavailable("Recommendation", RestrictedEndpoint::Recommendations)
          .await
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_recommendations_for_track_id(
    &mut self,
    track_id: TrackId<'static>,
    country: Option<Country>,
  ) {
    let seed_tracks = Some(vec![track_id.clone()]);
    let first_track: Box<Option<TrackInfo>> = Box::new(None);

    self
      .get_recommendations_for_seed(None, seed_tracks, first_track, country)
      .await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::App;
  use crate::core::config::ClientConfig;
  use crate::infra::network::requests::SpotifyApiError;
  use reqwest::StatusCode;
  use std::path::PathBuf;
  use std::sync::Arc;
  use tokio::sync::Mutex;

  #[tokio::test]
  async fn recommendations_short_circuit_before_request_when_the_key_lost_them() {
    let app = Arc::new(Mutex::new(App::default()));
    app
      .lock()
      .await
      .raise_spotify_key_tier(RestrictedEndpoint::Recommendations, None);
    let mut network = Network::new(None, ClientConfig::new(), &app, PathBuf::new());

    network
      .get_recommendations_for_seed(None, None, Box::new(None), None)
      .await;

    let app = app.lock().await;
    assert_eq!(
      app.status_message(),
      Some("Recommendation: removed by Spotify for apps registered after 2024-11-27")
    );
    assert_ne!(app.get_current_route().id, RouteId::Error);
  }

  #[test]
  fn recommendation_errors_classify_removed_endpoint_responses() {
    for status in [StatusCode::FORBIDDEN, StatusCode::NOT_FOUND] {
      let error = anyhow::Error::new(SpotifyApiError {
        status,
        body: "Forbidden".to_string(),
        detail: None,
      });
      assert!(is_development_mode_error(&error));
    }

    assert!(!is_development_mode_error(&anyhow::anyhow!(
      "transport failure"
    )));
  }
}
