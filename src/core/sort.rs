//! Sorting types and utilities for spotatui contexts
//!
//! Provides sorting functionality for playlists, albums, artists, etc.

use rspotify::model::track::FullTrack;

/// Fields that can be used for sorting
#[derive(Clone, Copy, PartialEq, Debug, Default)]
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
}

impl SortField {
  /// Get display name for the sort field
  pub fn display_name(&self) -> &'static str {
    match self {
      SortField::Default => "Default",
      SortField::Name => "Name",
      SortField::DateAdded => "Date Added",
      SortField::Artist => "Artist",
      SortField::Duration => "Duration",
      SortField::Album => "Album",
    }
  }

  /// Get the keyboard shortcut for this field
  pub fn shortcut(&self) -> Option<char> {
    match self {
      SortField::Default => Some('d'),
      SortField::Name => Some('n'),
      SortField::DateAdded => Some('a'),
      SortField::Artist => Some('r'),
      SortField::Duration => Some('t'),
      SortField::Album => Some('l'),
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
      _ => None,
    }
  }
}

/// Sort order direction
#[derive(Clone, Copy, PartialEq, Debug, Default)]
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

  /// Parse a `:desc` direction suffix. Only `desc` flips to descending;
  /// everything else (including `asc` or empty) is ascending.
  pub fn parse_suffix(s: &str) -> Self {
    match s.trim() {
      "desc" => SortOrder::Descending,
      _ => SortOrder::Ascending,
    }
  }
}

/// Context that supports sorting
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortContext {
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
      SortContext::PlaylistTracks => &[
        SortField::Default,
        SortField::Name,
        SortField::DateAdded,
        SortField::Artist,
        SortField::Album,
        SortField::Duration,
      ],
      SortContext::SavedAlbums => &[
        SortField::Default,
        SortField::Name,
        SortField::DateAdded,
        SortField::Artist,
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
  /// field is unknown or unavailable in this context.
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
    Ok(Self {
      field,
      order: SortOrder::parse_suffix(order_str),
    })
  }
}

pub struct Sorter {
  state: SortState,
}

impl Sorter {
  pub fn new(state: SortState) -> Self {
    Self { state }
  }

  pub fn sort_tracks(&self, tracks: &mut [FullTrack]) {
    if self.state.field == SortField::Default {
      return;
    }

    tracks.sort_by(|a, b| {
      let order = match self.state.field {
        SortField::Name => a.name.cmp(&b.name),
        SortField::Duration => a.duration.cmp(&b.duration),
        SortField::Artist => {
          let empty_string = String::new();
          let artist_a = a
            .artists
            .first()
            .map(|ar| &ar.name)
            .unwrap_or(&empty_string);
          let artist_b = b
            .artists
            .first()
            .map(|ar| &ar.name)
            .unwrap_or(&empty_string);
          artist_a.cmp(artist_b)
        }
        SortField::Album => a.album.name.cmp(&b.album.name),
        // DateAdded requires PlaylistItem wrapper which we don't have here.
        // Assuming Default order is DateAdded for playlists.
        _ => std::cmp::Ordering::Equal,
      };

      if self.state.order == SortOrder::Descending {
        order.reverse()
      } else {
        order
      }
    });
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
  fn test_context_available_fields() {
    let fields = SortContext::PlaylistTracks.available_fields();
    assert!(fields.contains(&SortField::Name));
    assert!(fields.contains(&SortField::Artist));

    let fields = SortContext::SavedArtists.available_fields();
    assert!(fields.contains(&SortField::Name));
    assert!(!fields.contains(&SortField::Artist));
  }
}
