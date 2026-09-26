use super::*;

impl App {
  /// Whether the request funnel will refuse `endpoint` for this key.
  pub(crate) fn spotify_endpoint_blocked(&self, endpoint: RestrictedEndpoint) -> bool {
    self.spotify_key_tier >= endpoint.required_tier()
  }

  /// Record that `endpoint` refused this key, and persist the new tier for the
  /// client ID. Ratchets up only.
  pub(crate) fn raise_spotify_key_tier(
    &mut self,
    endpoint: RestrictedEndpoint,
    client_id: Option<&str>,
  ) {
    let required = endpoint.required_tier();
    if required <= self.spotify_key_tier {
      return;
    }
    self.spotify_key_tier = required;

    let Some(client_id) = client_id.map(str::trim).filter(|id| !id.is_empty()) else {
      return;
    };
    if self
      .runtime_state
      .client_key_tiers
      .get(client_id)
      .is_some_and(|recorded| *recorded >= required)
    {
      return;
    }

    self
      .runtime_state
      .client_key_tiers
      .insert(client_id.to_string(), required);
    let client_key_tiers = self.runtime_state.client_key_tiers.clone();
    self.schedule_state_save(PersistedRuntimeState {
      client_key_tiers: Some(client_key_tiers),
      ..PersistedRuntimeState::default()
    });
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_refusal_records_the_tier_of_the_endpoint_that_refused() {
    let mut app = App::default();

    app.raise_spotify_key_tier(RestrictedEndpoint::RelatedArtists, Some(" client-a "));

    assert_eq!(app.spotify_key_tier, SpotifyKeyTier::Restricted2024);
    assert_eq!(
      app.runtime_state.client_key_tiers,
      std::collections::BTreeMap::from([("client-a".to_string(), SpotifyKeyTier::Restricted2024)])
    );
    assert_eq!(
      app.pending_state_save_patch.client_key_tiers,
      Some(std::collections::BTreeMap::from([(
        "client-a".to_string(),
        SpotifyKeyTier::Restricted2024
      )]))
    );
  }

  #[test]
  fn the_tier_ratchets_and_the_persisted_patch_is_not_sent_twice() {
    let mut app = App::default();

    app.raise_spotify_key_tier(RestrictedEndpoint::ArtistTopTracks, Some("client-a"));
    assert_eq!(app.spotify_key_tier, SpotifyKeyTier::Restricted2026);

    // A later, weaker refusal changes nothing — not the tier, not the patch.
    app.raise_spotify_key_tier(RestrictedEndpoint::RelatedArtists, Some("client-a"));
    assert_eq!(app.spotify_key_tier, SpotifyKeyTier::Restricted2026);
    assert_eq!(
      app.runtime_state.client_key_tiers,
      std::collections::BTreeMap::from([("client-a".to_string(), SpotifyKeyTier::Restricted2026)])
    );
  }

  #[test]
  fn a_refusal_without_a_client_id_still_tightens_this_run() {
    let mut app = App::default();

    app.raise_spotify_key_tier(RestrictedEndpoint::Recommendations, None);

    assert_eq!(app.spotify_key_tier, SpotifyKeyTier::Restricted2024);
    assert!(app.runtime_state.client_key_tiers.is_empty());
    assert!(app.pending_state_save_patch.client_key_tiers.is_none());
  }

  #[test]
  fn a_2024_key_blocks_only_the_2024_endpoints() {
    let mut app = App::default();
    app.raise_spotify_key_tier(RestrictedEndpoint::Recommendations, None);

    assert!(app.spotify_endpoint_blocked(RestrictedEndpoint::Recommendations));
    assert!(app.spotify_endpoint_blocked(RestrictedEndpoint::RelatedArtists));
    assert!(!app.spotify_endpoint_blocked(RestrictedEndpoint::ArtistTopTracks));
    assert!(!app.spotify_endpoint_blocked(RestrictedEndpoint::TracksByIds));
  }

  #[test]
  fn a_key_nothing_has_refused_blocks_nothing() {
    let app = App::default();

    for endpoint in [
      RestrictedEndpoint::Recommendations,
      RestrictedEndpoint::RelatedArtists,
      RestrictedEndpoint::ArtistTopTracks,
      RestrictedEndpoint::TracksByIds,
    ] {
      assert!(
        !app.spotify_endpoint_blocked(endpoint),
        "{endpoint:?} must stay reachable on a fresh key"
      );
    }
  }
}
