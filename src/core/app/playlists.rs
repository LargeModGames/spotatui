use super::*;
use crate::core::sort::Sorter;

/// Playlist URIs with this scheme address the local YouTube playlists file.
pub(crate) const YOUTUBE_PLAYLIST_PREFIX: &str = "youtube:playlist:";

#[derive(Clone)]
pub struct PendingPlaylistTrackAdd {
  /// Track id/URI passed through to `AddTrackToPlaylist`.
  pub track_id: String,
  pub track_name: String,
}

#[derive(Clone)]
pub struct PendingPlaylistTrackRemoval {
  /// Playlist id/URI passed through to `RemoveTrackFromPlaylistAtPosition`.
  pub playlist_id: String,
  pub playlist_name: String,
  /// Track id/URI passed through to `RemoveTrackFromPlaylistAtPosition`.
  pub track_id: String,
  pub track_name: String,
  pub position: usize,
}

/// A row in the add-to-playlist picker dialog: a navigable folder or an
/// editable destination playlist.
#[derive(Debug)]
pub enum PlaylistPickerRow<'a> {
  Folder(&'a PlaylistFolder),
  Playlist(&'a PlaylistInfo),
}

/// Whether every whitespace-separated term occurs in `name`. Matching is
/// case-insensitive unless the term contains an uppercase character.
fn playlist_name_matches(name: &str, terms: &[&str]) -> bool {
  let lowered = name.to_lowercase();
  terms.iter().all(|term| {
    if term.chars().any(char::is_uppercase) {
      name.contains(term)
    } else {
      lowered.contains(&term.to_lowercase())
    }
  })
}

/// Which stage of the "Create Playlist" form we are on
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CreatePlaylistStage {
  #[default]
  Name,
  AddTracks,
}

/// Which panel inside the AddTracks stage has focus
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CreatePlaylistFocus {
  #[default]
  SearchInput,
  SearchResults,
  AddedTracks,
}

impl App {
  // The per-source sidebar lists, part of the Library display state.

  pub(crate) fn local_playlists(&self) -> &Vec<PlaylistInfo> {
    &self.local_playlists
  }

  #[cfg_attr(not(feature = "local-files"), allow(dead_code))]
  pub(crate) fn local_playlists_mut(&mut self) -> &mut Vec<PlaylistInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.local_playlists
  }

  pub(crate) fn subsonic_playlists(&self) -> &Vec<PlaylistInfo> {
    &self.subsonic_playlists
  }

  #[cfg_attr(not(feature = "subsonic"), allow(dead_code))]
  pub(crate) fn subsonic_playlists_mut(&mut self) -> &mut Vec<PlaylistInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.subsonic_playlists
  }

  pub(crate) fn qobuz_playlists(&self) -> &Vec<PlaylistInfo> {
    &self.qobuz_playlists
  }

  #[cfg_attr(not(feature = "qobuz"), allow(dead_code))]
  pub(crate) fn qobuz_playlists_mut(&mut self) -> &mut Vec<PlaylistInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.qobuz_playlists
  }

  pub(crate) fn radio_stations(&self) -> &Vec<TrackInfo> {
    &self.radio_stations
  }

  #[cfg_attr(not(feature = "internet-radio"), allow(dead_code))]
  pub(crate) fn radio_stations_mut(&mut self) -> &mut Vec<TrackInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.radio_stations
  }

  pub(crate) fn youtube_playlists(&self) -> &Vec<PlaylistInfo> {
    &self.youtube_playlists
  }

  #[cfg_attr(not(feature = "youtube"), allow(dead_code))]
  pub(crate) fn youtube_playlists_mut(&mut self) -> &mut Vec<PlaylistInfo> {
    self.display_revisions.bump(DisplayDomain::Library);
    &mut self.youtube_playlists
  }
}

impl App {
  pub fn clear_playlist_track_dialog_state(&mut self) {
    self.pending_playlist_track_add = None;
    self.pending_playlist_track_removal = None;
    self.view.playlist_picker_selected_index = 0;
    self.view.playlist_picker_folder_id = 0;
    self.view.playlist_picker_filter.clear();
    self.view.playlist_picker_filter_editing = false;
  }

  pub fn clear_dialog_state(&mut self) {
    self.view.dialog = None;
    self.view.confirm = false;
    self.pending_keybinding_persist = None;
    self.pending_playlist_sync_master = None;
    self.pending_playlist_sync_remove = None;
    self.view.playlist_sync_picker_index = 0;
    self.clear_playlist_track_dialog_state();
  }

  /// Reset the create-playlist form; the caller pops the frame.
  pub fn clear_create_playlist_form_state(&mut self) {
    self.view.create_playlist_name = Vec::new();
    self.view.create_playlist_name_idx = 0;
    self.view.create_playlist_name_cursor = 0;
    self.view.create_playlist_stage = CreatePlaylistStage::Name;
    self.create_playlist_tracks = Vec::new();
    self.create_playlist_search_results = Vec::new();
    self.view.create_playlist_search_input = Vec::new();
    self.view.create_playlist_search_idx = 0;
    self.view.create_playlist_search_cursor = 0;
    self.view.create_playlist_selected_result = 0;
    self.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
  }

  pub fn playlist_is_editable(&self, playlist: &PlaylistInfo) -> bool {
    let Some(user) = &self.user else {
      return false;
    };

    playlist.owner_id.as_deref() == Some(user.id.as_str()) || playlist.collaborative
  }

  pub fn editable_playlists(&self) -> Vec<&PlaylistInfo> {
    self
      .all_playlists
      .iter()
      .filter(|playlist| self.playlist_is_editable(playlist))
      .collect()
  }

  /// The rows offered by the add-track picker dialog for the active source:
  /// local YouTube playlists under YouTube (flat), otherwise the user's
  /// editable Spotify playlists plus folder rows scoped to
  /// `playlist_picker_folder_id`, mirroring the sidebar's folder navigation.
  /// A non-empty `playlist_picker_filter` replaces the folder view with a flat
  /// list of the matching destinations from every folder.
  pub fn playlist_picker_items(&self) -> Vec<PlaylistPickerRow<'_>> {
    let terms: Vec<&str> = self
      .view
      .playlist_picker_filter
      .split_whitespace()
      .collect();
    if !terms.is_empty() {
      let candidates = if self.active_source == Source::YouTube {
        self.youtube_playlists.iter().collect()
      } else {
        self.editable_playlists()
      };
      return candidates
        .into_iter()
        .filter(|playlist| playlist_name_matches(&playlist.name, &terms))
        .map(PlaylistPickerRow::Playlist)
        .collect();
    }

    if self.active_source == Source::YouTube {
      return self
        .youtube_playlists
        .iter()
        .map(PlaylistPickerRow::Playlist)
        .collect();
    }

    // Fallback: folder items never built (rootlist fetch failed, streaming
    // disabled, …) — flat editable list, same as the pre-folder behavior.
    if self.playlist_folder_items.is_empty() {
      return self
        .editable_playlists()
        .into_iter()
        .map(PlaylistPickerRow::Playlist)
        .collect();
    }

    let mut rows: Vec<PlaylistPickerRow> = self
      .playlist_folder_items
      .iter()
      .filter_map(|item| match item {
        PlaylistFolderItem::Folder(f) if f.current_id == self.view.playlist_picker_folder_id => {
          Some(PlaylistPickerRow::Folder(f))
        }
        PlaylistFolderItem::Playlist { index, current_id }
          if *current_id == self.view.playlist_picker_folder_id =>
        {
          self
            .all_playlists
            .get(*index)
            .filter(|playlist| self.playlist_is_editable(playlist))
            .map(PlaylistPickerRow::Playlist)
        }
        _ => None,
      })
      .collect();
    if self.user_config.behavior.group_folders_first {
      rows.sort_by_key(|row| !matches!(row, PlaylistPickerRow::Folder(_)));
    }
    rows
  }

  pub fn begin_add_track_to_playlist_flow(&mut self, track_id: Option<String>, track_name: String) {
    let Some(track_id) = track_id else {
      self.set_status_message(self.strings().status.app.track_cannot_add.to_string(), 4);
      return;
    };

    // Under the YouTube source the destinations are the *local* playlists
    // (youtube_playlists.yml), not the Spotify ones — no user/playlist
    // fetches apply. The picker's Enter routes by source too.
    if self.active_source == Source::YouTube {
      if self.youtube_playlists.is_empty() {
        // Kick a (re)load in case the file changed on disk; if it is
        // genuinely empty the user needs to create a playlist first.
        self.dispatch(IoEvent::GetYouTubePlaylists);
        self.set_status_message(
          self.strings().status.app.youtube_no_playlists.to_string(),
          4,
        );
        return;
      }
      self.clear_dialog_state();
      self.pending_playlist_track_add = Some(PendingPlaylistTrackAdd {
        track_id,
        track_name,
      });
      self.push_navigation_stack(
        RouteId::Dialog,
        ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker),
      );
      return;
    }

    let mut requested_data = false;
    if self.user.is_none() {
      self.dispatch(IoEvent::GetUser);
      requested_data = true;
    }
    if self.playlists.is_none() {
      self.dispatch(IoEvent::GetPlaylists);
      requested_data = true;
    }
    if requested_data {
      self.set_status_message(
        self
          .strings()
          .status
          .app
          .playlist_destinations_loading
          .to_string(),
        4,
      );
      return;
    }

    if self.editable_playlists().is_empty() {
      self.set_status_message(
        self.strings().status.app.no_editable_playlists.to_string(),
        4,
      );
      return;
    }

    self.clear_dialog_state();
    self.pending_playlist_track_add = Some(PendingPlaylistTrackAdd {
      track_id,
      track_name,
    });
    self.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker),
    );
  }

  /// Resolve the track table's current selection and open the
  /// add-to-playlist picker for it. Silent no-op when no row is selected,
  /// exactly like the `w` key that has always driven it.
  /// Add a track to a playlist, routed by the playlist URI's scheme.
  pub(crate) fn add_track_to_playlist(&mut self, playlist: String, track: String) {
    if playlist.starts_with(YOUTUBE_PLAYLIST_PREFIX) {
      self.dispatch(IoEvent::AddTrackToYouTubePlaylist(playlist, track));
    } else {
      self.dispatch(IoEvent::AddTrackToPlaylist(playlist, track));
    }
  }

  /// Remove a track from a playlist, routed by the playlist URI's scheme.
  pub(crate) fn remove_track_from_playlist(
    &mut self,
    playlist: String,
    track: String,
    position: usize,
  ) {
    if playlist.starts_with(YOUTUBE_PLAYLIST_PREFIX) {
      self.dispatch(IoEvent::RemoveTrackFromYouTubePlaylist(playlist, track));
    } else {
      self.dispatch(IoEvent::RemoveTrackFromPlaylistAtPosition(
        playlist, track, position,
      ));
    }
  }

  pub fn begin_add_track_to_playlist_flow_from_selection(&mut self) {
    let Some(track) = self.track_table.tracks.get(self.view.track_table_index) else {
      return;
    };
    let track_id = track.id.clone();
    let track_name = track.name.clone();
    self.begin_add_track_to_playlist_flow(track_id, track_name);
  }

  /// Open the add-to-playlist picker for the item playing now, through the
  /// ownership order. A queued Spotify track stages the slot's own track.
  pub fn begin_add_playing_track_to_playlist_flow(&mut self) {
    match self.playing_item() {
      PlayingItem::Spotify(PlayableItem::Track(track)) => {
        let track_id = track.id.as_ref().map(|id| id.uri());
        let name = track.name.clone();
        self.begin_add_track_to_playlist_flow(track_id, name);
      }
      PlayingItem::Spotify(PlayableItem::Episode(_)) => {
        self.set_status_message(
          self.strings().status.app.playlist_tracks_only.to_string(),
          4,
        );
      }
      PlayingItem::Spotify(_) => {}
      PlayingItem::QueuedSpotify(track) => {
        let track_id = track.uri.clone();
        let name = track.name.clone();
        self.begin_add_track_to_playlist_flow(track_id, name);
      }
      PlayingItem::NotSpotify => {
        self.set_status_message(self.strings().status.app.playlist_needs_spotify, 4);
      }
      PlayingItem::Nothing => {
        self.set_status_message(self.strings().status.app.no_current_track.to_string(), 4);
      }
    }
  }

  /// Stage a remove-track-from-playlist confirmation for the track table's
  /// current selection and push the confirm dialog. Body moved verbatim
  /// from the track-table handler's `open_remove_from_playlist_dialog`.
  pub fn begin_remove_track_from_playlist_flow(&mut self) {
    // Local YouTube playlist: same confirm dialog, routed (by the
    // `youtube:playlist:` prefix in the pending target) to the local file edit
    // instead of the Spotify API. No snapshot position — removal is by video id.
    if self.track_table.context == Some(TrackTableContext::YouTubePlaylist) {
      let Some(playlist_uri) = self.youtube_open_playlist.clone() else {
        self.set_status_message(
          self
            .strings()
            .status
            .app
            .youtube_no_open_playlist
            .to_string(),
          4,
        );
        return;
      };
      let playlist_name = self
        .youtube_playlists
        .iter()
        .find(|p| p.uri == playlist_uri)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| self.strings().status.app.youtube_playlist.to_string());
      let Some(track) = self.track_table.tracks.get(self.view.track_table_index) else {
        return;
      };
      let Some(track_id) = track.id.clone() else {
        self.set_status_message(self.strings().status.app.track_cannot_edit.to_string(), 4);
        return;
      };
      let track_name = track.name.clone();
      self.clear_dialog_state();
      self.pending_playlist_track_removal = Some(PendingPlaylistTrackRemoval {
        playlist_id: playlist_uri,
        playlist_name,
        track_id,
        track_name,
        position: 0, // unused for local YouTube playlists
      });
      self.push_navigation_stack(
        RouteId::Dialog,
        ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm),
      );
      return;
    }

    let playlist_context = match self.current_playlist_removal_target() {
      Some(context) => context,
      None => {
        self.set_status_message(
          self
            .strings()
            .status
            .app
            .remove_requires_playlist
            .to_string(),
          4,
        );
        return;
      }
    };

    let track = match self.track_table.tracks.get(self.view.track_table_index) {
      Some(track) => track,
      None => return,
    };

    let track_id = match track.id.clone() {
      Some(id) => id,
      None => {
        self.set_status_message(self.strings().status.app.track_cannot_edit.to_string(), 4);
        return;
      }
    };
    let track_name = track.name.clone();

    let position = match self
      .playlist_track_positions
      .as_ref()
      .and_then(|positions| positions.get(self.view.track_table_index))
      .copied()
    {
      Some(position) => position,
      None => {
        self.set_status_message(
          self
            .strings()
            .status
            .app
            .remove_position_unknown
            .to_string(),
          4,
        );
        return;
      }
    };

    self.clear_dialog_state();
    self.pending_playlist_track_removal = Some(PendingPlaylistTrackRemoval {
      playlist_id: playlist_context.0,
      playlist_name: playlist_context.1,
      track_id,
      track_name,
      position,
    });
    self.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm),
    );
  }

  /// The active playlist's (base62 id, name) pair for the removal flow.
  fn current_playlist_removal_target(&self) -> Option<(String, String)> {
    let playlist_id = self.current_playlist_track_table_id()?;
    let playlist_id = playlist_id.id().to_string();
    let playlist_name = self
      .all_playlists
      .iter()
      .find(|playlist| playlist.id.as_deref() == Some(playlist_id.as_str()))
      .map(|playlist| playlist.name.clone())
      .or_else(|| {
        self
          .search_results
          .playlists
          .as_ref()
          .and_then(|playlists| {
            playlists
              .items
              .iter()
              .find(|playlist| playlist.id.as_deref() == Some(playlist_id.as_str()))
          })
          .map(|playlist| playlist.name.clone())
      })?;
    Some((playlist_id, playlist_name))
  }

  /// The sidebar-selected playlist's id for the unfollow confirm; `None` on a
  /// folder or pin row.
  pub fn selected_sidebar_playlist_id(&self) -> Option<String> {
    let selected_index = self.view.selected_playlist_index?;
    // Row 0 is the "+ Add Playlist" entry; display items start at row 1.
    let display_index = selected_index.checked_sub(1)?;
    let index = match self.get_playlist_display_item_at(display_index)? {
      PlaylistFolderItem::Playlist { index, .. } => *index,
      // A folder row or the community pin is not an unfollowable playlist.
      PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => return None,
    };
    self
      .all_playlists
      .get(index)
      .and_then(|playlist| playlist.id.clone())
  }

  /// The highlighted sidebar playlist as a sync endpoint, for the active source.
  pub fn selected_sidebar_playlist_endpoint(&self) -> Option<crate::core::playlist_sync::Endpoint> {
    let index = self.view.selected_playlist_index?;
    let playlist = match self.active_source {
      Source::Spotify => {
        let display_index = index.checked_sub(1)?;
        match self.get_playlist_display_item_at(display_index)? {
          PlaylistFolderItem::Playlist { index, .. } => self.all_playlists.get(*index)?,
          PlaylistFolderItem::Folder(_) | PlaylistFolderItem::CommunityPin => return None,
        }
      }
      // The Qobuz sidebar also lists the favorites row and favorite albums.
      Source::Qobuz => self
        .qobuz_playlists
        .get(index)
        .filter(|playlist| playlist.uri.starts_with("qobuz:playlist:"))?,
      Source::Subsonic => self.subsonic_playlists.get(index)?,
      Source::YouTube => self.youtube_playlists.get(index)?,
      Source::Local | Source::Radio | Source::AppleMusic => return None,
    };
    Some(crate::core::playlist_sync::Endpoint {
      source: self.active_source,
      playlist_uri: playlist.uri.clone(),
      name: playlist.name.clone(),
    })
  }

  /// The highlighted search-result playlist's Spotify id.
  pub fn selected_search_result_playlist_id(&self) -> Option<String> {
    let playlists = self.search_results.playlists.as_ref()?;
    let selected_index = self.view.search_selected_playlists_index?;
    playlists.items.get(selected_index)?.id.clone()
  }

  pub(crate) fn try_search_playlist_tracks_from_cached_pages(
    &mut self,
    playlist_id: &PlaylistId<'_>,
    query: &str,
  ) -> bool {
    if !self.is_playlist_track_table_active_for(playlist_id)
      || !self.playlist_track_pages.is_complete()
    {
      return false;
    }

    // Must be set so `playlist_track_search_is_current` passes
    self.pending_playlist_track_search = Some(query.to_string());

    let terms: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

    let mut matches = Vec::new();
    for page in &self.playlist_track_pages.pages {
      for (position, item) in &page.items {
        if let PlayableInfo::Track(track) = item {
          if playlist_track_info_matches_terms(track, &terms) {
            matches.push((track.clone(), *position as usize));
          }
        }
      }
    }

    self.apply_playlist_track_search_results(playlist_id, query.to_string(), matches)
  }

  pub(crate) fn try_sort_playlist_tracks_from_cached_pages(
    &mut self,
    playlist_id: &PlaylistId<'_>,
  ) -> bool {
    if !self.is_playlist_track_table_active_for(playlist_id)
      || !self.playlist_track_pages.is_complete()
    {
      return false;
    }

    let mut all_tracks: Vec<TrackInfo> = Vec::new();
    for page in &self.playlist_track_pages.pages {
      for (_, item) in &page.items {
        if let PlayableInfo::Track(track) = item {
          all_tracks.push(track.clone());
        }
      }
    }

    Sorter::new(self.playlist_sort).sort_tracks(&mut all_tracks);
    let _ = self.apply_sorted_playlist_tracks_if_current(playlist_id, all_tracks);
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(feature = "streaming")]
  #[test]
  fn add_playing_track_stages_the_queued_track_not_the_suspended_one() {
    use crate::core::app::test_support::*;
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.active_source = Source::YouTube;
    app.youtube_playlists = vec![PlaylistInfo {
      uri: "youtube:playlist:y1".to_string(),
      ..playlist_info("y1", "Local List", "owner", false)
    }];
    app.current_playback_context = Some(playing_track_context(full_track(
      "0000000000000000000001",
      "Suspended",
    )));
    app.queue_now = Some(crate::infra::queue::QueueNowPlaying::Spotify {
      track: queue_track(Some("spotify:track:queued"), "Queued"),
    });

    app.begin_add_playing_track_to_playlist_flow();

    let pending = app
      .pending_playlist_track_add
      .as_ref()
      .expect("staged for the picker");
    assert_eq!(pending.track_id, "spotify:track:queued");
    assert_eq!(pending.track_name, "Queued");
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn editable_playlists_include_owned_and_collaborative_only() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.all_playlists = vec![
      playlist_info("37i9dQZF1DXcBWIGoYBM5M", "Owned", "spotatui-owner", false),
      playlist_info(
        "37i9dQZF1DX4WYpdgoIcn6",
        "Collaborative",
        "friend-owner",
        true,
      ),
      playlist_info("37i9dQZF1DWZqd5JICZI0u", "Followed", "friend-owner", false),
    ];

    let editable_names = app
      .editable_playlists()
      .into_iter()
      .map(|playlist| playlist.name.clone())
      .collect::<Vec<_>>();

    assert_eq!(editable_names, vec!["Owned", "Collaborative"]);
  }

  #[test]
  fn begin_add_track_to_playlist_flow_requires_editable_playlist() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.playlists = Some(Paged {
      total: 1,
      ..Default::default()
    });
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DWZqd5JICZI0u",
      "Followed",
      "friend-owner",
      false,
    )];

    app.begin_add_track_to_playlist_flow(
      Some("0000000000000000000001".to_string()),
      "Track".to_string(),
    );

    assert_eq!(
      app.status_message.as_deref(),
      Some("No editable playlists available")
    );
    assert!(app.pending_playlist_track_add.is_none());
  }

  /// A sidebar with the "+ Add Playlist" row, then one folder, then one
  /// playlist row (both visible at the root level).
  fn app_with_sidebar_playlist() -> App {
    // Read-only resolvers: no channel needed, nothing dispatches.
    let mut app = App::default();
    app.user_config.behavior.pin_community_playlist = false;
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Owned",
      "spotatui-owner",
      false,
    )];
    app.playlist_folder_items = vec![
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
        name: "Mixes".to_string(),
        current_id: 0,
        target_id: 1,
      }),
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 0,
      },
    ];
    app
  }

  #[test]
  fn selected_sidebar_playlist_id_resolves_the_row_below_add_playlist() {
    let mut app = app_with_sidebar_playlist();
    app.user = Some(user_info("spotatui-owner"));
    // Row 0 is "+ Add Playlist", row 1 the folder, row 2 the playlist.
    app.view.selected_playlist_index = Some(2);

    assert_eq!(
      app.selected_sidebar_playlist_id().as_deref(),
      Some("37i9dQZF1DXcBWIGoYBM5M")
    );
  }

  #[test]
  fn selected_sidebar_playlist_id_skips_rows_that_are_not_playlists() {
    let mut app = app_with_sidebar_playlist();
    app.user = Some(user_info("spotatui-owner"));

    app.view.selected_playlist_index = Some(0);
    assert_eq!(app.selected_sidebar_playlist_id(), None, "+ Add Playlist");
    app.view.selected_playlist_index = Some(1);
    assert_eq!(app.selected_sidebar_playlist_id(), None, "a folder row");
    app.view.selected_playlist_index = None;
    assert_eq!(app.selected_sidebar_playlist_id(), None, "no selection");
  }

  #[test]
  fn selected_search_result_playlist_id_resolves_the_selection() {
    let mut app = App::default();
    app.search_results.playlists = Some(Paged {
      items: vec![
        playlist_info("37i9dQZF1DWZqd5JICZI0u", "First", "friend-owner", false),
        playlist_info("37i9dQZF1DXcBWIGoYBM5M", "Second", "friend-owner", false),
      ],
      total: 2,
      ..Default::default()
    });
    app.view.search_selected_playlists_index = Some(1);

    assert_eq!(
      app.selected_search_result_playlist_id().as_deref(),
      Some("37i9dQZF1DXcBWIGoYBM5M")
    );
  }

  #[test]
  fn playlist_track_matcher_matches_case_insensitively_across_fields() {
    let track = TrackInfo {
      generated_name: None,
      name: "Never Gonna Give You Up".into(),
      artists: vec!["Rick Astley".into()],
      album: "Whenever You Need Somebody".into(),
      duration_ms: 213_000,
      id: Some("rick123".into()),
      uri: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 1,
      explicit: false,
      image_url: None,
      release_date: None,
    };

    // Match on track title
    let terms = vec!["never".to_string(), "gonna".to_string()];
    assert!(playlist_track_info_matches_terms(&track, &terms));

    // Match across artist and album
    let terms = vec!["astley".to_string(), "whenever".to_string()];
    assert!(playlist_track_info_matches_terms(&track, &terms));

    // Miss on nonexistent term
    let terms = vec!["astley".to_string(), "metallica".to_string()];
    assert!(!playlist_track_info_matches_terms(&track, &terms));
  }

  #[test]
  fn try_search_and_sort_return_false_when_pages_are_incomplete() {
    let mut app = App::default();
    let playlist_id = PlaylistId::from_id("wifwifwifwifwfwif").unwrap();

    // App is neither active for this playlist nor has pages
    assert!(!app.try_search_playlist_tracks_from_cached_pages(&playlist_id, "rickroll"));
    assert!(!app.try_sort_playlist_tracks_from_cached_pages(&playlist_id));
  }

  #[test]
  fn try_search_and_sort_succeed_when_pages_are_complete() {
    let mut app = App::default();
    let playlist_id = PlaylistId::from_id("OwO").unwrap();

    // Set the context and the active playlist id
    app.track_table.context = Some(TrackTableContext::MyPlaylists);
    app.playlist_track_table_id = Some(playlist_id.clone().into_static());

    // Insert a complete page with 2 tracks (terminal page: next is None)
    let track1 = TrackInfo {
      generated_name: None,
      name: "Never Gonna Give You Up".into(),
      artists: vec!["Rick Astley".into()],
      album: "Whenever You Need Somebody".into(),
      duration_ms: 213_000,
      id: Some("1".into()),
      uri: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 1,
      explicit: false,
      image_url: None,
      release_date: None,
    };

    let track2 = TrackInfo {
      generated_name: None,
      name: "Together Forever".into(),
      artists: vec!["Rick Astley".into()],
      album: "Whenever You Need Somebody".into(),
      duration_ms: 200_000,
      id: Some("2".into()),
      uri: None,
      album_id: None,
      artist_refs: vec![],
      is_playable: true,
      is_local: false,
      track_number: 2,
      explicit: false,
      image_url: None,
      release_date: None,
    };

    app.playlist_track_pages.upsert_page_by_offset(Paged {
      items: vec![
        (0, PlayableInfo::Track(track1)),
        (1, PlayableInfo::Track(track2)),
      ],
      offset: 0,
      limit: 50,
      total: 2,
      next: None,
      previous: None,
    });

    // Test happy-path in-memory search
    assert!(app.try_search_playlist_tracks_from_cached_pages(&playlist_id, "Together"));
    assert_eq!(
      app.active_playlist_track_filter.as_deref(),
      Some("Together")
    );
    assert_eq!(app.track_table.tracks.len(), 1);

    // Test happy-path in-memory sort
    assert!(app.try_sort_playlist_tracks_from_cached_pages(&playlist_id));
    assert_eq!(app.track_table.tracks.len(), 2);
  }
}

pub fn playlist_track_info_matches_terms(track: &TrackInfo, terms: &[String]) -> bool {
  let haystack =
    format!("{} {} {}", track.name, track.album, track.artists.join(" "),).to_lowercase();
  terms.iter().all(|term| haystack.contains(term))
}
