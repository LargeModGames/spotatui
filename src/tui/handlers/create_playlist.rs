use crate::core::app::{App, CreatePlaylistFocus, CreatePlaylistStage};
use crate::infra::network::IoEvent;
use crate::tui::event::Key;
use unicode_width::UnicodeWidthChar;

pub fn handler(key: Key, app: &mut App) {
  match app.create_playlist_stage {
    CreatePlaylistStage::Name => handle_name_stage(key, app),
    CreatePlaylistStage::AddTracks => handle_add_tracks_stage(key, app),
  }
}

fn handle_name_stage(key: Key, app: &mut App) {
  match key {
    Key::Enter => {
      let name: String = app.create_playlist_name.iter().collect();
      if !name.trim().is_empty() {
        app.create_playlist_stage = CreatePlaylistStage::AddTracks;
        app.create_playlist_focus = CreatePlaylistFocus::SearchInput;
      }
    }
    Key::Esc => {
      close_form(app);
    }
    Key::Backspace if app.create_playlist_name_idx > 0 => {
      app.create_playlist_name_idx -= 1;
      let removed = app
        .create_playlist_name
        .remove(app.create_playlist_name_idx);
      let width = removed.width().unwrap_or(1) as u16;
      app.create_playlist_name_cursor = app.create_playlist_name_cursor.saturating_sub(width);
    }
    Key::Char(c) => {
      app
        .create_playlist_name
        .insert(app.create_playlist_name_idx, c);
      app.create_playlist_name_idx += 1;
      app.create_playlist_name_cursor += c.width().unwrap_or(1) as u16;
    }
    Key::Left if app.create_playlist_name_idx > 0 => {
      app.create_playlist_name_idx -= 1;
      let c = app.create_playlist_name[app.create_playlist_name_idx];
      app.create_playlist_name_cursor = app
        .create_playlist_name_cursor
        .saturating_sub(c.width().unwrap_or(1) as u16);
    }
    Key::Right if app.create_playlist_name_idx < app.create_playlist_name.len() => {
      let c = app.create_playlist_name[app.create_playlist_name_idx];
      app.create_playlist_name_idx += 1;
      app.create_playlist_name_cursor += c.width().unwrap_or(1) as u16;
    }
    _ => {}
  }
}

fn handle_add_tracks_stage(key: Key, app: &mut App) {
  match app.create_playlist_focus {
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
      let query: String = app.create_playlist_search_input.iter().collect();
      if !query.trim().is_empty() {
        app.dispatch(IoEvent::SearchTracksForPlaylist(query));
        app.create_playlist_focus = CreatePlaylistFocus::SearchResults;
      }
    }
    Key::Tab => {
      if !app.create_playlist_tracks.is_empty() {
        app.create_playlist_selected_result = 0;
        app.create_playlist_focus = CreatePlaylistFocus::AddedTracks;
      } else if !app.create_playlist_search_results.is_empty() {
        app.create_playlist_selected_result = 0;
        app.create_playlist_focus = CreatePlaylistFocus::SearchResults;
      }
    }
    Key::Down if !app.create_playlist_search_results.is_empty() => {
      app.create_playlist_selected_result = 0;
      app.create_playlist_focus = CreatePlaylistFocus::SearchResults;
    }
    Key::Backspace if app.create_playlist_search_idx > 0 => {
      app.create_playlist_search_idx -= 1;
      let removed = app
        .create_playlist_search_input
        .remove(app.create_playlist_search_idx);
      let width = removed.width().unwrap_or(1) as u16;
      app.create_playlist_search_cursor = app.create_playlist_search_cursor.saturating_sub(width);
    }
    Key::Char(c) => {
      app
        .create_playlist_search_input
        .insert(app.create_playlist_search_idx, c);
      app.create_playlist_search_idx += 1;
      app.create_playlist_search_cursor += c.width().unwrap_or(1) as u16;
    }
    Key::Left if app.create_playlist_search_idx > 0 => {
      app.create_playlist_search_idx -= 1;
      let c = app.create_playlist_search_input[app.create_playlist_search_idx];
      app.create_playlist_search_cursor = app
        .create_playlist_search_cursor
        .saturating_sub(c.width().unwrap_or(1) as u16);
    }
    Key::Right if app.create_playlist_search_idx < app.create_playlist_search_input.len() => {
      let c = app.create_playlist_search_input[app.create_playlist_search_idx];
      app.create_playlist_search_idx += 1;
      app.create_playlist_search_cursor += c.width().unwrap_or(1) as u16;
    }
    _ => {}
  }
}

fn handle_results_nav(key: Key, app: &mut App) {
  let count = app.create_playlist_search_results.len();
  match key {
    Key::Esc => {
      app.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    }
    Key::Up if count > 0 && app.create_playlist_selected_result > 0 => {
      app.create_playlist_selected_result -= 1;
    }
    Key::Down if count > 0 && app.create_playlist_selected_result + 1 < count => {
      app.create_playlist_selected_result += 1;
    }
    Key::Enter if count > 0 => {
      let idx = app.create_playlist_selected_result;
      if idx < count {
        let track = app.create_playlist_search_results[idx].clone();
        app.create_playlist_tracks.push(track);
      }
    }
    Key::Tab => {
      if !app.create_playlist_tracks.is_empty() {
        app.create_playlist_selected_result = 0;
        app.create_playlist_focus = CreatePlaylistFocus::AddedTracks;
      } else {
        app.create_playlist_focus = CreatePlaylistFocus::SearchInput;
      }
    }
    _ => {}
  }
}

fn handle_added_tracks_nav(key: Key, app: &mut App) {
  let count = app.create_playlist_tracks.len();
  match key {
    Key::Esc => {
      app.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    }
    Key::Tab => {
      app.create_playlist_focus = CreatePlaylistFocus::SearchInput;
    }
    Key::Up if count > 0 && app.create_playlist_selected_result > 0 => {
      app.create_playlist_selected_result -= 1;
    }
    Key::Down if count > 0 && app.create_playlist_selected_result + 1 < count => {
      app.create_playlist_selected_result += 1;
    }
    Key::Char('d') if count > 0 => {
      let idx = app.create_playlist_selected_result;
      if idx < count {
        app.create_playlist_tracks.remove(idx);
        if app.create_playlist_selected_result >= app.create_playlist_tracks.len()
          && !app.create_playlist_tracks.is_empty()
        {
          app.create_playlist_selected_result = app.create_playlist_tracks.len() - 1;
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
  let name: String = app.create_playlist_name.iter().collect();
  let track_ids: Vec<rspotify::model::idtypes::TrackId<'static>> = app
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
  app.create_playlist_name = Vec::new();
  app.create_playlist_name_idx = 0;
  app.create_playlist_name_cursor = 0;
  app.create_playlist_stage = CreatePlaylistStage::Name;
  app.create_playlist_tracks = Vec::new();
  app.create_playlist_search_results = Vec::new();
  app.create_playlist_search_input = Vec::new();
  app.create_playlist_search_idx = 0;
  app.create_playlist_search_cursor = 0;
  app.create_playlist_selected_result = 0;
  app.create_playlist_focus = CreatePlaylistFocus::SearchInput;
}
