pub mod artist;
pub mod audio_analysis;
pub mod create_playlist;
pub mod discover;
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
use crate::core::layout::{playbar_constraint, sidebar_constraints};
use ratatui::{
  layout::{Constraint, Layout, Rect},
  Frame,
};

pub use self::artist::draw_artist_albums;
pub use self::create_playlist::draw_create_playlist_form;
pub use self::discover::draw_discover;
pub use self::home::draw_home;
pub use self::library::draw_user_block;
#[cfg(feature = "cover-art")]
pub use self::player::draw_cover_art_view;
pub use self::player::{draw_device_list, draw_lyrics_view, draw_playbar};
pub use self::popups::{
  draw_announcement_prompt, draw_dialog, draw_error_screen, draw_exit_prompt, draw_help_menu,
  draw_party, draw_queue, draw_sort_menu,
};
pub use self::search::{draw_input_and_help_box, draw_search_results};
pub use self::tables::{
  draw_album_list, draw_album_table, draw_artist_table, draw_podcast_table,
  draw_recently_played_table, draw_recommendations_table, draw_show_episodes, draw_song_table,
};
use self::util::{get_main_layout_margin, SMALL_TERMINAL_WIDTH};

pub fn draw_main_layout(f: &mut Frame<'_>, app: &App) {
  let margin = get_main_layout_margin(app);
  // Responsive layout: new one kicks in at width 150 or higher
  if app.size.width >= SMALL_TERMINAL_WIDTH && !app.user_config.behavior.enforce_wide_search_bar {
    let [routes_area, playbar_area] = f.area().layout(
      &Layout::vertical([
        Constraint::Min(1),
        playbar_constraint(&app.user_config.behavior),
      ])
      .margin(margin),
    );

    // Nested main block with potential routes
    draw_routes(f, app, routes_area);

    // Currently playing
    draw_playbar(f, app, playbar_area);
  } else {
    let [input_area, routes_area, playbar_area] = f.area().layout(
      &Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        playbar_constraint(&app.user_config.behavior),
      ])
      .margin(margin),
    );

    // Search input and help
    draw_input_and_help_box(f, app, input_area);

    // Nested main block with potential routes
    draw_routes(f, app, routes_area);

    // Currently playing
    draw_playbar(f, app, playbar_area);
  }

  // Possibly draw confirm dialog
  draw_dialog(f, app);

  // Possibly draw sort menu
  draw_sort_menu(f, app);
}

pub fn draw_routes(f: &mut Frame<'_>, app: &App, layout_chunk: Rect) {
  let [user_area, content_area] = layout_chunk.layout(&Layout::horizontal(sidebar_constraints(
    &app.user_config.behavior,
  )));

  draw_user_block(f, app, user_area);

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
    RouteId::Artists => {
      draw_artist_table(f, app, content_area);
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
    | RouteId::AnnouncementPrompt
    | RouteId::ExitPrompt
    | RouteId::Settings
    | RouteId::HelpMenu
    | RouteId::Queue
    | RouteId::Party => {} // These are drawn outside the main routed content area.
    RouteId::Dialog => {} // This is handled in draw_dialog.
    RouteId::CreatePlaylist => {} // This is drawn as an overlay via draw_create_playlist_form.
  };
}
