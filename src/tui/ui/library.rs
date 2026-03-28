use crate::core::app::{ActiveBlock, App, LIBRARY_OPTIONS};
use crate::core::layout::library_constraints;
use ratatui::{
  layout::{Constraint, Layout, Rect},
  Frame,
};

use super::{
  search::draw_input_and_help_box,
  util::{draw_selectable_list, SMALL_TERMINAL_WIDTH},
};

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

  let current_route = app.get_current_route();

  let highlight_state = (
    current_route.active_block == ActiveBlock::MyPlaylists,
    current_route.hovered_block == ActiveBlock::MyPlaylists,
  );

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
  // Check for width to make a responsive layout
  if app.size.width >= SMALL_TERMINAL_WIDTH && !app.user_config.behavior.enforce_wide_search_bar {
    let lib_constraints = library_constraints(&app.user_config.behavior);
    let [input_area, library_area, playlist_area] = layout_chunk.layout(&Layout::vertical([
      Constraint::Length(3),
      lib_constraints[0],
      lib_constraints[1],
    ]));

    // Search input and help
    draw_input_and_help_box(f, app, input_area);
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
