//! Tolerant parsing of dispatch-layer string ids/URIs into rspotify id-types.
//!
//! `IoEvent` variants carry plain `String` ids/URIs — the multi-source dispatch
//! contract (handlers and UI never touch `rspotify::model`). The Spotify network
//! boundary is the one place allowed to reconstruct the strongly-typed rspotify
//! ids, and it does so here.
//!
//! Parsing is deliberately tolerant: a malformed or wrong-type id yields `None`
//! and the caller silently no-ops. This preserves the pre-refactor handler
//! behaviour, where each dispatch site guarded on `if let Ok(id) = ..from_id(..)`
//! and simply skipped the action for an unparseable id.

use rspotify::model::idtypes::{
  AlbumId, ArtistId, EpisodeId, PlayContextId, PlayableId, PlaylistId, ShowId, TrackId, UserId,
};

/// Generate a tolerant single-id parser for a monomorphic rspotify id-type.
///
/// Accepts either a bare base62 id or a full `spotify:{type}:{id}` URI (via
/// [`from_id_or_uri`]); returns `None` when the string is neither a valid id nor
/// a URI of the matching type.
macro_rules! mono_id {
  ($(#[$doc:meta])* $fn_name:ident => $id_ty:ident) => {
    $(#[$doc])*
    pub(crate) fn $fn_name(id_or_uri: &str) -> Option<$id_ty<'static>> {
      $id_ty::from_id_or_uri(id_or_uri)
        .ok()
        .map(|id| id.into_static())
    }
  };
}

mono_id!(
  /// Parse a single track id/URI.
  track_id => TrackId
);
mono_id!(
  /// Parse a single album id/URI.
  album_id => AlbumId
);
mono_id!(
  /// Parse a single artist id/URI.
  artist_id => ArtistId
);
mono_id!(
  /// Parse a single playlist id/URI.
  playlist_id => PlaylistId
);
mono_id!(
  /// Parse a single show id/URI.
  show_id => ShowId
);
mono_id!(
  /// Parse a single user id/URI.
  user_id => UserId
);

/// Parse a slice of track ids/URIs, dropping any that fail to parse.
pub(crate) fn track_ids(items: &[String]) -> Vec<TrackId<'static>> {
  items.iter().filter_map(|s| track_id(s)).collect()
}

/// Parse a slice of album ids/URIs, dropping any that fail to parse.
pub(crate) fn album_ids(items: &[String]) -> Vec<AlbumId<'static>> {
  items.iter().filter_map(|s| album_id(s)).collect()
}

/// Parse a slice of artist ids/URIs, dropping any that fail to parse.
pub(crate) fn artist_ids(items: &[String]) -> Vec<ArtistId<'static>> {
  items.iter().filter_map(|s| artist_id(s)).collect()
}

/// Parse a slice of show ids/URIs, dropping any that fail to parse.
pub(crate) fn show_ids(items: &[String]) -> Vec<ShowId<'static>> {
  items.iter().filter_map(|s| show_id(s)).collect()
}

/// Parse a playable id/URI into a [`PlayableId`] (track or episode).
///
/// The URI scheme selects the variant (`spotify:track:` vs `spotify:episode:`).
/// A bare base62 id is ambiguous, so it falls back to a track id — matching the
/// historical dispatch sites, which only ever built `PlayableId::Track` from a
/// bare id.
pub(crate) fn playable_id(uri_or_id: &str) -> Option<PlayableId<'static>> {
  if let Ok(id) = TrackId::from_uri(uri_or_id) {
    return Some(PlayableId::Track(id.into_static()));
  }
  if let Ok(id) = EpisodeId::from_uri(uri_or_id) {
    return Some(PlayableId::Episode(id.into_static()));
  }
  TrackId::from_id(uri_or_id)
    .ok()
    .map(|id| PlayableId::Track(id.into_static()))
}

/// Parse a slice of playable ids/URIs, dropping any that fail to parse.
pub(crate) fn playable_ids(items: &[String]) -> Vec<PlayableId<'static>> {
  items.iter().filter_map(|s| playable_id(s)).collect()
}

/// Parse a context URI into a [`PlayContextId`] (artist/album/playlist/show).
///
/// Requires a full `spotify:` URI: a bare base62 id is ambiguous across context
/// types, so it returns `None`. Dispatch sites pass the domain `uri` field.
pub(crate) fn play_context_id(uri: &str) -> Option<PlayContextId<'static>> {
  if let Ok(id) = ArtistId::from_uri(uri) {
    return Some(PlayContextId::Artist(id.into_static()));
  }
  if let Ok(id) = AlbumId::from_uri(uri) {
    return Some(PlayContextId::Album(id.into_static()));
  }
  if let Ok(id) = PlaylistId::from_uri(uri) {
    return Some(PlayContextId::Playlist(id.into_static()));
  }
  if let Ok(id) = ShowId::from_uri(uri) {
    return Some(PlayContextId::Show(id.into_static()));
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  // 22-char base62 id (matches the convention used elsewhere in the tests).
  const BARE: &str = "0000000000000000000001";

  #[test]
  fn play_context_id_sniffs_each_context_type_from_its_uri() {
    assert!(matches!(
      play_context_id(&format!("spotify:album:{BARE}")),
      Some(PlayContextId::Album(_))
    ));
    assert!(matches!(
      play_context_id(&format!("spotify:artist:{BARE}")),
      Some(PlayContextId::Artist(_))
    ));
    assert!(matches!(
      play_context_id(&format!("spotify:playlist:{BARE}")),
      Some(PlayContextId::Playlist(_))
    ));
    assert!(matches!(
      play_context_id(&format!("spotify:show:{BARE}")),
      Some(PlayContextId::Show(_))
    ));
  }

  #[test]
  fn play_context_id_rejects_bare_id_and_non_context_uri() {
    // A bare id is ambiguous across context types.
    assert!(play_context_id(BARE).is_none());
    // A track is a playable, not a play context.
    assert!(play_context_id(&format!("spotify:track:{BARE}")).is_none());
  }

  #[test]
  fn playable_id_sniffs_track_vs_episode_from_uri() {
    assert!(matches!(
      playable_id(&format!("spotify:track:{BARE}")),
      Some(PlayableId::Track(_))
    ));
    assert!(matches!(
      playable_id(&format!("spotify:episode:{BARE}")),
      Some(PlayableId::Episode(_))
    ));
  }

  #[test]
  fn playable_id_treats_bare_id_as_track() {
    assert!(matches!(playable_id(BARE), Some(PlayableId::Track(_))));
  }

  #[test]
  fn mono_parser_accepts_id_or_matching_uri_and_rejects_wrong_type() {
    assert!(track_id(BARE).is_some());
    assert!(track_id(&format!("spotify:track:{BARE}")).is_some());
    assert!(album_id(&format!("spotify:album:{BARE}")).is_some());
    // A wrong-type URI must not parse as a track (no fallback to from_id).
    assert!(track_id(&format!("spotify:album:{BARE}")).is_none());
  }

  #[test]
  fn collection_parser_drops_unparseable_entries() {
    let ids = vec![
      format!("spotify:track:{BARE}"),
      BARE.to_string(),
      "not a valid id!".to_string(),
    ];
    // Only the two well-formed entries survive.
    assert_eq!(track_ids(&ids).len(), 2);
  }
}
