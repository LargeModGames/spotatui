use super::common_key_events;
use crate::core::action::{Action, LibraryTarget};
use crate::core::app::{library_options, App};
#[cfg(feature = "local-files")]
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
      let next_index = common_key_events::on_high_press_handler();
      app.library.selected_index = next_index;
    }
    k if common_key_events::middle_event(k) => {
      let next_index = common_key_events::on_middle_press_handler(library_options());
      app.library.selected_index = next_index;
    }
    k if common_key_events::low_event(k) => {
      let next_index = common_key_events::on_low_press_handler(library_options());
      app.library.selected_index = next_index
    }
    // `library` should probably be an array of structs with enums rather than just using indexes
    // like this
    Key::Enter => activate_selected(app),
    _ => (),
  };
}

/// The down key's cursor move, named for the mouse wheel.
pub(super) fn select_next(app: &mut App) {
  let next_index =
    common_key_events::on_down_press_handler(library_options(), Some(app.library.selected_index));
  app.library.selected_index = next_index;
}

pub(super) fn select_previous(app: &mut App) {
  let next_index =
    common_key_events::on_up_press_handler(library_options(), Some(app.library.selected_index));
  app.library.selected_index = next_index;
}

pub(super) fn activate_selected(app: &mut App) {
  // Every row is resolved by NAME (through `LibraryTarget`), never by
  // position: feature-gated rows shift the indices of everything after
  // them, so a positional match silently remaps when features change.
  let Some(name) = library_options().get(app.library.selected_index).copied() else {
    return;
  };
  let Some(target) = LibraryTarget::from_name(name) else {
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
  use crate::core::app::RouteId;
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::channel;
  use std::time::SystemTime;

  fn app_with_selection(index: usize) -> (App, std::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.library.selected_index = index;
    (app, rx)
  }

  #[test]
  fn enter_on_stats_entry_opens_stats_screen() {
    let index = library_options()
      .iter()
      .position(|o| *o == "Stats")
      .unwrap();
    let (mut app, rx) = app_with_selection(index);
    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::Stats);
    assert!(app.stats_loading);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::LoadListeningStats(_))));
  }

  #[test]
  fn enter_on_liked_songs_still_fetches_saved_tracks() {
    let index = library_options()
      .iter()
      .position(|o| *o == "Liked Songs")
      .unwrap();
    let (mut app, rx) = app_with_selection(index);
    handler(Key::Enter, &mut app);
    assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    assert!(matches!(
      rx.try_recv(),
      Ok(IoEvent::GetCurrentSavedTracks(None))
    ));
  }

  #[cfg(feature = "local-files")]
  #[test]
  fn enter_on_local_files_switches_source_and_opens_the_browser() {
    let index = library_options()
      .iter()
      .position(|o| *o == "Local Files")
      .unwrap();
    let (mut app, rx) = app_with_selection(index);
    // The source switch persists; without a seeded path it would write the
    // developer's real state.yml.
    let dir = tempfile::tempdir().unwrap();
    app.state_path = Some(dir.path().join("state.yml"));
    handler(Key::Enter, &mut app);
    assert_eq!(app.active_source, Source::Local);
    assert_eq!(app.runtime_state.active_source, Source::Local);
    assert_eq!(app.get_current_route().id, RouteId::LocalBrowser);
    assert!(matches!(rx.try_recv(), Ok(IoEvent::GetLocalPlaylists)));
    assert_eq!(app.view.selected_playlist_index, Some(0));
    assert_eq!(app.view.local_playlists_index, 0);
  }
}
