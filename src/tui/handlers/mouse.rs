use super::{common_key_events, library, playbar, playlist, settings};
use crate::core::app::{
  ActiveBlock, App, RouteId, SettingValue, SettingsCategory, LIBRARY_OPTIONS,
};
use crate::core::layout::{
  compute_main_layout, fullscreen_view_layout, miniplayer_playbar_area, MainLayoutAreas,
};
use crate::tui::event::Key;
use crate::tui::ui::player::{playbar_control_at, playbar_progress_line};
use crate::tui::ui::tables::table_scroll_offset;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const SETTINGS_UNSAVED_PROMPT_WIDTH: u16 = 58;
const SETTINGS_UNSAVED_PROMPT_HEIGHT: u16 = 9;

pub fn handler(mouse: MouseEvent, app: &mut App) {
  let current_route = app.get_current_route();
  let current_route_id = current_route.id.clone();
  let current_active_block = current_route.active_block;

  if current_active_block == ActiveBlock::Settings {
    handle_settings_screen_mouse(mouse, app);
    return;
  }

  if current_route_id == RouteId::LyricsView {
    handle_fullscreen_playbar_mouse(
      ActiveBlock::LyricsView,
      fullscreen_view_playbar_area(app),
      mouse,
      app,
    );
    return;
  }

  if current_route_id == RouteId::CoverArtView {
    handle_fullscreen_playbar_mouse(
      ActiveBlock::CoverArtView,
      fullscreen_view_playbar_area(app),
      mouse,
      app,
    );
    return;
  }

  if current_route_id == RouteId::MiniPlayer {
    handle_fullscreen_playbar_mouse(
      ActiveBlock::MiniPlayer,
      miniplayer_view_playbar_area(app),
      mouse,
      app,
    );
    return;
  }

  if !is_main_layout_mouse_interactive(current_active_block) {
    return;
  }

  let Some(areas) = main_layout_areas(app) else {
    return;
  };

  if let Some(input_area) = areas.input {
    if rect_contains(input_area, mouse.column, mouse.row) {
      handle_input_mouse(mouse, input_area, app);
      return;
    }
  }

  if let Some(help_area) = areas.help {
    if rect_contains(help_area, mouse.column, mouse.row) {
      handle_help_mouse(mouse, app);
      return;
    }
  }

  if let Some(settings_area) = areas.settings {
    if rect_contains(settings_area, mouse.column, mouse.row) {
      handle_settings_mouse(mouse, app);
      return;
    }
  }

  if rect_contains(areas.playbar, mouse.column, mouse.row) {
    handle_playbar_mouse(mouse, areas.playbar, ActiveBlock::PlayBar, app);
    return;
  }

  if rect_contains(areas.library, mouse.column, mouse.row) {
    handle_library_mouse(mouse, areas.library, app);
    return;
  }

  if rect_contains(areas.playlists, mouse.column, mouse.row) {
    handle_playlist_mouse(mouse, areas.playlists, app);
    return;
  }

  if rect_contains(areas.content, mouse.column, mouse.row) {
    handle_content_table_mouse(mouse, areas.content, current_route_id, app);
  }
}

fn handle_library_mouse(mouse: MouseEvent, list_area: Rect, app: &mut App) {
  match mouse.kind {
    MouseEventKind::ScrollDown => {
      focus_library(app);
      library::handler(Key::Down, app);
    }
    MouseEventKind::ScrollUp => {
      focus_library(app);
      library::handler(Key::Up, app);
    }
    MouseEventKind::Down(MouseButton::Left) => {
      focus_library(app);
      select_clicked_library_item(mouse.row, list_area, app);
    }
    _ => {}
  }
}

fn handle_playlist_mouse(mouse: MouseEvent, list_area: Rect, app: &mut App) {
  match mouse.kind {
    MouseEventKind::ScrollDown => {
      focus_playlists(app);
      playlist::handler(Key::Down, app);
    }
    MouseEventKind::ScrollUp => {
      focus_playlists(app);
      playlist::handler(Key::Up, app);
    }
    MouseEventKind::Down(MouseButton::Left) => {
      focus_playlists(app);
      select_clicked_playlist(mouse.row, list_area, app);
    }
    _ => {}
  }
}

fn handle_content_table_mouse(
  mouse: MouseEvent,
  table_area: Rect,
  route_id: RouteId,
  app: &mut App,
) {
  let Some(active_block) = common_key_events::content_active_block_for_route(&route_id) else {
    return;
  };

  if !matches!(
    mouse.kind,
    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp | MouseEventKind::Down(MouseButton::Left)
  ) {
    return;
  }

  let item_count = content_table_item_count(active_block, app);
  if item_count == 0 {
    return;
  }

  match mouse.kind {
    MouseEventKind::ScrollDown => {
      focus_content_table(active_block, app);
      super::handle_block_events(Key::Down, app);
    }
    MouseEventKind::ScrollUp => {
      focus_content_table(active_block, app);
      super::handle_block_events(Key::Up, app);
    }
    MouseEventKind::Down(MouseButton::Left) => {
      focus_content_table(active_block, app);
      select_clicked_content_table_item(active_block, mouse.row, table_area, item_count, app);
    }
    _ => {}
  }
}

fn handle_input_mouse(mouse: MouseEvent, input_area: Rect, app: &mut App) {
  if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
    return;
  }

  focus_input(app);
  set_input_cursor_from_column(input_area, mouse.column, app);
}

fn handle_help_mouse(mouse: MouseEvent, app: &mut App) {
  if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
    app.push_navigation_stack(RouteId::HelpMenu, ActiveBlock::HelpMenu);
  }
}

fn handle_settings_mouse(mouse: MouseEvent, app: &mut App) {
  if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
    app.load_settings_for_category();
    app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
  }
}

fn handle_fullscreen_playbar_mouse(
  focus_block: ActiveBlock,
  playbar_area: Option<Rect>,
  mouse: MouseEvent,
  app: &mut App,
) {
  let Some(playbar_area) = playbar_area else {
    return;
  };

  if rect_contains(playbar_area, mouse.column, mouse.row) {
    handle_playbar_mouse(mouse, playbar_area, focus_block, app);
  }
}

fn handle_playbar_mouse(
  mouse: MouseEvent,
  playbar_area: Rect,
  focus_block: ActiveBlock,
  app: &mut App,
) {
  match mouse.kind {
    MouseEventKind::Down(MouseButton::Left) => {
      // Control buttons take precedence; they sit on a different row than the gauge.
      if let Some(control) = playbar_control_at(app, playbar_area, mouse.column, mouse.row) {
        focus_playbar(focus_block, app);
        playbar::handle_control(control, app);
        return;
      }

      // Otherwise, a click on the progress line seeks to that position (#157).
      if let Some(line) = playbar_progress_line(app, playbar_area) {
        if line.contains(mouse.column, mouse.row) {
          focus_playbar(focus_block, app);
          app.seek_to(line.position_at(mouse.column));
        }
      }
    }
    // Dragging along the progress row scrubs; the column is clamped into the line.
    MouseEventKind::Drag(MouseButton::Left) => {
      if let Some(line) = playbar_progress_line(app, playbar_area) {
        if line.on_row(mouse.row) {
          focus_playbar(focus_block, app);
          app.seek_to(line.position_at(mouse.column));
        }
      }
    }
    _ => {}
  }
}

fn handle_settings_screen_mouse(mouse: MouseEvent, app: &mut App) {
  if app.settings_unsaved_prompt_visible {
    handle_unsaved_settings_prompt_mouse(mouse, app);
    return;
  }

  let Some(areas) = settings_layout_areas(app) else {
    return;
  };

  if rect_contains(areas.tabs, mouse.column, mouse.row) {
    handle_settings_tabs_mouse(mouse, areas.tabs, app);
    return;
  }

  if rect_contains(areas.list, mouse.column, mouse.row) {
    handle_settings_list_mouse(mouse, areas.list, app);
  }
}

fn handle_unsaved_settings_prompt_mouse(mouse: MouseEvent, app: &mut App) {
  if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
    return;
  }

  let Some(areas) = settings_unsaved_prompt_areas(app) else {
    return;
  };

  if rect_contains(areas.yes, mouse.column, mouse.row) {
    settings::handler(Key::Char('y'), app);
    return;
  }

  if rect_contains(areas.no, mouse.column, mouse.row) {
    settings::handler(Key::Char('n'), app);
  }
}

fn handle_settings_tabs_mouse(mouse: MouseEvent, tabs_area: Rect, app: &mut App) {
  match mouse.kind {
    MouseEventKind::ScrollDown => switch_settings_category_right(app),
    MouseEventKind::ScrollUp => switch_settings_category_left(app),
    MouseEventKind::Down(MouseButton::Left) => {
      if let Some(clicked_tab_index) =
        settings_tab_index_from_click(tabs_area, mouse.column, mouse.row)
      {
        switch_settings_category_to(clicked_tab_index, app);
      }
    }
    _ => {}
  }
}

fn handle_settings_list_mouse(mouse: MouseEvent, list_area: Rect, app: &mut App) {
  match mouse.kind {
    MouseEventKind::ScrollDown if !selected_setting_expects_key_capture(app) => {
      settings::handler(Key::Down, app);
    }
    MouseEventKind::ScrollUp if !selected_setting_expects_key_capture(app) => {
      settings::handler(Key::Up, app);
    }
    MouseEventKind::Down(MouseButton::Left) => {
      select_clicked_setting(mouse.row, list_area, app);
    }
    _ => {}
  }
}

fn selected_setting_expects_key_capture(app: &App) -> bool {
  app.settings_edit_mode
    && app
      .settings_items
      .get(app.settings_selected_index)
      .map(|setting| matches!(setting.value, SettingValue::Key(_)))
      .unwrap_or(false)
}

fn select_clicked_setting(mouse_row: u16, list_area: Rect, app: &mut App) {
  let item_count = app.settings_items.len();
  let Some(clicked_index) = settings_item_index_from_click(list_area, mouse_row, item_count) else {
    return;
  };

  if app.settings_edit_mode {
    let clicked_selected_item = clicked_index == app.settings_selected_index;
    if clicked_selected_item {
      if selected_setting_expects_key_capture(app) {
        settings::handler(Key::Esc, app);
      } else {
        settings::handler(Key::Enter, app);
      }
    } else {
      app.settings_selected_index = clicked_index;
      app.settings_edit_mode = false;
      app.settings_edit_buffer.clear();
    }
    return;
  }

  let was_selected = app.settings_selected_index == clicked_index;
  app.settings_selected_index = clicked_index;
  if was_selected {
    settings::handler(Key::Enter, app);
  }
}

fn switch_settings_category_left(app: &mut App) {
  let current_index = app.settings_category.index();
  let new_index = if current_index == 0 {
    SettingsCategory::all().len() - 1
  } else {
    current_index - 1
  };
  switch_settings_category_to(new_index, app);
}

fn switch_settings_category_right(app: &mut App) {
  let current_index = app.settings_category.index();
  let new_index = (current_index + 1) % SettingsCategory::all().len();
  switch_settings_category_to(new_index, app);
}

fn switch_settings_category_to(index: usize, app: &mut App) {
  let category = SettingsCategory::from_index(index);
  if app.settings_category != category || app.settings_items.is_empty() {
    app.settings_category = category;
    app.load_settings_for_category();
  }

  if app.settings_edit_mode {
    app.settings_edit_mode = false;
    app.settings_edit_buffer.clear();
  }
}

fn is_main_layout_mouse_interactive(active_block: ActiveBlock) -> bool {
  !matches!(
    active_block,
    ActiveBlock::HelpMenu
      | ActiveBlock::Queue
      | ActiveBlock::Error
      | ActiveBlock::SelectDevice
      | ActiveBlock::Analysis
      | ActiveBlock::LyricsView
      | ActiveBlock::CoverArtView
      | ActiveBlock::MiniPlayer
      | ActiveBlock::AnnouncementPrompt
      | ActiveBlock::ExitPrompt
      | ActiveBlock::Settings
      | ActiveBlock::Dialog(_)
      | ActiveBlock::SortMenu
      | ActiveBlock::Party
  )
}

fn focus_playlists(app: &mut App) {
  app.set_current_route_state(
    Some(ActiveBlock::MyPlaylists),
    Some(ActiveBlock::MyPlaylists),
  );
}

fn focus_library(app: &mut App) {
  app.set_current_route_state(Some(ActiveBlock::Library), Some(ActiveBlock::Library));
}

fn focus_content_table(active_block: ActiveBlock, app: &mut App) {
  app.set_current_route_state(Some(active_block), Some(active_block));
}

fn focus_playbar(block: ActiveBlock, app: &mut App) {
  app.set_current_route_state(Some(block), Some(block));
}

fn focus_input(app: &mut App) {
  app.input_context = crate::core::app::InputContext::GlobalSearch;
  app.set_current_route_state(Some(ActiveBlock::Input), Some(ActiveBlock::Input));
}

fn set_input_cursor_from_column(input_area: Rect, mouse_column: u16, app: &mut App) {
  let inner_start = input_area.x.saturating_add(1);
  let target_col = mouse_column
    .saturating_sub(inner_start)
    .saturating_add(app.input_scroll_offset.get()) as usize;

  let mut width = 0usize;
  let mut idx = 0usize;
  for (i, ch) in app.input.iter().enumerate() {
    let ch_width = UnicodeWidthChar::width(*ch).unwrap_or(1);
    if width + ch_width > target_col {
      idx = i;
      break;
    }
    width += ch_width;
    idx = i + 1;
  }

  app.input_idx = idx;
  app.input_cursor_position = width as u16;
}

fn select_clicked_library_item(mouse_row: u16, list_area: Rect, app: &mut App) {
  let item_count = LIBRARY_OPTIONS.len();
  let selected_index = app.library.selected_index.min(item_count.saturating_sub(1));

  let Some(clicked_index) =
    list_item_index_from_click(list_area, mouse_row, selected_index, item_count)
  else {
    return;
  };

  let was_selected = app.library.selected_index == clicked_index;
  app.library.selected_index = clicked_index;

  if was_selected {
    library::handler(Key::Enter, app);
  }
}

fn select_clicked_playlist(mouse_row: u16, list_area: Rect, app: &mut App) {
  let item_count = app.get_playlist_display_count();
  if item_count == 0 {
    return;
  }

  let selected_index = app
    .selected_playlist_index
    .unwrap_or(0)
    .min(item_count.saturating_sub(1));

  let Some(clicked_index) =
    list_item_index_from_click(list_area, mouse_row, selected_index, item_count)
  else {
    return;
  };

  let was_selected = app.selected_playlist_index == Some(clicked_index);
  app.selected_playlist_index = Some(clicked_index);

  // Open the selected item when clicking it again.
  if was_selected {
    playlist::handler(Key::Enter, app);
  }
}

fn select_clicked_content_table_item(
  active_block: ActiveBlock,
  mouse_row: u16,
  table_area: Rect,
  item_count: usize,
  app: &mut App,
) {
  let selected_index =
    content_table_selected_index(active_block, app).min(item_count.saturating_sub(1));

  let Some(clicked_index) =
    table_item_index_from_click(table_area, mouse_row, selected_index, item_count)
  else {
    return;
  };

  let was_selected = content_table_selected_index(active_block, app) == clicked_index;
  set_content_table_selected_index(active_block, clicked_index, app);
  if was_selected {
    super::handle_block_events(Key::Enter, app);
  }
}

fn content_table_item_count(active_block: ActiveBlock, app: &App) -> usize {
  match active_block {
    ActiveBlock::AlbumList => app
      .library
      .saved_albums
      .get_results(None)
      .map(|albums| albums.items.len())
      .unwrap_or(0),
    ActiveBlock::TrackTable => app.track_table.tracks.len(),
    ActiveBlock::AlbumTracks => match app.album_table_context {
      crate::core::app::AlbumTableContext::Full => app
        .selected_album_full
        .as_ref()
        .map(|album| album.album.tracks.len())
        .unwrap_or(0),
      crate::core::app::AlbumTableContext::Simplified => app
        .selected_album_simplified
        .as_ref()
        .map(|album| album.tracks.items.len())
        .unwrap_or(0),
    },
    ActiveBlock::RecentlyPlayed => app
      .recently_played
      .result
      .as_ref()
      .map(|recently_played| recently_played.items.len())
      .unwrap_or(0),
    ActiveBlock::Artists => app
      .library
      .saved_artists
      .get_results(None)
      .map(|artists| artists.items.len())
      .unwrap_or(0),
    ActiveBlock::Podcasts => app
      .library
      .saved_shows
      .get_results(None)
      .map(|shows| shows.items.len())
      .unwrap_or(0),
    ActiveBlock::EpisodeTable => app
      .library
      .show_episodes
      .get_results(None)
      .map(|episodes| episodes.items.len())
      .unwrap_or(0),
    _ => 0,
  }
}

fn content_table_selected_index(active_block: ActiveBlock, app: &App) -> usize {
  match active_block {
    ActiveBlock::AlbumList => app.album_list_index,
    ActiveBlock::TrackTable => app.track_table.selected_index,
    ActiveBlock::AlbumTracks => match app.album_table_context {
      crate::core::app::AlbumTableContext::Full => app.saved_album_tracks_index,
      crate::core::app::AlbumTableContext::Simplified => app
        .selected_album_simplified
        .as_ref()
        .map(|album| album.selected_index)
        .unwrap_or(0),
    },
    ActiveBlock::RecentlyPlayed => app.recently_played.index,
    ActiveBlock::Artists => app.artists_list_index,
    ActiveBlock::Podcasts => app.shows_list_index,
    ActiveBlock::EpisodeTable => app.episode_list_index,
    _ => 0,
  }
}

fn set_content_table_selected_index(active_block: ActiveBlock, index: usize, app: &mut App) {
  match active_block {
    ActiveBlock::AlbumList => app.album_list_index = index,
    ActiveBlock::TrackTable => app.track_table.selected_index = index,
    ActiveBlock::AlbumTracks => match app.album_table_context {
      crate::core::app::AlbumTableContext::Full => app.saved_album_tracks_index = index,
      crate::core::app::AlbumTableContext::Simplified => {
        if let Some(album) = &mut app.selected_album_simplified {
          album.selected_index = index;
        }
      }
    },
    ActiveBlock::RecentlyPlayed => app.recently_played.index = index,
    ActiveBlock::Artists => app.artists_list_index = index,
    ActiveBlock::Podcasts => app.shows_list_index = index,
    ActiveBlock::EpisodeTable => app.episode_list_index = index,
    _ => {}
  }
}

fn list_item_index_from_click(
  list_area: Rect,
  mouse_row: u16,
  selected_index: usize,
  item_count: usize,
) -> Option<usize> {
  if item_count == 0 || list_area.height <= 2 {
    return None;
  }

  let viewport_height = list_area.height.saturating_sub(2) as usize;
  if viewport_height == 0 {
    return None;
  }

  let inner_top = list_area.y.saturating_add(1);
  let inner_bottom_exclusive = list_area
    .y
    .saturating_add(list_area.height)
    .saturating_sub(1);

  if mouse_row < inner_top || mouse_row >= inner_bottom_exclusive {
    return None;
  }

  // `draw_selectable_list` recreates list state each frame with offset 0, so ratatui scrolls
  // just enough to keep the selected row visible.
  let offset = selected_index
    .saturating_add(1)
    .saturating_sub(viewport_height);
  let row_index = (mouse_row - inner_top) as usize;
  let clicked_index = offset + row_index;

  (clicked_index < item_count).then_some(clicked_index)
}

fn table_item_index_from_click(
  table_area: Rect,
  mouse_row: u16,
  selected_index: usize,
  item_count: usize,
) -> Option<usize> {
  if item_count == 0 || table_area.height <= 5 {
    return None;
  }

  let visible_rows = table_area.height.saturating_sub(5) as usize;
  if visible_rows == 0 {
    return None;
  }

  let first_data_row = table_area.y.saturating_add(2);
  let last_data_row_exclusive = first_data_row.saturating_add(visible_rows as u16);
  if mouse_row < first_data_row || mouse_row >= last_data_row_exclusive {
    return None;
  }

  let offset = table_scroll_offset(selected_index, visible_rows);

  let row_index = (mouse_row - first_data_row) as usize;
  let row_index = row_index.min(visible_rows.saturating_sub(1));
  let clicked_index = (offset + row_index).min(item_count.saturating_sub(1));

  Some(clicked_index)
}

fn settings_tab_index_from_click(
  tabs_area: Rect,
  mouse_column: u16,
  mouse_row: u16,
) -> Option<usize> {
  if tabs_area.width <= 2 || tabs_area.height <= 2 {
    return None;
  }

  // `Tabs` renders a single row in the block's inner area.
  let tab_row = tabs_area.y.saturating_add(1);
  if mouse_row != tab_row {
    return None;
  }

  let inner_left = tabs_area.x.saturating_add(1);
  let inner_width = tabs_area.width.saturating_sub(2);
  if inner_width == 0 {
    return None;
  }

  if mouse_column < inner_left {
    return None;
  }

  let relative_column = (mouse_column - inner_left) as usize;
  if relative_column >= inner_width as usize {
    return None;
  }

  // Match ratatui's default Tabs layout in `render_tabs`: left padding + title + right padding,
  // then divider between tabs.
  let left_padding_width = 1usize;
  let right_padding_width = 1usize;
  let divider_width = 1usize;

  let mut cursor = 0usize;
  let categories = SettingsCategory::all();
  for (index, category) in categories.iter().enumerate() {
    let tab_start = cursor;
    let tab_width = left_padding_width + category.name().width() + right_padding_width;
    let tab_end = tab_start.saturating_add(tab_width);

    if relative_column >= tab_start && relative_column < tab_end {
      return Some(index);
    }

    cursor = tab_end;
    let is_last_tab = index + 1 == categories.len();
    if !is_last_tab {
      let divider_end = cursor.saturating_add(divider_width);

      // Divider clicks still map to a nearby tab for a forgiving UX.
      if relative_column >= cursor && relative_column < divider_end {
        return Some(index);
      }

      cursor = divider_end;
    }

    if cursor >= inner_width as usize {
      break;
    }
  }

  None
}

fn settings_item_index_from_click(
  list_area: Rect,
  mouse_row: u16,
  item_count: usize,
) -> Option<usize> {
  if item_count == 0 || list_area.height <= 2 {
    return None;
  }

  let inner_top = list_area.y.saturating_add(1);
  let inner_bottom_exclusive = list_area
    .y
    .saturating_add(list_area.height)
    .saturating_sub(1);
  if mouse_row < inner_top || mouse_row >= inner_bottom_exclusive {
    return None;
  }

  let row_index = (mouse_row - inner_top) as usize;
  (row_index < item_count).then_some(row_index)
}

struct SettingsLayoutAreas {
  tabs: Rect,
  list: Rect,
}

struct SettingsUnsavedPromptAreas {
  yes: Rect,
  no: Rect,
}

fn settings_layout_areas(app: &App) -> Option<SettingsLayoutAreas> {
  if app.size.width == 0 || app.size.height == 0 {
    return None;
  }

  let root = Rect::new(0, 0, app.size.width, app.size.height);
  let [tabs_area, list_area, _help_area] = root.layout(
    &Layout::vertical([
      Constraint::Length(3),
      Constraint::Min(1),
      Constraint::Length(3),
    ])
    .margin(2),
  );

  Some(SettingsLayoutAreas {
    tabs: tabs_area,
    list: list_area,
  })
}

fn settings_unsaved_prompt_areas(app: &App) -> Option<SettingsUnsavedPromptAreas> {
  if app.size.width == 0 || app.size.height == 0 {
    return None;
  }

  let bounds = Rect::new(0, 0, app.size.width, app.size.height);
  let width = std::cmp::min(
    bounds.width.saturating_sub(4),
    SETTINGS_UNSAVED_PROMPT_WIDTH,
  );
  if width == 0 {
    return None;
  }

  let height = SETTINGS_UNSAVED_PROMPT_HEIGHT.min(bounds.height.saturating_sub(2).max(1));
  let left = bounds.x + bounds.width.saturating_sub(width) / 2;
  let top = bounds.y + bounds.height.saturating_sub(height) / 2;
  let rect = Rect::new(left, top, width, height);

  let [_message_area, buttons_area, _hint_area] = rect.layout(
    &Layout::vertical([
      Constraint::Min(2),
      Constraint::Length(3),
      Constraint::Length(1),
    ])
    .margin(1),
  );

  let [yes_area, no_area] = buttons_area.layout(
    &Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).horizontal_margin(3),
  );

  Some(SettingsUnsavedPromptAreas {
    yes: yes_area,
    no: no_area,
  })
}

fn fullscreen_view_playbar_area(app: &App) -> Option<Rect> {
  if app.size.width == 0 || app.size.height == 0 {
    return None;
  }

  let root = Rect::new(0, 0, app.size.width, app.size.height);
  let (_, playbar_area) = fullscreen_view_layout(&app.user_config.behavior, root);
  playbar_area
}

fn miniplayer_view_playbar_area(app: &App) -> Option<Rect> {
  if app.size.width == 0 || app.size.height == 0 {
    return None;
  }

  Some(miniplayer_playbar_area(Rect::new(
    0,
    0,
    app.size.width,
    app.size.height,
  )))
}

fn main_layout_areas(app: &App) -> Option<MainLayoutAreas> {
  compute_main_layout(app)
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
  let right = rect.x.saturating_add(rect.width);
  let bottom = rect.y.saturating_add(rect.height);
  x >= rect.x && x < right && y >= rect.y && y < bottom
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::{
    AlbumTableContext, PlaylistFolderItem, RouteId, SelectedAlbum, SettingValue, SettingsCategory,
    TrackTableContext,
  };
  use crate::core::test_helpers::full_track;
  use crate::tui::ui::player::PlaybarControl;
  use chrono::{Duration, Utc};
  use crossterm::event::{KeyModifiers, MouseEvent};
  use ratatui::layout::Size;
  use rspotify::model::context::{Actions, CurrentPlaybackContext};
  use rspotify::model::enums::{DeviceType, RepeatState};
  use rspotify::model::{
    AlbumId, AlbumType, CurrentlyPlayingType, DatePrecision, Device, FullAlbum, FullTrack, Page,
    PlayableItem, SavedAlbum, SimplifiedAlbum, SimplifiedArtist, SimplifiedTrack,
  };
  use std::collections::HashMap;

  fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
      kind,
      column,
      row,
      modifiers: KeyModifiers::NONE,
    }
  }

  fn with_playlist_items(app: &mut App) {
    app.playlist_folder_items = vec![
      PlaylistFolderItem::Playlist {
        index: 0,
        current_id: 0,
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
  }

  #[allow(deprecated)]
  fn saved_album(index: usize) -> SavedAlbum {
    let id = format!("{index:0>22}");
    SavedAlbum {
      added_at: Utc::now(),
      album: FullAlbum {
        artists: vec![SimplifiedArtist {
          name: format!("Artist {index}"),
          ..Default::default()
        }],
        album_type: AlbumType::Album,
        available_markets: None,
        copyrights: vec![],
        external_ids: HashMap::new(),
        external_urls: HashMap::new(),
        genres: vec![],
        href: format!("https://example.com/albums/{id}"),
        id: AlbumId::from_id(&id).expect("valid album id").into_static(),
        images: vec![],
        name: format!("Album {index}"),
        popularity: 0,
        release_date: "2026-01-01".to_string(),
        release_date_precision: DatePrecision::Day,
        tracks: Page {
          href: format!("https://example.com/albums/{id}/tracks"),
          items: Vec::<SimplifiedTrack>::new(),
          limit: 0,
          next: None,
          offset: 0,
          previous: None,
          total: 0,
        },
        label: None,
      },
    }
  }

  fn with_saved_albums(app: &mut App, count: usize) {
    let page = Page {
      href: "https://example.com/me/albums".to_string(),
      items: (0..count).map(saved_album).collect(),
      limit: count as u32,
      next: None,
      offset: 0,
      previous: None,
      total: count as u32,
    };
    let domain_page = crate::infra::network::mapping::map_page(
      &page,
      crate::infra::network::mapping::saved_album_info,
    );
    app.library.saved_albums.add_pages(domain_page);
  }

  fn open_settings(app: &mut App) {
    app.settings_category = SettingsCategory::Behavior;
    app.load_settings_for_category();
    app.push_navigation_stack(RouteId::Settings, ActiveBlock::Settings);
  }

  #[allow(deprecated)]
  fn with_playbar_context(app: &mut App) {
    let item = PlayableItem::Track(FullTrack {
      album: SimplifiedAlbum {
        name: "Test Album".to_string(),
        ..Default::default()
      },
      artists: vec![SimplifiedArtist {
        name: "Test Artist".to_string(),
        ..Default::default()
      }],
      available_markets: Vec::new(),
      disc_number: 1,
      duration: Duration::milliseconds(180_000),
      explicit: false,
      external_ids: HashMap::new(),
      external_urls: HashMap::new(),
      href: None,
      id: None,
      is_local: false,
      is_playable: Some(true),
      linked_from: None,
      restrictions: None,
      name: "Test Track".to_string(),
      popularity: 50,
      preview_url: None,
      track_number: 1,
      r#type: rspotify::model::Type::Track,
    });

    app.current_playback_context = Some(CurrentPlaybackContext {
      device: Device {
        id: Some("device-1".to_string()),
        is_active: true,
        is_private_session: false,
        is_restricted: false,
        name: "Desk Speaker".to_string(),
        _type: DeviceType::Computer,
        volume_percent: Some(42),
      },
      repeat_state: RepeatState::Off,
      shuffle_state: false,
      context: None,
      timestamp: Utc::now(),
      progress: None,
      is_playing: false,
      item: Some(item),
      currently_playing_type: CurrentlyPlayingType::Track,
      actions: Actions::default(),
    });
  }

  fn find_playbar_control_click(
    app: &App,
    playbar_area: Rect,
    target: PlaybarControl,
  ) -> (u16, u16) {
    let max_x = playbar_area.x.saturating_add(playbar_area.width);
    let max_y = playbar_area.y.saturating_add(playbar_area.height);

    for y in playbar_area.y..max_y {
      for x in playbar_area.x..max_x {
        if playbar_control_at(app, playbar_area, x, y) == Some(target) {
          return (x, y);
        }
      }
    }

    panic!("expected to find click point for {:?}", target);
  }

  fn find_non_control_playbar_click(app: &App, playbar_area: Rect) -> (u16, u16) {
    let max_x = playbar_area.x.saturating_add(playbar_area.width);
    let max_y = playbar_area.y.saturating_add(playbar_area.height);

    for y in playbar_area.y..max_y {
      for x in playbar_area.x..max_x {
        if playbar_control_at(app, playbar_area, x, y).is_none() {
          return (x, y);
        }
      }
    }

    panic!("expected to find non-control click point in playbar");
  }

  #[test]
  fn scroll_over_playlists_changes_selection() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playlist_items(&mut app);
    app.selected_playlist_index = Some(0);

    let areas = main_layout_areas(&app).expect("layout areas");
    let x = areas.playlists.x + 1;
    let y = areas.playlists.y + 1;

    handler(mouse_event(MouseEventKind::ScrollDown, x, y), &mut app);
    assert_eq!(app.selected_playlist_index, Some(1));

    handler(mouse_event(MouseEventKind::ScrollUp, x, y), &mut app);
    assert_eq!(app.selected_playlist_index, Some(0));

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::MyPlaylists);
    assert_eq!(current_route.hovered_block, ActiveBlock::MyPlaylists);
  }

  #[test]
  fn click_search_input_focuses_input() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.input = "hello".chars().collect();
    app.input_idx = 0;
    app.input_cursor_position = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let input = areas.input.expect("input area");

    handler(
      mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        input.x + 2,
        input.y + 1,
      ),
      &mut app,
    );

    let route = app.get_current_route();
    assert_eq!(route.active_block, ActiveBlock::Input);
    assert_eq!(route.hovered_block, ActiveBlock::Input);
  }

  #[test]
  fn click_settings_opens_settings_screen() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };

    let areas = main_layout_areas(&app).expect("layout areas");
    let settings = areas.settings.expect("settings area");

    handler(
      mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        settings.x + 1,
        settings.y + 1,
      ),
      &mut app,
    );

    let route = app.get_current_route();
    assert_eq!(route.id, RouteId::Settings);
    assert_eq!(route.active_block, ActiveBlock::Settings);
    assert!(!app.settings_items.is_empty());
  }

  #[test]
  fn click_main_layout_playbar_control_triggers_action() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);

    let areas = main_layout_areas(&app).expect("layout areas");
    let (x, y) = find_playbar_control_click(&app, areas.playbar, PlaybarControl::PlayPause);
    assert!(!app.is_loading);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert!(app.is_loading);
    let route = app.get_current_route();
    assert_eq!(route.active_block, ActiveBlock::PlayBar);
    assert_eq!(route.hovered_block, ActiveBlock::PlayBar);
  }

  #[test]
  fn click_main_layout_playbar_progress_seeks() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app); // 180_000 ms track
    app.song_progress_ms = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let line = playbar_progress_line(&app, areas.playbar).expect("progress line");

    // A click a quarter of the way along the gauge seeks to ~25% of the track.
    let quarter_x = line.start + line.width / 4;
    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), quarter_x, line.row),
      &mut app,
    );
    let after_quarter = app.song_progress_ms;
    assert!(
      (35_000..=60_000).contains(&after_quarter),
      "quarter click gave {after_quarter}"
    );

    // A click three quarters along seeks further into the track.
    let three_quarter_x = line.start + (line.width * 3) / 4;
    handler(
      mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        three_quarter_x,
        line.row,
      ),
      &mut app,
    );
    assert!(
      (120_000..=150_000).contains(&app.song_progress_ms),
      "three-quarter click gave {}",
      app.song_progress_ms
    );
    assert!(app.song_progress_ms > after_quarter);

    assert_eq!(app.get_current_route().active_block, ActiveBlock::PlayBar);
  }

  #[test]
  fn click_playbar_time_label_does_not_seek() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);
    app.song_progress_ms = 12_345;

    let areas = main_layout_areas(&app).expect("layout areas");
    let line = playbar_progress_line(&app, areas.playbar).expect("progress line");

    // Clicking the time label (left of the gauge line) must not seek.
    let label_x = line.start - 2;
    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), label_x, line.row),
      &mut app,
    );
    assert_eq!(app.song_progress_ms, 12_345);
  }

  #[test]
  fn drag_playbar_progress_seeks() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);
    app.song_progress_ms = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let line = playbar_progress_line(&app, areas.playbar).expect("progress line");

    let mid_x = line.start + line.width / 2;
    handler(
      mouse_event(MouseEventKind::Drag(MouseButton::Left), mid_x, line.row),
      &mut app,
    );
    assert!(
      (80_000..=100_000).contains(&app.song_progress_ms),
      "drag to midpoint gave {}",
      app.song_progress_ms
    );
  }

  // Non-circular geometry check: the seek tests above derive their click column
  // from `playbar_progress_line`, so a wrong `start`/`width` would cancel out. This
  // renders the real playbar and asserts the computed line start matches the column
  // where the actual gauge symbols begin in the rendered buffer.
  #[test]
  fn playbar_progress_line_start_matches_rendered_gauge() {
    use crate::tui::ui::player::draw_playbar;
    use ratatui::{backend::TestBackend, Terminal};

    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);
    app.song_progress_ms = 60_000; // 1/3 of the 180s track -> a partly-filled gauge

    let areas = main_layout_areas(&app).expect("layout areas");
    let line = playbar_progress_line(&app, areas.playbar).expect("progress line");

    let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
    terminal
      .draw(|f| draw_playbar(f, &app, areas.playbar))
      .unwrap();
    let buffer = terminal.backend().buffer();

    // The LineGauge fills with the configured filled/unfilled symbols; neither
    // appears in the time label, so the first such cell on the progress row is
    // the true gauge start column. Parameterized on the config fields so an
    // overridden gauge glyph can't silently break the alignment guard.
    let filled = app.user_config.behavior.gauge_filled_icon.as_str();
    let unfilled = app.user_config.behavior.gauge_unfilled_icon.as_str();
    let rendered_start = (areas.playbar.x..areas.playbar.x + areas.playbar.width)
      .find(|&x| {
        matches!(
          buffer.cell((x, line.row)).map(|c| c.symbol()),
          Some(s) if s == filled || s == unfilled
        )
      })
      .expect("expected rendered gauge cells on the progress row");

    assert_eq!(
      rendered_start, line.start,
      "computed gauge start must match the rendered bar"
    );
  }

  #[test]
  fn click_lyrics_view_playbar_control_triggers_action() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);
    with_playbar_context(&mut app);

    let playbar_area = fullscreen_view_playbar_area(&app).expect("lyrics view playbar area");
    let (x, y) = find_playbar_control_click(&app, playbar_area, PlaybarControl::PlayPause);
    assert!(!app.is_loading);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert!(app.is_loading);
    let route = app.get_current_route();
    assert_eq!(route.id, RouteId::LyricsView);
    assert_eq!(route.active_block, ActiveBlock::LyricsView);
    assert_eq!(route.hovered_block, ActiveBlock::LyricsView);
  }

  #[test]
  fn click_miniplayer_control_triggers_action_and_keeps_miniplayer_focus() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::MiniPlayer, ActiveBlock::MiniPlayer);
    with_playbar_context(&mut app);

    let playbar_area = miniplayer_playbar_area(Rect::new(0, 0, app.size.width, app.size.height));
    let (x, y) = find_playbar_control_click(&app, playbar_area, PlaybarControl::PlayPause);
    assert!(!app.is_loading);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert!(app.is_loading);
    let route = app.get_current_route();
    assert_eq!(route.id, RouteId::MiniPlayer);
    assert_eq!(route.active_block, ActiveBlock::MiniPlayer);
    assert_eq!(route.hovered_block, ActiveBlock::MiniPlayer);
  }

  #[test]
  fn fullscreen_view_playbar_area_uses_configured_height() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.user_config.behavior.playbar_height_rows = 8;

    let playbar_area = fullscreen_view_playbar_area(&app).expect("fullscreen playbar area");

    assert_eq!(playbar_area, Rect::new(0, 42, 160, 8));
  }

  #[test]
  fn fullscreen_view_playbar_area_is_hidden_when_height_is_zero() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.user_config.behavior.playbar_height_rows = 0;

    assert!(fullscreen_view_playbar_area(&app).is_none());
  }

  #[test]
  fn click_hidden_lyrics_view_playbar_area_does_nothing() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.user_config.behavior.playbar_height_rows = 0;
    app.push_navigation_stack(RouteId::LyricsView, ActiveBlock::LyricsView);
    with_playbar_context(&mut app);

    let (initial_route_id, initial_active_block, initial_hovered_block) = {
      let route = app.get_current_route();
      (route.id.clone(), route.active_block, route.hovered_block)
    };

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), 80, 49),
      &mut app,
    );

    assert!(!app.is_loading);
    let route = app.get_current_route();
    assert_eq!(route.id, initial_route_id);
    assert_eq!(route.active_block, initial_active_block);
    assert_eq!(route.hovered_block, initial_hovered_block);
  }

  #[test]
  fn resized_playbar_control_click_still_maps_correctly() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.user_config.behavior.playbar_height_rows = 8;
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);

    let areas = main_layout_areas(&app).expect("layout areas");
    assert_eq!(areas.playbar.height, 8);
    let (x, y) = find_playbar_control_click(&app, areas.playbar, PlaybarControl::PlayPause);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert!(app.is_loading);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::PlayBar);
  }

  #[test]
  fn smaller_resized_playbar_control_click_still_maps_correctly() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.user_config.behavior.playbar_height_rows = 3;
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);

    let areas = main_layout_areas(&app).expect("layout areas");
    assert_eq!(areas.playbar.height, 3);
    let (x, y) = find_playbar_control_click(&app, areas.playbar, PlaybarControl::PlayPause);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert!(app.is_loading);
    assert_eq!(app.get_current_route().active_block, ActiveBlock::PlayBar);
  }

  #[test]
  fn click_playbar_outside_controls_does_nothing() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playbar_context(&mut app);

    let areas = main_layout_areas(&app).expect("layout areas");
    let (x, y) = find_non_control_playbar_click(&app, areas.playbar);
    let initial_active = app.get_current_route().active_block;
    let initial_hovered = app.get_current_route().hovered_block;

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert!(!app.is_loading);
    let route = app.get_current_route();
    assert_eq!(route.active_block, initial_active);
    assert_eq!(route.hovered_block, initial_hovered);
  }

  #[test]
  fn click_settings_tab_switches_category() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    open_settings(&mut app);

    let areas = settings_layout_areas(&app).expect("settings layout areas");
    let inner_left = areas.tabs.x + 1;
    let left_padding = 1u16;
    let right_padding = 1u16;
    let divider = 1u16;
    let behavior_tab_width =
      left_padding + SettingsCategory::Behavior.name().len() as u16 + right_padding;
    let icons_tab_width =
      left_padding + SettingsCategory::Icons.name().len() as u16 + right_padding;
    let keybindings_tab_width =
      left_padding + SettingsCategory::Keybindings.name().len() as u16 + right_padding;
    let theme_title_start = inner_left
      + behavior_tab_width
      + divider
      + icons_tab_width
      + divider
      + keybindings_tab_width
      + divider
      + left_padding;
    let theme_tab_x = theme_title_start + 1;
    let tab_y = areas.tabs.y + 1;

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), theme_tab_x, tab_y),
      &mut app,
    );

    assert_eq!(app.settings_category, SettingsCategory::Theme);
    assert_eq!(app.settings_selected_index, 0);
    assert!(!app.settings_items.is_empty());
  }

  #[test]
  fn scroll_in_settings_list_changes_selected_item() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    open_settings(&mut app);

    let areas = settings_layout_areas(&app).expect("settings layout areas");
    let x = areas.list.x + 2;
    let y = areas.list.y + 2;

    assert_eq!(app.settings_selected_index, 0);
    handler(mouse_event(MouseEventKind::ScrollDown, x, y), &mut app);
    assert_eq!(app.settings_selected_index, 1);

    handler(mouse_event(MouseEventKind::ScrollUp, x, y), &mut app);
    assert_eq!(app.settings_selected_index, 0);
  }

  #[test]
  fn clicking_selected_bool_setting_toggles_value() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    open_settings(&mut app);

    let bool_index = app
      .settings_items
      .iter()
      .position(|setting| matches!(setting.value, SettingValue::Bool(_)))
      .expect("expected a boolean setting");
    app.settings_selected_index = bool_index;

    let initial_value = app
      .settings_items
      .get(bool_index)
      .and_then(|setting| match setting.value {
        SettingValue::Bool(value) => Some(value),
        _ => None,
      })
      .expect("selected setting should be boolean");

    let areas = settings_layout_areas(&app).expect("settings layout areas");
    let y = areas.list.y + 1 + bool_index as u16;
    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), areas.list.x + 2, y),
      &mut app,
    );

    let updated_value = app
      .settings_items
      .get(bool_index)
      .and_then(|setting| match setting.value {
        SettingValue::Bool(value) => Some(value),
        _ => None,
      })
      .expect("selected setting should stay boolean");
    assert_ne!(updated_value, initial_value);
    assert!(!app.settings_edit_mode);
  }

  #[test]
  fn keybinding_capture_can_be_cancelled_with_mouse_click() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    open_settings(&mut app);
    app.settings_category = SettingsCategory::Keybindings;
    app.load_settings_for_category();
    app.settings_selected_index = 0;

    let original_value = app
      .settings_items
      .first()
      .and_then(|setting| match &setting.value {
        SettingValue::Key(value) => Some(value.clone()),
        _ => None,
      })
      .expect("first keybinding should be a key setting");

    let areas = settings_layout_areas(&app).expect("settings layout areas");
    let x = areas.list.x + 2;
    let y = areas.list.y + 1;

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );
    assert!(app.settings_edit_mode);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    let current_value = app
      .settings_items
      .first()
      .and_then(|setting| match &setting.value {
        SettingValue::Key(value) => Some(value.clone()),
        _ => None,
      })
      .expect("first keybinding should still be a key setting");

    assert!(!app.settings_edit_mode);
    assert_eq!(current_value, original_value);
  }

  #[test]
  fn click_no_on_unsaved_prompt_discards_changes_and_exits() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    open_settings(&mut app);

    let bool_index = app
      .settings_items
      .iter()
      .position(|setting| matches!(setting.value, SettingValue::Bool(_)))
      .expect("expected a boolean setting");
    app.settings_selected_index = bool_index;
    settings::handler(Key::Enter, &mut app);

    settings::handler(Key::Esc, &mut app);
    assert!(app.settings_unsaved_prompt_visible);

    let prompt_areas = settings_unsaved_prompt_areas(&app).expect("unsaved prompt areas");
    handler(
      mouse_event(
        MouseEventKind::Down(MouseButton::Left),
        prompt_areas.no.x + 1,
        prompt_areas.no.y,
      ),
      &mut app,
    );

    assert!(!app.settings_unsaved_prompt_visible);
    assert_ne!(app.get_current_route().active_block, ActiveBlock::Settings);
  }

  #[test]
  fn click_in_playlist_selects_row() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    with_playlist_items(&mut app);
    app.selected_playlist_index = Some(0);

    let areas = main_layout_areas(&app).expect("layout areas");
    let x = areas.playlists.x + 1;
    let y = areas.playlists.y + 2;

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert_eq!(app.selected_playlist_index, Some(1));
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::MyPlaylists);
  }

  #[test]
  fn click_in_saved_albums_selects_row() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::AlbumList, ActiveBlock::AlbumList);
    with_saved_albums(&mut app, 3);
    app.album_list_index = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let x = areas.content.x + 1;
    let y = areas.content.y + 3;

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert_eq!(app.album_list_index, 1);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::AlbumList);
  }

  #[test]
  fn click_in_song_table_first_highlights_second_plays() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
    app.track_table.context = Some(TrackTableContext::RecommendedTracks);
    app.track_table.tracks = vec![
      crate::core::plugin_api::TrackInfo::from(&full_track("0000000000000000000001", "Track 1")),
      crate::core::plugin_api::TrackInfo::from(&full_track("0000000000000000000002", "Track 2")),
    ];
    app.track_table.selected_index = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let x = areas.content.x + 1;
    let y = areas.content.y + 3;

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert_eq!(app.track_table.selected_index, 1);
    assert!(!app.is_loading);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), x, y),
      &mut app,
    );

    assert_eq!(app.track_table.selected_index, 1);
    assert!(app.is_loading);
  }

  #[test]
  fn scroll_over_saved_albums_changes_selection() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::AlbumList, ActiveBlock::AlbumList);
    with_saved_albums(&mut app, 3);
    app.album_list_index = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let x = areas.content.x + 1;
    let y = areas.content.y + 2;

    handler(mouse_event(MouseEventKind::ScrollDown, x, y), &mut app);
    assert_eq!(app.album_list_index, 1);

    handler(mouse_event(MouseEventKind::ScrollUp, x, y), &mut app);
    assert_eq!(app.album_list_index, 0);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::AlbumList);
  }

  #[test]
  fn content_table_selection_helpers_cover_supported_blocks() {
    let mut app = App::default();
    app.album_list_index = 2;
    app.saved_album_tracks_index = 3;
    app.recently_played.index = 4;
    app.artists_list_index = 5;
    app.shows_list_index = 6;
    app.episode_list_index = 7;

    assert_eq!(
      content_table_selected_index(ActiveBlock::AlbumList, &app),
      2
    );
    assert_eq!(
      content_table_selected_index(ActiveBlock::AlbumTracks, &app),
      3
    );
    assert_eq!(
      content_table_selected_index(ActiveBlock::RecentlyPlayed, &app),
      4
    );
    assert_eq!(content_table_selected_index(ActiveBlock::Artists, &app), 5);
    assert_eq!(content_table_selected_index(ActiveBlock::Podcasts, &app), 6);
    assert_eq!(
      content_table_selected_index(ActiveBlock::EpisodeTable, &app),
      7
    );

    set_content_table_selected_index(ActiveBlock::AlbumList, 8, &mut app);
    set_content_table_selected_index(ActiveBlock::AlbumTracks, 9, &mut app);
    set_content_table_selected_index(ActiveBlock::RecentlyPlayed, 10, &mut app);
    set_content_table_selected_index(ActiveBlock::Artists, 11, &mut app);
    set_content_table_selected_index(ActiveBlock::Podcasts, 12, &mut app);
    set_content_table_selected_index(ActiveBlock::EpisodeTable, 13, &mut app);

    assert_eq!(app.album_list_index, 8);
    assert_eq!(app.saved_album_tracks_index, 9);
    assert_eq!(app.recently_played.index, 10);
    assert_eq!(app.artists_list_index, 11);
    assert_eq!(app.shows_list_index, 12);
    assert_eq!(app.episode_list_index, 13);
  }

  #[test]
  fn content_table_selection_helpers_update_simplified_album_tracks() {
    let mut app = App::default();
    app.album_table_context = AlbumTableContext::Simplified;
    app.selected_album_simplified = Some(SelectedAlbum {
      album: crate::core::plugin_api::AlbumInfo {
        name: "Album".to_string(),
        ..Default::default()
      },
      tracks: crate::core::pagination::Paged::default(),
      selected_index: 1,
    });

    assert_eq!(
      content_table_selected_index(ActiveBlock::AlbumTracks, &app),
      1
    );

    set_content_table_selected_index(ActiveBlock::AlbumTracks, 2, &mut app);

    assert_eq!(
      app
        .selected_album_simplified
        .as_ref()
        .map(|album| album.selected_index),
      Some(2)
    );
  }

  #[test]
  fn click_outside_playlist_is_ignored() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    with_playlist_items(&mut app);
    app.selected_playlist_index = Some(1);

    handler(
      mouse_event(MouseEventKind::Down(MouseButton::Left), 0, 0),
      &mut app,
    );

    assert_eq!(app.selected_playlist_index, Some(1));
  }

  #[test]
  fn clicked_index_respects_list_scroll_offset() {
    let area = Rect::new(0, 0, 20, 8); // Inner height = 6 rows
    let selected_index = 8;
    let total_items = 20;

    let first_visible = list_item_index_from_click(area, 1, selected_index, total_items);
    let second_visible = list_item_index_from_click(area, 2, selected_index, total_items);

    assert_eq!(first_visible, Some(3));
    assert_eq!(second_visible, Some(4));
  }

  #[test]
  fn scroll_over_library_changes_selection() {
    let mut app = App::default();
    app.size = Size {
      width: 160,
      height: 50,
    };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    app.library.selected_index = 0;

    let areas = main_layout_areas(&app).expect("layout areas");
    let x = areas.library.x + 1;
    let y = areas.library.y + 1;

    handler(mouse_event(MouseEventKind::ScrollDown, x, y), &mut app);
    assert_eq!(app.library.selected_index, 1);

    handler(mouse_event(MouseEventKind::ScrollUp, x, y), &mut app);
    assert_eq!(app.library.selected_index, 0);

    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Library);
  }

  #[test]
  fn table_click_mapping_respects_table_offset() {
    let area = Rect::new(0, 0, 80, 12);
    let selected_index = 15;
    let item_count = 40;

    let first = table_item_index_from_click(area, 2, selected_index, item_count);
    let second = table_item_index_from_click(area, 3, selected_index, item_count);

    assert_eq!(first, Some(9));
    assert_eq!(second, Some(10));
  }
}
