use super::*;

/// Time range for Top Tracks/Artists in Discover feature
#[derive(Clone, PartialEq, Debug, Copy, Default)]
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
}
