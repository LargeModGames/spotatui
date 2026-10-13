use super::*;

/// Every sidebar row with what it needs; `App::library_rows` is the filtered list.
pub(crate) fn library_row_requirements() -> &'static [(LibraryTarget, Requirement)] {
  &[
    (
      LibraryTarget::Discover,
      Requirement::Source(Source::Spotify),
    ),
    (
      LibraryTarget::RecentlyPlayed,
      Requirement::Source(Source::Spotify),
    ),
    (LibraryTarget::Friends, Requirement::None),
    (LibraryTarget::Stats, Requirement::None),
    (LibraryTarget::PlaylistSync, Requirement::None),
    (
      LibraryTarget::LikedSongs,
      Requirement::Source(Source::Spotify),
    ),
    (LibraryTarget::Albums, Requirement::Source(Source::Spotify)),
    (LibraryTarget::Artists, Requirement::Source(Source::Spotify)),
    (
      LibraryTarget::Podcasts,
      Requirement::Source(Source::Spotify),
    ),
    #[cfg(feature = "local-files")]
    (LibraryTarget::LocalFiles, Requirement::None),
    #[cfg(feature = "ai-dj")]
    (LibraryTarget::AiDj, Requirement::None),
  ]
}

impl App {
  // Library display state. Every write goes through a `_mut` accessor, which bumps the Library revision.

  pub(crate) fn library(&self) -> &Library {
    &self.library
  }

  pub(crate) fn library_mut(&mut self) -> &mut Library {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.library
  }

  pub(crate) fn playlists(&self) -> &Option<Paged<PlaylistInfo>> {
    &self.playlists
  }

  pub(crate) fn playlists_mut(&mut self) -> &mut Option<Paged<PlaylistInfo>> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.playlists
  }

  pub(crate) fn all_playlists(&self) -> &Vec<PlaylistInfo> {
    &self.all_playlists
  }

  pub(crate) fn all_playlists_mut(&mut self) -> &mut Vec<PlaylistInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.all_playlists
  }

  pub(crate) fn playlist_folder_items(&self) -> &Vec<PlaylistFolderItem> {
    &self.playlist_folder_items
  }

  pub(crate) fn playlist_folder_items_mut(&mut self) -> &mut Vec<PlaylistFolderItem> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.playlist_folder_items
  }

  pub(crate) fn liked_song_ids_set(&self) -> &HashSet<String> {
    &self.liked_song_ids_set
  }

  pub(crate) fn liked_song_ids_set_mut(&mut self) -> &mut HashSet<String> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.liked_song_ids_set
  }

  pub(crate) fn followed_artist_ids_set(&self) -> &HashSet<String> {
    &self.followed_artist_ids_set
  }

  pub(crate) fn followed_artist_ids_set_mut(&mut self) -> &mut HashSet<String> {
    self.display_revisions.bump(DisplayDomain::Library);
    // The artist page shows whether its artist is followed.
    self.display_revisions.bump(DisplayDomain::Artist);
    &mut self.followed_artist_ids_set
  }

  pub(crate) fn saved_album_ids_set(&self) -> &HashSet<String> {
    &self.saved_album_ids_set
  }

  pub(crate) fn saved_album_ids_set_mut(&mut self) -> &mut HashSet<String> {
    self.display_revisions.bump(DisplayDomain::Library);
    // The album page shows whether its album is saved.
    self.display_revisions.bump(DisplayDomain::Album);
    &mut self.saved_album_ids_set
  }

  pub(crate) fn saved_show_ids_set(&self) -> &HashSet<String> {
    &self.saved_show_ids_set
  }

  pub(crate) fn saved_show_ids_set_mut(&mut self) -> &mut HashSet<String> {
    self.display_revisions.bump(DisplayDomain::Library);
    // The show page shows whether its show is saved.
    self.display_revisions.bump(DisplayDomain::Show);
    &mut self.saved_show_ids_set
  }

  pub(crate) fn user(&self) -> &Option<UserInfo> {
    &self.user
  }

  pub(crate) fn user_mut(&mut self) -> &mut Option<UserInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.user
  }

  pub(crate) fn playlist_folder_nodes(&self) -> &Option<Vec<PlaylistFolderNode>> {
    &self._playlist_folder_nodes
  }

  pub(crate) fn playlist_folder_nodes_mut(&mut self) -> &mut Option<Vec<PlaylistFolderNode>> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self._playlist_folder_nodes
  }
}

impl App {
  /// The sidebar rows the active source and the session can serve.
  pub(crate) fn library_rows(&self) -> Vec<LibraryTarget> {
    library_row_requirements()
      .iter()
      .filter(|(_, needs)| self.availability(*needs).is_available())
      .map(|(target, _)| *target)
      .collect()
  }

  /// Where the selected row sits among the rows offered now; the top row when it is hidden.
  pub(crate) fn library_cursor(&self) -> usize {
    self
      .library_rows()
      .iter()
      .position(|row| *row == self.view.library_selected)
      .unwrap_or(0)
  }
}

#[derive(Clone)]
pub struct Library {
  pub saved_tracks: ScrollableResultPages<Paged<TrackInfo>>,
  pub saved_albums: ScrollableResultPages<Paged<SavedAlbumInfo>>,
  pub saved_shows: ScrollableResultPages<Paged<ShowInfo>>,
  pub saved_artists: ScrollableResultPages<CursorPaged<ArtistInfo>>,
  pub show_episodes: ScrollableResultPages<Paged<EpisodeInfo>>,
}

// Is it possible to compose enums?
#[derive(Clone, PartialEq, Debug, Copy)]
pub enum AlbumTableContext {
  Simplified,
  Full,
}

#[derive(Clone, PartialEq, Debug, Copy)]
pub enum EpisodeTableContext {
  Simplified,
  Full,
}

#[derive(Clone)]
pub struct SelectedShow {
  pub show: ShowInfo,
}

#[derive(Clone)]
pub struct SelectedFullShow {
  pub show: ShowInfo,
}

#[derive(Clone)]
pub struct SelectedAlbum {
  pub album: crate::core::plugin_api::AlbumInfo,
  pub tracks: crate::core::pagination::Paged<crate::core::plugin_api::TrackInfo>,
  pub selected_index: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct SelectedFullAlbum {
  pub album: crate::core::plugin_api::AlbumInfo,
  pub selected_index: usize,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Artist {
  pub artist_id: String,
  pub artist_name: String,
  pub image_url: Option<String>,
  pub albums: crate::core::pagination::Paged<crate::core::plugin_api::AlbumInfo>,
  pub related_artists: Vec<crate::core::plugin_api::ArtistInfo>,
  pub top_tracks: Vec<crate::core::plugin_api::TrackInfo>,
  pub selected_album_index: usize,
  pub selected_related_artist_index: usize,
  pub selected_top_track_index: usize,
  pub artist_hovered_block: ArtistBlock,
  pub artist_selected_block: ArtistBlock,
}

impl Artist {
  /// Whether the RelatedArtists block has anything to show.
  pub fn related_artists_visible(&self) -> bool {
    !self.related_artists.is_empty()
  }

  /// Whether the TopTracks block has anything to show
  pub fn top_tracks_visible(&self) -> bool {
    !self.top_tracks.is_empty()
  }
}

impl App {
  /// Sort the recently-played track list in place per `recently_played_sort`.
  /// `Default` keeps the API's play order (a re-fetch restores it).
  pub fn sort_recently_played_items(&mut self) {
    let sort_state = self.recently_played_sort;
    if sort_state.field == SortField::Default {
      return;
    }
    if let Some(page) = self.recently_played.as_mut() {
      use crate::core::sort::sort_by_key_with_order;
      match sort_state.field {
        SortField::Name => {
          sort_by_key_with_order(&mut page.items, sort_state.order, |t| t.name.to_lowercase())
        }
        SortField::Artist => sort_by_key_with_order(&mut page.items, sort_state.order, |t| {
          t.artists.first().map(|s| s.to_lowercase())
        }),
        SortField::Album => sort_by_key_with_order(&mut page.items, sort_state.order, |t| {
          t.album.to_lowercase()
        }),
        _ => {}
      }
    }
  }

  /// Sort one list surface by a field, like the sort menu's Enter.
  pub(crate) fn apply_sort_field(&mut self, context: SortContext, field: SortField) {
    self.sort_state_mut(context).apply_field(field);

    // Actually sort the data
    match context {
      SortContext::PlaylistTracks => {
        if let Some(playlist_id) = self.current_playlist_track_table_id() {
          if self.try_sort_playlist_tracks_from_cached_pages(&playlist_id) {
            return;
          }

          self.dispatch(IoEvent::FetchAllPlaylistTracksAndSort(
            playlist_id.id().to_string(),
          ));
        }
      }
      SortContext::SavedAlbums => self.sort_saved_albums(),
      SortContext::SavedArtists => self.sort_saved_artists(),
      SortContext::RecentlyPlayed => self.sort_recently_played_items(),
    }
  }

  /// Flip the recorded direction without re-sorting loaded rows (the sort
  /// menu's uppercase shortcut, quirk kept).
  pub(crate) fn toggle_sort_order(&mut self, context: SortContext) {
    let sort_state = self.sort_state_mut(context);
    sort_state.order = sort_state.order.toggle();
  }

  pub(crate) fn sort_state(&self, context: SortContext) -> &SortState {
    match context {
      SortContext::PlaylistTracks => &self.playlist_sort,
      SortContext::SavedAlbums => &self.album_sort,
      SortContext::SavedArtists => &self.artist_sort,
      SortContext::RecentlyPlayed => &self.recently_played_sort,
    }
  }

  fn sort_state_mut(&mut self, context: SortContext) -> &mut SortState {
    match context {
      SortContext::PlaylistTracks => &mut self.playlist_sort,
      SortContext::SavedAlbums => &mut self.album_sort,
      SortContext::SavedArtists => &mut self.artist_sort,
      SortContext::RecentlyPlayed => &mut self.recently_played_sort,
    }
  }

  fn sort_saved_albums(&mut self) {
    use crate::core::sort::sort_by_key_with_order;

    let sort_state = self.album_sort;

    // Sort library.saved_albums pages
    for page in &mut self.library_mut().saved_albums.pages {
      match sort_state.field {
        SortField::Name => sort_by_key_with_order(&mut page.items, sort_state.order, |a| {
          a.album.name.to_lowercase()
        }),
        SortField::Artist => sort_by_key_with_order(&mut page.items, sort_state.order, |a| {
          a.album
            .artists
            .first()
            .map(|artist| artist.name.to_lowercase())
            .unwrap_or_default()
        }),
        SortField::DateAdded => {
          sort_by_key_with_order(&mut page.items, sort_state.order, |a| a.added_at.clone())
        }
        // Spotify dates are `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, so the strings
        // order chronologically; an album without one sorts first.
        SortField::ReleaseDate => sort_by_key_with_order(&mut page.items, sort_state.order, |a| {
          a.album.release_date.clone()
        }),
        _ => {}
      }
    }
  }

  fn sort_saved_artists(&mut self) {
    use crate::core::sort::sort_by_key_with_order;

    let sort_state = self.artist_sort;
    if sort_state.field != SortField::Name {
      return;
    }

    // Sort library.saved_artists pages
    for page in &mut self.library_mut().saved_artists.pages {
      sort_by_key_with_order(&mut page.items, sort_state.order, |a| a.name.to_lowercase());
    }

    // Also sort the app.artists vec
    sort_by_key_with_order(&mut self.artists, sort_state.order, |a| {
      a.name.to_lowercase()
    });
    self.display_revisions.bump(DisplayDomain::Artist);
  }

  pub(crate) fn reload_stats(&mut self) {
    self.stats_loading = true;
    self.display_revisions.bump(DisplayDomain::Stats);
    self.dispatch(IoEvent::LoadListeningStats(self.stats_period));
  }

  pub(crate) fn stats_loading(&self) -> bool {
    self.stats_loading
  }

  /// Cycling periods quickly can race two loads; only the response for the
  /// currently selected period lands.
  pub(crate) fn land_listening_stats(&mut self, period: RecapPeriod, stats: StatsData) {
    if self.stats_period != period {
      return;
    }
    self.stats_data = Some(stats);
    self.stats_loading = false;
    self.display_revisions.bump(DisplayDomain::Stats);
  }

  pub(crate) fn fail_listening_stats(&mut self) {
    self.stats_loading = false;
    self.display_revisions.bump(DisplayDomain::Stats);
  }

  /// The period is written before the fetch: the result handler only accepts
  /// data for the current period.
  pub(crate) fn set_stats_period(&mut self, period: RecapPeriod) {
    self.stats_period = period;
    self.stats_data = None;
    self.reload_stats();
  }

  pub(crate) fn cycle_stats_period(&mut self, forward: bool) {
    let period = if forward {
      self.stats_period.next()
    } else {
      self.stats_period.prev()
    };
    self.set_stats_period(period);
  }

  /// The saved tracks cached from offset 0 up to the first gap, the index of that
  /// run's last page, and whether a page follows it.
  pub(crate) fn saved_tracks_prefix(&self) -> (Vec<TrackInfo>, usize, bool) {
    let mut tracks = Vec::new();
    let mut expected_offset = 0;
    let mut seen_offsets = HashSet::new();
    let mut active_index = 0;
    let mut has_more = false;

    for (page_index, page) in self.library.saved_tracks.pages.iter().enumerate() {
      if page.offset != expected_offset || !seen_offsets.insert(page.offset) {
        break;
      }

      tracks.extend(page.items.iter().cloned());
      expected_offset = expected_offset.saturating_add(page.limit);
      active_index = page_index;
      has_more = page.next.is_some();

      if !has_more {
        break;
      }
    }

    (tracks, active_index, has_more)
  }

  /// The Liked Songs page cache; a write bumps `LikedSongs`, not `Library`.
  pub(crate) fn saved_tracks_mut(&mut self) -> &mut ScrollableResultPages<Paged<TrackInfo>> {
    self.display_revisions.bump(DisplayDomain::LikedSongs);
    &mut self.library.saved_tracks
  }

  pub fn set_saved_tracks_to_table_continuous(&mut self) {
    if self.active_saved_track_filter.is_some() {
      return;
    }

    let (tracks, active_index, _) = self.saved_tracks_prefix();
    self.library.saved_tracks.index = active_index;
    self.replace_track_table_tracks(tracks);
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  /// Refresh the table from the Liked Songs cache only while the table still shows Liked Songs.
  pub(crate) fn refresh_saved_tracks_table(&mut self) {
    if self.track_table.context == Some(TrackTableContext::SavedTracks) {
      self.set_saved_tracks_to_table_continuous();
    } else {
      self.library.saved_tracks.index = self.saved_tracks_prefix().1;
    }
  }

  pub fn reset_saved_tracks_view(&mut self) {
    self.active_saved_track_filter = None;
    self.track_table_epoch = self.track_table_epoch.wrapping_add(1);
    self.saved_tracks_prefetch_generation = self.saved_tracks_prefetch_generation.wrapping_add(1);
    self.saved_tracks_prefetch_in_flight.clear();
    self.saved_tracks_mut().clear();
    self.pending_track_table_selection = None;
    self.view.track_table_index = 0;
    self.track_table.tracks.clear();
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  /// Open a library sidebar section: the whole Enter consequence of a
  /// library row (fetches, route pushes, bookkeeping), moved verbatim from
  /// the library handler so every frontend fires the same sequence through
  /// `Action::OpenLibrary`.
  pub fn open_library_section(&mut self, target: LibraryTarget) {
    match target {
      LibraryTarget::Discover => {
        self.fetch_discover();
        self.push_navigation_stack(RouteId::Discover, ActiveBlock::Discover);
      }
      // The row pushes the route immediately; only the global navigate
      // binding defers the push to the network result.
      LibraryTarget::RecentlyPlayed => {
        self.dispatch(IoEvent::GetRecentlyPlayed);
        self.push_navigation_stack(RouteId::RecentlyPlayed, ActiveBlock::RecentlyPlayed);
      }
      LibraryTarget::Friends => {
        self.push_navigation_stack(RouteId::Friends, ActiveBlock::Friends);
        // Load friend code + friends list on first open (or if empty)
        if self.friend_code.is_none() {
          self.dispatch(IoEvent::GetFriendCode);
        }
        if self.friends.is_empty() && !self.friends_loading {
          self.dispatch(IoEvent::GetFriends);
        }
        self.last_friends_refresh_at = std::time::Instant::now();
      }
      LibraryTarget::Stats => {
        self.reload_stats();
        self.push_navigation_stack(RouteId::Stats, ActiveBlock::Stats);
      }
      LibraryTarget::PlaylistSync => {
        self.push_navigation_stack(RouteId::PlaylistSync, ActiveBlock::PlaylistSync);
      }
      LibraryTarget::LikedSongs => {
        self.clear_saved_track_filter();
        self.reset_saved_tracks_view();
        self.dispatch(IoEvent::GetCurrentSavedTracks(None));
        self.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      }
      LibraryTarget::Albums => {
        self.dispatch(IoEvent::GetCurrentUserSavedAlbums(None));
        self.push_navigation_stack(RouteId::AlbumList, ActiveBlock::AlbumList);
      }
      LibraryTarget::Artists => {
        self.dispatch(IoEvent::GetFollowedArtists(None));
        self.push_navigation_stack(RouteId::Artists, ActiveBlock::Artists);
      }
      LibraryTarget::Podcasts => {
        self.dispatch(IoEvent::GetCurrentUserSavedShows(None));
        self.push_navigation_stack(RouteId::Podcasts, ActiveBlock::Podcasts);
      }
      // The row cannot be selected in builds without `local-files`, so the
      // arm is a no-op there.
      LibraryTarget::LocalFiles => {
        #[cfg(feature = "local-files")]
        self.push_navigation_stack(RouteId::LocalBrowser, ActiveBlock::LocalBrowser);
      }
      // The row cannot be selected in builds without `ai-dj`, so the arm is
      // a no-op there.
      LibraryTarget::AiDj => {
        #[cfg(feature = "ai-dj")]
        self.open_ai_dj_screen();
      }
    }
  }

  pub fn clear_saved_track_filter(&mut self) {
    let showing_saved_tracks = self.track_table.context == Some(TrackTableContext::SavedTracks);
    self.active_saved_track_filter = None;
    self.view.input_context = InputContext::GlobalSearch;

    // Don't overwrite the table if the user has navigated away to a playlist or album!
    if !showing_saved_tracks {
      return;
    }

    if self.library.saved_tracks.pages.is_empty() {
      self.track_table.tracks.clear();
      self.view.track_table_index = 0;
      return;
    }
    self.set_saved_tracks_to_table_continuous();
  }

  pub fn apply_saved_track_search_results(&mut self, query: String, matches: Vec<TrackInfo>) {
    self.active_saved_track_filter = Some(query);
    self.forget_pending_row_selection();
    self.view.track_table_index = 0;
    self.track_table.tracks = matches;
  }

  pub(crate) fn try_search_saved_tracks_from_cached_pages(&mut self, query: &str) -> bool {
    if self.track_table.context != Some(TrackTableContext::SavedTracks)
      || !self.library.saved_tracks.is_complete()
    {
      return false;
    }

    let terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

    let mut matches = Vec::new();
    for page in &self.library.saved_tracks.pages {
      for track in &page.items {
        // If track has an ID and the user unliked it in this session, skip it!
        if let Some(id) = &track.id {
          if !self.liked_song_ids_set().contains(id) {
            continue;
          }
        }

        if playlist_track_info_matches_terms(track, &terms) {
          matches.push(track.clone());
        }
      }
    }

    self.apply_saved_track_search_results(query.to_string(), matches);
    true
  }

  pub fn is_saved_track_filter_active(&self) -> bool {
    self.track_table.context == Some(TrackTableContext::SavedTracks)
      && self.active_saved_track_filter.is_some()
  }

  pub fn next_missing_saved_tracks_offset(&self, page_index: usize) -> Option<u32> {
    let saved_tracks_page = self.library.saved_tracks.get_results(Some(page_index))?;
    saved_tracks_page.next.as_ref()?;

    let next_offset = saved_tracks_page.offset + saved_tracks_page.limit;
    self
      .library
      .saved_tracks
      .page_index_for_offset(next_offset)
      .is_none()
      .then_some(next_offset)
  }

  pub fn next_missing_saved_tracks_offset_continuous(&self) -> Option<u32> {
    let saved_tracks_page = self
      .library
      .saved_tracks
      .get_results(Some(self.library.saved_tracks.index))?;
    saved_tracks_page.next.as_ref()?;
    Some(saved_tracks_page.offset + saved_tracks_page.limit)
  }

  pub fn current_saved_tracks_has_more_tracks(&self) -> bool {
    self
      .library
      .saved_tracks
      .get_results(Some(self.library.saved_tracks.index))
      .is_some_and(|saved_tracks| saved_tracks.next.is_some())
  }

  pub fn set_saved_artists_to_table(&mut self, saved_artists_page: &CursorPaged<ArtistInfo>) {
    self.artists = saved_artists_page.items.clone();
    self.display_revisions.bump(DisplayDomain::Artist);
  }

  pub fn get_current_user_saved_artists_next(&mut self) {
    match self
      .library
      .saved_artists
      .get_results(Some(self.library.saved_artists.index + 1))
      .cloned()
    {
      Some(saved_artists) => {
        self.set_saved_artists_to_table(&saved_artists);
        self.library_mut().saved_artists.index += 1
      }
      None => {
        if let Some(saved_artists) = &self.library.saved_artists.clone().get_results(None) {
          if let Some(last_artist) = saved_artists.items.last() {
            if let Some(after) = last_artist.id.as_deref() {
              self.dispatch(IoEvent::GetFollowedArtists(Some(after.to_string())));
            }
          }
        }
      }
    }
  }

  pub fn get_current_user_saved_artists_previous(&mut self) {
    if self.library.saved_artists.index > 0 {
      self.library_mut().saved_artists.index -= 1;
    }

    if let Some(saved_artists) = &self.library.saved_artists.get_results(None).cloned() {
      self.set_saved_artists_to_table(saved_artists);
    }
  }

  pub fn get_current_user_saved_tracks_next(&mut self) {
    if !self.current_saved_tracks_has_more_tracks() {
      return;
    }

    if let Some(next_offset) = self.next_missing_saved_tracks_offset_continuous() {
      if self
        .library
        .saved_tracks
        .page_index_for_offset(next_offset)
        .is_some()
      {
        self.set_saved_tracks_to_table_continuous();
      } else if !self.saved_tracks_prefetch_in_flight.contains(&next_offset) {
        self.saved_tracks_prefetch_in_flight.insert(next_offset);
        self.dispatch(IoEvent::GetCurrentSavedTracks(Some(next_offset)));
      }
    }
  }

  pub fn get_current_user_saved_albums_next(&mut self) {
    let Some(current) = self.library.saved_albums.get_results(None) else {
      return;
    };
    if !current.has_next() {
      return;
    }
    let next_offset = current.offset + current.limit;
    if self
      .library
      .saved_albums
      .get_results(Some(self.library.saved_albums.index + 1))
      .is_some()
    {
      self.library_mut().saved_albums.index += 1;
    } else {
      self.dispatch(IoEvent::GetCurrentUserSavedAlbums(Some(next_offset)));
    }
    // Back to the top row, where the next page opens.
    self.view.album_list_index = 0;
  }

  /// Show a fetched saved-albums page; one whose offset is already cached replaces that copy.
  pub(crate) fn store_saved_albums_page(&mut self, page: Paged<SavedAlbumInfo>) {
    // Every album on a saved-albums page is saved, so the album page's
    // heart can show its state without a separate save check.
    for saved_album in &page.items {
      if let Some(id) = saved_album.album.id.as_deref() {
        self.saved_album_ids_set_mut().insert(id.to_string());
      }
    }
    let albums = &mut self.library_mut().saved_albums;
    match albums
      .pages
      .iter()
      .position(|cached| cached.offset == page.offset)
    {
      Some(slot) => {
        albums.pages[slot] = page;
        albums.index = slot;
      }
      None => albums.add_pages(page),
    }
  }

  /// Show a fetched saved-shows page and mark its shows as saved.
  pub(crate) fn store_saved_shows_page(&mut self, page: Paged<ShowInfo>) {
    let ids: Vec<String> = page
      .items
      .iter()
      .filter_map(|show| show.id.clone())
      .collect();
    self.saved_show_ids_set.extend(ids);
    let shows = &mut self.library_mut().saved_shows;
    match shows
      .pages
      .iter()
      .position(|cached| cached.offset == page.offset)
    {
      Some(slot) => {
        shows.pages[slot] = page;
        shows.index = slot;
      }
      None => shows.add_pages(page),
    }
  }

  /// Drop the cached episodes: they belong to the show opened before.
  pub(crate) fn clear_show_episodes(&mut self) {
    self.display_revisions.bump(DisplayDomain::Show);
    self.library.show_episodes = ScrollableResultPages::new();
  }

  /// The id of the show the episode table shows.
  pub(crate) fn selected_show_id(&self) -> Option<&str> {
    match self.episode_table_context {
      EpisodeTableContext::Full => self.selected_show_full.as_ref()?.show.id.as_deref(),
      EpisodeTableContext::Simplified => self.selected_show_simplified.as_ref()?.show.id.as_deref(),
    }
  }

  /// Show a fetched episode page of the open show; `first` starts the list over.
  pub(crate) fn store_show_episodes_page(&mut self, page: Paged<EpisodeInfo>, first: bool) {
    // Only the show page draws episodes, so a page never resends the library.
    self.display_revisions.bump(DisplayDomain::Show);
    let episodes = &mut self.library.show_episodes;
    if first {
      *episodes = ScrollableResultPages::new();
    }
    match episodes
      .pages
      .iter()
      .position(|cached| cached.offset == page.offset)
    {
      Some(slot) => {
        episodes.pages[slot] = page;
        episodes.index = slot;
      }
      None => episodes.add_pages(page),
    }
  }

  pub fn get_current_user_saved_albums_previous(&mut self) {
    if self.library.saved_albums.index > 0 {
      self.library_mut().saved_albums.index -= 1;
    }
  }

  /// Open a saved album's track list from the visible saved-albums page.
  pub fn open_saved_album(&mut self, album_id: String) {
    let Some(album) = self
      .library
      .saved_albums
      .get_results(None)
      .and_then(|albums| {
        albums
          .items
          .iter()
          .find(|saved| saved.album.id.as_deref() == Some(album_id.as_str()))
      })
      .map(|saved| &saved.album)
    else {
      return;
    };
    // The library cache embeds only the first page of each album's
    // tracklist (50 tracks max); refetch longer albums in full.
    let cached_is_complete = album
      .total_tracks
      .is_none_or(|total| album.tracks.len() as u32 >= total);
    if !cached_is_complete {
      // GetAlbum sets the Full context and pushes AlbumTracks itself.
      self.dispatch(IoEvent::GetAlbum(album_id));
      return;
    }
    self.selected_album_full = Some(SelectedFullAlbum {
      album: album.clone(),
      selected_index: 0,
    });
    self.album_table_context = AlbumTableContext::Full;
    self.display_revisions.bump(DisplayDomain::Album);
    self.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
  }

  /// Save or unsave the item playing right now, through the ownership order.
  pub fn toggle_save_current_item(&mut self) {
    const REFUSED: &str = "The current playback source cannot be liked";
    let uri = match self.playing_item() {
      PlayingItem::Spotify(PlayableItem::Track(track)) => Ok(track.id.as_ref().map(|id| id.uri())),
      PlayingItem::Spotify(PlayableItem::Episode(episode)) => Ok(Some(episode.id.uri())),
      PlayingItem::Spotify(_) | PlayingItem::Nothing => Ok(None),
      PlayingItem::QueuedSpotify(track) => track.uri.clone().ok_or(REFUSED).map(Some),
      PlayingItem::NotSpotify => Err(REFUSED),
    };
    match uri {
      Ok(Some(uri)) => self.dispatch(IoEvent::ToggleSaveTrack(uri)),
      Ok(None) => {}
      Err(status) => self.set_status_message(status, 4),
    }
  }

  pub fn get_current_user_saved_shows_next(&mut self) {
    match self
      .library
      .saved_shows
      .get_results(Some(self.library.saved_shows.index + 1))
      .cloned()
    {
      Some(_) => self.library_mut().saved_shows.index += 1,
      None => {
        if let Some(saved_shows) = &self.library.saved_shows.get_results(None) {
          let offset = Some(saved_shows.offset + saved_shows.limit);
          self.dispatch(IoEvent::GetCurrentUserSavedShows(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_shows_previous(&mut self) {
    if self.library.saved_shows.index > 0 {
      self.library_mut().saved_shows.index -= 1;
    }
  }

  /// Next page of the open show's episodes; a no-op when no show is open.
  pub(crate) fn get_episode_table_next(&mut self) {
    let Some(show_id) = self.selected_episode_show_id() else {
      return;
    };
    match self
      .library
      .show_episodes
      .get_results(Some(self.library.show_episodes.index + 1))
      .cloned()
    {
      Some(_) => self.library_mut().show_episodes.index += 1,
      None => {
        if let Some(show_episodes) = &self.library.show_episodes.get_results(None) {
          let offset = Some(show_episodes.offset + show_episodes.limit);
          self.dispatch(IoEvent::GetCurrentShowEpisodes(show_id, offset));
        }
      }
    }
  }

  pub fn get_episode_table_previous(&mut self) {
    if self.library.show_episodes.index > 0 {
      self.library_mut().show_episodes.index -= 1;
    }
  }

  /// Resolve the currently selected show's id/URI (from the episode-table
  /// context). Returns `None` if the stored domain show has no id.
  pub(crate) fn selected_episode_show_id(&self) -> Option<String> {
    match self.episode_table_context {
      EpisodeTableContext::Full => self
        .selected_show_full
        .as_ref()
        .and_then(|s| s.show.id.clone()),
      EpisodeTableContext::Simplified => self
        .selected_show_simplified
        .as_ref()
        .and_then(|s| s.show.id.clone()),
    }
  }

  pub fn get_artist(&mut self, artist_id: String, input_artist_name: String) {
    let user_country = self.get_user_country();
    self.dispatch(IoEvent::GetArtist(
      artist_id,
      input_artist_name,
      user_country,
    ));
  }

  pub fn get_user_country(&self) -> Option<Country> {
    // `country` is stored as its ISO 3166-1 alpha-2 string (the multi-source
    // domain holds no rspotify types); re-derive the rspotify `Country` here at
    // the boundary, the same way IDs are re-parsed when dispatching IoEvents.
    let code = self
      .user
      .as_ref()
      .and_then(|user| user.country.as_deref())?;
    serde_json::from_value(serde_json::Value::String(code.to_string())).ok()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  #[test]
  fn default_sort_recently_played_seeds_state_and_sorts_items() {
    let (tx, _rx) = channel();
    let mut config = UserConfig::new();
    config.behavior.default_sort_recently_played = "name:desc".to_string();
    let mut app = App::new(tx, config, Some(SystemTime::now()));
    assert_eq!(app.recently_played_sort.field, SortField::Name);
    assert_eq!(app.recently_played_sort.order, SortOrder::Descending);

    app.recently_played = Some(crate::core::pagination::CursorPaged {
      items: vec![
        queue_track(None, "Alpha"),
        queue_track(None, "Charlie"),
        queue_track(None, "Bravo"),
      ],
      limit: 3,
      next: None,
      cursor_after: None,
      total: None,
    });

    app.sort_recently_played_items();

    let names: Vec<_> = app
      .recently_played
      .as_ref()
      .unwrap()
      .items
      .iter()
      .map(|t| t.name.as_str())
      .collect();
    assert_eq!(names, vec!["Charlie", "Bravo", "Alpha"]);
  }

  #[test]
  fn a_radio_result_is_dropped_once_liked_songs_opened_after_the_request() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.get_recommendations_for_track_id("0000000000000000000001".to_string());
    assert!(app.radio_result_is_current());

    app.reset_saved_tracks_view();

    assert!(!app.radio_result_is_current());
  }

  #[test]
  fn reset_saved_tracks_view_clears_cached_pages_and_bumps_generation() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.saved_tracks_prefetch_generation = 7;
    let saved_tracks_domain_page = crate::infra::network::mapping::map_page(
      &saved_tracks_page(
        0,
        2,
        &["0000000000000000000001", "0000000000000000000002"],
        false,
      ),
      |st| TrackInfo::from(&st.track),
    );
    app.library.saved_tracks.add_pages(saved_tracks_domain_page);
    app.track_table.tracks = vec![
      TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];
    app.view.track_table_index = 1;

    app.reset_saved_tracks_view();

    assert_eq!(app.saved_tracks_prefetch_generation, 8);
    assert!(app.library.saved_tracks.pages.is_empty());
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.view.track_table_index, 0);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
  }

  #[cfg(feature = "streaming")]
  mod queued_spotify_like {
    use super::*;
    use crate::infra::queue::QueueNowPlaying;
    use std::sync::mpsc::Receiver;

    /// An app whose cached playback context still names the suspended context's
    /// last Spotify track — the regression target: Like must never save it
    /// while the queue slot owns playback.
    #[allow(deprecated)]
    fn app_with_stale_context() -> (App, Receiver<IoEvent>) {
      let (tx, rx) = channel();
      let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
      app.current_playback_context = Some(CurrentPlaybackContext {
        device: Device {
          id: Some("native-device".to_string()),
          is_active: true,
          is_private_session: false,
          is_restricted: false,
          name: "spotatui".to_string(),
          _type: DeviceType::Computer,
          volume_percent: Some(50),
        },
        repeat_state: RepeatState::Off,
        shuffle_state: false,
        context: None,
        timestamp: Utc::now(),
        progress: None,
        is_playing: true,
        item: Some(PlayableItem::Track(FullTrack {
          album: SimplifiedAlbum::default(),
          artists: vec![SimplifiedArtist::default()],
          available_markets: Vec::new(),
          disc_number: 1,
          duration: chrono::Duration::milliseconds(180_000),
          explicit: false,
          external_ids: Default::default(),
          external_urls: Default::default(),
          href: None,
          id: Some(
            TrackId::from_id("0000000000000000000009")
              .unwrap()
              .into_static(),
          ),
          is_local: false,
          is_playable: Some(true),
          linked_from: None,
          restrictions: None,
          name: "Cached".to_string(),
          popularity: 50,
          preview_url: None,
          track_number: 1,
          r#type: rspotify::model::Type::Track,
        })),
        currently_playing_type: CurrentlyPlayingType::Track,
        actions: Actions::default(),
      });
      (app, rx)
    }

    #[test]
    fn like_saves_the_queued_spotify_track_not_the_cached_context_item() {
      let (mut app, rx) = app_with_stale_context();
      app.queue_now = Some(QueueNowPlaying::Spotify {
        track: queue_track(Some("spotify:track:queued"), "Queued"),
      });

      app.toggle_save_current_item();

      assert!(
        matches!(rx.try_recv(), Ok(IoEvent::ToggleSaveTrack(uri)) if uri == "spotify:track:queued"),
        "expected the queue slot's own track to be liked"
      );
      assert!(app.status_message.is_none());
    }

    #[test]
    fn like_for_uri_less_queued_track_never_falls_back_to_cached_context() {
      let (mut app, rx) = app_with_stale_context();
      app.queue_now = Some(QueueNowPlaying::Spotify {
        track: queue_track(None, "Queued"),
      });

      app.toggle_save_current_item();

      assert!(rx.try_recv().is_err(), "nothing may be dispatched");
      assert_eq!(
        app.status_message.as_deref(),
        Some("The current playback source cannot be liked")
      );
    }
  }

  #[test]
  fn playlist_sort_dispatches_for_current_playlist_table_id() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let sidebar_playlist = playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Sidebar Playlist",
      "spotatui-test-user",
      false,
    );
    let search_playlist_id = PlaylistId::from_id("37i9dQZF1DX4WYpdgoIcn6")
      .unwrap()
      .into_static();
    app.all_playlists = vec![sidebar_playlist];
    app.view.active_playlist_index = Some(0);
    app.track_table.context = Some(TrackTableContext::PlaylistSearch);
    app.playlist_track_table_id = Some(search_playlist_id.clone());

    app.apply_sort_field(SortContext::PlaylistTracks, SortField::Name);

    match rx.recv().unwrap() {
      IoEvent::FetchAllPlaylistTracksAndSort(playlist_id) => {
        assert_eq!(playlist_id, search_playlist_id.id());
      }
      _ => panic!("expected playlist sort fetch"),
    }
  }

  fn saved_album_page(offset: u32, ids: &[&str], has_next: bool) -> Paged<SavedAlbumInfo> {
    Paged {
      items: ids
        .iter()
        .map(|id| SavedAlbumInfo {
          album: crate::core::plugin_api::AlbumInfo {
            id: Some(id.to_string()),
            ..Default::default()
          },
          added_at: String::new(),
        })
        .collect(),
      offset,
      limit: ids.len() as u32,
      total: 6,
      next: has_next.then(|| "https://example.com/me/albums?next".to_string()),
      previous: None,
    }
  }

  #[test]
  fn release_date_sort_puts_the_newest_saved_album_first_when_descending() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    let mut page = saved_album_page(0, &["mid", "undated", "new", "old"], false);
    let dates = [Some("2019-06"), None, Some("2024-05-17"), Some("1999")];
    for (item, date) in page.items.iter_mut().zip(dates) {
      item.album.release_date = date.map(str::to_string);
    }
    app.library.saved_albums.upsert_page_by_offset(page);

    // The first press sorts ascending, the second flips to descending.
    app.apply_sort_field(SortContext::SavedAlbums, SortField::ReleaseDate);
    app.apply_sort_field(SortContext::SavedAlbums, SortField::ReleaseDate);

    let ids: Vec<&str> = app.library.saved_albums.pages[0]
      .items
      .iter()
      .filter_map(|a| a.album.id.as_deref())
      .collect();
    assert_eq!(ids, ["new", "mid", "old", "undated"]);
  }

  #[test]
  fn saved_albums_next_fetches_the_following_offset_and_resets_the_cursor() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app
      .library
      .saved_albums
      .upsert_page_by_offset(saved_album_page(0, &["a1", "a2"], true));
    app.view.album_list_index = 1;

    app.get_current_user_saved_albums_next();

    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetCurrentUserSavedAlbums(Some(2)))
    ));
    assert_eq!(app.view.album_list_index, 0);
  }

  #[test]
  fn saved_albums_next_on_the_final_page_fetches_nothing() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app
      .library
      .saved_albums
      .upsert_page_by_offset(saved_album_page(0, &["a1", "a2"], false));
    app.view.album_list_index = 1;

    app.get_current_user_saved_albums_next();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.view.album_list_index, 1);
    assert_eq!(app.library.saved_albums.index, 0);
  }

  #[test]
  fn saved_albums_next_flips_to_a_cached_page_without_fetching() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app
      .library
      .saved_albums
      .upsert_page_by_offset(saved_album_page(0, &["a1", "a2"], true));
    app
      .library
      .saved_albums
      .upsert_page_by_offset(saved_album_page(2, &["a3", "a4"], true));
    app.view.album_list_index = 1;

    app.get_current_user_saved_albums_next();

    assert!(rx.try_recv().is_err());
    assert_eq!(app.library.saved_albums.index, 1);
    assert_eq!(app.view.album_list_index, 0);
  }

  #[test]
  fn a_saved_albums_page_marks_its_albums_saved() {
    let mut app = App::default();
    app.store_saved_albums_page(saved_album_page(0, &["a1", "a2"], true));
    app.store_saved_albums_page(saved_album_page(2, &["a3"], false));

    assert!(app.saved_album_ids_set().contains("a1"));
    assert!(app.saved_album_ids_set().contains("a2"));
    assert!(app.saved_album_ids_set().contains("a3"));
  }

  #[test]
  fn a_saved_albums_page_fetched_again_replaces_its_cached_copy() {
    let mut app = App::default();
    app.store_saved_albums_page(saved_album_page(0, &["a1", "a2"], true));
    app.store_saved_albums_page(saved_album_page(2, &["a3", "a4"], true));

    // Reopening Albums fetches the first page again.
    app.store_saved_albums_page(saved_album_page(0, &["b1", "b2"], true));

    let albums = &app.library.saved_albums;
    assert_eq!(albums.pages.len(), 2);
    assert_eq!(albums.index, 0);
    assert_eq!(albums.pages[0].items[0].album.id.as_deref(), Some("b1"));

    // The next page is still one step forward, not stuck behind a duplicate.
    app.get_current_user_saved_albums_next();
    assert_eq!(app.library.saved_albums.index, 1);

    // A second response for the same offset does not append a copy.
    app.store_saved_albums_page(saved_album_page(2, &["a3", "a4"], true));
    assert_eq!(app.library.saved_albums.pages.len(), 2);
    assert_eq!(app.library.saved_albums.index, 1);
  }

  #[test]
  fn saved_tracks_prefix_stops_at_the_first_gap_in_the_cache() {
    let mut app = App::default();
    let page = |offset: u32, ids: &[&str]| Paged {
      items: ids
        .iter()
        .map(|id| TrackInfo::from(&full_track(id, id)))
        .collect(),
      offset,
      limit: 2,
      total: 6,
      next: Some("next".to_string()),
      previous: None,
    };
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(page(0, &["t1", "t2"]));
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(page(4, &["t5", "t6"]));

    let (tracks, last_page, has_more) = app.saved_tracks_prefix();

    assert_eq!(tracks.len(), 2);
    assert_eq!(last_page, 0);
    assert!(has_more);
    assert_eq!(App::default().saved_tracks_prefix(), (vec![], 0, false));

    let mut last = page(2, &["t3", "t4"]);
    last.next = None;
    app.library.saved_tracks.upsert_page_by_offset(last);

    let (tracks, last_page, has_more) = app.saved_tracks_prefix();

    assert_eq!(tracks.len(), 4);
    assert_eq!(last_page, 1);
    assert!(!has_more);
  }

  /// A Liked Songs page of the shape the network layer caches it in.
  fn saved_track_page(offset: u32, ids: &[&str], has_next: bool) -> Paged<TrackInfo> {
    Paged {
      items: ids
        .iter()
        .enumerate()
        .map(|(index, id)| TrackInfo::from(&full_track(id, &format!("Track {offset}-{index}"))))
        .collect(),
      limit: ids.len() as u32,
      next: has_next.then(|| "https://example.com/me/tracks?next".to_string()),
      offset,
      previous: None,
      total: 4,
    }
  }

  /// A Liked Songs table over a cache of `pages`, whose every id is in the liked
  /// set — the state `try_search_saved_tracks_from_cached_pages` reads.
  fn saved_track_search_app(pages: &[(u32, &[&str], bool)]) -> App {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.track_table.context = Some(TrackTableContext::SavedTracks);
    for (offset, ids, has_next) in pages {
      app
        .library
        .saved_tracks
        .upsert_page_by_offset(saved_track_page(*offset, ids, *has_next));
      for id in *ids {
        app.liked_song_ids_set_mut().insert(id.to_string());
      }
    }
    app.set_saved_tracks_to_table_continuous();
    app
  }

  fn cached_track_names(app: &App) -> Vec<String> {
    app
      .track_table
      .tracks
      .iter()
      .map(|track| track.name.clone())
      .collect()
  }

  #[test]
  fn saved_track_search_filters_the_complete_liked_cache() {
    let mut app = saved_track_search_app(&[
      (
        0,
        &["0000000000000000000001", "0000000000000000000002"],
        true,
      ),
      (
        2,
        &["0000000000000000000003", "0000000000000000000004"],
        false,
      ),
    ]);
    app.view.track_table_index = 1;
    app.select_row_when_next_page_lands(1);

    assert!(app.try_search_saved_tracks_from_cached_pages("track 2-1"));

    assert!(app.is_saved_track_filter_active());
    assert_eq!(app.active_saved_track_filter.as_deref(), Some("track 2-1"));
    assert_eq!(cached_track_names(&app), vec!["Track 2-1"]);
    assert_eq!(
      app.view.track_table_index, 0,
      "the cursor resets onto the hits"
    );
    assert!(
      app.pending_track_table_selection.is_none(),
      "a row parked for the unfiltered page is dropped"
    );
  }

  #[test]
  fn saved_track_search_skips_tracks_unliked_this_session() {
    let mut app = saved_track_search_app(&[(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    )]);
    // Unliked from the table: the id leaves the liked set the filter reads.
    app
      .liked_song_ids_set_mut()
      .remove("0000000000000000000002");

    assert!(app.try_search_saved_tracks_from_cached_pages("track 0"));

    assert_eq!(cached_track_names(&app), vec!["Track 0-0"]);
  }

  #[test]
  fn saved_track_search_is_refused_off_the_liked_songs_table() {
    let mut app = saved_track_search_app(&[(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    )]);
    app.track_table.context = Some(TrackTableContext::MyPlaylists);

    assert!(!app.try_search_saved_tracks_from_cached_pages("track 0"));

    assert!(!app.is_saved_track_filter_active());
    assert_eq!(app.track_table.tracks.len(), 2, "the rows are untouched");
  }

  #[test]
  fn saved_track_search_waits_for_the_whole_cache() {
    let mut app = saved_track_search_app(&[(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    )]);
    assert!(
      !app.library.saved_tracks.is_complete(),
      "a page still follows, so a match could hide below the cache"
    );

    assert!(!app.try_search_saved_tracks_from_cached_pages("track 0-1"));

    assert!(!app.is_saved_track_filter_active());
    assert_eq!(app.track_table.tracks.len(), 2, "the rows are untouched");
  }

  #[test]
  fn a_late_liked_page_does_not_replace_the_filtered_rows() {
    let mut app = saved_track_search_app(&[(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    )]);
    assert!(app.try_search_saved_tracks_from_cached_pages("track 0-1"));

    // The prefetched page lands while the filter is up.
    app
      .library
      .saved_tracks
      .upsert_page_by_offset(saved_track_page(2, &["0000000000000000000003"], true));
    app.refresh_saved_tracks_table();

    assert_eq!(cached_track_names(&app), vec!["Track 0-1"]);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
  }

  #[test]
  fn clearing_the_saved_track_filter_restores_the_cached_liked_view() {
    let mut app = saved_track_search_app(&[(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    )]);
    app.view.track_table_index = 1;
    app.select_row_when_next_page_lands(1);
    assert!(app.try_search_saved_tracks_from_cached_pages("track 0-1"));
    app.view.input_context = InputContext::SavedTrackSearch;

    app.clear_saved_track_filter();

    assert!(!app.is_saved_track_filter_active());
    assert_eq!(app.view.input_context, InputContext::GlobalSearch);
    assert_eq!(cached_track_names(&app), vec!["Track 0-0", "Track 0-1"]);
    assert_eq!(app.view.track_table_index, 0, "the parked row is dropped");
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
  }

  #[test]
  fn clearing_the_saved_track_filter_empties_a_table_with_no_cache() {
    let mut app = saved_track_search_app(&[(
      0,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    )]);
    assert!(app.try_search_saved_tracks_from_cached_pages("track 0-1"));
    app.saved_tracks_mut().clear();

    app.clear_saved_track_filter();

    assert!(!app.is_saved_track_filter_active());
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.view.track_table_index, 0);
  }

  #[test]
  fn reopening_liked_songs_drops_an_active_filter() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.track_table.context = Some(TrackTableContext::SavedTracks);
    app.active_saved_track_filter = Some("track 0-1".to_string());
    app.track_table.tracks = vec![TrackInfo::from(&full_track(
      "0000000000000000000002",
      "Track 0-1",
    ))];

    app.open_library_section(LibraryTarget::LikedSongs);

    assert!(!app.is_saved_track_filter_active());
    assert!(app.track_table.tracks.is_empty());
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetCurrentSavedTracks(None))
    ));
  }
}
