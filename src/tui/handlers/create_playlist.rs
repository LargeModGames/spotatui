use crate::core::action::Action;
use crate::core::app::{App, CreatePlaylistFocus, CreatePlaylistStage};
use crate::tui::event::Key;
use unicode_width::UnicodeWidthChar;

pub fn handler(key: Key, app: &mut App) {
  match app.view.create_playlist_stage {
    CreatePlaylistStage::Name => handle_name_stage(key, app),
    CreatePlaylistStage::AddTracks => handle_add_tracks_stage(key, app),
  }
}

fn handle_name_stage(key: Key, app: &mut App) {
  match key {
    Key::Enter => {
      let name: String = app.view.create_playlist_name.iter().collect();
      if !name.trim().is_empty() {
        // Under the YouTube source the playlist is a local file — create it
        // right away; there is no Spotify-search stage (videos are added later
        // via search + `w`).
        if app.active_source == crate::core::source::Source::YouTube {
          app.apply(Action::CreateYouTubePlaylist(name.trim().to_string()));
          close_form(app);
          return;
        }
        app.view.create_playlist_stage = CreatePlaylistStage::AddTracks;
        app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
      }
    }
    Key::Esc => {
      close_form(app);
    }
    Key::Backspace if app.view.create_playlist_name_idx > 0 => {
      app.view.create_playlist_name_idx -= 1;
      let removed = app
        .view
        .create_playlist_name
        .remove(app.view.create_playlist_name_idx);
      let width = removed.width().unwrap_or(1) as u16;
      app.view.create_playlist_name_cursor =
        app.view.create_playlist_name_cursor.saturating_sub(width);
    }
    Key::Char(c) => {
      app
        .view
        .create_playlist_name
        .insert(app.view.create_playlist_name_idx, c);
      app.view.create_playlist_name_idx += 1;
      app.view.create_playlist_name_cursor += c.width().unwrap_or(1) as u16;
    }
    Key::Left if app.view.create_playlist_name_idx > 0 => {
      app.view.create_playlist_name_idx -= 1;
      let c = app.view.create_playlist_name[app.view.create_playlist_name_idx];
      app.view.create_playlist_name_cursor = app
        .view
        .create_playlist_name_cursor
        .saturating_sub(c.width().unwrap_or(1) as u16);
    }
    Key::Right if app.view.create_playlist_name_idx < app.view.create_playlist_name.len() => {
      let c = app.view.create_playlist_name[app.view.create_playlist_name_idx];
      app.view.create_playlist_name_idx += 1;
      app.view.create_playlist_name_cursor += c.width().unwrap_or(1) as u16;
    }
    _ => {}
  }
}

fn handle_add_tracks_stage(key: Key, app: &mut App) {
  match app.view.create_playlist_focus {
    CreatePlaylistFocus::SearchInput => handle_search_input(key, app),
    CreatePlaylistFocus::SearchResults => handle_results_nav(key, app),
    CreatePlaylistFocus::AddedTracks => handle_added_tracks_nav(key, app),
  }
}

fn handle_search_input(key: Key, app: &mut App) {
  match key {
    Key::Esc => {
      close_form(app);
    }
    Key::Enter => {
      let query: String = app.view.create_playlist_search_input.iter().collect();
      if !query.trim().is_empty() {
        app.apply(Action::SearchTracksForPlaylist(query));
        app.view.create_playlist_focus = CreatePlaylistFocus::SearchResults;
      }
    }
    Key::Tab => {
      if !app.create_playlist_tracks.is_empty() {
        app.view.create_playlist_selected_result = 0;
        app.view.create_playlist_focus = CreatePlaylistFocus::AddedTracks;
      } else if !app.create_playlist_search_results.is_empty() {
        app.view.create_playlist_selected_result = 0;
        app.view.create_playlist_focus = CreatePlaylistFocus::SearchResults;
      }
    }
    Key::Down if !app.create_playlist_search_results.is_empty() => {
      app.view.create_playlist_selected_result = 0;
      app.view.create_playlist_focus = CreatePlaylistFocus::SearchResults;
    }
    Key::Backspace if app.view.create_playlist_search_idx > 0 => {
      app.view.create_playlist_search_idx -= 1;
      let removed = app
        .view
        .create_playlist_search_input
        .remove(app.view.create_playlist_search_idx);
      let width = removed.width().unwrap_or(1) as u16;
      app.view.create_playlist_search_cursor =
        app.view.create_playlist_search_cursor.saturating_sub(width);
    }
    Key::Char(c) => {
      app
        .view
        .create_playlist_search_input
        .insert(app.view.create_playlist_search_idx, c);
      app.view.create_playlist_search_idx += 1;
      app.view.create_playlist_search_cursor += c.width().unwrap_or(1) as u16;
    }
    Key::Left if app.view.create_playlist_search_idx > 0 => {
      app.view.create_playlist_search_idx -= 1;
      let c = app.view.create_playlist_search_input[app.view.create_playlist_search_idx];
      app.view.create_playlist_search_cursor = app
        .view
        .create_playlist_search_cursor
        .saturating_sub(c.width().unwrap_or(1) as u16);
    }
    Key::Right
      if app.view.create_playlist_search_idx < app.view.create_playlist_search_input.len() =>
    {
      let c = app.view.create_playlist_search_input[app.view.create_playlist_search_idx];
      app.view.create_playlist_search_idx += 1;
      app.view.create_playlist_search_cursor += c.width().unwrap_or(1) as u16;
    }
    _ => {}
  }
}

fn handle_results_nav(key: Key, app: &mut App) {
  let count = app.create_playlist_search_results.len();
  match key {
    Key::Esc => {
      app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    }
    Key::Up if count > 0 && app.view.create_playlist_selected_result > 0 => {
      app.view.create_playlist_selected_result -= 1;
    }
    Key::Down if count > 0 && app.view.create_playlist_selected_result + 1 < count => {
      app.view.create_playlist_selected_result += 1;
    }
    Key::Enter if count > 0 => {
      let idx = app.view.create_playlist_selected_result;
      if idx < count {
        let track = app.create_playlist_search_results[idx].clone();
        app.create_playlist_tracks.push(track);
      }
    }
    Key::Tab => {
      if !app.create_playlist_tracks.is_empty() {
        app.view.create_playlist_selected_result = 0;
        app.view.create_playlist_focus = CreatePlaylistFocus::AddedTracks;
      } else {
        app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
      }
    }
    _ => {}
  }
}

fn handle_added_tracks_nav(key: Key, app: &mut App) {
  let count = app.create_playlist_tracks.len();
  match key {
    Key::Esc => {
      app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    }
    Key::Tab => {
      app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    }
    Key::Up if count > 0 && app.view.create_playlist_selected_result > 0 => {
      app.view.create_playlist_selected_result -= 1;
    }
    Key::Down if count > 0 && app.view.create_playlist_selected_result + 1 < count => {
      app.view.create_playlist_selected_result += 1;
    }
    Key::Char('d') if count > 0 => {
      let idx = app.view.create_playlist_selected_result;
      if idx < count {
        app.create_playlist_tracks.remove(idx);
        if app.view.create_playlist_selected_result >= app.create_playlist_tracks.len()
          && !app.create_playlist_tracks.is_empty()
        {
          app.view.create_playlist_selected_result = app.create_playlist_tracks.len() - 1;
        }
      }
    }
    Key::Enter => {
      submit_playlist(app);
    }
    _ => {}
  }
}

fn submit_playlist(app: &mut App) {
  let name: String = app.view.create_playlist_name.iter().collect();
  // Bare base62 ids: the payload the form has always sent.
  let track_ids: Vec<String> = app
    .create_playlist_tracks
    .iter()
    .filter_map(|t| t.id.clone())
    .collect();

  app.apply(Action::CreatePlaylist {
    name,
    track_uris: track_ids,
  });
  close_form(app);
}

fn close_form(app: &mut App) {
  app.pop_navigation_stack();
  app.clear_create_playlist_form_state();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{ActiveBlock, RouteId};
  use crate::core::user_config::UserConfig;
  use crate::infra::network::IoEvent;
  use std::sync::mpsc::{channel, Receiver};
  use std::time::SystemTime;

  fn app_on_the_create_form() -> (App, Receiver<IoEvent>) {
    let (tx, rx) = channel();
    let mut app = App::new(tx, UserConfig::new(), Some(SystemTime::now()));
    app.push_navigation_stack(RouteId::CreatePlaylist, ActiveBlock::CreatePlaylistForm);
    (app, rx)
  }

  #[test]
  fn esc_on_the_name_stage_closes_and_resets_the_form() {
    let (mut app, rx) = app_on_the_create_form();
    handler(Key::Char('M'), &mut app);
    handler(Key::Char('i'), &mut app);

    handler(Key::Esc, &mut app);

    assert!(app.view.create_playlist_name.is_empty());
    assert_eq!(app.view.create_playlist_name_idx, 0);
    assert_eq!(app.view.create_playlist_name_cursor, 0);
    assert_eq!(app.view.create_playlist_stage, CreatePlaylistStage::Name);
    assert_eq!(
      app.view.create_playlist_focus,
      CreatePlaylistFocus::SearchInput
    );
    assert!(app.create_playlist_tracks.is_empty());
    assert!(app.create_playlist_search_results.is_empty());
    assert_eq!(app.get_current_route().id, RouteId::Home);
    assert!(rx.try_recv().is_err(), "closing the form asks for nothing");
  }

  #[test]
  fn enter_on_a_blank_name_stays_on_the_name_stage() {
    let (mut app, rx) = app_on_the_create_form();

    handler(Key::Enter, &mut app);

    assert_eq!(app.view.create_playlist_stage, CreatePlaylistStage::Name);
    assert_eq!(app.get_current_route().id, RouteId::CreatePlaylist);
    assert!(rx.try_recv().is_err(), "expected nothing dispatched");
  }

  #[test]
  fn enter_on_a_named_form_advances_before_creating_anything() {
    let (mut app, rx) = app_on_the_create_form();
    handler(Key::Char('M'), &mut app);

    handler(Key::Enter, &mut app);

    assert_eq!(
      app.view.create_playlist_stage,
      CreatePlaylistStage::AddTracks
    );
    assert_eq!(
      app.view.create_playlist_focus,
      CreatePlaylistFocus::SearchInput
    );
    assert!(rx.try_recv().is_err(), "expected nothing dispatched");
  }
}
