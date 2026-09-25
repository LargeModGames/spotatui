//! Which Spotify Web API endpoints a client ID may still call.
//!
//! Spotify restricted the Web API twice; where a key lands depends on its
//! registration date:
//!
//! | Registered | Tier | Lost |
//! |---|---|---|
//! | before 2024-11-27 | `Full` | nothing |
//! | 2024-11-27 ... 2026-02-11 | `Restricted2024` | recommendations, related artists |
//! | after 2026-02-11 | `Restricted2026` | the above, plus artist top tracks, `tracks?ids=` |
//!
//! A tier is learned from a 403/404 on one of these endpoints and only
//! ratchets up (`App::raise_spotify_key_tier`). Everything reads it through
//! `App::spotify_endpoint_blocked`: the request funnel and the screens that
//! render the result.

use serde::{Deserialize, Serialize};

/// How much of the Spotify Web API a client ID may use.
///
/// Ordered: `Full < Restricted2024 < Restricted2026`, so `max` is a ratchet and
/// `a >= b` means "at least as restricted as".
#[derive(
  Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SpotifyKeyTier {
  /// Registered before 2024-11-27: every endpoint spotatui uses.
  #[default]
  #[serde(rename = "full")]
  Full,
  /// Registered 2024-11-27 … 2026-02-11: no recommendations, no related artists.
  #[serde(rename = "restricted-2024")]
  Restricted2024,
  /// Registered after 2026-02-11: the 2024 set plus artist top tracks and
  /// `tracks?ids=`.
  #[serde(rename = "restricted-2026")]
  Restricted2026,
}

impl SpotifyKeyTier {
  /// Status-line text for an endpoint lost at this tier.
  pub const fn restriction_note(self) -> &'static str {
    match self {
      // Unreachable from `required_tier`; a note beats a panic.
      Self::Full => "unavailable for this Spotify app",
      Self::Restricted2024 => "removed by Spotify for apps registered after 2024-11-27",
      Self::Restricted2026 => "removed by Spotify for apps registered after 2026-02-11",
    }
  }

  /// `0` = full, `1` = 2024 cut, `2` = 2026 cut; higher clamps.
  pub fn from_level(level: u8) -> Self {
    match level {
      0 => Self::Full,
      1 => Self::Restricted2024,
      _ => Self::Restricted2026,
    }
  }

  /// The recorded tier, widened by the simulation.
  pub fn effective(recorded: Self, simulated: Option<Self>) -> Self {
    match simulated {
      Some(simulated) if simulated > recorded => simulated,
      _ => recorded,
    }
  }

  /// Whether a refusal of an endpoint needing `required` proves the real key
  /// lost it. False when already recorded, or when the simulation alone could
  /// have produced it - a simulated refusal is never recorded.
  pub fn refusal_is_evidence(required: Self, recorded: Self, simulated: Option<Self>) -> bool {
    required > recorded && !simulated.is_some_and(|simulated| simulated >= required)
  }
}

/// An endpoint whose availability depends on the key's tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RestrictedEndpoint {
  /// `GET recommendations` — the seed-based recommender.
  Recommendations,
  /// `GET artists/{id}/related-artists` — the "Fans also like" column.
  RelatedArtists,
  /// `GET artists/{id}/top-tracks` — the artist page's top ten.
  ArtistTopTracks,
  /// `GET tracks?ids=` — the batch track lookup.
  TracksByIds,
}

impl RestrictedEndpoint {
  /// The tier that first *loses* this endpoint.
  pub const fn required_tier(self) -> SpotifyKeyTier {
    match self {
      Self::Recommendations | Self::RelatedArtists => SpotifyKeyTier::Restricted2024,
      Self::ArtistTopTracks | Self::TracksByIds => SpotifyKeyTier::Restricted2026,
    }
  }

  /// [`SpotifyKeyTier::restriction_note`] of the tier that loses this endpoint.
  pub const fn unavailable_note(self) -> &'static str {
    self.required_tier().restriction_note()
  }
}

/// Match a request against the restricted table; `None` for everything else.
///
/// Exact segments, so `albums/{id}/tracks` is not the batch track lookup.
/// `me/top/{action}` survived both cuts and is never refused. Only `GET` is
/// classified: a write to the same path is a different endpoint.
pub fn restricted_endpoint(
  method: &str,
  path: &str,
  query: &[(&str, String)],
) -> Option<RestrictedEndpoint> {
  if !method.eq_ignore_ascii_case("get") {
    return None;
  }

  let mut segments = path
    .trim()
    .trim_matches('/')
    .split('/')
    .filter(|s| !s.is_empty());
  let first = segments.next()?;
  let second = segments.next();
  let third = segments.next();
  // Anything deeper is a different resource; refusing a prefix would be wrong.
  if segments.next().is_some() {
    return None;
  }

  match (first, second, third) {
    ("recommendations", None, None) => Some(RestrictedEndpoint::Recommendations),
    ("artists", Some(_), Some("related-artists")) => Some(RestrictedEndpoint::RelatedArtists),
    ("artists", Some(_), Some("top-tracks")) => Some(RestrictedEndpoint::ArtistTopTracks),
    ("tracks", None, None) if query.iter().any(|(key, _)| *key == "ids") => {
      Some(RestrictedEndpoint::TracksByIds)
    }
    _ => None,
  }
}

/// Pretend the key is narrower than it is (`0`/`1`/`2`), so all three
/// generations can be exercised without three Spotify apps. Widens the
/// effective tier only; a simulated refusal is never recorded.
pub const SIMULATED_TIER_ENV_VAR: &str = "MAKE_OUD_SUFFER";

/// The tier `MAKE_OUD_SUFFER` asks for, read from the environment.
pub fn simulated_tier() -> Option<SpotifyKeyTier> {
  simulated_tier_from(std::env::var(SIMULATED_TIER_ENV_VAR).ok().as_deref())
}

/// [`simulated_tier`] from a raw value, so tests never touch the
/// process environment (one process, shared threads).
pub fn simulated_tier_from(raw: Option<&str>) -> Option<SpotifyKeyTier> {
  raw?
    .trim()
    .parse::<u8>()
    .ok()
    .map(SpotifyKeyTier::from_level)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn ids_query() -> Vec<(&'static str, String)> {
    vec![("ids", "a,b".to_string())]
  }

  #[test]
  fn every_retired_endpoint_classifies_to_its_generation() {
    let cases = [
      (
        "recommendations",
        vec![],
        RestrictedEndpoint::Recommendations,
      ),
      (
        "artists/abc/related-artists",
        vec![],
        RestrictedEndpoint::RelatedArtists,
      ),
      (
        "artists/abc/top-tracks",
        vec![],
        RestrictedEndpoint::ArtistTopTracks,
      ),
      ("tracks", ids_query(), RestrictedEndpoint::TracksByIds),
    ];

    for (path, query, expected) in cases {
      assert_eq!(
        restricted_endpoint("GET", path, &query),
        Some(expected),
        "{path} should classify"
      );
    }
  }

  #[test]
  fn surviving_and_lookalike_paths_are_not_refused() {
    let allowed = [
      "me/top/artists",
      "me/top/tracks",
      "albums/abc/tracks",
      "artists/abc/albums",
      "playlists/abc/tracks",
      "me/player/recently-played",
      "recommendations/related",
      "search",
    ];

    for path in allowed {
      assert_eq!(
        restricted_endpoint("GET", path, &ids_query()),
        None,
        "{path} must not be refused"
      );
    }
  }

  #[test]
  fn the_batch_track_lookup_needs_the_ids_parameter() {
    assert_eq!(restricted_endpoint("GET", "tracks", &[]), None);
    assert_eq!(
      restricted_endpoint("GET", "tracks", &[("ids", "a".to_string())]),
      Some(RestrictedEndpoint::TracksByIds)
    );
    assert_eq!(
      restricted_endpoint("GET", "tracks", &[("id", "a".to_string())]),
      None
    );
  }

  #[test]
  fn only_get_requests_are_classified() {
    assert_eq!(
      restricted_endpoint("PUT", "artists/abc/top-tracks", &[]),
      None,
      "a write to a restricted path is a different endpoint"
    );
    assert_eq!(restricted_endpoint("post", "recommendations", &[]), None);
  }

  #[test]
  fn the_two_generations_split_where_spotify_split_them() {
    assert_eq!(
      RestrictedEndpoint::Recommendations.required_tier(),
      SpotifyKeyTier::Restricted2024
    );
    assert_eq!(
      RestrictedEndpoint::RelatedArtists.required_tier(),
      SpotifyKeyTier::Restricted2024
    );
    assert_eq!(
      RestrictedEndpoint::ArtistTopTracks.required_tier(),
      SpotifyKeyTier::Restricted2026
    );
    assert_eq!(
      RestrictedEndpoint::TracksByIds.required_tier(),
      SpotifyKeyTier::Restricted2026
    );
  }

  #[test]
  fn a_2024_key_keeps_what_a_2026_key_lost() {
    let tier_2024 = SpotifyKeyTier::Restricted2024;

    assert!(tier_2024 >= RestrictedEndpoint::Recommendations.required_tier());
    assert!(tier_2024 < RestrictedEndpoint::ArtistTopTracks.required_tier());
  }

  #[test]
  fn effective_tier_takes_the_widest_of_recorded_and_simulated() {
    assert_eq!(
      SpotifyKeyTier::effective(SpotifyKeyTier::Full, None),
      SpotifyKeyTier::Full
    );
    assert_eq!(
      SpotifyKeyTier::effective(SpotifyKeyTier::Full, Some(SpotifyKeyTier::Restricted2024)),
      SpotifyKeyTier::Restricted2024
    );
    assert_eq!(
      SpotifyKeyTier::effective(
        SpotifyKeyTier::Restricted2026,
        Some(SpotifyKeyTier::Restricted2024)
      ),
      SpotifyKeyTier::Restricted2026,
      "a simulation must never narrow a recorded tier"
    );
  }

  #[test]
  fn a_refusal_the_simulation_explains_is_not_recorded() {
    use SpotifyKeyTier::*;

    let recorded = Full;
    let simulated = Some(Restricted2024);

    // The simulation produced this refusal: no evidence about the real key.
    assert!(!SpotifyKeyTier::refusal_is_evidence(
      Restricted2024,
      recorded,
      simulated
    ));
    // A stricter endpoint still needs a real answer.
    assert!(SpotifyKeyTier::refusal_is_evidence(
      Restricted2026,
      recorded,
      simulated
    ));
    // With no simulation, a refusal is always evidence.
    assert!(SpotifyKeyTier::refusal_is_evidence(
      Restricted2024,
      recorded,
      None
    ));
    // Already recorded: a ratchet needs no second proof.
    assert!(!SpotifyKeyTier::refusal_is_evidence(
      Restricted2024,
      Restricted2024,
      None
    ));
  }

  #[test]
  fn simulated_tier_reads_the_env_value_without_touching_the_environment() {
    assert_eq!(simulated_tier_from(None), None);
    assert_eq!(simulated_tier_from(Some("")), None);
    assert_eq!(simulated_tier_from(Some("soon")), None);
    assert_eq!(simulated_tier_from(Some("0")), Some(SpotifyKeyTier::Full));
    assert_eq!(
      simulated_tier_from(Some(" 1 ")),
      Some(SpotifyKeyTier::Restricted2024)
    );
    assert_eq!(
      simulated_tier_from(Some("2")),
      Some(SpotifyKeyTier::Restricted2026)
    );
    assert_eq!(
      simulated_tier_from(Some("9")),
      Some(SpotifyKeyTier::Restricted2026)
    );
  }

  #[test]
  fn an_unavailable_note_names_the_registration_cutoff() {
    assert_eq!(
      RestrictedEndpoint::Recommendations.unavailable_note(),
      "removed by Spotify for apps registered after 2024-11-27"
    );
    assert_eq!(
      RestrictedEndpoint::ArtistTopTracks.unavailable_note(),
      "removed by Spotify for apps registered after 2026-02-11"
    );
  }

  #[test]
  fn a_tier_survives_a_state_round_trip_as_a_readable_name() {
    let tiers = std::collections::BTreeMap::from([
      ("client-a".to_string(), SpotifyKeyTier::Restricted2024),
      ("client-b".to_string(), SpotifyKeyTier::Restricted2026),
    ]);
    let yaml = serde_yaml::to_string(&tiers).unwrap();

    assert!(yaml.contains("restricted-2024"), "{yaml}");
    assert_eq!(
      serde_yaml::from_str::<std::collections::BTreeMap<String, SpotifyKeyTier>>(&yaml).unwrap(),
      tiers
    );
  }
}
