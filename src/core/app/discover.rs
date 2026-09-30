use super::*;

/// Time range for Top Tracks/Artists in Discover feature
#[derive(Clone, PartialEq, Eq, Debug, Copy, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(all(test, feature = "gui"), derive(ts_rs::TS))]
pub enum DiscoverTimeRange {
  /// Last 4 weeks
  Short,
  /// Last 6 months (default)
  #[default]
  Medium,
  /// All time
  Long,
}

impl DiscoverTimeRange {
  pub fn label(&self) -> &'static str {
    match self {
      DiscoverTimeRange::Short => "4 weeks",
      DiscoverTimeRange::Medium => "6 months",
      DiscoverTimeRange::Long => "All time",
    }
  }

  pub fn next(&self) -> Self {
    match self {
      DiscoverTimeRange::Short => DiscoverTimeRange::Medium,
      DiscoverTimeRange::Medium => DiscoverTimeRange::Long,
      DiscoverTimeRange::Long => DiscoverTimeRange::Short,
    }
  }

  pub fn prev(&self) -> Self {
    match self {
      DiscoverTimeRange::Short => DiscoverTimeRange::Long,
      DiscoverTimeRange::Medium => DiscoverTimeRange::Short,
      DiscoverTimeRange::Long => DiscoverTimeRange::Medium,
    }
  }
}

/// What a frontend shows for Discover; compared on every display pass, so every writer is caught.
#[derive(Clone, Default, PartialEq)]
pub struct DiscoverView {
  /// The range the cached top tracks belong to.
  pub range: Option<DiscoverTimeRange>,
  pub loading: bool,
  pub available: bool,
  pub mix_available: bool,
  pub top_tracks: Vec<TrackInfo>,
  pub artists_mix: Vec<TrackInfo>,
  /// The ids among both lists that are in Liked Songs.
  pub liked_ids: Vec<String>,
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
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetRecommendationsForSeed(
      seed_artists,
      seed_tracks,
      Box::new(first_track),
      user_country,
    ));
  }

  pub fn get_recommendations_for_track_id(&mut self, id: String) {
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

  /// Show the cached mix, or fetch it; a no-op while a fetch is in flight.
  pub(crate) fn open_discover_mix(&mut self, target: crate::core::action::DiscoverTarget) {
    use crate::core::action::DiscoverTarget;
    if self.discover_loading {
      return;
    }
    let (cached, event, hit) = match target {
      DiscoverTarget::ArtistsMix => (&self.discover_artists_mix, IoEvent::GetTopArtistsMix, true),
      DiscoverTarget::TopTracks(range) => (
        &self.discover_top_tracks,
        IoEvent::GetUserTopTracks(range),
        // The terminal empties the cache when its range changes; another frontend asks by range.
        self
          .discover_top_tracks_range
          .is_none_or(|cached| cached == range),
      ),
    };
    if cached.is_empty() || !hit {
      self.dispatch(event);
    } else {
      let tracks = cached.clone();
      self.show_tracks_in_table(tracks, TrackTableContext::DiscoverPlaylist);
    }
  }

  pub(crate) fn set_discover_top_tracks(
    &mut self,
    range: DiscoverTimeRange,
    tracks: Vec<TrackInfo>,
  ) {
    self.discover_top_tracks = tracks;
    self.discover_top_tracks_range = Some(range);
  }

  /// Bump Discover when anything its view reads changed.
  pub(super) fn note_discover_changes(&mut self) {
    let liked_ids: Vec<String> = self
      .discover_top_tracks
      .iter()
      .chain(&self.discover_artists_mix)
      .filter_map(|track| track.id.clone())
      .filter(|id| self.liked_song_ids_set.contains(id))
      .collect();
    let mix_available = !self
      .spotify_endpoint_blocked(crate::core::spotify_access::RestrictedEndpoint::ArtistTopTracks);
    let view = &self.discover_view;
    if view.range == self.discover_top_tracks_range
      && view.loading == self.discover_loading
      && view.available == self.spotify_connected
      && view.mix_available == mix_available
      && view.liked_ids == liked_ids
      && view.top_tracks == self.discover_top_tracks
      && view.artists_mix == self.discover_artists_mix
    {
      return;
    }
    self.discover_view = DiscoverView {
      range: self.discover_top_tracks_range,
      loading: self.discover_loading,
      available: self.spotify_connected,
      mix_available,
      top_tracks: self.discover_top_tracks.clone(),
      artists_mix: self.discover_artists_mix.clone(),
      liked_ids,
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
