use crate::core::app::{ActiveBlock, App, LIBRARY_OPTIONS};
use crate::core::layout::{is_wide_layout, library_constraints, split_input_help_and_settings};
use crate::core::source::Source;
use ratatui::{
  layout::{Constraint, Layout, Rect},
  Frame,
};

use super::{search::draw_input_and_help_box, util::draw_selectable_list};

pub fn draw_library_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Library,
    current_route.hovered_block == ActiveBlock::Library,
  );
  draw_selectable_list(
    f,
    app,
    layout_chunk,
    "Library",
    &LIBRARY_OPTIONS,
    highlight_state,
    Some(app.library.selected_index),
  );
}

pub fn draw_playlist_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let highlight_state = {
    let current_route = app.get_current_route();
    (
      current_route.active_block == ActiveBlock::MyPlaylists,
      current_route.hovered_block == ActiveBlock::MyPlaylists,
    )
  };

  // Local Files: the sidebar Playlists panel lists the local folders for the
  // active source instead of Spotify playlists (no write, so no "Add Playlist").
  if app.active_source == Source::Local {
    let items: Vec<String> = if app.local_playlists.is_empty() {
      vec!["(no folders \u{2014} set music dir, then press `d`)".to_string()]
    } else {
      app
        .local_playlists
        .iter()
        .map(|p| format!("\u{1F4C1} {}", p.name))
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      "Local Files",
      &items,
      highlight_state,
      app.selected_playlist_index,
    );
    return;
  }

  // Subsonic: the sidebar Playlists panel lists the server's playlists (no
  // local-write support, so no "Add Playlist").
  if app.active_source == Source::Subsonic {
    let items: Vec<String> = if app.subsonic_playlists.is_empty() {
      vec!["(no playlists \u{2014} configure server, then press `d`)".to_string()]
    } else {
      app
        .subsonic_playlists
        .iter()
        .map(|p| format!("\u{1F3B5} {}", p.name))
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      "Subsonic",
      &items,
      highlight_state,
      app.selected_playlist_index,
    );
    return;
  }

  // Internet Radio: the sidebar Playlists panel lists the configured stations.
  // A station is a leaf — Enter plays it directly instead of drilling in.
  if app.active_source == Source::Radio {
    let items: Vec<String> = if app.radio_stations.is_empty() {
      vec!["(no stations \u{2014} add behavior.radio_stations or search)".to_string()]
    } else {
      app
        .radio_stations
        .iter()
        .map(|s| format!("\u{1F4FB} {}", s.name))
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      "Radio Stations",
      &items,
      highlight_state,
      app.selected_playlist_index,
    );
    return;
  }

  // YouTube: the sidebar Playlists panel lists the user's *local* YouTube
  // playlists (youtube_playlists.yml — no Google account), plus a create
  // entry. Videos are found via search and added with `w`.
  if app.active_source == Source::YouTube {
    let mut items: Vec<String> = app
      .youtube_playlists
      .iter()
      .map(|p| format!("\u{1F4FC} {} ({})", p.name, p.track_count))
      .collect();
    items.push("+ New Playlist".to_string());
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      "YouTube Playlists",
      &items,
      highlight_state,
      app.selected_playlist_index,
    );
    return;
  }

  let display_items = app.get_playlist_display_items();

  let playlist_items: Vec<String> = if app.playlist_folder_items.is_empty() {
    // Fallback only when folder-aware items are not initialized yet
    match &app.playlists {
      Some(p) => p.items.iter().map(|item| item.name.to_owned()).collect(),
      None => vec![],
    }
  } else {
    display_items
      .iter()
      .map(|item| match item {
        crate::core::app::PlaylistFolderItem::Folder(folder) => {
          if folder.name.starts_with('\u{2190}') {
            // Back entry (already has arrow prefix)
            folder.name.clone()
          } else {
            format!("\u{1F4C1} {}", folder.name)
          }
        }
        crate::core::app::PlaylistFolderItem::Playlist { index, .. } => app
          .all_playlists
          .get(*index)
          .map(|p| p.name.clone())
          .unwrap_or_else(|| "Unknown".to_string()),
      })
      .collect()
  };

  let mut display_list = playlist_items;
  display_list.push("+ Add Playlist".to_string());

  draw_selectable_list(
    f,
    app,
    layout_chunk,
    "Playlists",
    &display_list,
    highlight_state,
    app.selected_playlist_index,
  );
}

pub fn draw_user_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  // Local Files has no saved library and no search, so the local-folder list
  // fills the whole sidebar — no input box, no Library panel.
  if app.active_source == Source::Local {
    draw_playlist_block(f, app, layout_chunk);
    return;
  }

  // Subsonic, Radio and YouTube support search but have no Spotify-style saved
  // library, so keep the search input and show the source's list, but hide the
  // Library panel.
  if app.active_source == Source::Subsonic
    || app.active_source == Source::Radio
    || app.active_source == Source::YouTube
  {
    if is_wide_layout(app) {
      let [input_area, playlist_area] = layout_chunk.layout(&Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
      ]));
      let [input_text_area, help_area, settings_area] =
        split_input_help_and_settings(app, input_area);
      draw_input_and_help_box(f, app, input_text_area, help_area, settings_area);
      draw_playlist_block(f, app, playlist_area);
    } else {
      draw_playlist_block(f, app, layout_chunk);
    }
    return;
  }

  // Check for width to make a responsive layout
  if is_wide_layout(app) {
    let lib_constraints = library_constraints(&app.user_config.behavior);
    let [input_area, library_area, playlist_area] = layout_chunk.layout(&Layout::vertical([
      Constraint::Length(3),
      lib_constraints[0],
      lib_constraints[1],
    ]));

    // Search input and help
    let [input_text_area, help_area, settings_area] =
      split_input_help_and_settings(app, input_area);
    draw_input_and_help_box(f, app, input_text_area, help_area, settings_area);
    draw_library_block(f, app, library_area);
    draw_playlist_block(f, app, playlist_area);
  } else {
    let [library_area, playlist_area] = layout_chunk.layout(&Layout::vertical(
      library_constraints(&app.user_config.behavior),
    ));

    // Search input and help
    draw_library_block(f, app, library_area);
    draw_playlist_block(f, app, playlist_area);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::PlaylistInfo;
  use ratatui::{backend::TestBackend, Terminal};

  fn rendered(app: &App, area: Rect) -> String {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw_user_block(f, app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
      .flat_map(|y| (0..area.width).map(move |x| (x, y)))
      .filter_map(|(x, y)| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  fn folder(name: &str) -> PlaylistInfo {
    PlaylistInfo {
      uri: format!("file:///music/{name}"),
      name: name.to_string(),
      owner: "local".to_string(),
      track_count: 0,
      id: None,
      owner_id: None,
      collaborative: false,
      public: None,
      image_url: None,
    }
  }

  #[test]
  fn local_source_sidebar_lists_folders_and_hides_library() {
    let mut app = App::default();
    app.active_source = Source::Local;
    app.local_playlists = vec![folder("Jazz")];
    let content = rendered(&app, Rect::new(0, 0, 32, 40));
    assert!(
      content.contains("Jazz"),
      "local folder should render: {content}"
    );
    assert!(
      content.contains("Local Files"),
      "panel title should be Local Files: {content}"
    );
    assert!(
      !content.contains("Liked Songs"),
      "Spotify library entries must be hidden under Local: {content}"
    );
  }

  #[test]
  fn spotify_source_sidebar_shows_library() {
    let app = App::default(); // Spotify is the default source
    let content = rendered(&app, Rect::new(0, 0, 32, 40));
    assert!(
      content.contains("Liked Songs"),
      "Spotify library entries should render: {content}"
    );
  }
}
