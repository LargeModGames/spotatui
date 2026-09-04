use super::*;

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
  pub fn clear_playlist_track_dialog_state(&mut self) {
    self.pending_playlist_track_add = None;
    self.pending_playlist_track_removal = None;
    self.view.playlist_picker_selected_index = 0;
    self.view.playlist_picker_folder_id = 0;
  }

  pub fn clear_dialog_state(&mut self) {
    self.view.dialog = None;
    self.view.confirm = false;
    self.pending_keybinding_persist = None;
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
  pub fn playlist_picker_items(&self) -> Vec<PlaylistPickerRow<'_>> {
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
      self.set_status_message("Track cannot be added to playlist".to_string(), 4);
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
          "No YouTube playlists yet — create one from the sidebar".to_string(),
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
      self.set_status_message("Playlist destinations loading, try again".to_string(), 4);
      return;
    }

    if self.editable_playlists().is_empty() {
      self.set_status_message("No editable playlists available".to_string(), 4);
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

  /// Open the add-to-playlist picker for the item playing now. Reads only
  /// `current_playback_context`, so a native queue slot stages the suspended
  /// context's track.
  pub fn begin_add_playing_track_to_playlist_flow(&mut self) {
    match self
      .current_playback_context
      .as_ref()
      .and_then(|context| context.item.as_ref())
    {
      Some(PlayableItem::Track(track)) => {
        let track_id = track.id.as_ref().map(|id| id.uri());
        let name = track.name.clone();
        self.begin_add_track_to_playlist_flow(track_id, name);
      }
      Some(PlayableItem::Episode(_)) => {
        self.set_status_message("Only tracks can be added to playlists".to_string(), 4);
      }
      Some(_) => {}
      None => {
        self.set_status_message("No track currently playing".to_string(), 4);
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
        self.set_status_message("No YouTube playlist is open".to_string(), 4);
        return;
      };
      let playlist_name = self
        .youtube_playlists
        .iter()
        .find(|p| p.uri == playlist_uri)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "YouTube playlist".to_string());
      let Some(track) = self.track_table.tracks.get(self.view.track_table_index) else {
        return;
      };
      let Some(track_id) = track.id.clone() else {
        self.set_status_message("Track cannot be edited in playlist".to_string(), 4);
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
          "Remove only works in selected playlist views".to_string(),
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
        self.set_status_message("Track cannot be edited in playlist".to_string(), 4);
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
        self.set_status_message("Cannot resolve track position for removal".to_string(), 4);
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

  /// The highlighted search-result playlist's Spotify id.
  pub fn selected_search_result_playlist_id(&self) -> Option<String> {
    let playlists = self.search_results.playlists.as_ref()?;
    let selected_index = self.view.search_selected_playlists_index?;
    playlists.items.get(selected_index)?.id.clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
}
