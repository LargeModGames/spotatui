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

  pub fn current_user_saved_album_delete(&mut self, block: ActiveBlock) {
    info!("removing album from saved albums");
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(albums) = &self.search_results.albums {
          if let Some(selected_index) = self.search_results.selected_album_index {
            let selected_album = &albums.items[selected_index];
            if let Some(ref id_str) = selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id_str.clone()));
            }
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(albums) = self.library.saved_albums.get_results(None) {
          if let Some(selected_album) = albums.items.get(self.album_list_index) {
            if let Some(id) = selected_album.album.id.as_deref() {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id.to_string()));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          if let Some(selected_album) = artist.albums.items.get(artist.selected_album_index) {
            if let Some(id_str) = &selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(id_str.clone()));
            }
          }
        }
      }
      _ => (),
    }
  }

  pub fn current_user_saved_album_add(&mut self, block: ActiveBlock) {
    info!("adding album to saved albums");
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(albums) = &self.search_results.albums {
          if let Some(selected_index) = self.search_results.selected_album_index {
            let selected_album = &albums.items[selected_index];
            if let Some(ref id_str) = selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(id_str.clone()));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          if let Some(selected_album) = artist.albums.items.get(artist.selected_album_index) {
            if let Some(id_str) = &selected_album.id {
              self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(id_str.clone()));
            }
          }
        }
      }
      _ => (),
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

  pub fn get_episode_table_next(&mut self, show_id: String) {
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

  pub fn user_unfollow_artists(&mut self, block: ActiveBlock) {
    info!("unfollowing artist");
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(artists) = &self.search_results.artists {
          if let Some(selected_index) = self.search_results.selected_artists_index {
            let selected_artist = &artists.items[selected_index];
            if let Some(ref id_str) = selected_artist.id {
              self.dispatch(IoEvent::UserUnfollowArtists(vec![id_str.clone()]));
            }
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(artists) = self.library.saved_artists.get_results(None) {
          if let Some(id) = artists
            .items
            .get(self.artists_list_index)
            .and_then(|selected_artist| selected_artist.id.as_deref())
          {
            self.dispatch(IoEvent::UserUnfollowArtists(vec![id.to_string()]));
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          let selected_artis = &artist.related_artists[artist.selected_related_artist_index];
          if let Some(id_str) = &selected_artis.id {
            self.dispatch(IoEvent::UserUnfollowArtists(vec![id_str.clone()]));
          }
        }
      }
      _ => (),
    };
  }

  pub fn user_follow_artists(&mut self, block: ActiveBlock) {
    info!("following artist");
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(artists) = &self.search_results.artists {
          if let Some(selected_index) = self.search_results.selected_artists_index {
            let selected_artist = &artists.items[selected_index];
            if let Some(ref id_str) = selected_artist.id {
              self.dispatch(IoEvent::UserFollowArtists(vec![id_str.clone()]));
            }
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist {
          let selected_artis = &artist.related_artists[artist.selected_related_artist_index];
          if let Some(id_str) = &selected_artis.id {
            self.dispatch(IoEvent::UserFollowArtists(vec![id_str.clone()]));
          }
        }
      }
      _ => (),
    }
  }

  pub fn user_follow_show(&mut self, block: ActiveBlock) {
    info!("following show");
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows {
          if let Some(selected_index) = self.search_results.selected_shows_index {
            if let Some(show) = shows.items.get(selected_index) {
              if let Some(ref id_str) = show.id {
                self.dispatch(IoEvent::CurrentUserSavedShowAdd(id_str.clone()));
              }
            }
          }
        }
      }
      ActiveBlock::EpisodeTable => {
        if let Some(show_id) = self.selected_episode_show_id() {
          self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id));
        }
      }
      _ => (),
    }
  }

  /// Resolve the currently selected show's id/URI (from the episode-table
  /// context). Returns `None` if the stored domain show has no id.
  fn selected_episode_show_id(&self) -> Option<String> {
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

  pub fn user_unfollow_show(&mut self, block: ActiveBlock) {
    info!("unfollowing show");
    match block {
      ActiveBlock::Podcasts => {
        if let Some(id) = self
          .library
          .saved_shows
          .get_results(None)
          .and_then(|shows| shows.items.get(self.shows_list_index))
          .and_then(|selected_show| selected_show.id.as_deref())
        {
          self.dispatch(IoEvent::CurrentUserSavedShowDelete(id.to_string()));
        }
      }
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows {
          if let Some(selected_index) = self.search_results.selected_shows_index {
            if let Some(ref id_str) = shows.items[selected_index].id {
              self.dispatch(IoEvent::CurrentUserSavedShowDelete(id_str.clone()));
            }
          }
        }
      }
      ActiveBlock::EpisodeTable => {
        if let Some(show_id) = self.selected_episode_show_id() {
          self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
        }
      }
      _ => (),
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
}
