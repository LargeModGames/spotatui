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

#[derive(Default)]
pub struct SpotifyResultAndSelectedIndex<T> {
  pub index: usize,
  pub result: T,
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
}

pub struct SearchResult {
  pub albums: Option<crate::core::pagination::Paged<crate::core::plugin_api::AlbumInfo>>,
  pub artists: Option<crate::core::pagination::Paged<crate::core::plugin_api::ArtistInfo>>,
  pub playlists: Option<crate::core::pagination::Paged<crate::core::plugin_api::PlaylistInfo>>,
  pub tracks: Option<crate::core::pagination::Paged<crate::core::plugin_api::TrackInfo>>,
  pub shows: Option<crate::core::pagination::Paged<crate::core::plugin_api::ShowInfo>>,
  pub selected_album_index: Option<usize>,
  pub selected_artists_index: Option<usize>,
  pub selected_playlists_index: Option<usize>,
  pub selected_tracks_index: Option<usize>,
  pub selected_shows_index: Option<usize>,
  pub hovered_block: SearchResultBlock,
  pub selected_block: SearchResultBlock,
}

#[derive(Default)]
pub struct TrackTable {
  pub tracks: Vec<TrackInfo>,
  pub selected_index: usize,
  pub context: Option<TrackTableContext>,
  /// First row shown in the table. Persisted across frames so the cursor can
  /// move within the visible window without dragging the view (the view only
  /// scrolls when the cursor reaches an edge). Updated during draw, hence Cell.
  pub scroll_offset: std::cell::Cell<usize>,
}

/// Spectrum data for local audio visualization
#[derive(Clone, Default)]
pub struct SpectrumData {
  pub bands: [f32; 12],
  pub peak: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PendingTrackSelection {
  Index(usize),
}

impl App {
  pub fn replace_track_table_tracks(&mut self, tracks: Vec<TrackInfo>) {
    self.playlist_track_positions = None;

    let track_count = tracks.len();
    if track_count > 0 {
      if let Some(pending) = self.pending_track_table_selection.take() {
        self.track_table.selected_index = match pending {
          PendingTrackSelection::Index(index) => index.min(track_count.saturating_sub(1)),
        };
      } else {
        let max_index = track_count.saturating_sub(1);
        if self.track_table.selected_index > max_index {
          self.track_table.selected_index = max_index;
        }
      }
    } else {
      self.track_table.selected_index = 0;
    }

    self.track_table.tracks = tracks;
  }
}
