use super::*;

/// Minimal source-agnostic snapshot of the signed-in user, used for playlist
/// ownership checks and market/country resolution. Holds only string fields so
/// no `rspotify::model` type leaks into [`App`] state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UserInfo {
  pub id: String,
  pub display_name: Option<String>,
  /// ISO 3166-1 alpha-2 country code (e.g. `"US"`), when known.
  pub country: Option<String>,
}

/// State backing the monthly recap popup.
#[derive(Clone, Debug)]
pub struct RecapPromptState {
  pub path: PathBuf,
  pub listens: usize,
}

// Is it possible to compose enums?
#[derive(PartialEq, Debug)]
pub enum TrackTableContext {
  MyPlaylists,
  AlbumSearch,
  PlaylistSearch,
  SavedTracks,
  RecommendedTracks,
  DiscoverPlaylist,
  LocalPlaylist,
  SubsonicPlaylist,
  YouTubePlaylist,
  QobuzPlaylist,
}

/// The five search result pages. Their cursors and focus are `App.view`'s
/// `search_*` fields.
#[derive(Default)]
pub struct SearchResult {
  pub albums: Option<crate::core::pagination::Paged<crate::core::plugin_api::AlbumInfo>>,
  pub artists: Option<crate::core::pagination::Paged<crate::core::plugin_api::ArtistInfo>>,
  pub playlists: Option<crate::core::pagination::Paged<crate::core::plugin_api::PlaylistInfo>>,
  pub tracks: Option<crate::core::pagination::Paged<crate::core::plugin_api::TrackInfo>>,
  pub shows: Option<crate::core::pagination::Paged<crate::core::plugin_api::ShowInfo>>,
}

/// The shared track table's rows and what they are. Its cursor is
/// `App.view.track_table_index`, its scroll offset `view.track_table_scroll_offset`.
#[derive(Default)]
pub struct TrackTable {
  pub tracks: Vec<TrackInfo>,
  pub context: Option<TrackTableContext>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PendingTrackSelection {
  Index(usize),
}

impl App {
  pub fn replace_track_table_tracks(&mut self, tracks: Vec<TrackInfo>) {
    self.playlist_track_positions = None;

    let track_count = tracks.len();
    self.track_table.tracks = tracks;
    if track_count > 0 {
      if let Some(pending) = self.pending_track_table_selection.take() {
        self.view.track_table_index = match pending {
          PendingTrackSelection::Index(index) => index.min(track_count.saturating_sub(1)),
        };
      } else {
        self.clamp_track_table_cursor();
      }
    } else {
      self.view.track_table_index = 0;
    }
  }

  /// Show `tracks` in the shared table with the cursor on the top row.
  pub(crate) fn set_track_table(&mut self, tracks: Vec<TrackInfo>, context: TrackTableContext) {
    self.track_table.tracks = tracks;
    self.track_table.context = Some(context);
    self.view.track_table_index = 0;
  }

  /// Keep the track table cursor inside the rows the table holds now.
  pub(crate) fn clamp_track_table_cursor(&mut self) {
    let max_index = self.track_table.tracks.len().saturating_sub(1);
    if self.view.track_table_index > max_index {
      self.view.track_table_index = max_index;
    }
  }

  /// Keep every search cursor inside its page after the pages were replaced:
  /// a shorter page must never leave a cursor past its end.
  pub(crate) fn clamp_search_cursors(&mut self) {
    let pages = &self.search_results;
    clamp_cursor(
      &mut self.view.search_selected_tracks_index,
      page_len(&pages.tracks),
    );
    clamp_cursor(
      &mut self.view.search_selected_artists_index,
      page_len(&pages.artists),
    );
    clamp_cursor(
      &mut self.view.search_selected_album_index,
      page_len(&pages.albums),
    );
    clamp_cursor(
      &mut self.view.search_selected_playlists_index,
      page_len(&pages.playlists),
    );
    clamp_cursor(
      &mut self.view.search_selected_shows_index,
      page_len(&pages.shows),
    );
  }

  /// Test seed: a `view` write in a test outside `tui/` is counted by `view_writes_outside_tui`.
  #[cfg(test)]
  pub(crate) fn select_track_row(&mut self, index: usize) {
    self.view.track_table_index = index;
  }

  /// Park a row index for the page a `LoadMore` is about to fetch; the next
  /// `replace_track_table_tracks` moves the cursor there.
  pub fn select_row_when_next_page_lands(&mut self, index: usize) {
    self.pending_track_table_selection = Some(PendingTrackSelection::Index(index));
  }

  #[cfg(test)]
  pub(crate) fn pending_track_table_selection(&self) -> Option<PendingTrackSelection> {
    self.pending_track_table_selection
  }
}

fn page_len<T>(page: &Option<Paged<T>>) -> usize {
  page.as_ref().map(|p| p.items.len()).unwrap_or(0)
}

fn clamp_cursor(index: &mut Option<usize>, len: usize) {
  *index = match (*index, len) {
    (_, 0) => None,
    (Some(i), len) => Some(i.min(len - 1)),
    (None, _) => None,
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  fn page_of(len: usize) -> Option<Paged<u32>> {
    Some(Paged {
      items: (0..len as u32).collect(),
      total: len as u32,
      ..Default::default()
    })
  }

  /// Replacing a long page (e.g. 21 playlists, selected index 20) with a
  /// shorter one (e.g. 2 playlists) must clamp the selected index into range
  /// instead of leaving it pointing past the end — this is the root cause of
  /// panic-1 (unchecked `.items[selected_index]` indexing downstream).
  #[test]
  fn clamp_selects_last_valid_index_when_new_page_is_shorter() {
    let mut index = Some(20);
    let new_page = page_of(2);
    clamp_cursor(&mut index, page_len(&new_page));
    assert_eq!(index, Some(1));
  }

  #[test]
  fn clamp_resets_to_none_when_new_page_is_empty() {
    let mut index = Some(20);
    let empty_page: Option<Paged<u32>> = page_of(0);
    clamp_cursor(&mut index, page_len(&empty_page));
    assert_eq!(index, None);

    let mut index = Some(0);
    let none_page: Option<Paged<u32>> = None;
    clamp_cursor(&mut index, page_len(&none_page));
    assert_eq!(index, None);
  }

  #[test]
  fn clamp_leaves_in_range_index_untouched() {
    let mut index = Some(1);
    let new_page = page_of(5);
    clamp_cursor(&mut index, page_len(&new_page));
    assert_eq!(index, Some(1));
  }

  #[test]
  fn clamp_leaves_none_as_none_when_page_is_nonempty() {
    let mut index = None;
    let new_page = page_of(5);
    clamp_cursor(&mut index, page_len(&new_page));
    assert_eq!(index, None);
  }
}
