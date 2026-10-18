use crate::core::app::{ActiveBlock, App};
use crate::core::source::Source;
use crate::tui::layout::{is_wide_layout, library_constraints, split_input_help_and_settings};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  Frame,
};

use super::{
  search::draw_input_and_help_box,
  util::{draw_selectable_list, FOLDER_ICON},
};

pub fn draw_library_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let current_route = app.get_current_route();
  let highlight_state = (
    current_route.active_block == ActiveBlock::Library,
    current_route.hovered_block == ActiveBlock::Library,
  );
  let rows: Vec<&str> = app
    .library_rows()
    .iter()
    .map(|target| target.localized_name(app.strings()))
    .collect();
  draw_selectable_list(
    f,
    app,
    layout_chunk,
    app.strings().sidebar.library,
    &rows,
    highlight_state,
    Some(app.library_cursor()),
    &app.view.library_scroll_offset,
  );
}

/// Sidebar label for the pinned community playlist.
fn community_pin_label(app: &App) -> String {
  app.strings().sidebar.community_pin.to_string()
}

pub fn draw_playlist_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let strings = &app.strings().sidebar;
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
    let items: Vec<String> = if app.local_playlists().is_empty() {
      vec![strings.empty_local(&app.user_config.keys.manage_devices.to_string())]
    } else {
      app
        .local_playlists()
        .iter()
        .map(|p| format!("{FOLDER_ICON} {}", p.display_name(app.strings())))
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      strings.local_files,
      &items,
      highlight_state,
      app.view.selected_playlist_index,
      &app.view.playlist_scroll_offset,
    );
    return;
  }

  // Apple Music: "All songs" first, then the Music playlists, each opening
  // the shared track table. Read-only, so no "Add Playlist".
  if app.active_source == Source::AppleMusic {
    let items: Vec<String> = if app.apple_music_playlists().is_empty() {
      vec![strings.empty_apple_music(&app.user_config.keys.manage_devices.to_string())]
    } else {
      app
        .apple_music_playlists()
        .iter()
        .map(|p| {
          format!(
            "\u{1F3B5} {}",
            strings.playlist_label(Source::AppleMusic, &p.uri, &p.name)
          )
        })
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      strings.apple_music,
      &items,
      highlight_state,
      app.view.selected_playlist_index,
      &app.view.playlist_scroll_offset,
    );
    return;
  }

  // Subsonic: the sidebar Playlists panel lists the server's playlists (no
  // local-write support, so no "Add Playlist").
  if app.active_source == Source::Subsonic {
    let items: Vec<String> = if app.subsonic_playlists().is_empty() {
      vec![strings.empty_subsonic(&app.user_config.keys.manage_devices.to_string())]
    } else {
      app
        .subsonic_playlists()
        .iter()
        .map(|p| format!("\u{1F3B5} {}", p.name))
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      strings.subsonic,
      &items,
      highlight_state,
      app.view.selected_playlist_index,
      &app.view.playlist_scroll_offset,
    );
    return;
  }

  // Qobuz: the sidebar Playlists panel lists the favorites row, the user's
  // playlists, and the favorite albums, each opening the shared track table.
  if app.active_source == Source::Qobuz {
    let items: Vec<String> = if app.qobuz_playlists().is_empty() {
      vec![strings.empty_qobuz(&app.user_config.keys.manage_devices.to_string())]
    } else {
      app
        .qobuz_playlists()
        .iter()
        .map(|p| {
          strings
            .playlist_label(Source::Qobuz, &p.uri, &p.name)
            .to_string()
        })
        .collect()
    };
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      strings.qobuz,
      &items,
      highlight_state,
      app.view.selected_playlist_index,
      &app.view.playlist_scroll_offset,
    );
    return;
  }

  // Internet Radio: the sidebar Playlists panel lists the configured stations.
  // A station is a leaf — Enter plays it directly instead of drilling in.
  if app.active_source == Source::Radio {
    let items: Vec<String> = if app.radio_stations().is_empty() {
      vec![strings.empty_radio.to_string()]
    } else {
      app
        .radio_stations()
        .iter()
        .map(|s| format!("\u{1F4FB} {}", s.name))
        .collect()
    };

    draw_selectable_list(
      f,
      app,
      layout_chunk,
      strings.radio_stations,
      &items,
      highlight_state,
      app.view.selected_playlist_index,
      &app.view.playlist_scroll_offset,
    );
    return;
  }

  // YouTube: the sidebar Playlists panel lists the user's *local* YouTube
  // playlists (youtube_playlists.yml — no Google account), plus a create
  // entry. Videos are found via search and added with `w`.
  if app.active_source == Source::YouTube {
    let mut items: Vec<String> = app
      .youtube_playlists()
      .iter()
      .map(|p| format!("\u{1F4FC} {} ({})", p.name, p.track_count))
      .collect();
    items.push(strings.new_playlist.to_string());
    draw_selectable_list(
      f,
      app,
      layout_chunk,
      strings.youtube_playlists,
      &items,
      highlight_state,
      app.view.selected_playlist_index,
      &app.view.playlist_scroll_offset,
    );
    return;
  }

  let display_items = app.get_playlist_display_items();

  let playlist_items: Vec<String> = if app.playlist_folder_items().is_empty() {
    // Fallback only when folder-aware items are not initialized yet. Keep the
    // pin at row 0 so this branch stays consistent with the display-item count
    // (which injects the pin) and the Enter/click index math.
    let mut names: Vec<String> = Vec::new();
    if app.community_pin_visible() {
      names.push(community_pin_label(app));
    }
    if let Some(p) = app.playlists() {
      names.extend(p.items.iter().map(|item| item.name.to_owned()));
    }
    names
  } else {
    display_items
      .iter()
      .map(|item| match item {
        crate::core::app::PlaylistFolderItem::Folder(folder) => {
          let name = folder.display_name(app.strings());
          if name.starts_with('\u{2190}') {
            // Back entry (already has arrow prefix)
            name.into_owned()
          } else {
            format!("{FOLDER_ICON} {}", name)
          }
        }
        crate::core::app::PlaylistFolderItem::Playlist { index, .. } => app
          .all_playlists()
          .get(*index)
          .map(|p| p.name.clone())
          .unwrap_or_else(|| strings.unknown.to_string()),
        crate::core::app::PlaylistFolderItem::CommunityPin => community_pin_label(app),
      })
      .collect()
  };

  // "+ Add Playlist" is the leading row (row 0), above the display items.
  let mut display_list = vec![strings.add_playlist.to_string()];
  display_list.extend(playlist_items);

  draw_selectable_list(
    f,
    app,
    layout_chunk,
    strings.playlists,
    &display_list,
    highlight_state,
    app.view.selected_playlist_index,
    &app.view.playlist_scroll_offset,
  );
}

pub fn draw_user_block(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let lib_constraints = library_constraints(&app.runtime_state);
  if is_wide_layout(app) {
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
    let [library_area, playlist_area] = layout_chunk.layout(&Layout::vertical(lib_constraints));
    draw_library_block(f, app, library_area);
    draw_playlist_block(f, app, playlist_area);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::plugin_api::PlaylistInfo;
  use crate::tui::event::Key;
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
      generated_name: None,
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
  fn local_source_sidebar_lists_folders_below_the_free_library_rows() {
    let mut app = App::default_connected();
    app.active_source = Source::Local;
    *app.local_playlists_mut() = vec![folder("Jazz")];
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
      content.contains("Stats"),
      "the free library rows stay under Local: {content}"
    );
    assert!(
      !content.contains("Liked Songs"),
      "Spotify library entries must be hidden under Local: {content}"
    );
  }

  /// The foreground of the first cell drawing `symbol`, if any cell does.
  fn fg_of(app: &App, area: Rect, symbol: &str) -> Option<ratatui::style::Color> {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| draw_user_block(f, app, area)).unwrap();
    let buffer = terminal.backend().buffer();
    buffer
      .content()
      .iter()
      .find(|c| c.symbol() == symbol)
      .map(|c| c.fg)
  }

  #[test]
  fn sidebar_folder_icon_is_a_text_glyph_in_the_theme_text_color() {
    use crate::core::app::{PlaylistFolder, PlaylistFolderItem};
    use crate::core::theme::Color;
    use crate::tui::ui::util::FOLDER_ICON;
    let mut app = App::default();
    app.user_config.theme.text = Color::Rgb(1, 2, 3);
    *app.playlist_folder_items_mut() = vec![PlaylistFolderItem::Folder(PlaylistFolder {
      generated_name: false,
      name: "Moods".into(),
      current_id: 0,
      target_id: 1,
    })];
    let area = Rect::new(0, 0, 40, 40);
    let content = rendered(&app, area);
    // An emoji is drawn as a fixed-color picture no theme can recolor.
    assert!(!content.contains('\u{1F4C1}'), "{content}");
    assert!(
      content.contains(&format!("{FOLDER_ICON} Moods")),
      "{content}"
    );
    assert_eq!(
      fg_of(&app, area, FOLDER_ICON),
      Some(ratatui::style::Color::Rgb(1, 2, 3))
    );
  }

  #[test]
  fn local_source_folder_icon_is_a_text_glyph_in_the_theme_text_color() {
    use crate::core::theme::Color;
    use crate::tui::ui::util::FOLDER_ICON;
    let mut app = App::default_connected();
    app.active_source = Source::Local;
    app.user_config.theme.text = Color::Rgb(1, 2, 3);
    *app.local_playlists_mut() = vec![folder("Jazz")];
    let area = Rect::new(0, 0, 32, 40);
    let content = rendered(&app, area);
    assert!(!content.contains('\u{1F4C1}'), "{content}");
    assert!(
      content.contains(&format!("{FOLDER_ICON} Jazz")),
      "{content}"
    );
    assert_eq!(
      fg_of(&app, area, FOLDER_ICON),
      Some(ratatui::style::Color::Rgb(1, 2, 3))
    );
  }

  #[test]
  fn empty_local_sidebar_names_the_configured_source_key() {
    let mut app = App::default_connected();
    app.active_source = Source::Local;
    app.user_config.keys.manage_devices = Key::Char('D');
    // 60 columns: the 32-column tests above cut this hint off.
    let content = rendered(&app, Rect::new(0, 0, 60, 40));
    assert!(
      content.contains("press `D`"),
      "empty Local hint should name the rebound key: {content}"
    );
    assert!(
      !content.contains("press `d`"),
      "empty Local hint must not still show the default key: {content}"
    );
  }

  #[test]
  fn spotify_source_sidebar_shows_library() {
    let app = App::default_connected(); // Spotify is the default source
    let content = rendered(&app, Rect::new(0, 0, 32, 40));
    assert!(
      content.contains("Liked Songs"),
      "Spotify library entries should render: {content}"
    );
  }

  #[test]
  fn spotify_scope_without_a_session_keeps_only_the_free_library_rows() {
    let app = App::default();
    let content = rendered(&app, Rect::new(0, 0, 32, 40));
    assert!(
      content.contains("Stats") && content.contains("Friends"),
      "Friends and Stats need no session: {content}"
    );
    assert!(
      !content.contains("Liked Songs"),
      "Spotify rows need a session: {content}"
    );
  }

  #[test]
  fn community_pin_renders_below_add_and_above_playlists() {
    use crate::core::app::PlaylistFolderItem;
    use crate::core::test_helpers::playlist_info;
    let mut app = App::default(); // Spotify default, toggle on by default
    *app.all_playlists_mut() = vec![playlist_info(
      "37i9dQZF1DXcBWIGoYBM5M",
      "My Mix",
      "me",
      false,
    )];
    *app.playlist_folder_items_mut() = vec![PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 0,
    }];
    let content = rendered(&app, Rect::new(0, 0, 40, 40));
    // Top-to-bottom: "+ Add Playlist", then the pin, then the user's playlists.
    let add_idx = content
      .find("Add Playlist")
      .expect("Add Playlist should render");
    let pin_idx = content
      .find("spotatui community")
      .expect("pin should render for Spotify");
    let mix_idx = content.find("My Mix").expect("real playlist should render");
    assert!(add_idx < pin_idx, "Add must be above the pin: {content}");
    assert!(pin_idx < mix_idx, "pin must be above playlists: {content}");
  }

  #[test]
  fn community_pin_absent_under_non_spotify_source() {
    let mut app = App::default();
    app.active_source = Source::Local;
    *app.local_playlists_mut() = vec![folder("Jazz")];
    let content = rendered(&app, Rect::new(0, 0, 40, 40));
    assert!(
      !content.contains("spotatui community"),
      "pin must not render off Spotify: {content}"
    );
  }

  #[test]
  fn community_pin_absent_when_toggle_off() {
    let mut app = App::default();
    app.user_config.behavior.pin_community_playlist = false;
    let content = rendered(&app, Rect::new(0, 0, 40, 40));
    assert!(
      !content.contains("spotatui community"),
      "pin must not render when toggled off: {content}"
    );
  }

  #[test]
  fn community_pin_absent_when_already_following() {
    use crate::core::app::{PlaylistFolderItem, COMMUNITY_PLAYLIST_ID};
    use crate::core::test_helpers::playlist_info;
    let mut app = App::default();
    *app.all_playlists_mut() = vec![playlist_info(
      COMMUNITY_PLAYLIST_ID,
      "Community Follow",
      "spotatui",
      false,
    )];
    *app.playlist_folder_items_mut() = vec![PlaylistFolderItem::Playlist {
      index: 0,
      current_id: 0,
    }];
    let content = rendered(&app, Rect::new(0, 0, 40, 40));
    assert!(
      content.contains("Community Follow"),
      "followed playlist should render: {content}"
    );
    assert!(
      !content.contains("spotatui community"),
      "pin must be suppressed when already following: {content}"
    );
  }

  #[test]
  fn list_shrinking_after_scrolling_resets_offset_and_renders_items() {
    let mut app = App::default_connected();
    app.active_source = Source::Local;
    let area = Rect::new(0, 0, 32, 10);

    // Populate with 20 items and scroll down
    *app.local_playlists_mut() = (0..20).map(|i| folder(&format!("Folder {i}"))).collect();
    app.view.selected_playlist_index = Some(15);
    rendered(&app, area);
    assert!(
      app.view.playlist_scroll_offset.get() > 0,
      "offset should be scrolled down"
    );

    // Shrink the list to 2 items
    *app.local_playlists_mut() = vec![folder("ShrunkA"), folder("ShrunkB")];
    app.view.selected_playlist_index = Some(0);
    let content = rendered(&app, area);

    // Offset must clamp to 0 and render the remaining items instead of blank space
    assert_eq!(
      app.view.playlist_scroll_offset.get(),
      0,
      "offset must clamp back to 0"
    );
    assert!(
      content.contains("ShrunkA"),
      "shrunken list must render top item: {content}"
    );
    assert!(
      content.contains("ShrunkB"),
      "shrunken list must render second item: {content}"
    );
  }
}
