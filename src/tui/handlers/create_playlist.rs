use crate::core::app::{App, CreatePlaylistFocus, CreatePlaylistStage};
use crate::infra::network::IoEvent;
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
          app.dispatch(IoEvent::CreateYouTubePlaylist(name.trim().to_string()));
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
        app.dispatch(IoEvent::SearchTracksForPlaylist(query));
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
  let track_ids: Vec<String> = app
    .create_playlist_tracks
    .iter()
    .filter_map(|t| t.id.clone())
    .collect();

  app.dispatch(IoEvent::CreateNewPlaylist(name, track_ids));
  close_form(app);
}

fn close_form(app: &mut App) {
  app.pop_navigation_stack();
  // Reset form state
  app.view.create_playlist_name = Vec::new();
  app.view.create_playlist_name_idx = 0;
  app.view.create_playlist_name_cursor = 0;
  app.view.create_playlist_stage = CreatePlaylistStage::Name;
  app.create_playlist_tracks = Vec::new();
  app.create_playlist_search_results = Vec::new();
  app.view.create_playlist_search_input = Vec::new();
  app.view.create_playlist_search_idx = 0;
  app.view.create_playlist_search_cursor = 0;
  app.view.create_playlist_selected_result = 0;
  app.view.create_playlist_focus = CreatePlaylistFocus::SearchInput;
}
