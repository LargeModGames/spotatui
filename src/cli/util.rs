use clap::ArgMatches;
use rspotify::{
  model::enums::RepeatState,
  model::idtypes::Id,
  model::{artist::SimplifiedArtist, show::FullEpisode, track::FullTrack},
};
use std::time::Duration;

use crate::core::user_config::UserConfig;

// Helper function to extract URI from typed IDs or external URLs
fn get_uri_or_fallback<T: Id>(
  id: &Option<T>,
  external_urls: &std::collections::HashMap<String, String>,
) -> String {
  if let Some(id) = id {
    id.uri()
  } else {
    external_urls
      .get("spotify")
      .cloned()
      .unwrap_or_else(|| "N/A".to_string())
  }
}

// Possible types to list or search
#[derive(Debug)]
pub enum Type {
  Playlist,
  Track,
  Artist,
  Album,
  Show,
  Device,
  Liked,
}

impl Type {
  pub fn play_from_matches(m: &ArgMatches) -> Self {
    if m.get_flag("playlist") {
      Self::Playlist
    } else if m.get_flag("track") {
      Self::Track
    } else if m.get_flag("artist") {
      Self::Artist
    } else if m.get_flag("album") {
      Self::Album
    } else if m.get_flag("show") {
      Self::Show
    }
    // Enforced by clap
    else {
      unreachable!()
    }
  }

  pub fn search_from_matches(m: &ArgMatches) -> Self {
    if m.get_flag("playlists") {
      Self::Playlist
    } else if m.get_flag("tracks") {
      Self::Track
    } else if m.get_flag("artists") {
      Self::Artist
    } else if m.get_flag("albums") {
      Self::Album
    } else if m.get_flag("shows") {
      Self::Show
    }
    // Enforced by clap
    else {
      unreachable!()
    }
  }

  pub fn list_from_matches(m: &ArgMatches) -> Self {
    if m.get_flag("playlists") {
      Self::Playlist
    } else if m.get_flag("devices") {
      Self::Device
    } else if m.get_flag("liked") {
      Self::Liked
    }
    // Enforced by clap
    else {
      unreachable!()
    }
  }
}

//
// Possible flags to set
//

pub enum Flag {
  // Does not get toggled
  // * User chooses like -> Flag::Like(true)
  // * User chooses dislike -> Flag::Like(false)
  Like(bool),
  Shuffle,
  Repeat,
}

impl Flag {
  pub fn from_matches(m: &ArgMatches) -> Vec<Self> {
    // Multiple flags are possible
    let mut flags = Vec::new();

    // Only one of these two
    if m.get_flag("like") {
      flags.push(Self::Like(true));
    } else if m.get_flag("dislike") {
      flags.push(Self::Like(false));
    }

    if m.get_flag("shuffle") {
      flags.push(Self::Shuffle);
    }
    if m.get_flag("repeat") {
      flags.push(Self::Repeat);
    }
    flags
  }
}

// Possible directions to jump to
pub enum JumpDirection {
  Next,
  Previous,
}

impl JumpDirection {
  pub fn from_matches(m: &ArgMatches) -> (Self, u64) {
    let next_count = m.get_count("next");
    let prev_count = m.get_count("previous");
    if next_count > 0 {
      (Self::Next, next_count as u64)
    } else if prev_count > 0 {
      (Self::Previous, prev_count as u64)
    // Enforced by clap
    } else {
      unreachable!()
    }
  }
}

// For fomatting (-f / --format flag)

// Types to create a Format enum from
// Boxing was proposed by cargo clippy
// to reduce the size of this enum
pub enum FormatType {
  /// Domain playlist (the playlist CLI paths — library list and search — are
  /// fully migrated off rspotify).
  PlaylistInfo(Box<crate::core::plugin_api::PlaylistInfo>),
  Track(Box<FullTrack>),
  /// Domain track (used by sources already migrated off rspotify, e.g. the
  /// track table). The rspotify `Track` variant remains for not-yet-migrated
  /// CLI paths (current-playback item, search results).
  TrackInfo(Box<crate::core::plugin_api::TrackInfo>),
  Episode(Box<FullEpisode>),
  /// Domain variants — used by search results and other migrated paths.
  AlbumInfo(Box<crate::core::plugin_api::AlbumInfo>),
  ArtistInfo(Box<crate::core::plugin_api::ArtistInfo>),
  ShowInfo(Box<crate::core::plugin_api::ShowInfo>),
}

// Types that can be formatted
#[derive(Clone)]
pub enum Format {
  Album(String),
  Artist(String),
  Playlist(String),
  Track(String),
  Show(String),
  Uri(String),
  Device(String),
  Volume(u32),
  // Current position, duration
  Position((u32, u32)),
  // This is a bit long, should it be splitted up?
  Flags((RepeatState, bool, bool)),
  Playing(bool),
}

pub fn join_artists(a: Vec<SimplifiedArtist>) -> String {
  a.iter()
    .map(|l| l.name.clone())
    .collect::<Vec<String>>()
    .join(", ")
}

impl Format {
  // Extract important information from types
  pub fn from_type(t: FormatType) -> Vec<Self> {
    match t {
      FormatType::PlaylistInfo(p) => {
        vec![Self::Playlist(p.name), Self::Uri(p.uri)]
      }
      FormatType::Track(t) => {
        let joined_artists = join_artists(t.artists.clone());
        let uri = get_uri_or_fallback(&t.id, &t.external_urls);
        vec![
          Self::Album(t.album.name),
          Self::Artist(joined_artists),
          Self::Track(t.name),
          Self::Uri(uri),
        ]
      }
      FormatType::TrackInfo(t) => {
        // Domain track: artists is already a Vec<String>, album/uri are plain.
        vec![
          Self::Album(t.album),
          Self::Artist(t.artists.join(", ")),
          Self::Track(t.name),
          Self::Uri(t.uri.unwrap_or_default()),
        ]
      }
      FormatType::Episode(e) => {
        let uri = e.id.uri();
        #[allow(deprecated)]
        let publisher = e.show.publisher.clone();
        vec![
          Self::Show(e.show.name),
          Self::Artist(publisher),
          Self::Track(e.name),
          Self::Uri(uri),
        ]
      }
      FormatType::AlbumInfo(a) => {
        let joined_artists = a
          .artists
          .iter()
          .map(|r| r.name.clone())
          .collect::<Vec<_>>()
          .join(", ");
        let uri = a.uri.clone().unwrap_or_default();
        vec![
          Self::Album(a.name),
          Self::Artist(joined_artists),
          Self::Uri(uri),
        ]
      }
      FormatType::ArtistInfo(a) => {
        let uri = a.uri.clone().unwrap_or_default();
        vec![Self::Artist(a.name), Self::Uri(uri)]
      }
      FormatType::ShowInfo(s) => {
        let uri = s.uri.clone().unwrap_or_default();
        // Spotify no longer sends `publisher` to Development Mode apps; an
        // empty one must render as `None` (no %a value), not a blank artist.
        let mut values = vec![Self::Show(s.name), Self::Uri(uri)];
        if !s.publisher.is_empty() {
          values.insert(1, Self::Artist(s.publisher));
        }
        values
      }
    }
  }

  // Is there a better way?
  pub fn inner(&self, conf: UserConfig) -> String {
    match self {
      Self::Album(s) => s.clone(),
      Self::Artist(s) => s.clone(),
      Self::Playlist(s) => s.clone(),
      Self::Track(s) => s.clone(),
      Self::Show(s) => s.clone(),
      Self::Uri(s) => s.clone(),
      Self::Device(s) => s.clone(),
      // Because this match statements
      // needs to return a &String, I have to do it this way
      Self::Volume(s) => s.to_string(),
      Self::Position((curr, duration)) => {
        let current_progress_ms = *curr as u128;
        let duration = Duration::from_millis(*duration as u64);
        crate::core::format::display_track_progress(current_progress_ms, duration)
      }
      Self::Flags((r, s, l)) => {
        let like = if *l {
          conf.behavior.liked_icon
        } else {
          String::new()
        };
        let shuffle = if *s {
          conf.behavior.shuffle_icon
        } else {
          String::new()
        };
        let repeat = match r {
          RepeatState::Off => String::new(),
          RepeatState::Track => conf.behavior.repeat_track_icon,
          RepeatState::Context => conf.behavior.repeat_context_icon,
        };

        // Add them together (only those that aren't empty)
        [shuffle, repeat, like]
          .iter()
          .filter(|a| !a.is_empty())
          // Convert &String to String to join them
          .map(|s| s.to_string())
          .collect::<Vec<String>>()
          .join(" ")
      }
      Self::Playing(s) => {
        if *s {
          conf.behavior.playing_icon
        } else {
          conf.behavior.paused_icon
        }
      }
    }
  }

  pub fn get_placeholder(&self) -> &str {
    match self {
      Self::Album(_) => "%b",
      Self::Artist(_) => "%a",
      Self::Playlist(_) => "%p",
      Self::Track(_) => "%t",
      Self::Show(_) => "%h",
      Self::Uri(_) => "%u",
      Self::Device(_) => "%d",
      Self::Volume(_) => "%v",
      Self::Position(_) => "%r",
      Self::Flags(_) => "%f",
      Self::Playing(_) => "%s",
    }
  }
}

/// Render a format string in a single pass.
///
/// On `%` followed by one of `a b t p h u d v f s r`, emits the value of the
/// first matching placeholder (or `"None"` when no value carries it). Every
/// other character — including a lone `%` or an unknown specifier — is copied
/// verbatim, so playlist names like `100%hits` and defaults like `"%v% %d"`
/// survive.
pub fn render_format(format: &str, values: &[Format], conf: &UserConfig) -> String {
  let mut out = String::new();
  let mut chars = format.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '%' && chars.peek().is_some_and(|n| "abtphudvfsr".contains(*n)) {
      let specifier = *chars.peek().unwrap();
      let placeholder = format!("%{}", specifier);
      let rendered = values
        .iter()
        .find(|v| v.get_placeholder() == placeholder)
        .map(|v| v.inner(conf.clone()))
        .unwrap_or_else(|| "None".to_string());
      out.push_str(&rendered);
      chars.next();
    } else {
      out.push(c);
    }
  }
  out.trim().to_string()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::ShowInfo;

  #[test]
  fn a_playlist_name_containing_a_placeholder_is_printed_verbatim() {
    let values = vec![
      Format::Playlist("100%hits".into()),
      Format::Uri("spotify:playlist:x".into()),
    ];
    assert_eq!(
      render_format("%p (%u)", &values, &UserConfig::new()),
      "100%hits (spotify:playlist:x)"
    );
  }

  #[test]
  fn a_missing_progress_placeholder_prints_none() {
    let values = vec![Format::Track("Song".into())];
    assert_eq!(
      render_format("%t %r", &values, &UserConfig::new()),
      "Song None"
    );
  }

  #[test]
  fn a_percent_sign_after_the_volume_is_kept() {
    let values = vec![Format::Device("Kitchen".into()), Format::Volume(42)];
    assert_eq!(
      render_format("%v% %d", &values, &UserConfig::new()),
      "42% Kitchen"
    );
  }

  #[test]
  fn unsupported_placeholders_print_none() {
    let values = vec![Format::Track("T".into())];
    assert_eq!(
      render_format("%t - %a", &values, &UserConfig::new()),
      "T - None"
    );
  }

  #[test]
  fn a_show_search_result_lists_its_publisher_as_the_artist() {
    let values = Format::from_type(FormatType::ShowInfo(Box::new(ShowInfo {
      name: "Pod".into(),
      publisher: "Acme".into(),
      uri: Some("spotify:show:1".into()),
      ..Default::default()
    })));
    assert_eq!(
      render_format("%h - %a (%u)", &values, &UserConfig::new()),
      "Pod - Acme (spotify:show:1)"
    );
  }

  #[test]
  fn a_show_without_a_publisher_prints_none_for_the_artist() {
    let values = Format::from_type(FormatType::ShowInfo(Box::new(ShowInfo {
      name: "Pod".into(),
      publisher: String::new(),
      uri: Some("spotify:show:1".into()),
      ..Default::default()
    })));
    assert_eq!(
      render_format("%h - %a (%u)", &values, &UserConfig::new()),
      "Pod - None (spotify:show:1)"
    );
  }
}
