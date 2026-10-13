use super::*;

/// One shelf of Spotify's Made For You page, such as "Your Daily Mixes".
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct DiscoverShelf {
  pub title: String,
  pub playlists: Vec<DiscoverPlaylist>,
}

/// A playlist Spotify made for the user; most are not in the library.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub struct DiscoverPlaylist {
  pub uri: String,
  pub name: String,
  pub description: Option<String>,
  pub image_url: Option<String>,
}

/// What a frontend shows for Discover; compared on every display pass, so every writer is caught.
#[derive(Clone, Default, PartialEq)]
pub struct DiscoverView {
  pub loading: bool,
  pub available: bool,
  /// `None` until the first fetch lands.
  pub shelves: Option<Vec<DiscoverShelf>>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RecommendationsContext {
  Artist,
  Song,
}

impl App {
  pub fn get_recommendations_for_seed(
    &mut self,
    seed_artists: Option<Vec<String>>,
    seed_tracks: Option<Vec<String>>,
    first_track: Option<TrackInfo>,
  ) {
    self.begin_radio_request();
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetRecommendationsForSeed(
      seed_artists,
      seed_tracks,
      Box::new(first_track),
      user_country,
    ));
  }

  pub fn get_recommendations_for_track_id(&mut self, id: String) {
    self.begin_radio_request();
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetRecommendationsForTrackId(id, user_country));
  }

  /// Seed the track-radio recommendations flow from one track: set the Song
  /// context, record the seed label, and fetch recommendations for it.
  ///
  /// NOTE: preserves a pre-existing bug. The historic handler fed the
  /// track's full URI ("spotify:track:...") as the seed, which
  /// `TrackId::from_id` rejects, so the whole seed_tracks list collapses to
  /// `None` and the recommendation request goes out unseeded.
  /// `TrackInfo::uri` reproduces that exact URI string; switching to
  /// `track.id` (base62) would change behavior. Fix the seeding separately
  /// with its own verification.
  pub fn load_recommendations_for_track(&mut self, track: TrackInfo) {
    let seed_tracks = track.uri.clone().map(|uri| vec![uri]);
    self.recommendations_context = Some(RecommendationsContext::Song);
    self.recommendations_seed = track.name.clone();
    self.get_recommendations_for_seed(None, seed_tracks, Some(track));
  }

  pub fn load_recommendations_for_artist(&mut self, id: String, name: String) {
    self.recommendations_context = Some(RecommendationsContext::Artist);
    self.recommendations_seed = name;
    self.get_recommendations_for_seed(Some(vec![id]), None, None);
  }

  /// Unlike [`Self::load_recommendations_for_track`], no context row is
  /// prepended to the results table.
  pub fn load_recommendations_for_track_id(&mut self, id: String, name: String) {
    self.recommendations_context = Some(RecommendationsContext::Song);
    self.recommendations_seed = name;
    self.get_recommendations_for_track_id(id);
  }

  /// Fetch the Made For You page; the shelves already shown stay until it lands.
  pub(crate) fn fetch_discover(&mut self) {
    if !self.discover_loading {
      self.discover_loading = true;
      self.dispatch(IoEvent::GetMadeForYou);
    }
  }

  /// End a fetch, or a dropped one; `None` keeps the shelves of the last one.
  pub(crate) fn end_discover_fetch(&mut self, shelves: Option<Vec<DiscoverShelf>>) {
    self.discover_loading = false;
    if let Some(shelves) = shelves {
      let count = shelves
        .iter()
        .map(|shelf| shelf.playlists.len())
        .sum::<usize>();
      self.view.discover_selected_index = self
        .view
        .discover_selected_index
        .min(count.saturating_sub(1));
      self.discover_shelves = Some(shelves);
    }
  }

  pub fn discover_loading(&self) -> bool {
    self.discover_loading
  }

  pub fn discover_shelves(&self) -> &[DiscoverShelf] {
    self.discover_shelves.as_deref().unwrap_or_default()
  }

  /// The playlist at `index` when the shelves are read as one list.
  pub fn discover_playlist(&self, index: usize) -> Option<&DiscoverPlaylist> {
    self
      .discover_shelves()
      .iter()
      .flat_map(|shelf| &shelf.playlists)
      .nth(index)
  }

  /// Bump Discover when anything its view reads changed.
  pub(super) fn note_discover_changes(&mut self) {
    let view = &self.discover_view;
    if view.loading == self.discover_loading
      && view.available == self.spotify_connected
      && view.shelves == self.discover_shelves
    {
      return;
    }
    self.discover_view = DiscoverView {
      loading: self.discover_loading,
      available: self.spotify_connected,
      shelves: self.discover_shelves.clone(),
    };
    self.display_revisions.bump(DisplayDomain::Discover);
  }

  /// The Discover view the Discover revision counted.
  #[cfg(feature = "gui")]
  pub(crate) fn discover_view(&self) -> &DiscoverView {
    &self.discover_view
  }

  /// Open the shared track table on `tracks` with the cursor on the top row.
  pub(super) fn show_tracks_in_table(
    &mut self,
    tracks: Vec<TrackInfo>,
    context: TrackTableContext,
  ) {
    self.set_track_table(tracks, context);
    self.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::test_helpers::discover_shelves;

  #[test]
  fn a_landed_page_clamps_the_cursor_to_its_playlists() {
    let mut app = App::default();
    app.view.discover_selected_index = 9;

    app.end_discover_fetch(Some(discover_shelves()));

    assert_eq!(app.view.discover_selected_index, 2);
    assert_eq!(
      app
        .discover_playlist(2)
        .map(|playlist| playlist.name.as_str()),
      Some("Release Radar")
    );
  }
}
