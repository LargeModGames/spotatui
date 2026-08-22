use super::*;

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
  pub fn begin_add_track_to_playlist_flow_from_selection(&mut self) {
    let Some(track) = self.track_table.tracks.get(self.track_table.selected_index) else {
      return;
    };
    let track_id = track.id.clone();
    let track_name = track.name.clone();
    self.begin_add_track_to_playlist_flow(track_id, track_name);
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
      let Some(track) = self.track_table.tracks.get(self.track_table.selected_index) else {
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

    let track = match self.track_table.tracks.get(self.track_table.selected_index) {
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
      .and_then(|positions| positions.get(self.track_table.selected_index))
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

  pub fn user_follow_playlist(&mut self) {
    info!("following playlist");
    if let SearchResult {
      playlists: Some(ref playlists),
      selected_playlists_index: Some(selected_index),
      ..
    } = self.search_results
    {
      let Some(selected_playlist) = playlists.items.get(selected_index) else {
        return;
      };
      let selected_public = selected_playlist.public;
      if let Some(ref playlist_id_str) = selected_playlist.id {
        // owner_id carries the Spotify user id (populated in PlaylistInfo::from_simplified).
        // The network handler ignores this param (_playlist_owner_id), so a fallback
        // string is harmless — but we use the real id when available.
        let owner_id = selected_playlist
          .owner_id
          .clone()
          .unwrap_or_else(|| "unknown".to_string());
        self.dispatch(IoEvent::UserFollowPlaylist(
          owner_id,
          playlist_id_str.clone(),
          selected_public,
        ));
      }
    }
  }

  pub fn user_unfollow_playlist(&mut self) {
    info!("unfollowing playlist");
    if let (Some(selected_index), Some(user)) = (self.view.selected_playlist_index, &self.user) {
      // Row 0 is the "+ Add Playlist" entry; display items start at row 1.
      if let Some(PlaylistFolderItem::Playlist { index, .. }) = selected_index
        .checked_sub(1)
        .and_then(|i| self.get_playlist_display_item_at(i))
      {
        // Pass the stored string ids straight through to the IoEvent.
        let ids = self.all_playlists.get(*index).and_then(|playlist| {
          let selected_id = playlist.id.clone()?;
          Some((user.id.clone(), selected_id))
        });
        if let Some((user_id, selected_id)) = ids {
          self.dispatch(IoEvent::UserUnfollowPlaylist(user_id, selected_id));
        }
      }
    }
  }

  pub fn user_unfollow_playlist_search_result(&mut self) {
    info!("unfollowing playlist from search results");
    if let (Some(playlists), Some(selected_index), Some(user)) = (
      &self.search_results.playlists,
      self.search_results.selected_playlists_index,
      &self.user,
    ) {
      let Some(selected_playlist) = playlists.items.get(selected_index) else {
        return;
      };
      // `user.id` is the domain string id (UserInfo) and `selected_playlist.id`
      // is an Option<String> (PlaylistInfo); both pass straight to the IoEvent.
      if let Some(ref id_str) = selected_playlist.id {
        self.dispatch(IoEvent::UserUnfollowPlaylist(
          user.id.clone(),
          id_str.clone(),
        ));
      }
    }
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
}
