pub mod artist;
pub mod audio_analysis;
pub mod columns;
pub mod create_playlist;
pub mod discover;
pub mod friends;
pub mod help;
pub mod home;
pub mod library;
pub mod player;
pub mod popups;
pub mod search;
pub mod settings;
pub mod tables;
pub mod util;

use crate::core::app::{App, RouteId};
use crate::core::layout::{compute_main_layout, is_wide_layout};
use ratatui::{layout::Rect, Frame};

pub use self::artist::draw_artist_albums;
pub use self::create_playlist::draw_create_playlist_form;
pub use self::discover::draw_discover;
pub use self::friends::draw_friends;
pub use self::home::draw_home;
pub use self::library::draw_user_block;
#[cfg(feature = "cover-art")]
pub use self::player::draw_cover_art_view;
pub use self::player::draw_miniplayer;
pub use self::player::{draw_device_list, draw_lyrics_view, draw_playbar};
pub use self::popups::{
  draw_announcement_prompt, draw_dialog, draw_error_screen, draw_exit_prompt, draw_help_menu,
  draw_party, draw_plugin_popup, draw_queue, draw_sort_menu,
};
pub use self::search::{draw_input_and_help_box, draw_search_results};
pub use self::tables::{
  draw_album_list, draw_album_table, draw_artist_table, draw_local_browser, draw_podcast_table,
  draw_recently_played_table, draw_recommendations_table, draw_show_episodes, draw_song_table,
};
pub fn draw_main_layout(f: &mut Frame<'_>, app: &App) {
  if let Some(areas) = compute_main_layout(app) {
    // In wide layout the input row lives inside the sidebar and is drawn by
    // draw_user_block (it varies per source); the top row is only drawn here.
    if !is_wide_layout(app) {
      if let (Some(input_area), Some(help_area), Some(settings_area)) =
        (areas.input, areas.help, areas.settings)
      {
        draw_input_and_help_box(f, app, input_area, help_area, settings_area);
      }
    }
    draw_user_block(f, app, areas.sidebar);
    draw_route_content(f, app, areas.content);
    draw_playbar(f, app, areas.playbar);
  }

  // Possibly draw confirm dialog
  draw_dialog(f, app);

  // Possibly draw sort menu
  draw_sort_menu(f, app);
}

fn draw_route_content(f: &mut Frame<'_>, app: &App, content_area: Rect) {
  let current_route = app.get_current_route();

  match current_route.id {
    RouteId::Search => {
      draw_search_results(f, app, content_area);
    }
    RouteId::TrackTable => {
      draw_song_table(f, app, content_area);
    }
    RouteId::AlbumTracks => {
      draw_album_table(f, app, content_area);
    }
    RouteId::RecentlyPlayed => {
      draw_recently_played_table(f, app, content_area);
    }
    RouteId::Artist => {
      draw_artist_albums(f, app, content_area);
    }
    RouteId::AlbumList => {
      draw_album_list(f, app, content_area);
    }
    RouteId::PodcastEpisodes => {
      draw_show_episodes(f, app, content_area);
    }
    RouteId::Home => {
      draw_home(f, app, content_area);
    }
    RouteId::Discover => {
      draw_discover(f, app, content_area);
    }
    RouteId::Friends => {
      draw_friends(f, app, content_area);
    }
    RouteId::Artists => {
      draw_artist_table(f, app, content_area);
    }
    RouteId::LocalBrowser => {
      draw_local_browser(f, app, content_area);
    }
    RouteId::Podcasts => {
      draw_podcast_table(f, app, content_area);
    }
    RouteId::Recommendations => {
      draw_recommendations_table(f, app, content_area);
    }
    RouteId::Error
    | RouteId::SelectedDevice
    | RouteId::Analysis
    | RouteId::LyricsView
    | RouteId::CoverArtView
    | RouteId::MiniPlayer
    | RouteId::AnnouncementPrompt
    | RouteId::ExitPrompt
    | RouteId::Settings
    | RouteId::HelpMenu
    | RouteId::Queue
    | RouteId::Party => {} // These are drawn outside the main routed content area.
    RouteId::Dialog => {}         // This is handled in draw_dialog.
    RouteId::CreatePlaylist => {} // This is drawn as an overlay via draw_create_playlist_form.
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::ActiveBlock;
  use ratatui::{backend::TestBackend, buffer::Buffer, layout::Size, Terminal};

  fn render(width: u16, height: u16) -> (App, Buffer) {
    let mut app = App::default();
    app.size = Size { width, height };
    app.push_navigation_stack(RouteId::Home, ActiveBlock::Home);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw_main_layout(f, &app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (app, buffer)
  }

  fn row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area().width)
      .filter_map(|x| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
      .collect()
  }

  // The drawn input/help/settings boxes and sidebar panels must land exactly
  // where `compute_main_layout` puts the mouse hitboxes, and the search box
  // must be drawn once (a second copy used to overlay the Library panel in
  // wide layout, shifting it down three rows).
  #[test]
  fn wide_layout_draws_input_row_and_library_at_hitbox_positions() {
    let (app, buffer) = render(160, 50);
    let areas = compute_main_layout(&app).expect("layout areas");
    let input = areas.input.expect("input area");

    let input_row = row_text(&buffer, input.y);
    assert!(input_row.contains("Search"), "input row: {input_row}");
    assert!(input_row.contains("Help"), "input row: {input_row}");
    assert!(input_row.contains("Settings"), "input row: {input_row}");

    let library_row = row_text(&buffer, areas.library.y);
    assert!(
      library_row.contains("Library"),
      "library panel not at hitbox row: {library_row}"
    );
    let playlists_row = row_text(&buffer, areas.playlists.y);
    assert!(
      playlists_row.contains("Playlists"),
      "playlists panel not at hitbox row: {playlists_row}"
    );
  }

  #[test]
  fn narrow_layout_draws_input_row_and_library_at_hitbox_positions() {
    let (app, buffer) = render(100, 40);
    let areas = compute_main_layout(&app).expect("layout areas");
    let input = areas.input.expect("input area");

    let input_row = row_text(&buffer, input.y);
    assert!(input_row.contains("Search"), "input row: {input_row}");
    assert!(input_row.contains("Help"), "input row: {input_row}");
    assert!(input_row.contains("Settings"), "input row: {input_row}");

    let library_row = row_text(&buffer, areas.library.y);
    assert!(
      library_row.contains("Library"),
      "library panel not at hitbox row: {library_row}"
    );
  }
}
