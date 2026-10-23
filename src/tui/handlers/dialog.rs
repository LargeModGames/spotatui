use super::common_key_events;
use super::filter_input::{self, FilterEdit};
use crate::core::action::Action;
use crate::core::app::{ActiveBlock, App, DialogContext, PlaylistPickerRow, RouteId};
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  let dialog_context = match app.get_current_route().active_block {
    ActiveBlock::Dialog(context) => context,
    _ => return,
  };

  match dialog_context {
    DialogContext::AddTrackToPlaylistPicker => handle_add_to_playlist_picker(key, app),
    DialogContext::PlaylistSyncPicker => handle_playlist_sync_picker(key, app),
    DialogContext::PlaylistWindow
    | DialogContext::PlaylistSearch
    | DialogContext::RemoveTrackFromPlaylistConfirm
    | DialogContext::PersistKeybindingFallback
    | DialogContext::RemovePlaylistSyncLinkConfirm
    | DialogContext::YouTubePlaylistWindow => handle_confirmation_dialog(key, app, dialog_context),
  }
}

fn handle_confirmation_dialog(key: Key, app: &mut App, dialog_context: DialogContext) {
  match key {
    Key::Enter => {
      if app.view.confirm {
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
          DialogContext::RemovePlaylistSyncLinkConfirm => {
            handle_remove_playlist_sync_link_dialog(app);
          }
          DialogContext::AddTrackToPlaylistPicker => {}
          DialogContext::PlaylistSyncPicker => {}
        }
      } else if dialog_context == DialogContext::PersistKeybindingFallback {
        app.set_status_message(app.strings().popups.fallback_session_only, 4);
      }
      close_dialog(app);
    }
    Key::Char('q') => {
      if dialog_context == DialogContext::PersistKeybindingFallback {
        app.set_status_message(app.strings().popups.fallback_session_only, 4);
      }
      close_dialog(app);
    }
    k if common_key_events::right_event(k, &app.user_config.keys) => {
      app.view.confirm = !app.view.confirm
    }
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      app.view.confirm = !app.view.confirm
    }
    _ => {}
  }
}

/// Whether the add-to-playlist picker's search must see `key` before the
/// global bindings and the runner's back key claim it.
pub(crate) fn playlist_picker_captures_key(app: &App, key: Key) -> bool {
  app.get_current_route().active_block
    == ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    && (app.view.playlist_picker_filter_editing
      || key == app.user_config.keys.search
      || (key == Key::Esc && !app.view.playlist_picker_filter.is_empty()))
}

fn clear_playlist_picker_filter(app: &mut App) {
  app.view.playlist_picker_filter.clear();
  app.view.playlist_picker_filter_editing = false;
  app.view.playlist_picker_selected_index = 0;
}

fn handle_playlist_picker_filter_input(key: Key, app: &mut App) {
  let row_count = app.playlist_picker_items().len();
  match key {
    Key::Down if row_count > 0 => {
      app.view.playlist_picker_selected_index =
        (app.view.playlist_picker_selected_index + 1) % row_count;
      return;
    }
    Key::Up if row_count > 0 => {
      app.view.playlist_picker_selected_index = app
        .view
        .playlist_picker_selected_index
        .checked_sub(1)
        .unwrap_or(row_count - 1);
      return;
    }
    _ => {}
  }
  match filter_input::apply(key, &mut app.view.playlist_picker_filter) {
    FilterEdit::Cancel => clear_playlist_picker_filter(app),
    FilterEdit::Confirm => {
      if app.view.playlist_picker_filter.trim().is_empty() {
        clear_playlist_picker_filter(app);
      } else {
        app.view.playlist_picker_filter_editing = false;
      }
    }
    FilterEdit::Changed => app.view.playlist_picker_selected_index = 0,
    FilterEdit::Ignored => {}
  }
}

fn handle_add_to_playlist_picker(key: Key, app: &mut App) {
  if app.view.playlist_picker_filter_editing {
    handle_playlist_picker_filter_input(key, app);
    return;
  }
  if key == app.user_config.keys.search {
    app.view.playlist_picker_filter.clear();
    app.view.playlist_picker_filter_editing = true;
    app.view.playlist_picker_selected_index = 0;
    return;
  }
  if key == Key::Esc && !app.view.playlist_picker_filter.is_empty() {
    clear_playlist_picker_filter(app);
    return;
  }

  // Rows follow the active source: local YouTube playlists under YouTube,
  // editable Spotify playlists plus navigable folder rows otherwise.
  let picker_rows = app.playlist_picker_items();
  let row_count = picker_rows.len();
  match key {
    k if common_key_events::down_event(k, &app.user_config.keys) && row_count > 0 => {
      let next = common_key_events::on_down_press_handler(
        &picker_rows,
        Some(app.view.playlist_picker_selected_index),
      );
      app.view.playlist_picker_selected_index = next;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) && row_count > 0 => {
      let next = common_key_events::on_up_press_handler(
        &picker_rows,
        Some(app.view.playlist_picker_selected_index),
      );
      app.view.playlist_picker_selected_index = next;
    }
    k if common_key_events::high_event(k) && row_count > 0 => {
      app.view.playlist_picker_selected_index = common_key_events::on_high_press_handler();
    }
    k if common_key_events::middle_event(k) && row_count > 0 => {
      app.view.playlist_picker_selected_index =
        common_key_events::on_middle_press_handler(&picker_rows);
    }
    k if common_key_events::low_event(k) && row_count > 0 => {
      app.view.playlist_picker_selected_index =
        common_key_events::on_low_press_handler(&picker_rows);
    }
    Key::Enter => {
      let selected = app
        .view
        .playlist_picker_selected_index
        .min(row_count.saturating_sub(1));
      match picker_rows.get(selected) {
        // Descend into (or back out of) the folder; the dialog stays open,
        // same semantics as Enter in the sidebar (handlers/playlist.rs).
        Some(PlaylistPickerRow::Folder(folder)) => {
          let target_id = folder.target_id;
          app.view.playlist_picker_folder_id = target_id;
          app.view.playlist_picker_selected_index = 0;
        }
        Some(PlaylistPickerRow::Playlist(playlist)) => {
          // `id` stays the presence guard; the payload is the URI, which carries the source.
          let playlist_uri = playlist.id.is_some().then(|| playlist.uri.clone());
          let track = app
            .pending_playlist_track_add
            .as_ref()
            .map(|pending| pending.track_id.clone());
          if let (Some(playlist), Some(track)) = (playlist_uri, track) {
            app.apply(Action::AddTrackToPlaylist { playlist, track });
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

/// The mirror picker: pick the source the highlighted playlist is mirrored onto.
fn handle_playlist_sync_picker(key: Key, app: &mut App) {
  let rows = app.playlist_sync_picker_sources();
  let row_count = rows.len();
  match key {
    k if common_key_events::down_event(k, &app.user_config.keys) && row_count > 0 => {
      app.view.playlist_sync_picker_index =
        common_key_events::on_down_press_handler(&rows, Some(app.view.playlist_sync_picker_index));
    }
    k if common_key_events::up_event(k, &app.user_config.keys) && row_count > 0 => {
      app.view.playlist_sync_picker_index =
        common_key_events::on_up_press_handler(&rows, Some(app.view.playlist_sync_picker_index));
    }
    k if common_key_events::high_event(k) && row_count > 0 => {
      app.view.playlist_sync_picker_index = common_key_events::on_high_press_handler();
    }
    k if common_key_events::middle_event(k) && row_count > 0 => {
      app.view.playlist_sync_picker_index = common_key_events::on_middle_press_handler(&rows);
    }
    k if common_key_events::low_event(k) && row_count > 0 => {
      app.view.playlist_sync_picker_index = common_key_events::on_low_press_handler(&rows);
    }
    Key::Enter => {
      // No clamp: the offered list follows live state, so a stale cursor picks nothing.
      let source = rows.get(app.view.playlist_sync_picker_index).copied();
      if let Some(source) = source {
        app.apply(Action::LinkPlaylistTo(source));
      }
      close_dialog(app);
      // The run's progress shows on the sync screen, so open it.
      if source.is_some() {
        app.push_navigation_stack(RouteId::PlaylistSync, ActiveBlock::PlaylistSync);
      }
    }
    Key::Char('q') => close_dialog(app),
    _ => {}
  }
}

/// Confirmed removal of the highlighted playlist-sync link.
fn handle_remove_playlist_sync_link_dialog(app: &mut App) {
  let id = app.pending_playlist_sync_remove().map(str::to_string);
  if let Some(id) = id {
    app.apply(Action::RemovePlaylistSyncLink(id));
  }
}

fn handle_playlist_dialog(app: &mut App) {
  if let Some(playlist_id) = app.selected_sidebar_playlist_id() {
    app.apply(Action::UnfollowPlaylist(playlist_id));
  }
}

fn handle_playlist_search_dialog(app: &mut App) {
  if let Some(playlist_id) = app.selected_search_result_playlist_id() {
    app.apply(Action::UnfollowPlaylist(playlist_id));
  }
}

/// Confirmed deletion of the sidebar-selected local YouTube playlist.
fn handle_youtube_playlist_dialog(app: &mut App) {
  let uri = app
    .view
    .selected_playlist_index
    .and_then(|idx| app.youtube_playlists().get(idx))
    .map(|playlist| playlist.uri.clone());
  if let Some(uri) = uri {
    app.apply(Action::DeletePlaylist(uri));
  }
}

fn handle_remove_track_from_playlist_confirm(app: &mut App) {
  if let Some(pending_remove) = app.pending_playlist_track_removal.clone() {
    app.apply(Action::RemoveTrackFromPlaylist {
      playlist: pending_remove.playlist_id,
      track: pending_remove.track_id,
      position: pending_remove.position,
    });
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
  use crate::infra::network::IoEvent;
  use std::{sync::mpsc::channel, time::SystemTime};

  #[test]
  fn confirmation_dialog_toggles_with_vim_hl() {
    let mut app = App::default();
    app.push_navigation_stack(
      RouteId::Dialog,
      ActiveBlock::Dialog(DialogContext::RemoveTrackFromPlaylistConfirm),
    );
    app.view.confirm = false;

    handler(Key::Char('l'), &mut app);
    assert!(app.view.confirm);

    handler(Key::Char('h'), &mut app);
    assert!(!app.view.confirm);
  }

  #[test]
  fn add_to_playlist_picker_dispatches_selected_editable_playlist() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    *app.user_mut() = Some(user_info("spotatui-owner"));
    *app.playlists_mut() = Some(Paged {
      total: 3,
      ..Default::default()
    });
    *app.all_playlists_mut() = vec![
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
    app.view.playlist_picker_selected_index = 0;

    handler(Key::Enter, &mut app);

    match rx.recv().unwrap() {
      IoEvent::AddTrackToPlaylist(playlist_id, track_id) => {
        assert_eq!(playlist_id, "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M");
        assert_eq!(track_id, "0000000000000000000001");
      }
      _ => panic!("expected add-track event"),
    }
  }

  #[test]
  fn add_to_playlist_picker_navigates_folders_and_filters_uneditable() {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    *app.user_mut() = Some(user_info("spotatui-owner"));
    *app.all_playlists_mut() = vec![
      playlist_info("37i9dQZF1DXcBWIGoYBM5M", "Owned", "spotatui-owner", false),
      playlist_info("37i9dQZF1DWZqd5JICZI0u", "Followed", "friend-owner", false),
    ];
    *app.playlist_folder_items_mut() = vec![
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
        name: "Mixes".to_string(),
        current_id: 0,
        target_id: 1,
      }),
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
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
    assert_eq!(app.view.playlist_picker_folder_id, 1);
    assert_eq!(app.view.playlist_picker_selected_index, 0);
    assert!(matches!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    ));
    assert!(rx.try_recv().is_err());

    // Inside the folder: back row + the owned playlist. Enter adds the track.
    assert_eq!(app.playlist_picker_items().len(), 2);
    app.view.playlist_picker_selected_index = 1;
    handler(Key::Enter, &mut app);
    match rx.recv().unwrap() {
      IoEvent::AddTrackToPlaylist(playlist_id, track_id) => {
        assert_eq!(playlist_id, "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M");
        assert_eq!(track_id, "0000000000000000000001");
      }
      _ => panic!("expected add-track event"),
    }
  }

  fn picker_with_nested_playlists() -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    *app.user_mut() = Some(user_info("spotatui-owner"));
    *app.all_playlists_mut() = vec![
      playlist_info(
        "37i9dQZF1DXcBWIGoYBM5M",
        "Quiet Mornings",
        "spotatui-owner",
        false,
      ),
      playlist_info(
        "37i9dQZF1DWZqd5JICZI0u",
        "Quiet Followed",
        "friend-owner",
        false,
      ),
      playlist_info(
        "37i9dQZF1DX4WYpdgoIcn6",
        "Road Trip",
        "spotatui-owner",
        false,
      ),
    ];
    *app.playlist_folder_items_mut() = vec![
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
        name: "Mixes".to_string(),
        current_id: 0,
        target_id: 1,
      }),
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 1,
      },
      PlaylistFolderItem::Playlist {
        index: 1,
        current_id: 0,
      },
      PlaylistFolderItem::Playlist {
        index: 2,
        current_id: 0,
      },
    ];
    *app.playlists_mut() = Some(Paged {
      total: 3,
      ..Default::default()
    });
    app.begin_add_track_to_playlist_flow(
      Some("0000000000000000000001".to_string()),
      "Track".to_string(),
    );
    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    );
    (app, rx)
  }

  fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
      handler(Key::Char(c), app);
    }
  }

  #[test]
  fn picker_search_finds_editable_playlists_inside_folders() {
    let (mut app, rx) = picker_with_nested_playlists();

    handler(app.user_config.keys.search, &mut app);
    assert!(app.view.playlist_picker_filter_editing);
    type_text(&mut app, "quiet");

    let rows = app.playlist_picker_items();
    assert_eq!(rows.len(), 1);
    assert!(
      matches!(rows[0], PlaylistPickerRow::Playlist(p) if p.name == "Quiet Mornings"),
      "{rows:?}"
    );

    handler(Key::Enter, &mut app);
    assert!(!app.view.playlist_picker_filter_editing);
    assert_eq!(app.view.playlist_picker_filter, "quiet");

    handler(Key::Enter, &mut app);
    assert!(rx.try_recv().is_ok(), "Enter should add the track");
    assert_ne!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    );
  }

  #[test]
  fn picker_search_takes_letters_that_are_otherwise_picker_keys() {
    let (mut app, rx) = picker_with_nested_playlists();

    handler(app.user_config.keys.search, &mut app);
    type_text(&mut app, "jkqHML");

    assert_eq!(app.view.playlist_picker_filter, "jkqHML");
    assert!(matches!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    ));
    assert!(app.playlist_picker_items().is_empty());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn picker_search_arrows_move_the_selection_while_typing() {
    let (mut app, _rx) = picker_with_nested_playlists();
    app.view.playlist_picker_filter_editing = true;
    app.view.playlist_picker_filter = "r".to_string();
    assert_eq!(app.playlist_picker_items().len(), 2);

    handler(Key::Down, &mut app);
    assert_eq!(app.view.playlist_picker_selected_index, 1);
    handler(Key::Down, &mut app);
    assert_eq!(app.view.playlist_picker_selected_index, 0);
    handler(Key::Up, &mut app);
    assert_eq!(app.view.playlist_picker_selected_index, 1);
    assert_eq!(app.view.playlist_picker_filter, "r");
  }

  #[test]
  fn picker_search_matching_is_smart_case_and_ands_terms() {
    let (mut app, _rx) = picker_with_nested_playlists();

    app.view.playlist_picker_filter = "trip road".to_string();
    assert_eq!(app.playlist_picker_items().len(), 1);
    app.view.playlist_picker_filter = "road quiet".to_string();
    assert!(app.playlist_picker_items().is_empty());
    app.view.playlist_picker_filter = "ROAD".to_string();
    assert!(app.playlist_picker_items().is_empty());
    app.view.playlist_picker_filter = "Road".to_string();
    assert_eq!(app.playlist_picker_items().len(), 1);
  }

  #[test]
  fn esc_clears_the_picker_search_before_closing_the_picker() {
    let (mut app, _rx) = picker_with_nested_playlists();
    handler(app.user_config.keys.search, &mut app);
    type_text(&mut app, "road");
    handler(Key::Enter, &mut app);

    assert!(playlist_picker_captures_key(&app, Key::Esc));
    handler(Key::Esc, &mut app);
    assert!(app.view.playlist_picker_filter.is_empty());
    assert!(matches!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::AddTrackToPlaylistPicker)
    ));
    assert!(!playlist_picker_captures_key(&app, Key::Esc));
    assert_eq!(app.playlist_picker_items().len(), 2);
  }

  #[test]
  fn closing_the_picker_forgets_its_search() {
    let (mut app, _rx) = picker_with_nested_playlists();
    app.view.playlist_picker_filter = "road".to_string();
    app.view.playlist_picker_filter_editing = true;

    app.clear_dialog_state();

    assert!(app.view.playlist_picker_filter.is_empty());
    assert!(!app.view.playlist_picker_filter_editing);
  }

  #[test]
  fn picker_rows_respect_group_folders_first() {
    let (tx, _rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    *app.user_mut() = Some(user_info("spotatui-owner"));
    *app.all_playlists_mut() = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "Owned",
      "spotatui-owner",
      false,
    )];
    *app.playlist_folder_items_mut() = vec![
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 0,
      },
      PlaylistFolderItem::Folder(PlaylistFolder {
        generated_name: false,
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

  #[test]
  fn enter_in_the_mirror_picker_links_to_the_highlighted_source() {
    use crate::core::plugin_api::PlaylistInfo;
    use crate::core::source::Source;

    let (tx, rx) = channel();
    let mut app =
      App::new(tx, UserConfig::new(), Some(SystemTime::now())).under_source(Source::Qobuz);
    app.qobuz_playlists_mut().push(PlaylistInfo {
      generated_name: None,
      uri: "qobuz:playlist:9".to_string(),
      name: "Mine".to_string(),
      owner: "qobuz".to_string(),
      track_count: 3,
      id: Some("9".to_string()),
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: None,
    });
    app.view.selected_playlist_index = Some(0);
    app.apply(Action::OpenPlaylistSyncPicker);
    assert!(app.pending_playlist_sync_master().is_some());

    handler(Key::Enter, &mut app);

    assert!(app.pending_playlist_sync_master().is_none());
    assert!(!matches!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::PlaylistSyncPicker)
    ));
    assert_eq!(app.status_message(), Some("Mirroring Mine onto Spotify"));
    assert_eq!(app.get_current_route().id, RouteId::PlaylistSync);
    assert!(rx.try_recv().is_ok());
  }

  #[test]
  fn confirming_the_remove_dialog_forgets_the_link() {
    use crate::core::playlist_sync::{Endpoint, Link};
    use crate::core::source::Source;

    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.set_playlist_sync_links(vec![Link {
      id: "aaaaaaaaaaaa".to_string(),
      master: Endpoint {
        source: Source::Qobuz,
        playlist_uri: "qobuz:playlist:9".to_string(),
        name: "Mine".to_string(),
      },
      mirrors: Vec::new(),
    }]);
    app.begin_remove_playlist_sync_link();
    assert_eq!(app.pending_playlist_sync_remove(), Some("aaaaaaaaaaaa"));
    app.view.confirm = true;

    handler(Key::Enter, &mut app);

    assert_ne!(app.get_current_route().id, RouteId::Dialog);
    assert!(app.view.dialog.is_none());
    assert!(app.pending_playlist_sync_remove().is_none());
    assert!(rx.try_recv().is_ok());
  }
}
