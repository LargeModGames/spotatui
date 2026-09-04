use super::common_key_events;
use crate::core::action::Action;
#[cfg(feature = "local-files")]
use crate::core::action::LibraryTarget;
use crate::core::app::App;
#[cfg(any(feature = "local-files", test))]
use crate::core::source::Source;
use crate::tui::event::Key;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::right_event(k, &app.user_config.keys) => {
      common_key_events::handle_right_event(app)
    }
    k if common_key_events::down_event(k, &app.user_config.keys) => select_next(app),
    k if common_key_events::up_event(k, &app.user_config.keys) => select_previous(app),
    k if common_key_events::high_event(k) => {
      select_row(app, common_key_events::on_high_press_handler());
    }
    k if common_key_events::middle_event(k) => {
      let next_index = common_key_events::on_middle_press_handler(&app.library_rows());
      select_row(app, next_index);
    }
    k if common_key_events::low_event(k) => {
      let next_index = common_key_events::on_low_press_handler(&app.library_rows());
      select_row(app, next_index);
    }
    Key::Enter => activate_selected(app),
    _ => (),
  };
}

/// Moves the highlight to the row at `index` among the rows offered now.
pub(super) fn select_row(app: &mut App, index: usize) {
  if let Some(target) = app.library_rows().get(index) {
    app.view.library_selected = *target;
  }
}

/// The down key's cursor move, named for the mouse wheel.
pub(super) fn select_next(app: &mut App) {
  let index = app.library_cursor();
  let next_index = common_key_events::on_down_press_handler(&app.library_rows(), Some(index));
  select_row(app, next_index);
}

pub(super) fn select_previous(app: &mut App) {
  let index = app.library_cursor();
  let next_index = common_key_events::on_up_press_handler(&app.library_rows(), Some(index));
  select_row(app, next_index);
}

pub(super) fn activate_selected(app: &mut App) {
  let Some(target) = app.library_rows().get(app.library_cursor()).copied() else {
    return;
  };

  #[cfg(feature = "local-files")]
  if target == LibraryTarget::LocalFiles {
    open_local_source(app);
    return;
  }
  app.apply(Action::OpenLibrary(target));
}

/// Doubles as the "switch to Local source" shortcut: it flips the active source
/// so the sidebar re-scopes to local folders, then opens the browser. The
/// sidebar cursor resets are presentation state and stay in the TUI.
#[cfg(feature = "local-files")]
fn open_local_source(app: &mut App) {
  app.view.selected_playlist_index = Some(0);
  app.view.local_playlists_index = 0;
  app.apply(Action::SelectSource(Source::Local));
  app.apply(Action::OpenLibrary(LibraryTarget::LocalFiles));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::action::LibraryTarget;
  use crate::core::app::RouteId;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  /// A source switch persists; without a seeded path it would write the
  /// developer's real state.yml.
  fn seed_state_path(app: &mut App) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    app.state_path = Some(dir.path().join("state.yml"));
    dir
  }

  fn app_selecting(
    target: LibraryTarget,
    session: Option<SystemTime>,
  ) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), session);
    assert!(app.library_rows().contains(&target), "the row is offered");
    app.view.library_selected = target;
    (app, rx)
  }

  #[test]
  fn enter_on_stats_entry_opens_stats_screen() {
    let (mut app, rx) = app_selecting(LibraryTarget::Stats, Some(SystemTime::now()));
    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::Stats);
    assert!(app.stats_loading);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::LoadListeningStats(_))));
  }

  #[test]
  fn enter_on_stats_opens_it_without_a_spotify_session() {
    let (mut app, _rx) = app_selecting(LibraryTarget::Stats, None);
    assert!(!app.library_rows().contains(&LibraryTarget::LikedSongs));
    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::Stats);
    assert!(app.stats_loading);
  }

  #[test]
  fn enter_on_liked_songs_still_fetches_saved_tracks() {
    let (mut app, rx) = app_selecting(LibraryTarget::LikedSongs, Some(SystemTime::now()));
    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetCurrentSavedTracks(None))
    ));
  }

  #[test]
  fn the_bottom_key_lands_on_the_last_offered_row() {
    let (mut app, _rx) = app_selecting(LibraryTarget::Friends, None);
    handler(Key::Char('L'), &mut app);
    assert_eq!(app.library_cursor(), app.library_rows().len() - 1);
  }

  #[test]
  fn a_hidden_selection_falls_back_to_the_top_row_and_returns_with_its_source() {
    let (mut app, _rx) = app_selecting(LibraryTarget::Podcasts, Some(SystemTime::now()));
    let _dir = seed_state_path(&mut app);
    app.apply(Action::SelectSource(Source::Local));
    assert!(!app.library_rows().contains(&LibraryTarget::Podcasts));
    assert_eq!(app.library_cursor(), 0);

    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::Friends);

    app.apply(Action::SelectSource(Source::Spotify));
    assert_eq!(
      app.library_rows()[app.library_cursor()],
      LibraryTarget::Podcasts
    );
  }

  #[cfg(feature = "local-files")]
  #[test]
  fn enter_on_local_files_switches_source_and_opens_the_browser() {
    let (mut app, rx) = app_selecting(LibraryTarget::LocalFiles, Some(SystemTime::now()));
    let _dir = seed_state_path(&mut app);
    handler(Key::Enter, &mut app);
    assert_eq!(app.active_source, Source::Local);
    assert_eq!(app.runtime_state.active_source, Source::Local);
    assert_eq!(app.get_current_route().id, RouteId::LocalBrowser);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetLocalPlaylists)));
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(app.view.local_playlists_index, 0);
  }
}
