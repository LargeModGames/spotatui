use super::*;

/// Sidebar library entries.
///
/// Built at first use rather than declared as a `const` per feature combination.
/// Feature-gated rows ("Local Files", "AI DJ") used to mean one `#[cfg]` arm per
/// combination, which is a cartesian product that doubles with every new gated
/// row; composing the list instead stays linear. Callers should look entries up
/// by name (`iter().position(...)`) rather than by index, since the index depends
/// on which features are built in.
pub fn library_options() -> &'static [&'static str] {
  static OPTIONS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
  OPTIONS.get_or_init(|| {
    // `mut` is only used by the gated pushes below, so a build with neither
    // feature would otherwise warn.
    #[allow(unused_mut)]
    let mut options = vec![
      "Discover",
      "Recently Played",
      "Friends",
      "Stats",
      "Liked Songs",
      "Albums",
      "Artists",
      "Podcasts",
    ];
    #[cfg(feature = "local-files")]
    options.push("Local Files");
    #[cfg(feature = "ai-dj")]
    options.push("AI DJ");
    options
  })
}

#[derive(Clone)]
pub struct Library {
  pub selected_index: usize,
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
  pub albums: crate::core::pagination::Paged<crate::core::plugin_api::AlbumInfo>,
  pub related_artists: Vec<crate::core::plugin_api::ArtistInfo>,
  pub top_tracks: Vec<crate::core::plugin_api::TrackInfo>,
  pub selected_album_index: usize,
  pub selected_related_artist_index: usize,
  pub selected_top_track_index: usize,
  pub artist_hovered_block: ArtistBlock,
  pub artist_selected_block: ArtistBlock,
}

impl App {
  /// Sort the recently-played track list in place per `recently_played_sort`.
  /// `Default` keeps the API's play order (a re-fetch restores it).
  pub fn sort_recently_played_items(&mut self) {
    let sort_state = self.recently_played_sort;
    if sort_state.field == SortField::Default {
      return;
    }
    if let Some(page) = self.recently_played.result.as_mut() {
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
    for page in &mut self.library.saved_albums.pages {
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
    for page in &mut self.library.saved_artists.pages {
      sort_by_key_with_order(&mut page.items, sort_state.order, |a| a.name.to_lowercase());
    }

    // Also sort the app.artists vec
    sort_by_key_with_order(&mut self.artists, sort_state.order, |a| {
      a.name.to_lowercase()
    });
  }

  pub(crate) fn reload_stats(&mut self) {
    self.stats_loading = true;
    self.dispatch(IoEvent::LoadListeningStats(self.stats_period));
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

  pub fn set_saved_tracks_to_table_continuous(&mut self) {
    let mut tracks = Vec::new();
    let mut expected_offset = 0;
    let mut seen_offsets = HashSet::new();
    let mut active_index = 0;

    for (page_index, page) in self.library.saved_tracks.pages.iter().enumerate() {
      if page.offset != expected_offset || !seen_offsets.insert(page.offset) {
        break;
      }

      tracks.extend(page.items.iter().cloned());
      expected_offset = expected_offset.saturating_add(page.limit);
      active_index = page_index;

      if page.next.is_none() {
        break;
      }
    }

    self.library.saved_tracks.index = active_index;
    self.replace_track_table_tracks(tracks);
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  pub fn reset_saved_tracks_view(&mut self) {
    self.saved_tracks_prefetch_generation = self.saved_tracks_prefetch_generation.wrapping_add(1);
    self.saved_tracks_prefetch_in_flight.clear();
    self.library.saved_tracks.clear();
    self.pending_track_table_selection = None;
    self.track_table.selected_index = 0;
    self.track_table.tracks.clear();
    self.track_table.context = Some(TrackTableContext::SavedTracks);
  }

  /// Open a library sidebar section: the whole Enter consequence of a
  /// library row (fetches, route pushes, bookkeeping), moved verbatim from
  /// the library handler so every frontend fires the same sequence through
  /// `Action::OpenLibrary`.
  pub fn open_library_section(&mut self, target: crate::core::action::LibraryTarget) {
    use crate::core::action::LibraryTarget;
    match target {
      LibraryTarget::Discover => {
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
      LibraryTarget::LikedSongs => {
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
        self.library.saved_artists.index += 1
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
      self.library.saved_artists.index -= 1;
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
    match self
      .library
      .saved_albums
      .get_results(Some(self.library.saved_albums.index + 1))
      .cloned()
    {
      Some(_) => self.library.saved_albums.index += 1,
      None => {
        if let Some(saved_albums) = &self.library.saved_albums.get_results(None) {
          let offset = Some(saved_albums.offset + saved_albums.limit);
          self.dispatch(IoEvent::GetCurrentUserSavedAlbums(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_albums_previous(&mut self) {
    if self.library.saved_albums.index > 0 {
      self.library.saved_albums.index -= 1;
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
    self.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
  }

  /// Save or unsave the item playing right now, through the ownership order.
  pub fn toggle_save_current_item(&mut self) {
    let queue_now_is_spotify = self.queue_now_is_spotify();
    let queued_spotify_track_uri = queue_now_is_spotify
      .then(|| self.queue_now_spotify_track_uri())
      .flatten();

    if spotify_context_is_suspended(
      self.queue_owns_playback(),
      queue_now_is_spotify,
      self.active_decoded_source(),
    ) {
      self.set_status_message("The current playback source cannot be liked", 4);
      return;
    }

    // A queued Spotify track plays via a direct `player.load` outside the Spirc
    // context, so the cached playback context still names the suspended context's
    // track — resolve the queue slot's own track instead of falling through.
    if queue_now_is_spotify {
      match queued_spotify_track_uri {
        Some(uri) => self.dispatch(IoEvent::ToggleSaveTrack(uri)),
        None => self.set_status_message("The current playback source cannot be liked", 4),
      }
      return;
    }

    let uri = match self
      .current_playback_context
      .as_ref()
      .and_then(|context| context.item.as_ref())
    {
      Some(PlayableItem::Track(track)) => track.id.as_ref().map(|id| id.uri()),
      Some(PlayableItem::Episode(episode)) => Some(episode.id.uri()),
      _ => None,
    };
    if let Some(uri) = uri {
      self.dispatch(IoEvent::ToggleSaveTrack(uri));
    }
  }

  pub fn get_current_user_saved_shows_next(&mut self) {
    match self
      .library
      .saved_shows
      .get_results(Some(self.library.saved_shows.index + 1))
      .cloned()
    {
      Some(_) => self.library.saved_shows.index += 1,
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
      self.library.saved_shows.index -= 1;
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
      Some(_) => self.library.show_episodes.index += 1,
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
      self.library.show_episodes.index -= 1;
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

/// Whether Like must not consult the cached Spotify playback context. A queue
/// slot playing a *decoded* item (or any decoded per-source playback) suspends
/// the context; a queue slot playing a *Spotify* track stays eligible — it is
/// liked via the slot's own track, never the cached context.
fn spotify_context_is_suspended(
  queue_owns_playback: bool,
  queue_now_is_spotify: bool,
  decoded_source_active: bool,
) -> bool {
  (queue_owns_playback && !queue_now_is_spotify) || decoded_source_active
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

    app.recently_played.result = Some(crate::core::pagination::CursorPaged {
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
      .result
      .as_ref()
      .unwrap()
      .items
      .iter()
      .map(|t| t.name.as_str())
      .collect();
    assert_eq!(names, vec!["Charlie", "Bravo", "Alpha"]);
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
    app.track_table.selected_index = 1;

    app.reset_saved_tracks_view();

    assert_eq!(app.saved_tracks_prefetch_generation, 8);
    assert!(app.library.saved_tracks.pages.is_empty());
    assert!(app.track_table.tracks.is_empty());
    assert_eq!(app.track_table.selected_index, 0);
    assert_eq!(
      app.track_table.context,
      Some(TrackTableContext::SavedTracks)
    );
  }

  #[test]
  fn non_spotify_playback_cannot_use_cached_spotify_item_for_like() {
    // A decoded queue slot or any decoded per-source playback suspends Like.
    assert!(spotify_context_is_suspended(true, false, false));
    assert!(spotify_context_is_suspended(false, false, true));
    // A queue slot playing a *Spotify* track stays eligible.
    assert!(!spotify_context_is_suspended(true, true, false));
    // Plain Spotify context playback.
    assert!(!spotify_context_is_suspended(false, false, false));
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
}
