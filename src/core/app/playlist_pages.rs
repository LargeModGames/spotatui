use super::*;

fn sort_playlist_track_matches(matches: &mut [(FullTrack, usize)], sort_state: SortState) {
  if sort_state.field == SortField::Default {
    return;
  }

  matches.sort_by(|(track_a, position_a), (track_b, position_b)| {
    let order = match sort_state.field {
      SortField::Name => track_a.name.cmp(&track_b.name),
      SortField::Duration => track_a.duration.cmp(&track_b.duration),
      SortField::Artist => {
        let empty_string = String::new();
        let artist_a = track_a
          .artists
          .first()
          .map(|artist| &artist.name)
          .unwrap_or(&empty_string);
        let artist_b = track_b
          .artists
          .first()
          .map(|artist| &artist.name)
          .unwrap_or(&empty_string);
        artist_a.cmp(artist_b)
      }
      SortField::Album => track_a.album.name.cmp(&track_b.album.name),
      SortField::DateAdded => position_a.cmp(position_b),
      SortField::Default => std::cmp::Ordering::Equal,
    };

    if sort_state.order == SortOrder::Descending {
      order.reverse()
    } else {
      order
    }
  });
}

impl App {
  pub fn set_playlist_tracks_to_table_continuous(&mut self) {
    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut track_ids: Vec<String> = Vec::new();
    let mut positions: Vec<usize> = Vec::new();
    let mut expected_offset = 0;
    let mut seen_offsets = HashSet::new();
    let mut active_index = 0;
    let mut active_page = None;

    for (page_index, page) in self.playlist_track_pages.pages.iter().enumerate() {
      if page.offset != expected_offset || !seen_offsets.insert(page.offset) {
        break;
      }

      for (position, item) in page.items.iter() {
        if let PlayableInfo::Track(track) = item {
          if let Some(id) = track.id.as_ref() {
            track_ids.push(id.clone());
          }
          tracks.push(track.clone());
          positions.push(*position as usize);
        }
      }

      expected_offset = expected_offset.saturating_add(page.limit);
      active_index = page_index;
      active_page = Some(page.clone());

      if page.next.is_none() {
        break;
      }
    }

    self.playlist_track_pages.index = active_index;
    self.playlist_tracks = active_page;
    self.playlist_offset = 0;
    self.replace_track_table_tracks(tracks);
    self.playlist_track_positions = Some(positions);
    self.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
  }

  /// Open a playlist's track table: navigate immediately (the cleared table is
  /// the loading state) and fetch page 1. The fetch response used to be what
  /// pushed the route, so a queued or slow fetch made Enter look dead and
  /// trained users to spam it — each press queuing another full fetch. Now
  /// the screen opens on the first press, and a repeat press while the same
  /// open is still in flight only re-ensures navigation instead of dispatching
  /// a duplicate fetch.
  pub fn open_playlist_tracks(
    &mut self,
    playlist_id: PlaylistId<'static>,
    context: TrackTableContext,
  ) {
    let id_str = playlist_id.id().to_string();
    if self.pending_playlist_open.as_deref() == Some(id_str.as_str()) {
      // Same open already in flight: just make sure the screen is shown.
      if self.get_current_route().id != RouteId::TrackTable {
        self.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      }
      return;
    }
    self.pending_playlist_open = Some(id_str.clone());
    self.reset_playlist_tracks_view(playlist_id, context);
    self.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    self.set_status_message("Loading playlist…", 3);
    self.dispatch(IoEvent::GetPlaylistItems(id_str, self.playlist_offset));
  }

  /// Open a decoded source's playlist or folder in the shared track table,
  /// routed by URI scheme. Leaves the Spotify page cache alone.
  pub(crate) fn open_source_playlist_tracks(&mut self, uri: String) {
    let (context, event) = if uri.starts_with("file:") {
      (
        TrackTableContext::LocalPlaylist,
        IoEvent::GetLocalTracks(uri),
      )
    } else if uri.starts_with("subsonic:") {
      (
        TrackTableContext::SubsonicPlaylist,
        IoEvent::GetSubsonicTracks(uri),
      )
    } else if uri.starts_with(YOUTUBE_PLAYLIST_PREFIX) {
      (
        TrackTableContext::YouTubePlaylist,
        IoEvent::GetYouTubeTracks(uri),
      )
    } else {
      // Unknown scheme: silent no-op, like the other opening paths.
      return;
    };
    self.track_table.tracks = Vec::new();
    self.track_table.selected_index = 0;
    self.track_table.context = Some(context);
    self.dispatch(event);
    self.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
  }

  pub fn reset_playlist_tracks_view(
    &mut self,
    playlist_id: PlaylistId<'static>,
    context: TrackTableContext,
  ) {
    self.playlist_tracks_prefetch_generation =
      self.playlist_tracks_prefetch_generation.wrapping_add(1);
    self.playlist_tracks_prefetch_in_flight.clear();
    self.playlist_track_table_id = Some(playlist_id);
    self.active_playlist_track_filter = None;
    self.pending_playlist_track_search = None;
    self.playlist_track_pages.clear();
    self.playlist_tracks = None;
    self.playlist_offset = 0;
    self.pending_track_table_selection = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks.clear();
    self.track_table.context = Some(context);
    self.playlist_track_positions = None;
  }

  pub fn is_playlist_track_filter_active(&self) -> bool {
    self.active_playlist_track_filter.is_some()
  }

  pub fn clear_playlist_track_filter(&mut self) {
    self.active_playlist_track_filter = None;
    self.pending_playlist_track_search = None;
    self.view.input_context = InputContext::GlobalSearch;
    if self.playlist_track_pages.pages.is_empty() {
      self.track_table.tracks.clear();
      self.track_table.selected_index = 0;
      self.playlist_track_positions = None;
      return;
    }
    self.set_playlist_tracks_to_table_continuous();
  }

  pub fn apply_playlist_track_search_results(
    &mut self,
    playlist_id: &PlaylistId<'_>,
    query: String,
    mut matches: Vec<(FullTrack, usize)>,
  ) -> bool {
    if !self.is_playlist_track_table_active_for(playlist_id) {
      return false;
    }

    sort_playlist_track_matches(&mut matches, self.playlist_sort);

    let track_ids = matches
      .iter()
      .filter_map(|(track, _)| track.id.as_ref().map(|id| id.id().to_string()))
      .collect();
    let tracks: Vec<TrackInfo> = matches
      .iter()
      .map(|(track, _)| TrackInfo::from(track))
      .collect();
    let positions: Vec<usize> = matches.into_iter().map(|(_, position)| position).collect();

    self.active_playlist_track_filter = Some(query);
    self.pending_playlist_track_search = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks = tracks;
    self.playlist_track_positions = Some(positions);
    self.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
    true
  }

  pub fn is_playlist_track_table_context(&self) -> bool {
    matches!(
      self.track_table.context,
      Some(TrackTableContext::MyPlaylists) | Some(TrackTableContext::PlaylistSearch)
    )
  }

  pub fn current_playlist_track_table_id(&self) -> Option<PlaylistId<'static>> {
    self
      .is_playlist_track_table_context()
      .then_some(self.playlist_track_table_id.clone())
      .flatten()
  }

  pub fn current_playlist_track_total(&self) -> Option<u32> {
    self.current_playlist_track_table_id()?;
    self
      .playlist_tracks
      .as_ref()
      .map(|playlist_tracks| playlist_tracks.total)
      .or_else(|| {
        self
          .playlist_track_pages
          .pages
          .first()
          .map(|page| page.total)
      })
  }

  pub fn is_playlist_track_table_active_for(&self, playlist_id: &PlaylistId<'_>) -> bool {
    self
      .current_playlist_track_table_id()
      .as_ref()
      .is_some_and(|current_playlist_id| current_playlist_id.id() == playlist_id.id())
  }

  pub fn is_current_route_playlist_track_table_for(&self, playlist_id: &PlaylistId<'_>) -> bool {
    self.get_current_route().id == RouteId::TrackTable
      && self.is_playlist_track_table_active_for(playlist_id)
  }

  pub fn next_missing_playlist_tracks_offset(&self, page_index: usize) -> Option<u32> {
    let playlist_tracks_page = self.playlist_track_pages.get_results(Some(page_index))?;
    playlist_tracks_page.next.as_ref()?;

    let next_offset = playlist_tracks_page.offset + playlist_tracks_page.limit;
    self
      .playlist_track_pages
      .page_index_for_offset(next_offset)
      .is_none()
      .then_some(next_offset)
  }

  pub fn next_missing_playlist_tracks_offset_continuous(&self) -> Option<u32> {
    let playlist_tracks_page = self
      .playlist_track_pages
      .get_results(Some(self.playlist_track_pages.index))?;
    playlist_tracks_page.next.as_ref()?;
    Some(playlist_tracks_page.offset + playlist_tracks_page.limit)
  }

  pub fn current_playlist_has_more_tracks(&self) -> bool {
    if self.is_playlist_track_filter_active() {
      return false;
    }

    self
      .playlist_tracks
      .as_ref()
      .is_some_and(|playlist_tracks| playlist_tracks.next.is_some())
  }

  pub fn selected_playlist_track_position(&self) -> Option<usize> {
    self
      .playlist_track_positions
      .as_ref()
      .and_then(|positions| positions.get(self.track_table.selected_index))
      .copied()
  }

  pub fn get_playlist_tracks_next(&mut self) {
    if self.is_playlist_track_filter_active() {
      return;
    }

    let Some(playlist_id) = self.current_playlist_track_table_id() else {
      return;
    };
    if !self.current_playlist_has_more_tracks() {
      return;
    }

    if let Some(next_offset) = self.next_missing_playlist_tracks_offset_continuous() {
      if self
        .playlist_track_pages
        .page_index_for_offset(next_offset)
        .is_some()
      {
        self.set_playlist_tracks_to_table_continuous();
      } else if !self
        .playlist_tracks_prefetch_in_flight
        .contains(&next_offset)
      {
        self.playlist_tracks_prefetch_in_flight.insert(next_offset);
        self.dispatch(IoEvent::GetPlaylistItems(
          playlist_id.id().to_string(),
          next_offset,
        ));
      }
    }
  }

  pub fn apply_sorted_playlist_tracks_if_current(
    &mut self,
    playlist_id: &PlaylistId<'_>,
    tracks: Vec<FullTrack>,
  ) -> bool {
    if !self.is_playlist_track_table_active_for(playlist_id) {
      return false;
    }

    let tracks = tracks.iter().map(TrackInfo::from).collect();
    self.replace_track_table_tracks(tracks);
    self.track_table.selected_index = 0;
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  fn empty_playlist_page(
    offset: u32,
    total: u32,
    limit: u32,
    has_next: bool,
  ) -> Paged<(u32, PlayableInfo)> {
    Paged {
      items: vec![],
      limit,
      next: has_next.then(|| "https://example.com/playlists/test/items?next".to_string()),
      offset,
      previous: None,
      total,
    }
  }

  fn playlist_page(
    offset: u32,
    total: u32,
    ids: &[&str],
    has_next: bool,
  ) -> Paged<(u32, PlayableInfo)> {
    Paged {
      items: ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
          let position = offset + index as u32;
          let track = PlayableInfo::Track(TrackInfo::from(&full_track(
            id,
            &format!("Track {offset}-{index}"),
          )));
          (position, track)
        })
        .collect(),
      limit: ids.len() as u32,
      next: has_next.then(|| "https://example.com/playlists/test/items?next".to_string()),
      offset,
      previous: None,
      total,
    }
  }

  fn playlist_id(id: &str) -> PlaylistId<'static> {
    PlaylistId::from_id(id).unwrap().into_static()
  }

  #[test]
  fn reset_playlist_tracks_view_clears_cached_pages_and_bumps_generation() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = PlaylistId::from_id("37i9dQZF1DXcBWIGoYBM5M")
      .unwrap()
      .into_static();
    app.playlist_tracks_prefetch_generation = 4;
    app.playlist_track_table_id = Some(playlist_id.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(empty_playlist_page(0, 40, 20, true));
    app.playlist_tracks = Some(empty_playlist_page(0, 40, 20, true));
    app.playlist_offset = 20;
    app.track_table.selected_index = 1;
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];

    app.reset_playlist_tracks_view(playlist_id.clone(), TrackTableContext::MyPlaylists);

    assert_eq!(app.playlist_tracks_prefetch_generation, 5);
    assert_eq!(app.playlist_track_table_id, Some(playlist_id));
    assert!(app.playlist_track_pages.pages.is_empty());
    assert!(app.playlist_tracks.is_none());
    assert_eq!(app.playlist_offset, 0);
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.track_table.selected_index, 0);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::MyPlaylists)
    );
  }

  #[test]
  fn playlist_next_requests_adjacent_offset_when_cache_is_sparse() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");
    let first_page = empty_playlist_page(0, 100, 20, true);
    let last_page = empty_playlist_page(80, 100, 20, false);

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(first_page.clone());
    app.playlist_track_pages.upsert_page_by_offset(last_page);
    app.playlist_tracks = Some(first_page);
    app.playlist_offset = 0;

    app.get_playlist_tracks_next();

    match rx.recv().unwrap() {
      IoEvent::GetPlaylistItems(id, offset) => {
        assert_eq!(id, playlist_id.id());
        assert_eq!(offset, 20);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn playlist_next_uses_cached_adjacent_page_before_fetching() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = empty_playlist_page(0, 60, 20, true);
    let second_page = empty_playlist_page(20, 60, 20, true);

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(first_page.clone());
    app
      .playlist_track_pages
      .upsert_page_by_offset(second_page.clone());
    app.playlist_tracks = Some(first_page);
    app.playlist_offset = 0;

    app.get_playlist_tracks_next();

    assert_eq!(app.playlist_offset, 0);
    assert_eq!(
      app.playlist_tracks.as_ref().map(|page| page.offset),
      Some(20)
    );
    match rx.recv().unwrap() {
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        assert!(track_ids.is_empty());
      }
      _ => panic!("unexpected event"),
    }
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn playlist_continuous_table_stops_at_sparse_cache_gap() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = playlist_page(
      0,
      6,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let sparse_page = playlist_page(
      4,
      6,
      &["0000000000000000000005", "0000000000000000000006"],
      false,
    );

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.playlist_track_pages.upsert_page_by_offset(first_page);
    app.playlist_track_pages.upsert_page_by_offset(sparse_page);

    app.set_playlist_tracks_to_table_continuous();

    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1]));
    match rx.recv().unwrap() {
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        assert_eq!(track_ids.len(), 2);
      }
      _ => panic!("unexpected event"),
    }
  }

  #[test]
  fn playlist_next_cached_page_applies_pending_continuous_index() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let first_page = playlist_page(
      0,
      4,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    );
    let second_page = playlist_page(
      2,
      4,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    );

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app
      .playlist_track_pages
      .upsert_page_by_offset(first_page.clone());
    app.playlist_track_pages.upsert_page_by_offset(second_page);
    app.playlist_tracks = Some(first_page);
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];
    app.track_table.selected_index = 1;
    app.pending_track_table_selection = Some(PendingTrackSelection::Index(2));

    app.get_playlist_tracks_next();

    assert_eq!(app.track_table.tracks.len(), 4);
    assert_eq!(app.track_table.selected_index, 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1, 2, 3]));
  }

  #[test]
  fn playlist_search_results_preserve_source_positions_and_handle_no_matches() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app.pending_playlist_track_search = Some("track".to_string());

    assert!(app.apply_playlist_track_search_results(
      &playlist_id,
      "track".to_string(),
      vec![
        (full_track("0000000000000000000002", "Second"), 8),
        (full_track("0000000000000000000004", "Fourth"), 11),
      ],
    ));

    assert_eq!(app.active_playlist_track_filter, Some("track".to_string()));
    assert!(app.pending_playlist_track_search.is_none());
    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![8, 11]));
    match rx.recv().unwrap() {
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        assert_eq!(track_ids.len(), 2);
      }
      _ => panic!("unexpected event"),
    }

    assert!(app.apply_playlist_track_search_results(&playlist_id, "none".to_string(), vec![]));
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.playlist_track_positions, Some(vec![]));
  }

  #[test]
  fn clearing_playlist_search_restores_cached_continuous_view() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let page = playlist_page(
      0,
      2,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    );

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id);
    app.playlist_track_pages.upsert_page_by_offset(page);
    app.active_playlist_track_filter = Some("second".to_string());
    app.track_table.tracks = vec![TrackInfo::from(&full_track(
      "0000000000000000000002",
      "Second",
    ))];
    app.playlist_track_positions = Some(vec![1]);

    app.clear_playlist_track_filter();

    assert!(app.active_playlist_track_filter.is_none());
    assert_eq!(app.track_table.tracks.len(), 2);
    assert_eq!(app.playlist_track_positions, Some(vec![0, 1]));
  }

  #[test]
  fn apply_sorted_playlist_tracks_if_current_requires_matching_playlist_identity_and_context() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let sidebar_playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");
    let active_playlist_id = playlist_id("37i9dQZF1DX4WYpdgoIcn6");
    let original_track = full_track("0000000000000000000001", "Original");

    app.track_table.tracks = vec![TrackInfo::from(&original_track)];
    app.track_table.context = Some(TrackTableContext::PlaylistSearch);
    app.playlist_track_table_id = Some(active_playlist_id.clone());

    assert!(!app.apply_sorted_playlist_tracks_if_current(
      &sidebar_playlist_id,
      vec![full_track("0000000000000000000002", "Wrong Playlist")],
    ));
    assert_eq!(
      app.track_table.tracks[0].id.as_deref(),
      original_track.id.as_ref().map(|id| id.id())
    );

    app.track_table.context = Some(TrackTableContext::SavedTracks);
    assert!(!app.apply_sorted_playlist_tracks_if_current(
      &active_playlist_id,
      vec![full_track("0000000000000000000003", "Wrong Context")],
    ));
    assert_eq!(
      app.track_table.tracks[0].id.as_deref(),
      original_track.id.as_ref().map(|id| id.id())
    );
  }

  #[test]
  fn current_route_playlist_track_table_requires_track_table_route() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let playlist_id = playlist_id("37i9dQZF1DXcBWIGoYBM5M");

    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone());
    app.push_navigation_stack(RouteId::Search, ActiveBlock::SearchResultBlock);

    assert!(app.is_playlist_track_table_active_for(&playlist_id));
    assert!(!app.is_current_route_playlist_track_table_for(&playlist_id));

    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    assert!(app.is_current_route_playlist_track_table_for(&playlist_id));
  }
}
