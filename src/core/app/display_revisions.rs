use super::*;

#[derive(Clone, Copy)]
pub enum DisplayDomain {
  Route,
  Status,
  Source,
  Theme,
  Playback,
  Party,
  Devices,
  Search,
  Lyrics,
  Artist,
  Library,
}

/// Per-domain revisions that move only when that domain's displayed state changed.
#[derive(Default, Clone, Copy)]
pub struct DisplayRevisions {
  route: u64,
  status: u64,
  source: u64,
  theme: u64,
  playback: u64,
  party: u64,
  devices: u64,
  search: u64,
  lyrics: u64,
  artist: u64,
  library: u64,
}

impl DisplayRevisions {
  pub(super) fn bump(&mut self, domain: DisplayDomain) {
    let slot = match domain {
      DisplayDomain::Route => &mut self.route,
      DisplayDomain::Status => &mut self.status,
      DisplayDomain::Source => &mut self.source,
      DisplayDomain::Theme => &mut self.theme,
      DisplayDomain::Playback => &mut self.playback,
      DisplayDomain::Party => &mut self.party,
      DisplayDomain::Devices => &mut self.devices,
      DisplayDomain::Search => &mut self.search,
      DisplayDomain::Lyrics => &mut self.lyrics,
      DisplayDomain::Artist => &mut self.artist,
      DisplayDomain::Library => &mut self.library,
    };
    *slot = slot.wrapping_add(1);
  }

  #[cfg(any(test, feature = "gui"))]
  pub fn get(&self, domain: DisplayDomain) -> u64 {
    match domain {
      DisplayDomain::Route => self.route,
      DisplayDomain::Status => self.status,
      DisplayDomain::Source => self.source,
      DisplayDomain::Theme => self.theme,
      DisplayDomain::Playback => self.playback,
      DisplayDomain::Party => self.party,
      DisplayDomain::Devices => self.devices,
      DisplayDomain::Search => self.search,
      DisplayDomain::Lyrics => self.lyrics,
      DisplayDomain::Artist => self.artist,
      DisplayDomain::Library => self.library,
    }
  }
}

impl App {
  /// Bump a display revision from a producer whose fields are not behind a setter yet.
  pub(crate) fn bump_display(&mut self, domain: DisplayDomain) {
    self.display_revisions.bump(domain);
  }

  /// A copy of every display revision, for a frontend to diff against.
  #[cfg(any(test, feature = "gui"))]
  pub fn display_revisions(&self) -> DisplayRevisions {
    self.display_revisions
  }
}
