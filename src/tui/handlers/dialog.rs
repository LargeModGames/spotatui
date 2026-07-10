use super::common_key_events;
use crate::core::app::{ActiveBlock, App, DialogContext, PlaylistPickerRow};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  let dialog_context = match app.get_current_route().active_block {
    ActiveBlock::Dialog(context) => context,
    _ => return,
  };

  match dialog_context {
    DialogContext::AddTrackToPlaylistPicker => handle_add_to_playlist_picker(key, app),
    DialogContext::PlaylistWindow
    | DialogContext::PlaylistSearch
    | DialogContext::RemoveTrackFromPlaylistConfirm
    | DialogContext::PersistKeybindingFallback
    | DialogContext::YouTubePlaylistWindow => handle_confirmation_dialog(key, app, dialog_context),
  }
}

fn handle_confirmation_dialog(key: Key, app: &mut App, dialog_context: DialogContext) {
  match key {
    Key::Enter => {
      if app.confirm {
        match dialog_context {
          DialogContext::PlaylistWindow => handle_playlist_dialog(app),
          DialogContext::PlaylistSearch => handle_playlist_search_dialog(app),
          DialogContext::RemoveTrackFromPlaylistConfirm => {
            handle_remove_track_from_playlist_confirm(app);
          }
          DialogContext::PersistKeybindingFallback => {
            app.persist_open_settings_fallback();
          }
          DialogContext::YouTubePlaylistWindow => handle_youtube_playlist_dialog(app),
          DialogContext::AddTrackToPlaylistPicker => {}
        }
      } else if dialog_context == DialogContext::PersistKeybindingFallback {
        app.set_status_message("Using Alt+, for this session only", 4);
      }
      close_dialog(app);
    }
    Key::Char('q') => {
      if dialog_context == DialogContext::PersistKeybindingFallback {
        app.set_status_message("Using Alt+, for this session only", 4);
      }
      close_dialog(app);
    }
    k if common_key_events::right_event(k, &app.user_config.keys) => app.confirm = !app.confirm,
    k if common_key_events::left_event(k, &app.user_config.keys) => app.confirm = !app.confirm,
    _ => {}
  }
}

fn handle_add_to_playlist_picker(key: Key, app: &mut App) {
  // Rows follow the active source: local YouTube playlists under YouTube,
  // editable Spotify playlists plus navigable folder rows otherwise.
  let picker_rows = app.playlist_picker_items();
  let row_count = picker_rows.len();
  match key {
    k if common_key_events::down_event(k, &app.user_config.keys) && row_count > 0 => {
      let next = common_key_events::on_down_press_handler(
        &picker_rows,
        Some(app.playlist_picker_selected_index),
      );
      app.playlist_picker_selected_index = next;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) && row_count > 0 => {
      let next = common_key_events::on_up_press_handler(
        &picker_rows,
        Some(app.playlist_picker_selected_index),
      );
      app.playlist_picker_selected_index = next;
    }
    k if common_key_events::high_event(k) && row_count > 0 => {
      app.playlist_picker_selected_index = common_key_events::on_high_press_handler();
    }
    k if common_key_events::middle_event(k) && row_count > 0 => {
      app.playlist_picker_selected_index = common_key_events::on_middle_press_handler(&picker_rows);
    }
    k if common_key_events::low_event(k) && row_count > 0 => {
      app.playlist_picker_selected_index = common_key_events::on_low_press_handler(&picker_rows);
    }
    Key::Enter => {
      let selected = app
        .playlist_picker_selected_index
        .min(row_count.saturating_sub(1));
      match picker_rows.get(selected) {
        // Descend into (or back out of) the folder; the dialog stays open,
        // same semantics as Enter in the sidebar (handlers/playlist.rs).
        Some(PlaylistPickerRow::Folder(folder)) => {
          let target_id = folder.target_id;
          app.playlist_picker_folder_id = target_id;
          app.playlist_picker_selected_index = 0;
        }
        Some(PlaylistPickerRow::Playlist(playlist)) => {
          let playlist_id = playlist.id.clone();
          if let (Some(playlist_id), Some(pending_add)) =
            (playlist_id, app.pending_playlist_track_add.clone())
          {
            if app.active_source == crate::core::source::Source::YouTube {
              app.dispatch(IoEvent::AddTrackToYouTubePlaylist(
                playlist_id,
                pending_add.track_id,
              ));
            } else {
              app.dispatch(IoEvent::AddTrackToPlaylist(
                playlist_id,
                pending_add.track_id,
              ));
            }
          }
          close_dialog(app);
        }
        None => close_dialog(app),
      }
    }
    Key::Char('q') => {
      close_dialog(app);
    }
    _ => {}
  }
}

fn handle_playlist_dialog(app: &mut App) {
  app.user_unfollow_playlist()
}

fn handle_playlist_search_dialog(app: &mut App) {
  app.user_unfollow_playlist_search_result()
}

/// Confirmed deletion of the sidebar-selected local YouTube playlist.
fn handle_youtube_playlist_dialog(app: &mut App) {
  let uri = app
    .selected_playlist_index
    .and_then(|idx| app.youtube_playlists.get(idx))
    .map(|playlist| playlist.uri.clone());
  if let Some(uri) = uri {
    app.dispatch(IoEvent::DeleteYouTubePlaylist(uri));
  }
}

fn handle_remove_track_from_playlist_confirm(app: &mut App) {
  if let Some(pending_remove) = app.pending_playlist_track_removal.clone() {
    // A `youtube:playlist:` target is a local YouTube playlist edit; anything
    // else is the Spotify remove (which needs the snapshot position).
    if pending_remove.playlist_id.starts_with("youtube:playlist:") {
      app.dispatch(IoEvent::RemoveTrackFromYouTubePlaylist(
        pending_remove.playlist_id,
        pending_remove.track_id,
      ));
    } else {
      app.dispatch(IoEvent::RemoveTrackFromPlaylistAtPosition(
        pending_remove.playlist_id,
        pending_remove.track_id,
        pending_remove.position,
      ));
    }
  }
}

fn close_dialog(app: &mut App) {
  app.pop_navigation_stack();
  app.clear_dialog_state();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    app::{PendingPlaylistTrackAdd, PlaylistFolder, PlaylistFolderItem, RouteId},
    pagination::Paged,
    test_helpers::{playlist_info, user_info},
    user_config::UserConfig,
  };
  use std::{sync::mpsc::channel, time::SystemTime};

  #[test]
  fn confirmation_dialog_toggles_with_vim_hl() {
    let mut app = App::default();
    app.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm),
    );
    app.confirm = false;

    handler(Key::Char('l'), &mut app);
    assert!(app.confirm);

    handler(Key::Char('h'), &mut app);
    assert!(!app.confirm);
  }

  #[test]
  fn add_to_playlist_picker_dispatches_selected_editable_playlist() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.playlists = Some(Paged {
      total: 3,
      ..Default::default()
    });
    app.all_playlists = vec![
      playlist_info("37i9dQZF1DWZqd5JICZI0u", "Followed", "friend-owner", false),
      playlist_info("37i9dQZF1DXcBWIGoYBM5M", "Owned", "spotatui-owner", false),
      playlist_info(
        "37i9dQZF1DX4WYpdgoIcn6",
        "Collaborative",
        "friend-owner",
        true,
      ),
    ];
    app.pending_playlist_track_add = Some(PendingPlaylistTrackAdd {
      track_id: "0000000000000000000001".to_string(),
      track_name: "Track".to_string(),
    });
    app.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker),
    );
    app.playlist_picker_selected_index = 0;

    handler(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::AddTrackToPlaylist(playlist_id, track_id) => {
        assert_eq!(playlist_id, "37i9dQZF1DXcBWIGoYBM5M");
        assert_eq!(track_id, "0000000000000000000001");
      }
      _ => panic!("expected add-track event"),
    }
  }

  #[test]
  fn add_to_playlist_picker_navigates_folders_and_filters_uneditable() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.all_playlists = vec![
      playlist_info("37i9dQZF1DXcBWIGoYBM5M", "Owned", "spotatui-owner", false),
      playlist_info("37i9dQZF1DWZqd5JICZI0u", "Followed", "friend-owner", false),
    ];
    app.playlist_folder_items = vec![
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: "Mixes".to_string(),
        current_id: 0,
        target_id: 1,
      }),
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: "\u{2190} Mixes".to_string(),
        current_id: 1,
        target_id: 0,
      }),
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 1,
      },
      PlaylistFolderItem::Playlist {
        index: 1,
        current_id: 0,
      },
    ];
    app.pending_playlist_track_add = Some(PendingPlaylistTrackAdd {
      track_id: "0000000000000000000001".to_string(),
      track_name: "Track".to_string(),
    });
    app.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker),
    );

    // Root shows only the folder row: "Followed" is not editable.
    assert_eq!(app.playlist_picker_items().len(), 1);

    // Enter on the folder descends, keeps the dialog open, dispatches nothing.
    handler(Key::Enter, &mut app);
    assert_eq!(app.playlist_picker_folder_id, 1);
    assert_eq!(app.playlist_picker_selected_index, 0);
    assert!(matches!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    ));
    assert!(rx.try_recv().is_err());

    // Inside the folder: back row + the owned playlist. Enter adds the track.
    assert_eq!(app.playlist_picker_items().len(), 2);
    app.playlist_picker_selected_index = 1;
    handler(Key::Enter, &mut app);
    match rx.recv().unwrap() {
      IoEvent::AddTrackToPlaylist(playlist_id, track_id) => {
        assert_eq!(playlist_id, "37i9dQZF1DXcBWIGoYBM5M");
        assert_eq!(track_id, "0000000000000000000001");
      }
      _ => panic!("expected add-track event"),
    }
  }

  #[test]
  fn picker_rows_respect_group_folders_first() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.user = Some(user_info("spotatui-owner"));
    app.all_playlists = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Owned",
      "spotatui-owner",
      false,
    )];
    app.playlist_folder_items = vec![
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 0,
      },
      PlaylistFolderItem::Folder(PlaylistFolder {
        name: "Mixes".to_string(),
        current_id: 0,
        target_id: 1,
      }),
    ];

    assert!(matches!(
      app.playlist_picker_items()[0],
      PlaylistPickerRow::Playlist(_)
    ));
    app.user_config.behavior.group_folders_first = true;
    assert!(matches!(
      app.playlist_picker_items()[0],
      PlaylistPickerRow::Folder(_)
    ));
  }
}
