//! What a sidebar row, a key, or a setting needs before it can do anything:
//! a Spotify session, a capability of the active source, or one particular
//! source. The three terminal tables (sidebar, help menu, settings) carry a
//! [`Requirement`] per row and filter through [`availability`], so the rows
//! a frontend offers and the keys that work cannot disagree.

use crate::core::source::Source;

/// A capability of the active source, one per `Source::supports_*` predicate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Capability {
  Search,
  PlaylistWrite,
  Like,
}

impl Capability {
  pub fn supported_by(self, source: Source) -> bool {
    match self {
      Capability::Search => source.supports_search(),
      Capability::PlaylistWrite => source.supports_playlist_write(),
      Capability::Like => source.supports_like(),
    }
  }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Requirement {
  None,
  /// A Spotify session, whatever the browse scope.
  SpotifySession,
  /// The active source must have the capability; under Spotify a session too.
  Capability(Capability),
  /// The browse scope must be this source; for Spotify, with a session.
  Source(Source),
  /// Met when any one is met; unmet, it reports the first one.
  AnyOf(&'static [Requirement]),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Availability {
  Available,
  NeedsSpotify,
  /// The active source cannot do it.
  NotForSource(Source),
  /// Only this source does it.
  OnlyForSource(Source),
}

impl Availability {
  pub fn is_available(self) -> bool {
    self == Availability::Available
  }

  /// The suffix a row keeps when it stays visible although unmet.
  pub fn hint(self) -> Option<String> {
    match self {
      Availability::Available => None,
      Availability::NeedsSpotify => Some("needs Spotify".to_string()),
      Availability::NotForSource(source) => Some(format!("not for {}", source.label())),
      Availability::OnlyForSource(source) => Some(format!("{} only", source.label())),
    }
  }
}

pub fn availability(
  requirement: Requirement,
  source: Source,
  spotify_connected: bool,
) -> Availability {
  let session = |met: bool| {
    if met {
      Availability::Available
    } else {
      Availability::NeedsSpotify
    }
  };
  match requirement {
    Requirement::None => Availability::Available,
    Requirement::SpotifySession => session(spotify_connected),
    Requirement::Capability(capability) => {
      if !capability.supported_by(source) {
        Availability::NotForSource(source)
      } else if source == Source::Spotify {
        session(spotify_connected)
      } else {
        Availability::Available
      }
    }
    Requirement::Source(required) => {
      if source != required {
        Availability::OnlyForSource(required)
      } else if required == Source::Spotify {
        session(spotify_connected)
      } else {
        Availability::Available
      }
    }
    Requirement::AnyOf(options) => options
      .iter()
      .map(|option| availability(*option, source, spotify_connected))
      .find(|met| met.is_available())
      .or_else(|| {
        options
          .first()
          .map(|first| availability(*first, source, spotify_connected))
      })
      .unwrap_or(Availability::Available),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_requirement_is_always_available() {
    for source in Source::ALL {
      assert!(availability(Requirement::None, source, false).is_available());
    }
  }

  #[test]
  fn a_session_requirement_ignores_the_browse_scope() {
    for source in Source::ALL {
      assert_eq!(
        availability(Requirement::SpotifySession, source, true),
        Availability::Available
      );
      assert_eq!(
        availability(Requirement::SpotifySession, source, false),
        Availability::NeedsSpotify
      );
    }
  }

  #[test]
  fn a_capability_needs_the_source_and_under_spotify_a_session() {
    let like = Requirement::Capability(Capability::Like);
    assert_eq!(
      availability(like, Source::Spotify, true),
      Availability::Available
    );
    assert_eq!(
      availability(like, Source::Spotify, false),
      Availability::NeedsSpotify
    );
    assert_eq!(
      availability(like, Source::Qobuz, true),
      Availability::NotForSource(Source::Qobuz)
    );
    let search = Requirement::Capability(Capability::Search);
    assert_eq!(
      availability(search, Source::Qobuz, false),
      Availability::Available
    );
    assert_eq!(
      availability(search, Source::Local, true),
      Availability::NotForSource(Source::Local)
    );
  }

  #[test]
  fn a_source_requirement_is_the_scope_and_for_spotify_the_session_too() {
    let radio = Requirement::Source(Source::Radio);
    assert_eq!(
      availability(radio, Source::Radio, false),
      Availability::Available
    );
    assert_eq!(
      availability(radio, Source::Spotify, true),
      Availability::OnlyForSource(Source::Radio)
    );
    let spotify = Requirement::Source(Source::Spotify);
    assert_eq!(
      availability(spotify, Source::Spotify, true),
      Availability::Available
    );
    assert_eq!(
      availability(spotify, Source::Spotify, false),
      Availability::NeedsSpotify
    );
    assert_eq!(
      availability(spotify, Source::Local, true),
      Availability::OnlyForSource(Source::Spotify)
    );
  }

  #[test]
  fn any_of_is_met_by_one_option_and_reports_the_first_otherwise() {
    let like_or_radio = Requirement::AnyOf(&[
      Requirement::Capability(Capability::Like),
      Requirement::Source(Source::Radio),
    ]);
    assert_eq!(
      availability(like_or_radio, Source::Radio, false),
      Availability::Available
    );
    assert_eq!(
      availability(like_or_radio, Source::Spotify, true),
      Availability::Available
    );
    assert_eq!(
      availability(like_or_radio, Source::Local, true),
      Availability::NotForSource(Source::Local)
    );
  }

  #[test]
  fn hints_name_the_source_or_spotify() {
    assert_eq!(Availability::Available.hint(), None);
    assert_eq!(
      Availability::NeedsSpotify.hint().as_deref(),
      Some("needs Spotify")
    );
    assert_eq!(
      Availability::NotForSource(Source::Local).hint().as_deref(),
      Some("not for Local Files")
    );
    assert_eq!(
      Availability::OnlyForSource(Source::Radio).hint().as_deref(),
      Some("Internet Radio only")
    );
  }
}
