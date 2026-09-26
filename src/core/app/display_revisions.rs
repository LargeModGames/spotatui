use super::*;

#[derive(Clone, Copy)]
pub enum DisplayDomain {
  Route,
  Status,
  Source,
  Theme,
  Playback,
}

/// Per-domain revisions that move only when that domain's displayed state changed.
#[derive(Default, Clone, Copy)]
pub struct DisplayRevisions {
  route: u64,
  status: u64,
  source: u64,
  theme: u64,
  playback: u64,
}

impl DisplayRevisions {
  pub(super) fn bump(&mut self, domain: DisplayDomain) {
    let slot = match domain {
      DisplayDomain::Route => &mut self.route,
      DisplayDomain::Status => &mut self.status,
      DisplayDomain::Source => &mut self.source,
      DisplayDomain::Theme => &mut self.theme,
      DisplayDomain::Playback => &mut self.playback,
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
    }
  }
}

impl App {
  /// A copy of every display revision, for a frontend to diff against.
  #[cfg(any(test, feature = "gui"))]
  pub fn display_revisions(&self) -> DisplayRevisions {
    self.display_revisions
  }
}
