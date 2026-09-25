use super::*;

impl App {
  /// Whether the request funnel will refuse `endpoint` for this key.
  pub(crate) fn spotify_endpoint_blocked(&self, endpoint: RestrictedEndpoint) -> bool {
    self.spotify_endpoint_blocked_with(endpoint, simulated_tier())
  }

  /// [`Self::spotify_endpoint_blocked`] with the simulation passed in, for
  /// tests (which may not read the process environment: one process, shared
  /// threads). The env is read only by the two public wrappers above and below.
  fn spotify_endpoint_blocked_with(
    &self,
    endpoint: RestrictedEndpoint,
    simulated: Option<SpotifyKeyTier>,
  ) -> bool {
    SpotifyKeyTier::effective(self.spotify_key_tier, simulated) >= endpoint.required_tier()
  }

  /// Record that `endpoint` refused this key, and persist the new tier for the
  /// client ID. Ratchets up only; a refusal the simulation could have produced
  /// is not evidence about the real key, so it is dropped.
  pub(crate) fn raise_spotify_key_tier(
    &mut self,
    endpoint: RestrictedEndpoint,
    client_id: Option<&str>,
  ) {
    self.raise_spotify_key_tier_with(endpoint, client_id, simulated_tier())
  }

  /// [`Self::raise_spotify_key_tier`] with the simulation passed in, for tests.
  fn raise_spotify_key_tier_with(
    &mut self,
    endpoint: RestrictedEndpoint,
    client_id: Option<&str>,
    simulated: Option<SpotifyKeyTier>,
  ) {
    let required = endpoint.required_tier();
    if !SpotifyKeyTier::refusal_is_evidence(required, self.spotify_key_tier, simulated) {
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

  /// A simulated tier refuses for this run only; recording it would lock a real
  /// key into a tier it never had.
  #[test]
  fn a_simulated_tier_refuses_without_recording_anything() {
    let mut app = App::default();
    let simulated = Some(SpotifyKeyTier::Restricted2024);

    // The 2024 set goes, the 2026 set stays.
    assert!(app.spotify_endpoint_blocked_with(RestrictedEndpoint::Recommendations, simulated));
    assert!(app.spotify_endpoint_blocked_with(RestrictedEndpoint::RelatedArtists, simulated));
    assert!(!app.spotify_endpoint_blocked_with(RestrictedEndpoint::ArtistTopTracks, simulated));
    assert!(!app.spotify_endpoint_blocked_with(RestrictedEndpoint::TracksByIds, simulated));

    // A refusal the simulation explains is not evidence about the real key.
    app.raise_spotify_key_tier_with(
      RestrictedEndpoint::RelatedArtists,
      Some("client-a"),
      simulated,
    );
    assert_eq!(app.spotify_key_tier, SpotifyKeyTier::Full);
    assert!(app.runtime_state.client_key_tiers.is_empty());
    assert!(app.pending_state_save_patch.client_key_tiers.is_none());

    // One it cannot explain still counts: top tracks are lost only at 2026.
    app.raise_spotify_key_tier_with(
      RestrictedEndpoint::ArtistTopTracks,
      Some("client-a"),
      simulated,
    );
    assert_eq!(app.spotify_key_tier, SpotifyKeyTier::Restricted2026);
    assert_eq!(
      app.runtime_state.client_key_tiers.get("client-a"),
      Some(&SpotifyKeyTier::Restricted2026)
    );
  }
}
