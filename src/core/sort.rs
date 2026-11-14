//! Sorting types and utilities for spotatui contexts
//!
//! Provides sorting functionality for playlists, albums, artists, etc.

use crate::core::plugin_api::{PlaylistInfo, TrackInfo};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fields that can be used for sorting
///
/// The serde derives on this enum, `SortOrder` and `SortContext` are the
/// action-vocabulary wire shape: `Action::Sort` carries them directly.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub enum SortField {
  /// Original API order (no sorting applied)
  #[default]
  Default,
  /// Alphabetical by name/title
  Name,
  /// By date added (for playlists, saved albums)
  DateAdded,
  /// By artist name (for tracks)
  Artist,
  /// By track/album duration
  Duration,
  /// By album name (for tracks)
  Album,
  /// By release date (for albums)
  ReleaseDate,
  /// Latest local listen in a playlist context
  RecentlyPlayed,
}

impl SortField {
  /// Get the keyboard shortcut for this field
  pub fn shortcut(&self) -> Option<char> {
    match self {
      SortField::Default => Some('d'),
      SortField::Name => Some('n'),
      SortField::DateAdded => Some('a'),
      SortField::Artist => Some('r'),
      SortField::Duration => Some('t'),
      SortField::Album => Some('l'),
      SortField::ReleaseDate => Some('y'),
      SortField::RecentlyPlayed => Some('r'),
    }
  }

  /// Lowercase config-file token, e.g. `"name"`, `"date_added"`.
  pub fn to_config_str(self) -> &'static str {
    match self {
      SortField::Default => "default",
      SortField::Name => "name",
      SortField::DateAdded => "date_added",
      SortField::Artist => "artist",
      SortField::Duration => "duration",
      SortField::Album => "album",
      SortField::ReleaseDate => "release_date",
      SortField::RecentlyPlayed => "recently_played",
    }
  }

  /// Parse a config-file token back to a `SortField`. Returns `None` for
  /// unknown strings (callers surface the context's valid fields in the error).
  pub fn from_config_str(s: &str) -> Option<Self> {
    match s.trim() {
      "default" => Some(SortField::Default),
      "name" => Some(SortField::Name),
      "date_added" => Some(SortField::DateAdded),
      "artist" => Some(SortField::Artist),
      "duration" => Some(SortField::Duration),
      "album" => Some(SortField::Album),
      "release_date" => Some(SortField::ReleaseDate),
      "recently_played" => Some(SortField::RecentlyPlayed),
      _ => None,
    }
  }
}

/// Sort order direction
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum SortOrder {
  #[default]
  Ascending,
  Descending,
}

impl SortOrder {
  /// Toggle between ascending and descending
  pub fn toggle(&self) -> Self {
    match self {
      SortOrder::Ascending => SortOrder::Descending,
      SortOrder::Descending => SortOrder::Ascending,
    }
  }

  /// Get the sort indicator arrow
  #[allow(dead_code)]
  pub fn indicator(&self) -> &'static str {
    match self {
      SortOrder::Ascending => "↑",
      SortOrder::Descending => "↓",
    }
  }

  /// Get the sort indicator using caller-supplied icons (from config).
  pub fn indicator_icon<'a>(&self, ascending: &'a str, descending: &'a str) -> &'a str {
    match self {
      SortOrder::Ascending => ascending,
      SortOrder::Descending => descending,
    }
  }

  /// Config-file token for the direction suffix (`:desc`).
  #[allow(dead_code)]
  pub fn to_config_suffix(self) -> &'static str {
    match self {
      SortOrder::Ascending => "",
      SortOrder::Descending => ":desc",
    }
  }

  /// Parse a direction suffix. `desc` is descending and `asc` or an empty
  /// string is ascending; any other spelling is `None`, so a config typo is
  /// reported instead of silently sorting ascending.
  pub fn parse_suffix(s: &str) -> Option<Self> {
    match s.trim() {
      "desc" => Some(SortOrder::Descending),
      "" | "asc" => Some(SortOrder::Ascending),
      _ => None,
    }
  }
}

/// Context that supports sorting
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub enum SortContext {
  /// Spotify playlists in the library sidebar
  SidebarPlaylists,
  /// Tracks in a playlist
  PlaylistTracks,
  /// User's saved albums
  SavedAlbums,
  /// User's followed artists
  SavedArtists,
  #[allow(dead_code)]
  /// Recently played tracks
  RecentlyPlayed,
}

impl SortContext {
  /// Get the available sort fields for this context
  pub fn available_fields(&self) -> &'static [SortField] {
    match self {
      SortContext::SidebarPlaylists => &[
        SortField::Default,
        SortField::RecentlyPlayed,
        SortField::Name,
      ],
      SortContext::PlaylistTracks => &[
        SortField::Default,
        SortField::Name,
        SortField::DateAdded,
        SortField::Artist,
        SortField::Album,
        SortField::Duration,
        SortField::ReleaseDate,
      ],
      SortContext::SavedAlbums => &[
        SortField::Default,
        SortField::Name,
        SortField::DateAdded,
        SortField::Artist,
        SortField::ReleaseDate,
      ],
      SortContext::SavedArtists => &[SortField::Default, SortField::Name],
      SortContext::RecentlyPlayed => &[
        SortField::Default,
        SortField::Name,
        SortField::Artist,
        SortField::Album,
      ],
    }
  }
}

/// The three orders for Spotify's playlists sidebar. Folder slots stay fixed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum SidebarPlaylistOrder {
  #[default]
  Spotify,
  RecentlyPlayed,
  Name,
}

impl SidebarPlaylistOrder {
  pub fn field(self) -> SortField {
    match self {
      Self::Spotify => SortField::Default,
      Self::RecentlyPlayed => SortField::RecentlyPlayed,
      Self::Name => SortField::Name,
    }
  }

  pub fn from_field(field: SortField) -> Option<Self> {
    match field {
      SortField::Default => Some(Self::Spotify),
      SortField::RecentlyPlayed => Some(Self::RecentlyPlayed),
      SortField::Name => Some(Self::Name),
      _ => None,
    }
  }
}

/// Latest local listen by playlist context URI; read once off the UI thread and
/// maintained as the history collector appends records.
pub type PlaylistLastListens = BTreeMap<String, DateTime<Utc>>;

/// Reorder only playlist slots at one folder level. Folders, back rows and pins
/// keep their positions. Stable ties (including never-played playlists) keep the
/// rootlist order supplied by the caller, in either frontend.
pub fn sort_playlist_slots<'a, T: Clone>(
  items: &mut [T],
  order: SidebarPlaylistOrder,
  last_listens: &PlaylistLastListens,
  playlist: impl Fn(&T) -> Option<&'a PlaylistInfo>,
) {
  if order == SidebarPlaylistOrder::Spotify {
    return;
  }
  let slots: Vec<usize> = items
    .iter()
    .enumerate()
    .filter_map(|(index, item)| playlist(item).map(|_| index))
    .collect();
  let mut sorted: Vec<T> = slots.iter().map(|index| items[*index].clone()).collect();
  match order {
    SidebarPlaylistOrder::Spotify => {}
    SidebarPlaylistOrder::Name => {
      sorted.sort_by_cached_key(|item| playlist(item).unwrap().name.to_lowercase());
    }
    SidebarPlaylistOrder::RecentlyPlayed => {
      sorted.sort_by_cached_key(|item| {
        std::cmp::Reverse(last_listens.get(&playlist(item).unwrap().uri).copied())
      });
    }
  }
  for (index, item) in slots.into_iter().zip(sorted) {
    items[index] = item;
  }
}

/// Current sort state
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct SortState {
  pub field: SortField,
  pub order: SortOrder,
}

impl SortState {
  pub fn new() -> Self {
    Self::default()
  }

  /// Apply a new sort field, toggling order if same field selected
  pub fn apply_field(&mut self, field: SortField) {
    if self.field == field {
      self.order = self.order.toggle();
    } else {
      self.field = field;
      self.order = SortOrder::Ascending;
    }
  }

  /// Reset to default sort state
  #[allow(dead_code)]
  pub fn reset(&mut self) {
    self.field = SortField::Default;
    self.order = SortOrder::Ascending;
  }

  /// Render as a config token: `"field"` or `"field:desc"`.
  #[allow(dead_code)]
  pub fn to_config_str(self) -> String {
    format!(
      "{}{}",
      self.field.to_config_str(),
      self.order.to_config_suffix()
    )
  }

  /// Parse a `"<field>"` / `"<field>:desc"` spec, validating that the field is
  /// available in `ctx`. Hard error (with the context's valid fields) if the
  /// field is unknown or unavailable in this context, or if the direction is
  /// not `asc` / `desc` / empty.
  pub fn parse(spec: &str, ctx: SortContext) -> Result<Self, String> {
    let (field_str, order_str) = match spec.split_once(':') {
      Some((f, o)) => (f, o),
      None => (spec, ""),
    };
    let field = SortField::from_config_str(field_str).ok_or_else(|| {
      format!(
        "unknown sort field '{}' (valid for this context: {})",
        field_str.trim(),
        ctx
          .available_fields()
          .iter()
          .map(|f| f.to_config_str())
          .collect::<Vec<_>>()
          .join(", ")
      )
    })?;
    if !ctx.available_fields().contains(&field) {
      return Err(format!(
        "sort field '{}' is not available in this context (valid: {})",
        field.to_config_str(),
        ctx
          .available_fields()
          .iter()
          .map(|f| f.to_config_str())
          .collect::<Vec<_>>()
          .join(", ")
      ));
    }
    let order = SortOrder::parse_suffix(order_str).ok_or_else(|| {
      format!(
        "unknown sort direction '{}' (expected asc or desc)",
        order_str.trim()
      )
    })?;
    Ok(Self { field, order })
  }
}

/// Sort by a precomputed key — one key per item (O(n) allocations) instead of
/// one per comparison (O(n log n)) — honoring the sort direction. Stable in
/// both directions: equal keys keep their prior relative order.
pub fn sort_by_key_with_order<T, K: Ord, F: FnMut(&T) -> K>(
  items: &mut [T],
  order: SortOrder,
  mut key: F,
) {
  match order {
    SortOrder::Ascending => items.sort_by_cached_key(|item| key(item)),
    SortOrder::Descending => items.sort_by_cached_key(|item| std::cmp::Reverse(key(item))),
  }
}

pub struct Sorter {
  state: SortState,
}

impl Sorter {
  pub fn new(state: SortState) -> Self {
    Self { state }
  }

  pub fn sort_tracks(&self, tracks: &mut [TrackInfo]) {
    if self.state.field == SortField::Default {
      return;
    }

    // DateAdded requires PlaylistItem metadata, which is not carried by
    // the source-agnostic track snapshot. Preserve playlist order.
    if self.state.field == SortField::DateAdded {
      return;
    }

    match self.state.field {
      SortField::Name => {
        sort_by_key_with_order(tracks, self.state.order, |t| t.name.to_lowercase())
      }
      SortField::Artist => sort_by_key_with_order(tracks, self.state.order, |t| {
        t.artists.first().map(|s| s.to_lowercase())
      }),
      SortField::Album => {
        sort_by_key_with_order(tracks, self.state.order, |t| t.album.to_lowercase())
      }
      SortField::Duration => sort_by_key_with_order(tracks, self.state.order, |t| t.duration_ms),
      SortField::ReleaseDate => {
        sort_by_key_with_order(tracks, self.state.order, |t| t.release_date.clone())
      }
      _ => {}
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_sort_state_apply_field() {
    let mut state = SortState::new();
    assert_eq!(state.field, SortField::Default);
    assert_eq!(state.order, SortOrder::Ascending);

    // Apply new field
    state.apply_field(SortField::Name);
    assert_eq!(state.field, SortField::Name);
    assert_eq!(state.order, SortOrder::Ascending);

    // Apply same field toggles order
    state.apply_field(SortField::Name);
    assert_eq!(state.field, SortField::Name);
    assert_eq!(state.order, SortOrder::Descending);

    // Apply different field resets order
    state.apply_field(SortField::Artist);
    assert_eq!(state.field, SortField::Artist);
    assert_eq!(state.order, SortOrder::Ascending);
  }

  #[test]
  fn test_sort_order_toggle() {
    assert_eq!(SortOrder::Ascending.toggle(), SortOrder::Descending);
    assert_eq!(SortOrder::Descending.toggle(), SortOrder::Ascending);
  }

  #[test]
  fn sort_order_indicators_and_config_suffixes_match_direction() {
    assert_eq!(SortOrder::Ascending.indicator(), "↑");
    assert_eq!(SortOrder::Descending.indicator(), "↓");
    assert_eq!(SortOrder::Ascending.to_config_suffix(), "");
    assert_eq!(SortOrder::Descending.to_config_suffix(), ":desc");
  }

  #[test]
  fn sort_state_config_round_trips() {
    let parsed = SortState::parse("artist:desc", SortContext::PlaylistTracks).unwrap();
    assert_eq!(
      parsed,
      SortState {
        field: SortField::Artist,
        order: SortOrder::Descending
      }
    );
    assert_eq!(parsed.to_config_str(), "artist:desc");
    assert_eq!(
      SortState::parse("artist", SortContext::SavedArtists).unwrap_err(),
      "sort field 'artist' is not available in this context (valid: default, name)"
    );
  }

  #[test]
  fn unknown_sort_direction_is_rejected_instead_of_sorting_ascending() {
    for spec in ["name:dsc", "name:DESC", "name:descending"] {
      let direction = spec.split_once(':').unwrap().1;
      assert_eq!(
        SortState::parse(spec, SortContext::SavedArtists).unwrap_err(),
        format!("unknown sort direction '{direction}' (expected asc or desc)")
      );
    }
    // The valid spellings keep working; the suffix stays trimmed.
    for (spec, order) in [
      ("name", SortOrder::Ascending),
      ("name:asc", SortOrder::Ascending),
      ("name:desc", SortOrder::Descending),
      ("name: desc", SortOrder::Descending),
    ] {
      assert_eq!(
        SortState::parse(spec, SortContext::SavedArtists)
          .unwrap()
          .order,
        order
      );
    }
  }

  #[test]
  fn release_date_sort_is_offered_for_saved_albums_and_playlists_only() {
    for context in [SortContext::SavedAlbums, SortContext::PlaylistTracks] {
      let parsed = SortState::parse("release_date:desc", context).unwrap();
      assert_eq!(parsed.field, SortField::ReleaseDate);
      assert_eq!(parsed.to_config_str(), "release_date:desc");
    }
    assert!(SortState::parse("release_date", SortContext::SavedArtists).is_err());
    assert!(SortState::parse("release_date", SortContext::RecentlyPlayed).is_err());
  }

  #[test]
  fn test_context_available_fields() {
    let fields = SortContext::PlaylistTracks.available_fields();
    assert!(fields.contains(&SortField::Name));
    assert!(fields.contains(&SortField::Artist));

    let fields = SortContext::SavedArtists.available_fields();
    assert!(fields.contains(&SortField::Name));
    assert!(!fields.contains(&SortField::Artist));
  }

  fn track(name: &str, artist: Option<&str>, album: &str, duration_ms: u64) -> TrackInfo {
    TrackInfo {
      generated_name: None,
      uri: None,
      name: name.to_string(),
      artists: artist.map(|a| vec![a.to_string()]).unwrap_or_default(),
      album: album.to_string(),
      duration_ms,
      id: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 0,
      explicit: false,
      image_url: None,
      release_date: None,
    }
  }

  fn sorted_names(tracks: &[TrackInfo]) -> Vec<&str> {
    tracks.iter().map(|t| t.name.as_str()).collect()
  }

  fn dated(name: &str, release_date: Option<&str>) -> TrackInfo {
    TrackInfo {
      release_date: release_date.map(str::to_string),
      ..track(name, None, "", 0)
    }
  }

  #[test]
  fn release_date_sort_orders_tracks_by_date_with_undated_tracks_first() {
    let tracks = vec![
      dated("2019-06-21", Some("2019-06-21")),
      dated("undated", None),
      dated("1997", Some("1997")),
      dated("2019", Some("2019")),
      dated("2005-03", Some("2005-03")),
    ];

    let mut ascending = tracks.clone();
    Sorter::new(SortState {
      field: SortField::ReleaseDate,
      order: SortOrder::Ascending,
    })
    .sort_tracks(&mut ascending);
    assert_eq!(
      sorted_names(&ascending),
      ["undated", "1997", "2005-03", "2019", "2019-06-21"]
    );

    let mut descending = tracks;
    Sorter::new(SortState {
      field: SortField::ReleaseDate,
      order: SortOrder::Descending,
    })
    .sort_tracks(&mut descending);
    assert_eq!(
      sorted_names(&descending),
      ["2019-06-21", "2019", "2005-03", "1997", "undated"]
    );
  }

  #[test]
  fn name_sort_ignores_letter_case() {
    let mut tracks = vec![
      track("beta", None, "", 0),
      track("Alpha", None, "", 0),
      track("gamma", None, "", 0),
      track("Delta", None, "", 0),
    ];
    Sorter::new(SortState {
      field: SortField::Name,
      order: SortOrder::Ascending,
    })
    .sort_tracks(&mut tracks);
    assert_eq!(
      sorted_names(&tracks),
      vec!["Alpha", "beta", "Delta", "gamma"]
    );
  }

  #[test]
  fn descending_name_sort_ignores_letter_case() {
    let mut tracks = vec![
      track("beta", None, "", 0),
      track("Alpha", None, "", 0),
      track("gamma", None, "", 0),
      track("Delta", None, "", 0),
    ];
    Sorter::new(SortState {
      field: SortField::Name,
      order: SortOrder::Descending,
    })
    .sort_tracks(&mut tracks);
    assert_eq!(
      sorted_names(&tracks),
      vec!["gamma", "Delta", "beta", "Alpha"]
    );
  }

  #[test]
  fn artist_sort_ignores_letter_case_and_puts_tracks_without_artists_first() {
    let mut tracks = vec![
      track("alt-J track", Some("alt-J"), "", 0),
      track("Björk track", Some("Björk"), "", 0),
      track("ABBA track", Some("ABBA"), "", 0),
      track("no artist", None, "", 0),
    ];
    Sorter::new(SortState {
      field: SortField::Artist,
      order: SortOrder::Ascending,
    })
    .sort_tracks(&mut tracks);
    assert_eq!(
      sorted_names(&tracks),
      vec!["no artist", "ABBA track", "alt-J track", "Björk track"]
    );
  }

  #[test]
  fn album_sort_ignores_letter_case() {
    let mut tracks = vec![
      track("t1", None, "Zebra", 0),
      track("t2", None, "apple", 0),
      track("t3", None, "Mango", 0),
    ];
    Sorter::new(SortState {
      field: SortField::Album,
      order: SortOrder::Ascending,
    })
    .sort_tracks(&mut tracks);
    let albums: Vec<&str> = tracks.iter().map(|t| t.album.as_str()).collect();
    assert_eq!(albums, vec!["apple", "Mango", "Zebra"]);
  }

  #[test]
  fn duration_sort_orders_by_length() {
    let mut tracks = vec![
      track("long", None, "", 200_000),
      track("short", None, "", 100_000),
    ];
    Sorter::new(SortState {
      field: SortField::Duration,
      order: SortOrder::Ascending,
    })
    .sort_tracks(&mut tracks);
    assert_eq!(
      tracks.iter().map(|t| t.duration_ms).collect::<Vec<_>>(),
      vec![100_000, 200_000]
    );
  }

  #[test]
  fn date_added_and_default_sorts_keep_playlist_order() {
    let original = vec![
      track("b", None, "", 0),
      track("A", None, "", 0),
      track("c", None, "", 0),
    ];
    for field in [SortField::DateAdded, SortField::Default] {
      for order in [SortOrder::Ascending, SortOrder::Descending] {
        let mut tracks = original.clone();
        Sorter::new(SortState { field, order }).sort_tracks(&mut tracks);
        assert_eq!(
          sorted_names(&tracks),
          vec!["b", "A", "c"],
          "{field:?} {order:?}"
        );
      }
    }
  }
}
