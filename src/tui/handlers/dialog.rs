use super::common_key_events;
use crate::core::app::{ActiveBlock, App, DialogContext};
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
    | DialogContext::PersistKeybindingFallback => {
      handle_confirmation_dialog(key, app, dialog_context)
    }
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
    k if common_key_events::right_event(k) => app.confirm = !app.confirm,
    k if common_key_events::left_event(k) => app.confirm = !app.confirm,
    _ => {}
  }
}

fn handle_add_to_playlist_picker(key: Key, app: &mut App) {
  let editable_playlists = app.editable_playlist_indices();
  let playlist_count = editable_playlists.len();
  match key {
    k if common_key_events::down_event(k) => {
      if playlist_count > 0 {
        let next = common_key_events::on_down_press_handler(
          &editable_playlists,
          Some(app.playlist_picker_selected_index),
        );
        app.playlist_picker_selected_index = next;
      }
    }
    k if common_key_events::up_event(k) => {
      if playlist_count > 0 {
        let next = common_key_events::on_up_press_handler(
          &editable_playlists,
          Some(app.playlist_picker_selected_index),
        );
        app.playlist_picker_selected_index = next;
      }
    }
    k if common_key_events::high_event(k) => {
      if playlist_count > 0 {
        app.playlist_picker_selected_index = common_key_events::on_high_press_handler();
      }
    }
    k if common_key_events::middle_event(k) => {
      if playlist_count > 0 {
        app.playlist_picker_selected_index =
          common_key_events::on_middle_press_handler(&editable_playlists);
      }
    }
    k if common_key_events::low_event(k) => {
      if playlist_count > 0 {
        app.playlist_picker_selected_index =
          common_key_events::on_low_press_handler(&editable_playlists);
      }
    }
    Key::Enter => {
      if let Some(pending_add) = app.pending_playlist_track_add.clone() {
        if let Some(playlist) = app.editable_playlist_at_picker_index(
          app
            .playlist_picker_selected_index
            .min(playlist_count.saturating_sub(1)),
        ) {
          app.dispatch(IoEvent::AddTrackToPlaylist(
            playlist.id.clone().into_static(),
            pending_add.track_id,
          ));
        }
      }
      close_dialog(app);
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

fn handle_remove_track_from_playlist_confirm(app: &mut App) {
  if let Some(pending_remove) = app.pending_playlist_track_removal.clone() {
    app.dispatch(IoEvent::RemoveTrackFromPlaylistAtPosition(
      pending_remove.playlist_id,
      pending_remove.track_id,
      pending_remove.position,
    ));
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
    app::{PendingPlaylistTrackAdd, RouteId},
    user_config::UserConfig,
  };
  use rspotify::model::{
    idtypes::{PlaylistId, TrackId, UserId},
    page::Page,
    playlist::PlaylistTracksRef,
    user::{PrivateUser, PublicUser},
    SimplifiedPlaylist,
  };
  use rspotify::prelude::Id;
  use std::{collections::HashMap, sync::mpsc::channel, time::SystemTime};

  fn private_user(id: &str) -> PrivateUser {
    PrivateUser {
      country: None,
      display_name: Some("Test User".to_string()),
      email: None,
      explicit_content: None,
      external_urls: HashMap::new(),
      followers: None,
      href: "https://api.spotify.com/v1/me".to_string(),
      id: UserId::from_id(id).unwrap().into_static(),
      images: None,
      product: None,
    }
  }

  fn public_user(id: &str) -> PublicUser {
    PublicUser {
      display_name: Some(id.to_string()),
      external_urls: HashMap::new(),
      followers: None,
      href: format!("https://api.spotify.com/v1/users/{id}"),
      id: UserId::from_id(id).unwrap().into_static(),
      images: Vec::new(),
    }
  }

  fn simplified_playlist(
    id: &str,
    name: &str,
    owner_id: &str,
    collaborative: bool,
  ) -> SimplifiedPlaylist {
    SimplifiedPlaylist {
      collaborative,
      external_urls: HashMap::new(),
      href: format!("https://api.spotify.com/v1/playlists/{id}"),
      id: PlaylistId::from_id(id).unwrap().into_static(),
      images: Vec::new(),
      name: name.to_string(),
      owner: public_user(owner_id),
      public: Some(false),
      snapshot_id: "snapshot".to_string(),
      tracks: PlaylistTracksRef {
        href: format!("https://api.spotify.com/v1/playlists/{id}/tracks"),
        total: 5,
      },
    }
  }

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
    let mut app = App::new(tx, UserConfig::new(), SystemTime::now());
    app.user = Some(private_user("spotatui-owner"));
    app.playlists = Some(Page {
      href: "https://api.spotify.com/v1/me/playlists".to_string(),
      items: vec![],
      limit: 50,
      next: None,
      offset: 0,
      previous: None,
      total: 3,
    });
    app.all_playlists = vec![
      simplified_playlist(
        "37i9dQZF1DWZqd5JICZI0u",
        "Followed",
        "friend-owner",
        false,
      ),
      simplified_playlist(
        "37i9dQZF1DXcBWIGoYBM5M",
        "Owned",
        "spotatui-owner",
        false,
      ),
      simplified_playlist(
        "37i9dQZF1DX4WYpdgoIcn6",
        "Collaborative",
        "friend-owner",
        true,
      ),
    ];
    app.pending_playlist_track_add = Some(PendingPlaylistTrackAdd {
      track_id: TrackId::from_id("0000000000000000000001").unwrap().into_static(),
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
        assert_eq!(playlist_id.id(), "37i9dQZF1DXcBWIGoYBM5M");
        assert_eq!(track_id.id(), "0000000000000000000001");
      }
      _ => panic!("expected add-track event"),
    }
  }
}
