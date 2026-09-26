use super::common_key_events;
use crate::core::action::Action;
use crate::core::app::App;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k, &app.user_config.keys) => {
      common_key_events::handle_left_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => {
      let next = common_key_events::on_down_press_handler(
        app.playlist_sync_links(),
        Some(app.view.playlist_sync_selected_link),
      );
      app.view.playlist_sync_selected_link = next;
    }
    k if common_key_events::up_event(k, &app.user_config.keys) => {
      let next = common_key_events::on_up_press_handler(
        app.playlist_sync_links(),
        Some(app.view.playlist_sync_selected_link),
      );
      app.view.playlist_sync_selected_link = next;
    }
    k if common_key_events::high_event(k) => {
      app.view.playlist_sync_selected_link = common_key_events::on_high_press_handler();
    }
    k if common_key_events::middle_event(k) => {
      let next = common_key_events::on_middle_press_handler(app.playlist_sync_links());
      app.view.playlist_sync_selected_link = next;
    }
    k if common_key_events::low_event(k) => {
      let next = common_key_events::on_low_press_handler(app.playlist_sync_links());
      app.view.playlist_sync_selected_link = next;
    }
    Key::Char('s') => {
      app.apply(Action::RunPlaylistSync);
    }
    Key::Char('D') => app.begin_remove_playlist_sync_link(),
    _ => {}
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, DialogContext};
  use crate::core::playlist_sync::{Endpoint, Link};
  use crate::core::source::Source;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn link(id: &str, name: &str) -> Link {
    Link {
      id: id.to_string(),
      master: Endpoint {
        source: Source::Qobuz,
        playlist_uri: format!("qobuz:playlist:{id}"),
        name: name.to_string(),
      },
      mirrors: Vec::new(),
    }
  }

  fn app_with_links() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.set_playlist_sync_links(vec![
      link("aaaaaaaaaaaa", "Road Trip"),
      link("bbbbbbbbbbbb", "Gym"),
    ]);
    (app, rx)
  }

  #[test]
  fn d_opens_the_remove_confirm_for_the_highlighted_link() {
    let (mut app, _rx) = app_with_links();
    app.view.playlist_sync_selected_link = 1;

    handler(Key::Char('D'), &mut app);

    assert_eq!(
      app.get_current_route().active_block,
      ActiveBlock::Dialog(DialogContext::RemovePlaylistSyncLinkConfirm)
    );
    assert_eq!(app.view.dialog.as_deref(), Some("Gym"));
    assert!(!app.view.confirm);
  }

  #[test]
  fn s_asks_for_one_run_of_every_link() {
    let (mut app, rx) = app_with_links();

    handler(Key::Char('s'), &mut app);

    assert!(rx.try_recv().is_ok());
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn the_cursor_wraps_over_the_links() {
    let (mut app, _rx) = app_with_links();

    handler(Key::Down, &mut app);
    assert_eq!(app.view.playlist_sync_selected_link, 1);

    handler(Key::Down, &mut app);
    assert_eq!(app.view.playlist_sync_selected_link, 0);

    handler(Key::Up, &mut app);
    assert_eq!(app.view.playlist_sync_selected_link, 1);
  }
}
